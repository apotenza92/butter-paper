//! Tables on a page: the tables a PDF's structure tree declares, and the
//! changes Edit PDF Content makes to them.
//!
//! A table is only ever what the document says is one: a `Table` element
//! of its structure tree, its cells holding the text drawn in their marked
//! content ([`crate::pdf_structure`]). A ruled grid without one is drawing.
//! Rows and columns reach as far as their cells' text; the rules the page
//! draws only place the edges between them, so a border stays a row's
//! border as the row grows.
//!
//! A [`PageTableEdit`] changes a table by bands: its rows (top to bottom)
//! and columns (left to right) as edited, each a base row or column at a new
//! size, or a new one formatted like a base one. Content in a band moves
//! with it, keeping its offset from the band's start; a rule or the edge of
//! a fill at a band's end stays at its end, so borders follow a row that
//! grows and cells' fills grow with it. A deleted band's content collapses
//! to nothing, and a new band draws copies of its model band's rules and
//! fills. Each band's start is fixed by the bands before it, so only the
//! table moves: content outside it stays where it is.

use serde::{Deserialize, Serialize};

use crate::pdf_structure::{TaggedCell, TaggedTable};
use crate::pdf_text_edit::{PageTextLayer, PageTextLine, TextEditRun, TextLineAnchor};

/// A filled rectangle thinner than this, in points, draws a rule.
const RULE_MAX_THICKNESS: f64 = 3.;
/// Most rules considered on one page.
const MAX_RULES: usize = 20_000;
/// How close to a band's end, along the band, a point stays at its end.
const BAND_END_TOLERANCE: f64 = 1.5;
/// How far past a band's edges content may reach and still be in it.
const BAND_TOLERANCE: f64 = 1.;

/// An axis-aligned rule a page draws, in page space.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageRule {
    pub horizontal: bool,
    /// Its position across: `y` for a horizontal rule, `x` for a vertical.
    pub at: f64,
    /// Its extent along, `from` < `to`.
    pub from: f64,
    pub to: f64,
    pub width: f64,
}

/// An axis-aligned filled rectangle (a cell's shading): left, bottom,
/// right, top in page space.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageFill {
    pub rect: [f64; 4],
}

/// One painted path's subpaths in page space.
pub struct PaintedPath<'a> {
    pub subpaths: &'a [(Vec<(f64, f64)>, bool)],
    pub stroked: bool,
    pub filled: bool,
    /// The stroke width in page units.
    pub line_width: f64,
}

fn axis_rect(points: &[(f64, f64)]) -> Option<[f64; 4]> {
    let mut points = points.to_vec();
    if points.len() == 5 && near(points[0], points[4]) {
        points.pop();
    }
    if points.len() != 4 {
        return None;
    }
    let xs = points.iter().map(|point| point.0);
    let ys = points.iter().map(|point| point.1);
    let rect = [
        xs.clone().fold(f64::MAX, f64::min),
        ys.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
        ys.fold(f64::MIN, f64::max),
    ];
    // Every corner on the rectangle's corners.
    let on_corner = |(x, y): (f64, f64)| {
        ((x - rect[0]).abs() < 0.05 || (x - rect[2]).abs() < 0.05)
            && ((y - rect[1]).abs() < 0.05 || (y - rect[3]).abs() < 0.05)
    };
    points.iter().all(|point| on_corner(*point)).then_some(rect)
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 0.05 && (a.1 - b.1).abs() < 0.05
}

/// Adds the rules and fills `path` draws.
pub fn collect_rules(path: &PaintedPath<'_>, rules: &mut Vec<PageRule>, fills: &mut Vec<PageFill>) {
    if rules.len() >= MAX_RULES {
        return;
    }
    for (points, closed) in path.subpaths {
        if path.filled
            && let Some(rect) = axis_rect(points)
        {
            let (width, height) = (rect[2] - rect[0], rect[3] - rect[1]);
            if width.min(height) <= RULE_MAX_THICKNESS
                && width.max(height) >= 2. * width.min(height)
            {
                let horizontal = width >= height;
                rules.push(if horizontal {
                    PageRule {
                        horizontal,
                        at: (rect[1] + rect[3]) / 2.,
                        from: rect[0],
                        to: rect[2],
                        width: height,
                    }
                } else {
                    PageRule {
                        horizontal,
                        at: (rect[0] + rect[2]) / 2.,
                        from: rect[1],
                        to: rect[3],
                        width,
                    }
                });
            } else if width > RULE_MAX_THICKNESS && height > RULE_MAX_THICKNESS {
                fills.push(PageFill { rect });
            }
            continue;
        }
        if !path.stroked || path.line_width > RULE_MAX_THICKNESS {
            continue;
        }
        let count = points.len();
        let segments = if *closed {
            count
        } else {
            count.saturating_sub(1)
        };
        for index in 0..segments {
            let (a, b) = (points[index], points[(index + 1) % count]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let length = dx.hypot(dy);
            if length < 1. {
                continue;
            }
            if dy.abs() <= 0.01 * length {
                rules.push(PageRule {
                    horizontal: true,
                    at: (a.1 + b.1) / 2.,
                    from: a.0.min(b.0),
                    to: a.0.max(b.0),
                    width: path.line_width,
                });
            } else if dx.abs() <= 0.01 * length {
                rules.push(PageRule {
                    horizontal: false,
                    at: (a.0 + b.0) / 2.,
                    from: a.1.min(b.1),
                    to: a.1.max(b.1),
                    width: path.line_width,
                });
            }
        }
    }
}

/// Identifies a table within its page: its bounds in page space, in
/// hundredths of a point.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TableKey {
    pub left: i64,
    pub bottom: i64,
    pub right: i64,
    pub top: i64,
}

fn centipoints(value: f64) -> i64 {
    (value * 100.).round() as i64
}

#[derive(Clone, Debug, PartialEq)]
pub struct TableCell {
    pub row: usize,
    pub column: usize,
    pub row_span: usize,
    pub column_span: usize,
    /// The text lines in the cell, top to bottom.
    pub lines: Vec<TextLineAnchor>,
}

/// A ruled table found on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct PageTable {
    /// Column boundaries, left to right.
    pub columns: Vec<f64>,
    /// Row boundaries, top to bottom.
    pub rows: Vec<f64>,
    /// In reading order: by row, then column.
    pub cells: Vec<TableCell>,
    /// Why the table's rows and columns cannot change, if they cannot; its
    /// cells' text may still be edited.
    pub read_only: Option<String>,
    /// The structure that declares it, as far as it is on the page.
    pub tags: Option<TaggedTable>,
}

impl PageTable {
    pub fn key(&self) -> TableKey {
        TableKey {
            left: centipoints(self.columns[0]),
            bottom: centipoints(*self.rows.last().expect("a table has rows")),
            right: centipoints(*self.columns.last().expect("a table has columns")),
            top: centipoints(self.rows[0]),
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len() - 1
    }

    pub fn column_count(&self) -> usize {
        self.columns.len() - 1
    }

    /// Left, bottom, right, top.
    pub fn bounds(&self) -> [f64; 4] {
        [
            self.columns[0],
            *self.rows.last().expect("a table has rows"),
            *self.columns.last().expect("a table has columns"),
            self.rows[0],
        ]
    }

    /// A cell's bounds: left, bottom, right, top.
    pub fn cell_bounds(&self, cell: &TableCell) -> [f64; 4] {
        [
            self.columns[cell.column],
            self.rows[cell.row + cell.row_span],
            self.columns[cell.column + cell.column_span],
            self.rows[cell.row],
        ]
    }

    /// The cell at grid unit (`row`, `column`).
    pub fn cell_at_unit(&self, row: usize, column: usize) -> Option<usize> {
        self.cells.iter().position(|cell| {
            (cell.row..cell.row + cell.row_span).contains(&row)
                && (cell.column..cell.column + cell.column_span).contains(&column)
        })
    }

    /// The cell containing page point (`x`, `y`).
    pub fn cell_at(&self, x: f64, y: f64) -> Option<usize> {
        let column = (0..self.column_count())
            .find(|column| x >= self.columns[*column] && x <= self.columns[column + 1])?;
        let row =
            (0..self.row_count()).find(|row| y <= self.rows[*row] && y >= self.rows[row + 1])?;
        self.cell_at_unit(row, column)
    }
}

/// A line's visible box in page space: left, bottom, right, top; `None`
/// for a blank line or one not drawn left to right.
pub fn line_box(line: &PageTextLine) -> Option<[f64; 4]> {
    let first = line
        .chars
        .iter()
        .find(|character| !character.character.is_whitespace())?;
    let last = line
        .chars
        .iter()
        .rfind(|character| !character.character.is_whitespace())?;
    let quad = line.quad(first.x, last.x + last.advance);
    let xs = quad.iter().map(|point| point.0);
    let ys = quad.iter().map(|point| point.1);
    Some([
        xs.clone().fold(f64::MAX, f64::min),
        ys.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
        ys.fold(f64::MIN, f64::max),
    ])
}

/// Where a line's letters sit, for placing it in a table: its visible box
/// from the baseline to half its ascent (accents and descenders reach into
/// the rows around it). Left, bottom, right, top.
pub fn line_core_box(line: &PageTextLine) -> Option<[f64; 4]> {
    let first = line
        .chars
        .iter()
        .find(|character| !character.character.is_whitespace())?;
    let last = line
        .chars
        .iter()
        .rfind(|character| !character.character.is_whitespace())?;
    let (x0, x1) = (first.x, last.x + last.advance);
    let (y0, y1) = (line.rise, line.rise + line.ascent * 0.5);
    let m = line.matrix;
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
    let xs = corners.iter().map(|point| point.0);
    let ys = corners.iter().map(|point| point.1);
    Some([
        xs.clone().fold(f64::MAX, f64::min),
        ys.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
        ys.fold(f64::MIN, f64::max),
    ])
}

/// Whether a line runs left to right, upright, on the page.
fn horizontal_line(line: &PageTextLine) -> bool {
    let m = line.matrix;
    let length = m[0].hypot(m[1]);
    length > 1e-9 && m[0] / length > 0.999 && m[3] * m[0] - m[1] * m[2] > 0.
}

/// The tables `layer`'s structure tree declares, with each cell's lines.
/// Nothing else makes a table: a ruled grid without a declared table is
/// drawing, and its text edits as paragraphs.
pub fn page_tables(layer: &PageTextLayer) -> Vec<PageTable> {
    let mut tables = layer
        .tagged_tables
        .iter()
        .filter_map(|tagged| declared_table(layer, tagged))
        .collect::<Vec<_>>();
    tables.sort_by(|a, b| {
        b.rows[0]
            .total_cmp(&a.rows[0])
            .then(a.columns[0].total_cmp(&b.columns[0]))
    });
    tables
}

/// A declared table placed on the page: its cells hold the lines drawn in
/// their marked content, and its rows and columns reach as far as their
/// text, their edges on the borders the page draws between them.
fn declared_table(layer: &PageTextLayer, tagged: &TaggedTable) -> Option<PageTable> {
    if tagged.row_count == 0 || tagged.column_count == 0 || tagged.cells.is_empty() {
        return None;
    }
    let cell_of = tagged
        .cells
        .iter()
        .enumerate()
        .flat_map(|(index, cell)| cell.marked_content_ids.iter().map(move |id| (*id, index)))
        .collect::<std::collections::HashMap<_, _>>();
    let mut read_only = None;
    let mut contents: Vec<Vec<(TextLineAnchor, [f64; 4])>> = vec![Vec::new(); tagged.cells.len()];
    let mut outside = Vec::new();
    for line in &layer.lines {
        let cells = line
            .marked_content_ids
            .iter()
            .filter_map(|id| cell_of.get(id).copied())
            .collect::<std::collections::BTreeSet<_>>();
        let Some(bounds) = line_box(line) else {
            continue;
        };
        let cell = match cells.len() {
            0 => {
                outside.push(bounds);
                continue;
            }
            1 if line
                .marked_content_ids
                .iter()
                .all(|id| cell_of.contains_key(id)) =>
            {
                *cells.first().expect("one cell")
            }
            1 => {
                read_only.get_or_insert_with(|| "a line runs out of the table".to_owned());
                continue;
            }
            _ => {
                read_only.get_or_insert_with(|| "a line runs across cells".to_owned());
                continue;
            }
        };
        if !horizontal_line(line) {
            read_only.get_or_insert_with(|| "the table has text that is not upright".to_owned());
        }
        if let Some(reason) = &line.read_only {
            read_only.get_or_insert_with(|| reason.clone());
        }
        if line.in_form {
            read_only.get_or_insert_with(|| "the table's text is drawn by a form".to_owned());
        }
        contents[cell].push((line.anchor, bounds));
    }
    // Rows at either end whose content here draws no text are not on this
    // page: their text, if any, is drawn on another. A row with no content
    // at all is an empty row of the table.
    let drawn_elsewhere = |row: usize| {
        let mut cells = tagged
            .cells
            .iter()
            .zip(&contents)
            .filter(|(cell, _)| (cell.row..cell.row + cell.row_span).contains(&row));
        cells.clone().all(|(_, lines)| lines.is_empty())
            && cells.any(|(cell, _)| !cell.marked_content_ids.is_empty())
    };
    let mut first_row = 0;
    while first_row < tagged.row_count && drawn_elsewhere(first_row) {
        first_row += 1;
    }
    let mut last_row = tagged.row_count.checked_sub(1)?;
    while last_row > first_row && drawn_elsewhere(last_row) {
        last_row -= 1;
    }
    if first_row > last_row {
        return None;
    }
    let (kept_cells, contents): (Vec<_>, Vec<_>) = tagged
        .cells
        .iter()
        .zip(contents)
        .filter_map(|(cell, lines)| {
            let start = cell.row.max(first_row);
            let end = (cell.row + cell.row_span).min(last_row + 1);
            (start < end).then(|| {
                let cell = TaggedCell {
                    row: start - first_row,
                    row_span: end - start,
                    ..cell.clone()
                };
                (cell, lines)
            })
        })
        .unzip();
    let tagged = &TaggedTable {
        row_count: last_row + 1 - first_row,
        column_count: tagged.column_count,
        cells: kept_cells,
        element: tagged.element,
        row_elements: tagged
            .row_elements
            .get(first_row..=last_row)
            .map(<[_]>::to_vec)
            .unwrap_or_default(),
        continues: tagged.continues || first_row > 0 || last_row + 1 < tagged.row_count,
    };
    let boxes = contents.iter().flatten().map(|(_, bounds)| *bounds);
    let left = boxes.clone().map(|b| b[0]).reduce(f64::min)?;
    let bottom = boxes.clone().map(|b| b[1]).reduce(f64::min)?;
    let right = boxes.clone().map(|b| b[2]).reduce(f64::max)?;
    let top = boxes.map(|b| b[3]).reduce(f64::max)?;
    // Each row's and column's text, from its cells that span only it.
    let extent = |row: bool, index: usize| {
        tagged
            .cells
            .iter()
            .zip(&contents)
            .filter(|(cell, _)| {
                if row {
                    cell.row == index && cell.row_span == 1
                } else {
                    cell.column == index && cell.column_span == 1
                }
            })
            .flat_map(|(_, lines)| lines.iter().map(|(_, bounds)| *bounds))
            .map(|b| if row { (-b[3], -b[1]) } else { (b[0], b[2]) })
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
    };
    let row_extents = (0..tagged.row_count)
        .map(|row| extent(true, row))
        .collect::<Vec<_>>();
    let column_extents = (0..tagged.column_count)
        .map(|column| extent(false, column))
        .collect::<Vec<_>>();
    // Rows run down the page: measured downwards, as -y.
    let across_rows = |rule: &&PageRule| rule.horizontal && rule.from < right && rule.to > left;
    let across_columns = |rule: &&PageRule| !rule.horizontal && rule.from < top && rule.to > bottom;
    let row_rules = layer
        .rules
        .iter()
        .filter(across_rows)
        .map(|rule| -rule.at)
        .collect::<Vec<_>>();
    let column_rules = layer
        .rules
        .iter()
        .filter(across_columns)
        .map(|rule| rule.at)
        .collect::<Vec<_>>();
    // How far the table may reach: to the nearest other text beside it.
    let page = [f64::MIN, f64::MIN, f64::MAX, f64::MAX];
    let overlaps = |a: (f64, f64), b: (f64, f64)| a.0 < b.1 && b.0 < a.1;
    let above = outside
        .iter()
        .filter(|b| overlaps((b[0], b[2]), (left, right)) && b[1] >= top)
        .map(|b| -b[1])
        .fold(-page[3], f64::max);
    let below = outside
        .iter()
        .filter(|b| overlaps((b[0], b[2]), (left, right)) && b[3] <= bottom)
        .map(|b| -b[3])
        .fold(-page[1], f64::min);
    let before = outside
        .iter()
        .filter(|b| overlaps((b[1], b[3]), (bottom, top)) && b[2] <= left)
        .map(|b| b[2])
        .fold(page[0], f64::max);
    let after = outside
        .iter()
        .filter(|b| overlaps((b[1], b[3]), (bottom, top)) && b[0] >= right)
        .map(|b| b[0])
        .fold(page[2], f64::min);
    let (rows, rows_placed) = band_edges(&row_extents, &row_rules, above, below);
    let (columns, columns_placed) = band_edges(&column_extents, &column_rules, before, after);
    if !(rows_placed && columns_placed) {
        read_only.get_or_insert_with(|| "a row or column has no text to place it by".to_owned());
    }
    let mut cells = Vec::with_capacity(tagged.cells.len());
    for (cell, lines) in tagged.cells.iter().zip(contents) {
        let mut lines = lines;
        lines.sort_by(|a, b| b.1[3].total_cmp(&a.1[3]).then(a.1[0].total_cmp(&b.1[0])));
        cells.push(TableCell {
            row: cell.row,
            column: cell.column,
            row_span: cell.row_span,
            column_span: cell.column_span,
            lines: lines.into_iter().map(|(anchor, _)| anchor).collect(),
        });
    }
    Some(PageTable {
        columns,
        rows: rows.into_iter().map(|edge| -edge).collect(),
        cells,
        read_only,
        tags: Some(tagged.clone()),
    })
}

/// The edges of a table's bands along one axis, ascending, from each
/// band's text (`None` for a band with none) and the positions of the
/// rules the page draws across the table, with the table reaching no
/// further than `before` and `after`. An edge sits on the first rule
/// between two bands' text, else midway between them; the outer edges on
/// the nearest rule beyond the text, else at the text. Whether every band
/// could be placed: a band with no text takes the rules in its gap, one
/// per edge, or is placed with no size.
fn band_edges(
    extents: &[Option<(f64, f64)>],
    rules: &[f64],
    before: f64,
    after: f64,
) -> (Vec<f64>, bool) {
    let mut rules = rules.to_vec();
    rules.sort_by(f64::total_cmp);
    rules.dedup();
    let between = |low: f64, high: f64| {
        rules
            .iter()
            .copied()
            .filter(move |at| *at > low && *at < high)
    };
    let known = extents
        .iter()
        .enumerate()
        .filter_map(|(index, extent)| extent.map(|extent| (index, extent)))
        .collect::<Vec<_>>();
    let (Some(first), Some(last)) = (known.first().copied(), known.last().copied()) else {
        return (vec![0.; extents.len() + 1], false);
    };
    let mut edges = vec![f64::NAN; extents.len() + 1];
    let mut placed = true;
    // `count` edges in `low..high`, nearest `low` first when `from_low`.
    let mut fill = |edges: &mut [f64], low: f64, high: f64, fallback: f64, from_low: bool| {
        let mut found = between(low, high).collect::<Vec<_>>();
        if !from_low {
            found.reverse();
        }
        if found.len() >= edges.len() {
            let mut chosen = found[..edges.len()].to_vec();
            chosen.sort_by(f64::total_cmp);
            edges.copy_from_slice(&chosen);
        } else {
            if edges.len() > 1 {
                placed = false;
            }
            let at = found.first().copied().unwrap_or(fallback);
            edges.fill(at);
        }
    };
    // Before the first band with text, and after the last.
    fill(&mut edges[..=first.0], before, first.1.0, first.1.0, false);
    fill(&mut edges[last.0 + 1..], last.1.1, after, last.1.1, true);
    for pair in known.windows(2) {
        let ((a, (_, a_end)), (b, (b_start, _))) = (pair[0], pair[1]);
        let middle = (a_end + b_start) / 2.;
        fill(&mut edges[a + 1..=b], a_end, b_start, middle, true);
    }
    // Text overlapping its neighbour's leaves the edges out of order: keep
    // them ascending.
    for index in 1..edges.len() {
        if edges[index] < edges[index - 1] {
            edges[index] = edges[index - 1];
        }
    }
    (edges, placed)
}

/// A cell's text area: where its text may run along its lines (left and
/// right, in page space), and how close to the cell's bottom its text may
/// come before the row grows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellInset {
    pub left: f64,
    pub right: f64,
    /// The gap kept between the text's lowest point and the cell's bottom.
    pub bottom_gap: f64,
}

/// The text area of `table`'s cell `cell`, from where its text sits: the
/// gap between the cell's left edge and its text is kept on both sides.
pub fn cell_inset(layer: &PageTextLayer, table: &PageTable, cell: usize) -> CellInset {
    let bounds = table.cell_bounds(&table.cells[cell]);
    let boxes = table.cells[cell]
        .lines
        .iter()
        .filter_map(|anchor| layer.line(anchor).and_then(line_box))
        .collect::<Vec<_>>();
    let default_pad = 2.;
    if boxes.is_empty() {
        return CellInset {
            left: bounds[0] + default_pad,
            right: bounds[2] - default_pad,
            bottom_gap: default_pad,
        };
    }
    let text_left = boxes.iter().map(|b| b[0]).fold(f64::MAX, f64::min);
    let text_right = boxes.iter().map(|b| b[2]).fold(f64::MIN, f64::max);
    let text_top = boxes.iter().map(|b| b[3]).fold(f64::MIN, f64::max);
    let text_bottom = boxes.iter().map(|b| b[1]).fold(f64::MAX, f64::min);
    let left_pad = (text_left - bounds[0]).max(0.);
    let right_pad = (bounds[2] - text_right).max(0.);
    // Left-aligned text keeps its left gap at the right too; right-aligned
    // or centred text, its right gap.
    let pad = left_pad.min(right_pad).clamp(0.5, 12.);
    let top_gap = (bounds[3] - text_top).max(0.);
    let bottom_gap = (text_bottom - bounds[1]).max(0.);
    CellInset {
        left: bounds[0] + pad.min(left_pad),
        right: (bounds[2] - pad.min(right_pad)).max(text_right),
        bottom_gap: top_gap.min(bottom_gap).clamp(0.5, 12.),
    }
}

/// Where a band of an edited table comes from.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BandSource {
    /// The base table's row or column at this index.
    Base(usize),
    /// A new row or column, drawn like the base one at this index.
    New(usize),
}

/// A row or column of an edited table.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TableBand {
    pub source: BandSource,
    /// Its height (a row) or width (a column), in points.
    pub size: f64,
    /// A row's height as its border was dragged to: its text still grows
    /// it past that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_size: Option<f64>,
}

/// Changes to one table's rows and columns.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageTableEdit {
    pub page_index: u32,
    /// The base table's row boundaries, top to bottom, and its column
    /// boundaries, left to right, in page space.
    pub base_rows: Vec<f64>,
    pub base_columns: Vec<f64>,
    /// The rows, top to bottom, and columns, left to right, as edited.
    pub rows: Vec<TableBand>,
    pub columns: Vec<TableBand>,
    /// Text typed into cells the base table has no text in (empty cells,
    /// and new rows' and columns' cells), drawn as new text.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cell_texts: Vec<TableCellText>,
}

/// New text in a cell of an edited table.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TableCellText {
    /// The cell's first row and column in the edited table.
    pub row: usize,
    pub column: usize,
    /// Its lines, top to bottom.
    pub lines: Vec<TableCellLine>,
}

/// A line of new cell text, placed from its cell's top left corner.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TableCellLine {
    /// From the cell's left edge to where the line starts, in points.
    pub x: f64,
    /// From the cell's top edge down to the line's baseline, in points.
    pub baseline: f64,
    pub runs: Vec<TextEditRun>,
    /// A hard line break ends it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hard_break: bool,
}

impl TableCellText {
    pub fn text(&self) -> String {
        self.lines
            .iter()
            .map(|line| {
                line.runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn is_empty(&self) -> bool {
        self.lines
            .iter()
            .all(|line| line.runs.iter().all(|run| run.text.trim().is_empty()))
    }
}

impl PageTableEdit {
    /// The table as it is.
    pub fn unchanged(page_index: u32, table: &PageTable) -> Self {
        let bands = |boundaries: &[f64]| {
            boundaries
                .windows(2)
                .enumerate()
                .map(|(index, pair)| TableBand {
                    source: BandSource::Base(index),
                    size: (pair[1] - pair[0]).abs(),
                    set_size: None,
                })
                .collect()
        };
        Self {
            page_index,
            base_rows: table.rows.clone(),
            base_columns: table.columns.clone(),
            rows: bands(&table.rows),
            columns: bands(&table.columns),
            cell_texts: Vec::new(),
        }
    }

    pub fn key(&self) -> TableKey {
        TableKey {
            left: centipoints(self.base_columns[0]),
            bottom: centipoints(*self.base_rows.last().expect("a table has rows")),
            right: centipoints(*self.base_columns.last().expect("a table has columns")),
            top: centipoints(self.base_rows[0]),
        }
    }

    /// Whether it leaves the table as it is.
    pub fn is_unchanged(&self) -> bool {
        let same = |bands: &[TableBand], base: &[f64]| {
            bands.len() + 1 == base.len()
                && bands.iter().enumerate().all(|(index, band)| {
                    band.source == BandSource::Base(index)
                        && band.set_size.is_none()
                        && (band.size - (base[index + 1] - base[index]).abs()).abs() < 0.005
                })
        };
        same(&self.rows, &self.base_rows)
            && same(&self.columns, &self.base_columns)
            && self.cell_texts.iter().all(TableCellText::is_empty)
    }

    /// Whether the edit is well formed: sizes positive and finite, each
    /// base band used at most once and every band's source in range.
    pub fn validate(&self) -> Result<(), String> {
        let check = |bands: &[TableBand], base: usize, what: &str| {
            if bands.is_empty() {
                return Err(format!("a table needs a {what}"));
            }
            let mut used = vec![false; base];
            for band in bands {
                if !band.size.is_finite() || band.size < 0. || band.size > 100_000. {
                    return Err(format!("a {what} has an invalid size"));
                }
                match band.source {
                    BandSource::Base(index) | BandSource::New(index) if index >= base => {
                        return Err(format!("a {what} names a {what} the table does not have"));
                    }
                    BandSource::Base(index) => {
                        if std::mem::replace(&mut used[index], true) {
                            return Err(format!("a {what} appears twice"));
                        }
                    }
                    BandSource::New(_) => {}
                }
            }
            Ok(())
        };
        let ordered = |values: &[f64], descending: bool| {
            values.len() >= 2
                && values.iter().all(|value| value.is_finite())
                && values.windows(2).all(|pair| {
                    if descending {
                        pair[0] > pair[1]
                    } else {
                        pair[0] < pair[1]
                    }
                })
        };
        if !ordered(&self.base_rows, true) || !ordered(&self.base_columns, false) {
            return Err("the table's base rows or columns are out of order".into());
        }
        check(&self.rows, self.base_rows.len() - 1, "row")?;
        check(&self.columns, self.base_columns.len() - 1, "column")?;
        for text in &self.cell_texts {
            if text.row >= self.rows.len() || text.column >= self.columns.len() {
                return Err("cell text names a cell the table does not have".into());
            }
            for line in &text.lines {
                if !(line.x.is_finite() && line.baseline.is_finite())
                    || line.runs.iter().any(|run| {
                        !(run.style.font_size.is_finite() && run.style.font_size.abs() > 0.)
                    })
                {
                    return Err("cell text is placed invalidly".into());
                }
            }
        }
        Ok(())
    }

    /// The top left corner of output cell (`row`, `column`), in page space.
    pub fn cell_origin(&self, row: usize, column: usize) -> Option<(f64, f64)> {
        let rows = self.output_rows();
        let columns = self.output_columns();
        Some((*columns.get(column)?, *rows.get(row)?))
    }

    pub fn row_map(&self) -> BandMap {
        BandMap::new(
            self.base_rows
                .iter()
                .map(|y| self.base_rows[0] - y)
                .collect(),
            &self.rows,
        )
    }

    pub fn column_map(&self) -> BandMap {
        BandMap::new(
            self.base_columns
                .iter()
                .map(|x| x - self.base_columns[0])
                .collect(),
            &self.columns,
        )
    }

    /// The edited table's row boundaries, top to bottom, in page space.
    pub fn output_rows(&self) -> Vec<f64> {
        let top = self.base_rows[0];
        std::iter::once(top)
            .chain(self.rows.iter().scan(top, |at, band| {
                *at -= band.size;
                Some(*at)
            }))
            .collect()
    }

    /// The edited table's column boundaries, left to right.
    pub fn output_columns(&self) -> Vec<f64> {
        let left = self.base_columns[0];
        std::iter::once(left)
            .chain(self.columns.iter().scan(left, |at, band| {
                *at += band.size;
                Some(*at)
            }))
            .collect()
    }

    /// How far the table's bottom moves down.
    pub fn growth(&self) -> f64 {
        let base = self.base_rows[0] - self.base_rows[self.base_rows.len() - 1];
        self.rows.iter().map(|band| band.size).sum::<f64>() - base
    }

    /// The edit's row and column maps, to place and move many pieces of
    /// content with.
    pub fn map(&self) -> TableMap<'_> {
        TableMap {
            edit: self,
            rows: self.row_map(),
            columns: self.column_map(),
        }
    }

    /// Where the edit moves the content at page `rect` (left, bottom,
    /// right, top). Content outside the table stays.
    pub fn placement(&self, rect: [f64; 4]) -> Placement {
        self.map().placement(rect)
    }

    /// Maps a page point of content placed by `placement`, by its band (or
    /// point by point when it spans bands).
    pub fn map_point(&self, placement: &Placement, point: (f64, f64)) -> Option<(f64, f64)> {
        self.map().map_point_as(placement, None, point)
    }

    /// [`Self::map_point`] for the copy of content drawn in output row and
    /// column `instance` (`None`: the original's).
    pub fn map_point_as(
        &self,
        placement: &Placement,
        instance: Option<(Option<usize>, Option<usize>)>,
        point: (f64, f64),
    ) -> Option<(f64, f64)> {
        self.map().map_point_as(placement, instance, point)
    }

    /// The output (row, column) instances that draw content placed by
    /// `placement`: the original's place first when it survives, then the
    /// copies new rows and columns draw.
    pub fn instances(&self, placement: &Placement) -> Vec<(Option<usize>, Option<usize>)> {
        self.map().instances(placement)
    }

    /// How far a line at page `rect` moves, or `None` when it is removed.
    pub fn line_offset(&self, rect: [f64; 4]) -> Option<(f64, f64)> {
        self.map().line_offset(rect)
    }
}

/// A table edit with its row and column maps built: see the
/// [`PageTableEdit`] methods of the same names.
pub struct TableMap<'a> {
    edit: &'a PageTableEdit,
    rows: BandMap,
    columns: BandMap,
}

impl TableMap<'_> {
    pub fn placement(&self, rect: [f64; 4]) -> Placement {
        let top = self.edit.base_rows[0];
        let left = self.edit.base_columns[0];
        let bottom = self.edit.base_rows[self.edit.base_rows.len() - 1];
        let right = self.edit.base_columns[self.edit.base_columns.len() - 1];
        // Reaching past the table on every side (a clip around the page):
        // not the table's, it stays.
        if rect[0] < left - BAND_TOLERANCE
            && rect[2] > right + BAND_TOLERANCE
            && rect[1] < bottom - BAND_TOLERANCE
            && rect[3] > top + BAND_TOLERANCE
        {
            return Placement {
                row: AxisPlace::Before,
                column: AxisPlace::Before,
            };
        }
        Placement {
            row: self.rows.classify(top - rect[3], top - rect[1]),
            column: self.columns.classify(rect[0] - left, rect[2] - left),
        }
    }

    pub fn map_point_as(
        &self,
        placement: &Placement,
        instance: Option<(Option<usize>, Option<usize>)>,
        (x, y): (f64, f64),
    ) -> Option<(f64, f64)> {
        if !(placement.row.inside() && placement.column.inside()) {
            return Some((x, y));
        }
        let top = self.edit.base_rows[0];
        let left = self.edit.base_columns[0];
        let (row_out, column_out) = instance.unwrap_or((None, None));
        let mapped_y = self.rows.map(&placement.row, row_out, top - y)?;
        let mapped_x = self.columns.map(&placement.column, column_out, x - left)?;
        Some((left + mapped_x, top - mapped_y))
    }

    pub fn instances(&self, placement: &Placement) -> Vec<(Option<usize>, Option<usize>)> {
        if !(placement.row.inside() && placement.column.inside()) {
            return vec![(None, None)];
        }
        let row_instances = self.rows.instances(&placement.row);
        let column_instances = self.columns.instances(&placement.column);
        let mut instances = Vec::new();
        for row in &row_instances {
            for column in &column_instances {
                instances.push((*row, *column));
            }
        }
        instances
    }

    pub fn line_offset(&self, rect: [f64; 4]) -> Option<(f64, f64)> {
        let placement = self.placement(rect);
        if !(placement.row.inside() && placement.column.inside()) {
            return Some((0., 0.));
        }
        // A line moves with its band's start.
        let dy = self.rows.start_offset(&placement.row)?;
        let dx = self.columns.start_offset(&placement.column)?;
        Some((dx, -dy))
    }
}

/// How table edits move content at page `rect`: `None` when a deleted row
/// or column removes it.
pub fn content_offset(tables: &[&PageTableEdit], rect: [f64; 4]) -> Option<(f64, f64)> {
    let mut offset = (0., 0.);
    for table in tables {
        let (dx, dy) = table.line_offset(rect)?;
        offset.0 += dx;
        offset.1 += dy;
    }
    Some(offset)
}

/// Where content lies along one axis of a table.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AxisPlace {
    Before,
    /// Within one band, from its start to its end (a cell's fill or side
    /// border): it stretches with the band.
    Band(usize),
    /// Within one band, not across it: it moves with the band's end when it
    /// lies at the end (a border below the band), else with its start.
    Rigid {
        band: usize,
        end: bool,
    },
    /// Across bands.
    Spanning,
    After,
}

impl AxisPlace {
    fn inside(&self) -> bool {
        matches!(self, Self::Band(_) | Self::Rigid { .. } | Self::Spanning)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub row: AxisPlace,
    pub column: AxisPlace,
}

/// One axis of an edited table, as distances from its start: base band
/// boundaries and the edited bands.
#[derive(Clone, Debug)]
pub struct BandMap {
    base: Vec<f64>,
    /// Each output band's start, size and source.
    out: Vec<(f64, f64, BandSource)>,
}

impl BandMap {
    fn new(base: Vec<f64>, bands: &[TableBand]) -> Self {
        let mut at = 0.;
        let out = bands
            .iter()
            .map(|band| {
                let start = at;
                at += band.size;
                (start, band.size, band.source)
            })
            .collect();
        Self { base, out }
    }

    fn count(&self) -> usize {
        self.base.len() - 1
    }

    fn total(&self) -> f64 {
        self.out.last().map_or(0., |(start, size, _)| start + size)
    }

    /// The output band showing base band `band`.
    fn output_of(&self, band: usize) -> Option<usize> {
        self.out
            .iter()
            .position(|(_, _, source)| *source == BandSource::Base(band))
    }

    /// Where content from `from` to `to` lies. Content at a boundary
    /// belongs to the band before it (a rule ends the band above it).
    fn classify(&self, from: f64, to: f64) -> AxisPlace {
        let end = self.base[self.count()];
        if to < -BAND_TOLERANCE {
            return AxisPlace::Before;
        }
        if from > end + BAND_TOLERANCE {
            return AxisPlace::After;
        }
        for band in 0..self.count() {
            let (start, end) = (self.base[band], self.base[band + 1]);
            if from >= start - BAND_TOLERANCE && to <= end + BAND_TOLERANCE {
                let at_start = from <= start + BAND_END_TOLERANCE;
                let at_end = to >= end - BAND_END_TOLERANCE;
                return if at_start && at_end {
                    AxisPlace::Band(band)
                } else {
                    AxisPlace::Rigid {
                        band,
                        end: from >= end - BAND_END_TOLERANCE,
                    }
                };
            }
        }
        // Across bands, inside the table: a border or fill runs from one
        // boundary to another and stretches; anything else (a drawing's
        // stroke over a border) moves whole with the band at its middle.
        let on_boundary = |d: f64| {
            self.base
                .iter()
                .any(|boundary| (d - boundary).abs() <= BAND_END_TOLERANCE)
        };
        let inside = from >= -BAND_TOLERANCE && to <= end + BAND_TOLERANCE;
        if inside && !(on_boundary(from) && on_boundary(to)) {
            let middle = (from + to) / 2.;
            let band = (0..self.count())
                .find(|band| middle <= self.base[band + 1])
                .unwrap_or(self.count() - 1);
            return AxisPlace::Rigid { band, end: false };
        }
        AxisPlace::Spanning
    }

    /// Where a base band's content collapses when it is deleted: the end
    /// of the output band before its place.
    fn collapse(&self, band: usize) -> f64 {
        (0..band)
            .rev()
            .find_map(|before| self.output_of(before))
            .map_or(0., |index| self.out[index].0 + self.out[index].1)
    }

    /// Maps distance `d` of content in `place`, drawn in output band
    /// `instance` (`None`: the base band's own).
    fn map(&self, place: &AxisPlace, instance: Option<usize>, d: f64) -> Option<f64> {
        match place {
            AxisPlace::Before => Some(d),
            AxisPlace::After => Some(d + self.total() - self.base[self.count()]),
            AxisPlace::Band(band) => {
                let output = instance.or_else(|| self.output_of(*band));
                let Some(output) = output else {
                    return Some(self.collapse(*band));
                };
                let (start, size, _) = self.out[output];
                let (base_start, base_end) = (self.base[*band], self.base[band + 1]);
                Some(if (d - base_end).abs() <= BAND_END_TOLERANCE {
                    start + size + (d - base_end)
                } else {
                    start + (d - base_start)
                })
            }
            AxisPlace::Rigid { band, end } => {
                let Some(output) = instance.or_else(|| self.output_of(*band)) else {
                    // A border at the deleted band's start or end keeps its
                    // width where the band collapses; anything else flattens,
                    // as everything does when no band is left.
                    let collapse = self.collapse(*band);
                    let edge = [self.base[*band], self.base[band + 1]]
                        .into_iter()
                        .filter(|_| !self.out.is_empty())
                        .find(|edge| (d - edge).abs() <= BAND_END_TOLERANCE);
                    return Some(edge.map_or(collapse, |edge| collapse + d - edge));
                };
                let (start, size, _) = self.out[output];
                Some(if *end {
                    d + start + size - self.base[band + 1]
                } else {
                    d + start - self.base[*band]
                })
            }
            AxisPlace::Spanning => {
                // Point by point: a boundary goes where the band before it
                // ends, an inner point with its band.
                let count = self.count();
                if let Some(boundary) =
                    (0..=count).find(|index| (d - self.base[*index]).abs() <= BAND_END_TOLERANCE)
                {
                    let offset = d - self.base[boundary];
                    if boundary == 0 {
                        return Some(offset);
                    }
                    if boundary == count {
                        return Some(self.total() + offset);
                    }
                    if let Some(before) = self.output_of(boundary - 1) {
                        let (start, size, _) = self.out[before];
                        return Some(start + size + offset);
                    }
                    if let Some(after) = self.output_of(boundary) {
                        return Some(self.out[after].0 + offset);
                    }
                    return Some(self.collapse(boundary));
                }
                let band = (0..count)
                    .find(|band| d >= self.base[*band] && d <= self.base[band + 1])
                    .unwrap_or(if d < 0. { 0 } else { count - 1 });
                self.map(&AxisPlace::Band(band), None, d)
            }
        }
    }

    /// How far content in `place` moves with its band's start, or `None`
    /// when its band is deleted.
    fn start_offset(&self, place: &AxisPlace) -> Option<f64> {
        match place {
            AxisPlace::Before => Some(0.),
            AxisPlace::After => Some(self.total() - self.base[self.count()]),
            AxisPlace::Band(band) | AxisPlace::Rigid { band, .. } => {
                let output = self.output_of(*band)?;
                Some(self.out[output].0 - self.base[*band])
            }
            AxisPlace::Spanning => Some(0.),
        }
    }

    /// The output bands that draw content in `place`: its own band's (as
    /// `None`) when it survives, then each new band modelled on it.
    fn instances(&self, place: &AxisPlace) -> Vec<Option<usize>> {
        match place {
            AxisPlace::Band(band) | AxisPlace::Rigid { band, .. } => {
                let mut instances = vec![None];
                instances.extend(
                    self.out
                        .iter()
                        .enumerate()
                        .filter(|(_, (_, _, source))| *source == BandSource::New(*band))
                        .map(|(index, _)| Some(index)),
                );
                instances
            }
            _ => vec![None],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_text_edit::{TextLineChar, TextRunStyle};

    fn h(at: f64, from: f64, to: f64) -> PageRule {
        PageRule {
            horizontal: true,
            at,
            from,
            to,
            width: 0.5,
        }
    }

    fn v(at: f64, from: f64, to: f64) -> PageRule {
        PageRule {
            horizontal: false,
            at,
            from,
            to,
            width: 0.5,
        }
    }

    fn line(x: f64, y: f64, text: &str, ordinal: u32) -> PageTextLine {
        let chars = text
            .chars()
            .enumerate()
            .map(|(index, character)| TextLineChar {
                character,
                x: index as f64 * 5.,
                advance: 5.,
                style: 0,
                synthetic: false,
            })
            .collect::<Vec<_>>();
        PageTextLine {
            anchor: TextLineAnchor {
                x_centipoints: (x * 100.) as i64,
                y_centipoints: (y * 100.) as i64,
                ordinal,
            },
            text: text.into(),
            end_x: text.chars().count() as f64 * 5.,
            chars,
            styles: vec![TextRunStyle {
                font: b"F1".to_vec(),
                font_size: 10.,
                char_spacing: 0.,
                word_spacing: 0.,
                fill: None,
                face: None,
            }],
            matrix: [1., 0., 0., 1., x, y],
            horizontal_scale: 1.,
            rise: 0.,
            ascent: 7.,
            descent: -2.,
            fill: None,
            read_only: None,
            in_form: false,
            clip_bounds: None,
            marked_content_ids: Vec::new(),
            paragraph: None,
        }
    }

    /// A declared table of `rows` x `columns` cells, cell `n` (by row, then
    /// column) holding marked content `n`.
    pub(super) fn declared(rows: usize, columns: usize) -> TaggedTable {
        TaggedTable {
            row_count: rows,
            column_count: columns,
            cells: (0..rows * columns)
                .map(|index| TaggedCell {
                    row: index / columns,
                    column: index % columns,
                    row_span: 1,
                    column_span: 1,
                    marked_content_ids: vec![index as i64],
                    element: None,
                    header: false,
                })
                .collect(),
            ..Default::default()
        }
    }

    /// A declared 3 x 2 table ruled from (100, 100) to (300, 160): rows 20
    /// high, columns 100 wide, with a line of text in each cell.
    pub(super) fn grid_layer() -> PageTextLayer {
        let mut rules = Vec::new();
        for y in [160., 140., 120., 100.] {
            rules.push(h(y, 100., 300.));
        }
        for x in [100., 200., 300.] {
            rules.push(v(x, 100., 160.));
        }
        let mut lines = Vec::new();
        for (row, y) in [146., 126., 106.].into_iter().enumerate() {
            for column in 0..2 {
                let mut cell = line(
                    104. + column as f64 * 100.,
                    y,
                    &format!("r{row}c{column}"),
                    0,
                );
                cell.marked_content_ids = vec![(row * 2 + column) as i64];
                lines.push(cell);
            }
        }
        PageTextLayer {
            page_index: 0,
            lines,
            tagged_tables: vec![declared(3, 2)],
            rules,
            ..Default::default()
        }
    }

    #[test]
    fn filled_thin_rectangles_and_stroked_segments_are_rules() {
        let mut rules = Vec::new();
        let mut fills = Vec::new();
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64| {
            (vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)], true)
        };
        collect_rules(
            &PaintedPath {
                subpaths: &[
                    rect(0., 0., 100., 0.75),
                    rect(0., 0., 0.75, 30.),
                    rect(10., 10., 50., 30.),
                ],
                stroked: false,
                filled: true,
                line_width: 1.,
            },
            &mut rules,
            &mut fills,
        );
        assert_eq!(rules.len(), 2);
        assert!(rules[0].horizontal && (rules[0].at - 0.375).abs() < 1e-9);
        assert!(!rules[1].horizontal && rules[1].to == 30.);
        assert_eq!(
            fills,
            [PageFill {
                rect: [10., 10., 50., 30.]
            }]
        );
        rules.clear();
        collect_rules(
            &PaintedPath {
                subpaths: &[
                    (vec![(0., 5.), (80., 5.)], false),
                    (vec![(3., 0.), (40., 40.)], false),
                ],
                stroked: true,
                filled: false,
                line_width: 0.5,
            },
            &mut rules,
            &mut fills,
        );
        assert_eq!(rules, [h(5., 0., 80.)]);
    }

    #[test]
    fn a_declared_table_holds_its_cells_text_and_sits_on_its_borders() {
        let layer = grid_layer();
        let tables = page_tables(&layer);
        assert_eq!(tables.len(), 1);
        let table = &tables[0];
        assert_eq!(table.rows, [160., 140., 120., 100.]);
        assert_eq!(table.columns, [100., 200., 300.]);
        assert_eq!(table.cells.len(), 6);
        for (index, cell) in table.cells.iter().enumerate() {
            assert_eq!((cell.row, cell.column), (index / 2, index % 2));
            assert_eq!(cell.lines, [layer.lines[index].anchor]);
        }
        assert!(table.read_only.is_none());
    }

    #[test]
    fn a_ruled_grid_the_document_does_not_declare_is_drawing() {
        let mut layer = grid_layer();
        layer.tagged_tables.clear();
        assert!(page_tables(&layer).is_empty());
        // Nor does a declared table with none of its text on the page.
        let mut layer = grid_layer();
        for line in &mut layer.lines {
            line.marked_content_ids.clear();
        }
        assert!(page_tables(&layer).is_empty());
    }

    #[test]
    fn without_borders_edges_sit_at_and_between_the_text() {
        let mut layer = grid_layer();
        layer.rules.clear();
        let table = &page_tables(&layer)[0];
        // Lines reach from 2 below their baseline to 7 above it.
        assert_eq!(table.rows, [153., 138.5, 118.5, 104.]);
        assert_eq!(table.columns, [104., 164., 224.]);
        assert!(table.read_only.is_none());
    }

    #[test]
    fn declared_spans_merge_cells_whatever_the_rules() {
        let mut layer = grid_layer();
        layer.tagged_tables[0].cells.remove(1);
        layer.tagged_tables[0].cells[0].column_span = 2;
        layer.lines.remove(1);
        let table = &page_tables(&layer)[0];
        assert_eq!(table.cells.len(), 5);
        assert_eq!(
            (table.cells[0].row_span, table.cells[0].column_span),
            (1, 2)
        );
        assert_eq!(table.columns, [100., 200., 300.]);
    }

    #[test]
    fn text_past_a_border_stays_in_its_cell_and_a_line_across_cells_locks_rows() {
        let mut layer = grid_layer();
        layer.lines[0] = line(104., 146., "this text is far too long for one cell", 0);
        layer.lines[0].marked_content_ids = vec![0];
        let table = &page_tables(&layer)[0];
        assert_eq!(table.cells[0].lines, [layer.lines[0].anchor]);
        assert!(table.read_only.is_none());
        let mut layer = grid_layer();
        layer.lines[0].marked_content_ids = vec![0, 1];
        let table = &page_tables(&layer)[0];
        assert!(table.cells[0].lines.is_empty());
        assert_eq!(table.read_only.as_deref(), Some("a line runs across cells"));
    }

    #[test]
    fn rows_with_no_text_here_are_left_to_the_page_their_text_is_on() {
        let mut layer = grid_layer();
        // The table's last row is drawn on the next page.
        layer.lines.truncate(4);
        let table = &page_tables(&layer)[0];
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.rows, [160., 140., 120.]);
        assert!(table.read_only.is_none());
        // An empty row between two with text takes the rules in its gap.
        let mut layer = grid_layer();
        layer.lines.drain(2..4);
        let table = &page_tables(&layer)[0];
        assert_eq!(table.rows, [160., 140., 120., 100.]);
        assert!(table.read_only.is_none());
    }

    #[test]
    fn a_growing_row_moves_the_rows_below_and_stretches_borders() {
        let table = &page_tables(&grid_layer())[0];
        let mut edit = PageTableEdit::unchanged(0, table);
        assert!(edit.is_unchanged());
        edit.rows[0].size = 30.;
        assert!(!edit.is_unchanged());
        assert_eq!(edit.output_rows(), [160., 130., 110., 90.]);
        assert!((edit.growth() - 10.).abs() < 1e-9);
        // Text in the grown row stays; text below moves down.
        assert_eq!(edit.line_offset([104., 144., 130., 153.]), Some((0., 0.)));
        assert_eq!(edit.line_offset([104., 124., 130., 133.]), Some((0., -10.)));
        // The rule between rows one and two ends row one: it moves down.
        let rule = edit.placement([100., 139.75, 300., 140.25]);
        assert_eq!(rule.row, AxisPlace::Rigid { band: 0, end: true });
        assert_eq!(edit.map_point(&rule, (100., 140.25)), Some((100., 130.25)));
        // A vertical border of the first row stretches; the table's full
        // height border keeps its ends at the table's top and new bottom.
        let border = edit.placement([199.75, 140., 200.25, 160.]);
        assert_eq!(edit.map_point(&border, (200., 160.)), Some((200., 160.)));
        assert_eq!(edit.map_point(&border, (200., 140.)), Some((200., 130.)));
        // A stroke inside the first row that only reaches its bottom moves
        // whole, with the row's top: it does not stretch.
        let stroke = edit.placement([150., 141., 155., 147.]);
        assert_eq!(
            stroke.row,
            AxisPlace::Rigid {
                band: 0,
                end: false
            }
        );
        assert_eq!(edit.map_point(&stroke, (150., 141.)), Some((150., 141.)));
        assert_eq!(edit.map_point(&stroke, (155., 147.)), Some((155., 147.)));
        // One crossing into the next row moves whole with the row it is
        // mostly in.
        let crossing = edit.placement([150., 136., 152., 142.]);
        assert_eq!(
            crossing.row,
            AxisPlace::Rigid {
                band: 1,
                end: false
            }
        );
        assert_eq!(edit.map_point(&crossing, (150., 142.)), Some((150., 132.)));
        let full = edit.placement([99.75, 100., 100.25, 160.]);
        assert_eq!(full.row, AxisPlace::Spanning);
        assert_eq!(edit.map_point(&full, (100., 100.)), Some((100., 90.)));
        assert_eq!(edit.map_point(&full, (100., 160.)), Some((100., 160.)));
        // Content below the table stays.
        let below = edit.placement([100., 50., 150., 60.]);
        assert_eq!(edit.map_point(&below, (100., 50.)), Some((100., 50.)));
        assert_eq!(edit.line_offset([100., 50., 150., 60.]), Some((0., 0.)));
    }

    #[test]
    fn deleted_rows_collapse_and_new_rows_copy_their_model() {
        let table = &page_tables(&grid_layer())[0];
        let mut edit = PageTableEdit::unchanged(0, table);
        // Delete the middle row; add a row like the first at the end.
        edit.rows.remove(1);
        edit.rows.push(TableBand {
            source: BandSource::New(0),
            size: 20.,
            set_size: None,
        });
        edit.validate().unwrap();
        assert_eq!(edit.output_rows(), [160., 140., 120., 100.]);
        assert_eq!(edit.line_offset([104., 124., 130., 133.]), None);
        assert_eq!(edit.line_offset([104., 104., 130., 113.]), Some((0., 20.)));
        let first_row_fill = edit.placement([100., 140., 200., 160.]);
        let instances = edit.instances(&first_row_fill);
        assert_eq!(instances, [(None, None), (Some(2), None)]);
        assert_eq!(
            edit.map_point_as(&first_row_fill, Some(instances[1]), (100., 140.)),
            Some((100., 100.))
        );
        assert_eq!(
            edit.map_point_as(&first_row_fill, Some(instances[1]), (100., 160.)),
            Some((100., 120.))
        );
        // A deleted row's border collapses onto the row before.
        let middle = edit.placement([199.75, 120., 200.25, 140.]);
        assert_eq!(edit.map_point(&middle, (200., 120.)), Some((200., 140.)));
        assert_eq!(edit.map_point(&middle, (200., 140.)), Some((200., 140.)));
    }

    #[test]
    fn reordered_columns_move_with_their_content() {
        let table = &page_tables(&grid_layer())[0];
        let mut edit = PageTableEdit::unchanged(0, table);
        edit.columns.swap(0, 1);
        edit.columns[0].size = 60.;
        assert_eq!(edit.output_columns(), [100., 160., 260.]);
        assert_eq!(
            edit.line_offset([204., 144., 230., 153.]),
            Some((-100., 0.))
        );
        assert_eq!(edit.line_offset([104., 144., 130., 153.]), Some((60., 0.)));
        let validate = |columns: Vec<TableBand>| {
            PageTableEdit {
                columns,
                ..edit.clone()
            }
            .validate()
        };
        assert!(validate(vec![edit.columns[0], edit.columns[0]]).is_err());
        assert!(validate(Vec::new()).is_err());
        assert!(edit.validate().is_ok());
    }
}

#[cfg(test)]
mod stress {
    use super::*;

    /// A drawing's worth of rules (a CAD sheet's grid and hatching) around
    /// a declared table is looked at quickly.
    #[test]
    fn many_rules_are_quick() {
        let mut rules = Vec::new();
        for index in 0..4000 {
            let at = 10. + index as f64 * 3.;
            rules.push(PageRule {
                horizontal: true,
                at,
                from: 10.,
                to: 12_010.,
                width: 0.25,
            });
            rules.push(PageRule {
                horizontal: false,
                at,
                from: 10.,
                to: 12_010.,
                width: 0.25,
            });
        }
        let mut layer = super::tests::grid_layer();
        layer.rules.extend(rules);
        let started = std::time::Instant::now();
        assert_eq!(page_tables(&layer).len(), 1);
        let elapsed = started.elapsed();
        eprintln!("{elapsed:?}");
        assert!(elapsed.as_secs() < 2, "{elapsed:?}");
    }
}
