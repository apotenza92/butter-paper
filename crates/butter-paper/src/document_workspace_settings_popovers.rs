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
        snap_angle_custom_input: Entity<InputState>,
        rem: Pixels,
        cx: &mut Context<Self>,
    ) -> Popover {
        let semantic_settings = self.semantic_snap_settings;
        let snap_open_control = cx.entity().downgrade();
        let snap_markup_control = cx.entity().downgrade();
        Popover::new("document-workspace-snap-settings-owner")
            .anchor(Anchor::TopRight)
            // Its right edge stands a rail's width left of the button, so it
            // opens over the page and leaves the tool rail clear.
            .mr(right_rail_width(RIGHT_RAIL_COLUMNS, rem))
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
                let angle = {
                    let control = control.clone();
                    move |on: bool, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_angle_enabled(on, cx);
                        });
                    }
                };
                let angle_preset = {
                    let control = control.clone();
                    move |preset: AngleSnapPreset, cx: &mut App| {
                        let _ = control.update(cx, |workspace, cx| {
                            workspace.set_semantic_snap_angle_preset(preset, cx);
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
                let snap_to = snap_section(
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
                );
                let construction = snap_section(
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
                );
                let dimension = snap_section(
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
                );
                let angles = snap_section(
                    "Angles",
                    angle_snap_section(settings, &snap_angle_custom_input, angle, angle_preset, cx),
                    cx,
                );
                let points = snap_section(
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
                );
                let guides = snap_section(
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
                );
                // Column 1 is what to snap to, column 2 the constraints and
                // grids. Too narrow for both, one column keeps that order, so
                // Tab runs the same way in either form.
                let wide = snap_settings_wide(window.viewport_size().width, window.rem_size());
                let column = |id: &'static str, sections: Vec<gpui::AnyElement>| {
                    let mut column = v_flex()
                        .id(id)
                        .debug_selector(move || id.into())
                        .w_96()
                        .gap_4();
                    for (index, section) in sections.into_iter().enumerate() {
                        if index > 0 {
                            column = column.child(snap_separator(cx));
                        }
                        column = column.child(section);
                    }
                    column
                };
                let first = column(DOCUMENT_SNAP_FIRST_COLUMN_ID, vec![snap_to, points, guides]);
                let second = column(
                    DOCUMENT_SNAP_SECOND_COLUMN_ID,
                    vec![angles, dimension, construction],
                );
                v_flex()
                    .id(DOCUMENT_SNAP_POPOVER_ID)
                    .debug_selector(|| DOCUMENT_SNAP_POPOVER_ID.into())
                    // The window less the surface's own padding and the
                    // window margins, so its bottom edge stays in view.
                    .max_h(window.viewport_size().height - window.rem_size() * 3.)
                    // The panel scrolls, so it clips: room inside its edge
                    // keeps a focused control's ring whole in either form.
                    .m(px(-crate::layout_metrics::FOCUS_RING_ROOM))
                    .p(px(crate::layout_metrics::FOCUS_RING_ROOM))
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
                    .map(|panel| {
                        if wide {
                            panel.child(h_flex().items_start().gap_8().child(first).child(second))
                        } else {
                            panel.child(first).child(snap_separator(cx)).child(second)
                        }
                    })
            })
    }
}

/// The Angles section: the switch, the step it snaps to (which Shift uses
/// while the switch is off, so it stays enabled) and a custom step.
fn angle_snap_section(
    settings: SemanticSnapSettings,
    custom_input: &Entity<InputState>,
    on_toggle: impl Fn(bool, &mut App) + 'static,
    on_preset: impl Fn(AngleSnapPreset, &mut App) + 'static,
    cx: &App,
) -> gpui::AnyElement {
    let selected = settings.angle_snap_preset();
    // Each button has its own handler: the group's handler hears only
    // mouse clicks, and these must also work from the keyboard.
    let on_preset = std::rc::Rc::new(on_preset);
    let presets = AngleSnapPreset::ALL
        .into_iter()
        .zip(DOCUMENT_SNAP_ANGLE_PRESET_IDS)
        .map(|(preset, id)| {
            let on_preset = on_preset.clone();
            Button::new(id)
                .debug_selector(move || id.into())
                .label(preset.label())
                .selected(preset == selected)
                .on_click(move |_, _, cx| on_preset(preset, cx))
        });
    let increments = ButtonGroup::new(DOCUMENT_SNAP_ANGLE_INCREMENT_GROUP_ID)
        .small()
        .outline()
        .children(presets);
    let hint = angle_snap_shift_hint(settings.angle_snap_enabled());
    v_flex()
        .gap_3()
        .child(snap_switch_row(
            DOCUMENT_SNAP_ANGLE_ID,
            "Snap angles",
            settings.angle_snap_enabled(),
            false,
            on_toggle,
        ))
        .child(
            h_flex()
                .justify_between()
                .gap_3()
                .text_sm()
                .child("Increment")
                .child(increments),
        )
        .child(snap_number_row(
            "Custom",
            PropertyNumericInput::new(
                DOCUMENT_SNAP_ANGLE_CUSTOM_VALUE_ID,
                "Custom angle increment",
                custom_input,
            )
            .suffix("°")
            .disabled(selected != AngleSnapPreset::Custom),
        ))
        .child(
            gpui::div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(hint),
        )
        .into_any_element()
}

/// What Shift does to lines and turns with angle snapping on or off.
pub(super) fn angle_snap_shift_hint(enabled: bool) -> &'static str {
    if enabled {
        "Hold Shift to draw or turn freely."
    } else {
        "Hold Shift to snap to the increment."
    }
}

/// Whether the Snap settings panel shows its two columns in a viewport
/// `viewport_width` wide: two 24-rem columns, the 2-rem gap between them, its
/// own padding, the tool rail it stands beside and the window margins.
/// Narrower, it shows one column.
pub(super) fn snap_settings_wide(viewport_width: Pixels, rem: Pixels) -> bool {
    let panel = rem * (24. * 2. + 2. + 1.5);
    let margins = rem;
    viewport_width >= panel + right_rail_width(RIGHT_RAIL_COLUMNS, rem) + margins
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_panel_shows_two_columns_only_where_they_fit_beside_the_rail() {
        let rem = px(16.);
        // Two 384px columns, their 32px gap, 24px padding, the 88px rail
        // and 16px of margins.
        assert!(snap_settings_wide(px(928.), rem));
        assert!(!snap_settings_wide(px(927.), rem));
        // The default window and half a 1080p screen: two columns.
        assert!(snap_settings_wide(px(1200.), rem));
        assert!(snap_settings_wide(px(960.), rem));
        // The least document window: one column.
        assert!(!snap_settings_wide(
            crate::window_title_bar::DOCUMENT_WINDOW_MIN_SIZE.width,
            rem
        ));
    }
}
