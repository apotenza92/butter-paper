//! A click-out only dismisses (interactions.md, "Click-out"). When a press on
//! the canvas dismisses something, that press and its release do nothing
//! else: no selection box, no lasso, no new markup, no tool action and no text
//! caret. Popups, menus and dialogs follow the same rule through
//! `overlay_state::press_owned_by_overlay`; this module answers the question
//! for the canvas's own states, in one place, before any gesture starts.

use gpui::{Context, Modifiers};

use super::{DocumentId, DocumentWorkspace, PageInteraction};
use crate::annotation_model::PdfPoint;

/// What a press on the canvas dismisses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CanvasDismissal {
    /// Highlighted page text (Select tool). A press in page text is a new
    /// text selection instead, and Shift extends the highlight.
    TextHighlight,
    /// Selected markups (Select tool), dismissed by a press on page text, a
    /// form field or a link. A press on another markup changes the
    /// selection, and a press on empty page already dismisses alone in the
    /// annotation model.
    MarkupSelection,
}

/// The state of a press that dismissed something, kept until its release.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ClickOutState {
    /// The press dismissed something: its drag and release do nothing.
    pub(super) held: bool,
    /// The click count of the last dismissing press. Further clicks of the
    /// same multi-click sequence count from the one after it, so a double
    /// click that began by dismissing is a single click.
    pub(super) dismissed_click_count: Option<usize>,
}

impl DocumentWorkspace {
    /// The click count a press acts on: a click of a sequence that began by
    /// dismissing something counts from the click after the dismissal.
    pub(super) fn click_count_after_dismissal(&mut self, click_count: usize) -> usize {
        match self.click_out.dismissed_click_count {
            Some(dismissed) if click_count > dismissed => click_count - dismissed,
            _ => {
                self.click_out.dismissed_click_count = None;
                click_count
            }
        }
    }

    /// Records a press that dismissed something, which the canvas, an
    /// overlay or a text editor consumed.
    pub(super) fn note_dismissing_press(&mut self, click_count: usize) {
        self.click_out.held = true;
        self.click_out.dismissed_click_count = Some(click_count);
    }

    /// Records a press that an open popup, menu or dialog owned. It counts
    /// for multi-click sequences only: the popup has its own drag and release.
    pub(super) fn note_overlay_press(&mut self, click_count: usize) {
        self.click_out.dismissed_click_count = Some(click_count);
    }

    /// Whether a press with the Select tool at `point` dismisses something.
    /// Hover and press share this hit test: the cursor and the status bar
    /// read it too (`click_out_pending`).
    pub(super) fn canvas_press_dismissal(
        &mut self,
        interaction: &PageInteraction,
        point: PdfPoint,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> Option<CanvasDismissal> {
        let document_id = interaction.document_id;
        // Shift and Alt add, extend or remove; Cmd/Ctrl draws a box anywhere.
        // They change a selection on purpose and never only dismiss.
        if modifiers.shift || modifiers.alt || modifiers.secondary() {
            return None;
        }
        if self.editing_content(document_id, cx) {
            // Edit PDF: a press on empty page already ends the edit or
            // selection and does no more (`begin_content_press`); text and
            // objects are selections of their own.
            return None;
        }
        if self.text_highlighted(document_id, cx) {
            return (!self.text_selection_hit_at(interaction, point, cx))
                .then_some(CanvasDismissal::TextHighlight);
        }
        let markups_selected = self.session(document_id, cx).is_some_and(|session| {
            session
                .read(cx)
                .annotations
                .has_selection(document_id.value())
        });
        if markups_selected
            && !self.markup_under_point(interaction, point, cx)
            && (self.text_selection_hit_at(interaction, point, cx)
                || self.form_widget_hit(interaction, point, cx).is_some()
                || self.link_hit(interaction, point, cx).is_some())
        {
            return Some(CanvasDismissal::MarkupSelection);
        }
        None
    }

    /// Dismisses what a press dismisses, and nothing else.
    pub(super) fn dismiss_on_press(
        &mut self,
        dismissal: CanvasDismissal,
        document_id: DocumentId,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        match dismissal {
            CanvasDismissal::TextHighlight => self.clear_text_selection(cx),
            CanvasDismissal::MarkupSelection => {
                if let Some(session) = self.session(document_id, cx).cloned() {
                    session.update(cx, |session, cx| {
                        session.annotations.clear_selection(document_id.value());
                        cx.notify();
                    });
                }
            }
        }
        self.note_dismissing_press(click_count);
        cx.notify();
    }

    /// Ends a dismissing press's drag: it moves nothing.
    pub(super) fn click_out_drag(&self) -> bool {
        self.click_out.held
    }

    /// Ends a dismissing press at its release: the release does nothing.
    pub(super) fn finish_click_out(&mut self) -> bool {
        std::mem::take(&mut self.click_out.held)
    }

    /// What the status bar says in place of the Select tool's box hint while
    /// a click on empty page would only dismiss something: highlighted page
    /// text or selected markups. The next click starts a box.
    pub fn click_out_status(
        &self,
        document_id: DocumentId,
        cx: &gpui::App,
    ) -> Option<&'static str> {
        let session = self.session(document_id, cx)?.read(cx);
        if session.annotations.tool() != crate::annotation_adapter::AnnotationTool::Select
            || self.pan_tool_active
            || self.editing_content(document_id, cx)
        {
            return None;
        }
        if self.text_highlighted(document_id, cx) {
            Some(super::CLICK_OUT_TEXT_STATUS)
        } else if session.annotations.has_selection(document_id.value()) {
            Some(super::CLICK_OUT_MARKUP_STATUS)
        } else {
            None
        }
    }

    /// Whether page text is highlighted in the document: a selection with
    /// text in it, not only a caret.
    pub fn text_highlighted(&self, document_id: DocumentId, cx: &gpui::App) -> bool {
        self.selected_page_text(document_id, cx).is_some()
    }
}
