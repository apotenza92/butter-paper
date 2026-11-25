//! Single Page view's page memory, kept for each file after its tab closes.
//!
//! A tab remembers each page's zoom and scroll (`native_document_view_state`).
//! When the tab closes, that memory moves here, for the last
//! [`MAX_REMEMBERED_FILES`] files, least recently used first out, and returns
//! when the same file opens again. It lives in the application's own data,
//! never in the PDF.
//!
//! A file is known by its canonical path. A file that moved within its
//! volume is found again by file identity (`file_locator`: a bookmark on
//! macOS, the file ID on Windows; Linux has none). The session manifest codec
//! writes the memory, so paths survive any spelling and a write is atomic.
use crate::{
    native_document_view_state::{RememberedPageView, RestartView, RestartZoom},
    page_view_control::PageViewMode,
    session_manifest::{SessionManifestStore, SessionSnapshot, encode_path, normalized_path_key},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use gpui::{Context, Task};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

/// The most files whose pages are remembered. The least recently used is
/// forgotten first.
pub const MAX_REMEMBERED_FILES: usize = 50;
const LOCATORS_NAME: &str = "page-memory-locators.json";

/// One page's remembered view: its zero-based page index and its view.
pub type PageViews = Vec<(u32, RememberedPageView)>;

struct Entry {
    path: PathBuf,
    /// The page that was shown when the tab closed.
    current_page: u32,
    page_views: PageViews,
}

#[derive(Default)]
pub struct PageMemory {
    /// The least recently used file first.
    entries: Vec<Entry>,
    /// File identities by `locator_key`; a hint, checked when resolved.
    locators: HashMap<String, Vec<u8>>,
    root: Option<PathBuf>,
    store: Option<Arc<SessionManifestStore>>,
    publication: Option<Task<()>>,
}

impl PageMemory {
    /// The memory kept in `root`, made if it is missing. A damaged file only
    /// means that nothing is remembered.
    pub fn open(root: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let store = Arc::new(
            SessionManifestStore::open(root.clone()).map_err(|error| format!("{error:?}"))?,
        );
        let entries = match store.load() {
            Ok(plan) => plan
                .into_documents()
                .0
                .into_iter()
                .map(|document| Entry {
                    current_page: document.view().current_page(),
                    page_views: document.view().page_views().to_vec(),
                    path: document.into_path(),
                })
                .filter(|entry| !entry.page_views.is_empty())
                .collect::<Vec<_>>(),
            Err(error) => {
                eprintln!("Page memory could not be loaded: {error:?}");
                Vec::new()
            }
        };
        let mut memory = Self {
            entries,
            locators: load_locators(&root),
            root: Some(root),
            store: Some(store),
            publication: None,
        };
        memory.trim();
        Ok(memory)
    }

    /// Wait for a pending write during application shutdown.
    pub fn take_pending_publication(&mut self) -> Option<Task<()>> {
        self.publication.take()
    }

    /// The files remembered, the least recently used first.
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.entries.iter().map(|entry| entry.path.as_path())
    }

    /// The pages remembered for the file at `path`: by path, or, for a file
    /// that moved since, by file identity. Empty when there are none.
    pub fn lookup(&self, path: &Path) -> PageViews {
        self.position(path)
            .map(|index| self.entries[index].page_views.clone())
            .unwrap_or_default()
    }

    /// The view that reopens the file at `path` where its last tab left it:
    /// the page shown then, in Single Page view, with that page's zoom and
    /// scroll and the other pages remembered. `None` when that page has no
    /// remembered view (it was left in Continuous view), so the file opens
    /// as usual with its pages remembered.
    pub fn restart_view(&self, path: &Path) -> Option<RestartView> {
        let entry = &self.entries[self.position(path)?];
        let (_, shown) = entry
            .page_views
            .iter()
            .find(|(page, _)| *page == entry.current_page)?;
        let (scroll_x, scroll_y) = shown.scroll();
        Some(
            RestartView::new(
                entry.current_page,
                PageViewMode::SinglePage,
                shown.zoom(),
                scroll_x,
                scroll_y,
            )
            .with_page_views(
                entry
                    .page_views
                    .iter()
                    .filter(|(page, _)| *page != entry.current_page)
                    .copied()
                    .collect(),
            ),
        )
    }

    /// Remembers `page_views` for the file at `path` as its latest use, with
    /// the page shown. No pages forgets the file. A file found by identity
    /// at a new path takes that path.
    pub fn remember(
        &mut self,
        path: PathBuf,
        current_page: u32,
        page_views: PageViews,
        cx: &mut Context<Self>,
    ) {
        if self.record_at(path, current_page, page_views) {
            cx.notify();
            self.publish(cx);
        }
    }

    /// The memory change of [`Self::remember`], without the write. `true`
    /// when something changed.
    #[cfg(test)]
    fn record(&mut self, path: PathBuf, page_views: PageViews) -> bool {
        self.record_at(path, 0, page_views)
    }

    fn record_at(&mut self, path: PathBuf, current_page: u32, page_views: PageViews) -> bool {
        let existing = self.position(&path);
        if let Some(index) = existing
            && index + 1 == self.entries.len()
            && self.entries[index].path == path
            && self.entries[index].current_page == current_page
            && self.entries[index].page_views == page_views
            && !page_views.is_empty()
        {
            return false;
        }
        if let Some(index) = existing {
            let old = self.entries.remove(index);
            if old.path != path {
                // The identity follows the file to its new path.
                if let Some(locator) = self.locators.remove(&locator_key(&old.path)) {
                    self.locators.insert(locator_key(&path), locator);
                }
            }
        }
        if page_views.is_empty() {
            self.locators.remove(&locator_key(&path));
            return existing.is_some();
        }
        self.entries.push(Entry {
            path,
            current_page,
            page_views,
        });
        self.trim();
        true
    }

    /// The entry for the file at `path`: the one with its path, else the one
    /// whose identity now resolves to it.
    fn position(&self, path: &Path) -> Option<usize> {
        let key = normalized_path_key(path);
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| normalized_path_key(&entry.path) == key)
        {
            return Some(index);
        }
        // Only a file that is gone from its old path can have moved.
        self.entries.iter().position(|entry| {
            self.locators
                .get(&locator_key(&entry.path))
                .is_some_and(|locator| {
                    !entry.path.exists()
                        && crate::file_locator::resolve(locator)
                            .is_some_and(|found| normalized_path_key(&found) == key)
                })
        })
    }

    fn trim(&mut self) {
        let excess = self.entries.len().saturating_sub(MAX_REMEMBERED_FILES);
        for entry in self.entries.drain(..excess) {
            self.locators.remove(&locator_key(&entry.path));
        }
    }

    /// The memory as the manifest codec stores it: a path and a restart view
    /// holding only its pages for each file, the least recently used first.
    fn snapshot(&self) -> SessionSnapshot {
        let (paths, views): (Vec<_>, Vec<_>) = self
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.path.clone(),
                    RestartView::new(
                        entry.current_page,
                        PageViewMode::Continuous,
                        RestartZoom::FitWidth,
                        0.,
                        0.,
                    )
                    .with_page_views(entry.page_views.clone()),
                )
            })
            .unzip();
        SessionSnapshot::new(paths, None).with_restart_views(views)
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        let Some(store) = self.store.clone() else {
            return;
        };
        let snapshot = self.snapshot();
        // Files with no identity yet get one, off the main thread.
        let missing = self
            .entries
            .iter()
            .map(|entry| entry.path.clone())
            .filter(|path| !self.locators.contains_key(&locator_key(path)))
            .collect::<Vec<_>>();
        let known = self.locators.clone();
        let root = self.root.clone();
        let previous = self.publication.take();
        let background = cx.background_executor().clone();
        self.publication = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let found = background
                .spawn(async move {
                    if let Err(error) = store.replace(&snapshot) {
                        eprintln!("Page memory could not be saved: {error:?}");
                    }
                    let found = missing
                        .into_iter()
                        .filter_map(|path| {
                            let locator = crate::file_locator::locate(&path)?;
                            Some((locator_key(&path), locator))
                        })
                        .collect::<HashMap<_, _>>();
                    if let Some(root) = root {
                        let mut all = known;
                        all.extend(found.clone());
                        if let Err(error) = save_locators(&root, &all) {
                            eprintln!("Page memory locations could not be saved: {error}");
                        }
                    }
                    found
                })
                .await;
            if found.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, _| {
                for (key, locator) in found {
                    // Only a file still remembered keeps its identity.
                    if this
                        .entries
                        .iter()
                        .any(|entry| locator_key(&entry.path) == key)
                    {
                        this.locators.insert(key, locator);
                    }
                }
            });
        }));
    }
}

fn locator_key(path: &Path) -> String {
    encode_path(path).1
}

/// A missing or unreadable locator file only means moves are not followed.
fn load_locators(root: &Path) -> HashMap<String, Vec<u8>> {
    let Ok(text) = std::fs::read_to_string(root.join(LOCATORS_NAME)) else {
        return HashMap::new();
    };
    let Ok(serde_json::Value::Object(file)) = serde_json::from_str(&text) else {
        return HashMap::new();
    };
    let Some(serde_json::Value::Object(locators)) = file.get("locators") else {
        return HashMap::new();
    };
    locators
        .iter()
        .filter_map(|(key, value)| Some((key.clone(), BASE64.decode(value.as_str()?).ok()?)))
        .collect()
}

fn save_locators(root: &Path, locators: &HashMap<String, Vec<u8>>) -> Result<(), String> {
    let locators = locators
        .iter()
        .map(|(key, locator)| (key.clone(), serde_json::Value::from(BASE64.encode(locator))))
        .collect::<serde_json::Map<_, _>>();
    let text = serde_json::json!({ "version": 1, "locators": locators }).to_string();
    let temporary = root.join(format!("{LOCATORS_NAME}.tmp"));
    std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, root.join(LOCATORS_NAME)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_document_view_state::{RememberedPageView, RestartZoom};

    fn views(pages: &[u32]) -> PageViews {
        pages
            .iter()
            .map(|page| {
                (
                    *page,
                    RememberedPageView::new(RestartZoom::Manual(150. + *page as f32), 12., 34.),
                )
            })
            .collect()
    }

    fn path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join("butter-paper-page-memory")
            .join(name)
    }

    #[test]
    fn a_file_gets_its_pages_back_and_another_file_is_unaffected() {
        let mut memory = PageMemory::default();
        assert!(memory.lookup(&path("a.pdf")).is_empty());
        assert!(memory.record(path("a.pdf"), views(&[0, 3])));
        assert!(memory.record(path("b.pdf"), views(&[1])));
        assert_eq!(memory.lookup(&path("a.pdf")), views(&[0, 3]));
        assert_eq!(memory.lookup(&path("b.pdf")), views(&[1]));
        assert!(memory.lookup(&path("c.pdf")).is_empty());
        // Newer pages for a file replace its older ones, not another's.
        assert!(memory.record(path("a.pdf"), views(&[2])));
        assert_eq!(memory.lookup(&path("a.pdf")), views(&[2]));
        assert_eq!(memory.lookup(&path("b.pdf")), views(&[1]));
        // The same pages again are no change.
        assert!(!memory.record(path("a.pdf"), views(&[2])));
    }

    #[test]
    fn no_pages_forgets_a_file() {
        let mut memory = PageMemory::default();
        memory.record(path("a.pdf"), views(&[0]));
        memory.record(path("b.pdf"), views(&[1]));
        assert!(memory.record(path("a.pdf"), Vec::new()));
        assert!(memory.lookup(&path("a.pdf")).is_empty());
        assert_eq!(memory.lookup(&path("b.pdf")), views(&[1]));
        assert!(
            !memory.record(path("a.pdf"), Vec::new()),
            "nothing to forget"
        );
    }

    #[test]
    fn the_last_fifty_files_stay_and_the_least_recently_used_goes_first() {
        let mut memory = PageMemory::default();
        for index in 0..MAX_REMEMBERED_FILES {
            memory.record(path(&format!("file-{index}.pdf")), views(&[index as u32]));
        }
        assert_eq!(memory.paths().count(), 50);
        // Using the oldest file again makes the next oldest the first to go.
        memory.record(path("file-0.pdf"), views(&[0, 9]));
        memory.record(path("file-50.pdf"), views(&[50]));
        assert_eq!(memory.paths().count(), 50);
        assert!(memory.lookup(&path("file-1.pdf")).is_empty());
        assert_eq!(memory.lookup(&path("file-0.pdf")), views(&[0, 9]));
        assert_eq!(memory.lookup(&path("file-50.pdf")), views(&[50]));
        assert_eq!(memory.lookup(&path("file-2.pdf")), views(&[2]));
        for index in 51..60 {
            memory.record(path(&format!("file-{index}.pdf")), views(&[0]));
        }
        assert_eq!(memory.paths().count(), 50);
        assert_eq!(
            memory.paths().next().unwrap(),
            path("file-11.pdf"),
            "the oldest nine went in order of use"
        );
        assert_eq!(memory.lookup(&path("file-0.pdf")), views(&[0, 9]));
    }

    #[test]
    fn the_memory_is_read_back_from_its_file_in_order_and_stays_at_fifty() {
        let root = std::env::temp_dir().join(format!("bp-page-memory-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut memory = PageMemory::open(root.clone()).unwrap();
        for index in 0..MAX_REMEMBERED_FILES + 5 {
            memory.record(path(&format!("file-{index}.pdf")), views(&[index as u32]));
        }
        memory.record(path("file-5.pdf"), views(&[5, 6]));
        memory
            .store
            .as_ref()
            .unwrap()
            .replace(&memory.snapshot())
            .unwrap();
        let reread = PageMemory::open(root.clone()).unwrap();
        assert_eq!(reread.paths().count(), MAX_REMEMBERED_FILES);
        assert_eq!(
            reread.paths().collect::<Vec<_>>(),
            memory.paths().collect::<Vec<_>>()
        );
        assert_eq!(reread.paths().last().unwrap(), path("file-5.pdf"));
        assert_eq!(reread.lookup(&path("file-5.pdf")), views(&[5, 6]));
        // The page shown is kept with the pages.
        let mut shown = PageMemory::open(root.clone()).unwrap();
        shown.record_at(path("file-5.pdf"), 6, views(&[5, 6]));
        shown
            .store
            .as_ref()
            .unwrap()
            .replace(&shown.snapshot())
            .unwrap();
        let view = PageMemory::open(root.clone())
            .unwrap()
            .restart_view(&path("file-5.pdf"))
            .unwrap();
        assert_eq!(view.current_page(), 6);
        assert!(reread.lookup(&path("file-0.pdf")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_reopens_on_the_page_it_was_left_on_with_that_pages_view() {
        let mut memory = PageMemory::default();
        let shown = views(&[4]);
        memory.record_at(path("a.pdf"), 4, [views(&[1, 2]), shown.clone()].concat());
        let view = memory.restart_view(&path("a.pdf")).unwrap();
        assert_eq!(view.current_page(), 4);
        assert_eq!(view.mode(), PageViewMode::SinglePage);
        assert_eq!(view.zoom(), shown[0].1.zoom());
        assert_eq!(view.scroll(), shown[0].1.scroll());
        assert_eq!(view.page_views(), views(&[1, 2]).as_slice());
        // Left in Continuous view: the page has no view, so nothing reopens it.
        memory.record_at(path("b.pdf"), 7, views(&[1]));
        assert_eq!(memory.restart_view(&path("b.pdf")), None);
        assert_eq!(memory.restart_view(&path("c.pdf")), None);
    }
}
