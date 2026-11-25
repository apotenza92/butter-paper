//! Running a visual search: each document's pages are drawn one at a time
//! by its PDF worker (which is serial, and draws them after everything the
//! viewer asks for), and a pool of matcher threads searches the drawn pages
//! in parallel with a [`PreparedTemplate`]. The run sends what it found page
//! by page, with a small picture of each hit for the result list.
//!
//! A drawn page is kept in a [`VisualRenderCache`] (grey, one byte for each
//! pixel), so the next search of the same pages at the same scale needs no
//! worker. The search ends within one page of a cancel: Stop, a new search
//! or a document that closes.
//!
//! This module has no user interface. The workspace builds the documents
//! ([`VisualSearchDocument`]), starts a [`VisualSearchRun`] and merges the
//! [`VisualSearchEvent`]s into [`VisualResults`].

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use crate::{
    page_geometry::PageCoordinateSpace,
    visual_search::{GreyImage, PreparedTemplate, VisualHit, VisualPage, VisualSearchError},
};

/// The hits a search keeps, over all documents.
pub const VISUAL_HIT_LIMIT: usize = 10_000;

/// The longest side of a hit's picture in the result list, in pixels.
pub const TILE_EDGE_PX: usize = 96;

/// How much memory the render cache can hold.
pub const VISUAL_RENDER_CACHE_BYTES: usize = 128 * 1024 * 1024;

/// A page drawn for a search, and how to place hits on it.
#[derive(Clone, Debug)]
pub struct VisualPageRender {
    /// Page content only, in its real colours, in display orientation.
    pub image: Arc<GreyImage>,
    /// Converts hits to PDF user space.
    pub space: Option<PageCoordinateSpace>,
}

/// What a visual search reads of one document.
pub trait VisualSearchDocument: Send + Sync {
    fn id(&self) -> u64;
    fn page_count(&self) -> u32;
    /// The page to search first; the pages go on from there, then round.
    fn start_page(&self) -> u32 {
        0
    }
    /// Draws a page at `scale` pixels per point. It can take long: it is
    /// called on a background thread.
    fn render_page(&self, page_index: u32, scale: f32) -> Result<VisualPageRender, String>;
}

/// A small grey picture of a hit and the page around it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TilePicture {
    pub width: usize,
    pub height: usize,
    /// One byte for each pixel, 0 black, 255 paper.
    pub pixels: Vec<u8>,
}

impl TilePicture {
    /// Cuts `hit`'s box with a margin from `page` and scales it down so its
    /// longer side is at most [`TILE_EDGE_PX`].
    pub fn cut(page: &GreyImage, hit: &VisualHit) -> Self {
        Self::cut_rect(page, hit.pixel_rect)
    }

    /// A whole picture, scaled down as [`Self::cut`] does.
    pub fn of_image(image: &GreyImage) -> Self {
        Self::cut_rect(
            image,
            crate::visual_search::PixelRect {
                x: 0,
                y: 0,
                width: image.width(),
                height: image.height(),
            },
        )
    }

    /// `rect` of `page` with a margin, scaled down as [`Self::cut`] does.
    pub fn cut_rect(page: &GreyImage, rect: crate::visual_search::PixelRect) -> Self {
        let margin = (rect.width.max(rect.height) / 6).max(2);
        let left = rect.x.saturating_sub(margin);
        let top = rect.y.saturating_sub(margin);
        let right = (rect.x + rect.width + margin).min(page.width());
        let bottom = (rect.y + rect.height + margin).min(page.height());
        let (width, height) = (
            right.saturating_sub(left).max(1),
            bottom.saturating_sub(top).max(1),
        );
        let scale = (TILE_EDGE_PX as f32 / width.max(height) as f32).min(1.0);
        let (out_width, out_height) = (
            ((width as f32 * scale).round() as usize).max(1),
            ((height as f32 * scale).round() as usize).max(1),
        );
        let mut pixels = Vec::with_capacity(out_width * out_height);
        for y in 0..out_height {
            let y0 = top + y * height / out_height;
            let y1 = (top + (y + 1) * height / out_height).max(y0 + 1);
            for x in 0..out_width {
                let x0 = left + x * width / out_width;
                let x1 = (left + (x + 1) * width / out_width).max(x0 + 1);
                let (mut total, mut count) = (0u32, 0u32);
                for row in y0..y1.min(page.height()) {
                    for column in x0..x1.min(page.width()) {
                        total += u32::from(page.pixels()[row * page.width() + column]);
                        count += 1;
                    }
                }
                pixels.push(total.checked_div(count).map_or(255, |mean| mean as u8));
            }
        }
        Self {
            width: out_width,
            height: out_height,
            pixels,
        }
    }

    /// The picture as premultiplied BGRA, opaque.
    pub fn to_bgra(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&value| [value, value, value, 255])
            .collect()
    }
}

/// One hit with its picture.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualSearchHit {
    pub document_id: u64,
    pub page_index: u32,
    pub hit: VisualHit,
    pub tile: Arc<TilePicture>,
}

/// What a search found on one page.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualPageOutcome {
    pub document_id: u64,
    pub page_index: u32,
    /// Best score first.
    pub hits: Vec<VisualSearchHit>,
    /// Why the page could not be searched, if it could not.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum VisualSearchEvent {
    Page(VisualPageOutcome),
    /// Every page of the document was searched.
    DocumentDone {
        document_id: u64,
    },
    /// The search found [`VISUAL_HIT_LIMIT`] hits and stopped.
    LimitReached,
    /// The last event: every document is done, or the search stopped.
    Finished {
        cancelled: bool,
    },
}

// Render cache

/// Pages drawn for a search, shared by every window. A page is kept with
/// the state it was drawn from (`validity`: the document's epoch, its
/// edits on the page and its place) and its scale.
pub struct VisualRenderCache {
    budget: usize,
    bytes: usize,
    tick: u64,
    pages: HashMap<(u64, u32), CachedPage>,
    by_use: BTreeMap<u64, (u64, u32)>,
}

struct CachedPage {
    validity: u64,
    scale_bits: u32,
    render: VisualPageRender,
    tick: u64,
}

impl Default for VisualRenderCache {
    fn default() -> Self {
        Self::with_budget(VISUAL_RENDER_CACHE_BYTES)
    }
}

impl VisualRenderCache {
    pub fn with_budget(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            tick: 0,
            pages: HashMap::new(),
            by_use: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// The page as drawn from `validity` at `scale`, if it is kept.
    pub fn get(
        &mut self,
        document_id: u64,
        page_index: u32,
        validity: u64,
        scale: f32,
    ) -> Option<VisualPageRender> {
        self.tick += 1;
        let tick = self.tick;
        let page = self.pages.get_mut(&(document_id, page_index))?;
        if page.validity != validity || page.scale_bits != scale.to_bits() {
            return None;
        }
        self.by_use.remove(&page.tick);
        page.tick = tick;
        self.by_use.insert(tick, (document_id, page_index));
        Some(page.render.clone())
    }

    pub fn insert(
        &mut self,
        document_id: u64,
        page_index: u32,
        validity: u64,
        scale: f32,
        render: VisualPageRender,
    ) {
        self.remove(document_id, page_index);
        let bytes = render.image.pixels().len();
        if bytes > self.budget {
            return;
        }
        while self.bytes + bytes > self.budget {
            let Some((_, key)) = self.by_use.pop_first() else {
                break;
            };
            if let Some(old) = self.pages.remove(&key) {
                self.bytes -= old.render.image.pixels().len();
            }
        }
        self.tick += 1;
        self.by_use.insert(self.tick, (document_id, page_index));
        self.bytes += bytes;
        self.pages.insert(
            (document_id, page_index),
            CachedPage {
                validity,
                scale_bits: scale.to_bits(),
                render,
                tick: self.tick,
            },
        );
    }

    fn remove(&mut self, document_id: u64, page_index: u32) {
        if let Some(old) = self.pages.remove(&(document_id, page_index)) {
            self.by_use.remove(&old.tick);
            self.bytes -= old.render.image.pixels().len();
        }
    }

    /// Forgets a document's pages, when it closes.
    pub fn remove_document(&mut self, document_id: u64) {
        let keys = self
            .pages
            .keys()
            .filter(|(id, _)| *id == document_id)
            .copied()
            .collect::<Vec<_>>();
        for (id, page) in keys {
            self.remove(id, page);
        }
    }
}

/// The cache, shared between threads.
#[derive(Clone, Default)]
pub struct SharedVisualRenderCache(Arc<Mutex<VisualRenderCache>>);

impl SharedVisualRenderCache {
    pub fn with_budget(budget: usize) -> Self {
        Self(Arc::new(Mutex::new(VisualRenderCache::with_budget(budget))))
    }

    pub fn lock(&self) -> MutexGuard<'_, VisualRenderCache> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// Run

type Sink = Arc<dyn Fn(VisualSearchEvent) + Send + Sync>;

struct Shared {
    cancel: Arc<AtomicBool>,
    /// Documents that closed: their pages are no longer drawn or matched.
    closed: Mutex<HashSet<u64>>,
    limit_reached: AtomicBool,
    hits: AtomicUsize,
    sink: Sink,
    /// Free places for a drawn page outside the matchers' queue. Each
    /// document has its own thread, so this limit, not the document count,
    /// sets how many large page pictures are in memory at one time.
    render_slots: Mutex<usize>,
    render_slot_free: Condvar,
}

/// A drawn page's place outside the queue, given back when dropped.
struct RenderSlot<'a>(&'a Shared);

impl Drop for RenderSlot<'_> {
    fn drop(&mut self) {
        let mut slots = self
            .0
            .render_slots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slots += 1;
        self.0.render_slot_free.notify_one();
    }
}

impl Shared {
    /// Waits for a place to draw a page. `None` when the search stops or
    /// `document_id` closes first.
    fn render_slot(&self, document_id: u64) -> Option<RenderSlot<'_>> {
        let mut slots = self
            .render_slots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if self.stopped() || self.closed(document_id) {
                return None;
            }
            if *slots > 0 {
                *slots -= 1;
                return Some(RenderSlot(self));
            }
            // A stop sets a flag and notifies nothing: look again soon.
            slots = self
                .render_slot_free
                .wait_timeout(slots, Duration::from_millis(50))
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    fn stopped(&self) -> bool {
        self.cancel.load(Ordering::Acquire) || self.limit_reached.load(Ordering::Acquire)
    }

    fn closed(&self, document_id: u64) -> bool {
        self.closed
            .lock()
            .is_ok_and(|closed| closed.contains(&document_id))
    }
}

/// A page drawn and waiting for a matcher.
struct PageJob {
    document_id: u64,
    page_index: u32,
    render: Result<VisualPageRender, String>,
    remaining: Arc<AtomicU32>,
}

/// Where the template came from: that page marks the source hit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TemplateSource {
    pub document_id: u64,
    pub page_index: u32,
}

/// A visual search in progress. Dropping it cancels it.
pub struct VisualSearchRun {
    shared: Arc<Shared>,
    threads: Vec<JoinHandle<()>>,
}

/// The matcher threads a search uses: one core is left for the window.
pub fn matcher_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |count| count.get())
        .saturating_sub(1)
        .clamp(1, 8)
}

impl VisualSearchRun {
    /// Starts drawing and searching `documents` on background threads and
    /// returns at once. Events arrive at `sink` from those threads. The last
    /// is [`VisualSearchEvent::Finished`].
    pub fn start(
        documents: Vec<Arc<dyn VisualSearchDocument>>,
        template: Arc<PreparedTemplate>,
        source: Option<TemplateSource>,
        matchers: usize,
        sink: impl Fn(VisualSearchEvent) + Send + Sync + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            cancel: Arc::new(AtomicBool::new(false)),
            closed: Mutex::default(),
            limit_reached: AtomicBool::new(false),
            hits: AtomicUsize::new(0),
            sink: Arc::new(sink),
            render_slots: Mutex::new(matchers.max(1)),
            render_slot_free: Condvar::new(),
        });
        if documents.is_empty() {
            (shared.sink)(VisualSearchEvent::Finished { cancelled: false });
            return Self {
                shared,
                threads: Vec::new(),
            };
        }
        let matchers = matchers.max(1);
        // A page waits for a matcher while the next one is drawn: at most
        // this many drawn pages wait at one time.
        let (jobs, queue) = async_channel::bounded::<PageJob>(matchers);
        let scale = template.page_scale();
        let mut threads = Vec::new();
        for document in documents {
            let (shared, jobs) = (shared.clone(), jobs.clone());
            threads.push(
                std::thread::Builder::new()
                    .name("butter-paper-visual-render".into())
                    .spawn(move || render_document(document.as_ref(), scale, &shared, &jobs))
                    .expect("a visual search thread starts"),
            );
        }
        drop(jobs);
        let running = Arc::new(AtomicUsize::new(matchers));
        for _ in 0..matchers {
            let (shared, queue, template, running) = (
                shared.clone(),
                queue.clone(),
                template.clone(),
                running.clone(),
            );
            threads.push(
                std::thread::Builder::new()
                    .name("butter-paper-visual-match".into())
                    .spawn(move || {
                        while let Ok(job) = queue.recv_blocking() {
                            match_page(job, &template, source, &shared);
                        }
                        if running.fetch_sub(1, Ordering::AcqRel) == 1 {
                            (shared.sink)(VisualSearchEvent::Finished {
                                cancelled: shared.cancel.load(Ordering::Acquire),
                            });
                        }
                    })
                    .expect("a visual search thread starts"),
            );
        }
        Self { shared, threads }
    }

    /// Stops the search after the pages being drawn and matched now.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.shared.cancel.load(Ordering::Acquire)
    }

    /// Stops searching one document, which closed; the others go on.
    pub fn cancel_document(&self, document_id: u64) {
        if let Ok(mut closed) = self.shared.closed.lock() {
            closed.insert(document_id);
        }
    }

    /// Waits until every thread of the search has ended.
    pub fn join(mut self) {
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

impl Drop for VisualSearchRun {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The pages from `start` to the last, then from the first to `start`.
fn page_order(page_count: u32, start: u32) -> impl Iterator<Item = u32> {
    let start = start.min(page_count.saturating_sub(1));
    (start..page_count).chain(0..start)
}

/// Draws a document's pages one at a time and hands them to the matchers.
/// After the last page, or a stop, the document's matcher jobs end it.
fn render_document(
    document: &dyn VisualSearchDocument,
    scale: f32,
    shared: &Shared,
    jobs: &async_channel::Sender<PageJob>,
) {
    let document_id = document.id();
    let page_count = document.page_count();
    if page_count == 0 {
        (shared.sink)(VisualSearchEvent::DocumentDone { document_id });
        return;
    }
    let remaining = Arc::new(AtomicU32::new(page_count));
    for page_index in page_order(page_count, document.start_page()) {
        let Some(_slot) = shared.render_slot(document_id) else {
            return;
        };
        let render = document.render_page(page_index, scale);
        if shared.stopped() || shared.closed(document_id) {
            return;
        }
        let job = PageJob {
            document_id,
            page_index,
            render,
            remaining: remaining.clone(),
        };
        if jobs.send_blocking(job).is_err() {
            return;
        }
    }
}

fn match_page(
    job: PageJob,
    template: &PreparedTemplate,
    source: Option<TemplateSource>,
    shared: &Shared,
) {
    if shared.stopped() || shared.closed(job.document_id) {
        return;
    }
    let PageJob {
        document_id,
        page_index,
        render,
        remaining,
    } = job;
    let mut outcome = VisualPageOutcome {
        document_id,
        page_index,
        hits: Vec::new(),
        error: None,
    };
    match render {
        Err(error) => outcome.error = Some(error),
        Ok(render) => {
            let contains_source = source.is_some_and(|source| {
                source.document_id == document_id && source.page_index == page_index
            });
            let page = VisualPage {
                image: &render.image,
                scale: template.page_scale(),
                space: render.space,
                contains_source,
            };
            match template.search_page(page, &shared.cancel) {
                Ok(matches) => {
                    outcome.hits = matches
                        .hits
                        .into_iter()
                        .map(|hit| VisualSearchHit {
                            document_id,
                            page_index,
                            tile: Arc::new(TilePicture::cut(&render.image, &hit)),
                            hit,
                        })
                        .collect();
                }
                Err(VisualSearchError::Cancelled) => return,
                Err(error) => outcome.error = Some(error.to_string()),
            }
        }
    }
    // The cap holds across pages matched at once.
    let before = shared.hits.fetch_add(outcome.hits.len(), Ordering::AcqRel);
    let room = VISUAL_HIT_LIMIT.saturating_sub(before);
    if outcome.hits.len() >= room && !outcome.hits.is_empty() {
        outcome.hits.truncate(room);
        if !shared.limit_reached.swap(true, Ordering::AcqRel) {
            (shared.sink)(VisualSearchEvent::Page(outcome));
            (shared.sink)(VisualSearchEvent::LimitReached);
        }
        return;
    }
    (shared.sink)(VisualSearchEvent::Page(outcome));
    if remaining.fetch_sub(1, Ordering::AcqRel) == 1 {
        (shared.sink)(VisualSearchEvent::DocumentDone { document_id });
    }
}

// Results

/// What one document's search found so far.
#[derive(Clone, Debug, Default)]
pub struct VisualDocumentResults {
    pub document_id: u64,
    pub name: String,
    pub page_count: u32,
    /// Hits by page, in page order; on a page, best score first.
    pages: BTreeMap<u32, Vec<VisualSearchHit>>,
    pub pages_searched: u32,
    /// Pages that could not be searched, and why (the first).
    pub pages_failed: u32,
    pub first_error: Option<String>,
    pub done: bool,
}

impl VisualDocumentResults {
    /// Pages with hits at `threshold` or above, with those hits. The source
    /// hit shows at any threshold.
    pub fn pages(&self, threshold: f32) -> impl Iterator<Item = (u32, Vec<&VisualSearchHit>)> {
        self.pages.iter().filter_map(move |(page, hits)| {
            let shown = hits
                .iter()
                .filter(|hit| shows(hit, threshold))
                .collect::<Vec<_>>();
            (!shown.is_empty()).then_some((*page, shown))
        })
    }

    pub fn hit_count(&self, threshold: f32) -> usize {
        self.pages
            .values()
            .flatten()
            .filter(|hit| shows(hit, threshold))
            .count()
    }
}

fn shows(hit: &VisualSearchHit, threshold: f32) -> bool {
    hit.hit.source || hit.hit.score >= threshold - 1e-4
}

/// The events of a visual search, merged. The similarity threshold only
/// filters what is kept: a change of it needs no new search.
#[derive(Clone, Debug, Default)]
pub struct VisualResults {
    documents: Vec<VisualDocumentResults>,
    limit_reached: bool,
    finished: bool,
    cancelled: bool,
}

/// Where a hit is: its document, page and place among that page's hits.
pub type VisualHitKey = (u64, u32, usize);

impl VisualResults {
    /// Empty results for `documents`, given as `(id, name, page count)` in
    /// the order a list shows them.
    pub fn new(documents: impl IntoIterator<Item = (u64, String, u32)>) -> Self {
        Self {
            documents: documents
                .into_iter()
                .map(|(document_id, name, page_count)| VisualDocumentResults {
                    document_id,
                    name,
                    page_count,
                    ..Default::default()
                })
                .collect(),
            ..Self::default()
        }
    }

    pub fn apply(&mut self, event: VisualSearchEvent) {
        match event {
            VisualSearchEvent::Page(page) => {
                let Some(document) = self
                    .documents
                    .iter_mut()
                    .find(|document| document.document_id == page.document_id)
                else {
                    return;
                };
                document.pages_searched += 1;
                if let Some(error) = page.error {
                    document.pages_failed += 1;
                    document.first_error.get_or_insert(error);
                }
                if !page.hits.is_empty() {
                    document.pages.insert(page.page_index, page.hits);
                }
            }
            VisualSearchEvent::DocumentDone { document_id } => {
                if let Some(document) = self
                    .documents
                    .iter_mut()
                    .find(|document| document.document_id == document_id)
                {
                    document.done = true;
                }
            }
            VisualSearchEvent::LimitReached => self.limit_reached = true,
            VisualSearchEvent::Finished { cancelled } => {
                self.finished = true;
                self.cancelled = cancelled;
            }
        }
    }

    pub fn documents(&self) -> &[VisualDocumentResults] {
        &self.documents
    }

    pub fn hit_count(&self, threshold: f32) -> usize {
        self.documents
            .iter()
            .map(|document| document.hit_count(threshold))
            .sum()
    }

    /// Pages with a hit at `threshold` or above.
    pub fn page_count_with_hits(&self, threshold: f32) -> usize {
        self.documents
            .iter()
            .map(|document| document.pages(threshold).count())
            .sum()
    }

    /// Documents with a hit at `threshold` or above.
    pub fn document_count_with_hits(&self, threshold: f32) -> usize {
        self.documents
            .iter()
            .filter(|document| document.hit_count(threshold) > 0)
            .count()
    }

    pub fn limit_reached(&self) -> bool {
        self.limit_reached
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn was_cancelled(&self) -> bool {
        self.cancelled
    }

    /// Pages searched and pages to search, over all documents.
    pub fn progress(&self) -> (u32, u32) {
        self.documents.iter().fold((0, 0), |(done, all), document| {
            (done + document.pages_searched, all + document.page_count)
        })
    }

    pub fn pages_failed(&self) -> u32 {
        self.documents
            .iter()
            .map(|document| document.pages_failed)
            .sum()
    }

    pub fn first_error(&self) -> Option<&str> {
        self.documents
            .iter()
            .find_map(|document| document.first_error.as_deref())
    }

    /// A hit by its key, whatever the threshold.
    pub fn hit(&self, (document_id, page_index, index): VisualHitKey) -> Option<&VisualSearchHit> {
        self.documents
            .iter()
            .find(|document| document.document_id == document_id)?
            .pages
            .get(&page_index)?
            .get(index)
    }

    /// The hits shown at `threshold`, in list order: by document, then
    /// page, then best first.
    pub fn shown_keys(&self, threshold: f32) -> Vec<VisualHitKey> {
        let mut keys = Vec::new();
        for document in &self.documents {
            for (page, hits) in &document.pages {
                keys.extend(
                    hits.iter()
                        .enumerate()
                        .filter(|(_, hit)| shows(hit, threshold))
                        .map(|(index, _)| (document.document_id, *page, index)),
                );
            }
        }
        keys
    }

    /// The hits of one page of a document, whatever the threshold.
    pub fn page_hits(&self, document_id: u64, page_index: u32) -> &[VisualSearchHit] {
        self.documents
            .iter()
            .find(|document| document.document_id == document_id)
            .and_then(|document| document.pages.get(&page_index))
            .map_or(&[], Vec::as_slice)
    }

    /// Drops a document that closed.
    pub fn remove_document(&mut self, document_id: u64) {
        self.documents
            .retain(|document| document.document_id != document_id);
    }
}

// The document as the app reads it

/// A page of an open document, as a search draws it.
#[derive(Clone, Copy, Debug)]
pub struct VisualPageState {
    /// The page as shown.
    pub rotation: crate::annotation_model::PageRotation,
    pub space: Option<PageCoordinateSpace>,
    /// Changes when what the page shows changes (its edits, its place).
    pub validity: u64,
}

/// An open document, drawn by its PDF worker for a search.
pub struct ResourceVisualDocument {
    id: u64,
    resource: Arc<dyn crate::document_resource::NativeDocumentResource>,
    pages: Vec<VisualPageState>,
    start_page: u32,
    cache: SharedVisualRenderCache,
}

impl ResourceVisualDocument {
    pub fn new(
        id: u64,
        resource: Arc<dyn crate::document_resource::NativeDocumentResource>,
        pages: Vec<VisualPageState>,
        cache: SharedVisualRenderCache,
    ) -> Self {
        Self {
            id,
            resource,
            pages,
            start_page: 0,
            cache,
        }
    }

    /// Searches from `page_index`: the template's page gives its source
    /// hit first.
    pub fn starting_at(mut self, page_index: u32) -> Self {
        self.start_page = page_index;
        self
    }
}

impl VisualSearchDocument for ResourceVisualDocument {
    fn id(&self) -> u64 {
        self.id
    }

    fn page_count(&self) -> u32 {
        self.pages.len() as u32
    }

    fn start_page(&self) -> u32 {
        self.start_page
    }

    fn render_page(&self, page_index: u32, scale: f32) -> Result<VisualPageRender, String> {
        let state = self
            .pages
            .get(page_index as usize)
            .ok_or_else(|| "the page is outside the document".to_owned())?;
        if let Some(render) = self
            .cache
            .lock()
            .get(self.id, page_index, state.validity, scale)
        {
            return Ok(render);
        }
        let raster = self
            .resource
            .render_search_page(page_index, scale, state.rotation)?;
        let image = GreyImage::from_bgra(
            raster.width() as usize,
            raster.height() as usize,
            raster.width() as usize * 4,
            raster.pixels_bgra(),
        )
        .map_err(|error| error.to_string())?;
        let render = VisualPageRender {
            image: Arc::new(image),
            space: state.space,
        };
        self.cache
            .lock()
            .insert(self.id, page_index, state.validity, scale, render.clone());
        Ok(render)
    }
}
