//! The snapping settings popover on the viewer toolbar.

use super::*;
use crate::accessible_button::close_icon_button;
use crate::property_controls::{PropertyNumericInput, popover_header};

impl DocumentWorkspace {
    pub(super) fn render_snap_settings_popover(
        &self,
        save_busy: bool,
        snap_grid_spacing_input: Entity<InputState>,
        snap_dimension_increment_input: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> Popover {
        let semantic_settings = self.semantic_snap_settings;
        let snap_open_control = cx.entity().downgrade();
        let snap_markup_control = cx.entity().downgrade();
        Popover::new("document-workspace-snap-settings-owner")
            .anchor(Anchor::TopRight)
            .open(crate::overlay_state::sync_overlay_open(
                "snap-settings",
                self.semantic_snap_settings_open,
                cx,
            ))
            .on_open_change(move |open, _, cx| {
                let _ = snap_open_control.update(cx, |workspace, cx| {
                    workspace.semantic_snap_settings_open = *open;
                    cx.notify();
                });
            })
            .trigger(
                rail_tool_button(DOCUMENT_SNAP_SETTINGS_ID, "Snap settings", "magnet")
                    .debug_selector(|| DOCUMENT_SNAP_SETTINGS_ID.into())
                    .tooltip("Snap settings")
                    .disabled(save_busy),
            )
            .content(move |_, window, cx| {
                let close_popover = cx.entity();
                let control = snap_markup_control.clone();
                let settings = semantic_settings;
                let grid_enabled = settings.is_source_enabled(SemanticSnapSource::ConstructionGrid);
                let content_enabled = settings.is_source_enabled(SemanticSnapSource::Content);
                let page_grid_enabled = settings.is_source_enabled(SemanticSnapSource::PageGrid);
                let targets_enabled =
                    settings.annotations_enabled() || content_enabled || page_grid_enabled;
                let source = |source: SemanticSnapSource| {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_source(source, on, cx);
                        });
                    }
                };
                let target = |target: SemanticSnapTarget| {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_target(target, on, cx);
                        });
                    }
                };
                let guide = |guide: SemanticSnapGuideType| {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_guide(guide, on, cx);
                        });
                    }
                };
                let markup = {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_annotations_enabled(on, cx);
                        });
                    }
                };
                let grid_visible = {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_grid_visible(on, cx);
                        });
                    }
                };
                let dimension = {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_dimension_increment_enabled(on, cx);
                        });
                    }
                };
                let guides = {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_guides_enabled(on, cx);
                        });
                    }
                };
                v_flex()
                    .id(DOCUMENT_SNAP_POPOVER_ID)
                    .debug_selector(|| DOCUMENT_SNAP_POPOVER_ID.into())
                    .w_96()
                    .max_h(window.viewport_size().height - px(32.))
                    .overflow_y_scroll()
                    .gap_4()
                    .child(popover_header(
                        "Snap settings",
                        close_icon_button(DOCUMENT_SNAP_CLOSE_ID, "Close").on_click(
                            move |_, window, cx| {
                                close_popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                            },
                        ),
                    ))
                    .child(snap_section(
                        "Snap to",
                        snap_tile_grid(vec![
                            snap_tile(
                                DOCUMENT_SNAP_CONTENT_ID,
                                "Content",
                                None,
                                content_enabled,
                                false,
                                source(SemanticSnapSource::Content),
                            ),
                            snap_tile(
                                DOCUMENT_SNAP_MARKUP_ID,
                                "Markup",
                                None,
                                settings.annotations_enabled(),
                                false,
                                markup,
                            ),
                            snap_tile(
                                DOCUMENT_SNAP_PAGE_GRID_ID,
                                "Page grid",
                                None,
                                page_grid_enabled,
                                false,
                                source(SemanticSnapSource::PageGrid),
                            ),
                        ]),
                        cx,
                    ))
                    .child(snap_separator(cx))
                    .child(snap_section(
                        "Construction grid",
                        v_flex()
                            .gap_3()
                            .child(snap_switch_row(
                                DOCUMENT_SNAP_CONSTRUCTION_GRID_ID,
                                "Snap to grid",
                                grid_enabled,
                                false,
                                source(SemanticSnapSource::ConstructionGrid),
                            ))
                            .child(snap_switch_row(
                                DOCUMENT_SNAP_CONSTRUCTION_GRID_VISIBLE_ID,
                                "Show grid",
                                settings.construction_grid_visible(),
                                !grid_enabled,
                                grid_visible,
                            ))
                            .child(snap_number_row(
                                "Spacing",
                                PropertyNumericInput::new(
                                    DOCUMENT_SNAP_CONSTRUCTION_GRID_SPACING_ID,
                                    "Construction grid spacing",
                                    &snap_grid_spacing_input,
                                )
                                .suffix("mm")
                                .disabled(!grid_enabled),
                            ))
                            .into_any_element(),
                        cx,
                    ))
                    .child(snap_separator(cx))
                    .child(snap_section(
                        "Dimension increments",
                        v_flex()
                            .gap_3()
                            .child(snap_switch_row(
                                DOCUMENT_SNAP_DIMENSION_INCREMENT_ID,
                                "Snap dimensions",
                                settings.dimension_increment_enabled(),
                                false,
                                dimension,
                            ))
                            .child(snap_number_row(
                                "Increment",
                                PropertyNumericInput::new(
                                    DOCUMENT_SNAP_DIMENSION_INCREMENT_VALUE_ID,
                                    "Dimension increment",
                                    &snap_dimension_increment_input,
                                )
                                .suffix("mm")
                                .disabled(!settings.dimension_increment_enabled()),
                            ))
                            .into_any_element(),
                        cx,
                    ))
                    .child(snap_separator(cx))
                    .child(snap_section(
                        "Snap points",
                        snap_tile_grid(vec![
                            snap_tile(
                                DOCUMENT_SNAP_ENDPOINT_ID,
                                "Ends",
                                Some("square"),
                                settings.is_target_selected(SemanticSnapTarget::Endpoint),
                                !targets_enabled,
                                target(SemanticSnapTarget::Endpoint),
                            ),
                            snap_tile(
                                DOCUMENT_SNAP_MIDPOINT_ID,
                                "Midpoints",
                                Some("triangle"),
                                settings.is_target_selected(SemanticSnapTarget::Midpoint),
                                !targets_enabled,
                                target(SemanticSnapTarget::Midpoint),
                            ),
                            snap_tile(
                                DOCUMENT_SNAP_CENTER_ID,
                                "Centres",
                                Some("circle"),
                                settings.is_target_selected(SemanticSnapTarget::Center),
                                !targets_enabled,
                                target(SemanticSnapTarget::Center),
                            ),
                            snap_tile(
                                DOCUMENT_SNAP_INTERSECTION_ID,
                                "Intersections",
                                Some("x"),
                                settings.is_target_selected(SemanticSnapTarget::Intersection),
                                !targets_enabled,
                                target(SemanticSnapTarget::Intersection),
                            ),
                            snap_tile(
                                DOCUMENT_SNAP_NEAREST_ID,
                                "Nearest",
                                Some("diamond"),
                                settings.is_target_selected(SemanticSnapTarget::Nearest),
                                !targets_enabled,
                                target(SemanticSnapTarget::Nearest),
                            ),
                        ]),
                        cx,
                    ))
                    .child(snap_separator(cx))
                    .child(snap_section(
                        "Snap guides",
                        v_flex()
                            .gap_3()
                            .child(snap_switch_row(
                                DOCUMENT_SNAP_GUIDES_ID,
                                "Show snap guides",
                                settings.guides_enabled(),
                                false,
                                guides,
                            ))
                            .child(snap_tile_grid(vec![
                                snap_tile(
                                    DOCUMENT_SNAP_GUIDE_ALIGNMENT_ID,
                                    "Alignment",
                                    None,
                                    settings.is_guide_enabled(SemanticSnapGuideType::Alignment),
                                    !settings.guides_enabled(),
                                    guide(SemanticSnapGuideType::Alignment),
                                ),
                                snap_tile(
                                    DOCUMENT_SNAP_GUIDE_EQUAL_SIZE_ID,
                                    "Equal size",
                                    None,
                                    settings.is_guide_enabled(SemanticSnapGuideType::EqualSize),
                                    !settings.guides_enabled(),
                                    guide(SemanticSnapGuideType::EqualSize),
                                ),
                                snap_tile(
                                    DOCUMENT_SNAP_GUIDE_EQUAL_SPACING_ID,
                                    "Equal spacing",
                                    None,
                                    settings.is_guide_enabled(SemanticSnapGuideType::EqualSpacing),
                                    !settings.guides_enabled(),
                                    guide(SemanticSnapGuideType::EqualSpacing),
                                ),
                            ]))
                            .into_any_element(),
                        cx,
                    ))
            })
    }
}
