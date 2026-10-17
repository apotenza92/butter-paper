//! The page thumbnails sidebar.

use super::*;
use gpui::Hsla;
use gpui_component::resizable::ResizablePanel;

/// One thumbnail: page index, display size, PDF size and rotation,
/// coordinate space, rendered image, page-scale label and render error.
pub(super) type ThumbnailRow = (
    u32,
    (f32, f32),
    ((f32, f32), PageRotation),
    PageCoordinateSpace,
    Option<Arc<RenderImage>>,
    Option<String>,
    Option<String>,
);

/// The rows the thumbnails list lays out in a frame, gathered as each row
/// prepaints (only rows on screen do) and handed to the workspace once the
/// frame is done, so it draws the thumbnails shown first.
#[derive(Default)]
pub(super) struct ThumbnailRowsShown {
    rows: Vec<u32>,
    flush_queued: bool,
}

impl ThumbnailRowsShown {
    fn record(
        shown: &Rc<RefCell<Self>>,
        document_id: DocumentId,
        page_index: u32,
        owner: WeakEntity<DocumentWorkspace>,
        cx: &mut App,
    ) {
        let mut state = shown.borrow_mut();
        state.rows.push(page_index);
        if std::mem::replace(&mut state.flush_queued, true) {
            return;
        }
        let shown = shown.clone();
        cx.defer(move |cx| {
            let rows = {
                let mut state = shown.borrow_mut();
                state.flush_queued = false;
                std::mem::take(&mut state.rows)
            };
            let _ = owner.update(cx, |workspace, cx| {
                workspace.show_thumbnail_rows(document_id, &rows, cx)
            });
        });
    }
}

/// What the thumbnails sidebar shows for the active document.
pub(super) struct ThumbnailSidebarInputs {
    pub document_id: DocumentId,
    pub current_page: u32,
    pub thumbnails: Vec<ThumbnailRow>,
    pub thumbnails_visible: bool,
    pub thumbnail_scroll: UniformListScrollHandle,
    pub hovered_annotation: Option<(DocumentId, u32, MarkupId)>,
    pub focused_annotation_id: Option<MarkupId>,
    pub image_assets: Arc<HashMap<String, Arc<RenderImage>>>,
    pub selection_color: Hsla,
    pub ink_mutation_disabled: bool,
}

impl DocumentWorkspace {
    pub(super) fn render_thumbnail_sidebar(
        &self,
        inputs: ThumbnailSidebarInputs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ResizablePanel {
        let ThumbnailSidebarInputs {
            document_id,
            current_page,
            thumbnails,
            thumbnails_visible,
            thumbnail_scroll,
            hovered_annotation,
            focused_annotation_id,
            image_assets,
            selection_color,
            ink_mutation_disabled,
        } = inputs;
        resizable_panel()
                        .visible(thumbnails_visible)
                        .size(window.rem_size() * 18.75)
                        // Preserve full stock controls when the interface scale grows.
                        .size_range(window.rem_size() * 16. ..window.rem_size() * 24.)
                        .flex_none()
                        .child({
                            let thumbnail_rows = Arc::new(thumbnails);
                            let thumbnail_count = thumbnail_rows.len();
                            let thumbnail_hovered = hovered_annotation.clone();
                            let thumbnail_focused = focused_annotation_id.clone();
                            let thumbnail_control = cx.entity().downgrade();
                            let thumbnail_images = image_assets.clone();
                            let rows_shown = self.thumbnail_rows_shown.clone();
                            v_flex().id("document-left-sidebar")
                                .on_prepaint(self.left_sidebar_sizing.measure())
                                .debug_selector(|| "document-left-sidebar".into())
                                .size_full().min_h_0()
                                .bg(cx.theme().background)
                                .child(h_flex()
                                    .id("document-left-sidebar-header")
                                    .debug_selector(|| "document-left-sidebar-header".into())
                                    .w_full().h_12().flex_shrink_0()
                                    .justify_center().px_2().text_xs().font_semibold()
                                    .border_b_1().border_color(cx.theme().border)
                                    .child("Page Thumbnails"))
                                .child(
                            gpui::div()
                                .id(DOCUMENT_THUMBNAIL_STRIP_ID)
                                .debug_selector(|| DOCUMENT_THUMBNAIL_STRIP_ID.into())
                                .role(Role::List)
                                .aria_label("Document pages")
                                .relative()
                                .w_full()
                                .h_full()
                                .flex_1().min_h_0()
                                .child(
                                    gpui::div().size_full().p_2().child(
                                    uniform_list(
                                        "document-workspace-thumbnail-list",
                                        thumbnail_count,
                                        move |range, _, cx| {
                                            range
                                                .map(|row_index| {
                                                    let (
                                                        page_index,
                                                        page_size,
                                                        (pdf_page_size, rotation),
                                                        coordinate_space,
                                                        image,
                                                        scale_label,
                                                        preview_error,
                                                    ) = thumbnail_rows[row_index].clone();
                                                    // Built only for rows the list lays out,
                                                    // not for every page on every frame.
                                                    let workspace = thumbnail_control.upgrade();
                                                    let scene = workspace.as_ref().map_or_else(
                                                        || AnnotationAdapter::default().thumbnail_scene(document_id.value(), page_index),
                                                        |workspace| workspace.read(cx).thumbnail_annotation_scene(document_id, page_index, cx),
                                                    );
                                                    let highlights_precomposed = workspace
                                                        .as_ref()
                                                        .and_then(|workspace| workspace.read(cx).session(document_id, cx).cloned())
                                                        .is_some_and(|session| {
                                                            session.read(cx).highlight_composite.annotation_revision == scene.revision
                                                        });
                                                    let stable_id = document_thumbnail_id(
                                                        document_id,
                                                        page_index,
                                                    );
                                                    let selector = stable_id.clone();
                                                    let click_control =
                                                        thumbnail_control.clone();
                                                    let page_label =
                                                        format!("Page {}", page_index + 1);
                                                    gpui::div().pb_2().child(v_flex()
                                                        .id(stable_id.clone())
                                                        .debug_selector(move || {
                                                            selector.clone()
                                                        })
                                                        .role(Role::Group)
                                                        .accessibility_id(stable_id)
                                                        .aria_label(page_label.clone())
                                                        .aria_position_in_set(row_index + 1)
                                                        .aria_size_of_set(thumbnail_count)
                                                        .aria_selected(page_index == current_page)
                                                        .when(page_index == current_page, |row| {
                                                            row.aria_active_descendant()
                                                        })
                                                        .w_full().h_48()
                                                        .flex_none()
                                                        .p_2()
                                                        .gap_2()
                                                        .on_prepaint({
                                                            let owner = thumbnail_control.clone();
                                                            let rows_shown = rows_shown.clone();
                                                            move |_, _, cx| ThumbnailRowsShown::record(&rows_shown, document_id, page_index, owner.clone(), cx)
                                                        })
                                                        .rounded(cx.theme().radius)
                                                        .border_1()
                                                        .border_color(if page_index == current_page {
                                                            cx.theme().primary
                                                        } else {
                                                            cx.theme().border
                                                        })
                                                        .on_click(move |_, _, cx| {
                                                            let _ = click_control.update(
                                                                cx,
                                                                |workspace, cx| {
                                                                    workspace
                                                                        .activate_thumbnail_page(
                                                                            document_id,
                                                                            page_index,
                                                                            cx,
                                                                        );
                                                                },
                                                            );
                                                        })
                                                        .child(h_flex().w_full().gap_1()
                                                            .child(h_flex().flex_1().min_w_0().child(accessible_icon_button(
                                                                Button::new(format!("thumbnail-{page_index}-open"))
                                                                    .debug_selector(move || format!("thumbnail-{page_index}-open"))
                                                                    .ghost().small().label(page_label.clone())
                                                                    .toggled(page_index == current_page)
                                                                    .on_click({
                                                                        let owner = thumbnail_control.clone();
                                                                        move |_, _, cx| {
                                                                            let _ = owner.update(cx, |workspace, cx| workspace.activate_thumbnail_page(document_id, page_index, cx));
                                                                            cx.stop_propagation();
                                                                        }
                                                                    }), format!("Open page {}", page_index + 1)))
                                                                .when_some(scale_label, |header, label| header.child(
                                                                    h_flex().flex_1().min_w_0().justify_center().child(
                                                                        gpui::div().id(format!("thumbnail-{page_index}-scale-badge"))
                                                                            .debug_selector(move || format!("thumbnail-{page_index}-scale-badge"))
                                                                            .child(gpui_component::tag::Tag::secondary().small().child(label))
                                                                    )
                                                                )))
                                                            .children([PageRotationDirection::Left, PageRotationDirection::Right].into_iter().map(|direction| {
                                                                let control = thumbnail_control.clone();
                                                                let left = direction == PageRotationDirection::Left;
                                                                let label = if left { "Rotate left" } else { "Rotate right" };
                                                                accessible_icon_button(
                                                                    Button::new(format!("thumbnail-{page_index}-rotate-{}", if left { "left" } else { "right" }))
                                                                        .debug_selector(move || format!("thumbnail-{page_index}-rotate-{}", if left { "left" } else { "right" }))
                                                                        .icon(gpui_component::Icon::default().path(if left { "icons/thumbnail-rotate-left.svg" } else { "icons/thumbnail-rotate-right.svg" }))
                                                                        .ghost().disabled(ink_mutation_disabled).tooltip(label)
                                                                        .on_click(move |_, _, cx| {
                                                                            let _ = control.update(cx, |workspace, cx| workspace.rotate_thumbnail_page_async(document_id, page_index, direction, cx));
                                                                            cx.stop_propagation();
                                                                        }), label)
                                                            })))
                                                        .child(
                                                            h_flex()
                                                                .id(format!("thumbnail-preview-{page_index}"))
                                                                .debug_selector(move || format!("thumbnail-preview-{page_index}"))
                                                                .relative()
                                                                .w_full()
                                                                .justify_center()
                                                                .h_32().flex_none().overflow_hidden()
                                                                .when_some(
                                                                    image.clone(),
                                                                    |page, image| {
                                                                        page.child(
                                                                            canvas(|_, _, _| (), move |bounds, _, window, _| {
                                                                                if let Some((page_bounds, _)) = contained_page_bounds_for_space(bounds, page_size, coordinate_space) {
                                                                                    let _ = window.paint_image(page_bounds, page_bounds, Default::default(), image.clone(), 0, false);
                                                                                }
                                                                            }).size_full()
                                                                        )
                                                                        .child(annotation_layer(
                                                                            document_id,
                                                                            page_index,
                                                                            page_size,
                                                                            pdf_page_size,
                                                                            rotation,
                                                                            coordinate_space,
                                                                            scene,
                                                                            None,
                                                                            None,
                                                                            highlights_precomposed,
                                                                            thumbnail_images.clone(),
                                                                            selection_color,
                                                                            thumbnail_hovered.clone().filter(|(doc, page, _)| {
                                                                                *doc == document_id && *page == page_index
                                                                            }).map(|(_, _, id)| id),
                                                                            None,
                                                                            thumbnail_focused.clone(),
                                                                            cx.theme().border,
                                                                            None,
                                                                            None,
                                                                            None,
                                                                            Vec::new(),
                                                                            None,
                                                                            None,
                                                                            None,
                                                                        ))
                                                                    },
                                                                )
                                                                .when(
                                                                    image.is_none() && preview_error.is_none(),
                                                                    |page| {
                                                                        page.items_center()
                                                                            .justify_center()
                                                                            .bg(cx.theme().muted)
                                                                            .text_xs()
                                                                            .text_color(
                                                                                cx.theme()
                                                                                    .muted_foreground,
                                                                            )
                                                                            .child("Loading preview…")
                                                                    },
                                                                )
                                                                .when_some(preview_error, |page, _| {
                                                                    let owner = thumbnail_control.clone();
                                                                    page.child(v_flex().gap_2().items_center()
                                                                        .child("Preview unavailable")
                                                                        .child(Button::new(format!("thumbnail-{page_index}-retry"))
                                                                            .debug_selector(move || format!("thumbnail-{page_index}-retry"))
                                                                            .small().label("Retry preview").disabled(ink_mutation_disabled)
                                                                            .on_click(move |_, _, cx| {
                                                                                let _ = owner.update(cx, |workspace, cx| workspace.request_thumbnail_preview(document_id, page_index, true, cx));
                                                                                cx.stop_propagation();
                                                                            })))
                                                                }),
                                                        ))
                                                })
                                                .collect::<Vec<_>>()
                                        },
                                    )
                                    .size_full()
                                    .track_scroll(&thumbnail_scroll),
                                    ),
                                )
                                .vertical_scrollbar(&thumbnail_scroll)
                                )
                        })
    }
}
