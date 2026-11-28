//! Groups a page's editable text lines into paragraph blocks, as Acrobat
//! and OnlyOffice present text for editing. A block is a run of lines in one
//! column with the same orientation and size, an even line pitch and a
//! shared edge (left, right, centre, or both for justified text). A list's
//! items, with their wrapped lines at the items' text indent and the lists
//! nested in them, form one block when the page's layout makes the list
//! certain ([`crate::pdf_text_lists::list_confidence`]); a paragraph that
//! runs off the foot of a column and continues at the top of the next is
//! one block over both.
//!
//! A tagged page's paragraphs are its structure tree's. Lines the tree puts
//! in no paragraph (all of an untagged page's) join only on firm layout
//! evidence: a line that wraps onto the next, as a word processor wraps
//! (see [`untagged_blocks`]).

use serde::{Deserialize, Serialize};

use crate::pdf_text_edit::{PageTextLayer, PageTextLine, TextLineAnchor, TextLineChar};
use crate::pdf_text_lists::{
    ALIGN_EM, Confidence, ItemShape, MAX_ITEM_PITCH_EM, MarkerReading, SMALL_MARKER,
    list_confidence, read_marker, strong_bullet, text_marker,
};

/// The page's list items: each item's first line, with its list's marker.
type ListItems = std::collections::HashMap<TextLineAnchor, ListMarker>;

/// Lines further apart than this many ems start a new block: line pitch is
/// usually 1–1.6 em, and paragraph spacing adds to it.
const MAX_PITCH_EM: f64 = 1.75;
const MIN_PITCH_EM: f64 = 0.9;
/// How much a block's line pitch may vary.
const PITCH_TOLERANCE: f64 = 0.12;
/// How far shared edges may differ, in ems (at least a point).
const EDGE_TOLERANCE_EM: f64 = 0.15;
/// Letters that are bullets only when drawn in a font of their own (Word's
/// "o" in Courier New, Wingdings' square read as "§").
const GLYPH_BULLETS: [char; 2] = ['o', '§'];
/// A column's last line continues in the next column when it ends this
/// close to the column's right edge, as a fraction of its width.
const FULL_LINE: f64 = 0.85;
/// Columns a paragraph continues between are as wide as each other, to
/// this share of the first's width (ragged columns end unevenly).
const COLUMN_WIDTH_TOLERANCE: f64 = 0.15;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum TextBlockAlignment {
    Left,
    Right,
    Center,
    Justified,
}

pub use crate::pdf_text_lists::{ListMarker, NumberStyle};

/// The most list levels a block knows.
pub const MAX_LIST_LEVELS: usize = 6;

/// One level of a list: its marker, and where its items' marker and text
/// start, from the list's marker edge.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct ListLevel {
    pub marker: ListMarker,
    /// From the list's marker edge to this level's, along the lines.
    pub offset: f64,
    /// From this level's marker edge to its text.
    pub indent: f64,
}

/// A block whose lines are list items.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextList {
    pub marker: ListMarker,
    /// From an item's marker to where its wrapped lines start, along the
    /// lines in page units.
    pub indent: f64,
    /// The levels the list's items may take, by offset: its own (offset 0,
    /// `marker` and `indent`), those nested in it and those the page shows
    /// around it. Each level has a marker of its own, which tells an item's
    /// level. Empty: only its own.
    #[serde(default, skip_serializing_if = "no_levels")]
    pub levels: [Option<ListLevel>; MAX_LIST_LEVELS],
}

fn no_levels(levels: &[Option<ListLevel>; MAX_LIST_LEVELS]) -> bool {
    levels.iter().all(Option::is_none)
}

impl TextList {
    /// A list of one level.
    pub fn new(marker: ListMarker, indent: f64) -> Self {
        Self {
            marker,
            indent,
            levels: [None; MAX_LIST_LEVELS],
        }
    }

    /// The list's levels, by offset; its own when it knows no others.
    pub fn levels(&self) -> Vec<ListLevel> {
        let mut levels = self.levels.iter().flatten().copied().collect::<Vec<_>>();
        if !levels.iter().any(|level| level.marker == self.marker) {
            levels.push(ListLevel {
                marker: self.marker,
                offset: 0.,
                indent: self.indent,
            });
        }
        levels.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        levels
    }

    /// The level whose marker is `marker`.
    pub fn level_of(&self, marker: ListMarker) -> Option<ListLevel> {
        self.levels()
            .into_iter()
            .find(|level| level.marker == marker)
    }

    /// The list with `levels` (at most [`MAX_LIST_LEVELS`], by offset).
    pub fn with_levels(mut self, levels: &[ListLevel]) -> Self {
        self.levels = [None; MAX_LIST_LEVELS];
        for (slot, level) in self.levels.iter_mut().zip(levels) {
            *slot = Some(*level);
        }
        self
    }

    /// The markers of its levels.
    pub fn kinds(&self) -> Vec<ListMarker> {
        self.levels()
            .into_iter()
            .map(|level| level.marker)
            .collect()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageTextBlock {
    /// In reading order: top to bottom, then the next column's.
    pub lines: Vec<TextLineAnchor>,
    pub alignment: TextBlockAlignment,
    /// The block's extent along its lines in its first column, in page
    /// units from the page origin along the text direction.
    pub left: f64,
    pub right: f64,
    /// The distance between baselines; `None` for a single line.
    pub pitch: Option<f64>,
    /// Each line's column edges, as `left` and `right` are the first's; a
    /// list item's left edge is its marker's. Empty: every line's are
    /// `left` and `right`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<(f64, f64)>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<TextList>,
    /// The table cell the block is in: its text wraps within the cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<BlockCell>,
    /// Untagged text in a box the page rules on all four sides (a table
    /// the PDF does not declare): where its lines may run along them, as a
    /// cell's text runs within its cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruled: Option<(f64, f64)>,
}

/// A table cell holding a block: the table, the cell's index in it, and
/// where the cell's text may run along its lines, in page space.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct BlockCell {
    pub table: crate::pdf_tables::TableKey,
    pub cell: usize,
    pub left: f64,
    pub right: f64,
}

impl PageTextBlock {
    /// Line `index`'s column edges.
    pub fn edges_of(&self, index: usize) -> (f64, f64) {
        self.edges
            .get(index)
            .copied()
            .unwrap_or((self.left, self.right))
    }
}

/// A line's place in page space, along and across its direction.
struct Placed<'a> {
    line: &'a PageTextLine,
    /// The font most of its visible characters use.
    font: &'a [u8],
    direction: (f64, f64),
    em: f64,
    /// Along the direction.
    left: f64,
    right: f64,
    /// Across it: larger is higher on the page.
    baseline: f64,
    /// The marker it begins with, whether or not it is in a list.
    raw: Option<RawMarker>,
    /// The list marker it begins with, when it is an item of a list
    /// ([`find_lists`]), and where its text after the marker starts.
    marker: Option<(ListMarker, f64)>,
}

/// A marker a line begins with, and where it is drawn.
#[derive(Clone, Copy)]
struct RawMarker {
    reading: MarkerReading,
    strong: bool,
    small: bool,
    leader: bool,
    marker_right: f64,
    text_left: f64,
}

fn normalized(x: f64, y: f64) -> Option<(f64, f64)> {
    let length = x.hypot(y);
    (length > 1e-9).then(|| (x / length, y / length))
}

fn place(line: &PageTextLine) -> Option<Placed<'_>> {
    let m = line.matrix;
    let direction = normalized(m[0], m[1])?;
    let normal = (-direction.1, direction.0);
    let mut visible = line
        .chars
        .iter()
        .filter(|character| !character.character.is_whitespace());
    let first = visible.next()?;
    let last = visible.next_back().unwrap_or(first);
    let style = &line.styles[first.style];
    let mut counts = std::collections::BTreeMap::<&[u8], usize>::new();
    for character in line
        .chars
        .iter()
        .filter(|character| !character.character.is_whitespace())
    {
        *counts
            .entry(&line.styles[character.style].font)
            .or_default() += 1;
    }
    let font = counts.into_iter().max_by_key(|(_, count)| *count)?.0;
    let at = |x: f64| {
        let (px, py) = (m[0] * x + m[4], m[1] * x + m[5]);
        px * direction.0 + py * direction.1
    };
    let raw = list_marker(line).map(|(reading, strong, text_start)| {
        let text = line.chars[text_start..]
            .iter()
            .find(|character| !character.character.is_whitespace());
        let marker_end = &line.chars[text_start - 1];
        let size = |character: &TextLineChar| line.styles[character.style].font_size.abs();
        RawMarker {
            reading,
            strong,
            small: text.is_some_and(|text| size(first) < SMALL_MARKER * size(text)),
            leader: crate::pdf_text_lists::has_leader(&line.text),
            marker_right: at(marker_end.x + marker_end.advance),
            text_left: text.map_or(at(last.x + last.advance), |text| at(text.x)),
        }
    });
    let (origin_x, origin_y) = line.origin();
    Some(Placed {
        line,
        font,
        direction,
        em: style.font_size.abs() * m[2].hypot(m[3]),
        left: at(first.x),
        right: at(last.x + last.advance),
        baseline: origin_x * normal.0 + origin_y * normal.1 + line.rise * m[3].hypot(m[2]),
        raw,
        marker: None,
    })
}

/// `line` placed, with its marker when `lists` (from [`find_lists`]) makes
/// it a list item.
fn place_in<'a>(line: &'a PageTextLine, lists: &ListItems) -> Option<Placed<'a>> {
    let mut placed = place(line)?;
    if let (Some(raw), Some(marker)) = (placed.raw, lists.get(&line.anchor)) {
        placed.marker = Some((*marker, raw.text_left));
    }
    Some(placed)
}

/// The marker `line` begins with, whether it is a bullet only a list
/// draws, and the index of the character after it. The marker is set off
/// from text after it.
fn list_marker(line: &PageTextLine) -> Option<(MarkerReading, bool, usize)> {
    let start = line
        .chars
        .iter()
        .position(|character| !character.character.is_whitespace())?;
    let chars = &line.chars[start..];
    let text = chars
        .iter()
        .map(|character| character.character)
        .take(12)
        .collect::<Vec<_>>();
    let separated = |index: usize| {
        chars
            .get(index)
            .is_some_and(|character| character.character.is_whitespace())
            || chars.get(index).is_some_and(|character| {
                // A gap with no space drawn: the text starts a tab away.
                let previous = &chars[index - 1];
                character.x - (previous.x + previous.advance)
                    > 0.2 * line.styles[previous.style].font_size.abs()
            })
    };
    let first = chars[0].character;
    // A letter drawn as a bullet in its own font, as Word's second-level
    // "o" in Courier New.
    if GLYPH_BULLETS.contains(&first) && separated(1) {
        let font = |character: &TextLineChar| &line.styles[character.style].font;
        let text = chars[1..]
            .iter()
            .find(|character| !character.character.is_whitespace());
        if text.is_some_and(|text| font(text) != font(&chars[0])) {
            let reading = MarkerReading {
                marker: ListMarker::Bullet(first),
                value: 0,
                len: 1,
                alternative: None,
            };
            return Some((reading, true, start + 1));
        }
    }
    let reading = read_marker(&text)?;
    // Text follows the marker.
    if reading.len >= chars.len() || !separated(reading.len) {
        return None;
    }
    let strong = matches!(reading.marker, ListMarker::Bullet(bullet) if strong_bullet(bullet));
    Some((reading, strong, start + reading.len))
}

/// The marker a list item begins with in `text`, and its length with the
/// spaces after it: a marker of `list`'s levels, or with no list a bullet
/// or a decimal number.
pub fn list_item_marker(text: &[char], list: Option<&TextList>) -> Option<(ListMarker, usize)> {
    list_item(text, list).map(|(marker, _, length)| (marker, length))
}

/// [`list_item_marker`] with the ordinal's value.
pub fn list_item(text: &[char], list: Option<&TextList>) -> Option<(ListMarker, u32, usize)> {
    let kinds = list.map(TextList::kinds).unwrap_or_default();
    // A letter drawn as a bullet reads as a word in plain text: it counts
    // only as the list's own bullet.
    if let Some(&first) = text.first()
        && kinds.contains(&ListMarker::Bullet(first))
    {
        let gap = text[1..]
            .iter()
            .take_while(|character| **character == ' ' || **character == '\u{a0}')
            .count();
        if gap > 0 {
            return Some((ListMarker::Bullet(first), 0, 1 + gap));
        }
    }
    text_marker(text, &kinds)
}

/// An item of a run of list items: its first line, its wrapped lines.
struct RunItem<'p> {
    line: &'p Placed<'p>,
    raw: RawMarker,
    wrapped: Vec<&'p Placed<'p>>,
    followed_at_marker: bool,
    /// Its lowest line, or the lowest line of a list nested in it.
    seen_bottom: f64,
}

impl RunItem<'_> {
    fn bottom(&self) -> f64 {
        self.wrapped.last().unwrap_or(&self.line).baseline
    }
}

/// Lines that may be a list's items, one below another.
struct Run<'p> {
    items: Vec<RunItem<'p>>,
    open: bool,
    right: f64,
}

impl<'p> Run<'p> {
    fn first(&self) -> &RunItem<'p> {
        &self.items[0]
    }

    fn last(&self) -> &RunItem<'p> {
        self.items.last().expect("a run has an item")
    }

    fn marker_left(&self) -> f64 {
        self.first().line.left
    }

    /// Where its markers start furthest left: right-aligned numbers start
    /// at different places.
    fn marker_edge(&self) -> f64 {
        self.items
            .iter()
            .map(|item| item.line.left)
            .fold(f64::MAX, f64::min)
    }

    fn text_left(&self) -> f64 {
        self.first().raw.text_left
    }

    fn em(&self) -> f64 {
        self.first().line.em.max(1e-3)
    }

    fn seen_bottom(&self) -> f64 {
        self.last().seen_bottom
    }

    fn top(&self) -> f64 {
        self.first().line.baseline
    }

    fn compatible(&self, line: &Placed<'_>) -> bool {
        let direction = self.first().line.direction;
        direction.0 * line.direction.0 + direction.1 * line.direction.1 > 0.999
            && (line.em - self.em()).abs() <= 0.1 * self.em()
    }

    fn shapes(&self) -> Vec<ItemShape> {
        self.items
            .iter()
            .map(|item| ItemShape {
                reading: item.raw.reading,
                strong: item.raw.strong,
                marker_left: item.line.left,
                marker_right: item.raw.marker_right,
                text_left: item.raw.text_left,
                em: item.line.em,
                first_baseline: item.line.baseline,
                last_baseline: item.seen_bottom,
                hanging: (!item.wrapped.is_empty()).then_some(true),
                followed_at_marker: item.followed_at_marker,
                small: item.raw.small,
                leader: item.raw.leader,
            })
            .collect()
    }
}

/// A list found on the page, with the lists nested in it that have
/// markers of their own.
struct FoundList {
    /// Each item's first line, with its level's marker.
    items: Vec<(TextLineAnchor, ListMarker)>,
    /// Every line of the list and of the lists nested in it, top to bottom.
    members: Vec<TextLineAnchor>,
    /// Not merged into another list.
    root: bool,
    /// Its levels, from its own (offset 0), by offset.
    levels: Vec<ListLevel>,
    marker: ListMarker,
    indent: f64,
}

/// The lists among `placed` (one scope: the page outside tables, or one
/// cell). A run of lines whose markers share their edges and count on,
/// each with its wrapped lines at the items' text, is a list when
/// [`list_confidence`] is confident. A run nested in a list's item (its
/// markers further in, between the item and the next) is a list when it
/// could be one at all, and joins the list as a level when its marker
/// differs from the list's other levels'.
fn find_lists<'p>(
    placed: &'p [Placed<'p>],
    rules: &[crate::pdf_tables::PageRule],
    log: Option<&mut Vec<String>>,
) -> Vec<FoundList> {
    let mut order = placed.iter().collect::<Vec<_>>();
    order.sort_by(|a, b| {
        b.baseline
            .total_cmp(&a.baseline)
            .then(a.left.total_cmp(&b.left))
    });
    let mut runs: Vec<Run<'p>> = Vec::new();
    for line in order {
        // A table of contents entry has its page number (and a dot leader)
        // further along its baseline; a table row's cells are ruled apart.
        let line_raw = line.raw.map(|mut raw| {
            let tabular = placed.iter().any(|other| {
                (other.baseline - line.baseline).abs() < 0.3 * line.em
                    && other.left > line.right
                    && page_reference(other.line.text.trim())
            }) || upright(line)
                && rules.iter().any(|rule| {
                    !rule.horizontal
                        && rule.at > line.left
                        && rule.at < line.right
                        && rule.from <= line.baseline
                        && rule.to >= line.baseline
                });
            raw.leader |= tabular;
            raw
        });
        let em = line.em.max(1e-3);
        let tolerance = (ALIGN_EM * em).max(1.);
        let taken;
        if let Some(raw) = line_raw {
            // The next item of a run.
            taken = runs
                .iter()
                .enumerate()
                .filter(|(_, run)| run.open && run.compatible(line))
                .filter_map(|(index, run)| {
                    let markers = (line.left - run.marker_left()).abs() <= tolerance
                        || raw.reading.marker.is_number()
                            && crate::pdf_text_lists::ordinals_aligned(
                                (run.marker_left(), run.first().raw.marker_right),
                                (line.left, raw.marker_right),
                                em,
                            );
                    let text = crate::pdf_text_lists::text_aligned(
                        run.text_left(),
                        raw.marker_right,
                        raw.text_left,
                        em,
                    ) || crate::pdf_text_lists::text_aligned(
                        raw.text_left,
                        run.first().raw.marker_right,
                        run.text_left(),
                        em,
                    );
                    let pitch = run.seen_bottom() - line.baseline;
                    let readings = run
                        .items
                        .iter()
                        .map(|item| item.raw.reading)
                        .chain([raw.reading])
                        .collect::<Vec<_>>();
                    (markers
                        && text
                        && (0.8 * em..=MAX_ITEM_PITCH_EM * em).contains(&pitch)
                        && crate::pdf_text_lists::counted_marker(&readings).is_some())
                    .then_some((index, pitch))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, _)| index);
            if let Some(index) = taken {
                runs[index].items.push(RunItem {
                    line,
                    raw,
                    wrapped: Vec::new(),
                    followed_at_marker: false,
                    seen_bottom: line.baseline,
                });
            }
        } else {
            // A wrapped line of a run's last item, at the items' text.
            let line_pitch = |run: &Run<'_>| {
                let pitch = run.last().bottom() - line.baseline;
                let unbroken = (run.seen_bottom() - run.last().bottom()).abs() < 1e-6;
                (unbroken && (MIN_PITCH_EM * em..=MAX_PITCH_EM * em).contains(&pitch))
                    .then_some(pitch)
            };
            taken = runs
                .iter()
                .enumerate()
                .filter(|(_, run)| run.open && run.compatible(line))
                .filter(|(_, run)| (line.left - run.text_left()).abs() <= tolerance)
                .filter_map(|(index, run)| line_pitch(run).map(|pitch| (index, pitch)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(index, _)| index);
            if let Some(index) = taken {
                let item = runs[index].items.last_mut().expect("a run has an item");
                item.wrapped.push(line);
                item.seen_bottom = line.baseline;
            } else {
                for run in runs
                    .iter_mut()
                    .filter(|run| run.open && run.compatible(line))
                {
                    if (line.left - run.marker_left()).abs() <= tolerance
                        && run.last().wrapped.is_empty()
                        && line_pitch(run).is_some()
                    {
                        run.items
                            .last_mut()
                            .expect("a run has an item")
                            .followed_at_marker = true;
                    }
                }
            }
        }
        // Text below a run's last item ends the run, unless it is further
        // in than the items' text (a list nested in the item).
        for (index, run) in runs.iter_mut().enumerate() {
            if Some(index) == taken || !run.open {
                continue;
            }
            let overlaps = line.left < run.right && line.right > run.marker_left();
            if !overlaps || line.baseline >= run.seen_bottom() - 0.5 {
                continue;
            }
            let run_tolerance = (ALIGN_EM * run.em()).max(1.);
            if line.left >= run.text_left() - run_tolerance
                && run.seen_bottom() - line.baseline <= MAX_ITEM_PITCH_EM * run.em()
            {
                run.items.last_mut().expect("a run has an item").seen_bottom = line.baseline;
                run.right = run.right.max(line.right);
            } else {
                run.open = false;
            }
        }
        match taken {
            Some(index) => runs[index].right = runs[index].right.max(line.right),
            None => {
                if let Some(raw) = line_raw {
                    runs.push(Run {
                        items: vec![RunItem {
                            line,
                            raw,
                            wrapped: Vec::new(),
                            followed_at_marker: false,
                            seen_bottom: line.baseline,
                        }],
                        open: true,
                        right: line.right,
                    });
                }
            }
        }
    }
    let verdicts = runs
        .iter()
        .map(|run| list_confidence(&run.shapes()))
        .collect::<Vec<_>>();
    if let Some(log) = log {
        for (run, verdict) in runs.iter().zip(&verdicts) {
            let text = run.first().line.line.text.trim();
            log.push(format!(
                "{:?}\t{}\t{} items\t{}",
                verdict.confidence,
                verdict.reason,
                run.items.len(),
                text.chars().take(60).collect::<String>()
            ));
        }
    }
    // The list each run is nested in: the confident run with its markers
    // furthest in that are still left of the run's, whose items span it.
    let parents = runs
        .iter()
        .map(|run| {
            let tolerance = (ALIGN_EM * run.em()).max(1.);
            runs.iter()
                .enumerate()
                .filter(|(index, parent)| {
                    verdicts[*index].confidence == Confidence::High
                        && parent.compatible(run.first().line)
                        && parent.marker_left() + tolerance < run.marker_left()
                        && run.marker_left() < parent.right
                        && run.top() < parent.top()
                        && run.top() >= parent.seen_bottom() - 0.5
                })
                .max_by(|a, b| a.1.marker_left().total_cmp(&b.1.marker_left()))
                .map(|(index, _)| index)
        })
        .collect::<Vec<_>>();
    let accepted = (0..runs.len())
        .map(|index| match verdicts[index].confidence {
            Confidence::High => true,
            Confidence::Low => verdicts[index].marker.is_some() && parents[index].is_some(),
            Confidence::None => false,
        })
        .collect::<Vec<_>>();
    // Merge nested runs into their list's tree while each level keeps a
    // marker of its own at one place.
    let mut merged_into = vec![None; runs.len()];
    let mut trees: Vec<(usize, Vec<ListLevel>, Vec<usize>)> = Vec::new();
    for index in 0..runs.len() {
        if !accepted[index] {
            continue;
        }
        let parent_tree = parents[index]
            .filter(|parent| accepted[*parent])
            .and_then(|parent| merged_into[parent]);
        let marker = verdicts[index]
            .marker
            .expect("an accepted run has a marker");
        let run = &runs[index];
        let tolerance = (ALIGN_EM * run.em()).max(1.);
        let joined = parent_tree.is_some_and(|tree: usize| {
            let (root, levels, members) = &mut trees[tree];
            let offset = run.marker_edge() - runs[*root].marker_edge();
            // Right-aligned ordinals start further left the wider they are.
            let tolerance = if marker.is_number() {
                tolerance.max(1.5 * run.em())
            } else {
                tolerance
            };
            match levels.iter().find(|level| level.marker == marker) {
                Some(level) if (level.offset - offset).abs() > tolerance => false,
                Some(level) => {
                    // The level starts where its furthest-left marker does.
                    if offset < level.offset {
                        let level = levels
                            .iter_mut()
                            .find(|level| level.marker == marker)
                            .expect("the level was found");
                        level.indent += level.offset - offset;
                        level.offset = offset;
                    }
                    members.push(index);
                    true
                }
                None if levels.len() < MAX_LIST_LEVELS => {
                    levels.push(ListLevel {
                        marker,
                        offset,
                        indent: run.text_left() - run.marker_edge(),
                    });
                    members.push(index);
                    true
                }
                None => false,
            }
        });
        if joined {
            merged_into[index] = parent_tree;
        } else if verdicts[index].confidence == Confidence::High {
            merged_into[index] = Some(trees.len());
            trees.push((
                index,
                vec![ListLevel {
                    marker,
                    offset: 0.,
                    indent: run.text_left() - run.marker_edge(),
                }],
                vec![index],
            ));
        }
    }
    trees
        .into_iter()
        .map(|(root, mut levels, members)| {
            levels.sort_by(|a, b| a.offset.total_cmp(&b.offset));
            let mut items = Vec::new();
            let mut lines: Vec<&Placed<'_>> = Vec::new();
            for index in &members {
                let marker = verdicts[*index]
                    .marker
                    .expect("an accepted run has a marker");
                for item in &runs[*index].items {
                    items.push((item.line.line.anchor, marker));
                    lines.push(item.line);
                    lines.extend(item.wrapped.iter().copied());
                }
            }
            lines.sort_by(|a, b| {
                b.baseline
                    .total_cmp(&a.baseline)
                    .then(a.left.total_cmp(&b.left))
            });
            let run = &runs[root];
            FoundList {
                items,
                members: lines.iter().map(|line| line.line.anchor).collect(),
                root: true,
                levels,
                marker: verdicts[root].marker.expect("an accepted run has a marker"),
                indent: run.text_left() - run.marker_edge(),
            }
        })
        .collect()
}

/// The page's own list of a kind (`numbered` or bulleted), for a list the
/// format bar makes: its marker, the style its first marker is drawn in
/// (a bullet's font) and its indent in ems of that marker's size.
pub fn page_list_template(
    layer: &PageTextLayer,
    numbered: bool,
) -> Option<(ListMarker, crate::pdf_text_edit::TextRunStyle, f64)> {
    paragraph_blocks(layer).into_iter().find_map(|block| {
        let list = block.list?;
        if list.marker.is_number() != numbered {
            return None;
        }
        let line = layer.line(&block.lines[0])?;
        let first = line
            .chars
            .iter()
            .find(|character| !character.character.is_whitespace())?;
        let style = line.styles[first.style].clone();
        let em = style.font_size.abs() * line.matrix[2].hypot(line.matrix[3]);
        (em > 0.).then(|| (list.marker, style, list.indent / em))
    })
}

/// Each run of possible list items on the page outside tables, with the
/// verdict on it: for tuning the rule on a corpus.
#[doc(hidden)]
pub fn list_runs_report(layer: &PageTextLayer) -> Vec<String> {
    let placed = layer
        .lines
        .iter()
        .filter(|line| line.read_only.is_none())
        .filter_map(place)
        .collect::<Vec<_>>();
    let mut log = Vec::new();
    find_lists(&placed, &layer.rules, Some(&mut log));
    log
}

/// Whether `text` is only a page number, a dot leader, or both, as a table
/// of contents draws them after an entry.
fn page_reference(text: &str) -> bool {
    let text = text.trim_start_matches(|c: char| c.is_whitespace() || ".…·_".contains(c));
    !text.is_empty()
        && text.chars().count() <= 5
        && text.chars().all(|c| c.is_ascii_alphanumeric())
        && (text.chars().all(|c| c.is_ascii_digit())
            || text.chars().all(|c| "ivxlcIVXLC".contains(c)))
        || text.is_empty()
}

/// The block an untagged list found on the page makes: all its lines, its
/// levels' too.
fn list_block(
    list: &FoundList,
    members: &[&Placed<'_>],
    rules: &[crate::pdf_tables::PageRule],
) -> PageTextBlock {
    let left = members
        .iter()
        .map(|line| line.left)
        .fold(f64::MAX, f64::min);
    let right = members
        .iter()
        .map(|line| line.right)
        .fold(f64::MIN, f64::max);
    let item = |line: &Placed<'_>| {
        list.items
            .iter()
            .any(|(anchor, _)| *anchor == line.line.anchor)
    };
    // The line pitch: between an item's wrapped lines when it has some, else
    // between items.
    let pitch = members
        .windows(2)
        .find(|pair| !item(pair[1]))
        .or_else(|| members.windows(2).next())
        .map(|pair| pair[0].baseline - pair[1].baseline);
    // Justified: every line an item wraps from reaches the right edge.
    let em = members[0].em;
    let tolerance = (EDGE_TOLERANCE_EM * em).max(1.);
    let wrapped_from = members
        .windows(2)
        .filter(|pair| !item(pair[1]))
        .collect::<Vec<_>>();
    let justified = wrapped_from.len() >= 2
        && wrapped_from
            .iter()
            .all(|pair| (pair[0].right - right).abs() <= tolerance);
    let (top, bottom) = (members[0].baseline, members[members.len() - 1].baseline);
    let ruled = members
        .iter()
        .all(|line| upright(line))
        .then(|| ruled_box(rules, left, right, bottom, top))
        .flatten();
    PageTextBlock {
        lines: members.iter().map(|line| line.line.anchor).collect(),
        alignment: if justified {
            TextBlockAlignment::Justified
        } else {
            TextBlockAlignment::Left
        },
        left,
        right,
        pitch,
        edges: vec![(left, right); members.len()],
        list: Some(TextList::new(list.marker, list.indent).with_levels(&list.levels)),
        cell: None,
        ruled,
    }
}

/// `list` with the levels the page shows: a list found with levels nested
/// in it at the same marker, placed from `list`'s marker edge.
fn with_page_levels(list: TextList, found: &[FoundList]) -> TextList {
    if list.levels().len() > 1 {
        return list;
    }
    for tree in found.iter().filter(|tree| tree.levels.len() > 1) {
        if let Some(own) = tree.levels.iter().find(|level| level.marker == list.marker) {
            let levels = tree
                .levels
                .iter()
                .map(|level| ListLevel {
                    marker: level.marker,
                    offset: level.offset - own.offset,
                    indent: if level.marker == list.marker {
                        list.indent
                    } else {
                        level.indent
                    },
                })
                .collect::<Vec<_>>();
            return list.with_levels(&levels);
        }
    }
    list
}

struct Building<'a> {
    lines: Vec<&'a Placed<'a>>,
    pitch: Option<f64>,
    /// `pitch` was measured between a list item's wrapped lines.
    wrapped_pitch: bool,
    alignment: Option<TextBlockAlignment>,
    /// A list: its marker, the items' marker edge and their text indent.
    list: Option<ListBuilding>,
}

struct ListBuilding {
    marker: ListMarker,
    marker_left: f64,
    /// Where the first item's text starts.
    text_left: f64,
    /// Where wrapped lines start, once one is seen.
    wrapped_left: Option<f64>,
}

impl Building<'_> {
    fn last(&self) -> &Placed<'_> {
        self.lines.last().expect("a block has a line")
    }
}

/// Whether `line` continues `block` directly below its last line, and the
/// alignment the block then has.
fn continues(block: &Building<'_>, line: &Placed<'_>) -> Option<TextBlockAlignment> {
    let previous = block.last();
    let same_direction =
        previous.direction.0 * line.direction.0 + previous.direction.1 * line.direction.1 > 0.999;
    if !same_direction
        || previous.font != line.font
        || (line.em - previous.em).abs() > 0.05 * previous.em
    {
        return None;
    }
    let pitch = previous.baseline - line.baseline;
    let matches = |expected: f64| (pitch - expected).abs() <= PITCH_TOLERANCE * expected + 0.5;
    // A list's items may be spaced apart further than an item's wrapped
    // lines.
    let item = block.list.is_some() && line.marker.is_some();
    let spaced = match block.pitch {
        Some(expected) if item => pitch >= expected * (1. - PITCH_TOLERANCE) - 0.5,
        // The first wrapped line after items measured only their spacing.
        Some(expected) if block.list.is_some() && !block.wrapped_pitch => {
            pitch <= expected * (1. + PITCH_TOLERANCE) + 0.5
        }
        Some(expected) => matches(expected),
        None => true,
    };
    if !(MIN_PITCH_EM * line.em..=MAX_PITCH_EM * line.em).contains(&pitch) || !spaced {
        return None;
    }
    if previous.right.min(line.right) <= previous.left.max(line.left) {
        return None;
    }
    let tolerance = (EDGE_TOLERANCE_EM * line.em).max(1.);
    if let Some(list) = &block.list {
        // The next item, or a wrapped line of this one.
        let aligned = |edge: f64| (line.left - edge).abs() <= tolerance;
        let continues = match line.marker {
            Some((marker, _)) => marker == list.marker && aligned(list.marker_left),
            None => match list.wrapped_left {
                Some(wrapped_left) => aligned(wrapped_left),
                None => aligned(list.text_left) || aligned(list.marker_left),
            },
        };
        return continues.then_some(TextBlockAlignment::Left);
    }
    if line.marker.is_some() {
        return None;
    }
    let left = (line.left - previous.left).abs() <= tolerance;
    let right = (line.right - previous.right).abs() <= tolerance;
    let center =
        ((line.left + line.right) - (previous.left + previous.right)).abs() / 2. <= tolerance;
    match block.alignment {
        // The last line of a left-aligned or justified paragraph is shorter.
        Some(TextBlockAlignment::Left | TextBlockAlignment::Justified) => {
            left.then_some(TextBlockAlignment::Left)
        }
        Some(TextBlockAlignment::Right) => right.then_some(TextBlockAlignment::Right),
        Some(TextBlockAlignment::Center) => center.then_some(TextBlockAlignment::Center),
        None if left => Some(TextBlockAlignment::Left),
        None if right => Some(TextBlockAlignment::Right),
        None if center => Some(TextBlockAlignment::Center),
        None => None,
    }
}

/// A finished block, with what linking columns needs.
struct Finished<'a> {
    block: PageTextBlock,
    first: &'a Placed<'a>,
    last: &'a Placed<'a>,
    /// Across the lines: the highest and lowest baselines.
    top: f64,
    bottom: f64,
}

fn finish<'a>(block: Building<'a>) -> Finished<'a> {
    let left = block
        .lines
        .iter()
        .map(|line| line.left)
        .fold(f64::MAX, f64::min);
    let right = block
        .lines
        .iter()
        .map(|line| line.right)
        .fold(f64::MIN, f64::max);
    let em = block.lines[0].em;
    let tolerance = (EDGE_TOLERANCE_EM * em).max(1.);
    let count = block.lines.len();
    let (alignment, list) = match &block.list {
        Some(list) => {
            // Justified: every line an item wraps from reaches the edge.
            let wrapped_from = (0..count.saturating_sub(1))
                .filter(|index| block.lines[index + 1].marker.is_none())
                .collect::<Vec<_>>();
            let justified = wrapped_from.len() >= 2
                && wrapped_from
                    .iter()
                    .all(|index| (block.lines[*index].right - right).abs() <= tolerance);
            (
                if justified {
                    TextBlockAlignment::Justified
                } else {
                    TextBlockAlignment::Left
                },
                Some(TextList::new(
                    list.marker,
                    list.wrapped_left.unwrap_or(list.text_left) - left,
                )),
            )
        }
        None => {
            // Justified: every line but the last reaches both edges.
            let justified = count > 2
                && block.lines[..count - 1].iter().all(|line| {
                    (line.right - right).abs() <= tolerance && (line.left - left).abs() <= tolerance
                });
            (
                if justified {
                    TextBlockAlignment::Justified
                } else {
                    block.alignment.unwrap_or(TextBlockAlignment::Left)
                },
                None,
            )
        }
    };
    let edges = if list.is_some() {
        vec![(left, right); count]
    } else {
        Vec::new()
    };
    Finished {
        block: PageTextBlock {
            lines: block.lines.iter().map(|line| line.line.anchor).collect(),
            alignment,
            left,
            right,
            pitch: block.pitch,
            edges,
            list,
            cell: None,
            ruled: None,
        },
        first: block.lines[0],
        last: block.lines[count - 1],
        top: block.lines[0].baseline,
        bottom: block.lines[count - 1].baseline,
    }
}

/// Whether `next` continues `block` at the top of the next column: the
/// same text in a column of the same width to its right, `block` the last
/// in its column and running to its foot, `next` the first in its own.
fn continues_in_next_column(blocks: &[Finished<'_>], block: usize, next: usize) -> bool {
    let (a, b) = (&blocks[block], &blocks[next]);
    if a.block.list.is_some() || b.block.list.is_some() {
        return false;
    }
    let flowing = |alignment| {
        matches!(
            alignment,
            TextBlockAlignment::Left | TextBlockAlignment::Justified
        )
    };
    let em = a.last.em;
    let (a_left, a_right) = a.block.edges_of(a.block.lines.len() - 1);
    let (b_left, b_right) = b.block.edges_of(0);
    let same_text =
        a.last.direction.0 * b.first.direction.0 + a.last.direction.1 * b.first.direction.1 > 0.999
            && a.last.font == b.first.font
            && (a.last.em - b.first.em).abs() <= 0.05 * em
            && flowing(a.block.alignment)
            && flowing(b.block.alignment)
            && match (a.block.pitch, b.block.pitch) {
                (Some(pa), Some(pb)) => (pa - pb).abs() <= PITCH_TOLERANCE * pa + 0.5,
                _ => true,
            };
    let width = a_right - a_left;
    let next_column = b_left > a_right
        && ((b_right - b_left) - width).abs() <= COLUMN_WIDTH_TOLERANCE * width
        && b.top > a.bottom;
    // The column's last line is full and does not end a sentence.
    let ends = a.last.line.text.trim_end().chars().last();
    let runs_on = a.last.right - a_left >= FULL_LINE * width
        && !ends.is_some_and(|character| ".!?:;".contains(character));
    if !(same_text && next_column && runs_on) {
        return false;
    }
    let overlaps = |other: &Finished<'_>, left: f64, right: f64| {
        let (other_left, other_right) = (other.block.left, other.block.right);
        other_left.max(left) < other_right.min(right)
    };
    // Nothing below `block` in its column, nor above `next` in its own.
    blocks.iter().enumerate().all(|(index, other)| {
        index == block
            || index == next
            || !(overlaps(other, a_left, a_right) && other.bottom < a.bottom
                || overlaps(other, b_left, b_right) && other.top > b.top)
    })
}

/// The page's editable lines grouped into paragraph blocks, top to bottom
/// and left to right. Read-only and blank lines belong to no block. A table
/// cell's lines group on their own, never with text outside the cell.
pub fn paragraph_blocks(layer: &PageTextLayer) -> Vec<PageTextBlock> {
    paragraph_blocks_in(layer, &crate::pdf_tables::page_tables(layer))
}

/// [`paragraph_blocks`] with the page's tables already found.
pub fn paragraph_blocks_in(
    layer: &PageTextLayer,
    tables: &[crate::pdf_tables::PageTable],
) -> Vec<PageTextBlock> {
    let mut in_cell = std::collections::HashMap::new();
    for (table_index, table) in tables.iter().enumerate() {
        for (cell_index, cell) in table.cells.iter().enumerate() {
            for anchor in &cell.lines {
                in_cell.insert(*anchor, (table_index, cell_index));
            }
        }
    }
    let editable = |line: &&PageTextLine| line.read_only.is_none();
    let mut blocks = grouped_blocks(
        layer
            .lines
            .iter()
            .filter(editable)
            .filter(|line| !in_cell.contains_key(&line.anchor))
            .collect(),
        true,
        layer,
        None,
    );
    for (table_index, table) in tables.iter().enumerate() {
        for (cell_index, cell) in table.cells.iter().enumerate() {
            let lines = cell
                .lines
                .iter()
                .filter_map(|anchor| layer.line(anchor))
                .filter(editable)
                .collect::<Vec<_>>();
            if lines.is_empty() {
                continue;
            }
            let inset = crate::pdf_tables::cell_inset(layer, table, cell_index);
            for mut block in grouped_blocks(lines, false, layer, Some(inset.right)) {
                block.cell = Some(BlockCell {
                    table: tables[table_index].key(),
                    cell: cell_index,
                    left: inset.left,
                    right: inset.right,
                });
                blocks.push(block);
            }
        }
    }
    // Top to bottom, then left to right, by first line.
    let position = |block: &PageTextBlock| {
        layer
            .line(&block.lines[0])
            .map_or((0., 0.), |line| line.origin())
    };
    blocks.sort_by(|a, b| {
        let (a, b) = (position(a), position(b));
        b.1.total_cmp(&a.1).then(a.0.total_cmp(&b.0))
    });
    blocks
}

/// `lines` grouped into blocks: lines the structure tree puts in one
/// paragraph group. Lines in none (all of an untagged page's) group as
/// [`untagged_blocks`] finds; `cell_right` is where a table cell's text may
/// run to.
fn grouped_blocks(
    lines: Vec<&PageTextLine>,
    link_columns: bool,
    layer: &PageTextLayer,
    cell_right: Option<f64>,
) -> Vec<PageTextBlock> {
    let placed = lines
        .iter()
        .filter_map(|line| place(line))
        .collect::<Vec<_>>();
    let found = find_lists(&placed, &layer.rules, None);
    let items = found
        .iter()
        .flat_map(|list| list.items.iter().copied())
        .collect::<ListItems>();
    // An untagged list is one block, its levels and wrapped lines with it.
    let mut list_blocks = Vec::new();
    let mut in_list = std::collections::HashSet::new();
    for list in found.iter().filter(|list| list.root) {
        let members = list
            .members
            .iter()
            .filter_map(|anchor| placed.iter().find(|line| line.line.anchor == *anchor))
            .collect::<Vec<_>>();
        // Tagged lines join the list's block only when the list covers the
        // whole of each paragraph element they are in (a list nested in
        // another is an element of its own).
        let paragraphs = members
            .iter()
            .filter_map(|line| line.line.paragraph)
            .collect::<std::collections::HashSet<_>>();
        let whole = lines.iter().all(|line| {
            line.paragraph
                .is_none_or(|paragraph| !paragraphs.contains(&paragraph))
                || list.members.contains(&line.anchor)
        });
        let mixed =
            !paragraphs.is_empty() && members.iter().any(|line| line.line.paragraph.is_none());
        if !whole || mixed {
            continue;
        }
        in_list.extend(list.members.iter().copied());
        list_blocks.push(list_block(list, &members, &layer.rules));
    }
    let (untagged, lines): (Vec<_>, Vec<_>) = lines
        .into_iter()
        .filter(|line| !in_list.contains(&line.anchor))
        .partition(|line| line.paragraph.is_none());
    let mut groups: Vec<(Option<(u32, u16)>, Vec<&PageTextLine>)> = Vec::new();
    for line in lines {
        match line.paragraph.and_then(|paragraph| {
            groups
                .iter_mut()
                .find(|(other, _)| *other == Some(paragraph))
        }) {
            Some((_, group)) => group.push(line),
            None => groups.push((line.paragraph, vec![line])),
        }
    }
    let tops = groups
        .iter()
        .flat_map(|(_, group)| group.iter())
        .map(|line| (line.anchor, line.origin()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut blocks = untagged_blocks(untagged, layer, cell_right, &items);
    blocks.extend(list_blocks);
    for (paragraph, group) in groups {
        let mut pieces = blocks_of(group, link_columns, &items);
        if paragraph.is_none() || pieces.len() < 2 {
            blocks.extend(pieces);
            continue;
        }
        // One paragraph the layout splits (across columns, around a
        // figure) is one block, its pieces in reading order: down a
        // column, then the next column to the right.
        pieces.sort_by(|a, b| {
            let top = |piece: &PageTextBlock| tops.get(&piece.lines[0]).map_or(0., |o| o.1);
            if a.left.max(b.left) < a.right.min(b.right) {
                top(b).total_cmp(&top(a))
            } else {
                a.left.total_cmp(&b.left)
            }
        });
        let mut joined = pieces[0].clone();
        joined.edges = pieces
            .iter()
            .flat_map(|piece| (0..piece.lines.len()).map(|index| piece.edges_of(index)))
            .collect();
        joined.lines = pieces
            .iter()
            .flat_map(|piece| piece.lines.iter().copied())
            .collect();
        joined.pitch = joined
            .pitch
            .or_else(|| pieces.iter().find_map(|piece| piece.pitch));
        blocks.push(joined);
    }
    // Each list knows the levels the page shows around it.
    for block in &mut blocks {
        if let Some(list) = &mut block.list {
            *list = with_page_levels(*list, &found);
        }
    }
    blocks
}

/// Lines ending within this many ems of a text column's widest line reach
/// its edge: a column's edge is where at least two of its lines end.
const COLUMN_EDGE_EM: f64 = 1.;
/// A space between words, in ems, when a line draws none (Arial's and
/// Times's are 0.25–0.28).
const SPACE_EM: f64 = 0.25;
/// How far a line may seem to fit short of its column's edge and still
/// count as overflowing it, in points: widths and rules measure to within
/// rounding.
const FIT_SLACK: f64 = 0.5;

/// The lines the structure tree puts in no paragraph, grouped on firm
/// layout evidence only, as a wrong join is worse than a missed one. A
/// line joins the one above it when both are upright, start at one left
/// edge in the same font and size, at the block's even line pitch with no
/// rule between them, and the line above is full: the next line's first
/// word would not have fitted after it in the text column. The text column
/// ends at its table cell's text edge, at a box the page rules on all four
/// sides (inset as far as the text starts from its left rule), or where at
/// least two lines starting at the same edge end. Centred and right-aligned
/// text stay a line at a time; lists found on the page are blocks of their
/// own ([`find_lists`]).
fn untagged_blocks(
    lines: Vec<&PageTextLine>,
    layer: &PageTextLayer,
    cell_right: Option<f64>,
    items: &ListItems,
) -> Vec<PageTextBlock> {
    let mut placed = lines
        .into_iter()
        .filter_map(|line| place_in(line, items))
        .collect::<Vec<_>>();
    placed.sort_by(|a, b| {
        b.baseline
            .total_cmp(&a.baseline)
            .then(a.left.total_cmp(&b.left))
    });
    // The page's lines, placed only if a column edge must be found from
    // them: a table cell's text edge is known.
    let page = std::cell::OnceCell::new();
    let page = || {
        page.get_or_init(|| {
            layer
                .lines
                .iter()
                .filter(|line| line.read_only.is_none())
                .filter_map(place)
                .filter(upright)
                .collect::<Vec<_>>()
        })
        .as_slice()
    };
    let mut blocks: Vec<Building<'_>> = Vec::new();
    for line in &placed {
        let best = blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| wraps_onto(block, line, &page, &layer.rules, cell_right))
            .map(|(index, block)| (index, block.last().baseline - line.baseline))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((index, pitch)) => {
                let block = &mut blocks[index];
                block.pitch.get_or_insert(pitch);
                block.alignment = Some(TextBlockAlignment::Left);
                block.lines.push(line);
            }
            None => blocks.push(Building {
                lines: vec![line],
                pitch: None,
                wrapped_pitch: false,
                alignment: None,
                list: line.marker.map(|(marker, text_left)| ListBuilding {
                    marker,
                    marker_left: line.left,
                    text_left,
                    wrapped_left: None,
                }),
            }),
        }
    }
    blocks
        .into_iter()
        .map(|block| {
            let (top, bottom) = (block.lines[0].baseline, block.last().baseline);
            let ruled = block
                .lines
                .iter()
                .all(|line| upright(line))
                .then(|| {
                    let left = block
                        .lines
                        .iter()
                        .map(|line| line.left)
                        .fold(f64::MAX, f64::min);
                    let right = block
                        .lines
                        .iter()
                        .map(|line| line.right)
                        .fold(f64::MIN, f64::max);
                    ruled_box(&layer.rules, left, right, bottom, top)
                })
                .flatten();
            let mut finished = finish(block).block;
            finished.ruled = ruled;
            finished
        })
        .collect()
}

fn upright(line: &Placed<'_>) -> bool {
    line.direction.0 > 0.999
}

/// Whether `line` is the next line of `block`, wrapped from its last: see
/// [`untagged_blocks`].
fn wraps_onto<'a>(
    block: &Building<'_>,
    line: &Placed<'_>,
    page: &dyn Fn() -> &'a [Placed<'a>],
    rules: &[crate::pdf_tables::PageRule],
    cell_right: Option<f64>,
) -> bool {
    let previous = block.last();
    if block.list.is_some()
        || line.marker.is_some()
        || !upright(previous)
        || !upright(line)
        || previous.font != line.font
        || (line.em - previous.em).abs() > 0.05 * previous.em
    {
        return false;
    }
    let em = previous.em;
    let tolerance = (EDGE_TOLERANCE_EM * em).max(1.);
    let pitch = previous.baseline - line.baseline;
    let even = block
        .pitch
        .is_none_or(|expected| (pitch - expected).abs() <= PITCH_TOLERANCE * expected + 0.5);
    if (line.left - previous.left).abs() > tolerance
        || !(MIN_PITCH_EM * em..=MAX_PITCH_EM * em).contains(&pitch)
        || !even
    {
        return false;
    }
    let (left, right) = (previous.left.min(line.left), previous.right.max(line.right));
    // A rule between them, such as a table row's border, ends the block.
    if rules.iter().any(|rule| {
        rule.horizontal
            && rule.at < previous.baseline
            && rule.at > line.baseline
            && rule.from < right
            && rule.to > left
    }) {
        return false;
    }
    let column = cell_right
        .or_else(|| {
            ruled_box(rules, left, right, line.baseline, previous.baseline).map(|edges| edges.1)
        })
        .or_else(|| {
            let ends = page()
                .iter()
                .filter(|other| {
                    other.font == previous.font
                        && (other.em - em).abs() <= 0.05 * em
                        && (other.left - previous.left).abs() <= tolerance
                })
                .map(|other| other.right)
                .collect::<Vec<_>>();
            let widest = ends.iter().copied().fold(f64::MIN, f64::max);
            (ends
                .iter()
                .filter(|end| **end >= widest - COLUMN_EDGE_EM * em)
                .count()
                >= 2)
                .then_some(widest)
        });
    let space = previous
        .line
        .chars
        .iter()
        .find(|character| character.character == ' ' && character.advance > 0.)
        .map_or(SPACE_EM * em, |character| {
            character.advance * previous.line.matrix[0].hypot(previous.line.matrix[1])
        });
    column.is_some_and(|column| previous.right + space + first_word(line) > column - FIT_SLACK)
}

/// The width of `line`'s first word, in page units.
fn first_word(line: &Placed<'_>) -> f64 {
    let m = line.line.matrix;
    let chars = &line.line.chars;
    let Some(start) = chars
        .iter()
        .position(|character| !character.character.is_whitespace())
    else {
        return 0.;
    };
    let end = chars[start..]
        .iter()
        .take_while(|character| !character.character.is_whitespace())
        .last()
        .map_or(chars[start].x, |character| character.x + character.advance);
    (end - chars[start].x) * m[0].hypot(m[1])
}

/// Where text from `left` to `right` between baselines `bottom` and `top`
/// may run along its lines when the page rules a box round it on all four
/// sides: from `left` to as far from the right rule as `left` is from the
/// left rule (a word processor pads a cell evenly).
pub fn ruled_box(
    rules: &[crate::pdf_tables::PageRule],
    left: f64,
    right: f64,
    bottom: f64,
    top: f64,
) -> Option<(f64, f64)> {
    let spans = |rule: &crate::pdf_tables::PageRule, from: f64, to: f64| {
        rule.from <= from + 0.5 && rule.to >= to - 0.5
    };
    let vertical = rules
        .iter()
        .filter(|rule| !rule.horizontal && spans(rule, bottom, top));
    let rule_left = vertical
        .clone()
        .map(|rule| rule.at)
        .filter(|at| *at < left)
        .fold(f64::MIN, f64::max);
    let rule_right = vertical
        .map(|rule| rule.at)
        .filter(|at| *at > right)
        .fold(f64::MAX, f64::min);
    if rule_left == f64::MIN || rule_right == f64::MAX {
        return None;
    }
    let mut horizontal = rules
        .iter()
        .filter(|rule| rule.horizontal && spans(rule, rule_left, rule_right));
    let above = horizontal.clone().any(|rule| rule.at > top);
    let below = horizontal.any(|rule| rule.at < bottom);
    (above && below).then_some((left, rule_right - (left - rule_left)))
}

/// `lines` grouped into blocks; with `link_columns`, a paragraph running on
/// into the next column is one block.
fn blocks_of(
    lines: Vec<&PageTextLine>,
    link_columns: bool,
    items: &ListItems,
) -> Vec<PageTextBlock> {
    let mut placed = lines
        .into_iter()
        .filter_map(|line| place_in(line, items))
        .collect::<Vec<_>>();
    placed.sort_by(|a, b| {
        b.baseline
            .total_cmp(&a.baseline)
            .then(a.left.total_cmp(&b.left))
    });
    let mut blocks: Vec<Building<'_>> = Vec::new();
    for line in &placed {
        // The nearest block whose last line is directly above.
        let best = blocks
            .iter()
            .enumerate()
            .filter_map(|(index, block)| {
                continues(block, line)
                    .map(|alignment| (index, alignment, block.last().baseline - line.baseline))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2));
        match best {
            Some((index, alignment, pitch)) => {
                let block = &mut blocks[index];
                // A list's line pitch is its wrapped lines' when it has
                // them; an item may follow further below.
                if block.list.is_some() && line.marker.is_none() {
                    if !block.wrapped_pitch {
                        block.pitch = Some(pitch);
                        block.wrapped_pitch = true;
                    }
                } else {
                    block.pitch.get_or_insert(pitch);
                }
                block.alignment = Some(alignment);
                if let Some(list) = &mut block.list
                    && line.marker.is_none()
                {
                    list.wrapped_left.get_or_insert(line.left);
                }
                block.lines.push(line);
            }
            None => blocks.push(Building {
                lines: vec![line],
                pitch: None,
                wrapped_pitch: false,
                alignment: None,
                list: line.marker.map(|(marker, text_left)| ListBuilding {
                    marker,
                    marker_left: line.left,
                    text_left,
                    wrapped_left: None,
                }),
            }),
        }
    }
    let mut blocks = blocks.into_iter().map(finish).collect::<Vec<_>>();
    // Join paragraphs that continue in the next column, in reading order.
    'linking: loop {
        if !link_columns {
            break;
        }
        for block in 0..blocks.len() {
            let next = (0..blocks.len())
                .filter(|next| *next != block && continues_in_next_column(&blocks, block, *next))
                .min_by(|a, b| blocks[*a].block.left.total_cmp(&blocks[*b].block.left));
            if let Some(next) = next {
                let joined = blocks.remove(next);
                let block = if next < block { block - 1 } else { block };
                let a = &mut blocks[block];
                let mut edges = (0..a.block.lines.len())
                    .map(|index| a.block.edges_of(index))
                    .collect::<Vec<_>>();
                edges.extend(
                    (0..joined.block.lines.len()).map(|index| joined.block.edges_of(index)),
                );
                a.block.lines.extend(joined.block.lines);
                a.block.edges = edges;
                a.block.pitch = a.block.pitch.or(joined.block.pitch);
                if joined.block.alignment == TextBlockAlignment::Justified {
                    a.block.alignment = TextBlockAlignment::Justified;
                }
                a.last = joined.last;
                a.bottom = joined.bottom;
                continue 'linking;
            }
        }
        break;
    }
    blocks.into_iter().map(|finished| finished.block).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_text_edit::{TextLineAnchor, TextLineChar, TextRunStyle};

    fn line(y: f64, text: &str, paragraph: Option<(u32, u16)>) -> PageTextLine {
        PageTextLine {
            anchor: TextLineAnchor {
                x_centipoints: 7200,
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
            styles: vec![TextRunStyle {
                font: b"F1".to_vec(),
                font_size: 10.,
                char_spacing: 0.,
                word_spacing: 0.,
                fill: None,
                face: None,
            }],
            matrix: [1., 0., 0., 1., 72., y],
            horizontal_scale: 1.,
            rise: 0.,
            ascent: 7.,
            descent: -2.,
            fill: None,
            read_only: None,
            in_form: false,
            clip_bounds: None,
            marked_content_ids: Vec::new(),
            paragraph,
        }
    }

    fn layer(lines: Vec<PageTextLine>) -> PageTextLayer {
        PageTextLayer {
            lines,
            ..Default::default()
        }
    }

    #[test]
    fn lines_group_only_as_the_structure_tree_does() {
        // An address block: three lines at a paragraph's spacing.
        let texts = [
            "Client name: Mr A Client",
            "DOB: 81 yo M",
            "28 Some Street 3036",
        ];
        let ys = [700., 688., 676.];
        let untagged = layer(
            texts
                .iter()
                .zip(ys)
                .map(|(text, y)| line(y, text, None))
                .collect(),
        );
        assert_eq!(
            paragraph_blocks(&untagged).len(),
            3,
            "nothing says they are one paragraph"
        );
        let tagged = layer(
            texts
                .iter()
                .zip(ys)
                .enumerate()
                .map(|(index, (text, y))| line(y, text, Some((10 + index as u32, 0))))
                .collect(),
        );
        assert_eq!(paragraph_blocks(&tagged).len(), 3, "three paragraphs");
        let one = layer(
            texts
                .iter()
                .zip(ys)
                .map(|(text, y)| line(y, text, Some((10, 0))))
                .collect(),
        );
        let blocks = paragraph_blocks(&one);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].lines.len(), 3);
    }

    /// An untagged line of `text` at `x`, `y`: 10 pt text, 5 points a
    /// character.
    fn at(x: f64, y: f64, text: &str) -> PageTextLine {
        let mut line = line(y, text, None);
        line.matrix[4] = x;
        line.anchor.x_centipoints = (x * 100.) as i64;
        line
    }

    fn lists(lines: Vec<PageTextLine>) -> Vec<(Option<TextList>, Vec<String>)> {
        let layer = layer(lines);
        paragraph_blocks(&layer)
            .into_iter()
            .map(|block| {
                let texts = block
                    .lines
                    .iter()
                    .map(|anchor| layer.line(anchor).unwrap().text.clone())
                    .collect();
                (block.list, texts)
            })
            .collect()
    }

    #[test]
    fn an_untagged_list_with_its_nested_levels_is_one_block() {
        let blocks = lists(vec![
            at(72., 700., "1. One runs on to a"),
            at(87., 686., "second line"),
            at(92., 672., "a. Sub one"),
            at(92., 658., "b. Sub two"),
            at(72., 644., "2. Two"),
            at(72., 600., "After the list."),
        ]);
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        let (list, texts) = &blocks[0];
        let list = list.expect("a list");
        assert_eq!(list.marker, ListMarker::decimal('.'));
        assert!((list.indent - 15.).abs() < 1e-9);
        let levels = list.levels();
        assert_eq!(levels.len(), 2, "{levels:?}");
        assert!((levels[1].offset - 20.).abs() < 1e-9);
        assert_eq!(
            levels[1].marker,
            ListMarker::Number {
                style: NumberStyle::LowerAlpha,
                open: false,
                suffix: '.'
            }
        );
        assert_eq!(texts.len(), 5);
        assert!(blocks[1].0.is_none());
    }

    #[test]
    fn headings_sentences_contents_and_single_dashes_stay_plain() {
        let none = |lines| {
            lists(lines)
                .into_iter()
                .all(|(list, texts)| list.is_none() || panic!("{texts:?}"))
        };
        // Numbered headings with body text between them.
        assert!(none(vec![
            at(72., 700., "1. Introduction"),
            at(72., 680., "The survey covered the roof and walls."),
            at(72., 650., "2. Methods"),
            at(72., 630., "Each elevation was inspected."),
        ]));
        // A sentence that starts with a number, wrapped back to the margin.
        assert!(none(vec![
            at(72., 700., "1. The other samples passed and the"),
            at(72., 686., "laboratory issued its certificate."),
        ]));
        // A table of contents: page numbers along the entries' baselines.
        assert!(none(vec![
            at(72., 700., "1. Introduction"),
            at(400., 700., "3"),
            at(72., 686., "2. Methods"),
            at(400., 686., "5"),
        ]));
        // A dash at the start of a line of prose.
        assert!(none(vec![at(72., 700., "- as noted above, the work"),]));
        // Numbers that do not count on (dates).
        assert!(none(vec![
            at(72., 700., "12. March 2024 in Hamburg"),
            at(72., 686., "3. April 2024 after the audit"),
        ]));
    }

    #[test]
    fn one_bullet_glyph_set_off_by_a_tab_is_a_list() {
        let mut bullet = at(72., 700., "\u{2022} One item");
        // The text starts a tab (an em and a half) after the bullet.
        for character in &mut bullet.chars[1..] {
            character.x += 10.;
        }
        let blocks = lists(vec![bullet]);
        assert_eq!(
            blocks[0].0.map(|list| list.marker),
            Some(ListMarker::Bullet('\u{2022}'))
        );
        // With a word space only (a quarter em), it may be text.
        let mut spaced = at(72., 700., "\u{2022} One item");
        for character in &mut spaced.chars[2..] {
            character.x -= 2.5;
        }
        let blocks = lists(vec![spaced]);
        assert!(blocks[0].0.is_none());
    }
}
