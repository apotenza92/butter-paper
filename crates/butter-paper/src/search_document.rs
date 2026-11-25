//! A document a search reads: its pages' text through the PDF worker, as the
//! Select tool shows it (edits included), and its markups. This is the one
//! implementation of [`SearchDocument`] the app uses; the workspace builds
//! one per open document from a snapshot of its state.

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::{
    document_resource::NativeDocumentResource,
    pdf_content_moves::{ContentExplode, ContentMove},
    pdf_page_base::PageBase,
    pdf_tables::PageTableEdit,
    pdf_text_edit::PageTextEdit,
    pdf_text_selection::selectable_lines_with_tables,
    search_markups::MarkupSearchRecord,
    search_run::SearchDocument,
    search_text::PageSearchText,
    search_text_cache::{SharedSearchTextCache, TextValidity},
};

/// The changes a page shows over the file's own text. Each list holds those
/// of the whole document; a page uses its own.
#[derive(Clone, Debug, Default)]
pub struct DocumentTextEdits {
    pub text_edits: Vec<PageTextEdit>,
    pub table_edits: Vec<PageTableEdit>,
    pub content_moves: Vec<ContentMove>,
    /// Blocks exploded into pages, which a search reads as page content.
    pub explodes: Vec<ContentExplode>,
    /// Pages shown as an earlier file had them.
    pub bases: Vec<PageBase>,
}

impl DocumentTextEdits {
    /// A number that changes when anything that shows on `page_index`
    /// changes.
    pub fn fingerprint(&self, page_index: u32) -> u64 {
        let mut hasher = DefaultHasher::new();
        for edit in self
            .text_edits
            .iter()
            .filter(|e| e.page_index == page_index)
        {
            format!("{edit:?}").hash(&mut hasher);
        }
        for edit in self
            .table_edits
            .iter()
            .filter(|e| e.page_index == page_index)
        {
            format!("{edit:?}").hash(&mut hasher);
        }
        for item in self
            .content_moves
            .iter()
            .filter(|m| m.page_index == page_index)
        {
            format!("{item:?}").hash(&mut hasher);
        }
        for explode in self.explodes.iter().filter(|e| e.page_index == page_index) {
            // Not the pasted clip itself, which its id names.
            (
                explode.rect.map(f64::to_bits),
                explode.nth,
                explode.offset.map(f64::to_bits),
                explode.scale.map(f64::to_bits),
                explode.rotation.to_bits(),
                explode.paste.as_ref().map(|paste| &paste.id),
            )
                .hash(&mut hasher);
        }
        for base in self.bases.iter().filter(|b| b.page_index == page_index) {
            base.id.hash(&mut hasher);
        }
        hasher.finish()
    }

    fn base_of(&self, page_index: u32) -> Option<&PageBase> {
        self.bases.iter().find(|base| base.page_index == page_index)
    }
}

pub struct ResourceSearchDocument {
    id: u64,
    name: String,
    page_count: u32,
    start_page: u32,
    resource: Arc<dyn NativeDocumentResource>,
    epoch: u64,
    edits: DocumentTextEdits,
    markups: Arc<Vec<MarkupSearchRecord>>,
    cache: SharedSearchTextCache,
}

impl ResourceSearchDocument {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        name: impl Into<String>,
        page_count: u32,
        resource: Arc<dyn NativeDocumentResource>,
        epoch: u64,
        edits: DocumentTextEdits,
        markups: Arc<Vec<MarkupSearchRecord>>,
        cache: SharedSearchTextCache,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            page_count,
            start_page: 0,
            resource,
            epoch,
            edits,
            markups,
            cache,
        }
    }

    /// Searches from `page_index` forward, then from the first page: the
    /// find bar starts where the reader is.
    pub fn starting_at(mut self, page_index: u32) -> Self {
        self.start_page = page_index.min(self.page_count.saturating_sub(1));
        self
    }
}

impl SearchDocument for ResourceSearchDocument {
    fn id(&self) -> u64 {
        self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn page_count(&self) -> u32 {
        self.page_count
    }

    fn start_page(&self) -> u32 {
        self.start_page
    }

    fn page_text(&self, page_index: u32) -> Result<Arc<PageSearchText>, String> {
        let validity = TextValidity {
            epoch: self.epoch,
            fingerprint: self.edits.fingerprint(page_index),
        };
        if let Some(text) = self.cache.lock().get(self.id, page_index, validity) {
            return Ok(text);
        }
        let base = self.edits.base_of(page_index);
        let explodes = self
            .edits
            .explodes
            .iter()
            .filter(|explode| explode.page_index == page_index)
            .cloned()
            .collect::<Vec<_>>();
        // The cache is not held while the worker reads.
        let layer = self
            .resource
            .page_search_text(page_index, base, &explodes)?;
        let tables = crate::pdf_tables::page_tables(&layer);
        let blocks = crate::pdf_text_blocks::paragraph_blocks_in(&layer, &tables);
        let lines = selectable_lines_with_tables(
            &layer,
            &blocks,
            &self.edits.text_edits,
            &self.edits.table_edits,
            &self.edits.content_moves,
        );
        let text = Arc::new(PageSearchText::with_rules(
            page_index,
            Arc::new(lines),
            &layer.rules,
        ));
        self.cache
            .lock()
            .insert(self.id, page_index, validity, text.clone());
        Ok(text)
    }

    fn markups(&self) -> Arc<Vec<MarkupSearchRecord>> {
        self.markups.clone()
    }
}
