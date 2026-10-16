//! Shared recent-PDF history. The manifest codec supplies lossless paths and
//! atomic publication in a separate directory; this is never a session restore.
//! Beside it, a locator file keeps each PDF's file system identity so the list
//! follows a PDF that moves (`file_locator`).
use crate::session_manifest::{
    SessionManifestStore, SessionSnapshot, encode_path, normalized_path_key,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use gpui::{Context, Task};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_RECENT_DOCUMENTS: usize = 20;
const LOCATORS_NAME: &str = "recent-locators.json";

#[derive(Default)]
pub struct RecentDocuments {
    paths: Vec<PathBuf>,
    /// File identities by `locator_key`; a hint, checked when resolved.
    locators: HashMap<String, Vec<u8>>,
    root: Option<PathBuf>,
    store: Option<Arc<SessionManifestStore>>,
    publication: Option<Task<()>>,
}

/// What a refresh found for one recorded path.
enum Found {
    Present(Option<Vec<u8>>),
    Moved(PathBuf, Option<Vec<u8>>),
    Missing,
}

impl RecentDocuments {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let store = Arc::new(
            SessionManifestStore::open(root.clone()).map_err(|error| format!("{error:?}"))?,
        );
        let (mut paths, _) = store
            .load()
            .map_err(|error| format!("{error:?}"))?
            .into_parts();
        paths.truncate(MAX_RECENT_DOCUMENTS);
        Ok(Self {
            paths,
            locators: load_locators(&root),
            root: Some(root),
            store: Some(store),
            publication: None,
        })
    }

    /// Wait for pending history publication during application shutdown.
    pub fn take_pending_publication(&mut self) -> Option<Task<()>> {
        self.publication.take()
    }

    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    pub(crate) fn remember(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let key = normalized_path_key(&path);
        self.paths
            .retain(|existing| normalized_path_key(existing) != key);
        self.paths.insert(0, path);
        self.paths.truncate(MAX_RECENT_DOCUMENTS);
        cx.notify();
        self.publish(cx);
        // Record the new entry's identity before it can move.
        self.refresh(cx);
    }

    /// Checks every PDF is still a file at its recorded path. One that moved
    /// within its volume takes its new path; one deleted, moved to the Trash
    /// or otherwise unfound leaves the list, so it only offers documents that
    /// open. Checks run off the main thread; a slow drive never blocks a
    /// window.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.paths.is_empty() {
            return;
        }
        let entries = self
            .paths
            .iter()
            .map(|path| (path.clone(), self.locators.get(&locator_key(path)).cloned()))
            .collect::<Vec<_>>();
        let check = cx.background_executor().spawn(async move {
            entries
                .into_iter()
                .map(|(path, locator)| {
                    let found = find(&path, locator.as_deref());
                    (path, locator, found)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |history, cx| {
            let results = check.await;
            let _ = history.update(cx, |history, cx| history.apply(results, cx));
        })
        .detach();
    }

    fn apply(&mut self, results: Vec<(PathBuf, Option<Vec<u8>>, Found)>, cx: &mut Context<Self>) {
        let mut changed = false;
        for (path, locator, found) in results {
            let key = normalized_path_key(&path);
            // The list may have changed while the check ran.
            let Some(index) = self
                .paths
                .iter()
                .position(|existing| normalized_path_key(existing) == key)
            else {
                continue;
            };
            match found {
                Found::Present(fresh) => {
                    if fresh.is_some() && fresh != locator {
                        changed |= self.set_locator(&path, fresh);
                    }
                }
                Found::Moved(new_path, fresh) => {
                    self.locators.remove(&locator_key(&path));
                    let new_key = normalized_path_key(&new_path);
                    if self
                        .paths
                        .iter()
                        .any(|existing| normalized_path_key(existing) == new_key)
                    {
                        self.paths.remove(index);
                    } else {
                        self.set_locator(&new_path, fresh);
                        self.paths[index] = new_path;
                    }
                    changed = true;
                }
                Found::Missing => {
                    self.locators.remove(&locator_key(&path));
                    self.paths.remove(index);
                    changed = true;
                }
            }
        }
        if changed {
            cx.notify();
            self.publish(cx);
        }
    }

    fn set_locator(&mut self, path: &Path, locator: Option<Vec<u8>>) -> bool {
        let key = locator_key(path);
        match locator {
            Some(locator) => self.locators.insert(key, locator.clone()) != Some(locator),
            None => self.locators.remove(&key).is_some(),
        }
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        if let Some(store) = self.store.clone() {
            let snapshot = SessionSnapshot::new(self.paths.clone(), None);
            let locators = self
                .paths
                .iter()
                .filter_map(|path| {
                    let key = locator_key(path);
                    let locator = self.locators.get(&key)?;
                    Some((key, BASE64.encode(locator).into()))
                })
                .collect::<serde_json::Map<_, _>>();
            let root = self.root.clone();
            let previous = self.publication.take();
            let background = cx.background_executor().clone();
            self.publication = Some(cx.spawn(async move |_, _| {
                if let Some(previous) = previous {
                    previous.await;
                }
                background
                    .spawn(async move {
                        if let Err(error) = store.replace(&snapshot) {
                            eprintln!("Recent PDFs could not be saved: {error:?}");
                        }
                        if let Some(root) = root
                            && let Err(error) = save_locators(&root, locators)
                        {
                            eprintln!("Recent PDF locations could not be saved: {error}");
                        }
                    })
                    .await;
            }));
        }
    }
}

fn find(path: &Path, locator: Option<&[u8]>) -> Found {
    if path.is_file() {
        return Found::Present(crate::file_locator::locate(path));
    }
    match locator.and_then(crate::file_locator::resolve) {
        Some(new_path) => {
            let fresh = crate::file_locator::locate(&new_path);
            Found::Moved(new_path, fresh)
        }
        None => Found::Missing,
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

fn save_locators(
    root: &Path,
    locators: serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let text = serde_json::json!({ "version": 1, "locators": locators }).to_string();
    let temporary = root.join(format!("{LOCATORS_NAME}.tmp"));
    std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, root.join(LOCATORS_NAME)).map_err(|error| error.to_string())
}

/// A path as people read it: Windows' `\\?\` verbatim prefix, which
/// opened documents keep for their canonical identity, is left off a drive
/// path where the plain form names the same file.
pub fn display_path(path: &std::path::Path) -> String {
    dunce::simplified(path).to_string_lossy().into_owned()
}

/// Stable identity across ordering changes, including non-Unicode paths.
pub fn recent_document_id(path: &std::path::Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    normalized_path_key(path).hash(&mut hash);
    format!("recent-pdf-{:016x}", hash.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[test]
    fn display_paths_leave_off_the_windows_verbatim_prefix() {
        #[cfg(windows)]
        {
            assert_eq!(
                display_path(std::path::Path::new(r"\\?\C:\Users\a\code\plan.pdf")),
                r"C:\Users\a\code\plan.pdf"
            );
        }
        assert_eq!(
            display_path(std::path::Path::new("/Users/a/code/plan.pdf")),
            "/Users/a/code/plan.pdf"
        );
    }

    #[gpui::test]
    fn recent_files_deduplicate_order_bound_and_persist(cx: &mut gpui::TestAppContext) {
        let root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-scratch/recent-documents");
        let _ = std::fs::remove_dir_all(&root);
        let history = cx.new(|_| RecentDocuments::open(root.clone()).unwrap());
        for ix in 0..25 {
            let path = root.join(format!("{ix}.pdf"));
            std::fs::write(&path, b"%PDF-1.7").unwrap();
            history.update(cx, |history, cx| history.remember(path, cx));
        }
        history.update(cx, |history, cx| history.remember(root.join("10.pdf"), cx));
        cx.run_until_parked();
        let loaded = RecentDocuments::open(root.clone()).unwrap();
        assert_eq!(loaded.paths().len(), 20);
        assert_eq!(loaded.paths()[0], root.join("10.pdf"));
        assert_eq!(
            loaded
                .paths()
                .iter()
                .filter(|path| path.ends_with("10.pdf"))
                .count(),
            1
        );
        assert!(!loaded.paths().contains(&root.join("0.pdf")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[gpui::test]
    fn missing_files_leave_the_history(cx: &mut gpui::TestAppContext) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-scratch/recent-documents-missing");
        let _ = std::fs::remove_dir_all(&root);
        let history = cx.new(|_| RecentDocuments::open(root.clone()).unwrap());
        let kept = root.join("kept.pdf");
        std::fs::write(&kept, b"%PDF-1.7").unwrap();
        // A folder with a PDF's name is not a document either.
        std::fs::create_dir_all(root.join("folder.pdf")).unwrap();
        for path in [
            root.join("moved.pdf"),
            kept.clone(),
            root.join("folder.pdf"),
        ] {
            history.update(cx, |history, cx| history.remember(path, cx));
        }
        history.update(cx, |history, cx| history.refresh(cx));
        cx.run_until_parked();
        history.read_with(cx, |history, _| {
            assert_eq!(history.paths(), std::slice::from_ref(&kept));
        });
        let loaded = RecentDocuments::open(root.clone()).unwrap();
        assert_eq!(loaded.paths(), std::slice::from_ref(&kept));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(any(target_os = "macos", windows))]
    #[gpui::test]
    fn moved_files_keep_their_place_at_the_new_path(cx: &mut gpui::TestAppContext) {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-scratch/recent-documents-moved");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("archive")).unwrap();
        let history = cx.new(|_| RecentDocuments::open(root.clone()).unwrap());
        let (first, moved, last) = (root.join("a.pdf"), root.join("b.pdf"), root.join("c.pdf"));
        for path in [&last, &moved, &first] {
            std::fs::write(path, b"%PDF-1.7").unwrap();
            history.update(cx, |history, cx| history.remember(path.clone(), cx));
        }
        cx.run_until_parked();
        let destination = root.join("archive/b renamed.pdf");
        std::fs::rename(&moved, &destination).unwrap();
        // A restart sees the move through the saved locator.
        let reopened = cx.new(|_| RecentDocuments::open(root.clone()).unwrap());
        reopened.update(cx, |history, cx| history.refresh(cx));
        cx.run_until_parked();
        reopened.read_with(cx, |history, _| {
            let names = history
                .paths()
                .iter()
                .map(|path| path.canonicalize().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                names,
                [&first, &destination, &last].map(|path| path.canonicalize().unwrap())
            );
        });
        std::fs::remove_dir_all(root).unwrap();
    }
}
