//! Convert and Open in Word: the active document as Word converts it,
//! saved beside the PDF ([`crate::word_export`]).

use super::*;

pub const DOCUMENT_WORD_REPLACE_CANCEL_ID: &str = "document-word-replace-cancel";
pub const DOCUMENT_WORD_KEEP_BOTH_ID: &str = "document-word-keep-both";
pub const DOCUMENT_WORD_REPLACE_ID: &str = "document-word-replace";

impl DocumentWorkspace {
    /// Converts the active document to Word and opens it there. Unsaved
    /// changes are saved first (Save As for a document that needs a file),
    /// so Word converts what the document shows.
    pub fn convert_active_document_to_word(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let (ready, dirty, path) = {
            let session = session.read(cx);
            (
                matches!(session.status, NativeDocumentStatus::Ready)
                    && session.save_status != NativeDocumentSaveStatus::Saving,
                session.is_dirty(),
                session.path.clone(),
            )
        };
        if !ready || self.word_conversions.contains(&document_id) {
            return;
        }
        if dirty {
            self.word_after_save = Some(document_id);
            self.annotation_statuses.insert(
                document_id,
                "Saving the PDF before converting it to Word…".into(),
            );
            self.save_active_document(cx);
            cx.notify();
            return;
        }
        self.convert_or_ask(document_id, path, cx);
    }

    /// After a save of `document_id`: converts it when that save was for
    /// Convert and Open in Word and left nothing unsaved.
    pub(super) fn convert_saved_document_to_word(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) {
        if self.word_after_save != Some(document_id) {
            return;
        }
        self.word_after_save = None;
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let (dirty, path) = {
            let session = session.read(cx);
            (session.is_dirty(), session.path.clone())
        };
        if dirty {
            return;
        }
        self.convert_or_ask(document_id, path, cx);
    }

    /// Converts into `Name.docx` beside the PDF, or, when that exists, asks
    /// whether to replace it or keep both (the next render opens the
    /// question, as it has the window).
    fn convert_or_ask(&mut self, document_id: DocumentId, pdf: PathBuf, cx: &mut Context<Self>) {
        let docx = crate::word_export::docx_path_for(&pdf);
        if docx.exists() {
            self.word_replace_prompt = Some((document_id, pdf));
            cx.notify();
            return;
        }
        self.start_word_conversion(document_id, pdf, docx, cx);
    }

    /// Asks whether to replace the Word document a conversion would save
    /// over, or keep both (the new one as `Name 2.docx` and on).
    pub(super) fn open_word_replace_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((document_id, pdf)) = self.word_replace_prompt.take() else {
            return;
        };
        if window.has_active_dialog(cx) {
            return;
        }
        let docx = crate::word_export::docx_path_for(&pdf);
        let name = docx
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let owner = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (keep_owner, keep_pdf) = (owner.clone(), pdf.clone());
            let (replace_owner, replace_pdf, replace_docx) =
                (owner.clone(), pdf.clone(), docx.clone());
            alert
                .close_button(false)
                .title(format!("Replace “{name}”?"))
                .description(
                    "A Word document with this name is already beside the PDF. Replacing it \
                     loses any changes made to it in Word.",
                )
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new(DOCUMENT_WORD_REPLACE_CANCEL_ID)
                                .debug_selector(|| DOCUMENT_WORD_REPLACE_CANCEL_ID.into())
                                .outline()
                                .label("Cancel")
                                .on_click(|_, window, cx| window.close_dialog(cx)),
                        )
                        .child(
                            Button::new(DOCUMENT_WORD_KEEP_BOTH_ID)
                                .debug_selector(|| DOCUMENT_WORD_KEEP_BOTH_ID.into())
                                .outline()
                                .label("Keep Both")
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let pdf = keep_pdf.clone();
                                    let docx = crate::word_export::unused_docx_path_for(&pdf);
                                    let _ = keep_owner.update(cx, |workspace, cx| {
                                        workspace.start_word_conversion(document_id, pdf, docx, cx)
                                    });
                                }),
                        )
                        .child(dialog_action_button(
                            Button::new(DOCUMENT_WORD_REPLACE_ID)
                                .debug_selector(|| DOCUMENT_WORD_REPLACE_ID.into())
                                .danger()
                                .label("Replace"),
                        )),
                )
                .on_ok(move |_, _, cx| {
                    let (pdf, docx) = (replace_pdf.clone(), replace_docx.clone());
                    replace_owner
                        .update(cx, |workspace, cx| {
                            workspace.start_word_conversion(document_id, pdf, docx, cx)
                        })
                        .is_ok()
                })
        });
    }

    /// Forgets a conversion waiting on a save that did not happen.
    pub(super) fn cancel_word_after_save(&mut self, document_id: DocumentId) {
        if self.word_after_save == Some(document_id) {
            self.word_after_save = None;
            if self
                .annotation_statuses
                .get(&document_id)
                .is_some_and(|status| status.starts_with("Saving the PDF before converting"))
            {
                self.annotation_statuses.remove(&document_id);
            }
        }
    }

    fn start_word_conversion(
        &mut self,
        document_id: DocumentId,
        pdf: PathBuf,
        docx: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.word_conversions.contains(&document_id) {
            return;
        }
        let name = docx
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.word_conversions.insert(document_id);
        self.annotation_statuses
            .insert(document_id, format!("Converting to Word as {name}…"));
        cx.notify();
        let task = cx
            .background_executor()
            .spawn(async move { crate::word_export::convert_and_open(&pdf, &docx) });
        cx.spawn(async move |workspace, cx| {
            let result = task.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.word_conversions.remove(&document_id);
                let status = match result {
                    Ok(()) => format!("Saved {name} beside the PDF and opened it in Word."),
                    Err(error) => error,
                };
                workspace.annotation_statuses.insert(document_id, status);
                cx.notify();
            });
        })
        .detach();
    }
}
