//! Rewrapping an edited paragraph (Acrobat's paragraph editing): the
//! paragraph's text is wrapped at its width, from the first line the edit
//! changes, into its lines and, when it grows, lines added below. Lines
//! before the change, and lines the text wraps back onto unchanged, keep
//! their bytes. Rewrapped lines keep the paragraph's alignment: justified
//! lines are stretched to its right edge, centred and right-aligned lines
//! are placed by its centre or right edge. A list item's wrapped lines start
//! at the list's indent. A chosen line spacing, or text larger than the
//! paragraph's, places every line afresh, each as far below the one before
//! as its text needs, continuing at the top of the next column when the
//! paragraph runs through columns.

use std::ops::Range;
use std::sync::Arc;

use crate::pdf_text_blocks::{PageTextBlock, TextBlockAlignment, TextList, list_item_marker};
use crate::pdf_text_edit::{
    ExtraLine, LineFit, PageTextEdit, PageTextLayer, PageTextLine, ParagraphFormat, TextEditError,
    TextEditLayout, TextEditRun, TextLineAnchor, TextLineLayout, TextRunStyle, extra_line_base,
    layout_text_edit_fitted, normalized_runs,
};

/// How far past its width a line may reach before wrapping, in text space.
const WIDTH_TOLERANCE: f64 = 0.5;
/// For ragged text, whose right edge is only its longest line, as a share of
/// the line's width: rewrapped text is measured by its fonts' widths, not
/// the kerning it was drawn with.
const RAGGED_TOLERANCE: f64 = 0.02;
/// Lines a paragraph may grow by.
const MAX_EXTRA_LINES: usize = 200;

/// One character of a paragraph's text and the style it is drawn in. A
/// hard line break is `'\n'`.
pub type StyledChar = (char, TextRunStyle);

/// A paragraph being edited: its lines in the base document, and where its
/// text may run on each.
#[derive(Clone, Debug)]
pub struct Paragraph {
    layer: Arc<PageTextLayer>,
    /// In reading order.
    lines: Vec<PageTextLine>,
    alignment: TextBlockAlignment,
    /// The lines' unit direction on the page.
    direction: (f64, f64),
    /// For each line, its column's left and right edges along the lines, in
    /// page units; a list item's left edge is its marker's.
    page_edges: Vec<(f64, f64)>,
    /// The lines of each column the paragraph runs through, in order.
    columns: Vec<Range<usize>>,
    list: Option<TextList>,
    /// A single line grows rather than wraps.
    grows: bool,
    /// How far along the lines a growing line may run: to the page's edge,
    /// keeping the margin it has on its other side, up to an em.
    grow_limit: f64,
    /// Baseline to baseline below the last line, in its text space.
    pitch: f64,
    /// The paragraph's font size, in the last line's text space.
    em: f64,
    /// Lines in the base text are joined by a space that neither draws.
    joined_by_space: Vec<bool>,
    /// A hard break follows the line in the base text: the next starts a
    /// list item.
    breaks: Vec<bool>,
    /// Settings chosen in place of the page's.
    format: ParagraphFormat,
    /// The table cell the paragraph is in.
    cell: Option<crate::pdf_text_blocks::BlockCell>,
    /// Every line is laid out afresh, though the text is unchanged: its
    /// cell's width changed.
    reflow: bool,
}

/// A line of the wrapped paragraph: its characters, and where it is drawn.
#[derive(Clone, Debug)]
pub struct WrappedLine {
    /// Into the paragraph's characters; excludes a soft-break space or hard
    /// break that ends it.
    pub range: Range<usize>,
    /// Its place in order: the paragraph line it is drawn on, or past those
    /// it is drawn on, a line added below.
    pub slot: usize,
    /// Its text space (an added line's is the last line's, moved).
    pub line: PageTextLine,
    pub layout: TextLineLayout,
    pub fit: LineFit,
    /// Whether it is the base line, unchanged.
    pub unchanged: bool,
    /// Whether it is drawn as a line added to the paragraph's last, and how
    /// far below that line, in its text space.
    pub added: bool,
    pub below: f64,
}

fn runs_of(chars: &[StyledChar]) -> Vec<TextEditRun> {
    let mut runs: Vec<TextEditRun> = Vec::new();
    for (character, style) in chars {
        match runs.last_mut() {
            Some(run) if &run.style == style => run.text.push(*character),
            _ => runs.push(TextEditRun {
                style: style.clone(),
                text: character.to_string(),
            }),
        }
    }
    runs
}

fn styled(line: &PageTextLine) -> Vec<StyledChar> {
    line.chars
        .iter()
        .map(|character| (character.character, line.styles[character.style].clone()))
        .collect()
}

fn styled_runs(runs: &[TextEditRun]) -> Vec<StyledChar> {
    runs.iter()
        .flat_map(|run| {
            run.text
                .chars()
                .map(|character| (character, run.style.clone()))
        })
        .collect()
}

/// Where a line's visible text starts and ends in a layout.
fn visible(layout: &TextLineLayout) -> Option<(f64, f64)> {
    let mut drawn = layout
        .chars
        .iter()
        .filter(|character| !character.character.is_whitespace());
    let first = drawn.next()?;
    let last = drawn.next_back().unwrap_or(first);
    Some((first.x, last.x + last.advance))
}

/// The page origin as an anchor (ordinal 0).
fn anchor_at((x, y): (f64, f64)) -> TextLineAnchor {
    TextLineAnchor {
        x_centipoints: (x * 100.).round() as i64,
        y_centipoints: (y * 100.).round() as i64,
        ordinal: 0,
    }
}

/// Whether the text from `position` begins a list item: it starts the
/// paragraph or follows a hard break.
fn starts_item(text: &[StyledChar], position: usize) -> bool {
    position == 0
        || text
            .get(position - 1)
            .is_some_and(|(character, _)| *character == '\n')
}

/// The largest font size in `chars`, at least `floor`.
/// Whether `text` ends in a hard break that no line of `wrapped` follows.
fn ends_in_break(text: &[StyledChar], wrapped: &[WrappedLine]) -> bool {
    text.last().is_some_and(|(character, _)| *character == '\n')
        && !wrapped.iter().any(|line| line.range.start == text.len())
}

fn largest_size(chars: &[StyledChar], floor: f64) -> f64 {
    chars
        .iter()
        .filter(|(character, _)| !character.is_whitespace())
        .map(|(_, style)| style.font_size.abs())
        .fold(floor, f64::max)
}

/// A space's width, as a fraction of the font size, when judging whether
/// a line ended early.
const EARLY_BREAK_SPACE_EM: f64 = 0.25;
const EARLY_BREAK_TOLERANCE_EM: f64 = 0.1;
/// How far short of text beside it on its baseline a line stops: further
/// than words of one line are apart (`MAX_MERGE_GAP_EM`), so the two still
/// read as separate lines once saved.
const NEIGHBOUR_GAP_EM: f64 = 1.2;

/// Where a line's visible characters start and end, in its text space.
fn visible_span_x(line: &PageTextLine) -> Option<(f64, f64)> {
    let first = line
        .chars
        .iter()
        .find(|character| !character.character.is_whitespace())?;
    let last = line
        .chars
        .iter()
        .rfind(|character| !character.character.is_whitespace())?;
    Some((first.x, last.x + last.advance))
}

/// A line's visible width and its first word's width, in page units.
fn visible_widths(line: &PageTextLine) -> Option<(f64, f64)> {
    let scale = line.matrix[0].hypot(line.matrix[1]);
    let start = line
        .chars
        .iter()
        .position(|character| !character.character.is_whitespace())?;
    let end = line
        .chars
        .iter()
        .rposition(|character| !character.character.is_whitespace())?;
    let first = &line.chars[start];
    let last = &line.chars[end];
    let word_end = line.chars[start..]
        .iter()
        .take_while(|character| !character.character.is_whitespace())
        .last()
        .map_or(first.x, |character| character.x + character.advance);
    Some((
        (last.x + last.advance - first.x) * scale,
        (word_end - first.x) * scale,
    ))
}

impl Paragraph {
    /// The paragraph `block` names in `layer`.
    pub fn new(layer: Arc<PageTextLayer>, block: &PageTextBlock) -> Option<Self> {
        let lines = block
            .lines
            .iter()
            .map(|anchor| layer.line(anchor).cloned())
            .collect::<Option<Vec<_>>>()?;
        let first = lines.first()?;
        let direction = {
            let m = first.matrix;
            let length = m[0].hypot(m[1]);
            (m[0] / length, m[1] / length)
        };
        let normal = (-direction.1, direction.0);
        let mut page_edges = (0..lines.len())
            .map(|index| block.edges_of(index))
            .collect::<Vec<_>>();
        // A new column starts where a line is higher on the page than the
        // one before.
        let baseline = |line: &PageTextLine| {
            let (x, y) = line.origin();
            x * normal.0 + y * normal.1
        };
        let mut columns = Vec::new();
        columns.push(0..1);
        for index in 1..lines.len() {
            if baseline(&lines[index]) > baseline(&lines[index - 1]) {
                columns.push(index..index + 1);
            } else {
                columns.last_mut().expect("a column").end = index + 1;
            }
        }
        // A single line (a heading, a label) grows rather than wraps, as in
        // Revu; only a hard break starts a new line. A table cell's text
        // wraps at the cell's width, as in a word processor.
        let grows = lines.len() == 1 && block.cell.is_none();
        let last = lines.last()?;
        let em = last
            .chars
            .first()
            .map_or(12., |character| {
                last.styles[character.style].font_size.abs()
            })
            .max(1e-3);
        let pitch = block
            .pitch
            .unwrap_or_else(|| em * 1.2 * last.matrix[3].hypot(last.matrix[2]))
            / last.matrix[3].hypot(last.matrix[2]);
        let joined_by_space = lines
            .iter()
            .map(|line| !line.text.ends_with(char::is_whitespace))
            .collect();
        // A line that ended early was broken by the writer (Shift+Enter, a
        // label before its text): the next line's first word would have
        // fitted within the widest line. In a list, a break precedes each
        // item.
        let widths = lines.iter().map(visible_widths).collect::<Vec<_>>();
        // The text column: it reaches as far right as any line on the page
        // that starts within this paragraph's span (indented lines too).
        let span = |line: &PageTextLine| {
            let (start, end) = visible_span_x(line)?;
            let at = |x: f64| {
                let (px, py) = (
                    line.matrix[0] * x + line.matrix[4],
                    line.matrix[1] * x + line.matrix[5],
                );
                px * direction.0 + py * direction.1
            };
            Some((at(start), at(end)))
        };
        let size = em * last.matrix[3].hypot(last.matrix[2]);
        let tolerance = (EARLY_BREAK_TOLERANCE_EM * size).max(1.);
        let left = lines
            .iter()
            .filter_map(span)
            .map(|(left, _)| left)
            .fold(f64::MAX, f64::min);
        let own_widest = widths
            .iter()
            .flatten()
            .map(|(width, _)| *width)
            .fold(0., f64::max);
        let widest = layer
            .lines
            .iter()
            .filter(|line| {
                let m = line.matrix;
                let length = m[0].hypot(m[1]);
                (m[0] * direction.0 + m[1] * direction.1) / length > 0.999
            })
            .filter_map(span)
            .filter(|(start, _)| *start >= left - tolerance && *start < left + own_widest)
            .map(|(_, end)| end - left)
            .fold(own_widest, f64::max);
        // Left-aligned text may run on to the column's edge, as it does in
        // the writer's app, not just to its own longest line.
        // Never past other text on the line's own baseline (a table's next
        // cell).
        // Where the next text on `line`'s baseline starts, after `end`.
        let neighbour = |line: &PageTextLine, end: f64| {
            layer
                .lines
                .iter()
                .filter(|other| {
                    !block.lines.contains(&other.anchor)
                        && (baseline(other) - baseline(line)).abs() <= 0.5 * size
                })
                .filter_map(span)
                .map(|(start, _)| start)
                .filter(|start| *start > end)
                .fold(f64::MAX, f64::min)
        };
        if let Some(cell) = block.cell {
            // A cell's text runs within the cell: left-aligned and justified
            // text from where it starts to the cell's right, right-aligned
            // text from the cell's left to where it ends, centred text
            // across the cell.
            for edges in &mut page_edges {
                *edges = match block.alignment {
                    TextBlockAlignment::Left | TextBlockAlignment::Justified => {
                        (edges.0, cell.right.max(edges.0 + 1.))
                    }
                    TextBlockAlignment::Right | TextBlockAlignment::Center => {
                        (cell.left, cell.right.max(cell.left + 1.))
                    }
                };
            }
        } else if block.alignment == TextBlockAlignment::Left && columns.len() == 1 {
            for (line, edges) in lines.iter().zip(&mut page_edges) {
                let Some((_, end)) = span(line) else {
                    continue;
                };
                let neighbour = neighbour(line, end);
                edges.1 = edges
                    .1
                    .max((left + widest).min(neighbour - NEIGHBOUR_GAP_EM * size));
            }
        }
        let own_end = span(first).map_or(left, |(_, end)| end);
        let page_limit = layer.page_box.map_or(f64::INFINITY, |[x0, y0, x1, y1]| {
            let along = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
                .map(|(x, y)| x * direction.0 + y * direction.1);
            let start = along.into_iter().fold(f64::MAX, f64::min);
            let end = along.into_iter().fold(f64::MIN, f64::max);
            end - (left - start).clamp(0., size)
        });
        let grow_limit = page_limit
            .min(neighbour(first, own_end) - NEIGHBOUR_GAP_EM * size)
            .max(own_end);
        let breaks = (0..lines.len())
            .map(|index| {
                let Some(next) = lines.get(index + 1) else {
                    return false;
                };
                if block.list.is_some()
                    && list_item_marker(
                        &next.text.trim_start().chars().collect::<Vec<_>>(),
                        block.list.as_ref(),
                    )
                    .is_some()
                {
                    return true;
                }
                let (Some((_, right)), Some((_, first_word))) =
                    (span(&lines[index]), widths[index + 1])
                else {
                    return false;
                };
                right + EARLY_BREAK_SPACE_EM * size + first_word < left + widest - tolerance
            })
            .collect();
        Some(Self {
            layer,
            alignment: block.alignment,
            direction,
            page_edges,
            columns,
            list: block.list,
            lines,
            grows,
            grow_limit,
            pitch,
            em,
            joined_by_space,
            breaks,
            format: ParagraphFormat::default(),
            cell: block.cell,
            reflow: false,
        })
    }

    /// The paragraph laid out afresh at its edges, for a cell whose column
    /// was resized.
    pub fn reflowed(mut self) -> Self {
        self.reflow = true;
        self
    }

    /// The table cell the paragraph is in.
    pub fn cell(&self) -> Option<crate::pdf_text_blocks::BlockCell> {
        self.cell
    }

    /// The paragraph with settings chosen in place of the page's.
    pub fn with_format(mut self, format: ParagraphFormat) -> Self {
        self.format = format;
        self
    }

    pub fn set_format(&mut self, format: ParagraphFormat) {
        self.format = format;
    }

    pub fn format(&self) -> ParagraphFormat {
        self.format
    }

    /// The list the paragraph's lines are items of.
    pub fn list(&self) -> Option<TextList> {
        self.list
    }

    /// The alignment lines are placed with.
    pub fn alignment(&self) -> TextBlockAlignment {
        self.format.alignment.unwrap_or(self.alignment)
    }

    /// Baseline to baseline, as a multiple of the font size.
    pub fn line_spacing(&self) -> f64 {
        self.format.line_spacing.unwrap_or(self.pitch / self.em)
    }

    pub fn lines(&self) -> &[PageTextLine] {
        &self.lines
    }

    pub fn layer(&self) -> &Arc<PageTextLayer> {
        &self.layer
    }

    pub fn page_index(&self) -> u32 {
        self.layer.page_index
    }

    pub fn anchors(&self) -> Vec<TextLineAnchor> {
        self.lines.iter().map(|line| line.anchor).collect()
    }

    /// A page position along the lines, in `line`'s text space.
    fn to_text(&self, line: &PageTextLine, along: f64) -> f64 {
        let m = line.matrix;
        let (x, y) = line.origin();
        (along - (x * self.direction.0 + y * self.direction.1)) / m[0].hypot(m[1])
    }

    /// Where text may run on `line` in a column with `page_edges`: from the
    /// left edge, or for a list item's wrapped line from the list's indent,
    /// to the right edge; in `line`'s text space.
    fn edges_on(&self, line: &PageTextLine, page_edges: (f64, f64), item: bool) -> (f64, f64) {
        let indent = match self.list {
            Some(list) if !item => list.indent,
            _ => 0.,
        };
        (
            self.to_text(line, page_edges.0 + indent),
            self.to_text(line, page_edges.1),
        )
    }

    /// Whether base line `slot`'s text starts a list item.
    fn base_item(&self, slot: usize) -> bool {
        self.list.is_some() && (slot == 0 || self.breaks[slot - 1])
    }

    /// The paragraph's text with each line's current runs (an earlier edit,
    /// or its own) and lines an earlier edit added, joined as the base text
    /// is.
    pub fn text(&self, current: &[Vec<TextEditRun>], extra: &[ExtraLine]) -> Vec<StyledChar> {
        self.text_with_breaks(current, &[], extra)
    }

    /// [`Self::text`], where `breaks` says for each line an earlier edit
    /// changed whether a hard break follows it (`None`: as in the base text).
    pub fn text_with_breaks(
        &self,
        current: &[Vec<TextEditRun>],
        breaks: &[Option<bool>],
        extra: &[ExtraLine],
    ) -> Vec<StyledChar> {
        enum Join {
            Space(TextRunStyle),
            Break(TextRunStyle),
            None,
        }
        // Each line's text, how it joins the next and whether an edit kept
        // it, empty, before a hard break. An emptied line (its text moved
        // below at a chosen spacing) joins nothing, unless every line is
        // empty.
        let mut pieces = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let runs = current
                .get(index)
                .map_or_else(|| line.original_runs(), Clone::clone);
            let chars = styled_runs(&runs);
            let style = chars
                .last()
                .map(|(_, style)| style.clone())
                .or_else(|| line.styles.first().cloned());
            let edited_break = breaks.get(index).copied().flatten();
            let hard = edited_break.unwrap_or(self.breaks[index]);
            let join = match style {
                Some(style) if hard => Join::Break(style),
                Some(style) if self.joined_by_space[index] => Join::Space(style),
                _ => Join::None,
            };
            pieces.push((chars, join, edited_break == Some(true)));
        }
        for line in extra {
            let chars = styled_runs(&line.runs);
            let ends_with_space = chars
                .last()
                .is_some_and(|(character, _)| character.is_whitespace());
            let style = chars
                .last()
                .map(|(_, style)| style.clone())
                .or_else(|| self.lines.last()?.styles.first().cloned());
            let join = match style {
                Some(style) if line.hard_break => Join::Break(style),
                Some(style) if !ends_with_space && !chars.is_empty() => Join::Space(style),
                _ => Join::None,
            };
            pieces.push((chars, join, line.hard_break));
        }
        if pieces.iter().any(|(chars, _, _)| !chars.is_empty()) {
            pieces.retain(|(chars, _, kept)| !chars.is_empty() || *kept);
        }
        let mut text = Vec::new();
        let count = pieces.len();
        for (index, (chars, join, _)) in pieces.into_iter().enumerate() {
            text.extend(chars);
            if index + 1 < count {
                match join {
                    Join::Space(style) => text.push((' ', style)),
                    Join::Break(style) => text.push(('\n', style)),
                    Join::None => {}
                }
            }
        }
        text
    }

    /// The base text and each line's range in it.
    fn base(&self) -> (Vec<StyledChar>, Vec<Range<usize>>) {
        let mut text = Vec::new();
        let mut ranges = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            let start = text.len();
            text.extend(styled(line));
            ranges.push(start..text.len());
            if index + 1 < self.lines.len() && (self.breaks[index] || self.joined_by_space[index]) {
                let style = line.chars.last().map_or_else(
                    || {
                        line.styles
                            .first()
                            .cloned()
                            .unwrap_or_else(|| self.lines[0].styles[0].clone())
                    },
                    |character| line.styles[character.style].clone(),
                );
                text.push((if self.breaks[index] { '\n' } else { ' ' }, style));
            }
        }
        (text, ranges)
    }

    /// Whether `text` is placed afresh line by line: a line spacing is
    /// chosen, or text is larger than any in the base paragraph, so lines
    /// must open up for it.
    fn flowed(&self, text: &[StyledChar], base: &[StyledChar]) -> bool {
        self.format.line_spacing.is_some() || largest_size(text, 0.) > largest_size(base, 0.) * 1.01
    }

    /// Lays out `chars` on `line` without a fit.
    fn measure(
        &self,
        line: &PageTextLine,
        chars: &[StyledChar],
    ) -> Result<TextLineLayout, TextEditError> {
        layout_text_edit_fitted(&self.layer, line, &runs_of(chars), LineFit::default())
    }

    /// The fit that aligns a rewrapped line as the paragraph is aligned,
    /// between `edges` in its text space. A line that keeps its base
    /// line's origin keeps where its text starts; others start at the left
    /// edge. `chars` are the line's: when they begin a list item (`item`),
    /// its text after the marker starts at the list's indent.
    #[allow(clippy::too_many_arguments)]
    fn fit(
        &self,
        line: &PageTextLine,
        (left, right): (f64, f64),
        chars: &[StyledChar],
        item: bool,
        layout: &TextLineLayout,
        last_of_paragraph: bool,
        keeps_origin: bool,
    ) -> LineFit {
        let Some((start, end)) = visible(layout) else {
            return LineFit::default();
        };
        let align_left = if keeps_origin { 0. } else { left - start };
        let mut fit = match self.alignment() {
            TextBlockAlignment::Justified if !last_of_paragraph => LineFit {
                shift: align_left,
                justify_end: Some(right - align_left),
                tab: None,
            },
            TextBlockAlignment::Justified | TextBlockAlignment::Left => LineFit {
                shift: align_left,
                ..LineFit::default()
            },
            TextBlockAlignment::Center => LineFit {
                shift: (left + right) / 2. - (start + end) / 2.,
                ..LineFit::default()
            },
            TextBlockAlignment::Right => LineFit {
                shift: right - end,
                ..LineFit::default()
            },
        };
        if let Some(list) = self.list
            && item
            && let Some((_, marker)) = list_item_marker(
                &chars
                    .iter()
                    .map(|(character, _)| *character)
                    .collect::<Vec<_>>(),
                Some(&list),
            )
            && marker < chars.len()
        {
            let indent = list.indent / line.matrix[0].hypot(line.matrix[1]);
            // The marker starts at the left edge, or where the line's text
            // starts when it keeps its origin.
            let marker_at = if keeps_origin { start } else { left };
            fit.tab = Some((marker, marker_at + indent - fit.shift));
        }
        fit
    }

    /// Wraps `text` (the paragraph as edited) into lines.
    pub fn wrap(&self, text: &[StyledChar]) -> Result<Vec<WrappedLine>, TextEditError> {
        let (base, ranges) = self.base();
        if self.flowed(text, &base) {
            self.wrap_flowed(text)
        } else {
            self.wrap_in_place(text, &base, &ranges)
        }
    }

    /// Where slot `slot` is drawn when lines keep their places: its base
    /// line or, past the last, a line added below at the paragraph's pitch;
    /// its column's page edges; and how far below the last line it is.
    fn place(&self, slot: usize) -> (PageTextLine, (f64, f64), f64) {
        let count = self.lines.len();
        if slot < count {
            (self.lines[slot].clone(), self.page_edges[slot], 0.)
        } else {
            let below = self.pitch * (slot + 1 - count) as f64;
            (
                extra_line_base(&self.lines[count - 1], below),
                self.page_edges[count - 1],
                below,
            )
        }
    }

    /// Line `index`'s baseline across the last line's text space.
    fn height_in_last(&self, index: usize) -> f64 {
        let last = &self.lines[self.lines.len() - 1];
        let (x, y) = self.lines[index].origin();
        last.text_space_point(x, y).map_or(0., |(_, y)| y)
    }

    /// Wraps `text` onto the paragraph's own lines, adding lines below when
    /// it grows. Lines the edit does not reach keep their bytes.
    fn wrap_in_place(
        &self,
        text: &[StyledChar],
        base: &[StyledChar],
        ranges: &[Range<usize>],
    ) -> Result<Vec<WrappedLine>, TextEditError> {
        let count = self.lines.len();
        // A chosen alignment, or a resized cell, places every line afresh.
        let restyled = !self.format.is_default() || self.reflow;
        let prefix = base
            .iter()
            .zip(text)
            .take_while(|(left, right)| left == right)
            .count();
        let suffix = base
            .iter()
            .rev()
            .zip(text.iter().rev())
            .take_while(|(left, right)| left == right)
            .count()
            .min(base.len().min(text.len()) - prefix);
        let delta = text.len() as isize - base.len() as isize;
        let unchanged = text.len() == base.len() && prefix == base.len();
        // The first line the edit reaches: lines before it keep their bytes.
        let first_changed = if restyled {
            0
        } else if unchanged {
            count
        } else {
            ranges
                .iter()
                .position(|range| prefix <= range.end)
                .unwrap_or(count - 1)
                .min(count - 1)
        };
        let kept = |slot: usize, range: Range<usize>, chars: &[StyledChar]| {
            let line = self.lines[slot].clone();
            self.measure(&line, chars).map(|layout| WrappedLine {
                range,
                slot,
                line,
                layout,
                fit: LineFit::default(),
                unchanged: true,
                added: false,
                below: 0.,
            })
        };
        let mut wrapped = Vec::new();
        for (slot, range) in ranges.iter().enumerate().take(first_changed) {
            wrapped.push(kept(slot, range.clone(), &base[range.clone()])?);
        }
        if first_changed == count {
            return Ok(wrapped);
        }

        // Where base line `slot` starts in the edited text, when it lies in
        // the unchanged tail.
        let base_start_in_text = |slot: usize| -> Option<usize> {
            let start = ranges.get(slot)?.start;
            let mapped = start as isize + delta;
            (mapped >= (text.len() - suffix) as isize && start >= prefix).then_some(mapped as usize)
        };
        let mut position = ranges[first_changed].start;
        let mut slot = first_changed;
        while position < text.len() || slot < count {
            if slot >= count + MAX_EXTRA_LINES {
                return Err(TextEditError::LimitExceeded(
                    "the paragraph is too long".into(),
                ));
            }
            // The rest of the paragraph is unchanged from here: keep its lines.
            if slot > first_changed
                && !restyled
                && let Some(start) = base_start_in_text(slot)
                && start == position
            {
                for (slot, base_range) in ranges.iter().enumerate().skip(slot) {
                    let start = (base_range.start as isize + delta) as usize;
                    let range = start..start + base_range.len();
                    wrapped.push(kept(slot, range.clone(), &text[range])?);
                }
                break;
            }
            let (mut line, page_edges, mut below) = self.place(slot);
            let mut added = slot >= count;
            // A list line that changes between an item and an item's wrapped
            // line is drawn afresh in its place: matching the base line's
            // glyphs would carry its marker's spacing into the text.
            if !added
                && position < text.len()
                && self.list.is_some()
                && self.base_item(slot) != starts_item(text, position)
            {
                below = -self.height_in_last(slot);
                line = extra_line_base(&self.lines[count - 1], below);
                added = true;
            }
            if position >= text.len() {
                // The text ran out: no line is drawn on the remaining base
                // lines, which `edits` empties, but for one to hold the
                // caret after a hard break at the end, or in emptied text.
                if !wrapped.is_empty() && !ends_in_break(text, &wrapped) {
                    break;
                }
                let layout = self.measure(&line, &[])?;
                wrapped.push(WrappedLine {
                    range: position..position,
                    slot,
                    line,
                    layout,
                    fit: LineFit::default(),
                    unchanged: false,
                    added,
                    below,
                });
                slot += 1;
                continue;
            }
            let item = starts_item(text, position);
            let edges = self.edges_on(&line, page_edges, item);
            let keeps_origin = !added && (self.list.is_none() || self.base_item(slot) == item);
            let (end, next, hard) = self.break_line(
                &line,
                (!keeps_origin).then_some(edges.0),
                edges.1,
                item,
                text,
                position,
            )?;
            let layout = self.measure(&line, &text[position..end])?;
            // A line that comes out as it was, ending as it did, keeps its
            // bytes.
            let same_as_base = !restyled
                && slot < count
                && text[position..end] == base[ranges[slot].clone()]
                && hard == (slot + 1 < count && self.breaks[slot]);
            let last_of_paragraph = hard || next >= text.len();
            let fit = if same_as_base {
                LineFit::default()
            } else {
                self.fit(
                    &line,
                    edges,
                    &text[position..end],
                    item,
                    &layout,
                    last_of_paragraph,
                    keeps_origin,
                )
            };
            let layout =
                layout_text_edit_fitted(&self.layer, &line, &runs_of(&text[position..end]), fit)?;
            wrapped.push(WrappedLine {
                range: position..end,
                slot,
                line,
                layout,
                fit,
                unchanged: same_as_base,
                added,
                below,
            });
            position = next;
            slot += 1;
        }
        // A hard break at the end starts an empty line, for the caret.
        if ends_in_break(text, &wrapped) {
            let (line, _, below) = self.place(slot);
            let layout = self.measure(&line, &[])?;
            wrapped.push(WrappedLine {
                range: text.len()..text.len(),
                slot,
                line,
                layout,
                fit: LineFit::default(),
                unchanged: false,
                added: slot >= count,
                below,
            });
        }
        Ok(wrapped)
    }

    /// Places every line afresh: the first on the paragraph's first line,
    /// each next as far below as the line spacing and its largest text
    /// need, moving to the top of the next column at the foot of one.
    fn wrap_flowed(&self, text: &[StyledChar]) -> Result<Vec<WrappedLine>, TextEditError> {
        let count = self.lines.len();
        let last = &self.lines[count - 1];
        let spacing = self.line_spacing();
        let columns = self
            .columns
            .iter()
            .map(|lines| {
                (
                    self.height_in_last(lines.start),
                    self.height_in_last(lines.end - 1),
                    self.page_edges[lines.start],
                )
            })
            .collect::<Vec<_>>();
        let first = &self.lines[0];
        let mut wrapped = Vec::new();
        let mut position = 0;
        let mut column = 0;
        let mut baseline = columns[0].0;
        loop {
            let slot = wrapped.len();
            if slot > count + MAX_EXTRA_LINES {
                return Err(TextEditError::LimitExceeded(
                    "the paragraph is too long".into(),
                ));
            }
            // Text ending in a hard break ends on an empty line, for the
            // caret.
            if slot > 0 && position >= text.len() && !ends_in_break(text, &wrapped) {
                break;
            }
            let item = starts_item(text, position);
            if slot == 0 {
                // The first line keeps its baseline.
                let edges = self.edges_on(first, self.page_edges[0], item);
                let keeps_origin = self.list.is_none() || item;
                let (end, next, hard) = self.break_line(
                    first,
                    (!keeps_origin).then_some(edges.0),
                    edges.1,
                    item,
                    text,
                    position,
                )?;
                let layout = self.measure(first, &text[position..end])?;
                let fit = self.fit(
                    first,
                    edges,
                    &text[position..end],
                    item,
                    &layout,
                    hard || next >= text.len(),
                    keeps_origin,
                );
                wrapped.push(WrappedLine {
                    range: position..end,
                    slot,
                    line: first.clone(),
                    layout: layout_text_edit_fitted(
                        &self.layer,
                        first,
                        &runs_of(&text[position..end]),
                        fit,
                    )?,
                    fit,
                    unchanged: false,
                    added: false,
                    below: 0.,
                });
                position = next;
                continue;
            }
            // The line in the current column, or at the top of the next
            // when it would fall below this one's foot.
            let placed = |column: usize, top: Option<f64>| {
                let measuring = extra_line_base(last, 0.);
                let edges = self.edges_on(&measuring, columns[column].2, item);
                let (end, next, hard) =
                    self.break_line(&measuring, Some(edges.0), edges.1, item, text, position)?;
                let at = top.unwrap_or_else(|| {
                    baseline - spacing * largest_size(&text[position..end], self.em)
                });
                Ok::<_, TextEditError>((end, next, hard, at, edges))
            };
            let mut line = placed(column, None)?;
            if column + 1 < columns.len() && line.3 < columns[column].1 - 0.5 * spacing * self.em {
                column += 1;
                line = placed(column, Some(columns[column].0))?;
            }
            let (end, next, hard, at, edges) = line;
            baseline = at;
            let below = -at;
            let base = extra_line_base(last, below);
            let layout = self.measure(&base, &text[position..end])?;
            let fit = self.fit(
                &base,
                edges,
                &text[position..end],
                item,
                &layout,
                hard || next >= text.len(),
                false,
            );
            wrapped.push(WrappedLine {
                range: position..end,
                slot,
                layout: layout_text_edit_fitted(
                    &self.layer,
                    &base,
                    &runs_of(&text[position..end]),
                    fit,
                )?,
                line: base,
                fit,
                unchanged: false,
                added: true,
                below,
            });
            position = next;
        }
        Ok(wrapped)
    }

    /// Where the line on `line` starting at `start` ends when text may run
    /// to `right`, starting where it is laid out or, given `left`, there:
    /// its end, the start of the next line, and whether a hard break ended
    /// it. A list item (`item`) also needs the room its marker's tab takes.
    #[allow(clippy::too_many_arguments)]
    fn break_line(
        &self,
        line: &PageTextLine,
        left: Option<f64>,
        right: f64,
        item: bool,
        text: &[StyledChar],
        start: usize,
    ) -> Result<(usize, usize, bool), TextEditError> {
        let right = if self.grows {
            self.to_text(line, self.grow_limit)
        } else {
            right
        };
        let tolerance = if self.alignment() == TextBlockAlignment::Justified {
            WIDTH_TOLERANCE
        } else {
            WIDTH_TOLERANCE.max(RAGGED_TOLERANCE * (right - left.unwrap_or(0.)))
        };
        let marker = self.list.filter(|_| item).and_then(|list| {
            let chars = text[start..]
                .iter()
                .map(|(character, _)| *character)
                .collect::<Vec<_>>();
            list_item_marker(&chars, Some(&list)).map(|(_, length)| (list, length))
        });
        // Where the text ends once placed, from its layout.
        let placed_end = |layout: &TextLineLayout| {
            visible(layout).map(|(first, end)| {
                let at = left.unwrap_or(first);
                let tab = marker
                    .and_then(|(list, length)| {
                        let after = layout.chars.get(length)?.x - first + at;
                        let target = at + list.indent / line.matrix[0].hypot(line.matrix[1]);
                        Some((target - after).max(0.))
                    })
                    .unwrap_or(0.);
                end - first + at + tab
            })
        };
        let hard = text[start..]
            .iter()
            .position(|(character, _)| *character == '\n')
            .map(|offset| start + offset);
        let limit = hard.unwrap_or(text.len());
        // Break opportunities: after each run of spaces.
        let mut candidates = Vec::new();
        let mut index = start;
        while index < limit {
            if text[index].0.is_whitespace() {
                let mut after = index;
                while after < limit && text[after].0.is_whitespace() {
                    after += 1;
                }
                candidates.push((index, after));
                index = after;
            } else {
                index += 1;
            }
        }
        candidates.push((limit, limit));
        let mut chosen = None;
        for &(space, after) in &candidates {
            let layout = self.measure(line, &text[start..space])?;
            let fits = placed_end(&layout).is_none_or(|end| end <= right + tolerance);
            if fits || chosen.is_none() {
                chosen = Some((space, after));
                if !fits {
                    break;
                }
            } else {
                break;
            }
        }
        let (space, after) = chosen.expect("a line has at least one break");
        if after == limit && space == limit {
            return Ok(match hard {
                Some(hard) => (hard, hard + 1, true),
                None => (limit, limit, false),
            });
        }
        // Lines joined by a space end before it; others keep it.
        let keep_space = self
            .joined_by_space
            .iter()
            .filter(|joined| !**joined)
            .count()
            > self.joined_by_space.len() / 2;
        let end = if keep_space {
            (space + 1).min(after)
        } else {
            space
        };
        Ok((end, after, false))
    }

    /// The page-text edits that make the page show `wrapped`.
    pub fn edits(&self, text: &[StyledChar], wrapped: &[WrappedLine]) -> Vec<PageTextEdit> {
        let count = self.lines.len();
        let page_index = self.layer.page_index;
        let hard_break = |line: &WrappedLine| {
            text.get(line.range.end)
                .is_some_and(|(character, _)| *character == '\n')
        };
        let mut edits = Vec::new();
        for line in wrapped.iter().filter(|line| !line.added && !line.unchanged) {
            let base = &self.lines[line.slot];
            edits.push(PageTextEdit {
                page_index,
                anchor: base.anchor,
                original: base.original_runs(),
                runs: normalized_runs(&runs_of(&text[line.range.clone()])),
                layout: TextEditLayout {
                    fit: line.fit,
                    hard_break: hard_break(line),
                    ..Default::default()
                },
            });
        }
        // Base lines no line is drawn on are emptied: placed afresh, their
        // text is drawn on added lines.
        for base in self.lines.iter().enumerate().filter_map(|(slot, base)| {
            (!wrapped.iter().any(|line| !line.added && line.slot == slot)).then_some(base)
        }) {
            edits.push(PageTextEdit {
                page_index,
                anchor: base.anchor,
                original: base.original_runs(),
                runs: Vec::new(),
                layout: TextEditLayout::default(),
            });
        }
        // An added line's anchor must not coincide with a line already on
        // the page (lines at one origin are told apart by their order, which
        // the added line would change): move its origin, not its glyphs, by
        // hundredths of a point until it is free.
        let mut taken = self
            .layer
            .lines
            .iter()
            .map(|line| (line.anchor.x_centipoints, line.anchor.y_centipoints))
            .collect::<std::collections::BTreeSet<_>>();
        let last = &self.lines[count - 1];
        let step = 0.011 / last.matrix[0].hypot(last.matrix[1]);
        let extras = wrapped
            .iter()
            .filter(|line| line.added)
            .map(|line| {
                let mut origin_shift = 0.;
                let anchor = loop {
                    let base = extra_line_base(last, line.below);
                    let m = base.matrix;
                    let origin = (m[0] * origin_shift + m[4], m[1] * origin_shift + m[5]);
                    let anchor = anchor_at(origin);
                    if taken.insert((anchor.x_centipoints, anchor.y_centipoints)) {
                        break anchor;
                    }
                    origin_shift += step;
                };
                ExtraLine {
                    anchor,
                    below: line.below,
                    origin_shift,
                    fit: line.fit,
                    runs: normalized_runs(&runs_of(&text[line.range.clone()])),
                    hard_break: hard_break(line),
                }
            })
            .collect::<Vec<_>>();
        if !extras.is_empty() {
            match edits.iter_mut().find(|edit| edit.anchor == last.anchor) {
                Some(edit) => edit.layout.extra_lines = extras,
                None => edits.push(PageTextEdit {
                    page_index,
                    anchor: last.anchor,
                    original: last.original_runs(),
                    runs: last.original_runs(),
                    layout: TextEditLayout {
                        extra_lines: extras,
                        ..Default::default()
                    },
                }),
            }
        }
        // The first line keeps the chosen settings for reopening.
        if !self.format.is_default() {
            let first = &self.lines[0];
            match edits.iter_mut().find(|edit| edit.anchor == first.anchor) {
                Some(edit) => edit.layout.paragraph = self.format,
                None => edits.insert(
                    0,
                    PageTextEdit {
                        page_index,
                        anchor: first.anchor,
                        original: first.original_runs(),
                        runs: first.original_runs(),
                        layout: TextEditLayout {
                            paragraph: self.format,
                            ..Default::default()
                        },
                    },
                ),
            }
        }
        edits
    }
}
