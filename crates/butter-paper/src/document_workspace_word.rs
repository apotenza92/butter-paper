//! Convert to Word Document…: a dialog that converts the active document
//! with Butter Paper's own converter ([`crate::docx_export`]) into a Word
//! document saved beside the PDF. The dialog stays up throughout: it asks
//! first (Replace or Keep Both when the Word document exists), shows the
//! save and each page converted on a progress bar, then offers to show the
//! Word document in the file manager or open it in the system's app for
//! Word documents, or says why it could not convert.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use gpui::div;
use gpui_component::Icon;

use crate::dialog_buttons::dialog_close_button;
use crate::docx_export::ConvertStep;

pub const DOCUMENT_WORD_DIALOG_ID: &str = "document-word-dialog";
pub const DOCUMENT_WORD_PROGRESS_ID: &str = "document-word-progress";
pub const DOCUMENT_WORD_CANCEL_ID: &str = "document-word-cancel";
pub const DOCUMENT_WORD_CONVERT_ID: &str = "document-word-convert";
pub const DOCUMENT_WORD_KEEP_BOTH_ID: &str = "document-word-keep-both";
pub const DOCUMENT_WORD_REPLACE_ID: &str = "document-word-replace";
pub const DOCUMENT_WORD_SHOW_ID: &str = "document-word-show";
pub const DOCUMENT_WORD_OPEN_ID: &str = "document-word-open";

/// A Word document just converted, and what the system calls the apps that
/// show it.
#[derive(Clone)]
pub struct ConvertedWordDocument {
    pub docx: PathBuf,
    pub pages: usize,
    /// The default app for Word documents, if the system has one.
    pub app: Option<String>,
    pub file_manager: String,
}

/// Where the Word document goes when `Name.docx` is beside the PDF already.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WordTarget {
    /// `Name.docx`, asking first if it exists by then.
    New,
    Replace,
    /// The first free `Name 2.docx`, `Name 3.docx`, ….
    KeepBoth,
}

/// What the Convert to Word Document dialog shows.
pub enum WordDialogStage {
    /// Asking to convert, or to replace the Word document beside the PDF.
    Ready,
    /// Saving unsaved changes first.
    Saving,
    Converting {
        docx: PathBuf,
        step: ConvertStep,
    },
    Converted(ConvertedWordDocument),
    Failed(String),
}

/// The Convert to Word Document dialog's state.
pub struct WordDialog {
    document_id: DocumentId,
    /// The PDF's name and where it is.
    pdf: PathBuf,
    /// The PDF has changes to save before it converts.
    unsaved: bool,
    /// `Name.docx` is beside the PDF already.
    exists: bool,
    stage: WordDialogStage,
    /// Set when the dialog closes, to stop a conversion under way.
    stop: Arc<AtomicBool>,
}

impl WordDialog {
    pub fn stage(&self) -> &WordDialogStage {
        &self.stage
    }

    fn set_stage(&mut self, stage: WordDialogStage, cx: &mut Context<Self>) {
        self.stage = stage;
        cx.notify();
    }

    fn working(&self) -> bool {
        matches!(
            self.stage,
            WordDialogStage::Saving | WordDialogStage::Converting { .. }
        )
    }

    fn file_name(path: &Path) -> String {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The Word document's name: the one being written once it is known.
    fn docx_name(&self) -> String {
        match &self.stage {
            WordDialogStage::Converting { docx, .. } => Self::file_name(docx),
            WordDialogStage::Converted(converted) => Self::file_name(&converted.docx),
            _ => Self::file_name(&crate::docx_export::docx_path_for(&self.pdf)),
        }
    }

    /// The progress bar's value (out of 100) and whether it is still
    /// waiting to measure, and the line under it.
    fn progress(&self) -> Option<(f32, bool, String)> {
        match &self.stage {
            WordDialogStage::Saving => Some((0., true, "Saving the PDF…".into())),
            WordDialogStage::Converting { step, docx } => Some(match *step {
                ConvertStep::Reading => (0., true, "Reading the PDF…".into()),
                ConvertStep::Page { done, count } => (
                    // The last stretch is writing the Word document.
                    done as f32 / count.max(1) as f32 * 95.,
                    false,
                    format!("Converting page {} of {count}…", (done + 1).min(count)),
                ),
                ConvertStep::Writing => (95., false, format!("Saving {}…", Self::file_name(docx))),
            }),
            _ => None,
        }
    }
}

impl Render for WordDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let pdf_name = Self::file_name(&self.pdf);
        let docx_name = self.docx_name();
        // Kept while it works, so the dialog does not jump as it starts.
        let description = match &self.stage {
            WordDialogStage::Ready
            | WordDialogStage::Saving
            | WordDialogStage::Converting { .. } => Some(if self.unsaved {
                "Your changes are saved to the PDF first. The text, tables and images then \
                 convert into a Word document you can edit, saved beside the PDF."
            } else {
                "The text, tables and images convert into a Word document you can edit, saved \
                 beside the PDF."
            }),
            _ => None,
        };
        let subtitle = match &self.stage {
            WordDialogStage::Converted(converted) => format!(
                "{} {}, saved beside {pdf_name}",
                converted.pages,
                if converted.pages == 1 {
                    "page"
                } else {
                    "pages"
                }
            ),
            _ => format!("Beside {pdf_name}"),
        };
        let progress = self.progress();
        let done = matches!(self.stage, WordDialogStage::Converted(_));
        // The Word document being made: its progress shows in its row, as a
        // download's does.
        let file = h_flex()
            .w_full()
            .gap_3()
            .p_3()
            .items_center()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .flex_none()
                    .size_9()
                    .items_center()
                    .justify_center()
                    .rounded(theme.radius)
                    .bg(theme.muted)
                    .child(
                        Icon::new(IconName::File)
                            .text_color(theme.muted_foreground)
                            .size_5(),
                    ),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(div().text_sm().font_medium().truncate().child(docx_name))
                    .map(|column| match progress {
                        Some((value, waiting, label)) => column
                            .child(
                                div()
                                    .debug_selector(|| DOCUMENT_WORD_PROGRESS_ID.into())
                                    .child(
                                        Progress::new(DOCUMENT_WORD_PROGRESS_ID)
                                            .value(value)
                                            .loading(waiting),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .truncate()
                                    .child(label),
                            ),
                        None => column.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .truncate()
                                .child(subtitle),
                        ),
                    }),
            )
            .when(done, |row| {
                row.child(
                    Icon::new(IconName::CircleCheck)
                        .flex_none()
                        .text_color(theme.success)
                        .size_5(),
                )
            });
        let exists = matches!(self.stage, WordDialogStage::Ready) && self.exists;
        let replace_note = exists.then(|| {
            Alert::warning(
                "document-word-exists",
                format!(
                    "{} is already beside the PDF. Replace it, or keep both and save this one \
                     under a new name.",
                    self.docx_name()
                ),
            )
        });
        let failure = match &self.stage {
            WordDialogStage::Failed(error) => {
                Some(Alert::error("document-word-failed", error.clone()))
            }
            _ => None,
        };
        v_flex()
            .id(DOCUMENT_WORD_DIALOG_ID)
            .debug_selector(|| DOCUMENT_WORD_DIALOG_ID.into())
            .w_full()
            .gap_3()
            .children(description.map(|description| {
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(description)
            }))
            .child(file)
            .children(replace_note)
            .children(failure)
    }
}

impl DocumentWorkspace {
    /// File ▸ Convert to Word Document…: opens the dialog on the next frame.
    pub fn convert_active_document_to_word(&mut self, cx: &mut Context<Self>) {
        if self.word_dialog().is_some() {
            return;
        }
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        self.word_dialog_requested = Some(document_id);
        cx.notify();
    }

    /// The open Convert to Word Document dialog, if any.
    pub fn word_dialog(&self) -> Option<Entity<WordDialog>> {
        self.word_dialog.as_ref().and_then(WeakEntity::upgrade)
    }

    pub(super) fn open_word_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.word_dialog_requested.take() else {
            return;
        };
        if window.has_active_dialog(cx) || self.word_dialog().is_some() {
            return;
        }
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let (pdf, dirty) = {
            let session = session.read(cx);
            (session.path.clone(), session.is_dirty())
        };
        let unsaved = dirty || self.document_requires_save_as(document_id, cx);
        let exists = crate::docx_export::docx_path_for(&pdf).exists();
        let dialog = cx.new(|_| WordDialog {
            document_id,
            pdf,
            unsaved,
            exists,
            stage: WordDialogStage::Ready,
            stop: Arc::new(AtomicBool::new(false)),
        });
        self.word_dialog = Some(dialog.downgrade());
        let workspace = cx.entity().downgrade();
        window.open_dialog(cx, move |surface, _, cx| {
            let state = dialog.read(cx);
            let ready = matches!(state.stage, WordDialogStage::Ready);
            let footer = match &state.stage {
                WordDialogStage::Ready => {
                    let keep_both = state.exists.then(|| {
                        let workspace = workspace.clone();
                        Button::new(DOCUMENT_WORD_KEEP_BOTH_ID)
                            .debug_selector(|| DOCUMENT_WORD_KEEP_BOTH_ID.into())
                            .outline()
                            .label("Keep both")
                            .on_click(move |_, _, cx| {
                                let _ = workspace.update(cx, |workspace, cx| {
                                    workspace.convert_from_word_dialog(WordTarget::KeepBoth, cx)
                                });
                            })
                    });
                    let commit = if state.exists {
                        Button::new(DOCUMENT_WORD_REPLACE_ID)
                            .debug_selector(|| DOCUMENT_WORD_REPLACE_ID.into())
                            .danger()
                            .label("Replace")
                    } else {
                        Button::new(DOCUMENT_WORD_CONVERT_ID)
                            .debug_selector(|| DOCUMENT_WORD_CONVERT_ID.into())
                            .primary()
                            .label(if state.unsaved {
                                "Save and convert"
                            } else {
                                "Convert"
                            })
                    };
                    Some(
                        DialogFooter::new()
                            .child(dialog_close_button(
                                Button::new(DOCUMENT_WORD_CANCEL_ID)
                                    .debug_selector(|| DOCUMENT_WORD_CANCEL_ID.into())
                                    .outline()
                                    .label("Cancel"),
                            ))
                            .children(keep_both)
                            .child(dialog_action_button(commit)),
                    )
                }
                WordDialogStage::Converted(converted) => {
                    let show_path = converted.docx.clone();
                    Some(
                        DialogFooter::new()
                            .child(
                                Button::new(DOCUMENT_WORD_SHOW_ID)
                                    .debug_selector(|| DOCUMENT_WORD_SHOW_ID.into())
                                    .outline()
                                    .label(format!("Show in {}", converted.file_manager))
                                    .on_click(move |_, window, cx| {
                                        window.close_dialog(cx);
                                        if !crate::system_apps::reveal_in_file_explorer(&show_path)
                                        {
                                            cx.reveal_path(&show_path);
                                        }
                                    }),
                            )
                            .child(dialog_action_button(
                                Button::new(DOCUMENT_WORD_OPEN_ID)
                                    .debug_selector(|| DOCUMENT_WORD_OPEN_ID.into())
                                    .primary()
                                    .label(match &converted.app {
                                        Some(app) => format!("Open in {app}"),
                                        None => "Open".to_owned(),
                                    }),
                            )),
                    )
                }
                _ => None,
            };
            let (ok_dialog, ok_workspace) = (dialog.clone(), workspace.clone());
            let (stop, close_workspace) = (state.stop.clone(), workspace.clone());
            surface
                .title("Convert to Word document")
                .w(px(460.))
                .max_w_full()
                // Cancel decides while it asks; afterwards the X closes it.
                .close_button(!ready)
                // A stray press beside it must not stop a conversion.
                .overlay_closable(false)
                .child(dialog.clone())
                .when_some(footer, |surface, footer| surface.footer(footer))
                .on_ok(move |_, _, cx| match &ok_dialog.read(cx).stage {
                    WordDialogStage::Ready => {
                        let target = if ok_dialog.read(cx).exists {
                            WordTarget::Replace
                        } else {
                            WordTarget::New
                        };
                        let _ = ok_workspace.update(cx, |workspace, cx| {
                            workspace.convert_from_word_dialog(target, cx)
                        });
                        false
                    }
                    WordDialogStage::Converted(converted) => {
                        cx.open_with_system(&converted.docx);
                        true
                    }
                    _ => false,
                })
                .on_close(move |_, _, cx| {
                    stop.store(true, Ordering::Release);
                    // A save still under way stays a save: the conversion
                    // after it is forgotten, not left for a later dialog.
                    let _ = close_workspace.update(cx, |workspace, _| {
                        if workspace
                            .word_after_save
                            .is_some_and(|(waiting, _)| waiting == document_id)
                        {
                            workspace.word_after_save = None;
                        }
                    });
                })
        });
    }

    /// The dialog's Convert, Replace or Keep Both: saves first when the PDF
    /// has unsaved changes (Save As for one that needs a file).
    fn convert_from_word_dialog(&mut self, target: WordTarget, cx: &mut Context<Self>) {
        let Some(dialog) = self.word_dialog() else {
            return;
        };
        if dialog.read(cx).working() {
            return;
        }
        let document_id = dialog.read(cx).document_id;
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let (dirty, path) = {
            let session = session.read(cx);
            (session.is_dirty(), session.path.clone())
        };
        if dirty || self.document_requires_save_as(document_id, cx) {
            self.word_after_save = Some((document_id, target));
            dialog.update(cx, |dialog, cx| {
                dialog.set_stage(WordDialogStage::Saving, cx)
            });
            if self.active_document_id == Some(document_id) {
                self.save_active_document(cx);
            }
            return;
        }
        self.convert_saved_to_word(path, target, cx);
    }

    /// After a save of `document_id`: converts it when that save was for
    /// Convert to Word Document and left nothing unsaved.
    pub(super) fn convert_saved_document_to_word(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) {
        let Some((waiting, target)) = self.word_after_save else {
            return;
        };
        if waiting != document_id {
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
            self.cancel_word_after_save(document_id, None, cx);
            return;
        }
        self.convert_saved_to_word(path, target, cx);
    }

    /// Converts the saved PDF at `pdf`, or, when `Name.docx` turned out to
    /// be beside it already, asks again with Replace and Keep Both.
    fn convert_saved_to_word(&mut self, pdf: PathBuf, target: WordTarget, cx: &mut Context<Self>) {
        let Some(dialog) = self.word_dialog() else {
            return;
        };
        let docx = match target {
            WordTarget::Replace => crate::docx_export::docx_path_for(&pdf),
            WordTarget::KeepBoth => crate::docx_export::unused_docx_path_for(&pdf),
            WordTarget::New => {
                let docx = crate::docx_export::docx_path_for(&pdf);
                if docx.exists() {
                    dialog.update(cx, |dialog, cx| {
                        dialog.pdf = pdf;
                        dialog.unsaved = false;
                        dialog.exists = true;
                        dialog.set_stage(WordDialogStage::Ready, cx);
                    });
                    return;
                }
                docx
            }
        };
        let stop = dialog.update(cx, |dialog, cx| {
            dialog.pdf = pdf.clone();
            dialog.unsaved = false;
            dialog.set_stage(
                WordDialogStage::Converting {
                    docx: docx.clone(),
                    step: ConvertStep::Reading,
                },
                cx,
            );
            dialog.stop.clone()
        });
        let (steps, progress) = async_channel::unbounded::<ConvertStep>();
        let task = cx.background_executor().spawn(async move {
            let mut pages = 0;
            let finished = crate::docx_export::convert(&pdf, &docx, &mut |step| {
                if let ConvertStep::Page { count, .. } = step {
                    pages = count;
                }
                let _ = steps.send_blocking(step);
                !stop.load(Ordering::Acquire)
            })?;
            drop(steps);
            Ok(finished.then(|| ConvertedWordDocument {
                docx,
                pages,
                // Asked here, off the main thread: Linux asks `xdg-mime`.
                app: crate::system_apps::word_document_app(),
                file_manager: crate::system_apps::file_manager(),
            }))
        });
        let shown = dialog.downgrade();
        cx.spawn(async move |_, cx| {
            while let Ok(step) = progress.recv().await {
                let _ = shown.update(cx, |dialog, cx| {
                    if let WordDialogStage::Converting { step: shown, .. } = &mut dialog.stage {
                        *shown = step;
                        cx.notify();
                    }
                });
            }
            let result: Result<Option<ConvertedWordDocument>, String> = task.await;
            let _ = shown.update(cx, |dialog, cx| match result {
                Ok(Some(converted)) => dialog.set_stage(WordDialogStage::Converted(converted), cx),
                // Stopped: the dialog has closed.
                Ok(None) => {}
                Err(error) => dialog.set_stage(WordDialogStage::Failed(error), cx),
            });
        })
        .detach();
    }

    /// Forgets a conversion waiting on a save that did not happen: back to
    /// asking when the save was dismissed, or why when it failed.
    pub(super) fn cancel_word_after_save(
        &mut self,
        document_id: DocumentId,
        error: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .word_after_save
            .is_none_or(|(waiting, _)| waiting != document_id)
        {
            return;
        }
        self.word_after_save = None;
        let unsaved = self
            .session(document_id, cx)
            .is_some_and(|session| session.read(cx).is_dirty())
            || self.document_requires_save_as(document_id, cx);
        if let Some(dialog) = self.word_dialog() {
            dialog.update(cx, |dialog, cx| {
                dialog.unsaved = unsaved;
                let stage = match error {
                    Some(error) => WordDialogStage::Failed(format!(
                        "The PDF could not be saved, so it was not converted: {error}"
                    )),
                    None => WordDialogStage::Ready,
                };
                dialog.set_stage(stage, cx);
            });
        }
    }
}
