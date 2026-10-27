//! Editing a table with Edit PDF: the table as an edit shows it
//! ([`TableView`]), rows that grow to hold their cells' text, and rows
//! added and deleted.
//!
//! Cells keep their text where it is: a cell's base lines are edited as
//! paragraphs that wrap at the cell's width, and text typed into a cell the
//! base table has no text in (an empty cell, or a new row's or column's) is
//! kept with the table edit ([`TableCellText`]). A row is as tall as its
//! base height, or as its tallest cell's text needs.

use crate::pdf_tables::{
    BandSource, PageTable, PageTableEdit, TableCellLine, TableCellText, cell_inset, line_box,
};
use crate::pdf_text_edit::{PageTextEdit, PageTextLayer, TextLineAnchor, extra_line_base};

/// A cell of a table as edited: its first row and column and its spans in
/// the edited table, and the base cell it shows (none for a new row's or
/// column's cell).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputCell {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    pub base: Option<usize>,
}

/// A table and its edit (the table as it is when none).
#[derive(Clone, Debug)]
pub struct TableView {
    pub table: PageTable,
    pub edit: PageTableEdit,
}

fn source_index(source: BandSource) -> usize {
    match source {
        BandSource::Base(index) | BandSource::New(index) => index,
    }
}

impl TableView {
    /// The table as `edit` changes it.
    pub fn new(page_index: u32, table: PageTable, edit: Option<&PageTableEdit>) -> Self {
        let edit = edit
            .cloned()
            .unwrap_or_else(|| PageTableEdit::unchanged(page_index, &table));
        Self { table, edit }
    }

    pub fn rows(&self) -> Vec<f64> {
        self.edit.output_rows()
    }

    pub fn columns(&self) -> Vec<f64> {
        self.edit.output_columns()
    }

    /// Left, bottom, right, top of the edited table.
    pub fn bounds(&self) -> [f64; 4] {
        let rows = self.rows();
        let columns = self.columns();
        [
            columns[0],
            rows[rows.len() - 1],
            columns[columns.len() - 1],
            rows[0],
        ]
    }

    /// The edited table's cells, by row then column. A base cell stays one
    /// cell where its rows and columns stay together; a new row's or
    /// column's units are cells of their own.
    pub fn cells(&self) -> Vec<OutputCell> {
        let (rows, columns) = (&self.edit.rows, &self.edit.columns);
        let base_unit = |row: usize, column: usize| match (rows[row].source, columns[column].source)
        {
            (BandSource::Base(r), BandSource::Base(c)) => self.table.cell_at_unit(r, c),
            _ => None,
        };
        let unit = |row: usize, column: usize| row * columns.len() + column;
        let mut parent = (0..rows.len() * columns.len()).collect::<Vec<_>>();
        fn find(parent: &mut [usize], mut index: usize) -> usize {
            while parent[index] != index {
                parent[index] = parent[parent[index]];
                index = parent[index];
            }
            index
        }
        let join = |parent: &mut Vec<usize>, a: usize, b: usize| {
            let (a, b) = (find(parent, a), find(parent, b));
            if a != b {
                parent[a.max(b)] = a.min(b);
            }
        };
        for row in 0..rows.len() {
            for column in 0..columns.len() {
                let Some(cell) = base_unit(row, column) else {
                    continue;
                };
                if column + 1 < columns.len()
                    && base_unit(row, column + 1) == Some(cell)
                    && source_index(columns[column + 1].source)
                        == source_index(columns[column].source) + 1
                {
                    join(&mut parent, unit(row, column), unit(row, column + 1));
                }
                if row + 1 < rows.len()
                    && base_unit(row + 1, column) == Some(cell)
                    && source_index(rows[row + 1].source) == source_index(rows[row].source) + 1
                {
                    join(&mut parent, unit(row, column), unit(row + 1, column));
                }
            }
        }
        let mut groups: std::collections::BTreeMap<usize, Vec<(usize, usize)>> =
            std::collections::BTreeMap::new();
        for row in 0..rows.len() {
            for column in 0..columns.len() {
                groups
                    .entry(find(&mut parent, unit(row, column)))
                    .or_default()
                    .push((row, column));
            }
        }
        let mut cells = Vec::new();
        for units in groups.into_values() {
            let top = units.iter().map(|unit| unit.0).min().unwrap_or(0);
            let bottom = units.iter().map(|unit| unit.0).max().unwrap_or(0);
            let left = units.iter().map(|unit| unit.1).min().unwrap_or(0);
            let right = units.iter().map(|unit| unit.1).max().unwrap_or(0);
            if (bottom - top + 1) * (right - left + 1) == units.len() {
                cells.push(OutputCell {
                    row: top,
                    column: left,
                    row_span: bottom - top + 1,
                    column_span: right - left + 1,
                    base: base_unit(top, left),
                });
            } else {
                // A merged cell split unevenly by moved bands: each unit
                // is a cell.
                for (row, column) in units {
                    cells.push(OutputCell {
                        row,
                        column,
                        row_span: 1,
                        column_span: 1,
                        base: base_unit(row, column),
                    });
                }
            }
        }
        cells.sort_by_key(|cell| (cell.row, cell.column));
        cells
    }

    pub fn cell_bounds(&self, cell: &OutputCell) -> [f64; 4] {
        let rows = self.rows();
        let columns = self.columns();
        [
            columns[cell.column],
            rows[cell.row + cell.row_span],
            columns[cell.column + cell.column_span],
            rows[cell.row],
        ]
    }

    /// The cell at page point (`x`, `y`) of the edited table.
    pub fn cell_at(&self, x: f64, y: f64) -> Option<OutputCell> {
        self.cells().into_iter().find(|cell| {
            let [left, bottom, right, top] = self.cell_bounds(cell);
            x >= left && x <= right && y >= bottom && y <= top
        })
    }

    /// The output cell showing base cell `base`.
    pub fn cell_of_base(&self, base: usize) -> Option<OutputCell> {
        self.cells()
            .into_iter()
            .find(|cell| cell.base == Some(base))
    }

    /// Whether a cell has text in the base table: its text is edited as
    /// the base table's lines. Other cells' text is the table edit's own.
    pub fn has_base_text(&self, cell: &OutputCell) -> bool {
        cell.base
            .is_some_and(|base| !self.table.cells[base].lines.is_empty())
    }

    /// The new text of output cell (`row`, `column`).
    pub fn cell_text(&self, row: usize, column: usize) -> Option<&TableCellText> {
        self.edit
            .cell_texts
            .iter()
            .find(|text| text.row == row && text.column == column)
    }

    /// The base cell whose text a new cell is laid out like: the base cell
    /// its row and column model, or the nearest with text.
    pub fn model_cell(&self, cell: &OutputCell) -> Option<usize> {
        let row = source_index(self.edit.rows[cell.row].source);
        let column = source_index(self.edit.columns[cell.column].source);
        let with_text = |index: &usize| !self.table.cells[*index].lines.is_empty();
        if let Some(model) = self.table.cell_at_unit(row, column).filter(with_text) {
            return Some(model);
        }
        // The nearest cell with text: in the column, then anywhere.
        let distance = |index: usize| {
            let model = &self.table.cells[index];
            (model.column != column) as usize * 10_000
                + model.row.abs_diff(row) * 100
                + model.column.abs_diff(column)
        };
        (0..self.table.cells.len())
            .filter(with_text)
            .min_by_key(|index| distance(*index))
    }
}

/// Why `table`'s rows cannot be added or deleted, if they cannot: only a
/// table the document declares, wholly on its page, without merged cells,
/// keeps its structure true to its rows.
pub fn row_change_blocked(table: &PageTable) -> Option<String> {
    let Some(tags) = &table.tags else {
        return Some("The document does not declare this table".into());
    };
    if tags.continues {
        return Some("The table continues on another page".into());
    }
    if table
        .cells
        .iter()
        .any(|cell| cell.row_span != 1 || cell.column_span != 1)
    {
        return Some("The table has merged cells".into());
    }
    table
        .read_only
        .as_ref()
        .map(|reason| format!("The table's rows cannot change: {reason}"))
}

/// Why `table`'s columns cannot be resized, if they cannot: a table that
/// continues on another page keeps its columns as they are there.
pub fn column_change_blocked(table: &PageTable) -> Option<String> {
    let Some(tags) = &table.tags else {
        return Some("The document does not declare this table".into());
    };
    if tags.continues {
        return Some("The table continues on another page".into());
    }
    table
        .read_only
        .as_ref()
        .map(|reason| format!("The table's columns cannot change: {reason}"))
}

/// How much wider `view`'s edit makes the columns base cell `base` spans.
pub fn cell_growth(view: &TableView, base: usize) -> f64 {
    let Some(output) = view.cell_of_base(base) else {
        return 0.;
    };
    let cell = &view.table.cells[base];
    let base_width =
        view.table.columns[cell.column + cell.column_span] - view.table.columns[cell.column];
    let bounds = view.cell_bounds(&output);
    bounds[2] - bounds[0] - base_width
}

/// The narrowest base column `column` may be: its widest word with its
/// cells' gaps either side, so every word still fits on a line.
fn column_min_width(layer: &PageTextLayer, table: &PageTable, column: usize) -> f64 {
    let mut narrowest: f64 = 0.;
    for (index, cell) in table.cells.iter().enumerate() {
        if cell.column != column || cell.column_span != 1 || cell.lines.is_empty() {
            continue;
        }
        let bounds = table.cell_bounds(cell);
        let inset = cell_inset(layer, table, index);
        let gaps = (inset.left - bounds[0]) + (bounds[2] - inset.right);
        for line in cell.lines.iter().filter_map(|anchor| layer.line(anchor)) {
            let scale = line.matrix[0].hypot(line.matrix[1]) * line.horizontal_scale;
            let mut word: f64 = 0.;
            for character in &line.chars {
                if character.character.is_whitespace() {
                    narrowest = narrowest.max(word * scale + gaps);
                    word = 0.;
                } else {
                    word += character.advance;
                }
            }
            narrowest = narrowest.max(word * scale + gaps);
        }
    }
    narrowest
}

/// Moves the border between output columns `edge - 1` and `edge` by
/// `delta` points, keeping the table's width: one column widens as the
/// other narrows, neither below its widest word. The delta it moved by.
pub fn resize_column(layer: &PageTextLayer, view: &mut TableView, edge: usize, delta: f64) -> f64 {
    if edge == 0 || edge >= view.edit.columns.len() {
        return 0.;
    }
    let (left, right) = (view.edit.columns[edge - 1], view.edit.columns[edge]);
    let min = |band: crate::pdf_tables::TableBand| {
        column_min_width(layer, &view.table, source_index(band.source))
    };
    let low = (min(left) - left.size).min(0.);
    let high = (right.size - min(right)).max(0.);
    let delta = delta.clamp(low, high);
    view.edit.columns[edge - 1].size += delta;
    view.edit.columns[edge].size -= delta;
    delta
}

/// A new empty row's height, drawn like base row `model`: one line of its
/// first text, with that cell's gaps above and below; the row's own height
/// when it has no text.
fn new_row_size(layer: &PageTextLayer, table: &PageTable, model: usize) -> f64 {
    let row_height = table.rows[model] - table.rows[model + 1];
    table
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.row == model && cell.row_span == 1)
        .find_map(|(index, cell)| {
            let first = layer.line(cell.lines.first()?).and_then(line_box)?;
            let bounds = table.cell_bounds(cell);
            let inset = cell_inset(layer, table, index);
            let top_gap = (bounds[3] - first[3]).max(0.);
            Some(top_gap + (first[3] - first[1]) + inset.bottom_gap)
        })
        .map_or(row_height, |size| size.min(row_height))
}

/// Adds an empty row at output row `index` (0 to the row count), drawn like
/// output row `model`. Text typed into new cells below moves down with its
/// row.
pub fn insert_row(layer: &PageTextLayer, view: &mut TableView, index: usize, model: usize) {
    let index = index.min(view.edit.rows.len());
    let Some(model_band) = view.edit.rows.get(model) else {
        return;
    };
    let base = source_index(model_band.source);
    // Set to its line's height, so fitting rows starts from it rather than
    // its model's; text typed into it still grows it.
    let size = new_row_size(layer, &view.table, base);
    view.edit.rows.insert(
        index,
        crate::pdf_tables::TableBand {
            source: BandSource::New(base),
            size,
            set_size: Some(size),
        },
    );
    for text in &mut view.edit.cell_texts {
        if text.row >= index {
            text.row += 1;
        }
    }
}

/// Deletes output row `index` with its text. A table keeps its last row.
pub fn delete_row(view: &mut TableView, index: usize) -> bool {
    if view.edit.rows.len() <= 1 || index >= view.edit.rows.len() {
        return false;
    }
    view.edit.rows.remove(index);
    view.edit.cell_texts.retain(|text| text.row != index);
    for text in &mut view.edit.cell_texts {
        if text.row > index {
            text.row -= 1;
        }
    }
    true
}

/// The lowest point of `anchors`' text as `edits` draw it (lines they add
/// below included), in base page space; `None` when no text is left. Blank
/// lines an edit draws after a line break count once the text shows
/// anything, so a line break typed at the end takes room before its line
/// has text; lines an edit empties take none. Text emptied entirely keeps
/// its first line, which holds the caret.
pub fn text_bottom(
    layer: &PageTextLayer,
    anchors: &[TextLineAnchor],
    edits: &[PageTextEdit],
) -> Option<f64> {
    let mut bottom: Option<f64> = None;
    let mut blank = f64::MAX;
    let mut emptied: Option<f64> = None;
    let mut broke = false;
    let mut lower =
        |value: f64| bottom = Some(bottom.map_or(value, |bottom: f64| bottom.min(value)));
    for anchor in anchors {
        let Some(line) = layer.line(anchor) else {
            continue;
        };
        let edit = edits
            .iter()
            .find(|edit| edit.page_index == layer.page_index && edit.anchor == *anchor);
        let visible = match edit {
            Some(edit) => edit.runs.iter().any(|run| !run.text.trim().is_empty()),
            None => !line.text.trim().is_empty(),
        };
        let lowest = |line: &crate::pdf_text_edit::PageTextLine| {
            line.quad(0., line.end_x.max(1.))
                .iter()
                .map(|point| point.1)
                .fold(f64::MAX, f64::min)
        };
        if visible {
            lower(lowest(line));
        } else if edit.is_some() {
            if broke {
                blank = blank.min(lowest(line));
            }
            emptied = Some(emptied.map_or(lowest(line), |top: f64| top.max(lowest(line))));
        }
        broke = edit.is_some_and(|edit| edit.layout.hard_break);
        for extra in edit.iter().flat_map(|edit| &edit.layout.extra_lines) {
            let extra_bottom = lowest(&extra_line_base(line, extra.below));
            if extra.runs.iter().any(|run| !run.text.trim().is_empty()) {
                lower(extra_bottom);
            } else {
                blank = blank.min(extra_bottom);
            }
            broke = extra.hard_break;
        }
    }
    bottom.map(|bottom| bottom.min(blank)).or(emptied)
}

/// How tall new cell text is from its cell's top: its last line's baseline
/// and the descent below it. Blank lines count once the text shows
/// anything, at its largest size.
fn cell_text_height(text: &TableCellText) -> Option<f64> {
    let size = |line: &TableCellLine| {
        line.runs
            .iter()
            .map(|run| run.style.font_size.abs())
            .fold(0., f64::max)
    };
    let visible = |line: &&TableCellLine| line.runs.iter().any(|run| !run.text.trim().is_empty());
    let largest = text
        .lines
        .iter()
        .filter(visible)
        .map(size)
        .reduce(f64::max)?;
    text.lines
        .iter()
        .map(|line| {
            let size = if visible(&line) { size(line) } else { largest };
            line.baseline + 0.25 * size
        })
        .reduce(f64::max)
}

/// Sets each row's height to hold its cells' text as `edits` and the
/// edit's new cell text draw it: a row is as tall as its base (or model)
/// row, or as its tallest cell's text and the gap the cell keeps below it
/// need. A row whose text takes less room than its base text did is
/// shorter by the difference, keeping any room it had to spare. Columns
/// keep their widths.
pub fn fit_rows(layer: &PageTextLayer, view: &mut TableView, edits: &[PageTextEdit]) {
    let cells = view.cells();
    let base_height = |source: BandSource| {
        let index = source_index(source);
        view.table.rows[index] - view.table.rows[index + 1]
    };
    // What each cell's text needs from its top, as `edits` draw it.
    let need = |cell: &OutputCell, edits: &[PageTextEdit]| {
        let base = cell.base.expect("a cell with base text");
        let model = &view.table.cells[base];
        let top = view.table.rows[model.row];
        let gap = cell_inset(layer, &view.table, base).bottom_gap;
        text_bottom(layer, &model.lines, edits).map(|bottom| top - bottom + gap)
    };
    let base_needs = cells
        .iter()
        .map(|cell| view.has_base_text(cell).then(|| need(cell, &[])).flatten())
        .collect::<Vec<_>>();
    let needs = cells
        .iter()
        .map(|cell| {
            if view.has_base_text(cell) {
                need(cell, edits)
            } else {
                let gap = view
                    .model_cell(cell)
                    .map_or(2., |model| cell_inset(layer, &view.table, model).bottom_gap);
                view.cell_text(cell.row, cell.column)
                    .and_then(cell_text_height)
                    .map(|height| height + gap)
            }
        })
        .collect::<Vec<_>>();
    let mut sizes = Vec::with_capacity(view.edit.rows.len());
    for row in 0..view.edit.rows.len() {
        let base = base_height(view.edit.rows[row].source);
        // A row dragged to a height starts from it, not the base.
        let set = view.edit.rows[row].set_size;
        let mut size = set.unwrap_or(base);
        // The tallest text of the row's own cells, now and in the base.
        let (mut tallest, mut base_tallest) = (None::<f64>, None::<f64>);
        for ((cell, need), base_need) in cells.iter().zip(&needs).zip(&base_needs) {
            let Some(need) = need else {
                continue;
            };
            if cell.row + cell.row_span - 1 != row {
                continue;
            }
            let above: f64 = sizes[cell.row..row].iter().sum();
            // What the measure finds beyond the base row, text and gap as
            // drawn, is not room the text needs.
            let over = match base_need {
                Some(base_need) if cell.row_span == 1 => (base_need - base).max(0.),
                _ => 0.,
            };
            size = size.max(need - over - above);
            if cell.row_span == 1 {
                tallest = Some(tallest.map_or(*need, |tallest| tallest.max(*need)));
                if let Some(base_need) = base_need {
                    base_tallest =
                        Some(base_tallest.map_or(*base_need, |tallest| tallest.max(*base_need)));
                }
            }
        }
        if let (Some(tallest), Some(base_tallest)) = (tallest, base_tallest)
            && set.is_none()
            && tallest < base_tallest - 0.5
            && !spanned(&cells, row)
        {
            size = (base - (base_tallest - tallest)).max(tallest);
        }
        sizes.push(size);
    }
    for (band, size) in view.edit.rows.iter_mut().zip(sizes) {
        band.size = (size * 100.).round() / 100.;
    }
}

/// Whether a cell spanning several rows ends at output row `row`.
fn spanned(cells: &[OutputCell], row: usize) -> bool {
    cells
        .iter()
        .any(|cell| cell.row_span > 1 && cell.row + cell.row_span - 1 == row)
}

/// The lines of new cell text, from lines laid out in page space: each
/// line's start and baseline, measured from the cell's top left corner.
pub fn cell_text_lines(
    cell_origin: (f64, f64),
    lines: impl IntoIterator<Item = ((f64, f64), Vec<crate::pdf_text_edit::TextEditRun>, bool)>,
) -> Vec<TableCellLine> {
    lines
        .into_iter()
        .map(|((x, y), runs, hard_break)| TableCellLine {
            x: ((x - cell_origin.0) * 100.).round() / 100.,
            baseline: ((cell_origin.1 - y) * 100.).round() / 100.,
            runs,
            hard_break,
        })
        .collect()
}

/// Where a cell's text sits relative to its cell: how far its first line's
/// visible start is from the cell's left edge and its baseline below the
/// cell's top, from a base cell's first line.
pub fn model_text_place(
    layer: &PageTextLayer,
    table: &PageTable,
    cell: usize,
) -> Option<(f64, f64)> {
    let model = &table.cells[cell];
    let line = layer.line(model.lines.first()?)?;
    let bounds = table.cell_bounds(model);
    let visible = line_box(line)?;
    let (_, baseline) = line.origin();
    Some((visible[0] - bounds[0], bounds[3] - baseline))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_tables::{PageRule, page_tables};
    use crate::pdf_text_edit::{
        ExtraLine, PageTextLine, TextEditLayout, TextEditRun, TextLineChar, TextRunStyle,
    };

    fn style() -> TextRunStyle {
        TextRunStyle {
            font: b"F1".to_vec(),
            font_size: 10.,
            char_spacing: 0.,
            word_spacing: 0.,
            fill: None,
            face: None,
        }
    }

    fn line(x: f64, y: f64, text: &str, cell: i64) -> PageTextLine {
        PageTextLine {
            anchor: TextLineAnchor {
                x_centipoints: (x * 100.) as i64,
                y_centipoints: (y * 100.) as i64,
                ordinal: 0,
            },
            text: text.into(),
            chars: text
                .chars()
                .enumerate()
                .map(|(index, character)| TextLineChar {
                    character,
                    x: index as f64 * 5.,
                    advance: 5.,
                    style: 0,
                    synthetic: false,
                })
                .collect(),
            end_x: text.chars().count() as f64 * 5.,
            styles: vec![style()],
            matrix: [1., 0., 0., 1., x, y],
            horizontal_scale: 1.,
            rise: 0.,
            ascent: 7.,
            descent: -2.,
            fill: None,
            read_only: None,
            in_form: false,
            clip_bounds: None,
            marked_content_ids: vec![cell],
        }
    }

    /// A declared table of two columns and three rows ruled 20 high from y
    /// 160 down to 100; the first row's second cell is empty.
    fn layer() -> PageTextLayer {
        let rule = |horizontal: bool, at: f64, from: f64, to: f64| PageRule {
            horizontal,
            at,
            from,
            to,
            width: 0.5,
        };
        let mut rules = Vec::new();
        for y in [160., 140., 120., 100.] {
            rules.push(rule(true, y, 100., 300.));
        }
        for x in [100., 200., 300.] {
            rules.push(rule(false, x, 100., 160.));
        }
        PageTextLayer {
            page_index: 0,
            lines: vec![
                line(104., 146., "a", 0),
                line(104., 126., "b", 2),
                line(204., 126., "c", 3),
                line(104., 106., "d", 4),
                line(204., 106., "e", 5),
            ],
            tagged_tables: vec![crate::pdf_structure::TaggedTable {
                row_count: 3,
                column_count: 2,
                cells: (0..6)
                    .map(|index| crate::pdf_structure::TaggedCell {
                        row: index / 2,
                        column: index % 2,
                        row_span: 1,
                        column_span: 1,
                        marked_content_ids: vec![index as i64],
                        element: None,
                        header: false,
                    })
                    .collect(),
                ..Default::default()
            }],
            rules,
            ..Default::default()
        }
    }

    #[test]
    fn rows_are_added_a_line_tall_like_their_model_and_deleted_with_their_text() {
        let layer = layer();
        let mut view = view(&layer);
        assert_eq!(
            row_change_blocked(&view.table),
            None,
            "a declared table on one page with no merged cells"
        );
        // Text typed into the empty cell of the first row stays with it.
        view.edit.cell_texts.push(TableCellText {
            row: 2,
            column: 1,
            lines: Vec::new(),
        });
        insert_row(&layer, &mut view, 1, 0);
        assert_eq!(view.edit.rows.len(), 4);
        assert_eq!(view.edit.rows[1].source, BandSource::New(0));
        // "a" sits 7 below the row's top and its box is 9 tall; its cell
        // keeps 4 below it (its gap above, at most 12).
        assert!(
            (view.edit.rows[1].size - (7. + 9. + 4.)).abs() < 1e-9,
            "{:?}",
            view.edit.rows[1]
        );
        // Fitting the rows to their text keeps the new row a line tall.
        let size = view.edit.rows[1].size;
        fit_rows(&layer, &mut view, &[]);
        assert!(
            (view.edit.rows[1].size - size).abs() < 0.01,
            "{:?}",
            view.edit.rows[1]
        );
        assert_eq!(view.edit.cell_texts[0].row, 3);
        assert!(delete_row(&mut view, 3));
        assert!(view.edit.cell_texts.is_empty());
        assert!(delete_row(&mut view, 0) && delete_row(&mut view, 0) && !delete_row(&mut view, 0));
        assert_eq!(view.edit.rows.len(), 1);
        // A table that continues on another page keeps its rows.
        let mut layer = layer.clone();
        layer.tagged_tables[0].continues = true;
        assert_eq!(
            row_change_blocked(&page_tables(&layer)[0]).as_deref(),
            Some("The table continues on another page")
        );
    }

    #[test]
    fn a_column_border_moves_between_its_columns_down_to_their_widest_words() {
        let layer = layer();
        let mut view = view(&layer);
        let before = view.columns();
        assert_eq!(resize_column(&layer, &mut view, 1, 30.), 30.);
        assert_eq!(view.columns()[1], before[1] + 30.);
        assert_eq!(view.columns()[2], before[2]);
        // "a" (5 wide) with its 4 either side holds the first column at 13.
        let moved = resize_column(&layer, &mut view, 1, -500.);
        assert!(
            (view.columns()[1] - view.columns()[0] - 13.).abs() < 1e-9,
            "{moved}"
        );
        assert!(cell_growth(&view, 0) < 0.);
        assert_eq!(resize_column(&layer, &mut view, 0, 10.), 0.);
    }

    fn view(layer: &PageTextLayer) -> TableView {
        TableView::new(0, page_tables(layer).remove(0), None)
    }

    #[test]
    fn a_cell_whose_text_grows_grows_its_row() {
        let layer = layer();
        let mut view = view(&layer);
        fit_rows(&layer, &mut view, &[]);
        assert!(view.edit.is_unchanged());
        // "b" gains two lines below at a 12 point pitch.
        let b = layer.lines[1].clone();
        let runs = vec![TextEditRun {
            style: style(),
            text: "x".into(),
        }];
        let edit = PageTextEdit {
            page_index: 0,
            anchor: b.anchor,
            original: b.original_runs(),
            runs: b.original_runs(),
            layout: TextEditLayout {
                extra_lines: [12., 24.]
                    .map(|below| ExtraLine {
                        anchor: TextLineAnchor {
                            x_centipoints: 10_400,
                            y_centipoints: ((126. - below) * 100.) as i64,
                            ordinal: 0,
                        },
                        below,
                        origin_shift: 0.,
                        fit: Default::default(),
                        runs: runs.clone(),
                        hard_break: false,
                    })
                    .to_vec(),
                ..Default::default()
            },
        };
        fit_rows(&layer, &mut view, &[edit]);
        // Its text now reaches 126 - 24 - 2 = 100; the cell keeps the 4
        // point gap its text had above (140 - 133 = 7) and below (124 - 120
        // = 4): the row is 140 - 100 + 4 = 44 high.
        assert_eq!(view.edit.rows[1].size, 44.);
        assert_eq!(view.edit.rows[0].size, 20.);
        assert_eq!(view.edit.rows[2].size, 20.);
    }

    #[test]
    fn text_down_on_its_cells_border_leaves_the_row_as_it_is() {
        // "b" and "c" reach the row's bottom border at 120: the gap kept
        // below text is at least half a point, which the row never had.
        let mut layer = layer();
        for index in [1, 2] {
            let x = layer.lines[index].matrix[4];
            let text = layer.lines[index].text.clone();
            let cell = layer.lines[index].marked_content_ids[0];
            layer.lines[index] = line(x, 122., &text, cell);
        }
        let mut view = view(&layer);
        fit_rows(&layer, &mut view, &[]);
        assert!(view.edit.is_unchanged(), "{:?}", view.edit.rows);
    }

    #[test]
    fn a_line_break_typed_at_the_end_grows_its_row_at_once() {
        let layer = layer();
        let mut view = view(&layer);
        // "b" ends in a break: a blank line below it at a 12 point pitch.
        let b = layer.lines[1].clone();
        let blank = |below: f64| ExtraLine {
            anchor: TextLineAnchor {
                x_centipoints: 10_400,
                y_centipoints: ((126. - below) * 100.) as i64,
                ordinal: 0,
            },
            below,
            origin_shift: 0.,
            fit: Default::default(),
            runs: Vec::new(),
            hard_break: false,
        };
        let edit = PageTextEdit {
            page_index: 0,
            anchor: b.anchor,
            original: b.original_runs(),
            runs: b.original_runs(),
            layout: TextEditLayout {
                extra_lines: vec![blank(12.)],
                ..Default::default()
            },
        };
        fit_rows(&layer, &mut view, &[edit]);
        // The blank line reaches 126 - 12 - 2 = 112: 140 - 112 + 4 = 32.
        assert_eq!(view.edit.rows[1].size, 32.);

        // A cell of only blank lines keeps its row.
        let mut view = super::tests::view(&layer);
        let edit = PageTextEdit {
            page_index: 0,
            anchor: b.anchor,
            original: b.original_runs(),
            runs: Vec::new(),
            layout: TextEditLayout {
                extra_lines: vec![blank(12.)],
                ..Default::default()
            },
        };
        fit_rows(&layer, &mut view, &[edit]);
        assert_eq!(view.edit.rows[1].size, 20.);

        // New cell text ending in a break grows its row by the blank line.
        let mut view = super::tests::view(&layer);
        let line = |baseline: f64, text: &str| TableCellLine {
            x: 4.,
            baseline,
            runs: if text.is_empty() {
                Vec::new()
            } else {
                vec![TextEditRun {
                    style: style(),
                    text: text.into(),
                }]
            },
            hard_break: false,
        };
        view.edit.cell_texts.push(TableCellText {
            row: 0,
            column: 1,
            lines: vec![line(14., "new"), line(26., "")],
        });
        fit_rows(&layer, &mut view, &[]);
        assert_eq!(view.edit.rows[0].size, 26. + 2.5 + 4.);
    }

    #[test]
    fn a_growing_table_leaves_the_content_below_where_it_is() {
        let mut layer = layer();
        layer.page_box = Some([0., 0., 400., 200.]);
        layer.lines.push(line(104., 80., "below", -1));
        let mut view = view(&layer);
        fit_rows(&layer, &mut view, &[]);
        assert!(view.edit.is_unchanged());
        // The last row grown by its text.
        view.edit.rows[2].size += 20.;
        assert_eq!(view.edit.growth(), 20.);
        let below = [104., 80., 129., 83.5];
        assert_eq!(view.edit.line_offset(below), Some((0., 0.)));
        // Content moved by hand goes where it was put.
        let moves = [crate::pdf_content_moves::ContentMove {
            page_index: 0,
            kind: crate::pdf_content_moves::ContentKind::Text,
            rect: below,
            offset: [5., 7.],
            scale: [1., 1.],
            removed: false,
            copy: None,
        }];
        assert_eq!(
            crate::pdf_content_moves::line_offset(&[&view.edit], &moves, below),
            Some((5., 7.))
        );
        view.edit.validate().unwrap();
    }
}
