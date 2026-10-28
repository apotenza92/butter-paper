//! The Edit PDF Content tool on the canvas: hovering, clicking and dragging in
//! existing page text, the inline editor, and keeping the PDF worker's
//! rendering in step with page-text edits.
//!
//! The page itself shows edited text: every change is applied by the worker
//! and re-rendered, and the viewer keeps the previous tiles until the new
//! ones arrive. The workspace only draws the hover outline, the caret and
//! the selection over it.

use super::*;
use crate::page_text_editor::{
    Backspace, CopyText, CutText, DeleteForward, DeleteToLineStart, DeleteWordBackward,
    DeleteWordForward, FinishEditing, InsertNewline, MoveDown, MoveLeft, MoveLineEnd,
    MoveLineStart, MoveRight, MoveUp, MoveWordLeft, MoveWordRight, NextCell,
    PAGE_TEXT_EDITOR_CONTEXT, PageTextEditState, PageTextInput, PageTextInputEvent,
    PageTextScreenMap, PasteText, PreviousCell, RedoText, SelectAllText, SelectDown, SelectLeft,
    SelectLineEnd, SelectLineStart, SelectRight, SelectUp, SelectWordLeft, SelectWordRight,
    UndoText,
};
use crate::pdf_text_blocks::PageTextBlock;
use crate::pdf_text_edit::{
    ExtraLine, PageTextEdit, PageTextLayer, PageTextLine, TextLineAnchor, extra_line_base,
    layout_text_edit_fitted,
};
use crate::pdf_text_reflow::Paragraph;
use crate::pdf_worker::DocumentEdits;
use gpui::{AnyElement, ElementInputHandler};
use std::collections::BTreeSet;

/// The hover and press reach of a line beyond its glyph box, in pixels.
const PAGE_TEXT_HIT_TOLERANCE_PX: f64 = 3.;

type Quad = [(f64, f64); 4];

pub(super) enum PageTextLayerState {
    Pending,
    /// The page's text, its paragraphs and its tables.
    Ready(
        Arc<PageTextLayer>,
        Arc<Vec<PageTextBlock>>,
        Arc<Vec<crate::pdf_tables::PageTable>>,
    ),
    Failed(String),
}

/// Per document: the editable text of its pages and the edits (page text
/// and form fields) the PDF worker currently renders.
#[derive(Default)]
pub(super) struct PageTextDocumentState {
    epoch: u64,
    layers: HashMap<u32, PageTextLayerState>,
    /// The kept page each page's text was read from (`None`: the file's).
    layer_bases: HashMap<u32, Option<String>>,
    /// The edits the worker renders.
    shown: DocumentEdits,
    /// Edits sent to the worker and not yet acknowledged.
    sending: Option<DocumentEdits>,
    /// Edits the worker rejected; not retried until they change.
    rejected: Option<DocumentEdits>,
    /// The last hovered point, tested again when its page's text arrives.
    hover_probe: Option<(PageInteraction, PdfPoint)>,
}

impl PageTextDocumentState {
    /// Whether reading the page's text failed.
    pub(super) fn failed(&self, page_index: u32) -> bool {
        matches!(
            self.layers.get(&page_index),
            Some(PageTextLayerState::Failed(_))
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PageTextEditing {
    pub document_id: DocumentId,
    pub page_index: u32,
    /// The edited paragraph's first line.
    pub anchor: TextLineAnchor,
    pub epoch: u64,
    /// The press that started a drag selection is still held.
    pub dragging: bool,
    /// The table cell the paragraph is in.
    pub table: Option<TableEditingCell>,
    /// How far table edits move the paragraph from where its lines are in
    /// the base document, in page space.
    pub offset: (f64, f64),
}

/// A table cell being edited: the table, the cell's row and column in the
/// edited table, and whether its text is the table edit's own (a cell with
/// no text in the base table).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TableEditingCell {
    pub key: crate::pdf_tables::TableKey,
    pub row: usize,
    pub column: usize,
    pub virtual_cell: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PageTextHover {
    pub document_id: DocumentId,
    pub page_index: u32,
    /// The hovered paragraph's first line, or the read-only line.
    pub anchor: TextLineAnchor,
    pub editable: bool,
    /// It is in a table cell, which a press edits directly.
    pub cell: bool,
}

/// What the viewport draws over one page in Edit PDF mode, in page user
/// space.
#[derive(Clone, Default)]
pub(super) struct PageTextOverlay {
    pub page_index: u32,
    /// The frames of the hovered paragraph, drawing or image.
    pub hover: Vec<Quad>,
    /// The edited paragraph, a quad per line.
    pub frame: Vec<Quad>,
    pub selection: Vec<Quad>,
    pub caret: Option<[(f64, f64); 2]>,
    /// Underlines under text an input method is composing.
    pub marked: Vec<[(f64, f64); 2]>,
    /// Return marks after the edited paragraph's hard line breaks.
    pub breaks: Vec<Vec<(f64, f64)>>,
    /// Fainter marks after its lines that wrap into the next.
    pub wraps: Vec<Vec<(f64, f64)>>,
    pub input: Option<(Entity<PageTextInput>, FocusHandle)>,
    /// How far table edits move the edited paragraph from its base lines:
    /// the input method's text positions are the base lines'.
    pub input_offset: (f64, f64),
    /// Labels under edited paragraphs and tables that run into other text
    /// or off the page.
    pub warnings: Vec<super::tables::ContentWarning>,
    /// The selected content's outlines.
    pub selected: Vec<Quad>,
    /// Its resize handles, page points.
    pub handles: Vec<(f64, f64)>,
}

/// A line under a page point: in a paragraph that can be edited, or a
/// read-only line.
pub(super) struct PageTextHit {
    pub layer: Arc<PageTextLayer>,
    pub block: Option<PageTextBlock>,
    pub line: PageTextLine,
    /// How far table edits and moves move the line on the page.
    pub offset: (f64, f64),
    /// The table cell it is in.
    pub cell: Option<TableEditingCell>,
}

fn shifted(quad: &Quad, (dx, dy): (f64, f64)) -> Quad {
    quad.map(|(x, y)| (x + dx, y + dy))
}

/// The text-space span of a line's visible characters as `runs` lay it
/// out, without leading and trailing spaces.
/// The pages whose content differs between two sets of edits: pages whose
/// text or table edits, moves or kept pages differ, and the pages of form fields
/// whose values differ.
fn changed_pages(
    before: &DocumentEdits,
    after: &DocumentEdits,
    form: &crate::pdf_forms::FormLayer,
) -> BTreeSet<u32> {
    let on_page = |edits: &[PageTextEdit], page: u32| {
        edits
            .iter()
            .filter(|edit| edit.page_index == page)
            .cloned()
            .collect::<Vec<_>>()
    };
    let mut pages = before
        .text
        .iter()
        .chain(&after.text)
        .map(|edit| edit.page_index)
        .filter(|page| on_page(&before.text, *page) != on_page(&after.text, *page))
        .collect::<BTreeSet<_>>();
    let tables_on = |edits: &DocumentEdits, page: u32| {
        edits
            .tables
            .iter()
            .filter(|edit| edit.page_index == page)
            .cloned()
            .collect::<Vec<_>>()
    };
    pages.extend(
        before
            .tables
            .iter()
            .chain(&after.tables)
            .map(|edit| edit.page_index)
            .filter(|page| tables_on(before, *page) != tables_on(after, *page)),
    );
    let moves_on = |edits: &DocumentEdits, page: u32| {
        edits
            .moves
            .iter()
            .filter(|item| item.page_index == page)
            .cloned()
            .collect::<Vec<_>>()
    };
    pages.extend(
        before
            .moves
            .iter()
            .chain(&after.moves)
            .map(|item| item.page_index)
            .filter(|page| moves_on(before, *page) != moves_on(after, *page)),
    );
    let base_on = |edits: &DocumentEdits, page: u32| {
        edits
            .bases
            .iter()
            .find(|base| base.page_index == page)
            .map(|base| base.id.clone())
    };
    pages.extend(
        before
            .bases
            .iter()
            .chain(&after.bases)
            .map(|base| base.page_index)
            .filter(|page| base_on(before, *page) != base_on(after, *page)),
    );
    let value = |edits: &DocumentEdits, name: &str| {
        edits
            .forms
            .iter()
            .find(|edit| edit.name == name)
            .map(|edit| edit.value.clone())
    };
    for edit in before.forms.iter().chain(&after.forms) {
        if value(before, &edit.name) != value(after, &edit.name)
            && let Some(field) = form.field(&edit.name)
        {
            pages.extend(field.widgets.iter().map(|widget| widget.page_index));
        }
    }
    pages
}

fn visible_span(
    layer: &PageTextLayer,
    line: &PageTextLine,
    edit: Option<&PageTextEdit>,
) -> Option<(f64, f64)> {
    let layout = match edit {
        Some(edit) => layout_text_edit_fitted(layer, line, &edit.runs, edit.layout.fit).ok()?,
        None => {
            layout_text_edit_fitted(layer, line, &line.original_runs(), Default::default()).ok()?
        }
    };
    let mut visible = layout
        .chars
        .iter()
        .filter(|character| !character.character.is_whitespace());
    let first = visible.next()?;
    let last = visible.next_back().unwrap_or(first);
    Some((first.x, last.x + last.advance))
}

/// How far a page point is outside a line's box, in text space; zero when
/// inside.
fn line_distance(
    line: &PageTextLine,
    span: (f64, f64),
    point: (f64, f64),
    tolerance: f64,
) -> Option<f64> {
    let (x, y) = line.text_space_point(point.0, point.1)?;
    let bottom = line.rise + line.descent;
    let top = line.rise + line.ascent;
    let dx = if x < span.0 {
        span.0 - x
    } else if x > span.1 {
        x - span.1
    } else {
        0.
    };
    let dy = if y < bottom {
        bottom - y
    } else if y > top {
        y - top
    } else {
        0.
    };
    (dx <= tolerance && dy <= tolerance).then_some(dx.max(dy))
}

fn line_scale(line: &PageTextLine) -> f64 {
    let m = line.matrix;
    (m[0] * m[3] - m[1] * m[2]).abs().sqrt().max(1e-9)
}

/// How long edits pause before the thumbnails of the pages they changed
/// are drawn again.
const THUMBNAIL_REFRESH_DELAY: std::time::Duration = std::time::Duration::from_millis(400);

impl DocumentWorkspace {
    fn page_text_state(
        &mut self,
        document_id: DocumentId,
        epoch: u64,
    ) -> &mut PageTextDocumentState {
        let state = self.page_text.entry(document_id).or_default();
        if state.epoch != epoch {
            // A reopened worker renders its own source unedited.
            *state = PageTextDocumentState {
                epoch,
                ..PageTextDocumentState::default()
            };
        }
        state
    }

    /// The editable text of a page, requesting it from the worker when
    /// it has not been read.
    /// In Edit PDF mode, loads the text of the visible pages, so the first
    /// press on them finds their lines.
    pub(super) fn prefetch_page_text(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        if !self.content_edit_mode(document_id) {
            return;
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let pages = {
            let session = session.read(cx);
            session
                .viewer
                .plan_snapshot()
                .map(|plan| plan.visible_pages.to_vec())
                .unwrap_or_default()
        };
        // Text far from the view is read again if it comes back, so a long
        // document keeps only the pages around the view.
        let editing = self
            .page_text_editing
            .filter(|editing| editing.document_id == document_id)
            .map(|editing| editing.page_index);
        if !pages.is_empty()
            && let Some(state) = self.page_text.get_mut(&document_id)
        {
            let near = |page: &u32| {
                Some(*page) == editing
                    || pages.iter().any(|visible| {
                        (*visible as i64 - i64::from(*page)).abs() <= KEPT_PAGES_AROUND_VIEW
                    })
            };
            state
                .layers
                .retain(|page, layer| near(page) || matches!(layer, PageTextLayerState::Pending));
        }
        // Reading text shares the worker with rendering, so one page is read
        // at a time, always one in view now: pages flown past while
        // scrolling are never queued. Each read's arrival redraws and picks
        // the next.
        let reading = self.page_text.get(&document_id).is_some_and(|state| {
            state
                .layers
                .values()
                .any(|layer| matches!(layer, PageTextLayerState::Pending))
        });
        if reading {
            return;
        }
        // Then the pages either side, where a table in view may run on.
        let page_count = self
            .session(document_id, cx)
            .map_or(0, |session| session.read(cx).page_count());
        let neighbours = pages
            .iter()
            .flat_map(|page| [page.checked_sub(1), page.checked_add(1)])
            .flatten()
            .filter(|page| *page < page_count)
            .collect::<Vec<_>>();
        let next = pages
            .into_iter()
            .chain(neighbours)
            .filter_map(|page| u32::try_from(page).ok())
            .find(|page| {
                self.page_text
                    .get(&document_id)
                    .is_none_or(|state| !state.layers.contains_key(page))
            });
        if let Some(page) = next {
            self.page_text_layer(document_id, page, cx);
        }
    }

    pub(super) fn paragraph_line_quads(
        &self,
        document_id: DocumentId,
        page_index: u32,
        layer: &PageTextLayer,
        block: &PageTextBlock,
        cx: &App,
    ) -> Vec<Quad> {
        let mut quads = Vec::new();
        for anchor in &block.lines {
            let Some(line) = layer.line(anchor) else {
                continue;
            };
            let edit = self.page_text_line_edit(document_id, page_index, *anchor, cx);
            if let Some((x0, x1)) = visible_span(layer, line, edit.as_ref()) {
                quads.push(line.quad(x0, x1));
            }
            for extra in edit.iter().flat_map(|edit| &edit.layout.extra_lines) {
                let base = extra_line_base(line, extra.below);
                if let Ok(layout) = layout_text_edit_fitted(layer, &base, &extra.runs, extra.fit)
                    && let (Some(first), Some(last)) = (layout.chars.first(), layout.chars.last())
                {
                    quads.push(base.quad(first.x, last.x + last.advance));
                }
            }
        }
        quads
    }

    pub(super) fn page_text_layer(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Option<Arc<PageTextLayer>> {
        let session = self.session(document_id, cx)?.clone();
        let (epoch, resource) = {
            let session = session.read(cx);
            (session.resource_epoch, session.resource.clone())
        };
        let state = self.page_text_state(document_id, epoch);
        match state.layers.get(&page_index) {
            Some(PageTextLayerState::Ready(layer, _, _)) => return Some(layer.clone()),
            Some(PageTextLayerState::Pending | PageTextLayerState::Failed(_)) => return None,
            None => {}
        }
        let resource = resource?;
        // A page shown as an earlier file had it is read from that page.
        let base = self
            .page_bases_of(document_id, cx)
            .into_iter()
            .find(|base| base.page_index == page_index);
        let requested = base.as_ref().map(|base| base.id.clone());
        let state = self.page_text_state(document_id, epoch);
        state.layers.insert(page_index, PageTextLayerState::Pending);
        state.layer_bases.insert(page_index, requested.clone());
        let task = cx.background_executor().spawn(async move {
            match base {
                Some(base) => resource.page_base_text(page_index, &base),
                None => resource.page_text(page_index),
            }
        });
        cx.spawn(async move |owner, cx| {
            let result = task.await;
            let _ = owner.update(cx, |workspace, cx| {
                let Some(state) = workspace.page_text.get_mut(&document_id) else {
                    return;
                };
                if state.epoch != epoch || state.layer_bases.get(&page_index) != Some(&requested) {
                    return;
                }
                state.layers.insert(
                    page_index,
                    match result {
                        Ok(layer) => {
                            let tables = crate::pdf_tables::page_tables(&layer);
                            let blocks =
                                crate::pdf_text_blocks::paragraph_blocks_in(&layer, &tables);
                            PageTextLayerState::Ready(
                                Arc::new(layer),
                                Arc::new(blocks),
                                Arc::new(tables),
                            )
                        }
                        Err(error) => PageTextLayerState::Failed(error),
                    },
                );
                let probe = state
                    .hover_probe
                    .filter(|(interaction, _)| interaction.page_index == page_index);
                workspace.refresh_content_snapping_for_text(document_id, page_index, cx);
                if let Some((interaction, point)) = probe
                    && workspace.editing_content(document_id, cx)
                {
                    workspace.update_page_text_hover(interaction, point, cx);
                }
                cx.notify();
            });
        })
        .detach();
        None
    }

    pub(super) fn page_text_edits_of(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<PageTextEdit> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_page_text_edits(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    /// The edits the page should show: the document's, with the paragraph
    /// being edited as it is now.
    pub(super) fn desired_page_text_edits(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<PageTextEdit> {
        let mut edits = self.page_text_edits_of(document_id, cx);
        // A cell with no base text keeps its text with the table edit.
        if let Some(editing) = self.page_text_editing
            && editing.document_id == document_id
            && !editing.table.is_some_and(|table| table.virtual_cell)
            && let Some(state) = self
                .page_text_input
                .as_ref()
                .and_then(|(input, _)| input.read(cx).state.as_ref())
        {
            let anchors = state.paragraph().anchors();
            edits.retain(|edit| {
                edit.page_index != editing.page_index || !anchors.contains(&edit.anchor)
            });
            edits.extend(
                state
                    .edits()
                    .into_iter()
                    .filter(|edit| !edit.is_unchanged()),
            );
            edits.sort_by_key(|edit| (edit.page_index, edit.anchor));
        }
        edits
    }

    /// A loaded page's text, paragraphs and tables.
    pub(super) fn loaded_page_text(
        &self,
        document_id: DocumentId,
        page_index: u32,
    ) -> Option<(
        Arc<PageTextLayer>,
        Arc<Vec<PageTextBlock>>,
        Arc<Vec<crate::pdf_tables::PageTable>>,
    )> {
        match self.page_text.get(&document_id)?.layers.get(&page_index)? {
            PageTextLayerState::Ready(layer, blocks, tables) => {
                Some((layer.clone(), blocks.clone(), tables.clone()))
            }
            _ => None,
        }
    }

    /// Pages the document shows as an earlier file had them.
    pub(super) fn page_bases_of(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<crate::pdf_page_base::PageBase> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_page_bases(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    pub(super) fn page_table_edits_of(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<crate::pdf_tables::PageTableEdit> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_page_table_edits(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    /// The document's edit of a table on a page, if any.
    pub(super) fn page_table_edit(
        &self,
        document_id: DocumentId,
        page_index: u32,
        key: crate::pdf_tables::TableKey,
        cx: &App,
    ) -> Option<crate::pdf_tables::PageTableEdit> {
        self.page_table_edits_of(document_id, cx)
            .into_iter()
            .find(|edit| edit.page_index == page_index && edit.key() == key)
    }

    /// The table being edited as it is now: the document's edit, with the
    /// edited cell's text as typed and rows grown to hold their cells'.
    fn live_table_edit(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<crate::pdf_tables::PageTableEdit> {
        let editing = self
            .page_text_editing
            .filter(|editing| editing.document_id == document_id)?;
        let cell = editing.table?;
        let (layer, _, tables) = self.loaded_page_text(document_id, editing.page_index)?;
        let table = tables.iter().find(|table| table.key() == cell.key)?;
        let current = self.page_table_edit(document_id, editing.page_index, cell.key, cx);
        let mut view = crate::table_editing::TableView::new(
            editing.page_index,
            table.clone(),
            current.as_ref(),
        );
        if cell.virtual_cell
            && let Some(state) = self
                .page_text_input
                .as_ref()
                .and_then(|(input, _)| input.read(cx).state.as_ref())
        {
            let origin = view.edit.cell_origin(cell.row, cell.column)?;
            let lines = crate::table_editing::cell_text_lines(origin, state.laid_out_lines());
            view.edit
                .cell_texts
                .retain(|text| (text.row, text.column) != (cell.row, cell.column));
            if !lines.is_empty() {
                view.edit.cell_texts.push(crate::pdf_tables::TableCellText {
                    row: cell.row,
                    column: cell.column,
                    lines,
                });
                view.edit
                    .cell_texts
                    .sort_by_key(|text| (text.row, text.column));
            }
        }
        let text = self
            .desired_page_text_edits(document_id, cx)
            .into_iter()
            .filter(|edit| edit.page_index == editing.page_index)
            .collect::<Vec<_>>();
        crate::table_editing::fit_rows(&layer, &mut view, &text);
        Some(view.edit)
    }

    /// The table edits the pages should show: the document's, with the
    /// table being edited as it is now.
    pub(super) fn desired_page_table_edits(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<crate::pdf_tables::PageTableEdit> {
        let mut edits = self.page_table_edits_of(document_id, cx);
        if let Some(live) = self
            .live_table_edit(document_id, cx)
            .or_else(|| self.column_drag_edit(document_id))
        {
            edits.retain(|edit| !(edit.page_index == live.page_index && edit.key() == live.key()));
            if !live.is_unchanged() {
                edits.push(live);
            }
            edits.sort_by_key(|edit| (edit.page_index, edit.key()));
        }
        edits
    }

    /// Whether a table on the page is being edited, so that it shows
    /// table edits the document does not have yet.
    pub(super) fn editing_page_tables(&self, document_id: DocumentId, page_index: u32) -> bool {
        self.page_text_editing.is_some_and(|editing| {
            editing.document_id == document_id
                && editing.page_index == page_index
                && editing.table.is_some()
        })
    }

    /// The table edits a page shows now.
    pub(super) fn shown_page_table_edits(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Vec<crate::pdf_tables::PageTableEdit> {
        let edits = self.page_table_edits_of(document_id, cx);
        if self.editing_page_tables(document_id, page_index) || self.column_drag_active(document_id)
        {
            self.desired_page_table_edits(document_id, cx)
        } else {
            edits
        }
        .into_iter()
        .filter(|edit| edit.page_index == page_index)
        .collect()
    }

    /// The form field values the pages should show: the document's.
    fn form_field_edits_of(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<crate::pdf_forms::FormFieldEdit> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_form_field_edits(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    /// Forgets the page text read for `document_id`'s pages: after they
    /// are rearranged it belongs to other pages.
    pub(super) fn forget_page_text_layers(&mut self, document_id: DocumentId) {
        if let Some(state) = self.page_text.get_mut(&document_id) {
            state.layers.clear();
            state.layer_bases.clear();
            state.hover_probe = None;
        }
    }

    /// Sends the edits the pages should show (page text and form fields) to
    /// the worker when they differ from what it renders, one update at a
    /// time, then refreshes the view.
    pub(super) fn sync_document_edits(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            self.page_text.remove(&document_id);
            return;
        };
        let (epoch, resource) = {
            let session = session.read(cx);
            (session.resource_epoch, session.resource.clone())
        };
        let desired = DocumentEdits {
            text: self.desired_page_text_edits(document_id, cx),
            forms: self.form_field_edits_of(document_id, cx),
            tables: self.desired_page_table_edits(document_id, cx),
            moves: self.desired_page_content_moves(document_id, cx),
            bases: self.page_bases_of(document_id, cx),
            pages: self
                .session(document_id, cx)
                .map(|session| {
                    session
                        .read(cx)
                        .annotations
                        .document_page_slots(document_id.value())
                        .to_vec()
                })
                .unwrap_or_default(),
        };
        let form = resource
            .as_ref()
            .map(|resource| resource.form_layer())
            .unwrap_or_default();
        let state = self.page_text_state(document_id, epoch);
        // Undo or redo past a save shows a page as another file had it: its
        // text is read again.
        let stale = state
            .layer_bases
            .iter()
            .filter(|(page, read_from)| {
                let shown = desired
                    .bases
                    .iter()
                    .find(|base| base.page_index == **page)
                    .map(|base| &base.id);
                read_from.as_ref() != shown
            })
            .map(|(page, _)| *page)
            .collect::<Vec<_>>();
        for page in stale {
            state.layers.remove(&page);
            state.layer_bases.remove(&page);
        }
        if state.sending.is_none() && state.shown == desired {
            return;
        }
        if state.sending.is_some() || state.rejected.as_ref() == Some(&desired) {
            return;
        }
        let Some(resource) = resource else {
            return;
        };
        state.sending = Some(desired.clone());
        let sent = desired.clone();
        self.note_content_sprite_sent(document_id);
        let task = cx
            .background_executor()
            .spawn(async move { resource.set_document_edits(&sent) });
        cx.spawn(async move |owner, cx| {
            let result = task.await;
            let _ = owner.update(cx, |workspace, cx| {
                workspace.note_content_sprite_replied(document_id);
                let Some(state) = workspace.page_text.get_mut(&document_id) else {
                    return;
                };
                if state.epoch != epoch {
                    return;
                }
                state.sending = None;
                match result {
                    Ok(()) => {
                        // Rearranged pages are all drawn and read again:
                        // anything read while the worker had the old order
                        // shows other pages.
                        let rearranged = state.shown.pages != desired.pages;
                        let mut changed_pages = changed_pages(&state.shown, &desired, &form);
                        if rearranged {
                            state.layers.clear();
                            state.layer_bases.clear();
                        }
                        state.shown = desired.clone();
                        state.rejected = None;
                        // Snapping reads the page as shown: changed pages are
                        // read again, but not mid-drag, where what moves would
                        // become its own target.
                        let refresh_snapping = workspace.content_drag.is_none();
                        if let Some(session) = workspace.session(document_id, cx).cloned() {
                            let rendered = session.update(cx, |session, cx| {
                                if rearranged {
                                    changed_pages.extend(0..session.page_sizes.len() as u32);
                                    session.thumbnail_requests.clear();
                                }
                                if refresh_snapping {
                                    for page in &changed_pages {
                                        session.page_content_geometry_requests.remove(page);
                                        session.annotations.clear_semantic_snap_page_content_page(
                                            document_id.value(),
                                            *page,
                                        );
                                    }
                                }
                                session.viewer.refresh_content();
                                cx.notify();
                                changed_pages
                                    .into_iter()
                                    .filter(|page| {
                                        session
                                            .thumbnails
                                            .iter()
                                            .any(|thumbnail| thumbnail.page_index == *page)
                                            || session.thumbnail_requests.contains_key(page)
                                    })
                                    .collect::<Vec<_>>()
                            });
                            // Pages without a thumbnail yet show the edit when
                            // they are first rendered; the others once edits
                            // pause, after the page itself.
                            workspace.defer_thumbnail_refresh(document_id, rendered, cx);
                            if refresh_snapping {
                                workspace.ensure_visible_page_content_snap_geometry(
                                    document_id,
                                    false,
                                    cx,
                                );
                            }
                        }
                    }
                    Err(error) => {
                        state.rejected = Some(desired);
                        workspace
                            .annotation_statuses
                            .insert(document_id, format!("Edits cannot be shown: {error}"));
                    }
                }
                // Catch up with changes made while this update ran.
                workspace.sync_document_edits(document_id, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Redraws the thumbnails of `pages` once edits pause: while typing,
    /// the worker draws the page being edited, not its thumbnail, after
    /// each keystroke.
    fn defer_thumbnail_refresh(
        &mut self,
        document_id: DocumentId,
        pages: Vec<u32>,
        cx: &mut Context<Self>,
    ) {
        if pages.is_empty() {
            return;
        }
        self.thumbnail_refreshes
            .entry(document_id)
            .or_default()
            .extend(pages);
        self.thumbnail_refresh_generation = self.thumbnail_refresh_generation.wrapping_add(1);
        let generation = self.thumbnail_refresh_generation;
        cx.spawn(async move |workspace, cx| {
            cx.background_executor()
                .timer(THUMBNAIL_REFRESH_DELAY)
                .await;
            let _ = workspace.update(cx, |workspace, cx| {
                if workspace.thumbnail_refresh_generation != generation {
                    return;
                }
                for (document_id, pages) in std::mem::take(&mut workspace.thumbnail_refreshes) {
                    for page in pages {
                        workspace.refresh_page_thumbnail(document_id, page, cx);
                    }
                }
            });
        })
        .detach();
    }

    /// The document's edit of a line, if any.
    fn page_text_line_edit(
        &self,
        document_id: DocumentId,
        page_index: u32,
        anchor: TextLineAnchor,
        cx: &App,
    ) -> Option<PageTextEdit> {
        self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .document_page_text_edits(document_id.value())
                .iter()
                .find(|edit| edit.page_index == page_index && edit.anchor == anchor)
                .cloned()
        })
    }

    pub(super) fn page_text_blocks(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Option<(Arc<PageTextLayer>, Arc<Vec<PageTextBlock>>)> {
        self.page_text_layer(document_id, page_index, cx)?;
        match self.page_text.get(&document_id)?.layers.get(&page_index)? {
            PageTextLayerState::Ready(layer, blocks, _) => Some((layer.clone(), blocks.clone())),
            _ => None,
        }
    }

    /// The line under a page point, and the paragraph it is in. Lines an
    /// earlier edit added below a paragraph count as its lines.
    fn page_text_hit(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<PageTextHit> {
        let (layer, blocks) =
            self.page_text_blocks(interaction.document_id, interaction.page_index, cx)?;
        let tolerance_points = interaction
            .transform
            .tolerance_points(PAGE_TEXT_HIT_TOLERANCE_PX)
            .unwrap_or(1.);
        // The paragraph being edited is hit across its whole box, so a press
        // at a line's end keeps editing it.
        if let Some(editing) = self.page_text_editing.filter(|editing| {
            editing.document_id == interaction.document_id
                && editing.page_index == interaction.page_index
        }) && let Some(state) = self
            .page_text_input
            .as_ref()
            .and_then(|(input, _)| input.read(cx).state.as_ref())
        {
            let first = &state.paragraph().lines()[0];
            let (x, y) = (point.x - editing.offset.0, point.y - editing.offset.1);
            if state.contains_point(x, y, tolerance_points / line_scale(first)) {
                let block = if editing.table.is_some_and(|table| table.virtual_cell) {
                    Some(state_block(state))
                } else {
                    blocks
                        .iter()
                        .find(|block| block.lines[0] == editing.anchor)
                        .cloned()
                };
                if let Some(block) = block {
                    return Some(PageTextHit {
                        layer: state.paragraph().layer().clone(),
                        block: Some(block),
                        line: first.clone(),
                        offset: editing.offset,
                        cell: editing.table,
                    });
                }
            }
        }
        let tables =
            self.shown_page_table_edits(interaction.document_id, interaction.page_index, cx);
        let table_refs = tables.iter().collect::<Vec<_>>();
        let moves =
            self.shown_page_content_moves(interaction.document_id, interaction.page_index, cx);
        let mut best: Option<(f64, PageTextHit)> = None;
        let mut consider = |line: &PageTextLine,
                            block: Option<&PageTextBlock>,
                            span: (f64, f64),
                            offset: (f64, f64)| {
            let tolerance = tolerance_points / line_scale(line);
            let at = (point.x - offset.0, point.y - offset.1);
            let Some(distance) = line_distance(line, span, at, tolerance) else {
                return;
            };
            if best.as_ref().is_some_and(|(best, _)| *best <= distance) {
                return;
            }
            best = Some((
                distance,
                PageTextHit {
                    layer: layer.clone(),
                    block: block.cloned(),
                    line: line.clone(),
                    offset,
                    cell: None,
                },
            ));
        };
        for line in &layer.lines {
            // Lines a table edit moves are hit where they are shown; a
            // deleted row's are gone.
            let Some(offset) = line_offset(line, &table_refs, &moves) else {
                continue;
            };
            let edit = self.page_text_line_edit(
                interaction.document_id,
                interaction.page_index,
                line.anchor,
                cx,
            );
            let block = blocks
                .iter()
                .find(|block| block.lines.contains(&line.anchor));
            if let Some(span) = visible_span(&layer, line, edit.as_ref()) {
                consider(line, block, span, offset);
            }
            for extra in edit.iter().flat_map(|edit| &edit.layout.extra_lines) {
                let base = extra_line_base(line, extra.below);
                if let Some(span) = visible_span(
                    &layer,
                    &base,
                    Some(&PageTextEdit {
                        page_index: interaction.page_index,
                        anchor: extra.anchor,
                        original: Vec::new(),
                        runs: extra.runs.clone(),
                        layout: crate::pdf_text_edit::TextEditLayout {
                            fit: extra.fit,
                            ..Default::default()
                        },
                    }),
                ) {
                    consider(&base, block, span, offset);
                }
            }
        }
        let mut hit = best.map(|(_, hit)| hit)?;
        hit.cell = self.hit_table_cell(interaction, &hit, cx);
        Some(hit)
    }

    /// The text under a page point, a table's cells first: inside a table
    /// as it is shown, its cells (their text, or a new cell's) are hit
    /// rather than text outside the table that it now covers. Text the page
    /// draws inside a table that no cell declares is still its own text.
    pub(super) fn page_text_and_table_hit(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<PageTextHit> {
        let hit = self.page_text_hit(interaction, point, cx);
        if hit.as_ref().is_some_and(|hit| {
            hit.cell.is_some() || self.drawn_inside_a_table(interaction, &hit.line)
        }) {
            return hit;
        }
        self.table_cell_hit(interaction, point, cx).or(hit)
    }

    /// Whether the page draws `line` inside one of its tables.
    fn drawn_inside_a_table(&self, interaction: PageInteraction, line: &PageTextLine) -> bool {
        self.loaded_page_text(interaction.document_id, interaction.page_index)
            .is_some_and(|(_, _, tables)| tables.iter().any(|table| table.draws_inside(line)))
    }

    /// The table cell a hit paragraph is in, in the table as it is shown.
    fn hit_table_cell(
        &self,
        interaction: PageInteraction,
        hit: &PageTextHit,
        cx: &App,
    ) -> Option<TableEditingCell> {
        let cell = hit.block.as_ref()?.cell?;
        let (_, _, tables) =
            self.loaded_page_text(interaction.document_id, interaction.page_index)?;
        let table = tables.iter().find(|table| table.key() == cell.table)?;
        let edit = self
            .shown_page_table_edits(interaction.document_id, interaction.page_index, cx)
            .into_iter()
            .find(|edit| edit.key() == cell.table);
        let view = crate::table_editing::TableView::new(
            interaction.page_index,
            table.clone(),
            edit.as_ref(),
        );
        let output = view.cell_of_base(cell.cell)?;
        Some(TableEditingCell {
            key: cell.table,
            row: output.row,
            column: output.column,
            virtual_cell: false,
        })
    }

    /// What a table, grown by its edit, runs into: past the page's bottom
    /// edge, and the lines of text outside it (which never move) that it
    /// now covers, where they are drawn. `None` when it fits.
    pub(super) fn table_overlaps_below(
        &self,
        document_id: DocumentId,
        page_index: u32,
        key: crate::pdf_tables::TableKey,
        cx: &App,
    ) -> Option<(super::tables::ContentWarningKind, Vec<[f64; 4]>)> {
        use super::tables::ContentWarningKind;
        let (layer, _, tables) = self.loaded_page_text(document_id, page_index)?;
        let table = tables.iter().find(|table| table.key() == key)?;
        let shown_edits = self.shown_page_table_edits(document_id, page_index, cx);
        let edit = shown_edits
            .iter()
            .find(|edit| edit.key() == key)
            .cloned()
            .unwrap_or_else(|| crate::pdf_tables::PageTableEdit::unchanged(page_index, table));
        let view = crate::table_editing::TableView::new(page_index, table.clone(), Some(&edit));
        let shown = view.bounds();
        let past_page = layer.page_box.is_some_and(|page| shown[1] < page[1] - 0.5);
        // The table's own text, and text the page always drew inside it,
        // which no cell's tags hold, is not run into.
        let in_table = |line: &PageTextLine| {
            table.draws_inside(line)
                || table
                    .cells
                    .iter()
                    .any(|cell| cell.lines.contains(&line.anchor))
        };
        // Text that is not the table's, where table edits and moves by
        // hand leave it, under the table as edited.
        let refs = shown_edits.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        let overlapped = layer
            .lines
            .iter()
            .filter(|line| !in_table(line) && !line.text.trim().is_empty())
            .filter_map(|line| {
                let rect = crate::pdf_tables::line_box(line)?;
                let core = crate::pdf_tables::line_core_box(line)?;
                let (dx, dy) = crate::pdf_content_moves::line_offset(&refs, &moves, core)?;
                let moved = [rect[0] + dx, rect[1] + dy, rect[2] + dx, rect[3] + dy];
                (moved[0] < shown[2] - 0.5
                    && moved[2] > shown[0] + 0.5
                    && moved[1] < shown[3] - 0.5
                    && moved[3] > shown[1] + 0.5)
                    .then_some(moved)
            })
            .collect::<Vec<_>>();
        if past_page {
            Some((ContentWarningKind::PastPage, overlapped))
        } else if !overlapped.is_empty() {
            Some((ContentWarningKind::Overlaps, overlapped))
        } else {
            None
        }
    }

    /// An editor for `paragraph` (opened from `block`) with its lines as
    /// the document shows them now: earlier edits' text, added lines, hard
    /// breaks and chosen settings; or, for a new cell, `new_cell_text`.
    pub(super) fn paragraph_edit_state(
        &self,
        document_id: DocumentId,
        page_index: u32,
        paragraph: Paragraph,
        block: &PageTextBlock,
        new_cell_text: Option<Vec<crate::pdf_text_edit::TextEditRun>>,
        cx: &App,
    ) -> Result<PageTextEditState, crate::pdf_text_edit::TextEditError> {
        let edits = block
            .lines
            .iter()
            .map(|anchor| self.page_text_line_edit(document_id, page_index, *anchor, cx))
            .collect::<Vec<_>>();
        let current = match new_cell_text {
            Some(runs) => vec![runs],
            None => paragraph
                .lines()
                .iter()
                .zip(&edits)
                .map(|(line, edit)| {
                    edit.as_ref()
                        .map_or_else(|| line.original_runs(), |edit| edit.runs.clone())
                })
                .collect::<Vec<_>>(),
        };
        let added: Vec<ExtraLine> = edits
            .last()
            .and_then(|edit| edit.as_ref())
            .map(|edit| edit.layout.extra_lines.clone())
            .unwrap_or_default();
        // Settings chosen earlier are kept with the first line.
        let format = edits
            .first()
            .and_then(|edit| edit.as_ref())
            .map(|edit| edit.layout.paragraph)
            .unwrap_or_default();
        let paragraph = paragraph.with_format(format);
        // Hard breaks (ending list items) an earlier edit left.
        let breaks = edits
            .iter()
            .map(|edit| edit.as_ref().map(|edit| edit.layout.hard_break))
            .collect::<Vec<_>>();
        PageTextEditState::new(paragraph, &current, &breaks, &added, 0)
    }

    /// How much wider a table edit makes the columns a cell spans.
    pub(super) fn column_growth(
        &self,
        interaction: PageInteraction,
        cell: TableEditingCell,
        cx: &App,
    ) -> f64 {
        self.column_growth_on(interaction.document_id, interaction.page_index, cell, cx)
    }

    pub(super) fn column_growth_on(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cell: TableEditingCell,
        cx: &App,
    ) -> f64 {
        let Some(edit) = self.page_table_edit(document_id, page_index, cell.key, cx) else {
            return 0.;
        };
        let Some((_, _, tables)) = self.loaded_page_text(document_id, page_index) else {
            return 0.;
        };
        let Some(table) = tables.iter().find(|table| table.key() == cell.key) else {
            return 0.;
        };
        let view = crate::table_editing::TableView::new(page_index, table.clone(), Some(&edit));
        let Some(output) = view
            .cells()
            .into_iter()
            .find(|output| (output.row, output.column) == (cell.row, cell.column))
        else {
            return 0.;
        };
        let Some(base) = output.base.map(|base| &table.cells[base]) else {
            return 0.;
        };
        let base_width = table.columns[base.column + base.column_span] - table.columns[base.column];
        let bounds = view.cell_bounds(&output);
        bounds[2] - bounds[0] - base_width
    }

    /// The text a new cell has in the document's table edit, as one run
    /// list: lines joined by a space, or by a line break where one ends them.
    pub(super) fn new_cell_runs(
        &self,
        interaction: PageInteraction,
        cell: TableEditingCell,
        cx: &App,
    ) -> Vec<crate::pdf_text_edit::TextEditRun> {
        let Some(edit) = self.page_table_edit(
            interaction.document_id,
            interaction.page_index,
            cell.key,
            cx,
        ) else {
            return Vec::new();
        };
        let Some(text) = edit
            .cell_texts
            .iter()
            .find(|text| (text.row, text.column) == (cell.row, cell.column))
        else {
            return Vec::new();
        };
        let mut runs: Vec<crate::pdf_text_edit::TextEditRun> = Vec::new();
        for (index, line) in text.lines.iter().enumerate() {
            runs.extend(line.runs.iter().cloned());
            if index + 1 == text.lines.len() {
                continue;
            }
            let joiner = if line.hard_break {
                Some('\n')
            } else if !line
                .runs
                .last()
                .is_some_and(|run| run.text.ends_with(char::is_whitespace))
            {
                Some(' ')
            } else {
                None
            };
            if let (Some(joiner), Some(last)) = (joiner, runs.last_mut()) {
                last.text.push(joiner);
            }
        }
        crate::pdf_text_edit::normalized_runs(&runs)
    }

    /// A press in a table cell away from its text: the cell's nearest
    /// paragraph, or for a cell with no base text, a new paragraph for it.
    fn table_cell_hit(
        &self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &App,
    ) -> Option<PageTextHit> {
        let (layer, blocks, tables) =
            self.loaded_page_text(interaction.document_id, interaction.page_index)?;
        let shown =
            self.shown_page_table_edits(interaction.document_id, interaction.page_index, cx);
        let refs = shown.iter().collect::<Vec<_>>();
        let moves =
            self.shown_page_content_moves(interaction.document_id, interaction.page_index, cx);
        for table in tables.iter() {
            let edit = shown.iter().find(|edit| edit.key() == table.key());
            let view =
                crate::table_editing::TableView::new(interaction.page_index, table.clone(), edit);
            let Some(cell) = view.cell_at(point.x, point.y) else {
                continue;
            };
            let key = table.key();
            if view.has_base_text(&cell) {
                let base = cell.base?;
                // The cell's paragraph nearest the press.
                let nearest = blocks
                    .iter()
                    .filter(|block| {
                        block.cell.is_some_and(|block_cell| {
                            block_cell.table == key && block_cell.cell == base
                        })
                    })
                    .filter_map(|block| {
                        let line = layer.line(block.lines.last()?)?;
                        let offset = line_offset(line, &refs, &moves)?;
                        let (_, y) = line.origin();
                        Some(((y + offset.1 - point.y).abs(), block, line, offset))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0))?;
                return Some(PageTextHit {
                    layer: layer.clone(),
                    block: Some(nearest.1.clone()),
                    line: nearest.2.clone(),
                    offset: nearest.3,
                    cell: Some(TableEditingCell {
                        key,
                        row: cell.row,
                        column: cell.column,
                        virtual_cell: false,
                    }),
                });
            }
            if table.read_only.is_some() {
                return None;
            }
            let (layer, block, line) = new_cell_paragraph(&layer, &blocks, &view, &cell)?;
            return Some(PageTextHit {
                layer,
                block: Some(block),
                line,
                offset: (0., 0.),
                cell: Some(TableEditingCell {
                    key,
                    row: cell.row,
                    column: cell.column,
                    virtual_cell: true,
                }),
            });
        }
        None
    }

    /// The anchor that names a hit: its paragraph's first line, or the line.
    fn hit_anchor(hit: &PageTextHit) -> TextLineAnchor {
        hit.block
            .as_ref()
            .map_or(hit.line.anchor, |block| block.lines[0])
    }

    /// Hover feedback for the Edit PDF Content tool. Hover and press share the
    /// same hit test, so the I-beam always means a click edits there.
    pub(super) fn update_page_text_hover(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> bool {
        let hit = self.page_text_and_table_hit(interaction, point, cx);
        if let Some(state) = self.page_text.get_mut(&interaction.document_id) {
            state.hover_probe = Some((interaction, point));
        }
        let hover = hit.map(|hit| PageTextHover {
            document_id: interaction.document_id,
            page_index: interaction.page_index,
            anchor: Self::hit_anchor(&hit),
            editable: hit.block.is_some(),
            cell: hit.cell.is_some(),
        });
        if self.page_text_hover != hover {
            self.page_text_hover = hover;
            cx.notify();
        }
        true
    }

    fn page_text_input_entity(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<PageTextInput> {
        if let Some((input, _)) = &self.page_text_input {
            return input.clone();
        }
        let input = cx.new(PageTextInput::new);
        let subscription = cx.subscribe_in(
            &input,
            window,
            |workspace, _, event: &PageTextInputEvent, window, cx| match event {
                PageTextInputEvent::Changed => {
                    if let Some(editing) = workspace.page_text_editing {
                        workspace.annotation_statuses.remove(&editing.document_id);
                        workspace.sync_document_edits(editing.document_id, cx);
                    }
                    cx.notify();
                }
                PageTextInputEvent::Finish => {
                    // The paragraph stays selected, to move or edit again.
                    let edited = workspace.page_text_editing;
                    workspace.commit_page_text_editing(cx);
                    if let Some(edited) = edited.filter(|edited| edited.table.is_none()) {
                        workspace.select_paragraph(
                            edited.document_id,
                            edited.page_index,
                            edited.anchor,
                            cx,
                        );
                    }
                    workspace.workspace_focus.focus(window, cx);
                    cx.notify();
                }
                PageTextInputEvent::Tab { back } => {
                    workspace.move_to_next_table_cell(*back, window, cx);
                    cx.notify();
                }
                PageTextInputEvent::JoinPrevious => {
                    workspace.join_previous_paragraph(window, cx);
                    cx.notify();
                }
                PageTextInputEvent::Rejected(character) => {
                    if let Some(editing) = workspace.page_text_editing {
                        workspace.annotation_statuses.insert(
                            editing.document_id,
                            format!(
                                "No available font can draw \u{201c}{character}\u{201d} in this text"
                            ),
                        );
                    }
                    cx.notify();
                }
            },
        );
        self.page_text_input = Some((input.clone(), subscription));
        input
    }

    /// A press with the Edit PDF Content tool: place the caret in editable text
    /// (selecting a word or the line on double and triple click), or finish
    /// editing when the press is elsewhere.
    /// Asks before the first text edit of a signed document: saving changed
    /// content invalidates its signatures (Revu refuses; this allows it).
    fn confirm_editing_signed_text(
        owner: WeakEntity<Self>,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.open_alert_dialog(cx, move |alert, _, _| {
            let owner = owner.clone();
            alert
                .close_button(false)
                .title("Edit content in a signed PDF?")
                .description(
                    "This PDF is digitally signed. Saving changes to its text invalidates its signatures.",
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new(DOCUMENT_SIGNED_TEXT_EDIT_CANCEL_ID)
                                .debug_selector(|| DOCUMENT_SIGNED_TEXT_EDIT_CANCEL_ID.into())
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(dialog_action_button(
                            Button::new(DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID)
                                .debug_selector(|| DOCUMENT_SIGNED_TEXT_EDIT_CONFIRM_ID.into())
                                .danger()
                                .label("Edit PDF"),
                        )),
                )
                .on_ok(move |_, _, cx| {
                    owner
                        .update(cx, |workspace, cx| {
                            workspace.page_text_signed_confirmed.insert(document_id);
                            workspace.annotation_statuses.insert(
                                document_id,
                                "Click text to edit it. Saving invalidates the signatures.".into(),
                            );
                            cx.notify();
                        })
                        .is_ok()
                })
        });
    }

    pub(super) fn begin_page_text_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        click_count: usize,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let hit = self.page_text_and_table_hit(interaction, point, cx);
        let Some(hit) = hit else {
            self.commit_page_text_editing(cx);
            if let Some(PageTextLayerState::Failed(error)) = self
                .page_text
                .get(&interaction.document_id)
                .and_then(|state| state.layers.get(&interaction.page_index))
            {
                self.annotation_statuses.insert(
                    interaction.document_id,
                    format!("Text on this page cannot be edited: {error}"),
                );
            }
            self.workspace_focus.focus(window, cx);
            cx.notify();
            return true;
        };
        let Some(block) = hit.block.clone() else {
            self.commit_page_text_editing(cx);
            let reason = hit
                .line
                .read_only
                .clone()
                .unwrap_or_else(|| "the text has no visible characters".into());
            self.annotation_statuses.insert(
                interaction.document_id,
                format!("This text cannot be edited: {reason}"),
            );
            cx.notify();
            return true;
        };
        let anchor = block.lines[0];
        let continuing = self.page_text_editing.is_some_and(|editing| {
            editing.document_id == interaction.document_id
                && editing.page_index == interaction.page_index
                && editing.anchor == anchor
        });
        let signed = self
            .session(interaction.document_id, cx)
            .and_then(|session| session.read(cx).resource.clone())
            .is_some_and(|resource| resource.digitally_signed());
        if signed
            && !continuing
            && !self
                .page_text_signed_confirmed
                .contains(&interaction.document_id)
        {
            self.commit_page_text_editing(cx);
            Self::confirm_editing_signed_text(
                cx.entity().downgrade(),
                interaction.document_id,
                window,
                cx,
            );
            return true;
        }
        let input = self.page_text_input_entity(window, cx);
        if !continuing {
            self.commit_page_text_editing(cx);
            // A cell's text wraps at its column's width as edited.
            let mut block = block;
            if let (Some(cell), Some(table)) = (block.cell.as_mut(), hit.cell)
                && !table.virtual_cell
            {
                cell.right += self.column_growth(interaction, table, cx);
            }
            let Some(paragraph) = Paragraph::new(hit.layer.clone(), &block) else {
                return true;
            };
            // A new cell's text is the table edit's.
            let new_cell_text = hit.cell.filter(|cell| cell.virtual_cell).map(|cell| {
                // Kept in page units; the paragraph's line has its model's
                // text space.
                let m = hit.line.matrix;
                let scale = (m[0] * m[3] - m[1] * m[2]).abs().sqrt().max(1e-9);
                self.new_cell_runs(interaction, cell, cx)
                    .into_iter()
                    .map(|run| crate::pdf_text_edit::TextEditRun {
                        style: crate::pdf_text_edit::TextRunStyle {
                            font_size: run.style.font_size / scale,
                            char_spacing: run.style.char_spacing / scale,
                            word_spacing: run.style.word_spacing / scale,
                            ..run.style
                        },
                        text: run.text,
                    })
                    .collect()
            });
            let state = match self.paragraph_edit_state(
                interaction.document_id,
                interaction.page_index,
                paragraph,
                &block,
                new_cell_text,
                cx,
            ) {
                Ok(state) => state,
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                    cx.notify();
                    return true;
                }
            };
            let epoch = self
                .session(interaction.document_id, cx)
                .map_or(0, |session| session.read(cx).resource_epoch);
            input.update(cx, |input, cx| {
                input.state = Some(state);
                cx.notify();
            });
            self.page_text_editing = Some(PageTextEditing {
                document_id: interaction.document_id,
                page_index: interaction.page_index,
                anchor,
                epoch,
                dragging: false,
                table: hit.cell,
                offset: hit.offset,
            });
        }
        let offset = self
            .page_text_editing
            .map_or((0., 0.), |editing| editing.offset);
        input.update(cx, |input, cx| {
            if let Some(state) = input.state.as_mut() {
                let index = state.index_at_point(point.x - offset.0, point.y - offset.1);
                match click_count {
                    2 => state.select_word_at(
                        index.saturating_sub(usize::from(index == state.len() && index > 0)),
                    ),
                    3 => state.select_line_at(index),
                    count if count >= 4 => state.select_all(),
                    _ => state.move_to(index, modifiers.shift && continuing),
                }
            }
            cx.notify();
        });
        if let Some(editing) = self.page_text_editing.as_mut() {
            editing.dragging = click_count == 1;
        }
        // The press cleared hover; the pointer is over the edited paragraph.
        self.page_text_hover = Some(PageTextHover {
            document_id: interaction.document_id,
            page_index: interaction.page_index,
            anchor,
            editable: true,
            cell: self
                .page_text_editing
                .is_some_and(|editing| editing.table.is_some()),
        });
        self.annotation_statuses.remove(&interaction.document_id);
        input.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
        true
    }

    /// Extends the selection while a press in the edited paragraph is held.
    pub(super) fn update_page_text_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(editing) = self.page_text_editing.filter(|editing| editing.dragging) else {
            return false;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(editing.document_id, editing.page_index))
            .copied()
        else {
            return true;
        };
        let Some(point) = Self::interaction_point(interaction, position, false) else {
            return true;
        };
        if let Some((input, _)) = &self.page_text_input {
            input.update(cx, |input, cx| {
                if let Some(state) = input.state.as_mut() {
                    let index = state
                        .index_at_point(point.x - editing.offset.0, point.y - editing.offset.1);
                    state.move_to(index, true);
                    cx.notify();
                }
            });
        }
        cx.notify();
        true
    }

    pub(super) fn finish_page_text_drag(&mut self, cx: &mut Context<Self>) -> bool {
        match self.page_text_editing.as_mut() {
            Some(editing) if editing.dragging => {
                editing.dragging = false;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub fn edit_pdf_content_property_inspector(
        &self,
    ) -> Option<Entity<crate::edit_pdf_content_property_inspector::EditPdfContentPropertyInspector>>
    {
        self.edit_pdf_content_property_inspector
            .as_ref()
            .map(|(inspector, _)| inspector.clone())
    }

    pub(super) fn ensure_edit_pdf_content_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<crate::edit_pdf_content_property_inspector::EditPdfContentPropertyInspector> {
        use crate::edit_pdf_content_property_inspector::{
            EditPdfContentPropertyEvent, EditPdfContentPropertyInspector,
        };
        if let Some((inspector, _)) = &self.edit_pdf_content_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| EditPdfContentPropertyInspector::new(window, cx));
        let subscription = cx.subscribe_in(
            &inspector,
            window,
            |workspace, _, event: &EditPdfContentPropertyEvent, window, cx| {
                workspace.apply_edit_pdf_content_property_patch(event.patch.clone(), cx);
                // Typing continues in the text, as after a toolbar command.
                if event.return_focus
                    && let Some((input, _)) = &workspace.page_text_input
                {
                    let focus = input.read(cx).focus_handle(cx);
                    window.focus(&focus, cx);
                }
            },
        );
        self.edit_pdf_content_property_inspector = Some((inspector.clone(), subscription));
        inspector
    }

    /// Whether the Edit PDF Content properties show: when the tool's properties
    /// are open, as other tools' are (a double-click on the tool, or the
    /// properties button); editing text never opens them.
    pub(super) fn edit_pdf_content_properties_visible(&self, _cx: &App) -> bool {
        self.right_rail_actions_open
    }

    /// Shows the edited paragraph's text properties in the inspector.
    pub(super) fn sync_edit_pdf_content_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.edit_pdf_content_property_snapshot(cx);
        let inspector = self.ensure_edit_pdf_content_property_inspector(window, cx);
        match snapshot {
            Some(snapshot) => {
                if inspector.read(cx).snapshot() != Some(&snapshot) {
                    inspector.update(cx, |inspector, cx| inspector.sync(snapshot, window, cx));
                }
            }
            None => inspector.update(cx, |inspector, cx| inspector.clear(cx)),
        }
    }

    fn edit_pdf_content_property_snapshot(
        &self,
        cx: &App,
    ) -> Option<crate::edit_pdf_content_property_inspector::EditPdfContentPropertySnapshot> {
        let (input, _) = self.page_text_input.as_ref()?;
        let state = input.read(cx).state.as_ref()?;
        let style = state.current_style()?.clone();
        let paragraph = state.paragraph();
        let layer = paragraph.layer();
        let metrics = layer.font(&style.font)?;
        let (family, bold, italic) = match &style.face {
            Some(face) => (
                Some(crate::pdf_engine::text_face_family(face)),
                face.fallback.bold,
                face.fallback.italic,
            ),
            None => (None, metrics.fallback.bold, metrics.fallback.italic),
        };
        let line_fill = paragraph.lines()[0].fill;
        Some(
            crate::edit_pdf_content_property_inspector::EditPdfContentPropertySnapshot {
                family,
                size: style.font_size,
                fill: style.fill.or(line_fill).unwrap_or([0.; 3]),
                bold,
                italic,
                alignment: paragraph.alignment(),
                line_spacing: (paragraph.line_spacing() * 100.).round() / 100.,
                substitutes: state.substitutes(),
                selection: !state.selection().is_empty(),
            },
        )
    }

    /// Applies a text property to the selection, or the whole paragraph.
    fn apply_edit_pdf_content_property_patch(
        &mut self,
        patch: crate::edit_pdf_content_property_inspector::EditPdfContentPropertyPatch,
        cx: &mut Context<Self>,
    ) {
        use crate::edit_pdf_content_property_inspector::EditPdfContentPropertyPatch;
        use crate::pdf_text_edit::TextRunStyle;
        let Some(input) = self
            .page_text_input
            .as_ref()
            .map(|(input, _)| input.clone())
        else {
            return;
        };
        let Some(layer) = input
            .read(cx)
            .state
            .as_ref()
            .map(|state| state.paragraph().layer().clone())
        else {
            return;
        };
        // The family and style a run is set in now.
        let current = {
            let layer = layer.clone();
            move |style: &TextRunStyle| match &style.face {
                Some(face) => (
                    crate::pdf_engine::text_face_family(face),
                    face.fallback.bold,
                    face.fallback.italic,
                ),
                None => layer.font(&style.font).map_or_else(
                    || ("Arimo".to_owned(), false, false),
                    |metrics| {
                        (
                            crate::pdf_engine::page_font_family(metrics),
                            metrics.fallback.bold,
                            metrics.fallback.italic,
                        )
                    },
                ),
            }
        };
        let restyled = |style: &TextRunStyle, family: String, bold: bool, italic: bool| {
            let mut style = style.clone();
            let own = layer.font(&style.font).map(|metrics| {
                (
                    crate::pdf_engine::page_font_family(metrics),
                    metrics.fallback.bold,
                    metrics.fallback.italic,
                )
            });
            // Back in the page font's own family and style: draw with it.
            style.face = (own != Some((family.clone(), bold, italic)))
                .then(|| crate::pdf_engine::text_face_for(&family, bold, italic));
            style
        };
        match patch {
            EditPdfContentPropertyPatch::Family(family) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| match &family {
                        None => TextRunStyle {
                            face: None,
                            ..style.clone()
                        },
                        Some(family) => {
                            let (_, bold, italic) = current(style);
                            let mut next = restyled(style, family.clone(), bold, italic);
                            // A family chosen by name keeps that family even
                            // when it is the page font's.
                            if next.face.is_none() {
                                next.face =
                                    Some(crate::pdf_engine::text_face_for(family, bold, italic));
                            }
                            next
                        }
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Bold(bold) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| {
                        let (family, _, italic) = current(style);
                        restyled(style, family, bold, italic)
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Italic(italic) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| {
                        let (family, bold, _) = current(style);
                        restyled(style, family, bold, italic)
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Size(size) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| TextRunStyle {
                        font_size: size,
                        ..style.clone()
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Fill(fill) => input.update(cx, |input, cx| {
                input.restyle(
                    |style| TextRunStyle {
                        fill: Some(fill),
                        ..style.clone()
                    },
                    cx,
                )
            }),
            EditPdfContentPropertyPatch::Alignment(alignment) => input.update(cx, |input, cx| {
                let Some(mut format) = input.state.as_ref().map(|state| state.paragraph().format())
                else {
                    return;
                };
                format.alignment = Some(alignment);
                input.set_format(format, cx);
            }),
            EditPdfContentPropertyPatch::LineSpacing(spacing) => input.update(cx, |input, cx| {
                let Some(mut format) = input.state.as_ref().map(|state| state.paragraph().format())
                else {
                    return;
                };
                format.line_spacing = Some(spacing);
                input.set_format(format, cx);
            }),
        }
        cx.notify();
    }

    /// Records the paragraph being edited as one undoable change and stops
    /// editing. The page already shows the text.
    pub(super) fn commit_page_text_editing(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = self.page_text_editing else {
            return;
        };
        // The edited cell's table, with its rows grown to hold the text.
        let table = editing
            .table
            .map(|cell| (cell.key, self.live_table_edit(editing.document_id, cx)));
        self.page_text_editing = None;
        let state = self
            .page_text_input
            .as_ref()
            .and_then(|(input, _)| input.update(cx, |input, _| input.state.take()));
        let Some(state) = state else {
            return;
        };
        let current_epoch = self
            .session(editing.document_id, cx)
            .map(|session| session.read(cx).resource_epoch);
        if current_epoch != Some(editing.epoch) {
            // The document was reopened under the editor; its lines moved.
            self.annotation_statuses.insert(
                editing.document_id,
                "The edited text was discarded because the document was reloaded".into(),
            );
            self.sync_document_edits(editing.document_id, cx);
            return;
        }
        // A new cell's text is kept with its table edit.
        let virtual_cell = editing.table.is_some_and(|cell| cell.virtual_cell);
        let (edits, anchors) = if virtual_cell {
            (Vec::new(), Vec::new())
        } else {
            (state.edits(), state.paragraph().anchors())
        };
        let page_index = state.page_index();
        let tables = table.into_iter().collect::<Vec<_>>();
        let result = self.update_annotation_history_with_result(
            editing.document_id,
            cx,
            move |annotations, document_id| {
                annotations.set_document_page_content_edits(
                    document_id,
                    page_index,
                    &anchors,
                    edits,
                    tables,
                )
            },
        );
        if let Err(error) = result {
            self.annotation_statuses.insert(
                editing.document_id,
                format!("Edited text was not kept: {error}"),
            );
        }
        self.sync_document_edits(editing.document_id, cx);
        cx.notify();
    }

    /// Stops editing when the tool, document or page it belongs to goes
    /// away.
    pub(super) fn reconcile_page_text_editing(&mut self, cx: &mut Context<Self>) {
        // Selected content belongs to the active document in Edit PDF mode.
        if self.content_selection.as_ref().is_some_and(|selection| {
            self.active_document_id != Some(selection.document_id)
                || !self.editing_content(selection.document_id, cx)
        }) {
            self.content_selection = None;
            self.content_drag = None;
        }
        if self
            .content_hover
            .as_ref()
            .is_some_and(|hover| !self.editing_content(hover.document_id, cx))
        {
            self.content_hover = None;
        }
        let Some(editing) = self.page_text_editing else {
            return;
        };
        let still_editing = self.active_document_id == Some(editing.document_id)
            && self.editing_content(editing.document_id, cx)
            && !self.pan_tool_active;
        if !still_editing {
            self.commit_page_text_editing(cx);
        }
    }

    /// What the viewport draws for the Edit PDF Content tool on each page.
    /// Delivers input-method text to the page-text editor the way the
    /// platform's text input does: `compose` replaces the marked (composing)
    /// text and keeps it marked, otherwise `text` replaces it and commits.
    /// Returns whether an editor took the text.
    pub fn page_text_input_method(
        &mut self,
        text: &str,
        compose: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use gpui::EntityInputHandler as _;
        let Some(input) = self
            .page_text_input
            .as_ref()
            .map(|(input, _)| input.clone())
        else {
            return false;
        };
        if self.page_text_editing.is_none() {
            return false;
        }
        input.update(cx, |input, cx| {
            if compose {
                let end = text.encode_utf16().count();
                input.replace_and_mark_text_in_range(None, text, Some(end..end), window, cx);
            } else {
                input.replace_text_in_range(None, text, window, cx);
            }
        });
        cx.notify();
        true
    }

    /// For review logs: each page's text state and the edits the worker
    /// shows, sends and rejected.
    pub fn page_text_sync_summary(&self, document_id: DocumentId) -> String {
        let Some(state) = self.page_text.get(&document_id) else {
            return "no page text state".into();
        };
        let texts = |edits: &DocumentEdits| {
            edits
                .text
                .iter()
                .map(PageTextEdit::text)
                .chain(
                    edits
                        .forms
                        .iter()
                        .map(|edit| format!("{}={:?}", edit.name, edit.value)),
                )
                .collect::<Vec<_>>()
        };
        let mut layers = state
            .layers
            .iter()
            .map(|(page, layer)| {
                let layer = match layer {
                    PageTextLayerState::Pending => "pending".into(),
                    PageTextLayerState::Ready(_, blocks, tables) => {
                        format!("{} blocks, {} tables", blocks.len(), tables.len())
                    }
                    PageTextLayerState::Failed(error) => format!("failed: {error}"),
                };
                format!("page {page}: {layer}")
            })
            .collect::<Vec<_>>();
        layers.sort();
        format!(
            "layers {layers:?} probe {:?} hover {:?} shown {:?} sending {:?} rejected {:?}",
            state.hover_probe.map(|(_, point)| (point.x, point.y)),
            self.page_text_hover,
            texts(&state.shown),
            state.sending.as_ref().map(texts),
            state.rejected.as_ref().map(texts)
        )
    }

    /// The text of the line being edited, if any.
    pub fn page_text_editing_text(&self, cx: &App) -> Option<String> {
        self.page_text_editing?;
        let (input, _) = self.page_text_input.as_ref()?;
        Some(input.read(cx).state.as_ref()?.text())
    }

    /// The text an input method is composing in the page-text editor.
    pub fn page_text_composing(&self, cx: &App) -> Option<String> {
        let (input, _) = self.page_text_input.as_ref()?;
        let state = input.read(cx).state.as_ref()?;
        let range = state.marked()?;
        Some(
            state
                .text()
                .chars()
                .skip(range.start)
                .take(range.len())
                .collect(),
        )
    }

    pub(super) fn page_text_overlays(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Vec<PageTextOverlay> {
        let mut overlays: Vec<PageTextOverlay> = Vec::new();
        let editing = self
            .page_text_editing
            .filter(|editing| editing.document_id == document_id);
        let editing_layer =
            editing.and_then(|editing| self.page_text_layer(document_id, editing.page_index, cx));
        if let Some(editing) = editing
            && let Some((input, _)) = &self.page_text_input
            && let Some(state) = input.read(cx).state.as_ref()
        {
            let offset = editing.offset;
            let shift_points = |points: Vec<(f64, f64)>| {
                points
                    .into_iter()
                    .map(|(x, y)| (x + offset.0, y + offset.1))
                    .collect::<Vec<_>>()
            };
            let selection = state
                .selection_quads()
                .iter()
                .map(|quad| shifted(quad, offset))
                .collect::<Vec<_>>();
            let caret = selection
                .is_empty()
                .then(|| state.caret_segment())
                .flatten()
                .map(|segment| segment.map(|(x, y)| (x + offset.0, y + offset.1)));
            let base_frame = state.frame_quads();
            // One box round the paragraph, as hovered and selected
            // content is framed.
            let frame = super::content::quads_bounds(&base_frame)
                .map(|bounds| shifted(&bounds_quad(bounds), offset))
                .into_iter()
                .collect::<Vec<_>>();
            // Edited text never moves other text: a label warns when a
            // line runs past the page's bottom, or into other text (added
            // below, or grown into a table's next cell). A cell's table
            // warns for it.
            let anchors = state.paragraph().anchors();
            let past_page = editing_layer
                .as_ref()
                .and_then(|layer| layer.page_box)
                .is_some_and(|page| frame.iter().flatten().any(|point| point.1 < page[1] - 0.5));
            // The other text its changed lines now cover.
            let overlapped = editing_layer
                .as_ref()
                .map(|layer| {
                    // Text the paragraph already overlapped does not count.
                    let original = layer
                        .lines
                        .iter()
                        .filter(|line| anchors.contains(&line.anchor))
                        .map(|line| line.quad(0., line.end_x))
                        .collect::<Vec<_>>();
                    let changed = state
                        .wrapped()
                        .iter()
                        .zip(&base_frame)
                        .filter(|(line, _)| !line.unchanged)
                        .map(|(_, quad)| *quad)
                        .collect::<Vec<_>>();
                    layer
                        .lines
                        .iter()
                        .filter(|line| {
                            !anchors.contains(&line.anchor) && !line.text.trim().is_empty()
                        })
                        .map(|line| line.quad(0., line.end_x))
                        .filter(|other| !original.iter().any(|quad| quads_overlap(quad, other)))
                        .filter(|other| changed.iter().any(|quad| quads_overlap(quad, other)))
                        .filter_map(|other| super::content::quads_bounds(&[other]))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            use super::tables::{ContentWarning, ContentWarningKind};
            let kind = if past_page {
                Some(ContentWarningKind::PastPage)
            } else {
                (!overlapped.is_empty()).then_some(ContentWarningKind::Overlaps)
            };
            let warnings = kind
                .filter(|_| editing.table.is_none())
                .and_then(|kind| {
                    let bounds = super::content::quads_bounds(&frame)?;
                    Some(ContentWarning {
                        bounds,
                        kind,
                        overlapped,
                    })
                })
                .into_iter()
                .collect();
            let overlay = PageTextOverlay {
                page_index: editing.page_index,
                hover: Vec::new(),
                frame,
                warnings,
                selection,
                caret,
                marked: state
                    .marked_segments()
                    .into_iter()
                    .map(|segment| segment.map(|(x, y)| (x + offset.0, y + offset.1)))
                    .collect(),
                breaks: state.break_marks().into_iter().map(shift_points).collect(),
                wraps: state.wrap_marks().into_iter().map(shift_points).collect(),
                input: Some((input.clone(), input.read(cx).focus_handle(cx))),
                input_offset: offset,
                ..Default::default()
            };
            overlays.push(overlay);
        }
        if let Some(hover) = self
            .page_text_hover
            .clone()
            .filter(|hover| hover.document_id == document_id && hover.editable)
            && self.page_text_editing.is_none_or(|editing| {
                (editing.page_index, editing.anchor) != (hover.page_index, hover.anchor)
            })
            && let Some((layer, blocks)) = self.page_text_blocks(document_id, hover.page_index, cx)
            && let Some(block) = blocks.iter().find(|block| block.lines[0] == hover.anchor)
        {
            let tables = self.shown_page_table_edits(document_id, hover.page_index, cx);
            let moves = self.shown_page_content_moves(document_id, hover.page_index, cx);
            let offset = block_offset(&layer, block, &tables.iter().collect::<Vec<_>>(), &moves)
                .unwrap_or((0., 0.));
            let quads = super::content::quads_bounds(&self.paragraph_line_quads(
                document_id,
                hover.page_index,
                &layer,
                block,
                cx,
            ))
            .map(|bounds| shifted(&bounds_quad(bounds), offset))
            .into_iter()
            .collect::<Vec<_>>();
            match overlays
                .iter_mut()
                .find(|overlay| overlay.page_index == hover.page_index)
            {
                Some(overlay) => overlay.hover = quads,
                None => overlays.push(PageTextOverlay {
                    page_index: hover.page_index,
                    hover: quads,
                    ..Default::default()
                }),
            }
        }
        // The selected content, and the drawing or image under the pointer.
        for (page, selected, hover, handles) in self.content_overlays(document_id, cx) {
            match overlays
                .iter_mut()
                .find(|overlay| overlay.page_index == page)
            {
                Some(overlay) => {
                    overlay.selected = selected;
                    overlay.hover.extend(hover);
                    overlay.handles = handles;
                }
                None => overlays.push(PageTextOverlay {
                    page_index: page,
                    selected,
                    hover,
                    handles,
                    ..Default::default()
                }),
            }
        }
        // Each page's tables that run into text or off the page.
        for (page_index, warning) in self.table_warnings(document_id, cx) {
            match overlays
                .iter_mut()
                .find(|overlay| overlay.page_index == page_index)
            {
                Some(overlay) => overlay.warnings.push(warning),
                None => overlays.push(PageTextOverlay {
                    page_index,
                    warnings: vec![warning],
                    ..Default::default()
                }),
            }
        }
        overlays
    }
}

/// A paragraph for typing into a cell with no base text: an empty line where
/// the cell's model cell has its first line, in its style, wrapping within
/// the cell. Returns the page text with the line added, the block and the
/// line.
pub(super) fn new_cell_paragraph(
    layer: &Arc<PageTextLayer>,
    blocks: &[PageTextBlock],
    view: &crate::table_editing::TableView,
    cell: &crate::table_editing::OutputCell,
) -> Option<(Arc<PageTextLayer>, PageTextBlock, PageTextLine)> {
    let model = view.model_cell(cell)?;
    let (dx, dy) = crate::table_editing::model_text_place(layer, &view.table, model)?;
    let model_line = layer.line(view.table.cells[model].lines.first()?)?;
    let model_block = blocks
        .iter()
        .find(|block| block.lines.contains(&model_line.anchor));
    let [left, _, right, top] = view.cell_bounds(cell);
    let model_bounds = view.table.cell_bounds(&view.table.cells[model]);
    let inset = crate::pdf_tables::cell_inset(layer, &view.table, model);
    let (pad_left, pad_right) = (inset.left - model_bounds[0], model_bounds[2] - inset.right);
    let origin = (left + dx, top - dy);
    let style = model_line
        .chars
        .iter()
        .find(|character| !character.character.is_whitespace())
        .map_or(0, |character| character.style);
    let mut line = model_line.clone();
    line.text.clear();
    line.chars.clear();
    line.end_x = 0.;
    line.read_only = None;
    line.styles = vec![model_line.styles.get(style)?.clone()];
    line.matrix[4] = origin.0;
    line.matrix[5] = origin.1;
    line.anchor = TextLineAnchor {
        x_centipoints: (origin.0 * 100.).round() as i64,
        y_centipoints: (origin.1 * 100.).round() as i64,
        ordinal: u32::MAX,
    };
    let alignment = match model_block.map(|block| block.alignment) {
        Some(crate::pdf_text_blocks::TextBlockAlignment::Justified) | None => {
            crate::pdf_text_blocks::TextBlockAlignment::Left
        }
        Some(alignment) => alignment,
    };
    let block = PageTextBlock {
        lines: vec![line.anchor],
        alignment,
        left: origin.0,
        right: right - pad_right,
        pitch: model_block.and_then(|block| block.pitch),
        edges: Vec::new(),
        list: None,
        cell: Some(crate::pdf_text_blocks::BlockCell {
            table: view.table.key(),
            cell: model,
            left: left + pad_left,
            right: right - pad_right,
        }),
    };
    let mut augmented = (**layer).clone();
    augmented.lines.push(line.clone());
    Some((Arc::new(augmented), block, line))
}

/// The block an editor's paragraph was opened from (a new cell's).
fn state_block(state: &PageTextEditState) -> PageTextBlock {
    let paragraph = state.paragraph();
    PageTextBlock {
        lines: paragraph.anchors(),
        alignment: paragraph.alignment(),
        left: 0.,
        right: 0.,
        pitch: None,
        edges: Vec::new(),
        list: paragraph.list(),
        cell: paragraph.cell(),
    }
}

/// How far table edits move a block on the page: by its first line's place.
/// `None` when a deleted row or column removes it.
fn block_offset(
    layer: &PageTextLayer,
    block: &PageTextBlock,
    tables: &[&crate::pdf_tables::PageTableEdit],
    moves: &[crate::pdf_content_moves::ContentMove],
) -> Option<(f64, f64)> {
    line_offset(layer.line(&block.lines[0])?, tables, moves)
}

/// How far table edits and moves move a line on the page.
pub(super) fn line_offset(
    line: &PageTextLine,
    tables: &[&crate::pdf_tables::PageTableEdit],
    moves: &[crate::pdf_content_moves::ContentMove],
) -> Option<(f64, f64)> {
    if tables.is_empty() && moves.is_empty() {
        return Some((0., 0.));
    }
    match crate::pdf_tables::line_core_box(line) {
        Some(rect) => crate::pdf_content_moves::line_offset(tables, moves, rect),
        None => Some((0., 0.)),
    }
}

/// How many pages either side of the visible ones keep their text.
const KEPT_PAGES_AROUND_VIEW: i64 = 8;

/// `bounds` (left, bottom, right, top) as a quad.
fn bounds_quad(bounds: [f64; 4]) -> Quad {
    [
        (bounds[0], bounds[1]),
        (bounds[2], bounds[1]),
        (bounds[2], bounds[3]),
        (bounds[0], bounds[3]),
    ]
}

/// Whether two page-space quads' bounding boxes overlap.
fn quads_overlap(a: &Quad, b: &Quad) -> bool {
    let bounds = |quad: &Quad| {
        let xs = quad.iter().map(|(x, _)| *x);
        let ys = quad.iter().map(|(_, y)| *y);
        (
            xs.clone().fold(f64::INFINITY, f64::min),
            ys.clone().fold(f64::INFINITY, f64::min),
            xs.fold(f64::NEG_INFINITY, f64::max),
            ys.fold(f64::NEG_INFINITY, f64::max),
        )
    };
    // The middle of each box: tightly set lines' full boxes touch.
    let core = |(x0, y0, x1, y1): (f64, f64, f64, f64)| {
        let inset = (y1 - y0) * 0.25;
        (x0, y0 + inset, x1, y1 - inset)
    };
    let (a, b) = (core(bounds(a)), core(bounds(b)));
    a.0 < b.2 && b.0 < a.2 && a.1 < b.3 && b.1 < a.3
}

macro_rules! forward_actions {
    ($element:expr, $input:expr, [$(($action:ty, $method:ident)),* $(,)?]) => {{
        let mut element = $element;
        $(
            let input = $input.clone();
            element = element.on_action(move |action: &$action, window, cx| {
                input.update(cx, |input, cx| input.$method(action, window, cx));
            });
        )*
        element
    }};
}

/// Draws a page's Edit PDF Content overlay: the hovered line, and the edited
/// line's frame, selection and caret.
pub(super) fn page_text_overlay_element(
    overlay: PageTextOverlay,
    page_size: (f32, f32),
    coordinate_space: PageCoordinateSpace,
    accent: gpui::Hsla,
    warning: super::tables::WarningColors,
) -> AnyElement {
    let labels = overlay
        .warnings
        .iter()
        .filter_map(|label| {
            super::tables::content_warning_label(
                label.clone(),
                page_size,
                coordinate_space,
                warning,
            )
        })
        .collect::<Vec<_>>();
    let painted = overlay;
    let layer = gpui::div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let Some((page_bounds, transform)) =
                    contained_page_bounds_for_space(bounds, page_size, coordinate_space)
                else {
                    return;
                };
                let project = |(x, y): (f64, f64)| {
                    let local = transform.point_to_local_pixels(PdfPoint { x, y });
                    point(
                        page_bounds.origin.x + px(local.x as f32),
                        page_bounds.origin.y + px(local.y as f32),
                    )
                };
                let fill_quad = |quad: &Quad, color: gpui::Hsla, window: &mut Window| {
                    let mut builder = PathBuilder::fill();
                    builder.move_to(project(quad[0]));
                    for corner in &quad[1..] {
                        builder.line_to(project(*corner));
                    }
                    builder.close();
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, color);
                    }
                };
                // A frame: the box round a quad on screen, a few pixels off
                // the content at any zoom.
                let frame_bounds = |quad: &Quad| {
                    let corners = quad.map(project);
                    let padding = px(super::content::FRAME_PADDING_PIXELS);
                    let min_x = corners
                        .iter()
                        .map(|corner| corner.x)
                        .fold(px(f32::MAX), Pixels::min);
                    let max_x = corners
                        .iter()
                        .map(|corner| corner.x)
                        .fold(px(f32::MIN), Pixels::max);
                    let min_y = corners
                        .iter()
                        .map(|corner| corner.y)
                        .fold(px(f32::MAX), Pixels::min);
                    let max_y = corners
                        .iter()
                        .map(|corner| corner.y)
                        .fold(px(f32::MIN), Pixels::max);
                    gpui::Bounds::from_corners(
                        point(min_x - padding, min_y - padding),
                        point(max_x + padding, max_y + padding),
                    )
                };
                let stroke_frame = |quad: &Quad, color: gpui::Hsla, window: &mut Window| {
                    window.paint_quad(gpui::outline(
                        frame_bounds(quad),
                        color,
                        gpui::BorderStyle::Solid,
                    ));
                };
                // Frames only: content keeps its own colours. Selected and
                // hovered content take markups' selection and hover colours.
                use super::interaction_chrome::{
                    ChromeState, hover_colour, paint_feedback_handle, selection_colour,
                };
                for quad in &painted.hover {
                    stroke_frame(quad, hover_colour(), window);
                }
                for quad in &painted.frame {
                    stroke_frame(quad, accent, window);
                }
                // The text a table or paragraph runs into, pointed out.
                for rect in painted.warnings.iter().flat_map(|label| &label.overlapped) {
                    stroke_frame(&bounds_quad(*rect), warning.icon, window);
                }
                for quad in &painted.selected {
                    stroke_frame(quad, selection_colour(), window);
                }
                // Resize handles: markups' handles, white squares filled
                // under the pointer, on the frame.
                let frame = painted.selected.first().map(frame_bounds);
                let pointer = window.mouse_position();
                for &(x, y) in &painted.handles {
                    let mut centre = project((x, y));
                    if let Some(frame) = frame {
                        let padding = px(super::content::FRAME_PADDING_PIXELS);
                        let middle = frame.center();
                        let outwards = |at: Pixels, middle: Pixels| {
                            if (at - middle).abs() < px(0.5) {
                                px(0.)
                            } else if at < middle {
                                -padding
                            } else {
                                padding
                            }
                        };
                        centre.x += outwards(centre.x, middle.x);
                        centre.y += outwards(centre.y, middle.y);
                    }
                    let reach = px(super::content::HANDLE_SIZE_PIXELS / 2. + 2.);
                    let hot = (pointer.x - centre.x).abs() <= reach
                        && (pointer.y - centre.y).abs() <= reach;
                    paint_feedback_handle(centre, ChromeState::Selected, hot, window);
                }
                for quad in &painted.selection {
                    fill_quad(quad, accent.opacity(0.3), window);
                }
                let mark_color = accent;
                let marks = painted
                    .breaks
                    .iter()
                    .map(|stroke| (stroke, mark_color))
                    .chain(
                        painted
                            .wraps
                            .iter()
                            .map(|stroke| (stroke, mark_color.opacity(0.45))),
                    );
                for (stroke, color) in marks {
                    let mut builder = PathBuilder::stroke(px(1.));
                    builder.move_to(project(stroke[0]));
                    for point in &stroke[1..] {
                        builder.line_to(project(*point));
                    }
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, color);
                    }
                }
                for [start, end] in &painted.marked {
                    // Composing text is underlined, as in other text fields.
                    let mut builder = PathBuilder::stroke(px(1.));
                    builder.move_to(project(*start));
                    builder.line_to(project(*end));
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, gpui::black());
                    }
                }
                if let Some([bottom, top]) = painted.caret {
                    let mut builder = PathBuilder::stroke(px(1.5));
                    builder.move_to(project(bottom));
                    builder.line_to(project(top));
                    if let Ok(path) = builder.build() {
                        // Paper-relative: the caret is drawn on the page.
                        window.paint_path(path, gpui::black());
                    }
                }
                if let Some((input, _)) = &painted.input {
                    // Page space to window pixels, for the IME; its text
                    // positions are the base lines', moved by any table
                    // edit.
                    let (dx, dy) = painted.input_offset;
                    let along = project((1. + dx, dy));
                    let up = project((dx, 1. + dy));
                    let base = project((dx, dy));
                    let screen = PageTextScreenMap {
                        matrix: [
                            f64::from(f32::from(along.x - base.x)),
                            f64::from(f32::from(along.y - base.y)),
                            f64::from(f32::from(up.x - base.x)),
                            f64::from(f32::from(up.y - base.y)),
                            f64::from(f32::from(base.x)),
                            f64::from(f32::from(base.y)),
                        ],
                    };
                    input.update(cx, |input, _| input.screen_map = Some(screen));
                }
            },
        )
        .absolute()
        .inset_0(),
    );
    layer.children(labels).into_any_element()
}

/// Receives the editor's keys and IME input. It sits outside the pages so
/// it stays mounted, and focused, while the pages re-render.
pub(super) fn page_text_input_element(
    input: Entity<PageTextInput>,
    focus: FocusHandle,
) -> AnyElement {
    let handler_input = input.clone();
    let handler_focus = focus.clone();
    let element = gpui::div()
        .id("document-page-text-editor")
        .absolute()
        .left_0()
        .top_0()
        .size(px(1.))
        .key_context(PAGE_TEXT_EDITOR_CONTEXT)
        .track_focus(&focus)
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, cx| {
                    window.handle_input(
                        &handler_focus,
                        ElementInputHandler::new(bounds, handler_input.clone()),
                        cx,
                    );
                },
            )
            .size_full(),
        );
    forward_actions!(
        element,
        input,
        [
            (Backspace, backspace),
            (DeleteForward, delete_forward),
            (DeleteWordBackward, delete_word_backward),
            (DeleteWordForward, delete_word_forward),
            (DeleteToLineStart, delete_to_line_start),
            (MoveLeft, move_left),
            (MoveRight, move_right),
            (MoveWordLeft, move_word_left),
            (MoveWordRight, move_word_right),
            (MoveLineStart, move_line_start),
            (MoveLineEnd, move_line_end),
            (MoveUp, move_up),
            (MoveDown, move_down),
            (SelectUp, select_up),
            (SelectDown, select_down),
            (InsertNewline, insert_newline),
            (SelectLeft, select_left),
            (SelectRight, select_right),
            (SelectWordLeft, select_word_left),
            (SelectWordRight, select_word_right),
            (SelectLineStart, select_line_start),
            (SelectLineEnd, select_line_end),
            (SelectAllText, select_all),
            (CopyText, copy),
            (CutText, cut),
            (PasteText, paste),
            (UndoText, undo),
            (RedoText, redo),
            (FinishEditing, finish),
            (NextCell, next_cell),
            (PreviousCell, previous_cell),
        ]
    )
    .into_any_element()
}
