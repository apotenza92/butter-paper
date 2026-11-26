//! Where a search looks: the document in view, or every document open in
//! every window. The binary owns the list of windows; it tells the
//! library about it once, at startup, with [`set_workspace_directory`]. A
//! search then asks [`open_workspaces`] for the workspaces it reads.
//!
//! Document ids are unique in the app (all windows share one counter), so a
//! result names its document by id alone.

use std::rc::Rc;

use gpui::{AnyWindowHandle, App, Entity, Global, WeakEntity};

use crate::document_workspace::DocumentWorkspace;

/// The documents a search panel reads. A new panel reads the document in
/// view; the panel keeps a changed scope while its window is open.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchScope {
    /// The active tab of the panel's window.
    #[default]
    ThisDocument,
    /// Every tab in every window.
    AllOpenDocuments,
}

impl SearchScope {
    /// The scopes in the order the panel lists them.
    pub const ALL: [Self; 2] = [Self::ThisDocument, Self::AllOpenDocuments];

    pub fn label(self) -> &'static str {
        match self {
            Self::ThisDocument => "This document",
            Self::AllOpenDocuments => "All open documents",
        }
    }

    /// The scope a label names, as the panel's menu gives it back.
    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.label() == label)
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
    fn this_document_comes_first_and_is_the_default() {
        assert_eq!(SearchScope::default(), SearchScope::ThisDocument);
        assert_eq!(
            SearchScope::ALL.map(SearchScope::label),
            ["This document", "All open documents"]
        );
        for scope in SearchScope::ALL {
            assert_eq!(SearchScope::from_label(scope.label()), Some(scope));
        }
        assert_eq!(SearchScope::from_label("Everything"), None);
    }
}
