//! Sizes typed while drawing on a page with a scale (`size_entry`): the box
//! beside the pointer, the keys that type into it, and Enter placing the
//! markup at the typed size.

use super::*;
use crate::annotation_adapter::TypedSizeAnchor;
use crate::size_entry::{ScreenFrame, SizeEntry, SizeEntryField, scale_text};
use gpui::{AnyElement, KeyContext, div};

pub(super) const SIZE_ENTRY_ID: &str = "document-size-entry";
/// Present while a markup is being drawn on a page with a scale: Tab moves
/// between its sizes.
pub(super) const SIZE_READOUT_CONTEXT: &str = "SizeReadout";
/// Present once a size is typed: letters then type units, not tool shortcuts.
pub(super) const SIZE_ENTRY_CONTEXT: &str = "SizeEntry";

/// What the box beside the pointer shows.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct SizeEntryView {
    pub position: gpui::Point<Pixels>,
    pub fields: [SizeEntryField; 2],
    pub scale: String,
}

impl DocumentWorkspace {
    /// The markup being drawn on a page with a scale, with the page's scale
    /// and how the page lies on screen.
    fn size_entry_target(
        &self,
        cx: &App,
    ) -> Option<(DocumentId, TypedSizeAnchor, PageScale, ScreenFrame)> {
        let document_id = self.active_document_id?;
        let session = self.session(document_id, cx)?.read(cx);
        let anchor = session.annotations.typed_size_anchor(document_id.value())?;
        let scale = session
            .annotations
            .document_page_scale(document_id.value(), anchor.page_index)?
            .clone();
        let frame = self
            .page_interactions
            .get(&(document_id, anchor.page_index))
            .map_or(ScreenFrame::UPRIGHT, |interaction| {
                let origin = interaction
                    .transform
                    .point_to_local_pixels(PdfPoint { x: 0., y: 0. });
                let along = interaction
                    .transform
                    .point_to_local_pixels(PdfPoint { x: 1., y: 0. });
                ScreenFrame {
                    x_axis_angle: (-(along.y - origin.y)).atan2(along.x - origin.x),
                }
            });
        Some((document_id, anchor, scale, frame))
    }

    /// The entry for the markup being drawn, started afresh when that markup
    /// (or a path's segment) changed.
    fn current_size_entry(&mut self, cx: &App) -> Option<(SizeEntry, PageScale, ScreenFrame)> {
        let Some((document_id, anchor, scale, frame)) = self.size_entry_target(cx) else {
            self.size_entry = None;
            return None;
        };
        if !self
            .size_entry
            .as_ref()
            .is_some_and(|entry| entry.is_for(document_id.value(), &anchor))
        {
            self.size_entry = Some(SizeEntry::new(document_id.value(), anchor));
        }
        let entry = self.size_entry.as_mut()?;
        entry.anchor = anchor;
        Some((entry.clone(), scale, frame))
    }

    pub(super) fn size_entry_key_context(&self, cx: &App) -> KeyContext {
        let mut context = KeyContext::new_with_defaults();
        context.add(DOCUMENT_WORKSPACE_CONTEXT);
        if let Some((document_id, anchor, ..)) = self.size_entry_target(cx) {
            context.add(SIZE_READOUT_CONTEXT);
            if self
                .size_entry
                .as_ref()
                .is_some_and(|entry| entry.is_for(document_id.value(), &anchor) && entry.has_text())
            {
                context.add(SIZE_ENTRY_CONTEXT);
            }
        }
        context
    }

    pub(super) fn note_drawing_pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
    ) {
        self.drawing_pointer = Some((position, modifiers));
    }

    /// Hands the typed sizes to the drawing code and moves the preview to
    /// them, as if the pointer had moved.
    fn apply_size_entry(&mut self, cx: &mut Context<Self>) {
        let Some((entry, scale, frame)) = self.current_size_entry(cx) else {
            return;
        };
        let size = entry.typed_size(&scale, frame);
        let document_id = entry.document_id;
        if let Some(session) = self
            .active_document_id
            .and_then(|id| self.session(id, cx).cloned())
        {
            session.update(cx, |session, _| {
                session.annotations.set_typed_size(document_id, Some(size));
            });
        }
        self.replay_drawing_pointer(cx);
        cx.notify();
    }

    fn replay_drawing_pointer(&mut self, cx: &mut Context<Self>) {
        let Some((position, modifiers)) = self.drawing_pointer else {
            return;
        };
        if self
            .active_annotation_pointer
            .is_some_and(|active| active.placement_pending)
        {
            self.update_annotation_pointer(position, modifiers, cx);
        } else {
            self.update_annotation_hover(position, modifiers, cx);
        }
    }

    /// Types a key into the size being drawn. Returns whether it was taken.
    pub(super) fn handle_size_entry_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let modifiers = &event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
            return false;
        }
        let Some(character) = event.keystroke.key_char.as_deref().and_then(|text| {
            let mut characters = text.chars();
            characters.next().filter(|_| characters.next().is_none())
        }) else {
            return false;
        };
        let Some((mut entry, ..)) = self.current_size_entry(cx) else {
            return false;
        };
        // A size starts with a number; until then letters stay tool shortcuts.
        if !entry.has_text() && !(character.is_ascii_digit() || matches!(character, '.' | '-')) {
            return false;
        }
        if !entry.type_char(character) {
            return false;
        }
        self.size_entry = Some(entry);
        self.apply_size_entry(cx);
        true
    }

    /// Backspace while a size is typed edits it rather than deleting markups.
    pub(super) fn size_entry_backspace(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((mut entry, ..)) = self.current_size_entry(cx) else {
            return false;
        };
        if !entry.backspace() {
            return false;
        }
        self.size_entry = Some(entry);
        self.apply_size_entry(cx);
        true
    }

    pub(super) fn size_entry_tab(&mut self, backwards: bool, cx: &mut Context<Self>) -> bool {
        let Some((mut entry, ..)) = self.current_size_entry(cx) else {
            return false;
        };
        if backwards {
            entry.previous_field();
        } else {
            entry.next_field();
        }
        self.size_entry = Some(entry);
        cx.notify();
        true
    }

    /// Escape first clears what was typed; a second Escape cancels as usual.
    pub(super) fn size_entry_escape(&mut self, cx: &mut Context<Self>) -> bool {
        let Some((entry, ..)) = self.current_size_entry(cx) else {
            return false;
        };
        if !entry.has_text() {
            return false;
        }
        self.size_entry = Some(SizeEntry::new(entry.document_id, entry.anchor));
        self.apply_size_entry(cx);
        true
    }

    /// Enter with a size typed places the markup's next point there, as a
    /// click would.
    pub(super) fn size_entry_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some((entry, ..)) = self.current_size_entry(cx) else {
            return false;
        };
        if !entry.has_text() {
            return false;
        }
        let Some((position, modifiers)) = self.drawing_pointer else {
            return false;
        };
        self.apply_size_entry(cx);
        if self.begin_annotation_pointer(position, modifiers, 1, window, cx) {
            self.finish_annotation_pointer(position, modifiers, window, cx);
        }
        self.size_entry = None;
        self.replay_drawing_pointer(cx);
        cx.notify();
        true
    }

    pub(super) fn size_entry_view(&self, cx: &App) -> Option<SizeEntryView> {
        let (document_id, anchor, scale, frame) = self.size_entry_target(cx)?;
        let (position, _) = self.drawing_pointer?;
        let entry = self
            .size_entry
            .clone()
            .filter(|entry| entry.is_for(document_id.value(), &anchor))
            .unwrap_or_else(|| SizeEntry::new(document_id.value(), anchor));
        let fields = entry.fields(anchor.current, &scale, frame);
        Some(SizeEntryView {
            position,
            fields,
            scale: scale_text(&scale),
        })
    }

    pub(super) fn render_size_entry(&self, cx: &App) -> Option<AnyElement> {
        let view = self.size_entry_view(cx)?;
        let theme = cx.theme();
        let field = |field: &SizeEntryField| {
            h_flex()
                .gap_1()
                .items_center()
                .child(div().text_color(theme.muted_foreground).child(field.label))
                .child(
                    div()
                        .min_w(px(56.))
                        .px_1()
                        .rounded_sm()
                        .border_1()
                        .border_color(if field.active {
                            theme.ring
                        } else {
                            theme.border
                        })
                        .when(field.typed, |value| value.bg(theme.background))
                        .text_color(if field.incomplete {
                            theme.danger
                        } else if field.typed {
                            theme.foreground
                        } else {
                            theme.muted_foreground
                        })
                        .child(field.text.clone()),
                )
        };
        Some(
            gpui::deferred(
                gpui::anchored()
                    .position(view.position + gpui::point(px(16.), px(16.)))
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        h_flex()
                            .id(SIZE_ENTRY_ID)
                            .debug_selector(|| SIZE_ENTRY_ID.into())
                            .gap_2()
                            .px_1p5()
                            .py_1()
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.popover)
                            .shadow_md()
                            .text_xs()
                            .whitespace_nowrap()
                            .child(field(&view.fields[0]))
                            .child(field(&view.fields[1]))
                            .child(
                                div()
                                    .pl_2()
                                    .border_l_1()
                                    .border_color(theme.border)
                                    .text_color(theme.muted_foreground)
                                    .child(view.scale),
                            ),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}
