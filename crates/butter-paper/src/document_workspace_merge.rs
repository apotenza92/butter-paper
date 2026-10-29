//! Merging documents into a new one, from the Merge Documents dialog (File
//! ▸ Merge Documents…, or a tab's menu). The dialog lists the open
//! documents with tick boxes; other PDFs are added with Add PDFs… or by
//! dropping them on the dialog, and rows are dragged into order. Merging
//! reads each document as saved, so open documents with unsaved changes
//! are saved first. The result opens as a new untitled tab.

use super::*;
use crate::dialog_buttons::dialog_close_button;
use gpui::ExternalPaths;

gpui::actions!(document_workspace, [MergeDocuments]);

pub const DOCUMENT_MERGE_DIALOG_ID: &str = "document-merge-dialog";
pub const DOCUMENT_MERGE_CONFIRM_ID: &str = "document-merge-confirm";
pub const DOCUMENT_MERGE_CANCEL_ID: &str = "document-merge-cancel";
pub const DOCUMENT_MERGE_ADD_ID: &str = "document-merge-add";
const GRIP_ICON: &str = "icons/grip-vertical.svg";

/// How long merging waits for the documents it saved first.
const MERGE_SAVE_TIMEOUT: Duration = Duration::from_secs(120);

/// What a merge takes pages from.
#[derive(Clone, Debug, PartialEq)]
pub enum MergeSource {
    Open(DocumentId),
    File(PathBuf),
}

#[derive(Clone, Debug)]
struct MergeItem {
    /// Names the row while it moves.
    key: u64,
    source: MergeSource,
    title: SharedString,
    pages: Option<usize>,
    checked: bool,
    /// An open document with unsaved changes, saved before merging.
    unsaved: bool,
    /// Why it cannot be merged: never saved, or unreadable.
    blocked: Option<SharedString>,
    /// The file it is, so the same file is not added twice.
    path: PathBuf,
}

/// A row dragged to a new place in the list.
#[derive(Clone, Debug)]
struct MergeRowDrag {
    key: u64,
    title: SharedString,
}

/// The Merge Documents dialog's list.
pub struct MergeDialog {
    items: Vec<MergeItem>,
    next_key: u64,
}

impl MergeDialog {
    /// The documents ticked to merge, in order.
    pub fn chosen(&self) -> Vec<MergeSource> {
        self.items
            .iter()
            .filter(|item| item.checked && item.blocked.is_none())
            .map(|item| item.source.clone())
            .collect()
    }

    fn chosen_unsaved(&self) -> bool {
        self.items
            .iter()
            .any(|item| item.checked && item.blocked.is_none() && item.unsaved)
    }

    /// Adds PDFs to the end of the list, ticked; files already listed are
    /// left where they are.
    pub fn add_paths(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let listed = self
            .items
            .iter()
            .map(|item| item.path.clone())
            .collect::<Vec<_>>();
        for path in paths {
            let is_pdf = path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"));
            if !is_pdf || listed.contains(&path) {
                continue;
            }
            let key = self.next_key;
            self.next_key += 1;
            let title = path
                .file_name()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            self.items.push(MergeItem {
                key,
                source: MergeSource::File(path.clone()),
                title: title.into(),
                pages: None,
                checked: true,
                unsaved: false,
                blocked: None,
                path: path.clone(),
            });
            // Counted, and checked readable, without holding up the dialog.
            let count = cx.background_executor().spawn(async move {
                lopdf::Document::load(&path)
                    .map_err(|_| ())
                    .and_then(|document| {
                        if document.is_encrypted() || document.encryption_state.is_some() {
                            Err(())
                        } else {
                            Ok(document.get_pages().len())
                        }
                    })
            });
            cx.spawn(async move |dialog, cx| {
                let count = count.await;
                let _ = dialog.update(cx, |dialog, cx| {
                    if let Some(item) = dialog.items.iter_mut().find(|item| item.key == key) {
                        match count {
                            Ok(pages) => item.pages = Some(pages),
                            Err(()) => {
                                item.checked = false;
                                item.blocked =
                                    Some("Cannot be read, or is password protected".into());
                            }
                        }
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        cx.notify();
    }

    fn set_checked(&mut self, key: u64, checked: bool, cx: &mut Context<Self>) {
        if let Some(item) = self.items.iter_mut().find(|item| item.key == key) {
            item.checked = checked && item.blocked.is_none();
            cx.notify();
        }
    }

    fn remove(&mut self, key: u64, cx: &mut Context<Self>) {
        self.items.retain(|item| item.key != key);
        cx.notify();
    }

    /// Moves the row `key` to stand where `before` is.
    fn move_row(&mut self, key: u64, before: u64, cx: &mut Context<Self>) {
        if key == before {
            return;
        }
        let (Some(from), Some(to)) = (
            self.items.iter().position(|item| item.key == key),
            self.items.iter().position(|item| item.key == before),
        ) else {
            return;
        };
        let item = self.items.remove(from);
        self.items.insert(to, item);
        cx.notify();
    }

    fn prompt_for_pdfs(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add PDFs".into()),
            allowed_extensions: Some(vec!["pdf".into()]),
        });
        cx.spawn(async move |dialog, cx| {
            if let Ok(Ok(Some(paths))) = picker.await {
                let _ = dialog.update(cx, |dialog, cx| dialog.add_paths(paths, cx));
            }
        })
        .detach();
    }

    fn render_row(
        &self,
        index: usize,
        item: &MergeItem,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = item.key;
        let pages = match item.pages {
            Some(1) => "1 page".to_owned(),
            Some(pages) => format!("{pages} pages"),
            None => String::new(),
        };
        let note = item.blocked.clone().or_else(|| {
            (item.unsaved && item.checked).then(|| "Unsaved changes: saved first".into())
        });
        let row_group = SharedString::from(format!("document-merge-row-group-{index}"));
        let check_id = SharedString::from(format!("document-merge-check-{index}"));
        let check_selector = check_id.clone();
        h_flex()
            .id(SharedString::from(format!("document-merge-row-{index}")))
            .debug_selector(move || format!("document-merge-row-{index}"))
            .group(row_group.clone())
            .w_full()
            .min_w_0()
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(cx.theme().radius)
            .when(index > 0, |row| row.border_t_1())
            .border_color(cx.theme().border)
            .on_drag(
                MergeRowDrag {
                    key,
                    title: item.title.clone(),
                },
                |drag, _, _, cx| {
                    let label = drag.title.clone();
                    cx.new(|_| pages::PageDragGhost::labelled(label))
                },
            )
            .drag_over::<MergeRowDrag>(|row, _, _, cx| row.bg(cx.theme().muted))
            .on_drop(cx.listener(move |dialog, drag: &MergeRowDrag, _, cx| {
                dialog.move_row(drag.key, key, cx);
            }))
            .child(
                gpui_component::Icon::default()
                    .path(GRIP_ICON)
                    .small()
                    .text_color(cx.theme().muted_foreground)
                    .cursor_grab(),
            )
            .child(
                gpui_component::checkbox::Checkbox::new(check_id)
                    .debug_selector(move || check_selector.to_string())
                    .checked(item.checked)
                    .disabled(item.blocked.is_some())
                    .on_click(cx.listener(move |dialog, checked: &bool, _, cx| {
                        dialog.set_checked(key, *checked, cx);
                    })),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        gpui::div()
                            .truncate()
                            .text_sm()
                            .when(item.blocked.is_some(), |title| {
                                title.text_color(cx.theme().muted_foreground)
                            })
                            .child(item.title.clone()),
                    )
                    .when_some(note, |column, note| {
                        column.child(
                            gpui::div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(note),
                        )
                    }),
            )
            .child(
                gpui::div()
                    .flex_none()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(pages),
            )
            .when(matches!(item.source, MergeSource::File(_)), |row| {
                let label = format!("Remove {}", item.title);
                // Shown on hover or focus, as other remove controls are.
                row.child(accessible_icon_button(
                    Button::new(SharedString::from(format!("document-merge-remove-{index}")))
                        .icon(IconName::Delete)
                        .ghost()
                        .danger()
                        .small()
                        .tooltip(label.clone())
                        .opacity(0.)
                        .group_hover(row_group.clone(), |style| style.opacity(1.))
                        .focus(|style| style.opacity(1.))
                        .on_click(cx.listener(move |dialog, _: &ClickEvent, _, cx| {
                            dialog.remove(key, cx);
                        })),
                    label,
                ))
            })
            .into_any_element()
    }
}

impl Render for MergeDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows = self
            .items
            .clone()
            .iter()
            .enumerate()
            .map(|(index, item)| self.render_row(index, item, cx))
            .collect::<Vec<_>>();
        v_flex()
            .id(DOCUMENT_MERGE_DIALOG_ID)
            .debug_selector(|| DOCUMENT_MERGE_DIALOG_ID.into())
            .w_full()
            .gap_3()
            // The dialog covers the window, so files are dropped on it.
            .on_drop(cx.listener(|dialog, paths: &ExternalPaths, _, cx| {
                dialog.add_paths(paths.paths().to_vec(), cx);
            }))
            .child(
                gpui::div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Tick the documents to merge and drag them into order. The merge opens as a new document."),
            )
            .child(
                v_flex()
                    .id("document-merge-list")
                    .w_full()
                    .max_h(px(320.))
                    .overflow_y_scroll()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .children(rows),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .justify_between()
                    .p_3()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_dashed()
                    .border_color(cx.theme().border)
                    .drag_over::<ExternalPaths>(|zone, _, _, cx| {
                        zone.border_color(cx.theme().primary).bg(cx.theme().muted)
                    })
                    .child(
                        gpui::div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Drop PDFs here to add them"),
                    )
                    .child(
                        Button::new(DOCUMENT_MERGE_ADD_ID)
                            .debug_selector(|| DOCUMENT_MERGE_ADD_ID.into())
                            .outline()
                            .small()
                            .icon(IconName::Plus)
                            .label("Add PDFs…")
                            .on_click(cx.listener(|dialog, _: &ClickEvent, _, cx| {
                                dialog.prompt_for_pdfs(cx);
                            })),
                    ),
            )
    }
}

impl DocumentWorkspace {
    /// Opens the Merge Documents dialog on the next frame.
    pub fn request_merge_dialog(&mut self, cx: &mut Context<Self>) {
        self.merge_dialog_requested = true;
        cx.notify();
    }

    pub(super) fn merge_documents_from_action(
        &mut self,
        _: &MergeDocuments,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_merge_dialog(window, cx);
    }

    /// The open Merge Documents dialog, if any.
    pub fn merge_dialog(&self) -> Option<Entity<MergeDialog>> {
        self.merge_dialog.as_ref().and_then(WeakEntity::upgrade)
    }

    /// Opens the Merge Documents dialog with the open documents ticked, in
    /// tab order.
    pub fn open_merge_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.merge_dialog_requested = false;
        if self.merge_dialog().is_some() {
            return;
        }
        let items = self
            .sessions
            .iter()
            .enumerate()
            .map(|(key, session)| {
                let document_id = session.read(cx).id;
                let session = session.read(cx);
                let blocked = if !matches!(session.status, NativeDocumentStatus::Ready) {
                    Some("Still opening".into())
                } else if session.is_dirty() && self.document_requires_save_as(document_id, cx) {
                    Some("Never saved: save it to a file first".into())
                } else {
                    None
                };
                MergeItem {
                    key: key as u64,
                    source: MergeSource::Open(document_id),
                    title: session.title.clone().into(),
                    pages: Some(session.page_sizes.len()),
                    checked: blocked.is_none(),
                    unsaved: session.is_dirty(),
                    blocked,
                    path: session.path.clone(),
                }
            })
            .collect::<Vec<_>>();
        let next_key = items.len() as u64;
        let workspace = cx.entity().downgrade();
        let dialog = cx.new(|_| MergeDialog { items, next_key });
        self.merge_dialog = Some(dialog.downgrade());
        window.open_dialog(cx, move |surface, _, cx| {
            let state = dialog.read(cx);
            let count = state.chosen().len();
            let label = if state.chosen_unsaved() {
                "Save and merge"
            } else {
                "Merge"
            };
            let merge_dialog = dialog.clone();
            let merge_workspace = workspace.clone();
            surface
                .title("Merge documents")
                .w(px(560.))
                .max_w_full()
                .close_button(false)
                .child(dialog.clone())
                .footer(
                    DialogFooter::new()
                        .child(dialog_close_button(
                            Button::new(DOCUMENT_MERGE_CANCEL_ID)
                                .debug_selector(|| DOCUMENT_MERGE_CANCEL_ID.into())
                                .outline()
                                .label("Cancel"),
                        ))
                        .child(dialog_action_button(
                            Button::new(DOCUMENT_MERGE_CONFIRM_ID)
                                .debug_selector(|| DOCUMENT_MERGE_CONFIRM_ID.into())
                                .primary()
                                .disabled(count < 2)
                                .label(label),
                        )),
                )
                .on_ok(move |_, _, cx| {
                    let sources = merge_dialog.read(cx).chosen();
                    if sources.len() < 2 {
                        return false;
                    }
                    let _ = merge_workspace.update(cx, |workspace, cx| {
                        if let Err(error) = workspace.merge_sources(sources, cx) {
                            workspace.report_merge_error(error, cx);
                        }
                    });
                    true
                })
        });
    }

    fn report_merge_error(&mut self, error: String, cx: &mut Context<Self>) {
        if let Some(document_id) = self.active_document_id {
            self.annotation_statuses.insert(document_id, error);
        }
        cx.notify();
    }

    /// Merges `sources` into a new untitled document, saving open documents
    /// with unsaved changes first.
    pub fn merge_sources(
        &mut self,
        sources: Vec<MergeSource>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if sources.len() < 2 {
            return Err("Choose two or more documents to merge.".into());
        }
        let mut unsaved = Vec::new();
        for source in &sources {
            let MergeSource::Open(document_id) = source else {
                continue;
            };
            let session = self
                .session(*document_id, cx)
                .ok_or("A document to merge was closed.")?
                .read(cx);
            if session.is_dirty() {
                if self.document_requires_save_as(*document_id, cx) {
                    return Err(format!(
                        "Save {} to a file before merging it.",
                        session.title
                    ));
                }
                unsaved.push((*document_id, session.title.clone()));
            }
        }
        if unsaved.is_empty() {
            return self.merge_saved_sources(sources, cx);
        }
        for (document_id, title) in &unsaved {
            self.save_path(*document_id, cx)
                .map_err(|error| format!("{title} could not be saved: {error}"))?;
        }
        let started = Instant::now();
        cx.spawn(async move |workspace, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                let Ok(state) = workspace.update(cx, |workspace, cx| {
                    let mut saving = false;
                    for (document_id, title) in &unsaved {
                        let Some(session) = workspace.session(*document_id, cx) else {
                            return Err(format!("{title} was closed before merging."));
                        };
                        let session = session.read(cx);
                        if session.save_status == NativeDocumentSaveStatus::Saving {
                            saving = true;
                        } else if session.is_dirty() {
                            return Err(format!("{title} was not saved, so nothing was merged."));
                        }
                    }
                    Ok(saving)
                }) else {
                    return;
                };
                let result = match state {
                    Ok(true) if started.elapsed() < MERGE_SAVE_TIMEOUT => continue,
                    Ok(true) => Err("Saving took too long, so nothing was merged.".to_owned()),
                    Ok(false) => Ok(()),
                    Err(error) => Err(error),
                };
                let _ = workspace.update(cx, |workspace, cx| {
                    if let Err(error) =
                        result.and_then(|()| workspace.merge_saved_sources(sources, cx))
                    {
                        workspace.report_merge_error(error, cx);
                    }
                });
                return;
            }
        })
        .detach();
        Ok(())
    }

    /// Merges the saved files of `sources` and opens the result as a new
    /// untitled tab.
    fn merge_saved_sources(
        &mut self,
        sources: Vec<MergeSource>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let store = self
            .generated_document_store
            .clone()
            .ok_or("New documents are unavailable, so documents cannot be merged.")?;
        let paths = sources
            .iter()
            .map(|source| match source {
                MergeSource::Open(document_id) => self
                    .session(*document_id, cx)
                    .map(|session| session.read(cx).path.clone())
                    .ok_or_else(|| "A document to merge was closed.".to_owned()),
                MergeSource::File(path) => Ok(path.clone()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let key = format!(
            "merged-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.as_nanos())
        );
        let count = paths.len();
        if let Some(document_id) = self.active_document_id {
            self.annotation_statuses
                .insert(document_id, format!("Merging {count} documents…"));
        }
        cx.notify();
        let task = cx.background_executor().spawn({
            let store = store.clone();
            async move {
                let files = paths
                    .iter()
                    .map(|path| {
                        std::fs::read(path).map_err(|error| {
                            format!("{} could not be read: {error}", path.display())
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let merged = crate::pdf_merge::merge_pdfs(&files)?;
                store
                    .create_from_pdf_bytes(&key, &merged)
                    .map_err(|error| error.to_string())
            }
        });
        cx.spawn(async move |workspace, cx| {
            let result = task.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let status_owner = workspace.active_document_id;
                match result
                    .and_then(|source| workspace.create_owned_template_document(store, source, cx))
                {
                    Ok(_) => {
                        if let Some(document_id) = status_owner {
                            workspace.annotation_statuses.remove(&document_id);
                        }
                    }
                    Err(error) => workspace.report_merge_error(
                        format!("The documents could not be merged: {error}"),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
        Ok(())
    }
}
