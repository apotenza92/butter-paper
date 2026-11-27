//! The search panel (Cmd+Shift+F, Ctrl+Shift+F, or the Search button on the
//! left rail). It shares the left sidebar with Page Thumbnails: one shows at
//! a time, at one width. A query in the panel's language (`search_query`)
//! searches the document in view, or every open document in every window.
//! The results group by document, then page, with counts; each match shows a
//! snippet with the match marked, and each markup its subject, author, text
//! and replies. A click on a result, or Enter, goes to it (another tab or
//! window when it is there) and marks it on the page.
//!
//! The engine (`search_run`) reads one page at a time on background threads,
//! one per document, through each document's PDF worker after everything the
//! viewer asks for. Each change of the query, an option or the scope starts a
//! new search and ends the old one within one page.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    rc::Rc,
    time::Duration,
};

use gpui::{
    AnyWindowHandle, FontWeight, HighlightStyle, ListAlignment, ListState, StyledText, Task,
};
use gpui_component::{Icon, button::ButtonGroup};

use super::*;
use crate::{
    search_document::DocumentTextEdits,
    search_markups::MarkupSearchRecord,
    search_query::{SearchOptions, SearchQuery},
    search_run::{
        HitTarget, SEARCH_HIT_LIMIT, SearchConfig, SearchEvent, SearchHit, SearchResults,
        SearchRun, SearchSort,
    },
    search_scope::{SearchScope, open_workspaces},
};

#[path = "document_workspace_visual_search.rs"]
mod visual;
pub use visual::{
    DOCUMENT_SEARCH_MODE_TEXT_ID, DOCUMENT_SEARCH_MODE_VISUAL_ID, DOCUMENT_SEARCH_TRY_VISUAL_ID,
    DOCUMENT_VISUAL_ACTIONS_ID, DOCUMENT_VISUAL_CLEAR_ID, DOCUMENT_VISUAL_DRAW_BOX_ID,
    DOCUMENT_VISUAL_ERROR_ID, DOCUMENT_VISUAL_MIRRORED_ID, DOCUMENT_VISUAL_OPTIONS_BODY_ID,
    DOCUMENT_VISUAL_OPTIONS_ID, DOCUMENT_VISUAL_OPTIONS_SECTION_ID,
    DOCUMENT_VISUAL_OPTIONS_SUMMARY_ID, DOCUMENT_VISUAL_OPTIONS_TOGGLE_ID,
    DOCUMENT_VISUAL_PASTE_ID, DOCUMENT_VISUAL_PREVIEW_ID, DOCUMENT_VISUAL_PROGRESS_ID,
    DOCUMENT_VISUAL_RESULTS_ID, DOCUMENT_VISUAL_SIMILARITY_ID, DOCUMENT_VISUAL_SIMILARITY_INPUT_ID,
    DOCUMENT_VISUAL_SIMILARITY_ROW_ID, DOCUMENT_VISUAL_SIZES_ID, DOCUMENT_VISUAL_STOP_ID,
    DOCUMENT_VISUAL_SUMMARY_ID, DOCUMENT_VISUAL_TEMPLATE_ID, DOCUMENT_VISUAL_TURNS_ID, SearchMode,
    TemplateOrigin, VisualPanelRow, VisualPanelSummary, VisualTileView, visual_options_summary,
    visual_row_id, visual_tile_id,
};

pub const DOCUMENT_LEFT_RAIL_SEARCH_ID: &str = "document-left-rail-search";
pub const DOCUMENT_SEARCH_PANEL_ID: &str = "document-search-panel";
pub const DOCUMENT_SEARCH_INPUT_ID: &str = "document-search-input";
pub const DOCUMENT_SEARCH_MATCH_CASE_ID: &str = "document-search-match-case";
pub const DOCUMENT_SEARCH_EXPLAIN_ID: &str = "document-search-explain";
pub const DOCUMENT_SEARCH_COLLAPSE_ID: &str = "document-search-collapse";
pub const DOCUMENT_SEARCH_MORE_CONTEXT_ID: &str = "document-search-more-context";
pub const DOCUMENT_SEARCH_SORT_ID: &str = "document-search-sort";
pub const DOCUMENT_SEARCH_HEADER_ID: &str = "document-search-header";
pub const DOCUMENT_SEARCH_CLOSE_ID: &str = "document-search-close";
pub const DOCUMENT_SEARCH_CONTROLS_ID: &str = "document-search-controls";
pub const DOCUMENT_SEARCH_TOGGLES_ID: &str = "document-search-toggles";
/// The scope: a row of two tabs above Text and Visual, This document first.
pub const DOCUMENT_SEARCH_SCOPE_ID: &str = "document-search-scope";
pub const DOCUMENT_SEARCH_SCOPE_THIS_ID: &str = "document-search-scope-this";
pub const DOCUMENT_SEARCH_SCOPE_ALL_ID: &str = "document-search-scope-all";

/// The id of the tab that chooses `scope`.
pub const fn search_scope_button_id(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::ThisDocument => DOCUMENT_SEARCH_SCOPE_THIS_ID,
        SearchScope::AllOpenDocuments => DOCUMENT_SEARCH_SCOPE_ALL_ID,
    }
}
pub const DOCUMENT_SEARCH_SUMMARY_ID: &str = "document-search-summary";
pub const DOCUMENT_SEARCH_EXPLANATION_ID: &str = "document-search-explanation";
pub const DOCUMENT_SEARCH_RESULTS_ID: &str = "document-search-results";
pub const DOCUMENT_SEARCH_FOOTER_ID: &str = "document-search-footer";

/// The debug selector of the result row at `index` of the list.
pub fn search_panel_row_id(index: usize) -> String {
    format!("document-search-row-{index}")
}

/// The key context of the panel.
const SEARCH_PANEL_CONTEXT: &str = "SearchPanel";
/// The key context of the result list, while it has the focus.
const SEARCH_RESULTS_CONTEXT: &str = "SearchResults";
/// How long the query rests before a search starts.
const SEARCH_QUERY_DELAY: Duration = Duration::from_millis(250);
/// How long the document rests, after an edit, before the search runs again.
const SEARCH_EDIT_DELAY: Duration = Duration::from_millis(400);
/// The least time between two updates of the list while a search runs.
const SEARCH_UPDATE_INTERVAL: Duration = Duration::from_millis(100);
/// The characters of a snippet before its match, on one line.
const COMPACT_BEFORE: usize = 12;
/// The frames in which the query field takes back the focus it was given.
const SEARCH_FOCUS_FRAMES: u8 = 30;
/// The replies a markup row shows.
const SHOWN_REPLIES: usize = 3;
/// A result row's indent for each level under its document: a chevron and
/// its gap, so that a page's chevron is under its document's name and a
/// match under its page's name.
pub(super) const ROW_INDENT: f32 = 16.;
/// The least height of a result list, in points at the default interface
/// size: about five text rows, or two rows of hit pictures. In a shorter
/// window the controls above the list scroll instead.
pub(super) const RESULTS_MIN_HEIGHT: f32 = 160.;

/// The grid's unit: the panel's margins and the space between its controls.
pub(super) fn panel_space() -> gpui::Rems {
    crate::layout_metrics::scaled(crate::layout_metrics::SPACE)
}

/// The controls above a result list, in a region as tall as they are. In a
/// short window the region is shorter and they scroll, with the stock
/// overlay scroll bar, so that the list keeps `RESULTS_MIN_HEIGHT`.
pub(super) fn search_controls_region(
    controls: gpui::Stateful<gpui::Div>,
    scroll: &gpui::ScrollHandle,
) -> gpui::AnyElement {
    v_flex()
        .w_full()
        .min_h_0()
        .relative()
        .child(controls.min_h_0().overflow_y_scroll().track_scroll(scroll))
        .vertical_scrollbar(scroll)
        .into_any_element()
}

/// The line over a result list: "42 results in 3 documents". Hidden while
/// empty, so that the list starts under the controls.
pub(super) fn search_summary_line(
    id: &'static str,
    text: String,
    colour: gpui::Hsla,
) -> impl IntoElement {
    gpui::div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .px(panel_space())
        .pb(panel_space())
        .text_color(colour)
        .when(text.is_empty(), |summary| summary.hidden())
        .child(text)
}

gpui::actions!(
    search_panel,
    [
        /// Down in the query field: the focus goes to the first result.
        SearchFocusResults,
        SearchSelectPrevious,
        SearchSelectNext,
        SearchCollapseRow,
        SearchExpandRow,
        SearchOpenRow
    ]
);

/// The shortcut that opens the panel, as a keystroke: the platform's command
/// key with Shift and F.
pub const fn search_panel_shortcut(macos: bool) -> &'static str {
    if macos { "cmd-shift-f" } else { "ctrl-shift-f" }
}

/// The same shortcut as a tooltip writes it.
pub(super) const fn search_panel_shortcut_label(macos: bool) -> &'static str {
    if macos { "Cmd+Shift+F" } else { "Ctrl+Shift+F" }
}

/// The panel's keys. Bound after the workspace's, so that they act before
/// the workspace's arrows and Enter, and before the text field's Down.
pub(super) fn init_search_panel_actions(cx: &mut App) {
    let macos = cfg!(target_os = "macos");
    cx.bind_keys([
        KeyBinding::new(
            search_panel_shortcut(macos),
            FindInDocuments,
            Some("DocumentWorkspace"),
        ),
        // The stock text field could take the keys for itself.
        KeyBinding::new(
            search_panel_shortcut(macos),
            FindInDocuments,
            Some("DocumentWorkspace > Input"),
        ),
        KeyBinding::new("down", SearchFocusResults, Some("SearchPanel > Input")),
        KeyBinding::new("up", SearchSelectPrevious, Some(SEARCH_RESULTS_CONTEXT)),
        KeyBinding::new("down", SearchSelectNext, Some(SEARCH_RESULTS_CONTEXT)),
        KeyBinding::new("left", SearchCollapseRow, Some(SEARCH_RESULTS_CONTEXT)),
        KeyBinding::new("right", SearchExpandRow, Some(SEARCH_RESULTS_CONTEXT)),
        KeyBinding::new("enter", SearchOpenRow, Some(SEARCH_RESULTS_CONTEXT)),
        KeyBinding::new("space", SearchOpenRow, Some(SEARCH_RESULTS_CONTEXT)),
    ]);
}

/// The panel the left sidebar shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeftPanel {
    Thumbnails,
    Search,
}

/// The sort menu's entries, in menu order.
const SORTS: [(SearchSort, &str); 6] = [
    (SearchSort::DocumentOrder, "Document order"),
    (SearchSort::NameAscending, "Name A to Z"),
    (SearchSort::NameDescending, "Name Z to A"),
    (SearchSort::MostResults, "Most results first"),
    (SearchSort::MarkupsNewest, "Markups newest first"),
    (SearchSort::MarkupsOldest, "Markups oldest first"),
];

fn sort_label(sort: SearchSort) -> &'static str {
    SORTS
        .iter()
        .find(|(entry, _)| *entry == sort)
        .map_or("Document order", |(_, label)| label)
}

/// A group the user can collapse.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum GroupKey {
    Document(u64),
    Page(u64, u32),
}

/// One row of the result list.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum RowKey {
    Document(u64),
    Page(u64, u32),
    /// A hit: its document, page and place among that page's hits.
    Hit(u64, u32, usize),
}

impl RowKey {
    fn group(self) -> Option<GroupKey> {
        match self {
            Self::Document(id) => Some(GroupKey::Document(id)),
            Self::Page(id, page) => Some(GroupKey::Page(id, page)),
            Self::Hit(..) => None,
        }
    }

    /// The group that holds the row.
    fn parent(self) -> Option<Self> {
        match self {
            Self::Document(_) => None,
            Self::Page(id, _) => Some(Self::Document(id)),
            Self::Hit(id, page, _) => Some(Self::Page(id, page)),
        }
    }
}

/// A row as the list shows it, and as tests read it.
#[derive(Clone, Debug, PartialEq)]
pub enum SearchPanelRow {
    Document {
        /// The name, with "· window N" when another open document has it.
        name: String,
        count: usize,
        collapsed: bool,
    },
    Page {
        page_number: u32,
        count: usize,
        collapsed: bool,
    },
    /// Matches in page text that share a snippet: the text before them,
    /// the text from the first to the last, and the text after. `marks`
    /// gives each match in `matched` (empty: `matched` is one match).
    Text {
        before: String,
        matched: String,
        after: String,
        marks: Vec<std::ops::Range<usize>>,
    },
    /// A markup: "Subject · Author", its text (the match marked) and its
    /// replies ("Author: text").
    Markup {
        heading: String,
        before: String,
        matched: String,
        after: String,
        replies: Vec<String>,
        /// The reply the match is in.
        reply: Option<usize>,
    },
    /// A page that matches by its number or file name alone.
    Place,
}

/// What the panel shows, for tests and the review driver's log.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchPanelSummary {
    /// The panel is the sidebar's, and the sidebar shows.
    pub open: bool,
    /// The text in the field.
    pub text: String,
    /// The query the results are of: the field's text once it rests.
    pub searched: String,
    pub match_case: bool,
    pub explain: bool,
    /// The plain reading of the query, when Explain search is on.
    pub explanation: Option<String>,
    pub collapse: bool,
    pub more_context: bool,
    pub sort: SearchSort,
    pub scope: SearchScope,
    pub hits: usize,
    pub finished: bool,
    /// "42 results in 3 documents", "Searching…", "No results".
    pub summary: String,
    /// The notes under the list: the cap, and pages without text.
    pub footer: Vec<String>,
    pub rows: Vec<SearchPanelRow>,
    /// The row the keyboard is on.
    pub selected: Option<usize>,
    /// The row last opened.
    pub opened: Option<usize>,
}

/// The hits of a result opened in a window, which its pages paint.
pub(super) struct SearchMarks {
    pub(super) document_id: DocumentId,
    pub(super) pages: BTreeMap<u32, Vec<SearchHit>>,
    /// The opened hit: its page and its place on it.
    pub(super) current: Option<(u32, usize)>,
    /// The hits are a visual search's.
    pub(super) visual: bool,
}

/// A searched document: where it is and what its markup rows show.
struct PanelDocument {
    /// The name the rows show.
    label: String,
    path: std::path::PathBuf,
    /// Its window, when it is not the panel's.
    window: Option<AnyWindowHandle>,
    workspace: WeakEntity<DocumentWorkspace>,
    markups: HashMap<String, MarkupSearchRecord>,
    /// What the search read: a change runs the search again.
    read: Option<find_bar::FindRead>,
}

pub(super) struct SearchPanel {
    input: Entity<InputState>,
    _input_events: Subscription,
    mode: SearchMode,
    visual: visual::VisualPanel,
    /// Other windows whose changes can make the results stale.
    _observed: Vec<Subscription>,
    match_case: bool,
    explain: bool,
    collapse: bool,
    more_context: bool,
    sort: SearchSort,
    scope: SearchScope,
    /// The scroll position of the controls over the list, in a short window.
    controls_scroll: gpui::ScrollHandle,
    /// Whether the Visual tab's Options section is open: closed at first,
    /// then as it was left for the panel's lifetime.
    options_open: bool,
    /// The query of the search that runs or ran.
    query: Option<Arc<SearchQuery>>,
    query_text: String,
    /// The options and scope of the search that runs or ran.
    ran_with: (bool, SearchScope, Option<DocumentId>),
    results: SearchResults,
    run: Option<SearchRun>,
    generation: u64,
    delay: Option<Task<()>>,
    documents: HashMap<u64, PanelDocument>,
    /// Groups whose collapse differs from the Collapse results toggle.
    toggled: HashSet<GroupKey>,
    rows: Rc<Vec<(RowKey, SearchPanelRow)>>,
    list: ListState,
    results_focus: FocusHandle,
    selected: Option<RowKey>,
    opened: Option<RowKey>,
    /// Frames in which the field takes the focus back, when the page took
    /// it: the sidebar can show its panel a few frames after it opens.
    focus_frames: u8,
}

/// The text over the list: "42 results in 3 documents" when found,
/// "Searching…" before the first, "No results" when none, and nothing for an
/// empty query.
pub(super) fn search_summary_text(
    query_empty: bool,
    finished: bool,
    hits: usize,
    documents: usize,
) -> String {
    if query_empty {
        return String::new();
    }
    if hits == 0 {
        return if finished {
            "No results".into()
        } else {
            "Searching…".into()
        };
    }
    let results = if hits == 1 { "result" } else { "results" };
    let in_documents = if documents == 1 {
        "1 document".to_owned()
    } else {
        format!("{documents} documents")
    };
    let text = format!("{} {results} in {in_documents}", thousands(hits));
    if finished {
        text
    } else {
        format!("{text}, searching…")
    }
}

/// The notes under the list.
pub(super) fn search_footer_lines(limit_reached: bool, pages_without_text: u32) -> Vec<String> {
    let mut lines = Vec::new();
    if limit_reached {
        lines.push(format!(
            "First {} results. Narrow the search to see more.",
            thousands(SEARCH_HIT_LIMIT)
        ));
    }
    match pages_without_text {
        0 => {}
        1 => lines.push("1 page has no searchable text.".into()),
        count => lines.push(format!(
            "{} pages have no searchable text.",
            thousands(count as usize)
        )),
    }
    lines
}

fn thousands(count: usize) -> String {
    let digits = count.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    grouped
}

/// The text before a match on one line: its last characters, with an
/// ellipsis when cut.
fn compact_before(before: &str, more_context: bool) -> String {
    if more_context {
        return before.to_owned();
    }
    let count = before.chars().count();
    if count <= COMPACT_BEFORE {
        return before.to_owned();
    }
    let kept = before
        .chars()
        .skip(count - COMPACT_BEFORE)
        .collect::<String>();
    format!("…{}", kept.trim_start())
}

/// The names the rows show: a name that two open documents share gets
/// "· window N" when they are in different windows.
fn document_labels(documents: &[(u64, String, usize)]) -> HashMap<u64, String> {
    let mut labels = HashMap::new();
    for (id, name, window) in documents {
        let shared = documents.iter().any(|(other, other_name, other_window)| {
            other != id && other_name == name && other_window != window
        });
        let label = if shared {
            format!("{name} · window {window}")
        } else {
            name.clone()
        };
        labels.insert(*id, label);
    }
    labels
}

/// The rows of the list for `results` as sorted and collapsed.
fn build_rows(
    results: &SearchResults,
    sort: SearchSort,
    collapse: bool,
    toggled: &HashSet<GroupKey>,
) -> Vec<RowKey> {
    let mut rows = Vec::new();
    for document in results.ordered(sort) {
        if document.hit_count() == 0 {
            continue;
        }
        let id = document.document_id;
        rows.push(RowKey::Document(id));
        if toggled.contains(&GroupKey::Document(id)) {
            continue;
        }
        for (page, hits) in document.pages() {
            rows.push(RowKey::Page(id, page));
            if collapse != toggled.contains(&GroupKey::Page(id, page)) {
                continue;
            }
            rows.extend((0..hits.len()).map(|index| RowKey::Hit(id, page, index)));
        }
    }
    rows
}

impl SearchPanel {
    fn hits_of(&self, id: u64, page: u32) -> Option<&[SearchHit]> {
        self.results
            .documents()
            .iter()
            .find(|document| document.document_id == id)?
            .pages()
            .find(|(index, _)| *index == page)
            .map(|(_, hits)| hits)
    }

    fn hit(&self, key: RowKey) -> Option<&SearchHit> {
        let RowKey::Hit(id, page, index) = key else {
            return None;
        };
        self.hits_of(id, page)?.get(index)
    }

    fn collapsed(&self, group: GroupKey) -> bool {
        match group {
            GroupKey::Document(_) => self.toggled.contains(&group),
            GroupKey::Page(..) => self.collapse != self.toggled.contains(&group),
        }
    }

    fn row_view(&self, key: RowKey) -> SearchPanelRow {
        match key {
            RowKey::Document(id) => {
                let document = self
                    .results
                    .documents()
                    .iter()
                    .find(|document| document.document_id == id);
                SearchPanelRow::Document {
                    name: self
                        .documents
                        .get(&id)
                        .map(|document| document.label.clone())
                        .or_else(|| document.map(|document| document.name.clone()))
                        .unwrap_or_default(),
                    count: document.map_or(0, |document| document.hit_count()),
                    collapsed: self.collapsed(GroupKey::Document(id)),
                }
            }
            RowKey::Page(id, page) => SearchPanelRow::Page {
                page_number: page + 1,
                count: self.hits_of(id, page).map_or(0, <[SearchHit]>::len),
                collapsed: self.collapsed(GroupKey::Page(id, page)),
            },
            RowKey::Hit(id, ..) => {
                let Some(hit) = self.hit(key) else {
                    return SearchPanelRow::Place;
                };
                let snippet = &hit.snippet;
                match &hit.target {
                    HitTarget::Text { .. } => SearchPanelRow::Text {
                        before: compact_before(&snippet.before, self.more_context),
                        matched: snippet.matched.clone(),
                        after: snippet.after.clone(),
                        marks: snippet.marks.clone(),
                    },
                    HitTarget::Markup {
                        id: markup_id,
                        reply,
                        ..
                    } => {
                        let record = self
                            .documents
                            .get(&id)
                            .and_then(|document| document.markups.get(markup_id));
                        let heading = record.map_or_else(String::new, |record| {
                            match (record.subject.is_empty(), record.author.is_empty()) {
                                (false, false) => format!("{} · {}", record.subject, record.author),
                                (false, true) => record.subject.clone(),
                                (true, false) => record.author.clone(),
                                (true, true) => "Markup".to_owned(),
                            }
                        });
                        let replies = record
                            .map(|record| {
                                record
                                    .replies
                                    .iter()
                                    .map(|reply| {
                                        if reply.author.is_empty() {
                                            reply.text.clone()
                                        } else {
                                            format!("{}: {}", reply.author, reply.text)
                                        }
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        SearchPanelRow::Markup {
                            heading,
                            before: compact_before(&snippet.before, self.more_context),
                            matched: snippet.matched.clone(),
                            after: snippet.after.clone(),
                            replies,
                            reply: *reply,
                        }
                    }
                    HitTarget::Page | HitTarget::Visual { .. } => SearchPanelRow::Place,
                }
            }
        }
    }

    /// Makes the rows again from the results, keeping the list's measured
    /// rows where they did not change.
    fn rebuild_rows(&mut self) {
        let keys = build_rows(&self.results, self.sort, self.collapse, &self.toggled);
        let old = self.rows.clone();
        let same = old
            .iter()
            .zip(&keys)
            .take_while(|((old, _), new)| old == *new)
            .count();
        let rows = keys
            .iter()
            .map(|key| (*key, self.row_view(*key)))
            .collect::<Vec<_>>();
        if same < old.len() || same < rows.len() {
            splice_keeping_top(&self.list, same..old.len(), rows.len() - same);
        }
        self.rows = Rc::new(rows);
        if self
            .selected
            .is_some_and(|selected| !self.rows.iter().any(|(key, _)| *key == selected))
        {
            self.selected = None;
        }
    }

    fn row_index(&self, key: RowKey) -> Option<usize> {
        self.rows.iter().position(|(row, _)| *row == key)
    }
}

/// Replaces `old` rows of `list` with `count` new ones. Rows that arrive
/// above the first in view must not scroll the list away from its top:
/// documents finish in any order while results stream in.
fn splice_keeping_top(list: &ListState, old: std::ops::Range<usize>, count: usize) {
    let top = list.logical_scroll_top();
    let at_top = top.item_ix == 0 && top.offset_in_item <= px(0.);
    list.splice(old, count);
    if at_top {
        list.scroll_to(gpui::ListOffset::default());
    }
}

impl DocumentWorkspace {
    /// The panel, made the first time it is needed.
    fn ensure_search_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_panel.is_some() {
            return;
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search text and markups"));
        let events =
            cx.subscribe_in(
                &input,
                window,
                |workspace, _, event: &InputEvent, _, cx| match event {
                    InputEvent::Change => workspace.schedule_search(SEARCH_QUERY_DELAY, cx),
                    InputEvent::PressEnter { .. } => workspace.restart_search(cx),
                    _ => {}
                },
            );
        let scope = SearchScope::default();
        let visual = visual::VisualPanel::new(window, cx);
        self.search_panel = Some(SearchPanel {
            input,
            _input_events: events,
            mode: SearchMode::Text,
            visual,
            _observed: Vec::new(),
            match_case: false,
            explain: false,
            collapse: false,
            more_context: false,
            sort: SearchSort::DocumentOrder,
            scope,
            controls_scroll: gpui::ScrollHandle::new(),
            options_open: false,
            query: None,
            query_text: String::new(),
            ran_with: (false, scope, None),
            results: SearchResults::default(),
            run: None,
            generation: 0,
            delay: None,
            documents: HashMap::new(),
            toggled: HashSet::new(),
            rows: Rc::default(),
            list: ListState::new(0, ListAlignment::Top, px(240.)),
            results_focus: cx.focus_handle(),
            selected: None,
            opened: None,
            focus_frames: 0,
        });
    }

    /// The left sidebar's panel, when it shows.
    pub fn left_panel(&self) -> Option<LeftPanel> {
        self.left_panel
    }

    /// A rail button: its panel opens, or closes when it is open. The other
    /// panel gives way at the same width.
    pub(super) fn toggle_left_panel(&mut self, panel: LeftPanel, cx: &mut Context<Self>) {
        self.left_panel = if self.left_panel == Some(panel) {
            None
        } else {
            Some(panel)
        };
        cx.notify();
    }

    /// Puts the focus in the query field, its text selected. While the
    /// sidebar yields to the properties panel, the field is not drawn: the
    /// focus stays where it is, so the keyboard keeps working.
    pub(super) fn focus_search_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_search_panel(window, cx);
        if !self.left_sidebar_fits {
            self.release_search_query_focus();
            return;
        }
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        panel.focus_frames = SEARCH_FOCUS_FRAMES;
        let panel = &*panel;
        panel.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        if panel.run.is_none() && !panel.input.read(cx).value().trim().is_empty() {
            self.restart_search(cx);
        }
        cx.notify();
    }

    /// Cmd+Shift+F or Ctrl+Shift+F, and Edit ▸ Find in Documents: the panel
    /// opens (in place of Page Thumbnails) in the Text mode, with the focus in
    /// the query field, also when the panel was left in the Visual mode.
    /// Selected page text becomes the query.
    pub(super) fn open_search_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_document_id.is_none() {
            return;
        }
        // With the find bar open and a term in it, the shortcut takes that
        // search to the panel.
        if self.open_find_in_search_panel(window, cx) {
            return;
        }
        let seed = self.active_document_id.and_then(|document_id| {
            self.selected_page_text(document_id, cx)
                .as_deref()
                .and_then(find_bar::seed_query)
        });
        self.left_panel = Some(LeftPanel::Search);
        self.ensure_search_panel(window, cx);
        // The Visual mode has no field: the shortcut always shows Text.
        self.set_search_mode(SearchMode::Text, cx);
        if let Some(seed) = seed
            && let Some(panel) = self.search_panel.as_ref()
        {
            panel
                .input
                .update(cx, |input, cx| input.set_value(seed, window, cx));
            self.restart_search(cx);
        }
        self.focus_search_query(window, cx);
    }

    /// The find bar's button, and the panel's shortcut while the bar is open
    /// with a term in it: the Search panel opens in the Text mode with the
    /// bar's text, Match case and Match whole word, runs the search, and the
    /// find bar closes. The panel has no whole-word switch, so that goes into
    /// the query as a `/regex/`. The panel keeps its own scope. Where the left
    /// sidebar cannot show (a narrow window, Properties open), the bar stays
    /// open and the focus where it was. Returns whether the find bar had a
    /// term to take (so the shortcut does no more); an empty bar opens the
    /// panel as the shortcut always did.
    pub(super) fn open_find_in_search_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document_id) = self.active_document_id else {
            return false;
        };
        let Some(bar) = self.find_bars.get(&document_id).filter(|bar| bar.open) else {
            return false;
        };
        let text = bar.input.read(cx).value().to_string();
        if text.trim().is_empty() {
            return false;
        }
        if !self.left_sidebar_fits {
            return true;
        }
        let options = bar.options;
        self.close_find_bar(window, cx);
        self.left_panel = Some(LeftPanel::Search);
        self.ensure_search_panel(window, cx);
        self.set_search_mode(SearchMode::Text, cx);
        let query = crate::search_query::panel_query_for_literal(&text, options);
        if let Some(panel) = self.search_panel.as_mut() {
            panel.match_case = options.match_case;
            panel.input.update(cx, |input, cx| {
                input.set_value(query, window, cx);
            });
        }
        self.restart_search(cx);
        self.focus_search_query(window, cx);
        cx.notify();
        true
    }

    /// A menu command for the panel or the find bar arrived with no focus in
    /// the window: it acts on the next frame.
    pub(super) fn request_search_panel(&mut self, cx: &mut Context<Self>) {
        self.search_panel_requested = true;
        cx.notify();
    }

    pub(super) fn request_find_bar(&mut self, cx: &mut Context<Self>) {
        self.find_bar_requested = true;
        cx.notify();
    }

    pub(super) fn open_requested_search_panel(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if std::mem::take(&mut self.search_panel_requested) {
            self.open_search_panel(window, cx);
        }
        if std::mem::take(&mut self.find_bar_requested) {
            self.open_find_bar(window, cx);
        }
        self.keep_search_query_focus(window, cx);
    }

    /// While the sidebar shows the panel, the field keeps the focus that
    /// the shortcut or the rail gave it, unless the focus went elsewhere
    /// than the page.
    fn keep_search_query_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let workspace_focus = self.workspace_focus.clone();
        let shown = self.left_panel == Some(LeftPanel::Search) && self.left_sidebar_fits;
        let Some(panel) = self
            .search_panel
            .as_mut()
            .filter(|panel| panel.focus_frames > 0)
        else {
            return;
        };
        panel.focus_frames -= 1;
        let input = panel.input.clone();
        let field = input.read(cx).focus_handle(cx);
        let on_page = match window.focused(cx) {
            None => true,
            Some(focused) => focused == workspace_focus,
        };
        // The Visual mode has no field.
        if !shown || panel.mode != SearchMode::Text {
            panel.focus_frames = 0;
            return;
        }
        if !field.is_focused(window) && on_page {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// Escape in the panel: with focus in the field, it clears a query, then
    /// gives the focus back to the page; in the list, it gives the focus
    /// back to the page. The panel stays open: it is a sidebar.
    fn escape_in_search_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A box being drawn for a visual search stops first.
        if let Some(document_id) = self
            .signature_box
            .as_ref()
            .map(|capture| capture.document_id)
        {
            self.cancel_signature_box(document_id, cx);
            return;
        }
        let Some(panel) = self.search_panel.as_ref() else {
            return;
        };
        let input = panel.input.clone();
        let in_field = input.read(cx).focus_handle(cx).is_focused(window);
        if in_field && !input.read(cx).value().is_empty() {
            input.update(cx, |input, cx| input.set_value("", window, cx));
            self.restart_search(cx);
            return;
        }
        self.release_search_query_focus();
        self.workspace_focus.focus(window, cx);
        cx.notify();
    }

    fn schedule_search(&mut self, delay: Duration, cx: &mut Context<Self>) {
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        panel.delay = Some(cx.spawn(async move |workspace, cx| {
            cx.background_executor().timer(delay).await;
            let _ = workspace.update(cx, |workspace, cx| workspace.restart_search_if_changed(cx));
        }));
    }

    fn restart_search_if_changed(&mut self, cx: &mut Context<Self>) {
        let unchanged = self.search_panel.as_ref().is_some_and(|panel| {
            panel.run.is_some()
                && panel.query_text == panel.input.read(cx).value().as_ref()
                && panel.ran_with == self.search_settings()
                && !self.searched_documents_changed(cx)
        });
        if unchanged {
            // The wait is over: a later change schedules a new one.
            if let Some(panel) = self.search_panel.as_mut() {
                panel.delay = None;
            }
        } else {
            self.restart_search(cx);
        }
    }

    /// Whether a document the search read changed under it, in this window
    /// or another, or the ready documents in scope are not those it read.
    fn searched_documents_changed(&self, cx: &Context<Self>) -> bool {
        let Some(panel) = self.search_panel.as_ref() else {
            return false;
        };
        let own = cx.entity_id();
        let documents_changed = panel.documents.iter().any(|(id, document)| {
            document.read.as_ref().is_some_and(|read| {
                let id = DocumentId::new(*id);
                if document.workspace.entity_id() == own {
                    return !self.find_read_unchanged(id, read, cx);
                }
                document
                    .workspace
                    .upgrade()
                    .is_none_or(|workspace| !workspace.read(cx).find_read_unchanged(id, read, cx))
            })
        });
        if documents_changed {
            return true;
        }
        let mut ready = 0;
        let mut all_searched = true;
        self.for_each_ready_document_in_scope(own, panel.scope, cx, |id| {
            ready += 1;
            all_searched &= panel.documents.contains_key(&id);
        });
        !all_searched || ready != panel.documents.len()
    }

    /// Match case, the scope and, for This document, the document in view.
    fn search_settings(&self) -> (bool, SearchScope, Option<DocumentId>) {
        let Some(panel) = self.search_panel.as_ref() else {
            return (false, SearchScope::default(), None);
        };
        let document = match panel.scope {
            SearchScope::AllOpenDocuments => None,
            SearchScope::ThisDocument => self.active_document_id,
        };
        (panel.match_case, panel.scope, document)
    }

    /// The documents the scope names, as the search reads them: this
    /// window's tabs first, then the other windows' in window order.
    fn search_targets(
        &self,
        scope: SearchScope,
        cx: &mut Context<Self>,
    ) -> Vec<(find_bar::SearchSource, PanelDocument, usize)> {
        let cache = Self::find_cache(cx);
        let own = cx.entity();
        let windows = open_workspaces(cx);
        let own_number = windows
            .iter()
            .position(|(_, workspace)| *workspace == own)
            .map_or(1, |index| index + 1);
        let mut targets = Vec::new();
        let own_ids = match scope {
            SearchScope::ThisDocument => self.active_document_id.into_iter().collect(),
            SearchScope::AllOpenDocuments => self.session_order(cx),
        };
        for id in own_ids {
            if let Some(source) = self.search_source(id, 0, cache.clone(), cx) {
                let document = PanelDocument {
                    label: String::new(),
                    path: source.path.clone(),
                    window: None,
                    workspace: own.downgrade(),
                    markups: HashMap::new(),
                    read: Some(source.read.clone()),
                };
                targets.push((source, document, own_number));
            }
        }
        if scope == SearchScope::AllOpenDocuments {
            for (index, (handle, workspace)) in windows.iter().enumerate() {
                if *workspace == own {
                    continue;
                }
                let other = workspace.read(cx);
                for id in other.session_order(cx) {
                    if let Some(source) = other.search_source(id, 0, cache.clone(), cx) {
                        let document = PanelDocument {
                            label: String::new(),
                            path: source.path.clone(),
                            window: Some(*handle),
                            workspace: workspace.downgrade(),
                            markups: HashMap::new(),
                            read: Some(source.read.clone()),
                        };
                        targets.push((source, document, index + 1));
                    }
                }
            }
        }
        targets
    }

    /// Ends the search that runs and starts one for the field's text, the
    /// options and the scope.
    pub(super) fn restart_search(&mut self, cx: &mut Context<Self>) {
        let settings = self.search_settings();
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        panel.delay = None;
        panel.run = None;
        panel.generation += 1;
        let generation = panel.generation;
        let text = panel.input.read(cx).value().to_string();
        panel.query_text = text.clone();
        panel.ran_with = settings;
        panel.results = SearchResults::default();
        panel.documents.clear();
        panel.toggled.clear();
        panel.selected = None;
        panel.opened = None;
        let options = SearchOptions {
            match_case: panel.match_case,
            whole_word: false,
        };
        let scope = panel.scope;
        let query = Arc::new(SearchQuery::parse(&text, options));
        panel.query = (!text.trim().is_empty()).then(|| query.clone());
        // The old search's marks on the pages go with it.
        self.clear_search_marks_everywhere(cx);
        if text.trim().is_empty() {
            if let Some(panel) = self.search_panel.as_mut() {
                panel.rebuild_rows();
            }
            cx.notify();
            return;
        }
        let targets = self.search_targets(scope, cx);
        self.observe_other_windows(cx);
        let labels = document_labels(
            &targets
                .iter()
                .map(|(source, _, window)| (source.document.id(), source.title.clone(), *window))
                .collect::<Vec<_>>(),
        );
        let mut documents = Vec::new();
        let mut metadata = Vec::new();
        let mut panel_documents = HashMap::new();
        for (source, mut document, _) in targets {
            let id = source.document.id();
            document.label = labels.get(&id).cloned().unwrap_or_default();
            document.markups = source
                .records
                .iter()
                .map(|record| (record.id.clone(), record.clone()))
                .collect();
            metadata.push((id, source.title.clone(), source.document.page_count()));
            documents.push(source.document);
            panel_documents.insert(id, document);
        }
        let (events, receiver) = async_channel::unbounded::<SearchEvent>();
        // One row for the matches that share a snippet.
        let config = SearchConfig {
            join_nearby: true,
            ..SearchConfig::default()
        };
        let run = SearchRun::start(documents, query, config, move |event| {
            let _ = events.send_blocking(event);
        });
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        panel.results = SearchResults::new(metadata);
        panel.documents = panel_documents;
        panel.run = Some(run);
        panel.rebuild_rows();
        // The list takes what arrived at most ten times a second. The task
        // looks for events on a timer rather than waiting on the channel, so
        // no search thread wakes the window's executor.
        cx.spawn(async move |workspace, cx| {
            loop {
                cx.background_executor().timer(SEARCH_UPDATE_INTERVAL).await;
                let mut batch = Vec::new();
                while let Ok(event) = receiver.try_recv() {
                    batch.push(event);
                }
                if batch.is_empty() {
                    if receiver.is_closed() && receiver.is_empty() {
                        break;
                    }
                    continue;
                }
                let alive = workspace.update(cx, |workspace, cx| {
                    workspace.apply_search_events(generation, batch, cx)
                });
                if !alive.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// Merges what the search found. False when the events are of a search
    /// that ended, or the search finished: the task that delivers them stops.
    fn apply_search_events(
        &mut self,
        generation: u64,
        events: Vec<SearchEvent>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(panel) = self
            .search_panel
            .as_mut()
            .filter(|panel| panel.generation == generation)
        else {
            return false;
        };
        let mut finished = false;
        for event in events {
            finished |= matches!(event, SearchEvent::Finished { .. });
            panel.results.apply(event);
        }
        panel.rebuild_rows();
        cx.notify();
        !finished
    }

    /// What a document of `workspace` (this window's or another's) is now,
    /// for a check that the search read it so.
    /// The ready documents a scope names now: for All open documents, those
    /// of every window.
    fn ready_documents_in_scope(
        &self,
        own: &WeakEntity<DocumentWorkspace>,
        scope: SearchScope,
        cx: &App,
    ) -> HashSet<u64> {
        let mut ready = HashSet::new();
        self.for_each_ready_document_in_scope(own.entity_id(), scope, cx, |id| {
            ready.insert(id);
        });
        ready
    }

    /// Calls `each` with the id of each ready document the scope names now.
    fn for_each_ready_document_in_scope(
        &self,
        own: gpui::EntityId,
        scope: SearchScope,
        cx: &App,
        mut each: impl FnMut(u64),
    ) {
        let mut ready_in = |workspace: &DocumentWorkspace, ids: Vec<DocumentId>| {
            for id in ids {
                if workspace.session(id, cx).is_some_and(|session| {
                    matches!(session.read(cx).status, NativeDocumentStatus::Ready)
                }) {
                    each(id.value());
                }
            }
        };
        match scope {
            SearchScope::ThisDocument => {
                ready_in(self, self.active_document_id.into_iter().collect());
            }
            SearchScope::AllOpenDocuments => {
                ready_in(self, self.session_order(cx));
                for (_, workspace) in open_workspaces(cx) {
                    if workspace.entity_id() == own {
                        continue;
                    }
                    let other = workspace.read(cx);
                    ready_in(other, other.session_order(cx));
                }
            }
        }
    }

    /// Watches the other windows, so that a change to one of their
    /// documents, or to their tabs, runs an All open documents search again.
    fn observe_other_windows(&mut self, cx: &mut Context<Self>) {
        let own = cx.entity();
        let mut observed = Vec::new();
        for (_, workspace) in open_workspaces(cx) {
            if workspace == own {
                continue;
            }
            observed.push(cx.observe(&workspace, |workspace, _, cx| {
                workspace.poll_search_stale(cx);
            }));
            let sessions = workspace.read(cx).sessions.clone();
            for session in sessions {
                observed.push(cx.observe(&session, |workspace, _, cx| {
                    workspace.poll_search_stale(cx);
                }));
            }
        }
        if let Some(panel) = self.search_panel.as_mut() {
            panel._observed = observed;
        }
    }

    /// Runs the search again when a document it read changed under it, in
    /// this window or another, or the documents in scope changed, once the
    /// edits pause.
    pub(super) fn poll_search_stale(&mut self, cx: &mut Context<Self>) {
        self.poll_visual_stale(cx);
        let Some(panel) = self.search_panel.as_ref() else {
            return;
        };
        if panel.query.is_none() || panel.delay.is_some() || panel.run.is_none() {
            return;
        }
        if self.searched_documents_changed(cx) {
            self.schedule_search(SEARCH_EDIT_DELAY, cx);
        }
    }

    /// Clears the page marks of the panel's results in every window.
    fn clear_search_marks_everywhere(&mut self, cx: &mut Context<Self>) {
        if self.search_marks.take().is_some() {
            cx.notify();
        }
        let own = cx.entity();
        for (_, workspace) in open_workspaces(cx) {
            if workspace == own {
                continue;
            }
            workspace.update(cx, |workspace, cx| {
                if workspace.search_marks.take().is_some() {
                    cx.notify();
                }
            });
        }
    }

    /// A document of this window closed: its results and marks go, and a
    /// visual search no longer draws its pages.
    pub(super) fn forget_search_document(&mut self, document_id: DocumentId, cx: &mut App) {
        self.forget_visual_document(document_id, cx);
        if self
            .search_marks
            .as_ref()
            .is_some_and(|marks| marks.document_id == document_id)
        {
            self.search_marks = None;
        }
        if let Some(panel) = self.search_panel.as_mut()
            && panel.documents.remove(&document_id.value()).is_some()
        {
            // The list keeps no row of a closed document. The search runs
            // again when its tabs change.
            panel.selected = None;
            panel.opened = None;
        }
    }

    fn toggle_search_option(
        &mut self,
        change: impl FnOnce(&mut SearchPanel),
        search_again: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        change(panel);
        if search_again {
            self.restart_search(cx);
        } else if let Some(panel) = self.search_panel.as_mut() {
            panel.rebuild_rows();
        }
        cx.notify();
    }

    /// The scope control: all open documents or the one in view.
    pub fn set_search_scope(&mut self, scope: SearchScope, cx: &mut Context<Self>) {
        if self
            .search_panel
            .as_ref()
            .is_none_or(|panel| panel.scope == scope)
        {
            return;
        }
        self.toggle_search_option(|panel| panel.scope = scope, true, cx);
        self.restart_visual_search(cx);
    }

    /// The sort menu.
    pub fn set_search_sort(&mut self, sort: SearchSort, cx: &mut Context<Self>) {
        self.toggle_search_option(|panel| panel.sort = sort, false, cx);
    }

    /// The key of the row at `index`, and its group's collapse toggled.
    fn toggle_search_group(&mut self, key: RowKey, cx: &mut Context<Self>) {
        let Some(group) = key.group() else {
            return;
        };
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        if !panel.toggled.remove(&group) {
            panel.toggled.insert(group);
        }
        panel.rebuild_rows();
        cx.notify();
    }

    fn select_search_row(&mut self, key: RowKey, cx: &mut Context<Self>) {
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        panel.selected = Some(key);
        if let Some(index) = panel.row_index(key) {
            panel.list.scroll_to_reveal_item(index);
        }
        cx.notify();
    }

    fn step_search_selection(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(panel) = self.search_panel.as_ref() else {
            return;
        };
        let count = panel.rows.len();
        if count == 0 {
            return;
        }
        let next = match panel.selected.and_then(|key| panel.row_index(key)) {
            Some(index) if forward => (index + 1).min(count - 1),
            Some(index) => index.saturating_sub(1),
            None => 0,
        };
        let key = panel.rows[next].0;
        self.select_search_row(key, cx);
    }

    /// Down in the query field: the focus goes to the first result.
    fn focus_search_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.search_panel.as_ref() else {
            return;
        };
        let Some(first) = panel.rows.first().map(|(key, _)| *key) else {
            return;
        };
        let focus = panel.results_focus.clone();
        let key = panel.selected.unwrap_or(first);
        focus.focus(window, cx);
        self.select_search_row(key, cx);
    }

    /// Left: a group collapses; on a hit, the selection goes to its page.
    /// Right: a group expands.
    fn collapse_search_row(&mut self, collapse: bool, cx: &mut Context<Self>) {
        let Some(panel) = self.search_panel.as_ref() else {
            return;
        };
        let Some(key) = panel.selected else {
            return;
        };
        match key.group() {
            Some(group) if panel.collapsed(group) != collapse => self.toggle_search_group(key, cx),
            Some(_) => {
                if collapse && let Some(parent) = key.parent() {
                    self.select_search_row(parent, cx);
                }
            }
            None => {
                if collapse && let Some(parent) = key.parent() {
                    self.select_search_row(parent, cx);
                }
            }
        }
    }

    /// Enter or Space: a group collapses or expands; a hit opens.
    fn open_selected_search_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(key) = self.search_panel.as_ref().and_then(|panel| panel.selected) else {
            return;
        };
        self.activate_search_row(key, window, cx);
    }

    fn activate_search_row(&mut self, key: RowKey, window: &mut Window, cx: &mut Context<Self>) {
        if key.group().is_some() {
            self.toggle_search_group(key, cx);
        } else {
            self.open_search_hit(key, window, cx);
        }
    }

    /// Goes to a hit: its window and tab come to the front, the view goes to
    /// it, and its document's hits are marked, this one stronger. A markup
    /// the model holds is selected, as a click on it does. The panel keeps
    /// the focus, so the arrow keys go on through the results.
    fn open_search_hit(&mut self, key: RowKey, window: &mut Window, cx: &mut Context<Self>) {
        let RowKey::Hit(id, page, index) = key else {
            return;
        };
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        let Some(hit) = panel.hit(key).cloned() else {
            return;
        };
        let Some(document) = panel.documents.get(&id) else {
            return;
        };
        let pages = panel
            .results
            .documents()
            .iter()
            .find(|document| document.document_id == id)
            .map(|document| {
                document
                    .pages()
                    .map(|(page, hits)| (page, hits.to_vec()))
                    .collect::<BTreeMap<_, _>>()
            })
            .unwrap_or_default();
        let marks = SearchMarks {
            document_id: DocumentId::new(id),
            pages,
            current: Some((page, index)),
            visual: false,
        };
        let other_window = document.window;
        let target = document.workspace.clone();
        panel.selected = Some(key);
        panel.opened = Some(key);
        let focus = panel.results_focus.clone();
        focus.focus(window, cx);
        match (other_window, target.upgrade()) {
            (Some(handle), Some(workspace)) => {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                workspace.update(cx, |workspace, cx| {
                    workspace.show_search_hit(marks, &hit, cx)
                });
                // The marks of this window's documents go: one result is
                // marked at a time.
                if self.search_marks.take().is_some() {
                    cx.notify();
                }
            }
            (None, Some(_)) => {
                self.clear_search_marks_everywhere(cx);
                self.show_search_hit(marks, &hit, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    /// Shows a search panel hit in this window: its tab, its place and its
    /// marks.
    pub(super) fn show_search_hit(
        &mut self,
        marks: SearchMarks,
        hit: &SearchHit,
        cx: &mut Context<Self>,
    ) {
        let document_id = marks.document_id;
        if !self.activate_document(document_id, cx) {
            return;
        }
        self.search_marks = Some(marks);
        let bounds = match &hit.target {
            HitTarget::Text { .. } => find_bar::quads_box(&hit.quads),
            HitTarget::Markup { id, in_model, .. } => {
                if *in_model && let Ok(markup) = MarkupId::new(id.clone()) {
                    self.select_annotation(document_id, &markup, cx);
                }
                hit.bounds
            }
            HitTarget::Visual { .. } => hit.bounds,
            HitTarget::Page => None,
        };
        match bounds {
            Some(bounds) => self.reveal_hit_box(document_id, hit.page_index, bounds, cx),
            None => {
                self.activate_thumbnail_page(document_id, hit.page_index, cx);
            }
        }
        cx.notify();
    }

    /// What the search panel shows now; `None` before it was first opened.
    pub fn search_panel_summary(&self, cx: &App) -> Option<SearchPanelSummary> {
        let panel = self.search_panel.as_ref()?;
        let text = panel.input.read(cx).value().to_string();
        let query_empty = panel.query_text.trim().is_empty();
        Some(SearchPanelSummary {
            open: self.left_panel == Some(LeftPanel::Search),
            text,
            searched: panel.query_text.clone(),
            match_case: panel.match_case,
            explain: panel.explain,
            explanation: panel
                .explain
                .then(|| panel.query.as_ref().map(|query| query.explain()))
                .flatten(),
            collapse: panel.collapse,
            more_context: panel.more_context,
            sort: panel.sort,
            scope: panel.scope,
            hits: panel.results.hit_count(),
            finished: panel.results.is_finished(),
            summary: search_summary_text(
                query_empty,
                panel.results.is_finished(),
                panel.results.hit_count(),
                panel
                    .results
                    .documents()
                    .iter()
                    .filter(|document| document.hit_count() > 0)
                    .count(),
            ),
            footer: search_footer_lines(
                panel.results.limit_reached(),
                panel.results.pages_without_text(),
            ),
            rows: panel.rows.iter().map(|(_, row)| row.clone()).collect(),
            selected: panel.selected.and_then(|key| panel.row_index(key)),
            opened: panel.opened.and_then(|key| panel.row_index(key)),
        })
    }

    /// Whether the focus is in the result list.
    /// Whether the panel's query field has the focus.
    pub fn search_query_focused(&self, window: &Window, cx: &App) -> bool {
        self.search_panel
            .as_ref()
            .is_some_and(|panel| panel.input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// The query field stops taking back the focus: the user moved it.
    pub(super) fn release_search_query_focus(&mut self) {
        if let Some(panel) = self.search_panel.as_mut() {
            panel.focus_frames = 0;
        }
    }

    pub fn search_results_focused(&self, window: &Window) -> bool {
        self.search_panel
            .as_ref()
            .is_some_and(|panel| panel.results_focus.is_focused(window))
    }

    /// The X in the panel's header: the sidebar closes, as a click on the
    /// rail's Search button does, and the keyboard goes back to the page.
    /// The query, options and results stay for the next time it opens.
    pub(super) fn close_search_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.left_panel == Some(LeftPanel::Search) {
            self.left_panel = None;
        }
        self.release_search_query_focus();
        self.workspace_focus.focus(window, cx);
        cx.notify();
    }

    /// The panel's header: its title and the X that closes it.
    fn render_search_header(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        crate::property_controls::PropertyInspectorHeader::new(DOCUMENT_SEARCH_HEADER_ID, "Search")
            .trailing(
                crate::accessible_button::close_icon_button(
                    DOCUMENT_SEARCH_CLOSE_ID,
                    "Close search",
                )
                .on_click(cx.listener(|workspace, _: &ClickEvent, window, cx| {
                    workspace.close_search_panel(window, cx)
                })),
            )
            .into_any_element()
    }

    /// Text and Visual, the panel's two modes, as one segmented control.
    fn render_search_mode(&self, panel: &SearchPanel, cx: &mut Context<Self>) -> gpui::AnyElement {
        let mode_button =
            |id: &'static str, mode: SearchMode, label: &'static str, cx: &mut Context<Self>| {
                Button::new(id)
                    .debug_selector(move || id.into())
                    .accessibility_id(id)
                    .label(label)
                    .flex_1()
                    .min_w_0()
                    .selected(panel.mode == mode)
                    .on_click(cx.listener(move |workspace, _: &ClickEvent, window, cx| {
                        workspace.choose_search_mode(mode, window, cx)
                    }))
            };
        ButtonGroup::new(visual::DOCUMENT_SEARCH_MODE_ID)
            .w_full()
            .outline()
            .child(mode_button(
                DOCUMENT_SEARCH_MODE_TEXT_ID,
                SearchMode::Text,
                "Text",
                cx,
            ))
            .child(mode_button(
                DOCUMENT_SEARCH_MODE_VISUAL_ID,
                SearchMode::Visual,
                "Visual",
                cx,
            ))
            .into_any_element()
    }

    /// Where the search looks, as a row of tabs above Text and Visual: This
    /// document or All open documents. Both modes share it.
    fn render_search_scope(&self, panel: &SearchPanel, cx: &mut Context<Self>) -> gpui::AnyElement {
        let scope_button = |scope: SearchScope, cx: &mut Context<Self>| {
            let id = search_scope_button_id(scope);
            // The tabs read at the small text size, so both labels show whole
            // in the sidebar's width.
            crate::accessible_button::accessible_icon_button(
                Button::new(id)
                    .debug_selector(move || id.into())
                    .accessibility_id(id)
                    .compact()
                    .child(
                        gpui::div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .child(scope.label()),
                    )
                    // Each tab takes the room its label needs, and the rest
                    // is shared, so "All open documents" is not cut while
                    // "This document" has room to spare.
                    .flex_auto()
                    .min_w_0()
                    .selected(panel.scope == scope)
                    .on_click(cx.listener(move |workspace, _: &ClickEvent, _, cx| {
                        workspace.set_search_scope(scope, cx)
                    })),
                scope.label(),
            )
        };
        gpui::div()
            .id(DOCUMENT_SEARCH_SCOPE_ID)
            .debug_selector(|| DOCUMENT_SEARCH_SCOPE_ID.into())
            .w_full()
            .child(
                ButtonGroup::new("document-search-scope-tabs")
                    .w_full()
                    .outline()
                    .children(SearchScope::ALL.map(|scope| scope_button(scope, cx))),
            )
            .into_any_element()
    }

    /// The panel, in the left sidebar: the header, then the controls with
    /// the grid's margins (a properties panel's layout), then the summary
    /// and the results, which take the rest of the height.
    pub(super) fn render_search_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let Some(panel) = self.search_panel.as_ref() else {
            // The panel is made when it opens; a frame before that is empty.
            return gpui::div()
                .id(DOCUMENT_SEARCH_PANEL_ID)
                .size_full()
                .bg(cx.theme().background)
                .into_any_element();
        };
        let column = v_flex()
            .id(DOCUMENT_SEARCH_PANEL_ID)
            .debug_selector(|| DOCUMENT_SEARCH_PANEL_ID.into())
            .key_context(SEARCH_PANEL_CONTEXT)
            .size_full()
            .min_h_0()
            .text_sm()
            .bg(cx.theme().background)
            .on_action(cx.listener(|workspace, _: &Escape, window, cx| {
                workspace.escape_in_search_panel(window, cx)
            }))
            .on_action(
                cx.listener(|workspace, _: &SearchFocusResults, window, cx| {
                    workspace.focus_search_results(window, cx)
                }),
            )
            .child(self.render_search_header(cx));
        let mode = self.render_search_mode(panel, cx);
        let scope = self.render_search_scope(panel, cx);
        if panel.mode == SearchMode::Visual {
            let body = self.render_visual_search_body(panel, mode, scope, cx);
            return column
                // Cmd+V or Ctrl+V in the panel pastes the picture to find,
                // not markups on the page.
                .on_action(cx.listener(|workspace, _: &Paste, window, cx| {
                    workspace.paste_visual_picture(window, cx)
                }))
                .child(body)
                .into_any_element();
        }
        let toggle = |id: &'static str, label: &'static str, icon: Icon, on: bool| {
            crate::accessible_button::accessible_icon_button(
                Button::new(id)
                    .debug_selector(move || id.into())
                    .accessibility_id(id)
                    .ghost()
                    .icon(icon)
                    .selected(on)
                    .toggled(on)
                    .tooltip(label),
                label,
            )
        };
        let match_case = toggle(
            DOCUMENT_SEARCH_MATCH_CASE_ID,
            "Match case",
            Icon::new(IconName::CaseSensitive),
            panel.match_case,
        )
        .on_click(cx.listener(|workspace, _: &ClickEvent, _, cx| {
            workspace.toggle_search_option(|panel| panel.match_case = !panel.match_case, true, cx)
        }));
        let explain = toggle(
            DOCUMENT_SEARCH_EXPLAIN_ID,
            "Explain search",
            Icon::new(IconName::Info),
            panel.explain,
        )
        .on_click(cx.listener(|workspace, _: &ClickEvent, _, cx| {
            workspace.toggle_search_option(|panel| panel.explain = !panel.explain, false, cx)
        }));
        let collapse = toggle(
            DOCUMENT_SEARCH_COLLAPSE_ID,
            "Collapse results",
            Icon::default().path(crate::application_assets::LIST_COLLAPSE_ICON),
            panel.collapse,
        )
        .on_click(cx.listener(|workspace, _: &ClickEvent, _, cx| {
            workspace.toggle_search_option(
                |panel| {
                    panel.collapse = !panel.collapse;
                    panel.toggled.clear();
                },
                false,
                cx,
            )
        }));
        let more_context = toggle(
            DOCUMENT_SEARCH_MORE_CONTEXT_ID,
            "More context",
            Icon::default().path(crate::application_assets::MORE_CONTEXT_ICON),
            panel.more_context,
        )
        .on_click(cx.listener(|workspace, _: &ClickEvent, _, cx| {
            workspace.toggle_search_option(
                |panel| {
                    panel.more_context = !panel.more_context;
                    panel.list.remeasure();
                },
                false,
                cx,
            )
        }));
        let sort_owner = cx.entity().downgrade();
        let current_sort = panel.sort;
        let sort = crate::accessible_button::accessible_icon_button(
            Button::new(DOCUMENT_SEARCH_SORT_ID)
                .debug_selector(|| DOCUMENT_SEARCH_SORT_ID.into())
                .accessibility_id(DOCUMENT_SEARCH_SORT_ID)
                .ghost()
                .icon(IconName::SortAscending)
                .tooltip(format!("Sort: {}", sort_label(current_sort))),
            "Sort",
        )
        .dropdown_menu(move |mut menu, _, _| {
            for (sort, label) in SORTS {
                let owner = sort_owner.clone();
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .checked(sort == current_sort)
                        .on_click(move |_, _, cx| {
                            let _ = owner
                                .update(cx, |workspace, cx| workspace.set_search_sort(sort, cx));
                        }),
                );
            }
            menu
        });
        let summary = self.search_panel_summary(cx);
        let summary_text = summary
            .as_ref()
            .map(|summary| summary.summary.clone())
            .unwrap_or_default();
        let explanation = summary
            .as_ref()
            .and_then(|summary| summary.explanation.clone())
            .filter(|text| !text.is_empty());
        let footer = summary.map(|summary| summary.footer).unwrap_or_default();
        let without_text = panel.results.pages_without_text() > 0;
        let list = self.render_search_rows(panel, window, cx);
        let try_visual = Button::new(DOCUMENT_SEARCH_TRY_VISUAL_ID)
            .debug_selector(|| DOCUMENT_SEARCH_TRY_VISUAL_ID.into())
            .accessibility_id(DOCUMENT_SEARCH_TRY_VISUAL_ID)
            .outline()
            .label("Try Visual search")
            .on_click(cx.listener(|workspace, _: &ClickEvent, window, cx| {
                workspace.choose_search_mode(SearchMode::Visual, window, cx)
            }));
        let theme = cx.theme();
        let query = gpui::div()
            .id(DOCUMENT_SEARCH_INPUT_ID)
            .debug_selector(|| DOCUMENT_SEARCH_INPUT_ID.into())
            .w_full()
            .child(
                Input::new(&panel.input)
                    .accessibility_id(DOCUMENT_SEARCH_INPUT_ID)
                    .aria_label("Search text and markups")
                    .cleanable(true)
                    .prefix(Icon::new(IconName::Search).text_color(theme.muted_foreground)),
            );
        // The toggles are one group: icon buttons a half unit apart, as in
        // the Page Thumbnails header.
        let toggles = h_flex()
            .id(DOCUMENT_SEARCH_TOGGLES_ID)
            .debug_selector(|| DOCUMENT_SEARCH_TOGGLES_ID.into())
            .gap_1()
            .child(match_case)
            .child(explain)
            .child(collapse)
            .child(more_context)
            .child(sort);
        let controls = crate::property_controls::property_body()
            .id(DOCUMENT_SEARCH_CONTROLS_ID)
            .debug_selector(|| DOCUMENT_SEARCH_CONTROLS_ID.into())
            .child(scope)
            .child(mode)
            .child(
                v_flex()
                    .w_full()
                    .gap(panel_space())
                    .child(query)
                    .child(toggles)
                    .when_some(explanation, |group, explanation| {
                        group.child(
                            gpui::div()
                                .id(DOCUMENT_SEARCH_EXPLANATION_ID)
                                .debug_selector(|| DOCUMENT_SEARCH_EXPLANATION_ID.into())
                                .w_full()
                                .p(panel_space())
                                .rounded(theme.radius)
                                .bg(theme.muted)
                                .text_color(theme.foreground)
                                .child(explanation),
                        )
                    }),
            );
        column
            .child(search_controls_region(controls, &panel.controls_scroll))
            .child(search_summary_line(
                DOCUMENT_SEARCH_SUMMARY_ID,
                summary_text,
                theme.muted_foreground,
            ))
            .child(
                gpui::div()
                    .flex_1()
                    .flex_shrink_0()
                    .min_h(crate::layout_metrics::scaled(RESULTS_MIN_HEIGHT))
                    .relative()
                    .child(crate::layout_metrics::inside_line(
                        crate::layout_metrics::Edge::Top,
                        theme.border,
                    ))
                    .child(list),
            )
            .when(!footer.is_empty(), |column| {
                column.child(
                    v_flex()
                        .id(DOCUMENT_SEARCH_FOOTER_ID)
                        .debug_selector(|| DOCUMENT_SEARCH_FOOTER_ID.into())
                        .flex_none()
                        .relative()
                        .gap(panel_space())
                        .p(panel_space())
                        .child(crate::layout_metrics::inside_line(
                            crate::layout_metrics::Edge::Top,
                            theme.border,
                        ))
                        .text_color(theme.muted_foreground)
                        .children(footer)
                        // Pages with no text (scans, text drawn as lines)
                        // can still be searched by their look.
                        .when(without_text, |footer| {
                            footer.child(h_flex().child(try_visual))
                        }),
                )
            })
            .into_any_element()
    }

    /// The result list: only the rows in view are built.
    fn render_search_rows(
        &self,
        panel: &SearchPanel,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let rows = panel.rows.clone();
        let selected = panel.selected;
        let opened = panel.opened;
        let more_context = panel.more_context;
        let focused_list = panel.results_focus.clone();
        let list_owner = cx.entity().downgrade();
        let paths: Rc<HashMap<u64, SharedString>> = Rc::new(
            panel
                .documents
                .iter()
                .map(|(id, document)| (*id, document.path.display().to_string().into()))
                .collect(),
        );
        gpui::div()
            .id(DOCUMENT_SEARCH_RESULTS_ID)
            .debug_selector(|| DOCUMENT_SEARCH_RESULTS_ID.into())
            .key_context(SEARCH_RESULTS_CONTEXT)
            .track_focus(&panel.results_focus)
            .size_full()
            .relative()
            .on_action(cx.listener(|workspace, _: &SearchSelectPrevious, _, cx| {
                workspace.step_search_selection(false, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchSelectNext, _, cx| {
                workspace.step_search_selection(true, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchCollapseRow, _, cx| {
                workspace.collapse_search_row(true, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchExpandRow, _, cx| {
                workspace.collapse_search_row(false, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchOpenRow, window, cx| {
                workspace.open_selected_search_row(window, cx)
            }))
            .child(
                gpui::list(panel.list.clone(), move |index, window, cx| {
                    let Some((key, row)) = rows.get(index).cloned() else {
                        return gpui::div().into_any_element();
                    };
                    let list_focused = focused_list.is_focused(window);
                    let path = match key {
                        RowKey::Document(id) => paths.get(&id).cloned(),
                        _ => None,
                    };
                    search_row_element(
                        index,
                        key,
                        row,
                        path,
                        RowState {
                            selected: selected == Some(key),
                            list_focused,
                            opened: opened == Some(key),
                            more_context,
                        },
                        list_owner.clone(),
                        cx,
                    )
                })
                .size_full(),
            )
            .vertical_scrollbar(&panel.list)
            .into_any_element()
    }
}

#[derive(Clone, Copy)]
struct RowState {
    selected: bool,
    list_focused: bool,
    opened: bool,
    more_context: bool,
}

/// A snippet: each match bold and marked, the rest plain.
fn snippet_text(
    before: &str,
    matched: &str,
    after: &str,
    marks: &[std::ops::Range<usize>],
    mark: gpui::Hsla,
) -> StyledText {
    let text = format!("{before}{matched}{after}");
    let style = HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        background_color: Some(mark),
        ..Default::default()
    };
    let offset = before.len();
    let ranges = if marks.is_empty() {
        // The whole match is one mark.
        std::iter::once((offset, offset + matched.len()))
            .map(|(start, end)| (start..end, style))
            .collect::<Vec<_>>()
    } else {
        marks
            .iter()
            .filter(|range| range.end <= matched.len())
            .map(|range| (offset + range.start..offset + range.end, style))
            .collect::<Vec<_>>()
    };
    StyledText::new(text).with_highlights(ranges)
}

fn search_row_element(
    index: usize,
    key: RowKey,
    row: SearchPanelRow,
    path: Option<SharedString>,
    state: RowState,
    owner: WeakEntity<DocumentWorkspace>,
    cx: &mut App,
) -> gpui::AnyElement {
    let theme = cx.theme();
    let mark = theme.warning.opacity(0.35);
    let indent = match key {
        RowKey::Document(_) => 0.,
        RowKey::Page(..) => ROW_INDENT,
        RowKey::Hit(..) => 2. * ROW_INDENT,
    };
    let chevron = |collapsed: bool| {
        Icon::new(if collapsed {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        })
        .xsmall()
        .text_color(theme.muted_foreground)
    };
    let count = |count: usize| {
        gpui::div()
            .flex_none()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(thousands(count))
    };
    let clamp = |element: gpui::Div| {
        if state.more_context {
            element.line_clamp(3)
        } else {
            element.truncate()
        }
    };
    let content: gpui::AnyElement = match row {
        SearchPanelRow::Document {
            name,
            count: hits,
            collapsed,
        } => h_flex()
            .gap_1()
            .w_full()
            .child(chevron(collapsed))
            .child(
                gpui::div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_ellipsis_middle()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(name),
            )
            .child(count(hits))
            .into_any_element(),
        SearchPanelRow::Page {
            page_number,
            count: hits,
            collapsed,
        } => h_flex()
            .gap_1()
            .w_full()
            .child(chevron(collapsed))
            .child(
                gpui::div()
                    .flex_1()
                    .min_w_0()
                    .text_sm()
                    .child(format!("Page {page_number}")),
            )
            .child(count(hits))
            .into_any_element(),
        SearchPanelRow::Text {
            before,
            matched,
            after,
            marks,
        } => clamp(gpui::div().w_full().text_sm())
            .child(snippet_text(&before, &matched, &after, &marks, mark))
            .into_any_element(),
        SearchPanelRow::Markup {
            heading,
            before,
            matched,
            after,
            replies,
            reply: _,
        } => v_flex()
            .w_full()
            .gap_0p5()
            .child(
                h_flex()
                    .gap_1()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(
                        Icon::default()
                            .path(crate::application_assets::MARKUP_RESULT_ICON)
                            .xsmall(),
                    )
                    .child(gpui::div().min_w_0().truncate().child(heading)),
            )
            .when(
                !(before.is_empty() && matched.is_empty() && after.is_empty()),
                |column| {
                    column.child(clamp(gpui::div().w_full().text_sm()).child(snippet_text(
                        &before,
                        &matched,
                        &after,
                        &[],
                        mark,
                    )))
                },
            )
            .children(replies.into_iter().take(SHOWN_REPLIES).map(|reply| {
                gpui::div()
                    .w_full()
                    .pl_2()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .truncate()
                    .child(format!("↳ {reply}"))
            }))
            .into_any_element(),
        SearchPanelRow::Place => gpui::div()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child("The page matches")
            .into_any_element(),
    };
    let background = if state.selected && state.list_focused {
        theme.list_active
    } else if state.opened || state.selected {
        theme.list_even
    } else {
        theme.transparent
    };
    let click_owner = owner.clone();
    gpui::div()
        .id(gpui::ElementId::Name(search_panel_row_id(index).into()))
        .debug_selector(move || search_panel_row_id(index))
        .w_full()
        .py_1()
        // Clear of the overlay scroll bar.
        .pr(crate::layout_metrics::scaled(
            crate::layout_metrics::GROUP_SPACE,
        ))
        .pl(crate::layout_metrics::scaled(
            crate::layout_metrics::SPACE + indent,
        ))
        .bg(background)
        .when(state.selected && state.list_focused, |row| {
            row.border_l_2().border_color(theme.list_active_border)
        })
        .hover(|row| row.bg(theme.list_hover))
        .cursor_pointer()
        // A document row's tooltip gives the file's full path.
        .when_some(path, |row, path| {
            row.tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(path.clone()).build(window, cx)
            })
        })
        .child(content)
        .on_click(move |_, window, cx| {
            let _ = click_owner.update(cx, |workspace, cx| {
                workspace.activate_search_row(key, window, cx)
            });
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows that arrive above the top of the list leave it at its top; a
    /// list scrolled down keeps the rows it shows.
    #[test]
    fn rows_arriving_above_keep_the_list_at_its_top() {
        let list = ListState::new(3, ListAlignment::Top, px(100.));
        splice_keeping_top(&list, 0..0, 2);
        assert_eq!(list.logical_scroll_top().item_ix, 0);
        let list = ListState::new(3, ListAlignment::Top, px(100.));
        list.scroll_to(gpui::ListOffset {
            item_ix: 1,
            offset_in_item: px(0.),
        });
        splice_keeping_top(&list, 0..0, 2);
        assert_eq!(list.logical_scroll_top().item_ix, 3);
    }
    use crate::search_run::{PageOutcome, SearchEvent};
    use crate::search_text::Snippet;

    #[test]
    fn the_shortcut_follows_the_platform() {
        assert_eq!(search_panel_shortcut(true), "cmd-shift-f");
        assert_eq!(search_panel_shortcut(false), "ctrl-shift-f");
        assert_eq!(search_panel_shortcut_label(true), "Cmd+Shift+F");
        assert_eq!(search_panel_shortcut_label(false), "Ctrl+Shift+F");
    }

    #[test]
    fn the_summary_reads_as_the_search_goes() {
        assert_eq!(search_summary_text(true, true, 0, 0), "");
        assert_eq!(search_summary_text(false, false, 0, 0), "Searching…");
        assert_eq!(search_summary_text(false, true, 0, 0), "No results");
        assert_eq!(
            search_summary_text(false, true, 1, 1),
            "1 result in 1 document"
        );
        assert_eq!(
            search_summary_text(false, true, 1_234, 3),
            "1,234 results in 3 documents"
        );
        assert_eq!(
            search_summary_text(false, false, 5, 2),
            "5 results in 2 documents, searching…"
        );
    }

    #[test]
    fn the_footer_tells_of_the_cap_and_pages_without_text() {
        assert!(search_footer_lines(false, 0).is_empty());
        assert_eq!(
            search_footer_lines(false, 1),
            ["1 page has no searchable text."]
        );
        assert_eq!(
            search_footer_lines(true, 1_200),
            [
                "First 10,000 results. Narrow the search to see more.",
                "1,200 pages have no searchable text."
            ]
        );
    }

    #[test]
    fn a_long_snippet_start_is_cut_unless_more_context_is_on() {
        let before = "a".repeat(40);
        let cut = compact_before(&before, false);
        assert!(cut.starts_with('…'));
        assert_eq!(cut.chars().count(), COMPACT_BEFORE + 1);
        assert_eq!(compact_before(&before, true), before);
        assert_eq!(compact_before("short ", false), "short ");
    }

    #[test]
    fn names_two_windows_share_say_their_window() {
        let labels = document_labels(&[
            (1, "plan.pdf".into(), 1),
            (2, "plan.pdf".into(), 2),
            (3, "notes.pdf".into(), 1),
            (4, "notes.pdf".into(), 1),
        ]);
        assert_eq!(labels[&1], "plan.pdf · window 1");
        assert_eq!(labels[&2], "plan.pdf · window 2");
        // Two tabs of one window: the tabs tell them apart.
        assert_eq!(labels[&3], "notes.pdf");
        assert_eq!(labels[&4], "notes.pdf");
    }

    fn hit(document_id: u64, page_index: u32) -> SearchHit {
        SearchHit {
            document_id,
            page_index,
            target: HitTarget::Page,
            snippet: Snippet::default(),
            quads: Vec::new(),
            bounds: None,
        }
    }

    fn results() -> SearchResults {
        let mut results = SearchResults::new([
            (1, "b.pdf".to_owned(), 4),
            (2, "a.pdf".to_owned(), 4),
            (3, "c.pdf".to_owned(), 4),
        ]);
        for (document_id, page_index, hits) in [(1, 0, 2), (1, 2, 1), (2, 1, 3)] {
            results.apply(SearchEvent::Page(PageOutcome {
                document_id,
                page_index,
                hits: (0..hits).map(|_| hit(document_id, page_index)).collect(),
                has_text: true,
                error: None,
            }));
        }
        results
    }

    #[test]
    fn rows_group_by_document_then_page_and_skip_documents_without_hits() {
        let rows = build_rows(
            &results(),
            SearchSort::DocumentOrder,
            false,
            &HashSet::new(),
        );
        assert_eq!(
            rows,
            [
                RowKey::Document(1),
                RowKey::Page(1, 0),
                RowKey::Hit(1, 0, 0),
                RowKey::Hit(1, 0, 1),
                RowKey::Page(1, 2),
                RowKey::Hit(1, 2, 0),
                RowKey::Document(2),
                RowKey::Page(2, 1),
                RowKey::Hit(2, 1, 0),
                RowKey::Hit(2, 1, 1),
                RowKey::Hit(2, 1, 2),
            ]
        );
    }

    #[test]
    fn collapse_shows_documents_and_pages_and_a_group_can_differ() {
        let mut toggled = HashSet::new();
        let rows = build_rows(&results(), SearchSort::DocumentOrder, true, &toggled);
        assert_eq!(
            rows,
            [
                RowKey::Document(1),
                RowKey::Page(1, 0),
                RowKey::Page(1, 2),
                RowKey::Document(2),
                RowKey::Page(2, 1),
            ]
        );
        // One page opened under Collapse results, one document closed.
        toggled.insert(GroupKey::Page(1, 2));
        toggled.insert(GroupKey::Document(2));
        let rows = build_rows(&results(), SearchSort::DocumentOrder, true, &toggled);
        assert_eq!(
            rows,
            [
                RowKey::Document(1),
                RowKey::Page(1, 0),
                RowKey::Page(1, 2),
                RowKey::Hit(1, 2, 0),
                RowKey::Document(2),
            ]
        );
    }

    #[test]
    fn rows_follow_the_sort() {
        let documents = |sort| {
            build_rows(&results(), sort, true, &HashSet::new())
                .into_iter()
                .filter_map(|row| match row {
                    RowKey::Document(id) => Some(id),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(documents(SearchSort::DocumentOrder), [1, 2]);
        assert_eq!(documents(SearchSort::NameAscending), [2, 1]);
        assert_eq!(documents(SearchSort::NameDescending), [1, 2]);
        // Three hits each: a tie keeps the given order.
        assert_eq!(documents(SearchSort::MostResults), [1, 2]);
    }

    #[test]
    fn every_sort_has_a_menu_label() {
        assert_eq!(sort_label(SearchSort::DocumentOrder), "Document order");
        assert_eq!(
            sort_label(SearchSort::MarkupsOldest),
            "Markups oldest first"
        );
        assert_eq!(SORTS.len(), 6);
    }
}
