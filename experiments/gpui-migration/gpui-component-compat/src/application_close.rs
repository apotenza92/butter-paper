use std::{collections::HashSet, error::Error, fmt};

pub const APPLICATION_CLOSE_DIALOG_ID: &str = "unsaved-changes-dialog";
pub const APPLICATION_CLOSE_TITLE_ID: &str = "application-close-title";
pub const APPLICATION_CLOSE_DESCRIPTION_ID: &str = "application-close-description";
pub const APPLICATION_CLOSE_CANCEL_ID: &str = "application-close-cancel";
pub const APPLICATION_CLOSE_DISCARD_ALL_ID: &str = "unsaved-discard";
pub const APPLICATION_CLOSE_SAVE_ALL_ID: &str = "unsaved-save";
pub const APPLICATION_CLOSE_TITLE: &str = "Save changes before quitting?";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCloseAction {
    Cancel,
    DiscardAll,
    SaveAll,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCloseActionRole {
    Outline,
    Destructive,
    Primary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplicationCloseActionSpec {
    pub id: &'static str,
    pub action: ApplicationCloseAction,
    pub label: &'static str,
    pub role: ApplicationCloseActionRole,
    pub disabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseDialog {
    pub title: &'static str,
    pub description: String,
    pub dirty_document_count: usize,
    pub busy: bool,
    pub actions: [ApplicationCloseActionSpec; 3],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseDocument {
    /// Stable application-owned document identity. A filename is presentation
    /// data and must never be used to correlate asynchronous close results.
    pub id: String,
    pub name: String,
    /// `Some` means dirty. The revision binds an asynchronous save result to
    /// the exact document state for which the request was issued.
    pub dirty_revision: Option<u64>,
    /// Electron opens Save As for a temporary/blank document before saving.
    pub requires_save_as: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseSnapshot {
    /// Documents are in visible tab order. Save All retains this exact order.
    pub documents: Vec<ApplicationCloseDocument>,
    pub active_document_id: Option<String>,
}

impl ApplicationCloseSnapshot {
    fn dirty_documents(&self) -> Vec<ApplicationCloseDocument> {
        self.documents
            .iter()
            .filter(|document| document.dirty_revision.is_some())
            .cloned()
            .collect()
    }

    fn dirty_document_count(&self) -> usize {
        self.documents
            .iter()
            .filter(|document| document.dirty_revision.is_some())
            .count()
    }

    fn validate(&self) -> Result<(), ApplicationCloseContractError> {
        let mut identities = HashSet::new();
        for document in &self.documents {
            if document.id.is_empty() {
                return Err(ApplicationCloseContractError::EmptyDocumentIdentity);
            }
            if !identities.insert(document.id.as_str()) {
                return Err(ApplicationCloseContractError::DuplicateDocumentIdentity(
                    document.id.clone(),
                ));
            }
        }
        if let Some(active_document_id) = &self.active_document_id
            && !identities.contains(active_document_id.as_str())
        {
            return Err(ApplicationCloseContractError::UnknownActiveDocument(
                active_document_id.clone(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationCloseContractError {
    EmptyDocumentIdentity,
    DuplicateDocumentIdentity(String),
    UnknownActiveDocument(String),
}

impl fmt::Display for ApplicationCloseContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyDocumentIdentity => write!(formatter, "document identity must not be empty"),
            Self::DuplicateDocumentIdentity(id) => {
                write!(formatter, "duplicate document identity {id}")
            }
            Self::UnknownActiveDocument(id) => {
                write!(
                    formatter,
                    "active document {id} is not in the close snapshot"
                )
            }
        }
    }
}

impl Error for ApplicationCloseContractError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCloseRequestKind {
    SaveAs,
    Save,
    Release,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseToken {
    pub transaction_id: u64,
    pub request_sequence: u64,
    pub document_id: String,
    pub dirty_revision: u64,
    pub kind: ApplicationCloseRequestKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SaveDestination {
    Existing,
    /// An opaque adapter-owned target. The state machine does not inspect a
    /// path and never performs persistence itself.
    Selected(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationCloseCommand {
    RequestSaveAs {
        token: ApplicationCloseToken,
        document_id: String,
        suggested_name: String,
    },
    SaveDocument {
        token: ApplicationCloseToken,
        document_id: String,
        destination: SaveDestination,
    },
    ReleaseDocument {
        token: ApplicationCloseToken,
        document_id: String,
    },
    CancelApplicationClose {
        transaction_id: u64,
    },
    ConfirmApplicationClose {
        transaction_id: u64,
    },
    ReportSaveFailure {
        transaction_id: u64,
        document_id: String,
        message: String,
    },
    ReportSaveWarning {
        transaction_id: u64,
        document_id: String,
        message: String,
    },
    ReportReleaseFailure {
        transaction_id: u64,
        document_id: String,
        message: String,
        completed_release_document_ids: Vec<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ApplicationCloseResult {
    SaveAsSelected {
        token: ApplicationCloseToken,
        target: String,
    },
    SaveAsCancelled {
        token: ApplicationCloseToken,
    },
    SaveAsFailed {
        token: ApplicationCloseToken,
        message: String,
    },
    SaveSucceeded {
        token: ApplicationCloseToken,
    },
    SaveFailed {
        token: ApplicationCloseToken,
        message: String,
    },
    SavePublishedWithWarning {
        token: ApplicationCloseToken,
        message: String,
    },
    DocumentReleased {
        token: ApplicationCloseToken,
    },
    DocumentReleaseFailed {
        token: ApplicationCloseToken,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCloseTransitionStatus {
    Applied,
    IgnoredPending,
    IgnoredBusy,
    StaleResultRejected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseTransition {
    pub status: ApplicationCloseTransitionStatus,
    pub commands: Vec<ApplicationCloseCommand>,
}

impl ApplicationCloseTransition {
    fn applied(commands: Vec<ApplicationCloseCommand>) -> Self {
        Self {
            status: ApplicationCloseTransitionStatus::Applied,
            commands,
        }
    }

    fn status(status: ApplicationCloseTransitionStatus) -> Self {
        Self {
            status,
            commands: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCloseInterruptionReason {
    Cancelled,
    SaveAsCancelled,
    SaveFailed,
    SavePublishedWithWarning,
    ReleaseFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseInterruption {
    pub transaction_id: u64,
    pub reason: ApplicationCloseInterruptionReason,
    pub snapshot: ApplicationCloseSnapshot,
    pub completed_save_document_ids: Vec<String>,
    pub completed_release_document_ids: Vec<String>,
    pub target_document_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplicationCloseCompletionKind {
    NoDirtyDocuments,
    SavedAll,
    DiscardedAll,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationCloseCompletion {
    pub transaction_id: u64,
    pub kind: ApplicationCloseCompletionKind,
    pub document_ids: Vec<String>,
}

#[derive(Clone, Debug)]
struct CloseTransaction {
    id: u64,
    snapshot: ApplicationCloseSnapshot,
}

#[derive(Clone, Debug)]
struct PendingSave {
    token: ApplicationCloseToken,
}

#[derive(Clone, Debug)]
struct SavingState {
    transaction: CloseTransaction,
    dirty_documents: Vec<ApplicationCloseDocument>,
    next_document_index: usize,
    completed_document_ids: Vec<String>,
    pending: PendingSave,
}

#[derive(Clone, Debug)]
struct ReleasingState {
    transaction: CloseTransaction,
    pending_tokens: Vec<ApplicationCloseToken>,
    completed_release_document_ids: Vec<String>,
    completed_save_document_ids: Vec<String>,
    completion_kind: ApplicationCloseCompletionKind,
}

#[derive(Clone, Debug)]
enum CoordinatorState {
    Idle,
    AwaitingDecision(CloseTransaction),
    Saving(SavingState),
    Releasing(ReleasingState),
}

impl Default for CoordinatorState {
    fn default() -> Self {
        Self::Idle
    }
}

/// Application-owned close transaction state. This module owns ordering,
/// correlation and safe decisions only. Adapters execute its commands and
/// return generation-bound results; the module never writes, disposes, or
/// closes a real document.
#[derive(Debug, Default)]
pub struct ApplicationCloseCoordinator {
    state: CoordinatorState,
    next_transaction_id: u64,
    next_request_sequence: u64,
    last_interruption: Option<ApplicationCloseInterruption>,
    last_completion: Option<ApplicationCloseCompletion>,
}

impl ApplicationCloseCoordinator {
    pub fn request_close(
        &mut self,
        snapshot: ApplicationCloseSnapshot,
    ) -> Result<ApplicationCloseTransition, ApplicationCloseContractError> {
        snapshot.validate()?;
        if !matches!(self.state, CoordinatorState::Idle) {
            return Ok(ApplicationCloseTransition::status(
                ApplicationCloseTransitionStatus::IgnoredPending,
            ));
        }

        self.next_transaction_id = self.next_transaction_id.saturating_add(1);
        let transaction_id = self.next_transaction_id;
        self.last_interruption = None;
        self.last_completion = None;
        let transaction = CloseTransaction {
            id: transaction_id,
            snapshot,
        };
        if transaction.snapshot.dirty_document_count() == 0 {
            return Ok(self.start_release(
                transaction,
                ApplicationCloseCompletionKind::NoDirtyDocuments,
                Vec::new(),
            ));
        }

        self.state = CoordinatorState::AwaitingDecision(transaction);
        Ok(ApplicationCloseTransition::applied(Vec::new()))
    }

    pub fn dialog(&self) -> Option<ApplicationCloseDialog> {
        let (transaction, busy) = match &self.state {
            CoordinatorState::AwaitingDecision(transaction) => (transaction, false),
            CoordinatorState::Saving(state) => (&state.transaction, true),
            CoordinatorState::Releasing(state) => (&state.transaction, true),
            CoordinatorState::Idle => return None,
        };
        let dirty_document_count = transaction.snapshot.dirty_document_count();
        let description = format!(
            "{dirty_document_count} modified {} {} unsaved changes.",
            if dirty_document_count == 1 {
                "PDF"
            } else {
                "PDFs"
            },
            if dirty_document_count == 1 {
                "has"
            } else {
                "have"
            },
        );
        Some(ApplicationCloseDialog {
            title: APPLICATION_CLOSE_TITLE,
            description,
            dirty_document_count,
            busy,
            actions: action_specs(busy),
        })
    }

    pub fn last_interruption(&self) -> Option<&ApplicationCloseInterruption> {
        self.last_interruption.as_ref()
    }

    pub fn last_completion(&self) -> Option<&ApplicationCloseCompletion> {
        self.last_completion.as_ref()
    }

    pub fn choose(&mut self, action: ApplicationCloseAction) -> ApplicationCloseTransition {
        let state = std::mem::take(&mut self.state);
        let transaction = match state {
            CoordinatorState::AwaitingDecision(transaction) => transaction,
            other => {
                let busy = matches!(
                    &other,
                    CoordinatorState::Saving(_) | CoordinatorState::Releasing(_)
                );
                self.state = other;
                return ApplicationCloseTransition::status(if busy {
                    ApplicationCloseTransitionStatus::IgnoredBusy
                } else {
                    ApplicationCloseTransitionStatus::IgnoredPending
                });
            }
        };

        match action {
            ApplicationCloseAction::Cancel => {
                let transaction_id = transaction.id;
                self.last_interruption = Some(ApplicationCloseInterruption {
                    transaction_id,
                    reason: ApplicationCloseInterruptionReason::Cancelled,
                    snapshot: transaction.snapshot,
                    completed_save_document_ids: Vec::new(),
                    completed_release_document_ids: Vec::new(),
                    target_document_id: None,
                    message: None,
                });
                ApplicationCloseTransition::applied(vec![
                    ApplicationCloseCommand::CancelApplicationClose { transaction_id },
                ])
            }
            ApplicationCloseAction::SaveAll => self.start_save_all(transaction),
            ApplicationCloseAction::DiscardAll => self.start_discard_all(transaction),
        }
    }

    pub fn handle_result(&mut self, result: ApplicationCloseResult) -> ApplicationCloseTransition {
        let state = std::mem::take(&mut self.state);
        match state {
            CoordinatorState::Saving(saving) => self.handle_save_result(saving, result),
            CoordinatorState::Releasing(releasing) => self.handle_release_result(releasing, result),
            other => {
                self.state = other;
                ApplicationCloseTransition::status(
                    ApplicationCloseTransitionStatus::StaleResultRejected,
                )
            }
        }
    }

    fn start_save_all(&mut self, transaction: CloseTransaction) -> ApplicationCloseTransition {
        let dirty_documents = transaction.snapshot.dirty_documents();
        let document = dirty_documents
            .first()
            .expect("a dirty close transaction has at least one dirty document")
            .clone();
        let (pending, command) = self.initial_save_request(transaction.id, &document);
        self.state = CoordinatorState::Saving(SavingState {
            transaction,
            dirty_documents,
            next_document_index: 0,
            completed_document_ids: Vec::new(),
            pending,
        });
        ApplicationCloseTransition::applied(vec![command])
    }

    fn start_discard_all(&mut self, transaction: CloseTransaction) -> ApplicationCloseTransition {
        self.start_release(
            transaction,
            ApplicationCloseCompletionKind::DiscardedAll,
            Vec::new(),
        )
    }

    fn start_release(
        &mut self,
        transaction: CloseTransaction,
        completion_kind: ApplicationCloseCompletionKind,
        completed_save_document_ids: Vec<String>,
    ) -> ApplicationCloseTransition {
        let mut pending_tokens = Vec::with_capacity(transaction.snapshot.documents.len());
        for document in &transaction.snapshot.documents {
            let token = self.next_token(
                transaction.id,
                document,
                ApplicationCloseRequestKind::Release,
            );
            pending_tokens.push(token);
        }
        if pending_tokens.is_empty() {
            self.last_completion = Some(ApplicationCloseCompletion {
                transaction_id: transaction.id,
                kind: completion_kind,
                document_ids: completed_save_document_ids,
            });
            return ApplicationCloseTransition::applied(vec![
                ApplicationCloseCommand::ConfirmApplicationClose {
                    transaction_id: transaction.id,
                },
            ]);
        }
        self.state = CoordinatorState::Releasing(ReleasingState {
            transaction,
            pending_tokens,
            completed_release_document_ids: Vec::new(),
            completed_save_document_ids,
            completion_kind,
        });
        let first = self
            .state
            .first_release()
            .expect("a non-empty close transaction has a first release");
        ApplicationCloseTransition::applied(vec![first])
    }

    fn handle_save_result(
        &mut self,
        mut saving: SavingState,
        result: ApplicationCloseResult,
    ) -> ApplicationCloseTransition {
        let expected = saving.pending.token.clone();
        let result_token = result.token();
        if result_token != &expected {
            self.state = CoordinatorState::Saving(saving);
            return ApplicationCloseTransition::status(
                ApplicationCloseTransitionStatus::StaleResultRejected,
            );
        }

        match result {
            ApplicationCloseResult::SaveAsSelected { token, target }
                if token.kind == ApplicationCloseRequestKind::SaveAs =>
            {
                let document = saving.dirty_documents[saving.next_document_index].clone();
                let save_token = self.next_token(
                    saving.transaction.id,
                    &document,
                    ApplicationCloseRequestKind::Save,
                );
                saving.pending = PendingSave {
                    token: save_token.clone(),
                };
                self.state = CoordinatorState::Saving(saving);
                ApplicationCloseTransition::applied(vec![ApplicationCloseCommand::SaveDocument {
                    token: save_token,
                    document_id: document.id,
                    destination: SaveDestination::Selected(target),
                }])
            }
            ApplicationCloseResult::SaveAsCancelled { token }
                if token.kind == ApplicationCloseRequestKind::SaveAs =>
            {
                self.interrupt_save(
                    saving,
                    ApplicationCloseInterruptionReason::SaveAsCancelled,
                    Some(token.document_id),
                    None,
                )
            }
            ApplicationCloseResult::SaveAsFailed { token, message }
                if token.kind == ApplicationCloseRequestKind::SaveAs =>
            {
                self.interrupt_save(
                    saving,
                    ApplicationCloseInterruptionReason::SaveFailed,
                    Some(token.document_id),
                    Some(message),
                )
            }
            ApplicationCloseResult::SaveSucceeded { token }
                if token.kind == ApplicationCloseRequestKind::Save =>
            {
                saving.completed_document_ids.push(token.document_id);
                saving.next_document_index += 1;
                if saving.next_document_index == saving.dirty_documents.len() {
                    return self.start_release(
                        saving.transaction,
                        ApplicationCloseCompletionKind::SavedAll,
                        saving.completed_document_ids,
                    );
                }

                let document = saving.dirty_documents[saving.next_document_index].clone();
                let (pending, command) =
                    self.initial_save_request(saving.transaction.id, &document);
                saving.pending = pending;
                self.state = CoordinatorState::Saving(saving);
                ApplicationCloseTransition::applied(vec![command])
            }
            ApplicationCloseResult::SaveFailed { token, message }
                if token.kind == ApplicationCloseRequestKind::Save =>
            {
                self.interrupt_save(
                    saving,
                    ApplicationCloseInterruptionReason::SaveFailed,
                    Some(token.document_id),
                    Some(message),
                )
            }
            ApplicationCloseResult::SavePublishedWithWarning { token, message }
                if token.kind == ApplicationCloseRequestKind::Save =>
            {
                let document_id = token.document_id;
                saving.completed_document_ids.push(document_id.clone());
                self.interrupt_save(
                    saving,
                    ApplicationCloseInterruptionReason::SavePublishedWithWarning,
                    Some(document_id),
                    Some(message),
                )
            }
            _ => {
                self.state = CoordinatorState::Saving(saving);
                ApplicationCloseTransition::status(
                    ApplicationCloseTransitionStatus::StaleResultRejected,
                )
            }
        }
    }

    fn interrupt_save(
        &mut self,
        saving: SavingState,
        reason: ApplicationCloseInterruptionReason,
        target_document_id: Option<String>,
        message: Option<String>,
    ) -> ApplicationCloseTransition {
        let transaction_id = saving.transaction.id;
        self.last_interruption = Some(ApplicationCloseInterruption {
            transaction_id,
            reason,
            snapshot: saving.transaction.snapshot,
            completed_save_document_ids: saving.completed_document_ids,
            completed_release_document_ids: Vec::new(),
            target_document_id: target_document_id.clone(),
            message: message.clone(),
        });
        let mut commands = vec![ApplicationCloseCommand::CancelApplicationClose { transaction_id }];
        if let (Some(document_id), Some(message)) = (target_document_id, message) {
            commands.push(
                if reason == ApplicationCloseInterruptionReason::SavePublishedWithWarning {
                    ApplicationCloseCommand::ReportSaveWarning {
                        transaction_id,
                        document_id,
                        message,
                    }
                } else {
                    ApplicationCloseCommand::ReportSaveFailure {
                        transaction_id,
                        document_id,
                        message,
                    }
                },
            );
        }
        ApplicationCloseTransition::applied(commands)
    }

    fn handle_release_result(
        &mut self,
        mut releasing: ReleasingState,
        result: ApplicationCloseResult,
    ) -> ApplicationCloseTransition {
        let (token, failure) = match result {
            ApplicationCloseResult::DocumentReleased { token } => (token, None),
            ApplicationCloseResult::DocumentReleaseFailed { token, message } => {
                (token, Some(message))
            }
            _ => {
                self.state = CoordinatorState::Releasing(releasing);
                return ApplicationCloseTransition::status(
                    ApplicationCloseTransitionStatus::StaleResultRejected,
                );
            }
        };
        let Some(index) = releasing
            .pending_tokens
            .iter()
            .position(|pending| pending == &token)
        else {
            self.state = CoordinatorState::Releasing(releasing);
            return ApplicationCloseTransition::status(
                ApplicationCloseTransitionStatus::StaleResultRejected,
            );
        };
        if let Some(message) = failure {
            let transaction_id = releasing.transaction.id;
            let document_id = token.document_id.clone();
            self.last_interruption = Some(ApplicationCloseInterruption {
                transaction_id,
                reason: ApplicationCloseInterruptionReason::ReleaseFailed,
                snapshot: releasing.transaction.snapshot,
                completed_save_document_ids: releasing.completed_save_document_ids,
                completed_release_document_ids: releasing.completed_release_document_ids.clone(),
                target_document_id: Some(document_id.clone()),
                message: Some(message.clone()),
            });
            return ApplicationCloseTransition::applied(vec![
                ApplicationCloseCommand::CancelApplicationClose { transaction_id },
                ApplicationCloseCommand::ReportReleaseFailure {
                    transaction_id,
                    document_id,
                    message,
                    completed_release_document_ids: releasing.completed_release_document_ids,
                },
            ]);
        }
        releasing.pending_tokens.remove(index);
        releasing
            .completed_release_document_ids
            .push(token.document_id);
        if !releasing.pending_tokens.is_empty() {
            let next = release_command(&releasing.pending_tokens[0]);
            self.state = CoordinatorState::Releasing(releasing);
            return ApplicationCloseTransition::applied(vec![next]);
        }

        let transaction_id = releasing.transaction.id;
        let document_ids = match releasing.completion_kind {
            ApplicationCloseCompletionKind::SavedAll => releasing.completed_save_document_ids,
            ApplicationCloseCompletionKind::DiscardedAll => {
                releasing.completed_release_document_ids
            }
            ApplicationCloseCompletionKind::NoDirtyDocuments => {
                releasing.completed_release_document_ids
            }
        };
        self.last_completion = Some(ApplicationCloseCompletion {
            transaction_id,
            kind: releasing.completion_kind,
            document_ids,
        });
        ApplicationCloseTransition::applied(vec![
            ApplicationCloseCommand::ConfirmApplicationClose { transaction_id },
        ])
    }

    fn initial_save_request(
        &mut self,
        transaction_id: u64,
        document: &ApplicationCloseDocument,
    ) -> (PendingSave, ApplicationCloseCommand) {
        if document.requires_save_as {
            let token = self.next_token(
                transaction_id,
                document,
                ApplicationCloseRequestKind::SaveAs,
            );
            (
                PendingSave {
                    token: token.clone(),
                },
                ApplicationCloseCommand::RequestSaveAs {
                    token,
                    document_id: document.id.clone(),
                    suggested_name: document.name.clone(),
                },
            )
        } else {
            let token =
                self.next_token(transaction_id, document, ApplicationCloseRequestKind::Save);
            (
                PendingSave {
                    token: token.clone(),
                },
                ApplicationCloseCommand::SaveDocument {
                    token,
                    document_id: document.id.clone(),
                    destination: SaveDestination::Existing,
                },
            )
        }
    }

    fn next_token(
        &mut self,
        transaction_id: u64,
        document: &ApplicationCloseDocument,
        kind: ApplicationCloseRequestKind,
    ) -> ApplicationCloseToken {
        self.next_request_sequence = self.next_request_sequence.saturating_add(1);
        ApplicationCloseToken {
            transaction_id,
            request_sequence: self.next_request_sequence,
            document_id: document.id.clone(),
            dirty_revision: document.dirty_revision.unwrap_or(0),
            kind,
        }
    }
}

impl ApplicationCloseResult {
    fn token(&self) -> &ApplicationCloseToken {
        match self {
            Self::SaveAsSelected { token, .. }
            | Self::SaveAsCancelled { token }
            | Self::SaveAsFailed { token, .. }
            | Self::SaveSucceeded { token }
            | Self::SaveFailed { token, .. }
            | Self::SavePublishedWithWarning { token, .. }
            | Self::DocumentReleased { token }
            | Self::DocumentReleaseFailed { token, .. } => token,
        }
    }
}

impl CoordinatorState {
    fn first_release(&self) -> Option<ApplicationCloseCommand> {
        let Self::Releasing(releasing) = self else {
            return None;
        };
        releasing.pending_tokens.first().map(release_command)
    }
}

fn release_command(token: &ApplicationCloseToken) -> ApplicationCloseCommand {
    ApplicationCloseCommand::ReleaseDocument {
        token: token.clone(),
        document_id: token.document_id.clone(),
    }
}

fn action_specs(busy: bool) -> [ApplicationCloseActionSpec; 3] {
    [
        ApplicationCloseActionSpec {
            id: APPLICATION_CLOSE_CANCEL_ID,
            action: ApplicationCloseAction::Cancel,
            label: "Cancel",
            role: ApplicationCloseActionRole::Outline,
            disabled: busy,
        },
        ApplicationCloseActionSpec {
            id: APPLICATION_CLOSE_DISCARD_ALL_ID,
            action: ApplicationCloseAction::DiscardAll,
            label: "Discard All",
            role: ApplicationCloseActionRole::Destructive,
            disabled: busy,
        },
        ApplicationCloseActionSpec {
            id: APPLICATION_CLOSE_SAVE_ALL_ID,
            action: ApplicationCloseAction::SaveAll,
            label: if busy { "Saving…" } else { "Save All" },
            role: ApplicationCloseActionRole::Primary,
            disabled: busy,
        },
    ]
}
