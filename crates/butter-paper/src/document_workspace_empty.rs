//! The workspace with no active document: open/new actions, inactive tabs
//! and the opening or error status.

use super::*;

fn recent_document_path_tooltip(mut button: Button, path: String) -> Button {
    button.interactivity().tooltip(move |window, cx| {
        let path = path.clone();
        gpui_component::tooltip::Tooltip::element(move |window, _| {
            gpui::div()
                .w((window.viewport_size().width - px(64.)).min(window.rem_size() * 24.))
                .min_w_0()
                .whitespace_normal()
                .child(path.clone())
        })
        .build(window, cx)
    });
    button
}

impl DocumentWorkspace {
    fn render_recent_documents(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Div> {
        let paths = self.recent_documents.read(cx).paths();
        if paths.is_empty() {
            return None;
        }
        let rows = paths
            .iter()
            .map(|path| {
                let id = crate::recent_documents::recent_document_id(path);
                let selector = id.clone();
                let folder_selector = format!("{id}-folder");
                let name = path
                    .file_name()
                    .unwrap_or(path.as_os_str())
                    .to_string_lossy()
                    .into_owned();
                let full_path = path.to_string_lossy().into_owned();
                let folder = path
                    .parent()
                    .map(|folder| folder.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let open_path = path.clone();
                let reveal_path = path.clone();
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_2()
                    .child(accessible_icon_button(
                        Button::new(id.clone())
                            .debug_selector(move || selector.clone())
                            .ghost()
                            .flex_1()
                            .min_w_0()
                            .h_auto()
                            .py_2()
                            .justify_start()
                            .map(|button| recent_document_path_tooltip(button, full_path.clone()))
                            .child(
                                v_flex()
                                    .min_w_0()
                                    .flex_1()
                                    .items_start()
                                    .gap_1()
                                    .child(gpui::div().w_full().truncate().text_sm().child(name))
                                    .child(
                                        gpui::div()
                                            .w_full()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .text_ellipsis_middle()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(folder),
                                    ),
                            )
                            .on_click(cx.listener(move |workspace, _, _, cx| {
                                workspace.open_documents(
                                    DocumentOpenBatchRequest::new(
                                        DocumentOpenOrigin::Menu,
                                        [open_path.clone()],
                                    ),
                                    cx,
                                );
                            })),
                        format!("Open {full_path}"),
                    ))
                    .child(accessible_icon_button(
                        Button::new(format!("{id}-folder"))
                            .debug_selector(move || folder_selector.clone())
                            .ghost()
                            .icon(IconName::FolderOpen)
                            .tooltip("Show in folder")
                            .on_click(move |_, _, cx| cx.reveal_path(&reveal_path)),
                        format!("Show {full_path} in folder"),
                    ))
            })
            .collect::<Vec<_>>();
        Some(
            v_flex()
                .w(gpui::rems(32.))
                .max_w_full()
                .min_w_0()
                .gap_2()
                .mt_4()
                .child(
                    gpui::div()
                        .px_2()
                        .text_sm()
                        .font_semibold()
                        .child("Recently opened"),
                )
                .child(
                    v_flex()
                        .id("recent-documents-list")
                        .debug_selector(|| "recent-documents-list".into())
                        .w_full()
                        .min_w_0()
                        .max_h(
                            (window.viewport_size().height - window.rem_size() * 20.)
                                .max(window.rem_size() * 4.),
                        )
                        .overflow_y_scroll()
                        .children(rows),
                ),
        )
    }

    pub(super) fn render_without_document(
        &mut self,
        root: gpui::Stateful<gpui::Div>,
        session_tabs: Vec<(DocumentId, String, bool, bool)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let recent_documents = self.render_recent_documents(window, cx);
        let opening_title = self.sessions.iter().find_map(|session| {
            let session = session.read(cx);
            matches!(session.status, NativeDocumentStatus::Opening).then(|| session.title.clone())
        });
        let open_failure = self.last_document_open_failure().cloned();
        let error = self.last_file_error.clone().or_else(|| {
            self.sessions.iter().rev().find_map(|session| {
                let session = session.read(cx);
                if let NativeDocumentStatus::Failed(error) = &session.status {
                    Some(error.clone())
                } else {
                    None
                }
            })
        });
        let open_document = accessible_icon_button(
            Button::new(DOCUMENT_TAB_OPEN_ID)
                .debug_selector(|| DOCUMENT_TAB_OPEN_ID.into())
                .icon(IconName::Plus)
                .tooltip("Open PDF")
                .on_click(cx.listener(|workspace, _, _, cx| {
                    workspace.prompt_to_open_documents(cx);
                })),
            "Open PDF",
        );
        let document_actions = h_flex()
            .flex_shrink_0()
            .gap_2()
            .when_some(
                self.render_recovery_notifications_button(cx),
                |row, button| row.child(button),
            )
            .child(open_document)
            .child(self.template_control.clone())
            .children(
                cx.try_global::<crate::application_settings::ApplicationUpdatePresentation>()
                    .and_then(|presentation| {
                        crate::application_settings::update_button(presentation.state.status)
                    }),
            );
        let inactive_session_tabs = session_tabs
            .iter()
            .enumerate()
            .map(|(ix, (document_id, title, dirty, saving))| {
                let tab_selector = document_session_tab_id(*document_id);
                let debug_selector = tab_selector.clone();
                let tab_focus = self
                    .session_tab_focus_handles
                    .get(document_id)
                    .expect("every retained loading or failed session tab must own focus")
                    .clone();
                let close_id = document_session_close_id(*document_id);
                let close_selector = close_id.clone();
                let close_accessibility_id = close_id.clone();
                let close_control = cx.entity().downgrade();
                let document_id = *document_id;
                let close_label = document_tab_close_accessible_label(title);
                let accessibility_label = if *dirty {
                    format!("{title}, Unsaved changes")
                } else {
                    title.clone()
                };
                let visual_title = format_document_tab_label(title).to_owned();
                let reveal_close = self.session_tab_hovered == Some(document_id)
                    || tab_focus.contains_focused(window, cx);
                let close = accessible_icon_button(
                    Button::new(close_id)
                        .opacity(if reveal_close { 1. } else { 0. })
                        .focus(|style| style.opacity(1.))
                        .debug_selector(move || close_selector.clone())
                        .accessibility_id(close_accessibility_id)
                        .rounded(cx.theme().radius_full())
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .tooltip(close_label.clone())
                        .disabled(*saving)
                        .on_click(move |_: &ClickEvent, _, cx| {
                            let _ = close_control.update(cx, |workspace, cx| {
                                workspace.request_close_document(document_id, cx);
                            });
                            cx.stop_propagation();
                        }),
                    close_label,
                );
                Tab::new()
                    .on_hover(cx.listener(move |workspace, hovered, _, cx| {
                        let next = if *hovered {
                            Some(document_id)
                        } else {
                            workspace
                                .session_tab_hovered
                                .filter(|id| *id != document_id)
                        };
                        if workspace.session_tab_hovered != next {
                            workspace.session_tab_hovered = next;
                            cx.notify();
                        }
                    }))
                    .group(tab_selector.clone())
                    .debug_selector(move || debug_selector.clone())
                    .child(session_tab_overlay_label(
                        if *dirty {
                            format!("* {visual_title}")
                        } else {
                            visual_title
                        },
                        tab_selector,
                        reveal_close,
                        self.session_tab_label_offsets
                            .entry(document_id)
                            .or_default()
                            .clone(),
                    ))
                    .aria_label(accessibility_label)
                    .aria_description(DOCUMENT_TAB_REORDER_DESCRIPTION)
                    .aria_keyshortcuts(DOCUMENT_TAB_REORDER_KEYSHORTCUTS)
                    .track_focus(&tab_focus.tab_stop(ix == 0))
                    .child(session_tab_close_lane().child(close))
            })
            .collect::<Vec<_>>();
        let inactive_tab_click_control = cx.entity().downgrade();
        let inactive_tab_click_ids = session_tabs
            .iter()
            .map(|(document_id, _, _, _)| *document_id)
            .collect::<Vec<_>>();
        let inactive_tab_focus_handles = self.session_tab_focus_handles.clone();
        let session_tab_strip = h_flex()
            .id(DOCUMENT_SESSION_TABS_ID)
            .debug_selector(|| DOCUMENT_SESSION_TABS_ID.into())
            .w_full()
            .px_3()
            .items_stretch()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                gpui::div()
                    .id("document-workspace-native-session-tab-bar")
                    .debug_selector(|| "document-workspace-native-session-tab-bar".into())
                    .flex_1()
                    .py_2()
                    .pr_3()
                    // Preserve the pinned Tab's 32px frame plus py_2 even with no tabs.
                    .min_h(px(32.) + window.rem_size())
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .min_w_0()
                    .max_w_full()
                    .child(
                        TabBar::new("document-workspace-session-tabs-component")
                            .outline()
                            .max_width(px(190.))
                            // Use stock scrolling, not the always-visible optional tab menu.
                            .menu(false)
                            .children(inactive_session_tabs)
                            .on_click(move |tab_ix, window, cx| {
                                let focus = inactive_tab_click_control
                                    .update(cx, |workspace, cx| {
                                        let &document_id = inactive_tab_click_ids.get(*tab_ix)?;
                                        let _ = workspace.activate_document(document_id, cx);
                                        inactive_tab_focus_handles.get(&document_id).cloned()
                                    })
                                    .ok()
                                    .flatten();
                                if let Some(focus) = focus {
                                    focus.focus(window, cx);
                                }
                            }),
                    ),
            )
            .child(document_actions);
        root.child(session_tab_strip).child(
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_stretch()
                .child(self.render_left_rail(false, true, cx))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .child(self.viewer_toolbar.clone())
                        .child(
                            v_flex()
                                .id(DOCUMENT_EMPTY_ID)
                                .debug_selector(|| DOCUMENT_EMPTY_ID.into())
                                .flex_1()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .p_4()
                                .when_some(opening_title, |view, title| {
                                    let status = format!("Opening {title}");
                                    view.child(
                                        v_flex()
                                            .id(DOCUMENT_OPEN_STATUS_ID)
                                            .debug_selector(|| DOCUMENT_OPEN_STATUS_ID.into())
                                            .role(Role::Status)
                                            .aria_label(status.clone())
                                            .a11y_synthetic_children(|builder| {
                                                builder.parent_node().set_live(Live::Polite)
                                            })
                                            .w(px(240.))
                                            .items_center()
                                            .gap_2()
                                            .child(status)
                                            .child(
                                                gpui::div()
                                                    .id(DOCUMENT_OPEN_PROGRESS_ID)
                                                    .debug_selector(|| {
                                                        DOCUMENT_OPEN_PROGRESS_ID.into()
                                                    })
                                                    .w_full()
                                                    .child(
                                                        Progress::new(
                                                            "document-open-progress-component",
                                                        )
                                                        .loading(true),
                                                    ),
                                            ),
                                    )
                                })
                                .when(
                                    !self.sessions.iter().any(|session| {
                                        matches!(
                                            session.read(cx).status,
                                            NativeDocumentStatus::Opening
                                        )
                                    }),
                                    |view| view.child("Open a PDF to start"),
                                )
                                .child(
                                    Button::new(VIEWPORT_OPEN_DOCUMENT_ID)
                                        .debug_selector(|| VIEWPORT_OPEN_DOCUMENT_ID.into())
                                        .label("Open")
                                        .on_click(cx.listener(|workspace, _, _, cx| {
                                            workspace.prompt_to_open_documents(cx);
                                        })),
                                )
                                .when_some(recent_documents, |view, list| view.child(list))
                                .when_some(open_failure, |view, failure| {
                                    view.child(
                            v_flex()
                                .w(gpui::rems(32.)).max_w(window.viewport_size().width - px(32.)).min_w_0().whitespace_normal()
                                .gap_2()
                                .child(
                                    gpui::div()
                                        .id(DOCUMENT_OPEN_ERROR_ALERT_ID)
                                        .debug_selector(|| DOCUMENT_OPEN_ERROR_ALERT_ID.into())
                                        .child(
                                            Alert::error(
                                                "document-workspace-open-feedback-message",
                                                failure.presentation(),
                                            )
                                            .title("Some PDFs couldn’t be opened"),
                                        ),
                                )
                                .child(
                                    close_icon_button(DOCUMENT_OPEN_ERROR_DISMISS_ID, "Close opening error")
                                        .debug_selector(|| DOCUMENT_OPEN_ERROR_DISMISS_ID.into())
                                        .accessibility_id(DOCUMENT_OPEN_ERROR_DISMISS_ID)
                                        .on_click(cx.listener(|workspace, _, _, cx| {
                                            workspace.dismiss_document_open_failure(cx);
                                        })),
                                ),
                        )
                                })
                                .when_some(error, |view, error| {
                                    view.child(
                                        gpui::div()
                                            .id(DOCUMENT_ERROR_ID)
                                            .debug_selector(|| DOCUMENT_ERROR_ID.into())
                                            .text_sm()
                                            .text_color(cx.theme().danger)
                                            .child(error),
                                    )
                                }),
                        ),
                ),
        )
    }
}
