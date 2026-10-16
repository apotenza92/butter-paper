//! Selecting page text with the Select tool to copy it, as in Acrobat:
//! press and drag across text, double-click a word, triple-click a
//! paragraph, Shift-click to extend; Copy puts the text on the clipboard.
//! Markups and form fields above the text take the press first, and a press
//! away from text still draws a selection box.

use super::*;
use crate::pdf_text_edit::PageTextEdit;
use crate::pdf_text_selection::{
    SelectableLine, TextPosition, hit, nearest, paragraph_at, selectable_lines, selection_quads,
    selection_text, word_at,
};
use gpui::ClipboardItem;

/// How far beyond a line's box a press still starts selecting it, in
/// pixels.
const TEXT_SELECTION_TOLERANCE_PX: f64 = 3.;

/// Selected page text.
#[derive(Clone)]
pub(super) struct TextSelection {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub epoch: u64,
    /// The lines as they were when the selection began.
    pub lines: Arc<Vec<SelectableLine>>,
    pub anchor: TextPosition,
    pub focus: TextPosition,
    /// The press that began it is still held.
    pub dragging: bool,
}

/// A page's lines for selecting, kept until its edits change.
pub(super) type SelectableTextCache =
    HashMap<(DocumentId, u32), (u64, Vec<PageTextEdit>, Arc<Vec<SelectableLine>>)>;

impl DocumentWorkspace {
    /// The page's lines as it shows them, reading its text from the worker
    /// the first time.
    fn selectable_text(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Option<(u64, Arc<Vec<SelectableLine>>)> {
        let epoch = self.session(document_id, cx)?.read(cx).resource_epoch;
        let (layer, blocks) = self.page_text_blocks(document_id, page_index, cx)?;
        let edits = self
            .page_text_edits_of(document_id, cx)
            .into_iter()
            .filter(|edit| edit.page_index == page_index)
            .collect::<Vec<_>>();
        if let Some((cached_epoch, cached_edits, lines)) =
            self.selectable_text.get(&(document_id, page_index))
            && *cached_epoch == epoch
            && *cached_edits == edits
        {
            return Some((epoch, lines.clone()));
        }
        let lines = Arc::new(selectable_lines(&layer, &blocks, &edits));
        self.selectable_text
            .insert((document_id, page_index), (epoch, edits, lines.clone()));
        Some((epoch, lines))
    }

    /// Text under a point that a press would select: none under a markup.
    fn text_selection_hit(
        &mut self,
        interaction: &PageInteraction,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Option<(u64, Arc<Vec<SelectableLine>>, TextPosition)> {
        if self.markup_under_point(interaction, point, cx) {
            return None;
        }
        let tolerance = interaction
            .transform
            .tolerance_points(TEXT_SELECTION_TOLERANCE_PX)
            .unwrap_or(1.);
        let (epoch, lines) =
            self.selectable_text(interaction.document_id, interaction.page_index, cx)?;
        let position = hit(&lines, (point.x, point.y), tolerance)?;
        Some((epoch, lines, position))
    }

    /// Hover feedback over page text with the Select tool, where no markup
    /// or form field is. Hover and press share one hit test.
    pub(super) fn update_text_selection_hover(
        &mut self,
        interaction: &PageInteraction,
        point: Option<PdfPoint>,
        cx: &mut Context<Self>,
    ) -> bool {
        let hovered = point
            .and_then(|point| self.text_selection_hit(interaction, point, cx))
            .map(|_| interaction.document_id);
        if self.text_selection_hover != hovered {
            self.text_selection_hover = hovered;
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(super) fn text_selection_cursor(&self, document_id: DocumentId) -> Option<CursorStyle> {
        let dragging = self
            .text_selection
            .as_ref()
            .is_some_and(|selection| selection.document_id == document_id && selection.dragging);
        (dragging || self.text_selection_hover == Some(document_id)).then_some(CursorStyle::IBeam)
    }

    /// A Select-tool press in page text: place the start of a selection
    /// there (a word on double click, its paragraph on triple), or extend
    /// the selection to it with Shift. Returns false, leaving the press to
    /// markup selection, when no text is there.
    pub(super) fn begin_text_selection_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        click_count: usize,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((epoch, lines, position)) = self.text_selection_hit(&interaction, point, cx)
        else {
            self.clear_text_selection(cx);
            return false;
        };
        let extending = self.text_selection.as_ref().filter(|selection| {
            modifiers.shift
                && selection.document_id == interaction.document_id
                && selection.page_index == interaction.page_index
                && selection.epoch == epoch
                && Arc::ptr_eq(&selection.lines, &lines)
        });
        let (anchor, focus) = match (extending, click_count) {
            (Some(selection), _) => (selection.anchor, position),
            (None, 2) => word_at(&lines, position),
            (None, count) if count >= 3 => paragraph_at(&lines, position),
            (None, _) => (position, position),
        };
        // Selecting text clears a markup selection, as pressing the page
        // does.
        if let Some(session) = self.session(interaction.document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                session
                    .annotations
                    .clear_selection(interaction.document_id.value());
                cx.notify();
            });
        }
        self.text_selection = Some(TextSelection {
            document_id: interaction.document_id,
            page_index: interaction.page_index,
            epoch,
            lines,
            anchor,
            focus,
            dragging: click_count == 1,
        });
        cx.notify();
        true
    }

    /// Extends the selection while the press that began it is held.
    pub(super) fn update_text_selection_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(selection) = self.text_selection.as_ref().filter(|s| s.dragging) else {
            return false;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(selection.document_id, selection.page_index))
            .copied()
        else {
            return true;
        };
        let Some(point) = Self::interaction_point(interaction, position, false) else {
            return true;
        };
        if let Some(focus) = nearest(&selection.lines, (point.x, point.y))
            && let Some(selection) = self.text_selection.as_mut()
            && selection.focus != focus
        {
            selection.focus = focus;
            cx.notify();
        }
        true
    }

    pub(super) fn finish_text_selection_drag(&mut self, cx: &mut Context<Self>) -> bool {
        match self.text_selection.as_mut() {
            Some(selection) if selection.dragging => {
                selection.dragging = false;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(super) fn clear_text_selection(&mut self, cx: &mut Context<Self>) {
        if self.text_selection.take().is_some() {
            cx.notify();
        }
    }

    /// The selected text in a document, when there is any.
    pub fn selected_page_text(&self, document_id: DocumentId, cx: &App) -> Option<String> {
        let selection = self
            .text_selection
            .as_ref()
            .filter(|selection| selection.document_id == document_id)?;
        let epoch = self.session(document_id, cx)?.read(cx).resource_epoch;
        (selection.epoch == epoch)
            .then(|| selection_text(&selection.lines, selection.anchor, selection.focus))
            .filter(|text| !text.is_empty())
    }

    /// Copies selected page text. Returns false when none is selected.
    pub(super) fn copy_selected_page_text(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(text) = self.selected_page_text(document_id, cx) else {
            return false;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        true
    }

    /// The selection's highlight, drawn over its page.
    pub(super) fn text_selection_overlays(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Vec<page_text::PageTextOverlay> {
        let Some(selection) = self
            .text_selection
            .as_ref()
            .filter(|selection| selection.document_id == document_id)
        else {
            return Vec::new();
        };
        if self
            .session(document_id, cx)
            .is_none_or(|session| session.read(cx).resource_epoch != selection.epoch)
        {
            return Vec::new();
        }
        let quads = selection_quads(&selection.lines, selection.anchor, selection.focus);
        if quads.is_empty() {
            return Vec::new();
        }
        vec![page_text::PageTextOverlay {
            page_index: selection.page_index,
            paragraphs: Vec::new(),
            hover: Vec::new(),
            frame: Vec::new(),
            frame_color: None,
            selection: quads,
            caret: None,
            marked: Vec::new(),
            breaks: Vec::new(),
            wraps: Vec::new(),
            input: None,
        }]
    }
}
