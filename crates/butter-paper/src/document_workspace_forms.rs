//! Filling a PDF's form fields with the Select tool: field outlines, hover
//! and focus, the inline text editor, check boxes and radio buttons, the
//! choice menu, and moving between fields with Tab.
//!
//! Markups sit above form fields: a press on a markup or its handles
//! selects it; elsewhere a press on a field fills it. Each committed value
//! is one undoable change; the PDF worker renders the field's new
//! appearance, and saving writes the value.

use super::*;
use crate::pdf_forms::{FormField, FormFieldEdit, FormFieldKind, FormLayer, FormValue, XfaForm};
use gpui::{AnyElement, KeyBinding, actions};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::menu::PopupMenu;

/// The key context of the focused field: its text editor, or the focus that
/// takes keys for a check box, radio button or choice field.
pub const FORM_FIELD_CONTEXT: &str = "FormField";

actions!(
    form_field,
    [
        /// Commits the field and focuses the next in tab order.
        NextFormField,
        /// Commits the field and focuses the previous in tab order.
        PreviousFormField,
        /// Commits the field and stops filling.
        FinishFormField,
        /// Toggles a check box or radio button, or opens a choice field.
        ActivateFormField,
    ]
);

/// Tab and Escape take precedence over the text editor's own bindings
/// (Tab indents a multi-line editor) and over moving keyboard focus.
pub fn init_form_field_actions(cx: &mut App) {
    let editor = Some("FormField > Input");
    let focus = Some(FORM_FIELD_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("tab", NextFormField, editor),
        KeyBinding::new("shift-tab", PreviousFormField, editor),
        KeyBinding::new("escape", FinishFormField, editor),
        KeyBinding::new("tab", NextFormField, focus),
        KeyBinding::new("shift-tab", PreviousFormField, focus),
        KeyBinding::new("escape", FinishFormField, focus),
        KeyBinding::new("space", ActivateFormField, focus),
        KeyBinding::new("enter", ActivateFormField, focus),
    ]);
}

/// The hover and press reach of a field beyond its rectangle, in pixels.
const FORM_FIELD_HIT_TOLERANCE_PX: f64 = 2.;

pub const DOCUMENT_FORM_FIELD_EDITOR_ID: &str = "document-form-field-editor";
pub const DOCUMENT_FORM_FIELD_FOCUS_ID: &str = "document-form-field-focus";
pub const DOCUMENT_FORM_CHOICE_MENU_ID: &str = "document-form-choice-menu";
pub const DOCUMENT_SIGNED_FORM_FILL_CANCEL_ID: &str = "document-signed-form-fill-cancel";
pub const DOCUMENT_SIGNED_FORM_FILL_CONFIRM_ID: &str = "document-signed-form-fill-confirm";

/// One widget of a field in a document's form, as read when the document's
/// worker opened it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct FormWidgetRef {
    pub document_id: DocumentId,
    pub epoch: u64,
    pub field: usize,
    pub widget: usize,
}

#[derive(Clone)]
pub(super) enum FormFieldInput {
    Line(Entity<InputState>),
    Lines(Entity<TextareaState>),
}

impl FormFieldInput {
    fn value(&self, cx: &App) -> String {
        match self {
            Self::Line(input) => input.read(cx).value().to_string(),
            Self::Lines(input) => input.read(cx).value().to_string(),
        }
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self {
            Self::Line(input) => input.read(cx).focus_handle(cx),
            Self::Lines(input) => input.read(cx).focus_handle(cx),
        }
    }

    fn entity_id(&self) -> gpui::EntityId {
        match self {
            Self::Line(input) => input.entity_id(),
            Self::Lines(input) => input.entity_id(),
        }
    }
}

/// The text editor open over a text field (or an editable combo box).
pub(super) struct FormFieldEditor {
    pub target: FormWidgetRef,
    pub input: FormFieldInput,
    _subscription: Subscription,
}

/// The menu of a choice field's options, open below it.
pub(super) struct FormChoiceMenu {
    pub target: FormWidgetRef,
    pub menu: Entity<PopupMenu>,
    _subscription: Subscription,
}

/// What the viewport draws over one page for form filling, in page user
/// space (`[left, bottom, right, top]`).
#[derive(Clone)]
pub(super) struct FormOverlay {
    pub page_index: u32,
    /// Every fillable field on the page, outlined.
    pub outlines: Vec<[f64; 4]>,
    pub hover: Option<[f64; 4]>,
    pub focus: Option<[f64; 4]>,
    pub editor: Option<FormEditorPresentation>,
    pub menu: Option<FormMenuPresentation>,
}

/// An open choice menu and the field it belongs to.
#[derive(Clone)]
pub struct FormMenuPresentation {
    pub menu: Entity<PopupMenu>,
    pub rect: [f64; 4],
    /// The field's displayed height, which the menu keeps clear of.
    pub field_height: f32,
}

#[derive(Clone)]
pub(super) struct FormEditorPresentation {
    pub rect: [f64; 4],
    /// Window pixels per page point, as the page last painted.
    pub display_scale: f32,
    pub input: FormFieldInput,
    /// Zero fits the text to the field.
    pub font_size_pt: f64,
    pub quadding: i64,
}

fn rect_contains(rect: [f64; 4], point: PdfPoint, tolerance: f64) -> bool {
    point.x >= rect[0] - tolerance
        && point.x <= rect[2] + tolerance
        && point.y >= rect[1] - tolerance
        && point.y <= rect[3] + tolerance
}

fn rect_area(rect: [f64; 4]) -> f64 {
    (rect[2] - rect[0]) * (rect[3] - rect[1])
}

/// The editor text size for a field: its own, or one that fits a line in
/// it.
fn editor_font_size(font_size: f64, rect: [f64; 4], multiline: bool) -> f64 {
    if font_size > 0. {
        font_size
    } else if multiline {
        12.
    } else {
        ((rect[3] - rect[1]) * 0.65).clamp(4., 12.)
    }
}

impl DocumentWorkspace {
    /// The document's form fields and the resource epoch they belong to.
    pub(super) fn form_layer(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<(Arc<FormLayer>, u64)> {
        let session = self.session(document_id, cx)?.read(cx);
        let layer = session.resource.as_ref()?.form_layer();
        (!layer.fields.is_empty() || layer.xfa != XfaForm::None)
            .then_some((layer, session.resource_epoch))
    }

    fn form_field_at(
        &self,
        target: FormWidgetRef,
        cx: &App,
    ) -> Option<(Arc<FormLayer>, FormField)> {
        let (layer, epoch) = self.form_layer(target.document_id, cx)?;
        if epoch != target.epoch {
            return None;
        }
        let field = layer.fields.get(target.field)?.clone();
        field.widgets.get(target.widget)?;
        Some((layer, field))
    }

    fn form_field_edits(&self, document_id: DocumentId, cx: &App) -> Vec<FormFieldEdit> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_form_field_edits(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    /// A field's value as the document shows it now.
    fn form_field_value(&self, document_id: DocumentId, field: &FormField, cx: &App) -> FormValue {
        crate::pdf_forms::current_value(field, &self.form_field_edits(document_id, cx))
    }

    /// The widget under a page point: the smallest when fields overlap.
    /// Records a painted page's transform for the form editor and menu.
    /// True when the page of an open editor or menu changed scale, which
    /// needs another render to resize them.
    pub(super) fn record_form_page_transform(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        transform: PageTransform,
        cx: &App,
    ) -> bool {
        let previous = self
            .form_page_transforms
            .insert((document_id, page_index), transform);
        if previous
            .is_some_and(|previous| previous.pixels_per_point() == transform.pixels_per_point())
        {
            return false;
        }
        let on_page = |target: FormWidgetRef| {
            target.document_id == document_id
                && self
                    .form_field_at(target, cx)
                    .and_then(|(_, field)| field.widgets.get(target.widget).cloned())
                    .is_some_and(|widget| widget.page_index == page_index)
        };
        self.form_field_editor
            .as_ref()
            .is_some_and(|editor| on_page(editor.target))
            || self
                .form_choice_menu
                .as_ref()
                .is_some_and(|menu| on_page(menu.target))
    }

    fn form_widget_hit(
        &self,
        interaction: &PageInteraction,
        point: PdfPoint,
        cx: &App,
    ) -> Option<FormWidgetRef> {
        let (layer, epoch) = self.form_layer(interaction.document_id, cx)?;
        let tolerance = interaction
            .transform
            .tolerance_points(FORM_FIELD_HIT_TOLERANCE_PX)
            .unwrap_or(1.);
        let mut best: Option<(f64, FormWidgetRef)> = None;
        for (field_index, field) in layer.fields.iter().enumerate() {
            for (widget_index, widget) in field.widgets.iter().enumerate() {
                if widget.page_index != interaction.page_index
                    || !rect_contains(widget.rect, point, tolerance)
                {
                    continue;
                }
                let area = rect_area(widget.rect);
                if best.as_ref().is_none_or(|(best, _)| area < *best) {
                    best = Some((
                        area,
                        FormWidgetRef {
                            document_id: interaction.document_id,
                            epoch,
                            field: field_index,
                            widget: widget_index,
                        },
                    ));
                }
            }
        }
        best.map(|(_, hit)| hit)
    }

    /// Hover feedback for form fields under the Select tool, when no markup
    /// is under the pointer. Hover and press share one hit test.
    pub(super) fn update_form_field_hover(
        &mut self,
        interaction: &PageInteraction,
        point: Option<PdfPoint>,
        cx: &mut Context<Self>,
    ) -> bool {
        let hover = point.and_then(|point| self.form_widget_hit(interaction, point, cx));
        if self.form_field_hover != hover {
            self.form_field_hover = hover;
            cx.notify();
            true
        } else {
            false
        }
    }

    /// The cursor over a hovered field: what a click does there.
    pub(super) fn form_field_cursor(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<CursorStyle> {
        let hover = self
            .form_field_hover
            .filter(|hover| hover.document_id == document_id)?;
        let (_, field) = self.form_field_at(hover, cx)?;
        if !field.fillable() {
            return None;
        }
        Some(match field.kind {
            FormFieldKind::Text { .. } => CursorStyle::IBeam,
            _ => CursorStyle::PointingHand,
        })
    }

    /// Whether a markup or one of its handles is under a point: those take
    /// a press before the form fields beneath them.
    fn markup_under_point(&self, interaction: &PageInteraction, point: PdfPoint, cx: &App) -> bool {
        let Some(session) = self.session(interaction.document_id, cx) else {
            return false;
        };
        let Ok(tolerance) = interaction.transform.tolerance_points(4.) else {
            return false;
        };
        let annotations = &session.read(cx).annotations;
        let document = interaction.document_id.value();
        annotations
            .select_hover_hit(document, interaction.page_index, point, tolerance)
            .ok()
            .flatten()
            .is_some()
            || annotations
                .hover_transform_handle(document, interaction.page_index, point, tolerance)
                .ok()
                .flatten()
                .is_some()
            || annotations
                .hover_markup_id(document, interaction.page_index, point, tolerance)
                .ok()
                .flatten()
                .is_some()
    }

    /// A Select-tool press on a form field: focus it and fill it. Returns
    /// false, leaving the press to selection, when no field is there or a
    /// markup is above it.
    pub(super) fn begin_form_field_press(
        &mut self,
        interaction: PageInteraction,
        point: PdfPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let hit = (!self.markup_under_point(&interaction, point, cx))
            .then(|| self.form_widget_hit(&interaction, point, cx))
            .flatten();
        let Some(target) = hit else {
            // A press elsewhere finishes filling.
            self.finish_form_filling(cx);
            if self
                .form_layer(interaction.document_id, cx)
                .is_some_and(|(layer, _)| layer.xfa == XfaForm::Dynamic)
            {
                self.annotation_statuses.insert(
                    interaction.document_id,
                    "This PDF's form uses XFA, which Butter Paper cannot fill.".into(),
                );
            }
            return false;
        };
        let Some((_, field)) = self.form_field_at(target, cx) else {
            return false;
        };
        // Pressing a field clears a markup selection, as pressing the page
        // does.
        if let Some(session) = self.session(interaction.document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                session
                    .annotations
                    .clear_selection(interaction.document_id.value());
                cx.notify();
            });
        }
        if !field.fillable() {
            self.finish_form_filling(cx);
            let message = match field.kind {
                FormFieldKind::Signature => "Signing signature fields is not supported yet.",
                FormFieldKind::PushButton => "Form buttons and their actions are not supported.",
                _ => "This field is read-only.",
            };
            self.annotation_statuses
                .insert(interaction.document_id, message.into());
            cx.notify();
            return true;
        }
        let signed = self
            .session(interaction.document_id, cx)
            .and_then(|session| session.read(cx).resource.clone())
            .is_some_and(|resource| resource.digitally_signed());
        if signed
            && !self
                .page_text_signed_confirmed
                .contains(&interaction.document_id)
        {
            self.finish_form_filling(cx);
            Self::confirm_filling_signed_form(
                cx.entity().downgrade(),
                interaction.document_id,
                window,
                cx,
            );
            return true;
        }
        self.annotation_statuses.remove(&interaction.document_id);
        match field.kind {
            FormFieldKind::CheckBox | FormFieldKind::Radio { .. } => {
                self.focus_form_widget(target, false, window, cx);
                self.toggle_form_widget(target, cx);
            }
            _ => self.focus_form_widget(target, true, window, cx),
        }
        true
    }

    /// Asks before the first fill of a signed document: saving changed
    /// fields invalidates its signatures.
    fn confirm_filling_signed_form(
        owner: WeakEntity<Self>,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.open_alert_dialog(cx, move |alert, _, _| {
            let owner = owner.clone();
            alert
                .close_button(false)
                .title("Fill in a signed PDF?")
                .description(
                    "This PDF is digitally signed. Saving changes to its fields invalidates its signatures.",
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new(DOCUMENT_SIGNED_FORM_FILL_CANCEL_ID)
                                .debug_selector(|| DOCUMENT_SIGNED_FORM_FILL_CANCEL_ID.into())
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(dialog_action_button(
                            Button::new(DOCUMENT_SIGNED_FORM_FILL_CONFIRM_ID)
                                .debug_selector(|| DOCUMENT_SIGNED_FORM_FILL_CONFIRM_ID.into())
                                .danger()
                                .label("Fill In"),
                        )),
                )
                .on_ok(move |_, _, cx| {
                    owner
                        .update(cx, |workspace, cx| {
                            workspace.page_text_signed_confirmed.insert(document_id);
                            workspace.annotation_statuses.insert(
                                document_id,
                                "Click a field to fill it. Saving invalidates the signatures."
                                    .into(),
                            );
                            cx.notify();
                        })
                        .is_ok()
                })
        });
    }

    /// Focuses a widget: a text field opens its editor, a choice field its
    /// menu when `open` is set; a check box or radio button takes keys.
    fn focus_form_widget(
        &mut self,
        target: FormWidgetRef,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.form_field_editor.as_ref().map(|editor| editor.target) == Some(target) {
            return;
        }
        self.commit_form_field_editor(cx);
        self.form_choice_menu = None;
        let Some((_, field)) = self.form_field_at(target, cx) else {
            return;
        };
        self.form_field_focus = Some(target);
        self.reveal_form_widget(target, &field, cx);
        let value = self.form_field_value(target.document_id, &field, cx);
        match &field.kind {
            FormFieldKind::Text {
                multiline,
                password,
                max_len,
                ..
            } => {
                let text = field.display_text(&value);
                self.open_form_field_editor(
                    target, text, *multiline, *password, *max_len, window, cx,
                );
            }
            FormFieldKind::Combo { .. } | FormFieldKind::List { .. } if open => {
                self.open_form_choice_menu(target, window, cx);
            }
            _ => {
                self.form_focus.focus(window, cx);
            }
        }
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    fn open_form_field_editor(
        &mut self,
        target: FormWidgetRef,
        text: String,
        multiline: bool,
        password: bool,
        max_len: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = if multiline {
            FormFieldInput::Lines(cx.new(|cx| {
                TextareaState::new(window, cx)
                    .soft_wrap(true)
                    .default_value(text)
            }))
        } else {
            FormFieldInput::Line(cx.new(|cx| {
                let state = InputState::new(window, cx)
                    .masked(password)
                    .default_value(text);
                match max_len {
                    Some(max_len) => {
                        state.validate(move |value, _| value.chars().count() <= max_len)
                    }
                    None => state,
                }
            }))
        };
        let editor_id = input.entity_id();
        let handle_event = move |workspace: &mut Self,
                                 event: &InputEvent,
                                 window: &mut Window,
                                 cx: &mut Context<Self>| {
            let current = workspace
                .form_field_editor
                .as_ref()
                .is_some_and(|editor| editor.input.entity_id() == editor_id);
            if !current {
                return;
            }
            match event {
                InputEvent::Blur => {
                    workspace.commit_form_field_editor(cx);
                    cx.notify();
                }
                InputEvent::PressEnter {
                    secondary: false, ..
                } if !multiline => {
                    workspace.finish_form_filling(cx);
                    workspace.workspace_focus.focus(window, cx);
                    cx.notify();
                }
                _ => {}
            }
        };
        let subscription = match &input {
            FormFieldInput::Line(state) => cx.subscribe_in(
                state,
                window,
                move |workspace, _, event: &InputEvent, window, cx| {
                    handle_event(workspace, event, window, cx)
                },
            ),
            FormFieldInput::Lines(state) => cx.subscribe_in(
                state,
                window,
                move |workspace, _, event: &InputEvent, window, cx| {
                    handle_event(workspace, event, window, cx)
                },
            ),
        };
        let focus = input.focus_handle(cx);
        self.form_field_editor = Some(FormFieldEditor {
            target,
            input,
            _subscription: subscription,
        });
        focus.focus(window, cx);
    }

    /// Records the edited text as the field's value: one undoable change.
    pub(super) fn commit_form_field_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.form_field_editor.take() else {
            return;
        };
        let text = editor.input.value(cx).replace("\r\n", "\n");
        let Some((_, field)) = self.form_field_at(editor.target, cx) else {
            return;
        };
        let value = match &field.kind {
            FormFieldKind::Combo { .. } => {
                FormValue::Choices((!text.is_empty()).then_some(text).into_iter().collect())
            }
            _ => FormValue::Text(text),
        };
        self.set_form_field_value(editor.target.document_id, &field, value, cx);
    }

    fn set_form_field_value(
        &mut self,
        document_id: DocumentId,
        field: &FormField,
        value: FormValue,
        cx: &mut Context<Self>,
    ) {
        if let FormValue::Text(text) = &value
            && !crate::pdf_forms::text_is_drawable(text)
        {
            self.annotation_statuses.insert(
                document_id,
                "No available font can draw a character of this text".into(),
            );
            cx.notify();
            return;
        }
        let edit = FormFieldEdit {
            name: field.name.clone(),
            original: field.value.clone(),
            value,
        };
        let result = self.update_annotation_history_with_result(
            document_id,
            cx,
            move |annotations, document_id| {
                annotations.set_document_form_field_edit(document_id, edit)
            },
        );
        if let Err(error) = result {
            self.annotation_statuses.insert(
                document_id,
                format!("The field value was not kept: {error}"),
            );
        }
        self.sync_document_edits(document_id, cx);
        cx.notify();
    }

    /// A click on a check box or radio button, or Space on a focused one.
    fn toggle_form_widget(&mut self, target: FormWidgetRef, cx: &mut Context<Self>) {
        let Some((_, field)) = self.form_field_at(target, cx) else {
            return;
        };
        let value = self.form_field_value(target.document_id, &field, cx);
        if let Some(next) = field.toggled(&value, target.widget)
            && next != value
        {
            self.set_form_field_value(target.document_id, &field, next, cx);
        }
    }

    fn open_form_choice_menu(
        &mut self,
        target: FormWidgetRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, field)) = self.form_field_at(target, cx) else {
            return;
        };
        let (options, multi_select, editable) = match &field.kind {
            FormFieldKind::Combo { options, editable } => (options.clone(), false, *editable),
            FormFieldKind::List {
                options,
                multi_select,
            } => (options.clone(), *multi_select, false),
            _ => return,
        };
        let FormValue::Choices(chosen) = self.form_field_value(target.document_id, &field, cx)
        else {
            return;
        };
        let owner = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            let mut menu = menu.scrollable(true).max_h(px(320.)).min_w(px(160.));
            for option in &options {
                let owner = owner.clone();
                let export = option.export.clone();
                let label = if option.display.is_empty() {
                    "(blank)".to_owned()
                } else {
                    option.display.clone()
                };
                menu = menu.item(
                    PopupMenuItem::new(label)
                        .checked(chosen.contains(&option.export))
                        .on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |workspace, cx| {
                                workspace.choose_form_option(
                                    target,
                                    export.clone(),
                                    multi_select,
                                    window,
                                    cx,
                                );
                            });
                        }),
                );
            }
            if editable {
                let owner = owner.clone();
                menu = menu
                    .separator()
                    .item(
                        PopupMenuItem::new("Type a Value…").on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |workspace, cx| {
                                workspace.type_form_choice(target, window, cx);
                            });
                        }),
                    );
            }
            menu
        });
        let subscription = cx.subscribe_in(
            &menu,
            window,
            move |workspace, menu, _: &gpui::DismissEvent, window, cx| {
                if workspace
                    .form_choice_menu
                    .as_ref()
                    .is_some_and(|open| open.menu == *menu)
                {
                    workspace.form_choice_menu = None;
                    if workspace.form_field_editor.is_none() {
                        workspace.form_focus.focus(window, cx);
                    }
                    cx.notify();
                }
            },
        );
        menu.focus_handle(cx).focus(window, cx);
        self.form_choice_menu = Some(FormChoiceMenu {
            target,
            menu,
            _subscription: subscription,
        });
        cx.notify();
    }

    fn choose_form_option(
        &mut self,
        target: FormWidgetRef,
        export: String,
        multi_select: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, field)) = self.form_field_at(target, cx) else {
            return;
        };
        let FormValue::Choices(mut chosen) = self.form_field_value(target.document_id, &field, cx)
        else {
            return;
        };
        if multi_select {
            if let Some(index) = chosen.iter().position(|value| *value == export) {
                chosen.remove(index);
            } else {
                chosen.push(export);
            }
        } else {
            chosen = vec![export];
        }
        self.set_form_field_value(target.document_id, &field, FormValue::Choices(chosen), cx);
        self.form_choice_menu = None;
        self.form_focus.focus(window, cx);
        cx.notify();
    }

    /// An editable combo box takes typed text.
    fn type_form_choice(
        &mut self,
        target: FormWidgetRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, field)) = self.form_field_at(target, cx) else {
            return;
        };
        self.form_choice_menu = None;
        let value = self.form_field_value(target.document_id, &field, cx);
        let text = field.display_text(&value);
        self.open_form_field_editor(target, text, false, false, None, window, cx);
        cx.notify();
    }

    /// Scrolls a widget into view: within its page when the page is shown,
    /// else to its page.
    fn reveal_form_widget(
        &mut self,
        target: FormWidgetRef,
        field: &FormField,
        cx: &mut Context<Self>,
    ) {
        let Some(widget) = field.widgets.get(target.widget) else {
            return;
        };
        let interaction = self
            .page_interactions
            .get(&(target.document_id, widget.page_index))
            .copied();
        let viewport = self.viewport_bounds.get(&target.document_id).copied();
        let (Some(interaction), Some(viewport)) = (interaction, viewport) else {
            self.activate_thumbnail_page(target.document_id, widget.page_index, cx);
            return;
        };
        let Ok(rect) = crate::annotation_model::PdfRect::new(
            widget.rect[0],
            widget.rect[1],
            widget.rect[2] - widget.rect[0],
            widget.rect[3] - widget.rect[1],
        ) else {
            return;
        };
        let local = interaction.transform.rect_to_local_pixels(rect);
        let left = f32::from(interaction.bounds.origin.x) + local.x as f32;
        let top = f32::from(interaction.bounds.origin.y) + local.y as f32;
        let right = left + local.width as f32;
        let bottom = top + local.height as f32;
        let margin = 24.;
        let dx = if left < f32::from(viewport.left()) + margin {
            left - f32::from(viewport.left()) - margin
        } else if right > f32::from(viewport.right()) - margin {
            (right - f32::from(viewport.right()) + margin).min(left - f32::from(viewport.left()))
        } else {
            0.
        };
        let dy = if top < f32::from(viewport.top()) + margin {
            top - f32::from(viewport.top()) - margin
        } else if bottom > f32::from(viewport.bottom()) - margin {
            (bottom - f32::from(viewport.bottom()) + margin).min(top - f32::from(viewport.top()))
        } else {
            0.
        };
        if dx != 0. || dy != 0. {
            let Some((x, y)) = self
                .session(target.document_id, cx)
                .map(|session| session.read(cx).view_state.scroll())
            else {
                return;
            };
            self.set_viewport_scroll(target.document_id, x + dx, y + dy, cx);
        }
    }

    /// Moves focus to the next (or previous) fillable field in tab order,
    /// committing the focused one.
    pub(super) fn move_form_focus(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.form_field_focus else {
            return;
        };
        let Some((layer, epoch)) = self.form_layer(current.document_id, cx) else {
            return;
        };
        let order = layer
            .tab_order
            .iter()
            .copied()
            .filter(|(field, _)| layer.fields[*field].fillable())
            .collect::<Vec<_>>();
        if order.is_empty() {
            return;
        }
        let position = order
            .iter()
            .position(|entry| *entry == (current.field, current.widget));
        let next = match (position, forward) {
            (Some(index), true) => (index + 1) % order.len(),
            (Some(index), false) => (index + order.len() - 1) % order.len(),
            (None, true) => 0,
            (None, false) => order.len() - 1,
        };
        let (field, widget) = order[next];
        self.focus_form_widget(
            FormWidgetRef {
                document_id: current.document_id,
                epoch,
                field,
                widget,
            },
            false,
            window,
            cx,
        );
    }

    /// Commits the focused field and clears focus.
    pub(super) fn finish_form_filling(&mut self, cx: &mut Context<Self>) {
        self.commit_form_field_editor(cx);
        if self.form_field_focus.take().is_some() || self.form_choice_menu.take().is_some() {
            cx.notify();
        }
    }

    /// Stops filling when the document, tool or worker it belongs to goes
    /// away.
    pub(super) fn reconcile_form_filling(&mut self, cx: &mut Context<Self>) {
        let Some(focus) = self
            .form_field_focus
            .or(self.form_field_editor.as_ref().map(|editor| editor.target))
        else {
            return;
        };
        let still_filling = self.active_document_id == Some(focus.document_id)
            && self.annotation_tool(focus.document_id, cx) == Some(AnnotationTool::Select)
            && !self.pan_tool_active
            && self.form_field_at(focus, cx).is_some();
        if !still_filling {
            if self.form_field_at(focus, cx).is_none() {
                // The document was reopened under the editor.
                self.form_field_editor = None;
            }
            self.finish_form_filling(cx);
        }
    }

    /// Space or Enter on a focused check box, radio button or choice field.
    fn activate_focused_form_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.form_field_focus else {
            return;
        };
        let Some((_, field)) = self.form_field_at(target, cx) else {
            return;
        };
        match field.kind {
            FormFieldKind::CheckBox | FormFieldKind::Radio { .. } => {
                self.toggle_form_widget(target, cx)
            }
            FormFieldKind::Combo { .. } | FormFieldKind::List { .. } => {
                self.open_form_choice_menu(target, window, cx)
            }
            _ => {}
        }
    }

    fn finish_form_field_from_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.finish_form_filling(cx);
        self.workspace_focus.focus(window, cx);
    }

    /// What the viewport draws for form filling on each visible page.
    pub(super) fn form_overlays(&self, document_id: DocumentId, cx: &App) -> Vec<FormOverlay> {
        let Some((layer, epoch)) = self.form_layer(document_id, cx) else {
            return Vec::new();
        };
        let select_active = self.annotation_tool(document_id, cx) == Some(AnnotationTool::Select)
            && !self.pan_tool_active;
        let visible = self
            .session(document_id, cx)
            .and_then(|session| {
                session
                    .read(cx)
                    .viewer
                    .plan_snapshot()
                    .map(|plan| plan.visible_pages.to_vec())
            })
            .unwrap_or_default();
        let widget_rect = |target: Option<FormWidgetRef>| {
            target
                .filter(|target| target.document_id == document_id && target.epoch == epoch)
                .and_then(|target| {
                    let field = layer.fields.get(target.field)?;
                    let widget = field.widgets.get(target.widget)?;
                    Some((target, field, widget))
                })
        };
        let hover = widget_rect(self.form_field_hover)
            .filter(|(_, field, _)| field.fillable() && select_active);
        let focus = widget_rect(self.form_field_focus);
        let editor = self.form_field_editor.as_ref().and_then(|editor| {
            let (_, field, widget) = widget_rect(Some(editor.target))?;
            let multiline = matches!(
                field.kind,
                FormFieldKind::Text {
                    multiline: true,
                    ..
                }
            );
            Some((
                widget.page_index,
                FormEditorPresentation {
                    rect: widget.rect,
                    display_scale: self
                        .form_page_transforms
                        .get(&(document_id, widget.page_index))
                        .map_or(1., |transform| transform.pixels_per_point() as f32),
                    input: editor.input.clone(),
                    font_size_pt: editor_font_size(widget.font_size, widget.rect, multiline),
                    quadding: widget.quadding,
                },
            ))
        });
        let menu = self.form_choice_menu.as_ref().and_then(|menu| {
            let (_, _, widget) = widget_rect(Some(menu.target))?;
            let field_height = self
                .form_page_transforms
                .get(&(document_id, widget.page_index))
                .and_then(|transform| {
                    let [left, bottom, right, top] = widget.rect;
                    let rect = crate::annotation_model::PdfRect::new(
                        left,
                        bottom,
                        right - left,
                        top - bottom,
                    )
                    .ok()?;
                    Some(transform.rect_to_local_pixels(rect).height as f32)
                })
                .unwrap_or(0.);
            Some((
                widget.page_index,
                FormMenuPresentation {
                    menu: menu.menu.clone(),
                    rect: widget.rect,
                    field_height,
                },
            ))
        });
        // The visible pages, and the pages of the hovered, focused and
        // edited fields even before the view reports them.
        let pages = visible
            .into_iter()
            .filter_map(|page| u32::try_from(page).ok())
            .chain(hover.map(|(_, _, widget)| widget.page_index))
            .chain(focus.map(|(_, _, widget)| widget.page_index))
            .chain(editor.as_ref().map(|(page, _)| *page))
            .chain(menu.as_ref().map(|(page, _)| *page))
            .collect::<std::collections::BTreeSet<_>>();
        let mut overlays = Vec::new();
        for page in pages {
            let outlines = if select_active {
                layer
                    .fields
                    .iter()
                    .filter(|field| field.fillable())
                    .flat_map(|field| &field.widgets)
                    .filter(|widget| widget.page_index == page)
                    .map(|widget| widget.rect)
                    .collect()
            } else {
                Vec::new()
            };
            let overlay = FormOverlay {
                page_index: page,
                outlines,
                hover: hover
                    .filter(|(_, _, widget)| widget.page_index == page)
                    .map(|(_, _, widget)| widget.rect),
                focus: focus
                    .filter(|(_, _, widget)| widget.page_index == page)
                    .map(|(_, _, widget)| widget.rect),
                editor: editor
                    .clone()
                    .filter(|(editor_page, _)| *editor_page == page)
                    .map(|(_, editor)| editor),
                menu: menu
                    .clone()
                    .filter(|(menu_page, _)| *menu_page == page)
                    .map(|(_, menu)| menu),
            };
            if !overlay.outlines.is_empty()
                || overlay.hover.is_some()
                || overlay.focus.is_some()
                || overlay.editor.is_some()
                || overlay.menu.is_some()
            {
                overlays.push(overlay);
            }
        }
        overlays
    }

    /// For review logs and tests: the focused field and its value.
    pub fn form_field_focus_summary(&self, cx: &App) -> Option<(String, String)> {
        let focus = self.form_field_focus?;
        let (_, field) = self.form_field_at(focus, cx)?;
        let value = match &self.form_field_editor {
            Some(editor) if editor.target == focus => editor.input.value(cx),
            _ => field.display_text(&self.form_field_value(focus.document_id, &field, cx)),
        };
        Some((field.name, value))
    }

    /// The document's filled field values, by name.
    pub fn form_field_values(&self, document_id: DocumentId, cx: &App) -> Vec<(String, FormValue)> {
        let Some((layer, _)) = self.form_layer(document_id, cx) else {
            return Vec::new();
        };
        let edits = self.form_field_edits(document_id, cx);
        layer
            .fields
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    crate::pdf_forms::current_value(field, &edits),
                )
            })
            .collect()
    }
}

/// Receives keys for a focused check box, radio button or choice field.
/// It sits outside the pages so it stays mounted while they re-render.
pub(super) fn form_focus_element(
    focus: FocusHandle,
    owner: WeakEntity<DocumentWorkspace>,
) -> AnyElement {
    form_field_actions(
        gpui::div()
            .id(DOCUMENT_FORM_FIELD_FOCUS_ID)
            .debug_selector(|| DOCUMENT_FORM_FIELD_FOCUS_ID.into())
            .key_context(FORM_FIELD_CONTEXT)
            .absolute()
            .left_0()
            .top_0()
            .size(px(1.))
            .track_focus(&focus),
        owner.clone(),
    )
    .on_action(move |_: &ActivateFormField, window, cx| {
        let _ = owner.update(cx, |workspace, cx| {
            workspace.activate_focused_form_field(window, cx)
        });
    })
    .into_any_element()
}

/// Tab, Shift+Tab and Escape for the focused field.
fn form_field_actions(
    element: gpui::Stateful<gpui::Div>,
    owner: WeakEntity<DocumentWorkspace>,
) -> gpui::Stateful<gpui::Div> {
    let next = owner.clone();
    let previous = owner.clone();
    element
        .on_action(move |_: &NextFormField, window, cx| {
            let _ = next.update(cx, |workspace, cx| {
                workspace.move_form_focus(true, window, cx)
            });
        })
        .on_action(move |_: &PreviousFormField, window, cx| {
            let _ = previous.update(cx, |workspace, cx| {
                workspace.move_form_focus(false, window, cx)
            });
        })
        .on_action(move |_: &FinishFormField, window, cx| {
            let _ = owner.update(cx, |workspace, cx| {
                workspace.finish_form_field_from_key(window, cx)
            });
        })
}

/// Draws a page's form filling chrome: field outlines, the hovered and
/// focused fields, the text editor and the choice menu.
pub(super) fn form_overlay_element(
    overlay: FormOverlay,
    page_size: (f32, f32),
    coordinate_space: PageCoordinateSpace,
    accent: gpui::Hsla,
    owner: WeakEntity<DocumentWorkspace>,
) -> AnyElement {
    // Placed as fractions of the page layer, whose size is the page's
    // displayed size in any view.
    let layer_bounds = Bounds::new(
        point(px(0.), px(0.)),
        size(px(page_size.0), px(page_size.1)),
    );
    let placed = |rect: [f64; 4]| -> Option<[f32; 4]> {
        let (page_bounds, transform) =
            contained_page_bounds_for_space(layer_bounds, page_size, coordinate_space)?;
        let rect = crate::annotation_model::PdfRect::new(
            rect[0],
            rect[1],
            rect[2] - rect[0],
            rect[3] - rect[1],
        )
        .ok()?;
        let local = transform.rect_to_local_pixels(rect);
        Some([
            (f32::from(page_bounds.origin.x) + local.x as f32) / page_size.0,
            (f32::from(page_bounds.origin.y) + local.y as f32) / page_size.1,
            local.width as f32 / page_size.0,
            local.height as f32 / page_size.1,
        ])
    };
    let editor = overlay
        .editor
        .clone()
        .and_then(|editor| Some((placed(editor.rect)?, editor)));
    let menu = overlay
        .menu
        .clone()
        .and_then(|menu| Some((placed(menu.rect)?, menu)));
    let painted = overlay;
    let mut layer = gpui::div().absolute().inset_0().child(
        canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let Some((page_bounds, transform)) =
                    contained_page_bounds_for_space(bounds, page_size, coordinate_space)
                else {
                    return;
                };
                let project = |(x, y): (f64, f64)| {
                    let local = transform.point_to_local_pixels(PdfPoint { x, y });
                    point(
                        page_bounds.origin.x + px(local.x as f32),
                        page_bounds.origin.y + px(local.y as f32),
                    )
                };
                let corners = |rect: &[f64; 4]| {
                    [
                        (rect[0], rect[1]),
                        (rect[2], rect[1]),
                        (rect[2], rect[3]),
                        (rect[0], rect[3]),
                    ]
                };
                let paint = |rect: &[f64; 4],
                             fill: Option<gpui::Hsla>,
                             stroke: gpui::Hsla,
                             window: &mut Window| {
                    let quad = corners(rect);
                    if let Some(fill) = fill {
                        let mut builder = PathBuilder::fill();
                        builder.move_to(project(quad[0]));
                        for corner in &quad[1..] {
                            builder.line_to(project(*corner));
                        }
                        builder.close();
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, fill);
                        }
                    }
                    let mut builder = PathBuilder::stroke(px(1.));
                    builder.move_to(project(quad[0]));
                    for corner in &quad[1..] {
                        builder.line_to(project(*corner));
                    }
                    builder.close();
                    if let Ok(path) = builder.build() {
                        window.paint_path(path, stroke);
                    }
                };
                // As Edit PDF Content outlines paragraphs: fields are
                // outlined, and filled only when hovered or focused.
                for rect in &painted.outlines {
                    if Some(*rect) != painted.hover && Some(*rect) != painted.focus {
                        paint(rect, None, accent.opacity(0.35), window);
                    }
                }
                if let Some(rect) = &painted.hover
                    && painted.focus != Some(*rect)
                {
                    paint(
                        rect,
                        Some(accent.opacity(0.08)),
                        accent.opacity(0.6),
                        window,
                    );
                }
                if let Some(rect) = &painted.focus {
                    paint(rect, Some(accent.opacity(0.08)), accent, window);
                }
            },
        )
        .absolute()
        .inset_0(),
    );
    if let Some((place, editor)) = editor {
        let scale = editor.display_scale;
        let font_size = px((editor.font_size_pt as f32 * scale).max(6.));
        let padding = px((2. * scale).clamp(1., 6.));
        let key_owner = owner;
        let field = match &editor.input {
            FormFieldInput::Line(input) => Input::new(input)
                .aria_label("Form field")
                .appearance(false)
                .bordered(false)
                .h_full()
                .into_any_element(),
            FormFieldInput::Lines(input) => Textarea::new(input)
                .aria_label("Form field")
                .appearance(false)
                .bordered(false)
                .size_full()
                .editor_paddings(Edges {
                    top: padding,
                    right: padding,
                    bottom: padding,
                    left: padding,
                })
                .into_any_element(),
        };
        layer = layer.child(form_field_actions(
            gpui::div()
                .id(DOCUMENT_FORM_FIELD_EDITOR_ID)
                .key_context(FORM_FIELD_CONTEXT)
                .debug_selector(|| DOCUMENT_FORM_FIELD_EDITOR_ID.into())
                .absolute()
                .left(relative(place[0]))
                .top(relative(place[1]))
                .w(relative(place[2]))
                .h(relative(place[3]))
                .overflow_hidden()
                .occlude()
                .border_1()
                .border_color(accent)
                // Paper-relative: the field is edited on the page.
                .bg(gpui::white())
                .text_color(gpui::black())
                .text_size(font_size)
                .px(padding)
                .when(editor.quadding == 1, |editor| editor.text_center())
                .when(editor.quadding == 2, |editor| editor.text_right())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(field),
            key_owner,
        ));
    }
    if let Some((place, menu)) = menu {
        // Anchored at the field's middle with half its height of clearance
        // either side: the menu opens below the field, or above it when
        // the window has no room below, and never covers it.
        let clearance = px(menu.field_height / 2.);
        layer = layer.child(
            gpui::div()
                .id(DOCUMENT_FORM_CHOICE_MENU_ID)
                .debug_selector(|| DOCUMENT_FORM_CHOICE_MENU_ID.into())
                .absolute()
                .left(relative(place[0]))
                .top(relative(place[1] + place[3] / 2.))
                .child(
                    gpui::deferred(
                        gpui::anchored()
                            .child(gpui::div().flex().flex_col().py(clearance).child(menu.menu)),
                    )
                    .with_priority(1),
                ),
        );
    }
    layer.into_any_element()
}
