//! Editing existing page text in place (Revu's Edit Text).
//!
//! A page's top-level content is read into *lines*: consecutive text-showing
//! operators that share a baseline, colour, clip and orientation. A line may
//! mix fonts and sizes (runs). An edit replaces one line's text:
//!
//! - The line's first operator draws the whole edited line. Unchanged glyphs
//!   keep their original codes and exact positions (kerning, justified word
//!   gaps); inserted text uses the run's font, or a bundled fallback font for
//!   characters the page font lacks. Text after an insertion or deletion
//!   moves by the changed width.
//! - The line's other operators become pure text-matrix moves of their
//!   original width, and the first operator ends where it originally ended,
//!   so every following operator draws exactly where it did before.
//! - Only the edited operators' bytes change. Every other byte of every
//!   content stream is preserved.
//!
//! Edits are keyed by a line's anchor (its origin) and its original text, and
//! always apply to the unedited base document. Text inside Form XObjects,
//! Type3 fonts, vertical writing, CMaps other than Identity-H and invisible
//! (OCR) text are not editable.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fmt;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};

type Matrix = [f64; 6];

const IDENTITY: Matrix = [1., 0., 0., 1., 0., 0.];
const MAX_NESTING: usize = 64;
const MAX_REFERENCE_HOPS: usize = 32;
const DEFAULT_SPACE_WIDTH: f64 = 250.;
/// Gaps wider than this (in ems) between merged operators read as a space.
const SYNTHETIC_SPACE_GAP_EM: f64 = 0.15;
/// Operators further apart than this (in ems) start a new line.
const MAX_MERGE_GAP_EM: f64 = 1.0;
const MIN_MERGE_GAP_EM: f64 = -0.3;
/// The tallest a line's box reaches above and below its baseline.
const MAX_ASCENT_EM: f64 = 1.1;
const MAX_DESCENT_EM: f64 = 0.4;
/// After a list marker (a bullet or `1.`), Word and others tab to the
/// item's text.
const MAX_MARKER_GAP_EM: f64 = 5.0;
const BASELINE_TOLERANCE_EM: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextEditLimits {
    pub max_decoded_bytes_per_page: usize,
    pub max_operations_per_page: usize,
    pub max_lines_per_page: usize,
    pub max_chars_per_line: usize,
    pub max_cmap_entries: usize,
    /// Rewritten content streams are compressed. A document only shown
    /// while editing, never written, leaves them uncompressed: compressing
    /// a large page's content costs more than the rest of an edit.
    pub compress_streams: bool,
}

impl Default for TextEditLimits {
    fn default() -> Self {
        Self {
            max_decoded_bytes_per_page: 64 * 1024 * 1024,
            max_operations_per_page: 2_000_000,
            max_lines_per_page: 50_000,
            max_chars_per_line: 4_096,
            max_cmap_entries: 65_536,
            compress_streams: true,
        }
    }
}

impl TextEditLimits {
    /// The limits for a document only shown, never written.
    pub fn for_display() -> Self {
        Self {
            compress_streams: false,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextEditError {
    /// The page or one of its streams cannot be read.
    Malformed(String),
    LimitExceeded(String),
    /// The page does not exist.
    Page(String),
    /// The edited line is not in the base document as recorded.
    Stale(String),
    /// The line cannot be edited (unsupported font, Form XObject, ...).
    ReadOnly(String),
    /// Neither the line's font nor a bundled font can draw the character.
    Unencodable(char),
}

impl fmt::Display for TextEditError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(detail) => write!(formatter, "page text cannot be read: {detail}"),
            Self::LimitExceeded(detail) => write!(formatter, "page text exceeds a limit: {detail}"),
            Self::Page(detail) => write!(formatter, "page text is unavailable: {detail}"),
            Self::Stale(detail) => write!(formatter, "edited text no longer matches: {detail}"),
            Self::ReadOnly(detail) => write!(formatter, "text cannot be edited: {detail}"),
            Self::Unencodable(character) => write!(
                formatter,
                "no available font can draw {character:?} (U+{:04X})",
                u32::from(*character)
            ),
        }
    }
}

impl Error for TextEditError {}

impl From<lopdf::Error> for TextEditError {
    fn from(error: lopdf::Error) -> Self {
        Self::Malformed(error.to_string())
    }
}

/// Identifies a line within its page: its origin in page user space, in
/// hundredths of a point, and its order among lines with the same origin.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TextLineAnchor {
    pub x_centipoints: i64,
    pub y_centipoints: i64,
    pub ordinal: u32,
}

/// The text state a run of a line is drawn with.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextRunStyle {
    /// The font's name in the page's `/Resources /Font` dictionary.
    pub font: Vec<u8>,
    pub font_size: f64,
    pub char_spacing: f64,
    pub word_spacing: f64,
    /// A colour chosen for the run, as DeviceRGB components; `None` keeps
    /// the line's own fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<[f64; 3]>,
    /// A face chosen for the run; `None` draws it in `font`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<TextFace>,
}

/// A font chosen for text in place of its page font: the installed face
/// named `postscript_name`, or, when that is not installed (or for a
/// character it lacks), the bundled family in `fallback`'s style. `font`
/// still names the run's page font, which supplies its text state.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TextFace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postscript_name: Option<String>,
    pub fallback: FallbackFace,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum FallbackFamily {
    Sans,
    Serif,
    Mono,
}

/// The bundled face that draws characters a page font lacks.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct FallbackFace {
    pub family: FallbackFamily,
    pub bold: bool,
    pub italic: bool,
}

/// What the editor needs to lay out new text in one page font.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextFontMetrics {
    pub font: Vec<u8>,
    /// Glyph widths, in thousandths of an em, of the characters the font can
    /// draw for new text.
    pub advances: Vec<(char, f64)>,
    /// The character drawn with single-byte code 32, which word spacing
    /// widens.
    pub word_space: Option<char>,
    pub fallback: FallbackFace,
    /// The font's PostScript name without a subset tag: an installed font of
    /// that name draws the characters its program lacks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub postscript_name: Option<String>,
    /// In thousandths of an em.
    pub ascent: f64,
    pub descent: f64,
}

impl TextFontMetrics {
    fn advance(&self, character: char) -> Option<f64> {
        self.advances
            .binary_search_by(|(candidate, _)| candidate.cmp(&character))
            .ok()
            .map(|index| self.advances[index].1)
    }
}

/// One character of a line, in the line's text space.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextLineChar {
    pub character: char,
    pub x: f64,
    pub advance: f64,
    /// Index into the line's `styles`.
    pub style: usize,
    /// A gap between operators read as a space; it has no glyph.
    pub synthetic: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageTextLine {
    pub anchor: TextLineAnchor,
    pub text: String,
    pub chars: Vec<TextLineChar>,
    /// Where the line ends in its text space.
    pub end_x: f64,
    pub styles: Vec<TextRunStyle>,
    /// Maps the line's text space to page user space.
    pub matrix: [f64; 6],
    /// Horizontal scaling as a fraction (`Tz` / 100).
    pub horizontal_scale: f64,
    pub rise: f64,
    /// Extent above and below the baseline, in text space.
    pub ascent: f64,
    pub descent: f64,
    /// The line's fill as DeviceRGB components, when it is a device colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<[f64; 3]>,
    /// Why the line cannot be edited, if it cannot.
    pub read_only: Option<String>,
    /// The line is drawn by a Form XObject, not the page's own content.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub in_form: bool,
    /// The bounds of the clip the line is drawn in, in page space (left,
    /// bottom, right, top); `None` when it is not clipped.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_bounds: Option<[f64; 4]>,
    /// The marked-content identifiers the page's own content draws the
    /// line in, ascending: what ties it to the page's structure tree.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marked_content_ids: Vec<i64>,
    /// The paragraph the page's structure tree puts the line in: lines group
    /// into paragraphs by it alone on a tagged page.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<(u32, u16)>,
}

impl PageTextLine {
    /// The line's origin in page user space.
    pub fn origin(&self) -> (f64, f64) {
        apply(self.matrix, 0., 0.)
    }

    /// The four corners of the box from `x0` to `x1` (text space) between
    /// descent and ascent, in page user space.
    pub fn quad(&self, x0: f64, x1: f64) -> [(f64, f64); 4] {
        let bottom = self.rise + self.descent;
        let top = self.rise + self.ascent;
        [
            apply(self.matrix, x0, bottom),
            apply(self.matrix, x1, bottom),
            apply(self.matrix, x1, top),
            apply(self.matrix, x0, top),
        ]
    }

    /// The point in text space for a page user-space point.
    pub fn text_space_point(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        invert(self.matrix).map(|inverse| apply(inverse, x, y))
    }

    pub fn original_runs(&self) -> Vec<TextEditRun> {
        let mut runs: Vec<TextEditRun> = Vec::new();
        for character in &self.chars {
            let style = &self.styles[character.style];
            match runs.last_mut() {
                Some(run) if &run.style == style => run.text.push(character.character),
                _ => runs.push(TextEditRun {
                    style: style.clone(),
                    text: character.character.to_string(),
                }),
            }
        }
        runs
    }
}

/// The editable text of one page.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct PageTextLayer {
    pub page_index: u32,
    pub fonts: Vec<TextFontMetrics>,
    pub lines: Vec<PageTextLine>,
    /// The page's visible area (its crop box): left, bottom, right, top.
    #[serde(default)]
    pub page_box: Option<[f64; 4]>,
    /// The tables the page's structure tree declares: the only evidence
    /// its tables are found by.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tagged_tables: Vec<crate::pdf_structure::TaggedTable>,
    /// The axis-aligned rules and filled rectangles the page's own content
    /// draws, in page space: where a declared table's borders lie.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rules: Vec<crate::pdf_tables::PageRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fills: Vec<crate::pdf_tables::PageFill>,
    /// The drawings and images the page's own content draws, which Edit PDF
    /// mode can move.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<crate::pdf_content_moves::PageObject>,
    /// The file's permissions forbid changing its content (an owner
    /// restriction, which opens without a password, as banks set on
    /// statements). Its content is editable once the user agrees to edit it
    /// anyway.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub content_restricted: bool,
}

impl PageTextLayer {
    pub fn line(&self, anchor: &TextLineAnchor) -> Option<&PageTextLine> {
        self.lines.iter().find(|line| &line.anchor == anchor)
    }

    pub fn font(&self, name: &[u8]) -> Option<&TextFontMetrics> {
        self.fonts.iter().find(|font| font.font == name)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextEditRun {
    pub style: TextRunStyle,
    pub text: String,
}

/// How a line rewrapped in its paragraph is placed: moved along the line
/// (centred or right-aligned text) and, for justified text, stretched so
/// its last visible character ends at `justify_end`. A list item's text
/// after its marker starts at `tab`: the character index and where it
/// starts, at least. Text space units, before the move.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct LineFit {
    #[serde(default)]
    pub shift: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub justify_end: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab: Option<(usize, f64)>,
}

impl LineFit {
    pub fn is_plain(&self) -> bool {
        self.shift == 0. && self.justify_end.is_none() && self.tab.is_none()
    }
}

/// A line a paragraph grew by, drawn `below` the edited (last) line's
/// baseline in its text space. `anchor` names it once saved.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ExtraLine {
    pub anchor: TextLineAnchor,
    pub below: f64,
    /// How far along the line its operator starts from the paragraph's, in
    /// text space; its glyphs do not move. A hundredth of a point keeps its
    /// anchor from coinciding with a line already there.
    #[serde(default)]
    pub origin_shift: f64,
    #[serde(default, skip_serializing_if = "LineFit::is_plain")]
    pub fit: LineFit,
    pub runs: Vec<TextEditRun>,
    /// A hard line break ends it, as one ends a list item.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hard_break: bool,
}

/// Paragraph settings chosen while editing in place of those the page
/// shows. They are kept with the paragraph's first line so it reopens with
/// them; the lines' fits and positions already draw them.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ParagraphFormat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<crate::pdf_text_blocks::TextBlockAlignment>,
    /// Baseline to baseline, as a multiple of the paragraph's font size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<f64>,
}

impl ParagraphFormat {
    pub fn is_default(&self) -> bool {
        self.alignment.is_none() && self.line_spacing.is_none()
    }
}

/// Placement of a line in a rewrapped paragraph.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct TextEditLayout {
    #[serde(default, skip_serializing_if = "LineFit::is_plain")]
    pub fit: LineFit,
    /// Lines added below this one, the paragraph's last.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_lines: Vec<ExtraLine>,
    #[serde(default, skip_serializing_if = "ParagraphFormat::is_default")]
    pub paragraph: ParagraphFormat,
    /// A hard line break ends the line's text, as one ends a list item, so
    /// the paragraph reopens with it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hard_break: bool,
}

impl TextEditLayout {
    pub fn is_plain(&self) -> bool {
        self.fit.is_plain()
            && self.extra_lines.is_empty()
            && self.paragraph.is_default()
            && !self.hard_break
    }
}

/// Replaces the text of one line.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageTextEdit {
    pub page_index: u32,
    pub anchor: TextLineAnchor,
    /// The line in the base document. Only its text must still match;
    /// its runs restore the line when an edit is undone after saving.
    pub original: Vec<TextEditRun>,
    pub runs: Vec<TextEditRun>,
    /// Placement in a rewrapped paragraph; plain for a line edited alone.
    #[serde(default, skip_serializing_if = "TextEditLayout::is_plain")]
    pub layout: TextEditLayout,
}

impl PageTextEdit {
    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    pub fn original_text(&self) -> String {
        self.original.iter().map(|run| run.text.as_str()).collect()
    }

    /// Whether the edit leaves the line as it was.
    pub fn is_unchanged(&self) -> bool {
        self.draws_as_original() && self.layout.paragraph.is_default()
    }

    /// Whether the edit draws the line as it was (it may still record
    /// paragraph settings).
    pub fn draws_as_original(&self) -> bool {
        normalized_runs(&self.runs) == normalized_runs(&self.original)
            && self.layout.fit.is_plain()
            && self.layout.extra_lines.is_empty()
    }
}

/// Runs with empty runs dropped and neighbours of one style joined.
pub fn normalized_runs(runs: &[TextEditRun]) -> Vec<TextEditRun> {
    let mut normalized: Vec<TextEditRun> = Vec::new();
    for run in runs.iter().filter(|run| !run.text.is_empty()) {
        match normalized.last_mut() {
            Some(last) if last.style == run.style => last.text.push_str(&run.text),
            _ => normalized.push(run.clone()),
        }
    }
    normalized
}

/// Rewrites a history state's edits once `saved` edits are in the file, so
/// they apply to the saved file as their new base. A line `saved` changed
/// now reads as its saved runs; a state that differs from that becomes an
/// edit from it, and a state that matches it needs no edit.
pub fn rebase_text_edits(state: &[PageTextEdit], saved: &[PageTextEdit]) -> Vec<PageTextEdit> {
    let key = |edit: &PageTextEdit| (edit.page_index, edit.anchor);
    let mut rebased = state
        .iter()
        .filter(|edit| !saved.iter().any(|done| key(done) == key(edit)))
        .cloned()
        .collect::<Vec<_>>();
    for done in saved {
        let state_edit = state.iter().find(|edit| key(edit) == key(done));
        let desired_runs = state_edit.map_or(&done.original, |edit| &edit.runs);
        let desired = state_edit
            .map(|edit| edit.layout.clone())
            .unwrap_or_default();
        // The saved line already sits at its fit: a shift is relative to it.
        let fit = LineFit {
            shift: desired.fit.shift - done.layout.fit.shift,
            justify_end: desired.fit.justify_end,
            tab: desired.fit.tab,
        };
        let fit = if fit.shift.abs() < 1e-9
            && fit.justify_end == done.layout.fit.justify_end
            && fit.tab == done.layout.fit.tab
        {
            LineFit::default()
        } else {
            fit
        };
        let saved_extras = &done.layout.extra_lines;
        let mut edit = PageTextEdit {
            page_index: done.page_index,
            anchor: done.anchor,
            original: normalized_runs(&done.runs),
            runs: normalized_runs(desired_runs),
            layout: TextEditLayout {
                fit,
                extra_lines: Vec::new(),
                ..Default::default()
            },
        };
        // Lines a save added are lines of the file now: each takes the
        // desired state's text for it, or none.
        let mut extra_edits = Vec::new();
        for (index, extra) in saved_extras.iter().enumerate() {
            let wanted = desired.extra_lines.get(index);
            extra_edits.push(PageTextEdit {
                page_index: done.page_index,
                anchor: extra.anchor,
                original: normalized_runs(&extra.runs),
                runs: wanted.map_or_else(Vec::new, |wanted| normalized_runs(&wanted.runs)),
                layout: TextEditLayout {
                    fit: wanted.map_or_else(LineFit::default, |wanted| LineFit {
                        shift: wanted.fit.shift - extra.fit.shift,
                        justify_end: (wanted.fit.justify_end != extra.fit.justify_end)
                            .then_some(wanted.fit.justify_end)
                            .flatten(),
                        tab: (wanted.fit.tab != extra.fit.tab)
                            .then_some(wanted.fit.tab)
                            .flatten(),
                    }),
                    extra_lines: Vec::new(),
                    hard_break: wanted.is_some_and(|wanted| wanted.hard_break),
                    ..Default::default()
                },
            });
        }
        // Lines beyond those continue below the last line in the file.
        let further = desired.extra_lines.iter().skip(saved_extras.len());
        match (extra_edits.last_mut(), saved_extras.last()) {
            (Some(last_edit), Some(last_saved)) => {
                last_edit
                    .layout
                    .extra_lines
                    .extend(further.map(|extra| ExtraLine {
                        below: extra.below - last_saved.below,
                        ..extra.clone()
                    }));
            }
            _ => edit.layout.extra_lines.extend(further.cloned()),
        }
        for edit in std::iter::once(edit).chain(extra_edits) {
            if !edit.is_unchanged() {
                rebased.push(edit);
            }
        }
    }
    rebased.sort_by_key(|edit| (edit.page_index, edit.anchor));
    rebased
}

/// How a laid-out character is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LaidOutSource {
    /// The original line's character at this index.
    Original(usize),
    /// New text drawn with the run's font.
    Font,
    /// New text drawn with a bundled fallback font.
    Fallback,
    /// A space the font cannot draw: a gap with no glyph.
    Gap,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LaidOutChar {
    pub character: char,
    pub x: f64,
    pub advance: f64,
    /// Index into the edit's runs.
    pub run: usize,
    pub source: LaidOutSource,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextLineLayout {
    pub chars: Vec<LaidOutChar>,
    pub end_x: f64,
}

impl TextLineLayout {
    /// The caret position before character `index` (or the end).
    pub fn caret_x(&self, index: usize) -> f64 {
        match self.chars.get(index) {
            Some(character) => character.x,
            None => self
                .chars
                .last()
                .map_or(0., |character| character.x + character.advance),
        }
    }

    /// The caret index nearest to text-space `x`.
    pub fn index_at(&self, x: f64) -> usize {
        for (index, character) in self.chars.iter().enumerate() {
            if x < character.x + character.advance / 2. {
                return index;
            }
        }
        self.chars.len()
    }
}

/// Lays out an edit of `line`. The editor and the writer share this, so the
/// caret matches what is saved.
pub fn layout_text_edit(
    layer: &PageTextLayer,
    line: &PageTextLine,
    runs: &[TextEditRun],
) -> Result<TextLineLayout, TextEditError> {
    let original = line
        .chars
        .iter()
        .map(|character| (character.character, &line.styles[character.style]))
        .collect::<Vec<_>>();
    let edited = runs
        .iter()
        .enumerate()
        .flat_map(|(run_index, run)| {
            run.text
                .chars()
                .map(move |character| (character, run_index, &run.style))
        })
        .collect::<Vec<_>>();
    let matches = align(
        &original,
        &edited
            .iter()
            .map(|(character, _, style)| (*character, *style))
            .collect::<Vec<_>>(),
    );
    // Each character is drawn after the one before it, separated by the gap
    // it had before it in the original. Gaps on either side of whitespace
    // belong to the whitespace (justification widens both sides of a space)
    // and go with it when it is deleted. Inserted text takes the gap before
    // the character it is typed in front of, or, before whitespace, joins
    // the text before it.
    let is_space = |index: usize| line.chars[index].character.is_whitespace();
    let end_of = |index: usize| line.chars[index].x + line.chars[index].advance;
    let gap_before = |index: usize| match index.checked_sub(1) {
        Some(previous) => line.chars[index].x - end_of(previous),
        None => line.chars[index].x,
    };
    let mut chars = Vec::with_capacity(edited.len());
    let mut end = 0.;
    let mut next_original = 0;
    // While inserting: the original whose leading gap the insertion took.
    let mut inserting: Option<Option<usize>> = None;
    for (edited_index, (character, run_index, style)) in edited.iter().enumerate() {
        if let Some(original_index) = matches[edited_index] {
            let deleted_space_before =
                original_index > next_original && is_space(original_index - 1);
            let gap = if inserting == Some(Some(original_index)) || deleted_space_before {
                0.
            } else {
                gap_before(original_index)
            };
            let source = &line.chars[original_index];
            chars.push(LaidOutChar {
                character: *character,
                x: end + gap,
                advance: source.advance,
                run: *run_index,
                source: LaidOutSource::Original(original_index),
            });
            end += gap + source.advance;
            next_original = original_index + 1;
            inserting = None;
            continue;
        }
        let gap = match inserting {
            Some(_) => 0.,
            None => {
                let taken = (next_original < line.chars.len()
                    && (next_original == 0 || !is_space(next_original)))
                .then_some(next_original);
                inserting = Some(taken);
                taken.map_or(0., gap_before)
            }
        };
        let (advance, source) = inserted_advance(layer, line, style, *character)?;
        chars.push(LaidOutChar {
            character: *character,
            x: end + gap,
            advance,
            run: *run_index,
            source,
        });
        end += gap + advance;
    }
    let trailing = match line.chars.len().checked_sub(1) {
        Some(last) if next_original == line.chars.len() => line.end_x - end_of(last),
        Some(_) => 0.,
        None => line.end_x,
    };
    Ok(TextLineLayout {
        chars,
        end_x: end + trailing,
    })
}

/// [`layout_text_edit`], placed by `fit`: moved, and stretched across its
/// spaces to end at `fit.justify_end`.
pub fn layout_text_edit_fitted(
    layer: &PageTextLayer,
    line: &PageTextLine,
    runs: &[TextEditRun],
    fit: LineFit,
) -> Result<TextLineLayout, TextEditError> {
    let mut layout = layout_text_edit(layer, line, runs)?;
    apply_fit(&mut layout, fit);
    Ok(layout)
}

/// The layout of an extra line: all its text new, in `line`'s fonts, on a
/// baseline `below` it.
/// An added line with no text, drawn as a space, as word processors draw
/// an empty paragraph: so the page shows it is there and keeps its room,
/// and pagination moves it as it moves text.
fn drawn_blank(extra: &ExtraLine, edit: &PageTextEdit, line: &PageTextLine) -> ExtraLine {
    if extra.runs.iter().any(|run| !run.text.is_empty()) {
        return extra.clone();
    }
    let style = extra
        .runs
        .first()
        .or_else(|| edit.runs.first())
        .map(|run| run.style.clone())
        .or_else(|| line.styles.first().cloned());
    let Some(style) = style else {
        return extra.clone();
    };
    ExtraLine {
        runs: vec![TextEditRun {
            style,
            text: " ".into(),
        }],
        ..extra.clone()
    }
}

pub fn layout_extra_line(
    layer: &PageTextLayer,
    line: &PageTextLine,
    extra: &ExtraLine,
) -> Result<TextLineLayout, TextEditError> {
    layout_text_edit_fitted(
        layer,
        &extra_line_base(line, extra.below),
        &extra.runs,
        extra.fit,
    )
}

/// The text-space y offset of a baseline `below` `line`'s: away from its
/// glyphs' tops, which a negative font size turns upside down.
fn text_y_below(line: &PageTextLine, below: f64) -> f64 {
    let flipped = line
        .styles
        .first()
        .is_some_and(|style| style.font_size < 0.);
    if flipped { below } else { -below }
}

/// An empty line `below` `line`, in its text space and state, to lay out new
/// text on.
pub fn extra_line_base(line: &PageTextLine, below: f64) -> PageTextLine {
    let mut base = line.clone();
    base.text.clear();
    base.chars.clear();
    base.end_x = 0.;
    base.matrix = multiply([1., 0., 0., 1., 0., text_y_below(line, below)], line.matrix);
    base.read_only = None;
    base
}

fn apply_fit(layout: &mut TextLineLayout, fit: LineFit) {
    if let Some((index, target)) = fit.tab
        && let Some(start) = layout.chars.get(index).map(|character| character.x)
        && target > start
    {
        for character in &mut layout.chars[index..] {
            character.x += target - start;
        }
        if let Some(gap) = index.checked_sub(1) {
            layout.chars[gap].advance += target - start;
        }
        layout.end_x += target - start;
    }
    if let Some(target) = fit.justify_end {
        let first = layout
            .chars
            .iter()
            .position(|character| !character.character.is_whitespace());
        let last = layout
            .chars
            .iter()
            .rposition(|character| !character.character.is_whitespace());
        if let (Some(first), Some(last)) = (first, last) {
            let end = layout.chars[last].x + layout.chars[last].advance;
            let gaps = (first..last)
                .filter(|index| layout.chars[*index].character.is_whitespace())
                .count();
            if gaps > 0 && target > end {
                let extra = (target - end) / gaps as f64;
                let mut shift = 0.;
                for index in first..layout.chars.len() {
                    layout.chars[index].x += shift;
                    if index < last && layout.chars[index].character.is_whitespace() {
                        layout.chars[index].advance += extra;
                        shift += extra;
                    }
                }
                layout.end_x += shift;
            }
        }
    }
    if fit.shift != 0. {
        for character in &mut layout.chars {
            character.x += fit.shift;
        }
        layout.end_x += fit.shift;
    }
}

fn inserted_advance(
    layer: &PageTextLayer,
    line: &PageTextLine,
    style: &TextRunStyle,
    character: char,
) -> Result<(f64, LaidOutSource), TextEditError> {
    let font = layer.font(&style.font).ok_or_else(|| {
        TextEditError::ReadOnly(format!(
            "font {} is not on the page",
            String::from_utf8_lossy(&style.font)
        ))
    })?;
    let scale = line.horizontal_scale;
    if style.face.is_none() {
        if let Some(width) = font.advance(character) {
            let word_spacing = if font.word_space == Some(character) {
                style.word_spacing
            } else {
                0.
            };
            return Ok((
                (width / 1000. * style.font_size + style.char_spacing + word_spacing) * scale,
                LaidOutSource::Font,
            ));
        }
        if character == ' ' {
            let width = font.advance(' ').unwrap_or(DEFAULT_SPACE_WIDTH);
            return Ok((
                (width / 1000. * style.font_size + style.char_spacing) * scale,
                LaidOutSource::Gap,
            ));
        }
    }
    let width = substitute_glyph(font, style, character)
        .map(|(_, _, width)| width)
        .ok_or(TextEditError::Unencodable(character))?;
    Ok((
        (width / 1000. * style.font_size + style.char_spacing) * scale,
        LaidOutSource::Fallback,
    ))
}

/// The font drawing `character` in `style` when it is not the one the run
/// asks for (its page font, or its chosen face): `None` when that draws it.
pub fn substitute_font(
    layer: &PageTextLayer,
    style: &TextRunStyle,
    character: char,
) -> Option<String> {
    let metrics = layer.font(&style.font)?;
    if character.is_whitespace() {
        return None;
    }
    if style.face.is_none() && metrics.advance(character).is_some() {
        return None;
    }
    let (font, _, _) = substitute_glyph(metrics, style, character)?;
    if let Some(face) = &style.face
        && crate::pdf_engine::chosen_face_font(face) == Some(font)
    {
        return None;
    }
    Some(crate::pdf_engine::text_edit_font_name(font))
}

/// The font, glyph and advance that draw `character` in `style` where its
/// page font does not: the run's chosen face, or the page font's fallback.
fn substitute_glyph(
    metrics: &TextFontMetrics,
    style: &TextRunStyle,
    character: char,
) -> Option<(crate::pdf_engine::TextEditFallbackFont, u16, f64)> {
    match &style.face {
        Some(face) => crate::pdf_engine::chosen_face_glyph(face, character),
        None => crate::pdf_engine::text_edit_fallback_glyph(
            metrics.fallback,
            metrics.postscript_name.as_deref(),
            character,
        ),
    }
}

fn align(
    original: &[(char, &TextRunStyle)],
    edited: &[(char, &TextRunStyle)],
) -> Vec<Option<usize>> {
    let same = |left: &(char, &TextRunStyle), right: &(char, &TextRunStyle)| {
        left.0 == right.0 && left.1 == right.1
    };
    let mut result = vec![None; edited.len()];
    let mut prefix = 0;
    while prefix < original.len()
        && prefix < edited.len()
        && same(&original[prefix], &edited[prefix])
    {
        result[prefix] = Some(prefix);
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < original.len() - prefix
        && suffix < edited.len() - prefix
        && same(
            &original[original.len() - 1 - suffix],
            &edited[edited.len() - 1 - suffix],
        )
    {
        result[edited.len() - 1 - suffix] = Some(original.len() - 1 - suffix);
        suffix += 1;
    }
    let original_middle = &original[prefix..original.len() - suffix];
    let edited_middle = &edited[prefix..edited.len() - suffix];
    let (rows, columns) = (original_middle.len(), edited_middle.len());
    if rows == 0 || columns == 0 || rows.saturating_mul(columns) > 4_000_000 {
        return result;
    }
    let mut lengths = vec![0u32; (rows + 1) * (columns + 1)];
    let at = |row: usize, column: usize| row * (columns + 1) + column;
    for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            lengths[at(row, column)] = if same(&original_middle[row], &edited_middle[column]) {
                lengths[at(row + 1, column + 1)] + 1
            } else {
                lengths[at(row + 1, column)].max(lengths[at(row, column + 1)])
            };
        }
    }
    let (mut row, mut column) = (0, 0);
    while row < rows && column < columns {
        if same(&original_middle[row], &edited_middle[column]) {
            result[prefix + column] = Some(prefix + row);
            row += 1;
            column += 1;
        } else if lengths[at(row + 1, column)] >= lengths[at(row, column + 1)] {
            row += 1;
        } else {
            column += 1;
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Matrices

fn multiply(left: Matrix, right: Matrix) -> Matrix {
    [
        left[0] * right[0] + left[1] * right[2],
        left[0] * right[1] + left[1] * right[3],
        left[2] * right[0] + left[3] * right[2],
        left[2] * right[1] + left[3] * right[3],
        left[4] * right[0] + left[5] * right[2] + right[4],
        left[4] * right[1] + left[5] * right[3] + right[5],
    ]
}

fn apply(matrix: Matrix, x: f64, y: f64) -> (f64, f64) {
    (
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    )
}

fn invert(matrix: Matrix) -> Option<Matrix> {
    let determinant = matrix[0] * matrix[3] - matrix[1] * matrix[2];
    if !determinant.is_finite() || determinant.abs() < 1e-12 {
        return None;
    }
    let a = matrix[3] / determinant;
    let b = -matrix[1] / determinant;
    let c = -matrix[2] / determinant;
    let d = matrix[0] / determinant;
    Some([
        a,
        b,
        c,
        d,
        -(matrix[4] * a + matrix[5] * c),
        -(matrix[4] * b + matrix[5] * d),
    ])
}

fn same_linear(left: Matrix, right: Matrix) -> bool {
    let scale = left[..4]
        .iter()
        .chain(&right[..4])
        .fold(0f64, |largest, value| largest.max(value.abs()));
    scale > 0.
        && left[..4]
            .iter()
            .zip(&right[..4])
            .all(|(left, right)| (left - right).abs() <= scale * 1e-4)
}

// ---------------------------------------------------------------------------
// Content stream lexing

#[derive(Clone, Debug, PartialEq)]
enum Operand {
    Number(f64),
    Name(Vec<u8>),
    Str(Vec<u8>),
    Array(Vec<Operand>),
    Dict(Vec<(Vec<u8>, Operand)>),
    Other,
}

/// A content operator's name, held inline: operators are a few bytes.
#[derive(Clone, Copy, Default, Eq, Hash, PartialEq)]
struct Operator {
    bytes: [u8; 15],
    len: u8,
}

impl Operator {
    /// `word`, or an unknown operator when longer than any PDF operator.
    fn new(word: &[u8]) -> Self {
        let mut bytes = [0; 15];
        let word = if word.len() <= bytes.len() {
            word
        } else {
            b"?"
        };
        bytes[..word.len()].copy_from_slice(word);
        Self {
            bytes,
            len: word.len() as u8,
        }
    }

    fn as_slice(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

impl std::ops::Deref for Operator {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl<const N: usize> PartialEq<&[u8; N]> for Operator {
    fn eq(&self, other: &&[u8; N]) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl fmt::Debug for Operator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", String::from_utf8_lossy(self.as_slice()))
    }
}

#[derive(Clone, Debug, Default)]
struct ContentOp {
    operator: Operator,
    operands: Vec<Operand>,
    /// Byte range of operands and operator in `stream`.
    stream: usize,
    start: usize,
    end: usize,
    /// The operands began in an earlier stream than the operator.
    spans_streams: bool,
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b'\0' | b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(byte: u8) -> bool {
    !is_whitespace(byte) && !is_delimiter(byte)
}

enum Token {
    Operand(Operand),
    Operator(Operator),
    ArrayEnd,
    DictEnd,
}

struct Lexer<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Lexer<'a> {
    fn skip_whitespace_and_comments(&mut self) {
        while let Some(&byte) = self.bytes.get(self.position) {
            if is_whitespace(byte) {
                self.position += 1;
            } else if byte == b'%' {
                while let Some(&byte) = self.bytes.get(self.position) {
                    if byte == b'\r' || byte == b'\n' {
                        break;
                    }
                    self.position += 1;
                }
            } else {
                break;
            }
        }
    }

    /// The next token and where it starts.
    fn next(&mut self, depth: usize) -> Result<Option<(Token, usize)>, TextEditError> {
        if depth > MAX_NESTING {
            return Err(TextEditError::LimitExceeded(
                "content operands nest too deeply".into(),
            ));
        }
        self.skip_whitespace_and_comments();
        let start = self.position;
        let Some(&byte) = self.bytes.get(self.position) else {
            return Ok(None);
        };
        let token = match byte {
            b'(' => Token::Operand(Operand::Str(self.literal_string()?)),
            b'<' if self.bytes.get(self.position + 1) == Some(&b'<') => {
                self.position += 2;
                let mut entries = Vec::new();
                let mut key: Option<Vec<u8>> = None;
                loop {
                    match self.next(depth + 1)? {
                        Some((Token::DictEnd, _)) => break,
                        Some((Token::Operand(Operand::Name(name)), _)) if key.is_none() => {
                            key = Some(name);
                        }
                        Some((Token::Operand(value), _)) => {
                            if let Some(key) = key.take() {
                                entries.push((key, value));
                            }
                        }
                        Some((Token::Operator(_), _)) => {
                            if let Some(key) = key.take() {
                                entries.push((key, Operand::Other));
                            }
                        }
                        Some((Token::ArrayEnd, _)) => {}
                        None => {
                            return Err(TextEditError::Malformed("unterminated dictionary".into()));
                        }
                    }
                }
                Token::Operand(Operand::Dict(entries))
            }
            b'<' => Token::Operand(Operand::Str(self.hex_string()?)),
            b'>' if self.bytes.get(self.position + 1) == Some(&b'>') => {
                self.position += 2;
                Token::DictEnd
            }
            b'[' => {
                self.position += 1;
                let mut items = Vec::new();
                loop {
                    match self.next(depth + 1)? {
                        Some((Token::ArrayEnd, _)) => break,
                        Some((Token::Operand(operand), _)) => items.push(operand),
                        Some((Token::Operator(operator), _)) => {
                            items.push(match operator.as_slice() {
                                b"true" | b"false" | b"null" => Operand::Other,
                                _ => {
                                    return Err(TextEditError::Malformed(
                                        "operator inside an array".into(),
                                    ));
                                }
                            })
                        }
                        Some((Token::DictEnd, _)) | None => {
                            return Err(TextEditError::Malformed("unterminated array".into()));
                        }
                    }
                }
                Token::Operand(Operand::Array(items))
            }
            b']' => {
                self.position += 1;
                Token::ArrayEnd
            }
            b'/' => {
                self.position += 1;
                let mut name = Vec::new();
                while let Some(&byte) = self.bytes.get(self.position) {
                    if !is_regular(byte) {
                        break;
                    }
                    self.position += 1;
                    if byte == b'#'
                        && let Some(value) = self
                            .bytes
                            .get(self.position..self.position + 2)
                            .and_then(|hex| std::str::from_utf8(hex).ok())
                            .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                    {
                        name.push(value);
                        self.position += 2;
                    } else {
                        name.push(byte);
                    }
                }
                Token::Operand(Operand::Name(name))
            }
            b')' | b'>' | b'{' | b'}' => {
                // Stray delimiters are skipped, as viewers do.
                self.position += 1;
                return self.next(depth);
            }
            _ => {
                while let Some(&byte) = self.bytes.get(self.position) {
                    if !is_regular(byte) {
                        break;
                    }
                    self.position += 1;
                }
                let word = &self.bytes[start..self.position];
                match parse_number(word) {
                    Some(number) => Token::Operand(Operand::Number(number)),
                    None => Token::Operator(Operator::new(word)),
                }
            }
        };
        Ok(Some((token, start)))
    }

    fn literal_string(&mut self) -> Result<Vec<u8>, TextEditError> {
        self.position += 1;
        let mut value = Vec::new();
        let mut depth = 1usize;
        loop {
            let Some(&byte) = self.bytes.get(self.position) else {
                return Err(TextEditError::Malformed("unterminated string".into()));
            };
            self.position += 1;
            match byte {
                b'\\' => {
                    let Some(&escaped) = self.bytes.get(self.position) else {
                        return Err(TextEditError::Malformed("unterminated string".into()));
                    };
                    self.position += 1;
                    match escaped {
                        b'n' => value.push(b'\n'),
                        b'r' => value.push(b'\r'),
                        b't' => value.push(b'\t'),
                        b'b' => value.push(0x08),
                        b'f' => value.push(0x0c),
                        b'0'..=b'7' => {
                            let mut code = u32::from(escaped - b'0');
                            for _ in 0..2 {
                                match self.bytes.get(self.position) {
                                    Some(&digit @ b'0'..=b'7') => {
                                        code = code * 8 + u32::from(digit - b'0');
                                        self.position += 1;
                                    }
                                    _ => break,
                                }
                            }
                            value.push((code & 0xff) as u8);
                        }
                        b'\r' => {
                            if self.bytes.get(self.position) == Some(&b'\n') {
                                self.position += 1;
                            }
                        }
                        b'\n' => {}
                        other => value.push(other),
                    }
                }
                b'(' => {
                    depth += 1;
                    value.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(value);
                    }
                    value.push(byte);
                }
                _ => value.push(byte),
            }
        }
    }

    fn hex_string(&mut self) -> Result<Vec<u8>, TextEditError> {
        self.position += 1;
        let mut digits = Vec::new();
        loop {
            let Some(&byte) = self.bytes.get(self.position) else {
                return Err(TextEditError::Malformed("unterminated hex string".into()));
            };
            self.position += 1;
            match byte {
                b'>' => break,
                byte if is_whitespace(byte) => {}
                byte => digits.push(
                    (byte as char)
                        .to_digit(16)
                        .ok_or_else(|| TextEditError::Malformed("invalid hex string".into()))?
                        as u8,
                ),
            }
        }
        if digits.len() % 2 == 1 {
            digits.push(0);
        }
        Ok(digits
            .chunks(2)
            .map(|pair| (pair[0] << 4) | pair[1])
            .collect())
    }

    /// Skips inline image data after `ID`, returning where `EI` ends.
    fn skip_inline_image(&mut self) -> Result<usize, TextEditError> {
        // One whitespace byte separates ID from the data.
        self.position += 1;
        let bytes = self.bytes;
        let mut index = self.position;
        while index + 1 < bytes.len() {
            if bytes[index] == b'E'
                && bytes[index + 1] == b'I'
                && index > 0
                && is_whitespace(bytes[index - 1])
                && bytes
                    .get(index + 2)
                    .is_none_or(|&next| is_whitespace(next) || is_delimiter(next))
            {
                self.position = index + 2;
                return Ok(self.position);
            }
            index += 1;
        }
        Err(TextEditError::Malformed("unterminated inline image".into()))
    }
}

fn parse_number(word: &[u8]) -> Option<f64> {
    let first = *word.first()?;
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return None;
    }
    // Most numbers are short decimals: read them directly.
    if word.len() <= 15 {
        let (negative, digits) = match first {
            b'-' => (true, &word[1..]),
            b'+' => (false, &word[1..]),
            _ => (false, word),
        };
        let mut whole = 0u64;
        let mut fraction_digits = None;
        let mut simple = !digits.is_empty();
        for &byte in digits {
            match byte {
                b'0'..=b'9' => {
                    whole = whole * 10 + u64::from(byte - b'0');
                    if let Some(count) = fraction_digits.as_mut() {
                        *count += 1;
                    }
                }
                b'.' if fraction_digits.is_none() => fraction_digits = Some(0),
                _ => {
                    simple = false;
                    break;
                }
            }
        }
        if simple {
            const SCALE: [f64; 16] = [
                1., 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15,
            ];
            let value = whole as f64 / SCALE[fraction_digits.unwrap_or(0)];
            return Some(if negative { -value } else { value });
        }
    }
    let text = std::str::from_utf8(word).ok()?;
    if !(first.is_ascii_digit() || matches!(first, b'+' | b'-' | b'.')) {
        return None;
    }
    if !word
        .iter()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.'))
    {
        return None;
    }
    // PDF numbers have no exponent; tolerate doubled signs as viewers do.
    let trimmed = text.trim_start_matches(['+', '-']);
    let negative = text[..text.len() - trimmed.len()].contains('-');
    let value = if trimmed.is_empty() || trimmed == "." {
        0.
    } else {
        trimmed.parse::<f64>().ok()?
    };
    Some(if negative { -value } else { value })
}

fn parse_operations(
    streams: &[Vec<u8>],
    limits: &TextEditLimits,
) -> Result<Vec<ContentOp>, TextEditError> {
    // A page's content streams are one sequence (as if joined with white
    // space), and a producer may split them between any two tokens, even
    // inside a dictionary or an array (Adobe PDF Library splits content into
    // pieces of about 3.6 KB). They are lexed as one, with each position
    // mapped back to its own stream.
    let mut joined = Vec::with_capacity(streams.iter().map(|bytes| bytes.len() + 1).sum());
    let mut starts = Vec::with_capacity(streams.len());
    for bytes in streams {
        starts.push(joined.len());
        joined.extend_from_slice(bytes);
        joined.push(b'\n');
    }
    // The stream holding joined offset `offset`, and the offset within it.
    let locate = |offset: usize| {
        let stream = starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1);
        (stream, offset - starts.get(stream).copied().unwrap_or(0))
    };
    let mut operations = Vec::new();
    let mut operands = Vec::new();
    let mut operand_start: Option<(usize, usize)> = None;
    let mut lexer = Lexer {
        bytes: &joined,
        position: 0,
    };
    while let Some((token, start)) = lexer.next(0)? {
        let (stream_index, start) = locate(start);
        match token {
            Token::Operand(operand) => {
                operand_start.get_or_insert((stream_index, start));
                operands.push(operand);
            }
            Token::ArrayEnd | Token::DictEnd => {}
            Token::Operator(operator) => {
                if matches!(operator.as_slice(), b"true" | b"false" | b"null") {
                    operand_start.get_or_insert((stream_index, start));
                    operands.push(Operand::Other);
                    continue;
                }
                let mut end = lexer.position;
                if operator == b"BI" {
                    // Key/value pairs up to ID, then binary data up to EI.
                    loop {
                        match lexer.next(0)? {
                            Some((Token::Operator(word), _)) if word == b"ID" => break,
                            Some(_) => {}
                            None => {
                                return Err(TextEditError::Malformed(
                                    "unterminated inline image".into(),
                                ));
                            }
                        }
                    }
                    end = lexer.skip_inline_image()?;
                }
                let (end_stream, end) = locate(end.saturating_sub(1));
                let end = end + 1;
                let (first_stream, first_start) =
                    operand_start.take().unwrap_or((stream_index, start));
                operations.push(ContentOp {
                    operator,
                    operands: std::mem::take(&mut operands),
                    stream: stream_index,
                    start: if first_stream == stream_index {
                        first_start
                    } else {
                        start
                    },
                    end: if end_stream == stream_index {
                        end
                    } else {
                        streams[stream_index].len()
                    },
                    spans_streams: first_stream != stream_index || end_stream != stream_index,
                });
                if operations.len() > limits.max_operations_per_page {
                    return Err(TextEditError::LimitExceeded(format!(
                        "page content has more than {} operations",
                        limits.max_operations_per_page
                    )));
                }
            }
        }
    }
    Ok(operations)
}

// ---------------------------------------------------------------------------
// Fonts

#[derive(Clone, Debug)]
struct LoadedFont {
    /// The font's name in the resources of the stream that uses it; its key
    /// in a page's fonts adds the path of Form XObjects ("Fm0/F1").
    resource_name: Vec<u8>,
    /// Bytes per character code.
    code_length: usize,
    decode: HashMap<u32, String>,
    widths: HashMap<u32, f64>,
    default_width: f64,
    encode: BTreeMap<char, u32>,
    /// Codes known only from `/ActualText`; drawing them needs the span.
    learned: BTreeMap<char, u32>,
    simple: bool,
    metrics: TextFontMetrics,
    /// Whether the fallback family came from the font rather than a guess.
    family_known: bool,
    /// A predefined Unicode CMap: codes are UTF-16 (or UCS-2) text, and
    /// widths are keyed by the CID it maps them to.
    cmap: Option<crate::pdf_cmaps::UnicodeCMap>,
    /// Codes are two bytes, or four for a surrogate pair.
    utf16: bool,
    unsupported: Option<String>,
}

impl LoadedFont {
    fn unsupported(name: &[u8], reason: impl Into<String>) -> Self {
        Self {
            resource_name: name.to_vec(),
            code_length: 1,
            decode: HashMap::new(),
            widths: HashMap::new(),
            default_width: 0.,
            encode: BTreeMap::new(),
            learned: BTreeMap::new(),
            simple: true,
            family_known: false,
            cmap: None,
            utf16: false,
            metrics: TextFontMetrics {
                font: name.to_vec(),
                advances: Vec::new(),
                word_space: None,
                fallback: FallbackFace {
                    family: FallbackFamily::Sans,
                    bold: false,
                    italic: false,
                },
                postscript_name: None,
                ascent: 800.,
                descent: -200.,
            },
            unsupported: Some(reason.into()),
        }
    }

    fn width(&self, code: u32) -> f64 {
        let key = match self.cmap {
            Some(cmap) => {
                crate::pdf_cmaps::utf16_character(code).and_then(|character| cmap.cid(character))
            }
            None => Some(code),
        };
        key.and_then(|key| self.widths.get(&key))
            .copied()
            .unwrap_or(self.default_width)
    }

    /// The text a code stands for: its ToUnicode entry, or in a Unicode
    /// CMap the code itself.
    fn decode_code(&self, code: u32) -> Option<String> {
        self.decode.get(&code).cloned().or_else(|| {
            self.cmap?;
            crate::pdf_cmaps::utf16_character(code).map(String::from)
        })
    }

    fn codes(&self, bytes: &[u8]) -> Vec<(u32, Vec<u8>)> {
        let mut codes = Vec::new();
        let mut index = 0;
        while index + self.code_length <= bytes.len() {
            // A UTF-16 high surrogate takes the low one after it.
            let length = if self.utf16
                && (0xd8..=0xdb).contains(&bytes[index])
                && bytes
                    .get(index + 2)
                    .is_some_and(|byte| (0xdc..=0xdf).contains(byte))
                && index + 4 <= bytes.len()
            {
                4
            } else {
                self.code_length
            };
            let chunk = &bytes[index..index + length];
            codes.push((
                chunk
                    .iter()
                    .fold(0u32, |code, byte| (code << 8) | u32::from(*byte)),
                chunk.to_vec(),
            ));
            index += length;
        }
        codes
    }

    /// The code that draws `character` and whether it needs an
    /// `/ActualText` span to read back as that character.
    fn encode_char(&self, character: char) -> Option<(u32, bool)> {
        self.encode
            .get(&character)
            .map(|code| (*code, false))
            .or_else(|| self.learned.get(&character).map(|code| (*code, true)))
    }

    /// Adds codes learned from `/ActualText` to the editor's metrics.
    fn refresh_metrics(&mut self) {
        let mut advances = self
            .encode
            .iter()
            .chain(
                self.learned
                    .iter()
                    .filter(|(character, _)| !self.encode.contains_key(character)),
            )
            .map(|(character, code)| (*character, self.width(*code)))
            .collect::<Vec<_>>();
        advances.sort_by_key(|advance| advance.0);
        self.metrics.advances = advances;
    }

    fn code_bytes(&self, code: u32) -> Vec<u8> {
        let length = if self.utf16 && code > 0xffff {
            4
        } else {
            self.code_length
        };
        (0..length)
            .rev()
            .map(|shift| ((code >> (shift * 8)) & 0xff) as u8)
            .collect()
    }
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> Result<&'a Object, TextEditError> {
    let mut object = object;
    for _ in 0..MAX_REFERENCE_HOPS {
        match object {
            Object::Reference(id) => object = document.get_object(*id)?,
            _ => return Ok(object),
        }
    }
    Err(TextEditError::Malformed(
        "reference chain is too long".into(),
    ))
}

fn resolve_dict<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    resolve(document, object).ok()?.as_dict().ok()
}

fn get_dict<'a>(
    document: &'a Document,
    dictionary: &'a Dictionary,
    key: &[u8],
) -> Option<&'a Dictionary> {
    resolve_dict(document, dictionary.get(key).ok()?)
}

fn get_number(document: &Document, dictionary: &Dictionary, key: &[u8]) -> Option<f64> {
    object_number(resolve(document, dictionary.get(key).ok()?).ok()?)
}

fn object_number(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn get_name(document: &Document, dictionary: &Dictionary, key: &[u8]) -> Option<Vec<u8>> {
    resolve(document, dictionary.get(key).ok()?)
        .ok()?
        .as_name()
        .ok()
        .map(<[u8]>::to_vec)
}

fn is_subset_name(name: &[u8]) -> bool {
    name.len() > 7 && name[6] == b'+' && name[..6].iter().all(u8::is_ascii_uppercase)
}

fn base_name(name: &[u8]) -> String {
    let name = if is_subset_name(name) {
        &name[7..]
    } else {
        name
    };
    String::from_utf8_lossy(name).into_owned()
}

/// The serif class, boldness and slant an embedded TrueType or OpenType
/// program declares in its OS/2 table.
fn embedded_font_style(
    document: &Document,
    descriptor: &Dictionary,
) -> Option<(Option<bool>, bool, bool)> {
    const MAX_FONT_PROGRAM_BYTES: usize = 32 * 1024 * 1024;
    let program = [b"FontFile2".as_slice(), b"FontFile3"]
        .iter()
        .find_map(|key| {
            resolve(document, descriptor.get(key).ok()?)
                .ok()?
                .as_stream()
                .ok()
        })?;
    let bytes = decoded_cached(program, MAX_FONT_PROGRAM_BYTES).ok()?;
    let face = ttf_parser::RawFace::parse(&bytes, 0).ok()?;
    let os2 = face.table(ttf_parser::Tag::from_bytes(b"OS/2"))?;
    let weight = u16::from_be_bytes([*os2.get(4)?, *os2.get(5)?]);
    let family_class = *os2.get(30)?;
    let serif_style = *os2.get(33)?;
    let selection = u16::from_be_bytes([*os2.get(62)?, *os2.get(63)?]);
    // sFamilyClass 1-7 are serif classes and 8 is sans serif; PANOSE serif
    // styles 11-13 are sans.
    let serif = match (family_class, serif_style) {
        (1..=7, _) => Some(true),
        (8, _) => Some(false),
        (_, 2..=10) => Some(true),
        (_, 11..=13) => Some(false),
        _ => None,
    };
    Some((serif, weight >= 600, selection & 1 != 0))
}

/// The bundled face for a font, and whether its family (serif, sans or
/// mono) is known rather than assumed.
fn fallback_face(
    base_font: &str,
    descriptor: Option<(&Dictionary, &Document)>,
) -> (FallbackFace, bool) {
    let lower = base_font.to_ascii_lowercase();
    let embedded =
        descriptor.and_then(|(descriptor, document)| embedded_font_style(document, descriptor));
    let flags = descriptor
        .and_then(|(descriptor, document)| get_number(document, descriptor, b"Flags"))
        .map(|flags| flags as i64)
        .unwrap_or(0);
    let weight = descriptor
        .and_then(|(descriptor, document)| get_number(document, descriptor, b"FontWeight"))
        .unwrap_or(400.);
    let italic_angle = descriptor
        .and_then(|(descriptor, document)| get_number(document, descriptor, b"ItalicAngle"))
        .unwrap_or(0.);
    let mono = flags & 1 != 0
        || ["courier", "mono", "consol", "menlo", "monaco", "typewriter"]
            .iter()
            .any(|word| lower.contains(word));
    let named_serif = flags & 2 != 0
        || [
            "times",
            "serif",
            "roman",
            "georgia",
            "garamond",
            "libertine",
            "cambria",
            "minion",
            "palatino",
            "book",
            "baskerville",
            "caslon",
            "bodoni",
            "didot",
            "century",
            "charter",
            "utopia",
            "sabon",
            "bembo",
            "goudy",
            "antiqua",
            "mincho",
            "songti",
        ]
        .iter()
        .any(|word| lower.contains(word))
            && !lower.contains("sans");
    let named_sans = [
        "sans",
        "arial",
        "helvetica",
        "gothic",
        "grotesk",
        "verdana",
        "calibri",
        "frutiger",
        "univers",
        "futura",
        "myriad",
        "segoe",
        "tahoma",
        "roboto",
        "inter",
        "avenir",
        "franklin",
        "trebuchet",
        "heiti",
    ]
    .iter()
    .any(|word| lower.contains(word));
    let embedded_serif = embedded.and_then(|(serif, _, _)| serif);
    let known = mono || embedded_serif.is_some() || named_serif || named_sans;
    // A sans name wins over a weight word such as "Book".
    let serif = !mono && embedded_serif.unwrap_or(named_serif && !named_sans);
    let bold = weight >= 600.
        || embedded.is_some_and(|(_, bold, _)| bold)
        || flags & (1 << 18) != 0
        || ["bold", "black", "heavy", "semibold", "demi"]
            .iter()
            .any(|word| lower.contains(word));
    let italic = italic_angle.abs() > 0.5
        || embedded.is_some_and(|(_, _, italic)| italic)
        || flags & (1 << 6) != 0
        || ["italic", "oblique"]
            .iter()
            .any(|word| lower.contains(word));
    (
        FallbackFace {
            family: if mono {
                FallbackFamily::Mono
            } else if serif {
                FallbackFamily::Serif
            } else {
                FallbackFamily::Sans
            },
            bold,
            italic,
        },
        known,
    )
}

const STANDARD_14: [&str; 14] = [
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-Oblique",
    "Helvetica-BoldOblique",
    "Times-Roman",
    "Times-Bold",
    "Times-Italic",
    "Times-BoldItalic",
    "Courier",
    "Courier-Bold",
    "Courier-Oblique",
    "Courier-BoldOblique",
    "Symbol",
    "ZapfDingbats",
];

fn is_standard_latin_font(base_font: &str) -> bool {
    let name = base_font.split(',').next().unwrap_or(base_font);
    STANDARD_14[..12].contains(&name)
        || ["Arial", "TimesNewRoman", "Times New Roman", "CourierNew"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
}

/// Unicode mappings from a ToUnicode CMap, for codes of `code_length` bytes.
fn to_unicode_map(
    document: &Document,
    font: &Dictionary,
    code_length: usize,
    limits: &TextEditLimits,
) -> Result<HashMap<u32, String>, TextEditError> {
    let Ok(to_unicode) = font.get(b"ToUnicode") else {
        return Ok(HashMap::new());
    };
    let probe = dictionary! {
        "Type" => "Font",
        "ToUnicode" => to_unicode.clone(),
    };
    let mut entries = Vec::new();
    if let Ok(lopdf::Encoding::UnicodeMapEncoding(cmap)) =
        probe.get_font_encoding_with_limit(document, limits.max_decoded_bytes_per_page)
    {
        let ranges = &cmap.bf_ranges[code_length - 1];
        'ranges: for (range, _) in ranges.iter() {
            let mut code = *range.start();
            loop {
                if entries.len() >= limits.max_cmap_entries {
                    break 'ranges;
                }
                if let Some(units) = cmap.get(code, code_length as u8) {
                    entries.push((code, units));
                }
                if code == *range.end() {
                    break;
                }
                code += 1;
            }
        }
    }
    // lopdf rejects some producers' maps outright (a bare word for the
    // CMapName): read their mappings directly.
    if entries.is_empty()
        && let Ok(stream) = resolve(document, to_unicode).and_then(|object| {
            object
                .as_stream()
                .map_err(|error| TextEditError::Malformed(error.to_string()))
        })
    {
        let bytes = if stream.dict.get(b"Filter").is_ok() {
            stream
                .decompressed_content_with_limit(limits.max_decoded_bytes_per_page)
                .unwrap_or_default()
        } else {
            stream.content.clone()
        };
        entries = parse_to_unicode(&bytes, code_length, limits.max_cmap_entries);
    }
    let mut map = HashMap::new();
    for (code, units) in entries {
        let mut text = char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .unwrap_or_default();
        // Office maps each maths letter to it twice ("𝑥𝑥" for 𝑥): a
        // glyph is one character outside the Basic Multilingual
        // Plane, never a repeat of one.
        let mut characters = text.chars();
        if let Some(first) = characters.next()
            && u32::from(first) > 0xFFFF
            && characters.all(|character| character == first)
        {
            text = first.to_string();
        }
        if !text.is_empty() && !text.contains('\u{fffd}') {
            map.insert(code, text);
        }
    }
    // A glyph mapped to a tab is a space (LibreOffice's exports): PDF text
    // has no tab stops.
    if !map.values().any(|text| text == " ") {
        for text in map.values_mut().filter(|text| *text == "\t") {
            *text = " ".into();
        }
    }
    Ok(map)
}

/// The `bfchar` and `bfrange` mappings of a ToUnicode CMap for codes of
/// `code_length` bytes, as UTF-16.
fn parse_to_unicode(bytes: &[u8], code_length: usize, max: usize) -> Vec<(u32, Vec<u16>)> {
    enum Token {
        Hex(Vec<u8>),
        Open,
        Close,
        Word(Vec<u8>),
    }
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match bytes[at] {
            b'%' => {
                while at < bytes.len() && !matches!(bytes[at], b'\r' | b'\n') {
                    at += 1;
                }
            }
            // Dictionary brackets.
            b'<' if bytes.get(at + 1) == Some(&b'<') => at += 2,
            b'>' => at += 1,
            b'<' => {
                let end = bytes[at..]
                    .iter()
                    .position(|byte| *byte == b'>')
                    .map_or(bytes.len(), |offset| at + offset);
                let digits = bytes[at + 1..end]
                    .iter()
                    .filter_map(|byte| (*byte as char).to_digit(16))
                    .map(|digit| digit as u8)
                    .collect::<Vec<_>>();
                let mut hex = digits
                    .chunks(2)
                    .map(|pair| pair[0] << 4 | pair.get(1).copied().unwrap_or(0))
                    .collect::<Vec<_>>();
                hex.truncate(digits.len().div_ceil(2));
                tokens.push(Token::Hex(hex));
                at = end + 1;
            }
            b'[' => {
                tokens.push(Token::Open);
                at += 1;
            }
            b']' => {
                tokens.push(Token::Close);
                at += 1;
            }
            byte if byte.is_ascii_whitespace() => at += 1,
            _ => {
                let start = at;
                while at < bytes.len()
                    && !bytes[at].is_ascii_whitespace()
                    && !matches!(bytes[at], b'<' | b'>' | b'[' | b']' | b'%' | b'(' | b'/')
                    || at == start
                {
                    at += 1;
                }
                tokens.push(Token::Word(bytes[start..at].to_vec()));
            }
        }
    }
    let number = |hex: &[u8]| {
        hex.iter()
            .fold(0u32, |value, byte| value << 8 | u32::from(*byte))
    };
    let units = |hex: &[u8]| {
        hex.chunks(2)
            .map(|pair| u16::from(pair[0]) << 8 | u16::from(pair.get(1).copied().unwrap_or(0)))
            .collect::<Vec<_>>()
    };
    let mut entries = Vec::new();
    let mut index = 0;
    let mut section = None;
    while index < tokens.len() && entries.len() < max {
        match (&tokens[index], section) {
            (Token::Word(word), _) if word == b"beginbfchar" => section = Some(false),
            (Token::Word(word), _) if word == b"beginbfrange" => section = Some(true),
            (Token::Word(word), _) if word.starts_with(b"end") => section = None,
            (Token::Hex(source), Some(false)) => {
                if let Some(Token::Hex(destination)) = tokens.get(index + 1) {
                    if source.len() == code_length {
                        entries.push((number(source), units(destination)));
                    }
                    index += 1;
                }
            }
            (Token::Hex(low), Some(true)) => {
                let Some(Token::Hex(high)) = tokens.get(index + 1) else {
                    index += 1;
                    continue;
                };
                let (low_code, high_code) = (number(low), number(high));
                let in_range = low.len() == code_length && low_code <= high_code;
                match tokens.get(index + 2) {
                    Some(Token::Hex(destination)) => {
                        let base = units(destination);
                        if in_range && let Some(last) = base.last().copied() {
                            for (offset, code) in (low_code..=high_code).enumerate() {
                                if entries.len() >= max {
                                    break;
                                }
                                let mut text = base.clone();
                                *text.last_mut().expect("not empty") =
                                    last.wrapping_add(offset as u16);
                                entries.push((code, text));
                            }
                        }
                        index += 2;
                    }
                    Some(Token::Open) => {
                        let mut code = low_code;
                        index += 3;
                        while let Some(Token::Hex(destination)) = tokens.get(index) {
                            if in_range && code <= high_code && entries.len() < max {
                                entries.push((code, units(destination)));
                            }
                            code += 1;
                            index += 1;
                        }
                    }
                    _ => index += 1,
                }
            }
            _ => {}
        }
        index += 1;
    }
    entries
}

/// Codes listed explicitly in an `/Encoding /Differences` array.
fn difference_codes(document: &Document, font: &Dictionary) -> BTreeSet<u32> {
    difference_names(document, font).into_keys().collect()
}

/// The glyph name a simple font's Differences give each code, but
/// `.notdef`.
fn difference_names(document: &Document, font: &Dictionary) -> BTreeMap<u32, Vec<u8>> {
    let mut codes = BTreeMap::new();
    let Some(encoding) = get_dict(document, font, b"Encoding") else {
        return codes;
    };
    let Some(differences) = encoding
        .get(b"Differences")
        .ok()
        .and_then(|value| resolve(document, value).ok())
        .and_then(|value| value.as_array().ok())
    else {
        return codes;
    };
    let mut code = 0i64;
    for item in differences {
        match resolve(document, item) {
            Ok(Object::Integer(start)) => code = *start,
            Ok(Object::Name(name)) => {
                // `.notdef` is the absence of a glyph.
                if let Ok(value) = u32::try_from(code)
                    && name != b".notdef"
                {
                    codes.insert(value, name.clone());
                }
                code += 1;
            }
            _ => {}
        }
    }
    codes
}

fn load_font(
    document: &Document,
    name: &[u8],
    font: &Dictionary,
    limits: &TextEditLimits,
) -> LoadedFont {
    match try_load_font(document, name, font, limits) {
        Ok(font) => font,
        Err(error) => LoadedFont::unsupported(name, error.to_string()),
    }
}

fn try_load_font(
    document: &Document,
    name: &[u8],
    font: &Dictionary,
    limits: &TextEditLimits,
) -> Result<LoadedFont, TextEditError> {
    let subtype = get_name(document, font, b"Subtype").unwrap_or_default();
    let base_font = get_name(document, font, b"BaseFont").unwrap_or_default();
    let base = base_name(&base_font);
    let mut loaded = match subtype.as_slice() {
        b"Type0" => load_type0_font(document, name, font, &base, limits),
        b"Type1" | b"MMType1" | b"TrueType" => {
            load_simple_font(document, name, font, &base_font, &base, limits)
        }
        b"Type3" => Err(TextEditError::ReadOnly(
            "Type3 fonts are not editable".into(),
        )),
        other => Err(TextEditError::ReadOnly(format!(
            "font type {} is not editable",
            String::from_utf8_lossy(other)
        ))),
    }?;
    // Neither the name nor the descriptor says serif or sans: look at the
    // glyphs.
    if !loaded.family_known {
        let descriptor = if loaded.simple {
            get_dict(document, font, b"FontDescriptor")
        } else {
            font.get(b"DescendantFonts")
                .ok()
                .and_then(|value| resolve(document, value).ok())
                .and_then(|value| value.as_array().ok())
                .and_then(|array| array.first())
                .and_then(|value| resolve_dict(document, value))
                .and_then(|descendant| get_dict(document, descendant, b"FontDescriptor"))
        };
        if let Some(serif) = descriptor
            .and_then(|descriptor| font_program(document, descriptor))
            .and_then(|program| outline_serif(&program, &loaded.encode, !loaded.simple))
        {
            loaded.metrics.fallback.family = if serif {
                FallbackFamily::Serif
            } else {
                FallbackFamily::Sans
            };
            loaded.family_known = true;
        }
    }
    Ok(loaded)
}

/// A descriptor's embedded font program, decompressed.
fn font_program(document: &Document, descriptor: &Dictionary) -> Option<std::sync::Arc<[u8]>> {
    const MAX_FONT_PROGRAM_BYTES: usize = 32 * 1024 * 1024;
    let program = [b"FontFile2".as_slice(), b"FontFile3", b"FontFile"]
        .iter()
        .find_map(|key| {
            resolve(document, descriptor.get(key).ok()?)
                .ok()?
                .as_stream()
                .ok()
        })?;
    decoded_cached(program, MAX_FONT_PROGRAM_BYTES).ok()
}

/// The codes of `decode` whose glyph a TrueType program draws, looked up as
/// a viewer does: a symbolic font's by code, another's by character.
/// Whitespace needs no outline.
fn program_codes(program: &[u8], decode: &HashMap<u32, String>, symbolic: bool) -> BTreeSet<u32> {
    struct Any(bool);
    impl ttf_parser::OutlineBuilder for Any {
        fn move_to(&mut self, _: f32, _: f32) {
            self.0 = true;
        }
        fn line_to(&mut self, _: f32, _: f32) {}
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
        fn close(&mut self) {}
    }
    let Ok(face) = ttf_parser::Face::parse(program, 0) else {
        return BTreeSet::new();
    };
    let symbolic_font = symbolic;
    let symbolic = |code: u32| {
        face.tables()
            .cmap?
            .subtables
            .into_iter()
            .find_map(
                |subtable| match (subtable.platform_id, subtable.encoding_id) {
                    (ttf_parser::PlatformId::Windows, 0) => subtable.glyph_index(0xF000 + code),
                    (ttf_parser::PlatformId::Macintosh, 0) => subtable.glyph_index(code),
                    _ => None,
                },
            )
    };
    decode
        .iter()
        .filter(|(code, text)| {
            let mut characters = text.chars();
            let (Some(character), None) = (characters.next(), characters.next()) else {
                return false;
            };
            let glyph = if symbolic_font {
                symbolic(**code)
            } else {
                face.glyph_index(character)
            };
            let Some(glyph) = glyph.filter(|glyph| glyph.0 != 0) else {
                return false;
            };
            character.is_whitespace() || {
                let mut any = Any(false);
                face.outline_glyph(glyph, &mut any);
                any.0
            }
        })
        .map(|(code, _)| *code)
        .collect()
}

/// The glyph names a Type 1 or CFF program has; `None` for another kind
/// of program.
fn program_glyph_names(program: &[u8]) -> Option<BTreeSet<Vec<u8>>> {
    let names = if let Some(table) = ttf_parser::cff::Table::parse(program) {
        (0..table.number_of_glyphs())
            .filter_map(|glyph| table.glyph_name(ttf_parser::GlyphId(glyph)))
            .map(|name| name.as_bytes().to_vec())
            .collect()
    } else {
        type1_glyph_names(program)?
    };
    Some(names)
}

/// The names a font may give `character`'s glyph: the standard Latin
/// names of printable ASCII.
fn ascii_glyph_names(character: char) -> &'static [&'static str] {
    const NAMES: [&[&str]; 95] = [
        &["space"],
        &["exclam"],
        &["quotedbl"],
        &["numbersign"],
        &["dollar"],
        &["percent"],
        &["ampersand"],
        &["quotesingle", "quoteright"],
        &["parenleft"],
        &["parenright"],
        &["asterisk"],
        &["plus"],
        &["comma"],
        &["hyphen", "minus"],
        &["period"],
        &["slash"],
        &["zero"],
        &["one"],
        &["two"],
        &["three"],
        &["four"],
        &["five"],
        &["six"],
        &["seven"],
        &["eight"],
        &["nine"],
        &["colon"],
        &["semicolon"],
        &["less"],
        &["equal"],
        &["greater"],
        &["question"],
        &["at"],
        &["A"],
        &["B"],
        &["C"],
        &["D"],
        &["E"],
        &["F"],
        &["G"],
        &["H"],
        &["I"],
        &["J"],
        &["K"],
        &["L"],
        &["M"],
        &["N"],
        &["O"],
        &["P"],
        &["Q"],
        &["R"],
        &["S"],
        &["T"],
        &["U"],
        &["V"],
        &["W"],
        &["X"],
        &["Y"],
        &["Z"],
        &["bracketleft"],
        &["backslash"],
        &["bracketright"],
        &["asciicircum"],
        &["underscore"],
        &["grave", "quoteleft"],
        &["a"],
        &["b"],
        &["c"],
        &["d"],
        &["e"],
        &["f"],
        &["g"],
        &["h"],
        &["i"],
        &["j"],
        &["k"],
        &["l"],
        &["m"],
        &["n"],
        &["o"],
        &["p"],
        &["q"],
        &["r"],
        &["s"],
        &["t"],
        &["u"],
        &["v"],
        &["w"],
        &["x"],
        &["y"],
        &["z"],
        &["braceleft"],
        &["bar"],
        &["braceright"],
        &["asciitilde"],
    ];
    match character {
        ' '..='~' => NAMES[character as usize - 32],
        _ => &[],
    }
}

/// The glyph names in a Type 1 program's encrypted CharStrings.
fn type1_glyph_names(program: &[u8]) -> Option<BTreeSet<Vec<u8>>> {
    let find = |haystack: &[u8], needle: &[u8]| {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    };
    let mut rest = &program[find(program, b"eexec")? + 5..];
    while let [first, tail @ ..] = rest
        && first.is_ascii_whitespace()
    {
        rest = tail;
    }
    // The encrypted part is binary, or hexadecimal text.
    let cipher = if rest.len() >= 4 && rest[..4].iter().all(u8::is_ascii_hexdigit) {
        let digits = rest
            .iter()
            .filter(|byte| !byte.is_ascii_whitespace())
            .take_while(|byte| byte.is_ascii_hexdigit())
            .map(|byte| (*byte as char).to_digit(16).unwrap_or(0) as u8)
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| pair[0] << 4 | pair[1])
            .collect()
    } else {
        rest.to_vec()
    };
    let mut key: u16 = 55665;
    let plain = cipher
        .iter()
        .map(|&byte| {
            let plain = byte ^ (key >> 8) as u8;
            key = (u16::from(byte).wrapping_add(key))
                .wrapping_mul(52845)
                .wrapping_add(22719);
            plain
        })
        .collect::<Vec<_>>();
    let start = find(&plain, b"/CharStrings")?;
    let mut at = start + find(&plain[start..], b"begin")? + 5;
    let skip_space = |at: &mut usize| {
        while plain.get(*at).is_some_and(u8::is_ascii_whitespace) {
            *at += 1;
        }
    };
    let token = |at: &mut usize| {
        let start = *at;
        while plain
            .get(*at)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && (*at == start || *byte != b'/'))
        {
            *at += 1;
        }
        &plain[start..*at]
    };
    let mut names = BTreeSet::new();
    loop {
        skip_space(&mut at);
        if plain.get(at) != Some(&b'/') {
            break;
        }
        let name = token(&mut at);
        names.insert(name[1..].to_vec());
        skip_space(&mut at);
        let length = std::str::from_utf8(token(&mut at))
            .ok()?
            .parse::<usize>()
            .ok()?;
        skip_space(&mut at);
        token(&mut at); // RD or -|
        at += 1 + length;
        // ND, |- or "noaccess def" up to the next name.
        loop {
            skip_space(&mut at);
            match plain.get(at) {
                Some(b'/') | None => break,
                _ if plain[at..].starts_with(b"end") => break,
                _ => {
                    token(&mut at);
                }
            }
        }
    }
    Some(names)
}

/// Whether a font program's letters have serifs, judged by the outline of
/// a plain stem letter: serif faces draw "l", "I", "i", "H" or "T" with about
/// three times as many segments as sans faces (Tinos "l" 10, Arimo 4; STIX
/// 11, SF Compact 3). `codes` maps characters to the font's codes; in a
/// Type0 font (`cid_glyphs`) a code is also its glyph id.
fn outline_serif(program: &[u8], codes: &BTreeMap<char, u32>, cid_glyphs: bool) -> Option<bool> {
    struct Segments(usize);
    impl ttf_parser::OutlineBuilder for Segments {
        fn move_to(&mut self, _: f32, _: f32) {}
        fn line_to(&mut self, _: f32, _: f32) {
            self.0 += 1;
        }
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn close(&mut self) {}
    }
    // The fewest segments a serif face draws each letter with.
    const PROBES: [(char, usize); 5] = [('l', 7), ('I', 7), ('i', 11), ('H', 19), ('T', 11)];
    let code = |character: char| codes.get(&character).copied();
    if let Ok(face) = ttf_parser::Face::parse(program, 0) {
        let glyph = |character: char| {
            face.glyph_index(character).or_else(|| {
                let code = code(character)?;
                if cid_glyphs {
                    return u16::try_from(code).ok().map(ttf_parser::GlyphId);
                }
                // Symbolic TrueType fonts map codes through (3,0) at
                // 0xF000 + code, or (1,0) directly.
                face.tables()
                    .cmap?
                    .subtables
                    .into_iter()
                    .find_map(
                        |subtable| match (subtable.platform_id, subtable.encoding_id) {
                            (ttf_parser::PlatformId::Windows, 0) => {
                                subtable.glyph_index(0xF000 + code)
                            }
                            (ttf_parser::PlatformId::Macintosh, 0) => subtable.glyph_index(code),
                            _ => None,
                        },
                    )
            })
        };
        return PROBES.iter().find_map(|(character, threshold)| {
            let mut segments = Segments(0);
            face.outline_glyph(glyph(*character)?, &mut segments)?;
            Some(segments.0 >= *threshold)
        });
    }
    let table = ttf_parser::cff::Table::parse(program)?;
    PROBES.iter().find_map(|(character, threshold)| {
        let glyph = table
            .glyph_index_by_name(&character.to_string())
            .or_else(|| table.glyph_index(u8::try_from(code(*character)?).ok()?))?;
        let mut segments = Segments(0);
        table.outline(glyph, &mut segments).ok()?;
        Some(segments.0 >= *threshold)
    })
}

fn descriptor_metrics(document: &Document, descriptor: Option<&Dictionary>) -> (f64, f64) {
    let ascent = descriptor
        .and_then(|descriptor| get_number(document, descriptor, b"Ascent"))
        .filter(|value| *value > 0.)
        .unwrap_or(800.);
    let descent = descriptor
        .and_then(|descriptor| get_number(document, descriptor, b"Descent"))
        .filter(|value| *value < 0.)
        .unwrap_or(-200.);
    (ascent, descent)
}

fn is_embedded(document: &Document, descriptor: Option<&Dictionary>) -> bool {
    descriptor.is_some_and(|descriptor| {
        [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
            .iter()
            .any(|key| {
                descriptor
                    .get(key)
                    .is_ok_and(|value| resolve(document, value).is_ok())
            })
    })
}

/// The standard Symbol font's built-in encoding: each code's width
/// (Adobe's metrics) and character. Its serif and sans ©, ® and ™ read as
/// those characters; its bracket and integral pieces keep Adobe's private
/// code points.
#[rustfmt::skip]
const SYMBOL: [(u8, u16, char); 189] = [
    (32, 250, '\u{20}'), (33, 333, '\u{21}'), (34, 713, '\u{2200}'), (35, 500, '\u{23}'),
    (36, 549, '\u{2203}'), (37, 833, '\u{25}'), (38, 778, '\u{26}'), (39, 439, '\u{220b}'),
    (40, 333, '\u{28}'), (41, 333, '\u{29}'), (42, 500, '\u{2217}'), (43, 549, '\u{2b}'),
    (44, 250, '\u{2c}'), (45, 549, '\u{2212}'), (46, 250, '\u{2e}'), (47, 278, '\u{2f}'),
    (48, 500, '\u{30}'), (49, 500, '\u{31}'), (50, 500, '\u{32}'), (51, 500, '\u{33}'),
    (52, 500, '\u{34}'), (53, 500, '\u{35}'), (54, 500, '\u{36}'), (55, 500, '\u{37}'),
    (56, 500, '\u{38}'), (57, 500, '\u{39}'), (58, 278, '\u{3a}'), (59, 278, '\u{3b}'),
    (60, 549, '\u{3c}'), (61, 549, '\u{3d}'), (62, 549, '\u{3e}'), (63, 444, '\u{3f}'),
    (64, 549, '\u{2245}'), (65, 722, '\u{391}'), (66, 667, '\u{392}'), (67, 722, '\u{3a7}'),
    (68, 612, '\u{394}'), (69, 611, '\u{395}'), (70, 763, '\u{3a6}'), (71, 603, '\u{393}'),
    (72, 722, '\u{397}'), (73, 333, '\u{399}'), (74, 631, '\u{3d1}'), (75, 722, '\u{39a}'),
    (76, 686, '\u{39b}'), (77, 889, '\u{39c}'), (78, 722, '\u{39d}'), (79, 722, '\u{39f}'),
    (80, 768, '\u{3a0}'), (81, 741, '\u{398}'), (82, 556, '\u{3a1}'), (83, 592, '\u{3a3}'),
    (84, 611, '\u{3a4}'), (85, 690, '\u{3a5}'), (86, 439, '\u{3c2}'), (87, 768, '\u{3a9}'),
    (88, 645, '\u{39e}'), (89, 795, '\u{3a8}'), (90, 611, '\u{396}'), (91, 333, '\u{5b}'),
    (92, 863, '\u{2234}'), (93, 333, '\u{5d}'), (94, 658, '\u{22a5}'), (95, 500, '\u{5f}'),
    (96, 500, '\u{f8e5}'), (97, 631, '\u{3b1}'), (98, 549, '\u{3b2}'), (99, 549, '\u{3c7}'),
    (100, 494, '\u{3b4}'), (101, 439, '\u{3b5}'), (102, 521, '\u{3c6}'), (103, 411, '\u{3b3}'),
    (104, 603, '\u{3b7}'), (105, 329, '\u{3b9}'), (106, 603, '\u{3d5}'), (107, 549, '\u{3ba}'),
    (108, 549, '\u{3bb}'), (109, 576, '\u{3bc}'), (110, 521, '\u{3bd}'), (111, 549, '\u{3bf}'),
    (112, 549, '\u{3c0}'), (113, 521, '\u{3b8}'), (114, 549, '\u{3c1}'), (115, 603, '\u{3c3}'),
    (116, 439, '\u{3c4}'), (117, 576, '\u{3c5}'), (118, 713, '\u{3d6}'), (119, 686, '\u{3c9}'),
    (120, 493, '\u{3be}'), (121, 686, '\u{3c8}'), (122, 494, '\u{3b6}'), (123, 480, '\u{7b}'),
    (124, 200, '\u{7c}'), (125, 480, '\u{7d}'), (126, 549, '\u{223c}'), (160, 750, '\u{20ac}'),
    (161, 620, '\u{3d2}'), (162, 247, '\u{2032}'), (163, 549, '\u{2264}'),
    (164, 167, '\u{2044}'), (165, 713, '\u{221e}'), (166, 500, '\u{192}'),
    (167, 753, '\u{2663}'), (168, 753, '\u{2666}'), (169, 753, '\u{2665}'),
    (170, 753, '\u{2660}'), (171, 1042, '\u{2194}'), (172, 987, '\u{2190}'),
    (173, 603, '\u{2191}'), (174, 987, '\u{2192}'), (175, 603, '\u{2193}'),
    (176, 400, '\u{b0}'), (177, 549, '\u{b1}'), (178, 411, '\u{2033}'), (179, 549, '\u{2265}'),
    (180, 549, '\u{d7}'), (181, 713, '\u{221d}'), (182, 494, '\u{2202}'),
    (183, 460, '\u{2022}'), (184, 549, '\u{f7}'), (185, 549, '\u{2260}'),
    (186, 549, '\u{2261}'), (187, 549, '\u{2248}'), (188, 1000, '\u{2026}'),
    (189, 603, '\u{f8e6}'), (190, 1000, '\u{f8e7}'), (191, 658, '\u{21b5}'),
    (192, 823, '\u{2135}'), (193, 686, '\u{2111}'), (194, 795, '\u{211c}'),
    (195, 987, '\u{2118}'), (196, 768, '\u{2297}'), (197, 768, '\u{2295}'),
    (198, 823, '\u{2205}'), (199, 768, '\u{2229}'), (200, 768, '\u{222a}'),
    (201, 713, '\u{2283}'), (202, 713, '\u{2287}'), (203, 713, '\u{2284}'),
    (204, 713, '\u{2282}'), (205, 713, '\u{2286}'), (206, 713, '\u{2208}'),
    (207, 713, '\u{2209}'), (208, 768, '\u{2220}'), (209, 713, '\u{2207}'),
    (210, 790, '\u{ae}'), (211, 790, '\u{a9}'), (212, 890, '\u{2122}'), (213, 823, '\u{220f}'),
    (214, 549, '\u{221a}'), (215, 250, '\u{22c5}'), (216, 713, '\u{ac}'),
    (217, 603, '\u{2227}'), (218, 603, '\u{2228}'), (219, 1042, '\u{21d4}'),
    (220, 987, '\u{21d0}'), (221, 603, '\u{21d1}'), (222, 987, '\u{21d2}'),
    (223, 603, '\u{21d3}'), (224, 494, '\u{25ca}'), (225, 329, '\u{2329}'),
    (226, 790, '\u{ae}'), (227, 790, '\u{a9}'), (228, 786, '\u{2122}'), (229, 713, '\u{2211}'),
    (230, 384, '\u{f8eb}'), (231, 384, '\u{f8ec}'), (232, 384, '\u{f8ed}'),
    (233, 384, '\u{f8ee}'), (234, 384, '\u{f8ef}'), (235, 384, '\u{f8f0}'),
    (236, 494, '\u{f8f1}'), (237, 494, '\u{f8f2}'), (238, 494, '\u{f8f3}'),
    (239, 494, '\u{f8f4}'), (241, 329, '\u{232a}'), (242, 274, '\u{222b}'),
    (243, 686, '\u{2320}'), (244, 686, '\u{f8f5}'), (245, 686, '\u{2321}'),
    (246, 384, '\u{f8f6}'), (247, 384, '\u{f8f7}'), (248, 384, '\u{f8f8}'),
    (249, 384, '\u{f8f9}'), (250, 384, '\u{f8fa}'), (251, 384, '\u{f8fb}'),
    (252, 494, '\u{f8fc}'), (253, 494, '\u{f8fd}'), (254, 494, '\u{f8fe}'),
];

fn load_simple_font(
    document: &Document,
    name: &[u8],
    font: &Dictionary,
    base_font: &[u8],
    base: &str,
    limits: &TextEditLimits,
) -> Result<LoadedFont, TextEditError> {
    let descriptor = get_dict(document, font, b"FontDescriptor");
    let (fallback, family_known) =
        fallback_face(base, descriptor.map(|descriptor| (descriptor, document)));
    let mut decode = to_unicode_map(document, font, 1, limits)?;
    let to_unicode_codes = decode.keys().copied().collect::<BTreeSet<_>>();
    let mut without_to_unicode = font.clone();
    without_to_unicode.remove(b"ToUnicode");
    if let Ok(encoding) =
        without_to_unicode.get_font_encoding_with_limit(document, limits.max_decoded_bytes_per_page)
    {
        for code in 0u32..=255 {
            if decode.contains_key(&code) {
                continue;
            }
            if let Ok(text) = encoding.bytes_to_string(&[code as u8])
                && !text.is_empty()
                && !text.contains('\u{fffd}')
            {
                decode.insert(code, text);
            }
        }
    }
    // The standard Symbol font a viewer supplies: its own encoding.
    let symbol = base.split(',').next() == Some("Symbol")
        && !is_embedded(document, descriptor)
        && font.get(b"Encoding").is_err();
    if symbol {
        for (code, _, character) in SYMBOL {
            if !to_unicode_codes.contains(&u32::from(code)) {
                decode.insert(u32::from(code), character.to_string());
            }
        }
    }
    let first_char = get_number(document, font, b"FirstChar").unwrap_or(0.) as i64;
    let widths_array = font
        .get(b"Widths")
        .ok()
        .and_then(|value| resolve(document, value).ok())
        .and_then(|value| value.as_array().ok());
    let missing_width = descriptor
        .and_then(|descriptor| get_number(document, descriptor, b"MissingWidth"))
        .unwrap_or(0.);
    let mut widths = HashMap::new();
    let embedded = is_embedded(document, descriptor);
    if let Some(array) = widths_array {
        for (offset, value) in array.iter().enumerate() {
            let Some(width) = resolve(document, value).ok().and_then(object_number) else {
                continue;
            };
            if let Ok(code) = u32::try_from(first_char + offset as i64) {
                widths.insert(code, width);
            }
        }
    } else if symbol {
        for (code, width, _) in SYMBOL {
            widths.insert(u32::from(code), f64::from(width));
        }
    } else if !embedded {
        // A font without /Widths that the viewer supplies: the bundled faces
        // are metric matches for Helvetica, Times and Courier; another font
        // takes an installed font of its name, or the closest bundled family.
        let installed = (!is_standard_latin_font(base))
            .then(|| crate::pdf_engine::installed_font_named(base))
            .flatten();
        for (code, text) in &decode {
            let mut characters = text.chars();
            if let (Some(character), None) = (characters.next(), characters.next())
                && let Some(width) = installed
                    .and_then(|index| crate::pdf_engine::installed_glyph_advance(index, character))
                    .or_else(|| {
                        crate::pdf_engine::bundled_text_edit_glyph(fallback, character)
                            .map(|(_, width)| width)
                    })
            {
                widths.insert(*code, width);
            }
        }
    } else {
        return Err(TextEditError::ReadOnly(format!(
            "font {base} has no glyph widths"
        )));
    }
    // Which codes a font program certainly contains: a subset keeps only the
    // glyphs it lists; a complete or system font draws its whole encoding.
    let subset = is_subset_name(base_font);
    let mut listed = to_unicode_codes
        .union(&difference_codes(document, font))
        .copied()
        .collect::<BTreeSet<_>>();
    // A subset without a ToUnicode map or Differences (Word's TrueType
    // fonts): its program says which glyphs it kept.
    if subset
        && let Some(program) = descriptor.and_then(|descriptor| font_program(document, descriptor))
    {
        // Flag bit 3: symbolic, its codes looked up directly.
        let symbolic = descriptor
            .and_then(|descriptor| get_number(document, descriptor, b"Flags"))
            .is_some_and(|flags| flags as i64 & 4 != 0);
        // Word gives the characters it did not use no width.
        listed.extend(
            program_codes(&program, &decode, symbolic)
                .into_iter()
                .filter(|code| widths.get(code).is_some_and(|width| *width > 0.)),
        );
        // A Type 1 or CFF subset lists every code of its encoding (pdfTeX's
        // Differences) but keeps only the glyphs it drew.
        if let Some(names) = program_glyph_names(&program) {
            let differences = difference_names(document, font);
            listed.retain(|code| {
                if let Some(name) = differences.get(code) {
                    return names.contains(name);
                }
                let mut text = decode
                    .get(code)
                    .map(|text| text.chars())
                    .into_iter()
                    .flatten();
                match (text.next(), text.next()) {
                    (Some(character), None) if !character.is_whitespace() => {
                        let candidates = ascii_glyph_names(character);
                        candidates.is_empty()
                            || candidates
                                .iter()
                                .any(|name| names.contains(name.as_bytes()))
                    }
                    _ => true,
                }
            });
        }
    }
    // Viewers read a code a ToUnicode map leaves out as nothing: when a
    // subset has one, only its codes are typed.
    if subset && !to_unicode_codes.is_empty() {
        listed.retain(|code| to_unicode_codes.contains(code));
    }
    let mut encode = BTreeMap::new();
    let mut codes = decode.keys().copied().collect::<Vec<_>>();
    codes.sort_unstable();
    for code in codes {
        let available = if subset {
            listed.contains(&code)
        } else {
            widths.get(&code).is_some_and(|width| *width > 0.) || code == 32
        };
        if !available || !widths.contains_key(&code) {
            continue;
        }
        let mut characters = decode[&code].chars();
        if let (Some(character), None) = (characters.next(), characters.next()) {
            encode.entry(character).or_insert(code);
        }
    }
    let (ascent, descent) = descriptor_metrics(document, descriptor);
    let word_space = decode.get(&32).and_then(|text| {
        let mut characters = text.chars();
        match (characters.next(), characters.next()) {
            (Some(character), None) if encode.get(&character) == Some(&32) => Some(character),
            _ => None,
        }
    });
    let advances = encode
        .iter()
        .map(|(character, code)| {
            (
                *character,
                widths.get(code).copied().unwrap_or(missing_width),
            )
        })
        .collect();
    Ok(LoadedFont {
        resource_name: name.to_vec(),
        code_length: 1,
        decode,
        widths,
        default_width: missing_width,
        encode,
        learned: BTreeMap::new(),
        simple: true,
        family_known,
        cmap: None,
        utf16: false,
        metrics: TextFontMetrics {
            font: name.to_vec(),
            advances,
            word_space,
            fallback,
            postscript_name: Some(base.to_owned()),
            ascent,
            descent,
        },
        unsupported: None,
    })
}

fn load_type0_font(
    document: &Document,
    name: &[u8],
    font: &Dictionary,
    base: &str,
    limits: &TextEditLimits,
) -> Result<LoadedFont, TextEditError> {
    let encoding = get_name(document, font, b"Encoding").unwrap_or_default();
    let (cmap, utf16) = match encoding.as_slice() {
        b"Identity-H" => (None, false),
        b"Identity-V" => {
            return Err(TextEditError::ReadOnly(
                "vertical text is not editable".into(),
            ));
        }
        other => match crate::pdf_cmaps::UnicodeCMap::from_encoding(other) {
            Some((cmap, utf16)) => (Some(cmap), utf16),
            None => {
                return Err(TextEditError::ReadOnly(format!(
                    "font {base} uses CMap {}, which is not editable",
                    String::from_utf8_lossy(other)
                )));
            }
        },
    };
    let descendant = font
        .get(b"DescendantFonts")
        .ok()
        .and_then(|value| resolve(document, value).ok())
        .and_then(|value| value.as_array().ok())
        .and_then(|array| array.first())
        .and_then(|value| resolve_dict(document, value))
        .ok_or_else(|| TextEditError::ReadOnly(format!("font {base} has no descendant font")))?;
    let descriptor = get_dict(document, descendant, b"FontDescriptor");
    let (fallback, family_known) =
        fallback_face(base, descriptor.map(|descriptor| (descriptor, document)));
    let decode = to_unicode_map(document, font, 2, limits)?;
    if decode.is_empty() && cmap.is_none() {
        return Err(TextEditError::ReadOnly(format!(
            "font {base} has no Unicode mapping"
        )));
    }
    let default_width = get_number(document, descendant, b"DW").unwrap_or(1000.);
    let mut widths = HashMap::new();
    if let Some(array) = descendant
        .get(b"W")
        .ok()
        .and_then(|value| resolve(document, value).ok())
        .and_then(|value| value.as_array().ok())
    {
        let items = array
            .iter()
            .map(|item| resolve(document, item))
            .collect::<Result<Vec<_>, _>>()?;
        let mut index = 0;
        while index < items.len() {
            let Some(first) = object_number(items[index]) else {
                break;
            };
            match items.get(index + 1) {
                Some(Object::Array(list)) => {
                    for (offset, value) in list.iter().enumerate() {
                        if let Some(width) = resolve(document, value).ok().and_then(object_number) {
                            widths.insert(first as u32 + offset as u32, width);
                        }
                        if widths.len() > limits.max_cmap_entries {
                            break;
                        }
                    }
                    index += 2;
                }
                Some(last) => {
                    let (Some(last), Some(width)) = (
                        object_number(last),
                        items.get(index + 2).and_then(|value| object_number(value)),
                    ) else {
                        break;
                    };
                    let (first, last) = (first as u32, last as u32);
                    if last >= first && last - first <= limits.max_cmap_entries as u32 {
                        for code in first..=last {
                            widths.insert(code, width);
                        }
                    }
                    index += 3;
                }
                None => break,
            }
        }
    }
    let mut encode = BTreeMap::new();
    let mut codes = decode.keys().copied().collect::<Vec<_>>();
    codes.sort_unstable();
    for code in codes {
        let mut characters = decode[&code].chars();
        if let (Some(character), None) = (characters.next(), characters.next()) {
            encode.entry(character).or_insert(code);
        }
    }
    let (ascent, descent) = descriptor_metrics(document, descriptor);
    let width_of = |code: u32| match cmap {
        Some(cmap) => crate::pdf_cmaps::utf16_character(code)
            .and_then(|character| cmap.cid(character))
            .and_then(|cid| widths.get(&cid).copied()),
        None => widths.get(&code).copied(),
    };
    let advances = encode
        .iter()
        .map(|(character, code)| (*character, width_of(*code).unwrap_or(default_width)))
        .collect();
    Ok(LoadedFont {
        resource_name: name.to_vec(),
        code_length: 2,
        decode,
        widths,
        default_width,
        encode,
        learned: BTreeMap::new(),
        simple: false,
        family_known,
        cmap,
        utf16,
        metrics: TextFontMetrics {
            font: name.to_vec(),
            advances,
            word_space: None,
            fallback,
            postscript_name: Some(base.to_owned()),
            ascent,
            descent,
        },
        unsupported: None,
    })
}

// ---------------------------------------------------------------------------
// Pages

pub(crate) fn page_id(document: &Document, page_index: u32) -> Result<ObjectId, TextEditError> {
    page_index
        .checked_add(1)
        .and_then(|number| document.get_pages().get(&number).copied())
        .ok_or_else(|| TextEditError::Page(format!("page {page_index} does not exist")))
}

/// The page's own or nearest inherited `/Resources`.
pub(crate) fn page_resources(document: &Document, page_id: ObjectId) -> Option<Dictionary> {
    let mut node_id = page_id;
    for _ in 0..MAX_NESTING {
        let node = document.get_object(node_id).ok()?.as_dict().ok()?;
        if let Ok(resources) = node.get(b"Resources") {
            return resolve_dict(document, resources).cloned();
        }
        node_id = node.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn page_content_ids(
    document: &Document,
    page_id: ObjectId,
) -> Result<Vec<ObjectId>, TextEditError> {
    let page = document.get_object(page_id)?.as_dict()?;
    let Ok(contents) = page.get(b"Contents") else {
        return Ok(Vec::new());
    };
    match contents {
        Object::Reference(id) => match document.get_object(*id)? {
            Object::Array(items) => items
                .iter()
                .map(|item| {
                    item.as_reference()
                        .map_err(|_| TextEditError::Malformed("direct content stream".into()))
                })
                .collect(),
            _ => Ok(vec![*id]),
        },
        Object::Array(items) => items
            .iter()
            .map(|item| {
                item.as_reference()
                    .map_err(|_| TextEditError::Malformed("direct content stream".into()))
            })
            .collect(),
        _ => Err(TextEditError::Malformed(
            "page contents are not a stream reference".into(),
        )),
    }
}

/// Colour operators in effect, as text that sets them again (or, prefixed
/// with `?`, a description of operators that cannot be rewritten).
#[derive(Clone, Debug, Default, PartialEq)]
struct ColorState {
    fill_space: Option<String>,
    fill: Option<String>,
    /// The fill as DeviceRGB, when it is a device colour.
    device_fill: Option<[u64; 3]>,
    stroke_space: Option<String>,
    stroke: Option<String>,
    graphics_state: Option<String>,
}

#[derive(Clone, Debug)]
struct GraphicsState {
    ctm: Matrix,
    /// The clip's geometry as a key: equal clips (the same paths under the
    /// same matrices) share it, so text clipped alike can join one line.
    clip: u64,
    font: Option<Vec<u8>>,
    font_size: f64,
    char_spacing: f64,
    word_spacing: f64,
    horizontal_scale: f64,
    leading: f64,
    rise: f64,
    render_mode: i64,
    color: ColorState,
    /// The stroke width, in user space.
    line_width: f64,
    /// The clip's bounds in page space, when content is clipped.
    clip_bounds: Option<[f64; 4]>,
    /// The clipping paths the page's own content set, innermost first.
    clips: Option<std::sync::Arc<ClipLink>>,
}

/// A clipping path in effect: the painted path that set it, and the clips
/// it was set inside.
#[derive(Debug)]
struct ClipLink {
    paint: usize,
    parent: Option<std::sync::Arc<ClipLink>>,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            clip: 0,
            line_width: 1.,
            clip_bounds: None,
            clips: None,
            font: None,
            font_size: 0.,
            char_spacing: 0.,
            word_spacing: 0.,
            horizontal_scale: 1.,
            leading: 0.,
            rise: 0.,
            render_mode: 0,
            color: ColorState::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct ShownGlyph {
    code: Vec<u8>,
    unicode: Option<String>,
    /// Its text comes from an `/ActualText` span.
    actual_text: bool,
    /// Start in the operator's text space.
    x: f64,
    advance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ShowKind {
    Show,
    ShowArray,
    NextLineShow,
    SpacedNextLineShow { word: f64, character: f64 },
}

#[derive(Clone, Debug)]
struct ShowOp {
    op_index: usize,
    kind: ShowKind,
    state: GraphicsState,
    /// The text and line matrices where the operator starts drawing.
    text_matrix: Matrix,
    line_matrix: Matrix,
    /// Text matrix times CTM where the operator starts drawing.
    space: Matrix,
    glyphs: Vec<ShownGlyph>,
    has_string: bool,
    /// The operator's total text-matrix advance.
    advance: f64,
    /// How many marked-content levels enclose the operator up to and
    /// including the outermost `/ActualText` span (zero when none does).
    actual_text_depth: usize,
    /// The marked-content identifier of the innermost tagged content the
    /// page's own content draws it in: what links it to the page's
    /// structure tree.
    marked_content_id: Option<i64>,
}

struct ExtractedLine {
    public: PageTextLine,
    ops: Vec<usize>,
    /// For each character, the glyphs that draw it; `None` for a synthetic
    /// space.
    clusters: Vec<Option<Cluster>>,
}

/// The glyphs of one show operator that together draw `parts` characters,
/// of which a character is `part`. A glyph without text of its own (a
/// contextual alternate) joins the cluster before it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Cluster {
    show: usize,
    first_glyph: usize,
    last_glyph: usize,
    part: usize,
    parts: usize,
}

/// Where a decoded stream comes from: one of the page's content streams, or
/// a Form XObject drawn by `Do /name` from the stream `parent`.
#[derive(Clone, Debug)]
enum StreamOwner {
    Page,
    Form {
        object: ObjectId,
        parent: usize,
        name: Vec<u8>,
        /// The form's resources (its own, or those it inherits).
        resources: Dictionary,
    },
}

struct ExtractedPage {
    page_id: ObjectId,
    content_ids: Vec<ObjectId>,
    /// The page's content streams, then each drawn Form XObject's.
    streams: Vec<Vec<u8>>,
    owners: Vec<StreamOwner>,
    operations: Vec<ContentOp>,
    fonts: BTreeMap<Vec<u8>, LoadedFont>,
    shows: Vec<ShowOp>,
    lines: Vec<ExtractedLine>,
    paints: Vec<PaintOp>,
    draws: Vec<DrawOp>,
    /// The matrix in effect at each `q` of the page's own content streams.
    saves: HashMap<usize, Matrix>,
}

impl ExtractedPage {
    /// The rules and fills the page's own content paints, in page space.
    fn rules(
        &self,
    ) -> (
        Vec<crate::pdf_tables::PageRule>,
        Vec<crate::pdf_tables::PageFill>,
    ) {
        let mut rules = Vec::new();
        let mut fills = Vec::new();
        for paint in &self.paints {
            if !(paint.stroked || paint.filled) {
                continue;
            }
            let subpaths = path_subpaths(&self.operations, &paint.ops, paint.ctm)
                .into_iter()
                .filter(|subpath| !subpath.curved)
                .map(|subpath| {
                    (
                        subpath
                            .points
                            .iter()
                            .map(|point| (point.x, point.y))
                            .collect::<Vec<_>>(),
                        subpath.closed,
                    )
                })
                .collect::<Vec<_>>();
            // The stroke width in page units: the matrix's mean scale.
            let scale = (paint.ctm[0] * paint.ctm[3] - paint.ctm[1] * paint.ctm[2])
                .abs()
                .sqrt();
            crate::pdf_tables::collect_rules(
                &crate::pdf_tables::PaintedPath {
                    subpaths: &subpaths,
                    stroked: paint.stroked,
                    filled: paint.filled,
                    line_width: paint.line_width * scale,
                },
                &mut rules,
                &mut fills,
            );
        }
        (rules, fills)
    }

    /// A painted path's bounds in page space: what names it when it moves.
    fn paint_bounds(&self, paint: &PaintOp) -> Option<[f64; 4]> {
        points_bounds(
            path_subpaths(&self.operations, &paint.ops, paint.ctm)
                .iter()
                .flat_map(|subpath| subpath.points.iter())
                .map(|point| (point.x, point.y)),
        )
    }

    /// The outermost clip in `chain` that clips to a small part of the
    /// page (`page_box`): what it clips is one drawing, moved with it.
    fn local_clip(
        &self,
        chain: &Option<std::sync::Arc<ClipLink>>,
        page_box: Option<[f64; 4]>,
    ) -> Option<usize> {
        let area = |rect: [f64; 4]| (rect[2] - rect[0]).max(0.) * (rect[3] - rect[1]).max(0.);
        let mut outermost = None;
        let mut link = chain.as_deref();
        while let Some(clip) = link {
            let local = self
                .paints
                .get(clip.paint)
                .and_then(|paint| self.paint_bounds(paint))
                .is_some_and(|bounds| {
                    page_box.is_some_and(|page| area(bounds) < LOCAL_CLIP_SHARE * area(page))
                });
            if local {
                outermost = Some(clip.paint);
            }
            link = clip.parent.as_deref();
        }
        outermost
    }

    /// The drawings and images the page's own content draws: what a small
    /// clip clips is one drawing, named by the clip's bounds, with the text
    /// lines it clips; otherwise each painted path that is not only a clip,
    /// and each XObject, is one.
    fn objects(&self, page_box: Option<[f64; 4]>) -> Vec<crate::pdf_content_moves::PageObject> {
        use crate::pdf_content_moves::{ContentKind, PageObject};
        let mut objects = Vec::new();
        let mut groups = BTreeSet::new();
        for paint in &self.paints {
            if !(paint.stroked || paint.filled) || paint.clips {
                continue;
            }
            match self.local_clip(&paint.clip_chain, page_box) {
                Some(clip) => {
                    groups.insert(clip);
                }
                None => {
                    if let Some(rect) = self.paint_bounds(paint) {
                        objects.push(PageObject {
                            kind: ContentKind::Drawing,
                            rect,
                            texts: Vec::new(),
                        });
                    }
                }
            }
        }
        for draw in &self.draws {
            match self.local_clip(&draw.clip_chain, page_box) {
                Some(clip) => {
                    groups.insert(clip);
                }
                None => objects.push(PageObject {
                    kind: ContentKind::Image,
                    rect: draw.bounds,
                    texts: Vec::new(),
                }),
            }
        }
        for clip in groups {
            let Some(rect) = self.paint_bounds(&self.paints[clip]) else {
                continue;
            };
            let texts = self
                .lines
                .iter()
                .filter(|line| {
                    line.ops.first().is_some_and(|&show| {
                        self.local_clip(&self.shows[show].state.clips, page_box) == Some(clip)
                    })
                })
                .filter_map(|line| crate::pdf_tables::line_core_box(&line.public))
                .collect();
            objects.push(PageObject {
                kind: ContentKind::Drawing,
                rect,
                texts,
            });
        }
        objects
    }
}

/// A clip covering less than this share of the page clips one drawing.
const LOCAL_CLIP_SHARE: f64 = 0.5;

/// Decodes one content stream, counting its bytes against the page limit.
/// Decoded content streams and font programs, by their encoded bytes and
/// filters: a page is read again for each edit made to it.
static DECODED_STREAMS: std::sync::Mutex<DecodedStreams> = std::sync::Mutex::new(DecodedStreams {
    entries: None,
    bytes: 0,
});

/// Compressed streams smaller than this decode about as fast as they hash.
const MIN_CACHED_STREAM_BYTES: usize = 16 * 1024;
/// The most inflated bytes kept; the cache empties when it would exceed it.
const MAX_CACHED_DECODED_BYTES: usize = 256 * 1024 * 1024;

struct DecodedStreams {
    entries: Option<HashMap<(u64, u64, usize), std::sync::Arc<[u8]>>>,
    bytes: usize,
}

impl DecodedStreams {
    fn insert(&mut self, key: (u64, u64, usize), bytes: std::sync::Arc<[u8]>) {
        if bytes.len() > MAX_CACHED_DECODED_BYTES {
            return;
        }
        if self.bytes + bytes.len() > MAX_CACHED_DECODED_BYTES {
            self.entries = None;
            self.bytes = 0;
        }
        self.bytes += bytes.len();
        self.entries
            .get_or_insert_with(HashMap::new)
            .insert(key, bytes);
    }
}

impl DecodedStreams {
    fn get(&self, key: &(u64, u64, usize)) -> Option<&std::sync::Arc<[u8]>> {
        self.entries.as_ref()?.get(key)
    }
}

/// A large compressed stream's cache key: two independent hashes of its
/// filters and bytes, and its length.
fn decoded_cache_key(stream: &Stream) -> Option<(u64, u64, usize)> {
    use std::hash::{Hash as _, Hasher as _};
    if stream.content.len() < MIN_CACHED_STREAM_BYTES {
        return None;
    }
    let filters = format!(
        "{:?}{:?}",
        stream.dict.get(b"Filter").ok(),
        stream.dict.get(b"DecodeParms").ok()
    );
    let mut first = ClipHasher(0);
    let mut second = std::collections::hash_map::DefaultHasher::new();
    filters.hash(&mut first);
    filters.hash(&mut second);
    first.write(&stream.content);
    // The second hash samples the stream: it guards against the first's
    // collisions, not tampering.
    for chunk in stream.content.chunks(4096) {
        second.write(&chunk[..chunk.len().min(64)]);
    }
    Some((first.finish(), second.finish(), stream.content.len()))
}

/// `stream`'s decoded bytes, from the cache when a large stream was
/// decoded before.
fn decoded_cached(stream: &Stream, limit: usize) -> Result<std::sync::Arc<[u8]>, lopdf::Error> {
    if stream.dict.get(b"Filter").is_err() {
        return Ok(stream.content.as_slice().into());
    }
    let key = decoded_cache_key(stream);
    if let Some(key) = key
        && let Ok(cache) = DECODED_STREAMS.lock()
        && let Some(bytes) = cache.get(&key)
        && bytes.len() <= limit
    {
        return Ok(bytes.clone());
    }
    let bytes: std::sync::Arc<[u8]> = stream.decompressed_content_with_limit(limit)?.into();
    if let Some(key) = key
        && let Ok(mut cache) = DECODED_STREAMS.lock()
    {
        cache.insert(key, bytes.clone());
    }
    Ok(bytes)
}

fn decode_stream(
    stream: &Stream,
    total: &mut usize,
    limits: &TextEditLimits,
) -> Result<Vec<u8>, TextEditError> {
    let remaining = limits.max_decoded_bytes_per_page.saturating_sub(*total);
    let bytes = decoded_cached(stream, remaining)
        .map_err(|error| match error {
            lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. }) => {
                TextEditError::LimitExceeded(format!(
                    "page content exceeds {} decoded bytes",
                    limits.max_decoded_bytes_per_page
                ))
            }
            other => TextEditError::Malformed(format!("content stream cannot be decoded: {other}")),
        })?
        .to_vec();
    *total = total.saturating_add(bytes.len());
    if *total > limits.max_decoded_bytes_per_page {
        return Err(TextEditError::LimitExceeded(format!(
            "page content exceeds {} decoded bytes",
            limits.max_decoded_bytes_per_page
        )));
    }
    Ok(bytes)
}

fn decode_streams(
    document: &Document,
    content_ids: &[ObjectId],
    total: &mut usize,
    limits: &TextEditLimits,
) -> Result<Vec<Vec<u8>>, TextEditError> {
    content_ids
        .iter()
        .map(|id| decode_stream(document.get_object(*id)?.as_stream()?, total, limits))
        .collect()
}

fn numbers<const N: usize>(operands: &[Operand]) -> Option<[f64; N]> {
    if operands.len() < N {
        return None;
    }
    let mut values = [0.; N];
    for (value, operand) in values.iter_mut().zip(&operands[operands.len() - N..]) {
        let Operand::Number(number) = operand else {
            return None;
        };
        *value = *number;
    }
    Some(values)
}

/// Form XObjects drawn inside forms deeper than this are not read.
const MAX_FORM_DEPTH: usize = 8;

/// The resources a stream's operators name, and the font-key prefix of
/// the forms it is drawn through ("Fm0/").
struct Scope {
    prefix: Vec<u8>,
    resources: Dictionary,
}

/// Reads a page's operators, and those of the Form XObjects it draws, into
/// text shows.
struct Walk<'a> {
    document: &'a Document,
    limits: &'a TextEditLimits,
    fonts: BTreeMap<Vec<u8>, LoadedFont>,
    shows: Vec<ShowOp>,
    operations: Vec<ContentOp>,
    streams: Vec<Vec<u8>>,
    owners: Vec<StreamOwner>,
    actual_texts: Vec<(String, std::ops::Range<usize>)>,
    next_clip: u64,
    decoded_bytes: usize,
    /// How often each Form XObject is drawn on the page.
    form_uses: HashMap<ObjectId, usize>,
    /// Paths the page's own content streams paint (or clip with).
    paints: Vec<PaintOp>,
    /// XObjects the page's own content streams draw.
    draws: Vec<DrawOp>,
    /// The matrix in effect at each `q` of the page's own content streams.
    saves: HashMap<usize, Matrix>,
}

/// A path the page's content paints or clips with: its construction
/// operators and the operator that paints it.
#[derive(Clone, Debug)]
struct PaintOp {
    ops: Vec<usize>,
    paint: usize,
    ctm: Matrix,
    stroked: bool,
    filled: bool,
    /// It also sets the clip (`W`, `W*`).
    clips: bool,
    line_width: f64,
    /// The clips it is painted inside.
    clip_chain: Option<std::sync::Arc<ClipLink>>,
}

/// An XObject (an image or form) the page's content draws with `Do`, and
/// its bounds in page space.
#[derive(Clone, Debug)]
struct DrawOp {
    op: usize,
    ctm: Matrix,
    bounds: [f64; 4],
    /// The image XObject it draws; `None` for a form.
    image: Option<lopdf::ObjectId>,
    /// The clips it is drawn inside.
    clip_chain: Option<std::sync::Arc<ClipLink>>,
}

/// A point of a path in page space, and the operator coordinates it came
/// from: `slot` counts the operator's points (a rectangle's corners).
#[derive(Clone, Copy, Debug)]
struct PathPoint {
    op: usize,
    slot: usize,
    x: f64,
    y: f64,
}

#[derive(Clone, Debug, Default)]
struct Subpath {
    points: Vec<PathPoint>,
    closed: bool,
    curved: bool,
}

/// The subpaths `ops` construct under `ctm`, in page space.
fn path_subpaths(operations: &[ContentOp], ops: &[usize], ctm: Matrix) -> Vec<Subpath> {
    let mut subpaths: Vec<Subpath> = Vec::new();
    let point = |op: usize, slot: usize, x: f64, y: f64| {
        let (x, y) = apply(ctm, x, y);
        PathPoint { op, slot, x, y }
    };
    for &op in ops {
        let operation = &operations[op];
        let operands = &operation.operands;
        match operation.operator.as_slice() {
            b"re" => {
                if let Some([x, y, w, h]) = numbers::<4>(operands) {
                    subpaths.push(Subpath {
                        points: vec![
                            point(op, 0, x, y),
                            point(op, 1, x + w, y),
                            point(op, 2, x + w, y + h),
                            point(op, 3, x, y + h),
                        ],
                        closed: true,
                        curved: false,
                    });
                }
            }
            b"m" => {
                if let Some([x, y]) = numbers::<2>(operands) {
                    subpaths.push(Subpath {
                        points: vec![point(op, 0, x, y)],
                        ..Subpath::default()
                    });
                }
            }
            b"l" => {
                if let (Some([x, y]), Some(subpath)) = (numbers::<2>(operands), subpaths.last_mut())
                {
                    subpath.points.push(point(op, 0, x, y));
                }
            }
            b"c" => {
                if let (Some([x1, y1, x2, y2, x3, y3]), Some(subpath)) =
                    (numbers::<6>(operands), subpaths.last_mut())
                {
                    subpath.points.extend([
                        point(op, 0, x1, y1),
                        point(op, 1, x2, y2),
                        point(op, 2, x3, y3),
                    ]);
                    subpath.curved = true;
                }
            }
            b"v" | b"y" => {
                if let (Some([x1, y1, x2, y2]), Some(subpath)) =
                    (numbers::<4>(operands), subpaths.last_mut())
                {
                    subpath
                        .points
                        .extend([point(op, 0, x1, y1), point(op, 1, x2, y2)]);
                    subpath.curved = true;
                }
            }
            b"h" => {
                if let Some(subpath) = subpaths.last_mut() {
                    subpath.closed = true;
                }
            }
            _ => {}
        }
    }
    subpaths
}

/// The bounds of `points`: left, bottom, right, top.
fn points_bounds(points: impl Iterator<Item = (f64, f64)>) -> Option<[f64; 4]> {
    let mut bounds: Option<[f64; 4]> = None;
    for (x, y) in points {
        let b = bounds.get_or_insert([x, y, x, y]);
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    }
    bounds
}

impl Walk<'_> {
    /// Whether ExtGState `name` sets only default values.
    /// The marked-content identifier of property list `name` in the
    /// resources' `/Properties`.
    fn marked_content_properties_id(&self, scope: &Scope, name: &[u8]) -> Option<i64> {
        let properties = get_dict(self.document, &scope.resources, b"Properties")?;
        let list = properties
            .get(name)
            .ok()
            .and_then(|object| resolve_dict(self.document, object))?;
        match resolve(self.document, list.get(b"MCID").ok()?).ok()? {
            Object::Integer(id) => Some(*id),
            _ => None,
        }
    }

    fn neutral_graphics_state(&self, scope: &Scope, name: &[u8]) -> bool {
        let Some(states) = get_dict(self.document, &scope.resources, b"ExtGState") else {
            return false;
        };
        let Some(state) = states
            .get(name)
            .ok()
            .and_then(|object| resolve_dict(self.document, object))
        else {
            return false;
        };
        state.iter().all(|(key, value)| {
            let value = resolve(self.document, value).ok();
            match key.as_slice() {
                b"Type" => true,
                b"CA" | b"ca" => value.and_then(object_number) == Some(1.),
                b"BM" => match value {
                    Some(Object::Name(name)) => {
                        matches!(name.as_slice(), b"Normal" | b"Compatible")
                    }
                    Some(Object::Array(names)) => names.first().is_some_and(
                        |name| matches!(name, Object::Name(name) if name.as_slice() == b"Normal"),
                    ),
                    _ => false,
                },
                b"AIS" | b"OP" | b"op" => matches!(value, Some(Object::Boolean(false))),
                _ => false,
            }
        })
    }

    /// The key of font `name` in `scope`, loading it on first use.
    fn font(&mut self, scope: &Scope, name: &[u8]) -> Vec<u8> {
        let key = [scope.prefix.as_slice(), name].concat();
        if !self.fonts.contains_key(&key) {
            let mut font = match get_dict(self.document, &scope.resources, b"Font")
                .and_then(|fonts| fonts.get(name).ok())
                .and_then(|value| resolve_dict(self.document, value))
            {
                Some(font) => load_font(self.document, &key, font, self.limits),
                None => LoadedFont::unsupported(&key, "the text's font is missing"),
            };
            font.resource_name = name.to_vec();
            self.fonts.insert(key.clone(), font);
        }
        key
    }

    /// Reads the Form XObject `name` drawn from stream `parent`, with the
    /// graphics state at its `Do`.
    /// The page-space bounds of XObject `name` drawn under `ctm`: an
    /// image's unit square, or a form's bounding box.
    /// The image XObject `name` names in `scope`'s resources.
    fn xobject_image(&self, scope: &Scope, name: &[u8]) -> Option<lopdf::ObjectId> {
        let object = get_dict(self.document, &scope.resources, b"XObject")?
            .get(name)
            .ok()?
            .as_reference()
            .ok()?;
        let stream = self.document.get_object(object).ok()?.as_stream().ok()?;
        (get_name(self.document, &stream.dict, b"Subtype").as_deref() == Some(b"Image"))
            .then_some(object)
    }

    fn xobject_bounds(&self, scope: &Scope, name: &[u8], ctm: Matrix) -> Option<[f64; 4]> {
        let object = get_dict(self.document, &scope.resources, b"XObject")?
            .get(name)
            .ok()?
            .as_reference()
            .ok()?;
        let stream = self.document.get_object(object).ok()?.as_stream().ok()?;
        let numbers_of = |key: &[u8]| {
            let values = stream
                .dict
                .get(key)
                .ok()
                .and_then(|value| resolve(self.document, value).ok())?
                .as_array()
                .ok()?
                .iter()
                .map(object_number)
                .collect::<Option<Vec<_>>>()?;
            Some(values)
        };
        let (matrix, [x0, y0, x1, y1]) =
            match get_name(self.document, &stream.dict, b"Subtype").as_deref() {
                Some(b"Image") => (IDENTITY, [0., 0., 1., 1.]),
                Some(b"Form") => {
                    let matrix = numbers_of(b"Matrix")
                        .and_then(|values| <[f64; 6]>::try_from(values).ok())
                        .unwrap_or(IDENTITY);
                    let bbox =
                        numbers_of(b"BBox").and_then(|values| <[f64; 4]>::try_from(values).ok())?;
                    (matrix, bbox)
                }
                _ => return None,
            };
        let full = multiply(matrix, ctm);
        points_bounds(
            [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
                .into_iter()
                .map(|(x, y)| apply(full, x, y)),
        )
    }

    fn form(
        &mut self,
        scope: &Scope,
        name: &[u8],
        parent: usize,
        state: &GraphicsState,
        depth: usize,
    ) -> Result<(), TextEditError> {
        let Some(object) = get_dict(self.document, &scope.resources, b"XObject")
            .and_then(|xobjects| xobjects.get(name).ok())
            .and_then(|value| value.as_reference().ok())
        else {
            return Ok(());
        };
        let Ok(stream) = self.document.get_object(object).and_then(Object::as_stream) else {
            return Ok(());
        };
        if get_name(self.document, &stream.dict, b"Subtype").as_deref() != Some(b"Form")
            || depth >= MAX_FORM_DEPTH
        {
            return Ok(());
        }
        *self.form_uses.entry(object).or_default() += 1;
        let bytes = decode_stream(stream, &mut self.decoded_bytes, self.limits)?;
        let mut operations = parse_operations(std::slice::from_ref(&bytes), self.limits)?;
        if self.operations.len() + operations.len() > self.limits.max_operations_per_page {
            return Err(TextEditError::LimitExceeded(format!(
                "page content has more than {} operations",
                self.limits.max_operations_per_page
            )));
        }
        let stream_index = self.streams.len();
        for operation in &mut operations {
            operation.stream = stream_index;
        }
        let resources = get_dict(self.document, &stream.dict, b"Resources")
            .cloned()
            .unwrap_or_else(|| scope.resources.clone());
        let matrix = stream
            .dict
            .get(b"Matrix")
            .ok()
            .and_then(|value| resolve(self.document, value).ok())
            .and_then(|value| value.as_array().ok())
            .and_then(|values| {
                let values = values
                    .iter()
                    .map(object_number)
                    .collect::<Option<Vec<_>>>()?;
                <[f64; 6]>::try_from(values).ok()
            })
            .unwrap_or(IDENTITY);
        self.streams.push(bytes);
        self.owners.push(StreamOwner::Form {
            object,
            parent,
            name: name.to_vec(),
            resources: resources.clone(),
        });
        let first = self.operations.len();
        self.operations.extend(operations);
        let last = self.operations.len();
        let inner = Scope {
            prefix: [scope.prefix.as_slice(), name, b"/"].concat(),
            resources,
        };
        let mut inner_state = state.clone();
        inner_state.ctm = multiply(matrix, state.ctm);
        // A form clips to its bounding box: its text never joins a line
        // outside it.
        self.next_clip += 1;
        inner_state.clip = clip_key(&(b"form", self.next_clip, state.clip));
        self.run(first..last, &inner, inner_state, depth + 1)
    }

    fn run(
        &mut self,
        operation_range: std::ops::Range<usize>,
        scope: &Scope,
        mut state: GraphicsState,
        depth: usize,
    ) -> Result<(), TextEditError> {
        let mut stack: Vec<GraphicsState> = Vec::new();
        let mut text_matrix = IDENTITY;
        let mut line_matrix = IDENTITY;
        let mut pending_clip: Option<bool> = None;
        // The current path's construction operators, hashed for the clip
        // key, and their indices.
        let mut path = 0u64;
        let mut path_ops: Vec<usize> = Vec::new();
        // Marked content: each level's ActualText, the first show inside it
        // and its marked-content identifier.
        let mut marked: Vec<(Option<String>, usize, Option<i64>)> = Vec::new();
        for op_index in operation_range {
            // Taken while it is read, as forms add operations; put back below.
            let operation = std::mem::take(&mut self.operations[op_index]);
            let operands = &operation.operands;
            'step: {
                if operation.operator == b"Do" {
                    if let Some(Operand::Name(name)) = operands.last() {
                        if matches!(self.owners[operation.stream], StreamOwner::Page)
                            && let Some(bounds) = self.xobject_bounds(scope, name, state.ctm)
                        {
                            let image = self.xobject_image(scope, name);
                            self.draws.push(DrawOp {
                                op: op_index,
                                ctm: state.ctm,
                                bounds,
                                image,
                                clip_chain: state.clips.clone(),
                            });
                        }
                        self.form(scope, name, operation.stream, &state, depth)?;
                    }
                    break 'step;
                }
                match operation.operator.as_slice() {
                    b"q" => {
                        if stack.len() < 1_024 {
                            stack.push(state.clone());
                        }
                        if matches!(self.owners[operation.stream], StreamOwner::Page)
                            && self.saves.len() < 200_000
                        {
                            self.saves.insert(op_index, state.ctm);
                        }
                    }
                    b"Q" => {
                        if let Some(saved) = stack.pop() {
                            state = saved;
                        }
                    }
                    b"cm" => {
                        if let Some(matrix) = numbers::<6>(operands) {
                            state.ctm = multiply(matrix, state.ctm);
                        }
                    }
                    b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re" => {
                        let numbers = operands.iter().filter_map(|operand| match operand {
                            Operand::Number(value) => Some(value.to_bits()),
                            _ => None,
                        });
                        path = clip_key(&(path, operation.operator, numbers.collect::<SmallKey>()));
                        if path_ops.len() < 100_000 {
                            path_ops.push(op_index);
                        }
                    }
                    b"w" => {
                        if let Some([width]) = numbers::<1>(operands) {
                            state.line_width = width;
                        }
                    }
                    b"W" => pending_clip = Some(false),
                    b"W*" => pending_clip = Some(true),
                    b"n" | b"f" | b"F" | b"f*" | b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*" => {
                        let clips = pending_clip.is_some();
                        let clip_chain = state.clips.clone();
                        if let Some(even_odd) = pending_clip.take() {
                            let ctm = state.ctm.map(f64::to_bits);
                            state.clip = clip_key(&(state.clip, even_odd, ctm, path));
                            let bounds = points_bounds(
                                path_subpaths(&self.operations, &path_ops, state.ctm)
                                    .iter()
                                    .flat_map(|subpath| subpath.points.iter())
                                    .map(|point| (point.x, point.y)),
                            );
                            if let Some(bounds) = bounds {
                                state.clip_bounds = Some(match state.clip_bounds {
                                    Some(clip) => [
                                        clip[0].max(bounds[0]),
                                        clip[1].max(bounds[1]),
                                        clip[2].min(bounds[2]),
                                        clip[3].min(bounds[3]),
                                    ],
                                    None => bounds,
                                });
                            }
                        }
                        if matches!(self.owners[operation.stream], StreamOwner::Page)
                            && !path_ops.is_empty()
                            && self.paints.len() < 200_000
                        {
                            let operator = operation.operator.as_slice();
                            self.paints.push(PaintOp {
                                ops: std::mem::take(&mut path_ops),
                                paint: op_index,
                                ctm: state.ctm,
                                stroked: matches!(
                                    operator,
                                    b"S" | b"s" | b"B" | b"B*" | b"b" | b"b*"
                                ),
                                filled: matches!(
                                    operator,
                                    b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*"
                                ),
                                clips,
                                line_width: state.line_width,
                                clip_chain: clip_chain.clone(),
                            });
                            if clips {
                                state.clips = Some(std::sync::Arc::new(ClipLink {
                                    paint: self.paints.len() - 1,
                                    parent: clip_chain,
                                }));
                            }
                        }
                        path = 0;
                        path_ops.clear();
                    }
                    b"BT" => {
                        text_matrix = IDENTITY;
                        line_matrix = IDENTITY;
                    }
                    b"Tf" => {
                        if let (Some(Operand::Name(name)), Some([size])) =
                            (operands.first(), numbers::<1>(operands))
                        {
                            state.font = Some(self.font(scope, name));
                            state.font_size = size;
                        }
                    }
                    b"Tc" => {
                        if let Some([value]) = numbers::<1>(operands) {
                            state.char_spacing = value;
                        }
                    }
                    b"Tw" => {
                        if let Some([value]) = numbers::<1>(operands) {
                            state.word_spacing = value;
                        }
                    }
                    b"Tz" => {
                        if let Some([value]) = numbers::<1>(operands) {
                            state.horizontal_scale = value / 100.;
                        }
                    }
                    b"TL" => {
                        if let Some([value]) = numbers::<1>(operands) {
                            state.leading = value;
                        }
                    }
                    b"Ts" => {
                        if let Some([value]) = numbers::<1>(operands) {
                            state.rise = value;
                        }
                    }
                    b"Tr" => {
                        if let Some([value]) = numbers::<1>(operands) {
                            state.render_mode = value as i64;
                        }
                    }
                    b"Td" | b"TD" => {
                        if let Some([x, y]) = numbers::<2>(operands) {
                            if operation.operator == b"TD" {
                                state.leading = -y;
                            }
                            line_matrix = multiply([1., 0., 0., 1., x, y], line_matrix);
                            text_matrix = line_matrix;
                        }
                    }
                    b"Tm" => {
                        if let Some(matrix) = numbers::<6>(operands) {
                            line_matrix = matrix;
                            text_matrix = matrix;
                        }
                    }
                    b"T*" => {
                        line_matrix = multiply([1., 0., 0., 1., 0., -state.leading], line_matrix);
                        text_matrix = line_matrix;
                    }
                    b"g" | b"rg" | b"k" | b"sc" | b"scn" => {
                        state.color.fill = Some(operation_text(operands, &operation.operator));
                        if operation.operator != b"sc" && operation.operator != b"scn" {
                            state.color.fill_space = None;
                        }
                        state.color.device_fill = device_rgb(
                            operands,
                            &operation.operator,
                            state.color.fill_space.as_deref(),
                        )
                        .map(|rgb| rgb.map(f64::to_bits));
                    }
                    b"cs" => {
                        state.color.fill_space = Some(operation_text(operands, b"cs"));
                        state.color.fill = None;
                        state.color.device_fill = match operands.last() {
                            Some(Operand::Name(name))
                                if matches!(
                                    name.as_slice(),
                                    b"DeviceGray" | b"DeviceRGB" | b"DeviceCMYK"
                                ) =>
                            {
                                Some([0f64; 3].map(f64::to_bits))
                            }
                            _ => None,
                        };
                    }
                    b"G" | b"RG" | b"K" | b"SC" | b"SCN" => {
                        state.color.stroke = Some(format!("{operands:?}{:?}", operation.operator));
                        if operation.operator != b"SC" && operation.operator != b"SCN" {
                            state.color.stroke_space = None;
                        }
                    }
                    b"CS" => {
                        state.color.stroke_space = Some(format!("{operands:?}"));
                        state.color.stroke = None;
                    }
                    b"gs" => {
                        // A state that only restates the defaults (normal blend,
                        // full opacity), as Word writes before text, changes
                        // nothing.
                        let neutral = state.color.graphics_state.is_none()
                            && matches!(operands.first(), Some(Operand::Name(name))
                            if self.neutral_graphics_state(scope, name));
                        if !neutral {
                            state.color.graphics_state = Some(format!("{operands:?}"));
                        }
                    }
                    b"BMC" => marked.push((None, self.shows.len(), None)),
                    b"BDC" => {
                        let marked_content_id = match operands.last() {
                            Some(Operand::Dict(entries)) => marked_content_id(entries),
                            Some(Operand::Name(name)) => {
                                self.marked_content_properties_id(scope, name)
                            }
                            _ => None,
                        };
                        let actual_text = match operands.last() {
                            Some(Operand::Dict(entries)) => {
                                entries.iter().find_map(|(key, value)| {
                                    match (key.as_slice(), value) {
                                        (b"ActualText", Operand::Str(bytes)) => {
                                            Some(decode_text_string(bytes))
                                        }
                                        _ => None,
                                    }
                                })
                            }
                            _ => None,
                        };
                        marked.push((actual_text, self.shows.len(), marked_content_id));
                    }
                    b"EMC" => {
                        if let Some((Some(text), first_show, _)) = marked.pop() {
                            self.actual_texts.push((text, first_show..self.shows.len()));
                        }
                    }
                    b"Tj" | b"TJ" | b"'" | b"\"" => {
                        let kind = match operation.operator.as_slice() {
                            b"Tj" => ShowKind::Show,
                            b"TJ" => ShowKind::ShowArray,
                            b"'" => ShowKind::NextLineShow,
                            _ => {
                                let Some([word, character]) = (operands.len() >= 3)
                                    .then(|| numbers::<2>(&operands[..2]))
                                    .flatten()
                                else {
                                    break 'step;
                                };
                                ShowKind::SpacedNextLineShow { word, character }
                            }
                        };
                        if let ShowKind::SpacedNextLineShow { word, character } = kind {
                            state.word_spacing = word;
                            state.char_spacing = character;
                        }
                        if matches!(
                            kind,
                            ShowKind::NextLineShow | ShowKind::SpacedNextLineShow { .. }
                        ) {
                            line_matrix =
                                multiply([1., 0., 0., 1., 0., -state.leading], line_matrix);
                            text_matrix = line_matrix;
                        }
                        let space = multiply(text_matrix, state.ctm);
                        let starting_text_matrix = text_matrix;
                        let font = state.font.as_ref().and_then(|name| self.fonts.get(name));
                        let (glyphs, has_string, advance) =
                            shown_glyphs(operands, kind, &state, font);
                        text_matrix = multiply([1., 0., 0., 1., advance, 0.], text_matrix);
                        let actual_text_depth = marked
                            .iter()
                            .position(|(text, _, _)| text.is_some())
                            .map_or(0, |outermost| marked.len() - outermost);
                        // A form's identifiers belong to the form's own
                        // structure parents, not the page's.
                        let marked_content_id =
                            matches!(self.owners[operation.stream], StreamOwner::Page)
                                .then(|| marked.iter().rev().find_map(|(_, _, id)| *id))
                                .flatten();
                        self.shows.push(ShowOp {
                            op_index,
                            kind,
                            state: state.clone(),
                            text_matrix: starting_text_matrix,
                            line_matrix,
                            space,
                            glyphs,
                            has_string,
                            advance,
                            actual_text_depth,
                            marked_content_id,
                        });
                    }
                    _ => {}
                }
            }
            self.operations[op_index] = operation;
        }
        Ok(())
    }
}

/// The `/MCID` of an inline marked-content property list.
fn marked_content_id(entries: &[(Vec<u8>, Operand)]) -> Option<i64> {
    entries
        .iter()
        .find_map(|(key, value)| match (key.as_slice(), value) {
            (b"MCID", Operand::Number(id)) if id.fract() == 0. && *id >= 0. => Some(*id as i64),
            _ => None,
        })
}

/// A path operator's numbers, hashed without allocating: operators have
/// at most six.
#[derive(Hash)]
struct SmallKey([u64; 6], usize);

impl FromIterator<u64> for SmallKey {
    fn from_iter<I: IntoIterator<Item = u64>>(numbers: I) -> Self {
        let mut key = Self([0; 6], 0);
        for number in numbers {
            if key.1 < key.0.len() {
                key.0[key.1] = number;
            }
            key.1 += 1;
        }
        key
    }
}

fn extract_page(
    document: &Document,
    page_index: u32,
    limits: &TextEditLimits,
) -> Result<ExtractedPage, TextEditError> {
    let page_id = page_id(document, page_index)?;
    let content_ids = page_content_ids(document, page_id)?;
    let mut decoded_bytes = 0;
    let streams = decode_streams(document, &content_ids, &mut decoded_bytes, limits)?;
    let operations = parse_operations(&streams, limits)?;
    let resources = page_resources(document, page_id).unwrap_or_default();
    let mut walk = Walk {
        document,
        limits,
        fonts: BTreeMap::new(),
        shows: Vec::new(),
        owners: vec![StreamOwner::Page; streams.len()],
        operations,
        streams,
        actual_texts: Vec::new(),
        next_clip: 0,
        decoded_bytes,
        form_uses: HashMap::new(),
        paints: Vec::new(),
        draws: Vec::new(),
        saves: HashMap::new(),
    };
    let page_operations = 0..walk.operations.len();
    walk.run(
        page_operations,
        &Scope {
            prefix: Vec::new(),
            resources,
        },
        GraphicsState::default(),
        0,
    )?;
    let Walk {
        mut fonts,
        mut shows,
        operations,
        streams,
        owners,
        actual_texts,
        form_uses,
        paints,
        draws,
        saves,
        ..
    } = walk;

    for (text, range) in actual_texts {
        let mut drawing = range.filter(|index| !shows[*index].glyphs.is_empty());
        if let (Some(index), None) = (drawing.next(), drawing.next()) {
            apply_actual_text(&mut shows[index].glyphs, &text);
            // A lone glyph read as one character teaches the font that code.
            let show = &shows[index];
            if let ([glyph], Some(font)) = (
                show.glyphs.as_slice(),
                show.state
                    .font
                    .as_ref()
                    .and_then(|name| fonts.get_mut(name)),
            ) && let Some(unicode) = &glyph.unicode
                && let (Some(character), None) = {
                    let mut characters = unicode.chars();
                    (characters.next(), characters.next())
                }
                && glyph.actual_text
                && font.unsupported.is_none()
            {
                let code = glyph
                    .code
                    .iter()
                    .fold(0u32, |code, byte| (code << 8) | u32::from(*byte));
                if !font.decode.contains_key(&code) {
                    font.learned.entry(character).or_insert(code);
                }
            }
        }
    }
    // A Unicode-CMap font draws the characters its codes spell, so any
    // character the page draws in it can be typed again.
    let mut drawn = BTreeSet::new();
    for show in &shows {
        let Some(font) = show
            .state
            .font
            .as_ref()
            .and_then(|name| fonts.get_mut(name))
        else {
            continue;
        };
        if font.cmap.is_none() || font.unsupported.is_some() {
            continue;
        }
        for glyph in show.glyphs.iter().filter(|glyph| !glyph.actual_text) {
            let code = glyph
                .code
                .iter()
                .fold(0u32, |code, byte| (code << 8) | u32::from(*byte));
            if let Some(character) = crate::pdf_cmaps::utf16_character(code)
                && !font.encode.contains_key(&character)
            {
                font.encode.insert(character, code);
                drawn.insert(font.metrics.font.clone());
            }
        }
    }
    for font in fonts.values_mut() {
        if !font.learned.is_empty() || drawn.contains(&font.metrics.font) {
            font.refresh_metrics();
        }
    }
    let mut lines = group_lines(&operations, &fonts, &shows, limits)?;
    adopt_line_fallback_families(&mut fonts, &lines);
    // A form drawn more than once on the page (directly or through an
    // enclosing form) draws its text in each place: editing one would
    // change all.
    let repeated = |mut stream: usize| loop {
        match &owners[stream] {
            StreamOwner::Page => break false,
            StreamOwner::Form { object, parent, .. } => {
                if form_uses.get(object).copied().unwrap_or(0) > 1 {
                    break true;
                }
                stream = *parent;
            }
        }
    };
    for line in &mut lines {
        let stream = operations[shows[line.ops[0]].op_index].stream;
        if line.public.read_only.is_none() && repeated(stream) {
            line.public.read_only =
                Some("the text is in a form drawn more than once on this page".into());
        }
        line.public.in_form = matches!(owners[stream], StreamOwner::Form { .. });
        line.public.clip_bounds = shows[line.ops[0]].state.clip_bounds;
    }
    Ok(ExtractedPage {
        page_id,
        content_ids,
        streams,
        owners,
        operations,
        fonts,
        shows,
        lines,
        paints,
        draws,
        saves,
    })
}

fn decode_text_string(bytes: &[u8]) -> String {
    lopdf::decode_text_string(&Object::String(
        bytes.to_vec(),
        lopdf::StringFormat::Literal,
    ))
    .unwrap_or_default()
}

/// Assigns a span's `/ActualText` to the glyphs of the one operator inside
/// it. The span's text is authoritative: glyphs whose own mapping agrees with
/// it from either end keep their text, and the glyphs between take the rest
/// (a ligature its letters; the extra glyphs of a cluster nothing). Producers
/// such as LibreOffice map a ligature to the text of its first use, so its
/// mapping can disagree with a later span.
fn apply_actual_text(glyphs: &mut [ShownGlyph], text: &str) {
    if text.is_empty() || text.contains('\u{fffd}') || glyphs.is_empty() {
        return;
    }
    let mut rest = text;
    let mut start = 0;
    while let Some(mapped) = glyphs.get(start).and_then(|glyph| glyph.unicode.as_deref())
        && !mapped.is_empty()
        && rest.starts_with(mapped)
    {
        rest = &rest[mapped.len()..];
        start += 1;
    }
    let mut end = glyphs.len();
    while end > start
        && let Some(mapped) = glyphs[end - 1].unicode.as_deref()
        && !mapped.is_empty()
        && rest.ends_with(mapped)
    {
        rest = &rest[..rest.len() - mapped.len()];
        end -= 1;
    }
    let middle = &mut glyphs[start..end];
    let assign = |glyph: &mut ShownGlyph, text: String| {
        if glyph.unicode.as_ref() != Some(&text) {
            glyph.unicode = Some(text);
            glyph.actual_text = true;
        }
    };
    match middle {
        [] => {}
        [glyph] => assign(glyph, rest.to_owned()),
        glyphs if rest.chars().count() == glyphs.len() => {
            for (glyph, character) in glyphs.iter_mut().zip(rest.chars()) {
                assign(glyph, character.to_string());
            }
        }
        [first, others @ ..] => {
            assign(first, rest.to_owned());
            for glyph in others {
                assign(glyph, String::new());
            }
        }
    }
}

/// A font whose family is unknown takes the family most of the known fonts
/// on its lines have, so a bold run of a serif paragraph falls back to the
/// serif face.
fn adopt_line_fallback_families(
    fonts: &mut BTreeMap<Vec<u8>, LoadedFont>,
    lines: &[ExtractedLine],
) {
    let unknown = fonts
        .iter()
        .filter(|(_, font)| !font.family_known && font.unsupported.is_none())
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for name in unknown {
        let mut votes: BTreeMap<FallbackFamily, usize> = BTreeMap::new();
        for line in lines
            .iter()
            .filter(|line| line.public.styles.iter().any(|style| style.font == name))
        {
            for style in &line.public.styles {
                if let Some(font) = fonts.get(&style.font)
                    && font.family_known
                {
                    *votes.entry(font.metrics.fallback.family).or_default() += 1;
                }
            }
        }
        if let Some((family, _)) = votes.into_iter().max_by_key(|(_, count)| *count)
            && let Some(font) = fonts.get_mut(&name)
        {
            font.metrics.fallback.family = family;
        }
    }
}

fn shown_glyphs(
    operands: &[Operand],
    kind: ShowKind,
    state: &GraphicsState,
    font: Option<&LoadedFont>,
) -> (Vec<ShownGlyph>, bool, f64) {
    let items: Vec<&Operand> = match (kind, operands.last()) {
        (ShowKind::ShowArray, Some(Operand::Array(items))) => items.iter().collect(),
        (_, Some(operand @ Operand::Str(_))) => vec![operand],
        _ => Vec::new(),
    };
    let mut glyphs = Vec::new();
    let mut has_string = false;
    let mut x = 0.;
    let scale = state.horizontal_scale;
    for item in items {
        match item {
            Operand::Number(adjustment) => x -= adjustment / 1000. * state.font_size * scale,
            Operand::Str(bytes) => {
                has_string = true;
                let Some(font) = font else {
                    continue;
                };
                for (code, code_bytes) in font.codes(bytes) {
                    let word_spacing = if font.simple && code == 32 {
                        state.word_spacing
                    } else {
                        0.
                    };
                    let advance = (font.width(code) / 1000. * state.font_size
                        + state.char_spacing
                        + word_spacing)
                        * scale;
                    glyphs.push(ShownGlyph {
                        code: code_bytes,
                        unicode: font.decode_code(code),
                        actual_text: false,
                        x,
                        advance,
                    });
                    x += advance;
                }
            }
            _ => {}
        }
    }
    (glyphs, has_string, x)
}

/// A colour state's fill as DeviceRGB, when it is a device colour.
fn device_fill(color: &ColorState) -> Option<[f64; 3]> {
    match (&color.fill_space, &color.fill) {
        (None, None) => Some([0.; 3]),
        _ => color.device_fill.map(|rgb| rgb.map(f64::from_bits)),
    }
}

/// The fill of `show`'s text in a line begun by `first`, as a run's
/// `fill`: its device colour, or `None` for `first`'s own colour when that
/// is not a device colour. The outer `None` when the two cannot share a
/// line: the writer sets device colours and then restores `first`'s.
/// Text that is only filled (render mode 0) does not show its stroke
/// colour, so a run may differ in that too: a word processor sets both
/// for a word in another colour.
fn run_fill(first: &ShowOp, show: &ShowOp) -> Option<Option<[f64; 3]>> {
    let (first_color, color) = (&first.state.color, &show.state.color);
    if color == first_color {
        return Some(device_fill(color));
    }
    let restorable = first_color
        .fill_space
        .iter()
        .chain(&first_color.fill)
        .all(|part| !part.starts_with('?'));
    let device = device_fill(color)?;
    let filled_only = first.state.render_mode == 0 && show.state.render_mode == 0;
    (restorable
        && (filled_only
            || color.stroke_space == first_color.stroke_space
                && color.stroke == first_color.stroke)
        && color.graphics_state == first_color.graphics_state)
        .then_some(Some(device))
}

fn read_only_reason(
    operations: &[ContentOp],
    fonts: &BTreeMap<Vec<u8>, LoadedFont>,
    show: &ShowOp,
) -> Option<String> {
    if operations[show.op_index].spans_streams {
        return Some("the text operator spans content streams".into());
    }
    if !(0..=2).contains(&show.state.render_mode) {
        return Some("invisible or clipping text is not editable".into());
    }
    if show.state.font_size == 0. || show.state.horizontal_scale == 0. {
        return Some("the text has no size".into());
    }
    let Some(font) = show.state.font.as_ref().and_then(|name| fonts.get(name)) else {
        return Some("the text's font is missing".into());
    };
    if let Some(reason) = &font.unsupported {
        return Some(reason.clone());
    }
    if show.glyphs.iter().any(|glyph| glyph.unicode.is_none()) {
        return Some("the text has characters without a Unicode mapping".into());
    }
    // Lines are laid out left to right.
    if show
        .glyphs
        .iter()
        .filter_map(|glyph| glyph.unicode.as_deref())
        .flat_map(str::chars)
        .any(right_to_left)
    {
        return Some("right-to-left text is not editable".into());
    }
    None
}

/// Whether `character` is written right to left: Hebrew, Arabic, Syriac,
/// Thaana, N'Ko and their presentation forms.
fn right_to_left(character: char) -> bool {
    matches!(
        u32::from(character),
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF
    )
}

/// A key for a clip built from `parts` (the enclosing clip, the path, its
/// matrix and fill rule).
fn clip_key(parts: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher as _;
    let mut hasher = ClipHasher(0);
    parts.hash(&mut hasher);
    hasher.finish()
}

/// A fast multiplicative hash (FxHash's): clip keys only tell a page's
/// clips apart, and a page has millions of path operators to hash.
struct ClipHasher(u64);

impl std::hash::Hasher for ClipHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0.rotate_left(5) ^ value).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Whether `text` is only a list marker.
fn is_list_marker(text: &str) -> bool {
    let marker = text.trim();
    let chars = format!("{marker} ").chars().collect::<Vec<_>>();
    !marker.is_empty()
        && crate::pdf_text_blocks::item_marker(&chars).is_some_and(|(_, end)| end == chars.len())
}

fn group_lines(
    operations: &[ContentOp],
    fonts: &BTreeMap<Vec<u8>, LoadedFont>,
    shows: &[ShowOp],
    limits: &TextEditLimits,
) -> Result<Vec<ExtractedLine>, TextEditError> {
    struct Building {
        first: usize,
        members: Vec<usize>,
        /// End of the last member in the first member's text space.
        end_x: f64,
        inverse: Matrix,
        /// Its text so far.
        text: String,
    }
    let shown_text = |show: &ShowOp| {
        show.glyphs
            .iter()
            .filter_map(|glyph| glyph.unicode.as_deref())
            .collect::<String>()
    };
    let mut groups: Vec<Building> = Vec::new();
    let mut current: Option<Building> = None;
    for (show_index, show) in shows.iter().enumerate() {
        if show.glyphs.is_empty() {
            // Moves and empty strings never join a line. An empty string is
            // kept as an anchor so an emptied line can be edited again.
            if show.has_string {
                if let Some(building) = current.take() {
                    groups.push(building);
                }
                groups.push(Building {
                    first: show_index,
                    members: vec![show_index],
                    end_x: 0.,
                    inverse: IDENTITY,
                    text: String::new(),
                });
            }
            continue;
        }
        if let Some(building) = current.as_mut() {
            let first = &shows[building.first];
            let em = first.state.font_size.abs().max(show.state.font_size.abs());
            let (x, y) = apply(building.inverse, show.space[4], show.space[5]);
            // A list marker is followed by a tab: its text may start further
            // away than a word.
            let glyph_bullet =
                matches!(building.text.trim(), "o" | "§") && show.state.font != first.state.font;
            let max_gap = if glyph_bullet || is_list_marker(&building.text) {
                MAX_MARKER_GAP_EM
            } else {
                MAX_MERGE_GAP_EM
            };
            let compatible = read_only_reason(operations, fonts, first).is_none()
                && read_only_reason(operations, fonts, show).is_none()
                && show.state.render_mode == first.state.render_mode
                && run_fill(first, show).is_some()
                && show.state.clip == first.state.clip
                && show.state.horizontal_scale == first.state.horizontal_scale
                && show.state.rise == first.state.rise
                && !matches!(
                    show.kind,
                    ShowKind::NextLineShow | ShowKind::SpacedNextLineShow { .. }
                )
                && same_linear(show.space, first.space)
                && y.abs() <= BASELINE_TOLERANCE_EM * em
                && x - building.end_x >= MIN_MERGE_GAP_EM * em * first.state.horizontal_scale.abs()
                && x - building.end_x <= max_gap * em * first.state.horizontal_scale.abs();
            if compatible {
                building.members.push(show_index);
                building.end_x = x + show.advance;
                building.text.push_str(&shown_text(show));
                continue;
            }
            groups.push(current.take().expect("current line exists"));
        }
        let inverse = invert(show.space).unwrap_or(IDENTITY);
        current = Some(Building {
            first: show_index,
            members: vec![show_index],
            end_x: show.advance,
            inverse,
            text: shown_text(show),
        });
    }
    if let Some(building) = current.take() {
        groups.push(building);
    }
    if groups.len() > limits.max_lines_per_page {
        return Err(TextEditError::LimitExceeded(format!(
            "page has more than {} text lines",
            limits.max_lines_per_page
        )));
    }

    let mut lines = Vec::with_capacity(groups.len());
    let mut anchor_counts: HashMap<(i64, i64), u32> = HashMap::new();
    for group in groups {
        let first = &shows[group.first];
        let mut styles: Vec<TextRunStyle> = Vec::new();
        let mut chars = Vec::new();
        let mut clusters = Vec::new();
        let mut text = String::new();
        let mut read_only = read_only_reason(operations, fonts, first);
        let mut ascent = 0f64;
        let mut descent = 0f64;
        let mut previous_end: Option<f64> = None;
        for &member in &group.members {
            let show = &shows[member];
            if read_only.is_none() {
                read_only = read_only_reason(operations, fonts, show);
            }
            let style = TextRunStyle {
                font: show.state.font.clone().unwrap_or_default(),
                font_size: show.state.font_size,
                char_spacing: show.state.char_spacing,
                word_spacing: show.state.word_spacing,
                fill: run_fill(first, show).flatten(),
                face: None,
            };
            let style_index = match styles.iter().position(|existing| existing == &style) {
                Some(index) => index,
                None => {
                    styles.push(style);
                    styles.len() - 1
                }
            };
            if let Some(font) = show.state.font.as_ref().and_then(|name| fonts.get(name)) {
                ascent = ascent.max(font.metrics.ascent / 1000. * show.state.font_size.abs());
                descent = descent.min(font.metrics.descent / 1000. * show.state.font_size.abs());
            }
            let (offset, _) = apply(group.inverse, show.space[4], show.space[5]);
            let offset = if member == group.first { 0. } else { offset };
            let em = show.state.font_size.abs() * show.state.horizontal_scale.abs();
            for (glyph_index, glyph) in show.glyphs.iter().enumerate() {
                let x = offset + glyph.x;
                if glyph.unicode.as_deref() == Some("") {
                    if !extend_previous_cluster(
                        &mut chars,
                        &mut clusters,
                        member,
                        glyph_index,
                        x + glyph.advance,
                    ) {
                        read_only = Some("the text has glyphs without characters".into());
                    }
                    previous_end = Some(x + glyph.advance);
                    continue;
                }
                if let Some(end) = previous_end {
                    let gap = x - end;
                    let last_is_space = text.ends_with(char::is_whitespace);
                    let next_is_space = glyph
                        .unicode
                        .as_deref()
                        .is_some_and(|unicode| unicode.starts_with(char::is_whitespace));
                    if gap > SYNTHETIC_SPACE_GAP_EM * em && !last_is_space && !next_is_space {
                        text.push(' ');
                        chars.push(TextLineChar {
                            character: ' ',
                            x: end,
                            advance: gap,
                            style: style_index,
                            synthetic: true,
                        });
                        clusters.push(None);
                    }
                }
                let unicode = glyph.unicode.clone().unwrap_or_else(|| "\u{fffd}".into());
                let parts = unicode.chars().count().max(1);
                for (part, character) in unicode.chars().enumerate() {
                    text.push(character);
                    chars.push(TextLineChar {
                        character,
                        x: x + glyph.advance * part as f64 / parts as f64,
                        advance: glyph.advance / parts as f64,
                        style: style_index,
                        synthetic: false,
                    });
                    clusters.push(Some(Cluster {
                        show: member,
                        first_glyph: glyph_index,
                        last_glyph: glyph_index,
                        part,
                        parts,
                    }));
                }
                previous_end = Some(x + glyph.advance);
            }
            if chars.len() > limits.max_chars_per_line {
                read_only = Some("the line is too long to edit".into());
            }
        }
        if ascent == 0. && descent == 0. {
            ascent = 0.8 * first.state.font_size.abs();
            descent = -0.2 * first.state.font_size.abs();
        }
        // Some fonts declare extents far beyond their letters (Cambria
        // Math: over two ems below the baseline); the line's box stays the
        // height of text.
        let size = group
            .members
            .iter()
            .map(|member| shows[*member].state.font_size.abs())
            .fold(0., f64::max);
        ascent = ascent.min(MAX_ASCENT_EM * size);
        descent = descent.max(-MAX_DESCENT_EM * size);
        let (origin_x, origin_y) = apply(first.space, 0., 0.);
        let key = (
            (origin_x * 100.).round() as i64,
            (origin_y * 100.).round() as i64,
        );
        let ordinal = anchor_counts.entry(key).or_insert(0);
        let anchor = TextLineAnchor {
            x_centipoints: key.0,
            y_centipoints: key.1,
            ordinal: *ordinal,
        };
        *ordinal += 1;
        if styles.is_empty() {
            styles.push(TextRunStyle {
                font: first.state.font.clone().unwrap_or_default(),
                font_size: first.state.font_size,
                char_spacing: first.state.char_spacing,
                word_spacing: first.state.word_spacing,
                fill: device_fill(&first.state.color),
                face: None,
            });
        }
        let fill = device_fill(&first.state.color);
        let mut marked_content_ids = group
            .members
            .iter()
            .filter(|member| !shows[**member].glyphs.is_empty())
            .filter_map(|member| shows[*member].marked_content_id)
            .collect::<Vec<_>>();
        marked_content_ids.sort_unstable();
        marked_content_ids.dedup();
        lines.push(ExtractedLine {
            public: PageTextLine {
                anchor,
                text,
                chars,
                end_x: group.end_x,
                styles,
                matrix: first.space,
                horizontal_scale: first.state.horizontal_scale,
                rise: first.state.rise,
                ascent,
                descent,
                fill,
                read_only,
                in_form: false,
                clip_bounds: None,
                marked_content_ids,
                paragraph: None,
            },
            ops: group.members,
            clusters,
        });
    }
    Ok(lines)
}

/// Adds glyph `glyph_index` of `show` to the cluster of the last
/// characters, spreading them across the cluster's new width.
fn extend_previous_cluster(
    chars: &mut [TextLineChar],
    clusters: &mut [Option<Cluster>],
    show: usize,
    glyph_index: usize,
    end_x: f64,
) -> bool {
    let Some(Some(last)) = clusters.last().copied() else {
        return false;
    };
    if last.show != show || last.last_glyph + 1 != glyph_index {
        return false;
    }
    let count = last.parts;
    let start = chars.len() - count;
    let x = chars[start].x;
    let advance = (end_x - x) / count as f64;
    for (offset, (character, cluster)) in chars[start..]
        .iter_mut()
        .zip(clusters[start..].iter_mut())
        .enumerate()
    {
        character.x = x + advance * offset as f64;
        character.advance = advance;
        if let Some(cluster) = cluster {
            cluster.last_glyph = glyph_index;
        }
    }
    true
}

/// An image the page's own content draws: its XObject and its bounds in
/// page space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageImage {
    pub object: lopdf::ObjectId,
    pub rect: [f64; 4],
}

/// Reads the editable text of a page.
pub fn extract_page_text(
    document: &Document,
    page_index: u32,
    limits: &TextEditLimits,
) -> Result<PageTextLayer, TextEditError> {
    extract_page_text_and_images(document, page_index, limits).map(|(layer, _)| layer)
}

/// [`extract_page_text`], with the images the page's own content draws, in
/// drawing order.
pub fn extract_page_text_and_images(
    document: &Document,
    page_index: u32,
    limits: &TextEditLimits,
) -> Result<(PageTextLayer, Vec<PageImage>), TextEditError> {
    let page = extract_page(document, page_index, limits)?;
    let images = page
        .draws
        .iter()
        .filter_map(|draw| {
            Some(PageImage {
                object: draw.image?,
                rect: draw.bounds,
            })
        })
        .collect::<Vec<_>>();
    let (rules, fills) = page.rules();
    let objects = page.objects(visible_page_box(document, page_index));
    let paragraphs = crate::pdf_structure::page_paragraph_elements(document, page.page_id);
    let layer = PageTextLayer {
        page_index,
        fonts: page
            .fonts
            .values()
            .filter(|font| font.unsupported.is_none())
            .map(|font| font.metrics.clone())
            .collect(),
        lines: page
            .lines
            .into_iter()
            .map(|line| {
                let mut line = line.public;
                // A line in one paragraph of the structure tree.
                let mut found = line
                    .marked_content_ids
                    .iter()
                    .map(|id| paragraphs.get(id).copied());
                line.paragraph = match found.next() {
                    Some(Some(first)) if found.all(|other| other == Some(first)) => Some(first),
                    _ => None,
                };
                line
            })
            .collect(),
        page_box: visible_page_box(document, page_index),
        tagged_tables: page_id(document, page_index)
            .map(|page| crate::pdf_structure::page_tagged_tables(document, page))
            .unwrap_or_default(),
        rules,
        fills,
        objects,
        // An encrypted file can forbid changing its content (owner
        // restrictions, which open without a password).
        content_restricted: document
            .encryption_state
            .as_ref()
            .is_some_and(|state| !state.permissions().contains(lopdf::Permissions::MODIFIABLE)),
    };
    Ok((layer, images))
}

/// A page's crop box as left, bottom, right, top.
pub(crate) fn visible_page_box(document: &Document, page_index: u32) -> Option<[f64; 4]> {
    let page_id = page_id(document, page_index).ok()?;
    let view = crate::page_geometry::PageCoordinateSpace::from_lopdf_page(document, page_id)
        .ok()?
        .view_box();
    Some([view.x, view.y, view.right(), view.top()])
}

// ---------------------------------------------------------------------------
// Writing

/// An operator with number and name operands as content-stream text; other
/// operands give a `?`-prefixed description instead.
fn operation_text(operands: &[Operand], operator: &[u8]) -> String {
    let mut text = String::new();
    for operand in operands {
        match operand {
            Operand::Number(value) => text.push_str(&format_number(*value)),
            Operand::Name(name) => text.push_str(&format_name(name)),
            _ => return format!("?{operands:?}{operator:?}"),
        }
        text.push(' ');
    }
    text.push_str(&String::from_utf8_lossy(operator));
    text
}

/// The DeviceRGB colour a fill operator sets: `g`, `rg` and `k`, or `sc`
/// and `scn` in a device colour space (`space` is the `cs` in effect).
fn device_rgb(operands: &[Operand], operator: &[u8], space: Option<&str>) -> Option<[f64; 3]> {
    let device_space = space
        .is_none_or(|space| matches!(space, "/DeviceGray cs" | "/DeviceRGB cs" | "/DeviceCMYK cs"));
    let components = match operator {
        b"g" | b"rg" | b"k" => operands.len(),
        b"sc" | b"scn" if device_space => operands.len(),
        _ => return None,
    };
    let clamp = |value: f64| value.clamp(0., 1.);
    match components {
        1 => numbers::<1>(operands).map(|[gray]| [clamp(gray); 3]),
        3 => numbers::<3>(operands).map(|rgb| rgb.map(clamp)),
        4 => {
            let [c, m, y, k] = numbers::<4>(operands)?.map(clamp);
            Some([
                (1. - c) * (1. - k),
                (1. - m) * (1. - k),
                (1. - y) * (1. - k),
            ])
        }
        _ => None,
    }
}

fn format_number(value: f64) -> String {
    let mut text = String::new();
    push_number(&mut text, value);
    text
}

/// Appends `value` to four decimal places, without trailing zeros.
fn push_number(text: &mut String, value: f64) {
    use std::fmt::Write as _;
    let scaled = (value * 10_000.).round();
    if scaled.is_nan() || scaled.abs() >= 1e15 {
        let rounded = scaled / 10_000.;
        let start = text.len();
        let _ = write!(text, "{rounded:.4}");
        while text.len() > start && text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        return;
    }
    let scaled = scaled as i64;
    if scaled == 0 {
        text.push('0');
        return;
    }
    if scaled < 0 {
        text.push('-');
    }
    let magnitude = scaled.unsigned_abs();
    let _ = write!(text, "{}", magnitude / 10_000);
    let mut fraction = magnitude % 10_000;
    if fraction != 0 {
        text.push('.');
        let mut digits = 4;
        while fraction.is_multiple_of(10) {
            fraction /= 10;
            digits -= 1;
        }
        let _ = write!(text, "{fraction:0digits$}");
    }
}

fn format_matrix(matrix: Matrix) -> String {
    matrix
        .iter()
        .map(|value| format_number(*value))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_name(name: &[u8]) -> String {
    let mut text = String::from("/");
    for &byte in name {
        if byte.is_ascii_graphic() && is_regular(byte) && byte != b'#' {
            text.push(byte as char);
        } else {
            text.push_str(&format!("#{byte:02X}"));
        }
    }
    text
}

/// A text string as UTF-16BE hex with a byte-order mark.
fn format_text_string(text: &str) -> String {
    let mut bytes = vec![0xfe, 0xff];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    format_hex(&bytes)
}

fn format_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2 + 2);
    text.push('<');
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text.push('>');
    text
}

#[derive(Clone, Debug, PartialEq)]
enum GlyphFont {
    Page(Vec<u8>),
    Fallback(crate::pdf_engine::TextEditFallbackFont),
}

struct EmittedGlyph {
    font: GlyphFont,
    style: TextRunStyle,
    /// Glyphs that read back only through `/ActualText`: the span's text
    /// and an identifier shared by the glyphs of one span.
    actual_text: Option<(usize, String)>,
    code: Vec<u8>,
    x: f64,
    /// The advance a viewer gives this glyph.
    advance: f64,
}

type FallbackFonts =
    BTreeMap<crate::pdf_engine::TextEditFallbackFont, (Vec<u8>, BTreeMap<char, u16>)>;

struct PlannedEdit {
    line: usize,
    runs: Vec<TextEditRun>,
    layout: TextLineLayout,
    /// Lines added below, laid out.
    extras: Vec<(ExtraLine, TextLineLayout)>,
    /// How far a table edit moves the line, in page space.
    offset: (f64, f64),
}

/// Applies edits to the base document. Edits for different lines of a page
/// are independent; each must name a line of the base document with its
/// original text.
pub fn apply_text_edits(
    document: &mut Document,
    edits: &[PageTextEdit],
    limits: &TextEditLimits,
) -> Result<(), TextEditError> {
    apply_content_edits(document, edits, &[], &[], limits)
}

/// Applies text edits, table edits and moves to the base document, a page
/// at a time: a table edit moves its table's rules, fills, images and
/// lines, a move moves its content, and text edits name lines where the
/// base document has them.
pub fn apply_content_edits(
    document: &mut Document,
    edits: &[PageTextEdit],
    tables: &[crate::pdf_tables::PageTableEdit],
    moves: &[crate::pdf_content_moves::ContentMove],
    limits: &TextEditLimits,
) -> Result<(), TextEditError> {
    apply_page_content_edits(document, edits, tables, moves, limits)
}

fn apply_page_content_edits(
    document: &mut Document,
    edits: &[PageTextEdit],
    tables: &[crate::pdf_tables::PageTableEdit],
    moves: &[crate::pdf_content_moves::ContentMove],
    limits: &TextEditLimits,
) -> Result<(), TextEditError> {
    type PageEdits<'a> = (
        Vec<&'a PageTextEdit>,
        Vec<&'a crate::pdf_tables::PageTableEdit>,
        Vec<crate::pdf_content_moves::ContentMove>,
    );
    let mut by_page: BTreeMap<u32, PageEdits> = BTreeMap::new();
    for edit in edits {
        by_page.entry(edit.page_index).or_default().0.push(edit);
    }
    for table in tables {
        table.validate().map_err(TextEditError::Malformed)?;
        by_page.entry(table.page_index).or_default().1.push(table);
    }
    for item in moves {
        item.validate().map_err(TextEditError::Malformed)?;
        by_page
            .entry(item.page_index)
            .or_default()
            .2
            .push(item.clone());
    }
    for (page_index, (edits, tables, moves)) in by_page {
        apply_page_edits(document, page_index, &edits, &tables, &moves, limits)?;
    }
    Ok(())
}

use crate::pdf_tables::content_offset as table_offset;

fn moved(offset: (f64, f64)) -> bool {
    offset.0.abs() > 1e-6 || offset.1.abs() > 1e-6
}

pub(crate) fn apply_page_edits(
    document: &mut Document,
    page_index: u32,
    edits: &[&PageTextEdit],
    tables: &[&crate::pdf_tables::PageTableEdit],
    moves: &[crate::pdf_content_moves::ContentMove],
    limits: &TextEditLimits,
) -> Result<(), TextEditError> {
    let page = extract_page(document, page_index, limits)?;
    let layer = PageTextLayer {
        page_index,
        fonts: page
            .fonts
            .values()
            .filter(|font| font.unsupported.is_none())
            .map(|font| font.metrics.clone())
            .collect(),
        lines: page.lines.iter().map(|line| line.public.clone()).collect(),
        page_box: visible_page_box(document, page_index),
        ..Default::default()
    };
    // Where table edits and moves put each line, and which they remove.
    let line_offsets = page
        .lines
        .iter()
        .map(
            |line| match crate::pdf_tables::line_core_box(&line.public) {
                Some(rect) if !tables.is_empty() || !moves.is_empty() => {
                    crate::pdf_content_moves::line_offset(tables, moves, rect)
                }
                _ => Some((0., 0.)),
            },
        )
        .collect::<Vec<_>>();
    let mut planned = Vec::new();
    let mut seen = BTreeSet::new();
    for edit in edits {
        let line_index = page
            .lines
            .iter()
            .position(|line| line.public.anchor == edit.anchor)
            .ok_or_else(|| {
                TextEditError::Stale(format!(
                    "page {page_index} has no text line at the edited position"
                ))
            })?;
        if !seen.insert(line_index) {
            return Err(TextEditError::Stale(
                "two edits change the same text line".into(),
            ));
        }
        let line = &page.lines[line_index].public;
        let original = edit.original_text();
        if line.text != original {
            return Err(TextEditError::Stale(format!(
                "expected {original:?}, found {:?}",
                line.text
            )));
        }
        if let Some(reason) = &line.read_only {
            return Err(TextEditError::ReadOnly(reason.clone()));
        }
        let Some(offset) = line_offsets[line_index] else {
            // A deleted row or column removes the line, edited or not; the
            // line itself is planned below.
            continue;
        };
        // An unchanged line keeps its bytes.
        if edit.draws_as_original() && !moved(offset) {
            continue;
        }
        for run in &edit.runs {
            let font = page.fonts.get(&run.style.font);
            if font.is_none_or(|font| font.unsupported.is_some()) {
                return Err(TextEditError::ReadOnly(format!(
                    "font {} cannot draw new text",
                    String::from_utf8_lossy(&run.style.font)
                )));
            }
        }
        if edit
            .runs
            .iter()
            .map(|run| run.text.chars().count())
            .sum::<usize>()
            > limits.max_chars_per_line
        {
            return Err(TextEditError::LimitExceeded(
                "the edited line is too long".into(),
            ));
        }
        let extras = edit
            .layout
            .extra_lines
            .iter()
            .map(|extra| {
                let extra = drawn_blank(extra, edit, line);
                let layout = layout_extra_line(&layer, line, &extra)?;
                Ok((extra, layout))
            })
            .collect::<Result<Vec<_>, TextEditError>>()?;
        planned.push(PlannedEdit {
            line: line_index,
            runs: edit.runs.clone(),
            layout: layout_text_edit_fitted(&layer, line, &edit.runs, edit.layout.fit)?,
            extras,
            offset,
        });
    }
    // Lines a table edit or move moves or removes that no text edit
    // changes.
    for (line_index, offset) in line_offsets.iter().enumerate() {
        let removed = offset.is_none();
        let offset = offset.unwrap_or((0., 0.));
        if !(removed || moved(offset)) || (!removed && seen.contains(&line_index)) {
            continue;
        }
        let line = &page.lines[line_index].public;
        if line.text.is_empty() {
            continue;
        }
        if let Some(reason) = &line.read_only {
            return Err(TextEditError::ReadOnly(format!(
                "the text cannot move: {reason}"
            )));
        }
        if line.in_form {
            return Err(TextEditError::ReadOnly(
                "the text cannot move: it is drawn by a form".into(),
            ));
        }
        if removed {
            planned.retain(|plan| plan.line != line_index);
        }
        let runs = if removed {
            Vec::new()
        } else {
            line.original_runs()
        };
        planned.push(PlannedEdit {
            line: line_index,
            layout: layout_text_edit_fitted(&layer, line, &runs, LineFit::default())?,
            runs,
            extras: Vec::new(),
            offset,
        });
    }
    let page_box = visible_page_box(document, page_index);
    let mut table_splices = table_content_splices(&page, tables, moves, page_box)?;
    grow_clips_for_added_lines(&page, &planned, page_box, &mut table_splices);
    let mut cell_lines = table_cell_lines(&page, &layer, tables)?;
    // The structure declaring each edited table, and a marked-content
    // identifier for each cell new text is drawn in.
    let (page_rules, _) = page.rules();
    let declared = crate::pdf_tables::page_tables(&PageTextLayer {
        lines: layer.lines.clone(),
        page_box: layer.page_box,
        tagged_tables: crate::pdf_structure::page_tagged_tables(document, page.page_id),
        rules: page_rules,
        ..Default::default()
    });
    let table_tags = tables
        .iter()
        .map(|edit| {
            declared
                .iter()
                .find(|table| table.key() == edit.key())
                .and_then(|table| table.tags.clone())
        })
        .collect::<Vec<_>>();
    let mut cell_marks: BTreeMap<(usize, usize, usize), i64> = BTreeMap::new();
    if table_tags.iter().any(Option::is_some) {
        let mut next = crate::pdf_structure::next_marked_content_id(document, page.page_id);
        for line in &cell_lines {
            if let (Some(cell), Some(id)) = (line.cell, next.as_mut())
                && table_tags[cell.0].is_some()
                && !cell_marks.contains_key(&cell)
            {
                cell_marks.insert(cell, *id);
                *id += 1;
            }
        }
    }
    // Copies draw after what they copy, in its state; copied text is new
    // text.
    let (inserts, copied_lines) = content_copies(&page, &layer, moves, page_box)?;
    for (op, text) in inserts {
        let operation = &page.operations[op];
        if operation.spans_streams {
            return Err(TextEditError::ReadOnly(
                "copied content spans content streams".into(),
            ));
        }
        match table_splices.iter_mut().find(|(stream, start, end, _)| {
            (*stream, *start, *end) == (operation.stream, operation.start, operation.end)
        }) {
            Some((_, _, _, replacement)) => replacement.push_str(&text),
            None => {
                let original = &page.streams[operation.stream][operation.start..operation.end];
                table_splices.push((
                    operation.stream,
                    operation.start,
                    operation.end,
                    format!("{}{text}", String::from_utf8_lossy(original)),
                ));
            }
        }
    }
    cell_lines.extend(copied_lines);
    if planned.is_empty() && table_splices.is_empty() && cell_lines.is_empty() {
        return Ok(());
    }

    // Embed the fallback fonts the edits need.
    let mut fallback_glyphs: BTreeMap<crate::pdf_engine::TextEditFallbackFont, BTreeSet<char>> =
        BTreeMap::new();
    for plan in &planned {
        for (laid_index, character) in plan.layout.chars.iter().enumerate() {
            if character.source == LaidOutSource::Fallback
                || needs_reencoding(&page, plan, character)
                || (gap_only(&page, plan, laid_index) && gap_needs_glyph(plan, laid_index))
            {
                let style = &plan.runs[character.run].style;
                let font = &page.fonts[&style.font];
                if page_code(font, style, character.character).is_some() {
                    continue;
                }
                let (fallback, _, _) = substitute_glyph(&font.metrics, style, character.character)
                    .ok_or(TextEditError::Unencodable(character.character))?;
                fallback_glyphs
                    .entry(fallback)
                    .or_default()
                    .insert(character.character);
            }
        }
        // Added lines' text is all new.
        for (extra, layout) in &plan.extras {
            for (laid_index, character) in layout.chars.iter().enumerate() {
                let drawn = match character.source {
                    LaidOutSource::Fallback => true,
                    LaidOutSource::Gap => layout_gap_needs_glyph(&layout.chars, laid_index),
                    LaidOutSource::Font | LaidOutSource::Original(_) => false,
                };
                let style = &extra.runs[character.run].style;
                let font = &page.fonts[&style.font];
                if !drawn || page_code(font, style, character.character).is_some() {
                    continue;
                }
                let (fallback, _, _) = substitute_glyph(&font.metrics, style, character.character)
                    .ok_or(TextEditError::Unencodable(character.character))?;
                fallback_glyphs
                    .entry(fallback)
                    .or_default()
                    .insert(character.character);
            }
        }
    }
    // New cell text is all new.
    for cell_line in &cell_lines {
        for (laid_index, character) in cell_line.layout.chars.iter().enumerate() {
            let drawn = match character.source {
                LaidOutSource::Fallback => true,
                LaidOutSource::Gap => layout_gap_needs_glyph(&cell_line.layout.chars, laid_index),
                LaidOutSource::Font | LaidOutSource::Original(_) => false,
            };
            let style = &cell_line.runs[character.run].style;
            let font = &page.fonts[&style.font];
            if !drawn || page_code(font, style, character.character).is_some() {
                continue;
            }
            let (fallback, _, _) = substitute_glyph(&font.metrics, style, character.character)
                .ok_or(TextEditError::Unencodable(character.character))?;
            fallback_glyphs
                .entry(fallback)
                .or_default()
                .insert(character.character);
        }
    }
    let mut fallback_objects = Vec::new();
    for (fallback, characters) in &fallback_glyphs {
        let (font_id, cids) =
            crate::pdf_engine::embed_text_edit_fallback(document, *fallback, characters)
                .map_err(|error| TextEditError::Malformed(error.to_string()))?;
        fallback_objects.push((*fallback, font_id, cids));
    }

    // Each line is written in the stream of its first operator, with that
    // stream's resources: the page's, or a Form XObject's (`Some(stream)`).
    let scope_of = |plan: &PlannedEdit| {
        let stream = page.operations[page.shows[page.lines[plan.line].ops[0]].op_index].stream;
        match page.owners[stream] {
            StreamOwner::Page => None,
            StreamOwner::Form { .. } => Some(stream),
        }
    };
    let mut page_resources_dictionary = page_resources(document, page.page_id).unwrap_or_default();
    let mut page_resources_changed = false;
    let mut form_resources: BTreeMap<usize, Dictionary> = BTreeMap::new();
    let mut fallback_fonts: BTreeMap<Option<usize>, FallbackFonts> = BTreeMap::new();
    if !fallback_objects.is_empty() {
        let mut scopes = planned.iter().map(scope_of).collect::<BTreeSet<_>>();
        if !cell_lines.is_empty() {
            scopes.insert(None);
        }
        for scope in scopes {
            let resources = match scope {
                None => {
                    page_resources_changed = true;
                    &mut page_resources_dictionary
                }
                Some(stream) => {
                    form_resources
                        .entry(stream)
                        .or_insert_with(|| match &page.owners[stream] {
                            StreamOwner::Form { resources, .. } => resources.clone(),
                            StreamOwner::Page => Dictionary::new(),
                        })
                }
            };
            let mut font_dictionary = get_dict(document, resources, b"Font")
                .cloned()
                .unwrap_or_default();
            let mut names = FallbackFonts::new();
            for (fallback, font_id, cids) in &fallback_objects {
                let existing = font_dictionary
                    .iter()
                    .find(|(_, value)| value.as_reference().ok() == Some(*font_id))
                    .map(|(name, _)| name.clone());
                let name = existing.unwrap_or_else(|| {
                    let mut index = 1;
                    loop {
                        let candidate = format!("BPEdit{index}").into_bytes();
                        if !font_dictionary.has(&candidate) {
                            break candidate;
                        }
                        index += 1;
                    }
                });
                font_dictionary.set(name.clone(), Object::Reference(*font_id));
                names.insert(*fallback, (name, cids.clone()));
            }
            resources.set("Font", Object::Dictionary(font_dictionary));
            fallback_fonts.insert(scope, names);
        }
    }

    // Replace each edited line's operators.
    let no_fallbacks = FallbackFonts::new();
    let mut splices: BTreeMap<usize, Vec<(usize, usize, String)>> = BTreeMap::new();
    for (stream, start, end, replacement) in table_splices {
        splices
            .entry(stream)
            .or_default()
            .push((start, end, replacement));
    }
    for plan in &planned {
        let line = &page.lines[plan.line];
        let first_show = &page.shows[line.ops[0]];
        let replacement = first_operator_replacement(
            &page,
            plan,
            fallback_fonts.get(&scope_of(plan)).unwrap_or(&no_fallbacks),
        )?;
        let operation = &page.operations[first_show.op_index];
        splices.entry(operation.stream).or_default().push((
            operation.start,
            operation.end,
            replacement,
        ));
        for &member in &line.ops[1..] {
            let show = &page.shows[member];
            let operation = &page.operations[show.op_index];
            splices.entry(operation.stream).or_default().push((
                operation.start,
                operation.end,
                move_replacement(show),
            ));
        }
    }
    let spliced = |stream: usize| -> Result<Vec<u8>, TextEditError> {
        let original = &page.streams[stream];
        let Some(stream_splices) = splices.get(&stream) else {
            return Ok(original.clone());
        };
        let mut stream_splices = stream_splices.iter().collect::<Vec<_>>();
        stream_splices.sort_by_key(|(start, _, _)| *start);
        let mut bytes = Vec::with_capacity(original.len() + 256);
        let mut cursor = 0;
        for (start, end, replacement) in stream_splices {
            if *start < cursor {
                return Err(TextEditError::Malformed("overlapping text edits".into()));
            }
            bytes.extend_from_slice(&original[cursor..*start]);
            bytes.push(b' ');
            bytes.extend_from_slice(replacement.as_bytes());
            bytes.push(b' ');
            cursor = *end;
        }
        bytes.extend_from_slice(&original[cursor..]);
        Ok(bytes)
    };

    // The page's own content streams.
    let mut content_ids = page.content_ids.clone();
    for &stream in splices.keys() {
        if matches!(page.owners[stream], StreamOwner::Page) {
            let mut written = Stream::new(Dictionary::new(), spliced(stream)?);
            if limits.compress_streams {
                written.compress()?;
            }
            content_ids[stream] = document.add_object(written);
        }
    }

    // Edited Form XObjects, and the forms that draw them, become copies
    // private to this page: another page, or another `Do` of the same name,
    // keeps the original.
    let parent_of = |stream: usize| match &page.owners[stream] {
        StreamOwner::Form { parent, .. } => Some(*parent),
        StreamOwner::Page => None,
    };
    let mut copied = BTreeSet::new();
    for &stream in splices.keys().chain(form_resources.keys()) {
        let mut current = Some(stream);
        while let Some(stream) = current.filter(|stream| parent_of(*stream).is_some()) {
            copied.insert(stream);
            current = parent_of(stream);
        }
    }
    let depth = |mut stream: usize| {
        let mut depth = 0;
        while let Some(parent) = parent_of(stream) {
            depth += 1;
            stream = parent;
        }
        depth
    };
    let mut order = copied.into_iter().collect::<Vec<_>>();
    order.sort_by_key(|stream| std::cmp::Reverse(depth(*stream)));
    let mut copies: BTreeMap<usize, ObjectId> = BTreeMap::new();
    // Points `resources`' XObject entries at the copies drawn from `parent`.
    let redirect = |resources: &mut Dictionary,
                    parent: usize,
                    copies: &BTreeMap<usize, ObjectId>,
                    document: &Document| {
        let children = copies
            .iter()
            .filter_map(|(child, id)| match &page.owners[*child] {
                StreamOwner::Form {
                    parent: child_parent,
                    name,
                    ..
                } if *child_parent == parent => Some((name.clone(), *id)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if children.is_empty() {
            return false;
        }
        let mut xobjects = get_dict(document, resources, b"XObject")
            .cloned()
            .unwrap_or_default();
        for (name, id) in children {
            xobjects.set(name, Object::Reference(id));
        }
        resources.set("XObject", Object::Dictionary(xobjects));
        true
    };
    for stream in order {
        let StreamOwner::Form {
            object, resources, ..
        } = &page.owners[stream]
        else {
            continue;
        };
        let mut resources = form_resources
            .remove(&stream)
            .unwrap_or_else(|| resources.clone());
        redirect(&mut resources, stream, &copies, document);
        let mut dictionary = document.get_object(*object)?.as_stream()?.dict.clone();
        for key in [b"Filter".as_slice(), b"DecodeParms", b"Length"] {
            dictionary.remove(key);
        }
        dictionary.set("Resources", Object::Dictionary(resources));
        let mut written = Stream::new(dictionary, spliced(stream)?);
        if limits.compress_streams {
            written.compress()?;
        }
        copies.insert(stream, document.add_object(written));
    }
    // Forms drawn by the page's content streams.
    let page_children = copies
        .keys()
        .any(|stream| parent_of(*stream).is_some_and(|parent| parent_of(parent).is_none()));
    if page_children {
        let children = copies
            .iter()
            .filter_map(|(stream, id)| match &page.owners[*stream] {
                StreamOwner::Form { parent, name, .. } if parent_of(*parent).is_none() => {
                    Some((name.clone(), *id))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut xobjects = get_dict(document, &page_resources_dictionary, b"XObject")
            .cloned()
            .unwrap_or_default();
        for (name, id) in children {
            xobjects.set(name, Object::Reference(id));
        }
        page_resources_dictionary.set("XObject", Object::Dictionary(xobjects));
        page_resources_changed = true;
    }

    // New cell text is drawn after the page's content, in the default
    // graphics state: the content is wrapped in `q` and `Q`.
    if !cell_lines.is_empty() {
        let fallbacks = fallback_fonts.get(&None).unwrap_or(&no_fallbacks);
        // A declared table's new text is tagged as its cell's content.
        let mut text = String::new();
        for line in &cell_lines {
            let drawn = cell_lines_text(&page, std::slice::from_ref(line), fallbacks)?;
            match line.cell.and_then(|cell| cell_marks.get(&cell)) {
                Some(id) => text.push_str(&format!("/P <</MCID {id}>> BDC\n{drawn}\nEMC\n")),
                None => {
                    text.push_str(&drawn);
                    text.push('\n');
                }
            }
        }
        let mut open = Stream::new(Dictionary::new(), b"q\n".to_vec());
        if limits.compress_streams {
            open.compress()?;
        }
        let mut drawn = Stream::new(Dictionary::new(), format!("\nQ\n{text}\n").into_bytes());
        if limits.compress_streams {
            drawn.compress()?;
        }
        content_ids.insert(0, document.add_object(open));
        content_ids.push(document.add_object(drawn));
    }
    let contents = if content_ids.len() == 1 {
        Object::Reference(content_ids[0])
    } else {
        Object::Array(content_ids.into_iter().map(Object::Reference).collect())
    };
    let page_dictionary = document.get_object_mut(page.page_id)?.as_dict_mut()?;
    page_dictionary.set("Contents", contents);
    if page_resources_changed {
        page_dictionary.set("Resources", Object::Dictionary(page_resources_dictionary));
    }
    // The structure follows the tables' rows, columns and new text.
    for (index, (edit, tags)) in tables.iter().zip(&table_tags).enumerate() {
        let Some(tags) = tags else {
            continue;
        };
        let band = |band: &crate::pdf_tables::TableBand| match band.source {
            crate::pdf_tables::BandSource::Base(base) => crate::pdf_structure::Band::Base(base),
            crate::pdf_tables::BandSource::New(_) => crate::pdf_structure::Band::New,
        };
        let texts = cell_marks
            .iter()
            .filter(|((table, ..), _)| *table == index)
            .map(|((_, row, column), id)| (*row, *column, *id))
            .collect::<Vec<_>>();
        crate::pdf_structure::retag_table(
            document,
            page.page_id,
            tags,
            &edit.rows.iter().map(band).collect::<Vec<_>>(),
            &edit.columns.iter().map(band).collect::<Vec<_>>(),
            &texts,
        )
        .map_err(TextEditError::ReadOnly)?;
    }
    Ok(())
}

/// Whether laid-out character `laid_index` is a space drawn as a gap, with
/// no glyph.
fn gap_only(page: &ExtractedPage, plan: &PlannedEdit, laid_index: usize) -> bool {
    match plan.layout.chars[laid_index].source {
        LaidOutSource::Gap => true,
        LaidOutSource::Original(index) => page.lines[plan.line].public.chars[index].synthetic,
        LaidOutSource::Font | LaidOutSource::Fallback => false,
    }
}

/// A gap reads back as a space only between two glyphs that are not
/// whitespace; anywhere else the space needs a glyph.
fn gap_needs_glyph(plan: &PlannedEdit, laid_index: usize) -> bool {
    layout_gap_needs_glyph(&plan.layout.chars, laid_index)
}

fn layout_gap_needs_glyph(chars: &[LaidOutChar], laid_index: usize) -> bool {
    let is_space = |index: Option<usize>| {
        index
            .and_then(|index| chars.get(index))
            .is_none_or(|character| character.character.is_whitespace())
    };
    is_space(laid_index.checked_sub(1)) || is_space(Some(laid_index + 1))
}

/// Whether an unchanged character must be encoded afresh because the glyph
/// it shared (a ligature) was split by the edit.
fn needs_reencoding(page: &ExtractedPage, plan: &PlannedEdit, character: &LaidOutChar) -> bool {
    let LaidOutSource::Original(index) = character.source else {
        return false;
    };
    original_glyph(page, plan, index).is_none()
        && !page.lines[plan.line].public.chars[index].synthetic
}

/// The original glyphs to draw for original character `index`, when the
/// edit kept every character of their cluster in order.
fn original_glyph<'a>(
    page: &'a ExtractedPage,
    plan: &PlannedEdit,
    index: usize,
) -> Option<&'a [ShownGlyph]> {
    let line = &page.lines[plan.line];
    let cluster = line.clusters[index]?;
    let position = plan
        .layout
        .chars
        .iter()
        .position(|character| character.source == LaidOutSource::Original(index))?;
    let first = index.checked_sub(cluster.part)?;
    let first_position = position.checked_sub(cluster.part)?;
    for offset in 0..cluster.parts {
        let laid = plan.layout.chars.get(first_position + offset)?;
        if laid.source != LaidOutSource::Original(first + offset) {
            return None;
        }
    }
    Some(&page.shows[cluster.show].glyphs[cluster.first_glyph..=cluster.last_glyph])
}

fn first_operator_replacement(
    page: &ExtractedPage,
    plan: &PlannedEdit,
    fallback_fonts: &FallbackFonts,
) -> Result<String, TextEditError> {
    let line = &page.lines[plan.line];
    let first = &page.shows[line.ops[0]];
    let state = &first.state;
    let scale = state.horizontal_scale;
    let mut glyphs = Vec::new();
    for (laid_index, character) in plan.layout.chars.iter().enumerate() {
        let style = &plan.runs[character.run].style;
        let font = &page.fonts[&style.font];
        let encoded = |code: u32, needs_span: bool| {
            let word_spacing = if font.simple && code == 32 {
                style.word_spacing
            } else {
                0.
            };
            EmittedGlyph {
                font: GlyphFont::Page(style.font.clone()),
                style: style.clone(),
                actual_text: needs_span.then(|| (laid_index, character.character.to_string())),
                code: font.code_bytes(code),
                x: character.x,
                advance: (font.width(code) / 1000. * style.font_size
                    + style.char_spacing
                    + word_spacing)
                    * scale,
            }
        };
        if gap_only(page, plan, laid_index) {
            if gap_needs_glyph(plan, laid_index) {
                match page_code(font, style, character.character) {
                    Some((code, needs_span)) => glyphs.push(encoded(code, needs_span)),
                    None => glyphs.push(fallback_glyph(
                        font,
                        style,
                        scale,
                        character,
                        fallback_fonts,
                    )?),
                }
            }
            continue;
        }
        match character.source {
            LaidOutSource::Gap => {}
            LaidOutSource::Original(index) => {
                let part = line.clusters[index].map_or(0, |cluster| cluster.part);
                if let Some(cluster) = original_glyph(page, plan, index) {
                    if part == 0 {
                        let actual_text =
                            cluster.iter().any(|glyph| glyph.actual_text).then(|| {
                                let text = cluster
                                    .iter()
                                    .filter_map(|glyph| glyph.unicode.as_deref())
                                    .collect::<String>();
                                (laid_index, text)
                            });
                        for glyph in cluster {
                            glyphs.push(EmittedGlyph {
                                font: GlyphFont::Page(style.font.clone()),
                                style: style.clone(),
                                actual_text: actual_text.clone(),
                                code: glyph.code.clone(),
                                x: character.x + glyph.x - cluster[0].x,
                                advance: glyph.advance,
                            });
                        }
                    }
                } else if let Some((code, needs_span)) = page_code(font, style, character.character)
                {
                    glyphs.push(encoded(code, needs_span));
                } else {
                    glyphs.push(fallback_glyph(
                        font,
                        style,
                        scale,
                        character,
                        fallback_fonts,
                    )?);
                }
            }
            LaidOutSource::Font => {
                let (code, needs_span) = page_code(font, style, character.character)
                    .ok_or(TextEditError::Unencodable(character.character))?;
                glyphs.push(encoded(code, needs_span));
            }
            LaidOutSource::Fallback => {
                glyphs.push(fallback_glyph(
                    font,
                    style,
                    scale,
                    character,
                    fallback_fonts,
                )?);
            }
        }
    }

    // Added lines' glyphs: their text is all new.
    let mut extra_glyphs = Vec::new();
    for (extra, layout) in &plan.extras {
        let mut drawn = Vec::new();
        for (laid_index, character) in layout.chars.iter().enumerate() {
            let style = &extra.runs[character.run].style;
            let font = &page.fonts[&style.font];
            let gap = character.source == LaidOutSource::Gap;
            if gap && !layout_gap_needs_glyph(&layout.chars, laid_index) {
                continue;
            }
            match page_code(font, style, character.character) {
                Some((code, needs_span))
                    if !matches!(character.source, LaidOutSource::Fallback) =>
                {
                    let word_spacing = if font.simple && code == 32 {
                        style.word_spacing
                    } else {
                        0.
                    };
                    drawn.push(EmittedGlyph {
                        font: GlyphFont::Page(style.font.clone()),
                        style: style.clone(),
                        actual_text: needs_span
                            .then(|| (usize::MAX - laid_index, character.character.to_string())),
                        code: font.code_bytes(code),
                        x: character.x,
                        advance: (font.width(code) / 1000. * style.font_size
                            + style.char_spacing
                            + word_spacing)
                            * scale,
                    });
                }
                _ => drawn.push(fallback_glyph(
                    font,
                    style,
                    scale,
                    character,
                    fallback_fonts,
                )?),
            }
        }
        extra_glyphs.push((extra, drawn));
    }

    let mut output = String::new();
    // An /ActualText span around the first operator would relabel the whole
    // edited line: close it before the line and reopen plain spans after.
    for _ in 0..first.actual_text_depth {
        output.push_str("EMC ");
    }
    match first.kind {
        ShowKind::NextLineShow => output.push_str("T* "),
        ShowKind::SpacedNextLineShow { word, character } => output.push_str(&format!(
            "{} Tw {} Tc T* ",
            format_number(word),
            format_number(character)
        )),
        ShowKind::Show | ShowKind::ShowArray => {}
    }
    // A line a table edit moves is drawn from its text matrix moved by the
    // offset, in its text space.
    let shifted = moved(plan.offset);
    let text_matrix = if shifted {
        let linear = [
            first.space[0],
            first.space[1],
            first.space[2],
            first.space[3],
            0.,
            0.,
        ];
        let (tx, ty) = invert(linear)
            .map(|inverse| apply(inverse, plan.offset.0, plan.offset.1))
            .ok_or_else(|| TextEditError::ReadOnly("the line cannot be moved".into()))?;
        let moved_matrix = multiply([1., 0., 0., 1., tx, ty], first.text_matrix);
        output.push_str(&format!("{} Tm ", format_matrix(moved_matrix)));
        moved_matrix
    } else {
        first.text_matrix
    };
    // Fonts are written by their names in the stream's own resources.
    let resource_name = |key: &[u8]| {
        page.fonts
            .get(key)
            .map_or_else(|| key.to_vec(), |font| font.resource_name.clone())
    };
    let first_font = resource_name(&state.font.clone().unwrap_or_default());
    let mut current_font = (first_font.clone(), state.font_size);
    let mut current_spacing = (state.char_spacing, state.word_spacing);
    // The operators that set the line's own fill again after a chosen colour.
    let color = &state.color;
    let restore_fill = match (&color.fill_space, &color.fill) {
        (None, None) => Some("0 g".to_owned()),
        (space, fill) => {
            let parts = space.iter().chain(fill).cloned().collect::<Vec<_>>();
            (!parts.iter().any(|part| part.starts_with('?'))).then(|| parts.join(" "))
        }
    };
    let line_fill = device_fill(color);
    let colored = glyphs
        .iter()
        .chain(extra_glyphs.iter().flat_map(|(_, drawn)| drawn))
        .any(|glyph| glyph.style.fill != line_fill);
    if colored && restore_fill.is_none() {
        return Err(TextEditError::ReadOnly(
            "the line's colour cannot be set again after a colour change".into(),
        ));
    }
    let restore_fill = restore_fill.unwrap_or_default();
    let mut current_fill = line_fill;
    // Draws glyphs from pen position `pen`, setting fonts, spacing and fill
    // as they change. Returns whether it wrote a string.
    let emit = |glyphs: &[EmittedGlyph],
                pen: &mut f64,
                output: &mut String,
                current_font: &mut (Vec<u8>, f64),
                current_spacing: &mut (f64, f64),
                current_fill: &mut Option<[f64; 3]>| {
        let mut wrote_string = false;
        let mut index = 0;
        while index < glyphs.len() {
            let segment_font = glyphs[index].font.clone();
            let segment_style = glyphs[index].style.clone();
            if *current_fill != segment_style.fill {
                match segment_style.fill {
                    Some([red, green, blue]) => output.push_str(&format!(
                        "{} {} {} rg ",
                        format_number(red),
                        format_number(green),
                        format_number(blue)
                    )),
                    None => {
                        output.push_str(&restore_fill);
                        output.push(' ');
                    }
                }
                *current_fill = segment_style.fill;
            }
            let font_name = match &segment_font {
                GlyphFont::Page(name) => resource_name(name),
                GlyphFont::Fallback(fallback) => fallback_fonts[fallback].0.clone(),
            };
            if *current_font != (font_name.clone(), segment_style.font_size) {
                output.push_str(&format!(
                    "{} {} Tf ",
                    format_name(&font_name),
                    format_number(segment_style.font_size)
                ));
                *current_font = (font_name, segment_style.font_size);
            }
            if current_spacing.0 != segment_style.char_spacing {
                output.push_str(&format!(
                    "{} Tc ",
                    format_number(segment_style.char_spacing)
                ));
            }
            if current_spacing.1 != segment_style.word_spacing {
                output.push_str(&format!(
                    "{} Tw ",
                    format_number(segment_style.word_spacing)
                ));
            }
            *current_spacing = (segment_style.char_spacing, segment_style.word_spacing);
            let unit = segment_style.font_size * scale / 1000.;
            let segment_span = glyphs[index].actual_text.clone();
            if let Some((_, text)) = &segment_span {
                output.push_str(&format!(
                    "/Span <</ActualText {}>> BDC ",
                    format_text_string(text)
                ));
            }
            output.push('[');
            let mut string = Vec::new();
            while index < glyphs.len()
                && glyphs[index].font == segment_font
                && glyphs[index].style == segment_style
                && glyphs[index].actual_text == segment_span
            {
                let glyph = &glyphs[index];
                let adjustment = rounded_adjustment(*pen - glyph.x, unit);
                if adjustment != 0. {
                    if !string.is_empty() {
                        output.push_str(&format_hex(&string));
                        string.clear();
                    }
                    output.push_str(&format_number(adjustment));
                    *pen -= adjustment * unit;
                }
                string.extend_from_slice(&glyph.code);
                *pen += glyph.advance;
                index += 1;
            }
            output.push_str(&format_hex(&string));
            wrote_string = true;
            output.push_str("] TJ ");
            if segment_span.is_some() {
                output.push_str("EMC ");
            }
        }
        wrote_string
    };
    let mut pen = 0.;
    let wrote_string = emit(
        &glyphs,
        &mut pen,
        &mut output,
        &mut current_font,
        &mut current_spacing,
        &mut current_fill,
    );
    // Added lines, each on its own baseline below the line.
    for (extra, drawn) in &extra_glyphs {
        output.push_str(&format!(
            "{} Tm ",
            format_matrix(multiply(
                [
                    1.,
                    0.,
                    0.,
                    1.,
                    extra.origin_shift,
                    text_y_below(&line.public, extra.below)
                ],
                text_matrix
            ))
        ));
        let mut extra_pen = extra.origin_shift;
        emit(
            drawn,
            &mut extra_pen,
            &mut output,
            &mut current_font,
            &mut current_spacing,
            &mut current_fill,
        );
    }
    // Restore the first operator's state and end where it originally ended.
    if current_fill != line_fill {
        output.push_str(&restore_fill);
        output.push(' ');
    }
    if current_font != (first_font.clone(), state.font_size) {
        output.push_str(&format!(
            "{} {} Tf ",
            format_name(&first_font),
            format_number(state.font_size)
        ));
    }
    if current_spacing.0 != state.char_spacing {
        output.push_str(&format!("{} Tc ", format_number(state.char_spacing)));
    }
    if current_spacing.1 != state.word_spacing {
        output.push_str(&format!("{} Tw ", format_number(state.word_spacing)));
    }
    if !extra_glyphs.is_empty() || shifted {
        // Back on the line, where the operator originally left the text
        // matrix: following operators move from there.
        output.push_str(&format!("{} Tm ", format_matrix(first.line_matrix)));
        pen = 0.;
        let offset = invert(first.line_matrix).map_or(0., |inverse| {
            apply(inverse, first.text_matrix[4], first.text_matrix[5]).0
        });
        pen -= offset;
    }
    let adjustment = rounded_adjustment(pen - first.advance, state.font_size * scale / 1000.);
    if !wrote_string || adjustment != 0. {
        output.push('[');
        if !wrote_string {
            // An emptied line keeps an empty string as its anchor.
            output.push_str("<>");
        }
        if adjustment != 0. {
            output.push_str(&format_number(adjustment));
        }
        output.push_str("] TJ");
    }
    for _ in 0..first.actual_text_depth {
        output.push_str(" /Span BMC");
    }
    Ok(output)
}

/// The page font's code for `character`, unless the run draws in a chosen
/// face.
fn page_code(font: &LoadedFont, style: &TextRunStyle, character: char) -> Option<(u32, bool)> {
    if style.face.is_some() {
        None
    } else {
        font.encode_char(character)
    }
}

/// A TJ adjustment, in thousandths of the font size, that moves the pen back
/// by `distance`; zero when negligible.
fn rounded_adjustment(distance: f64, unit: f64) -> f64 {
    if unit == 0. {
        return 0.;
    }
    let adjustment = (distance / unit * 10_000.).round() / 10_000.;
    if adjustment.abs() < 0.001 {
        0.
    } else {
        adjustment
    }
}

fn fallback_glyph(
    font: &LoadedFont,
    style: &TextRunStyle,
    scale: f64,
    character: &LaidOutChar,
    fallback_fonts: &FallbackFonts,
) -> Result<EmittedGlyph, TextEditError> {
    let (fallback, _, width) = substitute_glyph(&font.metrics, style, character.character)
        .ok_or(TextEditError::Unencodable(character.character))?;
    let cid = fallback_fonts
        .get(&fallback)
        .and_then(|(_, cids)| cids.get(&character.character))
        .ok_or(TextEditError::Unencodable(character.character))?;
    Ok(EmittedGlyph {
        font: GlyphFont::Fallback(fallback),
        style: style.clone(),
        actual_text: None,
        code: cid.to_be_bytes().to_vec(),
        x: character.x,
        advance: (width / 1000. * style.font_size + style.char_spacing) * scale,
    })
}

/// A path construction operator drawn with its points at `points` (page
/// space, by slot), under `ctm`.
fn path_operator_text(
    operation: &ContentOp,
    points: &BTreeMap<usize, (f64, f64)>,
    ctm: Matrix,
) -> Option<String> {
    let inverse = invert(ctm)?;
    let user = |slot: usize| points.get(&slot).map(|(x, y)| apply(inverse, *x, *y));
    let mut text = String::with_capacity(48);
    let numbers = |values: &[f64], text: &mut String| {
        for value in values {
            push_number(text, *value);
            text.push(' ');
        }
    };
    let (slots, operator): (&[usize], &str) = match operation.operator.as_slice() {
        b"re" => {
            let (x0, y0) = user(0)?;
            let (x1, y1) = user(2)?;
            numbers(&[x0, y0, x1 - x0, y1 - y0], &mut text);
            text.push_str("re");
            return Some(text);
        }
        b"m" => (&[0], "m"),
        b"l" => (&[0], "l"),
        b"c" => (&[0, 1, 2], "c"),
        b"v" => (&[0, 1], "v"),
        b"y" => (&[0, 1], "y"),
        b"h" => return Some("h".into()),
        _ => return None,
    };
    for slot in slots {
        let (x, y) = user(*slot)?;
        numbers(&[x, y], &mut text);
    }
    text.push_str(operator);
    Some(text)
}

/// Lines added below an edited line are drawn in its clip. A rectangle that
/// clips a small part of the page around that line (as Chrome sets one
/// around each run of text) grows to take them in, or they would not show.
fn grow_clips_for_added_lines(
    page: &ExtractedPage,
    planned: &[PlannedEdit],
    page_box: Option<[f64; 4]>,
    splices: &mut Vec<(usize, usize, usize, String)>,
) {
    let area = |rect: [f64; 4]| (rect[2] - rect[0]) * (rect[3] - rect[1]);
    let union = |a: [f64; 4], b: [f64; 4]| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    };
    // Each clip to grow: the page-space rectangle it grows to, and how far
    // a table edit moves the line it clips.
    let mut grown: BTreeMap<usize, ([f64; 4], (f64, f64))> = BTreeMap::new();
    for plan in planned.iter().filter(|plan| !plan.extras.is_empty()) {
        let line = &page.lines[plan.line].public;
        let Some(needed) = plan
            .extras
            .iter()
            .filter_map(|(extra, layout)| {
                let first = layout
                    .chars
                    .iter()
                    .find(|character| !character.character.is_whitespace())?;
                let last = layout
                    .chars
                    .iter()
                    .rfind(|character| !character.character.is_whitespace())?;
                let quad = extra_line_base(line, extra.below).quad(first.x, last.x + last.advance);
                points_bounds(quad.into_iter())
            })
            .reduce(union)
        else {
            continue;
        };
        let show = &page.shows[page.lines[plan.line].ops[0]];
        let mut link = show.state.clips.as_deref();
        while let Some(clip) = link {
            link = clip.parent.as_deref();
            let paint = &page.paints[clip.paint];
            let [op] = paint.ops[..] else {
                continue;
            };
            if page.operations[op].operator != b"re" {
                continue;
            }
            let Some(bounds) = page.paint_bounds(paint) else {
                continue;
            };
            let small = page_box.is_some_and(|page| area(bounds) < LOCAL_CLIP_SHARE * area(page));
            let inside = needed[0] >= bounds[0]
                && needed[1] >= bounds[1]
                && needed[2] <= bounds[2]
                && needed[3] <= bounds[3];
            if !small || inside {
                continue;
            }
            let entry = grown.entry(clip.paint).or_insert((bounds, plan.offset));
            entry.0 = union(entry.0, needed);
        }
    }
    for (paint_index, (rect, (dx, dy))) in grown {
        let paint = &page.paints[paint_index];
        let operation = &page.operations[paint.ops[0]];
        let Some(bounds) = page.paint_bounds(paint) else {
            continue;
        };
        if operation.spans_streams {
            continue;
        }
        // Each corner goes to the grown rectangle's corner on its sides.
        let points = path_subpaths(&page.operations, &paint.ops, paint.ctm)
            .iter()
            .flat_map(|subpath| subpath.points.iter())
            .map(|point| {
                let x = if (point.x - bounds[0]).abs() <= (point.x - bounds[2]).abs() {
                    rect[0]
                } else {
                    rect[2]
                };
                let y = if (point.y - bounds[1]).abs() <= (point.y - bounds[3]).abs() {
                    rect[1]
                } else {
                    rect[3]
                };
                (point.slot, (x + dx, y + dy))
            })
            .collect::<BTreeMap<_, _>>();
        let Some(text) = path_operator_text(operation, &points, paint.ctm) else {
            continue;
        };
        // A table edit's move of the clip is in the offset.
        splices.retain(|(stream, start, end, _)| {
            (*stream, *start, *end) != (operation.stream, operation.start, operation.end)
        });
        splices.push((operation.stream, operation.start, operation.end, text));
    }
}

/// The content changes table edits and moves make to a page's paths and
/// XObjects: rules, fills and clips in a table move with its rows and
/// columns (a deleted band's collapse), new rows and columns draw copies of
/// their model's, moved paths move whole, and images and forms are moved by
/// a matrix. Each change is a stream, the byte range it replaces and its
/// replacement.
fn table_content_splices(
    page: &ExtractedPage,
    tables: &[&crate::pdf_tables::PageTableEdit],
    moves: &[crate::pdf_content_moves::ContentMove],
    page_box: Option<[f64; 4]>,
) -> Result<Vec<(usize, usize, usize, String)>, TextEditError> {
    use crate::pdf_content_moves::{ContentKind, change_of};
    let mut splices = Vec::new();
    if tables.is_empty() && moves.is_empty() {
        return Ok(splices);
    }
    let maps = tables.iter().map(|table| table.map()).collect::<Vec<_>>();
    for paint in &page.paints {
        let subpaths = path_subpaths(&page.operations, &paint.ops, paint.ctm);
        // A changed path moves (or scales) whole, by its change alone: with
        // the small clip it is drawn in, or alone; a changed small clip
        // changes itself. A removed one is not painted.
        let local = |paint: &PaintOp| {
            page.paint_bounds(paint).is_some_and(|bounds| {
                let area = |rect: [f64; 4]| (rect[2] - rect[0]) * (rect[3] - rect[1]);
                page_box.is_some_and(|page| area(bounds) < LOCAL_CLIP_SHARE * area(page))
            })
        };
        let group = page.local_clip(&paint.clip_chain, page_box);
        let changed = match group {
            Some(clip) => page.paint_bounds(&page.paints[clip]).and_then(|rect| {
                change_of(moves, ContentKind::Drawing, rect).map(|item| (item, rect))
            }),
            None if paint.clips && !local(paint) => None,
            None => page.paint_bounds(paint).and_then(|rect| {
                change_of(moves, ContentKind::Drawing, rect).map(|item| (item, rect))
            }),
        };
        if let Some((item, rect)) = changed {
            if item.removed {
                // Its clip still clips; its paths are not painted.
                if !paint.clips {
                    let operation = &page.operations[paint.paint];
                    if operation.spans_streams {
                        return Err(TextEditError::ReadOnly(
                            "the drawing spans content streams".into(),
                        ));
                    }
                    splices.push((operation.stream, operation.start, operation.end, "n".into()));
                }
                continue;
            }
            let placement = item.placement();
            let mut points: BTreeMap<usize, BTreeMap<usize, (f64, f64)>> = BTreeMap::new();
            for point in subpaths.iter().flat_map(|subpath| &subpath.points) {
                points
                    .entry(point.op)
                    .or_default()
                    .insert(point.slot, placement.map(rect, (point.x, point.y)));
            }
            for (&op, points) in &points {
                let operation = &page.operations[op];
                if operation.spans_streams {
                    return Err(TextEditError::ReadOnly(
                        "the drawing spans content streams".into(),
                    ));
                }
                let text = path_operator_text(operation, points, paint.ctm)
                    .ok_or_else(|| TextEditError::ReadOnly("the drawing cannot move".into()))?;
                splices.push((operation.stream, operation.start, operation.end, text));
            }
            continue;
        }
        if maps.is_empty() {
            continue;
        }
        // Each operator's points as drawn, and copies of whole subpaths.
        let mut moved_points: BTreeMap<usize, BTreeMap<usize, (f64, f64)>> = BTreeMap::new();
        let mut changed_ops = BTreeSet::new();
        let mut copies: Vec<(usize, BTreeMap<usize, BTreeMap<usize, (f64, f64)>>)> = Vec::new();
        for (subpath_index, subpath) in subpaths.iter().enumerate() {
            let Some(rect) = points_bounds(subpath.points.iter().map(|point| (point.x, point.y)))
            else {
                continue;
            };
            let placements = maps
                .iter()
                .map(|map| map.placement(rect))
                .collect::<Vec<_>>();
            for point in &subpath.points {
                let (mut x, mut y) = (point.x, point.y);
                for (map, placement) in maps.iter().zip(&placements) {
                    let (mx, my) = map
                        .map_point_as(placement, None, (point.x, point.y))
                        .unwrap_or((point.x, point.y));
                    x += mx - point.x;
                    y += my - point.y;
                }
                if (x - point.x).abs() > 1e-6 || (y - point.y).abs() > 1e-6 {
                    changed_ops.insert(point.op);
                }
                moved_points
                    .entry(point.op)
                    .or_default()
                    .insert(point.slot, (x, y));
            }
            // A clip is never copied: it would clip the copy's content.
            if paint.clips || !(paint.stroked || paint.filled) {
                continue;
            }
            for (map, placement) in maps.iter().zip(&placements) {
                for instance in map.instances(placement).into_iter().skip(1) {
                    let mut copy: BTreeMap<usize, BTreeMap<usize, (f64, f64)>> = BTreeMap::new();
                    for point in &subpath.points {
                        let mapped = map
                            .map_point_as(placement, Some(instance), (point.x, point.y))
                            .unwrap_or((point.x, point.y));
                        copy.entry(point.op).or_default().insert(point.slot, mapped);
                    }
                    copies.push((subpath_index, copy));
                }
            }
        }
        let stream_of = |op: usize| page.operations[op].stream;
        for &op in &changed_ops {
            let operation = &page.operations[op];
            if operation.spans_streams {
                return Err(TextEditError::ReadOnly(
                    "the table's drawing spans content streams".into(),
                ));
            }
            let text = path_operator_text(operation, &moved_points[&op], paint.ctm)
                .ok_or_else(|| TextEditError::ReadOnly("the table's drawing cannot move".into()))?;
            splices.push((stream_of(op), operation.start, operation.end, text));
        }
        if !copies.is_empty() {
            let paint_operation = &page.operations[paint.paint];
            let original =
                &page.streams[paint_operation.stream][paint_operation.start..paint_operation.end];
            let mut text = String::from_utf8_lossy(original).into_owned();
            for (subpath_index, copy) in &copies {
                // The subpath's own operators, in order.
                let ops = subpaths[*subpath_index]
                    .points
                    .iter()
                    .map(|point| point.op)
                    .collect::<BTreeSet<_>>();
                let mut constructed = Vec::new();
                for &op in paint.ops.iter().filter(|op| ops.contains(op)) {
                    constructed.push(
                        path_operator_text(&page.operations[op], &copy[&op], paint.ctm)
                            .ok_or_else(|| {
                                TextEditError::ReadOnly("the table's drawing cannot copy".into())
                            })?,
                    );
                }
                // A closing `h` follows its subpath's last point.
                if subpaths[*subpath_index].closed
                    && let Some(last) = constructed.last()
                    && !last.ends_with(" re")
                {
                    constructed.push("h".into());
                }
                text.push(' ');
                text.push_str(&constructed.join(" "));
                text.push(' ');
                text.push_str(&String::from_utf8_lossy(&paint_operation.operator));
            }
            if paint_operation.spans_streams {
                return Err(TextEditError::ReadOnly(
                    "the table's drawing spans content streams".into(),
                ));
            }
            splices.push((
                paint_operation.stream,
                paint_operation.start,
                paint_operation.end,
                text,
            ));
        }
    }
    for draw in &page.draws {
        // An image is placed by its group's change, its own, or its table.
        let changed = page
            .local_clip(&draw.clip_chain, page_box)
            .and_then(|clip| page.paint_bounds(&page.paints[clip]))
            .and_then(|rect| change_of(moves, ContentKind::Drawing, rect).map(|item| (item, rect)))
            .or_else(|| {
                change_of(moves, ContentKind::Image, draw.bounds).map(|item| (item, draw.bounds))
            });
        let matrix = match changed {
            Some((item, _)) if item.removed => None,
            Some((item, rect)) => Some(item.placement().matrix(rect)),
            None => table_offset(tables, draw.bounds).map(|(dx, dy)| [1., 0., 0., 1., dx, dy]),
        };
        let operation = &page.operations[draw.op];
        let Some(matrix) = matrix else {
            splices.push((
                operation.stream,
                operation.start,
                operation.end,
                String::new(),
            ));
            continue;
        };
        if matrix == [1., 0., 0., 1., 0., 0.]
            || matrix
                .iter()
                .zip([1., 0., 0., 1., 0., 0.])
                .all(|(a, b)| (a - b).abs() < 1e-9)
        {
            continue;
        }
        // The page-space change in the image's user space: drawn under
        // `ctm`, it is `ctm · matrix · ctm⁻¹`.
        let inverse = invert(draw.ctm)
            .ok_or_else(|| TextEditError::ReadOnly("the image cannot move".into()))?;
        let user = multiply(multiply(draw.ctm, matrix), inverse);
        let original = &page.streams[operation.stream][operation.start..operation.end];
        splices.push((
            operation.stream,
            operation.start,
            operation.end,
            format!(
                "q {} cm {} Q",
                format_matrix(user),
                String::from_utf8_lossy(original)
            ),
        ));
    }
    Ok(splices)
}

/// The `q`…`Q` block of the page's own content around operator `op`: the
/// operators of the `q` that opened it and the `Q` that closes it, in one
/// stream.
fn enclosing_block(page: &ExtractedPage, op: usize) -> Option<(usize, usize)> {
    let stream = page.operations.get(op)?.stream;
    let same = |index: usize| page.operations[index].stream == stream;
    let mut depth = 0usize;
    let mut start = None;
    for index in (0..op).rev() {
        if !same(index) {
            return None;
        }
        match page.operations[index].operator.as_slice() {
            b"Q" => depth += 1,
            b"q" if depth == 0 => {
                start = Some(index);
                break;
            }
            b"q" => depth -= 1,
            _ => {}
        }
    }
    let start = start?;
    depth = 0;
    for index in op + 1..page.operations.len() {
        if !same(index) {
            return None;
        }
        match page.operations[index].operator.as_slice() {
            b"q" => depth += 1,
            b"Q" if depth == 0 => return Some((start, index)),
            b"Q" => depth -= 1,
            _ => {}
        }
    }
    None
}

/// What copies of content (`moves` with a copy number) add to the page:
/// text written after an operator, by operator, and copied text lines.
/// A copied path or image is drawn again right after itself, in its own
/// graphics state, moved by a matrix; a copied drawing in a small clip
/// copies the whole `q`…`Q` block around its clip after that block.
#[allow(clippy::type_complexity)]
fn content_copies(
    page: &ExtractedPage,
    layer: &PageTextLayer,
    moves: &[crate::pdf_content_moves::ContentMove],
    page_box: Option<[f64; 4]>,
) -> Result<(Vec<(usize, String)>, Vec<CellLine>), TextEditError> {
    use crate::pdf_content_moves::ContentKind;
    let mut inserts: Vec<(usize, String)> = Vec::new();
    let mut lines = Vec::new();
    let bytes = |op: usize| {
        let operation = &page.operations[op];
        String::from_utf8_lossy(&page.streams[operation.stream][operation.start..operation.end])
            .into_owned()
    };
    // The page-space change `matrix` in the user space of `ctm`.
    let user = |ctm: Matrix, matrix: Matrix| -> Result<Matrix, TextEditError> {
        let inverse = invert(ctm)
            .ok_or_else(|| TextEditError::ReadOnly("the copy cannot be placed".into()))?;
        Ok(multiply(multiply(ctm, matrix), inverse))
    };
    let area = |rect: [f64; 4]| (rect[2] - rect[0]).max(0.) * (rect[3] - rect[1]).max(0.);
    let local =
        |rect: [f64; 4]| page_box.is_some_and(|page| area(rect) < LOCAL_CLIP_SHARE * area(page));
    let copies = moves
        .iter()
        .filter(|item| item.copy.is_some() && !item.removed)
        .collect::<Vec<_>>();
    for item in &copies {
        let placement = item.placement();
        let matrix = placement.matrix(item.rect);
        match item.kind {
            ContentKind::Image => {
                let Some(draw) = page.draws.iter().find(|draw| {
                    item.names(ContentKind::Image, draw.bounds)
                        && page.local_clip(&draw.clip_chain, page_box).is_none()
                }) else {
                    continue;
                };
                inserts.push((
                    draw.op,
                    format!(
                        " q {} cm {} Q",
                        format_matrix(user(draw.ctm, matrix)?),
                        bytes(draw.op)
                    ),
                ));
            }
            ContentKind::Drawing => {
                // A small clip and all it clips.
                let clip = page.paints.iter().find(|paint| {
                    paint.clips
                        && page.local_clip(&paint.clip_chain, page_box).is_none()
                        && page.paint_bounds(paint).is_some_and(|rect| {
                            local(rect) && item.names(ContentKind::Drawing, rect)
                        })
                });
                if let Some(clip) = clip {
                    let (start, end) = enclosing_block(page, clip.paint).ok_or_else(|| {
                        TextEditError::ReadOnly("the drawing cannot be copied".into())
                    })?;
                    let outer = page.saves.get(&start).copied().unwrap_or(IDENTITY);
                    let (first, last) = (&page.operations[start], &page.operations[end]);
                    let block =
                        String::from_utf8_lossy(&page.streams[first.stream][first.start..last.end])
                            .into_owned();
                    inserts.push((
                        end,
                        format!(" q {} cm {block} Q", format_matrix(user(outer, matrix)?)),
                    ));
                    continue;
                }
                let Some(paint) = page.paints.iter().find(|paint| {
                    !paint.clips
                        && (paint.stroked || paint.filled)
                        && page.local_clip(&paint.clip_chain, page_box).is_none()
                        && page
                            .paint_bounds(paint)
                            .is_some_and(|rect| item.names(ContentKind::Drawing, rect))
                }) else {
                    continue;
                };
                let path = paint
                    .ops
                    .iter()
                    .map(|&op| bytes(op))
                    .collect::<Vec<_>>()
                    .join(" ");
                inserts.push((
                    paint.paint,
                    format!(
                        " q {} cm {path} {} Q",
                        format_matrix(user(paint.ctm, matrix)?),
                        bytes(paint.paint)
                    ),
                ));
            }
            ContentKind::Text => {
                // Text a copied drawing's clip holds comes with the drawing.
                if copies
                    .iter()
                    .any(|other| other.copy == item.copy && other.kind == ContentKind::Drawing)
                {
                    continue;
                }
                let Some(line) = layer.lines.iter().find(|line| {
                    crate::pdf_tables::line_core_box(line)
                        .is_some_and(|rect| item.names(ContentKind::Text, rect))
                }) else {
                    continue;
                };
                let m = line.matrix;
                let scale = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
                let runs = normalized_runs(
                    &line
                        .original_runs()
                        .into_iter()
                        .map(|run| TextEditRun {
                            style: TextRunStyle {
                                font_size: run.style.font_size * scale,
                                char_spacing: run.style.char_spacing * scale,
                                word_spacing: run.style.word_spacing * scale,
                                fill: run.style.fill.or(line.fill),
                                ..run.style
                            },
                            text: run.text,
                        })
                        .collect::<Vec<_>>(),
                );
                if runs.iter().any(|run| {
                    page.fonts
                        .get(&run.style.font)
                        .is_none_or(|font| font.unsupported.is_some())
                        || run.style.font.contains(&b'/')
                }) {
                    return Err(TextEditError::ReadOnly(
                        "the text's font cannot draw a copy".into(),
                    ));
                }
                let origin = placement.map(item.rect, line.origin());
                let base = new_text_line(origin, &runs);
                let layout = layout_text_edit_fitted(layer, &base, &runs, LineFit::default())?;
                lines.push(CellLine {
                    origin,
                    runs,
                    layout,
                    cell: None,
                });
            }
        }
    }
    Ok((inserts, lines))
}

/// A line of new cell text, laid out where its table edit draws it.
struct CellLine {
    origin: (f64, f64),
    runs: Vec<TextEditRun>,
    layout: TextLineLayout,
    /// The edited table (its index in the page's table edits), row and
    /// column it is drawn in; `None` for copied content.
    cell: Option<(usize, usize, usize)>,
}

/// The new cell text `tables` draw, laid out at its cells in the edited
/// tables (moved down when a table above pushes content below it).
fn table_cell_lines(
    page: &ExtractedPage,
    layer: &PageTextLayer,
    tables: &[&crate::pdf_tables::PageTableEdit],
) -> Result<Vec<CellLine>, TextEditError> {
    let mut lines = Vec::new();
    for (index, table) in tables.iter().enumerate() {
        for text in &table.cell_texts {
            let Some((left, top)) = table.cell_origin(text.row, text.column) else {
                continue;
            };
            let others = tables
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, other)| *other)
                .collect::<Vec<_>>();
            let (dx, dy) =
                table_offset(&others, [left, top - 1., left + 1., top]).unwrap_or((0., 0.));
            for line in &text.lines {
                let runs = normalized_runs(&line.runs);
                if runs.iter().all(|run| run.text.is_empty()) {
                    continue;
                }
                for run in &runs {
                    let font = page.fonts.get(&run.style.font);
                    if font.is_none_or(|font| font.unsupported.is_some())
                        || run.style.font.contains(&b'/')
                    {
                        return Err(TextEditError::ReadOnly(format!(
                            "font {} cannot draw new cell text",
                            String::from_utf8_lossy(&run.style.font)
                        )));
                    }
                }
                let origin = (left + line.x + dx, top - line.baseline + dy);
                let base = new_text_line(origin, &runs);
                let layout = layout_text_edit_fitted(layer, &base, &runs, LineFit::default())?;
                lines.push(CellLine {
                    origin,
                    runs,
                    layout,
                    cell: Some((index, text.row, text.column)),
                });
            }
        }
    }
    Ok(lines)
}

/// An empty upright line at page point `origin`, in the state of `runs`'
/// first style, to lay new text out on.
fn new_text_line(origin: (f64, f64), runs: &[TextEditRun]) -> PageTextLine {
    let style = runs.first().map(|run| run.style.clone());
    PageTextLine {
        anchor: TextLineAnchor {
            x_centipoints: (origin.0 * 100.).round() as i64,
            y_centipoints: (origin.1 * 100.).round() as i64,
            ordinal: 0,
        },
        text: String::new(),
        chars: Vec::new(),
        end_x: 0.,
        styles: style.into_iter().collect(),
        matrix: [1., 0., 0., 1., origin.0, origin.1],
        horizontal_scale: 1.,
        rise: 0.,
        ascent: 0.,
        descent: 0.,
        fill: None,
        read_only: None,
        in_form: false,
        clip_bounds: None,
        marked_content_ids: Vec::new(),
        paragraph: None,
    }
}

/// New cell text as its own text object, in the page's fonts or the
/// bundled fonts that draw characters they lack.
fn cell_lines_text(
    page: &ExtractedPage,
    lines: &[CellLine],
    fallback_fonts: &FallbackFonts,
) -> Result<String, TextEditError> {
    let mut output = String::from("q BT 0 Tr 0 Ts 100 Tz ");
    for line in lines {
        output.push_str(&format!(
            "1 0 0 1 {} {} Tm ",
            format_number(line.origin.0),
            format_number(line.origin.1)
        ));
        let mut glyphs: Vec<EmittedGlyph> = Vec::new();
        for (laid_index, character) in line.layout.chars.iter().enumerate() {
            let style = &line.runs[character.run].style;
            let font = &page.fonts[&style.font];
            if character.source == LaidOutSource::Gap
                && !layout_gap_needs_glyph(&line.layout.chars, laid_index)
            {
                continue;
            }
            match page_code(font, style, character.character) {
                Some((code, needs_span))
                    if !matches!(character.source, LaidOutSource::Fallback) =>
                {
                    let word_spacing = if font.simple && code == 32 {
                        style.word_spacing
                    } else {
                        0.
                    };
                    glyphs.push(EmittedGlyph {
                        font: GlyphFont::Page(style.font.clone()),
                        style: style.clone(),
                        actual_text: needs_span
                            .then(|| (laid_index, character.character.to_string())),
                        code: font.code_bytes(code),
                        x: character.x,
                        advance: font.width(code) / 1000. * style.font_size
                            + style.char_spacing
                            + word_spacing,
                    });
                }
                _ => glyphs.push(fallback_glyph(font, style, 1., character, fallback_fonts)?),
            }
        }
        let mut pen = 0.;
        let mut index = 0;
        while index < glyphs.len() {
            let segment_font = glyphs[index].font.clone();
            let style = glyphs[index].style.clone();
            let font_name = match &segment_font {
                GlyphFont::Page(name) => page
                    .fonts
                    .get(name)
                    .map_or_else(|| name.clone(), |font| font.resource_name.clone()),
                GlyphFont::Fallback(fallback) => fallback_fonts
                    .get(fallback)
                    .map(|(name, _)| name.clone())
                    .ok_or_else(|| TextEditError::Malformed("a fallback font is missing".into()))?,
            };
            let [red, green, blue] = style.fill.unwrap_or([0.; 3]);
            output.push_str(&format!(
                "{} {} {} rg {} {} Tf {} Tc {} Tw ",
                format_number(red),
                format_number(green),
                format_number(blue),
                format_name(&font_name),
                format_number(style.font_size),
                format_number(style.char_spacing),
                format_number(style.word_spacing)
            ));
            let span = glyphs[index].actual_text.clone();
            if let Some((_, text)) = &span {
                output.push_str(&format!(
                    "/Span <</ActualText {}>> BDC ",
                    format_text_string(text)
                ));
            }
            let unit = style.font_size / 1000.;
            output.push('[');
            let mut string = Vec::new();
            while index < glyphs.len()
                && glyphs[index].font == segment_font
                && glyphs[index].style == style
                && glyphs[index].actual_text == span
            {
                let glyph = &glyphs[index];
                let adjustment = rounded_adjustment(pen - glyph.x, unit);
                if adjustment != 0. {
                    if !string.is_empty() {
                        output.push_str(&format_hex(&string));
                        string.clear();
                    }
                    output.push_str(&format_number(adjustment));
                    pen -= adjustment * unit;
                }
                string.extend_from_slice(&glyph.code);
                pen += glyph.advance;
                index += 1;
            }
            output.push_str(&format_hex(&string));
            output.push_str("] TJ ");
            if span.is_some() {
                output.push_str("EMC ");
            }
        }
    }
    output.push_str("ET Q");
    Ok(output)
}

/// A replacement that moves the text matrix as the operator did, drawing
/// nothing.
fn move_replacement(show: &ShowOp) -> String {
    let mut output = String::new();
    match show.kind {
        ShowKind::NextLineShow => output.push_str("T* "),
        ShowKind::SpacedNextLineShow { word, character } => output.push_str(&format!(
            "{} Tw {} Tc T* ",
            format_number(word),
            format_number(character)
        )),
        ShowKind::Show | ShowKind::ShowArray => {}
    }
    let unit = show.state.font_size * show.state.horizontal_scale / 1000.;
    if unit != 0. && show.advance.abs() > 1e-9 {
        output.push_str(&format!("[{}] TJ", format_number(-show.advance / unit)));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::Object;

    fn helvetica() -> Dictionary {
        dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        }
    }

    fn document(content: &[u8], fonts: Vec<(&str, Object)>) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let mut font_dictionary = Dictionary::new();
        for (name, font) in fonts {
            let id = document.add_object(font);
            font_dictionary.set(name, id);
        }
        let content_id = document.add_object(Stream::new(Dictionary::new(), content.to_vec()));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => font_dictionary },
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document
    }

    #[test]
    fn moved_text_drawings_and_images_are_saved_where_they_were_put() {
        use crate::pdf_content_moves::{ContentKind, ContentMove};
        let content = b"BT /F1 12 Tf 72 700 Td (Moved) Tj ET \
            BT /F1 12 Tf 72 600 Td (Kept) Tj ET \
            0 0 1 RG 2 w 100 500 m 200 500 l S \
            0.9 g 300 300 50 40 re f \
            q 80 0 0 60 400 100 cm /Im0 Do Q";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 1,
                "Height" => 1,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            vec![0],
        ));
        let page_id = page_id(&document, 0).unwrap();
        let page = document.get_dictionary_mut(page_id).unwrap();
        let mut resources = page.get(b"Resources").unwrap().as_dict().unwrap().clone();
        resources.set("XObject", dictionary! { "Im0" => image });
        page.set("Resources", resources);
        let before = layer(&document);
        let moved_line = before
            .lines
            .iter()
            .find(|line| line.text == "Moved")
            .unwrap();
        let line_rect = crate::pdf_tables::line_core_box(moved_line).unwrap();
        // The page's drawings and image are objects to move.
        let drawing = |rect: [f64; 4]| {
            before
                .objects
                .iter()
                .find(|object| {
                    object.kind == ContentKind::Drawing
                        && object
                            .rect
                            .iter()
                            .zip(rect)
                            .all(|(a, b)| (a - b).abs() < 0.01)
                })
                .cloned()
                .unwrap()
        };
        let stroke = drawing([100., 500., 200., 500.]);
        let fill = drawing([300., 300., 350., 340.]);
        let picture = before
            .objects
            .iter()
            .find(|object| object.kind == ContentKind::Image)
            .cloned()
            .unwrap();
        assert_eq!(picture.rect, [400., 100., 480., 160.]);
        let item = |kind, rect, offset| ContentMove {
            page_index: 0,
            kind,
            rect,
            offset,
            scale: [1., 1.],
            removed: false,
            copy: None,
        };
        let moves = [
            item(ContentKind::Text, line_rect, [10., -20.]),
            item(ContentKind::Drawing, stroke.rect, [0., 50.]),
            item(ContentKind::Drawing, fill.rect, [-30., 0.]),
            item(ContentKind::Image, picture.rect, [5., 5.]),
        ];
        apply_content_edits(&mut document, &[], &[], &moves, &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        let origin = |text: &str| {
            after
                .lines
                .iter()
                .find(|line| line.text == text)
                .unwrap()
                .origin()
        };
        let (x, y) = origin("Moved");
        assert!((x - 82.).abs() < 0.01 && (y - 680.).abs() < 0.01, "{x} {y}");
        let (x, y) = origin("Kept");
        assert!((x - 72.).abs() < 0.01 && (y - 600.).abs() < 0.01);
        let has = |kind, rect: [f64; 4]| {
            after.objects.iter().any(|object| {
                object.kind == kind
                    && object
                        .rect
                        .iter()
                        .zip(rect)
                        .all(|(a, b)| (a - b).abs() < 0.01)
            })
        };
        assert!(has(ContentKind::Drawing, [100., 550., 200., 550.]));
        assert!(has(ContentKind::Drawing, [270., 300., 320., 340.]));
        assert!(has(ContentKind::Image, [405., 105., 485., 165.]));
    }

    #[test]
    fn a_drawing_in_a_small_clip_moves_with_its_clip_and_text() {
        use crate::pdf_content_moves::{ContentKind, ContentMove};
        let content = b"q 0 0 612 792 re W n \
            q 90 480 140 40 re W n 1 0 0 RG 3 w 100 500 m 200 500 l S \
            BT /F1 10 Tf 100 505 Td (Label) Tj ET Q Q";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        // The clipped drawing and its text are one object, named by the clip.
        let drawings = before
            .objects
            .iter()
            .filter(|object| object.kind == ContentKind::Drawing)
            .collect::<Vec<_>>();
        assert_eq!(drawings.len(), 1, "{drawings:?}");
        assert_eq!(drawings[0].rect, [90., 480., 230., 520.]);
        assert_eq!(drawings[0].texts.len(), 1);
        let text_rect = drawings[0].texts[0];
        let moves = [
            ContentMove {
                page_index: 0,
                kind: ContentKind::Drawing,
                rect: drawings[0].rect,
                offset: [0., -300.],
                scale: [1., 1.],
                removed: false,
                copy: None,
            },
            ContentMove {
                page_index: 0,
                kind: ContentKind::Text,
                rect: text_rect,
                offset: [0., -300.],
                scale: [1., 1.],
                removed: false,
                copy: None,
            },
        ];
        apply_content_edits(&mut document, &[], &[], &moves, &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        let moved = after
            .objects
            .iter()
            .find(|object| object.kind == ContentKind::Drawing)
            .unwrap();
        assert_eq!(moved.rect, [90., 180., 230., 220.]);
        let line = after
            .lines
            .iter()
            .find(|line| line.text == "Label")
            .unwrap();
        assert!((line.origin().1 - 205.).abs() < 0.01);
        // The text is still inside the moved clip.
        let clip = line.clip_bounds.unwrap();
        assert!(clip[1] <= 205. && clip[3] >= 205., "{clip:?}");
    }

    #[test]
    fn resized_and_deleted_content_is_saved_so() {
        use crate::pdf_content_moves::{ContentKind, ContentMove};
        let content = b"BT /F1 12 Tf 72 700 Td (Gone) Tj ET \
            BT /F1 12 Tf 72 600 Td (Kept) Tj ET \
            0 0 1 RG 2 w 100 500 m 200 500 l S \
            0.9 g 300 300 50 40 re f \
            q 80 0 0 60 400 100 cm /Im0 Do Q \
            q 10 0 0 10 50 50 cm /Im0 Do Q";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 1,
                "Height" => 1,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            vec![0],
        ));
        let page_id = page_id(&document, 0).unwrap();
        let page = document.get_dictionary_mut(page_id).unwrap();
        let mut resources = page.get(b"Resources").unwrap().as_dict().unwrap().clone();
        resources.set("XObject", dictionary! { "Im0" => image });
        page.set("Resources", resources);
        let before = layer(&document);
        let gone = before
            .lines
            .iter()
            .find(|line| line.text == "Gone")
            .unwrap();
        let item = |kind, rect, offset, scale, removed| ContentMove {
            page_index: 0,
            kind,
            rect,
            offset,
            scale,
            removed,
            copy: None,
        };
        let moves = [
            item(
                ContentKind::Text,
                crate::pdf_tables::line_core_box(gone).unwrap(),
                [0., 0.],
                [1., 1.],
                true,
            ),
            // The stroke deleted, the fill twice as wide, the photo half the
            // size and moved, the small image deleted.
            item(
                ContentKind::Drawing,
                [100., 500., 200., 500.],
                [0., 0.],
                [1., 1.],
                true,
            ),
            item(
                ContentKind::Drawing,
                [300., 300., 350., 340.],
                [0., 0.],
                [2., 1.],
                false,
            ),
            item(
                ContentKind::Image,
                [400., 100., 480., 160.],
                [10., 20.],
                [0.5, 0.5],
                false,
            ),
            item(
                ContentKind::Image,
                [50., 50., 60., 60.],
                [0., 0.],
                [1., 1.],
                true,
            ),
        ];
        apply_content_edits(&mut document, &[], &[], &moves, &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        assert!(after.lines.iter().all(|line| line.text != "Gone"));
        assert!(after.lines.iter().any(|line| line.text == "Kept"));
        let has = |kind, rect: [f64; 4]| {
            after.objects.iter().any(|object| {
                object.kind == kind
                    && object
                        .rect
                        .iter()
                        .zip(rect)
                        .all(|(a, b)| (a - b).abs() < 0.01)
            })
        };
        assert!(
            !has(ContentKind::Drawing, [100., 500., 200., 500.]),
            "{:?}",
            after.objects
        );
        assert!(
            has(ContentKind::Drawing, [300., 300., 400., 340.]),
            "{:?}",
            after.objects
        );
        assert!(
            has(ContentKind::Image, [410., 120., 450., 150.]),
            "{:?}",
            after.objects
        );
        assert_eq!(
            after
                .objects
                .iter()
                .filter(|object| object.kind == ContentKind::Image)
                .count(),
            1
        );
    }

    #[test]
    fn copied_content_is_drawn_again_where_it_was_put() {
        use crate::pdf_content_moves::{ContentKind, ContentMove};
        let content = b"BT /F1 12 Tf 72 700 Td (Copied) Tj ET \
            0 0 1 RG 2 w 100 500 m 200 500 l S \
            q 80 0 0 60 400 100 cm /Im0 Do Q \
            q 0 0 612 792 re W n q 90 300 140 40 re W n 1 0 0 RG 3 w 100 320 m 200 320 l S \
            BT /F1 10 Tf 100 325 Td (Label) Tj ET Q Q";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let image = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => 1,
                "Height" => 1,
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            vec![0],
        ));
        let page_id = page_id(&document, 0).unwrap();
        let page = document.get_dictionary_mut(page_id).unwrap();
        let mut resources = page.get(b"Resources").unwrap().as_dict().unwrap().clone();
        resources.set("XObject", dictionary! { "Im0" => image });
        page.set("Resources", resources);
        let before = layer(&document);
        let copied = before
            .lines
            .iter()
            .find(|line| line.text == "Copied")
            .unwrap();
        let label = before
            .lines
            .iter()
            .find(|line| line.text == "Label")
            .unwrap();
        let copy = |kind, rect, offset, copy| ContentMove {
            page_index: 0,
            kind,
            rect,
            offset,
            scale: [1., 1.],
            removed: false,
            copy: Some(copy),
        };
        let moves = [
            copy(
                ContentKind::Text,
                crate::pdf_tables::line_core_box(copied).unwrap(),
                [0., -100.],
                0,
            ),
            copy(ContentKind::Drawing, [100., 500., 200., 500.], [0., 50.], 1),
            copy(ContentKind::Image, [400., 100., 480., 160.], [0., 200.], 2),
            // The clipped drawing and its label, together.
            copy(ContentKind::Drawing, [90., 300., 230., 340.], [250., 0.], 3),
            copy(
                ContentKind::Text,
                crate::pdf_tables::line_core_box(label).unwrap(),
                [250., 0.],
                3,
            ),
        ];
        apply_content_edits(&mut document, &[], &[], &moves, &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        let origins = |text: &str| {
            after
                .lines
                .iter()
                .filter(|line| line.text == text)
                .map(|line| {
                    let (x, y) = line.origin();
                    ((x * 100.).round() / 100., (y * 100.).round() / 100.)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(origins("Copied"), [(72., 700.), (72., 600.)]);
        assert_eq!(origins("Label").len(), 2, "{:?}", after.lines);
        assert!(origins("Label").contains(&(350., 325.)));
        let has = |kind, rect: [f64; 4]| {
            after.objects.iter().any(|object| {
                object.kind == kind
                    && object
                        .rect
                        .iter()
                        .zip(rect)
                        .all(|(a, b)| (a - b).abs() < 0.01)
            })
        };
        assert!(has(ContentKind::Drawing, [100., 500., 200., 500.]));
        assert!(
            has(ContentKind::Drawing, [100., 550., 200., 550.]),
            "{:?}",
            after.objects
        );
        assert!(has(ContentKind::Image, [400., 100., 480., 160.]));
        assert!(
            has(ContentKind::Image, [400., 300., 480., 360.]),
            "{:?}",
            after.objects
        );
        assert!(has(ContentKind::Drawing, [90., 300., 230., 340.]));
        assert!(
            has(ContentKind::Drawing, [340., 300., 480., 340.]),
            "{:?}",
            after.objects
        );
    }

    fn reload(document: &mut Document) -> Document {
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).expect("save");
        Document::load_mem(&bytes).expect("reload")
    }

    fn layer(document: &Document) -> PageTextLayer {
        extract_page_text(document, 0, &TextEditLimits::default()).expect("extract")
    }

    fn edit(line: &PageTextLine, text: &str) -> PageTextEdit {
        PageTextEdit {
            page_index: 0,
            anchor: line.anchor,
            original: line.original_runs(),
            runs: vec![TextEditRun {
                style: line.styles[line.chars.first().map_or(0, |c| c.style)].clone(),
                text: text.into(),
            }],
            layout: Default::default(),
        }
    }

    fn page_content(document: &Document) -> Vec<u8> {
        let page_id = page_id(document, 0).unwrap();
        decode_streams(
            document,
            &page_content_ids(document, page_id).unwrap(),
            &mut 0,
            &TextEditLimits::default(),
        )
        .unwrap()
        .concat()
    }

    fn char_x(line: &PageTextLine, character: char, occurrence: usize) -> f64 {
        line.chars
            .iter()
            .filter(|candidate| candidate.character == character)
            .nth(occurrence)
            .unwrap()
            .x
    }

    #[test]
    fn edits_a_simple_line_and_preserves_other_content_bytes() {
        let content = b"q 0 0 1 rg 10 10 50 50 re f Q\nBT /F1 12 Tf 72 700 Td (Hello World) Tj ET\nBT /F1 12 Tf 72 680 Td (Second line) Tj ET\n";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        assert_eq!(
            before
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["Hello World", "Second line"]
        );
        let line = &before.lines[0];
        assert_eq!(line.read_only, None);
        assert_eq!(line.origin(), (72., 700.));

        let change = edit(line, "Hello Butter");
        let expected = layout_text_edit(&before, line, &change.runs).unwrap();
        apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
        let mut document = reload(&mut document);
        let after = layer(&document);
        assert_eq!(after.lines[0].text, "Hello Butter");
        assert_eq!(after.lines[0].anchor, line.anchor);
        assert_eq!(after.lines[1], before.lines[1]);
        // Unchanged glyphs keep their positions; the layout the editor shows
        // is what is written.
        for (laid, extracted) in expected.chars.iter().zip(&after.lines[0].chars) {
            assert!(
                (laid.x - extracted.x).abs() < 1e-3,
                "{laid:?} {extracted:?}"
            );
        }
        let content_after = page_content(&document);
        let text = String::from_utf8_lossy(&content_after);
        assert!(text.starts_with("q 0 0 1 rg 10 10 50 50 re f Q\nBT /F1 12 Tf 72 700 Td "));
        assert!(
            text.ends_with(" ET\nBT /F1 12 Tf 72 680 Td (Second line) Tj ET\n"),
            "{text}"
        );

        // A second edit applies to the edited document as its new base.
        let edited = layer(&document);
        let change = edit(&edited.lines[0], "Hi Butter");
        apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
        assert_eq!(layer(&reload(&mut document)).lines[0].text, "Hi Butter");
    }

    #[test]
    fn merges_positioned_words_and_keeps_untouched_glyph_positions() {
        // LibreOffice positions each word with its own text matrix.
        let content = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (The) Tj 1 0 0 1 100 700 Tm (quick) Tj 1 0 0 1 140 700 Tm (fox) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        assert_eq!(before.lines.len(), 1);
        let line = &before.lines[0];
        assert_eq!(line.text, "The quick fox");
        assert!(line.chars[3].synthetic);
        let quick_width = char_x(line, 'f', 0) - char_x(line, 'q', 0);

        apply_text_edits(
            &mut document,
            &[edit(line, "The slow fox")],
            &TextEditLimits::default(),
        )
        .unwrap();
        let after = layer(&reload(&mut document));
        let edited = &after.lines[0];
        assert_eq!(edited.text, "The slow fox");
        assert!((char_x(edited, 'T', 0) - char_x(line, 'T', 0)).abs() < 1e-3);
        assert!((char_x(edited, 'e', 0) - char_x(line, 'e', 0)).abs() < 1e-3);
        let slow_width = char_x(edited, 'f', 0) - char_x(edited, 's', 0);
        let shift = char_x(edited, 'f', 0) - char_x(line, 'f', 0);
        // "slow" replaced "quick" (and the gap after it is kept).
        assert!((shift - (slow_width - quick_width)).abs() < 1e-3, "{shift}");
    }

    #[test]
    fn following_operators_keep_their_positions() {
        let content = b"BT /F1 12 Tf 72 700 Td (Hello) Tj /CS0 cs 1 sc ( World) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        assert_eq!(before.lines.len(), 2, "a non-device colour splits lines");
        apply_text_edits(
            &mut document,
            &[edit(&before.lines[0], "Hi")],
            &TextEditLimits::default(),
        )
        .unwrap();
        let after = layer(&reload(&mut document));
        assert_eq!(after.lines[0].text, "Hi");
        assert_eq!(after.lines[1].text, " World");
        assert_eq!(after.lines[1].anchor, before.lines[1].anchor);
    }

    #[test]
    fn a_word_in_another_fill_and_stroke_joins_its_line() {
        // Chrome sets both colours for a word in black in a grey line, each
        // word in its own text object.
        let content = b".5 .5 .5 RG .5 .5 .5 rg BT /F1 12 Tf 72 700 Td (About ) Tj ET \
            0 0 0 RG 0 0 0 rg BT /F1 12 Tf 108.024 700 Td (Name ) Tj ET \
            .5 .5 .5 RG .5 .5 .5 rg BT /F1 12 Tf 143.364 700 Td (or more) Tj ET \
            BT /F1 12 Tf 72 680 Td (Next) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        assert_eq!(before.lines.len(), 2);
        let line = &before.lines[0];
        assert_eq!(line.text, "About Name or more");
        let fills = line
            .original_runs()
            .iter()
            .map(|run| run.style.fill)
            .collect::<Vec<_>>();
        assert_eq!(fills, vec![Some([0.5; 3]), Some([0.; 3]), Some([0.5; 3])]);
        apply_text_edits(
            &mut document,
            &[edit(line, "About Name or less")],
            &TextEditLimits::default(),
        )
        .unwrap();
        let after = layer(&reload(&mut document));
        assert_eq!(after.lines[0].text, "About Name or less");
        let fills = after.lines[0]
            .original_runs()
            .iter()
            .map(|run| run.style.fill)
            .collect::<Vec<_>>();
        assert_eq!(fills, vec![Some([0.5; 3])]);
        assert_eq!(after.lines[1].text, "Next");
        assert_eq!(after.lines[1].fill, Some([0.5; 3]));
    }

    #[test]
    fn run_colours_save_read_back_and_restore_the_line_fill() {
        let red = Some([1., 0., 0.]);
        let blue = Some([0., 0., 1.]);
        let content =
            b"BT /F1 12 Tf 72 700 Td (Hello) Tj 1 0 0 rg ( World) Tj 0 g 0 -20 Td (Next) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        assert_eq!(before.lines.len(), 2, "device colours share a line");
        let line = &before.lines[0];
        assert_eq!(line.fill, Some([0.; 3]));
        let runs = line.original_runs();
        assert_eq!(runs.len(), 2);
        assert_eq!(
            (runs[0].style.fill, runs[1].style.fill),
            (Some([0.; 3]), red)
        );
        let mut change = edit(line, "Hi");
        change.runs[0].style.fill = blue;
        change.runs.push(runs[1].clone());
        apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        assert_eq!(after.lines.len(), 2);
        assert_eq!(after.lines[0].text, "Hi World");
        let fills = after.lines[0]
            .original_runs()
            .iter()
            .map(|run| run.style.fill)
            .collect::<Vec<_>>();
        assert_eq!(fills, vec![blue, red]);
        // The rest of the stream draws in the colours it did.
        assert_eq!(after.lines[1].text, "Next");
        assert_eq!(after.lines[1].fill, Some([0.; 3]));

        // A colour space without device components is set again by name.
        let content =
            b"BT /F1 12 Tf /Cs1 cs 0.2 0.2 0.2 sc 72 700 Td (Grey text) Tj 0 -20 Td (After) Tj ET";
        let mut document = super::tests::document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let mut change = edit(&before.lines[0], "Grey ");
        change.runs.push(TextEditRun {
            style: TextRunStyle {
                fill: red,
                ..change.runs[0].style.clone()
            },
            text: "text".into(),
        });
        apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
        let reloaded = reload(&mut document);
        let after = layer(&reloaded);
        assert_eq!(after.lines.len(), 2);
        assert_eq!(after.lines[0].fill, None);
        let fills = after.lines[0]
            .original_runs()
            .iter()
            .map(|run| run.style.fill)
            .collect::<Vec<_>>();
        assert_eq!(fills, vec![None, red]);
        let page = reloaded.get_pages()[&1];
        let bytes = reloaded.get_page_content(page);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("1 0 0 rg "), "{text}");
        assert!(text.contains("/Cs1 cs 0.2 0.2 0.2 sc "), "{text}");
    }

    #[test]
    fn a_chosen_face_draws_the_run_in_that_font() {
        let content = b"BT /F1 12 Tf 72 700 Td (Plain words) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let line = &before.lines[0];
        let face = TextFace {
            postscript_name: None,
            fallback: FallbackFace {
                family: FallbackFamily::Serif,
                bold: true,
                italic: false,
            },
        };
        let mut change = edit(line, "Plain ");
        change.runs.push(TextEditRun {
            style: TextRunStyle {
                face: Some(face),
                font_size: 14.,
                ..change.runs[0].style.clone()
            },
            text: "words".into(),
        });
        let expected = layout_text_edit(&before, line, &change.runs).unwrap();
        assert!(
            expected.chars[6..]
                .iter()
                .all(|character| character.source == LaidOutSource::Fallback)
        );
        apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        let edited = &after.lines[0];
        assert_eq!(edited.text, "Plain words");
        let styles = edited
            .original_runs()
            .into_iter()
            .map(|run| (run.text, run.style.font_size))
            .collect::<Vec<_>>();
        assert_eq!(
            styles,
            vec![("Plain ".to_owned(), 12.), ("words".to_owned(), 14.)]
        );
        let font = after
            .font(&edited.styles[edited.chars[6].style].font)
            .unwrap();
        assert_eq!(font.postscript_name.as_deref(), Some("Tinos-Bold"));
        for (laid, extracted) in expected.chars.iter().zip(&edited.chars) {
            assert!(
                (laid.x - extracted.x).abs() < 1e-3,
                "{laid:?} {extracted:?}"
            );
        }
    }

    #[test]
    fn characters_the_font_lacks_use_an_embedded_fallback() {
        let content = b"BT /F1 12 Tf 72 700 Td (Area 5 m2) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let line = &before.lines[0];
        let change = edit(line, "Area 5 m\u{b2} \u{3a9}");
        let expected = layout_text_edit(&before, line, &change.runs).unwrap();
        assert_eq!(
            expected.chars.last().unwrap().source,
            LaidOutSource::Fallback
        );
        apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
        let document = reload(&mut document);
        let after = layer(&document);
        assert_eq!(after.lines[0].text, "Area 5 m\u{b2} \u{3a9}");
        assert_eq!(after.lines[0].anchor, line.anchor);
        assert!(after.fonts.iter().any(|font| font.font == b"BPEdit1"));
        assert_eq!(after.lines[0].styles.len(), 2);
        for (laid, extracted) in expected.chars.iter().zip(&after.lines[0].chars) {
            assert!(
                (laid.x - extracted.x).abs() < 1e-3,
                "{laid:?} {extracted:?}"
            );
        }
    }

    #[test]
    fn type0_identity_fonts_encode_new_text_with_their_glyphs_or_extend_the_fallback() {
        let mut document = document(b"", vec![]);
        let characters = "Helo Wrd".chars().collect::<BTreeSet<_>>();
        let (font_id, cids) = crate::pdf_engine::embed_text_edit_fallback(
            &mut document,
            crate::pdf_engine::text_edit_fallback_glyph(
                FallbackFace {
                    family: FallbackFamily::Sans,
                    bold: false,
                    italic: false,
                },
                None,
                'a',
            )
            .unwrap()
            .0,
            &characters,
        )
        .unwrap();
        let codes = "Hello Word"
            .chars()
            .flat_map(|character| cids[&character].to_be_bytes())
            .collect::<Vec<_>>();
        let content = format!("BT /F0 10 Tf 50 500 Td {} Tj ET", format_hex(&codes));
        let page_id = page_id(&document, 0).unwrap();
        let content_id = document.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let page = document
            .get_object_mut(page_id)
            .unwrap()
            .as_dict_mut()
            .unwrap();
        page.set("Contents", content_id);
        page.set(
            "Resources",
            dictionary! { "Font" => dictionary! { "F0" => font_id } },
        );
        let before = layer(&document);
        assert_eq!(before.lines[0].text, "Hello Word");
        assert_eq!(before.lines[0].read_only, None);
        apply_text_edits(
            &mut document,
            &[edit(&before.lines[0], "Hello World!")],
            &TextEditLimits::default(),
        )
        .unwrap();
        let after = layer(&reload(&mut document));
        assert_eq!(after.lines[0].text, "Hello World!");
    }

    #[test]
    fn an_emptied_line_keeps_its_anchor_so_it_can_be_restored() {
        let content = b"BT /F1 12 Tf 72 700 Td (Remove me) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        apply_text_edits(
            &mut document,
            &[edit(&before.lines[0], "")],
            &TextEditLimits::default(),
        )
        .unwrap();
        let mut emptied = reload(&mut document);
        let after = layer(&emptied);
        assert_eq!(after.lines.len(), 1);
        assert_eq!(after.lines[0].text, "");
        assert_eq!(after.lines[0].anchor, before.lines[0].anchor);
        let restore = PageTextEdit {
            page_index: 0,
            anchor: after.lines[0].anchor,
            original: Vec::new(),
            runs: before.lines[0].original_runs(),
            layout: Default::default(),
        };
        apply_text_edits(&mut emptied, &[restore], &TextEditLimits::default()).unwrap();
        assert_eq!(layer(&reload(&mut emptied)).lines[0].text, "Remove me");
    }

    #[test]
    fn stale_and_read_only_edits_are_rejected() {
        let content = b"BT /F1 12 Tf 72 700 Td (Visible) Tj 3 Tr 72 680 Td (Hidden) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let mut stale = edit(&before.lines[0], "Changed");
        stale.original[0].text = "Other".into();
        assert!(matches!(
            apply_text_edits(&mut document, &[stale], &TextEditLimits::default()),
            Err(TextEditError::Stale(_))
        ));
        assert!(before.lines[1].read_only.is_some());
        assert!(matches!(
            apply_text_edits(
                &mut document,
                &[edit(&before.lines[1], "Shown")],
                &TextEditLimits::default()
            ),
            Err(TextEditError::ReadOnly(_))
        ));
    }

    #[test]
    fn rebasing_after_save_restores_earlier_states_from_the_saved_text() {
        let style = TextRunStyle {
            font: b"F1".to_vec(),
            font_size: 12.,
            char_spacing: 0.,
            word_spacing: 0.,
            fill: None,
            face: None,
        };
        let run = |text: &str| TextEditRun {
            style: style.clone(),
            text: text.into(),
        };
        let anchor = TextLineAnchor {
            x_centipoints: 7200,
            y_centipoints: 70000,
            ordinal: 0,
        };
        let edit = |original: &str, text: &str| PageTextEdit {
            page_index: 0,
            anchor,
            original: vec![run(original)],
            runs: vec![run(text)],
            layout: Default::default(),
        };
        let saved = vec![edit("Draft", "Final")];
        // The saved state itself needs no edit; the state before it (no
        // edit) and a later one become edits from "Final".
        assert!(rebase_text_edits(&saved, &saved).is_empty());
        assert_eq!(rebase_text_edits(&[], &saved), vec![edit("Final", "Draft")]);
        assert_eq!(
            rebase_text_edits(&[edit("Draft", "Later")], &saved),
            vec![edit("Final", "Later")]
        );
    }

    #[test]
    fn lexes_escapes_comments_and_inline_images() {
        let content = b"% comment\nBT /F#31 12 Tf 72 700 Td (a\\(b\\) \\101\\\nc) Tj ET\nBI /W 2 /H 1 /BPC 8 /CS /G ID \x00EI\xff EI\nBT /F1 12 Tf 72 680 Td <48 69> Tj ET";
        let document = document(content, vec![("F1", helvetica().into())]);
        let lines = layer(&document).lines;
        assert_eq!(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>(),
            ["a(b) Ac", "Hi"]
        );
    }

    #[test]
    fn text_matrix_scaled_fonts_and_trailing_ops_follow_the_edit() {
        // Quartz scales a 1pt font with the text matrix.
        let content = b"BT 12 0 0 12 72 700 Tm /F1 1 Tf (Scaled) Tj (Tail) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        assert_eq!(before.lines[0].text, "ScaledTail");
        let tail = char_x(&before.lines[0], 'T', 0);
        apply_text_edits(
            &mut document,
            &[edit(&before.lines[0], "SizedTail")],
            &TextEditLimits::default(),
        )
        .unwrap();
        let after = layer(&reload(&mut document));
        assert_eq!(after.lines[0].text, "SizedTail");
        assert!(char_x(&after.lines[0], 'T', 0) < tail);
    }

    #[test]
    fn actual_text_overrides_a_ligature_mapped_to_its_first_use() {
        // LibreOffice maps a ligature glyph to the text of its first use
        // ("fix" in "fixings"); a later use carries the right /ActualText.
        let to_unicode = Stream::new(
            Dictionary::new(),
            b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
              1 begincodespacerange <00> <FF> endcodespacerange\n\
              5 beginbfchar <41> <006600690078> <63> <0063> <65> <0065> <70> <0070> <73> <0073> endbfchar\n\
              endcmap CMapName currentdict /CMap defineresource pop end end"
                .to_vec(),
        );
        let mut document = document(
            b"BT /F1 12 Tf 72 700 Td (spe) Tj /Span <</ActualText <FEFF00660069>>> BDC (A) Tj EMC (c) Tj ET",
            vec![],
        );
        let to_unicode = document.add_object(to_unicode);
        let mut font = helvetica();
        font.set("ToUnicode", to_unicode);
        let font = document.add_object(font);
        let page = page_id(&document, 0).unwrap();
        document
            .get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => font } },
            );
        let before = layer(&document);
        assert_eq!(before.lines[0].text, "spefic");

        let change = edit(&before.lines[0], "spefics");
        apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
        assert_eq!(layer(&reload(&mut document)).lines[0].text, "spefics");
    }

    #[test]
    fn a_unicode_map_lopdf_rejects_is_read_directly() {
        // A bare word for the CMapName, lower-case hex, a range and an
        // array range.
        let map = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
            /CMapName LiberationSerif-cmap def\n/CMapType 2 def\n\
            /CIDSystemInfo <<\n/Registry (Adobe)\n/Ordering (UCS)\n/Supplement 0\n>> def\n\
            1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
            2 beginbfchar\n<0024> <0041>\n<000b> <0028>\nendbfchar\n\
            2 beginbfrange\n<0044> <0046> <0061>\n<0050> <0051> [<0066006C> <00DF>]\nendbfrange\n\
            endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
        let mut entries = parse_to_unicode(map, 2, 100);
        entries.sort();
        let text = |units: &[u16]| String::from_utf16(units).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|(code, units)| (*code, text(units)))
                .collect::<Vec<_>>(),
            [
                (0x0b, "(".into()),
                (0x24, "A".into()),
                (0x44, "a".into()),
                (0x45, "b".into()),
                (0x46, "c".into()),
                (0x50, "fl".into()),
                (0x51, "ß".into()),
            ]
        );
    }

    #[test]
    fn the_standard_symbol_font_reads_and_advances_by_its_own_encoding() {
        // ProQuest's footer: "©" in the viewer's Symbol font, no /Widths.
        let symbol = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Symbol",
        };
        let document = document(
            b"BT /F1 10 Tf 72 700 Td (Copyright ) Tj /F2 10 Tf <D3> Tj /F1 10 Tf ( 2023 a) Tj /F2 10 Tf (a) Tj ET",
            vec![("F1", helvetica().into()), ("F2", symbol.into())],
        );
        let layer = layer(&document);
        let line = &layer.lines[0];
        assert_eq!(line.text, "Copyright \u{a9} 2023 a\u{3b1}");
        assert_eq!(line.read_only, None);
        // © is 790 units wide.
        let copyright = char_x(line, '\u{a9}', 0);
        assert!((char_x(line, ' ', 1) - copyright - 7.9).abs() < 1e-6);
    }

    #[test]
    fn right_to_left_text_is_read_only() {
        let to_unicode = Stream::new(
            Dictionary::new(),
            b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
              /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
              1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
              2 beginbfchar\n<41> <0644>\n<42> <0627>\nendbfchar\n\
              endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
                .to_vec(),
        );
        let mut document = document(
            b"BT /F1 12 Tf 72 700 Td (BA) Tj ET BT /F1 12 Tf 72 600 Td (cd) Tj ET",
            vec![],
        );
        let to_unicode = document.add_object(to_unicode);
        let mut font = helvetica();
        font.set("ToUnicode", to_unicode);
        let font = document.add_object(font);
        let page = page_id(&document, 0).unwrap();
        document
            .get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => font } },
            );
        let layer = layer(&document);
        assert_eq!(
            layer.lines[0].read_only.as_deref(),
            Some("right-to-left text is not editable")
        );
        assert_eq!(layer.lines[1].read_only, None);
    }

    #[test]
    fn office_maths_letters_and_tab_spaces_read_as_drawn() {
        // PowerPoint's Cambria Math: <41> is 𝑥 (U+1D465) twice, <42> "ff"
        // (a ligature, two letters) stays as it is; <43>, a tab, is a space.
        let to_unicode = Stream::new(
            Dictionary::new(),
            b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
              /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
              /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
              1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
              3 beginbfchar\n<41> <D835DC65D835DC65>\n<42> <00660066>\n<43> <0009>\nendbfchar\n\
              endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
                .to_vec(),
        );
        let mut document = document(b"BT /F1 12 Tf 72 700 Td (ABCB) Tj ET", vec![]);
        let to_unicode = document.add_object(to_unicode);
        let mut font = helvetica();
        font.set("ToUnicode", to_unicode);
        let font = document.add_object(font);
        let page = page_id(&document, 0).unwrap();
        document
            .get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => font } },
            );
        assert_eq!(layer(&document).lines[0].text, "\u{1D465}ff ff");
    }

    #[test]
    fn spaces_keep_the_justification_gaps_on_both_sides() {
        // A justified space: 3pt gaps before and after it.
        let content = b"BT /F1 10 Tf 72 700 Td [(ab) -300 ( ) -300 (cd)] TJ ET";
        let document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let line = &before.lines[0];
        assert_eq!(line.text, "ab cd");
        let end_of_b = char_x(line, 'b', 0) + line.chars[1].advance;

        // Deleting the space takes both gaps with it.
        let joined = layout_text_edit(&before, line, &edit(line, "abcd").runs).unwrap();
        assert!((joined.chars[2].x - end_of_b).abs() < 1e-6);
        // Text typed before the space joins the word before it.
        let typed = layout_text_edit(&before, line, &edit(line, "abX cd").runs).unwrap();
        assert!((typed.chars[2].x - end_of_b).abs() < 1e-6);
        // Text typed after it takes the gap after the space.
        let after_space = layout_text_edit(&before, line, &edit(line, "ab Xcd").runs).unwrap();
        assert!((after_space.chars[3].x - char_x(line, 'c', 0)).abs() < 1e-6);

        for text in ["abcd", "abX cd", "ab Xcd", "a cd"] {
            let mut edited = document.clone();
            apply_text_edits(&mut edited, &[edit(line, text)], &TextEditLimits::default()).unwrap();
            assert_eq!(layer(&reload(&mut edited)).lines[0].text, text);
        }
    }

    #[test]
    fn spaces_drawn_as_gaps_read_back_after_their_neighbours_change() {
        // A bullet, then a gap read as a space; a space, then a kerned gap.
        let content = br"BT /F1 10 Tf 72 700 Td [(\225) -2000 (Grade) ( are) -300 (kerned)] TJ ET";
        let document = document(content, vec![("F1", helvetica().into())]);
        let line = &layer(&document).lines[0];
        assert_eq!(line.text, "\u{2022} Grade are kerned");
        for text in [
            " Grade are kerned",
            "\u{2022} Grade  kerned",
            "\u{2022} Grade are ",
        ] {
            let mut edited = document.clone();
            apply_text_edits(&mut edited, &[edit(line, text)], &TextEditLimits::default()).unwrap();
            assert_eq!(layer(&reload(&mut edited)).lines[0].text, text);
        }
    }

    #[test]
    fn unchanged_edits_leave_the_content_stream_alone() {
        let content = b"BT /F1 12 Tf 72 700 Td [(Ke) 40 (rned)] TJ ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let line = &layer(&document).lines[0];
        let unchanged = edit(line, &line.text);
        apply_text_edits(&mut document, &[unchanged], &TextEditLimits::default()).unwrap();
        assert_eq!(page_content(&document), content);
    }

    #[test]
    fn letter_outlines_tell_serif_from_sans_when_names_do_not() {
        let tinos = include_bytes!("../assets/fonts/Tinos-Regular.ttf");
        let arimo = include_bytes!("../assets/fonts/Arimo-Bold.ttf");
        assert_eq!(outline_serif(tinos, &BTreeMap::new(), false), Some(true));
        assert_eq!(outline_serif(arimo, &BTreeMap::new(), false), Some(false));

        // A subset named only "F1", with no serif flag, embedding Tinos
        // without its OS/2 classification (renamed, so only the outlines
        // can tell).
        let mut bare = tinos.to_vec();
        let tables = usize::from(u16::from_be_bytes([bare[4], bare[5]]));
        let record = (0..tables)
            .map(|index| 12 + index * 16)
            .find(|record| &bare[*record..*record + 4] == b"OS/2")
            .unwrap();
        bare[record..record + 4].copy_from_slice(b"XS/2");
        assert_eq!(outline_serif(&bare, &BTreeMap::new(), false), Some(true));
        let program = Stream::new(dictionary! { "Length1" => bare.len() as i64 }, bare);
        let mut document = document(b"BT /F1 12 Tf 72 700 Td (Plain line) Tj ET", vec![]);
        let program = document.add_object(program);
        let descriptor = document.add_object(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "ABCDEF+F1",
            "Flags" => 32,
            "FontFile2" => program,
        });
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "ABCDEF+F1",
            "FirstChar" => 32,
            "LastChar" => 126,
            "Widths" => (32..=126).map(|_| Object::Integer(500)).collect::<Vec<_>>(),
            "Encoding" => "WinAnsiEncoding",
            "FontDescriptor" => descriptor,
        });
        let page = page_id(&document, 0).unwrap();
        document
            .get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "Resources",
                dictionary! { "Font" => dictionary! { "F1" => font } },
            );
        let layer = layer(&document);
        assert_eq!(
            layer.font(b"F1").unwrap().fallback.family,
            FallbackFamily::Serif
        );
    }

    fn unicode_cmap_font(encoding: &str) -> Object {
        // Adobe-Japan1 CIDs: 'A' 34, 'Z' 59, space 1, あ 843.
        dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "KozMinPr6N-Regular",
            "Encoding" => Object::Name(encoding.as_bytes().to_vec()),
            "DescendantFonts" => vec![Object::Dictionary(dictionary! {
                "Type" => "Font",
                "Subtype" => "CIDFontType0",
                "BaseFont" => "KozMinPr6N-Regular",
                "CIDSystemInfo" => dictionary! {
                    "Registry" => Object::string_literal("Adobe"),
                    "Ordering" => Object::string_literal("Japan1"),
                    "Supplement" => 6,
                },
                "DW" => 1000,
                "W" => vec![
                    1.into(), vec![Object::Integer(278)].into(),
                    34.into(), vec![Object::Integer(667)].into(),
                    59.into(), vec![Object::Integer(611)].into(),
                ],
            })],
        }
        .into()
    }

    #[test]
    fn unicode_cmap_fonts_read_by_cid_widths_and_edit() {
        // "Aあ A" with no ToUnicode: the codes are the text.
        let content = b"BT /F1 20 Tf 72 700 Td <00413042002000410041> Tj ET";
        let document = document(content, vec![("F1", unicode_cmap_font("UniJIS-UCS2-H"))]);
        let before = layer(&document);
        let line = &before.lines[0];
        assert_eq!(line.text, "Aあ AA");
        assert_eq!(line.read_only, None);
        // Widths come from the CIDs: A 667, あ the default 1000, space 278.
        let xs = line
            .chars
            .iter()
            .map(|character| character.x)
            .collect::<Vec<_>>();
        for (x, expected) in xs.iter().zip([0., 13.34, 33.34, 38.9, 52.24]) {
            assert!((x - expected).abs() < 0.01, "{xs:?}");
        }

        // あ is drawn on the page, so it is typed with the font; Z is not,
        // so a bundled font draws it.
        let mut edited = document.clone();
        apply_text_edits(
            &mut edited,
            &[edit(line, "Aああ ZA")],
            &TextEditLimits::default(),
        )
        .unwrap();
        let after = layer(&reload(&mut edited));
        assert_eq!(after.lines[0].text, "Aああ ZA");
        assert_eq!(after.lines[0].chars[1].x, line.chars[1].x);
        assert!(
            after
                .fonts
                .iter()
                .any(|font| font.font.starts_with(b"BPEdit"))
        );
    }

    #[test]
    fn utf16_cmap_codes_read_surrogate_pairs() {
        let content = b"BT /F1 20 Tf 72 700 Td <0041D840DC0B0041> Tj ET";
        let horizontal = document(content, vec![("F1", unicode_cmap_font("UniJIS-UTF16-H"))]);
        assert_eq!(layer(&horizontal).lines[0].text, "A\u{2000b}A");
        let vertical = document(content, vec![("F1", unicode_cmap_font("UniJIS-UTF16-V"))]);
        assert!(layer(&vertical).lines[0].read_only.is_some());
    }

    /// Two pages that both draw Form XObject /Fm0, whose text uses its own
    /// /F1 (Times, while the page's /F1 is Helvetica) at twice the size,
    /// placed at (100, 500). With `nested`, /Fm0 draws /Fm1, which has the
    /// text; with `twice`, page 0 draws /Fm0 a second time.
    fn form_document(nested: bool, twice: bool) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let helvetica = document.add_object(helvetica());
        let times = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Times-Roman",
            "Encoding" => "WinAnsiEncoding",
        });
        let text_form = |document: &mut Document, matrix: Vec<Object>| {
            document.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 300.into(), 100.into()],
                    "Matrix" => matrix,
                    "Resources" => dictionary! { "Font" => dictionary! { "F1" => times } },
                },
                b"BT /F1 10 Tf 0 10 Td (Form text) Tj ET".to_vec(),
            ))
        };
        let placed = vec![
            2.into(),
            0.into(),
            0.into(),
            2.into(),
            100.into(),
            500.into(),
        ];
        let form = if nested {
            let inner = text_form(
                &mut document,
                vec![1.into(), 0.into(), 0.into(), 1.into(), 0.into(), 0.into()],
            );
            document.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 300.into(), 100.into()],
                    "Matrix" => placed,
                    "Resources" => dictionary! { "XObject" => dictionary! { "Fm1" => inner } },
                },
                b"q /Fm1 Do Q".to_vec(),
            ))
        } else {
            text_form(&mut document, placed)
        };
        let mut kids = Vec::new();
        for page in 0..2 {
            let content = match (page, twice) {
                (0, false) => b"BT /F1 12 Tf 72 700 Td (Page text) Tj ET q /Fm0 Do Q".to_vec(),
                (0, true) => b"BT /F1 12 Tf 72 700 Td (Page text) Tj ET q /Fm0 Do Q q 1 0 0 1 0 -200 cm /Fm0 Do Q".to_vec(),
                _ => b"q /Fm0 Do Q".to_vec(),
            };
            let content_id = document.add_object(Stream::new(Dictionary::new(), content));
            kids.push(Object::Reference(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => helvetica },
                    "XObject" => dictionary! { "Fm0" => form },
                },
            })));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 2 }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog);
        reload(&mut document)
    }

    fn page_texts(document: &Document, page: u32) -> Vec<String> {
        extract_page_text(document, page, &TextEditLimits::default())
            .unwrap()
            .lines
            .into_iter()
            .map(|line| line.text)
            .collect()
    }

    #[test]
    fn text_in_a_form_xobject_edits_on_its_page_only() {
        for nested in [false, true] {
            let mut document = form_document(nested, false);
            let before = layer(&document);
            assert_eq!(page_texts(&document, 0), ["Page text", "Form text"]);
            let line = &before.lines[1];
            assert_eq!(line.read_only, None);
            // Placed by the form matrix, at twice the size.
            let (x, y) = line.origin();
            assert!(
                (x - 100.).abs() < 1e-6 && (y - 520.).abs() < 1e-6,
                "{x} {y}"
            );
            let font = if nested {
                b"Fm0/Fm1/F1".as_slice()
            } else {
                b"Fm0/F1"
            };
            assert_eq!(line.styles[0].font, font);
            assert_eq!(
                before.font(font).unwrap().fallback.family,
                FallbackFamily::Serif
            );

            let mut change = edit(line, "Form text edited Ж");
            change.page_index = 0;
            apply_text_edits(&mut document, &[change], &TextEditLimits::default()).unwrap();
            let document = reload(&mut document);
            assert_eq!(
                page_texts(&document, 0),
                ["Page text", "Form text edited Ж"],
                "nested {nested}"
            );
            // The other page still draws the original form.
            assert_eq!(page_texts(&document, 1), ["Form text"], "nested {nested}");
            // The fallback font joined the edited form's own resources.
            let edited = layer(&document);
            assert!(
                edited.lines[1]
                    .styles
                    .iter()
                    .any(|style| style.font.ends_with(b"/BPEdit1"))
            );
        }
    }

    #[test]
    fn a_form_drawn_twice_on_a_page_is_not_editable() {
        let document = form_document(false, true);
        let layer = layer(&document);
        let forms = layer
            .lines
            .iter()
            .filter(|line| line.text == "Form text")
            .collect::<Vec<_>>();
        assert_eq!(forms.len(), 2);
        for line in forms {
            assert_eq!(
                line.read_only.as_deref(),
                Some("the text is in a form drawn more than once on this page")
            );
        }
        // Page text is still editable.
        assert_eq!(layer.lines[0].read_only, None);
    }

    #[test]
    fn a_blank_added_line_is_drawn_as_a_space() {
        let content = b"BT /F1 12 Tf 72 700 Td (Last line) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let line = &before.lines[0];
        let mut grown = edit(line, "Last line");
        grown.layout.extra_lines.push(ExtraLine {
            anchor: TextLineAnchor {
                x_centipoints: 7200,
                y_centipoints: 68600,
                ordinal: 0,
            },
            below: 14.,
            origin_shift: 0.,
            fit: LineFit::default(),
            runs: Vec::new(),
            hard_break: false,
        });
        apply_text_edits(&mut document, &[grown], &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        let texts = after
            .lines
            .iter()
            .map(|line| (line.text.clone(), line.origin()))
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                ("Last line".to_owned(), (72., 700.)),
                (" ".to_owned(), (72., 686.)),
            ]
        );
    }

    #[test]
    fn a_line_clipped_on_its_own_grows_its_clip_for_lines_added_below() {
        // As Chrome writes a table cell's text: a rectangle one line tall
        // clips it.
        let content = b"q 70 696 120 16 re W* n BT /F1 12 Tf 72 700 Td (Heading) Tj ET Q";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let line = &before.lines[0];
        assert_eq!(line.clip_bounds, Some([70., 696., 190., 712.]));
        let mut grown = edit(line, "Heading");
        let style = grown.runs[0].style.clone();
        grown.layout.extra_lines.push(ExtraLine {
            anchor: TextLineAnchor {
                x_centipoints: 7200,
                y_centipoints: 68600,
                ordinal: 0,
            },
            below: 14.,
            origin_shift: 0.,
            fit: LineFit::default(),
            runs: vec![TextEditRun {
                style,
                text: "and more".into(),
            }],
            hard_break: false,
        });
        apply_text_edits(&mut document, &[grown], &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut document));
        let added = after
            .lines
            .iter()
            .find(|line| line.text == "and more")
            .unwrap();
        let [left, bottom, right, top] = crate::pdf_tables::line_box(added).unwrap();
        let clip = added.clip_bounds.unwrap();
        assert!(
            clip[0] <= left && clip[1] <= bottom && clip[2] >= right && clip[3] >= top,
            "{clip:?} clips {:?}",
            [left, bottom, right, top]
        );
        // The line it was set for keeps its place in it.
        assert_eq!(clip[3], 712.);
    }

    #[test]
    fn extra_lines_and_justified_fits_leave_following_text_in_place() {
        let content = b"BT /F1 12 Tf 72 700 Td (First line here) Tj 0 -14 Td (Second line) Tj 0 -40 Td (Next paragraph) Tj ET";
        let mut document = document(content, vec![("F1", helvetica().into())]);
        let before = layer(&document);
        let [first, second, next] = [&before.lines[0], &before.lines[1], &before.lines[2]];
        assert_eq!(next.origin(), (72., 646.));

        let mut stretched = edit(first, "First line here");
        stretched.layout.fit.justify_end = Some(200.);
        let mut grown = edit(second, "Second line");
        // (Unchanged text, so only the added line differs.)
        grown.layout.extra_lines.push(ExtraLine {
            anchor: TextLineAnchor {
                x_centipoints: 7200,
                y_centipoints: 67200,
                ordinal: 0,
            },
            below: 14.,
            origin_shift: 0.,
            fit: LineFit::default(),
            runs: vec![TextEditRun {
                style: second.styles[0].clone(),
                text: "Third added".into(),
            }],
            hard_break: false,
        });
        apply_text_edits(
            &mut document,
            &[stretched, grown.clone()],
            &TextEditLimits::default(),
        )
        .unwrap();
        let after = layer(&reload(&mut document));
        let texts = after
            .lines
            .iter()
            .map(|line| (line.text.clone(), line.origin()))
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            [
                ("First line here".to_owned(), (72., 700.)),
                ("Second line".to_owned(), (72., 686.)),
                ("Third added".to_owned(), (72., 672.)),
                ("Next paragraph".to_owned(), (72., 646.)),
            ]
        );
        // The justified line now ends at 200 and keeps its first word.
        let line = &after.lines[0];
        let last = line.chars.last().unwrap();
        assert!(
            (last.x + last.advance - 200.).abs() < 0.05,
            "{}",
            last.x + last.advance
        );
        assert_eq!(line.chars[0].x, first.chars[0].x);

        // After saving, undoing back to the original empties the added line
        // through its anchor.
        let saved = [grown.clone()];
        let undone = rebase_text_edits(&[], &saved);
        assert_eq!(undone.len(), 1, "{undone:#?}");
        assert_eq!(undone[0].original_text(), "Third added");
        assert!(undone[0].runs.is_empty());
        let mut saved_document = reload(&mut document);
        apply_text_edits(&mut saved_document, &undone, &TextEditLimits::default()).unwrap();
        let texts = layer(&reload(&mut saved_document))
            .lines
            .iter()
            .map(|line| line.text.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            texts,
            ["First line here", "Second line", "", "Next paragraph"]
        );
    }
}
