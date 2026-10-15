//! The workspace root element and its window-level key, focus and pointer
//! handling, including the canvas pointer bridge that follows a drag outside
//! the viewport.

use super::*;
use gpui::Canvas;

impl DocumentWorkspace {
    pub(super) fn render_workspace_root(
        &self,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        v_flex()
            .id(DOCUMENT_WORKSPACE_ID)
            .debug_selector(|| DOCUMENT_WORKSPACE_ID.into())
            .size_full()
            .min_h_0()
            .track_focus(&self.workspace_focus)
            .key_context(DOCUMENT_WORKSPACE_CONTEXT)
            .on_key_down(cx.listener(|workspace, event: &KeyDownEvent, window, cx| {
                workspace.handle_space_pan_down(event, window, cx);
            }))
            .on_key_up(cx.listener(|workspace, event: &KeyUpEvent, window, cx| {
                workspace.handle_space_pan_up(event, window, cx);
            }))
            .on_action(cx.listener(Self::open_pdf_from_action))
            .on_action(cx.listener(|workspace, _: &NewFromTemplate, _, cx| {
                workspace.template_manage_requests =
                    workspace.template_manage_requests.saturating_add(1);
                cx.emit(DocumentWorkspaceTemplateCommand::Manage);
                cx.notify();
            }))
            .on_action(cx.listener(|workspace, _: &SaveDocumentAsTemplate, _, cx| {
                workspace.handle_template_split_event(
                    TemplateSplitEvent::SaveDocumentAsTemplateRequested,
                    cx,
                );
            }))
            .on_action(cx.listener(Self::save_from_action))
            .on_action(cx.listener(Self::save_as_from_action))
            .on_action(cx.listener(|workspace, _: &NavigateHome, window, cx| {
                workspace.apply_document_navigation(DocumentNavigationAction::Home, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &NavigateEnd, window, cx| {
                workspace.apply_document_navigation(DocumentNavigationAction::End, window, cx);
            }))
            .on_action(
                cx.listener(|workspace, _: &NavigatePreviousPage, window, cx| {
                    workspace.apply_document_navigation(
                        DocumentNavigationAction::PreviousPage,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|workspace, _: &NavigateNextPage, window, cx| {
                workspace.apply_document_navigation(DocumentNavigationAction::NextPage, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &NavigateUp, window, cx| {
                workspace.apply_document_navigation(DocumentNavigationAction::ArrowUp, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &NavigateDown, window, cx| {
                workspace.apply_document_navigation(
                    DocumentNavigationAction::ArrowDown,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|workspace, _: &NavigateLeft, window, cx| {
                workspace.apply_document_navigation(
                    DocumentNavigationAction::ArrowLeft,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|workspace, _: &NavigateRight, window, cx| {
                workspace.apply_document_navigation(
                    DocumentNavigationAction::ArrowRight,
                    window,
                    cx,
                );
            }))
            .on_action(cx.listener(|workspace, _: &NavigatePageUp, window, cx| {
                workspace.apply_document_navigation(DocumentNavigationAction::PageUp, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &NavigatePageDown, window, cx| {
                workspace.apply_document_navigation(DocumentNavigationAction::PageDown, window, cx);
            }))
            .on_action(cx.listener(Self::close_active_document_from_action))
            .on_action(cx.listener(|workspace, _: &RotatePageLeft, _, cx| {
                workspace.rotate_active_page_from_action(PageRotationDirection::Left, cx);
            }))
            .on_action(cx.listener(|workspace, _: &RotatePageRight, _, cx| {
                workspace.rotate_active_page_from_action(PageRotationDirection::Right, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ZoomIn, _, cx| {
                workspace.zoom_active_document(ZOOM_STEP_FACTOR, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ZoomOut, _, cx| {
                workspace.zoom_active_document(1. / ZOOM_STEP_FACTOR, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ActualSize, _, cx| {
                workspace.set_active_manual_zoom(DEFAULT_VIEWER_ZOOM, cx);
            }))
            .on_action(cx.listener(|workspace, _: &FitWidth, _, cx| {
                workspace.set_active_fit_preset(ViewerFitPreset::Width, cx);
            }))
            .on_action(cx.listener(|workspace, _: &FitPage, _, cx| {
                workspace.set_active_fit_preset(ViewerFitPreset::Page, cx);
            }))
            .on_action(cx.listener(|workspace, _: &ContinuousView, _, cx| {
                workspace.set_active_page_view_mode(PageViewMode::Continuous, cx);
            }))
            .on_action(cx.listener(|workspace, _: &SinglePageView, _, cx| {
                workspace.set_active_page_view_mode(PageViewMode::SinglePage, cx);
            }))
            .on_action(cx.listener(|workspace, _: &SelectTool, window, cx| {
                workspace.select_available_annotation_tool(AnnotationTool::Select, window, cx);
            }))
            .on_action(cx.listener(|workspace, _: &PanTool, window, cx| {
                if window.has_focused_input(cx) {
                    return;
                }
                workspace.select_available_annotation_tool(AnnotationTool::Select, window, cx);
                if workspace.active_document_id.is_some() {
                    workspace.pan_tool_active = true;
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|workspace, _: &SetPageScale, window, cx| {
                let Some(document_id) = workspace.active_document_id else {
                    return;
                };
                let current_page = workspace
                    .session(document_id, cx)
                    .map(|session| session.read(cx).current_page)
                    .unwrap_or_default();
                // This listener runs inside a workspace update, so the control
                // cannot read the workspace back; hand it the page state.
                let current_scale = workspace.page_scale(document_id, current_page, cx);
                let mut presets = workspace.scale_presets(document_id, cx);
                presets.extend(built_in_scale_presets());
                let control = workspace.ensure_page_scale_control(window, cx);
                control.update(cx, |control, cx| {
                    control.open_for_state(
                        document_id,
                        current_page,
                        current_scale,
                        presets,
                        window,
                        cx,
                    );
                });
            }))
            .on_action(cx.listener(Self::select_text_box_tool_from_action))
            .on_action(cx.listener(Self::select_rectangle_tool_from_action))
            .on_action(cx.listener(Self::select_ellipse_tool_from_action))
            .on_action(cx.listener(Self::select_pen_tool_from_action))
            .on_action(cx.listener(Self::select_cloud_tool_from_action))
            .on_action(cx.listener(Self::select_callout_tool_from_action))
            .on_action(cx.listener(Self::select_line_tool_from_action))
            .on_action(cx.listener(Self::select_arc_tool_from_action))
            .on_action(cx.listener(Self::select_arrow_tool_from_action))
            .on_action(cx.listener(Self::select_polyline_tool_from_action))
            .on_action(cx.listener(Self::select_polygon_tool_from_action))
            .on_action(cx.listener(Self::select_polylength_tool_from_action))
            .on_action(cx.listener(Self::select_area_tool_from_action))
            .on_action(cx.listener(Self::select_cloud_plus_tool_from_action))
            .on_action(cx.listener(Self::select_dimension_tool_from_action))
            .on_action(cx.listener(Self::finish_vertex_path_from_action))
            .on_action(cx.listener(Self::select_highlight_tool_from_action))
            .on_action(cx.listener(Self::select_image_tool_from_action))
            .on_action(cx.listener(Self::select_snapshot_tool_from_action))
            .on_action(cx.listener(Self::select_edit_pdf_content_tool_from_action))
            .on_action(cx.listener(Self::select_length_tool_from_action))
            .on_action(cx.listener(Self::select_all_annotations_from_action))
            .on_action(cx.listener(Self::copy_annotations_from_action))
            .on_action(cx.listener(Self::cut_annotations_from_action))
            .on_action(cx.listener(Self::paste_annotations_from_action))
            .on_action(cx.listener(Self::delete_annotations_from_action))
            .on_action(cx.listener(Self::undo_annotations_from_action))
            .on_action(cx.listener(Self::redo_annotations_from_action))
            .on_action(cx.listener(|workspace, _: &Escape, window, cx| {
                let pending_editor_focused = workspace
                    .pending_text_box_focus(cx)
                    .is_some_and(|focus| focus.is_focused(window));
                if workspace.signature_popover_open
                    || window.has_active_dialog(cx)
                    || window.has_active_sheet(cx)
                    || (window.has_focused_input(cx) && !pending_editor_focused)
                {
                    cx.propagate();
                    return;
                }
                if workspace.pending_text_box_editor.is_some() {
                    if !workspace.cancel_pending_composite_text_editor(window, cx) {
                        let existing_text_box = workspace
                            .pending_text_box_editor
                            .as_ref()
                            .is_some_and(|editor| {
                                matches!(
                                    editor.target,
                                    PendingTextEditorTarget::ExistingTextBox { .. }
                                )
                            });
                        match workspace.commit_pending_text_box(cx) {
                            Ok(_) => {
                                if existing_text_box {
                                    workspace.workspace_focus.focus(window, cx);
                                } else {
                                    workspace.text_box_return_focus.focus(window, cx);
                                }
                            }
                            Err(error) => {
                                workspace.text_box_commit_error = Some(error);
                                cx.notify();
                            }
                        }
                    }
                    return;
                }
                if workspace
                    .page_scale_control
                    .as_ref()
                    .is_some_and(|control| {
                        control.update(cx, |control, cx| control.cancel_pick(cx))
                    })
                {
                    if let Some(document_id) = workspace.active_document_id {
                        workspace.annotation_statuses.remove(&document_id);
                    }
                    cx.notify();
                    return;
                }
                let Some(document_id) = workspace.active_document_id else {
                    cx.propagate();
                    return;
                };
                // Electron's capture-phase Escape finishes only cloud polygons,
                // then resets the canvas tool and selection. Other drafts cancel.
                if workspace.session(document_id, cx).is_some_and(|session| {
                    let session = session.read(cx);
                    session.annotations.cloud_pending(document_id.value())
                        || session.annotations.cloud_plus_pending(document_id.value())
                }) {
                    workspace.finish_vertex_path_from_action(&FinishVertexPath, window, cx);
                }
                workspace.pan_tool_active = false;
                workspace.pan_drag = None;
                workspace.space_pan_hold.clear_stash();
                workspace.active_annotation_pointer = None;
                if let Some(session) = workspace.session(document_id, cx).cloned() {
                    session.update(cx, |session, cx| {
                        let _ = session.annotations.cancel(PointerCancelReason::ToolChanged);
                        let _ = session.annotations.set_tool(AnnotationTool::Select);
                        session.annotations.clear_selection(document_id.value());
                        cx.notify();
                    });
                }
                workspace.annotation_statuses.remove(&document_id);
                cx.notify();
            }))
            .size_full()
            .min_h(px(360.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
    }

    pub(super) fn canvas_pointer_event_bridge(&self, cx: &mut Context<Self>) -> Canvas<()> {
        let down_control = cx.entity().downgrade();
        let move_control = cx.entity().downgrade();
        let up_control = cx.entity().downgrade();
        let exit_control = cx.entity().downgrade();
        canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event({
                    let middle_down_control = down_control.clone();
                    move |event: &MouseDownEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture
                            || event.button != MouseButton::Middle
                            || crate::overlay_state::press_owned_by_overlay(window, cx)
                        {
                            return;
                        }
                        let started = middle_down_control
                            .update(cx, |workspace, cx| {
                                workspace.begin_middle_button_pan(event.position, cx)
                            })
                            .unwrap_or(false);
                        if started {
                            window.prevent_default();
                        }
                    }
                });
                window.on_mouse_event({
                    let middle_up_control = up_control.clone();
                    move |event: &MouseUpEvent, phase, window, cx| {
                        if phase != DispatchPhase::Capture || event.button != MouseButton::Middle {
                            return;
                        }
                        let ended = middle_up_control
                            .update(cx, |workspace, cx| {
                                workspace.end_middle_button_pan(event.position, cx)
                            })
                            .unwrap_or(false);
                        if ended {
                            window.prevent_default();
                        }
                    }
                });
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    // A press while a popover, menu or dialog is open only
                    // dismisses it (or acts inside it); it never also starts a
                    // canvas selection or edit.
                    if crate::overlay_state::press_owned_by_overlay(window, cx) {
                        return;
                    }
                    let started = down_control
                        .update(cx, |workspace, cx| {
                            workspace.begin_annotation_pointer(
                                event.position,
                                event.modifiers,
                                event.click_count,
                                window,
                                cx,
                            )
                        })
                        .unwrap_or(false);
                    if started {
                        window.prevent_default();
                    }
                });
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let middle_panning = event.pressed_button == Some(MouseButton::Middle)
                        && move_control
                            .read_with(cx, |workspace, _| workspace.pan_drag.is_some())
                            .unwrap_or(false);
                    if event.pressed_button != Some(MouseButton::Left) && !middle_panning {
                        let handled = move_control
                            .update(cx, |workspace, cx| {
                                if workspace
                                    .active_annotation_pointer
                                    .is_some_and(|active| active.placement_pending)
                                {
                                    workspace.update_annotation_pointer(
                                        event.position,
                                        event.modifiers,
                                        cx,
                                    )
                                } else if workspace.cancel_active_annotation_pointer(cx) {
                                    true
                                } else {
                                    workspace.update_annotation_hover(
                                        event.position,
                                        event.modifiers,
                                        cx,
                                    )
                                }
                            })
                            .unwrap_or(false);
                        if handled {
                            window.prevent_default();
                        }
                        return;
                    }
                    let updated = move_control
                        .update(cx, |workspace, cx| {
                            workspace.update_annotation_pointer(event.position, event.modifiers, cx)
                        })
                        .unwrap_or(false);
                    if updated {
                        window.prevent_default();
                    }
                });
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let finished = up_control
                        .update(cx, |workspace, cx| {
                            let finished = workspace.finish_annotation_pointer(
                                event.position,
                                event.modifiers,
                                window,
                                cx,
                            );
                            // The cursor reflects what is under the pointer
                            // now, without waiting for the next move.
                            if finished {
                                workspace.update_annotation_hover(
                                    event.position,
                                    event.modifiers,
                                    cx,
                                );
                            }
                            finished
                        })
                        .unwrap_or(false);
                    if finished {
                        window.prevent_default();
                    }
                });
                window.on_mouse_event(move |_: &MouseExitEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let cancelled = exit_control
                        .update(cx, |workspace, cx| {
                            let hover_cleared = workspace.clear_hover_candidate(cx);
                            if workspace
                                .active_annotation_pointer
                                .is_some_and(|active| active.placement_pending)
                            {
                                hover_cleared
                            } else {
                                workspace.cancel_active_annotation_pointer(cx) || hover_cleared
                            }
                        })
                        .unwrap_or(false);
                    if cancelled {
                        window.prevent_default();
                    }
                });
            },
        )
        .absolute()
        .left_0()
        .bottom_0()
        .size(px(1.))
    }
}
