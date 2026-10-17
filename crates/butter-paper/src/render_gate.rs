//! Orders the requests waiting for a document's one PDF worker by how soon
//! the user needs their result. The worker draws one page at a time and a
//! heavy page can take a second, so a page the user clicked must not wait
//! behind thumbnails asked for earlier: whoever waits with the highest
//! priority goes next, and requests of equal priority go in order.

use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    sync::{Condvar, Mutex, MutexGuard},
};

/// Lowest first.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RenderPriority {
    /// Thumbnails, which the sidebar asks for one at a time.
    Thumbnail,
    /// The whole-page image of a page navigated to, under its tiles (a
    /// fallback and the Snapshot source), and its snapping geometry: needed
    /// after the tiles on screen.
    PageImage,
    /// Viewer tiles of the pages on screen.
    Viewer,
    /// What the user waits on directly: page text, edits, a page's render
    /// outside the viewer.
    Interactive,
}

#[derive(Default)]
pub struct RenderGate {
    state: Mutex<GateState>,
    turn: Condvar,
}

#[derive(Default)]
struct GateState {
    busy: bool,
    waiting: BinaryHeap<(RenderPriority, Reverse<u64>)>,
    next_ticket: u64,
}

/// The worker is this caller's until the turn drops.
pub struct RenderTurn<'a> {
    gate: &'a RenderGate,
}

impl RenderGate {
    fn lock(&self) -> MutexGuard<'_, GateState> {
        // The state stays consistent at every await point; a panic while
        // holding it cannot leave it half-updated.
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Waits until no request of higher priority, or of equal priority
    /// asked for earlier, is waiting, and the worker is free.
    pub fn enter(&self, priority: RenderPriority) -> RenderTurn<'_> {
        let mut state = self.lock();
        let ticket = (priority, Reverse(state.next_ticket));
        state.next_ticket += 1;
        state.waiting.push(ticket);
        while state.busy || state.waiting.peek() != Some(&ticket) {
            state = self
                .turn
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state.waiting.pop();
        state.busy = true;
        RenderTurn { gate: self }
    }
}

impl Drop for RenderTurn<'_> {
    fn drop(&mut self) {
        self.gate.lock().busy = false;
        self.gate.turn.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    /// Queues `waiters` behind a held turn, releases it, and returns the
    /// order they went in.
    fn order(waiters: &[(&'static str, RenderPriority)]) -> Vec<&'static str> {
        let gate = Arc::new(RenderGate::default());
        let order = Arc::new(Mutex::new(Vec::new()));
        let held = gate.enter(RenderPriority::Interactive);
        let threads = waiters
            .iter()
            .enumerate()
            .map(|(index, &(name, priority))| {
                let (waiter, order) = (gate.clone(), order.clone());
                let thread = thread::spawn(move || {
                    let _turn = waiter.enter(priority);
                    order.lock().unwrap().push(name);
                });
                // Each waiter queues before the next one asks.
                while gate.lock().waiting.len() <= index {
                    thread::sleep(Duration::from_millis(1));
                }
                thread
            })
            .collect::<Vec<_>>();
        drop(held);
        for thread in threads {
            thread.join().unwrap();
        }
        Arc::try_unwrap(order).unwrap().into_inner().unwrap()
    }

    #[test]
    fn a_page_the_user_asked_for_goes_before_waiting_thumbnails_and_tiles() {
        assert_eq!(
            order(&[
                ("thumbnail 1", RenderPriority::Thumbnail),
                ("tile", RenderPriority::Viewer),
                ("thumbnail 2", RenderPriority::Thumbnail),
                ("page", RenderPriority::Interactive),
            ]),
            ["page", "tile", "thumbnail 1", "thumbnail 2"]
        );
    }

    #[test]
    fn equal_priorities_go_in_the_order_they_asked() {
        assert_eq!(
            order(&[
                ("first", RenderPriority::Viewer),
                ("second", RenderPriority::Viewer),
                ("third", RenderPriority::Viewer),
            ]),
            ["first", "second", "third"]
        );
    }

    #[test]
    fn an_idle_gate_lets_any_request_straight_in() {
        let gate = RenderGate::default();
        drop(gate.enter(RenderPriority::Thumbnail));
        drop(gate.enter(RenderPriority::Interactive));
        assert!(!gate.lock().busy);
    }
}
