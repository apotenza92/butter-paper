//! Tables with Edit PDF: moving between cells, and the warning a table
//! shows when its grown rows run into other text or off the page.

use super::page_text::TableEditingCell;
use super::*;
use crate::pdf_tables::TableKey;
use crate::pdf_text_blocks::PageTextBlock;
use crate::pdf_text_edit::{PageTextLayer, TextLineAnchor};
use crate::table_editing::TableView;

impl DocumentWorkspace {
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

/// A label under content that runs into other text or off the page, at
/// its left edge, so the page shows what is wrong without outlining it.
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
    let [left, bottom, right, top] = warning.bounds;
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
