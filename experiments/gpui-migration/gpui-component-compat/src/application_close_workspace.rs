use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::{
    AnyView, AnyWindowHandle, App, AppContext as _, Context, Entity, ExternalPaths,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled, Subscription, Window,
    div, prelude::FluentBuilder as _,
};
use gpui_component::{
    Disableable as _, Root, WindowExt as _,
    alert::Alert,
    button::{Button, ButtonVariants as _},
    dialog::{DialogDescription, DialogFooter, DialogHeader, DialogTitle},
    h_flex, v_flex,
};

use crate::{
    application_close::{
        APPLICATION_CLOSE_DESCRIPTION_ID, APPLICATION_CLOSE_DIALOG_ID, APPLICATION_CLOSE_TITLE_ID,
        ApplicationCloseAction, ApplicationCloseActionRole, ApplicationCloseCommand,
        ApplicationCloseCompletionKind, ApplicationCloseContractError, ApplicationCloseCoordinator,
        ApplicationCloseDialog, ApplicationCloseDocument, ApplicationCloseInterruptionReason,
        ApplicationCloseResult, ApplicationCloseSnapshot, ApplicationCloseToken,
        ApplicationCloseTransition, ApplicationCloseTransitionStatus, SaveDestination,
    },
    document_workspace::{
        ApplyDisposition, DocumentId, DocumentOpenBatchRequest, DocumentOpenOrigin,
        DocumentWorkspace, NativeDocumentSaver, SaveDocumentRequest, SavedNativeDocument,
    },
    session_manifest::{SessionManifestError, SessionManifestStore, SessionSnapshot},
};

gpui::actions!(application_close_shell, [RequestApplicationClose]);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingApplicationSaveAs {
    pub token: ApplicationCloseToken,
    pub document_id: String,
    pub suggested_name: String,
}

#[derive(Debug)]
pub struct PendingApplicationSave {
    pub token: ApplicationCloseToken,
    pub document_id: DocumentId,
    pub target_path: PathBuf,
    request: SaveDocumentRequest,
}

pub const APPLICATION_CLOSE_RECOVERY_ID: &str = "application-close-recovery";
pub const APPLICATION_CLOSE_RECOVERY_ALERT_ID: &str = "application-close-recovery-alert";
pub const APPLICATION_CLOSE_RECOVERY_PRIMARY_ID: &str = "application-close-recovery-primary";
pub const APPLICATION_CLOSE_RECOVERY_CANCEL_ID: &str = "application-close-recovery-cancel";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCloseRecoveryKind {
    PickerFailed,
    TargetRejected,
    SaveFailed,
    PublishedWithWarning,
    SessionCheckpointFailed,
    ReleaseFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationCloseCheckpointPublication {
    Published {
        document_count: Option<usize>,
    },
    PublishedWithDurabilityWarning {
        document_count: Option<usize>,
        message: String,
    },
}

pub trait ApplicationCloseCheckpointPublisher: Send + Sync {
    fn publish(
        &self,
        snapshot: &SessionSnapshot,
    ) -> Result<ApplicationCloseCheckpointPublication, String>;
}

struct NoStoreCheckpointPublisher;

impl ApplicationCloseCheckpointPublisher for NoStoreCheckpointPublisher {
    fn publish(
        &self,
        _snapshot: &SessionSnapshot,
    ) -> Result<ApplicationCloseCheckpointPublication, String> {
        Ok(ApplicationCloseCheckpointPublication::Published {
            document_count: None,
        })
    }
}

impl ApplicationCloseCheckpointPublisher for SessionManifestStore {
    fn publish(
        &self,
        snapshot: &SessionSnapshot,
    ) -> Result<ApplicationCloseCheckpointPublication, String> {
        match self.replace(snapshot) {
            Ok(()) => Ok(ApplicationCloseCheckpointPublication::Published {
                document_count: self.load().ok().map(|plan| plan.into_parts().0.len()),
            }),
            Err(SessionManifestError::PublishedButDirectorySyncFailed { kind }) => Ok(
                ApplicationCloseCheckpointPublication::PublishedWithDurabilityWarning {
                    document_count: self.load().ok().map(|plan| plan.into_parts().0.len()),
                    message: format!("session checkpoint directory sync failed: {kind}"),
                },
            ),
            Err(error) => Err(format!("session checkpoint publication failed: {error:?}")),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseRecovery {
    pub kind: ApplicationCloseRecoveryKind,
    pub transaction_id: u64,
    pub document_id: String,
    pub document_name: String,
    pub title: String,
    pub message: String,
    pub primary_label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationCloseEffect {
    SaveAsRequested {
        token: ApplicationCloseToken,
        document_id: String,
        suggested_name: String,
    },
    SaveAsCancelled {
        transaction_id: u64,
        document_id: String,
    },
    SaveStarted {
        token: ApplicationCloseToken,
        document_id: String,
        target_path: PathBuf,
    },
    SaveApplied {
        token: ApplicationCloseToken,
        document_id: String,
        target_path: PathBuf,
    },
    StaleSaveRejected {
        token: ApplicationCloseToken,
        document_id: String,
    },
    SaveFailureReported {
        transaction_id: u64,
        document_id: String,
        message: String,
    },
    SaveWarningReported {
        transaction_id: u64,
        document_id: String,
        message: String,
    },
    SessionCheckpointPublished {
        transaction_id: u64,
        document_count: usize,
    },
    SessionCheckpointFailed {
        transaction_id: u64,
        message: String,
    },
    SessionCheckpointDurabilityWarning {
        transaction_id: u64,
        message: String,
    },
    ReleaseFailureReported {
        transaction_id: u64,
        document_id: String,
        message: String,
        completed_release_document_ids: Vec<String>,
    },
    ReleaseRequested {
        token: ApplicationCloseToken,
        document_id: String,
        worker_pid: Option<u32>,
    },
    ReleaseAcknowledged {
        token: ApplicationCloseToken,
        document_id: String,
        worker_pid: Option<u32>,
    },
    ReleaseFailed {
        token: ApplicationCloseToken,
        document_id: String,
        message: String,
    },
    CloseCancelled {
        transaction_id: u64,
        reason: ApplicationCloseInterruptionReason,
    },
    QuitRequested {
        transaction_id: u64,
        kind: ApplicationCloseCompletionKind,
    },
}

impl ApplicationCloseEffect {
    pub const fn is_quit_requested(&self) -> bool {
        matches!(self, Self::QuitRequested { .. })
    }
}

/// Experiment-owned adapter between the pure application-close transaction and
/// real native document sessions. It emits a quit intent but never terminates
/// the process. Ordinary opened documents save to their verified source;
/// generated documents alone request a new path.
pub struct ApplicationCloseWorkspace {
    workspace: Entity<DocumentWorkspace>,
    saver: Arc<dyn NativeDocumentSaver>,
    checkpoint_publisher: Arc<dyn ApplicationCloseCheckpointPublisher>,
    coordinator: ApplicationCloseCoordinator,
    close_snapshot: Option<ApplicationCloseSnapshot>,
    document_ids: HashMap<String, DocumentId>,
    pending_save_as: Option<PendingApplicationSaveAs>,
    pending_save_as_prompt: Option<ApplicationCloseToken>,
    pending_save: Option<PendingApplicationSave>,
    selected_save_paths: HashMap<String, PathBuf>,
    recovery: Option<ApplicationCloseRecovery>,
    next_save_failure_kind: Option<ApplicationCloseRecoveryKind>,
    effects: Vec<ApplicationCloseEffect>,
}

impl ApplicationCloseWorkspace {
    pub fn new(workspace: Entity<DocumentWorkspace>, saver: Arc<dyn NativeDocumentSaver>) -> Self {
        Self::with_checkpoint_publisher(workspace, saver, Arc::new(NoStoreCheckpointPublisher))
    }

    pub fn with_session_manifest_store(
        workspace: Entity<DocumentWorkspace>,
        saver: Arc<dyn NativeDocumentSaver>,
        store: SessionManifestStore,
    ) -> Self {
        Self::with_checkpoint_publisher(workspace, saver, Arc::new(store))
    }

    pub fn with_checkpoint_publisher(
        workspace: Entity<DocumentWorkspace>,
        saver: Arc<dyn NativeDocumentSaver>,
        checkpoint_publisher: Arc<dyn ApplicationCloseCheckpointPublisher>,
    ) -> Self {
        Self {
            workspace,
            saver,
            checkpoint_publisher,
            coordinator: ApplicationCloseCoordinator::default(),
            close_snapshot: None,
            document_ids: HashMap::new(),
            pending_save_as: None,
            pending_save_as_prompt: None,
            pending_save: None,
            selected_save_paths: HashMap::new(),
            recovery: None,
            next_save_failure_kind: None,
            effects: Vec::new(),
        }
    }

    pub fn workspace(&self) -> &Entity<DocumentWorkspace> {
        &self.workspace
    }

    pub fn dialog(&self) -> Option<ApplicationCloseDialog> {
        self.coordinator.dialog()
    }

    pub fn close_snapshot(&self) -> Option<&ApplicationCloseSnapshot> {
        self.close_snapshot.as_ref()
    }

    pub fn pending_save_as(&self) -> Option<&PendingApplicationSaveAs> {
        self.pending_save_as.as_ref()
    }

    pub fn pending_save(&self) -> Option<&PendingApplicationSave> {
        self.pending_save.as_ref()
    }

    pub fn recovery(&self) -> Option<&ApplicationCloseRecovery> {
        self.recovery.as_ref()
    }

    pub fn dismiss_recovery(&mut self, cx: &mut Context<Self>) -> bool {
        let dismissed = self.recovery.take().is_some();
        if dismissed {
            cx.notify();
        }
        dismissed
    }

    pub fn resume_recovery(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<ApplicationCloseTransitionStatus, ApplicationCloseContractError> {
        let Some(recovery) = self.recovery.take() else {
            return Ok(ApplicationCloseTransitionStatus::StaleResultRejected);
        };
        if recovery.kind == ApplicationCloseRecoveryKind::SessionCheckpointFailed {
            let transition = self
                .coordinator
                .retry_session_checkpoint(recovery.transaction_id);
            let status = self.apply_transition(transition, cx);
            cx.notify();
            return Ok(status);
        }
        let status = self.request_close(cx)?;
        if recovery.kind != ApplicationCloseRecoveryKind::ReleaseFailed && self.dialog().is_some() {
            self.choose(ApplicationCloseAction::SaveAll, cx);
            self.dispatch_pending_save(cx);
            self.prompt_for_pending_save_as(None, cx);
        }
        cx.notify();
        Ok(status)
    }

    pub fn effects(&self) -> &[ApplicationCloseEffect] {
        &self.effects
    }

    pub fn has_quit_intent(&self) -> bool {
        self.effects
            .iter()
            .any(ApplicationCloseEffect::is_quit_requested)
    }

    pub fn take_effects(&mut self) -> Vec<ApplicationCloseEffect> {
        std::mem::take(&mut self.effects)
    }

    pub fn request_close(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<ApplicationCloseTransitionStatus, ApplicationCloseContractError> {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.commit_pending_text_editor_before_application_close(cx)
            })
            .map_err(ApplicationCloseContractError::DraftCommitBlocked)?;
        let (snapshot, document_ids) = self.capture_snapshot(cx);
        let transition = self.coordinator.request_close(snapshot.clone())?;
        if transition.status == ApplicationCloseTransitionStatus::Applied {
            self.recovery = None;
            self.next_save_failure_kind = None;
            self.close_snapshot = Some(snapshot);
            self.document_ids = document_ids;
        }
        let status = self.apply_transition(transition, cx);
        cx.notify();
        Ok(status)
    }

    pub fn choose(
        &mut self,
        action: ApplicationCloseAction,
        cx: &mut Context<Self>,
    ) -> ApplicationCloseTransitionStatus {
        let transition = self.coordinator.choose(action);
        let status = self.apply_transition(transition, cx);
        cx.notify();
        status
    }

    /// User-facing application-close action seam. Save work is dispatched to
    /// the background executor; Cancel and Discard remain immediate.
    pub fn choose_and_dispatch(
        &mut self,
        action: ApplicationCloseAction,
        cx: &mut Context<Self>,
    ) -> ApplicationCloseTransitionStatus {
        let status = self.choose(action, cx);
        self.dispatch_pending_save(cx);
        self.prompt_for_pending_save_as(None, cx);
        status
    }

    pub fn resolve_save_as(
        &mut self,
        token: ApplicationCloseToken,
        target_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> ApplicationCloseTransitionStatus {
        let matches_pending = self
            .pending_save_as
            .as_ref()
            .is_some_and(|pending| pending.token == token);
        if matches_pending {
            self.pending_save_as = None;
            if self.pending_save_as_prompt.as_ref() == Some(&token) {
                self.pending_save_as_prompt = None;
            }
        }
        let result = match target_path {
            Some(target_path) => {
                let target = selected_path_key(&token);
                if matches_pending {
                    self.selected_save_paths.insert(target.clone(), target_path);
                }
                ApplicationCloseResult::SaveAsSelected { token, target }
            }
            None => {
                if matches_pending {
                    self.effects.push(ApplicationCloseEffect::SaveAsCancelled {
                        transaction_id: token.transaction_id,
                        document_id: token.document_id.clone(),
                    });
                }
                ApplicationCloseResult::SaveAsCancelled { token }
            }
        };
        let transition = self.coordinator.handle_result(result);
        let status = self.apply_transition(transition, cx);
        cx.notify();
        status
    }

    pub fn resolve_save_as_in_window(
        owner: &Entity<Self>,
        token: ApplicationCloseToken,
        target_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut App,
    ) -> ApplicationCloseTransitionStatus {
        let window_handle = window.window_handle();
        let status = owner.update(cx, |owner, cx| {
            let status = owner.resolve_save_as(token, target_path, cx);
            owner.dispatch_pending_save_with_window(Some(window_handle), cx);
            owner.prompt_for_pending_save_as(Some(window_handle), cx);
            status
        });
        Self::reconcile_dialog_window(owner, window, cx);
        status
    }

    /// Deterministic evidence seam. Product callbacks use
    /// `dispatch_pending_save` so PDF writing never blocks the GPUI thread.
    pub fn drive_pending_save(&mut self, cx: &mut Context<Self>) -> Option<ApplyDisposition> {
        let pending = self.pending_save.take()?;
        let save_result = self.saver.save(&pending.request);
        Some(self.apply_pending_save_result(pending, save_result, cx))
    }

    pub fn dispatch_pending_save(&mut self, cx: &mut Context<Self>) -> bool {
        self.dispatch_pending_save_with_window(None, cx)
    }

    fn dispatch_pending_save_with_window(
        &mut self,
        window_handle: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pending) = self.pending_save.take() else {
            return false;
        };
        let saver = self.saver.clone();
        let task = cx.background_executor().spawn(async move {
            let result = saver.save(&pending.request);
            (pending, result)
        });
        cx.spawn(async move |entity, cx| {
            let (pending, result) = task.await;
            let dialog_is_open = entity.update(cx, |owner, cx| {
                owner.apply_pending_save_result(pending, result, cx);
                owner.dispatch_pending_save_with_window(window_handle, cx);
                owner.prompt_for_pending_save_as(window_handle, cx);
                owner.dialog().is_some()
            });
            if let (Some(window_handle), Ok(dialog_is_open)) = (window_handle, dialog_is_open) {
                let _ = cx.update_window(window_handle, |_, window, cx| {
                    if !dialog_is_open && window.has_active_dialog(cx) {
                        window.close_dialog(cx);
                    } else {
                        window.refresh();
                    }
                });
            }
        })
        .detach();
        true
    }

    fn choose_and_dispatch_in_window(
        owner: &Entity<Self>,
        action: ApplicationCloseAction,
        window: &mut Window,
        cx: &mut App,
    ) -> ApplicationCloseTransitionStatus {
        let window_handle = window.window_handle();
        let status = owner.update(cx, |owner, cx| {
            let status = owner.choose(action, cx);
            owner.dispatch_pending_save_with_window(Some(window_handle), cx);
            owner.prompt_for_pending_save_as(Some(window_handle), cx);
            status
        });
        Self::reconcile_dialog_window(owner, window, cx);
        status
    }

    fn prompt_for_pending_save_as(
        &mut self,
        window_handle: Option<AnyWindowHandle>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pending) = self.pending_save_as.clone() else {
            return false;
        };
        if self.pending_save_as_prompt.as_ref() == Some(&pending.token) {
            return false;
        }
        self.pending_save_as_prompt = Some(pending.token.clone());
        let picker = cx.prompt_for_new_path(Path::new(""), Some(&pending.suggested_name));
        cx.spawn(async move |entity, cx| {
            let selected = match picker.await {
                Ok(Ok(path)) => Ok(path),
                Ok(Err(error)) => Err(format!("Could not open the Save As picker: {error}")),
                Err(error) => Err(format!("The Save As picker closed unexpectedly: {error}")),
            };
            let dialog_is_open = entity.update(cx, |owner, cx| {
                if owner.pending_save_as_prompt.as_ref() != Some(&pending.token) {
                    return owner.dialog().is_some();
                }
                owner.pending_save_as_prompt = None;
                match selected {
                    Ok(Some(target_path)) => {
                        owner.resolve_save_as(pending.token, Some(target_path), cx);
                        owner.dispatch_pending_save_with_window(window_handle, cx);
                        owner.prompt_for_pending_save_as(window_handle, cx);
                    }
                    Ok(None) => {
                        owner.resolve_save_as(pending.token, None, cx);
                    }
                    Err(message) => {
                        owner.fail_save_as(
                            pending.token,
                            message,
                            ApplicationCloseRecoveryKind::PickerFailed,
                            cx,
                        );
                    }
                }
                owner.dialog().is_some()
            });
            if let (Some(window_handle), Ok(dialog_is_open)) = (window_handle, dialog_is_open) {
                let _ = cx.update_window(window_handle, |_, window, cx| {
                    if !dialog_is_open && window.has_active_dialog(cx) {
                        window.close_dialog(cx);
                    } else {
                        window.refresh();
                    }
                });
            }
        })
        .detach();
        true
    }

    fn fail_save_as(
        &mut self,
        token: ApplicationCloseToken,
        message: String,
        kind: ApplicationCloseRecoveryKind,
        cx: &mut Context<Self>,
    ) -> ApplicationCloseTransitionStatus {
        if self
            .pending_save_as
            .as_ref()
            .is_some_and(|pending| pending.token == token)
        {
            self.pending_save_as = None;
        }
        self.pending_save_as_prompt = None;
        self.next_save_failure_kind = Some(kind);
        let transition = self
            .coordinator
            .handle_result(ApplicationCloseResult::SaveAsFailed { token, message });
        let status = self.apply_transition(transition, cx);
        cx.notify();
        status
    }

    fn apply_pending_save_result(
        &mut self,
        pending: PendingApplicationSave,
        save_result: Result<SavedNativeDocument, String>,
        cx: &mut Context<Self>,
    ) -> ApplyDisposition {
        let save_error = save_result.as_ref().err().cloned();
        let publication_warning = save_result
            .as_ref()
            .ok()
            .and_then(|saved| saved.publication_warning())
            .map(str::to_owned);
        let disposition = self.workspace.update(cx, |workspace, cx| {
            workspace.apply_save_result(&pending.request, save_result, cx)
        });

        let current_is_saved = disposition == ApplyDisposition::Applied
            && save_error.is_none()
            && self.workspace.read_with(cx, |workspace, cx| {
                workspace
                    .session(pending.document_id, cx)
                    .is_some_and(|session| {
                        let session = session.read(cx);
                        session.path() == pending.target_path.as_path()
                            && workspace
                                .document_dirty_revision(pending.document_id, cx)
                                .is_none()
                    })
            });

        let result = if current_is_saved {
            self.effects.push(ApplicationCloseEffect::SaveApplied {
                token: pending.token.clone(),
                document_id: pending.token.document_id.clone(),
                target_path: pending.target_path,
            });
            match publication_warning {
                Some(message) => {
                    self.next_save_failure_kind =
                        Some(ApplicationCloseRecoveryKind::PublishedWithWarning);
                    ApplicationCloseResult::SavePublishedWithWarning {
                        token: pending.token,
                        message,
                    }
                }
                None => ApplicationCloseResult::SaveSucceeded {
                    token: pending.token,
                },
            }
        } else {
            self.next_save_failure_kind = Some(ApplicationCloseRecoveryKind::SaveFailed);
            let message = save_error.unwrap_or_else(|| {
                "native Save result was stale or did not preserve the requested revision".to_owned()
            });
            if disposition != ApplyDisposition::Applied {
                self.effects
                    .push(ApplicationCloseEffect::StaleSaveRejected {
                        token: pending.token.clone(),
                        document_id: pending.token.document_id.clone(),
                    });
            }
            ApplicationCloseResult::SaveFailed {
                token: pending.token,
                message,
            }
        };
        let transition = self.coordinator.handle_result(result);
        self.apply_transition(transition, cx);
        cx.notify();
        disposition
    }

    pub fn drive_pending_save_in_window(
        owner: &Entity<Self>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<ApplyDisposition> {
        let disposition = owner.update(cx, |owner, cx| owner.drive_pending_save(cx));
        Self::reconcile_dialog_window(owner, window, cx);
        disposition
    }

    /// Opens the real pinned GPUI Component alert dialog. Its callbacks update
    /// the retained adapter and never call the application quit API.
    pub fn open_dialog(owner: &Entity<Self>, window: &mut Window, cx: &mut App) -> bool {
        if owner.read(cx).dialog().is_none() {
            return false;
        }
        let weak_owner = owner.downgrade();
        let cancel_owner = weak_owner.clone();
        let confirm_owner = weak_owner.clone();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let content_owner = weak_owner.clone();
            let cancel_owner = cancel_owner.clone();
            let confirm_owner = confirm_owner.clone();
            alert
                .close_button(false)
                .on_cancel(move |_, _, cx| {
                    cancel_owner
                        .update(cx, |owner, cx| {
                            if owner.dialog().is_some_and(|dialog| dialog.busy) {
                                return false;
                            }
                            owner.choose(ApplicationCloseAction::Cancel, cx);
                            true
                        })
                        .unwrap_or(true)
                })
                .on_ok(move |_, window, cx| {
                    if let Some(owner) = confirm_owner.upgrade() {
                        Self::choose_and_dispatch_in_window(
                            &owner,
                            ApplicationCloseAction::SaveAll,
                            window,
                            cx,
                        );
                    }
                    false
                })
                .content(move |content, _, cx| {
                    let Some(owner) = content_owner.upgrade() else {
                        return content;
                    };
                    let Some(dialog) = owner.read(cx).dialog() else {
                        return content;
                    };
                    let mut footer = DialogFooter::new();
                    for action in dialog.actions {
                        let action_owner = content_owner.clone();
                        let button = Button::new(action.id)
                            .label(action.label)
                            .disabled(action.disabled)
                            .on_click(move |_, window, cx| {
                                let Some(owner) = action_owner.upgrade() else {
                                    window.close_dialog(cx);
                                    return;
                                };
                                Self::choose_and_dispatch_in_window(
                                    &owner,
                                    action.action,
                                    window,
                                    cx,
                                );
                                let dismiss = owner.read(cx).dialog().is_none();
                                if dismiss {
                                    window.close_dialog(cx);
                                } else {
                                    window.refresh();
                                }
                            });
                        footer = footer.child(match action.role {
                            ApplicationCloseActionRole::Outline => button.outline(),
                            ApplicationCloseActionRole::Destructive => button.danger(),
                            ApplicationCloseActionRole::Primary => button.primary(),
                        });
                    }
                    content.child(
                        div()
                            .id(APPLICATION_CLOSE_DIALOG_ID)
                            .child(
                                DialogHeader::new()
                                    .child(DialogTitle::new().child(
                                        div().id(APPLICATION_CLOSE_TITLE_ID).child(dialog.title),
                                    ))
                                    .child(
                                        DialogDescription::new().child(
                                            div()
                                                .id(APPLICATION_CLOSE_DESCRIPTION_ID)
                                                .child(dialog.description),
                                        ),
                                    ),
                            )
                            .child(footer),
                    )
                })
        });
        owner.update(cx, |_, cx| cx.notify());
        window.refresh();
        true
    }

    fn reconcile_dialog_window(owner: &Entity<Self>, window: &mut Window, cx: &mut App) {
        if owner.read(cx).dialog().is_none() {
            if window.has_active_dialog(cx) {
                window.close_dialog(cx);
            }
        } else {
            owner.update(cx, |_, cx| cx.notify());
            window.refresh();
        }
    }

    fn capture_snapshot(
        &self,
        cx: &App,
    ) -> (ApplicationCloseSnapshot, HashMap<String, DocumentId>) {
        let workspace = self.workspace.read(cx);
        let mut document_ids = HashMap::new();
        let documents = workspace
            .sessions()
            .iter()
            .map(|session| {
                let session = session.read(cx);
                let document_id = session.id();
                let stable_id = document_id.to_string();
                document_ids.insert(stable_id.clone(), document_id);
                let dirty_revision = workspace.document_dirty_revision(document_id, cx);
                ApplicationCloseDocument {
                    id: stable_id,
                    name: session
                        .path()
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Untitled PDF")
                        .to_owned(),
                    dirty_revision,
                    requires_save_as: workspace.document_requires_save_as(document_id, cx),
                }
            })
            .collect();
        (
            ApplicationCloseSnapshot {
                documents,
                active_document_id: workspace
                    .active_document_id()
                    .map(|document_id| document_id.to_string()),
            },
            document_ids,
        )
    }

    fn apply_transition(
        &mut self,
        transition: ApplicationCloseTransition,
        cx: &mut Context<Self>,
    ) -> ApplicationCloseTransitionStatus {
        let status = transition.status;
        let mut commands = VecDeque::from(transition.commands);
        while let Some(command) = commands.pop_front() {
            match command {
                ApplicationCloseCommand::RequestSaveAs {
                    token,
                    document_id,
                    suggested_name,
                } => {
                    self.effects.push(ApplicationCloseEffect::SaveAsRequested {
                        token: token.clone(),
                        document_id: document_id.clone(),
                        suggested_name: suggested_name.clone(),
                    });
                    self.pending_save_as = Some(PendingApplicationSaveAs {
                        token,
                        document_id,
                        suggested_name,
                    });
                }
                ApplicationCloseCommand::SaveDocument {
                    token,
                    document_id,
                    destination,
                } => self.prepare_save(token, document_id, destination, &mut commands, cx),
                ApplicationCloseCommand::PublishSessionCheckpoint {
                    transaction_id,
                    completion_kind: _,
                } => {
                    let snapshot = self
                        .workspace
                        .read_with(cx, |workspace, cx| workspace.session_snapshot(cx));
                    let fallback_document_count = self
                        .close_snapshot
                        .as_ref()
                        .map_or(0, |snapshot| snapshot.documents.len());
                    match self.checkpoint_publisher.publish(&snapshot) {
                        Ok(ApplicationCloseCheckpointPublication::Published { document_count }) => {
                            self.effects
                                .push(ApplicationCloseEffect::SessionCheckpointPublished {
                                    transaction_id,
                                    document_count: document_count
                                        .unwrap_or(fallback_document_count),
                                });
                            let transition = self.coordinator.handle_result(
                                ApplicationCloseResult::SessionCheckpointPublished {
                                    transaction_id,
                                },
                            );
                            commands.extend(transition.commands);
                        }
                        Ok(
                            ApplicationCloseCheckpointPublication::PublishedWithDurabilityWarning {
                                document_count,
                                message,
                            },
                        ) => {
                            self.effects.push(
                                ApplicationCloseEffect::SessionCheckpointDurabilityWarning {
                                    transaction_id,
                                    message,
                                },
                            );
                            self.effects
                                .push(ApplicationCloseEffect::SessionCheckpointPublished {
                                    transaction_id,
                                    document_count: document_count
                                        .unwrap_or(fallback_document_count),
                                });
                            let transition = self.coordinator.handle_result(
                                ApplicationCloseResult::SessionCheckpointPublished {
                                    transaction_id,
                                },
                            );
                            commands.extend(transition.commands);
                        }
                        Err(message) => {
                            let transition = self.coordinator.handle_result(
                                ApplicationCloseResult::SessionCheckpointFailed {
                                    transaction_id,
                                    message,
                                },
                            );
                            commands.extend(transition.commands);
                        }
                    }
                }
                ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
                    self.release_document(token, document_id, &mut commands, cx)
                }
                ApplicationCloseCommand::CancelApplicationClose { transaction_id } => {
                    self.pending_save_as = None;
                    self.pending_save_as_prompt = None;
                    self.pending_save = None;
                    self.selected_save_paths.clear();
                    let reason = self
                        .coordinator
                        .last_interruption()
                        .map(|interruption| interruption.reason)
                        .unwrap_or(ApplicationCloseInterruptionReason::Cancelled);
                    self.effects.push(ApplicationCloseEffect::CloseCancelled {
                        transaction_id,
                        reason,
                    });
                }
                ApplicationCloseCommand::ConfirmApplicationClose { transaction_id } => {
                    self.pending_save_as = None;
                    self.pending_save_as_prompt = None;
                    self.pending_save = None;
                    self.selected_save_paths.clear();
                    let kind = self
                        .coordinator
                        .last_completion()
                        .map(|completion| completion.kind)
                        .unwrap_or(ApplicationCloseCompletionKind::NoDirtyDocuments);
                    self.effects.push(ApplicationCloseEffect::QuitRequested {
                        transaction_id,
                        kind,
                    });
                }
                ApplicationCloseCommand::ReportSaveFailure {
                    transaction_id,
                    document_id,
                    message,
                } => {
                    let kind = self
                        .next_save_failure_kind
                        .take()
                        .unwrap_or(ApplicationCloseRecoveryKind::SaveFailed);
                    self.set_recovery(kind, transaction_id, &document_id, message.clone());
                    self.effects
                        .push(ApplicationCloseEffect::SaveFailureReported {
                            transaction_id,
                            document_id,
                            message,
                        });
                }
                ApplicationCloseCommand::ReportSaveWarning {
                    transaction_id,
                    document_id,
                    message,
                } => {
                    self.next_save_failure_kind = None;
                    self.set_recovery(
                        ApplicationCloseRecoveryKind::PublishedWithWarning,
                        transaction_id,
                        &document_id,
                        message.clone(),
                    );
                    self.effects
                        .push(ApplicationCloseEffect::SaveWarningReported {
                            transaction_id,
                            document_id,
                            message,
                        });
                }
                ApplicationCloseCommand::ReportSessionCheckpointFailure {
                    transaction_id,
                    message,
                } => {
                    self.set_recovery(
                        ApplicationCloseRecoveryKind::SessionCheckpointFailed,
                        transaction_id,
                        "session-checkpoint",
                        message.clone(),
                    );
                    self.effects
                        .push(ApplicationCloseEffect::SessionCheckpointFailed {
                            transaction_id,
                            message,
                        });
                }
                ApplicationCloseCommand::ReportReleaseFailure {
                    transaction_id,
                    document_id,
                    message,
                    completed_release_document_ids,
                } => {
                    self.set_recovery(
                        ApplicationCloseRecoveryKind::ReleaseFailed,
                        transaction_id,
                        &document_id,
                        message.clone(),
                    );
                    self.effects
                        .push(ApplicationCloseEffect::ReleaseFailureReported {
                            transaction_id,
                            document_id,
                            message,
                            completed_release_document_ids,
                        });
                }
            }
        }
        status
    }

    fn set_recovery(
        &mut self,
        kind: ApplicationCloseRecoveryKind,
        transaction_id: u64,
        document_id: &str,
        detail: String,
    ) {
        let document_name = self
            .close_snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .documents
                    .iter()
                    .find(|document| document.id == document_id)
            })
            .map(|document| document.name.clone())
            .unwrap_or_else(|| "this PDF".to_owned());
        let (title, message, primary_label) = match kind {
            ApplicationCloseRecoveryKind::PickerFailed => (
                format!("Couldn’t choose a location for “{document_name}”"),
                format!("The application will stay open. {detail}"),
                "Try again",
            ),
            ApplicationCloseRecoveryKind::TargetRejected => (
                format!("Choose another location for “{document_name}”"),
                format!("The selected destination was not changed. {detail}"),
                "Choose another location",
            ),
            ApplicationCloseRecoveryKind::SaveFailed => (
                format!("Couldn’t save “{document_name}”"),
                format!("The application will stay open. {detail}"),
                "Try again",
            ),
            ApplicationCloseRecoveryKind::PublishedWithWarning => (
                format!("“{document_name}” was saved with a warning"),
                format!(
                    "The file will not be written again. Closing stopped because durability could not be confirmed. {detail}"
                ),
                "Continue closing",
            ),
            ApplicationCloseRecoveryKind::SessionCheckpointFailed => (
                "Couldn’t save the workspace session".to_owned(),
                format!("The application will stay open. {detail}"),
                "Try again",
            ),
            ApplicationCloseRecoveryKind::ReleaseFailed => (
                format!("Couldn’t close “{document_name}”"),
                format!("The document remains open. {detail}"),
                "Try closing again",
            ),
        };
        self.recovery = Some(ApplicationCloseRecovery {
            kind,
            transaction_id,
            document_id: document_id.to_owned(),
            document_name,
            title,
            message,
            primary_label: primary_label.to_owned(),
        });
    }

    fn prepare_save(
        &mut self,
        token: ApplicationCloseToken,
        document_id: String,
        destination: SaveDestination,
        commands: &mut VecDeque<ApplicationCloseCommand>,
        cx: &mut Context<Self>,
    ) {
        let Some(native_id) = self.document_ids.get(&document_id).copied() else {
            self.next_save_failure_kind = Some(ApplicationCloseRecoveryKind::SaveFailed);
            self.feed_save_failure(token, "native document session is closed".into(), commands);
            return;
        };
        let revision_matches = self.workspace.read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(native_id, cx) == Some(token.dirty_revision)
        });
        if !revision_matches {
            self.next_save_failure_kind = Some(ApplicationCloseRecoveryKind::SaveFailed);
            self.effects
                .push(ApplicationCloseEffect::StaleSaveRejected {
                    token: token.clone(),
                    document_id: document_id.clone(),
                });
            self.feed_save_failure(
                token,
                "document changed after the application-close snapshot".into(),
                commands,
            );
            return;
        }
        let selected_destination = matches!(destination, SaveDestination::Selected(_));
        let request = match destination {
            SaveDestination::Selected(target) => {
                let Some(target_path) = self.selected_save_paths.remove(&target) else {
                    self.next_save_failure_kind =
                        Some(ApplicationCloseRecoveryKind::TargetRejected);
                    self.feed_save_failure(
                        token,
                        "selected Save As path was stale or unavailable".into(),
                        commands,
                    );
                    return;
                };
                self.workspace.update(cx, |workspace, cx| {
                    workspace.begin_save_as(native_id, target_path, cx)
                })
            }
            SaveDestination::Existing => self
                .workspace
                .update(cx, |workspace, cx| workspace.begin_save(native_id, cx)),
        };
        match request {
            Ok(request) => {
                let target_path = request.target_path().to_path_buf();
                self.effects.push(ApplicationCloseEffect::SaveStarted {
                    token: token.clone(),
                    document_id,
                    target_path: target_path.clone(),
                });
                self.pending_save = Some(PendingApplicationSave {
                    token,
                    document_id: native_id,
                    target_path,
                    request,
                });
            }
            Err(message) => {
                self.next_save_failure_kind = Some(if selected_destination {
                    ApplicationCloseRecoveryKind::TargetRejected
                } else {
                    ApplicationCloseRecoveryKind::SaveFailed
                });
                self.feed_save_failure(token, message, commands);
            }
        }
    }

    fn feed_save_failure(
        &mut self,
        token: ApplicationCloseToken,
        message: String,
        commands: &mut VecDeque<ApplicationCloseCommand>,
    ) {
        let transition = self
            .coordinator
            .handle_result(ApplicationCloseResult::SaveFailed { token, message });
        commands.extend(transition.commands);
    }

    fn release_document(
        &mut self,
        token: ApplicationCloseToken,
        document_id: String,
        commands: &mut VecDeque<ApplicationCloseCommand>,
        cx: &mut Context<Self>,
    ) {
        let Some(native_id) = self.document_ids.get(&document_id).copied() else {
            let message = "application-close document identity is no longer live".to_owned();
            self.effects.push(ApplicationCloseEffect::ReleaseFailed {
                token: token.clone(),
                document_id: document_id.clone(),
                message: message.clone(),
            });
            commands.clear();
            let transition = self
                .coordinator
                .handle_result(ApplicationCloseResult::DocumentReleaseFailed { token, message });
            commands.extend(transition.commands);
            return;
        };
        let worker_pid = self.workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(native_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        });
        self.effects.push(ApplicationCloseEffect::ReleaseRequested {
            token: token.clone(),
            document_id: document_id.clone(),
            worker_pid,
        });
        let released = self.workspace.update(cx, |workspace, cx| {
            workspace
                .close_document_checked(native_id, cx)
                .and_then(|closed| {
                    (closed && workspace.session(native_id, cx).is_none())
                        .then_some(())
                        .ok_or_else(|| "native document release did not remove its session".into())
                })
        });
        if let Err(message) = released {
            self.effects.push(ApplicationCloseEffect::ReleaseFailed {
                token: token.clone(),
                document_id: document_id.clone(),
                message: message.clone(),
            });
            commands.clear();
            let transition = self
                .coordinator
                .handle_result(ApplicationCloseResult::DocumentReleaseFailed { token, message });
            commands.extend(transition.commands);
            return;
        }
        self.effects
            .push(ApplicationCloseEffect::ReleaseAcknowledged {
                token: token.clone(),
                document_id,
                worker_pid,
            });
        let transition = self
            .coordinator
            .handle_result(ApplicationCloseResult::DocumentReleased { token });
        commands.extend(transition.commands);
    }
}

impl Render for ApplicationCloseWorkspace {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.workspace.clone())
    }
}

/// Top-level window seam that owns GPUI Component modal layers exactly once.
pub struct ApplicationCloseShell {
    workspace: Entity<ApplicationCloseWorkspace>,
    content: Option<AnyView>,
    _native_close_subscription: Option<Subscription>,
}

impl ApplicationCloseShell {
    pub fn new(workspace: Entity<ApplicationCloseWorkspace>) -> Self {
        Self {
            workspace,
            content: None,
            _native_close_subscription: None,
        }
    }

    /// Installs the real platform-window close boundary. Every platform close
    /// request is first converted into the application-owned transaction. The
    /// window is removed only after that transaction emits its typed quit
    /// intent and all document resources have been acknowledged as released.
    pub fn new_for_native_window(
        workspace: Entity<ApplicationCloseWorkspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_for_native_window_with_content(workspace, None, window, cx)
    }

    pub fn new_for_native_window_with_content(
        workspace: Entity<ApplicationCloseWorkspace>,
        content: impl Into<Option<AnyView>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let close_owner = workspace.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let Some(close_owner) = close_owner.upgrade() else {
                return true;
            };
            let _ = close_owner.update(cx, |owner, cx| owner.request_close(cx));
            if close_owner.read(cx).dialog().is_some() && !window.has_active_dialog(cx) {
                ApplicationCloseWorkspace::open_dialog(&close_owner, window, cx);
            }
            false
        });
        let native_close_subscription =
            cx.observe_in(&workspace, window, |_, workspace, window, cx| {
                if workspace.read(cx).dialog().is_some() && !window.has_active_dialog(cx) {
                    ApplicationCloseWorkspace::open_dialog(&workspace, window, cx);
                }
                if workspace.read(cx).has_quit_intent() {
                    window.remove_window();
                }
            });
        Self {
            workspace,
            content: content.into(),
            _native_close_subscription: Some(native_close_subscription),
        }
    }

    pub fn workspace(&self) -> &Entity<ApplicationCloseWorkspace> {
        &self.workspace
    }

    fn request_application_close_from_action(
        &mut self,
        _: &RequestApplicationClose,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self
            .workspace
            .update(cx, |workspace, cx| workspace.request_close(cx));
        if self.workspace.read(cx).dialog().is_some() && !window.has_active_dialog(cx) {
            Self::open_workspace_dialog(&self.workspace, window, cx);
        }
    }

    fn open_dropped_documents(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths = paths.paths().iter().cloned().collect::<Vec<_>>();
        if paths.is_empty() {
            return;
        }
        let document_workspace = self.workspace.read(cx).workspace().clone();
        document_workspace.update(cx, |workspace, cx| {
            workspace.open_documents(
                DocumentOpenBatchRequest::new(DocumentOpenOrigin::Drop, paths),
                cx,
            );
        });
        window.activate_window();
    }
}

pub fn register_application_close_action(
    workspace: &Entity<ApplicationCloseWorkspace>,
    cx: &mut App,
) {
    let workspace = workspace.downgrade();
    cx.on_action(move |_: &RequestApplicationClose, cx| {
        let Some(workspace) = workspace.upgrade() else {
            return;
        };
        let _ = workspace.update(cx, |workspace, cx| workspace.request_close(cx));
    });
}

impl Render for ApplicationCloseShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dialog_layer = Root::render_dialog_layer(window, cx);
        let recovery = self.workspace.read(cx).recovery().cloned();
        let primary_owner = self.workspace.downgrade();
        let cancel_owner = self.workspace.downgrade();
        let content = self.content.clone().map_or_else(
            || self.workspace.clone().into_any_element(),
            IntoElement::into_any_element,
        );
        div()
            .relative()
            .size_full()
            .on_action(cx.listener(Self::request_application_close_from_action))
            .on_drop(cx.listener(Self::open_dropped_documents))
            .child(content)
            .when_some(recovery, |root, recovery| {
                let warning = recovery.kind == ApplicationCloseRecoveryKind::PublishedWithWarning;
                root.child(
                    v_flex()
                        .id(APPLICATION_CLOSE_RECOVERY_ID)
                        .debug_selector(|| APPLICATION_CLOSE_RECOVERY_ID.into())
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .gap_2()
                        .p_2()
                        .child(
                            div()
                                .id(APPLICATION_CLOSE_RECOVERY_ALERT_ID)
                                .debug_selector(|| APPLICATION_CLOSE_RECOVERY_ALERT_ID.into())
                                .child(if warning {
                                    Alert::warning(
                                        "application-close-recovery-warning",
                                        recovery.message.clone(),
                                    )
                                    .title(recovery.title.clone())
                                } else {
                                    Alert::error(
                                        "application-close-recovery-error",
                                        recovery.message.clone(),
                                    )
                                    .title(recovery.title.clone())
                                }),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new(APPLICATION_CLOSE_RECOVERY_CANCEL_ID)
                                        .debug_selector(|| {
                                            APPLICATION_CLOSE_RECOVERY_CANCEL_ID.into()
                                        })
                                        .outline()
                                        .label("Keep open")
                                        .on_click(move |_, _, cx| {
                                            let _ = cancel_owner
                                                .update(cx, |owner, cx| owner.dismiss_recovery(cx));
                                        }),
                                )
                                .child(
                                    Button::new(APPLICATION_CLOSE_RECOVERY_PRIMARY_ID)
                                        .debug_selector(|| {
                                            APPLICATION_CLOSE_RECOVERY_PRIMARY_ID.into()
                                        })
                                        .primary()
                                        .label(recovery.primary_label)
                                        .on_click(move |_, window, cx| {
                                            let Some(owner) = primary_owner.upgrade() else {
                                                return;
                                            };
                                            let _ = owner
                                                .update(cx, |owner, cx| owner.resume_recovery(cx));
                                            if owner.read(cx).dialog().is_some()
                                                && !window.has_active_dialog(cx)
                                            {
                                                Self::open_workspace_dialog(&owner, window, cx);
                                            } else {
                                                window.refresh();
                                            }
                                        }),
                                ),
                        ),
                )
            })
            .children(dialog_layer)
    }
}

impl ApplicationCloseShell {
    fn open_workspace_dialog(
        owner: &Entity<ApplicationCloseWorkspace>,
        window: &mut Window,
        cx: &mut App,
    ) {
        ApplicationCloseWorkspace::open_dialog(owner, window, cx);
    }
}

fn selected_path_key(token: &ApplicationCloseToken) -> String {
    format!(
        "application-close-path:{}:{}:{}",
        token.transaction_id, token.request_sequence, token.document_id
    )
}
