#[path = "../src/application_close.rs"]
mod application_close;

use application_close::{
    APPLICATION_CLOSE_DESCRIPTION_ID, APPLICATION_CLOSE_DIALOG_ID, APPLICATION_CLOSE_TITLE,
    APPLICATION_CLOSE_TITLE_ID, ApplicationCloseAction, ApplicationCloseActionRole,
    ApplicationCloseCommand, ApplicationCloseCoordinator, ApplicationCloseDocument,
    ApplicationCloseResult, ApplicationCloseSnapshot, ApplicationCloseTransitionStatus,
    SaveDestination,
};

fn document(
    id: &str,
    name: &str,
    dirty_revision: Option<u64>,
    requires_save_as: bool,
) -> ApplicationCloseDocument {
    ApplicationCloseDocument {
        id: id.to_owned(),
        name: name.to_owned(),
        dirty_revision,
        requires_save_as,
    }
}

fn snapshot(
    active_document_id: Option<&str>,
    documents: Vec<ApplicationCloseDocument>,
) -> ApplicationCloseSnapshot {
    ApplicationCloseSnapshot {
        documents,
        active_document_id: active_document_id.map(str::to_owned),
    }
}

fn only_command(
    transition: &application_close::ApplicationCloseTransition,
) -> &ApplicationCloseCommand {
    assert_eq!(transition.commands.len(), 1);
    &transition.commands[0]
}

fn save_token(command: &ApplicationCloseCommand) -> application_close::ApplicationCloseToken {
    match command {
        ApplicationCloseCommand::RequestSaveAs { token, .. }
        | ApplicationCloseCommand::SaveDocument { token, .. } => token.clone(),
        other => panic!("expected a save command, got {other:?}"),
    }
}

#[test]
fn application_close_dialog_freezes_the_electron_copy_order_roles_and_busy_state() {
    let mut close = ApplicationCloseCoordinator::default();
    let opened = close
        .request_close(snapshot(
            Some("one"),
            vec![
                document("one", "one.pdf", Some(3), false),
                document("two", "two.pdf", Some(5), true),
            ],
        ))
        .expect("the frozen snapshot is valid");

    assert_eq!(opened.status, ApplicationCloseTransitionStatus::Applied);
    assert!(opened.commands.is_empty());
    let dialog = close.dialog().expect("dirty documents open the dialog");
    assert_eq!(APPLICATION_CLOSE_DIALOG_ID, "unsaved-changes-dialog");
    assert_eq!(APPLICATION_CLOSE_TITLE_ID, "application-close-title");
    assert_eq!(
        APPLICATION_CLOSE_DESCRIPTION_ID,
        "application-close-description"
    );
    assert_eq!(dialog.title, APPLICATION_CLOSE_TITLE);
    assert_eq!(dialog.description, "2 modified PDFs have unsaved changes.");
    assert_eq!(dialog.dirty_document_count, 2);
    assert!(!dialog.busy);
    assert_eq!(
        dialog
            .actions
            .map(|action| (action.action, action.label, action.role)),
        [
            (
                ApplicationCloseAction::Cancel,
                "Cancel",
                ApplicationCloseActionRole::Outline,
            ),
            (
                ApplicationCloseAction::DiscardAll,
                "Discard All",
                ApplicationCloseActionRole::Destructive,
            ),
            (
                ApplicationCloseAction::SaveAll,
                "Save All",
                ApplicationCloseActionRole::Primary,
            ),
        ],
    );

    let saving = close.choose(ApplicationCloseAction::SaveAll);
    assert!(matches!(
        only_command(&saving),
        ApplicationCloseCommand::SaveDocument {
            document_id,
            destination: SaveDestination::Existing,
            ..
        } if document_id == "one"
    ));
    let dialog = close
        .dialog()
        .expect("the dialog stays visible while saving");
    assert!(dialog.busy);
    assert!(dialog.actions.iter().all(|action| action.disabled));
    assert_eq!(dialog.actions[2].label, "Saving…");
}

#[test]
fn application_close_cancel_preserves_tab_order_active_identity_dirty_revisions_and_workers() {
    let original = snapshot(
        Some("two"),
        vec![
            document("one", "one.pdf", None, false),
            document("two", "two.pdf", Some(7), false),
            document("three", "three.pdf", Some(11), true),
        ],
    );
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(original.clone())
        .expect("the frozen snapshot is valid");

    let cancelled = close.choose(ApplicationCloseAction::Cancel);
    let transaction_id = match only_command(&cancelled) {
        ApplicationCloseCommand::CancelApplicationClose { transaction_id } => *transaction_id,
        other => panic!("expected application close cancellation, got {other:?}"),
    };

    assert!(close.dialog().is_none());
    let interruption = close
        .last_interruption()
        .expect("cancellation retains deterministic evidence");
    assert_eq!(interruption.transaction_id, transaction_id);
    assert_eq!(interruption.snapshot, original);
    assert_eq!(
        interruption.completed_save_document_ids,
        Vec::<String>::new()
    );
    assert!(
        cancelled
            .commands
            .iter()
            .all(|command| !matches!(command, ApplicationCloseCommand::ReleaseDocument { .. }))
    );
}

#[test]
fn application_close_save_all_runs_in_tab_order_with_only_one_save_in_flight() {
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(snapshot(
            Some("clean"),
            vec![
                document("clean", "clean.pdf", None, false),
                document("one", "one.pdf", Some(1), false),
                document("two", "two.pdf", Some(2), true),
                document("three", "three.pdf", Some(3), false),
            ],
        ))
        .expect("the frozen snapshot is valid");

    let one = close.choose(ApplicationCloseAction::SaveAll);
    let one_token = save_token(only_command(&one));
    assert_eq!(one_token.document_id, "one");
    assert_eq!(one_token.dirty_revision, 1);

    let two_prompt =
        close.handle_result(ApplicationCloseResult::SaveSucceeded { token: one_token });
    let two_prompt_token = save_token(only_command(&two_prompt));
    assert!(matches!(
        only_command(&two_prompt),
        ApplicationCloseCommand::RequestSaveAs {
            document_id,
            suggested_name,
            ..
        } if document_id == "two" && suggested_name == "two.pdf"
    ));

    let two_save = close.handle_result(ApplicationCloseResult::SaveAsSelected {
        token: two_prompt_token,
        target: "chosen-two.pdf".to_owned(),
    });
    let two_save_token = save_token(only_command(&two_save));
    assert!(matches!(
        only_command(&two_save),
        ApplicationCloseCommand::SaveDocument {
            document_id,
            destination: SaveDestination::Selected(target),
            ..
        } if document_id == "two" && target == "chosen-two.pdf"
    ));

    let three = close.handle_result(ApplicationCloseResult::SaveSucceeded {
        token: two_save_token,
    });
    let three_token = save_token(only_command(&three));
    assert_eq!(three_token.document_id, "three");

    let mut release =
        close.handle_result(ApplicationCloseResult::SaveSucceeded { token: three_token });
    for expected_document_id in ["clean", "one", "two", "three"] {
        let token = match only_command(&release) {
            ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
                assert_eq!(document_id, expected_document_id);
                token.clone()
            }
            other => {
                panic!("Save All must release {expected_document_id} before quit, got {other:?}")
            }
        };
        release = close.handle_result(ApplicationCloseResult::DocumentReleased { token });
    }
    assert!(matches!(
        only_command(&release),
        ApplicationCloseCommand::ConfirmApplicationClose { .. }
    ));
    assert_eq!(
        close
            .last_completion()
            .expect("the completed transaction is retained")
            .document_ids,
        ["one", "two", "three"]
    );
}

#[test]
fn application_close_save_as_cancel_preserves_remaining_documents_and_retry_order() {
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(snapshot(
            Some("one"),
            vec![
                document("one", "one.pdf", Some(4), false),
                document("two", "two.pdf", Some(8), true),
                document("three", "three.pdf", Some(12), false),
            ],
        ))
        .expect("the frozen snapshot is valid");

    let one = close.choose(ApplicationCloseAction::SaveAll);
    let two = close.handle_result(ApplicationCloseResult::SaveSucceeded {
        token: save_token(only_command(&one)),
    });
    let cancelled = close.handle_result(ApplicationCloseResult::SaveAsCancelled {
        token: save_token(only_command(&two)),
    });
    assert!(matches!(
        only_command(&cancelled),
        ApplicationCloseCommand::CancelApplicationClose { .. }
    ));
    let interruption = close
        .last_interruption()
        .expect("the interrupted transaction is retained");
    assert_eq!(interruption.completed_save_document_ids, ["one"]);
    assert_eq!(interruption.target_document_id.as_deref(), Some("two"));

    close
        .request_close(snapshot(
            Some("one"),
            vec![
                document("one", "one.pdf", None, false),
                document("two", "two.pdf", Some(8), true),
                document("three", "three.pdf", Some(12), false),
            ],
        ))
        .expect("retry uses the authoritative post-save snapshot");
    let retry = close.choose(ApplicationCloseAction::SaveAll);
    assert_eq!(save_token(only_command(&retry)).document_id, "two");
}

#[test]
fn application_close_save_as_failure_is_not_misclassified_as_user_cancellation() {
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(snapshot(
            Some("generated"),
            vec![document("generated", "Untitled 2.pdf", Some(7), true)],
        ))
        .expect("the frozen snapshot is valid");
    let prompt = close.choose(ApplicationCloseAction::SaveAll);
    let failed = close.handle_result(ApplicationCloseResult::SaveAsFailed {
        token: save_token(only_command(&prompt)),
        message: "Save As requires a .pdf file name.".to_owned(),
    });

    assert_eq!(failed.commands.len(), 2);
    assert!(matches!(
        &failed.commands[0],
        ApplicationCloseCommand::CancelApplicationClose { .. }
    ));
    assert!(matches!(
        &failed.commands[1],
        ApplicationCloseCommand::ReportSaveFailure {
            document_id,
            message,
            ..
        } if document_id == "generated" && message == "Save As requires a .pdf file name."
    ));
    let interruption = close.last_interruption().unwrap();
    assert_eq!(
        interruption.reason,
        application_close::ApplicationCloseInterruptionReason::SaveFailed,
    );
    assert_eq!(
        interruption.target_document_id.as_deref(),
        Some("generated")
    );
    assert_eq!(
        interruption.completed_save_document_ids,
        Vec::<String>::new()
    );
}

#[test]
fn application_close_save_failure_stops_the_queue_and_preserves_retry_evidence() {
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(snapshot(
            Some("one"),
            vec![
                document("one", "one.pdf", Some(4), false),
                document("two", "two.pdf", Some(8), false),
                document("three", "three.pdf", Some(12), false),
            ],
        ))
        .expect("the frozen snapshot is valid");

    let one = close.choose(ApplicationCloseAction::SaveAll);
    let two = close.handle_result(ApplicationCloseResult::SaveSucceeded {
        token: save_token(only_command(&one)),
    });
    let failed = close.handle_result(ApplicationCloseResult::SaveFailed {
        token: save_token(only_command(&two)),
        message: "disk full".to_owned(),
    });

    assert_eq!(failed.commands.len(), 2);
    assert!(matches!(
        &failed.commands[0],
        ApplicationCloseCommand::CancelApplicationClose { .. }
    ));
    assert!(matches!(
        &failed.commands[1],
        ApplicationCloseCommand::ReportSaveFailure {
            document_id,
            message,
            ..
        } if document_id == "two" && message == "disk full"
    ));
    let interruption = close
        .last_interruption()
        .expect("the failure is retained for deterministic retry evidence");
    assert_eq!(interruption.completed_save_document_ids, ["one"]);
    assert_eq!(interruption.target_document_id.as_deref(), Some("two"));
    assert!(!failed.commands.iter().any(|command| matches!(
        command,
        ApplicationCloseCommand::SaveDocument { document_id, .. } if document_id == "three"
    )));

    close
        .request_close(snapshot(
            Some("one"),
            vec![
                document("one", "one.pdf", None, false),
                document("two", "two.pdf", Some(8), false),
                document("three", "three.pdf", Some(12), false),
            ],
        ))
        .expect("retry uses the authoritative post-failure snapshot");
    let retry = close.choose(ApplicationCloseAction::SaveAll);
    assert_eq!(save_token(only_command(&retry)).document_id, "two");
}

#[test]
fn published_save_warning_stops_close_without_reclassifying_the_published_document_as_unsaved() {
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(snapshot(
            Some("one"),
            vec![
                document("one", "one.pdf", Some(4), false),
                document("two", "two.pdf", Some(8), false),
            ],
        ))
        .expect("the frozen snapshot is valid");
    let first = close.choose(ApplicationCloseAction::SaveAll);
    let warning = close.handle_result(ApplicationCloseResult::SavePublishedWithWarning {
        token: save_token(only_command(&first)),
        message: "directory sync failed after publication".to_owned(),
    });

    assert_eq!(warning.commands.len(), 2);
    assert!(matches!(
        &warning.commands[0],
        ApplicationCloseCommand::CancelApplicationClose { .. }
    ));
    assert!(matches!(
        &warning.commands[1],
        ApplicationCloseCommand::ReportSaveWarning {
            document_id,
            message,
            ..
        } if document_id == "one" && message == "directory sync failed after publication"
    ));
    assert!(!warning.commands.iter().any(|command| matches!(
        command,
        ApplicationCloseCommand::SaveDocument { document_id, .. } if document_id == "two"
    )));
    let interruption = close
        .last_interruption()
        .expect("the published warning must remain available to the UI");
    assert_eq!(
        interruption.reason,
        application_close::ApplicationCloseInterruptionReason::SavePublishedWithWarning,
    );
    assert_eq!(interruption.completed_save_document_ids, ["one"]);
    assert_eq!(interruption.target_document_id.as_deref(), Some("one"));
}

#[test]
fn application_close_discard_all_releases_documents_serially_before_confirming() {
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(snapshot(
            Some("two"),
            vec![
                document("one", "one.pdf", None, false),
                document("two", "two.pdf", Some(9), false),
                document("three", "three.pdf", None, false),
            ],
        ))
        .expect("the frozen snapshot is valid");

    let first = close.choose(ApplicationCloseAction::DiscardAll);
    let first_token = match only_command(&first) {
        ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
            assert_eq!(document_id, "one");
            token.clone()
        }
        other => panic!("discard must start with one serial release, got {other:?}"),
    };
    let second =
        close.handle_result(ApplicationCloseResult::DocumentReleased { token: first_token });
    let second_token = match only_command(&second) {
        ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
            assert_eq!(document_id, "two");
            token.clone()
        }
        other => panic!("first acknowledgement must advance to document two, got {other:?}"),
    };
    let third = close.handle_result(ApplicationCloseResult::DocumentReleased {
        token: second_token,
    });
    let third_token = match only_command(&third) {
        ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
            assert_eq!(document_id, "three");
            token.clone()
        }
        other => panic!("second acknowledgement must advance to document three, got {other:?}"),
    };
    let confirmed =
        close.handle_result(ApplicationCloseResult::DocumentReleased { token: third_token });
    assert!(matches!(
        only_command(&confirmed),
        ApplicationCloseCommand::ConfirmApplicationClose { .. }
    ));
    assert_eq!(
        close
            .last_completion()
            .expect("discard completion is retained")
            .document_ids,
        ["one", "two", "three"]
    );
}

#[test]
fn application_close_release_failure_cancels_and_allows_a_fresh_retry() {
    let mut close = ApplicationCloseCoordinator::default();
    let frozen = snapshot(
        Some("one"),
        vec![
            document("one", "one.pdf", Some(4), false),
            document("two", "two.pdf", Some(9), false),
        ],
    );
    close.request_close(frozen.clone()).unwrap();
    let releases = close.choose(ApplicationCloseAction::DiscardAll);
    let first_token = match only_command(&releases) {
        ApplicationCloseCommand::ReleaseDocument { token, .. } => token.clone(),
        other => panic!("expected first release command, got {other:?}"),
    };
    let second =
        close.handle_result(ApplicationCloseResult::DocumentReleased { token: first_token });
    let token = match only_command(&second) {
        ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
            assert_eq!(document_id, "two");
            token.clone()
        }
        other => panic!("expected second release command, got {other:?}"),
    };
    let failed = close.handle_result(ApplicationCloseResult::DocumentReleaseFailed {
        token,
        message: "worker did not acknowledge close".into(),
    });
    assert_eq!(failed.commands.len(), 2);
    assert!(matches!(
        &failed.commands[0],
        ApplicationCloseCommand::CancelApplicationClose { .. }
    ));
    assert!(matches!(
        &failed.commands[1],
        ApplicationCloseCommand::ReportReleaseFailure {
            document_id,
            message,
            completed_release_document_ids,
            ..
        } if document_id == "two"
            && message == "worker did not acknowledge close"
            && completed_release_document_ids == &["one"]
    ));
    assert!(close.dialog().is_none());
    assert_eq!(
        close.last_interruption().unwrap().reason,
        application_close::ApplicationCloseInterruptionReason::ReleaseFailed
    );
    assert_eq!(
        close
            .last_interruption()
            .unwrap()
            .completed_release_document_ids,
        ["one"]
    );

    let retry = close
        .request_close(snapshot(
            Some("two"),
            vec![document("two", "two.pdf", Some(9), false)],
        ))
        .unwrap();
    assert!(retry.commands.is_empty());
    assert!(close.dialog().is_some());
}

#[test]
fn application_close_rejects_stale_save_and_release_results_without_advancing_state() {
    let mut close = ApplicationCloseCoordinator::default();
    close
        .request_close(snapshot(
            Some("one"),
            vec![document("one", "one.pdf", Some(1), false)],
        ))
        .expect("the frozen snapshot is valid");
    let first_save = close.choose(ApplicationCloseAction::SaveAll);
    let stale_save_token = save_token(only_command(&first_save));
    let cancelled = close.choose(ApplicationCloseAction::Cancel);
    assert_eq!(
        cancelled.status,
        ApplicationCloseTransitionStatus::IgnoredBusy
    );

    let stale = close.handle_result(ApplicationCloseResult::SaveSucceeded {
        token: application_close::ApplicationCloseToken {
            request_sequence: stale_save_token.request_sequence + 1,
            ..stale_save_token.clone()
        },
    });
    assert_eq!(
        stale.status,
        ApplicationCloseTransitionStatus::StaleResultRejected
    );
    assert!(stale.commands.is_empty());
    assert!(
        close
            .dialog()
            .expect("the current save remains pending")
            .busy
    );

    let releasing = close.handle_result(ApplicationCloseResult::SaveSucceeded {
        token: stale_save_token.clone(),
    });
    let release_token = match only_command(&releasing) {
        ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
            assert_eq!(document_id, "one");
            token.clone()
        }
        other => panic!("the saved document must release before quit, got {other:?}"),
    };
    let completed = close.handle_result(ApplicationCloseResult::DocumentReleased {
        token: release_token,
    });
    assert!(matches!(
        only_command(&completed),
        ApplicationCloseCommand::ConfirmApplicationClose { .. }
    ));

    close
        .request_close(snapshot(
            Some("one"),
            vec![document("one", "one.pdf", Some(2), false)],
        ))
        .expect("the next close transaction is valid");
    let release = close.choose(ApplicationCloseAction::DiscardAll);
    let release_token = match only_command(&release) {
        ApplicationCloseCommand::ReleaseDocument { token, .. } => token.clone(),
        other => panic!("expected a release intent, got {other:?}"),
    };
    let old_transaction_release = close.handle_result(ApplicationCloseResult::DocumentReleased {
        token: application_close::ApplicationCloseToken {
            transaction_id: release_token.transaction_id.saturating_sub(1),
            ..release_token.clone()
        },
    });
    assert_eq!(
        old_transaction_release.status,
        ApplicationCloseTransitionStatus::StaleResultRejected
    );
    assert!(close.dialog().expect("discard remains pending").busy);
}

#[test]
fn application_close_without_dirty_documents_releases_before_confirming_without_a_dialog() {
    let mut close = ApplicationCloseCoordinator::default();
    let transition = close
        .request_close(snapshot(
            Some("one"),
            vec![document("one", "one.pdf", None, false)],
        ))
        .expect("the frozen snapshot is valid");

    let release_token = match only_command(&transition) {
        ApplicationCloseCommand::ReleaseDocument { token, document_id } => {
            assert_eq!(document_id, "one");
            token.clone()
        }
        other => panic!("a clean application close must release before quit, got {other:?}"),
    };
    let confirmed = close.handle_result(ApplicationCloseResult::DocumentReleased {
        token: release_token,
    });
    assert!(matches!(
        only_command(&confirmed),
        ApplicationCloseCommand::ConfirmApplicationClose { .. }
    ));
    assert!(close.dialog().is_none());
}
