//! The Visual mode of the search panel: find more of a symbol, as a picture.
//!
//! The picture to find comes from a box drawn on a page with the Snapshot
//! tool (Draw box, `BoxPurpose::VisualSearch`; it leaves no markup and copies
//! nothing) or from a picture on the clipboard (Paste picture). The search
//! draws every page in scope with its content only, in its real colours even
//! while the view shows Dark pages, after everything the viewer asks for,
//! and matches the pages on background threads (`visual_search_run`).
//!
//! Results group by document, then page, with a small picture and the score
//! of each hit. The similarity slider filters what was found without a new
//! search. A click on a hit, or Enter, goes to it and outlines it on the
//! page, in the paper's search colours so that it shows on Dark pages too.

use std::{cell::RefCell, time::Instant};

use gpui::{Global, RenderImage};
use gpui_component::slider::{Slider, SliderEvent, SliderState};

use super::*;
use crate::visual_search::{
    DEFAULT_SIMILARITY, GreyImage, SCORE_FLOOR, SizeRange, VisualSearchOptions, VisualTemplate,
    working_scale,
};
use crate::visual_search_run::{
    ResourceVisualDocument, SharedVisualRenderCache, TemplateSource, TilePicture, VisualHitKey,
    VisualPageState, VisualResults, VisualSearchDocument, VisualSearchEvent, VisualSearchRun,
    matcher_threads,
};

pub const DOCUMENT_SEARCH_MODE_TEXT_ID: &str = "document-search-mode-text";
pub const DOCUMENT_SEARCH_MODE_VISUAL_ID: &str = "document-search-mode-visual";
pub const DOCUMENT_VISUAL_TEMPLATE_ID: &str = "document-visual-template";
pub const DOCUMENT_VISUAL_DRAW_BOX_ID: &str = "document-visual-draw-box";
pub const DOCUMENT_VISUAL_PASTE_ID: &str = "document-visual-paste";
pub const DOCUMENT_VISUAL_CLEAR_ID: &str = "document-visual-clear";
pub const DOCUMENT_VISUAL_SIMILARITY_ID: &str = "document-visual-similarity";
pub const DOCUMENT_VISUAL_TURNS_ID: &str = "document-visual-turns";
pub const DOCUMENT_VISUAL_SIZES_ID: &str = "document-visual-sizes";
pub const DOCUMENT_VISUAL_MIRRORED_ID: &str = "document-visual-mirrored";
pub const DOCUMENT_VISUAL_PROGRESS_ID: &str = "document-visual-progress";
pub const DOCUMENT_VISUAL_STOP_ID: &str = "document-visual-stop";
pub const DOCUMENT_VISUAL_SUMMARY_ID: &str = "document-visual-summary";
pub const DOCUMENT_VISUAL_RESULTS_ID: &str = "document-visual-results";
pub const DOCUMENT_VISUAL_ERROR_ID: &str = "document-visual-error";
pub const DOCUMENT_SEARCH_TRY_VISUAL_ID: &str = "document-search-try-visual";
pub(super) const DOCUMENT_SEARCH_MODE_ID: &str = "document-search-mode";

/// The debug selector of the hit picture at `index` of the shown hits.
pub fn visual_tile_id(index: usize) -> String {
    format!("document-visual-tile-{index}")
}

/// The debug selector of the result row at `index` of the Visual list.
pub fn visual_row_id(index: usize) -> String {
    format!("document-visual-row-{index}")
}

/// Hit pictures in one row of the list.
const TILES_PER_ROW: usize = 3;
/// How often the list takes what the search found.
const VISUAL_UPDATE_INTERVAL: Duration = Duration::from_millis(100);
/// How long a document rests, after a change, before the search runs again.
const VISUAL_EDIT_DELAY: Duration = Duration::from_millis(600);
/// The similarity the slider can show, in percent (owner decision: from
/// 70 %). The search keeps hits from [`SCORE_FLOOR`], so that its coarse
/// cutoffs stay as loose as before.
const SIMILARITY_MIN: f32 = 70.;
const SIMILARITY_MAX: f32 = 99.;

/// What the panel searches for.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchMode {
    #[default]
    Text,
    Visual,
}

/// Where the picture to find came from.
#[derive(Clone, Debug, PartialEq)]
pub enum TemplateOrigin {
    /// A box drawn on a page: its document, page and the document's name.
    PageBox {
        document_id: u64,
        page_index: u32,
        title: String,
    },
    /// A picture from the clipboard.
    Pasted,
}

struct TemplateCard {
    template: Arc<VisualTemplate>,
    origin: TemplateOrigin,
    /// The picture's size on the page, in points.
    size_pt: (f32, f32),
    picture: Arc<TilePicture>,
    image: Option<Arc<RenderImage>>,
}

/// A row of the Visual list.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum VisualRowKey {
    Document(u64),
    Page(u64, u32),
    /// Hits of a page: its document, page and the row's first place among
    /// the page's shown hits.
    Tiles(u64, u32, usize),
}

/// What the keyboard can stand on.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum VisualItem {
    Document(u64),
    Page(u64, u32),
    Hit(VisualHitKey),
}

/// A hit picture as the list shows it, and as tests read it.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualTileView {
    /// The score in percent.
    pub score: u8,
    /// The box the picture came from.
    pub source: bool,
    /// Clockwise turn on the page, in degrees, after the mirror.
    pub rotation: u16,
    pub mirrored: bool,
    /// The size against the picture, in percent.
    pub size: u16,
    /// The hit's box in page space: x0, y0, x1, y1.
    pub bounds: Option<[f64; 4]>,
    pub selected: bool,
    pub opened: bool,
}

/// A row of the Visual list as tests read it.
#[derive(Clone, Debug, PartialEq)]
pub enum VisualPanelRow {
    Document {
        name: String,
        count: usize,
        collapsed: bool,
    },
    Page {
        page_number: u32,
        count: usize,
        collapsed: bool,
    },
    Tiles(Vec<VisualTileView>),
}

/// What the Visual mode shows now, for tests and the review driver's log.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualPanelSummary {
    pub mode: SearchMode,
    /// The picture's origin, when there is one.
    pub template: Option<TemplateOrigin>,
    /// "plan.pdf, page 3" or "Pasted picture", and the size in points.
    pub template_text: Vec<String>,
    /// Why there is no picture, or no search.
    pub error: Option<String>,
    pub similarity: u8,
    pub turns: bool,
    pub sizes: bool,
    pub mirrored: bool,
    /// Whether a search runs now.
    pub running: bool,
    pub finished: bool,
    pub cancelled: bool,
    /// "Searching page 3 of 20…", "Stopped after 4 of 20 pages".
    pub progress: String,
    /// "23 matches on 9 pages in 2 documents".
    pub summary: String,
    pub hits: usize,
    pub rows: Vec<VisualPanelRow>,
    /// The shown hits, in list order.
    pub shown: Vec<VisualTileView>,
    /// The place of the opened hit among the shown ones.
    pub opened: Option<usize>,
    /// Whether the keyboard stands on a hit, and its place.
    pub selected_hit: Option<usize>,
    /// Pages searched and pages in scope.
    pub pages: (u32, u32),
}

/// A searched document: where it is.
struct VisualPanelDocument {
    label: String,
    window: Option<AnyWindowHandle>,
    workspace: WeakEntity<DocumentWorkspace>,
    /// What the search drew, page by page: a change runs it again.
    read: Vec<u64>,
}

pub(super) struct VisualPanel {
    template: Option<TemplateCard>,
    /// A box's picture being cut, or a search being prepared.
    pending: Option<Task<()>>,
    error: Option<String>,
    similarity: Entity<SliderState>,
    threshold: f32,
    turns: bool,
    sizes: bool,
    mirrored: bool,
    results: VisualResults,
    run: Option<VisualSearchRun>,
    generation: u64,
    delay: Option<Task<()>>,
    documents: HashMap<u64, VisualPanelDocument>,
    /// Groups the user collapsed.
    collapsed: HashSet<VisualItem>,
    rows: Rc<Vec<(VisualRowKey, VisualPanelRow)>>,
    list: ListState,
    selected: Option<VisualItem>,
    opened: Option<VisualHitKey>,
    images: Rc<RefCell<HashMap<VisualHitKey, Arc<RenderImage>>>>,
    /// When the search started, its first hit and its end, for the log.
    started: Option<Instant>,
    first_hit: Option<Duration>,
    finished_after: Option<Duration>,
    _similarity_events: Subscription,
}

/// The pages a visual search drew, for every window.
#[derive(Default)]
struct VisualCache(SharedVisualRenderCache);

impl Global for VisualCache {}

/// What one visual search reads of a document.
struct VisualTarget {
    id: u64,
    title: String,
    window_number: usize,
    window: Option<AnyWindowHandle>,
    workspace: WeakEntity<DocumentWorkspace>,
    resource: Arc<dyn NativeDocumentResource>,
    pages: Vec<VisualPageState>,
    /// The largest page, as shown, in points.
    largest: (f32, f32),
}

fn percent(value: f32) -> u8 {
    (value * 100.).round().clamp(0., 100.) as u8
}

fn hit_bounds(hit: &crate::visual_search::VisualHit) -> Option<[f64; 4]> {
    hit.pdf_rect
        .map(|rect| [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height])
}

fn tile_view(
    hit: &crate::visual_search_run::VisualSearchHit,
    selected: bool,
    opened: bool,
) -> VisualTileView {
    VisualTileView {
        score: percent(hit.hit.score),
        source: hit.hit.source,
        rotation: hit.hit.rotation.degrees(),
        mirrored: hit.hit.mirrored,
        size: (hit.hit.size * 100.).round() as u16,
        bounds: hit_bounds(&hit.hit),
        selected,
        opened,
    }
}

/// How a hit lies against the picture, for its tooltip.
fn tile_tooltip(tile: &VisualTileView) -> String {
    if tile.source {
        return "The box the picture came from".into();
    }
    let mut parts = vec![format!("{} % similar", tile.score)];
    if tile.rotation != 0 {
        parts.push(format!("turned {}°", tile.rotation));
    }
    if tile.mirrored {
        parts.push("mirrored".into());
    }
    if tile.size != 100 {
        parts.push(format!("{} % size", tile.size));
    }
    parts.join(", ")
}

/// "23 matches on 9 pages in 2 documents", "Searching…", "No matches".
pub(super) fn visual_summary_text(
    finished: bool,
    hits: usize,
    pages: usize,
    documents: usize,
) -> String {
    if hits == 0 {
        return if finished {
            "No matches".into()
        } else {
            "Searching…".into()
        };
    }
    let plural = |count: usize, one: &str, many: &str| {
        if count == 1 {
            format!("1 {one}")
        } else {
            format!("{count} {many}")
        }
    };
    format!(
        "{} on {} in {}",
        plural(hits, "match", "matches"),
        plural(pages, "page", "pages"),
        plural(documents, "document", "documents")
    )
}

/// "Searching page 3 of 20…" while it runs, "Stopped after 3 of 20 pages"
/// when stopped, and nothing when done.
pub(super) fn visual_progress_text(running: bool, cancelled: bool, done: u32, all: u32) -> String {
    if running {
        format!("Searching page {} of {all}…", (done + 1).min(all.max(1)))
    } else if cancelled {
        format!("Stopped after {done} of {all} pages")
    } else {
        String::new()
    }
}

impl VisualPanel {
    pub(super) fn new(window: &mut Window, cx: &mut Context<DocumentWorkspace>) -> Self {
        let similarity = cx.new(|_| {
            SliderState::new()
                .min(SIMILARITY_MIN)
                .max(SIMILARITY_MAX)
                .step(1.)
                .default_value(DEFAULT_SIMILARITY * 100.)
        });
        let events = cx.subscribe_in(
            &similarity,
            window,
            |workspace, _, event: &SliderEvent, _, cx| {
                let (SliderEvent::Change(value) | SliderEvent::Release(value)) = event;
                workspace.set_visual_similarity(value.start() / 100., cx);
            },
        );
        Self {
            template: None,
            pending: None,
            error: None,
            similarity,
            threshold: DEFAULT_SIMILARITY,
            turns: true,
            sizes: true,
            mirrored: true,
            results: VisualResults::default(),
            run: None,
            generation: 0,
            delay: None,
            documents: HashMap::new(),
            collapsed: HashSet::new(),
            rows: Rc::default(),
            list: ListState::new(0, ListAlignment::Top, px(240.)),
            selected: None,
            opened: None,
            images: Rc::default(),
            started: None,
            first_hit: None,
            finished_after: None,
            _similarity_events: events,
        }
    }

    fn running(&self) -> bool {
        self.run.is_some() && !self.results.is_finished()
    }

    /// The pages of a document with hits shown, and the hits' keys.
    fn shown_pages(&self, document_id: u64) -> Vec<(u32, Vec<VisualHitKey>)> {
        let Some(document) = self
            .results
            .documents()
            .iter()
            .find(|document| document.document_id == document_id)
        else {
            return Vec::new();
        };
        let all = self.results.shown_keys(self.threshold);
        document
            .pages(self.threshold)
            .map(|(page, _)| {
                (
                    page,
                    all.iter()
                        .filter(|(id, index, _)| *id == document_id && *index == page)
                        .copied()
                        .collect(),
                )
            })
            .collect()
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        for document in self.results.documents() {
            let id = document.document_id;
            let count = document.hit_count(self.threshold);
            if count == 0 {
                continue;
            }
            let collapsed = self.collapsed.contains(&VisualItem::Document(id));
            rows.push((
                VisualRowKey::Document(id),
                VisualPanelRow::Document {
                    name: self
                        .documents
                        .get(&id)
                        .map_or_else(|| document.name.clone(), |document| document.label.clone()),
                    count,
                    collapsed,
                },
            ));
            if collapsed {
                continue;
            }
            for (page, keys) in self.shown_pages(id) {
                let page_collapsed = self.collapsed.contains(&VisualItem::Page(id, page));
                rows.push((
                    VisualRowKey::Page(id, page),
                    VisualPanelRow::Page {
                        page_number: page + 1,
                        count: keys.len(),
                        collapsed: page_collapsed,
                    },
                ));
                if page_collapsed {
                    continue;
                }
                for (chunk_index, chunk) in keys.chunks(TILES_PER_ROW).enumerate() {
                    let tiles = chunk
                        .iter()
                        .filter_map(|key| {
                            let hit = self.results.hit(*key)?;
                            Some(tile_view(
                                hit,
                                self.selected == Some(VisualItem::Hit(*key)),
                                self.opened == Some(*key),
                            ))
                        })
                        .collect();
                    rows.push((
                        VisualRowKey::Tiles(id, page, chunk_index * TILES_PER_ROW),
                        VisualPanelRow::Tiles(tiles),
                    ));
                }
            }
        }
        let old = self.rows.clone();
        let same = old
            .iter()
            .zip(&rows)
            .take_while(|((old, _), (new, _))| old == new)
            .count();
        if same < old.len() || same < rows.len() {
            super::splice_keeping_top(&self.list, same..old.len(), rows.len() - same);
        }
        self.rows = Rc::new(rows);
        if self
            .selected
            .is_some_and(|selected| !self.items().contains(&selected))
        {
            self.selected = None;
        }
    }

    /// What the keyboard can stand on, in list order.
    fn items(&self) -> Vec<VisualItem> {
        let mut items = Vec::new();
        for (key, _) in self.rows.iter() {
            match *key {
                VisualRowKey::Document(id) => items.push(VisualItem::Document(id)),
                VisualRowKey::Page(id, page) => items.push(VisualItem::Page(id, page)),
                VisualRowKey::Tiles(id, page, first) => {
                    let keys = self
                        .shown_pages(id)
                        .into_iter()
                        .find(|(index, _)| *index == page)
                        .map(|(_, keys)| keys)
                        .unwrap_or_default();
                    items.extend(
                        keys.into_iter()
                            .skip(first)
                            .take(TILES_PER_ROW)
                            .map(VisualItem::Hit),
                    );
                }
            }
        }
        items
    }

    fn row_of(&self, item: VisualItem) -> Option<usize> {
        self.rows.iter().position(|(key, _)| match (*key, item) {
            (VisualRowKey::Document(a), VisualItem::Document(b)) => a == b,
            (VisualRowKey::Page(a, p), VisualItem::Page(b, q)) => a == b && p == q,
            (VisualRowKey::Tiles(id, page, first), VisualItem::Hit((hid, hpage, index))) => {
                id == hid && page == hpage && {
                    let keys = self
                        .shown_pages(id)
                        .into_iter()
                        .find(|(p, _)| *p == page)
                        .map(|(_, keys)| keys)
                        .unwrap_or_default();
                    keys.iter()
                        .position(|key| *key == (hid, hpage, index))
                        .is_some_and(|place| place >= first && place < first + TILES_PER_ROW)
                }
            }
            _ => false,
        })
    }

    /// Drops the hit pictures the list made, freeing their textures.
    fn take_images(&mut self) -> Vec<Arc<RenderImage>> {
        self.images
            .borrow_mut()
            .drain()
            .map(|(_, image)| image)
            .collect()
    }

    fn options(&self) -> VisualSearchOptions {
        let pasted = matches!(
            self.template.as_ref().map(|card| &card.origin),
            Some(TemplateOrigin::Pasted)
        );
        VisualSearchOptions {
            turns: self.turns,
            mirrored: self.mirrored,
            sizes: self.sizes.then_some(if pasted {
                SizeRange::PASTED_PICTURE
            } else {
                SizeRange::PAGE_BOX
            }),
            min_score: SCORE_FLOOR,
            max_hits: crate::visual_search::DEFAULT_MAX_HITS,
        }
    }
}

impl DocumentWorkspace {
    fn visual_cache(cx: &mut App) -> SharedVisualRenderCache {
        cx.default_global::<VisualCache>().0.clone()
    }

    /// The pages of a document as a visual search draws them now.
    pub(super) fn visual_page_states(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<Vec<VisualPageState>> {
        let session = self.session(document_id, cx)?.read(cx);
        if !matches!(session.status, NativeDocumentStatus::Ready) {
            return None;
        }
        let edits = DocumentTextEdits {
            text_edits: self.page_text_edits_of(document_id, cx),
            table_edits: self.page_table_edits_of(document_id, cx),
            content_moves: self.page_content_moves_of(document_id, cx),
            explodes: self.page_content_explodes_of(document_id, cx),
            bases: self.page_bases_of(document_id, cx),
        };
        let epoch = session.resource_epoch;
        let slots = &session.arranged_slots;
        Some(
            (0..session.page_sizes.len() as u32)
                .map(|page| {
                    let rotation = session
                        .page_rotation(page)
                        .unwrap_or(crate::annotation_model::PageRotation::Degrees0);
                    let mut hasher = std::collections::hash_map::DefaultHasher::new();
                    use std::hash::{Hash, Hasher};
                    epoch.hash(&mut hasher);
                    edits.fingerprint(page).hash(&mut hasher);
                    format!("{:?}", slots.get(page as usize)).hash(&mut hasher);
                    rotation.degrees().hash(&mut hasher);
                    VisualPageState {
                        rotation,
                        space: session.annotation_page_coordinate_space(page),
                        validity: hasher.finish(),
                    }
                })
                .collect(),
        )
    }

    /// One document, ready for a visual search to draw.
    fn visual_target(
        &self,
        document_id: DocumentId,
        window_number: usize,
        window: Option<AnyWindowHandle>,
        workspace: WeakEntity<DocumentWorkspace>,
        cx: &App,
    ) -> Option<VisualTarget> {
        let pages = self.visual_page_states(document_id, cx)?;
        let session = self.session(document_id, cx)?.read(cx);
        let resource = session.resource.clone()?;
        let largest = pages
            .iter()
            .filter_map(|page| page.space)
            .map(|space| {
                let (width, height) = space.display_size_points();
                (width as f32, height as f32)
            })
            .fold((0f32, 0f32), |best, size| {
                if size.0 * size.1 > best.0 * best.1 {
                    size
                } else {
                    best
                }
            });
        Some(VisualTarget {
            id: document_id.value(),
            title: session.title.clone(),
            window_number,
            window,
            workspace,
            resource,
            pages,
            largest,
        })
    }

    /// The documents the scope names: this window's tabs first, then the
    /// other windows' in window order.
    fn visual_targets(&self, scope: SearchScope, cx: &mut Context<Self>) -> Vec<VisualTarget> {
        let own = cx.entity();
        let windows = open_workspaces(cx);
        let own_number = windows
            .iter()
            .position(|(_, workspace)| *workspace == own)
            .map_or(1, |index| index + 1);
        let own_ids = match scope {
            SearchScope::ThisDocument => self.active_document_id.into_iter().collect(),
            SearchScope::AllOpenDocuments => self.session_order(cx),
        };
        let mut targets = own_ids
            .into_iter()
            .filter_map(|id| self.visual_target(id, own_number, None, own.downgrade(), cx))
            .collect::<Vec<_>>();
        if scope == SearchScope::AllOpenDocuments {
            for (index, (handle, workspace)) in windows.iter().enumerate() {
                if *workspace == own {
                    continue;
                }
                let other = workspace.read(cx);
                for id in other.session_order(cx) {
                    if let Some(target) =
                        other.visual_target(id, index + 1, Some(*handle), workspace.downgrade(), cx)
                    {
                        targets.push(target);
                    }
                }
            }
        }
        targets
    }

    fn visual_panel_mut(&mut self) -> Option<&mut VisualPanel> {
        self.search_panel.as_mut().map(|panel| &mut panel.visual)
    }

    /// The Text and Visual switch.
    pub fn set_search_mode(&mut self, mode: SearchMode, cx: &mut Context<Self>) {
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        if panel.mode == mode {
            return;
        }
        panel.mode = mode;
        panel.focus_frames = 0;
        // One mode's marks show at a time.
        self.clear_search_marks_everywhere(cx);
        cx.notify();
    }

    /// Draw box: the Snapshot tool's next box on the page in view is the
    /// picture to find.
    pub(super) fn begin_visual_search_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        // The page takes the keys: Esc stops the box.
        self.workspace_focus.focus(window, cx);
        if let Some(visual) = self.visual_panel_mut() {
            visual.error = None;
        }
        self.begin_signature_box(
            document_id,
            crate::document_workspace::signature_capture::BoxPurpose::VisualSearch,
            cx,
        );
    }

    /// A box drawn for a visual search: its picture is cut from the page as
    /// a search draws it, and the search starts.
    pub(crate) fn take_visual_search_box(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        rect: PdfRect,
        cx: &mut Context<Self>,
    ) {
        let scope = self
            .search_panel
            .as_ref()
            .map_or(SearchScope::AllOpenDocuments, |panel| panel.scope);
        let own = cx.entity().downgrade();
        let Some(target) = self.visual_target(document_id, 1, None, own, cx) else {
            return;
        };
        let Some(space) = target
            .pages
            .get(page_index as usize)
            .and_then(|page| page.space)
        else {
            return;
        };
        let viewport_box = space.pdf_rect_to_viewport(crate::page_geometry::PdfRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        });
        let largest = self
            .visual_targets(scope, cx)
            .iter()
            .map(|target| target.largest)
            .chain([target.largest])
            .fold((0f32, 0f32), |best, size| {
                if size.0 * size.1 > best.0 * best.1 {
                    size
                } else {
                    best
                }
            });
        let scale = working_scale(viewport_box.width.min(viewport_box.height) as f32, largest);
        let title = target.title.clone();
        let document = ResourceVisualDocument::new(
            target.id,
            target.resource.clone(),
            target.pages.clone(),
            Self::visual_cache(cx),
        );
        let task = cx.background_executor().spawn(async move {
            let mut scale = scale;
            let mut render = document.render_page(page_index, scale)?;
            let mut template = VisualTemplate::from_page_box(&render.image, scale, viewport_box)
                .map_err(|error| error.to_string())?;
            // The scale for what the box holds, trimmed to its ink: a loose
            // box would otherwise give a coarse picture.
            let trimmed = template.source_box().unwrap_or(viewport_box);
            let better = working_scale(trimmed.width.min(trimmed.height) as f32, largest);
            if better != scale {
                scale = better;
                render = document.render_page(page_index, scale)?;
                let margin = 2. / f64::from(scale);
                let around = crate::page_geometry::PdfRect {
                    x: trimmed.x - margin,
                    y: trimmed.y - margin,
                    width: trimmed.width + 2. * margin,
                    height: trimmed.height + 2. * margin,
                };
                template = VisualTemplate::from_page_box(&render.image, scale, around)
                    .map_err(|error| error.to_string())?;
            }
            let trimmed = template.source_box().unwrap_or(viewport_box);
            let scale64 = f64::from(scale);
            let picture = TilePicture::cut_rect(
                &render.image,
                crate::visual_search::PixelRect {
                    x: (trimmed.x * scale64).floor().max(0.) as usize,
                    y: (trimmed.y * scale64).floor().max(0.) as usize,
                    width: (trimmed.width * scale64).ceil().max(1.) as usize,
                    height: (trimmed.height * scale64).ceil().max(1.) as usize,
                },
            );
            Ok::<_, String>((template, picture, trimmed))
        });
        let pending = cx.spawn(async move |workspace, cx| {
            let result = task.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let card = result.map(|(template, picture, trimmed)| TemplateCard {
                    template: Arc::new(template),
                    origin: TemplateOrigin::PageBox {
                        document_id: document_id.value(),
                        page_index,
                        title,
                    },
                    size_pt: (trimmed.width as f32, trimmed.height as f32),
                    picture: Arc::new(picture),
                    image: None,
                });
                workspace.set_visual_template(card, cx);
            });
        });
        self.left_panel = Some(LeftPanel::Search);
        if let Some(panel) = self.search_panel.as_mut() {
            panel.mode = SearchMode::Visual;
            panel.visual.pending = Some(pending);
            panel.visual.error = None;
        }
        cx.notify();
    }

    /// Paste picture, or Cmd+V or Ctrl+V in the Visual panel: the picture on
    /// the clipboard, such as a screenshot, is the picture to find. Its
    /// pixels are taken as screen pixels at the zoom of the page in view.
    pub(super) fn paste_visual_picture(&mut self, window: &Window, cx: &mut Context<Self>) {
        let image = cx.read_from_clipboard().and_then(|item| {
            item.entries().iter().find_map(|entry| match entry {
                gpui::ClipboardEntry::Image(image) => Some(image.bytes.clone()),
                _ => None,
            })
        });
        let screen_scale = self.screen_pixels_per_point(window, cx);
        let card = match image {
            None => Err("Copy a picture first, such as a screenshot.".to_owned()),
            Some(bytes) => grey_picture(&bytes).and_then(|picture| {
                let template = VisualTemplate::from_picture(&picture, screen_scale)
                    .map_err(|error| error.to_string())?;
                let size_pt = (
                    template.width_px() as f32 / template.scale(),
                    template.height_px() as f32 / template.scale(),
                );
                Ok(TemplateCard {
                    template: Arc::new(template),
                    origin: TemplateOrigin::Pasted,
                    size_pt,
                    picture: Arc::new(TilePicture::of_image(&picture)),
                    image: None,
                })
            }),
        };
        self.left_panel = Some(LeftPanel::Search);
        if let Some(panel) = self.search_panel.as_mut() {
            panel.mode = SearchMode::Visual;
        }
        self.set_visual_template(card, cx);
    }

    /// Screen pixels for each point of the page in view: from the page as
    /// last painted, else from the zoom.
    fn screen_pixels_per_point(&self, window: &Window, cx: &App) -> f32 {
        let Some(id) = self.active_document_id else {
            return window.scale_factor();
        };
        let Some(session) = self.session(id, cx).map(|session| session.read(cx)) else {
            return window.scale_factor();
        };
        let current = session.current_page as usize;
        let painted = self.viewport_painted_layouts.get(&id).and_then(|layouts| {
            let layout = layouts
                .iter()
                .find(|layout| layout.page == current)
                .or_else(|| layouts.first())?;
            let (width, _) = session.page_size(layout.page as u32)?;
            (width > 0. && layout.device_width > 0).then(|| layout.device_width as f32 / width)
        });
        painted.unwrap_or(session.view_state.zoom_percent() / 100. * window.scale_factor())
    }

    /// Dark pages turned on or off: the template and hit pictures are drawn
    /// again with the new paper.
    pub(crate) fn redraw_visual_pictures(&mut self, cx: &mut Context<Self>) {
        let paper = self.paper_appearance;
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        let mut old = visual.take_images();
        if let Some(card) = visual.template.as_mut() {
            old.extend(card.image.take());
            card.image = tile_render_image(&card.picture, paper);
        }
        defer_drop_images(old, cx);
        cx.notify();
    }

    /// A new picture to find, or why there is none: the old search ends,
    /// and a new one starts with the picture.
    fn set_visual_template(&mut self, card: Result<TemplateCard, String>, cx: &mut Context<Self>) {
        let paper = self.paper_appearance;
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        visual.pending = None;
        match card {
            Ok(mut card) => {
                card.image = tile_render_image(&card.picture, paper);
                if let Some(old) = visual.template.replace(card)
                    && let Some(image) = old.image
                {
                    defer_drop_images(vec![image], cx);
                }
                visual.error = None;
                self.restart_visual_search(cx);
            }
            Err(error) => {
                visual.error = Some(error);
                cx.notify();
            }
        }
    }

    /// The X on the picture: no picture, no results.
    pub(super) fn clear_visual_template(&mut self, cx: &mut Context<Self>) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        let old = visual.template.take().and_then(|card| card.image);
        visual.error = None;
        defer_drop_images(old.into_iter().collect(), cx);
        self.restart_visual_search(cx);
    }

    /// The similarity slider: hits under it hide, with no new search.
    pub(super) fn set_visual_similarity(&mut self, threshold: f32, cx: &mut Context<Self>) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        let threshold = threshold.clamp(SIMILARITY_MIN / 100., SIMILARITY_MAX / 100.);
        if (visual.threshold - threshold).abs() < 1e-4 {
            return;
        }
        visual.threshold = threshold;
        visual.rebuild_rows();
        self.refresh_visual_marks(cx);
        cx.notify();
    }

    /// Sets the slider and the filter together, as a drag on it does.
    pub fn set_visual_similarity_percent(
        &mut self,
        percent: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        let slider = visual.similarity.clone();
        slider.update(cx, |slider, cx| slider.set_value(percent, window, cx));
        self.set_visual_similarity(percent / 100., cx);
    }

    fn toggle_visual_option(
        &mut self,
        change: impl FnOnce(&mut VisualPanel),
        cx: &mut Context<Self>,
    ) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        change(visual);
        self.restart_visual_search(cx);
    }

    /// Stop: the search ends; what it found stays.
    pub(super) fn stop_visual_search(&mut self, cx: &mut Context<Self>) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        if let Some(run) = visual.run.as_ref() {
            run.cancel();
        }
        visual.delay = None;
        cx.notify();
    }

    /// Ends the visual search that runs and starts one for the picture, the
    /// options and the scope.
    pub(super) fn restart_visual_search(&mut self, cx: &mut Context<Self>) {
        let scope = self
            .search_panel
            .as_ref()
            .map_or(SearchScope::AllOpenDocuments, |panel| panel.scope);
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        visual.run = None;
        visual.delay = None;
        visual.generation += 1;
        let generation = visual.generation;
        visual.results = VisualResults::default();
        visual.documents.clear();
        visual.collapsed.clear();
        visual.selected = None;
        visual.opened = None;
        visual.started = None;
        visual.first_hit = None;
        visual.finished_after = None;
        let images = visual.take_images();
        defer_drop_images(images, cx);
        let template = visual.template.as_ref().map(|card| card.template.clone());
        let source = match visual.template.as_ref().map(|card| &card.origin) {
            Some(TemplateOrigin::PageBox {
                document_id,
                page_index,
                ..
            }) => Some(TemplateSource {
                document_id: *document_id,
                page_index: *page_index,
            }),
            _ => None,
        };
        let options = visual.options();
        if self.search_marks.as_ref().is_some_and(|marks| marks.visual) {
            self.clear_search_marks_everywhere(cx);
        }
        let Some(template) = template else {
            if let Some(visual) = self.visual_panel_mut() {
                visual.rebuild_rows();
            }
            cx.notify();
            return;
        };
        let targets = self.visual_targets(scope, cx);
        let largest =
            targets
                .iter()
                .map(|target| target.largest)
                .fold((0f32, 0f32), |best, size| {
                    if size.0 * size.1 > best.0 * best.1 {
                        size
                    } else {
                        best
                    }
                });
        let short_pt = template.width_px().min(template.height_px()) as f32 / template.scale();
        let fits = |scale: f32| {
            f64::from(largest.0 * scale).ceil() * f64::from(largest.1 * scale).ceil()
                <= crate::visual_search::MAX_PAGE_PIXELS as f64
        };
        // A box's picture was cut from its page drawn at its own scale: the
        // pages are drawn at that scale too (the source page is then drawn
        // once), unless a larger page in scope needs less.
        let scale = if source.is_some() && fits(template.scale()) {
            template.scale()
        } else {
            working_scale(short_pt, largest)
        };
        let prepared = match template.prepare(scale, &options) {
            Ok(prepared) => Arc::new(prepared),
            Err(error) => {
                if let Some(visual) = self.visual_panel_mut() {
                    visual.error = Some(error.to_string());
                    visual.rebuild_rows();
                }
                cx.notify();
                return;
            }
        };
        let labels = super::document_labels(
            &targets
                .iter()
                .map(|target| (target.id, target.title.clone(), target.window_number))
                .collect::<Vec<_>>(),
        );
        let cache = Self::visual_cache(cx);
        let mut documents: Vec<Arc<dyn VisualSearchDocument>> = Vec::new();
        let mut metadata = Vec::new();
        let mut panel_documents = HashMap::new();
        for target in targets {
            let mut document = ResourceVisualDocument::new(
                target.id,
                target.resource,
                target.pages.clone(),
                cache.clone(),
            );
            if let Some(source) = source.filter(|source| source.document_id == target.id) {
                document = document.starting_at(source.page_index);
            }
            metadata.push((target.id, target.title.clone(), target.pages.len() as u32));
            documents.push(Arc::new(document));
            panel_documents.insert(
                target.id,
                VisualPanelDocument {
                    label: labels.get(&target.id).cloned().unwrap_or(target.title),
                    window: target.window,
                    workspace: target.workspace,
                    read: target.pages.iter().map(|page| page.validity).collect(),
                },
            );
        }
        let (events, receiver) = async_channel::unbounded::<VisualSearchEvent>();
        let run = VisualSearchRun::start(
            documents,
            prepared,
            source,
            matcher_threads(),
            move |event| {
                let _ = events.send_blocking(event);
            },
        );
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        visual.results = VisualResults::new(metadata);
        visual.documents = panel_documents;
        visual.run = Some(run);
        visual.started = Some(Instant::now());
        visual.rebuild_rows();
        // As the text search: events are taken on a timer, so no search
        // thread wakes the window's executor.
        cx.spawn(async move |workspace, cx| {
            loop {
                cx.background_executor().timer(VISUAL_UPDATE_INTERVAL).await;
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
                    workspace.apply_visual_events(generation, batch, cx)
                });
                if !alive.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    fn apply_visual_events(
        &mut self,
        generation: u64,
        events: Vec<VisualSearchEvent>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(visual) = self
            .visual_panel_mut()
            .filter(|visual| visual.generation == generation)
        else {
            return false;
        };
        let mut finished = false;
        for event in events {
            finished |= matches!(event, VisualSearchEvent::Finished { .. });
            visual.results.apply(event);
        }
        let elapsed = visual.started.map(|started| started.elapsed());
        if visual.first_hit.is_none()
            && visual
                .results
                .documents()
                .iter()
                .any(|document| document.hit_count(0.) > 0)
        {
            visual.first_hit = elapsed;
        }
        if finished {
            visual.finished_after = elapsed;
        }
        visual.rebuild_rows();
        cx.notify();
        !finished
    }

    /// The marks on the page of an opened hit, again, after the filter
    /// changed.
    fn refresh_visual_marks(&mut self, cx: &mut Context<Self>) {
        let Some(key) = self.visual_panel_mut().and_then(|visual| visual.opened) else {
            return;
        };
        let shown = self
            .visual_panel_mut()
            .is_some_and(|visual| visual.results.shown_keys(visual.threshold).contains(&key));
        if shown {
            if let Some(marks) = self.visual_marks(key)
                && self
                    .search_marks
                    .as_ref()
                    .is_some_and(|current| current.visual)
            {
                self.search_marks = Some(marks);
            }
        } else {
            if let Some(visual) = self.visual_panel_mut() {
                visual.opened = None;
            }
            self.clear_search_marks_everywhere(cx);
        }
    }

    /// The marks of the document of hit `key`: its shown hits, `key` the
    /// current one.
    fn visual_marks(&self, key: VisualHitKey) -> Option<SearchMarks> {
        let visual = &self.search_panel.as_ref()?.visual;
        let (document_id, page, _) = key;
        let mut pages: BTreeMap<u32, Vec<SearchHit>> = BTreeMap::new();
        let mut current = None;
        for shown in visual.results.shown_keys(visual.threshold) {
            if shown.0 != document_id {
                continue;
            }
            let hit = visual.results.hit(shown)?;
            let list = pages.entry(shown.1).or_default();
            if shown == key {
                current = Some((page, list.len()));
            }
            list.push(SearchHit {
                document_id,
                page_index: shown.1,
                target: HitTarget::Visual {
                    score: hit.hit.score,
                    source: hit.hit.source,
                },
                snippet: Default::default(),
                quads: Vec::new(),
                bounds: hit_bounds(&hit.hit),
            });
        }
        Some(SearchMarks {
            document_id: DocumentId::new(document_id),
            pages,
            current,
            visual: true,
        })
    }

    fn select_visual_item(&mut self, item: VisualItem, cx: &mut Context<Self>) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        visual.selected = Some(item);
        visual.rebuild_rows();
        if let Some(row) = visual.row_of(item) {
            visual.list.scroll_to_reveal_item(row);
        }
        cx.notify();
    }

    /// Up and Down: the next or previous document, page or hit.
    pub(super) fn step_visual_selection(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        let items = visual.items();
        if items.is_empty() {
            return;
        }
        let next = match visual
            .selected
            .and_then(|item| items.iter().position(|other| *other == item))
        {
            Some(index) if forward => (index + 1).min(items.len() - 1),
            Some(index) => index.saturating_sub(1),
            None => 0,
        };
        self.select_visual_item(items[next], cx);
    }

    /// Left: a group collapses, or a hit's page is selected. Right: a group
    /// expands.
    pub(super) fn collapse_visual_item(&mut self, collapse: bool, cx: &mut Context<Self>) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        let Some(item) = visual.selected else {
            return;
        };
        match item {
            VisualItem::Hit((id, page, _)) => {
                if collapse {
                    self.select_visual_item(VisualItem::Page(id, page), cx);
                }
            }
            group => {
                let collapsed = visual.collapsed.contains(&group);
                if collapsed != collapse {
                    self.toggle_visual_group(group, cx);
                } else if collapse && let VisualItem::Page(id, _) = group {
                    self.select_visual_item(VisualItem::Document(id), cx);
                }
            }
        }
    }

    fn toggle_visual_group(&mut self, item: VisualItem, cx: &mut Context<Self>) {
        let Some(visual) = self.visual_panel_mut() else {
            return;
        };
        if !visual.collapsed.remove(&item) {
            visual.collapsed.insert(item);
        }
        visual.rebuild_rows();
        cx.notify();
    }

    /// Enter or Space: a group collapses or expands; a hit opens.
    pub(super) fn open_selected_visual_item(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.visual_panel_mut().and_then(|visual| visual.selected) else {
            return;
        };
        match item {
            VisualItem::Hit(key) => self.open_visual_hit(key, window, cx),
            group => self.toggle_visual_group(group, cx),
        }
    }

    /// Goes to a hit: its window and tab come to the front, the view goes
    /// to it, and its document's shown hits are outlined, this one stronger.
    pub fn open_visual_hit(
        &mut self,
        key: VisualHitKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(marks) = self.visual_marks(key) else {
            return;
        };
        let Some(panel) = self.search_panel.as_mut() else {
            return;
        };
        let focus = panel.results_focus.clone();
        let visual = &mut panel.visual;
        let Some(hit) = visual.results.hit(key) else {
            return;
        };
        let search_hit = SearchHit {
            document_id: key.0,
            page_index: key.1,
            target: HitTarget::Visual {
                score: hit.hit.score,
                source: hit.hit.source,
            },
            snippet: Default::default(),
            quads: Vec::new(),
            bounds: hit_bounds(&hit.hit),
        };
        let Some(document) = visual.documents.get(&key.0) else {
            return;
        };
        let (other_window, target) = (document.window, document.workspace.clone());
        visual.selected = Some(VisualItem::Hit(key));
        visual.opened = Some(key);
        visual.rebuild_rows();
        focus.focus(window, cx);
        match (other_window, target.upgrade()) {
            (Some(handle), Some(workspace)) => {
                let _ = handle.update(cx, |_, window, _| window.activate_window());
                workspace.update(cx, |workspace, cx| {
                    workspace.show_search_hit(marks, &search_hit, cx)
                });
                if self.search_marks.take().is_some() {
                    cx.notify();
                }
            }
            (None, Some(_)) => {
                self.clear_search_marks_everywhere(cx);
                self.show_search_hit(marks, &search_hit, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    /// Opens the hit at `index` of the shown hits, as a click on its picture
    /// does.
    pub fn open_shown_visual_hit(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = self.visual_panel_mut().and_then(|visual| {
            visual
                .results
                .shown_keys(visual.threshold)
                .get(index)
                .copied()
        });
        if let Some(key) = key {
            self.open_visual_hit(key, window, cx);
        }
    }

    /// A document of this window closed: its pages are no longer searched,
    /// and its hits go.
    pub(super) fn forget_visual_document(&mut self, document_id: DocumentId, cx: &mut App) {
        Self::visual_cache(cx)
            .lock()
            .remove_document(document_id.value());
        let Some(visual) = self.search_panel.as_mut().map(|panel| &mut panel.visual) else {
            return;
        };
        if let Some(run) = visual.run.as_ref() {
            run.cancel_document(document_id.value());
        }
        if visual.documents.remove(&document_id.value()).is_some() {
            visual.results.remove_document(document_id.value());
            visual.selected = None;
            if visual
                .opened
                .is_some_and(|(id, _, _)| id == document_id.value())
            {
                visual.opened = None;
            }
            let images = {
                let mut images = visual.images.borrow_mut();
                let keys = images
                    .keys()
                    .filter(|(id, _, _)| *id == document_id.value())
                    .copied()
                    .collect::<Vec<_>>();
                keys.into_iter()
                    .filter_map(|key| images.remove(&key))
                    .collect::<Vec<_>>()
            };
            defer_drop_images(images, cx);
            visual.rebuild_rows();
        }
        if let Some(VisualPanel {
            template: Some(card),
            ..
        }) = self.search_panel.as_ref().map(|panel| &panel.visual)
            && let TemplateOrigin::PageBox {
                document_id: id, ..
            } = card.origin
            && id == document_id.value()
        {
            // The picture stays: it no longer needs its page.
            if let Some(visual) = self.visual_panel_mut()
                && let Some(card) = visual.template.as_mut()
            {
                card.origin = TemplateOrigin::Pasted;
            }
        }
    }

    /// Runs the visual search again when a searched document's pages
    /// changed under it, or the documents in scope changed.
    pub(super) fn poll_visual_stale(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.search_panel.as_ref() else {
            return;
        };
        let visual = &panel.visual;
        if visual.template.is_none() || visual.delay.is_some() || visual.run.is_none() {
            return;
        }
        if visual
            .run
            .as_ref()
            .is_some_and(VisualSearchRun::is_cancelled)
        {
            // Stopped by the user: it stays stopped.
            return;
        }
        let own = cx.entity().downgrade();
        let changed = visual.documents.iter().any(|(id, document)| {
            let now = if document.workspace == own {
                self.visual_page_states(DocumentId::new(*id), cx)
            } else {
                document.workspace.upgrade().and_then(|workspace| {
                    workspace
                        .read(cx)
                        .visual_page_states(DocumentId::new(*id), cx)
                })
            };
            now.map(|pages| pages.iter().map(|page| page.validity).collect::<Vec<_>>())
                .as_ref()
                != Some(&document.read)
        });
        let searched = visual.documents.keys().copied().collect::<HashSet<_>>();
        let in_scope = self.ready_documents_in_scope(&own, panel.scope, cx);
        if changed || searched != in_scope {
            let Some(visual) = self.visual_panel_mut() else {
                return;
            };
            visual.delay = Some(cx.spawn(async move |workspace, cx| {
                cx.background_executor().timer(VISUAL_EDIT_DELAY).await;
                let _ = workspace.update(cx, |workspace, cx| workspace.restart_visual_search(cx));
            }));
        }
    }

    /// The panel's Visual body: the picture, the options, the progress and
    /// the results.
    pub(super) fn render_visual_search_body(
        &self,
        panel: &SearchPanel,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let visual = &panel.visual;
        let summary = self.visual_panel_summary(cx);
        let theme = cx.theme();
        let card = visual.template.as_ref();
        let picture_box = gpui::div()
            .flex_none()
            .size(px(56.))
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            // A picture of the page: its own paper, whatever the theme.
            .bg(gpui::white())
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .when_some(card.and_then(|card| card.image.clone()), |frame, image| {
                frame.child(
                    gpui::img(image)
                        .max_w(px(52.))
                        .max_h(px(52.))
                        .object_fit(gpui::ObjectFit::Contain),
                )
            })
            .when(card.is_none(), |frame| {
                frame.bg(theme.muted).child(
                    Icon::new(IconName::Search)
                        .small()
                        .text_color(theme.muted_foreground),
                )
            });
        let template_lines = summary
            .as_ref()
            .map(|summary| summary.template_text.clone())
            .unwrap_or_default();
        let draw_box = Button::new(DOCUMENT_VISUAL_DRAW_BOX_ID)
            .debug_selector(|| DOCUMENT_VISUAL_DRAW_BOX_ID.into())
            .accessibility_id(DOCUMENT_VISUAL_DRAW_BOX_ID)
            .small()
            .label("Draw box")
            .tooltip("Drag a box around what to find on the page")
            .loading(visual.pending.is_some())
            .on_click(cx.listener(|workspace, _: &ClickEvent, window, cx| {
                workspace.begin_visual_search_box(window, cx)
            }));
        let paste = Button::new(DOCUMENT_VISUAL_PASTE_ID)
            .debug_selector(|| DOCUMENT_VISUAL_PASTE_ID.into())
            .accessibility_id(DOCUMENT_VISUAL_PASTE_ID)
            .small()
            .label("Paste picture")
            .tooltip("Find a picture from the clipboard, such as a screenshot")
            .on_click(cx.listener(|workspace, _: &ClickEvent, window, cx| {
                workspace.paste_visual_picture(window, cx)
            }));
        let template_card = h_flex()
            .id(DOCUMENT_VISUAL_TEMPLATE_ID)
            .debug_selector(|| DOCUMENT_VISUAL_TEMPLATE_ID.into())
            .relative()
            .w_full()
            .gap_2()
            .items_start()
            .child(picture_box)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .children(template_lines.into_iter().enumerate().map(|(index, line)| {
                        gpui::div()
                            .min_w_0()
                            // The hint with no picture wraps; a name
                            // shortens.
                            .when(card.is_some(), |line| line.truncate())
                            .text_sm()
                            .when(index > 0 || card.is_none(), |line| {
                                line.text_color(theme.muted_foreground)
                            })
                            .when(card.is_some(), |line| line.pr_6())
                            .child(line)
                    }))
                    .child(h_flex().gap_1().flex_wrap().child(draw_box).child(paste)),
            )
            .when(card.is_some(), |row| {
                row.child(
                    gpui::div().absolute().top_0().right_0().child(
                        crate::accessible_button::close_icon_button(
                            DOCUMENT_VISUAL_CLEAR_ID,
                            "Clear the picture",
                        )
                        .on_click(cx.listener(
                            |workspace, _: &ClickEvent, _, cx| workspace.clear_visual_template(cx),
                        )),
                    ),
                )
            });
        let pasted = matches!(card.map(|card| &card.origin), Some(TemplateOrigin::Pasted));
        let checkbox = |id: &'static str, label: &'static str, on: bool, cx: &mut Context<Self>| {
            // The panel's other controls are small: so are these.
            gpui_component::checkbox::Checkbox::new(id)
                .debug_selector(move || id.into())
                .small()
                .label(label)
                .checked(on)
                .on_click(cx.listener(move |workspace, checked: &bool, _, cx| {
                    let checked = *checked;
                    workspace.toggle_visual_option(
                        |visual| match id {
                            DOCUMENT_VISUAL_TURNS_ID => visual.turns = checked,
                            DOCUMENT_VISUAL_SIZES_ID => visual.sizes = checked,
                            _ => visual.mirrored = checked,
                        },
                        cx,
                    )
                }))
        };
        let turns = checkbox(
            DOCUMENT_VISUAL_TURNS_ID,
            "Turned 90°, 180°, 270°",
            visual.turns,
            cx,
        );
        let sizes = checkbox(
            DOCUMENT_VISUAL_SIZES_ID,
            if pasted {
                "Sizes 50 % to 200 %"
            } else {
                "Sizes 85 % to 118 %"
            },
            visual.sizes,
            cx,
        );
        let mirrored = checkbox(
            DOCUMENT_VISUAL_MIRRORED_ID,
            "Include mirrored",
            visual.mirrored,
            cx,
        );
        let theme = cx.theme();
        let similarity = h_flex()
            .w_full()
            .gap_2()
            .items_center()
            .child(gpui::div().flex_none().text_sm().child("Similarity"))
            .child(
                gpui::div()
                    .id(DOCUMENT_VISUAL_SIMILARITY_ID)
                    .debug_selector(|| DOCUMENT_VISUAL_SIMILARITY_ID.into())
                    .flex_1()
                    .min_w_0()
                    .child(Slider::new(&visual.similarity)),
            )
            .child(
                gpui::div()
                    .flex_none()
                    .w(px(40.))
                    .text_right()
                    .text_sm()
                    .child(format!("{} %", percent(visual.threshold))),
            );
        let progress_text = summary
            .as_ref()
            .map(|summary| summary.progress.clone())
            .unwrap_or_default();
        let (done, all) = visual.results.progress();
        let running = visual.running();
        let progress_empty = progress_text.is_empty();
        let progress = v_flex()
            .id(DOCUMENT_VISUAL_PROGRESS_ID)
            .debug_selector(|| DOCUMENT_VISUAL_PROGRESS_ID.into())
            .w_full()
            .gap_1()
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .items_center()
                    .child(
                        gpui::div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(progress_text),
                    )
                    .when(running, |row| {
                        row.child(
                            Button::new(DOCUMENT_VISUAL_STOP_ID)
                                .debug_selector(|| DOCUMENT_VISUAL_STOP_ID.into())
                                .accessibility_id(DOCUMENT_VISUAL_STOP_ID)
                                .xsmall()
                                .label("Stop")
                                .on_click(cx.listener(|workspace, _: &ClickEvent, _, cx| {
                                    workspace.stop_visual_search(cx)
                                })),
                        )
                    }),
            )
            .when(running, |column| {
                column.child(
                    gpui_component::progress::Progress::new("document-visual-progress-bar").value(
                        if all == 0 {
                            0.
                        } else {
                            done as f32 * 100. / all as f32
                        },
                    ),
                )
            });
        let summary_text = summary
            .as_ref()
            .map(|summary| summary.summary.clone())
            .unwrap_or_default();
        let error = visual.error.clone();
        v_flex()
            .flex_1()
            .min_h_0()
            .child(
                v_flex()
                    .flex_none()
                    .gap_2()
                    .px_2()
                    .pb_2()
                    .child(template_card)
                    .when_some(error, |column, error| {
                        column.child(
                            gpui::div()
                                .id(DOCUMENT_VISUAL_ERROR_ID)
                                .debug_selector(|| DOCUMENT_VISUAL_ERROR_ID.into())
                                .text_sm()
                                .text_color(theme.danger)
                                .child(error),
                        )
                    })
                    .child(similarity)
                    .child(v_flex().gap_1().child(turns).child(sizes).child(mirrored))
                    .when(running || !progress_empty, |column| column.child(progress)),
            )
            .child(
                gpui::div()
                    .id(DOCUMENT_VISUAL_SUMMARY_ID)
                    .debug_selector(|| DOCUMENT_VISUAL_SUMMARY_ID.into())
                    .flex_none()
                    .px_2()
                    .pb_1()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .when(summary_text.is_empty(), |summary| summary.hidden())
                    .child(summary_text),
            )
            .child(
                gpui::div()
                    .flex_1()
                    .min_h_0()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(self.render_visual_rows(panel, cx)),
            )
            .into_any_element()
    }

    fn render_visual_rows(&self, panel: &SearchPanel, cx: &mut Context<Self>) -> gpui::AnyElement {
        let visual = &panel.visual;
        let rows = visual.rows.clone();
        let images = visual.images.clone();
        let paper = self.paper_appearance;
        let focused_list = panel.results_focus.clone();
        let owner = cx.entity().downgrade();
        let selected = visual.selected;
        let results = Rc::new(visual.results.clone());
        let threshold = visual.threshold;
        let shown = Rc::new(results.shown_keys(threshold));
        let pages: Rc<HashMap<(u64, u32), Vec<VisualHitKey>>> = Rc::new(shown.iter().fold(
            HashMap::new(),
            |mut pages: HashMap<_, Vec<_>>, key| {
                pages.entry((key.0, key.1)).or_default().push(*key);
                pages
            },
        ));
        gpui::div()
            .id(DOCUMENT_VISUAL_RESULTS_ID)
            .debug_selector(|| DOCUMENT_VISUAL_RESULTS_ID.into())
            .key_context(super::SEARCH_RESULTS_CONTEXT)
            .track_focus(&panel.results_focus)
            .size_full()
            .relative()
            .on_action(cx.listener(|workspace, _: &SearchSelectPrevious, _, cx| {
                workspace.step_visual_selection(false, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchSelectNext, _, cx| {
                workspace.step_visual_selection(true, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchCollapseRow, _, cx| {
                workspace.collapse_visual_item(true, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchExpandRow, _, cx| {
                workspace.collapse_visual_item(false, cx)
            }))
            .on_action(cx.listener(|workspace, _: &SearchOpenRow, window, cx| {
                workspace.open_selected_visual_item(window, cx)
            }))
            .child(
                gpui::list(visual.list.clone(), move |index, window, cx| {
                    let Some((key, row)) = rows.get(index).cloned() else {
                        return gpui::div().into_any_element();
                    };
                    let list_focused = focused_list.is_focused(window);
                    visual_row_element(
                        index,
                        key,
                        row,
                        VisualRowContext {
                            paper,
                            selected,
                            list_focused,
                            images: images.clone(),
                            results: results.clone(),
                            shown: shown.clone(),
                            pages: pages.clone(),
                        },
                        owner.clone(),
                        cx,
                    )
                })
                .size_full(),
            )
            .vertical_scrollbar(&visual.list)
            .into_any_element()
    }

    /// What the Visual mode shows now; `None` before the panel was opened.
    pub fn visual_panel_summary(&self, _cx: &App) -> Option<VisualPanelSummary> {
        let panel = self.search_panel.as_ref()?;
        let visual = &panel.visual;
        let threshold = visual.threshold;
        let shown_keys = visual.results.shown_keys(threshold);
        let shown = shown_keys
            .iter()
            .filter_map(|key| {
                let hit = visual.results.hit(*key)?;
                Some(tile_view(
                    hit,
                    visual.selected == Some(VisualItem::Hit(*key)),
                    visual.opened == Some(*key),
                ))
            })
            .collect::<Vec<_>>();
        let template_text = match visual.template.as_ref() {
            None => {
                vec!["Draw a box around a symbol on a page, or paste a picture of it.".to_owned()]
            }
            Some(card) => vec![
                match &card.origin {
                    TemplateOrigin::PageBox {
                        title, page_index, ..
                    } => format!("{title}, page {}", page_index + 1),
                    TemplateOrigin::Pasted => "Pasted picture".to_owned(),
                },
                format!(
                    "{} × {} pt",
                    card.size_pt.0.round() as i64,
                    card.size_pt.1.round() as i64
                ),
            ],
        };
        let running = visual.running();
        let (done, all) = visual.results.progress();
        let hits = shown.len();
        Some(VisualPanelSummary {
            mode: panel.mode,
            template: visual.template.as_ref().map(|card| card.origin.clone()),
            template_text,
            error: visual.error.clone(),
            similarity: percent(threshold),
            turns: visual.turns,
            sizes: visual.sizes,
            mirrored: visual.mirrored,
            running,
            finished: visual.results.is_finished(),
            cancelled: visual.results.was_cancelled(),
            progress: if visual.run.is_some() {
                visual_progress_text(running, visual.results.was_cancelled(), done, all)
            } else {
                String::new()
            },
            summary: if visual.run.is_some() {
                visual_summary_text(
                    visual.results.is_finished(),
                    hits,
                    visual.results.page_count_with_hits(threshold),
                    visual.results.document_count_with_hits(threshold),
                )
            } else {
                String::new()
            },
            hits,
            rows: visual.rows.iter().map(|(_, row)| row.clone()).collect(),
            opened: visual
                .opened
                .and_then(|key| shown_keys.iter().position(|other| *other == key)),
            selected_hit: match visual.selected {
                Some(VisualItem::Hit(key)) => shown_keys.iter().position(|other| *other == key),
                _ => None,
            },
            shown,
            pages: (done, all),
        })
    }

    /// When the visual search found its first hit and when it ended, after
    /// its start.
    pub fn visual_search_timing(&self) -> (Option<Duration>, Option<Duration>) {
        self.search_panel.as_ref().map_or((None, None), |panel| {
            (panel.visual.first_hit, panel.visual.finished_after)
        })
    }
}

/// A clipboard picture as grey pixels, transparent parts as paper.
fn grey_picture(bytes: &[u8]) -> Result<GreyImage, String> {
    let encoded = crate::image_asset_decode::clipboard_picture_png_or_jpeg(bytes)
        .map_err(|_| "Copy a picture first, such as a screenshot.".to_owned())?;
    let decoded = image::load_from_memory(&encoded)
        .map_err(|_| "The picture on the clipboard cannot be read.".to_owned())?
        .into_rgba8();
    let (width, height) = (decoded.width() as usize, decoded.height() as usize);
    let mut bgra = Vec::with_capacity(width * height * 4);
    for pixel in decoded.pixels() {
        let [r, g, b, a] = pixel.0;
        let premultiply = |channel: u8| ((u16::from(channel) * u16::from(a) + 127) / 255) as u8;
        bgra.extend_from_slice(&[premultiply(b), premultiply(g), premultiply(r), a]);
    }
    GreyImage::from_bgra(width, height, width * 4, &bgra).map_err(|error| error.to_string())
}

/// A hit or template picture as the list shows it: with dark pages on, its
/// greys go through the dark paper map, as the page around it does. The
/// search itself matches the real colours.
fn tile_render_image(picture: &TilePicture, paper: PaperAppearance) -> Option<Arc<RenderImage>> {
    let bgra = match paper {
        PaperAppearance::Real => picture.to_bgra(),
        PaperAppearance::Dark => {
            let map = dark_grey_map();
            picture
                .pixels
                .iter()
                .flat_map(|&grey| {
                    let [r, g, b] = map[usize::from(grey)];
                    [b, g, r, 255]
                })
                .collect()
        }
    };
    let pixels = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
        picture.width as u32,
        picture.height as u32,
        bgra,
    )?;
    Some(Arc::new(RenderImage::new(smallvec::smallvec![
        image::Frame::new(pixels)
    ])))
}

/// The dark paper map of each grey, 0 (black) to 255 (paper), in RGB.
fn dark_grey_map() -> &'static [[u8; 3]; 256] {
    static MAP: std::sync::OnceLock<[[u8; 3]; 256]> = std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        std::array::from_fn(|grey| {
            let grey = grey as u8;
            crate::paper_appearance::DarkPaper::DEFAULT.map_colour([grey, grey, grey])
        })
    })
}

struct VisualRowContext {
    paper: PaperAppearance,
    selected: Option<VisualItem>,
    list_focused: bool,
    images: Rc<RefCell<HashMap<VisualHitKey, Arc<RenderImage>>>>,
    results: Rc<VisualResults>,
    shown: Rc<Vec<VisualHitKey>>,
    pages: Rc<HashMap<(u64, u32), Vec<VisualHitKey>>>,
}

fn visual_row_element(
    index: usize,
    key: VisualRowKey,
    row: VisualPanelRow,
    context: VisualRowContext,
    owner: WeakEntity<DocumentWorkspace>,
    cx: &mut App,
) -> gpui::AnyElement {
    let theme = cx.theme();
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
            .child(count.to_string())
    };
    let group_item = match key {
        VisualRowKey::Document(id) => Some(VisualItem::Document(id)),
        VisualRowKey::Page(id, page) => Some(VisualItem::Page(id, page)),
        VisualRowKey::Tiles(..) => None,
    };
    let selected_group = group_item.is_some() && context.selected == group_item;
    let content: gpui::AnyElement = match row {
        VisualPanelRow::Document {
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
        VisualPanelRow::Page {
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
        VisualPanelRow::Tiles(tiles) => {
            let VisualRowKey::Tiles(id, page, first) = key else {
                return gpui::div().into_any_element();
            };
            let keys = context
                .pages
                .get(&(id, page))
                .map(|keys| {
                    keys.iter()
                        .skip(first)
                        .take(TILES_PER_ROW)
                        .copied()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut row = h_flex().w_full().gap_1();
            for (tile, hit_key) in tiles.into_iter().zip(keys) {
                let place = context
                    .shown
                    .iter()
                    .position(|other| *other == hit_key)
                    .unwrap_or_default();
                let image = {
                    let mut images = context.images.borrow_mut();
                    match images.get(&hit_key) {
                        Some(image) => Some(image.clone()),
                        None => context
                            .results
                            .hit(hit_key)
                            .and_then(|hit| tile_render_image(&hit.tile, context.paper))
                            .inspect(|image| {
                                images.insert(hit_key, image.clone());
                            }),
                    }
                };
                let focused = tile.selected && context.list_focused;
                let tooltip: SharedString = tile_tooltip(&tile).into();
                let click_owner = owner.clone();
                row = row.child(
                    v_flex()
                        .id(gpui::ElementId::Name(visual_tile_id(place).into()))
                        .debug_selector(move || visual_tile_id(place))
                        .flex_1()
                        .min_w_0()
                        .max_w(px(88.))
                        .gap_0p5()
                        .p_0p5()
                        .rounded(theme.radius)
                        .border_1()
                        .border_color(if focused {
                            theme.list_active_border
                        } else if tile.opened {
                            theme.ring
                        } else {
                            theme.transparent
                        })
                        .when(tile.opened || tile.selected, |tile| {
                            tile.bg(theme.list_even)
                        })
                        .hover(|tile| tile.bg(theme.list_hover))
                        .cursor_pointer()
                        .tooltip(move |window, cx| {
                            gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                        .child(
                            gpui::div()
                                .w_full()
                                .h(px(56.))
                                .rounded(theme.radius)
                                .border_1()
                                .border_color(theme.border)
                                // A picture of the page in its real colours.
                                .bg(gpui::white())
                                .overflow_hidden()
                                .flex()
                                .items_center()
                                .justify_center()
                                .when_some(image, |frame, image| {
                                    frame.child(
                                        gpui::img(image)
                                            .size_full()
                                            .object_fit(gpui::ObjectFit::Contain),
                                    )
                                }),
                        )
                        .child(
                            gpui::div()
                                .w_full()
                                .text_center()
                                .text_xs()
                                .text_color(if tile.source {
                                    theme.muted_foreground
                                } else {
                                    theme.foreground
                                })
                                .child(if tile.source {
                                    "Source".to_owned()
                                } else {
                                    format!("{} %", tile.score)
                                }),
                        )
                        .on_click(move |_, window, cx| {
                            let _ = click_owner.update(cx, |workspace, cx| {
                                workspace.open_visual_hit(hit_key, window, cx)
                            });
                        }),
                );
            }
            row.into_any_element()
        }
    };
    let background = if selected_group && context.list_focused {
        theme.list_active
    } else if selected_group {
        theme.list_even
    } else {
        theme.transparent
    };
    let indent = match key {
        VisualRowKey::Document(_) => 0.,
        VisualRowKey::Page(..) => 12.,
        VisualRowKey::Tiles(..) => 20.,
    };
    let click_owner = owner.clone();
    gpui::div()
        .id(gpui::ElementId::Name(visual_row_id(index).into()))
        .debug_selector(move || visual_row_id(index))
        .w_full()
        .py_1()
        // Clear of the overlay scroll bar.
        .pr_4()
        .pl(px(8. + indent))
        .bg(background)
        .when(selected_group && context.list_focused, |row| {
            row.border_l_2().border_color(theme.list_active_border)
        })
        .when(group_item.is_some(), |row| {
            row.hover(|row| row.bg(theme.list_hover)).cursor_pointer()
        })
        .child(content)
        .when_some(group_item, |row, item| {
            row.on_click(move |_, _, cx| {
                let _ =
                    click_owner.update(cx, |workspace, cx| workspace.toggle_visual_group(item, cx));
            })
        })
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With dark pages on, a hit picture's paper and ink take the dark
    /// paper colours; in real colours they stay as they are.
    #[test]
    fn hit_pictures_follow_dark_pages() {
        let picture = TilePicture {
            width: 3,
            height: 1,
            pixels: vec![255, 0, 128],
        };
        let real = tile_render_image(&picture, PaperAppearance::Real).unwrap();
        assert_eq!(
            real.as_bytes(0).unwrap(),
            [255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255]
        );
        let dark = tile_render_image(&picture, PaperAppearance::Dark).unwrap();
        let bytes = dark.as_bytes(0).unwrap();
        let colours = crate::paper_appearance::DarkPaper::DEFAULT;
        let bgr = |[r, g, b]: [u8; 3]| [b, g, r];
        assert_eq!(bytes[0..3], bgr(colours.paper), "paper");
        assert_eq!(bytes[4..7], bgr(colours.ink), "ink");
        assert_eq!(
            bytes[8..11],
            bgr(colours.map_colour([128, 128, 128])),
            "mid grey"
        );
    }

    #[test]
    fn the_summary_and_progress_read_as_the_search_goes() {
        assert_eq!(visual_summary_text(false, 0, 0, 0), "Searching…");
        assert_eq!(visual_summary_text(true, 0, 0, 0), "No matches");
        assert_eq!(
            visual_summary_text(true, 23, 9, 2),
            "23 matches on 9 pages in 2 documents"
        );
        assert_eq!(
            visual_summary_text(false, 1, 1, 1),
            "1 match on 1 page in 1 document"
        );
        assert_eq!(
            visual_progress_text(true, false, 33, 100),
            "Searching page 34 of 100…"
        );
        assert_eq!(
            visual_progress_text(false, true, 4, 20),
            "Stopped after 4 of 20 pages"
        );
        assert_eq!(visual_progress_text(false, false, 20, 20), "");
    }

    #[test]
    fn a_tile_tooltip_says_how_the_hit_lies() {
        let tile = VisualTileView {
            score: 91,
            source: false,
            rotation: 90,
            mirrored: true,
            size: 106,
            bounds: None,
            selected: false,
            opened: false,
        };
        assert_eq!(
            tile_tooltip(&tile),
            "91 % similar, turned 90°, mirrored, 106 % size"
        );
        assert_eq!(
            tile_tooltip(&VisualTileView {
                source: true,
                ..tile
            }),
            "The box the picture came from"
        );
    }
}
