//! The find bar (Cmd+F, Ctrl+F): a bar floating at the top right of the
//! canvas that finds literal text in the open document's page text and
//! markups. It shows "3 of 17", steps through the matches with Enter and
//! Shift+Enter, and highlights them on the pages in view, the current one
//! stronger. Each tab keeps its own bar.
//!
//! The search itself is the engine's (`search_run`): it reads one page at a
//! time on a background thread, through the document's PDF worker after
//! everything the viewer asks for, and sends what it finds page by page.
//! Each change of the query, an option or the document starts a new search
//! and ends the old one within one page.

use std::time::Duration;

use gpui::{Global, Task};
use gpui_component::Icon;

use super::*;
use crate::{
    pdf_page_structure::SourcePagePlaces,
    pdf_text_selection::Quad,
    search_document::{DocumentTextEdits, ResourceSearchDocument},
    search_markups::markup_records,
    search_query::{SearchOptions, SearchQuery},
    search_run::{
        HitTarget, SearchConfig, SearchDocument, SearchEvent, SearchHit, SearchResults, SearchRun,
        step_hit,
    },
    search_text_cache::SharedSearchTextCache,
};

/// Match case and Match whole word, a pair in the bar.
pub const DOCUMENT_FIND_OPTIONS_ID: &str = "document-find-options";
/// Previous match and Next match, a pair in the bar.
pub const DOCUMENT_FIND_STEPS_ID: &str = "document-find-steps";

/// The key context of the bar: the toggles' shortcuts act inside it.
pub(super) const FIND_BAR_CONTEXT: &str = "FindBar";
/// How long the query rests before a search starts.
const FIND_QUERY_DELAY: Duration = Duration::from_millis(150);
/// How long the document rests, after an edit, before the search runs again.
const FIND_EDIT_DELAY: Duration = Duration::from_millis(400);
/// How often the bar takes what its search found. A timer, not a wait on
/// the search thread, so that no search thread wakes the window's executor
/// (and tests advance it with the clock).
const FIND_UPDATE_INTERVAL: Duration = Duration::from_millis(50);
/// The longest query a selection seeds, in characters.
const FIND_SEED_CHARACTERS: usize = 256;
/// The bar's width, when the canvas has room: the field, the count and
/// five medium buttons, on the grid.
const FIND_BAR_WIDTH: f32 = 480.;
/// The count's width: room for "1,234 of 5,678".
const FIND_COUNT_WIDTH: f32 = 80.;
/// The field's least width before the bar shortens it no more.
const FIND_FIELD_MIN_WIDTH: f32 = 128.;
/// The space between the bar and a match scrolled into view under it.
const FIND_BAR_REVEAL_GAP: f32 = 16.;
/// The bar's distance from the top and right edges of the canvas.
const FIND_BAR_INSET: f32 = 8.;

/// The shortcut that opens the bar, as a keystroke: the platform's command
/// key with F.
pub const fn find_shortcut(macos: bool) -> &'static str {
    if macos { "cmd-f" } else { "ctrl-f" }
}

/// The shortcut for the next match when the focus is in the document.
pub const fn find_next_shortcut(macos: bool) -> &'static str {
    if macos { "cmd-g" } else { "f3" }
}

/// The shortcut for the previous match when the focus is in the document.
pub const fn find_previous_shortcut(macos: bool) -> &'static str {
    if macos { "cmd-shift-g" } else { "shift-f3" }
}

/// The Match case toggle's shortcut, inside the bar.
pub const fn find_match_case_shortcut(macos: bool) -> &'static str {
    if macos { "cmd-alt-c" } else { "alt-c" }
}

/// The Match whole word toggle's shortcut, inside the bar.
pub const fn find_whole_word_shortcut(macos: bool) -> &'static str {
    if macos { "cmd-alt-w" } else { "alt-w" }
}

/// What the bar was read from. A search runs again when it changes.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct FindRead {
    resource_epoch: u64,
    revision: u64,
    slots_revision: u64,
    slots: Vec<crate::pdf_page_structure::PageSlot>,
}

/// The pages' text a search has read, for every window. A document's pages
/// are keyed by its id, which no two open documents share.
#[derive(Default)]
struct FindTextCache(SharedSearchTextCache);

impl Global for FindTextCache {}

/// One document, ready for a search to read.
pub(super) struct SearchSource {
    pub(super) document: Arc<dyn SearchDocument>,
    pub(super) read: FindRead,
    /// Its markups, for the rows that show them.
    pub(super) records: Arc<Vec<crate::search_markups::MarkupSearchRecord>>,
    pub(super) title: String,
    pub(super) path: std::path::PathBuf,
}

/// One document's find bar.
pub(super) struct FindBar {
    pub(super) open: bool,
    pub(super) input: Entity<InputState>,
    _input_events: Subscription,
    pub(super) options: SearchOptions,
    /// The options of the search that runs or ran.
    options_run: SearchOptions,
    /// The text of the search that runs or ran.
    pub(super) query: String,
    pub(super) results: SearchResults,
    run: Option<SearchRun>,
    /// Counts the searches, so that an event of an old one is left out.
    generation: u64,
    /// The page the search started at: the one in view.
    start_page: u32,
    /// The current match: its page and its place among that page's matches.
    pub(super) current: Option<(u32, usize)>,
    /// Waits for the query or the document to rest.
    delay: Option<Task<()>>,
    read: Option<FindRead>,
}

/// What a find bar shows, for tests and the review driver's log.
#[derive(Clone, Debug, PartialEq)]
pub struct FindBarSummary {
    pub open: bool,
    /// The text in the field.
    pub text: String,
    pub match_case: bool,
    pub whole_word: bool,
    /// The matches found so far.
    pub hits: usize,
    /// The search has read every page.
    pub finished: bool,
    /// The current match: its page, and its place (from 1) among all.
    pub current: Option<(u32, usize)>,
    /// The count beside the field.
    pub count: String,
}

/// The matches painted on one page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindHighlights {
    pub page_index: u32,
    pub matches: Vec<[(f64, f64); 4]>,
    pub current: Vec<[(f64, f64); 4]>,
    /// A markup's match is an outline round the markup.
    pub outlines: Vec<[(f64, f64); 4]>,
    pub outline_current: Vec<[(f64, f64); 4]>,
}

/// A match to bring into view once its page is laid out.
#[derive(Clone, Copy)]
pub(super) struct PendingFindReveal {
    document_id: DocumentId,
    page_index: u32,
    rect: PdfRect,
    /// Frames to try before giving up.
    frames: u8,
}

/// How a rect was brought into view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RevealOutcome {
    /// The page is shown: the view moved to the rect, or it was in view.
    Revealed,
    /// The page was not shown, so the view went to its page.
    Navigated,
    /// Nothing to reveal: no such page, or the view is not laid out.
    Unavailable,
}

/// How the count reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FindCountTone {
    Muted,
    /// "No results".
    None,
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

/// The count's text: "3 of 17" when the search is done, "17+" while it
/// reads on (or stopped at its limit), "Searching…" before the first match,
/// "No results" when there is none, and nothing for an empty query.
pub(super) fn find_count_text(
    query_empty: bool,
    finished: bool,
    limit_reached: bool,
    hits: usize,
    position: Option<usize>,
) -> (String, FindCountTone) {
    if query_empty {
        return (String::new(), FindCountTone::Muted);
    }
    if hits == 0 {
        return if finished {
            ("No results".into(), FindCountTone::None)
        } else {
            ("Searching…".into(), FindCountTone::Muted)
        };
    }
    if !finished || limit_reached {
        return (format!("{}+", thousands(hits)), FindCountTone::Muted);
    }
    let text = match position {
        Some(position) => format!("{} of {}", thousands(position + 1), thousands(hits)),
        None => thousands(hits),
    };
    (text, FindCountTone::Muted)
}

/// The first line of `text`, as a query.
pub(super) fn seed_query(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(line.chars().take(FIND_SEED_CHARACTERS).collect())
}

/// The box round some quads: x0, y0, x1, y1.
pub(super) fn quads_box(quads: &[Quad]) -> Option<[f64; 4]> {
    let mut points = quads.iter().flatten();
    let first = points.next()?;
    let mut bounds = [first.0, first.1, first.0, first.1];
    for (x, y) in points {
        bounds = [
            bounds[0].min(*x),
            bounds[1].min(*y),
            bounds[2].max(*x),
            bounds[3].max(*y),
        ];
    }
    Some(bounds)
}

/// A box as a quad: bottom left, bottom right, top right, top left.
fn box_quad(bounds: [f64; 4]) -> Quad {
    [
        (bounds[0], bounds[1]),
        (bounds[2], bounds[1]),
        (bounds[2], bounds[3]),
        (bounds[0], bounds[3]),
    ]
}

/// The place of a match among the document's, in document order.
fn position_of(results: &SearchResults, current: (u32, usize)) -> Option<usize> {
    let document = results.documents().first()?;
    let mut before = 0;
    for (page, hits) in document.pages() {
        if page == current.0 {
            return (current.1 < hits.len()).then_some(before + current.1);
        }
        if page > current.0 {
            return None;
        }
        before += hits.len();
    }
    None
}

/// The match at `position` in document order, as its page and place on it.
fn key_at(results: &SearchResults, position: usize) -> Option<(u32, usize)> {
    let document = results.documents().first()?;
    let mut before = 0;
    for (page, hits) in document.pages() {
        if position < before + hits.len() {
            return Some((page, position - before));
        }
        before += hits.len();
    }
    None
}

fn hit_at(results: &SearchResults, key: (u32, usize)) -> Option<&SearchHit> {
    results
        .documents()
        .first()?
        .pages()
        .find(|(page, _)| *page == key.0)
        .and_then(|(_, hits)| hits.get(key.1))
}

impl DocumentWorkspace {
    /// The pages' text that every search of every window shares.
    pub(super) fn find_cache(cx: &mut App) -> SharedSearchTextCache {
        cx.default_global::<FindTextCache>().0.clone()
    }

    /// Whether the active document's find bar is open.
    pub(super) fn find_bar_open(&self) -> bool {
        self.active_document_id
            .and_then(|id| self.find_bars.get(&id))
            .is_some_and(|bar| bar.open)
    }

    /// Whether Escape, with the focus on the page, has something to clear
    /// before the find bar: a text or markup selection, a drawing in
    /// progress, a placement, the Pan tool or a tool other than Select.
    pub(super) fn escape_has_page_state(&self, document_id: DocumentId, cx: &App) -> bool {
        let workspace_state = self.pan_tool_active
            || self.pan_drag.is_some()
            || self.text_selection.is_some()
            || self.active_annotation_pointer.is_some()
            || self.text_box_placement.is_some()
            || self.placement_press.is_some();
        workspace_state
            || self.session(document_id, cx).is_some_and(|session| {
                let annotations = &session.read(cx).annotations;
                annotations.tool() != AnnotationTool::Select
                    || annotations.has_selection(document_id.value())
                    || annotations.active_surface().is_some()
            })
    }

    /// Whether a press at `position` is on the bar: it is the bar's, and
    /// starts nothing on the page under it.
    pub(super) fn find_bar_owns(&self, position: gpui::Point<Pixels>) -> bool {
        self.find_bar_open()
            && self
                .find_bar_bounds
                .get()
                .is_some_and(|bounds| bounds.contains(&position))
    }

    /// The bar's text field, made when the bar is first opened.
    fn find_input(
        &mut self,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(bar) = self.find_bars.get(&document_id) {
            return bar.input.clone();
        }
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in document"));
        let events = cx.subscribe_in(
            &input,
            window,
            move |workspace, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => workspace.schedule_find(document_id, FIND_QUERY_DELAY, cx),
                InputEvent::PressEnter { shift, .. } => {
                    workspace.step_find_for(document_id, !*shift, cx)
                }
                _ => {}
            },
        );
        self.find_bars.insert(
            document_id,
            FindBar {
                open: false,
                input: input.clone(),
                _input_events: events,
                options: SearchOptions::default(),
                options_run: SearchOptions::default(),
                query: String::new(),
                results: SearchResults::default(),
                run: None,
                generation: 0,
                start_page: 0,
                current: None,
                delay: None,
                read: None,
            },
        );
        input
    }

    /// Cmd+F or Ctrl+F: opens the active document's bar with the field
    /// focused and its text selected. Selected page text becomes the query.
    pub(super) fn open_find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let seed = self
            .selected_page_text(document_id, cx)
            .as_deref()
            .and_then(seed_query);
        let input = self.find_input(document_id, window, cx);
        let was_open = self.find_bars.get(&document_id).is_some_and(|bar| bar.open);
        if let Some(bar) = self.find_bars.get_mut(&document_id) {
            bar.open = true;
        }
        if let Some(seed) = seed {
            input.update(cx, |input, cx| input.set_value(seed, window, cx));
        }
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        // A bar opened again has no results of the last time: its field
        // holds the query, and the search runs now.
        if !was_open || self.find_bars[&document_id].run.is_none() {
            self.restart_find(document_id, cx);
        }
        cx.notify();
    }

    /// The X and Escape: closes the bar, ends its search, clears the
    /// highlights and gives the focus back to the document. The field keeps
    /// its text for the next time.
    pub(super) fn close_find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(bar) = self.find_bars.get_mut(&document_id).filter(|bar| bar.open) else {
            return;
        };
        bar.open = false;
        bar.run = None;
        bar.delay = None;
        bar.generation += 1;
        bar.results = SearchResults::default();
        bar.current = None;
        bar.read = None;
        self.find_bar_bounds.set(None);
        if self
            .find_reveal
            .is_some_and(|pending| pending.document_id == document_id)
        {
            self.find_reveal = None;
        }
        self.workspace_focus.focus(window, cx);
        cx.notify();
    }

    /// A document closed: its bar and its pages' text go.
    pub(super) fn forget_find_bar(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        if self.find_bars.remove(&document_id).is_some() {
            Self::find_cache(cx)
                .lock()
                .remove_document(document_id.value());
        }
        if self
            .find_reveal
            .is_some_and(|pending| pending.document_id == document_id)
        {
            self.find_reveal = None;
        }
    }

    pub(super) fn toggle_find_match_case(&mut self, cx: &mut Context<Self>) {
        self.toggle_find_option(|options| options.match_case = !options.match_case, cx);
    }

    pub(super) fn toggle_find_whole_word(&mut self, cx: &mut Context<Self>) {
        self.toggle_find_option(|options| options.whole_word = !options.whole_word, cx);
    }

    fn toggle_find_option(
        &mut self,
        change: impl FnOnce(&mut SearchOptions),
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(bar) = self.find_bars.get_mut(&document_id).filter(|bar| bar.open) else {
            return;
        };
        change(&mut bar.options);
        self.restart_find(document_id, cx);
        cx.notify();
    }

    /// Waits for the query to rest, then searches.
    fn schedule_find(&mut self, document_id: DocumentId, delay: Duration, cx: &mut Context<Self>) {
        let Some(bar) = self.find_bars.get_mut(&document_id) else {
            return;
        };
        bar.delay = Some(cx.spawn(async move |workspace, cx| {
            cx.background_executor().timer(delay).await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.restart_find_if_changed(document_id, cx)
            });
        }));
    }

    /// [`Self::restart_find`], unless the search that ran already read this
    /// text, with these options, from this document as it is. A change that
    /// was undone, or a value set twice, does not search again and move the
    /// current match.
    fn restart_find_if_changed(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let unchanged = self.find_bars.get(&document_id).is_some_and(|bar| {
            bar.run.is_some()
                && bar.query == bar.input.read(cx).value().as_ref()
                && bar.options_run == bar.options
                && bar
                    .read
                    .as_ref()
                    .is_some_and(|read| self.find_read_unchanged(document_id, read, cx))
        });
        if unchanged {
            // The wait is over: a later change schedules a new one.
            if let Some(bar) = self.find_bars.get_mut(&document_id) {
                bar.delay = None;
            }
            return;
        }
        self.restart_find(document_id, cx);
    }

    /// Whether the document is still as `read` found it. It copies nothing,
    /// so it can run each frame. Pages arranged back as they were count as
    /// unchanged.
    pub(super) fn find_read_unchanged(
        &self,
        document_id: DocumentId,
        read: &FindRead,
        cx: &App,
    ) -> bool {
        let Some(session) = self.session(document_id, cx) else {
            return false;
        };
        let session = session.read(cx);
        read.resource_epoch == session.resource_epoch
            && read.revision == session.annotations.document_revision(document_id.value())
            && (read.slots_revision == session.arranged_revision
                || read.slots == session.arranged_slots)
    }

    /// Ends the search that runs and starts one for the field's text and the
    /// options, from the page in view.
    pub(super) fn restart_find(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let Some(bar) = self.find_bars.get_mut(&document_id) else {
            return;
        };
        bar.delay = None;
        bar.run = None;
        bar.generation += 1;
        let generation = bar.generation;
        let text = bar.input.read(cx).value().to_string();
        let options = bar.options;
        bar.options_run = options;
        bar.query = text.clone();
        bar.results = SearchResults::default();
        bar.current = None;
        if !bar.open || text.trim().is_empty() {
            cx.notify();
            return;
        }
        let start_page = self
            .session(document_id, cx)
            .map_or(0, |session| session.read(cx).current_page);
        let Some((document, read)) = self.find_search_document(document_id, start_page, cx) else {
            // A document that is not open yet has nothing to search.
            if let Some(bar) = self.find_bars.get_mut(&document_id) {
                bar.results
                    .apply(SearchEvent::Finished { cancelled: false });
            }
            cx.notify();
            return;
        };
        let name = document.name().to_owned();
        let page_count = document.page_count();
        let query = Arc::new(SearchQuery::literal(&text, options));
        let (events, receiver) = async_channel::unbounded::<SearchEvent>();
        let run = SearchRun::start(
            vec![document],
            query,
            SearchConfig::default(),
            move |event| {
                let _ = events.send_blocking(event);
            },
        );
        let Some(bar) = self.find_bars.get_mut(&document_id) else {
            return;
        };
        bar.results = SearchResults::new([(document_id.value(), name, page_count)]);
        bar.start_page = start_page;
        bar.read = Some(read);
        bar.run = Some(run);
        cx.spawn(async move |workspace, cx| {
            loop {
                cx.background_executor().timer(FIND_UPDATE_INTERVAL).await;
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
                    // A finished or replaced search takes no more events.
                    batch
                        .into_iter()
                        .all(|event| workspace.apply_find_event(document_id, generation, event, cx))
                });
                if !alive.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    /// The document as a search reads it now: its page text with the edits
    /// on it, and its markups. Page numbers are those shown.
    fn find_search_document(
        &self,
        document_id: DocumentId,
        start_page: u32,
        cx: &mut Context<Self>,
    ) -> Option<(Arc<dyn SearchDocument>, FindRead)> {
        let cache = Self::find_cache(cx);
        let source = self.search_source(document_id, start_page, cache.clone(), cx)?;
        // Pages that moved are not the pages the cache holds.
        if self
            .find_bars
            .get(&document_id)
            .and_then(|bar| bar.read.as_ref())
            .is_some_and(|last| last.slots != source.read.slots)
        {
            cache.lock().remove_document(document_id.value());
        }
        Some((source.document, source.read))
    }

    /// One document as a search reads it now, for the find bar or the search
    /// panel of any window: its page text with the edits on it, and its
    /// markups. Page numbers are those shown.
    pub(super) fn search_source(
        &self,
        document_id: DocumentId,
        start_page: u32,
        cache: SharedSearchTextCache,
        cx: &App,
    ) -> Option<SearchSource> {
        let session = self.session(document_id, cx)?.read(cx);
        if !matches!(session.status, NativeDocumentStatus::Ready) {
            return None;
        }
        let resource = session.resource.clone()?;
        let page_count = session.page_sizes.len() as u32;
        let title = session.title.clone();
        let path = session.path.clone();
        let epoch = session.resource_epoch;
        let slots = session.arranged_slots.clone();
        let slots_revision = session.arranged_revision;
        let places = SourcePagePlaces::new(&slots);
        let annotations = session
            .annotations
            .annotations_in_document(document_id.value());
        let revision = session.annotations.document_revision(document_id.value());
        // The file's entries name file pages: the document shows them where
        // its pages are now, and not at all once a page is deleted.
        let metadata = session
            .markup_metadata
            .iter()
            .filter_map(|entry| {
                let mut entry = entry.clone();
                entry.page_index = places.shown(entry.page_index)?;
                Some(entry)
            })
            .collect::<Vec<_>>();
        let records = Arc::new(markup_records(
            &annotations,
            &metadata,
            &crate::pdf_engine::markup_author(),
        ));
        let edits = DocumentTextEdits {
            text_edits: self.page_text_edits_of(document_id, cx),
            table_edits: self.page_table_edits_of(document_id, cx),
            content_moves: self.page_content_moves_of(document_id, cx),
            explodes: self.page_content_explodes_of(document_id, cx),
            bases: self.page_bases_of(document_id, cx),
        };
        let read = FindRead {
            resource_epoch: epoch,
            revision,
            slots_revision,
            slots,
        };
        let document = ResourceSearchDocument::new(
            document_id.value(),
            title.clone(),
            page_count,
            resource,
            epoch,
            edits,
            records.clone(),
            cache,
        )
        .starting_at(start_page);
        Some(SearchSource {
            document: Arc::new(document),
            read,
            records,
            title,
            path,
        })
    }

    /// Merges what the search found. False when the event is of a search
    /// that ended, and the task that delivers events stops.
    fn apply_find_event(
        &mut self,
        document_id: DocumentId,
        generation: u64,
        event: SearchEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(bar) = self
            .find_bars
            .get_mut(&document_id)
            .filter(|bar| bar.generation == generation)
        else {
            return false;
        };
        let finished = matches!(event, SearchEvent::Finished { .. });
        bar.results.apply(event);
        if bar.current.is_none() && bar.results.hit_count() > 0 {
            // The first match at or after the page in view; the pages before
            // it come last, so one is chosen only when nothing follows.
            let position = bar
                .results
                .first_hit_from(document_id.value(), bar.start_page)
                .or_else(|| (bar.results.is_finished()).then_some(0));
            if let Some(key) = position.and_then(|position| key_at(&bar.results, position)) {
                bar.current = Some(key);
                self.reveal_find_hit(document_id, cx);
            }
        }
        cx.notify();
        !finished
    }

    /// Enter, Shift+Enter, the chevrons and F3: the next or previous match,
    /// going round at the ends.
    pub(super) fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some(document_id) = self.active_document_id {
            self.step_find_for(document_id, forward, cx);
        }
    }

    fn step_find_for(&mut self, document_id: DocumentId, forward: bool, cx: &mut Context<Self>) {
        let Some(bar) = self.find_bars.get(&document_id).filter(|bar| bar.open) else {
            return;
        };
        // A query still resting is searched first: Enter finds what was typed.
        if bar.input.read(cx).value().as_ref() != bar.query {
            self.restart_find(document_id, cx);
            return;
        }
        let Some(bar) = self.find_bars.get_mut(&document_id) else {
            return;
        };
        let count = bar.results.hit_count();
        let next = match bar.current.and_then(|key| position_of(&bar.results, key)) {
            Some(position) => step_hit(position, count, forward),
            None => (count > 0).then_some(if forward { 0 } else { count - 1 }),
        };
        let Some(key) = next.and_then(|position| key_at(&bar.results, position)) else {
            return;
        };
        bar.current = Some(key);
        // The field keeps the focus, so that Enter goes on.
        self.reveal_find_hit(document_id, cx);
        cx.notify();
    }

    /// The box of the current match, in page space.
    fn current_find_box(&self, document_id: DocumentId) -> Option<(u32, [f64; 4])> {
        let bar = self.find_bars.get(&document_id)?;
        let key = bar.current?;
        let hit = hit_at(&bar.results, key)?;
        let bounds = match &hit.target {
            HitTarget::Text { .. } => quads_box(&hit.quads),
            _ => hit.bounds,
        }?;
        Some((hit.page_index, bounds))
    }

    /// How far from the top of the view a match must be to show below the
    /// bar: the bar's lower edge, and a gap. The bar sits over the page.
    fn find_bar_clearance(&self, document_id: DocumentId) -> f32 {
        let Some(bounds) = self.find_bar_bounds.get().filter(|_| self.find_bar_open()) else {
            return 0.;
        };
        let Some(viewport) = self.viewport_bounds.get(&document_id) else {
            return 0.;
        };
        (f32::from(bounds.bottom()) - f32::from(viewport.top())).max(0.) + FIND_BAR_REVEAL_GAP
    }

    /// Brings the current match into view: in Continuous view the pages
    /// scroll to it, and in Single Page view the view turns to its page.
    fn reveal_find_hit(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let Some((page_index, bounds)) = self.current_find_box(document_id) else {
            return;
        };
        self.reveal_hit_box(document_id, page_index, bounds, cx);
    }

    /// Brings a hit's box (page space: x0, y0, x1, y1) into view, and goes
    /// to its page first when that page is not laid out.
    pub(super) fn reveal_hit_box(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        bounds: [f64; 4],
        cx: &mut Context<Self>,
    ) {
        let Ok(rect) = PdfRect::new(
            bounds[0],
            bounds[1],
            (bounds[2] - bounds[0]).max(0.),
            (bounds[3] - bounds[1]).max(0.),
        ) else {
            return;
        };
        match self.reveal_page_rect(
            document_id,
            page_index,
            rect,
            self.find_bar_clearance(document_id),
            cx,
        ) {
            RevealOutcome::Navigated => {
                self.find_reveal = Some(PendingFindReveal {
                    document_id,
                    page_index,
                    rect,
                    frames: 120,
                });
            }
            RevealOutcome::Revealed | RevealOutcome::Unavailable => self.find_reveal = None,
        }
    }

    /// After the view went to a match's page: once that page is laid out,
    /// scrolls to the match within it. Called as a frame starts, before the
    /// frame's pages record where they are.
    pub(super) fn retry_find_reveal(&mut self, cx: &mut Context<Self>) {
        let Some(mut pending) = self.find_reveal else {
            return;
        };
        let settled = self
            .session(pending.document_id, cx)
            .is_some_and(|session| {
                let session = session.read(cx);
                session.pending_navigation_scroll.is_none()
                    && session.pending_page_view_scroll.is_none()
                    && session.pending_page_frame.is_none()
            })
            && !self.navigation_in_flight.contains(&pending.document_id)
            && self.view_motion.is_none();
        let shown = self
            .page_interactions
            .contains_key(&(pending.document_id, pending.page_index));
        if settled && shown {
            self.find_reveal = None;
            let clearance = self.find_bar_clearance(pending.document_id);
            let _ = self.reveal_page_rect(
                pending.document_id,
                pending.page_index,
                pending.rect,
                clearance,
                cx,
            );
            return;
        }
        pending.frames = pending.frames.saturating_sub(1);
        self.find_reveal = (pending.frames > 0).then_some(pending);
        // Frames come only while something changes: ask for the next.
        if self.find_reveal.is_some() {
            cx.notify();
        }
    }

    /// Runs the search again when the document changed under it, once the
    /// edits pause.
    pub(super) fn poll_find_stale(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(bar) = self.find_bars.get(&document_id) else {
            return;
        };
        if !bar.open || bar.query.trim().is_empty() || bar.delay.is_some() {
            return;
        }
        let Some(read) = bar.read.as_ref() else {
            return;
        };
        if !self.find_read_unchanged(document_id, read, cx) {
            self.schedule_find(document_id, FIND_EDIT_DELAY, cx);
        }
    }

    /// What the document's find bar shows now; `None` before it was ever
    /// opened.
    pub fn find_bar_summary(&self, document_id: DocumentId, cx: &App) -> Option<FindBarSummary> {
        let bar = self.find_bars.get(&document_id)?;
        let (count, _) = self.find_count(document_id, cx);
        Some(FindBarSummary {
            open: bar.open,
            text: bar.input.read(cx).value().to_string(),
            match_case: bar.options.match_case,
            whole_word: bar.options.whole_word,
            hits: bar.results.hit_count(),
            finished: bar.results.is_finished(),
            current: bar
                .current
                .and_then(|key| position_of(&bar.results, key))
                .map(|position| (bar.current.map_or(0, |key| key.0), position + 1)),
            count,
        })
    }

    /// The matches the viewport paints, page by page: those on the pages in
    /// view, and the current one.
    pub fn find_highlights(&self, document_id: DocumentId, cx: &App) -> Vec<FindHighlights> {
        self.find_overlays(document_id, cx)
            .into_iter()
            .map(|overlay| FindHighlights {
                page_index: overlay.page_index,
                matches: overlay.search_matches,
                current: overlay.search_current,
                outlines: overlay.search_outlines,
                outline_current: overlay.search_outline_current,
            })
            .collect()
    }

    /// The count beside the field, and how it reads.
    fn find_count(&self, document_id: DocumentId, cx: &App) -> (String, FindCountTone) {
        let Some(bar) = self.find_bars.get(&document_id) else {
            return (String::new(), FindCountTone::Muted);
        };
        // The field is read now, not as the search last read it: an empty
        // field has no count at once, and a first query waits for its search.
        if bar.input.read(cx).value().trim().is_empty() {
            return (String::new(), FindCountTone::Muted);
        }
        if bar.query.trim().is_empty() {
            return ("Searching…".into(), FindCountTone::Muted);
        }
        find_count_text(
            bar.query.trim().is_empty(),
            bar.results.is_finished(),
            bar.results.limit_reached(),
            bar.results.hit_count(),
            bar.current.and_then(|key| position_of(&bar.results, key)),
        )
    }

    /// The matches on the pages in view, for the viewport to paint over the
    /// page text: all of them, and the current one stronger. The find bar's
    /// when it is open, else those of a search panel result opened here.
    pub(super) fn find_overlays(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<page_text::PageTextOverlay> {
        if let Some(bar) = self
            .find_bars
            .get(&document_id)
            .filter(|bar| bar.open && bar.results.hit_count() > 0)
        {
            let Some(document) = bar.results.documents().first() else {
                return Vec::new();
            };
            return self.hit_overlays(
                document_id,
                |page| {
                    document
                        .pages()
                        .find(|(index, _)| *index == page)
                        .map(|(_, hits)| hits)
                },
                bar.current,
                cx,
            );
        }
        if let Some(marks) = self
            .search_marks
            .as_ref()
            .filter(|marks| marks.document_id == document_id)
        {
            return self.hit_overlays(
                document_id,
                |page| marks.pages.get(&page).map(Vec::as_slice),
                marks.current,
                cx,
            );
        }
        Vec::new()
    }

    /// The overlays of the hits on the pages in view and the current one.
    fn hit_overlays<'a>(
        &self,
        document_id: DocumentId,
        hits_of: impl Fn(u32) -> Option<&'a [SearchHit]>,
        current: Option<(u32, usize)>,
        cx: &App,
    ) -> Vec<page_text::PageTextOverlay> {
        let mut pages = self
            .viewport_painted_layouts
            .get(&document_id)
            .map(|layouts| {
                layouts
                    .iter()
                    .map(|layout| layout.page as u32)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(session) = self.session(document_id, cx) {
            let current = session.read(cx).current_page;
            pages.extend([current.saturating_sub(1), current, current + 1]);
        }
        if let Some((page, _)) = current {
            pages.push(page);
        }
        pages.sort_unstable();
        pages.dedup();
        let mut overlays = Vec::new();
        for page in pages {
            let Some(hits) = hits_of(page) else {
                continue;
            };
            let mut overlay = page_text::PageTextOverlay {
                page_index: page,
                ..Default::default()
            };
            for (index, hit) in hits.iter().enumerate() {
                let current = current == Some((page, index));
                match &hit.target {
                    HitTarget::Text { .. } => {
                        let quads = hit.quads.iter().copied();
                        if current {
                            overlay.search_current.extend(quads);
                        } else {
                            overlay.search_matches.extend(quads);
                        }
                    }
                    HitTarget::Markup { .. } | HitTarget::Visual { .. } => {
                        if let Some(bounds) = hit.bounds {
                            if current {
                                overlay.search_outline_current.push(box_quad(bounds));
                            } else {
                                overlay.search_outlines.push(box_quad(bounds));
                            }
                        }
                    }
                    HitTarget::Page => {}
                }
            }
            overlays.push(overlay);
        }
        overlays
    }

    /// Adds the matches to the overlays the viewport paints for each page.
    pub(super) fn merge_find_overlays(
        &self,
        document_id: DocumentId,
        overlays: &mut Vec<page_text::PageTextOverlay>,
        cx: &App,
    ) {
        for found in self.find_overlays(document_id, cx) {
            match overlays
                .iter_mut()
                .find(|overlay| overlay.page_index == found.page_index)
            {
                Some(overlay) => {
                    overlay.search_matches = found.search_matches;
                    overlay.search_current = found.search_current;
                    overlay.search_outlines = found.search_outlines;
                    overlay.search_outline_current = found.search_outline_current;
                }
                None => overlays.push(found),
            }
        }
    }

    /// The bar over the top right of the canvas, when it is open.
    pub(super) fn render_find_bar(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let document_id = self.active_document_id?;
        let bar = self.find_bars.get(&document_id).filter(|bar| bar.open)?;
        let macos = cfg!(target_os = "macos");
        let (count, tone) = self.find_count(document_id, cx);
        let has_hits = bar.results.hit_count() > 0;
        let options = bar.options;
        let bounds = self.find_bar_bounds.clone();
        let theme = cx.theme();
        let count_colour = match tone {
            FindCountTone::Muted => theme.muted_foreground,
            FindCountTone::None => theme.danger,
        };
        let key = |shortcut: &str| crate::shortcut_label::shortcut_kbd(shortcut);
        let with_key = |button: Button, label: &'static str, shortcut: &str| match key(shortcut) {
            Some(binding) => button.tooltip_with_key_binding(label, binding),
            None => button.tooltip(label),
        };
        let match_case = with_key(
            Button::new(DOCUMENT_FIND_MATCH_CASE_ID)
                .debug_selector(|| DOCUMENT_FIND_MATCH_CASE_ID.into())
                .accessibility_id(DOCUMENT_FIND_MATCH_CASE_ID)
                .ghost()
                .icon(IconName::CaseSensitive)
                .selected(options.match_case)
                .toggled(options.match_case),
            "Match Case",
            if macos { "Cmd+Alt+C" } else { "Alt+C" },
        )
        .on_click(
            cx.listener(|workspace, _: &ClickEvent, _, cx| workspace.toggle_find_match_case(cx)),
        );
        let whole_word = with_key(
            Button::new(DOCUMENT_FIND_WHOLE_WORD_ID)
                .debug_selector(|| DOCUMENT_FIND_WHOLE_WORD_ID.into())
                .accessibility_id(DOCUMENT_FIND_WHOLE_WORD_ID)
                .ghost()
                .icon(Icon::default().path(crate::application_assets::WHOLE_WORD_ICON))
                .selected(options.whole_word)
                .toggled(options.whole_word),
            "Match Whole Word",
            if macos { "Cmd+Alt+W" } else { "Alt+W" },
        )
        .on_click(
            cx.listener(|workspace, _: &ClickEvent, _, cx| workspace.toggle_find_whole_word(cx)),
        );
        let previous = with_key(
            Button::new(DOCUMENT_FIND_PREVIOUS_ID)
                .debug_selector(|| DOCUMENT_FIND_PREVIOUS_ID.into())
                .accessibility_id(DOCUMENT_FIND_PREVIOUS_ID)
                .ghost()
                .icon(IconName::ChevronUp)
                .disabled(!has_hits),
            "Previous Match",
            "Shift+Enter",
        )
        .on_click(cx.listener(|workspace, _: &ClickEvent, _, cx| workspace.step_find(false, cx)));
        let next = with_key(
            Button::new(DOCUMENT_FIND_NEXT_ID)
                .debug_selector(|| DOCUMENT_FIND_NEXT_ID.into())
                .accessibility_id(DOCUMENT_FIND_NEXT_ID)
                .ghost()
                .icon(IconName::ChevronDown)
                .disabled(!has_hits),
            "Next Match",
            "Enter",
        )
        .on_click(cx.listener(|workspace, _: &ClickEvent, _, cx| workspace.step_find(true, cx)));
        // The app's one X size (`close_icon_button`), as in every header.
        let close =
            crate::accessible_button::close_icon_button(DOCUMENT_FIND_CLOSE_ID, "Close find bar")
                .on_click(cx.listener(|workspace, _: &ClickEvent, window, cx| {
                    workspace.close_find_bar(window, cx)
                }));
        let space = crate::layout_metrics::scaled(crate::layout_metrics::SPACE);
        let theme = cx.theme();
        Some(
            gpui::div()
                .absolute()
                .top(px(FIND_BAR_INSET))
                .left(px(FIND_BAR_INSET))
                .right(px(FIND_BAR_INSET))
                .flex()
                .justify_end()
                .child(
                    h_flex()
                        .id(DOCUMENT_FIND_BAR_ID)
                        .debug_selector(|| DOCUMENT_FIND_BAR_ID.into())
                        .key_context(FIND_BAR_CONTEXT)
                        .relative()
                        .items_center()
                        // A bar of medium controls, as the toolbar: 8 + 32 + 8,
                        // its border inside the 48 px.
                        .h(crate::layout_metrics::scaled(crate::layout_metrics::BAR))
                        .gap(space)
                        .px(space)
                        .w(crate::layout_metrics::scaled(FIND_BAR_WIDTH))
                        .max_w_full()
                        .rounded(theme.radius_lg)
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .text_color(theme.popover_foreground)
                        .shadow_md()
                        .cursor(CursorStyle::Arrow)
                        .occlude()
                        .on_action(cx.listener(|workspace, _: &Escape, window, cx| {
                            workspace.close_find_bar(window, cx)
                        }))
                        .on_action(cx.listener(|workspace, _: &ToggleFindMatchCase, _, cx| {
                            workspace.toggle_find_match_case(cx)
                        }))
                        .on_action(cx.listener(|workspace, _: &ToggleFindWholeWord, _, cx| {
                            workspace.toggle_find_whole_word(cx)
                        }))
                        .child(
                            gpui::canvas(move |area, _, _| bounds.set(Some(area)), |_, _, _, _| {})
                                .absolute()
                                .inset_0(),
                        )
                        .child(
                            gpui::div()
                                .id(DOCUMENT_FIND_INPUT_ID)
                                .debug_selector(|| DOCUMENT_FIND_INPUT_ID.into())
                                .flex_1()
                                .min_w(crate::layout_metrics::scaled(FIND_FIELD_MIN_WIDTH))
                                .child(
                                    Input::new(&bar.input)
                                        .accessibility_id(DOCUMENT_FIND_INPUT_ID)
                                        .aria_label("Find in document")
                                        .prefix(
                                            Icon::new(IconName::Search)
                                                .text_color(theme.muted_foreground),
                                        ),
                                ),
                        )
                        .child(
                            gpui::div()
                                .id(DOCUMENT_FIND_COUNT_ID)
                                .debug_selector(|| DOCUMENT_FIND_COUNT_ID.into())
                                .min_w_0()
                                .w(crate::layout_metrics::scaled(FIND_COUNT_WIDTH))
                                .truncate()
                                .text_sm()
                                .text_color(count_colour)
                                .child(count),
                        )
                        // Related buttons pair a half unit apart; the
                        // pairs and the X are a unit apart.
                        .child(
                            h_flex()
                                .id(DOCUMENT_FIND_OPTIONS_ID)
                                .debug_selector(|| DOCUMENT_FIND_OPTIONS_ID.into())
                                .flex_none()
                                .gap_1()
                                .child(match_case)
                                .child(whole_word),
                        )
                        .child(
                            h_flex()
                                .id(DOCUMENT_FIND_STEPS_ID)
                                .debug_selector(|| DOCUMENT_FIND_STEPS_ID.into())
                                .flex_none()
                                .gap_1()
                                .child(previous)
                                .child(next),
                        )
                        .child(close),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortcuts_follow_the_platform() {
        assert_eq!(find_shortcut(true), "cmd-f");
        assert_eq!(find_shortcut(false), "ctrl-f");
        assert_eq!(find_next_shortcut(true), "cmd-g");
        assert_eq!(find_next_shortcut(false), "f3");
        assert_eq!(find_previous_shortcut(true), "cmd-shift-g");
        assert_eq!(find_previous_shortcut(false), "shift-f3");
        assert_eq!(find_match_case_shortcut(true), "cmd-alt-c");
        assert_eq!(find_match_case_shortcut(false), "alt-c");
        assert_eq!(find_whole_word_shortcut(true), "cmd-alt-w");
        assert_eq!(find_whole_word_shortcut(false), "alt-w");
    }

    #[test]
    fn the_count_reads_as_the_search_goes() {
        let text = |empty, finished, limit, hits, position| {
            find_count_text(empty, finished, limit, hits, position).0
        };
        assert_eq!(text(true, true, false, 0, None), "");
        assert_eq!(text(false, false, false, 0, None), "Searching…");
        assert_eq!(text(false, true, false, 0, None), "No results");
        assert_eq!(
            find_count_text(false, true, false, 0, None).1,
            FindCountTone::None
        );
        assert_eq!(text(false, false, false, 17, Some(2)), "17+");
        assert_eq!(text(false, true, false, 17, Some(2)), "3 of 17");
        assert_eq!(text(false, true, true, 10_000, Some(0)), "10,000+");
        assert_eq!(
            text(false, true, false, 1_234, Some(1_233)),
            "1,234 of 1,234"
        );
    }

    #[test]
    fn a_selection_seeds_the_query_with_its_first_line() {
        assert_eq!(seed_query("fire door\nexit").as_deref(), Some("fire door"));
        assert_eq!(seed_query("\n  exit  \nmore").as_deref(), Some("exit"));
        assert_eq!(seed_query("  \n "), None);
        let long = "x".repeat(400);
        assert_eq!(
            seed_query(&long).map(|query| query.chars().count()),
            Some(256)
        );
    }

    #[test]
    fn quads_give_their_box() {
        let quads = [
            box_quad([10., 20., 30., 40.]),
            box_quad([5., 25., 12., 60.]),
        ];
        assert_eq!(quads_box(&quads), Some([5., 20., 30., 60.]));
        assert_eq!(quads_box(&[]), None);
    }
}
