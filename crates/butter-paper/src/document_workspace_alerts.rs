//! The alerts stacked above the canvas: save, recovery, open and pick
//! feedback for the active document.

use super::*;

/// What the alert stack shows for the active document.
pub(super) struct DocumentAlerts {
    pub document_id: DocumentId,
    pub redact_pending: bool,
    pub text_box_commit_error: Option<String>,
    pub save_failure: Option<DocumentSaveFailure>,
    pub save_failure_title: String,
    pub recovery_preparation_issue: Option<(String, bool, bool)>,
    pub presentation_error: Option<String>,
    pub recovery_pending: bool,
}

impl DocumentWorkspace {
    /// The opening error, dismissed by the X in its top-right corner.
    pub(super) fn document_open_failure_alert(
        &self,
        failure: &DocumentOpenFailure,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        gpui::div()
            .id(DOCUMENT_OPEN_ERROR_ALERT_ID)
            .debug_selector(|| DOCUMENT_OPEN_ERROR_ALERT_ID.into())
            .relative()
            .w_full()
            .min_w_0()
            .child(
                Alert::error(
                    "document-workspace-open-feedback-message",
                    failure.presentation(),
                )
                .title("Some PDFs couldn’t be opened")
                // Room for the X, which sits over the alert's corner.
                .pr_10(),
            )
            .child(gpui::div().absolute().top_1().right_1().child(
                close_icon_button(DOCUMENT_OPEN_ERROR_DISMISS_ID, "Close opening error").on_click(
                    cx.listener(|workspace, _, _, cx| {
                        workspace.dismiss_document_open_failure(cx);
                    }),
                ),
            ))
    }

    pub(super) fn render_document_alerts(
        &self,
        column: gpui::Div,
        alerts: DocumentAlerts,
        recovery_only: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let DocumentAlerts {
            document_id,
            redact_pending,
            text_box_commit_error,
            save_failure,
            save_failure_title,
            recovery_preparation_issue,
            presentation_error,
            recovery_pending,
        } = alerts;
        column
        .when(redact_pending && !recovery_only, |root| {
            root.child(
                gpui::div()
                    .id(DOCUMENT_REDACT_PENDING_ALERT_ID)
                    .debug_selector(|| DOCUMENT_REDACT_PENDING_ALERT_ID.into())
                    .w_full()
                    .px_3()
                    .py_2()
                    .child(
                        Alert::warning(
                            "document-workspace-redact-pending-message",
                            PENDING_REDACTION_STATUS,
                        )
                        .title("Pending redaction mark"),
                ),
            )
        })
        .when_some(text_box_commit_error.filter(|_| !recovery_only), |root, error| {
            root.child(
                gpui::div()
                    .id(DOCUMENT_TEXT_BOX_COMMIT_ERROR_ALERT_ID)
                    .debug_selector(|| DOCUMENT_TEXT_BOX_COMMIT_ERROR_ALERT_ID.into())
                    .w_full()
                    .px_3()
                    .py_2()
                    .child(
                        Alert::error("document-workspace-text-box-commit-error-message", error)
                            .title("Couldn’t finish editing"),
                    ),
            )
        })
        .when_some(self.last_document_open_failure().cloned().filter(|_| !recovery_only), |root, failure| {
            root.child(
                gpui::div()
                    .w_full()
                    .p_2()
                    .child(self.document_open_failure_alert(&failure, cx)),
            )
        })
        .when_some(save_failure.filter(|_| recovery_only), |root, failure| {
            root.child(
                v_flex()
                    .debug_selector(|| DOCUMENT_SAVE_ERROR_ALERT_ID.into())
                    .w_full()
                    .gap_2()
                    .p_2()
                    .child(
                        Alert::error(
                            DOCUMENT_SAVE_ERROR_ALERT_ID,
                            failure.presentation_message().to_owned(),
                        )
                            .title(save_failure_title)
                            .flex_1().min_w_0(),
                    )
                    .child(h_flex().w_full().flex_wrap().gap_2()
                    .when(
                        failure.operation == DocumentSaveFailureOperation::InPlace
                            && failure.can_retry(),
                        |actions| {
                            actions.child(
                                Button::new(DOCUMENT_SAVE_ERROR_RETRY_ID)
                                    .debug_selector(|| DOCUMENT_SAVE_ERROR_RETRY_ID.into())
                                    .accessibility_id(DOCUMENT_SAVE_ERROR_RETRY_ID)
                                    .label("Retry")
                                    .on_click(cx.listener(move |workspace, _, _, cx| {
                                        workspace.retry_document_save_failure(document_id, cx);
                                    })),
                            )
                        },
                    )
                    .child(
                        Button::new(DOCUMENT_SAVE_ERROR_SAVE_AS_ID)
                            .debug_selector(|| DOCUMENT_SAVE_ERROR_SAVE_AS_ID.into())
                            .accessibility_id(DOCUMENT_SAVE_ERROR_SAVE_AS_ID)
                            .outline()
                            .label("Save As…")
                            .on_click(cx.listener(move |workspace, _, _, cx| {
                                workspace.prompt_to_save_as(document_id, cx);
                            })),
                    )

                    ),
            )
        })
        .when_some(
            recovery_preparation_issue.filter(|_| recovery_only),
            |root, (error, ambiguous, rebase_failed)| {
                let confirmation_pending = (ambiguous || rebase_failed)
                    && self.recovery_confirmation_pending.contains(&document_id);
                let action_id = if ambiguous {
                    DOCUMENT_RECOVERY_PREPARATION_CONFIRM_ID
                } else if rebase_failed {
                    DOCUMENT_RECOVERY_REBASE_RETRY_ID
                } else {
                    DOCUMENT_RECOVERY_PREPARATION_RETRY_ID
                };
                root.child(
                    v_flex()
                        .id(DOCUMENT_RECOVERY_PREPARATION_ALERT_ID)
                        .debug_selector(|| DOCUMENT_RECOVERY_PREPARATION_ALERT_ID.into())
                        .w_full()
                        .gap_2()
                        .p_2()
                        .child(
                            Alert::error(
                                "document-recovery-preparation-message",
                                format!(
                                    "Butter Paper cannot safely accept edits until crash recovery is available: {error}"
                                ),
                            )
                            .title(if ambiguous {
                                "Crash recovery needs confirmation"
                            } else if rebase_failed {
                                "Crash recovery needs repair"
                            } else {
                                "Crash recovery unavailable"
                            })
                            .flex_1().min_w_0(),
                        )
                        .child(
                            Button::new(action_id)
                                .debug_selector(move || action_id.into())
                                .accessibility_id(action_id)
                                .label(if confirmation_pending {
                                    if rebase_failed {
                                        "Retrying…"
                                    } else {
                                        "Checking…"
                                    }
                                } else if ambiguous {
                                    "Check again"
                                } else if rebase_failed {
                                    "Retry recovery"
                                } else {
                                    "Retry"
                                })
                                .disabled(confirmation_pending)
                                .on_click(cx.listener(move |workspace, _, _, cx| {
                                    if let Err(error) = workspace
                                        .retry_document_recovery_preparation(document_id, cx)
                                    {
                                        workspace.last_file_error = Some(error);
                                        cx.notify();
                                    }
                                })),
                        ),
                )
            },
        )
        .when_some(self.last_file_error.clone().filter(|_| !recovery_only), |root, error| {
            root.child(
                gpui::div()
                    .id(DOCUMENT_ERROR_ID)
                    .debug_selector(|| DOCUMENT_ERROR_ID.into())
                    .w_full()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
        })
        .when_some(presentation_error.filter(|_| !recovery_only), |root, error| {
            root.child(
                h_flex()
                    .id(DOCUMENT_RECOVERY_ALERT_ID)
                    .debug_selector(|| DOCUMENT_RECOVERY_ALERT_ID.into())
                    .w_full()
                    .gap_2()
                    .p_2()
                    .child(
                        Alert::info("document-worker-recovery-message", error)
                            .title("PDF rendering stopped")
                            .flex_1().min_w_0(),
                    )
                    .child(
                        Button::new(DOCUMENT_RECOVERY_RETRY_ID)
                            .debug_selector(|| DOCUMENT_RECOVERY_RETRY_ID.into())
                            .label(if recovery_pending { "Retrying…" } else { "Retry" })
                            .disabled(recovery_pending)
                            .on_click(cx.listener(move |workspace, _, _, cx| {
                                if let Err(error) =
                                    workspace.retry_document_recovery_async(document_id, cx)
                                {
                                    workspace.last_file_error = Some(error);
                                    cx.notify();
                                }
                            })),
                    ),
            )
        })
    }
}
