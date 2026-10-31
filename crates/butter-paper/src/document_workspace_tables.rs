//! Tables with Edit PDF: moving between cells, and the warning a table
//! shows when its grown rows run into other text or off the page.

use super::page_text::TableEditingCell;
use super::*;
use crate::pdf_tables::TableKey;
use crate::pdf_text_blocks::PageTextBlock;
use crate::pdf_text_edit::{PageTextLayer, TextLineAnchor};
use crate::table_editing::TableView;

gpui::actions!(
    document_workspace,
    [
        /// Adds an empty row above the table row right-clicked in Edit PDF.
        InsertTableRowAbove,
        /// Adds an empty row below the table row right-clicked in Edit PDF.
        InsertTableRowBelow,
        /// Deletes the table row right-clicked in Edit PDF, with its text.
        DeleteTableRow
    ]
);

/// The table row a right-click in Edit PDF was on: what the page menu's
/// row items change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TableMenuTarget {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub key: TableKey,
    /// The row in the table as it is shown.
    pub row: usize,
}

/// How near a table's inner column border, in pixels, a press drags it.
const COLUMN_EDGE_PIXELS: f64 = 4.;

/// A table's inner column border being dragged: the table edit it began
/// with and the edit it shows now.
#[derive(Clone, Debug)]
pub(super) struct ColumnDrag {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub key: TableKey,
    /// The output column edge being moved.
    pub edge: usize,
    pub start_x: f64,
    pub base: crate::pdf_tables::PageTableEdit,
    pub edit: crate::pdf_tables::PageTableEdit,
    pub moved: bool,
}

#[derive(Clone, Copy)]
enum RowChange {
    Above,
    Below,
    Delete,
}

impl DocumentWorkspace {
    /// Notes the table row under a right-click at `position`, in Edit PDF
    /// mode, for the page menu's row items.
    pub(super) fn note_table_menu_target(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.table_menu_target = None;
        if !self.content_edit_mode(document_id) {
            return;
        }
        let Some(interaction) = self
            .page_interactions
            .get(&(document_id, page_index))
            .copied()
        else {
            return;
        };
        let Some(point) = Self::interaction_point(interaction, position, true) else {
            return;
        };
        let Some((_, _, tables)) = self.loaded_page_text(document_id, page_index) else {
            return;
        };
        let shown = self.shown_page_table_edits(document_id, page_index, cx);
        for table in tables.iter() {
            let key = table.key();
            let view = TableView::new(
                page_index,
                table.clone(),
                shown.iter().find(|edit| edit.key() == key),
            );
            if let Some(cell) = view.cell_at(point.x, point.y) {
                self.table_menu_target = Some(TableMenuTarget {
                    document_id,
                    page_index,
                    key,
                    row: cell.row,
                });
                return;
            }
        }
    }

    /// The right-clicked table row, and why its table's rows cannot change
    /// if they cannot.
    pub(super) fn table_menu_row(&self, document_id: DocumentId) -> Option<Option<String>> {
        let target = self
            .table_menu_target
            .filter(|target| target.document_id == document_id)?;
        let (_, _, tables) = self.loaded_page_text(document_id, target.page_index)?;
        let table = tables.iter().find(|table| table.key() == target.key)?;
        Some(crate::table_editing::row_change_blocked(table))
    }

    pub(super) fn insert_table_row_above_from_action(
        &mut self,
        _: &InsertTableRowAbove,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_table_row(RowChange::Above, cx);
    }

    pub(super) fn insert_table_row_below_from_action(
        &mut self,
        _: &InsertTableRowBelow,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_table_row(RowChange::Below, cx);
    }

    pub(super) fn delete_table_row_from_action(
        &mut self,
        _: &DeleteTableRow,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.change_table_row(RowChange::Delete, cx);
    }

    /// Adds or deletes the right-clicked row, as one undo step.
    fn change_table_row(&mut self, change: RowChange, cx: &mut Context<Self>) {
        let Some(target) = self.table_menu_target.take() else {
            return;
        };
        // Text being typed is kept first, so the row change follows it.
        self.commit_page_text_editing(cx);
        let document_id = target.document_id;
        let Some((layer, _, tables)) = self.loaded_page_text(document_id, target.page_index) else {
            return;
        };
        let Some(table) = tables.iter().find(|table| table.key() == target.key) else {
            return;
        };
        if let Some(reason) = crate::table_editing::row_change_blocked(table) {
            self.annotation_statuses.insert(document_id, reason);
            cx.notify();
            return;
        }
        let edit = self.page_table_edit(document_id, target.page_index, target.key, cx);
        let mut view = TableView::new(target.page_index, table.clone(), edit.as_ref());
        let changed = match change {
            RowChange::Above => {
                crate::table_editing::insert_row(&layer, &mut view, target.row, target.row);
                true
            }
            RowChange::Below => {
                crate::table_editing::insert_row(&layer, &mut view, target.row + 1, target.row);
                true
            }
            RowChange::Delete => crate::table_editing::delete_row(&mut view, target.row),
        };
        if !changed {
            return;
        }
        let page_index = target.page_index;
        let tables = vec![(target.key, Some(view.edit))];
        let result = self.update_annotation_history_with_result(
            document_id,
            cx,
            move |annotations, document_id| {
                annotations.set_document_page_content_edits(
                    document_id,
                    page_index,
                    &[],
                    Vec::new(),
                    tables,
                )
            },
        );
        if let Err(error) = result {
            self.annotation_statuses
                .insert(document_id, format!("The table was not changed: {error}"));
        }
        self.sync_document_edits(document_id, cx);
        cx.notify();
    }

    /// The inner column border of a table near `point`, whose columns can
    /// be resized: its table and output column edge.
    pub(super) fn table_column_edge_at(
        &self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &App,
    ) -> Option<(TableKey, usize)> {
        let (document_id, page_index) = (interaction.document_id, interaction.page_index);
        let tolerance = interaction
            .transform
            .tolerance_points(COLUMN_EDGE_PIXELS)
            .ok()?;
        let (_, _, tables) = self.loaded_page_text(document_id, page_index)?;
        let shown = self.shown_page_table_edits(document_id, page_index, cx);
        for table in tables.iter() {
            if crate::table_editing::column_change_blocked(table).is_some() {
                continue;
            }
            let key = table.key();
            let view = TableView::new(
                page_index,
                table.clone(),
                shown.iter().find(|edit| edit.key() == key),
            );
            let [_, bottom, _, top] = view.bounds();
            if point.y < bottom || point.y > top {
                continue;
            }
            let columns = view.columns();
            if let Some(edge) = (1..columns.len().saturating_sub(1))
                .find(|edge| (point.x - columns[*edge]).abs() <= tolerance)
            {
                return Some((key, edge));
            }
        }
        None
    }

    /// Starts dragging a table's inner column border, if `point` is on one.
    pub(super) fn begin_column_drag(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((key, edge)) = self.table_column_edge_at(interaction, point, cx) else {
            return false;
        };
        if self.hold_restricted_content(interaction.document_id, interaction.page_index, cx) {
            return true;
        }
        self.commit_page_text_editing(cx);
        let (document_id, page_index) = (interaction.document_id, interaction.page_index);
        let Some((_, _, tables)) = self.loaded_page_text(document_id, page_index) else {
            return false;
        };
        let Some(table) = tables.iter().find(|table| table.key() == key) else {
            return false;
        };
        let edit = self.page_table_edit(document_id, page_index, key, cx);
        let view = TableView::new(page_index, table.clone(), edit.as_ref());
        self.column_drag = Some(ColumnDrag {
            document_id,
            page_index,
            key,
            edge,
            start_x: point.x,
            base: view.edit.clone(),
            edit: view.edit,
            moved: false,
        });
        cx.notify();
        true
    }

    /// Moves the dragged column border with the pointer.
    pub(super) fn update_column_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.column_drag.as_ref() else {
            return false;
        };
        let (document_id, page_index) = (drag.document_id, drag.page_index);
        let Some(interaction) = self
            .page_interactions
            .get(&(document_id, page_index))
            .copied()
        else {
            return true;
        };
        let Some(point) = Self::interaction_point(interaction, position, false) else {
            return true;
        };
        let Some((layer, _, tables)) = self.loaded_page_text(document_id, page_index) else {
            return true;
        };
        let Some(drag) = self.column_drag.as_mut() else {
            return true;
        };
        let Some(table) = tables.iter().find(|table| table.key() == drag.key) else {
            return true;
        };
        let mut view = TableView::new(page_index, table.clone(), Some(&drag.base));
        crate::table_editing::resize_column(&layer, &mut view, drag.edge, point.x - drag.start_x);
        drag.moved |= view.edit != drag.edit;
        drag.edit = view.edit;
        self.sync_document_edits(document_id, cx);
        cx.notify();
        true
    }

    /// Ends a column border drag: the two columns' text rewraps at their
    /// new widths and rows grow to hold it, as one undo step.
    pub(super) fn finish_column_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.column_drag.take() else {
            return false;
        };
        if drag.moved {
            self.commit_table_rewrapped(drag.document_id, drag.page_index, drag.key, drag.edit, cx);
        }
        cx.notify();
        true
    }

    /// The dragged column border's table edit, shown while it is dragged.
    pub(super) fn column_drag_edit(
        &self,
        document_id: DocumentId,
    ) -> Option<crate::pdf_tables::PageTableEdit> {
        self.column_drag
            .as_ref()
            .filter(|drag| drag.document_id == document_id)
            .map(|drag| drag.edit.clone())
    }

    pub(super) fn column_drag_active(&self, document_id: DocumentId) -> bool {
        self.column_drag
            .as_ref()
            .is_some_and(|drag| drag.document_id == document_id)
    }

    /// Keeps table `key`'s `edit` with its cells' paragraphs wrapped at
    /// their columns' widths and its rows tall enough for them.
    fn commit_table_rewrapped(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        key: TableKey,
        edit: crate::pdf_tables::PageTableEdit,
        cx: &mut Context<Self>,
    ) {
        let Some((layer, blocks, tables)) = self.loaded_page_text(document_id, page_index) else {
            return;
        };
        let Some(table) = tables.iter().find(|table| table.key() == key) else {
            return;
        };
        let mut view = TableView::new(page_index, table.clone(), Some(&edit));
        let before = self.page_table_edit(document_id, page_index, key, cx);
        let before = TableView::new(page_index, table.clone(), before.as_ref());
        let mut anchors = Vec::new();
        let mut edits = Vec::new();
        for block in blocks.iter() {
            // Only cells whose width changed reflow; others keep their text.
            let Some(cell) = block.cell.filter(|cell| cell.table == key) else {
                continue;
            };
            let growth = crate::table_editing::cell_growth(&view, cell.cell);
            if (growth - crate::table_editing::cell_growth(&before, cell.cell)).abs() < 1e-6 {
                continue;
            }
            let mut block = block.clone();
            if let Some(cell) = block.cell.as_mut() {
                cell.right += growth;
            }
            let Some(paragraph) = crate::pdf_text_reflow::Paragraph::new(layer.clone(), &block)
                .map(crate::pdf_text_reflow::Paragraph::reflowed)
            else {
                continue;
            };
            let Ok(state) =
                self.paragraph_edit_state(document_id, page_index, paragraph, &block, None, cx)
            else {
                continue;
            };
            anchors.extend(block.lines.iter().copied());
            edits.extend(state.edits());
        }
        let mut shown = self
            .page_text_edits_of(document_id, cx)
            .into_iter()
            .filter(|edit| edit.page_index == page_index && !anchors.contains(&edit.anchor))
            .collect::<Vec<_>>();
        shown.extend(edits.iter().cloned());
        crate::table_editing::fit_rows(&layer, &mut view, &shown);
        let tables = vec![(key, Some(view.edit))];
        let result = self.update_annotation_history_with_result(
            document_id,
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
            self.annotation_statuses
                .insert(document_id, format!("The table was not changed: {error}"));
        }
        self.sync_document_edits(document_id, cx);
    }

    /// The table `key` on a loaded page, as the document edits it.
    pub(super) fn page_table_view(
        &self,
        document_id: DocumentId,
        page_index: u32,
        key: TableKey,
        cx: &App,
    ) -> Option<TableView> {
        let (_, _, tables) = self.loaded_page_text(document_id, page_index)?;
        let table = tables.iter().find(|table| table.key() == key)?;
        let edit = self.page_table_edit(document_id, page_index, key, cx);
        // A table whose rows are all deleted is gone.
        let view = TableView::new(page_index, table.clone(), edit.as_ref());
        (!view.edit.rows.is_empty()).then_some(view)
    }

    /// The cell being edited, if any, and its table's page.
    pub(super) fn editing_table_cell(&self) -> Option<(DocumentId, u32, TableEditingCell)> {
        let editing = self.page_text_editing?;
        Some((editing.document_id, editing.page_index, editing.table?))
    }
}

/// Why an edited table or paragraph is shown with a warning: content
/// outside it never moves, so the user moves what is in the way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ContentWarningKind {
    /// It runs past the page's bottom edge.
    PastPage,
    /// It runs into other text on the page.
    Overlaps,
}

impl ContentWarningKind {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::PastPage => "Runs off the bottom of the page",
            Self::Overlaps => "Overlaps other text",
        }
    }
}

/// A warning label under edited content: the content's page-space bounds
/// (left, bottom, right, top), why, and the other text it covers, which is
/// framed in the warning colour (page-space boxes, where it is drawn).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ContentWarning {
    pub bounds: [f64; 4],
    pub kind: ContentWarningKind,
    pub overlapped: Vec<[f64; 4]>,
}

impl DocumentWorkspace {
    /// The warnings each visible page shows for its tables: every edited
    /// table, and the one being edited, that runs into text or off the page.
    pub(super) fn table_warnings(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<(u32, ContentWarning)> {
        if !self.editing_content(document_id, cx) {
            return Vec::new();
        }
        let visible = self
            .session(document_id, cx)
            .and_then(|session| {
                session
                    .read(cx)
                    .viewer
                    .plan_snapshot()
                    .map(|plan| plan.visible_pages.to_vec())
            })
            .unwrap_or_default();
        let mut tables = Vec::new();
        if let Some((document, page, cell)) = self.editing_table_cell()
            && document == document_id
        {
            tables.push((page, cell.key));
        }
        for edit in self.page_table_edits_of(document_id, cx) {
            let shown = visible
                .iter()
                .any(|page| u32::try_from(*page).ok() == Some(edit.page_index));
            if shown && !tables.contains(&(edit.page_index, edit.key())) {
                tables.push((edit.page_index, edit.key()));
            }
        }
        tables
            .into_iter()
            .filter_map(|(page_index, key)| {
                let (kind, overlapped) =
                    self.table_overlaps_below(document_id, page_index, key, cx)?;
                // As shown: while a cell is typed in, its live edit.
                let (_, _, tables) = self.loaded_page_text(document_id, page_index)?;
                let table = tables.iter().find(|table| table.key() == key)?;
                let edit = self
                    .shown_page_table_edits(document_id, page_index, cx)
                    .into_iter()
                    .find(|edit| edit.key() == key);
                let view = TableView::new(page_index, table.clone(), edit.as_ref());
                Some((
                    page_index,
                    ContentWarning {
                        bounds: view.bounds(),
                        kind,
                        overlapped,
                    },
                ))
            })
            .collect()
    }
}

/// The theme colours of a warning label: a popover's, with a warning icon.
#[derive(Clone, Copy, Debug)]
pub(super) struct WarningColors {
    surface: gpui::Hsla,
    border: gpui::Hsla,
    text: gpui::Hsla,
    /// The icon's, and the frames round the text it overlaps.
    pub icon: gpui::Hsla,
}

impl WarningColors {
    pub(super) fn new(cx: &App) -> Self {
        let theme = cx.theme();
        Self {
            surface: theme.popover,
            border: theme.border,
            text: theme.popover_foreground,
            icon: theme.warning,
        }
    }
}

/// How far a warning label sits below its content, in pixels, and the room
/// it needs there: with less room above the page's bottom edge, it sits just
/// inside that edge instead.
const WARNING_LABEL_GAP_PX: f32 = 4.;
const WARNING_LABEL_ROOM_PX: f32 = 30.;

/// A label for content that runs into other text or off the page, so the
/// page shows what is wrong without outlining it: under the text it runs
/// into, at that text's left edge, or under content that runs off the page.
/// It takes no presses: the page under it edits as before.
pub(super) fn content_warning_label(
    warning: ContentWarning,
    page_size: (f32, f32),
    coordinate_space: PageCoordinateSpace,
    colors: WarningColors,
) -> Option<gpui::AnyElement> {
    let layer_bounds = Bounds::new(
        point(px(0.), px(0.)),
        size(px(page_size.0), px(page_size.1)),
    );
    let (page_bounds, transform) =
        contained_page_bounds_for_space(layer_bounds, page_size, coordinate_space)?;
    // Page points as fractions of the page layer, which is drawn scaled.
    let kind = warning.kind;
    // Under what is wrong: the text it runs into, or its own bottom edge.
    let anchor = match kind {
        ContentWarningKind::Overlaps => warning
            .overlapped
            .iter()
            .copied()
            .reduce(|a, b| {
                [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ]
            })
            .unwrap_or(warning.bounds),
        ContentWarningKind::PastPage => warning.bounds,
    };
    let [left, bottom, right, top] = anchor;
    let corners = [(left, bottom), (right, bottom), (left, top), (right, top)].map(|(x, y)| {
        let local = transform.point_to_local_pixels(PdfPoint { x, y });
        (
            (f32::from(page_bounds.origin.x) + local.x as f32) / page_size.0,
            (f32::from(page_bounds.origin.y) + local.y as f32) / page_size.1,
        )
    });
    let page_left = f32::from(page_bounds.origin.x) / page_size.0;
    let page_right = f32::from(page_bounds.origin.x + page_bounds.size.width) / page_size.0;
    let page_bottom = f32::from(page_bounds.origin.y + page_bounds.size.height) / page_size.1;
    let x = corners
        .iter()
        .map(|corner| corner.0)
        .fold(f32::MAX, f32::min)
        .clamp(page_left, page_right);
    let y = corners
        .iter()
        .map(|corner| corner.1)
        .fold(f32::MIN, f32::max)
        .min(page_bottom);
    let label = h_flex()
        .gap_1()
        .items_center()
        .px_1p5()
        .py_0p5()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.surface)
        .text_color(colors.text)
        .shadow_sm()
        .text_xs()
        .whitespace_nowrap()
        .debug_selector(move || format!("content-warning-{kind:?}"))
        .child(
            gpui_component::Icon::new(IconName::TriangleAlert)
                .xsmall()
                .text_color(colors.icon),
        )
        .child(kind.message());
    // The room from the content down to the page's bottom edge, at least
    // the label's: so with too little room, it ends at the edge.
    Some(
        gpui::div()
            .absolute()
            .left(relative(x))
            .bottom(relative(1. - page_bottom))
            .h(relative(page_bottom - y))
            .min_h(px(WARNING_LABEL_ROOM_PX))
            .pt(px(WARNING_LABEL_GAP_PX))
            .child(label)
            .into_any_element(),
    )
}

impl DocumentWorkspace {
    /// Tab in a table cell: finishes it and edits the next cell (Shift-Tab,
    /// the previous), its text selected; Tab in the last cell, and
    /// Shift-Tab in the first, stay. Outside a table, Tab types a space.
    pub(super) fn move_to_next_table_cell(
        &mut self,
        back: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((document_id, page_index, cell)) = self.editing_table_cell() else {
            if let Some((input, _)) = &self.page_text_input {
                input.update(cx, |input, cx| input.type_text(" ", cx));
            }
            return;
        };
        let Some(view) = self.page_table_view(document_id, page_index, cell.key, cx) else {
            return;
        };
        let cells = view.cells();
        let Some(current) = cells
            .iter()
            .position(|output| (output.row, output.column) == (cell.row, cell.column))
        else {
            return;
        };
        let target = if back {
            current.checked_sub(1)
        } else {
            Some(current + 1).filter(|next| *next < cells.len())
        };
        // At either end the cell stays edited: the table gains no row.
        let Some(target) = target.map(|target| cells[target]) else {
            return;
        };
        self.commit_page_text_editing(cx);
        // The finished edit may have grown rows: the cell as it is now.
        let Some(view) = self.page_table_view(document_id, page_index, cell.key, cx) else {
            return;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(document_id, page_index))
            .copied()
        else {
            return;
        };
        // Press where the cell's text starts, or its middle when it has none.
        let shown = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown.iter().collect::<Vec<_>>();
        let point = target
            .base
            .filter(|_| view.has_base_text(&target))
            .and_then(|base| {
                let (layer, _, _) = self.loaded_page_text(document_id, page_index)?;
                let line = layer.line(view.table.cells[base].lines.first()?)?;
                let rect = crate::pdf_tables::line_core_box(line)?;
                let moves = self.shown_page_content_moves(document_id, page_index, cx);
                let (dx, dy) = crate::pdf_content_moves::line_offset(&refs, &moves, rect)?;
                Some(PdfPoint {
                    x: (rect[0] + rect[2]) / 2. + dx,
                    y: (rect[1] + rect[3]) / 2. + dy,
                })
            })
            .unwrap_or_else(|| {
                let [left, bottom, right, top] = view.cell_bounds(&target);
                PdfPoint {
                    x: (left + right) / 2.,
                    y: (top + bottom) / 2.,
                }
            });
        self.begin_page_text_press(interaction, point, 1, Modifiers::default(), window, cx);
        self.finish_page_text_drag(cx);
        if let Some((input, _)) = &self.page_text_input {
            input.update(cx, |input, cx| input.select_all_text(cx));
        }
    }

    /// Backspace in an empty paragraph: editing continues at the end of the
    /// paragraph above it in its cell (outside tables, the one above it).
    pub(super) fn join_previous_paragraph(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing) = self.page_text_editing else {
            return;
        };
        let (document_id, page_index) = (editing.document_id, editing.page_index);
        let Some((layer, blocks, _)) = self.loaded_page_text(document_id, page_index) else {
            return;
        };
        let Some(current) = blocks
            .iter()
            .find(|block| block.lines.first() == Some(&editing.anchor))
        else {
            return;
        };
        let baseline = |layer: &PageTextLayer, anchor: Option<&TextLineAnchor>| {
            anchor
                .and_then(|anchor| layer.line(anchor))
                .map(|line| line.origin().1)
        };
        let Some(top) = baseline(&layer, current.lines.first()) else {
            return;
        };
        let overlaps =
            |block: &PageTextBlock| block.left < current.right && current.left < block.right;
        let cell_of = |block: &PageTextBlock| block.cell.map(|cell| (cell.table, cell.cell));
        // The nearest paragraph above, by its last line.
        let lowest = |layer: &PageTextLayer,
                      blocks: &mut dyn Iterator<Item = &PageTextBlock>,
                      below: f64| {
            blocks
                .filter_map(|block| Some((baseline(layer, block.lines.last())?, block)))
                .filter(|(bottom, _)| *bottom > below + 0.5)
                .min_by(|a, b| a.0.total_cmp(&b.0))
                .and_then(|(_, block)| block.lines.first().copied())
        };
        let above = lowest(
            &layer,
            &mut blocks
                .iter()
                .filter(|block| cell_of(block) == cell_of(current) && overlaps(block)),
            top,
        );
        if let Some(anchor) = above {
            self.commit_page_text_editing(cx);
            self.edit_paragraph_end(document_id, page_index, anchor, window, cx);
        }
    }

    /// Starts editing the paragraph whose first line is `anchor`, with the
    /// caret at its end.
    fn edit_paragraph_end(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        anchor: TextLineAnchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(interaction) = self
            .page_interactions
            .get(&(document_id, page_index))
            .copied()
        else {
            return;
        };
        let Some((layer, _, _)) = self.loaded_page_text(document_id, page_index) else {
            return;
        };
        let Some(rect) = layer
            .line(&anchor)
            .and_then(crate::pdf_tables::line_core_box)
        else {
            return;
        };
        let shown = self.shown_page_table_edits(document_id, page_index, cx);
        let refs = shown.iter().collect::<Vec<_>>();
        let moves = self.shown_page_content_moves(document_id, page_index, cx);
        let Some((dx, dy)) = crate::pdf_content_moves::line_offset(&refs, &moves, rect) else {
            return;
        };
        let point = PdfPoint {
            x: (rect[0] + rect[2]) / 2. + dx,
            y: (rect[1] + rect[3]) / 2. + dy,
        };
        self.begin_page_text_press(interaction, point, 1, Modifiers::default(), window, cx);
        self.finish_page_text_drag(cx);
        if let Some((input, _)) = &self.page_text_input {
            input.update(cx, |input, cx| input.move_to_end(cx));
        }
    }
}
