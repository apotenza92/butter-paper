//! The document tab strip: tabs, drag reordering and tear-off, overflow and
//! the pinned Open/New actions.

use super::*;

impl DocumentWorkspace {
    pub(super) fn render_session_tab_strip(
        &mut self,
        session_tabs: Vec<(DocumentId, String, bool, bool)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let selected_session_ix = self.active_document_id.and_then(|active_id| {
            session_tabs
                .iter()
                .position(|(document_id, _, _, _)| *document_id == active_id)
        });
        let session_tab_count = session_tabs.len();
        let session_tab_ids = session_tabs
            .iter()
            .map(|(document_id, _, _, _)| *document_id)
            .collect::<Vec<_>>();
        let session_drag_active = self
            .session_tab_pointer_drag
            .as_ref()
            .is_some_and(|drag| drag.activated);
        let overflow_items = session_tabs
            .iter()
            .map(|(id, title, _, _)| (*id, title.clone()))
            .collect::<Vec<_>>();
        let rendered_session_tabs = session_tabs.into_iter().enumerate().map(
                |(tab_ix, (tab_document_id, tab_title, dirty, saving))| {
                    let tab_id = document_session_tab_id(tab_document_id);
                    let tab_selector = tab_id.clone();
                    let tab_focus = self
                        .session_tab_focus_handles
                        .get(&tab_document_id)
                        .expect("every retained session tab must own a focus handle")
                        .clone();
                    let tab_bounds = self
                        .session_tab_bounds
                        .get(&tab_document_id)
                        .expect("every retained session tab must own a bounds cell")
                        .clone();
                    let is_active = self.active_document_id == Some(tab_document_id);
                    let publication_warning = self
                        .session(tab_document_id, cx)
                        .and_then(|session| {
                            session
                                .read(cx)
                                .publication_durability_warning()
                                .map(str::to_owned)
                        });
                    let is_dragged = self.session_tab_pointer_drag.as_ref().is_some_and(|drag| {
                        drag.activated && drag.document_id == tab_document_id
                    });
                    let incoming_before = self.incoming_tab_drop == Some(tab_ix);
                    let incoming_after = self.incoming_tab_drop == Some(tab_ix + 1)
                        && tab_ix + 1 == session_tab_count;
                    let is_drop_target = incoming_before
                        || incoming_after
                        || self.session_tab_pointer_drag.as_ref().is_some_and(|drag| {
                            drag.activated && drag.document_id != tab_document_id
                                && drag.over_document_id == tab_document_id
                                && self.session_tab_scroll.bounds().contains(&drag.current)
                        });
                    let drop_after = incoming_after
                        || (!incoming_before
                            && self.session_tab_pointer_drag.as_ref().is_some_and(|drag| {
                                session_tab_ids.iter().position(|id| *id == drag.document_id)
                                    .is_some_and(|source_ix| source_ix < tab_ix)
                            }));
                    let accessibility_label = if publication_warning.is_some() {
                        format!("{tab_title}, Saved with warning")
                    } else if dirty {
                        format!("{tab_title}, Unsaved changes")
                    } else {
                        tab_title.clone()
                    };
                    let bounds_trace = canvas(
                        move |bounds, _, _| {
                            if !session_drag_active {
                                tab_bounds.set(bounds);
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .bottom_0();
                    let drag_id = document_tab_drag_id(&tab_document_id.to_string());
                    let drag_selector = drag_id.clone();
                    let drag_trace = is_dragged.then(|| {
                        gpui::div()
                            .id(drag_id)
                            .debug_selector(move || drag_selector.clone())
                            .absolute()
                            .left_0()
                            .right_0()
                            .top_0()
                            .bottom_0()
                    });
                    let drop_id = document_tab_drop_target_id(&tab_document_id.to_string());
                    let drop_selector = drop_id.clone();
                    let drop_trace = is_drop_target.then(|| {
                        gpui::div()
                            .id(drop_id)
                            .debug_selector(move || drop_selector.clone())
                            .absolute()
                            .left_0()
                            .right_0()
                            .top_0()
                            .bottom_0()
                            .when(drop_after, |cue| cue.border_r_2())
                            .when(!drop_after, |cue| cue.border_l_2())
                            .border_color(cx.theme().primary)
                    });
                    let keyboard_control = cx.entity().downgrade();
                    let keyboard_ids = session_tab_ids.clone();
                    let keyboard_focus_handles = self.session_tab_focus_handles.clone();
                    let keyboard_tab_focus = tab_focus.clone();
                    let reveal_close = self.session_tab_hovered == Some(tab_document_id)
                        || tab_focus.contains_focused(window, cx);
                    let tab = Tab::new()
                        // Short titles still leave a grab area clear of the
                        // close button, as browsers keep a minimum tab width.
                        .min_w(px(DOCUMENT_SESSION_TAB_MIN_WIDTH))
                        .when(is_dragged, |tab| tab.opacity(0.6))
                        .on_hover(cx.listener(move |workspace, hovered, _, cx| {
                            let next = if *hovered { Some(tab_document_id) } else {
                                workspace.session_tab_hovered.filter(|id| *id != tab_document_id)
                            };
                            if workspace.session_tab_hovered != next {
                                workspace.session_tab_hovered = next;
                                cx.notify();
                            }
                        }))
                        .group(tab_id.clone())
                        .debug_selector(move || tab_selector.clone())
                        .child(session_tab_overlay_label(
                            format_document_tab_label(&tab_title).to_owned(),
                            tab_id.clone(), reveal_close, dirty, cx,
                        ))
                        .aria_label(accessibility_label)
                        .aria_description(DOCUMENT_TAB_REORDER_DESCRIPTION)
                        .aria_keyshortcuts(DOCUMENT_TAB_REORDER_KEYSHORTCUTS)
                        .child(bounds_trace)
                        .children(drag_trace)
                        .children(drop_trace)
                        .track_focus(&tab_focus.clone().tab_stop(is_active))
                        // Workspace navigation bindings dispatch actions before raw
                        // key handlers. Tabs must consume them at their own scope.
                        .on_action(cx.listener(move |workspace, _: &NavigateHome, window, cx| {
                            workspace.navigate_session_tab(tab_document_id, DocumentNavigationAction::Home, window, cx);
                        }))
                        .on_action(cx.listener(move |workspace, _: &NavigateEnd, window, cx| {
                            workspace.navigate_session_tab(tab_document_id, DocumentNavigationAction::End, window, cx);
                        }))
                        .on_action(cx.listener(move |workspace, _: &NavigateLeft, window, cx| {
                            workspace.navigate_session_tab(tab_document_id, DocumentNavigationAction::ArrowLeft, window, cx);
                        }))
                        .on_action(cx.listener(move |workspace, _: &NavigateRight, window, cx| {
                            workspace.navigate_session_tab(tab_document_id, DocumentNavigationAction::ArrowRight, window, cx);
                        }))
                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                            let modifiers = event.keystroke.modifiers;
                            if modifiers.alt && modifiers.shift {
                                let direction = match event.keystroke.key.as_str() {
                                    "left" => Some(-1),
                                    "right" => Some(1),
                                    _ => None,
                                };
                                let Some(direction) = direction else {
                                    return;
                                };
                                let moved = keyboard_control
                                    .update(cx, |workspace, cx| {
                                        workspace.move_document_session_by_keyboard(
                                            tab_document_id,
                                            direction,
                                            cx,
                                        )
                                    })
                                    .unwrap_or(false);
                                if moved {
                                    keyboard_tab_focus.focus(window, cx);
                                }
                                cx.stop_propagation();
                                return;
                            }
                            if modifiers.alt
                                || modifiers.control
                                || modifiers.platform
                                || session_tab_count == 0
                            {
                                return;
                            }
                            let target_ix = match event.keystroke.key.as_str() {
                                "left" => Some((tab_ix + session_tab_count - 1) % session_tab_count),
                                "right" => Some((tab_ix + 1) % session_tab_count),
                                "home" => Some(0),
                                "end" => Some(session_tab_count - 1),
                                _ => None,
                            };
                            let Some(target_ix) = target_ix else {
                                return;
                            };
                            let target_id = keyboard_ids[target_ix];
                            let target_focus = keyboard_focus_handles.get(&target_id).cloned();
                            let _ = keyboard_control.update(cx, |workspace, cx| {
                                workspace.activate_document(target_id, cx);
                            });
                            if let Some(target_focus) = target_focus {
                                target_focus.focus(window, cx);
                            }
                            cx.stop_propagation();
                        })
                        ;
                    let close_id = document_session_close_id(tab_document_id);
                    let close_selector = close_id.clone();
                    let close_accessibility_id = close_id.clone();
                    let close_label = document_tab_close_accessible_label(&tab_title);
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
                            .disabled(saving)
                            .on_click(move |_: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                            }),
                        close_label,
                    );
                    let close = if let Some(publication_warning) = publication_warning {
                        let confirmation_open =
                            self.pending_close_document_id == Some(tab_document_id);
                        let open_control = cx.entity().downgrade();
                        let content_control = cx.entity().downgrade();
                        let warning_title = tab_title.clone();
                        Popover::new(format!(
                            "{tab_document_id}-publication-warning-close-popover"
                        ))
                        .anchor(Anchor::TopRight)
                        .open(crate::overlay_state::sync_overlay_open(format!("workspace-tab-confirmation-{tab_document_id}"), confirmation_open, cx))
                        .overlay_closable(true)
                        .on_open_change(move |open, _, cx| {
                            let _ = open_control.update(cx, |workspace, cx| {
                                if *open {
                                    workspace.request_close_document(tab_document_id, cx);
                                } else {
                                    workspace.resolve_dirty_close_cancel(cx);
                                }
                            });
                        })
                        .w_80()
                        .trigger(close)
                        .content(move |_, _window, cx| {
                            let popover = cx.entity();
                            let cancel_popover = popover.clone();
                            let cancel_control = content_control.clone();
                            let continue_control = content_control.clone();
                            v_flex()
                                .id(DOCUMENT_PUBLICATION_WARNING_CLOSE_ID)
                                .debug_selector(|| DOCUMENT_PUBLICATION_WARNING_CLOSE_ID.into())
                                .w_full()
                                .gap_3()
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(gpui::div().font_semibold().child(format!(
                                            "“{warning_title}” was saved with a warning"
                                        )))
                                        .child(
                                            gpui::div()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!(
                                                    "The file will not be written again. Closing would remove its crash-recovery fallback even though directory durability was not confirmed. {publication_warning}"
                                                )),
                                        ),
                                )
                                .child(
                                    h_flex()
                                        .justify_end()
                                        .gap_2()
                                        .child(
                                            Button::new(
                                                DOCUMENT_PUBLICATION_WARNING_CLOSE_CANCEL_ID,
                                            )
                                            .debug_selector(|| {
                                                DOCUMENT_PUBLICATION_WARNING_CLOSE_CANCEL_ID.into()
                                            })
                                            .outline()
                                            .label("Keep open")
                                            .on_click(move |_: &ClickEvent, window, cx| {
                                                let _ = cancel_control.update(
                                                    cx,
                                                    |workspace, cx| {
                                                        workspace.resolve_dirty_close_cancel(cx);
                                                    },
                                                );
                                                cancel_popover.update(cx, |popover, cx| {
                                                    popover.dismiss(window, cx)
                                                });
                                            }),
                                        )
                                        .child(
                                            Button::new(
                                                DOCUMENT_PUBLICATION_WARNING_CLOSE_CONTINUE_ID,
                                            )
                                            .debug_selector(|| {
                                                DOCUMENT_PUBLICATION_WARNING_CLOSE_CONTINUE_ID
                                                    .into()
                                            })
                                            .primary()
                                            .label("Continue closing")
                                            .on_click(move |_: &ClickEvent, window, cx| {
                                                let successor_focus = continue_control
                                                    .update(cx, |workspace, cx| {
                                                        let disposition = workspace
                                                            .resolve_publication_warning_close(cx);
                                                        (disposition
                                                            == CloseRequestDisposition::Closed)
                                                            .then(|| {
                                                                workspace.active_document_id
                                                                    .and_then(|document_id| {
                                                                        workspace
                                                                            .session_tab_focus_handles
                                                                            .get(&document_id)
                                                                            .cloned()
                                                                    })
                                                            })
                                                            .flatten()
                                                    })
                                                    .ok()
                                                    .flatten();
                                                if let Some(successor_focus) = successor_focus {
                                                    successor_focus.focus(window, cx);
                                                } else {
                                                    window.refresh();
                                                }
                                            }),
                                        ),
                                )
                        })
                        .into_any_element()
                    } else if dirty {
                        let confirmation_open =
                            self.pending_close_document_id == Some(tab_document_id);
                        let open_control = cx.entity().downgrade();
                        let content_control = cx.entity().downgrade();
                        Popover::new(format!("{tab_document_id}-dirty-close-popover"))
                            .anchor(Anchor::TopRight)
                            .open(crate::overlay_state::sync_overlay_open(format!("workspace-tab-dirty-close-{tab_document_id}"), confirmation_open, cx))
                            .overlay_closable(!saving)
                            .on_open_change(move |open, _, cx| {
                                let _ = open_control.update(cx, |workspace, cx| {
                                    if *open {
                                        workspace.request_close_document(tab_document_id, cx);
                                    } else if !saving {
                                        workspace.resolve_dirty_close_cancel(cx);
                                    }
                                });
                            })
                            .w_80()
                            .trigger(close)
                            .content(move |_, _window, cx| {
                                let popover = cx.entity();
                                let cancel_popover = popover.clone();
                                let discard_popover = popover.clone();
                                let cancel_control = content_control.clone();
                                let discard_control = content_control.clone();
                                let save_control = content_control.clone();
                                v_flex()
                                    .id(DOCUMENT_DIRTY_CLOSE_ID)
                                    .debug_selector(|| DOCUMENT_DIRTY_CLOSE_ID.into())
                                    .w_full()
                                    .min_w_0()
                                    .whitespace_normal()
                                    .gap_3()
                                    .child(
                                        v_flex()
                                            .w_full()
                                            .min_w_0()
                                            .gap_1()
                                            .child(
                                                gpui::div()
                                                    .w_full()
                                                    .whitespace_normal()
                                                    .font_semibold()
                                                    .child(format!(
                                                        "Save changes to {tab_title}?"
                                                    )),
                                            )
                                            .child(
                                                gpui::div()
                                                    .id("dirty-close-explanation")
                                                    .debug_selector(|| "dirty-close-explanation".into())
                                                    .w_full()
                                                    .whitespace_normal()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child("Your changes will be lost if you close this tab without saving."),
                                            ),
                                    )
                                    .child(
                                        h_flex()
                                            .justify_end()
                                            .gap_2()
                                            .child(
                                                Button::new(DOCUMENT_DIRTY_CLOSE_CANCEL_ID)
                                                    .debug_selector(|| DOCUMENT_DIRTY_CLOSE_CANCEL_ID.into())
                                                    .outline()
                                                    .label("Cancel")
                                                    .disabled(saving)
                                                    .on_click(move |_: &ClickEvent, window, cx| {
                                                        let _ = cancel_control.update(cx, |workspace, cx| {
                                                            workspace.resolve_dirty_close_cancel(cx);
                                                        });
                                                        cancel_popover.update(cx, |popover, cx| {
                                                            popover.dismiss(window, cx)
                                                        });
                                                    }),
                                            )
                                            .child(
                                                Button::new(DOCUMENT_DIRTY_CLOSE_DISCARD_ID)
                                                    .debug_selector(|| DOCUMENT_DIRTY_CLOSE_DISCARD_ID.into())
                                                    .danger()
                                                    .label("Discard")
                                                    .disabled(saving)
                                                    .on_click(move |_: &ClickEvent, window, cx| {
                                                        let (resolution, successor_focus) = discard_control
                                                            .update(cx, |workspace, cx| {
                                                                let resolution = workspace.resolve_dirty_close_discard(cx);
                                                                let successor_focus = workspace
                                                                    .active_document_id
                                                                    .and_then(|document_id| {
                                                                        workspace
                                                                            .session_tab_focus_handles
                                                                            .get(&document_id)
                                                                            .cloned()
                                                                    });
                                                                (resolution, successor_focus)
                                                            })
                                                            .unwrap_or((
                                                                DirtyCloseResolution::NoPendingDocument,
                                                                None,
                                                            ));
                                                        if resolution == DirtyCloseResolution::Discarded {
                                                            discard_popover.update(cx, |popover, cx| {
                                                                popover.dismiss(window, cx)
                                                            });
                                                            if let Some(successor_focus) = successor_focus {
                                                                successor_focus.focus(window, cx);
                                                            }
                                                        } else {
                                                            window.refresh();
                                                        }
                                                    }),
                                            )
                                            .child(
                                                Button::new(DOCUMENT_DIRTY_CLOSE_SAVE_ID)
                                                    .debug_selector(|| DOCUMENT_DIRTY_CLOSE_SAVE_ID.into())
                                                    .primary()
                                                    .label(if saving { "Saving…" } else { "Save" })
                                                    .disabled(saving)
                                                    .on_click(move |_: &ClickEvent, _, cx| {
                                                        let _ = save_control.update(cx, |workspace, cx| {
                                                            workspace.prompt_to_save_pending_close(cx);
                                                        });
                                                    }),
                                            ),
                                    )
                            })
                            .into_any_element()
                    } else {
                        close
                            .on_click({
                                let control = cx.entity().downgrade();
                                move |_: &ClickEvent, window, cx| {
                                    let successor_focus = control
                                        .update(cx, |workspace, cx| {
                                            let _ = workspace
                                                .request_close_document(tab_document_id, cx);
                                            workspace.active_document_id.and_then(|document_id| {
                                                workspace
                                                    .session_tab_focus_handles
                                                    .get(&document_id)
                                                    .cloned()
                                            })
                                        })
                                        .ok()
                                        .flatten();
                                    if let Some(successor_focus) = successor_focus {
                                        successor_focus.focus(window, cx);
                                    }
                                    cx.stop_propagation();
                                }
                            })
                            .into_any_element()
                    };
                    let close_bounds = self
                        .session_tab_close_bounds
                        .get(&tab_document_id)
                        .expect("every retained session tab close must own a bounds cell")
                        .clone();
                    let close_bounds_trace = canvas(
                        move |bounds, _, _| close_bounds.set(bounds),
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .bottom_0();
                    tab.child(
                        session_tab_close_lane()
                            .child(h_flex().relative().child(close).child(close_bounds_trace)),
                    )
                },
            )
            .collect::<Vec<_>>();
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
        let recovery_button = self.render_recovery_notifications_button(cx);
        let update_button = cx
            .try_global::<crate::application_settings::ApplicationUpdatePresentation>()
            .and_then(|presentation| crate::application_settings::update_button(presentation, cx));
        // With three or more tool columns, Open and New sit above the rail's
        // last three columns, and the strip's divider continues the rail's
        // edge. Narrower rails cannot fit them, so they keep a padded row.
        let rail_columns = self.right_rail_columns;
        let aligned = rail_columns >= 3;
        let rem = window.rem_size();
        self.template_control.update(cx, |control, cx| {
            control.set_segment_width(template_segment_width(rail_columns), cx)
        });
        let has_extras = recovery_button.is_some() || update_button.is_some();
        let overflow_lane = self.session_tabs_overflow.then(|| {
            let control = cx.entity().downgrade();
            h_flex()
                .id("document-tabs-overflow-lane")
                .debug_selector(|| "document-tabs-overflow-lane".into())
                .flex_shrink_0()
                .when(aligned, |lane| lane.pl_3())
                .pr_3()
                .when(!aligned || has_extras, |lane| lane.border_r_1())
                .border_color(cx.theme().border)
                .child(
                    accessible_icon_button(
                        Button::new("document-tabs-overflow")
                            .debug_selector(|| "document-tabs-overflow".into())
                            .icon(IconName::ChevronDown)
                            .tooltip("Open documents"),
                        "Open documents",
                    )
                    .dropdown_menu(move |mut menu, _, _| {
                        menu = menu.scrollable(true);
                        for (ix, (id, title)) in overflow_items.iter().enumerate() {
                            let id = *id;
                            let control = control.clone();
                            menu = menu.item(
                                PopupMenuItem::new(title.clone())
                                    .checked(selected_session_ix == Some(ix))
                                    .on_click(move |_, window, cx| {
                                        let _ = control.update(cx, |workspace, cx| {
                                            let _ = workspace.activate_document(id, cx);
                                            if let Some(focus) =
                                                workspace.session_tab_focus_handles.get(&id)
                                            {
                                                focus.focus(window, cx);
                                            }
                                            cx.notify();
                                        });
                                    }),
                            );
                        }
                        menu
                    }),
                )
        });
        let document_actions = if aligned {
            let extras = has_extras.then(|| {
                h_flex()
                    .flex_shrink_0()
                    // Full height, so the update lane's divider spans the strip.
                    .self_stretch()
                    .px_3()
                    .gap_2()
                    .children(recovery_button)
                    .children(update_button)
            });
            let start = (rail_columns - 2) as i16;
            h_flex().flex_shrink_0().children(extras).child(
                h_flex()
                    .id(DOCUMENT_WORKSPACE_TAB_ACTIONS_ID)
                    .debug_selector(|| DOCUMENT_WORKSPACE_TAB_ACTIONS_ID.into())
                    .flex_none()
                    .self_stretch()
                    .w(right_rail_width(rail_columns, rem))
                    .border_l_1()
                    .border_color(cx.theme().border)
                    .justify_center()
                    .child(
                        gpui::div()
                            .grid()
                            .w(gpui::rems(2.5 * rail_columns as f32 - 0.5))
                            .grid_cols(rail_columns as u16)
                            .gap_2()
                            .items_center()
                            .child(gpui::div().col_start(start).child(open_document))
                            .child(
                                gpui::div()
                                    .col_start(start + 1)
                                    .col_span(2)
                                    .child(self.template_control.clone()),
                            ),
                    ),
            )
        } else {
            h_flex()
                .flex_shrink_0()
                .gap_2()
                .children(recovery_button)
                .child(open_document)
                .child(self.template_control.clone())
                .children(update_button)
        };
        let tab_bar_click_control = cx.entity().downgrade();
        let tab_bar_click_ids = session_tab_ids.clone();
        let tab_bar_focus_handles = self.session_tab_focus_handles.clone();
        let native_session_tabs = TabBar::new("document-workspace-session-tabs-component")
            .outline()
            .track_scroll(&self.session_tab_scroll)
            .max_width(px(190.))
            // The pinned option is unconditional; the application supplies the
            // measured overflow state rather than enabling it for every document.
            .menu(false)
            .children(rendered_session_tabs)
            .on_click(move |tab_ix, window, cx| {
                let focus = tab_bar_click_control
                    .update(cx, |workspace, cx| {
                        let &document_id = tab_bar_click_ids.get(*tab_ix)?;
                        let active_drag = workspace
                            .session_tab_pointer_drag
                            .as_ref()
                            .is_some_and(|drag| drag.activated && drag.document_id == document_id);
                        if active_drag || workspace.take_suppressed_session_tab_click(document_id) {
                            cx.stop_propagation();
                            return None;
                        }
                        let _ = workspace.activate_document(document_id, cx);
                        tab_bar_focus_handles.get(&document_id).cloned()
                    })
                    .ok()
                    .flatten();
                if let Some(focus) = focus {
                    focus.focus(window, cx);
                }
            })
            .when_some(selected_session_ix, |tabs, selected_ix| {
                tabs.selected_index(selected_ix)
            });
        let reorder_announcement = self.session_tab_reorder_announcement.clone();
        let reorder_status = gpui::div()
            .id(DOCUMENT_TAB_REORDER_STATUS_ID)
            .debug_selector(|| DOCUMENT_TAB_REORDER_STATUS_ID.into())
            .accessibility_id(DOCUMENT_TAB_REORDER_STATUS_ID)
            .role(Role::Status)
            .aria_label(reorder_announcement)
            .a11y_synthetic_children(|builder| builder.parent_node().set_live(Live::Polite))
            .absolute()
            .left_0()
            .bottom_0()
            .size(px(1.))
            .overflow_hidden()
            .opacity(0.);
        let down_control = cx.entity().downgrade();
        let move_control = cx.entity().downgrade();
        let up_control = cx.entity().downgrade();
        let wheel_scroll = self.session_tab_scroll.clone();
        let wheel_control = cx.entity().downgrade();
        let pointer_event_bridge = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture
                        || !wheel_scroll.bounds().contains(&event.position)
                    {
                        return;
                    }
                    let delta = event.delta.pixel_delta(px(16.));
                    let max = wheel_scroll.max_offset().x;
                    if max > px(0.) {
                        wheel_scroll.set_offset(point(
                            session_tab_wheel_offset(wheel_scroll.offset().x, max, delta),
                            px(0.),
                        ));
                        let _ = wheel_control.update(cx, |_, cx| cx.notify());
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                });
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture
                        || event.button != MouseButton::Left
                        || crate::overlay_state::press_owned_by_overlay(window, cx)
                    {
                        return;
                    }
                    let focus = down_control
                        .update(cx, |workspace, cx| {
                            // Only a visible tab can be grabbed: tabs scrolled
                            // under the overflow control keep their bounds.
                            if !workspace
                                .session_tab_scroll
                                .bounds()
                                .contains(&event.position)
                            {
                                return None;
                            }
                            let document_id = workspace.sessions.iter().find_map(|session| {
                                let document_id = session.read(cx).id;
                                let bounds = workspace.session_tab_bounds.get(&document_id)?.get();
                                let close_bounds =
                                    workspace.session_tab_close_bounds.get(&document_id)?.get();
                                (bounds.contains(&event.position)
                                    && !close_bounds.contains(&event.position))
                                .then_some(document_id)
                            })?;
                            let focus = workspace
                                .session_tab_focus_handles
                                .get(&document_id)
                                .cloned();
                            workspace
                                .begin_session_tab_pointer_drag(document_id, event.position, cx)
                                .then_some(focus)
                                .flatten()
                        })
                        .ok()
                        .flatten();
                    if let Some(focus) = focus {
                        focus.focus(window, cx);
                    }
                });
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    if event.pressed_button != Some(MouseButton::Left) {
                        let _ = move_control.update(cx, |workspace, cx| {
                            workspace.cancel_session_tab_pointer_drag(cx);
                        });
                        return;
                    }
                    let activated = move_control
                        .update(cx, |workspace, cx| {
                            workspace.update_session_tab_pointer_drag(event.position, cx)
                        })
                        .unwrap_or(false);
                    if activated {
                        window.prevent_default();
                    }
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        let _ = up_control.update(cx, |workspace, cx| {
                            workspace.update_session_tab_pointer_drag(event.position, cx);
                            workspace.finish_session_tab_pointer_drag(cx);
                        });
                    }
                });
            },
        )
        .absolute()
        .left_0()
        .bottom_0()
        .size(px(1.));
        let cancel_drag_control = cx.entity().downgrade();
        let overflow_control = cx.entity().downgrade();
        let overflow_scroll = self.session_tab_scroll.clone();
        let overflow_bounds = self
            .session_tab_bounds
            .values()
            .chain(self.session_tab_close_bounds.values())
            .cloned()
            .collect::<Vec<_>>();
        let strip_bounds = self.session_tab_strip_bounds.clone();
        // Browser-style: while a tab is dragged, a copy follows the pointer,
        // including beyond the strip where a drop moves it to another window.
        let dragged_tab_ghost = self
            .session_tab_pointer_drag
            .as_ref()
            .filter(|drag| {
                drag.activated
                    && !(self.external_tab_drag_preview
                        && !self.session_tab_strip_bounds.get().contains(&drag.current))
            })
            .and_then(|drag| {
                let title = self.session(drag.document_id, cx)?.read(cx).title.clone();
                // Only a window with other tabs can tear one off.
                let outside = self.sessions.len() > 1
                    && !self.session_tab_strip_bounds.get().contains(&drag.current);
                Some((
                    drag.current,
                    format_document_tab_label(&title).to_owned(),
                    outside,
                ))
            })
            .map(|(position, label, outside)| {
                gpui::deferred(
                    gpui::anchored()
                        .position(position - point(px(24.), px(14.)))
                        .child(
                            gpui::div()
                                .id(DOCUMENT_TAB_DRAG_GHOST_ID)
                                .debug_selector(|| DOCUMENT_TAB_DRAG_GHOST_ID.into())
                                .px_3()
                                .h(px(28.))
                                .flex()
                                .items_center()
                                .gap_2()
                                .rounded_md()
                                .border_1()
                                .border_color(cx.theme().border)
                                .bg(cx.theme().background)
                                .shadow_lg()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .opacity(0.92)
                                .child(if outside {
                                    format!("Open {label} in New window")
                                } else {
                                    label
                                }),
                        ),
                )
                .with_priority(2)
            });
        h_flex()
            .id(DOCUMENT_SESSION_TABS_ID)
            .debug_selector(|| DOCUMENT_SESSION_TABS_ID.into())
            .w_full()
            .pl_3()
            // Aligned actions meet the strip directly, so the divider lands
            // on the rail's edge.
            .when(!aligned, |strip| strip.pr_3().gap_3())
            .items_stretch()
            .border_b_1()
            .border_color(cx.theme().border)
            .relative()
            .child(
                canvas(
                    move |bounds, _, _| strip_bounds.set(bounds),
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .children(dragged_tab_ghost)
            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape"
                    && cancel_drag_control
                        .update(cx, |workspace, cx| {
                            workspace.cancel_session_tab_pointer_drag(cx)
                        })
                        .unwrap_or(false)
                {
                    cx.stop_propagation();
                }
            })
            .child(
                gpui::div()
                    .id("document-workspace-native-session-tab-bar")
                    .debug_selector(|| "document-workspace-native-session-tab-bar".into())
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .max_w_full()
                    .py_2()
                    .pr_3()
                    .overflow_hidden()
                    .when(
                        !aligned || self.session_tabs_overflow || has_extras,
                        |strip| strip.border_r_1(),
                    )
                    .border_color(cx.theme().border)
                    .on_prepaint(move |_, _, cx| {
                        // Read after child prepaint has refreshed the tab bounds.
                        let overflow_scroll = overflow_scroll.clone();
                        cx.defer(move |cx| {
                            let measured = overflow_bounds
                                .iter()
                                .map(|bounds| bounds.get())
                                .filter(|bounds| bounds.size.width > px(0.))
                                .collect::<Vec<_>>();
                            let overflow = session_tab_strip_overflows(
                                &measured,
                                overflow_scroll.bounds().size.width,
                            );
                            let _ = overflow_control.update(cx, |workspace, cx| {
                                if let Some(id) = workspace.session_tab_reveal.take()
                                    && let Some(tab) = workspace.session_tab_bounds.get(&id)
                                {
                                    let tab = tab.get();
                                    let right = workspace
                                        .session_tab_close_bounds
                                        .get(&id)
                                        .map(|bounds| bounds.get().right())
                                        .unwrap_or(tab.right())
                                        .max(tab.right());
                                    let viewport = workspace.session_tab_scroll.bounds();
                                    let correction = if tab.left() < viewport.left() {
                                        viewport.left() - tab.left()
                                    } else if right > viewport.right() {
                                        viewport.right() - right
                                    } else {
                                        px(0.)
                                    };
                                    if correction != px(0.) {
                                        let offset = workspace.session_tab_scroll.offset();
                                        let max = workspace.session_tab_scroll.max_offset().x;
                                        workspace.session_tab_scroll.set_offset(point(
                                            (offset.x + correction).clamp(-max, px(0.)),
                                            px(0.),
                                        ));
                                        cx.notify();
                                    }
                                }
                                if workspace.session_tabs_overflow != overflow {
                                    workspace.session_tabs_overflow = overflow;
                                    cx.notify();
                                }
                            });
                        });
                    })
                    .child(native_session_tabs),
            )
            .children(overflow_lane)
            .child(document_actions)
            .child(reorder_status)
            .child(pointer_event_bridge)
    }
}

/// Each half of the New split button when it spans two tool columns.
pub(super) fn template_segment_width(rail_columns: usize) -> Option<gpui::Rems> {
    (rail_columns >= 3).then(|| gpui::rems(2.25))
}
