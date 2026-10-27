//! The inline editor for existing page text (the Edit PDF Content tool).
//!
//! [`PageTextEditState`] is the editing model: one line's characters, each
//! with the run style it is drawn in, a caret and selection, and the layout
//! the saved PDF will have ([`crate::pdf_text_edit::layout_text_edit`]).
//! [`PageTextInput`] receives keyboard and IME input for it; the workspace
//! draws the caret and selection over the page, which shows the edited text
//! as PDFium renders it.

use std::ops::Range;

use gpui::{
    App, Bounds, ClipboardItem, Context, EntityInputHandler, EventEmitter, FocusHandle, Focusable,
    KeyBinding, Pixels, UTF16Selection, Window, actions, point, px, size,
};

use crate::pdf_text_blocks::{ListMarker, item_marker, list_item_marker};
use crate::pdf_text_edit::{
    ExtraLine, PageTextEdit, ParagraphFormat, TextEditError, TextEditRun, TextRunStyle,
};
use crate::pdf_text_reflow::{Paragraph, StyledChar, WrappedLine};

pub const PAGE_TEXT_EDITOR_CONTEXT: &str = "PageTextEditor";
const UNDO_LIMIT: usize = 200;

actions!(
    page_text_editor,
    [
        Backspace,
        DeleteForward,
        DeleteWordBackward,
        DeleteWordForward,
        DeleteToLineStart,
        MoveLeft,
        MoveRight,
        MoveWordLeft,
        MoveWordRight,
        MoveLineStart,
        MoveLineEnd,
        MoveUp,
        MoveDown,
        SelectUp,
        SelectDown,
        InsertNewline,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectAllText,
        CopyText,
        CutText,
        PasteText,
        UndoText,
        RedoText,
        FinishEditing,
        NextCell,
        PreviousCell,
    ]
);

pub fn init_page_text_editor_actions(cx: &mut App) {
    let context = Some(PAGE_TEXT_EDITOR_CONTEXT);
    #[cfg(target_os = "macos")]
    let platform = [
        KeyBinding::new("alt-backspace", DeleteWordBackward, context),
        KeyBinding::new("alt-delete", DeleteWordForward, context),
        KeyBinding::new("cmd-backspace", DeleteToLineStart, context),
        KeyBinding::new("alt-left", MoveWordLeft, context),
        KeyBinding::new("alt-right", MoveWordRight, context),
        KeyBinding::new("cmd-left", MoveLineStart, context),
        KeyBinding::new("cmd-right", MoveLineEnd, context),
        KeyBinding::new("alt-shift-left", SelectWordLeft, context),
        KeyBinding::new("alt-shift-right", SelectWordRight, context),
        KeyBinding::new("cmd-shift-left", SelectLineStart, context),
        KeyBinding::new("cmd-shift-right", SelectLineEnd, context),
        KeyBinding::new("cmd-a", SelectAllText, context),
        KeyBinding::new("cmd-c", CopyText, context),
        KeyBinding::new("cmd-x", CutText, context),
        KeyBinding::new("cmd-v", PasteText, context),
        KeyBinding::new("cmd-z", UndoText, context),
        KeyBinding::new("cmd-shift-z", RedoText, context),
    ];
    #[cfg(not(target_os = "macos"))]
    let platform = [
        KeyBinding::new("ctrl-backspace", DeleteWordBackward, context),
        KeyBinding::new("ctrl-delete", DeleteWordForward, context),
        KeyBinding::new("ctrl-shift-backspace", DeleteToLineStart, context),
        KeyBinding::new("ctrl-left", MoveWordLeft, context),
        KeyBinding::new("ctrl-right", MoveWordRight, context),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, context),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, context),
        KeyBinding::new("ctrl-a", SelectAllText, context),
        KeyBinding::new("ctrl-c", CopyText, context),
        KeyBinding::new("ctrl-x", CutText, context),
        KeyBinding::new("ctrl-v", PasteText, context),
        KeyBinding::new("ctrl-z", UndoText, context),
        KeyBinding::new("ctrl-y", RedoText, context),
        KeyBinding::new("ctrl-shift-z", RedoText, context),
    ];
    cx.bind_keys(platform);
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, context),
        KeyBinding::new("shift-backspace", Backspace, context),
        KeyBinding::new("delete", DeleteForward, context),
        KeyBinding::new("left", MoveLeft, context),
        KeyBinding::new("right", MoveRight, context),
        KeyBinding::new("home", MoveLineStart, context),
        KeyBinding::new("end", MoveLineEnd, context),
        KeyBinding::new("up", MoveUp, context),
        KeyBinding::new("down", MoveDown, context),
        KeyBinding::new("shift-up", SelectUp, context),
        KeyBinding::new("shift-down", SelectDown, context),
        KeyBinding::new("shift-left", SelectLeft, context),
        KeyBinding::new("shift-right", SelectRight, context),
        KeyBinding::new("shift-home", SelectLineStart, context),
        KeyBinding::new("shift-end", SelectLineEnd, context),
        // Enter starts a new line in the paragraph; Escape (or a click
        // elsewhere) finishes.
        KeyBinding::new("enter", InsertNewline, context),
        KeyBinding::new("shift-enter", InsertNewline, context),
        KeyBinding::new("escape", FinishEditing, context),
        // In a table cell, Tab and Shift-Tab go to the next and previous
        // cell; elsewhere Tab types a space.
        KeyBinding::new("tab", NextCell, context),
        KeyBinding::new("shift-tab", PreviousCell, context),
    ]);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CharClass {
    Space,
    Word,
    Punctuation,
}

fn char_class(character: char) -> CharClass {
    if character.is_whitespace() {
        CharClass::Space
    } else if character.is_alphanumeric() || character == '_' {
        CharClass::Word
    } else {
        CharClass::Punctuation
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Snapshot {
    chars: Vec<char>,
    styles: Vec<usize>,
    caret: usize,
    anchor: usize,
    format: ParagraphFormat,
}

/// Four corners in page user space.
pub type PageQuad = [(f64, f64); 4];

/// One paragraph being edited: its text (hard line breaks are `'\n'`), each
/// character's run style, a caret and selection, and the paragraph wrapped
/// as the saved PDF will show it ([`Paragraph::wrap`]).
#[derive(Clone, Debug)]
pub struct PageTextEditState {
    paragraph: Paragraph,
    chars: Vec<char>,
    /// Each character's index into `style_table`.
    styles: Vec<usize>,
    style_table: Vec<TextRunStyle>,
    caret: usize,
    anchor: usize,
    marked: Option<Range<usize>>,
    wrapped: Vec<WrappedLine>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    rejected: Option<char>,
}

impl PageTextEditState {
    /// Opens `paragraph` with its lines' current runs (earlier edits, or
    /// their own) and lines an earlier edit added to it.
    pub fn new(
        paragraph: Paragraph,
        current: &[Vec<TextEditRun>],
        breaks: &[Option<bool>],
        added: &[ExtraLine],
        caret: usize,
    ) -> Result<Self, TextEditError> {
        if let Some(reason) = paragraph
            .lines()
            .iter()
            .find_map(|line| line.read_only.clone())
        {
            return Err(TextEditError::ReadOnly(reason));
        }
        let mut style_table: Vec<TextRunStyle> = Vec::new();
        let mut chars = Vec::new();
        let mut styles = Vec::new();
        for (character, style) in paragraph.text_with_breaks(current, breaks, added) {
            let index = match style_table.iter().position(|known| known == &style) {
                Some(index) => index,
                None => {
                    style_table.push(style);
                    style_table.len() - 1
                }
            };
            chars.push(character);
            styles.push(index);
        }
        let mut state = Self {
            paragraph,
            chars,
            styles,
            style_table,
            caret: 0,
            anchor: 0,
            marked: None,
            wrapped: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            rejected: None,
        };
        state.relayout()?;
        state.caret = caret.min(state.len());
        state.anchor = state.caret;
        Ok(state)
    }

    pub fn paragraph(&self) -> &Paragraph {
        &self.paragraph
    }

    pub fn page_index(&self) -> u32 {
        self.paragraph.page_index()
    }

    pub fn wrapped(&self) -> &[WrappedLine] {
        &self.wrapped
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn selection(&self) -> Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }

    pub fn marked(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    /// The character the last typing or paste could not draw.
    pub fn rejected(&self) -> Option<char> {
        self.rejected
    }

    fn styled(&self) -> Vec<StyledChar> {
        self.chars
            .iter()
            .zip(&self.styles)
            .map(|(character, style)| (*character, self.style_table[*style].clone()))
            .collect()
    }

    /// The edits that make the page show this paragraph.
    pub fn edits(&self) -> Vec<PageTextEdit> {
        self.paragraph.edits(&self.styled(), &self.wrapped)
    }

    /// Each wrapped line with visible text as new text: where its first
    /// character starts on its baseline in page space, its runs with sizes
    /// and spacing in page units, and whether a hard break ends it.
    pub fn laid_out_lines(&self) -> Vec<((f64, f64), Vec<TextEditRun>, bool)> {
        let styled = self.styled();
        let mut lines = Vec::new();
        for line in &self.wrapped {
            let Some(first) = line.layout.chars.first() else {
                continue;
            };
            let chars = &styled[line.range.clone()];
            if chars.iter().all(|(character, _)| character.is_whitespace()) {
                continue;
            }
            let m = line.line.matrix;
            let origin = (m[0] * first.x + m[4], m[1] * first.x + m[5]);
            let scale = (m[0] * m[3] - m[1] * m[2]).abs().sqrt();
            let in_page_units = |style: &TextRunStyle| TextRunStyle {
                font_size: style.font_size * scale,
                char_spacing: style.char_spacing * scale,
                word_spacing: style.word_spacing * scale,
                ..style.clone()
            };
            let mut runs: Vec<TextEditRun> = Vec::new();
            for (character, style) in chars {
                let style = in_page_units(style);
                match runs.last_mut() {
                    Some(run) if run.style == style => run.text.push(*character),
                    _ => runs.push(TextEditRun {
                        style,
                        text: character.to_string(),
                    }),
                }
            }
            let hard_break = self.chars.get(line.range.end) == Some(&'\n');
            lines.push((origin, runs, hard_break));
        }
        lines
    }

    /// The wrapped line that shows `index`: the line it is in, or that it
    /// ends, preferring the later line at a soft break.
    fn line_of(&self, index: usize) -> usize {
        self.wrapped
            .iter()
            .position(|line| index >= line.range.start && index < line.range.end)
            .or_else(|| {
                self.wrapped
                    .iter()
                    .rposition(|line| index >= line.range.start && index <= line.range.end)
            })
            .unwrap_or(self.wrapped.len().saturating_sub(1))
    }

    /// Text-space x of the caret position `index` on wrapped line `line`.
    fn x_on(&self, line: usize, index: usize) -> f64 {
        let wrapped = &self.wrapped[line];
        let offset = index.clamp(wrapped.range.start, wrapped.range.end) - wrapped.range.start;
        match wrapped.layout.chars.get(offset) {
            Some(character) => character.x,
            None => wrapped.layout.chars.last().map_or_else(
                || wrapped.fit.shift,
                |character| character.x + character.advance,
            ),
        }
    }

    fn quad_on(&self, line: usize, x0: f64, x1: f64) -> PageQuad {
        self.wrapped[line].line.quad(x0, x1)
    }

    /// The caret, bottom to top, in page space.
    pub fn caret_segment(&self) -> Option<[(f64, f64); 2]> {
        if self.wrapped.is_empty() {
            return None;
        }
        let line = self.line_of(self.caret);
        let x = self.x_on(line, self.caret);
        let quad = self.quad_on(line, x, x);
        Some([quad[0], quad[3]])
    }

    fn range_quads(&self, range: Range<usize>) -> Vec<PageQuad> {
        self.wrapped
            .iter()
            .enumerate()
            .filter(|(_, line)| {
                range.start < line.range.end.max(line.range.start + 1)
                    && range.end > line.range.start
            })
            .map(|(index, line)| {
                let start = range.start.max(line.range.start);
                let end = range.end.min(line.range.end);
                self.quad_on(index, self.x_on(index, start), self.x_on(index, end))
            })
            .collect()
    }

    /// The selection, a quad per wrapped line, in page space.
    pub fn selection_quads(&self) -> Vec<PageQuad> {
        let selection = self.selection();
        if selection.is_empty() {
            return Vec::new();
        }
        self.range_quads(selection)
    }

    /// Underlines under composing (IME) text, in page space.
    pub fn marked_segments(&self) -> Vec<[(f64, f64); 2]> {
        self.marked
            .clone()
            .map(|range| {
                self.range_quads(range)
                    .into_iter()
                    .map(|quad| [quad[0], quad[1]])
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Each wrapped line's extent, in page space: the paragraph's frame.
    pub fn frame_quads(&self) -> Vec<PageQuad> {
        (0..self.wrapped.len())
            .map(|index| {
                let line = &self.wrapped[index];
                let start = line
                    .layout
                    .chars
                    .first()
                    .map_or(line.fit.shift, |character| character.x.min(0.));
                let end = self.x_on(index, line.range.end).max(start);
                self.quad_on(index, start, end)
            })
            .collect()
    }

    /// A return mark (↵) after each line that ends in a hard break, as
    /// strokes in page space.
    pub fn break_marks(&self) -> Vec<Vec<(f64, f64)>> {
        self.line_end_marks(true, |at| {
            vec![
                vec![at(0.85, 0.7), at(0.85, 0.35), at(0.15, 0.35)],
                vec![at(0.4, 0.5), at(0.15, 0.35), at(0.4, 0.2)],
            ]
        })
    }

    /// A wrap mark (⤵) after each line whose text continues on the next.
    pub fn wrap_marks(&self) -> Vec<Vec<(f64, f64)>> {
        self.line_end_marks(false, |at| {
            vec![
                vec![
                    at(0.1, 0.75),
                    at(0.35, 0.73),
                    at(0.52, 0.62),
                    at(0.6, 0.45),
                    at(0.6, 0.2),
                ],
                vec![at(0.42, 0.36), at(0.6, 0.18), at(0.78, 0.36)],
            ]
        })
    }

    /// `strokes` in a unit square after the end of each line that ends in a
    /// hard break (`hard`) or wraps into the next line.
    fn line_end_marks(
        &self,
        hard: bool,
        strokes: impl Fn(&dyn Fn(f64, f64) -> (f64, f64)) -> Vec<Vec<(f64, f64)>>,
    ) -> Vec<Vec<(f64, f64)>> {
        (0..self.wrapped.len())
            .filter(|index| {
                let broken = self.chars.get(self.wrapped[*index].range.end) == Some(&'\n');
                if hard {
                    broken
                } else {
                    !broken && index + 1 < self.wrapped.len()
                }
            })
            .flat_map(|index| {
                let line = &self.wrapped[index].line;
                let height = (line.ascent - line.descent).abs();
                let end = self.x_on(index, self.wrapped[index].range.end);
                // Unit square from the box's bottom left to its top right.
                let quad = self.quad_on(index, end + 0.15 * height, end + 0.65 * height);
                let at = |u: f64, v: f64| {
                    let bottom = lerp(quad[0], quad[1], u);
                    let top = lerp(quad[3], quad[2], u);
                    lerp(bottom, top, v)
                };
                strokes(&at)
            })
            .collect()
    }

    /// The bounds of `range`, in page space, for the input method.
    pub fn range_quad(&self, range: Range<usize>) -> Option<PageQuad> {
        let line = self.line_of(range.start);
        let end = range.end.min(self.wrapped.get(line)?.range.end);
        Some(self.quad_on(line, self.x_on(line, range.start), self.x_on(line, end)))
    }

    /// The caret position nearest page point (`x`, `y`): on the wrapped line
    /// whose baseline is nearest, then by x along it.
    pub fn index_at_point(&self, x: f64, y: f64) -> usize {
        let Some((line, text_x)) = self
            .wrapped
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let (text_x, text_y) = line.line.text_space_point(x, y)?;
                let middle = line.line.rise + (line.line.ascent + line.line.descent) / 2.;
                Some((index, text_x, (text_y - middle).abs()))
            })
            .min_by(|a, b| a.2.total_cmp(&b.2))
            .map(|(index, text_x, _)| (index, text_x))
        else {
            return 0;
        };
        self.index_on_line(line, text_x)
    }

    fn index_on_line(&self, line: usize, x: f64) -> usize {
        let wrapped = &self.wrapped[line];
        let offset = wrapped.layout.index_at(x).min(wrapped.range.len());
        wrapped.range.start + offset
    }

    /// Whether page point (`x`, `y`) is on the paragraph, within `tolerance`
    /// text-space units of a wrapped line's box.
    pub fn contains_point(&self, x: f64, y: f64, tolerance: f64) -> bool {
        self.frame_quads()
            .iter()
            .zip(&self.wrapped)
            .any(|(_, line)| {
                let Some((text_x, text_y)) = line.line.text_space_point(x, y) else {
                    return false;
                };
                let bottom = line.line.rise + line.line.descent - tolerance;
                let top = line.line.rise + line.line.ascent + tolerance;
                let start = line
                    .layout
                    .chars
                    .first()
                    .map_or(line.fit.shift, |c| c.x.min(0.))
                    - tolerance;
                let end = line
                    .layout
                    .chars
                    .last()
                    .map_or(line.fit.shift, |c| c.x + c.advance)
                    + tolerance;
                (bottom..=top).contains(&text_y)
                    && (start..=end.max(start + 2. * tolerance)).contains(&text_x)
            })
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            chars: self.chars.clone(),
            styles: self.styles.clone(),
            caret: self.caret,
            anchor: self.anchor,
            format: self.paragraph.format(),
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.chars = snapshot.chars;
        self.styles = snapshot.styles;
        self.paragraph.set_format(snapshot.format);
        self.caret = snapshot.caret.min(self.chars.len());
        self.anchor = snapshot.anchor.min(self.chars.len());
        self.marked = None;
        self.relayout().expect("restored text was laid out before");
    }

    fn relayout(&mut self) -> Result<(), TextEditError> {
        self.wrapped = self.paragraph.wrap(&self.styled())?;
        Ok(())
    }

    /// The style new text at `range` takes: the first replaced character's,
    /// else the character before it, else the one after it, else the
    /// paragraph's first.
    fn style_for(&self, range: &Range<usize>) -> usize {
        if !range.is_empty() {
            self.styles[range.start]
        } else if range.start > 0 {
            self.styles[range.start - 1]
        } else if range.end < self.styles.len() {
            self.styles[range.end]
        } else {
            0
        }
    }

    /// Replaces `range` with `text`, keeping the previous text when the
    /// paragraph's fonts cannot draw it. Line breaks become hard breaks.
    /// Returns whether the text changed.
    pub fn replace(&mut self, range: Range<usize>, text: &str) -> bool {
        let range = range.start.min(self.len())..range.end.min(self.len());
        let range = range.start.min(range.end)..range.end;
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let inserted = text
            .chars()
            .map(|character| if character == '\t' { ' ' } else { character })
            .filter(|character| *character == '\n' || !character.is_control())
            .collect::<Vec<_>>();
        if range.is_empty() && inserted.is_empty() {
            return false;
        }
        let before = self.snapshot();
        if self.style_table.is_empty() {
            self.style_table
                .push(self.paragraph.lines()[0].styles[0].clone());
        }
        let style = self.style_for(&range);
        self.chars.splice(range.clone(), inserted.iter().copied());
        self.styles
            .splice(range.clone(), std::iter::repeat_n(style, inserted.len()));
        self.caret = range.start + inserted.len();
        self.anchor = self.caret;
        self.marked = None;
        self.renumber();
        self.commit_change(before, inserted.first().copied())
    }

    /// Numbers a numbered list's items in order from the first item's
    /// number, as list numbering is automatic.
    fn renumber(&mut self) {
        if !matches!(
            self.paragraph.list().map(|list| list.marker),
            Some(ListMarker::Number { .. })
        ) {
            return;
        }
        let mut next = None;
        let mut start = 0;
        while start <= self.chars.len() {
            let digits = self.chars[start..]
                .iter()
                .take_while(|character| character.is_ascii_digit())
                .count();
            if let Some((ListMarker::Number { .. }, _)) = item_marker(&self.chars[start..]) {
                let number = self.chars[start..start + digits]
                    .iter()
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or(1);
                let wanted = *next.get_or_insert(number);
                next = Some(wanted + 1);
                let replacement = wanted.to_string().chars().collect::<Vec<_>>();
                if replacement[..] != self.chars[start..start + digits] {
                    let style = self.styles[start];
                    self.chars
                        .splice(start..start + digits, replacement.iter().copied());
                    self.styles.splice(
                        start..start + digits,
                        std::iter::repeat_n(style, replacement.len()),
                    );
                    let grown = replacement.len() as isize - digits as isize;
                    for position in [&mut self.caret, &mut self.anchor] {
                        if *position > start {
                            *position = (*position as isize + grown).max(start as isize) as usize;
                        }
                    }
                }
            }
            match self.chars[start..]
                .iter()
                .position(|character| *character == '\n')
            {
                Some(offset) => start += offset + 1,
                None => break,
            }
        }
    }

    /// The style the selection starts in, or that typing at the caret takes.
    pub fn current_style(&self) -> Option<&TextRunStyle> {
        if self.chars.is_empty() {
            return self.paragraph.lines()[0].styles.first();
        }
        self.style_table.get(self.style_for(&self.selection()))
    }

    /// The characters a style change applies to: the selection, or with
    /// none the whole paragraph.
    fn style_range(&self) -> Range<usize> {
        let selection = self.selection();
        if selection.is_empty() {
            0..self.len()
        } else {
            selection
        }
    }

    /// Restyles the selection (or the whole paragraph) with `change`, as one
    /// undoable step. Keeps the previous styles when no font can draw the
    /// text so. Returns whether anything changed.
    pub fn restyle(&mut self, change: impl Fn(&TextRunStyle) -> TextRunStyle) -> bool {
        let range = self.style_range();
        let before = self.snapshot();
        let mut changed = false;
        for index in range {
            let style = change(&self.style_table[self.styles[index]]);
            if style == self.style_table[self.styles[index]] {
                continue;
            }
            let interned = match self.style_table.iter().position(|known| known == &style) {
                Some(interned) => interned,
                None => {
                    self.style_table.push(style);
                    self.style_table.len() - 1
                }
            };
            self.styles[index] = interned;
            changed = true;
        }
        if !changed {
            return false;
        }
        self.commit_change(before, None)
    }

    /// Sets the paragraph's alignment and line spacing, as one undoable step.
    pub fn set_format(&mut self, format: ParagraphFormat) -> bool {
        if format == self.paragraph.format() {
            return false;
        }
        let before = self.snapshot();
        self.paragraph.set_format(format);
        self.commit_change(before, None)
    }

    /// Lays out a change made since `before`, recording it for undo, or
    /// returns to `before` when it cannot be drawn.
    fn commit_change(&mut self, before: Snapshot, inserted: Option<char>) -> bool {
        match self.relayout() {
            Ok(()) => {
                self.rejected = None;
                if self.undo.len() == UNDO_LIMIT {
                    self.undo.remove(0);
                }
                self.undo.push(before);
                self.redo.clear();
                true
            }
            Err(error) => {
                self.rejected = match error {
                    TextEditError::Unencodable(character) => Some(character),
                    _ => inserted,
                };
                self.restore(before);
                false
            }
        }
    }

    /// The fonts that draw characters in place of the ones their runs ask
    /// for, by name.
    pub fn substitutes(&self) -> Vec<String> {
        let layer = self.paragraph.layer();
        let mut names = Vec::new();
        for (character, style) in self.chars.iter().zip(&self.styles) {
            if let Some(name) =
                crate::pdf_text_edit::substitute_font(layer, &self.style_table[*style], *character)
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        names
    }

    /// Replaces `range` with composing (IME) text, marking it.
    pub fn replace_marked(&mut self, range: Range<usize>, text: &str) -> bool {
        let start = range.start.min(self.len());
        let changed = self.replace(range, text);
        if changed {
            let end = start + text.chars().count();
            self.marked = (end > start).then_some(start..end);
        }
        changed
    }

    pub fn unmark(&mut self) {
        self.marked = None;
    }

    pub fn insert(&mut self, text: &str) -> bool {
        let range = self.marked.clone().unwrap_or_else(|| self.selection());
        self.replace(range, text)
    }

    /// Starts a new line (Enter). In a list it starts the next item, with
    /// the item's bullet or the next number; in an empty item it ends the
    /// list there, removing the item's marker.
    pub fn insert_newline(&mut self) -> bool {
        let range = self.marked.clone().unwrap_or_else(|| self.selection());
        if self.paragraph.list().is_none() {
            return self.replace(range, "\n");
        }
        let item_start = self.chars[..range.start]
            .iter()
            .rposition(|character| *character == '\n')
            .map_or(0, |index| index + 1);
        let list = self.paragraph().list();
        let Some((marker, length)) = list_item_marker(&self.chars[item_start..], list.as_ref())
        else {
            return self.replace(range, "\n");
        };
        let item_end = self.chars[item_start..]
            .iter()
            .position(|character| *character == '\n')
            .map_or(self.len(), |offset| item_start + offset);
        if range.is_empty() && range.start == item_end && item_start + length == item_end {
            return self.replace(item_start..item_end, "");
        }
        let marker_text = match marker {
            ListMarker::Bullet(_) => self.chars[item_start..item_start + length].to_vec(),
            ListMarker::Number { .. } => {
                // The next number; renumbering sets every later one.
                let digits = self.chars[item_start..]
                    .iter()
                    .take_while(|character| character.is_ascii_digit())
                    .count();
                let number = self.chars[item_start..item_start + digits]
                    .iter()
                    .collect::<String>()
                    .parse::<u32>()
                    .unwrap_or(0);
                (number + 1)
                    .to_string()
                    .chars()
                    .chain(
                        self.chars[item_start + digits..item_start + length]
                            .iter()
                            .copied(),
                    )
                    .collect()
            }
        };
        let before = self.snapshot();
        let break_style = self.style_for(&range);
        // The new marker is drawn as the item's: its number or bullet in the
        // marker's style, the rest in that of the item's own.
        // Matched from the end: a longer number takes the first digit's.
        let marker_styles = (0..marker_text.len())
            .map(|index| {
                let from = (item_start + length + index)
                    .checked_sub(marker_text.len())
                    .filter(|from| *from >= item_start)
                    .unwrap_or(item_start);
                self.styles[from]
            })
            .collect::<Vec<_>>();
        self.chars.splice(
            range.clone(),
            std::iter::once('\n').chain(marker_text.iter().copied()),
        );
        self.styles.splice(
            range.clone(),
            std::iter::once(break_style).chain(marker_styles),
        );
        self.caret = range.start + 1 + marker_text.len();
        self.anchor = self.caret;
        self.marked = None;
        self.renumber();
        self.commit_change(before, Some('\n'))
    }

    pub fn backspace(&mut self) -> bool {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.replace(selection, "");
        }
        if self.caret == 0 {
            return false;
        }
        self.replace(self.caret - 1..self.caret, "")
    }

    pub fn delete_forward(&mut self) -> bool {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.replace(selection, "");
        }
        if self.caret >= self.len() {
            return false;
        }
        self.replace(self.caret..self.caret + 1, "")
    }

    pub fn delete_word_backward(&mut self) -> bool {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.replace(selection, "");
        }
        let start = self.word_start(self.caret);
        self.replace(start..self.caret, "")
    }

    pub fn delete_word_forward(&mut self) -> bool {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.replace(selection, "");
        }
        let end = self.word_end(self.caret);
        self.replace(self.caret..end, "")
    }

    /// Deletes to the start of the caret's wrapped line.
    pub fn delete_to_start(&mut self) -> bool {
        let selection = self.selection();
        if !selection.is_empty() {
            return self.replace(selection, "");
        }
        let start = self.line_start(self.caret);
        self.replace(start..self.caret, "")
    }

    pub fn move_to(&mut self, index: usize, extend: bool) {
        self.caret = index.min(self.len());
        if !extend {
            self.anchor = self.caret;
        }
        self.marked = None;
    }

    pub fn move_left(&mut self, extend: bool) {
        let selection = self.selection();
        let target = if !extend && !selection.is_empty() {
            selection.start
        } else {
            self.caret.saturating_sub(1)
        };
        self.move_to(target, extend);
    }

    pub fn move_right(&mut self, extend: bool) {
        let selection = self.selection();
        let target = if !extend && !selection.is_empty() {
            selection.end
        } else {
            self.caret + 1
        };
        self.move_to(target, extend);
    }

    pub fn move_word_left(&mut self, extend: bool) {
        self.move_to(self.word_start(self.caret), extend);
    }

    pub fn move_word_right(&mut self, extend: bool) {
        self.move_to(self.word_end(self.caret), extend);
    }

    fn line_start(&self, index: usize) -> usize {
        self.wrapped
            .get(self.line_of(index))
            .map_or(0, |line| line.range.start)
    }

    fn line_end(&self, index: usize) -> usize {
        self.wrapped
            .get(self.line_of(index))
            .map_or(self.len(), |line| line.range.end)
    }

    /// Home: the start of the caret's wrapped line.
    pub fn move_line_start(&mut self, extend: bool) {
        self.move_to(self.line_start(self.caret), extend);
    }

    /// End: the end of the caret's wrapped line.
    pub fn move_line_end(&mut self, extend: bool) {
        self.move_to(self.line_end(self.caret), extend);
    }

    /// Up or down a wrapped line, keeping the caret's x; past the first or
    /// last line, to the paragraph's start or end.
    pub fn move_vertically(&mut self, down: bool, extend: bool) {
        if self.wrapped.is_empty() {
            return;
        }
        let line = self.line_of(self.caret);
        let x = self.x_on(line, self.caret);
        let target = if down {
            if line + 1 < self.wrapped.len() {
                self.index_on_line(line + 1, x)
            } else {
                self.len()
            }
        } else if line > 0 {
            self.index_on_line(line - 1, x)
        } else {
            0
        };
        self.move_to(target, extend);
    }

    pub fn select_all(&mut self) {
        self.anchor = 0;
        self.caret = self.len();
        self.marked = None;
    }

    /// Selects the wrapped line holding `index`.
    pub fn select_line_at(&mut self, index: usize) {
        self.anchor = self.line_start(index);
        self.caret = self.line_end(index);
        self.marked = None;
    }

    /// Selects the word (or run of spaces or punctuation) at `index`.
    pub fn select_word_at(&mut self, index: usize) {
        if self.chars.is_empty() {
            return;
        }
        let index = index.min(self.len() - 1);
        let class = char_class(self.chars[index]);
        let mut start = index;
        while start > 0 && char_class(self.chars[start - 1]) == class {
            start -= 1;
        }
        let mut end = index + 1;
        while end < self.len() && char_class(self.chars[end]) == class {
            end += 1;
        }
        self.anchor = start;
        self.caret = end;
        self.marked = None;
    }

    fn word_start(&self, from: usize) -> usize {
        let mut index = from.min(self.len());
        while index > 0 && char_class(self.chars[index - 1]) == CharClass::Space {
            index -= 1;
        }
        if index > 0 {
            let class = char_class(self.chars[index - 1]);
            while index > 0 && char_class(self.chars[index - 1]) == class {
                index -= 1;
            }
        }
        index
    }

    fn word_end(&self, from: usize) -> usize {
        let mut index = from.min(self.len());
        while index < self.len() && char_class(self.chars[index]) == CharClass::Space {
            index += 1;
        }
        if index < self.len() {
            let class = char_class(self.chars[index]);
            while index < self.len() && char_class(self.chars[index]) == class {
                index += 1;
            }
        }
        index
    }

    pub fn selected_text(&self) -> String {
        self.chars[self.selection()].iter().collect()
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.restore(snapshot);
        true
    }

    pub fn utf16_offset(&self, index: usize) -> usize {
        self.chars[..index.min(self.len())]
            .iter()
            .map(|character| character.len_utf16())
            .sum()
    }

    pub fn index_from_utf16(&self, offset: usize) -> usize {
        let mut utf16 = 0;
        for (index, character) in self.chars.iter().enumerate() {
            if utf16 >= offset {
                return index;
            }
            utf16 += character.len_utf16();
        }
        self.len()
    }

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.index_from_utf16(range.start)..self.index_from_utf16(range.end)
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.utf16_offset(range.start)..self.utf16_offset(range.end)
    }
}

/// Maps page user space to window pixels, recorded when the overlay
/// paints, so the IME can place its candidate window.
#[derive(Clone, Copy, Debug)]
pub struct PageTextScreenMap {
    /// Page user space to window pixels.
    pub matrix: [f64; 6],
}

impl PageTextScreenMap {
    fn point(&self, x: f64, y: f64) -> (f64, f64) {
        let m = self.matrix;
        (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
    }
}

pub enum PageTextInputEvent {
    /// The edited text changed.
    Changed,
    /// Escape: keep the edit and stop editing.
    Finish,
    /// The line's fonts cannot draw a typed character.
    Rejected(char),
    /// Tab (`back`: Shift-Tab): the next or previous table cell.
    Tab { back: bool },
    /// Backspace in an empty paragraph: remove it and continue the
    /// paragraph before.
    JoinPrevious,
}

/// Receives keyboard and IME input for the line being edited.
pub struct PageTextInput {
    focus_handle: FocusHandle,
    pub state: Option<PageTextEditState>,
    pub screen_map: Option<PageTextScreenMap>,
}

impl EventEmitter<PageTextInputEvent> for PageTextInput {}

impl Focusable for PageTextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl PageTextInput {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            state: None,
            screen_map: None,
        }
    }

    fn edit_with(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut PageTextEditState) -> bool,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if change(state) {
            cx.emit(PageTextInputEvent::Changed);
        } else if let Some(character) = state.rejected() {
            cx.emit(PageTextInputEvent::Rejected(character));
        }
        cx.notify();
    }

    /// Restyles the selection, or the paragraph (text properties).
    pub fn restyle(
        &mut self,
        change: impl Fn(&TextRunStyle) -> TextRunStyle,
        cx: &mut Context<Self>,
    ) {
        self.edit_with(cx, |state| state.restyle(change));
    }

    /// Sets the paragraph's alignment and line spacing.
    pub fn set_format(&mut self, format: ParagraphFormat, cx: &mut Context<Self>) {
        self.edit_with(cx, |state| state.set_format(format));
    }

    fn move_with(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut PageTextEditState)) {
        if let Some(state) = self.state.as_mut() {
            change(state);
            cx.notify();
        }
    }

    pub fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.state.as_ref().is_some_and(PageTextEditState::is_empty) {
            cx.emit(PageTextInputEvent::JoinPrevious);
            return;
        }
        self.edit_with(cx, PageTextEditState::backspace);
    }

    pub fn delete_forward(&mut self, _: &DeleteForward, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_with(cx, PageTextEditState::delete_forward);
    }

    pub fn delete_word_backward(
        &mut self,
        _: &DeleteWordBackward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_with(cx, PageTextEditState::delete_word_backward);
    }

    pub fn delete_word_forward(
        &mut self,
        _: &DeleteWordForward,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_with(cx, PageTextEditState::delete_word_forward);
    }

    pub fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_with(cx, PageTextEditState::delete_to_start);
    }

    pub fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_left(false));
    }

    pub fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_right(false));
    }

    pub fn move_word_left(&mut self, _: &MoveWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_word_left(false));
    }

    pub fn move_word_right(&mut self, _: &MoveWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_word_right(false));
    }

    pub fn move_line_start(&mut self, _: &MoveLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_line_start(false));
    }

    pub fn move_line_end(&mut self, _: &MoveLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_line_end(false));
    }

    pub fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_vertically(false, false));
    }

    pub fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_vertically(true, false));
    }

    pub fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_vertically(false, true));
    }

    pub fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_vertically(true, true));
    }

    pub fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_with(cx, PageTextEditState::insert_newline);
    }

    pub fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_left(true));
    }

    pub fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_right(true));
    }

    pub fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_word_left(true));
    }

    pub fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_with(cx, |state| state.move_word_right(true));
    }

    pub fn select_line_start(
        &mut self,
        _: &SelectLineStart,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_with(cx, |state| state.move_line_start(true));
    }

    pub fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, |state| state.move_line_end(true));
    }

    pub fn select_all(&mut self, _: &SelectAllText, _: &mut Window, cx: &mut Context<Self>) {
        self.move_with(cx, PageTextEditState::select_all);
    }

    pub fn copy(&mut self, _: &CopyText, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = &self.state {
            let text = state.selected_text();
            if !text.is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }

    pub fn cut(&mut self, _: &CutText, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = &self.state {
            let text = state.selected_text();
            if text.is_empty() {
                return;
            }
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        self.edit_with(cx, |state| state.replace(state.selection(), ""));
    }

    pub fn paste(&mut self, _: &PasteText, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        // Line breaks become hard breaks in the paragraph.
        self.edit_with(cx, |state| state.insert(&text));
    }

    pub fn undo(&mut self, _: &UndoText, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_with(cx, PageTextEditState::undo);
    }

    pub fn redo(&mut self, _: &RedoText, _: &mut Window, cx: &mut Context<Self>) {
        self.edit_with(cx, PageTextEditState::redo);
    }

    pub fn finish(&mut self, _: &FinishEditing, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PageTextInputEvent::Finish);
    }

    pub fn next_cell(&mut self, _: &NextCell, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PageTextInputEvent::Tab { back: false });
    }

    pub fn previous_cell(&mut self, _: &PreviousCell, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PageTextInputEvent::Tab { back: true });
    }

    /// Types `text` at the caret, as one undoable step.
    pub fn type_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let changed = self.state.as_mut().is_some_and(|state| state.insert(text));
        if changed {
            cx.emit(PageTextInputEvent::Changed);
            cx.notify();
        }
    }

    /// Puts the caret at the end of the paragraph.
    pub fn move_to_end(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.state.as_mut() {
            state.move_to(state.len(), false);
            cx.notify();
        }
    }

    /// Selects all of the paragraph's text.
    pub fn select_all_text(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.state.as_mut() {
            state.select_all();
            cx.notify();
        }
    }
}

impl EntityInputHandler for PageTextInput {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let state = self.state.as_ref()?;
        let range = state.range_from_utf16(&range);
        adjusted_range.replace(state.range_to_utf16(&range));
        Some(state.chars[range].iter().collect())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let state = self.state.as_ref()?;
        Some(UTF16Selection {
            range: state.range_to_utf16(&state.selection()),
            reversed: state.caret < state.anchor,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        let state = self.state.as_ref()?;
        state
            .marked
            .as_ref()
            .map(|range| state.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.state.as_mut() {
            state.unmark();
            cx.notify();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_with(cx, |state| {
            let range = range
                .as_ref()
                .map(|range| state.range_from_utf16(range))
                .or_else(|| state.marked.clone())
                .unwrap_or_else(|| state.selection());
            state.replace(range, text)
        });
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.edit_with(cx, |state| {
            let range = range
                .as_ref()
                .map(|range| state.range_from_utf16(range))
                .or_else(|| state.marked.clone())
                .unwrap_or_else(|| state.selection());
            let start = range.start;
            let changed = state.replace_marked(range, new_text);
            if changed && let Some(selected) = new_selected_range {
                let base = state.utf16_offset(start);
                let caret = state.index_from_utf16(base + selected.end);
                let anchor = state.index_from_utf16(base + selected.start);
                state.move_to(anchor, false);
                state.move_to(caret, true);
                state.marked = Some(start..start + new_text.chars().count());
            }
            changed
        });
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let state = self.state.as_ref()?;
        let map = self.screen_map?;
        let quad = state.range_quad(state.range_from_utf16(&range_utf16))?;
        let corners = quad.map(|(x, y)| map.point(x, y));
        let left = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
        let right = corners
            .iter()
            .map(|c| c.0)
            .fold(f64::NEG_INFINITY, f64::max);
        let top = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
        let bottom = corners
            .iter()
            .map(|c| c.1)
            .fold(f64::NEG_INFINITY, f64::max);
        Some(Bounds::new(
            point(px(left as f32), px(top as f32)),
            size(px((right - left) as f32), px((bottom - top) as f32)),
        ))
    }

    fn character_index_for_point(
        &mut self,
        position: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let state = self.state.as_ref()?;
        let map = self.screen_map?;
        let m = map.matrix;
        let determinant = m[0] * m[3] - m[1] * m[2];
        if determinant.abs() < 1e-12 {
            return None;
        }
        let x = f64::from(f32::from(position.x)) - m[4];
        let y = f64::from(f32::from(position.y)) - m[5];
        let page_x = (m[3] * x - m[2] * y) / determinant;
        let page_y = (m[0] * y - m[1] * x) / determinant;
        Some(state.utf16_offset(state.index_at_point(page_x, page_y)))
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        let state = self.state.as_ref()?;
        Some(state.utf16_offset(state.len()))
    }
}

fn lerp(a: (f64, f64), b: (f64, f64), t: f64) -> (f64, f64) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf_text_blocks::{PageTextBlock, TextBlockAlignment};
    use crate::pdf_text_edit::{
        FallbackFace, FallbackFamily, PageTextLayer, PageTextLine, TextFontMetrics, TextLineAnchor,
        TextLineChar,
    };
    use std::sync::Arc;

    fn style(font: &[u8]) -> TextRunStyle {
        TextRunStyle {
            font: font.to_vec(),
            font_size: 10.,
            char_spacing: 0.,
            word_spacing: 0.,
            fill: None,
            face: None,
        }
    }

    /// A line of `text` at `y` in fonts that draw ASCII at 500/1000 em
    /// (5 points at 10 points); characters from `bold_from` use the bold font.
    fn line(text: &str, y: f64, bold_from: usize) -> PageTextLine {
        let chars = text
            .chars()
            .enumerate()
            .map(|(index, character)| TextLineChar {
                character,
                x: index as f64 * 5.,
                advance: 5.,
                style: usize::from(index >= bold_from),
                synthetic: false,
            })
            .collect();
        PageTextLine {
            anchor: TextLineAnchor {
                x_centipoints: 7200,
                y_centipoints: (y * 100.) as i64,
                ordinal: 0,
            },
            text: text.into(),
            chars,
            end_x: text.chars().count() as f64 * 5.,
            styles: vec![style(b"F1"), style(b"F2")],
            matrix: [1., 0., 0., 1., 72., y],
            horizontal_scale: 1.,
            rise: 0.,
            ascent: 8.,
            descent: -2.,
            fill: None,
            read_only: None,
            in_form: false,
            clip_bounds: None,
        }
    }

    fn layer(lines: Vec<PageTextLine>) -> Arc<PageTextLayer> {
        let font = |name: &[u8]| TextFontMetrics {
            font: name.to_vec(),
            advances: (' '..='~').map(|character| (character, 500.)).collect(),
            word_space: Some(' '),
            fallback: FallbackFace {
                family: FallbackFamily::Sans,
                bold: false,
                italic: false,
            },
            postscript_name: None,
            ascent: 800.,
            descent: -200.,
        };
        Arc::new(PageTextLayer {
            page_index: 0,
            fonts: vec![font(b"F1"), font(b"F2")],
            lines,
            page_box: None,
            rules: Vec::new(),
            fills: Vec::new(),
            objects: Vec::new(),
        })
    }

    /// The paragraph of `lines`, left-aligned, `width` points wide.
    fn editor(lines: Vec<PageTextLine>, width: f64, caret: usize) -> PageTextEditState {
        let block = PageTextBlock {
            lines: lines.iter().map(|line| line.anchor).collect(),
            alignment: TextBlockAlignment::Left,
            left: 72.,
            right: 72. + width,
            pitch: (lines.len() > 1).then_some(14.),
            edges: Vec::new(),
            list: None,
            cell: None,
        };
        let layer = layer(lines);
        let paragraph = Paragraph::new(layer, &block).unwrap();
        PageTextEditState::new(paragraph, &[], &[], &[], caret).unwrap()
    }

    /// A line of `text` starting at `x`, `y`, in the regular font.
    fn line_at(text: &str, x: f64, y: f64) -> PageTextLine {
        let mut line = line(text, y, usize::MAX);
        line.matrix[4] = x;
        line.anchor.x_centipoints = (x * 100.) as i64;
        line
    }

    /// The editor for the block the page's lines group the first into.
    fn block_editor(lines: Vec<PageTextLine>, caret: usize) -> PageTextEditState {
        let layer = layer(lines);
        let block = crate::pdf_text_blocks::paragraph_blocks(&layer)
            .into_iter()
            .find(|block| block.lines[0] == layer.lines[0].anchor)
            .unwrap();
        let paragraph = Paragraph::new(layer, &block).unwrap();
        PageTextEditState::new(paragraph, &[], &[], &[], caret).unwrap()
    }

    /// Where line `index` of the wrapped paragraph starts drawing, in its
    /// text space.
    fn drawn_from(state: &PageTextEditState, index: usize) -> f64 {
        state.wrapped()[index]
            .layout
            .chars
            .iter()
            .find(|character| !character.character.is_whitespace())
            .unwrap()
            .x
    }

    #[test]
    fn enter_at_the_end_moves_the_caret_to_a_new_line_at_once() {
        let mut state = editor(vec![line("Hello", 700., usize::MAX)], 400., 5);
        let before = state.caret_segment().unwrap();
        assert!(state.insert_newline());
        assert_eq!(state.text(), "Hello\n");
        // An empty line follows the break before anything is typed: the
        // caret is on it, at the start, a line below.
        assert_eq!(state.wrapped().len(), 2);
        assert_eq!(state.wrapped()[1].range, 6..6);
        let after = state.caret_segment().unwrap();
        assert!(after[0].1 < before[0].1 - 5., "{before:?} {after:?}");
        assert!(after[0].0 < before[0].0, "{before:?} {after:?}");
        assert_eq!(state.frame_quads().len(), 2);
        // Typing fills that line.
        assert!(state.insert("x"));
        assert_eq!(state.wrapped().len(), 2);
        assert_eq!(state.wrapped()[1].range, 6..7);
        // Undoing the break leaves one line again.
        while state.undo() {}
        assert_eq!(state.wrapped().len(), 1);
    }

    #[test]
    fn a_line_that_wraps_shows_a_wrap_mark_not_a_return_mark() {
        // Two lines of a wrapped paragraph: "beta" did not fit after "alpha".
        let state = block_editor(
            vec![
                line("alpha alpha alpha", 600., usize::MAX),
                line("beta", 586., usize::MAX),
            ],
            0,
        );
        assert_eq!(state.text(), "alpha alpha alpha beta");
        assert!(state.break_marks().is_empty());
        // One mark of two strokes, after the first line only.
        let marks = state.wrap_marks();
        assert_eq!(marks.len(), 2);
        assert!(
            marks[0].iter().all(|(_, y)| (598.0..610.).contains(y)),
            "{marks:?}"
        );
    }

    #[test]
    fn lines_the_writer_broke_stay_broken_and_show_return_marks() {
        // The wide line sets the text column; the three short lines below
        // it are one paragraph of Shift+Enter lines.
        let mut state = block_editor(
            vec![
                line("Strategy", 600., usize::MAX),
                line("Governance", 586., usize::MAX),
                line("Testing", 572., usize::MAX),
                line(
                    "A wide line that sets how far the text column reaches",
                    700.,
                    usize::MAX,
                ),
            ],
            8,
        );
        assert_eq!(state.text(), "Strategy\nGovernance\nTesting");
        // Two marks of two strokes each: after Strategy and Governance.
        let marks = state.break_marks();
        assert_eq!(marks.len(), 4);
        assert!(state.wrap_marks().is_empty());
        let after_strategy = marks[0].iter().map(|(x, _)| *x).fold(f64::MIN, f64::max);
        assert!(after_strategy > 72. + 8. * 5., "{marks:?}");
        assert!(
            marks[0].iter().all(|(_, y)| (598.0..610.).contains(y)),
            "{marks:?}"
        );
        state.replace(8..8, " and planning");
        assert_eq!(state.text(), "Strategy and planning\nGovernance\nTesting");
        let lines = state
            .wrapped()
            .iter()
            .map(|line| state.text()[line.range.clone()].to_string())
            .collect::<Vec<_>>();
        assert_eq!(lines, ["Strategy and planning", "Governance", "Testing"]);
    }

    fn single() -> PageTextEditState {
        editor(vec![line("Hello world", 700., 6)], 55., 5)
    }

    fn texts(state: &PageTextEditState) -> Vec<String> {
        state
            .edits()
            .iter()
            .map(|edit| {
                std::iter::once(edit.text())
                    .chain(edit.layout.extra_lines.iter().map(|extra| {
                        extra
                            .runs
                            .iter()
                            .map(|run| run.text.as_str())
                            .collect::<String>()
                    }))
                    .collect::<Vec<_>>()
                    .join(" | ")
            })
            .collect()
    }

    #[test]
    fn typing_inherits_the_style_before_the_caret_and_moves_text_after_it() {
        let mut state = single();
        assert!(state.insert(","));
        assert_eq!(state.text(), "Hello, world");
        assert_eq!(state.caret(), 6);
        let runs = &state.edits()[0].runs;
        assert_eq!(runs[0].text, "Hello, ");
        assert_eq!(runs[1].text, "world");
        assert_eq!(runs[1].style, style(b"F2"));
        // A single line grows rather than wraps; "Hello" stays put.
        assert_eq!(state.wrapped().len(), 1);
        assert_eq!(state.wrapped()[0].layout.chars[0].x, 0.);
        assert_eq!(state.wrapped()[0].layout.chars[7].x, 35.);
        // Typing over a selection keeps the selected text's style.
        state.select_word_at(9);
        assert_eq!(state.selected_text(), "world");
        assert!(state.insert("there"));
        assert_eq!(state.edits()[0].runs.last().unwrap().style, style(b"F2"));
    }

    #[test]
    fn deleting_words_selection_and_undo() {
        let mut state = single();
        state.move_to(state.len(), false);
        assert!(state.delete_word_backward());
        assert_eq!(state.text(), "Hello ");
        assert!(state.undo());
        assert_eq!(state.text(), "Hello world");
        assert!(state.edits().iter().all(PageTextEdit::is_unchanged));
        assert!(state.redo());
        assert_eq!(state.text(), "Hello ");
        state.select_all();
        assert!(state.backspace());
        assert!(state.is_empty());
    }

    #[test]
    fn word_selection_and_movement_follow_character_classes() {
        let mut state = single();
        state.select_word_at(8);
        assert_eq!(state.selected_text(), "world");
        state.move_to(11, false);
        state.move_word_left(true);
        assert_eq!(state.selection(), 6..11);
        state.move_left(false);
        assert_eq!(state.caret(), 6);
        state.move_word_left(false);
        assert_eq!(state.caret(), 0);
        state.move_word_right(false);
        assert_eq!(state.caret(), 5);
    }

    #[test]
    fn characters_no_font_draws_are_refused() {
        let mut state = single();
        state.move_to(0, false);
        assert!(!state.insert("\u{10ffff}"));
        assert_eq!(state.rejected(), Some('\u{10ffff}'));
        assert_eq!(state.text(), "Hello world");
        assert_eq!(state.utf16_offset(3), 3);
        assert_eq!(state.index_from_utf16(3), 3);
    }

    #[test]
    fn points_map_to_the_nearer_caret_position() {
        let state = single();
        assert_eq!(state.index_at_point(69., 702.), 0);
        assert_eq!(state.index_at_point(83., 702.), 2);
        assert_eq!(state.index_at_point(200., 702.), 11);
        let [bottom, top] = state.caret_segment().unwrap();
        assert_eq!((bottom, top), ((97., 698.), (97., 708.)));
    }

    #[test]
    fn paragraphs_wrap_typing_into_the_next_line_and_below() {
        // Two lines, 11 characters (55 points) wide.
        let mut state = editor(
            vec![line("Hello world", 700., 99), line("two words", 686., 99)],
            55.,
            5,
        );
        assert_eq!(state.text(), "Hello world two words");
        assert!(state.insert(" big"));
        // "world" no longer fits the first line: it moves down, and the
        // paragraph grows by a line below.
        assert_eq!(texts(&state), ["Hello big", "world two | words"]);
        assert_eq!(state.wrapped().len(), 3);
        assert_eq!(state.frame_quads().len(), 3);
        // Down keeps the caret's x on the next line; up returns.
        state.move_to(2, false);
        state.move_vertically(true, false);
        assert_eq!(state.caret(), 12);
        state.move_vertically(false, false);
        assert_eq!(state.caret(), 2);
        // Enter starts a new line.
        state.move_to(5, false);
        assert!(state.insert_newline());
        assert_eq!(state.wrapped()[0].range, 0..5);
        assert_eq!(state.wrapped()[1].range.start, 6);
        // Undo returns to the paragraph as it was.
        while state.undo() {}
        assert!(state.edits().iter().all(PageTextEdit::is_unchanged));
    }

    #[test]
    fn list_items_are_one_paragraph_and_enter_starts_the_next_item() {
        let lines = vec![
            line_at("- First item runs on", 72., 700.),
            line_at("and wraps here", 82., 686.),
            line_at("- Second item", 72., 672.),
            line_at("- Third", 72., 658.),
        ];
        let layer = layer(lines.clone());
        let blocks = crate::pdf_text_blocks::paragraph_blocks(&layer);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        let list = blocks[0].list.unwrap();
        assert_eq!(list.marker, ListMarker::Bullet('-'));
        assert!((list.indent - 10.).abs() < 1e-9);

        let mut state = block_editor(lines.clone(), 0);
        assert_eq!(
            state.text(),
            "- First item runs on and wraps here\n- Second item\n- Third"
        );
        // Enter after "here" starts an item with the same bullet.
        state.move_to(35, false);
        assert!(state.insert_newline());
        assert_eq!(
            state.text(),
            "- First item runs on and wraps here\n- \n- Second item\n- Third"
        );
        assert_eq!(state.caret(), 38);
        assert!(state.insert("New"));
        // The new item is drawn at the bullets' edge, on the line below.
        let new_item = state
            .wrapped()
            .iter()
            .position(|line| state.text()[line.range.clone()].starts_with("- New"))
            .unwrap();
        assert_eq!(new_item, 2);
        assert!(drawn_from(&state, 2).abs() < 1e-9);
        // The edits keep the item breaks for reopening.
        let edits = state.edits();
        assert!(edits.iter().any(|edit| edit.layout.hard_break));
        // Enter in an empty item ends the list there: its bullet goes.
        let end = state.len();
        state.move_to(end, false);
        assert!(state.insert_newline());
        assert!(state.text().ends_with("- Third\n- "));
        assert!(state.insert_newline());
        assert!(state.text().ends_with("- Third\n"));

        // A wrapped line of an item starts at the list's indent, even on a
        // line that showed an item.
        let mut state = block_editor(lines, 35);
        assert!(state.insert(" and more words"));
        let wrapped = state
            .wrapped()
            .iter()
            .map(|line| state.text()[line.range.clone()].to_owned())
            .collect::<Vec<_>>();
        assert_eq!(wrapped[2], "more words", "{wrapped:?}");
        // Line 2 was "- Second item" at the bullets' edge; it now continues
        // the first item at the indent (10 points in, from 72).
        assert!((drawn_from(&state, 2) - 10.).abs() < 1e-9);
    }

    #[test]
    fn numbered_items_count_on_when_an_item_is_added_or_removed() {
        let lines = vec![
            line_at("1. One", 72., 700.),
            line_at("2. Two", 72., 686.),
            line_at("3. Three", 72., 672.),
        ];
        let mut state = block_editor(lines, 6);
        assert_eq!(
            state.paragraph().list().map(|list| list.marker),
            Some(ListMarker::Number { suffix: '.' })
        );
        assert!(state.insert_newline());
        assert_eq!(state.text(), "1. One\n2. \n3. Two\n4. Three");
        assert_eq!(state.caret(), 10);
        assert!(state.replace(6..10, ""));
        assert_eq!(state.text(), "1. One\n2. Two\n3. Three");
        assert!(state.undo());
        assert_eq!(state.text(), "1. One\n2. \n3. Two\n4. Three");
    }

    #[test]
    fn larger_text_opens_the_lines_below_it() {
        let mut state = editor(
            vec![
                line("Alpha beta gamma", 700., usize::MAX),
                line("delta epsilon", 686., usize::MAX),
            ],
            200.,
            0,
        );
        state.select_all();
        assert!(state.restyle(|style| TextRunStyle {
            font_size: 20.,
            ..style.clone()
        }));
        // The first line keeps its baseline; the next is 1.4 of the larger
        // size below it (28 points, to 672), 14 below the last line.
        let wrapped = state.wrapped();
        assert!(!wrapped[0].added);
        assert!(wrapped[1].added);
        assert!(
            (wrapped[1].below - 14.).abs() < 1e-9,
            "{}",
            wrapped[1].below
        );
        // The second base line is emptied; its text moved.
        assert!(
            state
                .edits()
                .iter()
                .any(|edit| edit.anchor.y_centipoints == 68600 && edit.runs.is_empty())
        );
    }

    #[test]
    fn a_paragraph_continues_from_the_foot_of_a_column_to_the_top_of_the_next() {
        let lines = vec![
            line_at("Text in the left col", 72., 700.),
            line_at("umn runs to its foot", 72., 686.),
            line_at("and carries on right", 72., 672.),
            line_at("into the next column", 300., 700.),
            line_at("where it ends.", 300., 686.),
        ];
        let layer = layer(lines.clone());
        let blocks = crate::pdf_text_blocks::paragraph_blocks(&layer);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        assert_eq!(blocks[0].lines.len(), 5);
        assert_eq!(blocks[0].edges_of(3), (300., 400.));

        // Growing adds lines below the last column's last line.
        let mut state = block_editor(lines.clone(), 0);
        let end = state.len();
        state.move_to(end, false);
        assert!(state.insert(" More text to wrap"));
        let added = state.wrapped().last().unwrap();
        assert!(added.added && (added.below - 14.).abs() < 1e-9);
        assert!(drawn_from(&state, state.wrapped().len() - 1).abs() < 1e-9);

        // At double spacing the left column holds two lines; the third
        // starts at the top of the right column.
        let mut state = block_editor(lines, 0);
        assert!(state.set_format(ParagraphFormat {
            alignment: None,
            line_spacing: Some(2.),
        }));
        let wrapped = state.wrapped();
        // In the last line's text space (at 300, 686): the second line is
        // 20 below the first (680), in the left column (72 is 228 left).
        assert!((wrapped[1].below - 6.).abs() < 1e-9, "{}", wrapped[1].below);
        assert!((drawn_from(&state, 1) + 228.).abs() < 1e-9);
        assert!(
            (wrapped[2].below + 14.).abs() < 1e-9,
            "{}",
            wrapped[2].below
        );
        assert!(drawn_from(&state, 2).abs() < 1e-9);
    }
}
