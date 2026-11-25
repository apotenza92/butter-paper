//! Running a search: one background thread per document reads its pages one
//! at a time (the document's PDF worker is serial), tests each page's text
//! and each markup against the query, and sends what it found page by page.
//! The search stops between pages when it is cancelled, so the user can
//! change the query at any time and the old search ends within one page.
//!
//! This module has no user interface. The window builds the documents
//! ([`SearchDocument`]), starts a [`SearchRun`] with a function that
//! receives the [`SearchEvent`]s, and merges them into [`SearchResults`].

use std::{
    collections::{BTreeMap, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
};

use crate::{
    pdf_text_selection::{Quad, TextPosition},
    search_markups::MarkupSearchRecord,
    search_query::{SearchQuery, UnitContext},
    search_text::{PageSearchText, Snippet, snippet_of, snippet_of_ranges},
};

/// The results a search keeps: more than a reader can use.
pub const SEARCH_HIT_LIMIT: usize = 10_000;

/// The characters of text on each side of a match in a snippet.
pub const SNIPPET_CONTEXT: usize = 40;

/// What a search reads of one document. The pages' text can take long to
/// read, so [`Self::page_text`] is called on a background thread.
pub trait SearchDocument: Send + Sync {
    /// Names the document in results and keys its cached text.
    fn id(&self) -> u64;
    /// The file's name, for `file:`.
    fn name(&self) -> &str;
    fn page_count(&self) -> u32;
    /// The page to read first: pages go on from here to the last, then from
    /// the first. The find bar starts at the page in view.
    fn start_page(&self) -> u32 {
        0
    }
    fn page_text(&self, page_index: u32) -> Result<Arc<PageSearchText>, String>;
    fn markups(&self) -> Arc<Vec<MarkupSearchRecord>>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum HitTarget {
    /// A match in page text, a range of the page's selectable lines.
    Text {
        start: TextPosition,
        end: TextPosition,
    },
    /// A markup, or a reply to it.
    Markup {
        id: String,
        /// The editable model holds it, so the hit can select it.
        in_model: bool,
        /// The reply that matched, if the match is in one.
        reply: Option<usize>,
        /// The markup's last change, as the file has it, for sorting.
        modified: Option<String>,
    },
    /// A page that matches by its place alone (`page:3`, `file:level`):
    /// nothing on it to highlight.
    Page,
    /// A place where a visual search found its picture: the hit's `bounds`
    /// are its box. `source` is the box the picture came from.
    Visual { score: f32, source: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchHit {
    pub document_id: u64,
    pub page_index: u32,
    pub target: HitTarget,
    pub snippet: Snippet,
    /// A text hit's boxes in page space.
    pub quads: Vec<Quad>,
    /// A markup's box in page space: x0, y0, x1, y1.
    pub bounds: Option<[f64; 4]>,
}

/// What a search found on one page.
#[derive(Clone, Debug, PartialEq)]
pub struct PageOutcome {
    pub document_id: u64,
    pub page_index: u32,
    /// In reading order: the text matches, then the markups of the page.
    pub hits: Vec<SearchHit>,
    /// The page has text. A scan, or a drawing with its text drawn as lines,
    /// has none, and a search cannot find anything on it.
    pub has_text: bool,
    /// Why the page's text could not be read, if it could not. Such a page
    /// counts as one without text.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SearchEvent {
    Page(PageOutcome),
    /// The document's search ended: every page, or the search stopped.
    DocumentDone {
        document_id: u64,
    },
    /// The search found [`SearchConfig::hit_limit`] hits and stopped.
    LimitReached,
    /// Every document is done, or the search was cancelled. Last event.
    Finished {
        cancelled: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub struct SearchConfig {
    pub hit_limit: usize,
    /// Read each page's text. Off, only markups are searched.
    pub include_text: bool,
    pub include_markups: bool,
    /// Matches in one paragraph close enough to share a snippet are one hit
    /// with each match marked (the panel's rows). Off, each match is a hit
    /// (the find bar steps through them).
    pub join_nearby: bool,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            hit_limit: SEARCH_HIT_LIMIT,
            include_text: true,
            include_markups: true,
            join_nearby: false,
        }
    }
}

type Sink = Arc<dyn Fn(SearchEvent) + Send + Sync>;

struct Shared {
    cancel: AtomicBool,
    limit_reached: AtomicBool,
    hits: AtomicUsize,
    running: AtomicUsize,
    sink: Sink,
    config: SearchConfig,
}

impl Shared {
    fn stopped(&self) -> bool {
        self.cancel.load(Ordering::Acquire) || self.limit_reached.load(Ordering::Acquire)
    }
}

/// A search in progress. Dropping it cancels it.
pub struct SearchRun {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

impl SearchRun {
    /// Starts reading `documents` on background threads, one each, and
    /// returns at once. Events arrive at `sink` from those threads, in order
    /// for each document. The last is [`SearchEvent::Finished`].
    pub fn start(
        documents: Vec<Arc<dyn SearchDocument>>,
        query: Arc<SearchQuery>,
        config: SearchConfig,
        sink: impl Fn(SearchEvent) + Send + Sync + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            cancel: AtomicBool::new(false),
            limit_reached: AtomicBool::new(false),
            hits: AtomicUsize::new(0),
            running: AtomicUsize::new(documents.len()),
            sink: Arc::new(sink),
            config,
        });
        if documents.is_empty() || query.is_empty() {
            (shared.sink)(SearchEvent::Finished { cancelled: false });
            return Self {
                shared,
                threads: Vec::new(),
            };
        }
        let threads = documents
            .into_iter()
            .map(|document| {
                let (shared, query) = (shared.clone(), query.clone());
                std::thread::Builder::new()
                    .name("butter-paper-search".into())
                    .spawn(move || {
                        search_document(document.as_ref(), &query, &shared);
                        (shared.sink)(SearchEvent::DocumentDone {
                            document_id: document.id(),
                        });
                        if shared.running.fetch_sub(1, Ordering::AcqRel) == 1 {
                            (shared.sink)(SearchEvent::Finished {
                                cancelled: shared.cancel.load(Ordering::Acquire),
                            });
                        }
                    })
                    .expect("a search thread starts")
            })
            .collect();
        Self { shared, threads }
    }

    /// Stops the search after the pages being read now. Events already sent
    /// stay valid; later ones are few and a user of the run ignores them.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.shared.cancel.load(Ordering::Acquire)
    }

    /// Waits until every thread of the search has ended.
    pub fn join(mut self) {
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for SearchRun {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The pages from `start` to the last, then from the first to `start`.
fn page_order(page_count: u32, start: u32) -> impl Iterator<Item = u32> {
    let start = start.min(page_count.saturating_sub(1));
    (start..page_count).chain(0..start)
}

fn search_document(document: &dyn SearchDocument, query: &SearchQuery, shared: &Shared) {
    if !query.file_may_match(document.name()) {
        return;
    }
    let config = shared.config;
    let markups = if config.include_markups {
        document.markups()
    } else {
        Arc::default()
    };
    let mut markups_by_page: HashMap<u32, Vec<&MarkupSearchRecord>> = HashMap::new();
    for record in markups.iter() {
        markups_by_page
            .entry(record.page_index)
            .or_default()
            .push(record);
    }
    for page_index in page_order(document.page_count(), document.start_page()) {
        if shared.stopped() {
            return;
        }
        let context = UnitContext {
            file_name: document.name(),
            page_number: page_index + 1,
        };
        let mut outcome = PageOutcome {
            document_id: document.id(),
            page_index,
            hits: Vec::new(),
            has_text: false,
            error: None,
        };
        if config.include_text {
            match document.page_text(page_index) {
                Ok(text) => {
                    outcome.has_text = !text.is_empty();
                    if outcome.has_text {
                        outcome.hits.extend(text_hits(
                            document.id(),
                            page_index,
                            &text,
                            query,
                            context,
                            config.join_nearby,
                        ));
                    }
                }
                Err(error) => outcome.error = Some(error),
            }
        }
        for record in markups_by_page.get(&page_index).into_iter().flatten() {
            if let Some(hit) = markup_hit(document.id(), record, query, context) {
                outcome.hits.push(hit);
            }
        }
        // The cap holds across documents that run at once.
        let before = shared.hits.fetch_add(outcome.hits.len(), Ordering::AcqRel);
        let room = config.hit_limit.saturating_sub(before);
        if outcome.hits.len() >= room {
            outcome.hits.truncate(room);
            if !shared.limit_reached.swap(true, Ordering::AcqRel) {
                (shared.sink)(SearchEvent::Page(outcome));
                (shared.sink)(SearchEvent::LimitReached);
                return;
            }
        }
        (shared.sink)(SearchEvent::Page(outcome));
    }
}

fn text_hits(
    document_id: u64,
    page_index: u32,
    text: &PageSearchText,
    query: &SearchQuery,
    context: UnitContext<'_>,
    join_nearby: bool,
) -> Vec<SearchHit> {
    let Some(ranges) = query.match_page(context, &text.text) else {
        return Vec::new();
    };
    if ranges.is_empty() {
        // The page matches by its place alone, or by words it does not
        // hold ("-draft").
        return vec![SearchHit {
            document_id,
            page_index,
            target: HitTarget::Page,
            snippet: Snippet::default(),
            quads: Vec::new(),
            bounds: None,
        }];
    }
    let matches = text.matches(ranges);
    if !join_nearby {
        return matches
            .into_iter()
            .map(|found| SearchHit {
                document_id,
                page_index,
                quads: text.quads(&found),
                snippet: text.snippet(found.range.clone(), SNIPPET_CONTEXT),
                target: HitTarget::Text {
                    start: found.start,
                    end: found.end,
                },
                bounds: None,
            })
            .collect();
    }
    // Matches of one paragraph whose gap fits in a snippet share one hit,
    // while the hit's text stays the length of a snippet's context.
    let mut groups: Vec<Vec<crate::search_text::TextMatch>> = Vec::new();
    for found in matches {
        let joins = groups.last().is_some_and(|group| {
            let (first, last) = (&group[0], &group[group.len() - 1]);
            let gap = &text.text[last.range.end..found.range.start.max(last.range.end)];
            !gap.contains('\n')
                && gap.chars().count() <= SNIPPET_CONTEXT
                && text.text[first.range.start..found.range.end.max(first.range.start)]
                    .chars()
                    .count()
                    <= 2 * SNIPPET_CONTEXT
        });
        match groups.last_mut() {
            Some(group) if joins => group.push(found),
            _ => groups.push(vec![found]),
        }
    }
    groups
        .into_iter()
        .map(|group| {
            let ranges = group
                .iter()
                .map(|found| found.range.clone())
                .collect::<Vec<_>>();
            SearchHit {
                document_id,
                page_index,
                quads: group.iter().flat_map(|found| text.quads(found)).collect(),
                snippet: snippet_of_ranges(&text.text, &ranges, SNIPPET_CONTEXT),
                target: HitTarget::Text {
                    start: group[0].start,
                    end: group[group.len() - 1].end,
                },
                bounds: None,
            }
        })
        .collect()
}

fn markup_hit(
    document_id: u64,
    record: &MarkupSearchRecord,
    query: &SearchQuery,
    context: UnitContext<'_>,
) -> Option<SearchHit> {
    let range = query.match_markup(context, &record.body, &record.authors, &record.subject)?;
    let (snippet, reply) = match range {
        Some(range) => (
            snippet_of(&record.body, range.clone(), SNIPPET_CONTEXT),
            record.reply_at(range.start),
        ),
        // It matches by its author, subject or place: show its text.
        None => (snippet_of(&record.body, 0..0, SNIPPET_CONTEXT), None),
    };
    Some(SearchHit {
        document_id,
        page_index: record.page_index,
        target: HitTarget::Markup {
            id: record.id.clone(),
            in_model: record.in_model,
            reply,
            modified: record.modified.clone(),
        },
        snippet,
        quads: Vec::new(),
        bounds: record.bounds,
    })
}

// Results

/// How a result list orders its documents.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchSort {
    /// As the documents were given: the active window's tabs first.
    #[default]
    DocumentOrder,
    NameAscending,
    NameDescending,
    MostResults,
    /// By the latest change to a markup that matched.
    MarkupsNewest,
    MarkupsOldest,
}

/// What one document's search has found so far.
#[derive(Clone, Debug)]
pub struct DocumentResults {
    pub document_id: u64,
    pub name: String,
    pub page_count: u32,
    /// Hits by page, in page order.
    pages: BTreeMap<u32, Vec<SearchHit>>,
    pub pages_searched: u32,
    pub pages_without_text: u32,
    /// The pages whose text could not be read, and why (the first).
    pub pages_unread: u32,
    pub first_error: Option<String>,
    pub done: bool,
}

impl DocumentResults {
    pub fn hit_count(&self) -> usize {
        self.pages.values().map(Vec::len).sum()
    }

    pub fn pages_with_hits(&self) -> usize {
        self.pages.len()
    }

    /// Pages that have hits, with them, in page order.
    pub fn pages(&self) -> impl Iterator<Item = (u32, &[SearchHit])> {
        self.pages
            .iter()
            .map(|(page, hits)| (*page, hits.as_slice()))
    }

    pub fn hits(&self) -> impl Iterator<Item = &SearchHit> {
        self.pages.values().flatten()
    }

    fn latest_markup_change(&self) -> Option<&str> {
        self.hits()
            .filter_map(|hit| match &hit.target {
                HitTarget::Markup { modified, .. } => modified.as_deref(),
                _ => None,
            })
            .max()
    }

    fn earliest_markup_change(&self) -> Option<&str> {
        self.hits()
            .filter_map(|hit| match &hit.target {
                HitTarget::Markup { modified, .. } => modified.as_deref(),
                _ => None,
            })
            .min()
    }
}

/// The events of a search, merged into what a list shows.
#[derive(Clone, Debug, Default)]
pub struct SearchResults {
    documents: Vec<DocumentResults>,
    limit_reached: bool,
    finished: bool,
    cancelled: bool,
}

impl SearchResults {
    /// An empty set of results for `documents`, given as `(id, name, page
    /// count)` in the order a list shows them by default.
    pub fn new(documents: impl IntoIterator<Item = (u64, String, u32)>) -> Self {
        Self {
            documents: documents
                .into_iter()
                .map(|(document_id, name, page_count)| DocumentResults {
                    document_id,
                    name,
                    page_count,
                    pages: BTreeMap::new(),
                    pages_searched: 0,
                    pages_without_text: 0,
                    pages_unread: 0,
                    first_error: None,
                    done: false,
                })
                .collect(),
            ..Self::default()
        }
    }

    pub fn apply(&mut self, event: SearchEvent) {
        match event {
            SearchEvent::Page(page) => {
                let Some(document) = self
                    .documents
                    .iter_mut()
                    .find(|document| document.document_id == page.document_id)
                else {
                    return;
                };
                document.pages_searched += 1;
                if !page.has_text {
                    document.pages_without_text += 1;
                }
                if let Some(error) = page.error {
                    document.pages_unread += 1;
                    document.first_error.get_or_insert(error);
                }
                if !page.hits.is_empty() {
                    document.pages.insert(page.page_index, page.hits);
                }
            }
            SearchEvent::DocumentDone { document_id } => {
                if let Some(document) = self
                    .documents
                    .iter_mut()
                    .find(|document| document.document_id == document_id)
                {
                    document.done = true;
                }
            }
            SearchEvent::LimitReached => self.limit_reached = true,
            SearchEvent::Finished { cancelled } => {
                self.finished = true;
                self.cancelled = cancelled;
            }
        }
    }

    pub fn documents(&self) -> &[DocumentResults] {
        &self.documents
    }

    pub fn hit_count(&self) -> usize {
        self.documents.iter().map(DocumentResults::hit_count).sum()
    }

    /// The search stopped at [`SEARCH_HIT_LIMIT`] hits (or the configured
    /// one): there are more.
    pub fn limit_reached(&self) -> bool {
        self.limit_reached
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn was_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Pages read and pages to read, over all documents.
    pub fn progress(&self) -> (u32, u32) {
        self.documents.iter().fold((0, 0), |(read, all), document| {
            (read + document.pages_searched, all + document.page_count)
        })
    }

    pub fn pages_without_text(&self) -> u32 {
        self.documents
            .iter()
            .map(|document| document.pages_without_text)
            .sum()
    }

    /// The documents in `sort` order. Documents with no hit are included:
    /// a list shows only those with hits.
    pub fn ordered(&self, sort: SearchSort) -> Vec<&DocumentResults> {
        let mut documents = self.documents.iter().collect::<Vec<_>>();
        // `sort_by` keeps the order of equal ones: the given order.
        match sort {
            SearchSort::DocumentOrder => {}
            SearchSort::NameAscending => documents.sort_by_cached_key(|d| d.name.to_lowercase()),
            SearchSort::NameDescending => {
                documents.sort_by_cached_key(|d| std::cmp::Reverse(d.name.to_lowercase()));
            }
            SearchSort::MostResults => {
                documents.sort_by_key(|document| std::cmp::Reverse(document.hit_count()));
            }
            SearchSort::MarkupsNewest => documents.sort_by(|a, b| {
                // Documents with no markup hit go last.
                b.latest_markup_change().cmp(&a.latest_markup_change())
            }),
            SearchSort::MarkupsOldest => documents.sort_by(|a, b| {
                match (a.earliest_markup_change(), b.earliest_markup_change()) {
                    (Some(a), Some(b)) => a.cmp(b),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
            }),
        }
        documents
    }

    /// Every hit in the order a list shows them.
    pub fn hits(&self, sort: SearchSort) -> Vec<&SearchHit> {
        self.ordered(sort)
            .into_iter()
            .flat_map(DocumentResults::hits)
            .collect()
    }

    /// The place in [`Self::hits`] (in document order) of the first hit on
    /// `page_index` of the document or later, for the find bar's first match
    /// at or after the view. `None` when no hit is there or after.
    pub fn first_hit_from(&self, document_id: u64, page_index: u32) -> Option<usize> {
        let mut index = 0;
        for document in &self.documents {
            if document.document_id == document_id {
                let before = document
                    .pages
                    .range(..page_index)
                    .map(|(_, hits)| hits.len())
                    .sum::<usize>();
                return (before < document.hit_count()).then_some(index + before);
            }
            index += document.hit_count();
        }
        None
    }
}

/// The place after `current` in a list of `count` hits, and before it, going
/// round at the ends.
pub fn step_hit(current: usize, count: usize, forward: bool) -> Option<usize> {
    (count > 0).then(|| {
        if forward {
            (current + 1) % count
        } else {
            (current + count - 1) % count
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        search_query::SearchOptions,
        search_text::tests::{line, page},
    };
    use std::{
        sync::{Mutex, mpsc},
        time::{Duration, Instant},
    };

    /// A document of `pages` pages whose page text comes from `text`.
    struct FakeDocument {
        id: u64,
        name: String,
        pages: u32,
        start: u32,
        delay: Duration,
        text: Box<dyn Fn(u32) -> Result<Vec<&'static str>, String> + Send + Sync>,
        markups: Vec<MarkupSearchRecord>,
        read: Mutex<Vec<u32>>,
    }

    impl FakeDocument {
        fn new(id: u64, name: &str, pages: u32) -> Self {
            Self {
                id,
                name: name.to_owned(),
                pages,
                start: 0,
                delay: Duration::ZERO,
                text: Box::new(|_| Ok(vec!["nothing here"])),
                markups: Vec::new(),
                read: Mutex::default(),
            }
        }

        fn with_text(
            mut self,
            text: impl Fn(u32) -> Result<Vec<&'static str>, String> + Send + Sync + 'static,
        ) -> Self {
            self.text = Box::new(text);
            self
        }
    }

    impl SearchDocument for FakeDocument {
        fn id(&self) -> u64 {
            self.id
        }
        fn name(&self) -> &str {
            &self.name
        }
        fn page_count(&self) -> u32 {
            self.pages
        }
        fn start_page(&self) -> u32 {
            self.start
        }
        fn page_text(&self, page_index: u32) -> Result<Arc<PageSearchText>, String> {
            std::thread::sleep(self.delay);
            self.read.lock().unwrap().push(page_index);
            let lines = (self.text)(page_index)?;
            let mut page = page(
                lines
                    .into_iter()
                    .enumerate()
                    .map(|(index, text)| line(text, index, 0., 700. - 20. * index as f64))
                    .collect(),
            );
            page.page_index = page_index;
            Ok(Arc::new(page))
        }
        fn markups(&self) -> Arc<Vec<MarkupSearchRecord>> {
            Arc::new(self.markups.clone())
        }
    }

    fn markup(
        id: &str,
        page_index: u32,
        author: &str,
        text: &str,
        modified: Option<&str>,
    ) -> MarkupSearchRecord {
        MarkupSearchRecord::new(
            id.to_owned(),
            page_index,
            Some([0., 0., 10., 10.]),
            true,
            "Text Box".into(),
            author.to_owned(),
            text.to_owned(),
            Vec::new(),
            modified.map(str::to_owned),
        )
    }

    fn run_to_end(
        documents: Vec<Arc<dyn SearchDocument>>,
        query: &str,
        config: SearchConfig,
    ) -> (Vec<SearchEvent>, SearchResults) {
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let results_for = documents
            .iter()
            .map(|d| (d.id(), d.name().to_owned(), d.page_count()))
            .collect::<Vec<_>>();
        let run = SearchRun::start(
            documents,
            Arc::new(SearchQuery::parse(query, SearchOptions::default())),
            config,
            move |event| {
                let _ = sender.lock().unwrap().send(event);
            },
        );
        run.join();
        let events = receiver.try_iter().collect::<Vec<_>>();
        let mut results = SearchResults::new(results_for);
        for event in events.iter().cloned() {
            results.apply(event);
        }
        (events, results)
    }

    #[test]
    fn a_search_reports_each_page_in_document_order_then_finishes() {
        let document = FakeDocument::new(1, "plan.pdf", 4).with_text(|page| {
            Ok(vec![if page % 2 == 1 {
                "fire door"
            } else {
                "nothing"
            }])
        });
        let (events, results) = run_to_end(vec![Arc::new(document)], "door", Default::default());
        let pages = events
            .iter()
            .filter_map(|event| match event {
                SearchEvent::Page(page) => Some(page.page_index),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(pages, [0, 1, 2, 3]);
        assert_eq!(
            events.last(),
            Some(&SearchEvent::Finished { cancelled: false })
        );
        assert_eq!(results.hit_count(), 2);
        assert!(results.is_finished());
        let doc = &results.documents()[0];
        assert!(doc.done);
        assert_eq!(
            doc.pages().map(|(page, _)| page).collect::<Vec<_>>(),
            [1, 3]
        );
    }

    #[test]
    fn a_search_starts_at_the_page_it_is_given_and_wraps() {
        let mut document = FakeDocument::new(1, "plan.pdf", 5);
        document.start = 3;
        let document = Arc::new(document);
        let (events, _) = run_to_end(vec![document.clone()], "x", Default::default());
        assert_eq!(*document.read.lock().unwrap(), [3, 4, 0, 1, 2]);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, SearchEvent::Page(_)))
                .count(),
            5
        );
    }

    #[test]
    fn page_hits_have_snippets_and_boxes() {
        let document = FakeDocument::new(1, "plan.pdf", 1)
            .with_text(|_| Ok(vec!["to stair 2: Fire door 60 min"]));
        let (_, results) = run_to_end(
            vec![Arc::new(document)],
            "\"fire door\"",
            Default::default(),
        );
        let hits = results.hits(SearchSort::DocumentOrder);
        assert_eq!(hits.len(), 1);
        let hit = hits[0];
        assert_eq!(hit.snippet.matched, "Fire door");
        assert_eq!(hit.snippet.before, "to stair 2: ");
        assert_eq!(hit.snippet.after, " 60 min");
        assert_eq!(hit.quads.len(), 1);
        assert!(matches!(
            hit.target,
            HitTarget::Text {
                start: TextPosition { line: 0, index: 12 },
                end: TextPosition { line: 0, index: 21 },
            }
        ));
    }

    #[test]
    fn the_hits_of_a_page_are_text_first_then_markups_and_all_in_page_order() {
        let mut document = FakeDocument::new(1, "plan.pdf", 3)
            .with_text(|page| Ok(vec![if page == 0 { "door at 0" } else { "door at 1+" }]));
        document.markups = vec![
            markup("m-late", 1, "Alex", "check the door", None),
            markup("m-first", 0, "Alex", "door too", None),
        ];
        let (_, results) = run_to_end(vec![Arc::new(document)], "door", Default::default());
        let hits = results.hits(SearchSort::DocumentOrder);
        let describe = |hit: &&SearchHit| match &hit.target {
            HitTarget::Text { .. } => format!("text p{}", hit.page_index),
            HitTarget::Markup { id, .. } => format!("{id} p{}", hit.page_index),
            HitTarget::Page | HitTarget::Visual { .. } => "page".to_owned(),
        };
        assert_eq!(
            hits.iter().map(describe).collect::<Vec<_>>(),
            ["text p0", "m-first p0", "text p1", "m-late p1", "text p2"]
        );
    }

    #[test]
    fn a_markup_hit_tells_which_reply_matched() {
        let mut record = markup("tb", 0, "Alex", "Check the rating", None);
        record = MarkupSearchRecord::new(
            record.id,
            0,
            record.bounds,
            true,
            "Text Box".into(),
            "Alex".into(),
            "Check the rating".into(),
            vec![
                crate::search_markups::MarkupReply {
                    author: "Sam".into(),
                    text: "Agreed".into(),
                },
                crate::search_markups::MarkupReply {
                    author: "Kim".into(),
                    text: "Needs 60 min".into(),
                },
            ],
            None,
        );
        let mut document = FakeDocument::new(1, "plan.pdf", 1);
        document.markups = vec![record];
        let (_, results) = run_to_end(vec![Arc::new(document)], "60", Default::default());
        let hits = results.hits(SearchSort::DocumentOrder);
        assert_eq!(hits.len(), 1);
        assert!(matches!(
            hits[0].target,
            HitTarget::Markup { reply: Some(1), .. }
        ));
        let (_, results) = {
            let mut document = FakeDocument::new(1, "plan.pdf", 1);
            document.markups = vec![markup("tb", 0, "Alex", "Check the rating", None)];
            run_to_end(vec![Arc::new(document)], "rating", Default::default())
        };
        assert!(matches!(
            results.hits(SearchSort::DocumentOrder)[0].target,
            HitTarget::Markup { reply: None, .. }
        ));
    }

    #[test]
    fn a_markup_matched_by_author_alone_shows_its_text() {
        let mut document = FakeDocument::new(1, "plan.pdf", 1);
        document.markups = vec![
            markup("a", 0, "Alex", "Check the door", None),
            markup("b", 0, "Sam", "Other", None),
        ];
        let (_, results) = run_to_end(vec![Arc::new(document)], "author:alex", Default::default());
        let hits = results.hits(SearchSort::DocumentOrder);
        // The page text has no author, so only the markup matches.
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].snippet.after, "Check the door");
    }

    #[test]
    fn a_query_of_a_place_alone_gives_a_hit_for_each_matching_page() {
        let document = FakeDocument::new(1, "plan.pdf", 6);
        let (_, results) = run_to_end(vec![Arc::new(document)], "page:2-3", Default::default());
        let hits = results.hits(SearchSort::DocumentOrder);
        assert_eq!(
            hits.iter().map(|h| h.page_index).collect::<Vec<_>>(),
            [1, 2]
        );
        assert!(hits.iter().all(|hit| hit.target == HitTarget::Page));
    }

    #[test]
    fn pages_without_text_and_pages_that_fail_are_counted_and_the_search_goes_on() {
        let document = FakeDocument::new(1, "scan.pdf", 4).with_text(|page| match page {
            0 => Ok(vec![]),
            1 => Err("the page cannot be read".into()),
            2 => Ok(vec!["   "]),
            _ => Ok(vec!["door"]),
        });
        let (_, results) = run_to_end(vec![Arc::new(document)], "door", Default::default());
        let doc = &results.documents()[0];
        assert_eq!(doc.pages_searched, 4);
        assert_eq!(doc.pages_without_text, 3);
        assert_eq!(doc.pages_unread, 1);
        assert_eq!(doc.first_error.as_deref(), Some("the page cannot be read"));
        assert_eq!(results.hit_count(), 1);
        assert_eq!(results.pages_without_text(), 3);
    }

    #[test]
    fn a_file_filter_skips_a_document_without_reading_it() {
        let skipped = Arc::new(FakeDocument::new(1, "Sections.pdf", 3));
        let read = Arc::new(FakeDocument::new(2, "Level 2.pdf", 3));
        let (_, results) = run_to_end(
            vec![skipped.clone(), read.clone()],
            "nothing file:level",
            Default::default(),
        );
        assert!(skipped.read.lock().unwrap().is_empty());
        assert_eq!(read.read.lock().unwrap().len(), 3);
        assert!(results.documents().iter().all(|d| d.done));
        assert_eq!(results.hit_count(), 3);
    }

    #[test]
    fn documents_run_at_the_same_time_and_each_keeps_its_page_order() {
        let documents = (0..4)
            .map(|id| {
                let mut document =
                    FakeDocument::new(id, &format!("d{id}.pdf"), 6).with_text(|_| Ok(vec!["door"]));
                document.delay = Duration::from_millis(15);
                Arc::new(document) as Arc<dyn SearchDocument>
            })
            .collect::<Vec<_>>();
        let started = Instant::now();
        let (events, results) = run_to_end(documents, "door", Default::default());
        // Four documents of six 15 ms pages: well under the 360 ms that
        // reading them one after the other takes.
        assert!(started.elapsed() < Duration::from_millis(300));
        assert_eq!(results.hit_count(), 24);
        for id in 0..4 {
            let pages = events
                .iter()
                .filter_map(|event| match event {
                    SearchEvent::Page(page) if page.document_id == id => Some(page.page_index),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(pages, [0, 1, 2, 3, 4, 5]);
        }
        let finished = events
            .iter()
            .filter(|e| matches!(e, SearchEvent::Finished { .. }))
            .count();
        assert_eq!(finished, 1);
        assert_eq!(
            events.last(),
            Some(&SearchEvent::Finished { cancelled: false })
        );
    }

    #[test]
    fn results_arrive_page_by_page_while_the_search_runs() {
        let mut document = FakeDocument::new(1, "big.pdf", 200).with_text(|_| Ok(vec!["a door"]));
        document.delay = Duration::from_millis(2);
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let run = SearchRun::start(
            vec![Arc::new(document)],
            Arc::new(SearchQuery::parse("door", SearchOptions::default())),
            SearchConfig::default(),
            move |event| {
                let _ = sender.lock().unwrap().send(event);
            },
        );
        // The first page comes long before the last.
        let first = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            first,
            SearchEvent::Page(PageOutcome { page_index: 0, .. })
        ));
        assert!(receiver.try_iter().count() < 199);
        run.cancel();
        run.join();
    }

    #[test]
    fn cancelling_stops_the_search_within_a_page_and_says_so() {
        let mut document = FakeDocument::new(1, "big.pdf", 500).with_text(|_| Ok(vec!["door"]));
        document.delay = Duration::from_millis(5);
        let document = Arc::new(document);
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let run = SearchRun::start(
            vec![document.clone()],
            Arc::new(SearchQuery::parse("door", SearchOptions::default())),
            SearchConfig::default(),
            move |event| {
                let _ = sender.lock().unwrap().send(event);
            },
        );
        // Wait for a few pages, then cancel.
        for _ in 0..3 {
            receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        run.cancel();
        assert!(run.is_cancelled());
        run.join();
        let read = document.read.lock().unwrap().len();
        assert!(read < 100, "{read} of 500 pages read after the cancel");
        let events = receiver.try_iter().collect::<Vec<_>>();
        assert_eq!(
            events.last(),
            Some(&SearchEvent::Finished { cancelled: true })
        );
    }

    #[test]
    fn dropping_a_run_cancels_it() {
        let mut document = FakeDocument::new(1, "big.pdf", 500).with_text(|_| Ok(vec!["door"]));
        document.delay = Duration::from_millis(5);
        let document = Arc::new(document);
        let (sender, receiver) = mpsc::channel();
        let sender = Mutex::new(sender);
        let run = SearchRun::start(
            vec![document.clone()],
            Arc::new(SearchQuery::parse("door", SearchOptions::default())),
            SearchConfig::default(),
            move |event| {
                let _ = sender.lock().unwrap().send(event);
            },
        );
        receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(run);
        // The thread ends after its page; no Finished{false} arrives.
        loop {
            match receiver.recv_timeout(Duration::from_secs(5)) {
                Ok(SearchEvent::Finished { cancelled }) => {
                    assert!(cancelled);
                    break;
                }
                Ok(_) => {}
                Err(error) => panic!("the search did not end: {error}"),
            }
        }
        assert!(document.read.lock().unwrap().len() < 100);
    }

    #[test]
    fn nearby_matches_share_one_hit_with_each_term_marked() {
        let document: Arc<dyn SearchDocument> =
            Arc::new(FakeDocument::new(1, "plan.pdf", 1).with_text(|_| {
                Ok(vec![
                    "The fire exit door is open and the fire alarm sounds",
                    "A door on the next paragraph",
                ])
            }));
        let joined = SearchConfig {
            join_nearby: true,
            ..SearchConfig::default()
        };
        let (_, results) = run_to_end(vec![document.clone()], "fire door", joined);
        let hits = results.hits(SearchSort::DocumentOrder);
        // One row for the first paragraph, one for the second.
        assert_eq!(hits.len(), 2, "{hits:#?}");
        let snippet = &hits[0].snippet;
        let marked = snippet
            .marks
            .iter()
            .map(|range| &snippet.matched[range.clone()])
            .collect::<Vec<_>>();
        assert_eq!(marked, ["fire", "door", "fire"]);
        assert_eq!(snippet.matched, "fire exit door is open and the fire");
        assert_eq!(hits[0].quads.len(), 3, "one box for each match");
        assert_eq!(hits[1].snippet.matched, "door");
        // The find bar's setting keeps one hit for each match.
        let (_, results) = run_to_end(vec![document], "fire door", SearchConfig::default());
        assert_eq!(results.hit_count(), 4);
    }

    #[test]
    fn the_hit_limit_stops_the_search_and_is_reported() {
        let document = Arc::new(
            FakeDocument::new(1, "big.pdf", 100).with_text(|_| Ok(vec!["door door door door"])),
        );
        let config = SearchConfig {
            hit_limit: 10,
            ..SearchConfig::default()
        };
        let (events, results) = run_to_end(vec![document.clone()], "door", config);
        assert_eq!(results.hit_count(), 10);
        assert!(results.limit_reached());
        assert!(events.contains(&SearchEvent::LimitReached));
        // 4 hits a page: the third page was cut short and none was read after.
        assert_eq!(document.read.lock().unwrap().len(), 3);
        assert_eq!(
            events.last(),
            Some(&SearchEvent::Finished { cancelled: false })
        );
    }

    #[test]
    fn the_hit_limit_is_shared_by_documents_running_together() {
        let documents = (0..4)
            .map(|id| {
                Arc::new(FakeDocument::new(id, "d.pdf", 50).with_text(|_| Ok(vec!["door door"])))
                    as Arc<dyn SearchDocument>
            })
            .collect::<Vec<_>>();
        let config = SearchConfig {
            hit_limit: 25,
            ..SearchConfig::default()
        };
        let (_, results) = run_to_end(documents, "door", config);
        assert_eq!(results.hit_count(), 25);
        assert!(results.limit_reached());
    }

    #[test]
    fn an_empty_query_or_no_documents_finishes_at_once() {
        let (events, _) = run_to_end(
            vec![Arc::new(FakeDocument::new(1, "a.pdf", 3))],
            "   ",
            Default::default(),
        );
        assert_eq!(events, [SearchEvent::Finished { cancelled: false }]);
        let (events, _) = run_to_end(Vec::new(), "door", Default::default());
        assert_eq!(events, [SearchEvent::Finished { cancelled: false }]);
    }

    #[test]
    fn markups_can_be_left_out_or_searched_alone() {
        let mut document =
            FakeDocument::new(1, "plan.pdf", 1).with_text(|_| Ok(vec!["door in text"]));
        document.markups = vec![markup("m", 0, "Alex", "door in markup", None)];
        let document: Arc<dyn SearchDocument> = Arc::new(document);
        let only_text = SearchConfig {
            include_markups: false,
            ..SearchConfig::default()
        };
        let (_, results) = run_to_end(vec![document.clone()], "door", only_text);
        assert_eq!(results.hit_count(), 1);
        let only_markups = SearchConfig {
            include_text: false,
            ..SearchConfig::default()
        };
        let (_, results) = run_to_end(vec![document], "door", only_markups);
        assert_eq!(results.hit_count(), 1);
        assert!(matches!(
            results.hits(SearchSort::DocumentOrder)[0].target,
            HitTarget::Markup { .. }
        ));
    }

    fn sorted_names(results: &SearchResults, sort: SearchSort) -> Vec<String> {
        results
            .ordered(sort)
            .into_iter()
            .map(|document| document.name.clone())
            .collect()
    }

    fn two_hit_page(document_id: u64, hits: usize) -> SearchEvent {
        let hit = SearchHit {
            document_id,
            page_index: 0,
            target: HitTarget::Page,
            snippet: Snippet::default(),
            quads: Vec::new(),
            bounds: None,
        };
        SearchEvent::Page(PageOutcome {
            document_id,
            page_index: 0,
            hits: vec![hit; hits],
            has_text: true,
            error: None,
        })
    }

    #[test]
    fn documents_sort_by_name_and_by_count_and_keep_the_given_order_for_ties() {
        let mut results = SearchResults::new([
            (1, "b plan.pdf".to_owned(), 1),
            (2, "A sections.pdf".to_owned(), 1),
            (3, "c notes.pdf".to_owned(), 1),
            (4, "d tie.pdf".to_owned(), 1),
        ]);
        for (id, hits) in [(1, 2), (2, 1), (3, 5), (4, 2)] {
            results.apply(two_hit_page(id, hits));
        }
        assert_eq!(
            sorted_names(&results, SearchSort::DocumentOrder),
            ["b plan.pdf", "A sections.pdf", "c notes.pdf", "d tie.pdf"]
        );
        assert_eq!(
            sorted_names(&results, SearchSort::NameAscending),
            ["A sections.pdf", "b plan.pdf", "c notes.pdf", "d tie.pdf"]
        );
        assert_eq!(
            sorted_names(&results, SearchSort::NameDescending),
            ["d tie.pdf", "c notes.pdf", "b plan.pdf", "A sections.pdf"]
        );
        assert_eq!(
            sorted_names(&results, SearchSort::MostResults),
            ["c notes.pdf", "b plan.pdf", "d tie.pdf", "A sections.pdf"]
        );
    }

    #[test]
    fn documents_sort_by_the_date_of_their_markup_hits() {
        let mut results = SearchResults::new([
            (1, "old.pdf".to_owned(), 1),
            (2, "none.pdf".to_owned(), 1),
            (3, "new.pdf".to_owned(), 1),
        ]);
        let markup_page = |document_id, modified: &str| {
            SearchEvent::Page(PageOutcome {
                document_id,
                page_index: 0,
                hits: vec![SearchHit {
                    document_id,
                    page_index: 0,
                    target: HitTarget::Markup {
                        id: "m".into(),
                        in_model: true,
                        reply: None,
                        modified: Some(modified.to_owned()),
                    },
                    snippet: Snippet::default(),
                    quads: Vec::new(),
                    bounds: None,
                }],
                has_text: true,
                error: None,
            })
        };
        results.apply(markup_page(1, "D:20250101000000"));
        results.apply(two_hit_page(2, 1));
        results.apply(markup_page(3, "D:20261001000000"));
        assert_eq!(
            sorted_names(&results, SearchSort::MarkupsNewest),
            ["new.pdf", "old.pdf", "none.pdf"]
        );
        assert_eq!(
            sorted_names(&results, SearchSort::MarkupsOldest),
            ["old.pdf", "new.pdf", "none.pdf"]
        );
    }

    #[test]
    fn hits_group_by_document_then_page_whatever_order_the_pages_arrive_in() {
        let mut results = SearchResults::new([(1, "a.pdf".to_owned(), 5)]);
        let hit_on = |page_index| {
            SearchEvent::Page(PageOutcome {
                document_id: 1,
                page_index,
                hits: vec![SearchHit {
                    document_id: 1,
                    page_index,
                    target: HitTarget::Page,
                    snippet: Snippet::default(),
                    quads: Vec::new(),
                    bounds: None,
                }],
                has_text: true,
                error: None,
            })
        };
        // A search that began at page 3.
        for page in [3, 4, 0, 1] {
            results.apply(hit_on(page));
        }
        let pages = results.documents()[0]
            .pages()
            .map(|(page, _)| page)
            .collect::<Vec<_>>();
        assert_eq!(pages, [0, 1, 3, 4]);
        // The list is by page; "first at or after the view" finds page 3.
        assert_eq!(results.first_hit_from(1, 2), Some(2));
        assert_eq!(results.first_hit_from(1, 0), Some(0));
        assert_eq!(results.first_hit_from(1, 4), Some(3));
        assert_eq!(results.first_hit_from(1, 5), None);
        assert_eq!(results.first_hit_from(9, 0), None);
    }

    #[test]
    fn first_hit_from_counts_the_hits_of_earlier_documents() {
        let mut results =
            SearchResults::new([(1, "a.pdf".to_owned(), 2), (2, "b.pdf".to_owned(), 2)]);
        results.apply(two_hit_page(1, 3));
        results.apply(two_hit_page(2, 2));
        assert_eq!(results.first_hit_from(2, 0), Some(3));
    }

    #[test]
    fn stepping_through_hits_goes_round_at_the_ends() {
        assert_eq!(step_hit(0, 3, true), Some(1));
        assert_eq!(step_hit(2, 3, true), Some(0));
        assert_eq!(step_hit(0, 3, false), Some(2));
        assert_eq!(step_hit(1, 3, false), Some(0));
        assert_eq!(step_hit(0, 1, true), Some(0));
        assert_eq!(step_hit(0, 0, true), None);
    }

    #[test]
    fn progress_counts_pages_read_over_all_documents() {
        let mut results =
            SearchResults::new([(1, "a.pdf".to_owned(), 10), (2, "b.pdf".to_owned(), 5)]);
        results.apply(two_hit_page(1, 0));
        results.apply(two_hit_page(2, 0));
        results.apply(two_hit_page(2, 0));
        assert_eq!(results.progress(), (3, 15));
    }
}
