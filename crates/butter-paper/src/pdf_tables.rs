//! Tables on a page: the ruled grids page content draws, and the changes
//! Edit PDF Content makes to them.
//!
//! A table is found from the rules (thin stroked lines and thin filled
//! rectangles) a page draws: horizontal and vertical rules that cross form a
//! grid, and a missing border between two grid units merges them into one
//! cell. Every text line inside the grid must lie in one cell; a line that
//! crosses a border means the grid is not a table of that text.
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

use crate::pdf_text_edit::{PageTextLayer, PageTextLine, TextEditRun, TextLineAnchor};

/// A filled rectangle thinner than this, in points, draws a rule.
const RULE_MAX_THICKNESS: f64 = 3.;
/// Rules this close along their position, in points, are one rule.
const RULE_MERGE_POSITION: f64 = 1.;
/// Rules along one position separated by a gap this small join.
const RULE_MERGE_GAP: f64 = 2.;
/// How far apart a rule's end and another rule may be and still cross.
const CROSS_TOLERANCE: f64 = 2.;
/// Grid lines this close are one boundary.
const BOUNDARY_TOLERANCE: f64 = 1.5;
/// A border between grid units must cover this share of their edge.
const BORDER_COVERAGE: f64 = 0.6;
/// The smallest table, in points.
const MIN_TABLE_WIDTH: f64 = 20.;
const MIN_TABLE_HEIGHT: f64 = 8.;
/// Most rules or tables considered on one page.
const MAX_RULES: usize = 20_000;
const MAX_TABLE_UNITS: usize = 10_000;
/// How close to a band's end, along the band, a point stays at its end.
const BAND_END_TOLERANCE: f64 = 1.5;
/// How far past a band's edges content may reach and still be in it.
const BAND_TOLERANCE: f64 = 1.;
/// How far past the one rule it crosses a rule must reach, on both sides,
/// to be a border.
const STRAY_REACH: f64 = 10.;

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

/// Rules along one position joined into longer rules, horizontal and
/// vertical apart.
fn merge_rules(rules: Vec<PageRule>) -> Vec<PageRule> {
    let (horizontal, vertical): (Vec<_>, Vec<_>) =
        rules.into_iter().partition(|rule| rule.horizontal);
    let mut merged = merge_parallel(horizontal);
    merged.extend(merge_parallel(vertical));
    merged
}

/// [`merge_rules`] for rules of one orientation.
fn merge_parallel(mut rules: Vec<PageRule>) -> Vec<PageRule> {
    rules.sort_by(|a, b| a.at.total_cmp(&b.at).then(a.from.total_cmp(&b.from)));
    // Group by position, then join overlapping extents in each group.
    let mut groups: Vec<Vec<PageRule>> = Vec::new();
    for rule in rules {
        match groups.last_mut() {
            Some(group)
                if (rule.at - group[group.len() - 1].at).abs() <= RULE_MERGE_POSITION
                    && (rule.at - group[0].at).abs() <= 2. * RULE_MERGE_POSITION =>
            {
                group.push(rule)
            }
            _ => groups.push(vec![rule]),
        }
    }
    let mut merged = Vec::new();
    for mut group in groups {
        group.sort_by(|a, b| a.from.total_cmp(&b.from));
        let at = group.iter().map(|rule| rule.at).sum::<f64>() / group.len() as f64;
        let mut current: Option<PageRule> = None;
        for rule in group {
            match current.as_mut() {
                Some(joined) if rule.from <= joined.to + RULE_MERGE_GAP => {
                    joined.to = joined.to.max(rule.to);
                    joined.width = joined.width.max(rule.width);
                }
                _ => {
                    if let Some(done) = current.take() {
                        merged.push(done);
                    }
                    current = Some(PageRule { at, ..rule });
                }
            }
        }
        merged.extend(current);
    }
    merged
}

fn find(parent: &mut [usize], mut index: usize) -> usize {
    while parent[index] != index {
        parent[index] = parent[parent[index]];
        index = parent[index];
    }
    index
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (a, b) = (find(parent, a), find(parent, b));
    if a != b {
        parent[a.max(b)] = a.min(b);
    }
}

/// Positions within `BOUNDARY_TOLERANCE` of each other as one, ascending.
fn boundaries(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    let mut clusters: Vec<Vec<f64>> = Vec::new();
    for value in values {
        match clusters.last_mut() {
            Some(cluster) if value - cluster[cluster.len() - 1] <= BOUNDARY_TOLERANCE => {
                cluster.push(value)
            }
            _ => clusters.push(vec![value]),
        }
    }
    clusters
        .into_iter()
        .map(|cluster| cluster.iter().sum::<f64>() / cluster.len() as f64)
        .collect()
}

/// How much of `from..to` the rules at `at` cover.
fn coverage(rules: &[&PageRule], at: f64, from: f64, to: f64) -> f64 {
    let mut covered: Vec<(f64, f64)> = rules
        .iter()
        .filter(|rule| (rule.at - at).abs() <= BOUNDARY_TOLERANCE + rule.width / 2.)
        .map(|rule| (rule.from.max(from), rule.to.min(to)))
        .filter(|(start, end)| end > start)
        .collect();
    covered.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut total = 0.;
    let mut reached = from;
    for (start, end) in covered {
        let start = start.max(reached);
        if end > start {
            total += end - start;
            reached = end;
        }
    }
    total / (to - from).max(1e-9)
}

/// The tables `layer`'s rules draw, with each cell's lines. A grid that
/// does not hold its text in cells (a line crossing a border, or text
/// beside text in one cell) is not a table.
pub fn page_tables(layer: &PageTextLayer) -> Vec<PageTable> {
    if layer.rules.is_empty() {
        return Vec::new();
    }
    // A page with more rules than are looked at (a drawing's line work):
    // the longest, which borders are among.
    let mut rules = layer.rules.clone();
    if rules.len() > MAX_RULES {
        rules.sort_by(|a, b| (b.to - b.from).total_cmp(&(a.to - a.from)));
        rules.truncate(MAX_RULES);
    }
    let merged = merge_rules(rules);
    let crosses = |h: &PageRule, v: &PageRule| {
        let tolerance = CROSS_TOLERANCE + h.width.max(v.width);
        v.at >= h.from - tolerance
            && v.at <= h.to + tolerance
            && h.at >= v.from - tolerance
            && h.at <= v.to + tolerance
    };
    let horizontal = merged
        .iter()
        .filter(|rule| rule.horizontal)
        .collect::<Vec<_>>();
    let vertical = merged
        .iter()
        .filter(|rule| !rule.horizontal)
        .collect::<Vec<_>>();
    if horizontal.len() < 2 || vertical.is_empty() {
        return Vec::new();
    }
    let count = horizontal.len() + vertical.len();
    let rule = |index: usize| {
        if index < horizontal.len() {
            horizontal[index]
        } else {
            vertical[index - horizontal.len()]
        }
    };
    let mut crossings = vec![Vec::new(); count];
    for (h_index, h) in horizontal.iter().enumerate() {
        for (v_index, v) in vertical.iter().enumerate() {
            if crosses(h, v) {
                crossings[h_index].push(horizontal.len() + v_index);
                crossings[horizontal.len() + v_index].push(h_index);
            }
        }
    }
    // A border runs from one crossing rule to another. A rule crossing
    // fewer than two (a drawing's stroke over a table's border) is not one,
    // unless it reaches well past the one it crosses on both sides (an
    // open-sided table's rule).
    let mut kept = vec![true; count];
    let mut pending = (0..count).collect::<Vec<_>>();
    while let Some(index) = pending.pop() {
        if !kept[index] {
            continue;
        }
        let crossing = crossings[index]
            .iter()
            .copied()
            .filter(|other| kept[*other])
            .collect::<Vec<_>>();
        let border = match crossing.as_slice() {
            [] => false,
            [other] => {
                let (this, other) = (rule(index), rule(*other));
                (other.at - this.from).min(this.to - other.at) >= STRAY_REACH
            }
            _ => true,
        };
        if !border {
            kept[index] = false;
            pending.extend(crossing);
        }
    }
    let mut parent = (0..count).collect::<Vec<_>>();
    for (index, crossing) in crossings.iter().enumerate() {
        for other in crossing {
            if kept[index] && kept[*other] {
                union(&mut parent, index, *other);
            }
        }
    }
    let mut components: std::collections::BTreeMap<usize, (Vec<&PageRule>, Vec<&PageRule>)> =
        std::collections::BTreeMap::new();
    for index in (0..count).filter(|index| kept[*index]) {
        let root = find(&mut parent, index);
        let entry = components.entry(root).or_default();
        if index < horizontal.len() {
            entry.0.push(horizontal[index]);
        } else {
            entry.1.push(vertical[index - horizontal.len()]);
        }
    }
    let mut tables = components
        .into_values()
        .filter_map(|(horizontal, vertical)| grid(layer, &horizontal, &vertical))
        .collect::<Vec<_>>();
    // A table inside another's cell: the outer is not a table of its text.
    let contains = |outer: &PageTable, inner: &PageTable| {
        let (a, b) = (outer.bounds(), inner.bounds());
        a != b
            && a[0] <= b[0] + 0.5
            && a[1] <= b[1] + 0.5
            && a[2] >= b[2] - 0.5
            && a[3] >= b[3] - 0.5
    };
    let nested = tables
        .iter()
        .map(|outer| tables.iter().any(|inner| contains(outer, inner)))
        .collect::<Vec<_>>();
    let mut index = 0;
    tables.retain(|_| {
        index += 1;
        !nested[index - 1]
    });
    tables.sort_by(|a, b| {
        b.rows[0]
            .total_cmp(&a.rows[0])
            .then(a.columns[0].total_cmp(&b.columns[0]))
    });
    tables
}

fn grid(
    layer: &PageTextLayer,
    horizontal: &[&PageRule],
    vertical: &[&PageRule],
) -> Option<PageTable> {
    if horizontal.len() < 2 || vertical.is_empty() {
        return None;
    }
    let mut xs = boundaries(vertical.iter().map(|rule| rule.at).collect());
    let mut ys = boundaries(horizontal.iter().map(|rule| rule.at).collect());
    // A table open at its sides or ends: its rules reach past the last
    // crossing rule, at least half of them (one lengthened by a stroke
    // drawn in line with it does not open the table). The table ends where
    // the shortest of them does.
    let reach = |rules: &[&PageRule], last: f64, end: bool| {
        let past = rules
            .iter()
            .map(|rule| if end { rule.to } else { rule.from })
            .filter(|reach| {
                (reach - last).abs() > BOUNDARY_TOLERANCE * 2. && (*reach > last) == end
            })
            .collect::<Vec<_>>();
        if past.len() < 2 || past.len() * 2 < rules.len() {
            return None;
        }
        past.into_iter()
            .reduce(if end { f64::min } else { f64::max })
    };
    if let Some(left) = reach(horizontal, xs[0], false) {
        xs.insert(0, left);
    }
    if let Some(right) = reach(horizontal, xs[xs.len() - 1], true) {
        xs.push(right);
    }
    if let Some(bottom) = reach(vertical, ys[0], false) {
        ys.insert(0, bottom);
    }
    if let Some(top) = reach(vertical, ys[ys.len() - 1], true) {
        ys.push(top);
    }
    ys.reverse();
    let (rows, columns) = (ys.len().checked_sub(1)?, xs.len().checked_sub(1)?);
    if rows == 0
        || columns == 0
        || rows * columns < 2
        || rows * columns > MAX_TABLE_UNITS
        || xs[columns] - xs[0] < MIN_TABLE_WIDTH
        || ys[0] - ys[rows] < MIN_TABLE_HEIGHT
    {
        return None;
    }
    // Units with no border between them are one cell.
    let unit = |row: usize, column: usize| row * columns + column;
    let mut parent = (0..rows * columns).collect::<Vec<_>>();
    for row in 0..rows {
        for column in 0..columns {
            if column + 1 < columns
                && coverage(vertical, xs[column + 1], ys[row + 1], ys[row]) < BORDER_COVERAGE
            {
                union(&mut parent, unit(row, column), unit(row, column + 1));
            }
            if row + 1 < rows
                && coverage(horizontal, ys[row + 1], xs[column], xs[column + 1]) < BORDER_COVERAGE
            {
                union(&mut parent, unit(row, column), unit(row + 1, column));
            }
        }
    }
    let mut extents: std::collections::BTreeMap<usize, (usize, usize, usize, usize, usize)> =
        std::collections::BTreeMap::new();
    for row in 0..rows {
        for column in 0..columns {
            let root = find(&mut parent, unit(row, column));
            let entry = extents.entry(root).or_insert((row, column, row, column, 0));
            entry.0 = entry.0.min(row);
            entry.1 = entry.1.min(column);
            entry.2 = entry.2.max(row);
            entry.3 = entry.3.max(column);
            entry.4 += 1;
        }
    }
    let mut cells = Vec::new();
    for (top, left, bottom, right, units) in extents.into_values() {
        let (row_span, column_span) = (bottom - top + 1, right - left + 1);
        // A cell is a rectangle of units.
        if row_span * column_span != units {
            return None;
        }
        cells.push(TableCell {
            row: top,
            column: left,
            row_span,
            column_span,
            lines: Vec::new(),
        });
    }
    if cells.len() < 2 {
        return None;
    }
    cells.sort_by_key(|cell| (cell.row, cell.column));
    let mut table = PageTable {
        columns: xs,
        rows: ys,
        cells,
        read_only: None,
    };
    let [left, bottom, right, top] = table.bounds();
    let mut inside = Vec::new();
    for line in &layer.lines {
        let (Some(bounds), Some(core)) = (line_box(line), line_core_box(line)) else {
            continue;
        };
        let (x, y) = ((core[0] + core[2]) / 2., (core[1] + core[3]) / 2.);
        if x < left || x > right || y < bottom || y > top {
            continue;
        }
        let cell_index = table.cell_at(x, y)?;
        let cell_bounds = table.cell_bounds(&table.cells[cell_index]);
        // A line crossing a border is not in a cell: the grid is not a
        // table of this text.
        if bounds[0] < cell_bounds[0] - BAND_TOLERANCE * 2.
            || bounds[2] > cell_bounds[2] + BAND_TOLERANCE * 2.
        {
            return None;
        }
        inside.push((line, bounds, cell_index));
    }
    let mut boxes: Vec<Vec<[f64; 4]>> = vec![Vec::new(); table.cells.len()];
    for (line, bounds, cell_index) in inside {
        if !horizontal_line(line) {
            table.read_only = Some("the table has text that is not upright".into());
        }
        // Text beside text in one cell: the grid is missing its columns.
        if boxes[cell_index].iter().any(|other| {
            other[1].max(bounds[1]) < other[3].min(bounds[3]) - 0.25 * (bounds[3] - bounds[1])
                && (other[2] < bounds[0] || bounds[2] < other[0])
        }) {
            return None;
        }
        boxes[cell_index].push(bounds);
        if line.read_only.is_some() && table.read_only.is_none() {
            table.read_only = line.read_only.clone();
        }
        if line.in_form && table.read_only.is_none() {
            table.read_only = Some("the table's text is drawn by a form".into());
        }
        table.cells[cell_index].lines.push(line.anchor);
    }
    // A grid with no text is a drawing's.
    if table.cells.iter().all(|cell| cell.lines.is_empty()) {
        return None;
    }
    for (cell, boxes) in table.cells.iter_mut().zip(&boxes) {
        let mut order = cell.lines.iter().copied().zip(boxes).collect::<Vec<_>>();
        order.sort_by(|a, b| b.1[3].total_cmp(&a.1[3]).then(a.1[0].total_cmp(&b.1[0])));
        cell.lines = order.into_iter().map(|(anchor, _)| anchor).collect();
    }
    Some(table)
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
        }
    }

    /// A 3 x 2 grid from (100, 100) to (300, 160): rows 20 high, columns
    /// 100 wide, with a line of text in each cell.
    fn grid_layer() -> PageTextLayer {
        let mut rules = Vec::new();
        for y in [160., 140., 120., 100.] {
            rules.push(h(y, 100., 300.));
        }
        for x in [100., 200., 300.] {
            rules.push(v(x, 100., 160.));
        }
        let mut lines = Vec::new();
        for (row, y) in [146., 126., 106.].into_iter().enumerate() {
            lines.push(line(104., y, &format!("r{row}c0"), 0));
            lines.push(line(204., y, &format!("r{row}c1"), 0));
        }
        PageTextLayer {
            page_index: 0,
            lines,
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
    fn a_ruled_grid_is_a_table_with_its_text_in_cells() {
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
    fn an_empty_grid_is_a_drawing() {
        let layer = PageTextLayer {
            page_index: 1,
            lines: vec![line(100., 700., "If you have any queries", 0)],
            rules: vec![
                h(791.5, 40., 574.),
                h(778.5, 40., 574.),
                v(40., 778., 792.),
                v(170., 778., 792.),
                v(574., 778., 792.),
            ],
            ..Default::default()
        };
        assert!(page_tables(&layer).is_empty());
    }

    #[test]
    fn per_cell_border_pieces_join_and_a_missing_border_merges_cells() {
        let mut layer = grid_layer();
        // Chrome draws each cell's border separately; the middle border of
        // the first row is missing, so its two cells are one.
        layer.rules = Vec::new();
        for y in [160., 140., 120., 100.] {
            layer.rules.push(h(y, 100., 200.));
            layer.rules.push(h(y, 200.5, 300.));
        }
        for (from, to) in [(140., 160.), (120., 140.), (100., 120.)] {
            layer.rules.push(v(100., from, to));
            layer.rules.push(v(300., from, to));
            if from < 140. {
                layer.rules.push(v(200., from, to));
            }
        }
        layer.lines.remove(1);
        let tables = page_tables(&layer);
        assert_eq!(tables.len(), 1);
        let table = &tables[0];
        assert_eq!(table.cells.len(), 5);
        assert_eq!(
            (table.cells[0].row_span, table.cells[0].column_span),
            (1, 2)
        );
    }

    #[test]
    fn a_line_across_a_border_or_text_beside_text_is_not_a_table() {
        let mut layer = grid_layer();
        layer.lines[0] = line(104., 146., "this text is far too long for one cell", 0);
        assert!(page_tables(&layer).is_empty());
        let mut layer = grid_layer();
        // No middle column rule: each row's two lines sit side by side.
        layer
            .rules
            .retain(|rule| rule.horizontal || rule.at != 200.);
        assert!(page_tables(&layer).is_empty());
    }

    #[test]
    fn a_framed_paragraph_is_not_a_table() {
        let mut layer = grid_layer();
        layer.rules = vec![
            h(160., 100., 300.),
            h(100., 100., 300.),
            v(100., 100., 160.),
            v(300., 100., 160.),
        ];
        layer.lines.truncate(1);
        assert!(page_tables(&layer).is_empty());
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

    /// A drawing's worth of rules (a CAD sheet's grid and hatching) is
    /// looked at quickly, whether or not it is a table.
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
        let layer = PageTextLayer {
            rules,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        assert_eq!(merge_rules(layer.rules.clone()).len(), 8000);
        let _ = page_tables(&layer);
        let elapsed = started.elapsed();
        eprintln!("{elapsed:?}");
        assert!(elapsed.as_secs() < 2, "{elapsed:?}");
    }
}
