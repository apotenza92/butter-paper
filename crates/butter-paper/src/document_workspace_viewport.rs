//! The scrolled document viewport: page tiles, annotation layers and the
//! opening status over them.

use super::*;
use crate::document_viewer::ViewerPlanSnapshot;
use gpui::{Canvas, Hsla};
use std::collections::BTreeSet;

/// One visible page: layout, display size, PDF size and rotation, coordinate
/// space, annotation scene, whether highlights are precomposed, painted tiles,
/// the painted-viewer authority, render quality and any render error.
pub(super) type ViewerPageRender = (
    PageLayout,
    (f32, f32),
    ((f32, f32), PageRotation),
    PageCoordinateSpace,
    AnnotationScene,
    bool,
    Vec<(TileRequest, Arc<RenderImage>)>,
    Option<PaintedViewerAuthority>,
    Option<ViewerRenderQuality>,
    Option<String>,
);

/// Everything the viewport draws, gathered by the workspace render.
pub(super) struct ViewportInputs {
    pub active_selection_marquee: Option<(u32, SelectionMarquee)>,
    pub construction_grid_spacing_mm: Option<f64>,
    pub current_image: Option<Arc<RenderImage>>,
    pub current_page: u32,
    pub document_id: DocumentId,
    pub document_opening_batch_count: usize,
    pub focused_annotation_id: Option<MarkupId>,
    pub hot_annotation_handle: Option<(DocumentId, u32, MarkupId, usize)>,
    pub hovered_annotation: Option<(DocumentId, u32, MarkupId)>,
    pub image_assets: Arc<HashMap<String, Arc<RenderImage>>>,
    pub object_snap_tracking_result: Option<ObjectSnapTrackingResult>,
    pub pending_image_preview: Option<PendingImagePreview>,
    pub pending_text_box_presentation: Option<PendingTextBoxPresentation>,
    pub page_text_overlays: Vec<PageTextOverlay>,
    /// The picture of content being dragged, painted where it is going.
    pub content_sprite: Option<super::content::SpritePaint>,
    pub form_overlays: Vec<FormOverlay>,
    pub relationship_snap_guides: Vec<RelationshipSnapGuide>,
    /// The direction held at an angle snap increment.
    pub angle_snap_guide: Option<crate::semantic_snapping::AngleSnapGuide>,
    pub selection_color: Hsla,
    pub semantic_snap_decision: Option<SemanticSnapDecision>,
    /// The calibration line of the Page Scale dialog.
    pub calibration_preview: Option<crate::page_scale_control::CalibrationPreview>,
    /// The pages outlined, and whether the Scale that acts on them is in
    /// hand.
    pub page_outline: Option<PageOutline>,
    pub viewer_pages: Vec<ViewerPageRender>,
    /// Each page's tiles from before a zoom, drawn under its tiles.
    pub viewer_zoom_fallback: HashMap<usize, Vec<(crate::viewer::Rect, Arc<RenderImage>)>>,
    /// The page whose picture is turning into a new orientation.
    pub page_turn: Option<super::PageTurnPaint>,
    /// The page whose markups are faded, and how opaque they are drawn.
    pub page_markup_opacity: Option<(usize, f32)>,
    pub viewer_pan_margin: (f32, f32),
    pub viewer_plan: Option<ViewerPlanSnapshot>,
    pub viewer_scroll: ScrollHandle,
    pub viewport_cursor_style: CursorStyle,
    pub viewport_observer: Canvas<gpui::Hitbox>,
    /// Split View's other pane: it draws the pages and their markups only,
    /// and a press or wheel there makes it the active pane.
    pub other_pane: bool,
}

/// The test id of what Edit PDF draws over a page (paragraph frames,
/// hover, selection, the edited paragraph). In Split View's other pane it
/// starts with "other-".
pub fn document_edit_pdf_overlay_id(document_id: DocumentId, page_index: u32) -> String {
    format!("{document_id}-edit-pdf-overlay-{page_index}")
}

/// An element id in the other pane of Split View, apart from the active
/// pane's.
fn pane_id(id: String, other_pane: bool) -> String {
    if other_pane {
        format!("other-{id}")
    } else {
        id
    }
}

impl DocumentWorkspace {
    pub(super) fn render_document_viewport(
        &self,
        inputs: ViewportInputs,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let ViewportInputs {
            active_selection_marquee,
            construction_grid_spacing_mm,
            current_image,
            current_page,
            document_id,
            document_opening_batch_count,
            focused_annotation_id,
            hot_annotation_handle,
            hovered_annotation,
            image_assets,
            object_snap_tracking_result,
            pending_image_preview,
            pending_text_box_presentation,
            page_text_overlays,
            content_sprite,
            form_overlays,
            relationship_snap_guides,
            angle_snap_guide,
            selection_color,
            semantic_snap_decision,
            calibration_preview,
            page_outline,
            viewer_pages,
            viewer_zoom_fallback,
            page_turn,
            page_markup_opacity,
            viewer_pan_margin,
            viewer_plan,
            viewer_scroll,
            viewport_cursor_style,
            viewport_observer,
            other_pane,
        } = inputs;
        let viewport_id = if other_pane {
            super::split::SPLIT_OTHER_VIEWPORT_ID
        } else {
            DOCUMENT_VIEWPORT_ID
        };
        let page_text_input = page_text_overlays
            .iter()
            .find_map(|overlay| overlay.input.clone())
            .map(|(input, focus)| page_text::page_text_input_element(input, focus));
        let form_focus =
            forms::form_focus_element(self.form_focus.clone(), cx.entity().downgrade());
        let form_owner = cx.entity().downgrade();
        let warning_colors = tables::WarningColors::new(cx);
        let outline_color = page_outline.as_ref().map(|outline| {
            let theme = cx.theme();
            let dark = theme.mode.is_dark();
            theme.foreground.opacity(match (outline.emphasized, dark) {
                (false, false) => 0.35,
                (false, true) => 0.6,
                (true, false) => 0.75,
                (true, true) => 0.9,
            })
        });
        gpui::div()
                                .id(viewport_id)
                                .debug_selector(move || viewport_id.into())
                                .relative()
                                .size_full()
                                .children(page_text_input)
                                .when(!other_pane, |viewport| viewport.child(form_focus))
                                .cursor(viewport_cursor_style)
                                .overflow_scroll()
                                .track_scroll(&viewer_scroll)
                                .on_scroll_wheel(cx.listener(
                                    move |workspace, event: &ScrollWheelEvent, window, cx| {
                                        if other_pane {
                                            // The wheel only makes the pane active.
                                            workspace.activate_other_pane(document_id, window, cx);
                                            window.prevent_default();
                                        } else if workspace.handle_viewport_wheel(document_id, event, window, cx) {
                                            window.prevent_default();
                                        }
                                    },
                                ))
                                .on_pinch(cx.listener(
                                    move |workspace, event: &gpui::PinchEvent, window, cx| {
                                        if other_pane {
                                            workspace.activate_other_pane(document_id, window, cx);
                                        } else {
                                            workspace.handle_viewport_pinch(document_id, event, window, cx);
                                        }
                                    },
                                ))
                                .child(viewport_observer)
                                .when_some(viewer_plan, |viewport, plan| {
                                    let content_width = plan
                                        .page_layouts
                                        .iter()
                                        .map(|layout| {
                                            layout.logical_rect.x
                                                + layout.logical_rect.width
                                                + 24.
                                        })
                                        .fold(1., f32::max);
                                    // Blank space around the pages lets them be
                                    // panned past their edges (see `pan_margin`).
                                    viewport.child(
                                        gpui::div()
                                            .flex_none()
                                            .w(px(content_width + viewer_pan_margin.0 * 2.))
                                            .h(px(plan.total_height.max(1.) + viewer_pan_margin.1 * 2.))
                                            .px(px(viewer_pan_margin.0))
                                            .py(px(viewer_pan_margin.1))
                                            .child(
                                        gpui::div()
                                            .relative()
                                            .w(px(content_width))
                                            .h(px(plan.total_height.max(1.)))
                                            .flex_none()
                                            .children(viewer_pages.into_iter().map(
                                                |(layout, page_size, (pdf_page_size, rotation), coordinate_space, scene, highlights_precomposed, tiles, painted_viewer, quality, render_error)| {
                                                    let page_id = pane_id(
                                                        document_viewer_page_id(document_id, layout.page),
                                                        other_pane,
                                                    );
                                                    let page_selector = page_id.clone();
                                                    let page_index = layout.page as u32;
                                                    let quality_marker = quality.map(|quality| {
                                                        let quality_id = pane_id(
                                                            document_viewer_quality_id(
                                                                document_id,
                                                                layout.page,
                                                                quality,
                                                            ),
                                                            other_pane,
                                                        );
                                                        let quality_selector = quality_id.clone();
                                                        gpui::div()
                                                            .id(quality_id)
                                                            .debug_selector(move || {
                                                                quality_selector.clone()
                                                            })
                                                            .absolute()
                                                            .left_0()
                                                            .top_0()
                                                            .size(px(1.))
                                                    });
                                                    let render_error_surface = render_error
                                                        .filter(|_| quality.is_none())
                                                        .map(|error| {
                                                            let error_id = document_viewer_error_id(
                                                                document_id,
                                                                layout.page,
                                                            );
                                                            let error_selector = error_id.clone();
                                                            let retry_id = document_viewer_retry_id(
                                                                document_id,
                                                                layout.page,
                                                            );
                                                            let retry_selector = retry_id.clone();
                                                            let retry_control =
                                                                cx.entity().downgrade();
                                                            v_flex()
                                                                .id(error_id)
                                                                .debug_selector(move || {
                                                                    error_selector.clone()
                                                                })
                                                                .absolute()
                                                                .inset_0()
                                                                .occlude()
                                                                .items_start()
                                                                .justify_start()
                                                                .gap_2()
                                                                .p_3()
                                                                .child(
                                                                    Alert::error(
                                                                        format!(
                                                                            "viewer-page-{}-render-error-message",
                                                                            layout.page
                                                                        ),
                                                                        error,
                                                                    )
                                                                    .title("Unable to render page"),
                                                                )
                                                                .child(
                                                                    Button::new(retry_id)
                                                                        .debug_selector(move || {
                                                                            retry_selector
                                                                                .clone()
                                                                        })
                                                                        .label("Retry")
                                                                        .on_click(move |_, _, cx| {
                                                                            let _ = retry_control
                                                                                .update(
                                                                                    cx,
                                                                                    |workspace, cx| {
                                                                                        workspace.retry_viewer_page(
                                                                                            document_id,
                                                                                            layout.page,
                                                                                            cx,
                                                                                        );
                                                                                    },
                                                                                );
                                                                        }),
                                                                )
                                                        });
                                                    // A turning page draws only its turning
                                                    // picture.
                                                    let turning = page_turn
                                                        .as_ref()
                                                        .filter(|turn| turn.page == layout.page)
                                                        .cloned();
                                                    // Tiles from before a zoom, scaled under
                                                    // the new ones until those arrive.
                                                    let zoom_fallback_tiles = viewer_zoom_fallback
                                                        .get(&layout.page)
                                                        .filter(|_| turning.is_none())
                                                        .into_iter()
                                                        .flatten()
                                                        .map(|(rect, image)| {
                                                            let image_size = image.size(0);
                                                            let pad = crate::document_resource::TILE_IMAGE_PADDING as f32;
                                                            let border = |extent: f32, pixels: i32| {
                                                                let inner = pixels as f32 - 2. * pad;
                                                                if inner > 0. { extent * pad / inner } else { 0. }
                                                            };
                                                            let border_x = border(rect.width, image_size.width.0);
                                                            let border_y = border(rect.height, image_size.height.0);
                                                            gpui::div()
                                                                .absolute()
                                                                .left(px(rect.x))
                                                                .top(px(rect.y))
                                                                .w(px(rect.width))
                                                                .h(px(rect.height))
                                                                .overflow_hidden()
                                                                .child(
                                                                    img(image.clone())
                                                                        .absolute()
                                                                        .left(px(-border_x))
                                                                        .top(px(-border_y))
                                                                        .w(px(rect.width + 2. * border_x))
                                                                        .h(px(rect.height + 2. * border_y))
                                                                        .object_fit(ObjectFit::Fill),
                                                                )
                                                        })
                                                        .collect::<Vec<_>>();
                                                    let page_tiles = tiles
                                                        .into_iter()
                                                        .filter(|_| turning.is_none())
                                                        .enumerate()
                                                        .map(|(tile_index, (request, image))| {
                                                            let scale = (request
                                                                .device_scale_millis
                                                                as f32
                                                                / 1_000.)
                                                                .max(0.1);
                                                            let tile_id = pane_id(
                                                                document_viewer_tile_id(
                                                                    document_id,
                                                                    plan.generation,
                                                                    layout.page,
                                                                    tile_index,
                                                                ),
                                                                other_pane,
                                                            );
                                                            let tile_selector = tile_id.clone();
                                                            let width = request.crop.width as f32 / scale;
                                                            let height = request.crop.height as f32 / scale;
                                                            // The image's border of repeated edge
                                                            // pixels lies outside the tile and is
                                                            // clipped away.
                                                            let image_size = image.size(0);
                                                            let pad = crate::document_resource::TILE_IMAGE_PADDING as f32;
                                                            let border = |extent: f32, pixels: i32| {
                                                                let inner = pixels as f32 - 2. * pad;
                                                                if inner > 0. { extent * pad / inner } else { 0. }
                                                            };
                                                            let border_x = border(width, image_size.width.0);
                                                            let border_y = border(height, image_size.height.0);
                                                            gpui::div()
                                                                .id(tile_id)
                                                                .debug_selector(move || {
                                                                    tile_selector.clone()
                                                                })
                                                                .absolute()
                                                                .left(px(
                                                                    request.crop.x as f32 / scale,
                                                                ))
                                                                .top(px(
                                                                    request.crop.y as f32 / scale,
                                                                ))
                                                                .w(px(width))
                                                                .h(px(height))
                                                                .overflow_hidden()
                                                                .child(
                                                                    img(image)
                                                                        .absolute()
                                                                        .left(px(-border_x))
                                                                        .top(px(-border_y))
                                                                        .w(px(width + 2. * border_x))
                                                                        .h(px(height + 2. * border_y))
                                                                        .object_fit(
                                                                            ObjectFit::Fill,
                                                                        ),
                                                                )
                                                        });
                                                    let base_image_id = format!("{page_id}-base-image");
                                                    let base_image_selector = base_image_id.clone();
                                                    gpui::div()
                                                        .id(page_id)
                                                        .debug_selector(move || {
                                                            page_selector.clone()
                                                        })
                                                        .absolute()
                                                        .left(px(layout.logical_rect.x))
                                                        .top(px(layout.logical_rect.y))
                                                        .w(px(layout.logical_rect.width))
                                                        .h(px(layout.logical_rect.height))
                                                        .when(turning.is_none(), |page| {
                                                            // The paper, light or dark.
                                                            page.bg(crate::paper_chrome::chrome_colour(|chrome| chrome.paper))
                                                        })
                                                        // A ring outside the page (never a border, which
                                                        // would inset the image): a dark page stands
                                                        // apart from the dark viewport.
                                                        .when_some(crate::paper_chrome::page_edge(), |page, edge| {
                                                            page.shadow(vec![gpui::BoxShadow {
                                                                color: edge,
                                                                offset: gpui::point(px(0.), px(0.)),
                                                                blur_radius: px(0.),
                                                                spread_radius: px(1.),
                                                                inset: false,
                                                            }])
                                                        })
                                                        .when(
                                                            page_index == current_page
                                                                && turning.is_none(),
                                                            |page| {
                                                                page.when_some(
                                                                    current_image.clone(),
                                                                    |page, image| {
                                                                        page.child(
                                                                            img(image)
                                                                                .id(base_image_id)
                                                                                .debug_selector(move || base_image_selector.clone())
                                                                                .size_full()
                                                                                .object_fit(
                                                                                    ObjectFit::Fill,
                                                                                ),
                                                                        )
                                                                    },
                                                                )
                                                            },
                                                        )
                                                        .children(zoom_fallback_tiles)
                                                        .children(page_tiles)
                                                        .children(
                                                            turning
                                                                .clone()
                                                                .map(super::page_turn::page_turn_element),
                                                        )
                                                        .children(
                                                            content_sprite
                                                                .clone()
                                                                .filter(|sprite| sprite.page_index == page_index)
                                                                .map(|sprite| {
                                                                    super::content::content_sprite_element(
                                                                        sprite,
                                                                        page_size,
                                                                        coordinate_space,
                                                                    )
                                                                }),
                                                        )
                                                        .children(quality_marker)
                                                        .child(gpui::div()
                                                            .absolute()
                                                            .inset_0()
                                                            .when_some(
                                                                page_markup_opacity
                                                                    .filter(|(page, _)| *page == page_index as usize)
                                                                    .map(|(_, opacity)| opacity),
                                                                |layer, opacity| layer.opacity(opacity),
                                                            )
                                                            .child(annotation_layer(
                                                            document_id,
                                                            page_index,
                                                            page_size,
                                                            pdf_page_size,
                                                            rotation,
                                                            coordinate_space,
                                                            scene,
                                                            pending_image_preview.clone().filter(|preview| preview.page_index == page_index),
                                                            pending_text_box_presentation.clone().filter(|preview| preview.page_index == page_index),
                                                            highlights_precomposed,
                                                            image_assets.clone(),
                                                            selection_color,
                                                            hovered_annotation.clone().filter(|(doc, page, _)| {
                                                                *doc == document_id && *page == page_index
                                                            }).map(|(_, _, id)| id),
                                                            hot_annotation_handle.clone().filter(|(doc, page, _, _)| {
                                                                *doc == document_id && *page == page_index
                                                            }).map(|(_, _, id, index)| (id, index)),
                                                            focused_annotation_id.clone(),
                                                            cx.theme().border,
                                                            construction_grid_spacing_mm,
                                                            (page_index == current_page)
                                                                .then(|| semantic_snap_decision.clone())
                                                                .flatten(),
                                                            (page_index == current_page)
                                                                .then(|| object_snap_tracking_result.clone())
                                                                .flatten(),
                                                            if page_index == current_page {
                                                                relationship_snap_guides.clone()
                                                            } else {
                                                                Vec::new()
                                                            },
                                                            (page_index == current_page)
                                                                .then_some(angle_snap_guide)
                                                                .flatten(),
                                                            active_selection_marquee
                                                                .as_ref()
                                                                .filter(|(active_page, _)| {
                                                                    *active_page == page_index
                                                                })
                                                                .map(|(_, marquee)| marquee.clone()),
                                                            (!other_pane).then(|| cx.entity().downgrade()),
                                                            painted_viewer,
                                                            calibration_preview.clone().filter(|preview| {
                                                                preview.document_id == document_id
                                                                    && preview.page_index == page_index
                                                            }),
                                                        )))
                                                        .children(
                                                            page_text_overlays
                                                                .iter()
                                                                .find(|overlay| overlay.page_index == page_index)
                                                                .cloned()
                                                                .map(|overlay| {
                                                                    let overlay_id = pane_id(
                                                                        document_edit_pdf_overlay_id(
                                                                            document_id,
                                                                            page_index,
                                                                        ),
                                                                        other_pane,
                                                                    );
                                                                    gpui::div()
                                                                        .absolute()
                                                                        .inset_0()
                                                                        .debug_selector(move || overlay_id)
                                                                        .child(page_text::page_text_overlay_element(
                                                                            overlay,
                                                                            page_size,
                                                                            coordinate_space,
                                                                            selection_color,
                                                                            warning_colors,
                                                                        ))
                                                                }),
                                                        )
                                                        .children(
                                                            form_overlays
                                                                .iter()
                                                                .find(|overlay| overlay.page_index == page_index)
                                                                .cloned()
                                                                .map(|overlay| {
                                                                    forms::form_overlay_element(
                                                                        overlay,
                                                                        page_size,
                                                                        coordinate_space,
                                                                        selection_color,
                                                                        form_owner.clone(),
                                                                    )
                                                                }),
                                                        )
                                                        .children(render_error_surface)
                                                        .children(
                                                            page_outline
                                                                .as_ref()
                                                                .filter(|outline| outline.pages.contains(&page_index))
                                                                .zip(outline_color)
                                                                .map(|(_, color)| page_outline_element(color)),
                                                        )
                                                },
                                            )),
                                            ),
                                    )
                                })
                                // Before its first layout a document draws no page:
                                // a page fitted to the window here, then laid out at
                                // its zoom a frame later, would jump in size.
                                .when(document_opening_batch_count > 0, |viewport| {
                                    let status = if document_opening_batch_count == 1 {
                                        "Opening PDF".to_owned()
                                    } else {
                                        format!(
                                            "Opening PDFs from {document_opening_batch_count} requests"
                                        )
                                    };
                                    viewport.child(
                                        v_flex()
                                            .id(DOCUMENT_OPEN_STATUS_ID)
                                            .debug_selector(|| DOCUMENT_OPEN_STATUS_ID.into())
                                            .absolute()
                                            .inset_0()
                                            .occlude()
                                            .items_center()
                                            .justify_center()
                                            .gap_3()
                                            .bg(cx.theme().background)
                                            .role(Role::Status)
                                            .aria_label(status.clone())
                                            .a11y_synthetic_children(|builder| {
                                                builder.parent_node().set_live(Live::Polite)
                                            })
                                            .child(status)
                                            .child(
                                                gpui::div()
                                                    .id(DOCUMENT_OPEN_PROGRESS_ID)
                                                    .debug_selector(|| {
                                                        DOCUMENT_OPEN_PROGRESS_ID.into()
                                                    })
                                                    .w(px(240.))
                                                    .child(
                                                        Progress::new(
                                                            "document-active-open-progress-component",
                                                        )
                                                        .loading(true),
                                                    ),
                                            ),
                                    )
                                })
    }
}

/// The pages the canvas outlines: the current page, or the pages the Scale
/// panel applies to.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PageOutline {
    pub pages: BTreeSet<u32>,
    /// The Scale is hovered or its panel open: the outline is stronger.
    pub emphasized: bool,
}

/// The page's outline, just outside its edge: a theme colour, not the
/// paper's.
pub(super) const PAGE_OUTLINE_WIDTH: f32 = 2.;

fn page_outline_element(color: Hsla) -> gpui::Div {
    gpui::div()
        .absolute()
        .left(px(-PAGE_OUTLINE_WIDTH))
        .top(px(-PAGE_OUTLINE_WIDTH))
        .right(px(-PAGE_OUTLINE_WIDTH))
        .bottom(px(-PAGE_OUTLINE_WIDTH))
        .border(px(PAGE_OUTLINE_WIDTH))
        .border_color(color)
}

impl DocumentWorkspace {
    /// What the canvas outlines: while the toolbar's Scale is hovered or
    /// its panel open, the pages it acts on; otherwise the current page,
    /// once more than one page is in view.
    pub(super) fn page_outline(
        &self,
        document_id: DocumentId,
        current_page: u32,
        several_pages_visible: bool,
        cx: &App,
    ) -> Option<PageOutline> {
        let panel_pages = self.page_scale_control.as_ref().and_then(|control| {
            let control = control.read(cx);
            (control.is_open() && control.target().map(|(id, _)| id) == Some(document_id))
                .then(|| control.outlined_pages(cx))
        });
        let emphasized = panel_pages.is_some() || self.page_scale_hovered;
        if !emphasized && !several_pages_visible {
            return None;
        }
        let pages = panel_pages.unwrap_or_else(|| BTreeSet::from([current_page]));
        Some(PageOutline { pages, emphasized })
    }
}
