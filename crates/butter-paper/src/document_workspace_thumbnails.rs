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
                            let selected_pages = Arc::new(
                                self.page_selection
                                    .as_ref()
                                    .filter(|selection| selection.document_id == document_id)
                                    .map(|selection| selection.pages.clone())
                                    .unwrap_or_default(),
                            );
                            // Shown only while thumbnails are dragged.
                            let drop_before = self
                                .page_drop_target
                                .filter(|(owner, _)| *owner == document_id && cx.has_active_drag())
                                .map(|(_, page)| page);
                            let can_paste = self.can_paste_pages(document_id, cx);
                            let thumbnail_focus = self.thumbnail_focus.clone();
                            let strip_scroll = thumbnail_scroll.clone();
                            let strip_control = cx.entity().downgrade();
                            v_flex().id("document-left-sidebar")
                                .on_prepaint(self.left_sidebar_sizing.measure())
                                .debug_selector(|| "document-left-sidebar".into())
                                .size_full().min_h_0()
                                .bg(cx.theme().background)
                                .child(crate::property_controls::PropertyInspectorHeader::new(
                                    "document-left-sidebar-header",
                                    "Page Thumbnails",
                                ).trailing({
                                    let focus = self.thumbnail_focus.clone();
                                    accessible_icon_button(
                                        Button::new(DOCUMENT_THUMBNAIL_ADD_PAGE_ID)
                                            .debug_selector(|| DOCUMENT_THUMBNAIL_ADD_PAGE_ID.into())
                                            .icon(IconName::Plus)
                                            .ghost()
                                            .small()
                                            .disabled(ink_mutation_disabled)
                                            .tooltip("Add page"),
                                        "Add page",
                                    )
                                    .dropdown_menu(move |menu, _, _| {
                                        menu.action_context(focus.clone())
                                            .menu("After Current Page", Box::new(InsertPageAfter))
                                            .menu("At End of Document", Box::new(InsertPageAtEnd))
                                    })
                                }))
                                .child(
                            gpui::div()
                                .id(DOCUMENT_THUMBNAIL_STRIP_ID)
                                .debug_selector(|| DOCUMENT_THUMBNAIL_STRIP_ID.into())
                                .track_focus(&self.thumbnail_focus)
                                .key_context(pages::PAGE_THUMBNAILS_CONTEXT)
                                // A press elsewhere gives the keyboard back to
                                // the page, so Copy and Delete act on markups.
                                .on_mouse_down_out({
                                    let control = strip_control.clone();
                                    move |_, window, cx| {
                                        let _ = control.update(cx, |workspace, cx| {
                                            if workspace.thumbnail_focus.contains_focused(window, cx) {
                                                workspace.workspace_focus.focus(window, cx);
                                            }
                                        });
                                    }
                                })
                                // Dragged thumbnails scroll the list near its
                                // top and bottom edges.
                                .on_drag_move::<pages::PageDrag>(move |event, window, _| {
                                    let edge = px(48.);
                                    let y = event.event.position.y;
                                    let step = if y < event.bounds.top() + edge && y >= event.bounds.top() {
                                        px(16.)
                                    } else if y > event.bounds.bottom() - edge && y <= event.bounds.bottom() {
                                        px(-16.)
                                    } else {
                                        return;
                                    };
                                    let handle = strip_scroll.0.borrow().base_handle.clone();
                                    let offset = handle.offset();
                                    let max = handle.max_offset();
                                    handle.set_offset(point(
                                        offset.x,
                                        (offset.y + step).clamp(-max.y, px(0.)),
                                    ));
                                    window.refresh();
                                })
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
                                                    // Selected thumbnails, or the current page
                                                    // when none are.
                                                    let selected = selected_pages.contains(&page_index)
                                                        || (selected_pages.is_empty() && page_index == current_page);
                                                    let dragged = if selected_pages.contains(&page_index) {
                                                        selected_pages.iter().copied().collect::<Vec<_>>()
                                                    } else {
                                                        vec![page_index]
                                                    };
                                                    let drop_line = |top: bool| {
                                                        gpui::div()
                                                            .absolute()
                                                            .left_0()
                                                            .right_0()
                                                            .h(px(2.))
                                                            .when(top, |line| line.top(px(1.)))
                                                            .when(!top, |line| line.bottom(px(1.)))
                                                            .rounded_full()
                                                            .bg(cx.theme().primary)
                                                    };
                                                    // Each row keeps a gap above and below its
                                                    // page for the line, inside the row, as the
                                                    // list clips anything outside its rows.
                                                    let last = page_index + 1 == thumbnail_count as u32;
                                                    gpui::div().py_1().relative()
                                                        .when(drop_before == Some(page_index), |row| row.child(drop_line(true)))
                                                        .when(last && drop_before == Some(page_index + 1), |row| row.child(drop_line(false)))
                                                        .child(v_flex()
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
                                                        .border_color(if selected {
                                                            cx.theme().primary
                                                        } else {
                                                            cx.theme().border
                                                        })
                                                        .when(selected_pages.len() > 1 && selected, |row| {
                                                            row.bg(cx.theme().muted)
                                                        })
                                                        .on_click({
                                                            let focus = thumbnail_focus.clone();
                                                            move |event, window, cx| {
                                                                focus.focus(window, cx);
                                                                let modifiers = event.modifiers();
                                                                let _ = click_control.update(
                                                                    cx,
                                                                    |workspace, cx| {
                                                                        workspace.click_thumbnail(
                                                                            document_id,
                                                                            page_index,
                                                                            modifiers,
                                                                            cx,
                                                                        );
                                                                    },
                                                                );
                                                            }
                                                        })
                                                        .on_mouse_down(gpui::MouseButton::Right, {
                                                            let focus = thumbnail_focus.clone();
                                                            let control = thumbnail_control.clone();
                                                            move |_, window, cx| {
                                                                focus.focus(window, cx);
                                                                let _ = control.update(cx, |workspace, cx| {
                                                                    workspace.select_thumbnail_for_menu(document_id, page_index, cx);
                                                                });
                                                            }
                                                        })
                                                        .when(!ink_mutation_disabled, |row| {
                                                            let control = thumbnail_control.clone();
                                                            let drop_control = thumbnail_control.clone();
                                                            row.on_drag(
                                                                pages::PageDrag { document_id, pages: dragged.clone() },
                                                                |drag, _, _, cx| drag.ghost(cx),
                                                            )
                                                            .on_drag_move::<pages::PageDrag>(move |event, _, cx| {
                                                                let _ = control.update(cx, |workspace, cx| {
                                                                    workspace.update_page_drop_target(event, page_index, cx);
                                                                });
                                                            })
                                                            .on_drop::<pages::PageDrag>(move |drag, _, cx| {
                                                                let _ = drop_control.update(cx, |workspace, cx| {
                                                                    workspace.drop_pages(drag, cx);
                                                                });
                                                            })
                                                        })
                                                        .child(h_flex().w_full().gap_1()
                                                            .child(h_flex().flex_1().min_w_0().child(accessible_icon_button(
                                                                Button::new(format!("thumbnail-{page_index}-open"))
                                                                    .debug_selector(move || format!("thumbnail-{page_index}-open"))
                                                                    .ghost().small().label(page_label.clone())
                                                                    .toggled(page_index == current_page)
                                                                    .on_click({
                                                                        let owner = thumbnail_control.clone();
                                                                        let focus = thumbnail_focus.clone();
                                                                        move |event, window, cx| {
                                                                            focus.focus(window, cx);
                                                                            let modifiers = event.modifiers();
                                                                            let _ = owner.update(cx, |workspace, cx| workspace.click_thumbnail(document_id, page_index, modifiers, cx));
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
                                                        )
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
                                                                        // The page's edge, so a white page reads
                                                                        // as paper against the panel.
                                                                        let edge = cx.theme().border;
                                                                        page.child(
                                                                            canvas(|_, _, _| (), move |bounds, _, window, _| {
                                                                                if let Some((page_bounds, _)) = contained_page_bounds_for_space(bounds, page_size, coordinate_space) {
                                                                                    let _ = window.paint_image(page_bounds, page_bounds, Default::default(), image.clone(), 0, false);
                                                                                    window.paint_quad(gpui::outline(page_bounds, edge, gpui::BorderStyle::Solid));
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
                                                        )
                                                        .context_menu({
                                                            let focus = thumbnail_focus.clone();
                                                            let control = thumbnail_control.clone();
                                                            move |menu, _, cx| {
                                                                let selected = control
                                                                    .upgrade()
                                                                    .map_or(1, |workspace| workspace.read(cx).selected_pages(document_id, cx).len());
                                                                let delete = if selected == 1 {
                                                                    "Delete Page".to_owned()
                                                                } else {
                                                                    format!("Delete {selected} Pages")
                                                                };
                                                                menu.action_context(focus.clone())
                                                                    .menu("Insert Blank Page Before", Box::new(InsertPageBefore))
                                                                    .menu("Insert Blank Page After", Box::new(InsertPageAfter))
                                                                    .menu("Duplicate", Box::new(DuplicatePages))
                                                                    .separator()
                                                                    .menu("Cut", Box::new(CutPages))
                                                                    .menu("Copy", Box::new(CopyPages))
                                                                    .menu_with_disabled("Paste After", Box::new(PastePages), !can_paste)
                                                                    .separator()
                                                                    .menu("Rotate Left", Box::new(RotatePagesLeft))
                                                                    .menu("Rotate Right", Box::new(RotatePagesRight))
                                                                    .separator()
                                                                    .menu_with_disabled(delete, Box::new(DeletePages), selected >= thumbnail_count)
                                                            }
                                                        }))
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
