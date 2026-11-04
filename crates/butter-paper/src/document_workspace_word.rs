//! Convert to Word Document…: a dialog that converts the active document
//! with Butter Paper's own converter ([`crate::docx_export`]) into a Word
//! document saved beside the PDF. The dialog stays up throughout: it asks
//! first (Replace or Keep Both when the Word document exists), shows the
//! save and each page converted on a progress bar, then offers to show the
//! Word document in the file manager or open it in the system's app for
//! Word documents, or says why it could not convert. A PDF that is another
//! app's copy or on a read-only disk converts into Downloads instead
//! ([`crate::save_places`]). When the system will not let it save in the
//! folder (macOS protects Downloads, Documents and the Desktop; Windows'
//! Controlled folder access), it offers Downloads or a Save panel.

use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use gpui::div;
use gpui_component::Icon;

use crate::dialog_buttons::dialog_close_button;
use crate::docx_export::{ConvertError, ConvertStep};

pub const DOCUMENT_WORD_DIALOG_ID: &str = "document-word-dialog";
pub const DOCUMENT_WORD_PROGRESS_ID: &str = "document-word-progress";
pub const DOCUMENT_WORD_CANCEL_ID: &str = "document-word-cancel";
pub const DOCUMENT_WORD_CONVERT_ID: &str = "document-word-convert";
pub const DOCUMENT_WORD_KEEP_BOTH_ID: &str = "document-word-keep-both";
pub const DOCUMENT_WORD_REPLACE_ID: &str = "document-word-replace";
pub const DOCUMENT_WORD_SHOW_ID: &str = "document-word-show";
/// After the system refused the PDF's folder: a Save panel for another place.
pub const DOCUMENT_WORD_CHOOSE_ID: &str = "document-word-choose";
/// After the system refused the PDF's folder: the Word document in Downloads.
pub const DOCUMENT_WORD_DOWNLOADS_ID: &str = "document-word-downloads";
/// After the system refused the PDF's folder, with no Downloads to offer:
/// the setting that allows that folder.
pub const DOCUMENT_WORD_PRIVACY_ID: &str = "document-word-privacy";
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum WordTarget {
    /// `Name.docx`, asking first if it exists by then.
    New,
    Replace,
    /// The first free `Name 2.docx`, `Name 3.docx`, ….
    KeepBoth,
    /// Where it was chosen in a Save panel.
    Chosen(PathBuf),
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
    /// The system did not let it save in the PDF's folder.
    NotPermitted,
}

/// The Convert to Word Document dialog's state.
pub struct WordDialog {
    document_id: DocumentId,
    /// The PDF's name and where it is.
    pdf: PathBuf,
    /// Where its Word document goes: beside it, or Downloads for another
    /// app's copy.
    folder: PathBuf,
    /// The folder as a person knows it: “Documents”, or “this PDF’s
    /// folder” for one with a name only an app knows.
    folder_label: String,
    /// The Downloads folder, when it is not `folder`.
    downloads: Option<PathBuf>,
    /// The PDF has changes to save before it converts.
    unsaved: bool,
    /// The system's privacy protection refused the folder (which a setting
    /// can allow), not the folder's own permissions.
    privacy_refused: bool,
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

    /// The setting that allows the refused folder, when one would.
    fn folder_permission(&self) -> Option<crate::save_places::FolderPermission> {
        self.privacy_refused
            .then(|| crate::save_places::folder_permission(&self.folder))
            .flatten()
    }

    /// Where its Word document goes, and how the dialog names it.
    fn set_folder(&mut self, (folder, label, downloads): (PathBuf, String, Option<PathBuf>)) {
        self.folder = folder;
        self.folder_label = label;
        self.downloads = downloads;
    }

    /// The name of the folder `path` is in.
    fn folder_name(path: &Path) -> String {
        path.parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "that folder".to_owned())
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
            _ => Self::file_name(&crate::docx_export::docx_path_in(&self.folder, &self.pdf)),
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
        let beside = self.pdf.parent() == Some(self.folder.as_path());
        let destination = if beside {
            "beside the PDF".to_owned()
        } else {
            format!("in {}", self.folder_label)
        };
        let description = match &self.stage {
            WordDialogStage::Ready
            | WordDialogStage::Saving
            | WordDialogStage::Converting { .. } => Some(if self.unsaved {
                format!(
                    "Your changes are saved to the PDF first. The text, tables and images then \
                     convert into a Word document you can edit, saved {destination}."
                )
            } else {
                format!(
                    "The text, tables and images convert into a Word document you can edit, \
                     saved {destination}."
                )
            }),
            _ => None,
        };
        let place = |docx: &Path| {
            if docx.parent() == self.pdf.parent() {
                format!("beside {pdf_name}")
            } else {
                format!("in {}", Self::folder_name(docx))
            }
        };
        let subtitle = match &self.stage {
            WordDialogStage::Converted(converted) => format!(
                "{} {}, saved {}",
                converted.pages,
                if converted.pages == 1 {
                    "page"
                } else {
                    "pages"
                },
                place(&converted.docx)
            ),
            WordDialogStage::Converting { docx, .. } => {
                let place = place(docx);
                let mut place = place.chars();
                place
                    .next()
                    .map(|first| first.to_uppercase().chain(place).collect())
                    .unwrap_or_default()
            }
            _ => {
                if beside {
                    format!("Beside {pdf_name}")
                } else {
                    format!("In {}", self.folder_label)
                }
            }
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
                    "{} is already {destination}. Replace it, or keep both and save this one \
                     under a new name.",
                    self.docx_name()
                ),
            )
        });
        let failure = match &self.stage {
            WordDialogStage::Failed(error) => {
                Some(Alert::error("document-word-failed", error.clone()))
            }
            WordDialogStage::NotPermitted => {
                let folder = &self.folder_label;
                let other = if self.downloads.is_some() {
                    "Save it in Downloads or another folder"
                } else {
                    "Choose another folder"
                };
                Some(
                    Alert::warning(
                        "document-word-not-permitted",
                        match self.folder_permission() {
                            Some(permission) if cfg!(windows) => format!(
                                "Controlled folder access may protect it. {other}. {}",
                                permission.sentence("convert again")
                            ),
                            Some(permission) => {
                                format!("{other}. {}", permission.sentence("convert again"))
                            }
                            None => format!("{other}."),
                        },
                    )
                    .title(if !self.privacy_refused {
                        format!("You don’t have permission to save in {folder}")
                    } else if cfg!(target_os = "macos") {
                        format!("macOS didn’t let Butter Paper save in {folder}")
                    } else if cfg!(windows) {
                        format!("Windows didn’t let Butter Paper save in {folder}")
                    } else {
                        format!("Butter Paper can’t save in {folder}")
                    }),
                )
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
        // A PDF that can't be saved where it opened still converts as it is.
        let unsaved = dirty;
        let (folder, folder_label, downloads) = self.word_folder(document_id, &pdf, cx);
        let exists = crate::docx_export::docx_path_in(&folder, &pdf).exists();
        let dialog = cx.new(|_| WordDialog {
            document_id,
            pdf,
            folder,
            folder_label,
            downloads,
            unsaved,
            privacy_refused: false,
            exists,
            stage: WordDialogStage::Ready,
            stop: Arc::new(AtomicBool::new(false)),
        });
        self.word_dialog = Some(dialog.downgrade());
        let workspace = cx.entity().downgrade();
        window.open_dialog(cx, move |surface, _, cx| {
            let state = dialog.read(cx);
            let ready = matches!(state.stage, WordDialogStage::Ready);
            let working = matches!(
                state.stage,
                WordDialogStage::Saving | WordDialogStage::Converting { .. }
            );
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
                WordDialogStage::NotPermitted => {
                    let choose = {
                        let workspace = workspace.clone();
                        Button::new(DOCUMENT_WORD_CHOOSE_ID)
                            .debug_selector(|| DOCUMENT_WORD_CHOOSE_ID.into())
                            .label("Choose where to save…")
                            .on_click(move |_, _, cx| {
                                let _ = workspace.update(cx, |workspace, cx| {
                                    workspace.choose_word_document_path(cx)
                                });
                            })
                    };
                    let footer = match state.downloads.clone() {
                        // Downloads in one press, as the place to go.
                        Some(downloads) => {
                            let workspace = workspace.clone();
                            let pdf = state.pdf.clone();
                            DialogFooter::new().child(choose.outline()).child(
                                Button::new(DOCUMENT_WORD_DOWNLOADS_ID)
                                    .debug_selector(|| DOCUMENT_WORD_DOWNLOADS_ID.into())
                                    .primary()
                                    .label("Save to Downloads")
                                    .on_click(move |_, _, cx| {
                                        let docx = crate::docx_export::unused_docx_path_in(
                                            &downloads, &pdf,
                                        );
                                        let _ = workspace.update(cx, |workspace, cx| {
                                            workspace.convert_from_word_dialog(
                                                WordTarget::Chosen(docx),
                                                cx,
                                            )
                                        });
                                    }),
                            )
                        }
                        None => DialogFooter::new()
                            .children(state.folder_permission().map(|permission| {
                                Button::new(DOCUMENT_WORD_PRIVACY_ID)
                                    .debug_selector(|| DOCUMENT_WORD_PRIVACY_ID.into())
                                    .outline()
                                    .label(permission.button)
                                    .on_click(move |_, _, cx| cx.open_url(permission.url))
                            }))
                            .child(choose.primary()),
                    };
                    Some(footer)
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
                // A press beside it closes it, as for any dialog, except
                // while it works: a stray press must not stop a conversion.
                .overlay_closable(!working)
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
                            .as_ref()
                            .is_some_and(|(waiting, _)| *waiting == document_id)
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
        if dirty {
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
        let Some((waiting, _)) = &self.word_after_save else {
            return;
        };
        if *waiting != document_id {
            return;
        }
        let Some((_, target)) = self.word_after_save.take() else {
            return;
        };
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

    /// Where the document's Word document goes: beside its PDF at `pdf`,
    /// or Downloads when that is another app's copy or on a read-only disk;
    /// with the folder's name as the dialog shows it, and Downloads when
    /// that is somewhere else to offer.
    fn word_folder(
        &self,
        document_id: DocumentId,
        pdf: &Path,
        cx: &App,
    ) -> (PathBuf, String, Option<PathBuf>) {
        let downloads = self.downloads_folder().map(Path::to_path_buf);
        let beside = pdf.parent().map(Path::to_path_buf).unwrap_or_default();
        let folder = match (self.document_source_place(document_id, cx), &downloads) {
            (Some(place), Some(downloads)) if place.moves_new_files() => downloads.clone(),
            _ => beside,
        };
        let label = if downloads.as_ref() == Some(&folder) {
            "Downloads".to_owned()
        } else if self.save_places.is_app_copy(pdf) && pdf.parent() == Some(folder.as_path()) {
            "this PDF’s folder".to_owned()
        } else {
            folder
                .file_name()
                .map(|name| format!("“{}”", name.to_string_lossy()))
                .unwrap_or_else(|| "this folder".to_owned())
        };
        let downloads = downloads.filter(|downloads| *downloads != folder);
        (folder, label, downloads)
    }

    /// Converts the saved PDF at `pdf`, or, when `Name.docx` turned out to
    /// be there already, asks again with Replace and Keep Both.
    fn convert_saved_to_word(&mut self, pdf: PathBuf, target: WordTarget, cx: &mut Context<Self>) {
        let Some(dialog) = self.word_dialog() else {
            return;
        };
        // A Save As before it may have moved the PDF.
        let place = self.word_folder(dialog.read(cx).document_id, &pdf, cx);
        let folder = place.0.clone();
        dialog.update(cx, |dialog, _| dialog.set_folder(place));
        let docx = match target {
            WordTarget::Chosen(docx) => docx,
            WordTarget::Replace => crate::docx_export::docx_path_in(&folder, &pdf),
            WordTarget::KeepBoth => crate::docx_export::unused_docx_path_in(&folder, &pdf),
            WordTarget::New => {
                let docx = crate::docx_export::docx_path_in(&folder, &pdf);
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
                // As people read it: without Windows' `\\?\` prefix,
                // which an open document's canonical path has.
                docx: dunce::simplified(&docx).to_path_buf(),
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
            let result: Result<Option<ConvertedWordDocument>, ConvertError> = task.await;
            let _ = shown.update(cx, |dialog, cx| match result {
                Ok(Some(converted)) => dialog.set_stage(WordDialogStage::Converted(converted), cx),
                // Stopped: the dialog has closed.
                Ok(None) => {}
                Err(ConvertError::NotPermitted(error)) => {
                    dialog.privacy_refused = crate::save_places::refused_by_privacy(&error);
                    dialog.set_stage(WordDialogStage::NotPermitted, cx)
                }
                Err(ConvertError::Failed(error)) => {
                    dialog.set_stage(WordDialogStage::Failed(error), cx)
                }
            });
        })
        .detach();
    }

    /// After the system refused the folder: a Save panel, starting in
    /// Downloads, for where the Word document goes, then converts to it.
    fn choose_word_document_path(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.word_dialog() else {
            return;
        };
        let (pdf, folder) = {
            let dialog = dialog.read(cx);
            (dialog.pdf.clone(), dialog.folder.clone())
        };
        let docx = crate::docx_export::docx_path_in(&folder, &pdf);
        let directory = self
            .downloads_folder()
            .map(Path::to_path_buf)
            .unwrap_or(folder);
        let name = docx
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let picker = cx.prompt_for_new_path(&directory, name.as_deref());
        cx.spawn(async move |workspace, cx| {
            let chosen = match picker.await {
                Ok(Ok(Some(path))) => path,
                // Dismissed: the dialog still offers it.
                Ok(Ok(None)) | Err(_) => return,
                Ok(Err(error)) => {
                    let _ = workspace.update(cx, |workspace, cx| {
                        if let Some(dialog) = workspace.word_dialog() {
                            dialog.update(cx, |dialog, cx| {
                                dialog.set_stage(
                                    WordDialogStage::Failed(format!(
                                        "Could not open the Save panel: {error}"
                                    )),
                                    cx,
                                )
                            });
                        }
                    });
                    return;
                }
            };
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.convert_from_word_dialog(WordTarget::Chosen(chosen), cx)
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
            .as_ref()
            .is_none_or(|(waiting, _)| *waiting != document_id)
        {
            return;
        }
        self.word_after_save = None;
        let unsaved = self
            .session(document_id, cx)
            .is_some_and(|session| session.read(cx).is_dirty());
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
