//! The prompt for a PDF whose changes can't go back to it (another app's
//! copy, a read-only file or disk): asked once, on its first change, with
//! Save to Downloads and Choose where to save….

use super::*;
use crate::dialog_buttons::dialog_close_button;

impl DocumentWorkspace {
    /// The active document, when it has just had its first change and
    /// should now be asked where to save: not while a text edit is still
    /// under way, which the prompt would interrupt.
    pub(super) fn source_place_prompt_due(&self, cx: &App) -> Option<DocumentId> {
        if self.pending_text_box_editor.is_some()
            || self.page_text_editing.is_some()
            || self.form_field_editor.is_some()
        {
            return None;
        }
        let document_id = self.active_document_id()?;
        let session = self.session(document_id, cx)?.read(cx);
        (session.source_place.is_some()
            && !session.source_place_asked
            && !session.save_as_required
            && session.is_dirty()
            && session.save_status != NativeDocumentSaveStatus::Saving)
            .then_some(document_id)
    }

    pub(super) fn open_source_place_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(document_id) = self.source_place_prompt_due(cx) else {
            return;
        };
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let Some(place) = session.read(cx).source_place else {
            return;
        };
        session.update(cx, |session, _| session.source_place_asked = true);
        let downloads = self.downloads_folder().is_some();
        let workspace = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, cx| {
            let choose = {
                let workspace = workspace.clone();
                Button::new(DOCUMENT_SOURCE_PLACE_CHOOSE_ID)
                    .debug_selector(|| DOCUMENT_SOURCE_PLACE_CHOOSE_ID.into())
                    .label("Choose where to save…")
                    .on_click(move |_, window, cx| {
                        window.close_dialog(cx);
                        let _ = workspace.update(cx, |workspace, cx| {
                            workspace.prompt_to_save_as(document_id, cx)
                        });
                    })
            };
            // Wrapping, so a narrow window or wide labels keep it inside.
            let footer = DialogFooter::new().flex_wrap().child(dialog_close_button(
                Button::new(DOCUMENT_SOURCE_PLACE_KEEP_EDITING_ID)
                    .debug_selector(|| DOCUMENT_SOURCE_PLACE_KEEP_EDITING_ID.into())
                    .outline()
                    .label("Keep editing"),
            ));
            let footer = if downloads {
                footer.child(choose.outline()).child(dialog_action_button(
                    Button::new(DOCUMENT_SOURCE_PLACE_DOWNLOADS_ID)
                        .debug_selector(|| DOCUMENT_SOURCE_PLACE_DOWNLOADS_ID.into())
                        .primary()
                        .label("Save to Downloads"),
                ))
            } else {
                footer.child(choose.primary())
            };
            let workspace = workspace.clone();
            dialog
                .title(place.title())
                .w(px(540.))
                .max_w_full()
                .close_button(false)
                .child(
                    gpui::div()
                        .debug_selector(|| DOCUMENT_SOURCE_PLACE_DIALOG_ID.into())
                        .pb_2()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(place.message()),
                )
                .footer(footer)
                .on_ok(move |_, _, cx| {
                    workspace
                        .update(cx, |workspace, cx| {
                            workspace.save_document_to_downloads(document_id, cx)
                        })
                        .is_ok()
                })
        });
    }
}
