//! Session publication for several document windows sharing one session store.
//!
//! Each window closes through its own unsaved-changes transaction, and that
//! transaction publishes the restart manifest. Publishing also clears the
//! dirty-session marker and stops live marker writes, so only the final window
//! may reach the store:
//!
//! - Closing one of several windows publishes nothing; the others keep the
//!   marker live and the manifest is written when the last one closes.
//! - Closing the last window publishes that window's documents.
//! - During Quit every window closes; each adds its documents and the final
//!   one publishes the combined manifest.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use crate::application_close_workspace::{
    ApplicationCloseCheckpointPublication, ApplicationCloseCheckpointPublisher,
};
use crate::session_manifest::SessionSnapshot;

#[derive(Default)]
struct CoordinatorState {
    open: BTreeSet<u64>,
    quitting: bool,
    closed_in_quit: BTreeSet<u64>,
    pending: Option<SessionSnapshot>,
}

#[derive(Default)]
pub struct WindowSessionCoordinator {
    state: Mutex<CoordinatorState>,
}

impl WindowSessionCoordinator {
    fn state(&self) -> std::sync::MutexGuard<'_, CoordinatorState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn window_opened(&self, window_id: u64) {
        self.state().open.insert(window_id);
    }

    /// Records that a window has gone; returns whether any remain.
    pub fn window_closed(&self, window_id: u64) -> bool {
        let mut state = self.state();
        state.open.remove(&window_id);
        !state.open.is_empty()
    }

    pub fn open_window_count(&self) -> usize {
        self.state().open.len()
    }

    pub fn begin_quit(&self) {
        let mut state = self.state();
        if !state.quitting {
            state.quitting = true;
            state.closed_in_quit.clear();
            state.pending = None;
        }
    }

    /// The user kept a window open, so the application no longer quits.
    /// Windows already closed stay closed; their documents are not restored.
    pub fn cancel_quit(&self) {
        let mut state = self.state();
        state.quitting = false;
        state.closed_in_quit.clear();
        state.pending = None;
    }

    pub fn is_quitting(&self) -> bool {
        self.state().quitting
    }

    /// Returns the snapshot to publish for a closing window, or `None` when
    /// another open window will publish later.
    pub fn checkpoint_for_close(
        &self,
        window_id: u64,
        snapshot: SessionSnapshot,
    ) -> Option<SessionSnapshot> {
        let mut state = self.state();
        if state.quitting {
            state.closed_in_quit.insert(window_id);
            let mut combined = state.pending.take().unwrap_or_else(|| SessionSnapshot::new(Vec::new(), None));
            combined.append(snapshot);
            if state.open.is_subset(&state.closed_in_quit) {
                Some(combined)
            } else {
                state.pending = Some(combined);
                None
            }
        } else if state.open.len() <= 1 {
            Some(snapshot)
        } else {
            None
        }
    }
}

/// Close-time publisher for one window.
pub struct WindowCheckpointPublisher {
    window_id: u64,
    coordinator: Arc<WindowSessionCoordinator>,
    store: Arc<dyn ApplicationCloseCheckpointPublisher>,
}

impl WindowCheckpointPublisher {
    pub fn new(
        window_id: u64,
        coordinator: Arc<WindowSessionCoordinator>,
        store: Arc<dyn ApplicationCloseCheckpointPublisher>,
    ) -> Self {
        Self {
            window_id,
            coordinator,
            store,
        }
    }
}

impl ApplicationCloseCheckpointPublisher for WindowCheckpointPublisher {
    fn publish(
        &self,
        snapshot: &SessionSnapshot,
    ) -> Result<ApplicationCloseCheckpointPublication, String> {
        match self
            .coordinator
            .checkpoint_for_close(self.window_id, snapshot.clone())
        {
            Some(combined) => self.store.publish(&combined),
            None => Ok(ApplicationCloseCheckpointPublication::Published {
                document_count: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn snapshot(paths: &[&str], active: Option<usize>) -> SessionSnapshot {
        SessionSnapshot::new(paths.iter().map(PathBuf::from).collect(), active)
    }

    #[test]
    fn closing_one_of_several_windows_leaves_publication_to_the_last() {
        let coordinator = WindowSessionCoordinator::default();
        coordinator.window_opened(1);
        coordinator.window_opened(2);
        assert_eq!(coordinator.checkpoint_for_close(1, snapshot(&["/a.pdf"], Some(0))), None);
        assert!(coordinator.window_closed(1));
        assert_eq!(
            coordinator.checkpoint_for_close(2, snapshot(&["/b.pdf"], Some(0))),
            Some(snapshot(&["/b.pdf"], Some(0)))
        );
        assert!(!coordinator.window_closed(2));
    }

    #[test]
    fn quit_publishes_every_window_once_whatever_the_close_order() {
        let coordinator = WindowSessionCoordinator::default();
        for id in [1, 2, 3] {
            coordinator.window_opened(id);
        }
        coordinator.begin_quit();
        // Window 2 publishes before window 1 has been removed.
        assert_eq!(coordinator.checkpoint_for_close(2, snapshot(&["/b.pdf"], Some(0))), None);
        assert_eq!(coordinator.checkpoint_for_close(1, snapshot(&["/a.pdf", "/b.pdf"], Some(1))), None);
        coordinator.window_closed(1);
        let combined = coordinator
            .checkpoint_for_close(3, snapshot(&["/c.pdf"], None))
            .unwrap();
        assert_eq!(
            combined.documents(),
            &[PathBuf::from("/b.pdf"), PathBuf::from("/a.pdf"), PathBuf::from("/c.pdf")]
        );
        assert_eq!(combined, snapshot(&["/b.pdf", "/a.pdf", "/c.pdf"], Some(0)));
    }

    #[test]
    fn a_cancelled_quit_returns_to_per_window_closing() {
        let coordinator = WindowSessionCoordinator::default();
        coordinator.window_opened(1);
        coordinator.window_opened(2);
        coordinator.begin_quit();
        assert_eq!(coordinator.checkpoint_for_close(1, snapshot(&["/a.pdf"], None)), None);
        coordinator.window_closed(1);
        coordinator.cancel_quit();
        assert!(!coordinator.is_quitting());
        assert_eq!(
            coordinator.checkpoint_for_close(2, snapshot(&["/b.pdf"], None)),
            Some(snapshot(&["/b.pdf"], None))
        );
    }

    #[test]
    fn a_single_window_publishes_directly() {
        let coordinator = WindowSessionCoordinator::default();
        coordinator.window_opened(7);
        assert_eq!(
            coordinator.checkpoint_for_close(7, snapshot(&[], None)),
            Some(snapshot(&[], None))
        );
    }
}
