//! The workspace with no active document: open/new actions, inactive tabs
//! and the opening or error status.

use super::tabs::session_tab_track;
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
        let file_manager = crate::system_apps::file_manager_label();
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
                let full_path = crate::recent_documents::display_path(path);
                let folder = path
                    .parent()
                    .map(crate::recent_documents::display_path)
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
                                // A file moved or deleted since the list was checked updates it now.
                                workspace
                                    .recent_documents
                                    .update(cx, |history, cx| history.refresh(cx));
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
                            .tooltip(format!("Show in {file_manager}"))
                            .on_click(move |_, _, cx| {
                                crate::system_apps::reveal(&reveal_path, cx);
                            }),
                        format!("Show {full_path} in {file_manager}"),
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

    /// The New Tab, after the document tabs, while one is open.
    pub(super) fn render_new_tab(&mut self, cx: &mut Context<Self>) -> Option<Tab> {
        if !self.new_tab_open {
            return None;
        }
        let reveal_close = self.new_tab_hovered;
        let close_control = cx.entity().downgrade();
        let close_label = document_tab_close_accessible_label("New Tab");
        let new_tab_bounds = self.new_tab_bounds.clone();
        let close = accessible_icon_button(
            Button::new(DOCUMENT_NEW_TAB_CLOSE_ID)
                .opacity(if reveal_close { 1. } else { 0. })
                .focus(|style| style.opacity(1.))
                .debug_selector(|| DOCUMENT_NEW_TAB_CLOSE_ID.into())
                .accessibility_id(DOCUMENT_NEW_TAB_CLOSE_ID)
                .ghost()
                .xsmall()
                .icon(IconName::Close)
                .tooltip(close_label.clone())
                .on_click(move |_: &ClickEvent, _, cx| {
                    let _ = close_control.update(cx, |workspace, cx| workspace.close_new_tab(cx));
                    cx.stop_propagation();
                }),
            close_label,
        );
        Some(
            Tab::new()
                .min_w(crate::layout_metrics::scaled(
                    DOCUMENT_SESSION_TAB_MIN_WIDTH,
                ))
                // The New Tab is the selected tab while it shows.
                .child(session_tab_frame(DOCUMENT_NEW_TAB_ID, true, false, cx))
                .on_hover(cx.listener(|workspace, hovered: &bool, _, cx| {
                    if workspace.new_tab_hovered != *hovered {
                        workspace.new_tab_hovered = *hovered;
                        cx.notify();
                    }
                }))
                .debug_selector(|| DOCUMENT_NEW_TAB_ID.into())
                .group(DOCUMENT_NEW_TAB_ID)
                .child(
                    canvas(
                        move |bounds, _, _| new_tab_bounds.set(bounds),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .inset_0(),
                )
                .child(session_tab_overlay_label(
                    "New Tab".into(),
                    "New Tab".into(),
                    DOCUMENT_NEW_TAB_ID.into(),
                    reveal_close,
                    false,
                    cx,
                ))
                .aria_label("New Tab")
                .child(session_tab_close_lane().child(close)),
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
        // Recovery and Update, as in the document strip.
        let (has_extras, strip_end) = self.render_tab_strip_end(cx);
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
                // Keyboard focus, not the focus a click gives for the arrow
                // keys, reveals the close button and the ring.
                let keyboard_focus =
                    tab_focus.contains_focused(window, cx) && window.last_input_was_keyboard();
                let reveal_close = self.session_tab_hovered == Some(document_id) || keyboard_focus;
                let close = accessible_icon_button(
                    Button::new(close_id)
                        .opacity(if reveal_close { 1. } else { 0. })
                        .focus(|style| style.opacity(1.))
                        .debug_selector(move || close_selector.clone())
                        .accessibility_id(close_accessibility_id)
                        .ghost()
                        .xsmall()
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
                let tab_bounds = self
                    .session_tab_bounds
                    .get(&document_id)
                    .expect("every retained session tab must own a bounds cell")
                    .clone();
                let bounds_trace =
                    canvas(move |bounds, _, _| tab_bounds.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .inset_0();
                Tab::new()
                    // Not selected while the New Tab page shows.
                    .child(session_tab_frame(
                        tab_selector.clone(),
                        false,
                        keyboard_focus,
                        cx,
                    ))
                    .child(bounds_trace)
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
                        visual_title,
                        title.clone(),
                        tab_selector,
                        reveal_close,
                        *dirty,
                        cx,
                    ))
                    .aria_label(accessibility_label)
                    .aria_description(DOCUMENT_TAB_REORDER_DESCRIPTION)
                    .aria_keyshortcuts(DOCUMENT_TAB_REORDER_KEYSHORTCUTS)
                    .track_focus(&tab_focus.tab_stop(ix == 0))
                    .child(session_tab_close_lane().child(close))
            })
            .collect::<Vec<_>>();
        let new_tab = self.render_new_tab(cx);
        let has_tabs = !inactive_session_tabs.is_empty() || new_tab.is_some();
        let inactive_tab_click_control = cx.entity().downgrade();
        let inactive_tab_click_ids = session_tabs
            .iter()
            .map(|(document_id, _, _, _)| *document_id)
            .collect::<Vec<_>>();
        let inactive_tab_focus_handles = self.session_tab_focus_handles.clone();
        // With documents open, a New Tab sits in the document strip, where tabs
        // keep dragging, keyboard reordering and the overflow menu.
        let session_tab_strip = if (self.new_tab_open || self.new_tab_opening_shown().is_some())
            && !session_tabs.is_empty()
        {
            self.render_session_tab_strip(session_tabs.clone(), window, cx)
        } else {
            h_flex()
                .id(DOCUMENT_SESSION_TABS_ID)
                .debug_selector(|| DOCUMENT_SESSION_TABS_ID.into())
                .w_full()
                // A bar: 8 + 32 + 8, its line drawn inside.
                .h(crate::layout_metrics::scaled(crate::layout_metrics::BAR))
                .flex_shrink_0()
                .items_stretch()
                .relative()
                .child(
                    gpui::div()
                        .id("document-workspace-native-session-tab-bar")
                        .debug_selector(|| "document-workspace-native-session-tab-bar".into())
                        .flex_1()
                        .flex()
                        .items_stretch()
                        .when(has_extras, |strip| strip.border_r_1())
                        .border_color(cx.theme().border)
                        .min_w_0()
                        .max_w_full()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|workspace, event: &MouseDownEvent, _, cx| {
                                workspace.open_new_tab_from_tab_bar(event, cx);
                            }),
                        )
                        .child(session_tab_track(
                            // Stock segmented tabs, as the loaded strip. With
                            // no tabs, the actions start the strip.
                            has_tabs.then(|| {
                                TabBar::new("document-workspace-session-tabs-component")
                                    .segmented()
                                    .large()
                                    .tab_height(px(SESSION_TAB_HEIGHT))
                                    .inset(px(SESSION_TAB_ROW_GAP))
                                    .tab_gap(px(SESSION_TAB_ROW_GAP))
                                    .min_w_0()
                                    .max_width(px(190.))
                                    // Use stock scrolling, not the always-visible optional tab menu.
                                    .menu(false)
                                    .children(inactive_session_tabs)
                                    .children(new_tab)
                                    .on_click(move |tab_ix, window, cx| {
                                        let focus = inactive_tab_click_control
                                            .update(cx, |workspace, cx| {
                                                let &document_id =
                                                    inactive_tab_click_ids.get(*tab_ix)?;
                                                let _ =
                                                    workspace.activate_document(document_id, cx);
                                                inactive_tab_focus_handles
                                                    .get(&document_id)
                                                    .cloned()
                                            })
                                            .ok()
                                            .flatten();
                                        if let Some(focus) = focus {
                                            focus.focus(window, cx);
                                        }
                                    })
                            }),
                            self.render_session_tab_actions(None, has_tabs, cx),
                            false,
                            cx,
                        )),
                )
                .child(strip_end)
                .child(crate::layout_metrics::inside_line(
                    crate::layout_metrics::Edge::Bottom,
                    cx.theme().border,
                ))
        };
        root.child(session_tab_strip).child(
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .items_stretch()
                .child(self.render_left_rail(false, true, cx))
                .child(
                    v_flex()
                        .relative()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        // No view controls without a page to view.
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
                                            .w(crate::layout_metrics::scaled(240.))
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
                                    |view| view.child("Open a PDF or create a document to start"),
                                )
                                // An empty window offers both ways to start: an
                                // existing PDF, or fresh paper from the last
                                // template used.
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            Button::new(VIEWPORT_OPEN_DOCUMENT_ID)
                                                .debug_selector(|| VIEWPORT_OPEN_DOCUMENT_ID.into())
                                                .label("Open…")
                                                .on_click(cx.listener(|workspace, _, _, cx| {
                                                    workspace.prompt_to_open_documents(cx);
                                                })),
                                        )
                                        .child(
                                            Button::new(VIEWPORT_NEW_DOCUMENT_ID)
                                                .debug_selector(|| VIEWPORT_NEW_DOCUMENT_ID.into())
                                                .label(
                                                    self.template_control
                                                        .read(cx)
                                                        .last_template_label(),
                                                )
                                                .on_click(cx.listener(|workspace, _, _, cx| {
                                                    workspace.template_control.update(
                                                        cx,
                                                        |control, cx| {
                                                            control.create_from_last_template(cx)
                                                        },
                                                    );
                                                })),
                                        ),
                                )
                                .when_some(recent_documents, |view, list| view.child(list))
                                .when_some(open_failure, |view, failure| {
                                    view.child(
                                        gpui::div()
                                            .w(gpui::rems(32.))
                                            .max_w(window.viewport_size().width - px(32.))
                                            .min_w_0()
                                            .whitespace_normal()
                                            .child(self.document_open_failure_alert(&failure, cx)),
                                    )
                                })
                                .when_some(error, |view, error| {
                                    view.child(
                                        gpui::div()
                                            .id(DOCUMENT_ERROR_ID)
                                            .debug_selector(|| DOCUMENT_ERROR_ID.into())
                                            .child(gpui_component::alert::Alert::error(
                                                "document-error-alert",
                                                error,
                                            )),
                                    )
                                }),
                        )
                        .children(toast_layer(window, cx)),
                ),
        )
    }
}
