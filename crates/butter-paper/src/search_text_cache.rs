//! The page texts a search has read, kept so that the next search of the
//! same document needs no worker. It is separate from the workspace's own
//! page text (which keeps only the pages near the view) and bounded in
//! memory: the pages least recently used go first.

use std::{
    collections::{BTreeMap, HashMap},
    sync::{Arc, Mutex, MutexGuard},
};

use crate::search_text::PageSearchText;

/// How much memory the cache can hold, as far as it can tell.
pub const SEARCH_TEXT_CACHE_BYTES: usize = 64 * 1024 * 1024;

/// What a page's text was read from. A page whose document, edits or pages
/// changed has another one, and the old text is not used.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextValidity {
    /// The document's resource epoch: a save or reopen is a new one.
    pub epoch: u64,
    /// A hash of the page's edits and moves.
    pub fingerprint: u64,
}

struct Slot {
    validity: TextValidity,
    text: Arc<PageSearchText>,
    bytes: usize,
    tick: u64,
}

pub struct SearchTextCache {
    budget: usize,
    bytes: usize,
    tick: u64,
    pages: HashMap<(u64, u32), Slot>,
    /// Pages by when they were last used, oldest first.
    by_use: BTreeMap<u64, (u64, u32)>,
}

impl Default for SearchTextCache {
    fn default() -> Self {
        Self::with_budget(SEARCH_TEXT_CACHE_BYTES)
    }
}

impl SearchTextCache {
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

    fn remove(&mut self, key: (u64, u32)) {
        if let Some(slot) = self.pages.remove(&key) {
            self.bytes -= slot.bytes;
            self.by_use.remove(&slot.tick);
        }
    }

    /// The page's text, if it was read from `validity`.
    pub fn get(
        &mut self,
        document_id: u64,
        page_index: u32,
        validity: TextValidity,
    ) -> Option<Arc<PageSearchText>> {
        let key = (document_id, page_index);
        if self.pages.get(&key)?.validity != validity {
            self.remove(key);
            return None;
        }
        self.tick += 1;
        let tick = self.tick;
        let slot = self.pages.get_mut(&key)?;
        self.by_use.remove(&slot.tick);
        slot.tick = tick;
        self.by_use.insert(tick, key);
        Some(slot.text.clone())
    }

    pub fn insert(
        &mut self,
        document_id: u64,
        page_index: u32,
        validity: TextValidity,
        text: Arc<PageSearchText>,
    ) {
        let key = (document_id, page_index);
        self.remove(key);
        self.tick += 1;
        let bytes = text.approximate_bytes();
        self.bytes += bytes;
        self.by_use.insert(self.tick, key);
        self.pages.insert(
            key,
            Slot {
                validity,
                text,
                bytes,
                tick: self.tick,
            },
        );
        // The page just read stays, even when it alone is over budget.
        while self.bytes > self.budget && self.pages.len() > 1 {
            let Some((_, oldest)) = self.by_use.pop_first() else {
                break;
            };
            if let Some(slot) = self.pages.remove(&oldest) {
                self.bytes -= slot.bytes;
            }
        }
    }

    /// Drops every page of a document: it closed, or its pages moved.
    pub fn remove_document(&mut self, document_id: u64) {
        let keys = self
            .pages
            .keys()
            .filter(|(id, _)| *id == document_id)
            .copied()
            .collect::<Vec<_>>();
        for key in keys {
            self.remove(key);
        }
    }
}

/// The cache, shared by the search of every document.
#[derive(Clone, Default)]
pub struct SharedSearchTextCache(Arc<Mutex<SearchTextCache>>);

impl SharedSearchTextCache {
    pub fn with_budget(budget: usize) -> Self {
        Self(Arc::new(Mutex::new(SearchTextCache::with_budget(budget))))
    }

    pub fn lock(&self) -> MutexGuard<'_, SearchTextCache> {
        // Every method leaves the cache whole, so a panic elsewhere while
        // it was held does not make it wrong.
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_text::tests::{line, page};

    fn text(characters: usize) -> Arc<PageSearchText> {
        Arc::new(page(vec![line(&"a".repeat(characters), 0, 0., 0.)]))
    }

    fn validity(epoch: u64) -> TextValidity {
        TextValidity {
            epoch,
            fingerprint: 0,
        }
    }

    #[test]
    fn a_page_is_found_while_it_is_valid() {
        let mut cache = SearchTextCache::default();
        cache.insert(1, 0, validity(1), text(10));
        assert!(cache.get(1, 0, validity(1)).is_some());
        assert!(cache.get(1, 1, validity(1)).is_none());
        assert!(cache.get(2, 0, validity(1)).is_none());
    }

    #[test]
    fn a_page_read_from_other_edits_or_another_epoch_is_dropped() {
        let mut cache = SearchTextCache::default();
        cache.insert(1, 0, validity(1), text(10));
        assert!(cache.get(1, 0, validity(2)).is_none());
        assert!(cache.is_empty(), "the stale page leaves the cache");
        assert_eq!(cache.bytes(), 0);
        cache.insert(1, 0, validity(1), text(10));
        let edited = TextValidity {
            epoch: 1,
            fingerprint: 9,
        };
        assert!(cache.get(1, 0, edited).is_none());
    }

    #[test]
    fn the_pages_used_least_recently_go_first_when_the_budget_is_passed() {
        let one = text(100).approximate_bytes();
        let mut cache = SearchTextCache::with_budget(one * 3 + one / 2);
        for page in 0..3 {
            cache.insert(1, page, validity(1), text(100));
        }
        // Page 0 is used again, so page 1 is now the oldest.
        assert!(cache.get(1, 0, validity(1)).is_some());
        cache.insert(1, 3, validity(1), text(100));
        assert_eq!(cache.len(), 3);
        assert!(cache.get(1, 1, validity(1)).is_none());
        assert!(cache.get(1, 0, validity(1)).is_some());
        assert!(cache.get(1, 2, validity(1)).is_some());
        assert!(cache.get(1, 3, validity(1)).is_some());
        assert!(cache.bytes() <= one * 3 + one / 2);
    }

    #[test]
    fn a_page_over_the_budget_by_itself_is_still_kept() {
        let mut cache = SearchTextCache::with_budget(1);
        cache.insert(1, 0, validity(1), text(100));
        assert_eq!(cache.len(), 1);
        cache.insert(1, 1, validity(1), text(100));
        assert_eq!(cache.len(), 1);
        assert!(cache.get(1, 1, validity(1)).is_some());
    }

    #[test]
    fn replacing_a_page_does_not_count_it_twice() {
        let mut cache = SearchTextCache::default();
        cache.insert(1, 0, validity(1), text(100));
        let once = cache.bytes();
        cache.insert(1, 0, validity(1), text(100));
        assert_eq!(cache.bytes(), once);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn closing_a_document_drops_only_its_pages() {
        let mut cache = SearchTextCache::default();
        cache.insert(1, 0, validity(1), text(10));
        cache.insert(1, 1, validity(1), text(10));
        cache.insert(2, 0, validity(1), text(10));
        cache.remove_document(1);
        assert_eq!(cache.len(), 1);
        assert!(cache.get(2, 0, validity(1)).is_some());
        assert_eq!(cache.bytes(), text(10).approximate_bytes());
    }
}
