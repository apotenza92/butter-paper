//! Where a search looks: every document open in every window, or the
//! document in view. The binary owns the list of windows; it tells the
//! library about it once, at startup, with [`set_workspace_directory`]. A
//! search then asks [`open_workspaces`] for the workspaces it reads.
//!
//! Document ids are unique in the app (all windows share one counter), so a
//! result names its document by id alone.

use std::rc::Rc;

use gpui::{AnyWindowHandle, App, Entity, Global, WeakEntity};

use crate::document_workspace::DocumentWorkspace;

/// The documents a search panel reads.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchScope {
    /// Every tab in every window.
    #[default]
    AllOpenDocuments,
    /// The active tab of the panel's window.
    ThisDocument,
}

impl SearchScope {
    pub fn label(self) -> &'static str {
        match self {
            Self::AllOpenDocuments => "All open documents",
            Self::ThisDocument => "This document",
        }
    }
}

type Directory = dyn Fn(&App) -> Vec<(AnyWindowHandle, WeakEntity<DocumentWorkspace>)>;

/// The app's document windows, in window order, as the binary lists them.
struct WorkspaceDirectory(Rc<Directory>);

impl Global for WorkspaceDirectory {}

/// Tells searches where the document windows are. The binary calls this
/// once; tests call it to stand for several windows.
pub fn set_workspace_directory(
    directory: impl Fn(&App) -> Vec<(AnyWindowHandle, WeakEntity<DocumentWorkspace>)> + 'static,
    cx: &mut App,
) {
    cx.set_global(WorkspaceDirectory(Rc::new(directory)));
}

/// The open document windows and their workspaces, in window order. Empty
/// when no directory is set: a search then reads its own window only.
pub fn open_workspaces(cx: &App) -> Vec<(AnyWindowHandle, Entity<DocumentWorkspace>)> {
    let Some(directory) = cx.try_global::<WorkspaceDirectory>() else {
        return Vec::new();
    };
    let directory = directory.0.clone();
    directory(cx)
        .into_iter()
        .filter_map(|(window, workspace)| Some((window, workspace.upgrade()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scopes_have_their_labels() {
        assert_eq!(SearchScope::default(), SearchScope::AllOpenDocuments);
        assert_eq!(SearchScope::AllOpenDocuments.label(), "All open documents");
        assert_eq!(SearchScope::ThisDocument.label(), "This document");
    }
}
