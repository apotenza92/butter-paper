//! The scrolled document viewport: page tiles, annotation layers and the
//! opening status over them.

use super::*;
use crate::document_viewer::ViewerPlanSnapshot;
use gpui::{Canvas, Hsla};

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
    pub annotation_scene: AnnotationScene,
    pub construction_grid_spacing_mm: Option<f64>,
    pub current_coordinate_space: PageCoordinateSpace,
    pub current_highlights_precomposed: bool,
    pub current_image: Option<Arc<RenderImage>>,
    pub current_page: u32,
    pub current_page_rotation: PageRotation,
    pub current_page_size: (f32, f32),
    pub current_pdf_page_size: (f32, f32),
    pub document_id: DocumentId,
    pub document_opening_batch_count: usize,
    pub focused_annotation_id: Option<MarkupId>,
    pub has_viewer_pages: bool,
    pub hot_annotation_handle: Option<(DocumentId, u32, MarkupId, usize)>,
    pub hovered_annotation: Option<(DocumentId, u32, MarkupId)>,
    pub image_assets: Arc<HashMap<String, Arc<RenderImage>>>,
    pub object_snap_tracking_result: Option<ObjectSnapTrackingResult>,
    pub pending_image_preview: Option<PendingImagePreview>,
    pub pending_text_box_presentation: Option<PendingTextBoxPresentation>,
    pub page_text_overlays: Vec<PageTextOverlay>,
    pub relationship_snap_guides: Vec<RelationshipSnapGuide>,
    pub selection_color: Hsla,
    pub semantic_snap_decision: Option<SemanticSnapDecision>,
    pub viewer_pages: Vec<ViewerPageRender>,
    pub viewer_pan_margin: (f32, f32),
    pub viewer_plan: Option<ViewerPlanSnapshot>,
    pub viewer_scroll: ScrollHandle,
    pub viewport_cursor_style: CursorStyle,
    pub viewport_observer: Canvas<()>,
}

impl DocumentWorkspace {
    pub(super) fn render_document_viewport(
        &self,
        inputs: ViewportInputs,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let ViewportInputs {
            active_selection_marquee,
            annotation_scene,
            construction_grid_spacing_mm,
            current_coordinate_space,
            current_highlights_precomposed,
            current_image,
            current_page,
            current_page_rotation,
            current_page_size,
            current_pdf_page_size,
            document_id,
            document_opening_batch_count,
            focused_annotation_id,
            has_viewer_pages,
            hot_annotation_handle,
            hovered_annotation,
            image_assets,
            object_snap_tracking_result,
            pending_image_preview,
            pending_text_box_presentation,
            page_text_overlays,
            relationship_snap_guides,
            selection_color,
            semantic_snap_decision,
            viewer_pages,
            viewer_pan_margin,
            viewer_plan,
            viewer_scroll,
            viewport_cursor_style,
            viewport_observer,
        } = inputs;
        let page_text_input = page_text_overlays
            .iter()
            .find_map(|overlay| overlay.input.clone())
            .map(|(input, focus)| page_text::page_text_input_element(input, focus));
        gpui::div()
                                .id(DOCUMENT_VIEWPORT_ID)
                                .debug_selector(|| DOCUMENT_VIEWPORT_ID.into())
                                .relative()
                                .size_full()
                                .children(page_text_input)
                                .cursor(viewport_cursor_style)
                                .overflow_scroll()
                                .track_scroll(&viewer_scroll)
                                .on_scroll_wheel(cx.listener(
                                    move |workspace, event: &ScrollWheelEvent, window, cx| {
                                        if workspace.handle_viewport_wheel(document_id, event, window, cx) {
                                            window.prevent_default();
                                        }
                                    },
                                ))
                                .on_pinch(cx.listener(
                                    move |workspace, event: &gpui::PinchEvent, window, cx| {
                                        workspace.handle_viewport_pinch(document_id, event, window, cx);
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
                                                    let page_id = document_viewer_page_id(
                                                        document_id,
                                                        layout.page,
                                                    );
                                                    let page_selector = page_id.clone();
                                                    let page_index = layout.page as u32;
                                                    let quality_marker = quality.map(|quality| {
                                                        let quality_id = document_viewer_quality_id(
                                                            document_id,
                                                            layout.page,
                                                            quality,
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
                                                    let page_tiles = tiles
                                                        .into_iter()
                                                        .enumerate()
                                                        .map(|(tile_index, (request, image))| {
                                                            let scale = (request
                                                                .device_scale_millis
                                                                as f32
                                                                / 1_000.)
                                                                .max(0.1);
                                                            let tile_id = document_viewer_tile_id(
                                                                document_id,
                                                                plan.generation,
                                                                layout.page,
                                                                tile_index,
                                                            );
                                                            let tile_selector = tile_id.clone();
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
                                                                .w(px(
                                                                    request.crop.width as f32
                                                                        / scale,
                                                                ))
                                                                .h(px(
                                                                    request.crop.height as f32
                                                                        / scale,
                                                                ))
                                                                .child(
                                                                    img(image)
                                                                        .size_full()
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
                                                        .bg(gpui::rgb(0xffffff))
                                                        .when(
                                                            page_index == current_page,
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
                                                        .children(page_tiles)
                                                        .children(quality_marker)
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
                                                            active_selection_marquee
                                                                .as_ref()
                                                                .filter(|(active_page, _)| {
                                                                    *active_page == page_index
                                                                })
                                                                .map(|(_, marquee)| marquee.clone()),
                                                            Some(cx.entity().downgrade()),
                                                            painted_viewer,
                                                        ))
                                                        .children(
                                                            page_text_overlays
                                                                .iter()
                                                                .find(|overlay| overlay.page_index == page_index)
                                                                .cloned()
                                                                .map(|overlay| {
                                                                    page_text::page_text_overlay_element(
                                                                        overlay,
                                                                        page_size,
                                                                        coordinate_space,
                                                                        selection_color,
                                                                    )
                                                                }),
                                                        )
                                                        .children(render_error_surface)
                                                },
                                            )),
                                            ),
                                    )
                                })
                                .when(!has_viewer_pages, |viewport| {
                                    viewport.when_some(current_image, |viewport, image| {
                                        viewport.child(
                                            gpui::div()
                                                .relative()
                                                .size_full()
                                                .child(
                                                    img(image)
                                                        .size_full()
                                                        .object_fit(ObjectFit::Contain),
                                                )
                                                .child(annotation_layer(
                                                    document_id,
                                                    current_page,
                                                    current_page_size,
                                                    current_pdf_page_size,
                                                    current_page_rotation,
                                                    current_coordinate_space,
                                                    annotation_scene,
                                                    pending_image_preview.clone().filter(|preview| preview.page_index == current_page),
                                                    pending_text_box_presentation.clone().filter(|preview| preview.page_index == current_page),
                                                    current_highlights_precomposed,
                                                    image_assets,
                                                    selection_color,
                                                    hovered_annotation.clone().filter(|(doc, page, _)| {
                                                        *doc == document_id && *page == current_page
                                                    }).map(|(_, _, id)| id),
                                                    hot_annotation_handle.clone().filter(|(doc, page, _, _)| {
                                                        *doc == document_id && *page == current_page
                                                    }).map(|(_, _, id, index)| (id, index)),
                                                    focused_annotation_id.clone(),
                                                    cx.theme().border,
                                                    construction_grid_spacing_mm,
                                                    semantic_snap_decision.clone(),
                                                    object_snap_tracking_result.clone(),
                                                    relationship_snap_guides.clone(),
                                                    active_selection_marquee
                                                        .as_ref()
                                                        .filter(|(active_page, _)| {
                                                            *active_page == current_page
                                                        })
                                                        .map(|(_, marquee)| marquee.clone()),
                                                    Some(cx.entity().downgrade()),
                                                    None,
                                                )),
                                        )
                                    })
                                })
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
