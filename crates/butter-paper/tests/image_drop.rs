//! Images dropped on a document's window are placed at once where they are
//! dropped, selected, as one undoable step.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use butter_paper::document_resource::{
    NativeDocumentResource, OpenedNativeDocument, RasterSurface, ThumbnailSurface,
};
use butter_paper::document_workspace::{
    DocumentId, DocumentWorkspace, document_annotation_layer_id, init_document_workspace_actions,
};
use butter_paper::page_view_control::PageViewMode;
use butter_paper::pdf_text_edit::PageTextLayer;
use butter_paper::pdf_worker::DocumentEdits;
use gpui::{AppContext as _, TestAppContext, point, px};
use gpui_component::Root;

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .unwrap()
}

struct BlankResource {
    released: AtomicBool,
}

impl NativeDocumentResource for BlankResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4302)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_thumbnail(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_tile(
        &self,
        request: butter_paper::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn page_text(&self, page_index: u32) -> Result<PageTextLayer, String> {
        Ok(PageTextLayer {
            page_index,
            ..Default::default()
        })
    }

    fn set_document_edits(&self, _: &DocumentEdits) -> Result<(), String> {
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

/// A workspace showing a blank 612 x 792 page.
fn blank_workspace(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<DocumentWorkspace>,
    DocumentId,
    &mut gpui::VisualTestContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("blank.pdf"), cx)
    });
    let document_id = request.document_id;
    workspace.update(cx, |workspace, cx| {
        let opened = OpenedNativeDocument::new(
            "blank.pdf",
            vec![(612., 792.)],
            raster(32, 40),
            vec![ThumbnailSurface::new(0, raster(8, 10))],
            std::sync::Arc::new(BlankResource {
                released: AtomicBool::new(false),
            }),
        )
        .unwrap();
        workspace.apply_open_result(&request, Ok(opened), cx);
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 100., cx);
        workspace
            .refresh_viewport_async(document_id, 900., 1000., 1., cx)
            .unwrap();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    (workspace, document_id, cx)
}

/// The window position of point (`x`, `y`) of the upright page, and the
/// page's bounds in the window.
fn window_point(
    cx: &mut gpui::VisualTestContext,
    document_id: DocumentId,
    (x, y): (f64, f64),
) -> (gpui::Point<gpui::Pixels>, gpui::Bounds<gpui::Pixels>) {
    let (width, height) = (612_f32, 792_f32);
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / width).min(f32::from(layer.size.height) / height);
    let origin = point(
        layer.origin.x + (layer.size.width - px(width * scale)) / 2.,
        layer.origin.y + (layer.size.height - px(height * scale)) / 2.,
    );
    (
        point(
            origin.x + px(x as f32 * scale),
            origin.y + px((f64::from(height) - y) as f32 * scale),
        ),
        layer,
    )
}

fn checker() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/bp-image-checker-v1.png")
}

#[gpui::test]
fn a_dropped_image_is_placed_where_it_is_dropped(cx: &mut TestAppContext) {
    let (workspace, document_id, cx) = blank_workspace(cx);
    let (at, _) = window_point(cx, document_id, (200., 500.));
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.drop_paths(vec![checker()], at, cx)
    }));
    cx.run_until_parked();
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(snapshot.images.len(), 1, "placed at once, with no click");
    let rect = snapshot.images[0].rect;
    assert!(
        rect.x <= 200. + 1. && rect.x + rect.width >= 200. - 1.,
        "{rect:?}"
    );
    assert!(
        rect.y <= 500. + 1. && rect.y + rect.height >= 500. - 1.,
        "{rect:?}"
    );
    assert_eq!(snapshot.selected_id.as_ref(), Some(&snapshot.images[0].id));
    assert_eq!(snapshot.undo_depth, 1, "one undoable step");
    // The next click selects or draws as before: no image is pending.
    let tool = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_tool(document_id, cx)
    });
    assert_eq!(
        tool,
        Some(butter_paper::annotation_adapter::AnnotationTool::Select)
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(snapshot.images.is_empty());
}

#[gpui::test]
fn an_image_dropped_beside_the_page_lands_on_its_nearest_edge(cx: &mut TestAppContext) {
    let (workspace, document_id, cx) = blank_workspace(cx);
    let (_, page) = window_point(cx, document_id, (0., 0.));
    // Left of the page, level with its middle.
    let at = point(page.left() - px(30.), page.center().y);
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.drop_paths(vec![checker(), checker()], at, cx)
    }));
    cx.run_until_parked();
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(snapshot.images.len(), 2, "each image dropped is placed");
    assert!(
        snapshot.images[0].rect.x >= -1.,
        "{:?}",
        snapshot.images[0].rect
    );
    // The second steps aside so it does not hide the first.
    assert_ne!(snapshot.images[0].rect, snapshot.images[1].rect);
}

#[gpui::test]
fn files_that_are_not_images_are_not_placed(cx: &mut TestAppContext) {
    let (workspace, document_id, cx) = blank_workspace(cx);
    let (at, _) = window_point(cx, document_id, (200., 500.));
    assert!(!workspace.update(cx, |workspace, cx| {
        workspace.drop_paths(vec![PathBuf::from("notes.txt")], at, cx)
    }));
    cx.run_until_parked();
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(snapshot.images.is_empty());
}

/// Applies an image property change as the properties panel does.
fn set_aspect_locked(
    workspace: &gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    locked: bool,
    cx: &mut gpui::VisualTestContext,
) {
    use butter_paper::engineering_visual_property_inspector::{
        EngineeringVisualPropertyEvent, EngineeringVisualPropertyKind,
        EngineeringVisualPropertyPatch,
    };
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let changed = workspace
        .update(cx, |workspace, cx| {
            workspace.apply_engineering_visual_property_event(
                &EngineeringVisualPropertyEvent {
                    document_id,
                    annotation_id: snapshot.images[0].id.clone(),
                    expected_revision: snapshot.revision,
                    expected_kind: EngineeringVisualPropertyKind::Image,
                    patch: EngineeringVisualPropertyPatch::AspectLocked(locked),
                },
                cx,
            )
        })
        .unwrap();
    assert!(changed);
}

#[gpui::test]
fn new_images_keep_their_proportions_until_unlocked(cx: &mut TestAppContext) {
    let (workspace, document_id, cx) = blank_workspace(cx);
    let (at, _) = window_point(cx, document_id, (300., 400.));
    workspace.update(cx, |workspace, cx| {
        workspace.drop_paths(vec![checker()], at, cx)
    });
    cx.run_until_parked();
    let snapshot = |cx: &mut gpui::VisualTestContext| {
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(document_id, cx)
            })
            .unwrap()
    };
    let placed = snapshot(cx).images[0].clone();
    assert!(placed.aspect_locked, "locked by default");
    let ratio = placed.rect.width / placed.rect.height;

    // Unlocked, it stretches; locked again, it takes its own shape back,
    // keeping its width and centre, as one undo step.
    set_aspect_locked(&workspace, document_id, false, cx);
    assert!(!snapshot(cx).images[0].aspect_locked);
    let mut stretched = placed.rect;
    stretched.height += 40.;
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_image_rect(document_id, stretched, cx)
        })
        .unwrap();
    set_aspect_locked(&workspace, document_id, true, cx);
    let relocked = snapshot(cx).images[0].clone();
    assert!(relocked.aspect_locked);
    assert_eq!(relocked.rect.width, stretched.width);
    assert!((relocked.rect.width / relocked.rect.height - ratio).abs() < 1e-6);
    let centre = |rect: butter_paper::annotation_model::PdfRect| rect.y + rect.height / 2.;
    assert!((centre(relocked.rect) - centre(stretched)).abs() < 1e-6);
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    let undone = snapshot(cx).images[0].clone();
    assert!(!undone.aspect_locked);
    assert_eq!(undone.rect, stretched);
}

#[gpui::test]
fn with_the_setting_off_new_images_are_free(cx: &mut TestAppContext) {
    let (workspace, document_id, cx) = blank_workspace(cx);
    cx.update(|_, cx| cx.set_global(butter_paper::application_shell::LockImageProportions(false)));
    let (at, _) = window_point(cx, document_id, (300., 400.));
    workspace.update(cx, |workspace, cx| {
        workspace.drop_paths(vec![checker()], at, cx)
    });
    cx.run_until_parked();
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(!snapshot.images[0].aspect_locked);
}
