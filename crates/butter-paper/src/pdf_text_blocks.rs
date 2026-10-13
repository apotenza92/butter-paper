//! Groups a page's editable text lines into paragraph blocks, as Acrobat
//! and OnlyOffice present text for editing. A block is a run of lines in one
//! column with the same orientation and size, an even line pitch and a
//! shared edge (left, right, centre, or both for justified text). A list's
//! items, with their wrapped lines at the items' text indent, form one
//! block; a paragraph that runs off the foot of a column and continues at
//! the top of the next is one block over both.

use serde::{Deserialize, Serialize};

use crate::pdf_text_edit::{PageTextLayer, PageTextLine, TextLineAnchor};

/// Lines further apart than this many ems start a new block: line pitch is
/// usually 1–1.6 em, and paragraph spacing adds to it.
const MAX_PITCH_EM: f64 = 1.75;
const MIN_PITCH_EM: f64 = 0.9;
/// How much a block's line pitch may vary.
const PITCH_TOLERANCE: f64 = 0.12;
/// How far shared edges may differ, in ems (at least a point).
const EDGE_TOLERANCE_EM: f64 = 0.15;
/// Characters that begin a list item.
const BULLETS: [char; 8] = ['•', '◦', '‣', '▪', '–', '—', '-', '*'];
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

/// What begins each item of a list.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ListMarker {
    /// The same character before every item.
    Bullet(char),
    /// Numbers counting up, each followed by `suffix` (`.` or `)`).
    Number { suffix: char },
}

/// A block whose lines are list items.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextList {
    pub marker: ListMarker,
    /// From an item's marker to where its wrapped lines start, along the
    /// lines in page units.
    pub indent: f64,
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
    /// The list marker it begins with, and where its text after the marker
    /// starts.
    marker: Option<(ListMarker, f64)>,
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
    let marker = list_marker(line).map(|(marker, text_start)| {
        let text_left = line.chars[text_start..]
            .iter()
            .find(|character| !character.character.is_whitespace())
            .map_or(at(last.x + last.advance), |character| at(character.x));
        (marker, text_left)
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
        marker,
    })
}

/// The list marker `line` begins with, and the index of the character after
/// it.
fn list_marker(line: &PageTextLine) -> Option<(ListMarker, usize)> {
    let start = line
        .chars
        .iter()
        .position(|character| !character.character.is_whitespace())?;
    let chars = &line.chars[start..];
    let separated = |index: usize| {
        chars
            .get(index)
            .is_none_or(|character| character.character.is_whitespace())
    };
    let first = chars[0].character;
    if BULLETS.contains(&first) {
        return separated(1).then_some((ListMarker::Bullet(first), start + 1));
    }
    let digits = chars
        .iter()
        .take_while(|character| character.character.is_ascii_digit())
        .count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let suffix = chars.get(digits)?.character;
    (matches!(suffix, '.' | ')') && chars.get(digits + 1).is_some() && separated(digits + 1))
        .then_some((ListMarker::Number { suffix }, start + digits + 1))
}

/// The marker a list item begins with in `text`, and its length in
/// characters including the spaces after it.
pub fn item_marker(text: &[char]) -> Option<(ListMarker, usize)> {
    let first = *text.first()?;
    let spaces = |from: usize| {
        text[from..]
            .iter()
            .take_while(|character| **character == ' ' || **character == '\u{a0}')
            .count()
    };
    if BULLETS.contains(&first) {
        let gap = spaces(1);
        return (gap > 0).then_some((ListMarker::Bullet(first), 1 + gap));
    }
    let digits = text
        .iter()
        .take_while(|character| character.is_ascii_digit())
        .count();
    if digits == 0 || digits > 3 {
        return None;
    }
    let suffix = *text.get(digits)?;
    let gap = spaces(digits + 1);
    (matches!(suffix, '.' | ')') && gap > 0)
        .then_some((ListMarker::Number { suffix }, digits + 1 + gap))
}

struct Building<'a> {
    lines: Vec<&'a Placed<'a>>,
    pitch: Option<f64>,
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
    if !(MIN_PITCH_EM * line.em..=MAX_PITCH_EM * line.em).contains(&pitch)
        || block
            .pitch
            .is_some_and(|expected| (pitch - expected).abs() > PITCH_TOLERANCE * expected + 0.5)
    {
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
                Some(TextList {
                    marker: list.marker,
                    indent: list.wrapped_left.unwrap_or(list.text_left) - list.marker_left,
                }),
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
/// and left to right. Read-only and blank lines belong to no block.
pub fn paragraph_blocks(layer: &PageTextLayer) -> Vec<PageTextBlock> {
    let mut placed = layer
        .lines
        .iter()
        .filter(|line| line.read_only.is_none())
        .filter_map(place)
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
                block.pitch.get_or_insert(pitch);
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
