//! A single, conditional home for recovery and save problems.
use super::*;
use crate::property_controls::popover_header;
use gpui_component::Icon;

pub const RECOVERY_NOTIFICATIONS_BUTTON_ID: &str = "recovery-notifications-button";
pub const RECOVERY_NOTIFICATIONS_PANEL_ID: &str = "recovery-notifications-panel";
pub const RECOVERY_NOTIFICATIONS_CLOSE_ID: &str = "recovery-notifications-close";

impl DocumentWorkspace {
    pub fn recovery_attention_count(&self, cx: &App) -> usize {
        usize::from(self.startup_recovery_inspecting)
            + self.startup_recovery_items.len()
            + usize::from(
                self.session_recovery_warning.is_some() && self.startup_recovery_items.is_empty(),
            )
            + self
                .sessions
                .iter()
                .map(|session| {
                    let session = session.read(cx);
                    usize::from(matches!(
                        session.save_status,
                        NativeDocumentSaveStatus::Failed(_)
                    )) + usize::from(matches!(
                        session.recovery_preparation,
                        DocumentRecoveryPreparation::Failed { .. }
                            | DocumentRecoveryPreparation::Ambiguous { .. }
                            | DocumentRecoveryPreparation::RebaseFailed { .. }
                    ))
                })
                .sum::<usize>()
    }

    pub(super) fn render_recovery_notifications_button(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<Popover> {
        let count = self.recovery_attention_count(cx);
        let open = self.recovery_notifications_open && count > 0;
        crate::overlay_state::sync_overlay_open("recovery-notifications", open, cx);
        if count == 0 {
            return None;
        }
        let owner = cx.entity().downgrade();
        let content_owner = owner.clone();
        let focus = self.recovery_notifications_focus.clone();
        Some(
            Popover::new("recovery-notifications")
                .anchor(Anchor::TopRight)
                .open(open)
                .on_open_change(move |open, _, cx| {
                    let _ = owner.update(cx, |workspace, cx| {
                        workspace.recovery_notifications_open = *open;
                        cx.notify();
                    });
                })
                .trigger(BadgedTrigger {
                    button: crate::accessible_button::accessible_icon_button(
                        Button::new(RECOVERY_NOTIFICATIONS_BUTTON_ID)
                            .debug_selector(|| RECOVERY_NOTIFICATIONS_BUTTON_ID.into())
                            .accessibility_id(RECOVERY_NOTIFICATIONS_BUTTON_ID)
                            .icon(Icon::new(IconName::Info).text_color(cx.theme().warning))
                            .outline()
                            .label("Recovery")
                            .selected(open),
                        format!("Recovery ({count})"),
                    ),
                    count,
                })
                .content(move |_, window, cx| {
                    let popover = cx.entity();
                    let content = content_owner
                        .update(cx, |workspace, cx| {
                            workspace
                                .render_recovery_notification_content(cx)
                                .into_any_element()
                        })
                        .ok();
                    v_flex()
                        .id(RECOVERY_NOTIFICATIONS_PANEL_ID)
                        .debug_selector(|| RECOVERY_NOTIFICATIONS_PANEL_ID.into())
                        .w(gpui::rems(26.))
                        .max_w(window.viewport_size().width - px(32.))
                        .max_h(window.viewport_size().height - px(128.))
                        .min_w_0()
                        .whitespace_normal()
                        .gap_3()
                        .focus_trap("recovery-notifications-focus", &focus)
                        .child(popover_header(
                            "Recovery",
                            close_icon_button(RECOVERY_NOTIFICATIONS_CLOSE_ID, "Close recovery")
                                .on_click(move |_, window, cx| {
                                    popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                                }),
                        ))
                        .child(
                            gpui::div()
                                .id("recovery-notifications-scroll")
                                .w_full()
                                .min_w_0()
                                .min_h_0()
                                .flex_1()
                                .overflow_y_scroll()
                                .when_some(content, |body, content| body.child(content)),
                        )
                }),
        )
    }

    fn render_global_recovery_notices(
        &self,
        column: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        column
            .when_some(self.startup_recovery_message(), |root, message| {
                root.child(
                    gpui::div()
                        .w_full()
                        .p_2()
                        .child(self.render_startup_recovery_inbox(message, cx)),
                )
            })
            .when_some(
                self.session_recovery_warning.clone().filter(|_| {
                    self.startup_recovery_items.is_empty() && !self.startup_recovery_inspecting
                }),
                |root, warning| {
                    root.child(
                        v_flex()
                            .w_full()
                            .gap_2()
                            .p_2()
                            .child(
                                gpui::div()
                                    .id(DOCUMENT_RECOVERY_WARNING_ALERT_ID)
                                    .debug_selector(|| DOCUMENT_RECOVERY_WARNING_ALERT_ID.into())
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        Alert::warning(
                                            "document-workspace-recovery-warning-message",
                                            warning,
                                        )
                                        .title("Previous unsaved changes"),
                                    ),
                            )
                            .child(
                                Button::new(DOCUMENT_RECOVERY_WARNING_DISMISS_ID)
                                    .debug_selector(|| DOCUMENT_RECOVERY_WARNING_DISMISS_ID.into())
                                    .accessibility_id(DOCUMENT_RECOVERY_WARNING_DISMISS_ID)
                                    .outline()
                                    .label("Mark reviewed")
                                    .on_click(cx.listener(|workspace, _, _, cx| {
                                        workspace.dismiss_session_recovery_warning(cx);
                                    })),
                            ),
                    )
                },
            )
    }

    fn render_recovery_notification_content(&self, cx: &mut Context<Self>) -> gpui::Div {
        let mut body = self.render_global_recovery_notices(
            v_flex().w_full().min_w_0().whitespace_normal().gap_3(),
            cx,
        );
        for entity in &self.sessions {
            let session = entity.read(cx);
            let save_failure = match &session.save_status {
                NativeDocumentSaveStatus::Failed(failure) => Some(failure.clone()),
                _ => None,
            };
            let recovery_preparation_issue = match &session.recovery_preparation {
                DocumentRecoveryPreparation::Failed { message, .. } => {
                    Some((message.clone(), false, false))
                }
                DocumentRecoveryPreparation::Ambiguous { message, .. } => {
                    Some((message.clone(), true, false))
                }
                DocumentRecoveryPreparation::RebaseFailed { message, .. } => {
                    Some((message.clone(), false, true))
                }
                _ => None,
            };
            if save_failure.is_none() && recovery_preparation_issue.is_none() {
                continue;
            }
            let document_id = session.id;
            let title = session.title.clone();
            let has_save_failure = save_failure.is_some();
            let document_body = v_flex()
                .w_full()
                .min_w_0()
                .whitespace_normal()
                .gap_2()
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .gap_2()
                        .items_start()
                        .child(
                            gpui::div()
                                .flex_1()
                                .min_w_0()
                                .whitespace_normal()
                                .font_semibold()
                                .child(title.clone()),
                        )
                        .when(has_save_failure, |header| {
                            header.child(
                                close_icon_button(
                                    DOCUMENT_SAVE_ERROR_DISMISS_ID,
                                    "Close save error",
                                )
                                .debug_selector(|| DOCUMENT_SAVE_ERROR_DISMISS_ID.into())
                                .accessibility_id(DOCUMENT_SAVE_ERROR_DISMISS_ID)
                                .on_click(cx.listener(
                                    move |workspace, _, _, cx| {
                                        workspace.dismiss_document_save_failure(document_id, cx);
                                    },
                                )),
                            )
                        }),
                );
            let document_body = self.render_document_alerts(
                document_body,
                DocumentAlerts {
                    document_id,
                    text_box_commit_error: None,
                    save_failure,
                    save_failure_title: "Couldn’t save this document".into(),
                    recovery_preparation_issue,
                    presentation_error: None,
                    recovery_pending: false,
                },
                true,
                cx,
            );
            body = body.child(
                gpui::div()
                    .id(("recovery-document", document_id.value()))
                    .w_full()
                    .min_w_0()
                    .child(document_body),
            );
        }
        body
    }
}

/// A button with the stock count Badge on its corner, usable as a Popover's
/// trigger: selection passes to the button.
#[derive(IntoElement)]
struct BadgedTrigger {
    button: Button,
    count: usize,
}

impl gpui_component::Selectable for BadgedTrigger {
    fn selected(mut self, selected: bool) -> Self {
        self.button = self.button.selected(selected);
        self
    }

    fn is_selected(&self) -> bool {
        self.button.is_selected()
    }
}

impl gpui::RenderOnce for BadgedTrigger {
    fn render(self, _: &mut gpui::Window, _: &mut gpui::App) -> impl IntoElement {
        gpui_component::badge::Badge::new()
            .count(self.count)
            .child(self.button)
    }
}
