//! Selecting a page's text to copy it, as the Select tool does in Acrobat:
//! the page's lines in reading order, where a point falls among their
//! characters, and the boxes and text between two such places. The lines
//! are those Edit PDF Content reads, as the page shows them after any edits.

use std::cmp::Ordering;

use crate::pdf_text_blocks::PageTextBlock;
use crate::pdf_text_edit::{
    PageTextEdit, PageTextLayer, PageTextLine, extra_line_base, layout_text_edit_fitted,
};

pub type Quad = [(f64, f64); 4];

/// A line of page text as the page shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct SelectableLine {
    /// Places the line on the page; its own characters are not used.
    pub line: PageTextLine,
    /// Each character and where it starts and ends along the line, in text
    /// space.
    pub chars: Vec<(char, f64, f64)>,
    /// Lines sharing a number are one paragraph.
    pub paragraph: usize,
}

impl SelectableLine {
    fn span(&self) -> Option<(f64, f64)> {
        let mut visible = self.chars.iter().filter(|(c, _, _)| !c.is_whitespace());
        let first = visible.next()?;
        let last = visible.next_back().unwrap_or(first);
        Some((first.1, last.2))
    }

    /// How far a page point is from the line's box, in page units, and the
    /// point in the line's text space.
    fn distance(&self, point: (f64, f64)) -> Option<(f64, (f64, f64))> {
        let span = self.span()?;
        let (x, y) = self.line.text_space_point(point.0, point.1)?;
        let bottom = self.line.rise + self.line.descent;
        let top = self.line.rise + self.line.ascent;
        let outside = |value: f64, low: f64, high: f64| {
            if value < low {
                low - value
            } else if value > high {
                value - high
            } else {
                0.
            }
        };
        let scale = {
            let m = self.line.matrix;
            (m[0] * m[3] - m[1] * m[2]).abs().sqrt()
        };
        let dx = outside(x, span.0, span.1);
        let dy = outside(y, bottom.min(top), bottom.max(top));
        Some((dx.hypot(dy) * scale, (x, y)))
    }

    /// The place between characters nearest text-space `x`.
    fn index_at(&self, x: f64) -> usize {
        self.chars
            .iter()
            .position(|(_, start, end)| x < (start + end) / 2.)
            .unwrap_or(self.chars.len())
    }
}

/// A place between two characters: before character `index` of line `line`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TextPosition {
    pub line: usize,
    pub index: usize,
}

/// The page's lines in reading order: each paragraph's lines together, in
/// the order the page draws its paragraphs, with the lines edits added
/// below a paragraph after it.
pub fn selectable_lines(
    layer: &PageTextLayer,
    blocks: &[PageTextBlock],
    edits: &[PageTextEdit],
) -> Vec<SelectableLine> {
    selectable_lines_with_tables(layer, blocks, edits, &[], &[])
}

/// [`selectable_lines`] on a page whose tables `tables` edit and whose
/// content `moves` move: lines a table edit or move moves are where it shows
/// them, a deleted row's are gone, and new cell text follows each table's
/// lines.
pub fn selectable_lines_with_tables(
    layer: &PageTextLayer,
    blocks: &[PageTextBlock],
    edits: &[PageTextEdit],
    tables: &[crate::pdf_tables::PageTableEdit],
    moves: &[crate::pdf_content_moves::ContentMove],
) -> Vec<SelectableLine> {
    let tables = tables
        .iter()
        .filter(|table| table.page_index == layer.page_index)
        .collect::<Vec<_>>();
    let moves = moves
        .iter()
        .filter(|item| item.page_index == layer.page_index)
        .cloned()
        .collect::<Vec<_>>();
    let moved = |line: &PageTextLine| {
        if tables.is_empty() && moves.is_empty() {
            return Some((0., 0.));
        }
        crate::pdf_tables::line_core_box(line).map_or(Some((0., 0.)), |rect| {
            crate::pdf_content_moves::line_offset(&tables, &moves, rect)
        })
    };
    let shift = |mut line: PageTextLine, (dx, dy): (f64, f64)| {
        line.matrix[4] += dx;
        line.matrix[5] += dy;
        line
    };
    let edit_of = |line: &PageTextLine| {
        edits
            .iter()
            .find(|edit| edit.page_index == layer.page_index && edit.anchor == line.anchor)
    };
    let mut lines = Vec::new();
    let mut paragraphs = 0;
    let mut done = vec![false; blocks.len()];
    for line in &layer.lines {
        let block = blocks
            .iter()
            .position(|block| block.lines.contains(&line.anchor));
        let members: Vec<&PageTextLine> = match block {
            Some(index) if done[index] => continue,
            Some(index) => {
                done[index] = true;
                blocks[index]
                    .lines
                    .iter()
                    .filter_map(|anchor| layer.line(anchor))
                    .collect()
            }
            None => vec![line],
        };
        let paragraph = paragraphs;
        paragraphs += 1;
        for member in members {
            let Some(offset) = moved(member) else {
                continue;
            };
            let edit = edit_of(member);
            let chars = match edit {
                Some(edit) => layout_text_edit_fitted(layer, member, &edit.runs, edit.layout.fit)
                    .map(|layout| {
                        layout
                            .chars
                            .iter()
                            .map(|c| (c.character, c.x, c.x + c.advance))
                            .collect()
                    })
                    .unwrap_or_default(),
                None => member
                    .chars
                    .iter()
                    .map(|c| (c.character, c.x, c.x + c.advance))
                    .collect(),
            };
            lines.push(SelectableLine {
                line: shift(member.clone(), offset),
                chars,
                paragraph,
            });
            for extra in edit.iter().flat_map(|edit| &edit.layout.extra_lines) {
                let base = extra_line_base(member, extra.below);
                let chars = layout_text_edit_fitted(layer, &base, &extra.runs, extra.fit)
                    .map(|layout| {
                        layout
                            .chars
                            .iter()
                            .map(|c| (c.character, c.x, c.x + c.advance))
                            .collect()
                    })
                    .unwrap_or_default();
                lines.push(SelectableLine {
                    line: shift(base, offset),
                    chars,
                    paragraph,
                });
            }
        }
    }
    // New cell text, each cell a paragraph.
    for table in &tables {
        for text in &table.cell_texts {
            let Some((left, top)) = table.cell_origin(text.row, text.column) else {
                continue;
            };
            let paragraph = paragraphs;
            paragraphs += 1;
            for cell_line in &text.lines {
                let Some(style) = cell_line.runs.first().map(|run| run.style.clone()) else {
                    continue;
                };
                let size = style.font_size.abs();
                let line = PageTextLine {
                    anchor: crate::pdf_text_edit::TextLineAnchor {
                        x_centipoints: ((left + cell_line.x) * 100.).round() as i64,
                        y_centipoints: ((top - cell_line.baseline) * 100.).round() as i64,
                        ordinal: u32::MAX,
                    },
                    text: String::new(),
                    chars: Vec::new(),
                    end_x: 0.,
                    styles: vec![style],
                    matrix: [1., 0., 0., 1., left + cell_line.x, top - cell_line.baseline],
                    horizontal_scale: 1.,
                    rise: 0.,
                    ascent: 0.8 * size,
                    descent: -0.2 * size,
                    fill: None,
                    read_only: None,
                    in_form: false,
                    clip_bounds: None,
                    marked_content_ids: Vec::new(),
                    paragraph: None,
                };
                let chars =
                    layout_text_edit_fitted(layer, &line, &cell_line.runs, Default::default())
                        .map(|layout| {
                            layout
                                .chars
                                .iter()
                                .map(|c| (c.character, c.x, c.x + c.advance))
                                .collect()
                        })
                        .unwrap_or_default();
                lines.push(SelectableLine {
                    line,
                    chars,
                    paragraph,
                });
            }
        }
    }
    lines.retain(|line| line.span().is_some());
    lines
}

/// The place in text under a page point, within `tolerance` page units of
/// a line: where a press starts selecting and the pointer shows an I-beam.
pub fn hit(lines: &[SelectableLine], point: (f64, f64), tolerance: f64) -> Option<TextPosition> {
    nearest_line(lines, point)
        .filter(|(_, distance, _)| *distance <= tolerance)
        .map(|(line, _, x)| TextPosition {
            line,
            index: lines[line].index_at(x),
        })
}

/// The place in the line nearest a page point, however far: where a drag
/// that began in text extends the selection to.
pub fn nearest(lines: &[SelectableLine], point: (f64, f64)) -> Option<TextPosition> {
    nearest_line(lines, point).map(|(line, _, x)| TextPosition {
        line,
        index: lines[line].index_at(x),
    })
}

fn nearest_line(lines: &[SelectableLine], point: (f64, f64)) -> Option<(usize, f64, f64)> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(index, line)| {
            line.distance(point)
                .map(|(distance, (x, _))| (index, distance, x))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
}

#[derive(Clone, Copy, PartialEq)]
enum CharClass {
    Space,
    Word,
    Other,
}

fn class(character: char) -> CharClass {
    if character.is_whitespace() {
        CharClass::Space
    } else if character.is_alphanumeric() || character == '_' || character == '\'' {
        CharClass::Word
    } else {
        CharClass::Other
    }
}

/// The word (or run of spaces or punctuation) at a place, as a double-click
/// selects.
pub fn word_at(lines: &[SelectableLine], at: TextPosition) -> (TextPosition, TextPosition) {
    let chars = &lines[at.line].chars;
    if chars.is_empty() {
        return (at, at);
    }
    // At a line's end the word is the one before.
    let index = at.index.min(chars.len() - 1);
    let kind = class(chars[index].0);
    let mut start = index;
    while start > 0 && class(chars[start - 1].0) == kind {
        start -= 1;
    }
    let mut end = index + 1;
    while end < chars.len() && class(chars[end].0) == kind {
        end += 1;
    }
    (
        TextPosition {
            line: at.line,
            index: start,
        },
        TextPosition {
            line: at.line,
            index: end,
        },
    )
}

/// The whole paragraph at a place, as a triple-click selects.
pub fn paragraph_at(lines: &[SelectableLine], at: TextPosition) -> (TextPosition, TextPosition) {
    let paragraph = lines[at.line].paragraph;
    let first = (0..=at.line)
        .rev()
        .take_while(|line| lines[*line].paragraph == paragraph)
        .last()
        .unwrap_or(at.line);
    let last = (at.line..lines.len())
        .take_while(|line| lines[*line].paragraph == paragraph)
        .last()
        .unwrap_or(at.line);
    (
        TextPosition {
            line: first,
            index: 0,
        },
        TextPosition {
            line: last,
            index: lines[last].chars.len(),
        },
    )
}

/// Each line's selected characters between two places, in either order.
fn ranges(
    lines: &[SelectableLine],
    a: TextPosition,
    b: TextPosition,
) -> impl Iterator<Item = (usize, usize, usize)> + '_ {
    let (start, end) = if a <= b { (a, b) } else { (b, a) };
    (start.line..=end.line.min(lines.len().saturating_sub(1)))
        .filter(move |_| start != end)
        .map(move |line| {
            let length = lines[line].chars.len();
            let from = if line == start.line { start.index } else { 0 };
            let to = if line == end.line { end.index } else { length };
            (line, from.min(length), to.min(length))
        })
}

/// The selection's boxes, one per line, in page user space.
pub fn selection_quads(lines: &[SelectableLine], a: TextPosition, b: TextPosition) -> Vec<Quad> {
    ranges(lines, a, b)
        .filter(|(_, from, to)| from < to)
        .map(|(line, from, to)| {
            let selectable = &lines[line];
            selectable
                .line
                .quad(selectable.chars[from].1, selectable.chars[to - 1].2)
        })
        .collect()
}

/// The selected text, its lines separated by line breaks.
pub fn selection_text(lines: &[SelectableLine], a: TextPosition, b: TextPosition) -> String {
    ranges(lines, a, b)
        .map(|(line, from, to)| {
            lines[line].chars[from..to.max(from)]
                .iter()
                .map(|(c, _, _)| *c)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_text_edit::{TextLineAnchor, TextLineChar, TextRunStyle};

    /// A line of 10pt characters 6pt apart, its baseline at `y`.
    fn line(text: &str, y: f64, ordinal: u32) -> PageTextLine {
        PageTextLine {
            anchor: TextLineAnchor {
                x_centipoints: 7_200,
                y_centipoints: (y * 100.) as i64,
                ordinal,
            },
            text: text.into(),
            chars: text
                .chars()
                .enumerate()
                .map(|(index, character)| TextLineChar {
                    character,
                    x: index as f64 * 6.,
                    advance: 6.,
                    style: 0,
                    synthetic: false,
                })
                .collect(),
            end_x: text.chars().count() as f64 * 6.,
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
            ascent: 8.,
            descent: -2.,
            fill: None,
            read_only: None,
            in_form: false,
            clip_bounds: None,
            marked_content_ids: Vec::new(),
            paragraph: None,
        }
    }

    fn page() -> Vec<SelectableLine> {
        let layer = PageTextLayer {
            page_index: 0,
            fonts: Vec::new(),
            lines: vec![
                line("Hello brave", 700., 0),
                line("new world.", 688., 1),
                line("Footer", 100., 2),
            ],
            page_box: None,
            tagged_tables: Vec::new(),
            rules: Vec::new(),
            fills: Vec::new(),
            objects: Vec::new(),
        };
        let blocks = vec![PageTextBlock {
            lines: vec![layer.lines[0].anchor, layer.lines[1].anchor],
            alignment: crate::pdf_text_blocks::TextBlockAlignment::Left,
            left: 0.,
            right: 66.,
            pitch: Some(12.),
            edges: Vec::new(),
            list: None,
            cell: None,
        }];
        selectable_lines(&layer, &blocks, &[])
    }

    fn at(line: usize, index: usize) -> TextPosition {
        TextPosition { line, index }
    }

    #[test]
    fn a_point_on_a_line_falls_between_the_characters_either_side() {
        let lines = page();
        // "Hello": the second character spans 78–84 on the page.
        assert_eq!(hit(&lines, (80., 703.), 1.), Some(at(0, 1)));
        assert_eq!(hit(&lines, (82., 703.), 1.), Some(at(0, 2)));
        assert_eq!(hit(&lines, (80., 400.), 1.), None, "nowhere near text");
        assert_eq!(nearest(&lines, (80., 400.)), Some(at(1, 1)));
    }

    #[test]
    fn selections_run_across_lines_in_reading_order() {
        let lines = page();
        let text = selection_text(&lines, at(1, 4), at(0, 6));
        assert_eq!(text, "brave\nnew ");
        assert_eq!(selection_quads(&lines, at(0, 6), at(1, 4)).len(), 2);
        assert_eq!(selection_text(&lines, at(0, 3), at(0, 3)), "");
    }

    #[test]
    fn double_click_takes_a_word_and_triple_click_its_paragraph() {
        let lines = page();
        let (start, end) = word_at(&lines, at(0, 7));
        assert_eq!(selection_text(&lines, start, end), "brave");
        let (start, end) = paragraph_at(&lines, at(1, 2));
        assert_eq!(
            selection_text(&lines, start, end),
            "Hello brave\nnew world."
        );
        let (start, end) = paragraph_at(&lines, at(2, 0));
        assert_eq!(selection_text(&lines, start, end), "Footer");
    }
}
