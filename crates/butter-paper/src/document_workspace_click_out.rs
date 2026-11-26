//! A click-out only dismisses (interactions.md, "Click-out"). When a press on
//! the canvas dismisses something, that press and its release do nothing
//! else: no selection box, no lasso, no new markup, no tool action and no text
//! caret. Popups, menus and dialogs follow the same rule through
//! `overlay_state::press_owned_by_overlay`; this module answers the question
//! for the canvas's own states, in one place, before any gesture starts.
//!
//! The rule is positive (`decide_press`): a left press on the page acts only
//! when the page has the user's attention. Nothing is open over it, no text
//! is typed anywhere in the window, the window was already active (macOS),
//! and, with Select, nothing is selected that the press only dismisses.

use gpui::{App, Context, Modifiers, Pixels, Point, Window};
use gpui_component::WindowExt as _;

use super::{
    AnnotationTool, DocumentId, DocumentWorkspace, NativeDocumentSaveStatus, NativeDocumentStatus,
    PageInteraction, PageViewMode, PendingTextEditorTarget,
};
use crate::annotation_model::PdfPoint;

/// What a left press on the page may do. Hover, cursor, status bar and press
/// all read it, so the pointer never promises a gesture the press refuses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CanvasPressGate {
    /// The press is not the canvas's: a modal canvas state owns it, it lands
    /// outside the page, or it lands in the editor that holds the text.
    Pass,
    /// The press only dismisses or finishes, then does nothing until release.
    ClickOut(ClickOut),
    /// The press may start the gesture that the tool and the target give.
    Act,
    /// The press lands on a handle or the body of the markup whose text is
    /// being edited in place, outside the editor: the text is committed (one
    /// undo step for the text) and the press then acts as `Act` does, so a
    /// move, resize or handle drag starts with no extra click.
    CommitThenAct,
}

/// Why a press only dismisses or finishes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClickOut {
    /// The press activated the window (macOS `first_mouse`).
    WindowActivation,
    /// Text is typed in the Text Box or caption editor.
    CanvasText,
    /// Text is typed in a form field, and the press is not on a field.
    FormField,
    /// Text is typed in a field outside the page: the page field, a property
    /// or default, or any other docked text field.
    ChromeText,
    /// Highlighted page text or selected markups.
    Canvas(CanvasDismissal),
}

/// Where the text being typed in the window is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TypingTarget {
    /// The Text Box or caption editor (`pending_text_box_editor`).
    CanvasText,
    /// The editor of a form field being filled.
    FormField,
    /// Any other text field of the window.
    ChromeText,
}

/// The facts a press is judged on, read before focus moves.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PressFacts {
    /// The press activated an inactive window, on a platform where such a
    /// press does not act (macOS).
    pub first_mouse: bool,
    /// The signature surface, a pending close or rotation, the signature
    /// box prompt or the find bar owns the press.
    pub modal_canvas_state: bool,
    /// The press can concern the canvas: it lands on the active document's
    /// viewport, or anywhere while a new Text Box is typed on the page.
    pub in_reach: bool,
    pub typing: Option<TypingTarget>,
    /// The press lands in the editor that holds the typed text.
    pub inside_typing_target: bool,
    /// The press lands on a form field (no markup above it).
    pub on_form_field: bool,
    /// The press lands on a handle, or on the body outside the editor, of the
    /// markup whose text is edited in place.
    pub on_edited_markup: bool,
    pub canvas_dismissal: Option<CanvasDismissal>,
}

/// The one rule for a left press on the page. Pure, so it is tested without
/// a window. One press gives one cause, in this order: window activation,
/// typed text, then what the canvas has selected.
pub(super) fn decide_press(facts: &PressFacts) -> CanvasPressGate {
    if facts.modal_canvas_state || !facts.in_reach {
        return CanvasPressGate::Pass;
    }
    if facts.inside_typing_target && facts.typing == Some(TypingTarget::CanvasText) {
        // It places the caret in the editor.
        return CanvasPressGate::Pass;
    }
    if facts.first_mouse {
        return CanvasPressGate::ClickOut(ClickOut::WindowActivation);
    }
    if facts.typing == Some(TypingTarget::CanvasText) && facts.on_edited_markup {
        return CanvasPressGate::CommitThenAct;
    }
    match facts.typing {
        Some(TypingTarget::CanvasText) => return CanvasPressGate::ClickOut(ClickOut::CanvasText),
        Some(TypingTarget::FormField) if !facts.on_form_field && !facts.inside_typing_target => {
            return CanvasPressGate::ClickOut(ClickOut::FormField);
        }
        Some(TypingTarget::ChromeText) => return CanvasPressGate::ClickOut(ClickOut::ChromeText),
        Some(TypingTarget::FormField) | None => {}
    }
    match facts.canvas_dismissal {
        Some(dismissal) => CanvasPressGate::ClickOut(ClickOut::Canvas(dismissal)),
        None => CanvasPressGate::Act,
    }
}

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
    /// The text being typed, as the last draw saw it. Hover, the cursor and
    /// the status bar read it; a press reads focus itself.
    pub(super) typing: Option<TypingTarget>,
    /// The click count of the last dismissing press. Further clicks of the
    /// same multi-click sequence count from the one after it, so a double
    /// click that began by dismissing is a single click.
    pub(super) dismissed_click_count: Option<usize>,
}

impl DocumentWorkspace {
    /// The text being typed in the window, read from focus. Any text field
    /// counts with no registration; only the canvas's own editors are named.
    /// While a calibration line is picked, the Page Scale panel's fields do
    /// not stop the pick.
    pub(super) fn typing_target(&self, window: &mut Window, cx: &mut App) -> Option<TypingTarget> {
        if self.pending_text_box_editor.is_some() {
            return Some(TypingTarget::CanvasText);
        }
        // Text fields register with the window's root; a window without one
        // (some tests) has none.
        if !matches!(window.root::<gpui_component::Root>(), Some(Some(_))) {
            return None;
        }
        let focus = window.focused_input(cx)?.focus_handle(cx);
        // Only a field the last frame drew in the workspace is typed in: an
        // editor removed while it held focus (a Text Box committed by
        // closing the app) keeps focus but is gone.
        if !self.workspace_focus.contains(&focus, window) {
            return None;
        }
        if self
            .form_field_editor
            .as_ref()
            .is_some_and(|editor| editor.input.focus_handle(cx) == focus)
        {
            return Some(TypingTarget::FormField);
        }
        (!self.page_scale_picking(cx)).then_some(TypingTarget::ChromeText)
    }

    /// Notes the text being typed for hover, the cursor and the status bar,
    /// once each draw.
    pub(super) fn observe_typing(&mut self, window: &mut Window, cx: &mut App) {
        self.click_out.typing = self.typing_target(window, cx);
    }

    /// The page under a press or the pointer, on the active document's
    /// viewport.
    pub(super) fn page_interaction_at(&self, position: Point<Pixels>) -> Option<PageInteraction> {
        self.page_interactions
            .values()
            .copied()
            .find(|interaction| {
                Some(interaction.document_id) == self.active_document_id
                    && interaction.bounds.contains(&position)
                    && self
                        .viewport_bounds
                        .get(&interaction.document_id)
                        .is_some_and(|bounds| bounds.contains(&position))
            })
    }

    /// Whether the document cannot take an edit now: it is not ready, a save
    /// is in progress, its recovery copy is in preparation, or a save prompt
    /// is open for it.
    pub(super) fn canvas_press_blocked(&self, document_id: DocumentId, cx: &App) -> bool {
        let Some(session) = self.session(document_id, cx) else {
            return true;
        };
        let session = session.read(cx);
        !matches!(session.status, NativeDocumentStatus::Ready)
            || session.save_status == NativeDocumentSaveStatus::Saving
            || session.recovery_preparation.edit_guard().is_err()
            || self.pending_save_prompt.is_some_and(|pending| {
                pending.document_id == document_id
                    && pending.document_generation == session.generation
            })
    }

    /// Judges a left press on the page (`decide_press`) from the facts
    /// before focus moves. `typing` is read from focus by the caller.
    pub(super) fn canvas_press_gate(
        &mut self,
        position: Point<Pixels>,
        modifiers: Modifiers,
        first_mouse: bool,
        typing: Option<TypingTarget>,
        cx: &mut Context<Self>,
    ) -> CanvasPressGate {
        let modal_canvas_state = self.signature_popover_open
            || self.pending_close_document_id.is_some()
            || self
                .active_document_id
                .and_then(|document_id| self.session(document_id, cx))
                .is_some_and(|session| session.read(cx).pending_rotation_generation.is_some())
            || self.signature_box_prompt_owns(position)
            || self.find_bar_owns(position);
        let on_document = self
            .active_document_id
            .and_then(|document_id| self.viewport_bounds.get(&document_id))
            .is_some_and(|bounds| bounds.contains(&position));
        // A Text Box typed on the page finishes on a press anywhere; text
        // edited in the side panel finishes on a press on the document.
        let new_text_box = matches!(
            self.pending_text_box_editor
                .as_ref()
                .map(|editor| &editor.target),
            Some(PendingTextEditorTarget::NewTextBox { .. })
        );
        let inside_typing_target = typing == Some(TypingTarget::CanvasText)
            && self
                .pending_text_editor_window_bounds(cx)
                .is_some_and(|bounds| bounds.contains(&position));
        let mut facts = PressFacts {
            first_mouse,
            modal_canvas_state,
            in_reach: on_document || new_text_box,
            typing,
            inside_typing_target,
            ..PressFacts::default()
        };
        if let Some(interaction) = self.page_interaction_at(position)
            && let Some(point) = Self::interaction_point(interaction, position, true)
        {
            facts.on_form_field = typing == Some(TypingTarget::FormField)
                && !self.markup_under_point(&interaction, point, cx)
                && self.form_widget_hit(&interaction, point, cx).is_some();
            facts.on_edited_markup = typing == Some(TypingTarget::CanvasText)
                && self.press_on_edited_markup(&interaction, point, cx);
            facts.canvas_dismissal =
                self.select_press_dismissal(&interaction, point, modifiers, cx);
        }
        decide_press(&facts)
    }

    /// What a Select press dismisses before it can act: none while Pan,
    /// a calibration pick or an armed box takes the press, or while the
    /// document cannot take an edit.
    fn select_press_dismissal(
        &mut self,
        interaction: &PageInteraction,
        point: PdfPoint,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> Option<CanvasDismissal> {
        let document_id = interaction.document_id;
        if self.pan_tool_active
            || self.page_scale_picking(cx)
            || self.canvas_press_blocked(document_id, cx)
            || self.annotation_tool(document_id, cx) != Some(AnnotationTool::Select)
            || self
                .active_selection_marquee(document_id, cx)
                .is_some_and(|(_, marquee)| {
                    marquee.shape == crate::selection_geometry::SelectionShape::Box
                })
        {
            return None;
        }
        self.canvas_press_dismissal(interaction, point, modifiers, cx)
    }

    /// Runs a click-out: it dismisses or finishes, gives the keyboard to the
    /// page, and does nothing else until its release.
    pub(super) fn click_out_on_press(
        &mut self,
        cause: ClickOut,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cause != ClickOut::WindowActivation {
            // The search panel's field must not take the focus back.
            self.release_search_query_focus();
        }
        match cause {
            // The press only activates the window; focus stays where the
            // window restores it.
            ClickOut::WindowActivation => {}
            ClickOut::CanvasText => {
                if let Err(error) = self.commit_pending_text_box(cx) {
                    self.text_box_commit_error = Some(error);
                } else {
                    self.workspace_focus.focus(window, cx);
                }
            }
            // The focus moves to the page whatever docked panel is open: a
            // field that kept it would make every press a click-out. No
            // overlay is open (`press_owned_by_overlay` took such a press).
            ClickOut::FormField => {
                self.finish_form_filling(cx);
                self.workspace_focus.focus(window, cx);
            }
            // The field commits or cancels on its own blur.
            ClickOut::ChromeText => self.workspace_focus.focus(window, cx),
            ClickOut::Canvas(dismissal) => {
                self.claim_canvas_focus(window, cx);
                if let Some(document_id) = self.active_document_id {
                    self.dismiss_on_press(dismissal, document_id, click_count, cx);
                }
            }
        }
        self.note_dismissing_press(click_count);
        cx.notify();
    }

    /// Whether the pointer over `point` offers nothing but a click-out:
    /// hover, the cursor and the status bar agree with the press.
    pub(super) fn hover_click_out(
        &mut self,
        interaction: &PageInteraction,
        point: PdfPoint,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let typing = self.click_out.typing;
        let facts = PressFacts {
            in_reach: true,
            typing,
            on_form_field: typing == Some(TypingTarget::FormField)
                && !self.markup_under_point(interaction, point, cx)
                && self.form_widget_hit(interaction, point, cx).is_some(),
            on_edited_markup: typing == Some(TypingTarget::CanvasText)
                && self.press_on_edited_markup(interaction, point, cx),
            canvas_dismissal: self.select_press_dismissal(interaction, point, modifiers, cx),
            ..PressFacts::default()
        };
        !matches!(
            decide_press(&facts),
            CanvasPressGate::Act | CanvasPressGate::CommitThenAct
        )
    }

    /// Whether `point` is on a handle, or on the body, of the markup whose
    /// text is being edited in place (a Text Box, Callout, Cloud+ or
    /// Dimension). A press there finishes the text and starts its move or
    /// resize at once. A new Text Box being drawn is no markup yet; empty
    /// page and other markups are not the edited markup's.
    pub(super) fn press_on_edited_markup(
        &self,
        interaction: &PageInteraction,
        point: PdfPoint,
        cx: &App,
    ) -> bool {
        let Some(editor) = self.pending_text_box_editor.as_ref() else {
            return false;
        };
        if editor.document_id != interaction.document_id
            || editor.page_index != interaction.page_index
            || matches!(editor.target, PendingTextEditorTarget::NewTextBox { .. })
        {
            return false;
        }
        let text_system = gpui::WindowTextSystem::new(cx.text_system().clone());
        let Some((Some(edited), _, _)) = self.pending_text_editor_layout(&text_system, cx) else {
            return false;
        };
        let Some(session) = self.session(interaction.document_id, cx) else {
            return false;
        };
        let Ok(tolerance) = interaction.transform.tolerance_points(4.) else {
            return false;
        };
        let annotations = &session.read(cx).annotations;
        let document = interaction.document_id.value();
        if annotations.tool() != AnnotationTool::Select {
            return false;
        }
        let handle = annotations
            .hover_transform_handle(document, interaction.page_index, point, tolerance)
            .ok()
            .flatten()
            .is_some_and(|(id, _)| id == edited);
        handle
            || annotations
                .hover_markup_id(document, interaction.page_index, point, tolerance)
                .ok()
                .flatten()
                .is_some_and(|id| id == edited)
    }

    /// Whether a press anywhere on the page only finishes text typed in a
    /// field outside it: the viewport shows the arrow, not a tool cursor.
    pub(super) fn chrome_typing_click_out(&self) -> bool {
        self.click_out.typing == Some(TypingTarget::ChromeText)
    }

    /// Moves the keyboard to the page, unless an overlay interaction keeps it
    /// (`canvas_focus_claim_allowed`).
    pub(super) fn claim_canvas_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.canvas_focus_claim_allowed(window, cx) {
            self.workspace_focus.focus(window, cx);
        }
    }

    /// Continuous view: a left press on a page makes it the current page
    /// (page field, thumbnail, Rotate and page scale), with no scroll. It
    /// holds until the view scrolls. Single Page view shows one page, which
    /// is current already.
    pub(super) fn pick_current_page_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(interaction) = self.page_interaction_at(position) else {
            return;
        };
        let Some(session) = self.session(interaction.document_id, cx).cloned() else {
            return;
        };
        let picked = session.update(cx, |session, cx| {
            if session.view_state.mode() != PageViewMode::Continuous
                || !matches!(session.status, NativeDocumentStatus::Ready)
                || interaction.page_index as usize >= session.page_count()
            {
                return false;
            }
            session.picked_page_scroll = Some(session.viewer.logical_scroll());
            if session.current_page == interaction.page_index {
                return false;
            }
            session.current_page = interaction.page_index;
            session.requested_page = interaction.page_index;
            cx.notify();
            true
        });
        if picked {
            if self.left_panel == Some(super::search_panel::LeftPanel::Thumbnails) {
                self.thumbnail_scroll.scroll_to_item(
                    interaction.page_index as usize,
                    gpui::ScrollStrategy::Nearest,
                );
            }
            self.sync_active_viewer_toolbar(cx);
            cx.notify();
        }
    }

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

    /// What the status bar says in place of the tool hint while a click on
    /// the page would only dismiss something: text typed in a field, or,
    /// with Select, highlighted page text or selected markups. The next
    /// click acts.
    pub fn click_out_status(
        &self,
        document_id: DocumentId,
        cx: &gpui::App,
    ) -> Option<&'static str> {
        if let Some(status) = self.typing_click_out_status() {
            return Some(status);
        }
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

    /// The status bar's line while text is typed in a field outside the page
    /// or in a form field. It comes before the tool's own line, Pan
    /// included, because a press on the page only finishes the typing.
    pub(super) fn typing_click_out_status(&self) -> Option<&'static str> {
        matches!(
            self.click_out.typing,
            Some(TypingTarget::ChromeText | TypingTarget::FormField)
        )
        .then_some(super::CLICK_OUT_TYPING_STATUS)
    }

    /// Whether page text is highlighted in the document: a selection with
    /// text in it, not only a caret.
    pub fn text_highlighted(&self, document_id: DocumentId, cx: &gpui::App) -> bool {
        self.selected_page_text(document_id, cx).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON_PAGE: PressFacts = PressFacts {
        first_mouse: false,
        modal_canvas_state: false,
        in_reach: true,
        typing: None,
        inside_typing_target: false,
        on_form_field: false,
        on_edited_markup: false,
        canvas_dismissal: None,
    };

    /// One row for each line of the design's case table that the facts of a
    /// press can express. Rows 10, 11, 20 and 21 never reach the gate
    /// (middle button, wheel, overlays); GPUI tests cover them.
    #[test]
    fn one_gate_decides_every_left_press_on_the_page() {
        use CanvasPressGate::{Act, ClickOut as Out, CommitThenAct, Pass};
        use TypingTarget::{CanvasText, ChromeText, FormField};
        let typing = |target| PressFacts {
            typing: Some(target),
            ..ON_PAGE
        };
        let rows: &[(&str, PressFacts, CanvasPressGate)] = &[
            ("1 nothing selected", ON_PAGE, Act),
            // Empty page with markups selected: the annotation model
            // deselects alone, so no dismissal is reported here.
            ("2 markups selected, empty page", ON_PAGE, Act),
            (
                "3 text highlighted",
                PressFacts {
                    canvas_dismissal: Some(CanvasDismissal::TextHighlight),
                    ..ON_PAGE
                },
                Out(ClickOut::Canvas(CanvasDismissal::TextHighlight)),
            ),
            (
                "4-9 page field open",
                typing(ChromeText),
                Out(ClickOut::ChromeText),
            ),
            ("12 page field, Enter pressed", ON_PAGE, Act),
            (
                "13 property field, markup selected (Q3: no deselect)",
                PressFacts {
                    canvas_dismissal: Some(CanvasDismissal::MarkupSelection),
                    ..typing(ChromeText)
                },
                Out(ClickOut::ChromeText),
            ),
            (
                "14 tool default field",
                typing(ChromeText),
                Out(ClickOut::ChromeText),
            ),
            (
                "15 form field, empty page",
                typing(FormField),
                Out(ClickOut::FormField),
            ),
            (
                "16 form field, another field",
                PressFacts {
                    on_form_field: true,
                    ..typing(FormField)
                },
                Act,
            ),
            (
                "17 Text Box editor, outside",
                typing(CanvasText),
                Out(ClickOut::CanvasText),
            ),
            (
                "17a edited markup's handle or body, outside the editor",
                PressFacts {
                    on_edited_markup: true,
                    ..typing(CanvasText)
                },
                CommitThenAct,
            ),
            (
                "17b the edited markup's own press inside the editor",
                PressFacts {
                    on_edited_markup: true,
                    inside_typing_target: true,
                    ..typing(CanvasText)
                },
                Pass,
            ),
            (
                "17c window activation still only activates",
                PressFacts {
                    on_edited_markup: true,
                    first_mouse: true,
                    ..typing(CanvasText)
                },
                Out(ClickOut::WindowActivation),
            ),
            (
                "17d a property field is typed in: the press only finishes it",
                PressFacts {
                    on_edited_markup: true,
                    ..typing(ChromeText)
                },
                Out(ClickOut::ChromeText),
            ),
            (
                "18 Text Box editor, inside",
                PressFacts {
                    inside_typing_target: true,
                    ..typing(CanvasText)
                },
                Pass,
            ),
            // Edit PDF text is not a registered text field: the content
            // press ends the edit and does no more.
            ("19 Edit PDF text editing", ON_PAGE, Act),
            ("22 tool picked from the rail", ON_PAGE, Act),
            ("23 tab focused", ON_PAGE, Act),
            ("24 thumbnails focused", ON_PAGE, Act),
            (
                "25 path draft, page field open",
                typing(ChromeText),
                Out(ClickOut::ChromeText),
            ),
            (
                "26 inactive window (macOS)",
                PressFacts {
                    first_mouse: true,
                    ..ON_PAGE
                },
                Out(ClickOut::WindowActivation),
            ),
            // Windows and Linux never report the activating press.
            ("27 inactive window (Windows, Linux)", ON_PAGE, Act),
            (
                "28 hover, page field open",
                typing(ChromeText),
                Out(ClickOut::ChromeText),
            ),
            (
                "modal canvas state",
                PressFacts {
                    modal_canvas_state: true,
                    ..typing(ChromeText)
                },
                Pass,
            ),
            (
                "outside the page",
                PressFacts {
                    in_reach: false,
                    ..typing(ChromeText)
                },
                Pass,
            ),
            (
                "activation before typing",
                PressFacts {
                    first_mouse: true,
                    ..typing(ChromeText)
                },
                Out(ClickOut::WindowActivation),
            ),
            (
                "typing before a selection",
                PressFacts {
                    canvas_dismissal: Some(CanvasDismissal::TextHighlight),
                    ..typing(FormField)
                },
                Out(ClickOut::FormField),
            ),
            (
                "form field press acts, then the selection rule",
                PressFacts {
                    on_form_field: true,
                    canvas_dismissal: Some(CanvasDismissal::MarkupSelection),
                    ..typing(FormField)
                },
                Out(ClickOut::Canvas(CanvasDismissal::MarkupSelection)),
            ),
        ];
        for (case, facts, expected) in rows {
            assert_eq!(decide_press(facts), *expected, "case {case}");
        }
    }
}
