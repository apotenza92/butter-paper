//! The text of a page as a search reads it: one string, with each of its
//! characters tied to the place it has in the page's selectable lines. A
//! match in the string is thus a range of those lines, which gives the boxes
//! to highlight and the place to reveal. The lines are those the Select tool
//! copies, so a search finds the text that Copy copies, edits included.

use std::{borrow::Cow, ops::Range, sync::Arc};

use crate::{
    pdf_tables::PageRule,
    pdf_text_selection::{Quad, SelectableLine, TextPosition, selection_quads},
};

/// The most space between two lines, as a share of the taller line's box,
/// that still leaves them adjacent for a phrase.
const JOIN_MAX_GAP: f64 = 0.6;
/// How much of the shorter line's box the two may share along the page and
/// still be stacked lines (their boxes of ascent and descent overlap a
/// little in tight text).
const JOIN_MAX_SHARED_HEIGHT: f64 = 0.5;

/// The box of a line's visible characters in page space: left, bottom,
/// right, top. `None` for a line that is not horizontal on the page or that
/// has nothing visible.
fn horizontal_line_box(line: &SelectableLine) -> Option<[f64; 4]> {
    let matrix = line.line.matrix;
    if matrix[1].abs() > 1e-6 || matrix[2].abs() > 1e-6 || matrix[0] <= 0. || matrix[3] <= 0. {
        return None;
    }
    let mut visible = line.chars.iter().filter(|(c, _, _)| !c.is_whitespace());
    let first = visible.next()?;
    let last = visible.next_back().unwrap_or(first);
    let quad = line.line.quad(first.1, last.2);
    let xs = quad.iter().map(|corner| corner.0);
    let ys = quad.iter().map(|corner| corner.1);
    Some([
        xs.clone().fold(f64::MAX, f64::min),
        ys.clone().fold(f64::MAX, f64::min),
        xs.fold(f64::MIN, f64::max),
        ys.fold(f64::MIN, f64::max),
    ])
}

/// Whether a phrase may run from the end of `above` to the start of
/// `below`: the lines of one paragraph, or two lines that are adjacent,
/// share some width, and have no rule of a table between them. The page's
/// own paragraph blocks miss text that a table's cells wrap.
fn lines_join(above: &SelectableLine, below: &SelectableLine, rules: &[PageRule]) -> bool {
    if above.paragraph == below.paragraph {
        return true;
    }
    let (Some(a), Some(b)) = (horizontal_line_box(above), horizontal_line_box(below)) else {
        return false;
    };
    let left = a[0].max(b[0]);
    let right = a[2].min(b[2]);
    if right <= left {
        return false;
    }
    let (height_a, height_b) = (a[3] - a[1], b[3] - b[1]);
    // The second line is under the first, not beside or above it.
    let (centre_a, centre_b) = ((a[1] + a[3]) / 2., (b[1] + b[3]) / 2.);
    if centre_b >= centre_a {
        return false;
    }
    let shared = (a[3].min(b[3]) - a[1].max(b[1])).max(0.);
    if shared > JOIN_MAX_SHARED_HEIGHT * height_a.min(height_b) {
        return false;
    }
    let gap = a[1] - b[3];
    if gap > JOIN_MAX_GAP * height_a.max(height_b) {
        return false;
    }
    !rules.iter().any(|rule| {
        rule.horizontal
            && rule.at < centre_a
            && rule.at > centre_b
            && rule.from < right
            && rule.to > left
    })
}

/// What a Latin ligature (U+FB00 to U+FB06) reads as. A search finds "fi" in
/// a page that draws the ligature "ﬁ", and the other way round.
pub fn ligature_expansion(character: char) -> Option<&'static str> {
    Some(match character {
        '\u{FB00}' => "ff",
        '\u{FB01}' => "fi",
        '\u{FB02}' => "fl",
        '\u{FB03}' => "ffi",
        '\u{FB04}' => "ffl",
        '\u{FB05}' | '\u{FB06}' => "st",
        _ => return None,
    })
}

/// `text` with each ligature read as its letters.
pub fn expand_ligatures(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|c| ligature_expansion(c).is_some()) {
        return Cow::Borrowed(text);
    }
    let mut expanded = String::with_capacity(text.len() + 4);
    for character in text.chars() {
        match ligature_expansion(character) {
            Some(letters) => expanded.push_str(letters),
            None => expanded.push(character),
        }
    }
    Cow::Owned(expanded)
}

/// One character of the page's lines in the search string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Entry {
    /// Where its text starts in the string, in bytes.
    offset: u32,
    /// The bytes it takes: more than its own for a ligature.
    len: u32,
    position: TextPosition,
}

/// A page's text, ready to search.
#[derive(Clone, Debug)]
pub struct PageSearchText {
    pub page_index: u32,
    /// The lines in reading order. A space joins two lines that continue
    /// each other (see [`PageSearchText::with_rules`]) and a line break
    /// joins any other two, so a phrase matches across the first and not
    /// across the second. Ligatures read as their letters.
    pub text: String,
    entries: Vec<Entry>,
    pub lines: Arc<Vec<SelectableLine>>,
}

/// A range of the page's lines that a search matched.
#[derive(Clone, Debug, PartialEq)]
pub struct TextMatch {
    pub start: TextPosition,
    pub end: TextPosition,
    /// Where the match is in [`PageSearchText::text`], in bytes.
    pub range: Range<usize>,
}

/// A match with the text around it, for a result list.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Snippet {
    pub before: String,
    pub matched: String,
    pub after: String,
    /// The parts of `matched` (byte ranges) that the terms matched, when a
    /// snippet holds more than one match. Empty: all of `matched` is one.
    pub marks: Vec<Range<usize>>,
}

impl PageSearchText {
    /// A page whose lines join by paragraph only.
    pub fn new(page_index: u32, lines: Arc<Vec<SelectableLine>>) -> Self {
        Self::with_rules(page_index, lines, &[])
    }

    /// A page whose phrases run across two lines when they are of one
    /// paragraph, or are adjacent, overlap along the page, and no `rules`
    /// (the page's table rules) lie between them.
    pub fn with_rules(
        page_index: u32,
        lines: Arc<Vec<SelectableLine>>,
        rules: &[PageRule],
    ) -> Self {
        let mut text = String::new();
        let mut entries = Vec::new();
        for (line_index, line) in lines.iter().enumerate() {
            if line_index > 0 {
                text.push(if lines_join(&lines[line_index - 1], line, rules) {
                    ' '
                } else {
                    '\n'
                });
            }
            for (index, (character, _, _)) in line.chars.iter().enumerate() {
                let offset = text.len();
                match ligature_expansion(*character) {
                    Some(letters) => text.push_str(letters),
                    None => text.push(*character),
                }
                entries.push(Entry {
                    offset: offset as u32,
                    len: (text.len() - offset) as u32,
                    position: TextPosition {
                        line: line_index,
                        index,
                    },
                });
            }
        }
        Self {
            page_index,
            text,
            entries,
            lines,
        }
    }

    /// Whether the page has nothing a search can match: a scan, or a drawing
    /// with its text drawn as lines.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// The matches `ranges` (byte ranges of [`Self::text`], in any order,
    /// overlapping ones joined) make. A range that holds only the spaces and
    /// line breaks between lines is no match.
    pub fn matches(&self, ranges: impl IntoIterator<Item = Range<usize>>) -> Vec<TextMatch> {
        let mut ranges = ranges.into_iter().collect::<Vec<_>>();
        ranges.sort_by_key(|range| (range.start, range.end));
        let mut joined: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            match joined.last_mut() {
                Some(last) if range.start < last.end => last.end = last.end.max(range.end),
                _ => joined.push(range),
            }
        }
        joined
            .into_iter()
            .filter_map(|range| {
                // The first character that ends after the range starts, and
                // the last that starts before it ends.
                let first = self
                    .entries
                    .partition_point(|entry| (entry.offset + entry.len) as usize <= range.start);
                let after = self
                    .entries
                    .partition_point(|entry| (entry.offset as usize) < range.end);
                if first >= after {
                    return None;
                }
                let last = self.entries[after - 1].position;
                Some(TextMatch {
                    start: self.entries[first].position,
                    end: TextPosition {
                        line: last.line,
                        index: last.index + 1,
                    },
                    range,
                })
            })
            .collect()
    }

    /// The boxes of a match, in page space: those the Select tool shows for
    /// the same range.
    pub fn quads(&self, found: &TextMatch) -> Vec<Quad> {
        selection_quads(&self.lines, found.start, found.end)
    }

    /// The text of a match as the page's lines have it.
    pub fn matched_text(&self, found: &TextMatch) -> &str {
        &self.text[found.range.clone()]
    }

    /// `range` with up to `context` characters of text each side. Line
    /// breaks and runs of spaces read as one space.
    pub fn snippet(&self, range: Range<usize>, context: usize) -> Snippet {
        snippet_of(&self.text, range, context)
    }

    /// An estimate of the memory the page takes, in bytes, for the cache.
    pub fn approximate_bytes(&self) -> usize {
        let lines = self
            .lines
            .iter()
            .map(|line| {
                std::mem::size_of::<SelectableLine>()
                    + line.chars.len() * std::mem::size_of::<(char, f64, f64)>()
                    + line.line.chars.len() * 64
                    + line.line.text.len()
                    + 256
            })
            .sum::<usize>();
        self.text.len() + self.entries.len() * std::mem::size_of::<Entry>() + lines
    }
}

/// [`PageSearchText::snippet`] for any text.
pub fn snippet_of(text: &str, range: Range<usize>, context: usize) -> Snippet {
    let range = range.start.min(text.len())..range.end.min(text.len());
    let tidy = |part: &str| {
        let mut tidy = String::with_capacity(part.len());
        for character in part.chars() {
            if character.is_whitespace() {
                if !tidy.ends_with(' ') {
                    tidy.push(' ');
                }
            } else {
                tidy.push(character);
            }
        }
        tidy
    };
    let before_start = text[..range.start]
        .char_indices()
        .rev()
        .nth(context.saturating_sub(1))
        .map_or(0, |(index, _)| index);
    let after_end = text[range.end..]
        .char_indices()
        .nth(context)
        .map_or(text.len(), |(index, _)| range.end + index);
    let before = tidy(&text[before_start..range.start]);
    let after = tidy(&text[range.end..after_end]);
    Snippet {
        before: if before_start == 0 {
            before.trim_start().to_owned()
        } else {
            before
        },
        matched: tidy(&text[range]),
        after: if after_end == text.len() {
            after.trim_end().to_owned()
        } else {
            after
        },
        marks: Vec::new(),
    }
}

/// A snippet of several matches near each other (`ranges`, sorted, not
/// overlapping): `matched` runs from the first to the last, and `marks`
/// gives each match in it.
pub fn snippet_of_ranges(text: &str, ranges: &[Range<usize>], context: usize) -> Snippet {
    let (Some(first), Some(last)) = (ranges.first(), ranges.last()) else {
        return Snippet::default();
    };
    let mut snippet = snippet_of(text, first.start..last.end, context);
    if ranges.len() < 2 {
        return snippet;
    }
    let mut matched = String::new();
    let mut marks = Vec::with_capacity(ranges.len());
    let push = |out: &mut String, part: &str| {
        for character in part.chars() {
            if character.is_whitespace() {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            } else {
                out.push(character);
            }
        }
    };
    let mut end = first.start;
    for range in ranges {
        let range = range.start.min(text.len())..range.end.min(text.len());
        if range.start > end {
            push(&mut matched, &text[end..range.start]);
        }
        let start = matched.len();
        push(&mut matched, &text[range.start.max(end)..range.end]);
        marks.push(start..matched.len());
        end = end.max(range.end);
    }
    snippet.matched = matched;
    snippet.marks = marks;
    snippet
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::pdf_text_edit::{PageTextLine, TextLineAnchor};

    /// A horizontal line of `text` at `x` and `baseline`: each character is
    /// 5 units wide, with no styles.
    pub(crate) fn line(text: &str, paragraph: usize, x: f64, baseline: f64) -> SelectableLine {
        SelectableLine {
            line: PageTextLine {
                anchor: TextLineAnchor {
                    x_centipoints: (x * 100.) as i64,
                    y_centipoints: (baseline * 100.) as i64,
                    ordinal: 0,
                },
                text: text.to_owned(),
                chars: Vec::new(),
                end_x: 0.,
                styles: Vec::new(),
                matrix: [1., 0., 0., 1., x, baseline],
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
            },
            chars: text
                .chars()
                .enumerate()
                .map(|(index, character)| (character, index as f64 * 5., index as f64 * 5. + 5.))
                .collect(),
            paragraph,
        }
    }

    pub(crate) fn page(lines: Vec<SelectableLine>) -> PageSearchText {
        PageSearchText::new(0, Arc::new(lines))
    }

    fn find(page: &PageSearchText, needle: &str) -> Vec<Range<usize>> {
        page.text
            .match_indices(needle)
            .map(|(start, found)| start..start + found.len())
            .collect()
    }

    #[test]
    fn lines_of_a_paragraph_join_with_a_space_and_paragraphs_with_a_break() {
        let page = page(vec![
            line("fire", 0, 0., 100.),
            line("door", 0, 0., 90.),
            line("exit", 1, 0., 70.),
        ]);
        assert_eq!(page.text, "fire door\nexit");
    }

    fn rule(at: f64, from: f64, to: f64) -> PageRule {
        PageRule {
            horizontal: true,
            at,
            from,
            to,
            width: 0.5,
        }
    }

    fn joined(lines: Vec<SelectableLine>, rules: &[PageRule]) -> String {
        PageSearchText::with_rules(0, Arc::new(lines), rules).text
    }

    #[test]
    fn adjacent_lines_that_overlap_join_although_they_are_not_one_paragraph() {
        // A table cell's wrapped text, which the paragraph blocks miss.
        let lines = vec![line("fire", 0, 0., 100.), line("door", 1, 0., 90.)];
        assert_eq!(joined(lines, &[]), "fire door");
    }

    #[test]
    fn a_rule_between_two_lines_keeps_them_apart() {
        let lines = vec![line("fire", 0, 0., 100.), line("door", 1, 0., 90.)];
        assert_eq!(joined(lines.clone(), &[rule(95., 0., 40.)]), "fire\ndoor");
        // A rule that does not reach their shared width, or lies beside
        // them, does not.
        assert_eq!(joined(lines.clone(), &[rule(95., 30., 80.)]), "fire door");
        assert_eq!(joined(lines.clone(), &[rule(120., 0., 40.)]), "fire door");
        assert_eq!(joined(lines, &[rule(70., 0., 40.)]), "fire door");
    }

    #[test]
    fn lines_that_share_no_width_do_not_join() {
        let lines = vec![line("fire", 0, 0., 100.), line("door", 1, 60., 90.)];
        assert_eq!(joined(lines, &[]), "fire\ndoor");
    }

    #[test]
    fn lines_far_apart_or_in_the_wrong_order_do_not_join() {
        let far = vec![line("fire", 0, 0., 100.), line("door", 1, 0., 70.)];
        assert_eq!(joined(far, &[]), "fire\ndoor");
        let upwards = vec![line("fire", 0, 0., 90.), line("door", 1, 0., 100.)];
        assert_eq!(joined(upwards, &[]), "fire\ndoor");
        let beside = vec![line("fire", 0, 0., 100.), line("door", 1, 10., 100.)];
        assert_eq!(joined(beside, &[]), "fire\ndoor");
    }

    #[test]
    fn a_phrase_runs_across_lines_that_join_and_not_across_a_rule() {
        let lines = vec![
            line("fire", 0, 0., 100.),
            line("door", 1, 0., 90.),
            line("exit", 2, 0., 80.),
        ];
        let page = PageSearchText::with_rules(0, Arc::new(lines), &[rule(85., 0., 40.)]);
        assert_eq!(page.text, "fire door\nexit");
        let matches = page.matches(find(&page, "fire door"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].end, TextPosition { line: 1, index: 4 });
        assert!(find(&page, "door exit").is_empty());
    }

    #[test]
    fn a_line_that_is_not_horizontal_joins_by_paragraph_only() {
        let mut turned = line("door", 1, 0., 90.);
        turned.line.matrix = [0., 1., -1., 0., 0., 90.];
        let lines = vec![line("fire", 0, 0., 100.), turned.clone()];
        assert_eq!(joined(lines, &[]), "fire\ndoor");
        turned.paragraph = 0;
        let lines = vec![line("fire", 0, 0., 100.), turned];
        assert_eq!(joined(lines, &[]), "fire door");
    }

    #[test]
    fn a_range_in_one_line_is_the_characters_it_covers() {
        let page = page(vec![line("to stair 2: fire door", 0, 0., 100.)]);
        let matches = page.matches(find(&page, "fire"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, TextPosition { line: 0, index: 12 });
        assert_eq!(matches[0].end, TextPosition { line: 0, index: 16 });
        assert_eq!(page.matched_text(&matches[0]), "fire");
    }

    #[test]
    fn a_range_across_two_lines_of_a_paragraph_ends_in_the_second() {
        let page = page(vec![line("fire", 0, 0., 100.), line("door", 0, 0., 90.)]);
        let matches = page.matches(find(&page, "fire door"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].start, TextPosition { line: 0, index: 0 });
        assert_eq!(matches[0].end, TextPosition { line: 1, index: 4 });
    }

    #[test]
    fn the_boxes_of_a_match_are_those_selecting_its_range_gives() {
        let page = page(vec![line("fire", 0, 0., 100.), line("door", 0, 0., 90.)]);
        let matches = page.matches(find(&page, "re do"));
        let expected = selection_quads(
            &page.lines,
            TextPosition { line: 0, index: 2 },
            TextPosition { line: 1, index: 2 },
        );
        assert_eq!(page.quads(&matches[0]), expected);
        assert_eq!(expected.len(), 2);
    }

    #[test]
    fn a_range_of_only_the_space_between_lines_is_no_match() {
        let page = page(vec![line("fire", 0, 0., 100.), line("door", 0, 0., 90.)]);
        assert!(page.matches(std::iter::once(4..5)).is_empty());
    }

    #[test]
    fn a_range_that_starts_in_the_space_starts_at_the_next_character() {
        let page = page(vec![line("fire", 0, 0., 100.), line("door", 0, 0., 90.)]);
        let matches = page.matches(std::iter::once(4..7));
        assert_eq!(matches[0].start, TextPosition { line: 1, index: 0 });
        assert_eq!(matches[0].end, TextPosition { line: 1, index: 2 });
    }

    #[test]
    fn overlapping_ranges_join_into_one_match() {
        let page = page(vec![line("abcdefgh", 0, 0., 100.)]);
        let matches = page.matches([2..5, 0..3, 6..8]);
        assert_eq!(
            matches.iter().map(|m| m.range.clone()).collect::<Vec<_>>(),
            [0..5, 6..8]
        );
    }

    #[test]
    fn a_ligature_reads_as_its_letters_and_a_match_inside_it_covers_it() {
        let page = page(vec![line("o\u{FB01}ce \u{FB03}x", 0, 0., 100.)]);
        assert_eq!(page.text, "ofice ffix");
        // "fi" is inside the first ligature, and again inside the second.
        let matches = page.matches(find(&page, "fi"));
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start, TextPosition { line: 0, index: 1 });
        assert_eq!(matches[0].end, TextPosition { line: 0, index: 2 });
        assert_eq!(matches[1].start, TextPosition { line: 0, index: 5 });
        assert_eq!(matches[1].end, TextPosition { line: 0, index: 6 });
        // "ffi" is the whole of the second ligature.
        let matches = page.matches(find(&page, "ffi"));
        assert_eq!(matches[0].start, TextPosition { line: 0, index: 5 });
        assert_eq!(matches[0].end, TextPosition { line: 0, index: 6 });
        // A match that runs on past a ligature ends after the next character.
        let matches = page.matches(find(&page, "fix"));
        assert_eq!(matches[0].start, TextPosition { line: 0, index: 5 });
        assert_eq!(matches[0].end, TextPosition { line: 0, index: 7 });
    }

    #[test]
    fn expanding_ligatures_covers_the_latin_block() {
        assert_eq!(
            expand_ligatures("\u{FB00}\u{FB01}\u{FB02}\u{FB03}\u{FB04}\u{FB05}\u{FB06}"),
            "fffiflffifflstst"
        );
        assert!(matches!(expand_ligatures("plain"), Cow::Borrowed("plain")));
    }

    #[test]
    fn a_snippet_gives_context_and_reads_breaks_as_spaces() {
        let text = "alpha  beta\ngamma delta epsilon zeta";
        let start = text.find("delta").unwrap();
        let snippet = snippet_of(text, start..start + 5, 8);
        assert_eq!(snippet.before, "a gamma ");
        assert_eq!(snippet.matched, "delta");
        assert_eq!(snippet.after, " epsilon");
    }

    #[test]
    fn a_snippet_at_the_ends_of_the_text_has_no_padding() {
        let snippet = snippet_of("fire door", 0..4, 40);
        assert_eq!(
            snippet,
            Snippet {
                before: String::new(),
                matched: "fire".into(),
                after: " door".into(),
                marks: Vec::new(),
            }
        );
    }

    #[test]
    fn a_snippet_of_several_matches_marks_each() {
        let text = "The fire  exit door\nis open";
        let fire = text.find("fire").unwrap();
        let door = text.find("door").unwrap();
        let snippet = snippet_of_ranges(text, &[fire..fire + 4, door..door + 4], 40);
        assert_eq!(snippet.before, "The ");
        assert_eq!(snippet.matched, "fire exit door");
        assert_eq!(snippet.after, " is open");
        let marked = snippet
            .marks
            .iter()
            .map(|range| &snippet.matched[range.clone()])
            .collect::<Vec<_>>();
        assert_eq!(marked, ["fire", "door"]);
    }

    #[test]
    fn a_snippet_keeps_multibyte_characters_whole() {
        let text = "ÄÖÜ äöü Σσ target ÄÖÜ äöü";
        let start = text.find("target").unwrap();
        let snippet = snippet_of(text, start..start + 6, 3);
        assert_eq!(snippet.before, "Σσ ");
        assert_eq!(snippet.after, " ÄÖ");
    }

    #[test]
    fn a_page_with_no_characters_is_empty() {
        assert!(page(Vec::new()).is_empty());
        assert!(page(vec![line("   ", 0, 0., 0.)]).is_empty());
        assert!(!page(vec![line("a", 0, 0., 0.)]).is_empty());
    }
}
