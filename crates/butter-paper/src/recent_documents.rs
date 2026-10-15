//! Shared recent-PDF history. The manifest codec supplies lossless paths and
//! atomic publication in a separate directory; this is never a session restore.
use crate::session_manifest::{SessionManifestStore, SessionSnapshot, normalized_path_key};
use gpui::{Context, Task};
use std::{path::PathBuf, sync::Arc};

const MAX_RECENT_DOCUMENTS: usize = 20;

#[derive(Default)]
pub struct RecentDocuments {
    paths: Vec<PathBuf>,
    store: Option<Arc<SessionManifestStore>>,
    publication: Option<Task<()>>,
}

impl RecentDocuments {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let store =
            Arc::new(SessionManifestStore::open(root).map_err(|error| format!("{error:?}"))?);
        let (mut paths, _) = store
            .load()
            .map_err(|error| format!("{error:?}"))?
            .into_parts();
        paths.truncate(MAX_RECENT_DOCUMENTS);
        Ok(Self {
            paths,
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
        if let Some(store) = self.store.clone() {
            let snapshot = SessionSnapshot::new(self.paths.clone(), None);
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
                    })
                    .await;
            }));
        }
    }
}

/// A path as people read it: Windows' `\\?\` verbatim prefix, which
/// opened documents keep for their canonical identity, is left off where
/// the plain form names the same file.
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
            assert_eq!(
                display_path(std::path::Path::new(r"\\?\UNC\server\share\plan.pdf")),
                r"\\server\share\plan.pdf"
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
            history.update(cx, |history, cx| {
                history.remember(root.join(format!("{ix}.pdf")), cx)
            });
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
}
