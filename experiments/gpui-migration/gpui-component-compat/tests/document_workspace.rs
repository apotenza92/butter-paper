#[cfg(unix)]
use std::os::unix::ffi::OsStringExt as _;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt as _;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use butter_paper_gpui_component_compat::document_tab_bar::{
    DOCUMENT_TAB_POINTER_DRAG_THRESHOLD, DOCUMENT_TAB_REORDER_STATUS_ID, TEMPLATE_CONTROL_GROUP_ID,
    TEMPLATE_CREATE_ID, TEMPLATE_ITEM_IDS, TEMPLATE_PICKER_ID, document_tab_drag_id,
    document_tab_drop_target_id,
};
use butter_paper_gpui_component_compat::document_workspace::{
    ApplyDisposition, CloseRequestDisposition, DOCUMENT_ANNOTATION_DELETE_ID,
    DOCUMENT_ANNOTATION_LOCK_ID, DOCUMENT_ANNOTATION_REDO_ID, DOCUMENT_ANNOTATION_UNDO_ID,
    DOCUMENT_ARC_TOOL_ID, DOCUMENT_AREA_TOOL_ID, DOCUMENT_ARROW_TOOL_ID, DOCUMENT_CALLOUT_TOOL_ID,
    DOCUMENT_CLOSE_ID, DOCUMENT_CLOUD_PLUS_TOOL_ID, DOCUMENT_CLOUD_TOOL_ID,
    DOCUMENT_DIMENSION_TOOL_ID, DOCUMENT_DIRTY_CLOSE_CANCEL_ID, DOCUMENT_DIRTY_CLOSE_DISCARD_ID,
    DOCUMENT_DIRTY_CLOSE_ID, DOCUMENT_DIRTY_CLOSE_SAVE_ID, DOCUMENT_ELLIPSE_PROPERTIES_ID,
    DOCUMENT_ELLIPSE_TOOL_ID,
    DOCUMENT_HIGHLIGHT_COLOR_GREEN_ID, DOCUMENT_HIGHLIGHT_OPACITY_50_ID,
    DOCUMENT_HIGHLIGHT_TOOL_ID, DOCUMENT_HIGHLIGHT_WIDTH_18_ID, DOCUMENT_IMAGE_TOOL_ID,
    DOCUMENT_LENGTH_TOOL_ID, DOCUMENT_LINE_TOOL_ID, DOCUMENT_OPEN_ERROR_ALERT_ID,
    DOCUMENT_OPEN_ERROR_DISMISS_ID, DOCUMENT_PAGE_ID, DOCUMENT_PEN_OPACITY_50_ID,
    DOCUMENT_PEN_OPACITY_ID, DOCUMENT_PEN_TOOL_ID, DOCUMENT_POLYGON_TOOL_ID,
    DOCUMENT_POLYLENGTH_TOOL_ID, DOCUMENT_POLYLINE_TOOL_ID, DOCUMENT_RECOVERY_ALERT_ID,
    DOCUMENT_RECOVERY_RETRY_ID, DOCUMENT_RECTANGLE_PROPERTIES_ID, DOCUMENT_RECTANGLE_STROKE_ID,
    DOCUMENT_RECTANGLE_TOOL_ID, DOCUMENT_REDACT_PENDING_ALERT_ID, DOCUMENT_REDACT_TOOL_ID,
    DOCUMENT_ROTATE_LEFT_ID, DOCUMENT_ROTATE_RIGHT_ID, DOCUMENT_SAVE_AS_ID,
    DOCUMENT_SAVE_ERROR_ALERT_ID, DOCUMENT_SAVE_ERROR_DISMISS_ID, DOCUMENT_SAVE_ERROR_RETRY_ID,
    DOCUMENT_SAVE_ERROR_SAVE_AS_ID, DOCUMENT_SAVE_ID, DOCUMENT_SELECT_TOOL_ID,
    DOCUMENT_SESSION_TABS_ID, DOCUMENT_SNAPSHOT_TOOL_ID, DOCUMENT_SNAP_MARKUP_ID,
    DOCUMENT_SNAP_POPOVER_ID, DOCUMENT_SNAP_SETTINGS_ID, DOCUMENT_STRAIGHT_LINE_COLOR_BLUE_ID,
    DOCUMENT_STRAIGHT_LINE_OPACITY_50_ID, DOCUMENT_STRAIGHT_LINE_OPACITY_ID,
    DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID, DOCUMENT_STRAIGHT_LINE_WIDTH_4_ID,
    DOCUMENT_STRAIGHT_LINE_WIDTH_ID, DOCUMENT_TEXT_BOX_EDITOR_ID, DOCUMENT_TEXT_BOX_TOOL_ID,
    DOCUMENT_THUMBNAIL_STRIP_ID, DOCUMENT_TOOLBAR_SCROLL_ID, DOCUMENT_VIEWPORT_ID,
    DOCUMENT_WORKSPACE_ID, DirtyCloseResolution, DocumentId, DocumentOpenBatchDisposition,
    DocumentOpenBatchRequest, DocumentOpenBatchStatus, DocumentOpenOrigin,
    DocumentSaveFailureOperation, DocumentWorkspace, GeneratedTemplateRequestDisposition,
    NativeDocumentOpener, NativeDocumentResource, NativeDocumentSaveStatus, NativeDocumentSaver,
    NativeDocumentStatus, OpenDocumentRequest, OpenedNativeDocument, PdfDocumentSaver,
    PdfiumWorkerBackend, RasterSurface, SaveDestination, SaveDocumentRequest, SavedNativeDocument,
    ThumbnailSurface, VIEWPORT_OPEN_DOCUMENT_ID, ViewerFitPreset, document_annotation_layer_id,
    document_session_close_id, document_session_tab_id, document_thumbnail_id,
    document_viewer_page_id, document_viewer_tile_id, init_document_workspace_actions,
    save_as_prompt_spec, straight_line_arrowhead_points,
};
use butter_paper_gpui_component_compat::page_scale_control::{
    CalibrationPointDisposition, PAGE_SCALE_APPLY_ID, PAGE_SCALE_CUSTOM_PDF_LENGTH_ID,
    PAGE_SCALE_CUSTOM_PDF_UNITS_ID, PAGE_SCALE_CUSTOM_REAL_LENGTH_ID,
    PAGE_SCALE_CUSTOM_REAL_UNITS_ID, PAGE_SCALE_DIALOG_BODY_ID, PAGE_SCALE_DIALOG_ID,
    PAGE_SCALE_KNOWN_LENGTH_ID, PAGE_SCALE_METHOD_CUSTOM_ID, PAGE_SCALE_METHOD_PRESET_ID,
    PAGE_SCALE_PAGES_ID, PAGE_SCALE_PICK_ALERT_ID, PAGE_SCALE_PICK_CANCEL_ID, PAGE_SCALE_PICK_ID,
    PAGE_SCALE_PICK_STATUS_ID, PAGE_SCALE_PRECISION_MODE_ID, PAGE_SCALE_PRECISION_VALUE_ID,
    PAGE_SCALE_RANGE_ID, PAGE_SCALE_SAVE_PRESET_ID, PAGE_SCALE_SEPARATE_Y_ID,
    PAGE_SCALE_TRIGGER_ID, PAGE_SCALE_Y_PDF_LENGTH_ID, PAGE_SCALE_Y_REAL_LENGTH_ID, PageScaleMode,
    PageScalePagesMode,
};
use butter_paper_gpui_component_compat::page_view_control::{
    CONTINUOUS_PRIMARY_ID, SINGLE_PAGE_PRIMARY_ID,
};
use butter_paper_gpui_component_compat::page_view_control::{PageViewMode, WheelBehavior};
use butter_paper_gpui_component_compat::rectangle_property_inspector::{
    ELLIPSE_INSPECTOR_FILL_COLOR_ID, ELLIPSE_INSPECTOR_FILL_ENABLED_ID,
    ELLIPSE_INSPECTOR_FILL_OPACITY_ID, ELLIPSE_INSPECTOR_HEIGHT_ID, ELLIPSE_INSPECTOR_LOCKED_ID,
    ELLIPSE_INSPECTOR_OPACITY_ID, ELLIPSE_INSPECTOR_ROTATION_ID,
    ELLIPSE_INSPECTOR_STROKE_COLOR_ID, ELLIPSE_INSPECTOR_STROKE_STYLE_ID,
    ELLIPSE_INSPECTOR_STROKE_WIDTH_ID, ELLIPSE_INSPECTOR_WIDTH_ID,
    ELLIPSE_INSPECTOR_X_ID, ELLIPSE_INSPECTOR_Y_ID, ELLIPSE_PROPERTY_INSPECTOR_ID,
    RECTANGLE_INSPECTOR_FILL_COLOR_ID, RECTANGLE_INSPECTOR_FILL_ENABLED_ID,
    RECTANGLE_INSPECTOR_FILL_OPACITY_ID, RECTANGLE_INSPECTOR_HEIGHT_ID,
    RECTANGLE_INSPECTOR_LOCKED_ID, RECTANGLE_INSPECTOR_OPACITY_ID, RECTANGLE_INSPECTOR_ROTATION_ID,
    RECTANGLE_INSPECTOR_STROKE_COLOR_ID, RECTANGLE_INSPECTOR_STROKE_STYLE_ID,
    RECTANGLE_INSPECTOR_STROKE_WIDTH_ID, RECTANGLE_INSPECTOR_WIDTH_ID,
    RECTANGLE_INSPECTOR_WIDTH_PX, RECTANGLE_INSPECTOR_X_ID, RECTANGLE_INSPECTOR_Y_ID,
    RECTANGLE_PROPERTY_INSPECTOR_ID, RectanglePropertyEvent, RectanglePropertyPatch,
    RectangularShapePropertyKind,
};
use butter_paper_gpui_component_compat::viewer_toolbar_strip::{
    FIT_PAGE_ID, FIT_WIDTH_ID, VIEWER_TOOLBAR_CONTENT_ID, VIEWER_TOOLBAR_ID,
    VIEWER_TOOLBAR_SCROLL_ID,
};
use butter_paper_gpui_component_compat::zoom_control::ZOOM_MENU_ID;
use butter_paper_gpui_gallery::annotation_adapter::{
    AnnotationAdapter, AnnotationTool, LENGTH_SCALE_REQUIRED_MESSAGE, PointerPhaseOutcome,
    StraightLinePropertyEdit, ellipse_resize_handle_point_for_rect,
    ellipse_rotation_handle_point_for_rect,
};
use butter_paper_gpui_gallery::annotation_model::{
    Annotation, AnnotationSnapshot, ArcControlPoint, BlendMode, EllipseAnnotation, InkTool,
    LengthAnnotation, LengthCalibration, LengthEndpoint, LineKind, MarkupId, MeasurementPathKind,
    PageRotation, PageRotationDirection, PageScale, PageScaleApplyTarget, PageTransform, PdfPoint,
    PdfRect, PenAnnotation, PenAppearance, RectangleAnnotation, RectangleAppearance,
    RectangleResizeHandle, ScalePrecision, ScaleSource, ScaleUnit, StraightLineAnnotation,
    StraightLineAppearance, StrokeStyle, TextBoxAnnotation, TextBoxStyle, VertexPathKind,
    ellipse_cubic_bezier_points,
};
use butter_paper_gpui_gallery::generated_document::{
    GeneratedDocumentRequest, GeneratedDocumentStore, GeneratedPattern,
};
use butter_paper_gpui_gallery::highlight_compositor::precompose_highlights_multiply_rgba;
use butter_paper_gpui_gallery::page_geometry::{
    PageCoordinateSpace, PdfPoint as CoordinatePoint, PdfRect as CoordinateRect,
    Rotation as CoordinateRotation,
};
use butter_paper_gpui_gallery::pdf_engine::{PdfPersistenceSession, PdfPublicationOutcome};
use butter_paper_gpui_gallery::pdf_file_authority::{SaveAsTargetAuthority, SaveTargetErrorKind};
use butter_paper_gpui_gallery::semantic_snapping::SemanticSnapRole;
use butter_paper_gpui_gallery::template_library::{BUILT_IN_BLANK_ID, TemplateLibrary};
use gpui::{
    AppContext as _, Modifiers, MouseButton, MouseDownEvent, MouseExitEvent, MouseUpEvent,
    ScrollDelta, ScrollWheelEvent, TestAppContext, point, px,
};
use gpui_component::{Root, WindowExt as _};
use sha2::{Digest as _, Sha256};

struct ScratchFiles(Vec<PathBuf>);

impl Drop for ScratchFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

struct ScratchDirectories(Vec<PathBuf>);

impl Drop for ScratchDirectories {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn scroll_annotation_target_into_view(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    target_id: &'static str,
) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let scroll = cx
        .debug_bounds(DOCUMENT_TOOLBAR_SCROLL_ID)
        .expect("the annotation toolbar must expose its horizontal scroll owner");
    let target = cx
        .debug_bounds(target_id)
        .unwrap_or_else(|| panic!("{target_id} must render before scrolling"));
    let delta_x = if target.right() > scroll.right() {
        -(f32::from(target.right() - scroll.right()) + 8.)
    } else if target.left() < scroll.left() {
        f32::from(scroll.left() - target.left()) + 8.
    } else {
        0.
    };
    if delta_x != 0. {
        cx.simulate_event(ScrollWheelEvent {
            position: scroll.center(),
            delta: ScrollDelta::Pixels(point(px(delta_x), px(0.))),
            ..Default::default()
        });
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let scrolled_target = cx.debug_bounds(target_id).unwrap();
    assert!(
        scrolled_target.left() >= scroll.left() && scrolled_target.right() <= scroll.right(),
        "{target_id} must be fully reachable after horizontal scrolling; viewport={scroll:?}, target={scrolled_target:?}, offset={:?}",
        workspace.read_with(cx, |workspace, _| workspace
            .annotation_toolbar_scroll_offset())
    );
}

#[test]
fn length_scale_is_page_scoped_and_generic_drag_creation_is_rejected() {
    let mut adapter = AnnotationAdapter::default();
    adapter
        .set_page_length_calibration(
            0,
            LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap(),
        )
        .unwrap();
    adapter.set_tool(AnnotationTool::Length).unwrap();

    let missing_scale = adapter
        .begin_length_placement(
            1,
            1,
            MarkupId::new("workspace:length:page-1").unwrap(),
            PdfPoint::new(72., 240.).unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        missing_scale,
        butter_paper_gpui_gallery::annotation_model::AnnotationError::InvalidGeometry(
            LENGTH_SCALE_REQUIRED_MESSAGE.to_owned(),
        )
    );
    assert!(adapter.document_scene(1, 1).lengths.is_empty());

    let legacy_drag = adapter
        .pointer_down(1, 0, 99, PdfPoint::new(72., 240.).unwrap(), 4.)
        .unwrap_err();
    assert_eq!(
        legacy_drag.to_string(),
        "invalid geometry: length creation requires the two-click placement interface"
    );
    assert!(adapter.document_scene(1, 0).lengths.is_empty());

    let imported_calibration = LengthCalibration::from_scale(72., 1., "m", 2, true)
        .unwrap()
        .with_label("Imported scale")
        .unwrap();
    adapter
        .load_imported_annotations(
            2,
            vec![Annotation::Length(
                LengthAnnotation::new(
                    MarkupId::new("bp:imported-length").unwrap(),
                    3,
                    PdfPoint::new(10., 10.).unwrap(),
                    PdfPoint::new(82., 10.).unwrap(),
                    imported_calibration.clone(),
                )
                .unwrap(),
            )],
        )
        .unwrap();
    assert_eq!(
        adapter.document_page_length_calibration(2, 3),
        Some(&imported_calibration)
    );
}

#[test]
fn highlight_samples_at_half_a_point_and_commits_at_exactly_three_css_pixels() {
    let mut adapter = AnnotationAdapter::default();
    adapter.set_observed_pixels_per_point(1.).unwrap();
    adapter.set_tool(AnnotationTool::Highlight).unwrap();
    adapter.queue_next_annotation_id(MarkupId::new("workspace:highlight:exact-threshold").unwrap());
    assert_eq!(
        adapter
            .pointer_down(9, 0, 1, PdfPoint::new(10., 10.).unwrap(), 4.)
            .unwrap(),
        butter_paper_gpui_gallery::annotation_adapter::PointerPhaseOutcome::GestureStarted
    );
    adapter
        .pointer_move(1, PdfPoint::new(10.49, 10.).unwrap())
        .unwrap();
    adapter
        .pointer_move(1, PdfPoint::new(10.5, 10.).unwrap())
        .unwrap();
    assert!(matches!(
        adapter
            .pointer_up(1, PdfPoint::new(13., 10.).unwrap())
            .unwrap(),
        butter_paper_gpui_gallery::annotation_adapter::PointerPhaseOutcome::AnnotationCreated(_)
    ));
    let snapshot = adapter.snapshot(9).unwrap();
    assert_eq!(snapshot.pens.len(), 1);
    assert_eq!(snapshot.pens[0].points().len(), 3);
    assert_eq!(snapshot.pens[0].tool(), InkTool::Highlight);
    assert_eq!(snapshot.pens[0].blend_mode(), BlendMode::Multiply);
    assert!(!snapshot.pens[0].smooth_curves);
}

struct RecordingResource {
    released: Arc<AtomicBool>,
}

struct RotationRecordingResource {
    released: Arc<AtomicBool>,
}

struct RejectingOpener;

struct SuccessfulOpener;

#[derive(Default)]
struct ScriptedOpenBatchOpener {
    opened: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}

#[derive(Default)]
struct PathRecordingOpener {
    opened: Mutex<Vec<(PathBuf, Arc<AtomicBool>)>>,
}

struct AnnotationAllSuccessfulOpener;

#[derive(Default)]
struct RecoveryOpener {
    released: Mutex<Vec<Arc<AtomicBool>>>,
}

impl NativeDocumentOpener for RejectingOpener {
    fn open(&self, _: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        Err("the unsupported-family save must fail before reopen".into())
    }
}

impl NativeDocumentOpener for SuccessfulOpener {
    fn open(&self, _: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        Ok(opened_document(Arc::new(AtomicBool::new(false))))
    }
}

impl NativeDocumentOpener for ScriptedOpenBatchOpener {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        if request
            .path
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"broken"))
        {
            return Err(format!(
                "deterministic invalid PDF: {}",
                request.path.display()
            ));
        }
        let released = Arc::new(AtomicBool::new(false));
        self.opened
            .lock()
            .unwrap()
            .push((request.path.clone(), released.clone()));
        Ok(opened_document(released))
    }
}

impl NativeDocumentOpener for PathRecordingOpener {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        let released = Arc::new(AtomicBool::new(false));
        self.opened
            .lock()
            .unwrap()
            .push((request.path.clone(), released.clone()));
        Ok(opened_single_page_document(released))
    }
}

impl NativeDocumentOpener for AnnotationAllSuccessfulOpener {
    fn open(&self, _: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        Ok(opened_annotation_all_document(Arc::new(AtomicBool::new(
            false,
        ))))
    }
}

impl NativeDocumentOpener for RecoveryOpener {
    fn open(&self, _: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        let released = Arc::new(AtomicBool::new(false));
        self.released.lock().unwrap().push(released.clone());
        Ok(opened_document(released))
    }
}

impl NativeDocumentResource for RecordingResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4242)
    }

    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        Ok(raster(width, page_index + 2))
    }

    fn render_tile(
        &self,
        request: butter_paper_gpui_gallery::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

impl NativeDocumentResource for RotationRecordingResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(4343)
    }

    fn render_page(&self, page_index: u32, _: u32) -> Result<RasterSurface, String> {
        match page_index {
            0 => patterned_raster(2, 3, &[1, 2, 3, 4, 5, 6]),
            1 => patterned_raster(3, 2, &[7, 8, 9, 10, 11, 12]),
            _ => Err("page is outside the deterministic rotation fixture".into()),
        }
    }

    fn render_tile(
        &self,
        request: butter_paper_gpui_gallery::viewer::TileRequest,
    ) -> Result<RasterSurface, String> {
        Ok(raster(
            request.crop.width as u32,
            request.crop.height as u32,
        ))
    }

    fn close(&self) -> Result<(), String> {
        self.released.store(true, Ordering::Release);
        Ok(())
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

fn raster(width: u32, height: u32) -> RasterSurface {
    RasterSurface::new(
        width,
        height,
        vec![0xff; width as usize * height as usize * 4],
    )
    .expect("the deterministic BGRA fixture must be valid")
}

fn patterned_raster(width: u32, height: u32, values: &[u8]) -> Result<RasterSurface, String> {
    if values.len() != width as usize * height as usize {
        return Err("patterned raster dimensions do not match its pixels".into());
    }
    let pixels = values
        .iter()
        .flat_map(|value| [*value, 0, 0, 0xff])
        .collect();
    RasterSurface::new(width, height, pixels)
}

fn save_as_destination(source: &Path, target: &Path) -> SaveDestination {
    SaveDestination::NewTarget(
        SaveAsTargetAuthority::bind(target.to_path_buf(), source)
            .expect("the persistence fixture must bind one absolute new PDF target"),
    )
}

fn workspace_save_target(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".prepared");
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(name)
}

#[test]
fn rotated_raster_and_crop_pixels_cover_all_quarter_turns() {
    let source = patterned_raster(2, 3, &[1, 2, 3, 4, 5, 6]).unwrap();
    let cases = [
        (PageRotation::Degrees0, (2, 3), vec![1, 2, 3, 4, 5, 6]),
        (PageRotation::Degrees90, (3, 2), vec![5, 3, 1, 6, 4, 2]),
        (PageRotation::Degrees180, (2, 3), vec![6, 5, 4, 3, 2, 1]),
        (PageRotation::Degrees270, (3, 2), vec![2, 4, 6, 1, 3, 5]),
    ];
    for (rotation, dimensions, expected) in cases {
        let rotated = source.rotated(rotation).unwrap();
        assert_eq!((rotated.width(), rotated.height()), dimensions);
        assert_eq!(
            rotated
                .pixels_bgra()
                .chunks_exact(4)
                .map(|pixel| pixel[0])
                .collect::<Vec<_>>(),
            expected
        );
        let crop = rotated.cropped(0, 0, 1, 1).unwrap();
        assert_eq!(crop.pixels_bgra()[0], expected[0]);
    }
}

#[test]
fn rotated_pointer_projection_places_and_hits_the_same_pdf_annotation() {
    for (sequence, rotation) in [
        PageRotation::Degrees0,
        PageRotation::Degrees90,
        PageRotation::Degrees180,
        PageRotation::Degrees270,
    ]
    .into_iter()
    .enumerate()
    {
        let document_id = 500 + sequence as u64;
        let mut adapter = AnnotationAdapter::default();
        adapter
            .load_imported_annotations_with_document_state(
                document_id,
                Vec::new(),
                Vec::new(),
                vec![(0, rotation)],
            )
            .unwrap();
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        let id = MarkupId::new(format!("rotation:pointer:{sequence}")).unwrap();
        adapter.queue_next_annotation_id(id.clone());
        let transform = PageTransform::new_rotated(612., 792., 1.5, rotation).unwrap();
        let start_pdf = PdfPoint::new(72., 144.).unwrap();
        let end_pdf = PdfPoint::new(180., 252.).unwrap();
        let start_local = transform.point_to_local_pixels(start_pdf);
        let end_local = transform.point_to_local_pixels(end_pdf);
        let projected_start = transform
            .point_from_local_pixels(start_local.x, start_local.y)
            .unwrap();
        let projected_end = transform
            .point_from_local_pixels(end_local.x, end_local.y)
            .unwrap();
        adapter
            .pointer_down(document_id, 0, 1, projected_start, 1.)
            .unwrap();
        adapter.pointer_move(1, projected_end).unwrap();
        assert_eq!(
            adapter.pointer_up(1, projected_end).unwrap(),
            PointerPhaseOutcome::AnnotationCreated(id.clone())
        );
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter.clear_selection(document_id);
        let center_pdf = PdfPoint::new(126., 198.).unwrap();
        let center_local = transform.point_to_local_pixels(center_pdf);
        let projected_center = transform
            .point_from_local_pixels(center_local.x, center_local.y)
            .unwrap();
        assert_eq!(
            adapter
                .pointer_down(document_id, 0, 2, projected_center, 1.)
                .unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_up(2, projected_center).unwrap();
        assert_eq!(adapter.snapshot(document_id).unwrap().selected_id, Some(id));
    }
}

fn opened_rotation_document(released: Arc<AtomicBool>) -> OpenedNativeDocument {
    OpenedNativeDocument::new(
        "bp-page-rotation-v1.pdf",
        vec![(2., 3.), (3., 2.)],
        patterned_raster(2, 3, &[1, 2, 3, 4, 5, 6]).unwrap(),
        vec![
            ThumbnailSurface::new(0, patterned_raster(2, 3, &[1, 2, 3, 4, 5, 6]).unwrap()),
            ThumbnailSurface::new(1, patterned_raster(3, 2, &[7, 8, 9, 10, 11, 12]).unwrap()),
        ],
        Arc::new(RotationRecordingResource { released }),
    )
    .unwrap()
    .with_page_rotations(vec![PageRotation::Degrees0, PageRotation::Degrees90])
}

fn opened_painted_rotation_document(released: Arc<AtomicBool>) -> OpenedNativeDocument {
    OpenedNativeDocument::new(
        "bp-page-rotation-v1.pdf",
        vec![(200., 300.), (300., 200.)],
        patterned_raster(2, 3, &[1, 2, 3, 4, 5, 6]).unwrap(),
        vec![
            ThumbnailSurface::new(0, patterned_raster(2, 3, &[1, 2, 3, 4, 5, 6]).unwrap()),
            ThumbnailSurface::new(1, patterned_raster(3, 2, &[7, 8, 9, 10, 11, 12]).unwrap()),
        ],
        Arc::new(RotationRecordingResource { released }),
    )
    .unwrap()
    .with_page_rotations(vec![PageRotation::Degrees0, PageRotation::Degrees90])
}

fn opened_document(released: Arc<AtomicBool>) -> OpenedNativeDocument {
    OpenedNativeDocument::new(
        "bp-multi-page-v1.pdf",
        vec![(612., 792.), (612., 792.), (612., 792.)],
        raster(32, 40),
        vec![
            ThumbnailSurface::new(0, raster(8, 10)),
            ThumbnailSurface::new(1, raster(8, 10)),
            ThumbnailSurface::new(2, raster(8, 10)),
        ],
        Arc::new(RecordingResource { released }),
    )
    .expect("the deterministic document payload must be valid")
}

fn opened_single_page_document(released: Arc<AtomicBool>) -> OpenedNativeDocument {
    OpenedNativeDocument::new(
        "Untitled.pdf",
        vec![(1_190.55, 841.89)],
        raster(32, 23),
        vec![ThumbnailSurface::new(0, raster(10, 7))],
        Arc::new(RecordingResource { released }),
    )
    .expect("the deterministic generated-document payload must be valid")
}

#[test]
fn opened_document_retains_coordinate_space_metadata() {
    let released = Arc::new(AtomicBool::new(false));
    let space = PageCoordinateSpace::new(
        CoordinateRect::new(0.0, 0.0, 612.0, 792.0).unwrap(),
        CoordinateRect::new(36.0, 72.0, 540.0, 720.0).unwrap(),
        CoordinateRotation::Degrees90,
        2.0,
    )
    .unwrap();
    let document = OpenedNativeDocument::new(
        "coordinate-space.pdf",
        vec![(1_440.0, 1_080.0)],
        raster(32, 24),
        vec![ThumbnailSurface::new(0, raster(8, 6))],
        Arc::new(RecordingResource { released }),
    )
    .unwrap()
    .with_page_coordinate_spaces(vec![space]);

    let retained = document.page_coordinate_space(0).unwrap();
    assert_eq!(retained.view_box(), space.view_box());
    assert_eq!(retained.rotation(), CoordinateRotation::Degrees90);
    assert_eq!(retained.user_unit(), 2.0);
    assert_eq!(retained.viewport_to_pdf(CoordinatePoint::new(72.0, 72.0)),
        CoordinatePoint::new(72.0, 108.0));
}

fn opened_annotation_all_document(released: Arc<AtomicBool>) -> OpenedNativeDocument {
    OpenedNativeDocument::new(
        "bp-annotation-all-v1.pdf",
        vec![(612., 792.), (756., 576.)],
        raster(32, 40),
        vec![
            ThumbnailSurface::new(0, raster(8, 10)),
            ThumbnailSurface::new(1, raster(10, 8)),
        ],
        Arc::new(RecordingResource { released }),
    )
    .expect("the two-page annotation fixture payload must be valid")
    .with_page_rotations(vec![PageRotation::Degrees0, PageRotation::Degrees90])
}

#[gpui::test]
fn native_open_batch_preserves_a_valid_document_and_surfaces_a_failed_sibling(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let opener = Arc::new(ScriptedOpenBatchOpener::default());
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let opener = opener.clone();
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(opener, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let request = DocumentOpenBatchRequest::new(
        DocumentOpenOrigin::System,
        [PathBuf::from("drawing.pdf"), PathBuf::from("broken.pdf")],
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.open_documents(request, cx)),
        DocumentOpenBatchDisposition::Started {
            batch_id: 1,
            candidate_count: 2,
        }
    );
    cx.run_until_parked();

    let (session_count, active_document, status, failure) =
        workspace.read_with(cx, |workspace, _| {
            (
                workspace.sessions().len(),
                workspace.active_document_id(),
                workspace.document_open_status().clone(),
                workspace.last_document_open_failure().cloned(),
            )
        });
    assert_eq!(
        session_count, 1,
        "the failed candidate must not leave a tab"
    );
    assert_eq!(active_document, Some(DocumentId::new(1)));
    assert_eq!(
        status,
        DocumentOpenBatchStatus::Completed {
            batch_id: 1,
            opened: vec![DocumentId::new(1)],
            focused_existing: None,
            failed_count: 1,
            status_message: "Loaded bp-multi-page-v1.pdf".into(),
        }
    );
    assert_eq!(
        failure.map(|failure| (failure.path, failure.message)),
        Some((
            PathBuf::from("broken.pdf"),
            "deterministic invalid PDF: broken.pdf".into(),
        ))
    );
    assert_eq!(opener.opened.lock().unwrap().len(), 1);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_OPEN_ERROR_ALERT_ID).is_some(),
        "a partial open failure must render through the real GPUI Component Alert"
    );
}

#[gpui::test]
fn native_open_batch_retains_ordered_failures_and_dismisses_only_feedback(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let opener = Arc::new(ScriptedOpenBatchOpener::default());
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let opener = opener.clone();
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(opener, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.open_documents(
            DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::Menu,
                [
                    PathBuf::from("broken-first.pdf"),
                    PathBuf::from("drawing.pdf"),
                    PathBuf::from("broken-second.pdf"),
                ],
            ),
            cx,
        )
    });
    cx.run_until_parked();

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .document_open_failures()
            .iter()
            .map(|failure| failure.path.clone())
            .collect::<Vec<_>>()),
        [
            PathBuf::from("broken-first.pdf"),
            PathBuf::from("broken-second.pdf"),
        ]
    );
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.document_open_status().clone()),
        DocumentOpenBatchStatus::Completed {
            failed_count: 2,
            ..
        }
    ));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let dismiss = cx
        .debug_bounds(DOCUMENT_OPEN_ERROR_DISMISS_ID)
        .expect("the feedback must expose a labeled GPUI Component Dismiss button");
    cx.simulate_click(dismiss.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.document_open_failures().is_empty()
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(DocumentId::new(2))
    );
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.document_open_status().clone()),
        DocumentOpenBatchStatus::Completed {
            failed_count: 2,
            ..
        }
    ));
    assert!(cx.debug_bounds(DOCUMENT_OPEN_ERROR_ALERT_ID).is_none());
}

#[gpui::test]
fn native_open_origin_policy_cancels_focuses_existing_and_force_opens_drop(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let opener = Arc::new(ScriptedOpenBatchOpener::default());
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let opener = opener.clone();
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(opener, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.open_documents(
            DocumentOpenBatchRequest::cancelled(DocumentOpenOrigin::Picker),
            cx,
        )),
        DocumentOpenBatchDisposition::Cancelled
    );
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.document_open_status().clone()),
        DocumentOpenBatchStatus::Idle
    );

    let drawing = PathBuf::from("drawing.pdf");
    workspace.update(cx, |workspace, cx| {
        workspace.open_documents(
            DocumentOpenBatchRequest::new(DocumentOpenOrigin::System, [drawing.clone()]),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let second_thumbnail_id =
        Box::leak(document_thumbnail_id(DocumentId::new(1), 1).into_boxed_str());
    let second_thumbnail = cx
        .debug_bounds(second_thumbnail_id)
        .expect("the first session must expose its second page thumbnail");
    cx.simulate_click(second_thumbnail.center(), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(DocumentId::new(1), cx)
            .unwrap()
            .read(cx)
            .current_page()),
        1
    );

    workspace.update(cx, |workspace, cx| {
        workspace.open_documents(
            DocumentOpenBatchRequest::new(DocumentOpenOrigin::Menu, [drawing.clone()]),
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1
    );
    assert_eq!(opener.opened.lock().unwrap().len(), 1);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(DocumentId::new(1), cx)
            .unwrap()
            .read(cx)
            .current_page()),
        1,
        "focusing an existing document must preserve its selected page"
    );
    assert!(matches!(
        workspace.read_with(cx, |workspace, _| workspace.document_open_status().clone()),
        DocumentOpenBatchStatus::Completed {
            opened,
            focused_existing: Some(focused_existing),
            failed_count: 0,
            ..
        } if opened.is_empty() && focused_existing == DocumentId::new(1)
    ));

    workspace.update(cx, |workspace, cx| {
        workspace.open_documents(
            DocumentOpenBatchRequest::new(DocumentOpenOrigin::Drop, [drawing]),
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        2
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(DocumentId::new(2))
    );
    let resources = opener
        .opened
        .lock()
        .unwrap()
        .iter()
        .map(|(_, released)| released.clone())
        .collect::<Vec<_>>();
    assert_eq!(resources.len(), 2);
    assert!(!resources[0].load(Ordering::Acquire));
    assert!(!resources[1].load(Ordering::Acquire));
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(DocumentId::new(2), cx)
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(DocumentId::new(1))
    );
    assert!(!resources[0].load(Ordering::Acquire));
    assert!(resources[1].load(Ordering::Acquire));
}

#[cfg(unix)]
#[gpui::test]
fn native_open_preserves_non_utf8_pdf_paths_without_consuming_ids_for_rejected_inputs(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let opener = Arc::new(ScriptedOpenBatchOpener::default());
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(opener.clone(), cx));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.open_documents(
            DocumentOpenBatchRequest::new(
                DocumentOpenOrigin::System,
                [PathBuf::from("ignored.txt")],
            ),
            cx,
        )),
        DocumentOpenBatchDisposition::NoAcceptedPaths
    );

    let mut bytes = b"drawing-".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b".PdF");
    let path = PathBuf::from(OsString::from_vec(bytes));
    workspace.update(cx, |workspace, cx| {
        workspace.open_documents(
            DocumentOpenBatchRequest::new(DocumentOpenOrigin::System, [path.clone()]),
            cx,
        )
    });
    cx.run_until_parked();

    assert_eq!(
        opener
            .opened
            .lock()
            .unwrap()
            .iter()
            .map(|(path, _)| path.clone())
            .collect::<Vec<_>>(),
        [path]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(DocumentId::new(1)),
        "rejected inputs must not consume a stable document ID"
    );
}

#[gpui::test]
fn native_open_ctrl_o_dispatches_the_shared_picker_and_cancel_is_a_noop(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    cx.update(init_document_workspace_actions);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| {
                DocumentWorkspace::with_opener(Arc::new(ScriptedOpenBatchOpener::default()), cx)
            });
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));

    cx.simulate_keystrokes("ctrl-o");
    assert!(
        cx.did_prompt_for_paths(),
        "Command-or-Control+O must reach the same native multi-PDF picker"
    );
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files);
        assert!(!options.directories);
        assert!(options.multiple);
        assert_eq!(options.prompt.as_deref(), Some("Open PDFs"));
        None
    });
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.document_open_status().clone()),
        DocumentOpenBatchStatus::Idle
    );
}

fn imported_rectangle(id: &str) -> RectangleAnnotation {
    RectangleAnnotation {
        id: MarkupId::new(id).unwrap(),
        page_index: 0,
        rect: PdfRect::new(72., 96., 144., 96.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::new("#2563eb", 1.5, Some("#dbeafe"), 1.)
            .unwrap()
            .with_fill_opacity(0.2)
            .unwrap(),
        locked: false,
    }
}

#[gpui::test]
fn ellipse_workspace_create_edit_lock_delete_undo_and_real_component_tool_are_coherent(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("ellipse-workspace.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, _| window.activate_window());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(DOCUMENT_ELLIPSE_TOOL_ID).is_some(),
        "the real GPUI Component Ellipse tool button must render"
    );

    let id = MarkupId::new("workspace:ellipse:journey-1").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_ellipse(
                request.document_id,
                0,
                id.clone(),
                PdfPoint::new(72., 144.).unwrap(),
                PdfPoint::new(192., 216.).unwrap(),
                true,
                cx,
            )
        })
        .unwrap();
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(created.ellipses.len(), 1);
    assert_eq!(
        created.ellipses[0].rect,
        PdfRect::new(72., 144., 120., 120.).unwrap()
    );
    assert_eq!(created.selected_id.as_ref(), Some(&id));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_scene(request.document_id, 0, cx)
            .ellipses
            .len()),
        1,
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_bounds = cx
        .debug_bounds("document-1-annotation-layer-0")
        .expect("the Ellipse annotation layer must render");
    let scale =
        (f32::from(layer_bounds.size.width) / 612.).min(f32::from(layer_bounds.size.height) / 792.);
    let page_origin = point(
        layer_bounds.origin.x + px((f32::from(layer_bounds.size.width) - 612. * scale) / 2.),
        layer_bounds.origin.y + px((f32::from(layer_bounds.size.height) - 792. * scale) / 2.),
    );
    let to_view = |pdf: PdfPoint| {
        point(
            page_origin.x + px(pdf.x as f32 * scale),
            page_origin.y + px((792. - pdf.y as f32) * scale),
        )
    };

    let move_start = to_view(PdfPoint::new(187.432_771_95, 226.961_005_94).unwrap());
    let move_end = to_view(PdfPoint::new(205.432_771_95, 220.961_005_94).unwrap());
    cx.simulate_mouse_down(move_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(move_end, Some(MouseButton::Left), Modifiers::default());
    let move_preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert!(move_preview.ellipses[0].preview);
    for (actual, expected) in [
        (move_preview.ellipses[0].rect.x, 90.),
        (move_preview.ellipses[0].rect.y, 138.),
        (move_preview.ellipses[0].rect.width, 120.),
        (move_preview.ellipses[0].rect.height, 120.),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    cx.simulate_mouse_up(move_end, MouseButton::Left, Modifiers::default());

    let resize_start = to_view(PdfPoint::new(210., 198.).unwrap());
    let resize_end = to_view(PdfPoint::new(234., 198.).unwrap());
    cx.simulate_mouse_down(resize_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(resize_end, Some(MouseButton::Left), Modifiers::default());
    let resize_preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert!(resize_preview.ellipses[0].preview);
    for (actual, expected) in [
        (resize_preview.ellipses[0].rect.x, 90.),
        (resize_preview.ellipses[0].rect.y, 138.),
        (resize_preview.ellipses[0].rect.width, 144.),
        (resize_preview.ellipses[0].rect.height, 120.),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    cx.simulate_mouse_up(resize_end, MouseButton::Left, Modifiers::default());

    let rect = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap()
        .ellipses[0]
        .rect;
    let rotate_start = ellipse_rotation_handle_point_for_rect(rect, 0., scale as f64).unwrap();
    let rotate_end = PdfPoint::new(rect.x + rect.width, rect.y + rect.height * 0.5).unwrap();
    cx.simulate_mouse_down(
        to_view(rotate_start),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(rotate_end), MouseButton::Left, Modifiers::default());
    let rotated = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(
        (rotated.ellipses[0].rotation_degrees - 90.).abs() < 0.001,
        "native pointer projection produced {} degrees",
        rotated.ellipses[0].rotation_degrees
    );

    let reset_handle = ellipse_rotation_handle_point_for_rect(rect, 90., scale as f64).unwrap();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: to_view(reset_handle),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: to_view(reset_handle),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap()
            .ellipses[0]
            .rotation_degrees,
        0.
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.translate_ellipse(request.document_id, id.clone(), 12., -6., cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_ellipse_rect(
                request.document_id,
                id.clone(),
                PdfRect::new(84., 138., 156., 120.).unwrap(),
                cx,
            )
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_ellipse_rotation(request.document_id, id.clone(), 30., cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_ellipse_rotation(request.document_id, id.clone(), 0., cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_ellipse_appearance(
                request.document_id,
                RectangleAppearance::new("#2563eb", 3., Some("#dbeafe"), 0.75).unwrap(),
                cx,
            )
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, true, cx)
        })
        .unwrap();
    let locked_before_delete = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap(),
        locked_before_delete,
        "locked Ellipse deletion is suppressed without changing history or selection"
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, false, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .ellipses
            .is_empty()
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    let restored = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(restored.ellipses.len(), 1);
    assert_eq!(
        restored.ellipses[0].rect,
        PdfRect::new(84., 138., 156., 120.).unwrap()
    );
    assert_eq!(restored.ellipses[0].rotation_degrees, 0.);
    assert_eq!(restored.ellipses[0].appearance.stroke_color(), "#2563eb");
}

#[test]
fn ellipse_workspace_save_as_reconciles_create_edit_and_delete_across_reopens() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let created_target = manifest_dir.join(format!(
        ".prepared/ellipse-workspace-created-{}.pdf",
        std::process::id()
    ));
    let edited_target = manifest_dir.join(format!(
        ".prepared/ellipse-workspace-edited-{}.pdf",
        std::process::id()
    ));
    let deleted_target = manifest_dir.join(format!(
        ".prepared/ellipse-workspace-deleted-{}.pdf",
        std::process::id()
    ));
    let _cleanup = ScratchFiles(vec![
        created_target.clone(),
        edited_target.clone(),
        deleted_target.clone(),
    ]);
    assert!(!created_target.exists() && !edited_target.exists() && !deleted_target.exists());

    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let id = MarkupId::new("workspace:ellipse:persistence-1").unwrap();
    let created = EllipseAnnotation::new(
        id.clone(),
        0,
        PdfRect::new(72., 144., 180., 96.).unwrap(),
        RectangleAppearance::default(),
    )
    .unwrap();
    let mut created_ellipses = source_session.ellipses().to_vec();
    created_ellipses.push(created.clone());
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(96),
            generation: 1,
            source_path: source.clone(),
            destination: save_as_destination(&source, &created_target),
            current_page: 0,
            annotation_revision: 1,
            annotations: AnnotationSnapshot {
                revision: 1,
                saved_revision: 0,
                dirty: true,
                selected_id: Some(id.clone()),
                annotation_order: Vec::new(),
                rectangles: source_session.rectangles().to_vec(),
                ellipses: created_ellipses,
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: source_session.pens().to_vec(),
                straight_lines: source_session.straight_lines().to_vec(),
                vertex_paths: source_session.vertex_paths().to_vec(),

                clouds: source_session.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: source_session.callouts().to_vec(),
                measurement_paths: source_session.measurement_paths().to_vec(),
                text_boxes: source_session.text_boxes().to_vec(),
                lengths: source_session.lengths().to_vec(),
                images: source_session.images().to_vec(),
                snapshots: source_session.snapshots().to_vec(),
                page_scales: source_session.page_scales().to_vec(),
                scale_presets: Vec::new(),
                page_length_calibrations: source_session
                    .page_length_calibrations()
                    .iter()
                    .map(|(page, scale)| (*page, scale.clone()))
                    .collect(),
                page_rotations: source_session
                    .page_rotations()
                    .iter()
                    .map(|(page, rotation)| (*page, *rotation))
                    .collect(),
                undo_depth: 1,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .unwrap();
    let created_reopen = PdfPersistenceSession::open(&created_target).unwrap();
    assert_eq!(
        created_reopen
            .ellipses()
            .iter()
            .find(|ellipse| ellipse.id == id),
        Some(&created)
    );

    let mut edited = created.clone();
    edited.rect = PdfRect::new(84., 132., 216., 120.).unwrap();
    edited.rotation_degrees = 30.;
    edited.appearance = RectangleAppearance::new("#2563eb", 3., Some("#dbeafe"), 0.75).unwrap();
    let edited_ellipses = created_reopen
        .ellipses()
        .iter()
        .map(|ellipse| {
            if ellipse.id == id {
                edited.clone()
            } else {
                ellipse.clone()
            }
        })
        .collect();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(96),
            generation: 2,
            source_path: created_target.clone(),
            destination: save_as_destination(&created_target, &edited_target),
            current_page: 0,
            annotation_revision: 2,
            annotations: AnnotationSnapshot {
                revision: 2,
                saved_revision: 1,
                dirty: true,
                selected_id: Some(id.clone()),
                annotation_order: Vec::new(),
                rectangles: created_reopen.rectangles().to_vec(),
                ellipses: edited_ellipses,
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: created_reopen.pens().to_vec(),
                straight_lines: created_reopen.straight_lines().to_vec(),
                vertex_paths: created_reopen.vertex_paths().to_vec(),

                clouds: created_reopen.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: created_reopen.callouts().to_vec(),
                measurement_paths: created_reopen.measurement_paths().to_vec(),
                text_boxes: created_reopen.text_boxes().to_vec(),
                lengths: created_reopen.lengths().to_vec(),
                images: created_reopen.images().to_vec(),
                snapshots: created_reopen.snapshots().to_vec(),
                page_scales: created_reopen.page_scales().to_vec(),
                scale_presets: Vec::new(),
                page_length_calibrations: created_reopen
                    .page_length_calibrations()
                    .iter()
                    .map(|(page, scale)| (*page, scale.clone()))
                    .collect(),
                page_rotations: created_reopen
                    .page_rotations()
                    .iter()
                    .map(|(page, rotation)| (*page, *rotation))
                    .collect(),
                undo_depth: 2,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .unwrap();
    let edited_reopen = PdfPersistenceSession::open(&edited_target).unwrap();
    assert_eq!(
        edited_reopen
            .ellipses()
            .iter()
            .find(|ellipse| ellipse.id == id),
        Some(&edited)
    );
    assert!(edited_reopen.ellipse_has_canonical_native_identity(&id));

    let remaining = edited_reopen
        .ellipses()
        .iter()
        .filter(|ellipse| ellipse.id != id)
        .cloned()
        .collect();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(96),
            generation: 3,
            source_path: edited_target.clone(),
            destination: save_as_destination(&edited_target, &deleted_target),
            current_page: 0,
            annotation_revision: 3,
            annotations: AnnotationSnapshot {
                revision: 3,
                saved_revision: 2,
                dirty: true,
                selected_id: None,
                annotation_order: Vec::new(),
                rectangles: edited_reopen.rectangles().to_vec(),
                ellipses: remaining,
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: edited_reopen.pens().to_vec(),
                straight_lines: edited_reopen.straight_lines().to_vec(),
                vertex_paths: edited_reopen.vertex_paths().to_vec(),

                clouds: edited_reopen.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: edited_reopen.callouts().to_vec(),
                measurement_paths: edited_reopen.measurement_paths().to_vec(),
                text_boxes: edited_reopen.text_boxes().to_vec(),
                lengths: edited_reopen.lengths().to_vec(),
                images: edited_reopen.images().to_vec(),
                snapshots: edited_reopen.snapshots().to_vec(),
                page_scales: edited_reopen.page_scales().to_vec(),
                scale_presets: Vec::new(),
                page_length_calibrations: edited_reopen
                    .page_length_calibrations()
                    .iter()
                    .map(|(page, scale)| (*page, scale.clone()))
                    .collect(),
                page_rotations: edited_reopen
                    .page_rotations()
                    .iter()
                    .map(|(page, rotation)| (*page, *rotation))
                    .collect(),
                undo_depth: 3,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .unwrap();
    let deleted_reopen = PdfPersistenceSession::open(&deleted_target).unwrap();
    assert!(
        deleted_reopen
            .ellipses()
            .iter()
            .all(|ellipse| ellipse.id != id)
    );
    assert!(!deleted_reopen.has_canonical_raw_annotation_name(&id));
}

#[gpui::test]
fn document_workspace_preserves_live_state_rejects_stale_results_and_releases_resources(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot
        .borrow_mut()
        .take()
        .expect("the workspace entity must be retained");

    let first_request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("bp-multi-page-v1.pdf"), cx)
    });
    let first_id = first_request.document_id;
    let released = Arc::new(AtomicBool::new(false));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &first_request,
            Ok(opened_document(released.clone())),
            cx,
        )),
        ApplyDisposition::Applied
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_WORKSPACE_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_PAGE_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_THUMBNAIL_STRIP_ID).is_some());
    let second_thumbnail = cx
        .debug_bounds("document-1-thumbnail-1")
        .expect("the second real thumbnail must render under its stable ID");

    let tool_bounds = cx
        .debug_bounds(DOCUMENT_RECTANGLE_TOOL_ID)
        .expect("the real GPUI Component Rectangle tool must render");
    cx.simulate_click(tool_bounds.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_bounds = cx
        .debug_bounds("document-1-annotation-layer-0")
        .expect("the page annotation layer must render");
    let scale =
        (f32::from(layer_bounds.size.width) / 612.).min(f32::from(layer_bounds.size.height) / 792.);
    let page_origin = point(
        layer_bounds.origin.x + px((f32::from(layer_bounds.size.width) - 612. * scale) / 2.),
        layer_bounds.origin.y + px((f32::from(layer_bounds.size.height) - 792. * scale) / 2.),
    );
    let start = point(
        page_origin.x + px(72. * scale),
        page_origin.y + px((792. - 96.) * scale),
    );
    let end = point(
        page_origin.x + px(216. * scale),
        page_origin.y + px((792. - 192.) * scale),
    );
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    let rectangle_id =
        MarkupId::new("workspace:rectangle:1").expect("the stable rectangle ID must be canonical");
    let annotation_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(first_id, cx)
        })
        .expect("the retained session must expose its annotation snapshot");
    assert_eq!(annotation_snapshot.revision, 1);
    assert_eq!(annotation_snapshot.saved_revision, 0);
    assert!(annotation_snapshot.dirty);
    assert_eq!(
        annotation_snapshot.selected_id.as_ref(),
        Some(&rectangle_id)
    );
    assert_eq!(annotation_snapshot.rectangles.len(), 1);
    let rectangle = annotation_snapshot.rectangles[0].rect;
    for (actual, expected) in [
        (rectangle.x, 72.),
        (rectangle.y, 96.),
        (rectangle.width, 144.),
        (rectangle.height, 96.),
    ] {
        assert!(
            (actual - expected).abs() < 0.001,
            "native pointer projection drifted: expected {expected}, got {actual}"
        );
    }
    let document_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(first_id, 0, cx)
    });
    let thumbnail_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.thumbnail_annotation_scene(first_id, 0, cx)
    });
    assert_eq!(document_scene.revision, 1);
    assert_eq!(thumbnail_scene.revision, 1);
    assert_eq!(document_scene.rectangles.len(), 1);
    assert_eq!(thumbnail_scene.rectangles.len(), 1);
    assert_eq!(
        document_scene.rectangles[0].id,
        thumbnail_scene.rectangles[0].id
    );
    assert_eq!(
        document_scene.rectangles[0].rect,
        thumbnail_scene.rectangles[0].rect
    );
    assert!(document_scene.rectangles[0].selected);
    assert!(!thumbnail_scene.rectangles[0].selected);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_RECTANGLE_TOOL_ID).is_some());
    assert_eq!(
        document_annotation_layer_id(first_id, 0),
        "document-1-annotation-layer-0"
    );
    assert!(cx.debug_bounds("document-1-annotation-layer-0").is_some());

    let stale_navigation = workspace.update(cx, |workspace, cx| {
        workspace
            .begin_page_navigation(first_id, 2, cx)
            .expect("page three must be navigable")
    });
    let current_navigation = workspace.update(cx, |workspace, cx| {
        workspace
            .begin_page_navigation(first_id, 1, cx)
            .expect("page two must be navigable")
    });
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &stale_navigation,
            Ok(raster(32, 42)),
            cx,
        )),
        ApplyDisposition::RejectedStale
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &current_navigation,
            Ok(raster(32, 41)),
            cx,
        )),
        ApplyDisposition::Applied
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(first_id, cx)
            .unwrap()
            .read(cx)
            .current_page()),
        1
    );

    cx.simulate_click(second_thumbnail.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(first_id, cx)
            .unwrap()
            .read(cx)
            .requested_page()),
        1,
        "the real thumbnail target must dispatch navigation by stable page identity"
    );

    let failed_request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("missing-second.pdf"), cx)
    });
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &failed_request,
            Err("deterministic second-open failure".into()),
            cx,
        )),
        ApplyDisposition::Applied
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(first_id)
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(first_id, cx)
            .unwrap()
            .read(cx)
            .current_page()),
        1
    );

    let pending = workspace.update(cx, |workspace, cx| {
        workspace
            .begin_page_navigation(first_id, 2, cx)
            .expect("the live document still accepts a request before close")
    });
    assert!(workspace.update(cx, |workspace, cx| workspace.close_document(first_id, cx)));
    assert!(released.load(Ordering::Acquire));
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(first_id, cx).is_none()
    }));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &pending,
            Ok(raster(32, 42)),
            cx,
        )),
        ApplyDisposition::RejectedClosed
    );
}

#[gpui::test]
fn document_worker_recovery_preserves_dirty_state_and_rejects_stale_replacements(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let opener = Arc::new(RecoveryOpener::default());
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let opener = opener.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(opener, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("bp-worker-recovery.pdf"), cx)
    });
    let document_id = request.document_id;
    let original_released = Arc::new(AtomicBool::new(false));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &request,
            Ok(opened_document(original_released.clone())),
            cx,
        )),
        ApplyDisposition::Applied
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("workspace:rectangle:recovery").unwrap(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 175., cx)
    });
    let navigation = workspace.update(cx, |workspace, cx| {
        workspace.begin_page_navigation(document_id, 1, cx).unwrap()
    });
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &navigation,
            Ok(raster(48, 60)),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let before_annotations = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let before_view = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(document_id, cx)
        })
        .unwrap();
    let before_raster = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(document_id, cx)
        })
        .unwrap();

    let failed_navigation = workspace.update(cx, |workspace, cx| {
        workspace.begin_page_navigation(document_id, 2, cx).unwrap()
    });
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &failed_navigation,
            Err("PDF worker WorkerCrashed: deterministic EOF".into()),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let failed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(
        failed.ready,
        "a presentation failure must not poison editable state"
    );
    assert_eq!(failed.current_page, 1);
    assert_eq!(failed.requested_page, 1);
    assert_eq!(
        failed.current_raster_width,
        before_raster.current_raster_width
    );
    assert_eq!(
        failed.current_raster_height,
        before_raster.current_raster_height
    );
    assert_eq!(
        failed.presentation_error.as_deref(),
        Some("PDF worker WorkerCrashed: deterministic EOF")
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)),
        Some(before_annotations.clone())
    );
    let failed_view = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(document_id, cx)
        })
        .unwrap();
    assert_eq!(failed_view.mode(), before_view.mode());
    assert_eq!(failed_view.zoom_preset(), before_view.zoom_preset());
    assert_eq!(failed_view.zoom_percent(), before_view.zoom_percent());
    assert_eq!(failed_view.scroll(), before_view.scroll());
    assert_eq!(
        failed_view.wheel_behavior(PageViewMode::SinglePage),
        before_view.wheel_behavior(PageViewMode::SinglePage)
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_RECOVERY_ALERT_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_RECOVERY_RETRY_ID).is_some());

    let stale = workspace.update(cx, |workspace, cx| {
        workspace.begin_document_recovery(document_id, cx).unwrap()
    });
    assert!(
        workspace
            .update(cx, |workspace, cx| workspace
                .begin_document_recovery(document_id, cx))
            .unwrap_err()
            .contains("already in progress")
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .apply_document_recovery_result(
                &stale,
                Err("deterministic recovery open failure".into()),
                cx,
            )),
        ApplyDisposition::Applied
    );
    let current = workspace.update(cx, |workspace, cx| {
        workspace.begin_document_recovery(document_id, cx).unwrap()
    });
    let stale_candidate = opener.open(&OpenDocumentRequest {
        document_id,
        generation: stale.generation,
        path: stale.path.clone(),
    });
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .apply_document_recovery_result(&stale, stale_candidate, cx,)),
        ApplyDisposition::RejectedStale
    );
    assert!(opener.released.lock().unwrap()[0].load(Ordering::Acquire));

    let recovered = workspace
        .read_with(cx, |workspace, _| {
            workspace.prepare_document_recovery(&current)
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .apply_document_recovery_result(&current, Ok(recovered), cx,)),
        ApplyDisposition::Applied
    );
    let after = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(after.ready);
    assert_eq!(after.current_page, 1);
    assert_eq!(after.requested_page, 1);
    assert!(after.presentation_error.is_none());
    assert!(after.recovery_pending.is_none());
    assert!(original_released.load(Ordering::Acquire));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)),
        Some(before_annotations)
    );
    let recovered_view = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(document_id, cx)
        })
        .unwrap();
    assert_eq!(recovered_view.mode(), before_view.mode());
    assert_eq!(recovered_view.zoom_preset(), before_view.zoom_preset());
    assert_eq!(recovered_view.zoom_percent(), before_view.zoom_percent());
    assert_eq!(recovered_view.scroll(), before_view.scroll());
    assert_eq!(
        recovered_view.wheel_behavior(PageViewMode::SinglePage),
        before_view.wheel_behavior(PageViewMode::SinglePage)
    );
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(document_id, cx)
    }));
    assert!(opener.released.lock().unwrap()[1].load(Ordering::Acquire));
}

#[gpui::test]
fn line_arrow_workspace_bridges_real_controls_drag_click_placement_and_shift(
    cx: &mut TestAppContext,
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
        workspace.begin_open(PathBuf::from("bp-line-arrow-bridge.pdf"), cx)
    });
    let document_id = request.document_id;
    let released = Arc::new(AtomicBool::new(false));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &request,
            Ok(opened_document(released)),
            cx,
        )),
        ApplyDisposition::Applied
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let line_button = cx
        .debug_bounds(DOCUMENT_LINE_TOOL_ID)
        .expect("the real Line Button must render under its stable ID");
    let arrow_button = cx
        .debug_bounds(DOCUMENT_ARROW_TOOL_ID)
        .expect("the real Arrow Button must render under its stable ID");
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));
    cx.simulate_keystrokes("l");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Line)
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(document_id, AnnotationTool::Select, cx)
        })
        .unwrap();
    let layer = cx
        .debug_bounds("document-1-annotation-layer-0")
        .expect("the real annotation canvas must render");
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let page_origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project = |x: f32, y: f32| {
        point(
            page_origin.x + px(x * scale),
            page_origin.y + px((792. - y) * scale),
        )
    };

    cx.simulate_click(line_button.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Line),
        "the real Line button must dispatch independently of its shortcut",
    );
    let line_start = project(72., 144.);
    let line_end = project(252., 240.);
    cx.simulate_mouse_down(line_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(line_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(line_end, MouseButton::Left, Modifiers::default());
    let first = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(first.straight_lines.len(), 1);
    assert_eq!(first.straight_lines[0].kind, LineKind::Line);
    assert_eq!(first.straight_lines[0].id.as_str(), "workspace:line:1");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("a");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Arrow)
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(document_id, AnnotationTool::Select, cx)
        })
        .unwrap();
    cx.simulate_click(arrow_button.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Arrow),
        "the real Arrow button must dispatch independently of its shortcut",
    );
    let arrow_start = project(90., 300.);
    let arrow_end = project(306., 324.);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    cx.simulate_mouse_down(arrow_start, MouseButton::Left, shift);
    cx.simulate_mouse_up(arrow_start, MouseButton::Left, shift);
    cx.simulate_event(MouseExitEvent {
        position: arrow_start,
        pressed_button: None,
        modifiers: shift,
    });
    cx.simulate_mouse_move(arrow_end, None, shift);
    let draft = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(document_id, 0, cx)
    });
    assert!(draft.straight_lines.iter().any(|line| line.draft));
    cx.simulate_mouse_down(arrow_end, MouseButton::Left, shift);
    cx.simulate_mouse_up(arrow_end, MouseButton::Left, shift);

    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(snapshot.straight_lines.len(), 2);
    let arrow = snapshot
        .straight_lines
        .iter()
        .find(|line| line.kind == LineKind::Arrow)
        .unwrap();
    assert_eq!(arrow.id.as_str(), "workspace:arrow:2");
    assert!(
        (arrow.start.y - arrow.end.y).abs() < 0.001,
        "Shift must preserve the gallery's orthogonal Arrow constraint"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
        "Line and Arrow creation are one-shot tools"
    );
    let scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(document_id, 0, cx)
    });
    assert_eq!(scene.straight_lines.len(), 2);
    assert!(scene.straight_lines.iter().all(|line| !line.draft));

    let line_id = MarkupId::new("workspace:line:1").unwrap();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &line_id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID);
    let properties = cx
        .debug_bounds(DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID)
        .expect("a selected Line must render the real properties trigger");
    assert!(properties.size.width > px(0.) && properties.size.height > px(0.));
    cx.simulate_click(properties.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let before_properties = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let color = cx
        .debug_bounds(DOCUMENT_STRAIGHT_LINE_COLOR_BLUE_ID)
        .expect("the real stroke-color control must render");
    cx.simulate_click(color.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_STRAIGHT_LINE_WIDTH_ID);
    let width_trigger = cx
        .debug_bounds(DOCUMENT_STRAIGHT_LINE_WIDTH_ID)
        .expect("the real stroke-width trigger must render");
    cx.simulate_click(width_trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let width = cx
        .debug_bounds(DOCUMENT_STRAIGHT_LINE_WIDTH_4_ID)
        .expect("the real 4 pt control must render");
    cx.simulate_click(width.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_STRAIGHT_LINE_OPACITY_ID);
    let opacity_trigger = cx
        .debug_bounds(DOCUMENT_STRAIGHT_LINE_OPACITY_ID)
        .expect("the real opacity trigger must render");
    cx.simulate_click(opacity_trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let opacity = cx
        .debug_bounds(DOCUMENT_STRAIGHT_LINE_OPACITY_50_ID)
        .expect("the real 50% control must render");
    cx.simulate_click(opacity.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.revision, before_properties.revision + 3);
    assert_eq!(edited.undo_depth, before_properties.undo_depth + 3);
    let edited_line = edited
        .straight_lines
        .iter()
        .find(|line| line.id == line_id)
        .unwrap();
    assert_eq!(edited_line.appearance.stroke_color(), "#2563eb");
    assert_eq!(edited_line.appearance.stroke_width_pt(), 4.);
    assert_eq!(edited_line.appearance.opacity(), 0.5);
    let unchanged_arrow = edited
        .straight_lines
        .iter()
        .find(|line| line.kind == LineKind::Arrow)
        .unwrap();
    assert_eq!(
        unchanged_arrow.appearance,
        StraightLineAppearance::default_for(LineKind::Arrow)
    );
    for scene in [
        workspace.read_with(cx, |workspace, cx| {
            workspace.annotation_scene(document_id, 0, cx)
        }),
        workspace.read_with(cx, |workspace, cx| {
            workspace.thumbnail_annotation_scene(document_id, 0, cx)
        }),
    ] {
        let scene_line = scene
            .straight_lines
            .iter()
            .find(|line| line.id == line_id)
            .unwrap();
        assert_eq!(scene_line.appearance, edited_line.appearance);
    }

    for _ in 0..3 {
        workspace
            .update(cx, |workspace, cx| {
                workspace.undo_annotations(document_id, cx)
            })
            .unwrap();
    }
    let undone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        undone
            .straight_lines
            .iter()
            .find(|line| line.id == line_id)
            .unwrap()
            .appearance,
        StraightLineAppearance::default_for(LineKind::Line)
    );
    for _ in 0..3 {
        workspace
            .update(cx, |workspace, cx| {
                workspace.redo_annotations(document_id, cx)
            })
            .unwrap();
    }
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(document_id, true, cx)
        })
        .unwrap();
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.edit_selected_straight_line_property(
                    document_id,
                    StraightLinePropertyEdit::Opacity(1.),
                    cx,
                )
            })
            .is_err(),
        "a locked Line must reject a semantic property edit"
    );
    let locked = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        locked
            .straight_lines
            .iter()
            .find(|line| line.id == line_id)
            .unwrap()
            .appearance,
        edited_line.appearance
    );
}

#[gpui::test]
fn semantic_snapping_workspace_uses_real_component_controls_and_controlled_state(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| gpui_component::init(cx));
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("bp-semantic-snapping-controls.pdf"), cx)
    });
    let document_id = request.document_id;
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )),
        ApplyDisposition::Applied,
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_SNAP_SETTINGS_ID);

    let trigger = cx
        .debug_bounds(DOCUMENT_SNAP_SETTINGS_ID)
        .expect("the real GPUI Component Snap settings Button must render");
    assert!(cx.debug_bounds(DOCUMENT_SNAP_POPOVER_ID).is_none());
    cx.simulate_click(trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_SNAP_POPOVER_ID).is_some());
    let markup = cx
        .debug_bounds(DOCUMENT_SNAP_MARKUP_ID)
        .expect("the real GPUI Component markup Checkbox must render");
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .semantic_snap_settings(document_id, cx))
            .unwrap()
            .annotations_enabled()
    );
    cx.simulate_click(markup.center(), Modifiers::default());
    assert!(
        !workspace
            .read_with(cx, |workspace, cx| workspace
                .semantic_snap_settings(document_id, cx))
            .unwrap()
            .annotations_enabled(),
        "the component callback must update the application-owned setting and active session",
    );
}

#[gpui::test]
fn semantic_snapping_workspace_routes_real_line_input_and_transient_guide_evidence(
    cx: &mut TestAppContext,
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
        workspace.begin_open(PathBuf::from("bp-semantic-snapping-line.pdf"), cx)
    });
    let document_id = request.document_id;
    let source_id = MarkupId::new("semantic-snap:workspace-source").unwrap();
    let source = RectangleAnnotation {
        id: source_id.clone(),
        page_index: 0,
        rect: PdfRect::new(100., 100., 100., 100.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Rectangle(source)])),
            cx,
        )),
        ApplyDisposition::Applied,
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_LINE_TOOL_ID);
    let line_button = cx
        .debug_bounds(DOCUMENT_LINE_TOOL_ID)
        .expect("the real GPUI Component Line button must render");
    cx.simulate_click(line_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the annotation layer must expose stable bounds");
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view = |x: f32, y: f32| {
        point(
            origin.x + px(x * scale),
            origin.y + px((792. - y) * scale),
        )
    };
    let history_before = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap()
        .undo_depth;

    cx.simulate_mouse_down(
        to_view(102., 100.),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(150., 198.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    let decision = workspace
        .read_with(cx, |workspace, cx| {
            workspace.semantic_snap_decision(document_id, cx)
        })
        .expect("the active gesture must expose an application-owned snap decision");
    assert_eq!(decision.point, PdfPoint::new(150., 200.).unwrap());
    assert_eq!(decision.owner_id.as_ref(), Some(&source_id));
    assert_eq!(decision.role, SemanticSnapRole::Midpoint);
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(document_id, cx)
            })
            .unwrap()
            .undo_depth,
        history_before,
        "preview snapping must not create history",
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let guide = cx
        .debug_bounds("snap-indicator")
        .expect("the raw GPUI domain guide must expose its stable evidence ID");
    assert_eq!((guide.size.width, guide.size.height), (px(1.), px(1.)));
    assert!(
        layer.contains(&guide.center()),
        "the noninteractive debug anchor must remain inside the annotation layer; the painted guide uses the live contained-page transform",
    );

    cx.simulate_mouse_up(
        to_view(150., 198.),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let created = snapshot
        .straight_lines
        .iter()
        .find(|line| line.id.as_str() == "workspace:line:1")
        .unwrap();
    assert_eq!(created.start, PdfPoint::new(100., 100.).unwrap());
    assert_eq!(created.end, PdfPoint::new(150., 200.).unwrap());
    assert_eq!(snapshot.undo_depth, history_before + 1);
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.semantic_snap_decision(document_id, cx)
            })
            .is_none(),
        "the guide decision must clear after commit",
    );
    assert!(cx.debug_bounds("snap-indicator").is_none());
}

#[test]
fn line_arrow_workspace_arrowhead_uses_the_frozen_pdf_point_geometry() {
    let points = straight_line_arrowhead_points(
        PdfPoint::new(0., 0.).unwrap(),
        PdfPoint::new(10., 0.).unwrap(),
        0.5,
    )
    .unwrap();
    assert_eq!(points[0], PdfPoint::new(10., 0.).unwrap());
    assert_eq!(points[1], PdfPoint::new(3., 2.).unwrap());
    assert_eq!(points[2], PdfPoint::new(3., -2.).unwrap());
}

#[test]
fn line_arrow_workspace_save_as_reconciles_two_typed_reopen_cycles() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let created_target = manifest_dir.join(format!(
        ".prepared/line-arrow-create-save-{}.pdf",
        std::process::id()
    ));
    let edited_target = manifest_dir.join(format!(
        ".prepared/line-arrow-edit-save-{}.pdf",
        std::process::id()
    ));
    let _scratch_files = ScratchFiles(vec![created_target.clone(), edited_target.clone()]);
    assert!(!created_target.exists() && !edited_target.exists());
    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let imported_lines = source_session.straight_lines().to_vec();
    let line = StraightLineAnnotation::new(
        MarkupId::new("workspace:line:persistence-1").unwrap(),
        0,
        PdfPoint::new(72., 144.).unwrap(),
        PdfPoint::new(252., 240.).unwrap(),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let arrow = StraightLineAnnotation::new(
        MarkupId::new("workspace:arrow:persistence-2").unwrap(),
        0,
        PdfPoint::new(90., 300.).unwrap(),
        PdfPoint::new(306., 300.).unwrap(),
        LineKind::Arrow,
        StraightLineAppearance::default_for(LineKind::Arrow),
    )
    .unwrap();
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(84),
            generation: 1,
            source_path: source.clone(),
            destination: save_as_destination(&source, &created_target),
            current_page: 0,
            annotation_revision: 1,
            annotations: AnnotationSnapshot {
                revision: 1,
                saved_revision: 0,
                dirty: true,
                selected_id: Some(arrow.id.clone()),
                annotation_order: Vec::new(),
                rectangles: source_session.rectangles().to_vec(),
                ellipses: source_session.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: source_session.pens().to_vec(),
                straight_lines: imported_lines
                    .iter()
                    .cloned()
                    .chain([line.clone(), arrow.clone()])
                    .collect(),
                vertex_paths: source_session.vertex_paths().to_vec(),

                clouds: source_session.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: source_session.callouts().to_vec(),
                measurement_paths: source_session.measurement_paths().to_vec(),
                text_boxes: source_session.text_boxes().to_vec(),
                lengths: source_session.lengths().to_vec(),
                images: source_session.images().to_vec(),
                snapshots: source_session.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 1,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Line and Arrow creation must survive typed Save As validation");
    let created = PdfPersistenceSession::open(&created_target).unwrap();
    assert!(created.straight_lines().contains(&line));
    assert!(created.straight_lines().contains(&arrow));

    let edited_arrow = StraightLineAnnotation::new(
        arrow.id.clone(),
        arrow.page_index,
        arrow.start,
        PdfPoint::new(342., 300.).unwrap(),
        arrow.kind,
        StraightLineAppearance::new("#2563eb", 4., 0.5, StrokeStyle::Solid).unwrap(),
    )
    .unwrap();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(84),
            generation: 2,
            source_path: created_target.clone(),
            destination: save_as_destination(&created_target, &edited_target),
            current_page: 0,
            annotation_revision: 2,
            annotations: AnnotationSnapshot {
                revision: 2,
                saved_revision: 1,
                dirty: true,
                selected_id: Some(edited_arrow.id.clone()),
                annotation_order: Vec::new(),
                rectangles: created.rectangles().to_vec(),
                ellipses: created.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: created.pens().to_vec(),
                straight_lines: imported_lines
                    .iter()
                    .cloned()
                    .chain([edited_arrow.clone()])
                    .collect(),
                vertex_paths: created.vertex_paths().to_vec(),

                clouds: created.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: created.callouts().to_vec(),
                measurement_paths: created.measurement_paths().to_vec(),
                text_boxes: created.text_boxes().to_vec(),
                lengths: created.lengths().to_vec(),
                images: created.images().to_vec(),
                snapshots: created.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 2,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Line deletion and Arrow edit must survive the second typed reopen");
    let edited = PdfPersistenceSession::open(&edited_target).unwrap();
    assert!(edited.straight_lines().contains(&edited_arrow));
    assert!(
        edited
            .straight_lines()
            .iter()
            .all(|value| value.id != line.id)
    );
    assert!(!edited.has_raw_annotation_name(&line.id));
}

#[gpui::test]
fn page_rotation_is_one_document_revision_rejects_stale_pixels_and_preserves_failure_state(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
        workspace.begin_open(PathBuf::from("bp-page-rotation-v1.pdf"), cx)
    });
    let released = Arc::new(AtomicBool::new(false));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &request,
            Ok(opened_rotation_document(released.clone())),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let document_id = request.document_id;
    let session = workspace
        .read_with(cx, |workspace, cx| {
            workspace.session(document_id, cx).cloned()
        })
        .unwrap();
    assert_eq!(
        session.read_with(cx, |session, _| session.page_rotation(0)),
        Some(PageRotation::Degrees0)
    );
    assert_eq!(
        session.read_with(cx, |session, _| session.page_size(0)),
        Some((2., 3.))
    );

    let rotate_right = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_rotation(document_id, 0, PageRotationDirection::Right, cx)
        })
        .unwrap();
    let dirty = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(dirty.revision, 1);
    assert!(dirty.dirty);
    assert_eq!(
        dirty.page_rotations,
        vec![(0, PageRotation::Degrees90), (1, PageRotation::Degrees90)]
    );
    assert_eq!(
        session.read_with(cx, |session, _| session.page_size(0)),
        Some((3., 2.))
    );
    let rotated = workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_rotation_for_evidence(&rotate_right, cx)
        })
        .unwrap();
    assert_eq!(
        (rotated.current_page.width(), rotated.current_page.height()),
        (3, 2)
    );
    assert_eq!(
        rotated.current_page.pixels_bgra(),
        &[
            5, 0, 0, 255, 3, 0, 0, 255, 1, 0, 0, 255, 6, 0, 0, 255, 4, 0, 0, 255, 2, 0, 0, 255,
        ]
    );

    let rotate_left = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_rotation(document_id, 0, PageRotationDirection::Left, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &rotate_right,
            Ok(rotated),
            cx,
        )),
        ApplyDisposition::RejectedStale
    );
    let unrotated = workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_rotation_for_evidence(&rotate_left, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &rotate_left,
            Ok(unrotated),
            cx,
        )),
        ApplyDisposition::Applied
    );
    assert_eq!(
        session.read_with(cx, |session, _| session.page_rotation(0)),
        Some(PageRotation::Degrees0)
    );
    assert_eq!(
        session.read_with(cx, |session, _| session.page_size(0)),
        Some((2., 3.))
    );

    let failed_rotation = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_rotation(document_id, 0, PageRotationDirection::Right, cx)
        })
        .unwrap();
    let pixels_before_failure = session.read_with(cx, |session, _| {
        session
            .current_base_raster()
            .unwrap()
            .pixels_bgra()
            .to_vec()
    });
    assert_eq!(
        workspace
            .update(cx, |workspace, cx| workspace.begin_save_as(
                document_id,
                PathBuf::from("must-not-save-pending-rotation.pdf"),
                cx,
            ))
            .unwrap_err(),
        "page rotation pixels are still pending; Save As is blocked"
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &failed_rotation,
            Err("deterministic rotation render failure".into()),
            cx,
        )),
        ApplyDisposition::Applied
    );
    assert_eq!(
        session.read_with(cx, |session, _| session.page_rotation(0)),
        Some(PageRotation::Degrees0),
        "a failed rotation render must roll back its document revision"
    );
    assert_eq!(
        session.read_with(cx, |session, _| session.page_size(0)),
        Some((2., 3.))
    );
    assert_eq!(
        session.read_with(cx, |session, _| session
            .current_base_raster()
            .unwrap()
            .pixels_bgra()
            .to_vec()),
        pixels_before_failure
    );
    assert_eq!(
        session.read_with(cx, |session, _| session
            .presentation_error()
            .map(str::to_owned)),
        Some("deterministic rotation render failure".to_owned())
    );
    assert!(session.read_with(cx, |session, _| matches!(
        session.status(),
        NativeDocumentStatus::Ready
    )));

    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        session.read_with(cx, |session, _| session.page_rotation(0)),
        Some(PageRotation::Degrees90)
    );
    assert_eq!(
        session.read_with(cx, |session, _| session.page_size(0)),
        Some((3., 2.))
    );
    assert_eq!(
        session.read_with(cx, |session, _| {
            let raster = session.current_base_raster().unwrap();
            (raster.width(), raster.height())
        }),
        (3, 2),
        "redo must regenerate coherent rotated pixels"
    );
    assert_eq!(
        session.read_with(cx, |session, _| {
            let raster = session.thumbnail_base_raster(0).unwrap();
            (raster.width(), raster.height())
        }),
        (3, 2),
        "redo must regenerate coherent rotated thumbnail pixels"
    );
    let late_after_undo = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_rotation(document_id, 0, PageRotationDirection::Right, cx)
        })
        .unwrap();
    let late_pixels = workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_rotation_for_evidence(&late_after_undo, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &late_after_undo,
            Ok(late_pixels),
            cx,
        )),
        ApplyDisposition::RejectedStale,
        "undo must invalidate a late rotation raster result"
    );
    let late_after_reopen = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_rotation(document_id, 0, PageRotationDirection::Left, cx)
        })
        .unwrap();
    let late_reopen_pixels = workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_rotation_for_evidence(&late_after_reopen, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &late_after_reopen,
            Ok(late_reopen_pixels.clone()),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let save_request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save_as(
                document_id,
                workspace_save_target("bp-page-rotation-reopened.pdf"),
                cx,
            )
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_save_result(
            &save_request,
            Ok(SavedNativeDocument::new(
                opened_rotation_document(Arc::new(AtomicBool::new(false))),
                save_request.annotation_revision,
            )),
            cx,
        )),
        ApplyDisposition::Applied
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &late_after_reopen,
            Ok(late_reopen_pixels),
            cx,
        )),
        ApplyDisposition::RejectedStale,
        "a validated reopen must invalidate a raster from the prior resource epoch"
    );

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(document_id, cx)
    }));
    assert!(released.load(Ordering::Acquire));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &failed_rotation,
            Err("closed".into()),
            cx,
        )),
        ApplyDisposition::RejectedClosed
    );
}

#[gpui::test]
fn real_component_rotation_buttons_dispatch_the_retained_rotation_journey(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("bp-page-rotation-ui-v1.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_rotation_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_ROTATE_LEFT_ID).is_some());
    let rotate_right = cx
        .debug_bounds(DOCUMENT_ROTATE_RIGHT_ID)
        .expect("the real GPUI Component Rotate Right button must render");
    cx.simulate_click(rotate_right.center(), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(request.document_id, cx)
            .unwrap()
            .read(cx)
            .page_rotation(0)),
        Some(PageRotation::Degrees90)
    );
}

#[gpui::test]
fn save_as_swaps_only_a_validated_reopen_and_preserves_the_live_document_on_failure(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot
        .borrow_mut()
        .take()
        .expect("the save harness must retain its workspace");
    let open_request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("source.pdf"), cx)
    });
    let original_released = Arc::new(AtomicBool::new(false));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &open_request,
            Ok(opened_document(original_released.clone())),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let rectangle_id = MarkupId::new("workspace:rectangle:save-1").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                open_request.document_id,
                0,
                rectangle_id.clone(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(72., 96.).unwrap(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();

    let failed_request: SaveDocumentRequest = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save_as(
                open_request.document_id,
                workspace_save_target("failed-save.pdf"),
                cx,
            )
        })
        .expect("a dirty ready document must begin Save As");
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_save_result(
            &failed_request,
            Err("injected reopen failure".into()),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let after_failure = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(open_request.document_id, cx)
        })
        .unwrap();
    assert!(after_failure.dirty);
    assert_eq!(after_failure.selected_id.as_ref(), Some(&rectangle_id));
    assert!(!original_released.load(Ordering::Acquire));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(open_request.document_id, cx)
                .unwrap()
                .read(cx)
                .path()
                .to_path_buf()
        }),
        PathBuf::from("source.pdf")
    );

    let successful_target = workspace_save_target("saved-and-reopened.pdf");
    let successful_request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save_as(open_request.document_id, successful_target.clone(), cx)
        })
        .unwrap();
    let pre_swap_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                open_request.document_id,
                PageViewMode::SinglePage,
                100.,
                1.,
                640.,
                480.,
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    let pre_swap_render = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(
                open_request.document_id,
                &pre_swap_plan,
                cx,
            )
        })
        .unwrap();
    assert!(pre_swap_render.rendered_tiles > 0);
    let reopened_released = Arc::new(AtomicBool::new(false));
    let saved = SavedNativeDocument::new(
        opened_document(reopened_released.clone()),
        successful_request.annotation_revision,
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_save_result(
            &successful_request,
            Ok(saved),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let after_success = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(open_request.document_id, cx)
        })
        .unwrap();
    assert!(!after_success.dirty);
    assert_eq!(after_success.revision, 1);
    assert_eq!(after_success.saved_revision, 1);
    assert_eq!(after_success.selected_id.as_ref(), Some(&rectangle_id));
    let post_swap_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                open_request.document_id,
                PageViewMode::SinglePage,
                100.,
                1.,
                640.,
                480.,
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    assert_ne!(post_swap_plan.generation, pre_swap_plan.generation);
    let post_swap_render = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(
                open_request.document_id,
                &post_swap_plan,
                cx,
            )
        })
        .unwrap();
    assert_eq!(post_swap_render.cache_hits, 0);
    assert_eq!(post_swap_render.rendered_tiles, post_swap_plan.tiles.len());
    assert!(original_released.load(Ordering::Acquire));
    assert!(!reopened_released.load(Ordering::Acquire));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(open_request.document_id, cx)
                .unwrap()
                .read(cx)
                .path()
                .to_path_buf()
        }),
        successful_target
    );
}

#[gpui::test]
fn in_place_save_targets_the_opened_path_and_preserves_the_live_dirty_session_on_failure(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let workspace = cx.new(DocumentWorkspace::new);
    let open_request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("opened-in-place.pdf"), cx)
    });
    let original_released = Arc::new(AtomicBool::new(false));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &open_request,
            Ok(opened_document(original_released.clone())),
            cx,
        )),
        ApplyDisposition::Applied
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                open_request.document_id,
                0,
                MarkupId::new("workspace:rectangle:in-place-save").unwrap(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();

    let request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save(open_request.document_id, cx)
        })
        .expect("a ready opened document must begin in-place Save");
    assert_eq!(request.source_path, PathBuf::from("opened-in-place.pdf"));
    assert_eq!(request.target_path(), request.source_path.as_path());
    assert!(request.is_in_place());
    assert_eq!(
        workspace
            .update(cx, |workspace, cx| workspace
                .begin_save(open_request.document_id, cx))
            .unwrap_err(),
        "document save is already in progress"
    );
    assert_eq!(
        workspace
            .update(cx, |workspace, cx| workspace.create_rectangle(
                open_request.document_id,
                0,
                MarkupId::new("workspace:rectangle:must-not-race-save").unwrap(),
                PdfPoint::new(240., 240.).unwrap(),
                PdfPoint::new(300., 300.).unwrap(),
                cx,
            ))
            .unwrap_err(),
        "document save is in progress"
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_save_result(
            &request,
            Err("injected in-place publication failure".into()),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let session = workspace
        .read_with(cx, |workspace, cx| {
            workspace.session(open_request.document_id, cx).cloned()
        })
        .unwrap();
    assert_eq!(
        session.read_with(cx, |session, _| session.path().to_path_buf()),
        request.source_path
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(open_request.document_id, cx))
            .unwrap()
            .dirty
    );
    assert!(matches!(
        session.read_with(cx, |session, _| session.save_status().clone()),
        butter_paper_gpui_component_compat::document_workspace::NativeDocumentSaveStatus::Failed(_)
    ));
    assert!(
        !original_released.load(Ordering::Acquire),
        "a failed in-place Save must preserve the live resource"
    );
}

#[gpui::test]
fn in_place_save_keeps_the_published_document_live_when_durability_has_a_warning(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let workspace = cx.new(DocumentWorkspace::new);
    let open_request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("published-with-warning.pdf"), cx)
    });
    let original_released = Arc::new(AtomicBool::new(false));
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &open_request,
            Ok(opened_document(original_released.clone())),
            cx,
        )
    });
    let request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save(open_request.document_id, cx)
        })
        .unwrap();
    let warning = "saved PDF was published, but its directory durability sync failed: injected";
    let reopened_released = Arc::new(AtomicBool::new(false));
    let saved = SavedNativeDocument::new(
        opened_document(reopened_released.clone()),
        request.annotation_revision,
    )
    .with_publication_warning(warning);

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.apply_save_result(&request, Ok(saved), cx)
        }),
        ApplyDisposition::Applied
    );
    assert!(original_released.load(Ordering::Acquire));
    assert!(!reopened_released.load(Ordering::Acquire));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status()),
        Some(warning.to_owned())
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(open_request.document_id, cx)
                .unwrap()
                .read(cx)
                .path()
                .to_path_buf()
        }),
        request.source_path
    );
}

#[gpui::test]
fn in_place_save_real_component_button_dispatches_the_active_opened_document(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("opened-save-button.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = cx
        .debug_bounds(DOCUMENT_SAVE_ID)
        .expect("the real GPUI Component Save button must render");
    cx.simulate_click(save.center(), Modifiers::default());
    assert!(matches!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(request.document_id, cx)
            .unwrap()
            .read(cx)
            .save_status()
            .clone()),
        butter_paper_gpui_component_compat::document_workspace::NativeDocumentSaveStatus::Failed(failure)
            if failure.message == "no document saver is configured"
    ));
}

#[gpui::test]
fn in_place_save_failure_renders_real_recovery_actions_without_losing_the_document(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("ordinary-save-recovery.pdf"), cx)
    });
    let original_released = Arc::new(AtomicBool::new(false));
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(&request, Ok(opened_document(original_released.clone())), cx)
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                request.document_id,
                0,
                MarkupId::new("workspace:rectangle:ordinary-save-recovery").unwrap(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = cx.debug_bounds(DOCUMENT_SAVE_ID).unwrap();
    cx.simulate_click(save.center(), Modifiers::default());
    cx.run_until_parked();
    let first_failure = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_save_failure(request.document_id, cx)
        })
        .expect("the failed ordinary Save must retain typed recovery state");
    assert_eq!(
        first_failure.operation,
        DocumentSaveFailureOperation::InPlace
    );
    assert_eq!(first_failure.message, "no document saver is configured");
    assert_eq!(first_failure.generation, 1);
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(request.document_id, cx)
            .unwrap()
            .dirty
    }));
    assert!(!original_released.load(Ordering::Acquire));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_SAVE_ERROR_ALERT_ID).is_some());
    let retry = cx.debug_bounds(DOCUMENT_SAVE_ERROR_RETRY_ID).unwrap();
    assert!(cx.debug_bounds(DOCUMENT_SAVE_ERROR_SAVE_AS_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_SAVE_ERROR_DISMISS_ID).is_some());
    cx.simulate_click(retry.center(), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.document_save_failure(request.document_id, cx)
            })
            .unwrap()
            .generation,
        2,
        "Retry must begin a fresh save attempt rather than replaying stale state"
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save_as = cx.debug_bounds(DOCUMENT_SAVE_ERROR_SAVE_AS_ID).unwrap();
    cx.simulate_click(save_as.center(), Modifiers::default());
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|directory| {
        assert_eq!(directory, Path::new(""));
        None
    });
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .document_save_failure(request.document_id, cx)
            .is_some()
    }));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let dismiss = cx.debug_bounds(DOCUMENT_SAVE_ERROR_DISMISS_ID).unwrap();
    cx.simulate_click(dismiss.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_SAVE_ERROR_ALERT_ID).is_none());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .document_save_failure(request.document_id, cx)
            .is_none()
            && workspace
                .annotation_snapshot(request.document_id, cx)
                .unwrap()
                .dirty
    }));
    assert!(!original_released.load(Ordering::Acquire));
}

#[gpui::test]
fn imported_rectangle_selection_property_delete_and_history_stay_document_owned(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("imported-rectangle.pdf"), cx)
    });
    let imported = imported_rectangle("pdf:imported-rectangle");
    let opened = opened_document(Arc::new(AtomicBool::new(false)))
        .with_annotations(vec![Annotation::Rectangle(imported.clone())]);
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(&request, Ok(opened), cx)
    });

    let initial = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(initial.rectangles, vec![imported.clone()]);
    assert_eq!(initial.revision, 0);
    assert_eq!(initial.saved_revision, 0);
    assert!(!initial.dirty);
    assert_eq!((initial.undo_depth, initial.redo_depth), (0, 0));

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &imported.id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_rectangle_stroke_width(request.document_id, 4., cx)
        })
        .unwrap();
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.rectangles[0].appearance.stroke_width_pt(), 4.);
    assert_eq!(
        (edited.revision, edited.undo_depth, edited.redo_depth),
        (1, 1, 0)
    );
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    let undone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(undone.rectangles[0].appearance.stroke_width_pt(), 1.5);
    assert_eq!((undone.undo_depth, undone.redo_depth), (0, 1));
    assert!(!undone.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(request.document_id, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    let deleted = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(deleted.rectangles.is_empty());
    assert_eq!((deleted.undo_depth, deleted.redo_depth), (2, 0));
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    let restored = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(restored.rectangles[0].appearance.stroke_width_pt(), 4.);
}

#[gpui::test]
fn rectangle_property_inspector_renders_stable_controls_and_commits_identity_bound_history(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("rectangle-property-inspector.pdf"), cx)
    });
    let imported = imported_rectangle("pdf:rectangle-property-inspector");
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Rectangle(imported.clone())])),
            cx,
        )
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &imported.id, cx)
    }));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_RECTANGLE_PROPERTIES_ID);
    let trigger = cx
        .debug_bounds(DOCUMENT_RECTANGLE_PROPERTIES_ID)
        .expect("the Rectangle selection must expose the real Properties trigger");
    cx.simulate_click(trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let panel = cx
        .debug_bounds(RECTANGLE_PROPERTY_INSPECTOR_ID)
        .expect("the real retained Rectangle inspector must render");
    assert_eq!(f32::from(panel.size.width), RECTANGLE_INSPECTOR_WIDTH_PX);
    for id in [
        RECTANGLE_INSPECTOR_LOCKED_ID,
        RECTANGLE_INSPECTOR_STROKE_COLOR_ID,
        RECTANGLE_INSPECTOR_OPACITY_ID,
        RECTANGLE_INSPECTOR_STROKE_WIDTH_ID,
        RECTANGLE_INSPECTOR_STROKE_STYLE_ID,
        RECTANGLE_INSPECTOR_FILL_ENABLED_ID,
        RECTANGLE_INSPECTOR_FILL_COLOR_ID,
        RECTANGLE_INSPECTOR_FILL_OPACITY_ID,
        RECTANGLE_INSPECTOR_X_ID,
        RECTANGLE_INSPECTOR_Y_ID,
        RECTANGLE_INSPECTOR_WIDTH_ID,
        RECTANGLE_INSPECTOR_HEIGHT_ID,
        RECTANGLE_INSPECTOR_ROTATION_ID,
    ] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "{id} must render under its stable ID"
        );
    }

    for patch in [
        RectanglePropertyPatch::StrokeColor("#dc2626".into()),
        RectanglePropertyPatch::Opacity(0.88),
        RectanglePropertyPatch::StrokeWidthPt(3.),
        RectanglePropertyPatch::StrokeStyle(StrokeStyle::Dotted),
        RectanglePropertyPatch::FillColor(Some("#abcdef".into())),
        RectanglePropertyPatch::FillOpacity(31. / 255.),
        RectanglePropertyPatch::X(12.),
        RectanglePropertyPatch::Y(24.),
        RectanglePropertyPatch::Width(80.),
        RectanglePropertyPatch::Height(40.),
        RectanglePropertyPatch::RotationDegrees(375.),
    ] {
        assert!(
            workspace
                .update(cx, |workspace, cx| workspace
                    .apply_rectangle_property_event(
                        &RectanglePropertyEvent {
                            document_id: request.document_id,
                            annotation_id: imported.id.clone(),
                            expected_kind: RectangularShapePropertyKind::Rectangle,
                            patch,
                        },
                        cx,
                    ))
                .unwrap()
        );
    }
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    let rectangle = &edited.rectangles[0];
    assert_eq!(rectangle.rect, PdfRect::new(12., 24., 80., 40.).unwrap());
    assert_eq!(rectangle.rotation_degrees, 15.);
    assert_eq!(rectangle.appearance.stroke_color(), "#dc2626");
    assert_eq!(rectangle.appearance.stroke_width_pt(), 3.);
    assert_eq!(rectangle.appearance.stroke_style(), StrokeStyle::Dotted);
    assert_eq!(rectangle.appearance.fill_color(), Some("#abcdef"));
    assert!((rectangle.appearance.fill_opacity() - 31. / 255.).abs() < 0.000_001);
    assert_eq!(rectangle.appearance.opacity(), 0.88);
    assert_eq!(
        (edited.revision, edited.undo_depth, edited.redo_depth),
        (11, 11, 0)
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let lock = cx
        .debug_bounds(RECTANGLE_INSPECTOR_LOCKED_ID)
        .expect("the real lock Switch must remain available");
    let viewport = cx.update(|window, _| window.viewport_size());
    let workspace_bounds = cx.debug_bounds(DOCUMENT_WORKSPACE_ID);
    let toolbar_bounds = cx.debug_bounds(DOCUMENT_TOOLBAR_SCROLL_ID);
    let thumbnail_bounds = cx.debug_bounds(DOCUMENT_THUMBNAIL_STRIP_ID);
    let inspector_scroll = cx.debug_bounds("rectangle-property-inspector-scroll");
    let details = cx.debug_bounds("rectangle-property-inspector-details");
    assert!(
        lock.right() <= viewport.width && lock.bottom() <= viewport.height,
        "the real lock Switch must be inside the test viewport; workspace={workspace_bounds:?}, toolbar={toolbar_bounds:?}, thumbnails={thumbnail_bounds:?}, panel={panel:?}, scroll={inspector_scroll:?}, details={details:?}, lock={lock:?}, viewport={viewport:?}"
    );
    cx.simulate_click(lock.center(), Modifiers::default());
    let locked = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(locked.rectangles[0].locked);
    assert_eq!(locked.revision, 12);
    assert!(
        workspace
            .update(cx, |workspace, cx| workspace
                .apply_rectangle_property_event(
                    &RectanglePropertyEvent {
                        document_id: request.document_id,
                        annotation_id: imported.id.clone(),
                        expected_kind: RectangularShapePropertyKind::Rectangle,
                        patch: RectanglePropertyPatch::Width(90.),
                    },
                    cx,
                ))
            .is_err()
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let unlock = cx.debug_bounds(RECTANGLE_INSPECTOR_LOCKED_ID).unwrap();
    cx.simulate_click(unlock.center(), Modifiers::default());
    assert!(
        !workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .rectangles[0]
            .locked
    );

    assert!(
        !workspace
            .update(cx, |workspace, cx| workspace
                .apply_rectangle_property_event(
                    &RectanglePropertyEvent {
                        document_id: request.document_id,
                        annotation_id: MarkupId::new("stale:rectangle").unwrap(),
                        expected_kind: RectangularShapePropertyKind::Rectangle,
                        patch: RectanglePropertyPatch::X(99.),
                    },
                    cx,
                ))
            .unwrap()
    );
    let unchanged = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(unchanged.rectangles[0].rect.x, 12.);
    assert_eq!(unchanged.revision, 13);

    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .rectangles[0]
            .locked
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert!(
        !workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .rectangles[0]
            .locked
    );
}

#[gpui::test]
fn shared_shape_property_inspector_ellipse_renders_and_commits_identity_bound_history(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("shared-shape-property-inspector.pdf"), cx)
    });
    let imported = EllipseAnnotation::new(
        MarkupId::new("pdf:ellipse-property-inspector").unwrap(),
        0,
        PdfRect::new(40., 60., 120., 80.).unwrap(),
        RectangleAppearance::default(),
    )
    .unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Ellipse(imported.clone())])),
            cx,
        )
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &imported.id, cx)
    }));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ELLIPSE_PROPERTIES_ID);
    let trigger = cx
        .debug_bounds(DOCUMENT_ELLIPSE_PROPERTIES_ID)
        .expect("the Ellipse selection must expose the shared Properties trigger");
    cx.simulate_click(trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let panel = cx
        .debug_bounds(ELLIPSE_PROPERTY_INSPECTOR_ID)
        .expect("the real retained Ellipse inspector must render");
    assert_eq!(f32::from(panel.size.width), RECTANGLE_INSPECTOR_WIDTH_PX);
    for id in [
        ELLIPSE_INSPECTOR_LOCKED_ID,
        ELLIPSE_INSPECTOR_STROKE_COLOR_ID,
        ELLIPSE_INSPECTOR_OPACITY_ID,
        ELLIPSE_INSPECTOR_STROKE_WIDTH_ID,
        ELLIPSE_INSPECTOR_STROKE_STYLE_ID,
        ELLIPSE_INSPECTOR_FILL_ENABLED_ID,
        ELLIPSE_INSPECTOR_FILL_COLOR_ID,
        ELLIPSE_INSPECTOR_FILL_OPACITY_ID,
        ELLIPSE_INSPECTOR_X_ID,
        ELLIPSE_INSPECTOR_Y_ID,
        ELLIPSE_INSPECTOR_WIDTH_ID,
        ELLIPSE_INSPECTOR_HEIGHT_ID,
        ELLIPSE_INSPECTOR_ROTATION_ID,
    ] {
        assert!(cx.debug_bounds(id).is_some(), "{id} must render under its stable ID");
    }

    let apply = |patch, cx: &mut gpui::VisualTestContext| {
        workspace
            .update(cx, |workspace, cx| {
                workspace.apply_rectangular_shape_property_event(
                    &RectanglePropertyEvent {
                        document_id: request.document_id,
                        annotation_id: imported.id.clone(),
                        expected_kind: RectangularShapePropertyKind::Ellipse,
                        patch,
                    },
                    cx,
                )
            })
    };
    for patch in [
        RectanglePropertyPatch::StrokeColor("#2563eb".into()),
        RectanglePropertyPatch::Opacity(0.72),
        RectanglePropertyPatch::StrokeWidthPt(4.),
        RectanglePropertyPatch::StrokeStyle(StrokeStyle::Dashed),
        RectanglePropertyPatch::FillColor(Some("#fef3c7".into())),
        RectanglePropertyPatch::FillOpacity(0.4),
        RectanglePropertyPatch::X(12.),
        RectanglePropertyPatch::Y(24.),
        RectanglePropertyPatch::Width(90.),
        RectanglePropertyPatch::Height(45.),
        RectanglePropertyPatch::RotationDegrees(375.),
    ] {
        assert!(apply(patch, cx).unwrap());
    }

    let edited = workspace
        .read_with(cx, |workspace, cx| workspace.annotation_snapshot(request.document_id, cx))
        .unwrap();
    let ellipse = &edited.ellipses[0];
    assert_eq!(ellipse.rect, PdfRect::new(12., 24., 90., 45.).unwrap());
    assert_eq!(ellipse.rotation_degrees, 15.);
    assert_eq!(ellipse.appearance.stroke_color(), "#2563eb");
    assert_eq!(ellipse.appearance.opacity(), 0.72);
    assert_eq!(ellipse.appearance.stroke_width_pt(), 4.);
    assert_eq!(ellipse.appearance.stroke_style(), StrokeStyle::Dashed);
    assert_eq!(ellipse.appearance.fill_color(), Some("#fef3c7"));
    assert_eq!(ellipse.appearance.fill_opacity(), 0.4);
    assert_eq!((edited.revision, edited.undo_depth, edited.redo_depth), (11, 11, 0));

    assert!(!apply(RectanglePropertyPatch::Width(90.), cx).unwrap());
    assert!(
        apply(RectanglePropertyPatch::Width(0.), cx).unwrap(),
        "the selected-property contract allows a zero-width Ellipse independently of placement"
    );
    assert!(apply(RectanglePropertyPatch::Width(90.), cx).unwrap());
    assert!(!workspace
        .update(cx, |workspace, cx| workspace.apply_rectangular_shape_property_event(
            &RectanglePropertyEvent {
                document_id: request.document_id,
                annotation_id: imported.id.clone(),
                expected_kind: RectangularShapePropertyKind::Rectangle,
                patch: RectanglePropertyPatch::X(100.),
            },
            cx,
        ))
        .unwrap());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let lock = cx
        .debug_bounds(ELLIPSE_INSPECTOR_LOCKED_ID)
        .expect("the Ellipse lock Switch must remain a real interactive control");
    cx.simulate_click(lock.center(), Modifiers::default());
    assert!(workspace
        .read_with(cx, |workspace, cx| workspace.annotation_snapshot(request.document_id, cx))
        .unwrap()
        .ellipses[0]
        .locked);
    assert!(apply(RectanglePropertyPatch::Width(100.), cx).is_err());
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace.annotation_snapshot(request.document_id, cx))
            .unwrap()
            .ellipses[0]
            .rect
            .width,
        90.
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let unlock = cx.debug_bounds(ELLIPSE_INSPECTOR_LOCKED_ID).unwrap();
    cx.simulate_click(unlock.center(), Modifiers::default());
    let unlocked = workspace
        .read_with(cx, |workspace, cx| workspace.annotation_snapshot(request.document_id, cx))
        .unwrap();
    assert!(!unlocked.ellipses[0].locked);
    assert_eq!((unlocked.revision, unlocked.undo_depth), (15, 15));
}

#[test]
fn shared_shape_property_inspector_ellipse_rotation_geometry_matches_handles() {
    let rect = PdfRect::new(40., 60., 120., 80.).unwrap();
    let (start, _) = ellipse_cubic_bezier_points(rect, 30.);
    let east = ellipse_resize_handle_point_for_rect(
        rect,
        30.,
        RectangleResizeHandle::East,
    );
    assert!((start.x - east.x).abs() < 0.000_001);
    assert!((start.y - east.y).abs() < 0.000_001);
}

#[gpui::test]
fn shared_shape_property_inspector_stays_open_across_rectangle_ellipse_selection(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("shared-shape-selection.pdf"), cx)
    });
    let rectangle = RectangleAnnotation {
        id: MarkupId::new("pdf:shared-rectangle").unwrap(),
        page_index: 0,
        rect: PdfRect::new(20., 30., 80., 50.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    let ellipse = EllipseAnnotation::new(
        MarkupId::new("pdf:shared-ellipse").unwrap(),
        0,
        PdfRect::new(120., 70., 90., 60.).unwrap(),
        RectangleAppearance::default(),
    )
    .unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false))).with_annotations(vec![
                Annotation::Rectangle(rectangle.clone()),
                Annotation::Ellipse(ellipse.clone()),
            ])),
            cx,
        )
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &rectangle.id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_RECTANGLE_PROPERTIES_ID);
    let trigger = cx.debug_bounds(DOCUMENT_RECTANGLE_PROPERTIES_ID).unwrap();
    cx.simulate_click(trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(RECTANGLE_PROPERTY_INSPECTOR_ID).is_some());

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &ellipse.id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(ELLIPSE_PROPERTY_INSPECTOR_ID).is_some(),
        "the open shared inspector must follow Rectangle to Ellipse selection"
    );

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &rectangle.id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(RECTANGLE_PROPERTY_INSPECTOR_ID).is_some(),
        "the open shared inspector must follow Ellipse back to Rectangle"
    );
}

#[gpui::test]
fn shared_shape_property_inspector_global_lock_unlocks_a_selected_ellipse(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("shared-shape-lock.pdf"), cx)
    });
    let ellipse = EllipseAnnotation::new(
        MarkupId::new("pdf:shared-ellipse-lock").unwrap(),
        0,
        PdfRect::new(40., 60., 120., 80.).unwrap(),
        RectangleAppearance::default(),
    )
    .unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Ellipse(ellipse.clone())])),
            cx,
        )
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &ellipse.id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_LOCK_ID);
    let lock = cx.debug_bounds(DOCUMENT_ANNOTATION_LOCK_ID).unwrap();
    cx.simulate_click(lock.center(), Modifiers::default());
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .ellipses[0]
            .locked
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_LOCK_ID);
    let unlock = cx.debug_bounds(DOCUMENT_ANNOTATION_LOCK_ID).unwrap();
    cx.simulate_click(unlock.center(), Modifiers::default());
    assert!(
        !workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .ellipses[0]
            .locked
    );
}

#[gpui::test]
fn imported_rectangle_pointer_move_and_resize_commit_previewed_geometry(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("imported-rectangle-pointer.pdf"), cx)
    });
    let imported = imported_rectangle("pdf:imported-pointer-rectangle");
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Rectangle(imported.clone())])),
            cx,
        )
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(request.document_id, AnnotationTool::Select, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_bounds = cx
        .debug_bounds("document-1-annotation-layer-0")
        .expect("the imported annotation layer must render");
    let scale =
        (f32::from(layer_bounds.size.width) / 612.).min(f32::from(layer_bounds.size.height) / 792.);
    let page_origin = point(
        layer_bounds.origin.x + px((f32::from(layer_bounds.size.width) - 612. * scale) / 2.),
        layer_bounds.origin.y + px((f32::from(layer_bounds.size.height) - 792. * scale) / 2.),
    );
    let to_view = |pdf_x: f32, pdf_y: f32| {
        point(
            page_origin.x + px(pdf_x * scale),
            page_origin.y + px((792. - pdf_y) * scale),
        )
    };

    let move_start = to_view(144., 144.);
    let move_end = to_view(162., 132.);
    cx.simulate_mouse_down(move_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(move_end, Some(MouseButton::Left), Modifiers::default());
    let move_preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert!(move_preview.rectangles[0].preview);
    for (actual, expected) in [
        (move_preview.rectangles[0].rect.x, 90.),
        (move_preview.rectangles[0].rect.y, 84.),
        (move_preview.rectangles[0].rect.width, 144.),
        (move_preview.rectangles[0].rect.height, 96.),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    cx.simulate_mouse_up(move_end, MouseButton::Left, Modifiers::default());

    let resize_start = to_view(234., 132.);
    let resize_end = to_view(264., 132.);
    cx.simulate_mouse_down(resize_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(resize_end, Some(MouseButton::Left), Modifiers::default());
    let resize_preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert!(resize_preview.rectangles[0].preview);
    for (actual, expected) in [
        (resize_preview.rectangles[0].rect.x, 90.),
        (resize_preview.rectangles[0].rect.y, 84.),
        (resize_preview.rectangles[0].rect.width, 174.),
        (resize_preview.rectangles[0].rect.height, 96.),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    cx.simulate_mouse_up(resize_end, MouseButton::Left, Modifiers::default());

    let committed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    for (actual, expected) in [
        (committed.rectangles[0].rect.x, 90.),
        (committed.rectangles[0].rect.y, 84.),
        (committed.rectangles[0].rect.width, 174.),
        (committed.rectangles[0].rect.height, 96.),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    assert_eq!(committed.selected_id.as_ref(), Some(&imported.id));
    assert_eq!(
        (
            committed.revision,
            committed.undo_depth,
            committed.redo_depth
        ),
        (2, 2, 0)
    );
}

#[gpui::test]
fn multi_selection_workspace_shift_click_group_move_is_ordered_lock_aware_and_one_revision(
    cx: &mut TestAppContext,
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
        workspace.begin_open(PathBuf::from("multi-selection.pdf"), cx)
    });
    let rectangle_a = imported_rectangle("multi:rectangle:a");
    let line_b = StraightLineAnnotation::new(
        MarkupId::new("multi:line:b").unwrap(),
        0,
        PdfPoint::new(50., 300.).unwrap(),
        PdfPoint::new(90., 300.).unwrap(),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    let rectangle_c = RectangleAnnotation {
        id: MarkupId::new("multi:rectangle:c-locked").unwrap(),
        page_index: 0,
        rect: PdfRect::new(300., 96., 144., 96.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: true,
    };
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(
                opened_document(Arc::new(AtomicBool::new(false))).with_annotations(vec![
                    Annotation::Rectangle(rectangle_a.clone()),
                    Annotation::StraightLine(line_b.clone()),
                    Annotation::Rectangle(rectangle_c.clone()),
                ]),
            ),
            cx,
        )
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(request.document_id, AnnotationTool::Select, cx)
        })
        .unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));

    cx.simulate_click(to_view(144., 144.), Modifiers::default());
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    cx.simulate_click(to_view(70., 300.), shift);
    cx.simulate_click(to_view(372., 144.), shift);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![
            rectangle_a.id.clone(),
            line_b.id.clone(),
            rectangle_c.id.clone(),
        ],
    );

    let start = to_view(70., 300.);
    let end = to_view(85., 310.);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    let preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    let preview_a = preview
        .rectangles
        .iter()
        .find(|annotation| annotation.id == rectangle_a.id)
        .unwrap();
    assert!(preview_a.preview);
    assert!((preview_a.rect.x - 87.).abs() < 0.001);
    let preview_b = preview
        .straight_lines
        .iter()
        .find(|annotation| annotation.id == line_b.id)
        .unwrap();
    assert!(preview_b.draft);
    assert!((preview_b.start.x - 65.).abs() < 0.001);
    let preview_c = preview
        .rectangles
        .iter()
        .find(|annotation| annotation.id == rectangle_c.id)
        .unwrap();
    assert!(!preview_c.preview);
    assert_eq!(preview_c.rect, rectangle_c.rect);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    let moved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((moved.revision, moved.undo_depth), (1, 1));
    let moved_a = moved
        .rectangles
        .iter()
        .find(|annotation| annotation.id == rectangle_a.id)
        .unwrap();
    for (actual, expected) in [(moved_a.rect.x, 87.), (moved_a.rect.y, 106.)] {
        assert!((actual - expected).abs() < 0.001);
    }
    let moved_b = moved
        .straight_lines
        .iter()
        .find(|annotation| annotation.id == line_b.id)
        .unwrap();
    for (actual, expected) in [
        (moved_b.start.x, 65.),
        (moved_b.start.y, 310.),
        (moved_b.end.x, 105.),
        (moved_b.end.y, 310.),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    assert_eq!(
        moved
            .rectangles
            .iter()
            .find(|annotation| annotation.id == rectangle_c.id)
            .unwrap()
            .rect,
        rectangle_c.rect,
    );
    let scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(
        scene
            .rectangles
            .iter()
            .filter(|annotation| annotation.selected)
            .count()
            + scene
                .straight_lines
                .iter()
                .filter(|annotation| annotation.selected)
                .count(),
        3,
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    let undone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        undone.rectangles,
        vec![rectangle_a.clone(), rectangle_c.clone()]
    );
    assert_eq!(undone.straight_lines, vec![line_b.clone()]);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![rectangle_a.id, line_b.id, rectangle_c.id],
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap()
            .straight_lines,
        moved.straight_lines,
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));
    cx.simulate_keystrokes("ctrl-c ctrl-v");
    let pasted_ids = workspace.read_with(cx, |workspace, cx| {
        workspace.selected_annotation_ids(request.document_id, cx)
    });
    assert_eq!(pasted_ids.len(), 3);
    assert!(
        pasted_ids
            .iter()
            .all(|id| id.as_str().starts_with("workspace:paste:"))
    );
    let pasted = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((pasted.revision, pasted.undo_depth), (2, 2));
    let pasted_a = pasted
        .rectangles
        .iter()
        .find(|annotation| annotation.id == pasted_ids[0])
        .unwrap();
    for (actual, expected) in [(pasted_a.rect.x, 99.), (pasted_a.rect.y, 94.)] {
        assert!((actual - expected).abs() < 0.001);
    }
    assert!(
        pasted
            .rectangles
            .iter()
            .find(|annotation| annotation.id == pasted_ids[2])
            .unwrap()
            .locked
    );

    cx.simulate_keystrokes("backspace");
    let deleted = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((deleted.revision, deleted.undo_depth), (3, 3));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![pasted_ids[2].clone()],
    );
    assert!(
        deleted
            .rectangles
            .iter()
            .all(|annotation| annotation.id != pasted_ids[0])
    );
    assert!(
        deleted
            .straight_lines
            .iter()
            .all(|annotation| annotation.id != pasted_ids[1])
    );
    assert!(
        deleted
            .rectangles
            .iter()
            .any(|annotation| annotation.id == pasted_ids[2])
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![pasted_ids[2].clone()],
        "undo filters current selection but does not restore its historical members",
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(request.document_id, cx)
        })
        .unwrap();
    cx.simulate_keystrokes("ctrl-a");
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.selected_annotation_ids(request.document_id, cx)
            })
            .len(),
        4,
        "Select All must include all current-page annotations, including locked annotations",
    );

    let selected = workspace.read_with(cx, |workspace, cx| {
        workspace.selected_annotation_ids(request.document_id, cx)
    });
    for id in &selected {
        assert!(workspace.update(cx, |workspace, cx| {
            workspace.toggle_annotation_selection(request.document_id, id, cx)
        }));
    }
    let locked_primary = MarkupId::new("multi:rectangle:c-locked").unwrap();
    let unlocked_secondary = MarkupId::new("multi:rectangle:a").unwrap();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.toggle_annotation_selection(request.document_id, &locked_primary, cx)
    }));
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.toggle_annotation_selection(request.document_id, &unlocked_secondary, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_DELETE_ID);
    let delete_center = cx
        .debug_bounds(DOCUMENT_ANNOTATION_DELETE_ID)
        .unwrap()
        .center();
    cx.simulate_click(delete_center, Modifiers::default());
    let mixed_deleted = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(
        mixed_deleted
            .rectangles
            .iter()
            .any(|annotation| annotation.id == locked_primary)
    );
    assert!(
        mixed_deleted
            .rectangles
            .iter()
            .all(|annotation| annotation.id != unlocked_secondary),
        "the real Delete button must remain enabled when a locked primary has an unlocked selected peer",
    );
}

#[gpui::test]
fn multi_selection_workspace_marquee_routes_real_pointer_coordinates_and_modifiers(
    cx: &mut TestAppContext,
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
        workspace.begin_open(PathBuf::from("marquee-selection.pdf"), cx)
    });
    let rectangle = RectangleAnnotation {
        id: MarkupId::new("marquee-workspace:rectangle").unwrap(),
        page_index: 0,
        rect: PdfRect::new(72., 96., 144., 96.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    let line = StraightLineAnnotation::new(
        MarkupId::new("marquee-workspace:line").unwrap(),
        0,
        PdfPoint::new(300., 100.).unwrap(),
        PdfPoint::new(350., 150.).unwrap(),
        LineKind::Line,
        StraightLineAppearance::default_for(LineKind::Line),
    )
    .unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(
                opened_document(Arc::new(AtomicBool::new(false))).with_annotations(vec![
                    Annotation::Rectangle(rectangle.clone()),
                    Annotation::StraightLine(line.clone()),
                ]),
            ),
            cx,
        )
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(request.document_id, AnnotationTool::Select, cx)
        })
        .unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    let drag_lasso =
        |cx: &mut gpui::VisualTestContext, points: &[(f32, f32)], modifiers: Modifiers| {
            let start = to_view(points[0].0, points[0].1);
            cx.simulate_mouse_down(start, MouseButton::Left, modifiers);
            for &(x, y) in &points[1..points.len() - 1] {
                cx.simulate_mouse_move(to_view(x, y), Some(MouseButton::Left), modifiers);
            }
            let end = points[points.len() - 1];
            cx.simulate_mouse_up(to_view(end.0, end.1), MouseButton::Left, modifiers);
        };

    let before = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    let box_start = to_view(50., 220.);
    let exact_threshold = point(box_start.x + px(6.), box_start.y);
    cx.simulate_mouse_down(box_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        exact_threshold,
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(exact_threshold, MouseButton::Left, Modifiers::default());
    let armed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.active_selection_marquee(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(armed.0, 0);
    assert_eq!(
        armed.1.shape,
        butter_paper_gpui_gallery::selection_geometry::SelectionShape::Box
    );
    assert!(!armed.1.active);
    let box_end = to_view(240., 70.);
    cx.simulate_mouse_move(box_end, None, Modifiers::default());
    let preview = workspace
        .read_with(cx, |workspace, cx| {
            workspace.active_selection_marquee(request.document_id, cx)
        })
        .unwrap();
    assert!(preview.1.active);
    assert_ne!(preview.1.start, preview.1.current);
    cx.simulate_click(box_end, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![rectangle.id.clone()],
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.active_selection_marquee(request.document_id, cx)
            })
            .is_none()
    );
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.toggle_annotation_selection(request.document_id, &rectangle.id, cx)
    }));

    drag_lasso(
        cx,
        &[(50., 220.), (240., 220.), (240., 70.), (50., 70.)],
        Modifiers::default(),
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![rectangle.id.clone()],
    );

    drag_lasso(
        cx,
        &[(280., 180.), (370., 180.), (370., 70.), (280., 70.)],
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![rectangle.id.clone(), line.id.clone()],
    );

    drag_lasso(
        cx,
        &[(50., 220.), (240., 220.), (240., 70.), (50., 70.)],
        Modifiers {
            shift: true,
            alt: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(request.document_id, cx)),
        vec![line.id],
        "Alt removal must take precedence over Shift addition",
    );
    let after = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (after.revision, after.undo_depth),
        (before.revision, before.undo_depth)
    );
}

#[gpui::test]
fn fresh_workspace_reopen_advances_generated_annotation_ids_past_imported_ids(
    cx: &mut TestAppContext,
) {
    let workspace = cx.new(DocumentWorkspace::new);
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("reopened-generated-ids.pdf"), cx)
    });
    let imported = RectangleAnnotation {
        id: MarkupId::new("workspace:paste:rectangle:41").unwrap(),
        page_index: 0,
        rect: PdfRect::new(72., 96., 144., 96.).unwrap(),
        rotation_degrees: 0.,
        appearance: RectangleAppearance::default(),
        locked: false,
    };
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Rectangle(imported.clone())])),
            cx,
        )
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &imported.id, cx)
    }));
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.copy_selected_annotations(request.document_id, cx)
        }),
        1
    );
    let pasted = workspace
        .update(cx, |workspace, cx| {
            workspace.paste_annotations(request.document_id, 0, cx)
        })
        .unwrap();
    assert_eq!(
        pasted,
        vec![MarkupId::new("workspace:paste:rectangle:42").unwrap()]
    );
}

#[gpui::test]
fn pen_pointer_drag_creates_a_stable_selected_path_and_ignores_short_gestures(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("pen-pointer.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_PEN_TOOL_ID);
    let pen_button = cx
        .debug_bounds(DOCUMENT_PEN_TOOL_ID)
        .expect("Pen control must render");
    cx.simulate_click(pen_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    let short_start = to_view(72., 96.);
    let short_end = to_view(73., 97.);
    cx.simulate_mouse_down(short_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(short_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(short_end, MouseButton::Left, Modifiers::default());
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens
            .is_empty()
    );

    let start = to_view(72., 96.);
    let middle = to_view(120., 144.);
    let end = to_view(180., 160.);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(middle, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(snapshot.pens.len(), 1);
    assert_eq!(snapshot.pens[0].id.as_str(), "workspace:pen:2");
    assert_eq!(snapshot.selected_id.as_ref(), Some(&snapshot.pens[0].id));
    assert_eq!(snapshot.pens[0].appearance.color(), "#ff0000");
    assert_eq!(snapshot.pens[0].appearance.width_pt(), 1.);
    assert_eq!(snapshot.pens[0].appearance.opacity(), 1.);
    assert!(snapshot.pens[0].smooth_curves);
    assert_eq!((snapshot.revision, snapshot.undo_depth), (1, 1));
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace.thumbnail_annotation_scene(
                request.document_id,
                0,
                cx,
            ))
            .pens
            .len(),
        1
    );
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_PEN_OPACITY_ID);
    let opacity = cx
        .debug_bounds(DOCUMENT_PEN_OPACITY_ID)
        .expect("selected Pen must expose its real opacity control");
    cx.simulate_click(opacity.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let half_opacity = cx
        .debug_bounds(DOCUMENT_PEN_OPACITY_50_ID)
        .expect("Pen opacity menu must expose its 50% preset");
    cx.simulate_click(half_opacity.center(), Modifiers::default());
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens[0]
            .appearance
            .opacity(),
        0.5
    );
    let original_points = snapshot.pens[0].points().to_vec();
    workspace
        .update(cx, |workspace, cx| {
            workspace.move_selected_pen(request.document_id, 10., 20., cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_pen_opacity(request.document_id, 0.75, cx)
        })
        .unwrap();
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.pens[0].appearance.opacity(), 0.75);
    assert_eq!(edited.pens[0].points()[0].x, original_points[0].x + 10.);
    assert_eq!(edited.pens[0].points()[0].y, original_points[0].y + 20.);
    assert_eq!((edited.revision, edited.undo_depth), (4, 4));
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens[0]
            .appearance
            .opacity(),
        0.5
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(request.document_id, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, true, cx)
        })
        .unwrap();
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.move_selected_pen(request.document_id, 1., 1., cx)
            })
            .is_err()
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, false, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens
            .is_empty()
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens
            .len(),
        1
    );
}

#[gpui::test]
fn highlight_real_control_pointer_history_and_scene_stay_application_owned(
    cx: &mut TestAppContext,
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
        workspace.begin_open(PathBuf::from("highlight-pointer.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_HIGHLIGHT_TOOL_ID).is_some());
    let focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| focus.focus(window, cx));
    cx.simulate_keystrokes("h");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Highlight)
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));

    let start = to_view(72., 240.);
    let middle = to_view(144., 252.);
    let end = to_view(240., 244.);
    cx.simulate_keystrokes("escape");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select)
    );
    cx.simulate_keystrokes("h");
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(middle, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(middle, MouseButton::Left, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select)
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens
            .is_empty()
    );
    cx.simulate_keystrokes("h");
    cx.simulate_mouse_down(start, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Right, Modifiers::default());
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens
            .is_empty()
    );
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(middle, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(created.pens.len(), 1);
    let highlight = &created.pens[0];
    assert_eq!(highlight.id.as_str(), "workspace:highlight:2");
    assert_eq!(highlight.tool(), InkTool::Highlight);
    assert_eq!(highlight.blend_mode(), BlendMode::Multiply);
    assert_eq!(highlight.appearance.color(), "#ffff00");
    assert_eq!(highlight.appearance.width_pt(), 12.);
    assert_eq!(highlight.appearance.opacity(), 1.);
    assert!(!highlight.smooth_curves);
    assert_eq!(created.selected_id.as_ref(), Some(&highlight.id));
    assert_eq!((created.revision, created.undo_depth), (1, 1));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select),
        "successful Highlight placement must return to Select"
    );
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace.thumbnail_annotation_scene(
                request.document_id,
                0,
                cx,
            ))
            .pens
            .len(),
        1
    );
    let stable_composite = workspace
        .read_with(cx, |workspace, cx| {
            workspace.highlight_composite_evidence(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(stable_composite.annotation_revision, 1);
    assert!(stable_composite.current_page_pixels > 0);
    assert!(stable_composite.thumbnail_pixels > 0);
    let original_start = highlight.points()[0];
    cx.simulate_mouse_down(middle, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        to_view(154., 272.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(to_view(154., 272.), MouseButton::Left, Modifiers::default());
    let moved_start = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap()
        .pens[0]
        .points()[0];
    assert!((moved_start.x - (original_start.x + 10.)).abs() < 0.001);
    assert!((moved_start.y - (original_start.y + 20.)).abs() < 0.001);
    let highlight_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::Continuous,
                100.,
                1.,
                720.,
                600.,
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &highlight_plan, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.highlight_composite_evidence(request.document_id, cx)
            })
            .unwrap()
            .viewer_tile_pixels
            > 0
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, true, cx)
        })
        .unwrap();
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.move_selected_pen(request.document_id, 1., 1., cx)
            })
            .is_err()
    );
    let locked_before_delete = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap(),
        locked_before_delete
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, false, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens
            .is_empty()
    );
    let deleted_composite = workspace
        .read_with(cx, |workspace, cx| {
            workspace.highlight_composite_evidence(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        deleted_composite.annotation_revision,
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap()
            .revision
    );
    assert_eq!(deleted_composite.current_page_pixels, 0);
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .pens
            .len(),
        1
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.highlight_composite_evidence(request.document_id, cx)
            })
            .unwrap()
            .current_page_pixels
            > 0
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_HIGHLIGHT_TOOL_ID);
    let highlight_button = cx.debug_bounds(DOCUMENT_HIGHLIGHT_TOOL_ID).unwrap();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: highlight_button.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: highlight_button.center(),
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let green = cx.debug_bounds(DOCUMENT_HIGHLIGHT_COLOR_GREEN_ID).unwrap();
    cx.simulate_click(green.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let width = cx.debug_bounds(DOCUMENT_HIGHLIGHT_WIDTH_18_ID).unwrap();
    cx.simulate_click(width.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let opacity = cx.debug_bounds(DOCUMENT_HIGHLIGHT_OPACITY_50_ID).unwrap();
    cx.simulate_click(opacity.center(), Modifiers::default());
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.highlight_defaults(request.document_id, cx)
            })
            .unwrap(),
        butter_paper_gpui_component_compat::document_workspace::PenAnnotationDefaults {
            color: "#00ff00".to_owned(),
            width_pt: 18.,
            opacity: 0.5,
        }
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_highlight(
                request.document_id,
                0,
                MarkupId::new("workspace:highlight:custom-defaults").unwrap(),
                &[
                    PdfPoint::new(72., 300.).unwrap(),
                    PdfPoint::new(180., 300.).unwrap(),
                ],
                cx,
            )
        })
        .unwrap();
    let custom = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap()
        .pens
        .into_iter()
        .find(|pen| pen.id.as_str() == "workspace:highlight:custom-defaults")
        .unwrap();
    assert_eq!(custom.appearance.color(), "#00ff00");
    assert_eq!(custom.appearance.width_pt(), 18.);
    assert_eq!(custom.appearance.opacity(), 0.5);
}

#[gpui::test]
fn text_box_click_opens_real_multiline_editor_and_blur_commits_once(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
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
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_window_active()));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("text-box-editor.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_TEXT_BOX_TOOL_ID);
    let tool = cx
        .debug_bounds(DOCUMENT_TEXT_BOX_TOOL_ID)
        .expect("real Text Box tool must render");
    cx.simulate_click(tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::TextBox)
    );
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let placement = point(
        origin.x + px(72. * scale),
        origin.y + px((792. - 96.) * scale),
    );
    cx.simulate_mouse_down(placement, MouseButton::Left, Modifiers::default());
    let editor_focus = workspace
        .read_with(cx, |workspace, cx| workspace.pending_text_box_focus(cx))
        .expect("primary page click must retain a pending Text Box editor");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    assert!(cx.update(|window, _| editor_focus.is_focused(window)));
    let pending = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(pending.text_boxes.is_empty());
    assert_eq!((pending.revision, pending.undo_depth), (0, 0));

    cx.simulate_keystrokes("h e l l o enter w o r l d");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some("hello\nworld".to_owned())
    );
    let return_focus = workspace.read_with(cx, |workspace, _| workspace.text_box_return_focus());
    cx.update(|window, cx| return_focus.focus(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    assert!(cx.update(|window, _| return_focus.is_focused(window)));
    assert!(!cx.update(|window, _| editor_focus.is_focused(window)));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        None,
        "blur must finish the retained edit session before document state is observed"
    );
    assert!(workspace.read_with(cx, |workspace, _| {
        workspace.text_box_commit_error().is_none()
    }));
    let committed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(committed.text_boxes.len(), 1);
    assert_eq!(committed.text_boxes[0].id.as_str(), "workspace:text:1");
    assert_eq!(committed.text_boxes[0].content(), "hello\nworld");
    assert_eq!(committed.text_boxes[0].style().font_family(), "Helvetica");
    assert_eq!(committed.text_boxes[0].style().font_size_pt(), 12.);
    assert_eq!(committed.text_boxes[0].style().color(), "#ff0000");
    assert_eq!((committed.revision, committed.undo_depth), (1, 1));
    assert_eq!(committed.selected_id, None);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select)
    );
}

#[gpui::test]
fn text_box_escape_commits_while_empty_blur_discards_without_phantom_history(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_window_active()));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("text-box-commit-policy.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let placement = point(
        origin.x + px(96. * scale),
        origin.y + px((792. - 120.) * scale),
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_TEXT_BOX_TOOL_ID);
    let tool = cx.debug_bounds(DOCUMENT_TEXT_BOX_TOOL_ID).unwrap();
    cx.simulate_click(tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_down(placement, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("x escape");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let after_escape = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(after_escape.text_boxes.len(), 1);
    assert_eq!(after_escape.text_boxes[0].content(), "x");
    assert_eq!((after_escape.revision, after_escape.undo_depth), (1, 1));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        None
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_TEXT_BOX_TOOL_ID);
    let tool = cx.debug_bounds(DOCUMENT_TEXT_BOX_TOOL_ID).unwrap();
    cx.simulate_click(tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_mouse_down(
        point(placement.x + px(24.), placement.y + px(24.)),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let return_focus = workspace.read_with(cx, |workspace, _| workspace.text_box_return_focus());
    cx.update(|window, cx| return_focus.focus(window, cx));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let after_empty_blur = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(after_empty_blur.text_boxes.len(), 1);
    assert_eq!(
        (after_empty_blur.revision, after_empty_blur.undo_depth),
        (1, 1)
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        None
    );
}

#[gpui::test]
fn text_box_edit_geometry_lock_delete_and_history_stay_document_owned(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let workspace = cx.new(DocumentWorkspace::new);
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("text-box-domain.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    let id = MarkupId::new("workspace:text:domain-1").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_text_box(
                request.document_id,
                TextBoxAnnotation::new(
                    id.clone(),
                    0,
                    PdfRect::new(72., 96., 80., 32.).unwrap(),
                    "original",
                    TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                )
                .unwrap(),
                cx,
            )
        })
        .unwrap();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.replace_selected_text(request.document_id, "edited\ntext", cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.resize_selected_text(request.document_id, 120., 48., cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.move_selected_text(request.document_id, 12., 18., cx)
        })
        .unwrap();
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.text_boxes[0].content(), "edited\ntext");
    assert_eq!(
        edited.text_boxes[0].layout_rect,
        PdfRect::new(84., 114., 120., 48.).unwrap()
    );
    assert_eq!((edited.revision, edited.undo_depth), (4, 4));

    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, true, cx)
        })
        .unwrap();
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.move_selected_text(request.document_id, 1., 1., cx)
            })
            .is_err()
    );
    let locked_before_delete = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap(),
        locked_before_delete
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, false, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .text_boxes
            .is_empty()
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .text_boxes
            .len(),
        1
    );
}

#[gpui::test]
fn length_uses_two_click_placement_scale_guard_preview_and_shift_constraint(
    cx: &mut TestAppContext,
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
    cx.run_until_parked();
    assert!(cx.update(|window, _| window.is_window_active()));
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("length-two-click.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    let opened = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((opened.revision, opened.undo_depth), (0, 0));
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("shift-alt-l");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Length),
        "the frozen Electron Shift+Alt+L shortcut must select Length in the workspace context"
    );
    let shortcut_selected = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (shortcut_selected.revision, shortcut_selected.undo_depth),
        (0, 0)
    );
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_LENGTH_TOOL_ID);
    let length_tool = cx
        .debug_bounds(DOCUMENT_LENGTH_TOOL_ID)
        .expect("the real Length tool must render");
    let rotate_left = cx
        .debug_bounds(DOCUMENT_ROTATE_LEFT_ID)
        .expect("the fixed Rotate Left control must render");
    assert!(
        length_tool.right() <= rotate_left.left(),
        "a scrolled annotation target must not overlap fixed document controls"
    );
    cx.simulate_click(length_tool.center(), Modifiers::default());
    let toolbar_selected = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (toolbar_selected.revision, toolbar_selected.undo_depth),
        (0, 0)
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    cx.simulate_click(to_view(72., 240.), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.annotation_status()),
        Some("Set page scale before placing measurement markups.".to_owned())
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .lengths
            .is_empty()
    );
    let rejected = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((rejected.revision, rejected.undo_depth), (0, 0));

    workspace
        .update(cx, |workspace, cx| {
            workspace.set_length_calibration(
                request.document_id,
                LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap(),
                cx,
            )
        })
        .unwrap();
    let calibrated = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        (calibrated.revision, calibrated.undo_depth),
        (1, 1),
        "setting the page scale must be the only committed history step before Length placement"
    );
    cx.simulate_click(to_view(72., 240.), Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.length_placement_pending(request.document_id, cx)
    }));
    let pending = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((pending.revision, pending.undo_depth), (1, 1));
    cx.simulate_mouse_move(to_view(216., 264.), None, Modifiers::default());
    let preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(preview.lengths.len(), 1);
    assert_eq!(preview.lengths[0].id.as_str(), "workspace:length:1");
    cx.simulate_click(to_view(216., 264.), Modifiers::default());
    let committed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(committed.lengths.len(), 1);
    assert_eq!(committed.lengths[0].id.as_str(), "workspace:length:1");
    assert_eq!(committed.lengths[0].caption(), "2.03 m");
    assert_eq!((committed.revision, committed.undo_depth), (2, 2));
    assert_eq!(
        committed.selected_id.as_ref(),
        Some(&committed.lengths[0].id)
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Length)
    );

    cx.simulate_click(to_view(300., 300.), Modifiers::default());
    cx.simulate_mouse_move(
        to_view(340., 330.),
        None,
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    let constrained = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    let constrained_preview = constrained.lengths.last().unwrap();
    assert!(
        constrained_preview.start.x == constrained_preview.end.x
            || constrained_preview.start.y == constrained_preview.end.y
    );
    cx.simulate_click(
        to_view(340., 330.),
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .lengths
            .len(),
        2
    );

    cx.simulate_click(to_view(400., 400.), Modifiers::default());
    cx.simulate_click(to_view(402., 400.), Modifiers::default());
    let minimum_rejected = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        minimum_rejected.lengths.len(),
        2,
        "exactly two PDF points is a no-op"
    );
    assert!(!workspace.read_with(cx, |workspace, cx| {
        workspace.length_placement_pending(request.document_id, cx)
    }));
    cx.simulate_click(to_view(430., 430.), Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.length_placement_pending(request.document_id, cx)
    }));
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("escape");
    assert!(!workspace.read_with(cx, |workspace, cx| {
        workspace.length_placement_pending(request.document_id, cx)
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select),
        "Escape must return to the frozen Electron Select tool"
    );
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .lengths
            .len(),
        2
    );
}

#[gpui::test]
fn page_scale_dialog_picks_two_points_applies_current_page_and_enables_length(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("page-scale.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    scroll_annotation_target_into_view(cx, &workspace, PAGE_SCALE_TRIGGER_ID);
    let trigger = cx
        .debug_bounds(PAGE_SCALE_TRIGGER_ID)
        .expect("the real Set Page Scale button must render");
    cx.simulate_click(trigger.center(), Modifiers::default());
    assert_eq!(
        workspace
            .read_with(cx, |workspace, _| workspace.page_scale_control())
            .unwrap()
            .read_with(cx, |control, _| control.target()),
        Some((request.document_id, 0)),
        "the real trigger must target the active stable document before opening the dialog"
    );
    cx.run_until_parked();
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(PAGE_SCALE_DIALOG_ID, "page-scale-dialog");
    assert_eq!(PAGE_SCALE_PICK_STATUS_ID, "page-scale-pick-status");
    assert_eq!(
        PAGE_SCALE_KNOWN_LENGTH_ID,
        "page-scale-calibrate-real-length"
    );
    assert_eq!(PAGE_SCALE_PICK_ID, "page-scale-pick-calibration");
    let control = workspace
        .read_with(cx, |workspace, _| workspace.page_scale_control())
        .unwrap();
    cx.update(|window, cx| {
        control.update(cx, |control, cx| control.begin_pick(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(control.read_with(cx, |control, _| control.is_picking()));
    assert!(cx.debug_bounds(PAGE_SCALE_PICK_ALERT_ID).is_some());
    assert!(cx.debug_bounds(PAGE_SCALE_PICK_CANCEL_ID).is_some());

    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    cx.simulate_click(to_view(72., 240.), Modifiers::default());
    cx.simulate_click(
        to_view(144., 260.),
        Modifiers {
            shift: true,
            ..Modifiers::default()
        },
    );
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    let (start, end) = control.read_with(cx, |control, _| control.points());
    let start = start.unwrap();
    let end = end.unwrap();
    assert!((start.x - 72.).abs() < 0.001 && (start.y - 240.).abs() < 0.001);
    assert!((end.x - 144.).abs() < 0.001 && (end.y - start.y).abs() < 0.001);
    let known_length = control.read_with(cx, |control, _| control.known_length_input());
    cx.update(|window, cx| {
        known_length.update(cx, |input, cx| input.set_value("2", window, cx));
    });

    assert_eq!(PAGE_SCALE_APPLY_ID, "page-scale-apply");
    assert!(control.update(cx, |control, cx| control.apply(cx)));
    cx.run_until_parked();
    let applied_calibration = workspace
        .read_with(cx, |workspace, cx| {
            workspace.page_length_calibration(request.document_id, 0, cx)
        })
        .unwrap();
    assert!(
        (applied_calibration.scale_x() * 72. - 2.).abs() < 0.001,
        "the authoritative page-scale ratio must preserve two metres per 72 PDF points"
    );
    let calibrated_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(
        calibrated_snapshot.dirty,
        "page scale alone is a document mutation"
    );
    assert_eq!(calibrated_snapshot.revision, 1);
    assert_eq!(calibrated_snapshot.page_length_calibrations.len(), 1);
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(request.document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired,
        "scale-only edits must cross the dirty-close boundary"
    );
    workspace.update(cx, |workspace, cx| workspace.resolve_dirty_close_cancel(cx));
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace.page_length_calibration(
                request.document_id,
                0,
                cx
            ))
            .is_none(),
        "page scale must share the document undo history"
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace.page_length_calibration(
                request.document_id,
                1,
                cx
            ))
            .is_none(),
        "the calibration must remain current-page scoped"
    );

    cx.update(|window, cx| window.close_dialog(cx));
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_annotation_tool(request.document_id, AnnotationTool::Length, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds("document-1-annotation-layer-0").unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_current_view =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    cx.simulate_click(to_current_view(72., 300.), Modifiers::default());
    cx.simulate_click(to_current_view(216., 300.), Modifiers::default());
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(snapshot.lengths.len(), 1);
    assert_eq!(snapshot.lengths[0].caption(), "4.000 m");
    assert!(snapshot.dirty);

    let second = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("page-scale-independent.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &second,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace.page_length_calibration(
                second.document_id,
                0,
                cx
            ))
            .is_none(),
        "page scale state must not leak to a second stable session"
    );
    cx.update(|window, cx| {
        control.update(cx, |control, cx| {
            control.open_for(second.document_id, 0, window, cx)
        });
    });
    assert_eq!(
        control.read_with(cx, |control, _| control.points()),
        (None, None)
    );
    assert_eq!(
        control.read_with(cx, |control, _| control.mode()),
        PageScaleMode::Preset,
        "the frozen Electron contract defaults a newly opened scale dialog to Preset"
    );
    assert!(control.update(cx, |control, cx| control.apply(cx)));
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.page_scale(second.document_id, 0, cx)
            })
            .unwrap()
            .source,
        ScaleSource::Preset
    );
}

#[gpui::test]
fn page_scale_dialog_custom_xy_fraction_range_and_saved_preset_are_one_revision(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("page-scale-complete.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let control = workspace
        .read_with(cx, |workspace, _| workspace.page_scale_control())
        .unwrap();
    cx.update(|window, cx| {
        control.update(cx, |control, cx| {
            control.open_for(request.document_id, 0, window, cx)
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| {
        control.update(cx, |control, cx| {
            control.configure_custom_for_test(
                ScaleUnit::In,
                ScaleUnit::Ft,
                true,
                ScalePrecision::fraction(16).unwrap(),
                PageScalePagesMode::Custom,
                true,
                window,
                cx,
            );
            control
                .pdf_length_input()
                .update(cx, |input, cx| input.set_value("1", window, cx));
            control
                .known_length_input()
                .update(cx, |input, cx| input.set_value("2", window, cx));
            control
                .y_pdf_length_input()
                .update(cx, |input, cx| input.set_value("2", window, cx));
            control
                .y_real_length_input()
                .update(cx, |input, cx| input.set_value("9", window, cx));
            control
                .custom_range_input()
                .update(cx, |input, cx| input.set_value("2-3", window, cx));
        });
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let visible_ids = control.read_with(cx, |control, _| control.visible_stable_ids());
    for stable_id in [
        PAGE_SCALE_DIALOG_ID,
        PAGE_SCALE_DIALOG_BODY_ID,
        PAGE_SCALE_METHOD_PRESET_ID,
        PAGE_SCALE_METHOD_CUSTOM_ID,
        PAGE_SCALE_CUSTOM_PDF_LENGTH_ID,
        PAGE_SCALE_CUSTOM_PDF_UNITS_ID,
        PAGE_SCALE_CUSTOM_REAL_LENGTH_ID,
        PAGE_SCALE_CUSTOM_REAL_UNITS_ID,
        PAGE_SCALE_SEPARATE_Y_ID,
        PAGE_SCALE_Y_PDF_LENGTH_ID,
        PAGE_SCALE_Y_REAL_LENGTH_ID,
        PAGE_SCALE_PAGES_ID,
        PAGE_SCALE_RANGE_ID,
        PAGE_SCALE_PRECISION_MODE_ID,
        PAGE_SCALE_PRECISION_VALUE_ID,
        PAGE_SCALE_SAVE_PRESET_ID,
        PAGE_SCALE_APPLY_ID,
    ] {
        assert!(
            visible_ids.contains(&stable_id),
            "the deterministic real-component render branch must include {stable_id}"
        );
    }

    assert!(control.update(cx, |control, cx| control.apply(cx)));
    let applied = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!((applied.revision, applied.undo_depth), (1, 1));
    assert_eq!(
        applied
            .page_scales
            .iter()
            .map(|scale| scale.page_index)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(
        applied
            .page_scales
            .iter()
            .all(|scale| scale.source == ScaleSource::Custom
                && scale.precision == ScalePrecision::fraction(16).unwrap()
                && (scale.scale_x - (2. / 72.)).abs() < 0.000_001
                && (scale.scale_y - (9. / 144.)).abs() < 0.000_001)
    );
    assert_eq!(applied.scale_presets.len(), 1);
    assert_eq!(applied.scale_presets[0].id, "scale-native-1");
    assert!(!applied.scale_presets[0].built_in);
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.page_scale(request.document_id, 0, cx)
            })
            .is_none(),
        "the custom range must preserve the untargeted current page"
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    let undone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(undone.page_scales.is_empty());
    assert!(undone.scale_presets.is_empty());
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(request.document_id, cx)
        })
        .unwrap();
    let redone = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(redone.page_scales, applied.page_scales);
    assert_eq!(redone.scale_presets, applied.scale_presets);
}

#[test]
fn page_scale_dialog_recalibrates_existing_lengths_in_one_history_revision() {
    let original = LengthCalibration::from_scale(72., 1., "m", 2, false)
        .unwrap()
        .with_label("Span")
        .unwrap();
    let length = LengthAnnotation::new(
        MarkupId::new("workspace:length:page-scale-history").unwrap(),
        0,
        PdfPoint::new(72., 144.).unwrap(),
        PdfPoint::new(216., 144.).unwrap(),
        original.clone(),
    )
    .unwrap();
    let mut adapter = AnnotationAdapter::default();
    adapter
        .load_imported_annotations(41, vec![Annotation::Length(length)])
        .unwrap();
    let canonical_before = adapter.canonical_json_snapshot(41).unwrap();

    let replacement = LengthCalibration::from_scale(72., 2., "m", 2, true).unwrap();
    adapter
        .set_document_page_length_calibration(41, 0, replacement.clone())
        .unwrap();
    let changed = adapter.snapshot(41).unwrap();
    assert_eq!((changed.revision, changed.undo_depth), (1, 1));
    assert!(changed.dirty);
    assert_eq!(changed.lengths[0].caption(), "Span: 4.00 m");
    assert_eq!(changed.lengths[0].calibration().label(), "Span");
    assert!(!changed.lengths[0].calibration().show_caption());
    assert_ne!(
        adapter.canonical_json_snapshot(41).unwrap(),
        canonical_before
    );

    adapter
        .set_document_page_length_calibration(
            41,
            0,
            LengthCalibration::from_scale(36., 1., "m", 2, true).unwrap(),
        )
        .unwrap();
    assert_eq!(
        adapter.snapshot(41).unwrap().revision,
        1,
        "an equivalent ratio is a no-op"
    );

    adapter.undo(41).unwrap();
    let undone = adapter.snapshot(41).unwrap();
    assert_eq!(undone.lengths[0].calibration(), &original);
    assert_eq!(undone.page_length_calibrations, vec![(0, original)]);
}

#[test]
fn page_scale_dialog_save_as_round_trips_scale_without_a_length_annotation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let target = manifest_dir.join(format!(
        ".prepared/page-scale-only-save-{}.pdf",
        std::process::id()
    ));
    assert!(!target.exists());
    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let scale = LengthCalibration::from_scale(72., 2., "m", 3, true).unwrap();
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(42),
            generation: 1,
            source_path: source.clone(),
            destination: save_as_destination(&source, &target),
            current_page: 0,
            annotation_revision: 1,
            annotations: AnnotationSnapshot {
                revision: 1,
                saved_revision: 0,
                dirty: true,
                selected_id: None,
                annotation_order: Vec::new(),
                rectangles: source_session.rectangles().to_vec(),
                ellipses: source_session.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: source_session.pens().to_vec(),
                straight_lines: Vec::new(),
                vertex_paths: source_session.vertex_paths().to_vec(),

                clouds: source_session.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: source_session.callouts().to_vec(),
                measurement_paths: source_session.measurement_paths().to_vec(),
                text_boxes: source_session.text_boxes().to_vec(),
                lengths: Vec::new(),
                images: source_session.images().to_vec(),
                snapshots: source_session.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: vec![(0, scale.clone())],
                page_rotations: Vec::new(),
                undo_depth: 1,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .unwrap();
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    let reopened_scale = reopened.page_length_calibrations().get(&0).unwrap();
    assert!(reopened_scale.same_scale_as(&scale));
    assert!(reopened.lengths().is_empty());
    std::fs::remove_file(target).unwrap();
}

#[test]
fn page_rotation_save_as_writes_pdf_rotate_and_independently_reopens_it() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let target = manifest_dir.join(format!(
        ".prepared/page-rotation-save-{}.pdf",
        std::process::id()
    ));
    assert!(!target.exists());
    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let original_direct_rotations = (0..source_session.page_count() as u32)
        .map(|page_index| source_session.direct_page_rotation(page_index))
        .collect::<Vec<_>>();
    let source_rotation = source_session.page_rotation(0).unwrap();
    let saved_rotation = source_rotation.rotate(PageRotationDirection::Right);
    let mut page_rotations = source_session
        .page_rotations()
        .iter()
        .map(|(page_index, rotation)| (*page_index, *rotation))
        .collect::<Vec<_>>();
    page_rotations
        .iter_mut()
        .find(|(page_index, _)| *page_index == 0)
        .unwrap()
        .1 = saved_rotation;
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(43),
            generation: 1,
            source_path: source.clone(),
            destination: save_as_destination(&source, &target),
            current_page: 0,
            annotation_revision: 1,
            annotations: AnnotationSnapshot {
                revision: 1,
                saved_revision: 0,
                dirty: true,
                selected_id: None,
                annotation_order: Vec::new(),
                rectangles: source_session.rectangles().to_vec(),
                ellipses: source_session.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: source_session.pens().to_vec(),
                straight_lines: Vec::new(),
                vertex_paths: source_session.vertex_paths().to_vec(),

                clouds: source_session.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: source_session.callouts().to_vec(),
                measurement_paths: source_session.measurement_paths().to_vec(),
                text_boxes: source_session.text_boxes().to_vec(),
                lengths: source_session.lengths().to_vec(),
                images: source_session.images().to_vec(),
                snapshots: source_session.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: source_session
                    .page_length_calibrations()
                    .iter()
                    .map(|(page_index, calibration)| (*page_index, calibration.clone()))
                    .collect(),
                page_rotations,
                undo_depth: 1,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .unwrap();
    assert_eq!(
        PdfPersistenceSession::open(&target)
            .unwrap()
            .page_rotation(0),
        Some(saved_rotation)
    );
    let saved_session = PdfPersistenceSession::open(&target).unwrap();
    assert_eq!(
        saved_session.direct_page_rotation(0),
        Some(saved_rotation.degrees())
    );
    for page_index in 1..source_session.page_count() as u32 {
        assert_eq!(
            saved_session.direct_page_rotation(page_index),
            original_direct_rotations[page_index as usize],
            "an untouched page must preserve an inherited or omitted /Rotate dictionary"
        );
    }
    assert_ne!(
        PdfPersistenceSession::open(&source)
            .unwrap()
            .page_rotation(0),
        Some(saved_rotation),
        "Save As must not mutate the source PDF"
    );
    std::fs::remove_file(target).unwrap();
}

#[test]
fn save_without_rotation_changes_preserves_every_direct_or_inherited_rotate_entry() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    let target = manifest_dir.join(format!(
        ".prepared/page-rotation-untouched-save-{}.pdf",
        std::process::id()
    ));
    assert!(!target.exists());
    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let direct = (0..source_session.page_count() as u32)
        .map(|page_index| source_session.direct_page_rotation(page_index))
        .collect::<Vec<_>>();
    source_session.save_as(&target).unwrap();
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    assert_eq!(reopened.page_count(), direct.len());
    for (page_index, expected) in direct.into_iter().enumerate() {
        assert_eq!(reopened.direct_page_rotation(page_index as u32), expected);
    }
    std::fs::remove_file(target).unwrap();
}

#[gpui::test]
fn page_scale_dialog_first_pick_binds_its_page_and_closed_targets_cannot_apply(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("page-scale-page-authority.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, PAGE_SCALE_TRIGGER_ID);
    let page_scale_trigger = cx.debug_bounds(PAGE_SCALE_TRIGGER_ID).unwrap().center();
    cx.simulate_click(page_scale_trigger, Modifiers::default());
    let control = workspace
        .read_with(cx, |workspace, _| workspace.page_scale_control())
        .unwrap();
    cx.update(|window, cx| {
        control.update(cx, |control, cx| control.begin_pick(window, cx));
        assert_eq!(
            control.update(cx, |control, cx| {
                control.record_point(
                    request.document_id,
                    1,
                    PdfPoint::new(72., 240.).unwrap(),
                    false,
                    window,
                    cx,
                )
            }),
            CalibrationPointDisposition::FirstPoint
        );
    });
    assert_eq!(
        control.read_with(cx, |control, _| control.target()),
        Some((request.document_id, 1)),
        "the first point, not the dialog-origin page, owns calibration authority"
    );
    cx.update(|window, cx| {
        assert_eq!(
            control.update(cx, |control, cx| {
                control.record_point(
                    request.document_id,
                    0,
                    PdfPoint::new(144., 240.).unwrap(),
                    false,
                    window,
                    cx,
                )
            }),
            CalibrationPointDisposition::Ignored
        );
        assert_eq!(
            control.update(cx, |control, cx| {
                control.record_point(
                    request.document_id,
                    1,
                    PdfPoint::new(144., 240.).unwrap(),
                    false,
                    window,
                    cx,
                )
            }),
            CalibrationPointDisposition::Completed
        );
    });
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.close_document(request.document_id, cx));
    });
    assert_eq!(
        control.read_with(cx, |control, _| control.points()),
        (None, None)
    );
    assert!(!control.update(cx, |control, cx| control.apply(cx)));
}

#[gpui::test]
fn retained_length_body_endpoint_lock_delete_and_undo_are_one_document_history(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("length-edit.pdf"), cx)
    });
    let length = LengthAnnotation::new(
        MarkupId::new("pdf:length-edit-1").unwrap(),
        0,
        PdfPoint::new(72., 144.).unwrap(),
        PdfPoint::new(216., 144.).unwrap(),
        LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap(),
    )
    .unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Length(length.clone())])),
            cx,
        )
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &length.id, cx)
    }));

    workspace
        .update(cx, |workspace, cx| {
            workspace.move_selected_length(request.document_id, 10., 20., cx)
        })
        .unwrap();
    let moved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(moved.lengths[0].start, PdfPoint::new(82., 164.).unwrap());
    assert_eq!(moved.lengths[0].end, PdfPoint::new(226., 164.).unwrap());
    assert_eq!((moved.revision, moved.undo_depth), (1, 1));

    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_length_endpoint(
                request.document_id,
                LengthEndpoint::End,
                PdfPoint::new(298., 164.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let endpoint_edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(endpoint_edited.lengths[0].caption(), "3.00 m");
    assert_eq!(
        (endpoint_edited.revision, endpoint_edited.undo_depth),
        (2, 2)
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, true, cx)
        })
        .unwrap();
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.move_selected_length(request.document_id, 1., 1., cx)
            })
            .is_err()
    );
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.set_selected_length_endpoint(
                    request.document_id,
                    LengthEndpoint::Start,
                    PdfPoint::new(90., 170.).unwrap(),
                    cx,
                )
            })
            .is_err()
    );
    let locked_before_delete = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap(),
        locked_before_delete
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(request.document_id, false, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(request.document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .lengths
            .is_empty()
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .lengths
            .len(),
        1
    );
}

#[gpui::test]
fn real_component_annotation_controls_dispatch_retained_commands(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("annotation-controls.pdf"), cx)
    });
    let imported = imported_rectangle("pdf:component-controls-rectangle");
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))
                .with_annotations(vec![Annotation::Rectangle(imported.clone())])),
            cx,
        )
    });
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &imported.id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for id in [
        DOCUMENT_SELECT_TOOL_ID,
        DOCUMENT_RECTANGLE_TOOL_ID,
        DOCUMENT_ANNOTATION_UNDO_ID,
        DOCUMENT_ANNOTATION_REDO_ID,
        DOCUMENT_RECTANGLE_STROKE_ID,
        DOCUMENT_ANNOTATION_LOCK_ID,
        DOCUMENT_ANNOTATION_DELETE_ID,
    ] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "{id} must render under its stable ID"
        );
    }

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_RECTANGLE_STROKE_ID);

    let stroke_center = cx
        .debug_bounds(DOCUMENT_RECTANGLE_STROKE_ID)
        .unwrap()
        .center();
    cx.simulate_click(stroke_center, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let width_four = cx
        .debug_bounds("document-workspace-stroke-4")
        .expect("the real GPUI Component stroke menu must open");
    cx.simulate_click(width_four.center(), Modifiers::default());
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap()
            .rectangles[0]
            .appearance
            .stroke_width_pt(),
        4.
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_UNDO_ID);
    let undo_center = cx
        .debug_bounds(DOCUMENT_ANNOTATION_UNDO_ID)
        .unwrap()
        .center();
    cx.simulate_click(undo_center, Modifiers::default());
    assert!(
        !workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .dirty
    );
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_REDO_ID);
    let redo_center = cx
        .debug_bounds(DOCUMENT_ANNOTATION_REDO_ID)
        .unwrap()
        .center();
    cx.simulate_click(redo_center, Modifiers::default());
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_LOCK_ID);
    let lock_center = cx
        .debug_bounds(DOCUMENT_ANNOTATION_LOCK_ID)
        .unwrap()
        .center();
    cx.simulate_click(lock_center, Modifiers::default());
    let locked = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert!(locked.rectangles[0].locked);
    let locked_revision = locked.revision;
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_DELETE_ID);
    let delete_center = cx
        .debug_bounds(DOCUMENT_ANNOTATION_DELETE_ID)
        .unwrap()
        .center();
    cx.simulate_click(delete_center, Modifiers::default());
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(request.document_id, cx)
            })
            .unwrap()
            .revision,
        locked_revision,
        "the disabled Delete control must suppress locked annotation deletion"
    );
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_LOCK_ID);
    let unlock_center = cx
        .debug_bounds(DOCUMENT_ANNOTATION_LOCK_ID)
        .unwrap()
        .center();
    cx.simulate_click(unlock_center, Modifiers::default());
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ANNOTATION_DELETE_ID);
    let delete_center = cx
        .debug_bounds(DOCUMENT_ANNOTATION_DELETE_ID)
        .unwrap()
        .center();
    cx.simulate_click(delete_center, Modifiers::default());
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .rectangles
            .is_empty()
    );
}

#[gpui::test]
fn polyline_polygon_workspace_renders_real_tools_and_retains_independent_vertex_paths(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("vertex-paths.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    for id in [DOCUMENT_POLYLINE_TOOL_ID, DOCUMENT_POLYGON_TOOL_ID] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "{id} must be a real GPUI Component Button with a stable rendered id"
        );
    }

    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_POLYLINE_TOOL_ID);
    let polyline_tool = cx.debug_bounds(DOCUMENT_POLYLINE_TOOL_ID).unwrap().center();
    cx.simulate_click(polyline_tool, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Polyline),
        "the real Polyline button must select the retained application tool state",
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the Polyline tool redraw must restore the annotation layer bounds");
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let page_origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project = |x: f32, y: f32| {
        point(
            page_origin.x + px(x * scale),
            page_origin.y + px((792. - y) * scale),
        )
    };
    for (index, point) in [
        project(120., 620.),
        project(276., 500.),
        project(428., 620.),
    ]
    .into_iter()
    .enumerate()
    {
        cx.simulate_mouse_down(point, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(point, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let draft = workspace.read_with(cx, |workspace, cx| {
            workspace.annotation_scene(request.document_id, 0, cx)
        });
        assert_eq!(
            draft.vertex_paths.first().map(|path| path.points.len()),
            Some(index + 1),
            "Polyline click {} must retain its PDF-space vertex",
            index + 1
        );
    }
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("escape");

    let after_polyline = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .expect("the Polyline click journey must create retained state");
    assert_eq!(after_polyline.vertex_paths.len(), 1);
    assert_eq!(
        after_polyline.vertex_paths[0].kind,
        VertexPathKind::Polyline
    );
    assert_eq!(after_polyline.vertex_paths[0].points().len(), 3);
    let polyline_id = after_polyline.vertex_paths[0].id.clone();
    assert_eq!(after_polyline.selected_id.as_ref(), Some(&polyline_id));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select),
        "Escape must commit a valid Polyline and restore the one-shot Select tool",
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_POLYGON_TOOL_ID);
    let polygon_tool = cx.debug_bounds(DOCUMENT_POLYGON_TOOL_ID).unwrap().center();
    cx.simulate_click(polygon_tool, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Polygon),
        "the real Polygon button must select the retained application tool state",
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the Polygon tool redraw must restore the annotation layer bounds");
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let page_origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project = |x: f32, y: f32| {
        point(
            page_origin.x + px(x * scale),
            page_origin.y + px((792. - y) * scale),
        )
    };
    for point in [
        project(150., 360.),
        project(336., 540.),
        project(456., 320.),
    ] {
        cx.simulate_mouse_down(point, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(point, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");

    let after_polygon = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .expect("the Polygon click journey must create retained state");
    assert_eq!(after_polygon.vertex_paths.len(), 2);
    assert_eq!(after_polygon.vertex_paths[0].id, polyline_id);
    assert_eq!(after_polygon.vertex_paths[1].kind, VertexPathKind::Polygon);
    assert_eq!(after_polygon.vertex_paths[1].points().len(), 3);
    assert!(after_polygon.dirty);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select),
        "Enter must commit a valid Polygon and restore the one-shot Select tool",
    );

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &polyline_id, cx)
    }));
    let moved_vertex = PdfPoint::new(192., 288.).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_vertex_path_point(request.document_id, 1, moved_vertex, cx)
        })
        .expect("the selected Polyline vertex must be editable by stable identity");
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.vertex_paths[0].points()[1], moved_vertex);
    assert_eq!(edited.vertex_paths[1], after_polygon.vertex_paths[1]);
    assert_eq!(
        edited.annotation_order,
        vec![polyline_id, edited.vertex_paths[1].id.clone()]
    );
}

#[gpui::test]
fn polylength_area_workspace_renders_real_tools_and_retains_independent_measurements(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let scale = PageScale::from_factors(
        0,
        ScaleSource::Custom,
        "2 ft = 72 pt",
        ScaleUnit::In,
        ScaleUnit::Ft,
        2. / 72.,
        2. / 72.,
        ScalePrecision::decimal(0.01).unwrap(),
    )
    .unwrap();
    let calibration = LengthCalibration::from_page_scale(&scale).unwrap();
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let scale = scale.clone();
        let calibration = calibration.clone();
        move |window, cx| {
            let workspace = cx.new(DocumentWorkspace::new);
            workspace_slot.replace(Some(workspace.clone()));
            let request = workspace.update(cx, |workspace, cx| {
                workspace.begin_open(PathBuf::from("measurement-paths.pdf"), cx)
            });
            workspace.update(cx, |workspace, cx| {
                workspace.apply_open_result(
                    &request,
                    Ok(opened_document(Arc::new(AtomicBool::new(false)))
                        .with_page_scales(vec![scale])
                        .with_page_length_calibrations(vec![(0, calibration)])),
                    cx,
                )
            });
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id =
        workspace.read_with(cx, |workspace, _| workspace.active_document_id().unwrap());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    for id in [DOCUMENT_POLYLENGTH_TOOL_ID, DOCUMENT_AREA_TOOL_ID] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "{id} must be a real GPUI Component Button with a stable rendered id",
        );
    }

    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let project = |layer: gpui::Bounds<gpui::Pixels>, x: f32, y: f32| {
        let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
        let origin = point(
            layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
            layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
        );
        point(origin.x + px(x * scale), origin.y + px((792. - y) * scale))
    };

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_POLYLENGTH_TOOL_ID);
    let polylength_button = cx
        .debug_bounds(DOCUMENT_POLYLENGTH_TOOL_ID)
        .unwrap()
        .center();
    cx.simulate_click(polylength_button, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Polylength),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds(layer_id).unwrap();
    let start = project(layer, 120., 620.);
    let end = project(layer, 192., 620.);
    cx.simulate_click(start, Modifiers::default());
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: end,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: end,
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    let after_polylength = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(after_polylength.measurement_paths.len(), 1);
    assert_eq!(
        after_polylength.measurement_paths[0].kind,
        MeasurementPathKind::Polylength,
    );
    assert_eq!(after_polylength.measurement_paths[0].caption(), "2.00 ft");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
        "a completed Polylength must restore the one-shot Select tool",
    );
    let polylength_id = after_polylength.measurement_paths[0].id.clone();

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_AREA_TOOL_ID);
    let area_button = cx.debug_bounds(DOCUMENT_AREA_TOOL_ID).unwrap().center();
    cx.simulate_click(area_button, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds(layer_id).unwrap();
    for (x, y) in [(120., 360.), (192., 360.), (192., 432.)] {
        cx.simulate_click(project(layer, x, y), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    let after_area = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(after_area.measurement_paths.len(), 2);
    assert_eq!(after_area.measurement_paths[0].id, polylength_id);
    assert_eq!(
        after_area.measurement_paths[1].kind,
        MeasurementPathKind::Area
    );
    assert_eq!(after_area.measurement_paths[1].caption(), "2.00 ft^2");
    assert!(after_area.vertex_paths.is_empty());

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &polylength_id, cx)
    }));
    let moved_vertex = PdfPoint::new(228., 620.).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_measurement_path_point(document_id, 1, moved_vertex, cx)
        })
        .expect("the selected Polylength vertex must be editable by stable identity");
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.measurement_paths[0].points()[1], moved_vertex);
    assert_eq!(edited.measurement_paths[1], after_area.measurement_paths[1]);

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_AREA_TOOL_ID);
    let area_button = cx.debug_bounds(DOCUMENT_AREA_TOOL_ID).unwrap().center();
    cx.simulate_click(area_button, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer = cx.debug_bounds(layer_id).unwrap();
    cx.simulate_click(project(layer, 300., 300.), Modifiers::default());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("escape");
    let cancelled = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(cancelled.measurement_paths.len(), 2);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
    );
}

#[gpui::test]
fn callout_workspace_renders_real_tool_and_retains_two_click_editor(cx: &mut TestAppContext) {
    const CALLOUT_TOOL_ID: &str = "tool-callout";

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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("callout.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    scroll_annotation_target_into_view(cx, &workspace, CALLOUT_TOOL_ID);
    let callout_button = cx
        .debug_bounds(CALLOUT_TOOL_ID)
        .expect("Callout must be a real GPUI Component Button with the frozen stable id");
    cx.simulate_click(callout_button.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Callout),
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));

    cx.simulate_click(project(72., 120.), Modifiers::default());
    cx.simulate_mouse_move(project(216., 192.), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let draft = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(draft.callouts.len(), 1);
    assert!(draft.callouts[0].draft);
    for (actual, expected) in draft.callouts[0].leader_points.iter().zip([
        PdfPoint::new(72., 120.).unwrap(),
        PdfPoint::new(144., 192.).unwrap(),
        PdfPoint::new(216., 192.).unwrap(),
    ]) {
        assert!((actual.x - expected.x).abs() <= 0.000_1);
        assert!((actual.y - expected.y).abs() <= 0.000_1);
    }
    let text_box = draft.callouts[0].text_box;
    for (actual, expected) in [
        (text_box.x, 216.),
        (text_box.y, 170.),
        (text_box.width, 150.),
        (text_box.height, 44.),
    ] {
        assert!((actual - expected).abs() <= 0.000_1);
    }
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .callouts
            .is_empty(),
        "the first click and hover must retain only a draft"
    );

    cx.simulate_click(project(216., 192.), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(created.callouts.len(), 1);
    assert_eq!(created.callouts[0].id.as_str(), "workspace:callout:1");
    assert_eq!(created.callouts[0].content(), "Callout");
    assert!(created.clouds.is_empty());
    assert!(created.text_boxes.is_empty());
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some("Callout".to_owned())
    );
    let editor_focus = workspace
        .read_with(cx, |workspace, cx| workspace.pending_text_box_focus(cx))
        .expect("the real Callout Textarea must own focus after creation");
    assert!(cx.update(|window, _| editor_focus.is_focused(window)));

    cx.simulate_keystrokes("n e w shift-enter l i n e enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.callouts.len(), 1);
    assert_eq!(edited.callouts[0].content(), "new\nline");
    assert_eq!(
        edited.undo_depth, 1,
        "create and initial text edit are one undo step"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        None
    );
}

#[gpui::test]
fn cloud_plus_workspace_renders_composite_and_opens_retained_text_editor(cx: &mut TestAppContext) {
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("cloud-plus.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_CLOUD_PLUS_TOOL_ID);
    let cloud_plus_button = cx
        .debug_bounds(DOCUMENT_CLOUD_PLUS_TOOL_ID)
        .expect("Cloud+ must be a real GPUI Component Button with the frozen stable id");
    cx.simulate_click(cloud_plus_button.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::CloudPlus),
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    for (x, y) in [(120., 420.), (300., 420.), (300., 600.)] {
        cx.simulate_click(project(x, y), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let draft = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(draft.cloud_pluses.len(), 1);
    assert!(draft.cloud_pluses[0].draft);
    assert!(draft.cloud_pluses[0].scallop_path.len() > draft.cloud_pluses[0].cloud_points.len());

    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(created.cloud_pluses.len(), 1);
    assert_eq!(
        created.cloud_pluses[0].id.as_str(),
        "workspace:cloud-plus:1"
    );
    assert_eq!(created.cloud_pluses[0].content(), "Cloud+");
    assert_eq!(created.cloud_pluses[0].text_box.width, 150.);
    assert_eq!(created.cloud_pluses[0].text_box.height, 44.);
    assert_eq!(created.cloud_pluses[0].leader_points().len(), 3);
    let initial_text_box = created.cloud_pluses[0].text_box;
    assert!(created.clouds.is_empty());
    assert!(created.callouts.is_empty());
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some("Cloud+".to_owned())
    );
    let editor_focus = workspace
        .read_with(cx, |workspace, cx| workspace.pending_text_box_focus(cx))
        .expect("the real Cloud+ Textarea must own focus after creation");
    assert!(cx.update(|window, _| editor_focus.is_focused(window)));

    cx.simulate_keystrokes(
        "o n e shift-enter t w o shift-enter t h r e e shift-enter f o u r enter",
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.cloud_pluses.len(), 1);
    assert_eq!(edited.cloud_pluses[0].content(), "one\ntwo\nthree\nfour");
    assert_eq!(edited.cloud_pluses[0].text_box.height, 67.2);
    assert_eq!(
        edited.cloud_pluses[0].text_box.y + edited.cloud_pluses[0].text_box.height * 0.5,
        initial_text_box.y + initial_text_box.height * 0.5,
        "multiline growth must preserve the composite text-box center"
    );
    assert_eq!(edited.cloud_pluses[0].leader_points().len(), 3);
    let connection = edited.cloud_pluses[0].leader_points()[2];
    assert_eq!(
        connection.y,
        edited.cloud_pluses[0].text_box.y + edited.cloud_pluses[0].text_box.height * 0.5,
        "multiline growth must reroute to the resized text-box edge"
    );
    assert_eq!(
        edited.undo_depth, 1,
        "Cloud+ creation and its initial text edit are one undo step"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        None
    );
}

#[gpui::test]
fn dimension_workspace_renders_real_component_tool_two_click_preview_and_caption_editor(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("dimension.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("l");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Line),
        "plain L must retain the frozen Line shortcut"
    );
    cx.simulate_keystrokes("shift-l");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Dimension),
        "Shift+L must select Dimension without replacing plain L"
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_DIMENSION_TOOL_ID);
    let dimension_button = cx
        .debug_bounds(DOCUMENT_DIMENSION_TOOL_ID)
        .expect("Dimension must render as a real GPUI Component Button with tool-dimension id");
    cx.simulate_click(dimension_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));

    cx.simulate_click(project(100., 300.), Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.dimension_placement_pending(request.document_id, cx)
    }));
    cx.simulate_mouse_move(project(260., 300.), None, Modifiers::default());
    let preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(preview.dimensions.len(), 1);
    assert_eq!(preview.dimensions[0].id.as_str(), "workspace:dimension:1");
    assert!(preview.dimensions[0].draft);
    assert_eq!(preview.dimensions[0].dimension_line_offset, 24.);

    cx.simulate_click(project(260., 300.), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(created.dimensions.len(), 1);
    assert_eq!(created.dimensions[0].id.as_str(), "workspace:dimension:1");
    assert_eq!(created.dimensions[0].content(), "Dimension");
    assert_eq!(created.dimensions[0].dimension_line_offset(), 24.);
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        Some("Dimension".to_owned())
    );
    let editor_focus = workspace
        .read_with(cx, |workspace, cx| workspace.pending_text_box_focus(cx))
        .expect("the real Dimension Textarea must own focus after creation");
    assert!(cx.update(|window, _| editor_focus.is_focused(window)));

    cx.simulate_keystrokes("d o o r space w i d t h enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.dimensions[0].content(), "door width");
    assert_eq!(
        edited.undo_depth, 1,
        "Dimension creation and initial caption edit must remain one undo step"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.pending_text_box_value(cx)),
        None
    );
}

#[gpui::test]
fn arc_workspace_renders_real_component_tool_three_click_preview_and_shift_snap(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("arc.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("shift-c");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Arc),
        "Shift+C must select Arc without replacing the plain-C Circle shortcut"
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ARC_TOOL_ID);
    let arc_button = cx
        .debug_bounds(DOCUMENT_ARC_TOOL_ID)
        .expect("Arc must render as a real GPUI Component Button with tool-arc id");
    cx.simulate_click(arc_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));

    cx.simulate_click(project(100., 300.), Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.arc_placement_pending(request.document_id, cx)
    }));
    cx.simulate_click(project(260., 300.), Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.arc_placement_pending(request.document_id, cx)
    }));

    let shifted = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    cx.simulate_mouse_move(project(180., 316.), None, shifted);
    let preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(preview.arcs.len(), 1);
    assert_eq!(preview.arcs[0].id.as_str(), "workspace:arc:1");
    assert!(preview.arcs[0].draft);
    assert_eq!(preview.arcs[0].sampled_path.len(), 65);
    assert!((preview.arcs[0].sweep_degrees().abs() - 90.).abs() < 0.000_01);

    cx.simulate_click(project(180., 316.), shifted);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(created.arcs.len(), 1);
    assert_eq!(created.arcs[0].id.as_str(), "workspace:arc:1");
    assert!((created.arcs[0].sweep_degrees().abs() - 90.).abs() < 0.000_01);
    assert_eq!(created.arcs[0].sampled_path(64).len(), 65);
    assert!(!workspace.read_with(cx, |workspace, cx| {
        workspace.arc_placement_pending(request.document_id, cx)
    }));
}

#[gpui::test]
fn redact_workspace_renders_real_component_tool_and_truthful_pending_overlay(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("redact.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_REDACT_TOOL_ID);
    let redact_button = cx
        .debug_bounds(DOCUMENT_REDACT_TOOL_ID)
        .expect("Redact must render as a real GPUI Component Button with tool-redact id");
    cx.simulate_click(redact_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Redact),
    );
    assert!(
        cx.debug_bounds(DOCUMENT_REDACT_PENDING_ALERT_ID).is_some(),
        "the real GPUI Component warning must remain visible while Redact is active",
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.redact_pending_status_text(cx)),
        Some(
            "Pending redaction mark — saving keeps the underlying PDF content; this mark does not securely remove text or graphics."
        ),
    );

    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    let start = project(144., 216.);
    let end = project(324., 288.);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(snapshot.redacts.len(), 1);
    assert_eq!(snapshot.redacts[0].id.as_str(), "workspace:redact:1");
    let expected_rect = PdfRect::new(144., 216., 180., 72.).unwrap();
    let actual_rect = snapshot.redacts[0].rect;
    assert!(
        (actual_rect.x - expected_rect.x).abs() < 0.001
            && (actual_rect.y - expected_rect.y).abs() < 0.001
            && (actual_rect.width - expected_rect.width).abs() < 0.001
            && (actual_rect.height - expected_rect.height).abs() < 0.001,
        "the rendered GPUI pointer round trip must preserve the literal PDF rectangle within 0.001 pt: expected={expected_rect:?}, actual={actual_rect:?}",
    );
    assert!(snapshot.dirty);
    assert_eq!((snapshot.undo_depth, snapshot.redo_depth), (1, 0));
    let page_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    let thumbnail_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.thumbnail_annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(page_scene.redacts.len(), 1);
    assert_eq!(thumbnail_scene.redacts.len(), 1);
    assert_eq!(page_scene.redacts[0].body_id, "redact.body");
    assert_eq!(page_scene.redacts[0].appearance.stroke_color(), "#ff0000");
    assert_eq!(
        page_scene.redacts[0].appearance.fill_color(),
        Some("#000000")
    );
    assert_eq!(page_scene.redacts[0].appearance.opacity(), 0.35);
    for id in [
        "redact.resize.nw",
        "redact.resize.n",
        "redact.resize.ne",
        "redact.resize.e",
        "redact.resize.se",
        "redact.resize.s",
        "redact.resize.sw",
        "redact.resize.w",
    ] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "selected Redact handle {id} must remain addressable"
        );
    }
}

#[gpui::test]
fn snapshot_workspace_renders_real_component_tool_and_two_click_base_raster_capture(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("snapshot.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_SNAPSHOT_TOOL_ID);
    let snapshot_button = cx
        .debug_bounds(DOCUMENT_SNAPSHOT_TOOL_ID)
        .expect("Snapshot must render as a real GPUI Component Button with tool-snapshot id");
    cx.simulate_click(snapshot_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Snapshot),
    );

    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));

    let first_corner = project(144., 216.);
    let second_corner = project(324., 288.);
    cx.simulate_click(first_corner, Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.snapshot_placement_pending(request.document_id, cx)
    }));
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(request.document_id, cx))
            .unwrap()
            .snapshots
            .is_empty()
    );

    cx.simulate_mouse_move(second_corner, None, Modifiers::default());
    let preview = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(preview.snapshots.len(), 1);
    assert!(preview.snapshots[0].draft);
    assert_eq!(preview.snapshots[0].body_id, "snapshot.body");

    cx.simulate_click(second_corner, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(snapshot.snapshots.len(), 1);
    assert_eq!(snapshot.snapshots[0].id.as_str(), "workspace:snapshot:1");
    assert_eq!(snapshot.snapshots[0].opacity(), 1.);
    assert_eq!(snapshot.snapshots[0].rotation_degrees(), 0.);
    assert!(
        snapshot.snapshots[0]
            .asset()
            .rgba()
            .iter()
            .all(|byte| *byte == 0xff)
    );
    assert!(snapshot.dirty);
    assert_eq!((snapshot.undo_depth, snapshot.redo_depth), (1, 0));
    assert!(!workspace.read_with(cx, |workspace, cx| {
        workspace.snapshot_placement_pending(request.document_id, cx)
    }));

    let page_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    let thumbnail_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.thumbnail_annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(
        page_scene.snapshots[0].asset_id,
        thumbnail_scene.snapshots[0].asset_id
    );
    for id in [
        "snapshot.body",
        "snapshot.resize.nw",
        "snapshot.resize.n",
        "snapshot.resize.ne",
        "snapshot.resize.e",
        "snapshot.resize.se",
        "snapshot.resize.s",
        "snapshot.resize.sw",
        "snapshot.resize.w",
        "snapshot.rotate",
    ] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "selected Snapshot part {id} must remain addressable"
        );
    }
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_snapshot_capture_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    let source_bytes = std::fs::read(&fixture).unwrap();
    let source_sha256 = format!("{:x}", Sha256::digest(&source_bytes));
    assert_eq!(
        source_sha256,
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
    );
    assert!(worker.is_file());
    assert!(library.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-snapshot-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-snapshot-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    let source_pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_091),
            generation: 1,
            path: fixture.clone(),
        })
        .expect("the public source PDF must open for the annotation-disabled pixel oracle");
    let source_page = source_pixel_proof.render_page(0, 320).unwrap();
    assert!(source_page.has_spatial_variation());
    let source_page_sha256 = Sha256::digest(source_page.pixels_bgra());
    let source_pixel_worker_pid = source_pixel_proof.worker_pid().unwrap();
    source_pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{source_pixel_worker_pid}")).exists());

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real fixture must own one live worker");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_SNAPSHOT_TOOL_ID);
    let snapshot_button = cx
        .debug_bounds(DOCUMENT_SNAPSHOT_TOOL_ID)
        .expect("the real GPUI Component Snapshot button must render");
    cx.simulate_click(snapshot_button.center(), Modifiers::default());
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };
    let create_start = to_view(72., 72.);
    let create_end = to_view(540., 720.);
    cx.simulate_click(create_start, Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.snapshot_placement_pending(document_id, cx)
    }));
    cx.simulate_mouse_move(create_end, None, Modifiers::default());
    cx.simulate_click(create_end, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("real Snapshot creation must update the retained session");
    assert_eq!(created.snapshots.len(), 1);
    let snapshot_id = created.snapshots[0].id.clone();
    assert_eq!(snapshot_id.as_str(), "workspace:snapshot:1");
    assert!(
        created.snapshots[0]
            .asset()
            .rgba()
            .iter()
            .any(|byte| *byte != 0)
    );
    let first_capture_pixel = &created.snapshots[0].asset().rgba()[0..4];
    assert!(
        created.snapshots[0]
            .asset()
            .rgba()
            .chunks_exact(4)
            .any(|pixel| pixel != first_capture_pixel)
    );
    assert_eq!(created.snapshots[0].opacity(), 1.);
    assert_eq!(created.snapshots[0].rotation_degrees(), 0.);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
    );

    let move_start = to_view(306., 396.);
    let move_end = to_view(318., 408.);
    cx.simulate_mouse_down(move_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(move_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(move_end, MouseButton::Left, Modifiers::default());
    let resize_start = to_view(552., 732.);
    let resize_end = to_view(564., 744.);
    cx.simulate_mouse_down(resize_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(resize_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(resize_end, MouseButton::Left, Modifiers::default());
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let edited_snapshot = edited.snapshots[0].clone();
    let expected_rect = PdfRect::new(84., 84., 480., 660.).unwrap();
    assert!(
        [
            (edited_snapshot.rect.x, expected_rect.x),
            (edited_snapshot.rect.y, expected_rect.y),
            (edited_snapshot.rect.width, expected_rect.width),
            (edited_snapshot.rect.height, expected_rect.height),
        ]
        .into_iter()
        .all(|(actual, expected)| (actual - expected).abs() < 0.001),
        "the real pointer move and resize must preserve the literal PDF edit",
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the Snapshot document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let save_failure = workspace.read_with(cx, |workspace, cx| {
        workspace
            .document_save_failure(document_id, cx)
            .map(|failure| failure.message)
    });
    assert!(!saved.dirty, "Save As failed: {save_failure:?}");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        source_sha256,
        "Save As must not modify the provenance-controlled source PDF",
    );
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path).unwrap();
    let persisted = independent
        .snapshots()
        .iter()
        .find(|snapshot| snapshot.id == snapshot_id)
        .expect("the stable Snapshot identity must survive Save As");
    assert_eq!(persisted.asset(), edited_snapshot.asset());
    assert!(independent.snapshot_has_canonical_native_identity(&snapshot_id));
    let saved_pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_092),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved Snapshot PDF must reopen for a PDFium pixel oracle");
    assert_eq!(
        Sha256::digest(saved_pixel_proof.render_page(0, 320).unwrap().pixels_bgra()),
        source_page_sha256,
        "a Snapshot annotation must not change the annotation-disabled page raster",
    );
    assert_ne!(
        Sha256::digest(
            saved_pixel_proof
                .render_page_with_pdf_annotations(0, 320)
                .unwrap()
                .pixels_bgra()
        ),
        source_page_sha256,
        "the moved and resized Snapshot must appear in annotation-enabled PDFium pixels",
    );
    let pixel_worker_pid = saved_pixel_proof.worker_pid().unwrap();
    saved_pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!(reopened.snapshots.len(), 1);
    assert_eq!(reopened.snapshots[0].id, snapshot_id);
    assert_eq!(reopened.snapshots[0].asset(), edited_snapshot.asset());
    assert!(!reopened.dirty);
    assert!(fresh_workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(reopened_document, &snapshot_id, cx)
    }));
    fresh_workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(reopened_document, cx)
        })
        .unwrap();
    fresh_workspace
        .update(cx, |workspace, cx| {
            workspace.save_path(reopened_document, cx)
        })
        .expect("deleting the Snapshot must begin an in-place save of the experiment copy");
    cx.run_until_parked();
    let final_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    let deleted = PdfPersistenceSession::open(&saved_path).unwrap();
    assert!(deleted.snapshots().is_empty());
    assert!(!deleted.has_raw_annotation_name(&snapshot_id));
    assert!(!deleted.has_canonical_raw_annotation_name(&snapshot_id));
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{final_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all real Snapshot sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_redact_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    let source_bytes = std::fs::read(&fixture).unwrap();
    let source_sha256 = format!("{:x}", Sha256::digest(&source_bytes));
    assert_eq!(
        source_sha256,
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b",
    );
    assert!(worker.is_file());
    assert!(library.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-redact-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-redact-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    let source_pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_081),
            generation: 1,
            path: fixture.clone(),
        })
        .expect("the public source PDF must open for the annotation-disabled pixel oracle");
    let source_page = source_pixel_proof.render_page(0, 320).unwrap();
    assert!(source_page.has_spatial_variation());
    let source_page_sha256 = Sha256::digest(source_page.pixels_bgra());
    let source_pixel_worker_pid = source_pixel_proof.worker_pid().unwrap();
    source_pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{source_pixel_worker_pid}")).exists());

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real fixture must own one live worker");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_REDACT_TOOL_ID);
    let redact_button = cx
        .debug_bounds(DOCUMENT_REDACT_TOOL_ID)
        .expect("the real GPUI Component Redact button must render");
    cx.simulate_click(redact_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_REDACT_PENDING_ALERT_ID).is_some());

    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };
    let create_start = to_view(144., 216.);
    let create_end = to_view(324., 288.);
    cx.simulate_mouse_down(create_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(create_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(create_end, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("real Redact creation must update the retained session");
    assert_eq!(created.redacts.len(), 1);
    let redact_id = created.redacts[0].id.clone();
    assert_eq!(redact_id.as_str(), "workspace:redact:1");

    let select_button = cx.debug_bounds(DOCUMENT_SELECT_TOOL_ID).unwrap();
    cx.simulate_click(select_button.center(), Modifiers::default());
    let move_start = to_view(234., 252.);
    let move_end = to_view(270., 276.);
    cx.simulate_mouse_down(move_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(move_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(move_end, MouseButton::Left, Modifiers::default());
    let resize_start = to_view(360., 312.);
    let resize_end = to_view(396., 336.);
    cx.simulate_mouse_down(resize_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(resize_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(resize_end, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let edited_redact = edited.redacts[0].clone();
    let expected_rect = PdfRect::new(180., 240., 216., 96.).unwrap();
    assert!(
        (edited_redact.rect.x - expected_rect.x).abs() < 0.001
            && (edited_redact.rect.y - expected_rect.y).abs() < 0.001
            && (edited_redact.rect.width - expected_rect.width).abs() < 0.001
            && (edited_redact.rect.height - expected_rect.height).abs() < 0.001,
        "the real pointer move and resize must preserve the literal PDF edit: expected={expected_rect:?}, actual={:?}",
        edited_redact.rect,
    );
    assert!(edited.dirty);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.redact_pending_status_text(cx)),
        Some(
            "Pending redaction mark — saving keeps the underlying PDF content; this mark does not securely remove text or graphics."
        ),
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the pending Redact document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let save_failure = workspace.read_with(cx, |workspace, cx| {
        workspace
            .document_save_failure(document_id, cx)
            .map(|failure| failure.message)
    });
    assert!(!saved.dirty, "Save As failed: {save_failure:?}");
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        source_sha256,
        "Save As must not modify the provenance-controlled source PDF",
    );
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path).unwrap();
    let persisted = independent
        .redacts()
        .iter()
        .find(|redact| redact.id == redact_id)
        .expect("the stable pending Redact identity must survive Save As");
    assert!(persisted.same_persisted_state_as(&edited_redact));
    assert!(independent.redact_has_canonical_native_identity(&redact_id));
    let saved_pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_082),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved pending Redact PDF must reopen for a PDFium pixel oracle");
    let saved_annotation_free_page = saved_pixel_proof.render_page(0, 320).unwrap();
    assert_eq!(
        Sha256::digest(saved_annotation_free_page.pixels_bgra()),
        source_page_sha256,
        "saving a pending Redact must leave the underlying page-content raster unchanged",
    );
    let saved_annotated_page = saved_pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .unwrap();
    assert!(saved_annotated_page.has_spatial_variation());
    let saved_pixel_worker_pid = saved_pixel_proof.worker_pid().unwrap();
    saved_pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{saved_pixel_worker_pid}")).exists());

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend.clone(), cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!(reopened.redacts.len(), 1);
    assert!(reopened.redacts[0].same_persisted_state_as(&edited_redact));
    assert!(!reopened.dirty);
    assert!(fresh_workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(reopened_document, &redact_id, cx)
    }));
    fresh_workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(reopened_document, cx)
        })
        .unwrap();
    fresh_workspace
        .update(cx, |workspace, cx| {
            workspace.save_path(reopened_document, cx)
        })
        .expect("deleting the pending mark must begin an in-place save of the experiment copy");
    cx.run_until_parked();
    let after_delete = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert!(after_delete.redacts.is_empty());
    assert!(!after_delete.dirty);
    let final_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();

    let deleted = PdfPersistenceSession::open(&saved_path).unwrap();
    assert!(deleted.redacts().is_empty());
    assert!(!deleted.has_raw_annotation_name(&redact_id));
    assert!(!deleted.has_canonical_raw_annotation_name(&redact_id));
    let deleted_pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_083),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved copy without the pending mark must reopen for a PDFium pixel oracle");
    assert_eq!(
        Sha256::digest(
            deleted_pixel_proof
                .render_page(0, 320)
                .unwrap()
                .pixels_bgra()
        ),
        source_page_sha256,
        "removing the pending mark must also leave the underlying page-content raster unchanged",
    );
    let deleted_pixel_worker_pid = deleted_pixel_proof.worker_pid().unwrap();
    deleted_pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{deleted_pixel_worker_pid}")).exists());
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{final_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all real pending-Redact sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
fn cloud_workspace_renders_real_component_tool_and_retains_scalloped_geometry(
    cx: &mut TestAppContext,
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("cloud.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_CLOUD_TOOL_ID);
    let cloud_button = cx
        .debug_bounds(DOCUMENT_CLOUD_TOOL_ID)
        .expect("Cloud must be a real GPUI Component Button with a stable rendered id");
    cx.simulate_click(cloud_button.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Cloud),
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(request.document_id, 0).into_boxed_str());
    let layer = cx.debug_bounds(layer_id).unwrap();
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let project =
        |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px((792. - y) * scale));
    for (x, y) in [(120., 420.), (300., 420.), (300., 600.)] {
        cx.simulate_click(project(x, y), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let draft = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(request.document_id, 0, cx)
    });
    assert_eq!(draft.clouds.len(), 1);
    assert!(draft.clouds[0].draft);
    assert!(draft.clouds[0].scallop_path.len() > draft.clouds[0].points.len());

    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(created.clouds.len(), 1);
    assert_eq!(created.clouds[0].border_effect_intensity(), 2.);
    assert_eq!(created.clouds[0].points().len(), 3);
    assert!(created.vertex_paths.is_empty());
    assert!(created.measurement_paths.is_empty());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Select),
    );

    let cloud_id = created.clouds[0].id.clone();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(request.document_id, &cloud_id, cx)
    }));
    let moved_vertex = PdfPoint::new(330., 620.).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_cloud_point(request.document_id, 1, moved_vertex, cx)
        })
        .unwrap();
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.clouds[0].id, cloud_id);
    assert_eq!(edited.clouds[0].points()[1], moved_vertex);
    assert!(edited.clouds[0].scallop_path().len() > edited.clouds[0].points().len());
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_polyline_polygon_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert!(
        worker.is_file(),
        "the exact PDF worker must already be built"
    );
    assert!(
        library.is_file(),
        "the checksum-reviewed development PDFium library must already exist"
    );
    assert!(
        fixture.is_file(),
        "the provenance-controlled fixture must exist"
    );

    let surface_root = manifest_dir
        .join(".prepared/real-vertex-path-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-vertex-path-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists(), "the Save As target must be new");
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real document must own a PDF worker");
    assert!(PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_POLYLINE_TOOL_ID);
    let polyline_tool = cx
        .debug_bounds(DOCUMENT_POLYLINE_TOOL_ID)
        .expect("the real GPUI Component Polyline button must render");
    cx.simulate_click(polyline_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live PDF annotation layer must render");
    let scale = (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let page_origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            page_origin.x + px(pdf_x as f32 * scale),
            page_origin.y + px((792. - pdf_y as f32) * scale),
        )
    };
    for vertex in [(96., 624.), (252., 516.), (432., 612.)] {
        let position = to_view(vertex.0, vertex.1);
        cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    let after_polyline = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(after_polyline.vertex_paths.len(), 1);
    assert_eq!(
        after_polyline.vertex_paths[0].kind,
        VertexPathKind::Polyline
    );
    assert_eq!(after_polyline.vertex_paths[0].points().len(), 3);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
        "Enter must commit the one-shot Polyline tool back to Select",
    );
    let polyline_id = after_polyline.vertex_paths[0].id.clone();

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_POLYGON_TOOL_ID);
    let polygon_tool = cx
        .debug_bounds(DOCUMENT_POLYGON_TOOL_ID)
        .expect("the real GPUI Component Polygon button must render");
    cx.simulate_click(polygon_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for vertex in [(132., 336.), (300., 492.), (468., 318.)] {
        let position = to_view(vertex.0, vertex.1);
        cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let close_position = to_view(136., 336.);
    cx.simulate_mouse_down(close_position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(close_position, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let after_polygon = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(after_polygon.vertex_paths.len(), 2);
    assert_eq!(after_polygon.vertex_paths[0].id, polyline_id);
    assert_eq!(after_polygon.vertex_paths[1].kind, VertexPathKind::Polygon);
    assert_eq!(after_polygon.vertex_paths[1].points().len(), 3);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
        "clicking the Polygon start point must close it and restore Select",
    );
    let polygon_before_edit = after_polygon.vertex_paths[1].clone();

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &polyline_id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let original_vertex = after_polygon.vertex_paths[0].points()[1];
    let moved_vertex = PdfPoint::new(original_vertex.x + 24., original_vertex.y - 18.).unwrap();
    let vertex_start = to_view(original_vertex.x, original_vertex.y);
    let vertex_end = to_view(moved_vertex.x, moved_vertex.y);
    cx.simulate_mouse_down(vertex_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(vertex_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(vertex_end, MouseButton::Left, Modifiers::default());
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.vertex_paths[0].id, polyline_id);
    assert!(
        (edited.vertex_paths[0].points()[1].x - moved_vertex.x).abs() <= 0.000_1
            && (edited.vertex_paths[0].points()[1].y - moved_vertex.y).abs() <= 0.000_1,
        "the viewport-to-PDF vertex edit must stay within 0.0001 PDF point",
    );
    assert_eq!(edited.vertex_paths[1], polygon_before_edit);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real vertex-path document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the validated Save As reopen must retain both vertex paths");
    let save_status = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .save_status()
            .clone()
    });
    assert!(
        !saved.dirty,
        "Save As must mark the current vertex-path revision clean: {save_status:?}",
    );
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path)
        .expect("the saved PDF must reopen through an independent typed parser");
    for expected in &edited.vertex_paths {
        let actual = independent
            .vertex_paths()
            .iter()
            .find(|path| path.id == expected.id)
            .expect("each stable vertex-path identity must survive Save As");
        assert!(actual.same_persisted_state_as(expected));
        assert!(independent.vertex_path_has_canonical_native_identity(&expected.id));
    }

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_073),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved PDF must reopen for an independent PDFium pixel oracle");
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .expect("the native PolyLine and Polygon annotations must render through PDFium");
    let annotation_free_page = pixel_proof
        .render_page(0, 320)
        .expect("the application base raster must remain annotation-free");
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "persisted vertex paths must change real PDFium annotation pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .expect("qpdf must be available for structural validation")
            .success()
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .expect("a distinct workspace must hydrate both saved vertex paths");
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert!(!reopened.dirty);
    assert!(reopened.selected_id.is_none());
    for expected in &edited.vertex_paths {
        let actual = reopened
            .vertex_paths
            .iter()
            .find(|path| path.id == expected.id)
            .expect("the fresh workspace must preserve stable vertex-path identity");
        assert!(actual.same_persisted_state_as(expected));
    }
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists()
            || std::fs::read_dir(&surface_root)
                .expect("the mapped-surface root must remain readable")
                .next()
                .is_none(),
        "all real vertex-path sessions must release their workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_polylength_area_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert!(
        worker.is_file(),
        "the exact PDF worker must already be built"
    );
    assert!(
        library.is_file(),
        "the checksum-reviewed development PDFium library must already exist"
    );
    assert!(
        fixture.is_file(),
        "the provenance-controlled fixture must exist"
    );

    let surface_root = manifest_dir
        .join(".prepared/real-measurement-path-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-measurement-path-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists(), "the Save As target must be new");
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real document must own a PDF worker");
    assert!(PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let scale = PageScale::from_factors(
        0,
        ScaleSource::Custom,
        "2 ft = 72 pt",
        ScaleUnit::In,
        ScaleUnit::Ft,
        2. / 72.,
        2. / 72.,
        ScalePrecision::decimal(0.01).unwrap(),
    )
    .unwrap();
    assert!(
        workspace
            .update(cx, |workspace, cx| workspace.apply_page_scale(
                document_id,
                scale.clone(),
                PageScaleApplyTarget::Current(0),
                cx,
            ))
            .expect("the real first page must accept a measurement scale")
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.page_scale(document_id, 0, cx)),
        Some(scale.clone()),
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_POLYLENGTH_TOOL_ID);
    let polylength_tool = cx
        .debug_bounds(DOCUMENT_POLYLENGTH_TOOL_ID)
        .expect("the real GPUI Component Polylength button must render");
    cx.simulate_click(polylength_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live PDF annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let page_origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            page_origin.x + px(pdf_x as f32 * render_scale),
            page_origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };

    for vertex in [(96., 624.), (168., 624.)] {
        cx.simulate_click(to_view(vertex.0, vertex.1), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let polylength_end = to_view(168., 552.);
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: polylength_end,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: polylength_end,
        modifiers: Modifiers::default(),
        click_count: 2,
    });
    let after_polylength = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the double-click journey must commit one real Polylength");
    assert_eq!(after_polylength.measurement_paths.len(), 1);
    assert_eq!(
        after_polylength.measurement_paths[0].kind,
        MeasurementPathKind::Polylength,
    );
    assert_eq!(after_polylength.measurement_paths[0].caption(), "4.00 ft");
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
        "double-click must commit Polylength and restore the one-shot Select tool",
    );
    let polylength_id = after_polylength.measurement_paths[0].id.clone();

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_AREA_TOOL_ID);
    let area_tool = cx
        .debug_bounds(DOCUMENT_AREA_TOOL_ID)
        .expect("the real GPUI Component Area button must render");
    cx.simulate_click(area_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for vertex in [(132., 336.), (204., 336.), (204., 408.)] {
        cx.simulate_click(to_view(vertex.0, vertex.1), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    let after_area = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the Enter journey must commit one real Area");
    assert_eq!(after_area.measurement_paths.len(), 2);
    assert_eq!(after_area.measurement_paths[0].id, polylength_id);
    assert_eq!(
        after_area.measurement_paths[1].kind,
        MeasurementPathKind::Area,
    );
    assert_eq!(after_area.measurement_paths[1].caption(), "2.00 ft^2");
    assert!(after_area.vertex_paths.is_empty());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
        "Enter must commit Area and restore the one-shot Select tool",
    );
    let area_before_edit = after_area.measurement_paths[1].clone();

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &polylength_id, cx)
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let original_vertex = after_area.measurement_paths[0].points()[1];
    let moved_vertex = PdfPoint::new(original_vertex.x + 24., original_vertex.y - 18.).unwrap();
    let vertex_start = to_view(original_vertex.x, original_vertex.y);
    let vertex_end = to_view(moved_vertex.x, moved_vertex.y);
    cx.simulate_mouse_down(vertex_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(vertex_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(vertex_end, MouseButton::Left, Modifiers::default());
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the real pointer edit must retain both measurement paths");
    assert_eq!(edited.measurement_paths[0].id, polylength_id);
    assert!(
        (edited.measurement_paths[0].points()[1].x - moved_vertex.x).abs() <= 0.000_1
            && (edited.measurement_paths[0].points()[1].y - moved_vertex.y).abs() <= 0.000_1,
        "the viewport-to-PDF measurement vertex edit must stay within 0.0001 PDF point",
    );
    assert_eq!(edited.measurement_paths[1], area_before_edit);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real measurement-path document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the validated Save As reopen must retain both measurement paths");
    let save_status = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .save_status()
            .clone()
    });
    assert!(
        !saved.dirty,
        "Save As must mark the current measurement-path revision clean: {save_status:?}",
    );
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path)
        .expect("the saved PDF must reopen through an independent typed parser");
    assert_eq!(independent.page_scales(), &[scale]);
    assert!(independent.vertex_paths().is_empty());
    for expected in &edited.measurement_paths {
        let actual = independent
            .measurement_paths()
            .iter()
            .find(|path| path.id == expected.id)
            .expect("each stable measurement-path identity must survive Save As");
        assert!(actual.same_persisted_state_as(expected));
        assert!(independent.measurement_path_has_canonical_native_identity(&expected.id));
    }

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_074),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved PDF must reopen for an independent PDFium pixel oracle");
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .expect("the native measurement annotations must render through PDFium");
    let annotation_free_page = pixel_proof
        .render_page(0, 320)
        .expect("the application base raster must remain annotation-free");
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "persisted measurement paths must change real PDFium annotation pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .expect("qpdf must be available for structural validation")
            .success()
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .expect("a distinct workspace must hydrate both saved measurement paths");
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert!(!reopened.dirty);
    assert!(reopened.selected_id.is_none());
    assert_eq!(
        fresh_workspace.read_with(cx, |workspace, cx| workspace.page_scale(
            reopened_document,
            0,
            cx,
        )),
        independent.page_scales().first().cloned(),
    );
    for expected in &edited.measurement_paths {
        let actual = reopened
            .measurement_paths
            .iter()
            .find(|path| path.id == expected.id)
            .expect("the fresh workspace must preserve stable measurement-path identity");
        assert!(actual.same_persisted_state_as(expected));
    }
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists()
            || std::fs::read_dir(&surface_root)
                .expect("the mapped-surface root must remain readable")
                .next()
                .is_none(),
        "all real measurement-path sessions must release their workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_cloud_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(fixture.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-cloud-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-cloud-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert!(PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_CLOUD_TOOL_ID);
    let cloud_button = cx
        .debug_bounds(DOCUMENT_CLOUD_TOOL_ID)
        .expect("the real GPUI Component Cloud button must render");
    cx.simulate_click(cloud_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live PDF annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };
    for vertex in [(120., 360.), (264., 360.), (264., 504.), (120., 504.)] {
        cx.simulate_click(to_view(vertex.0, vertex.1), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("Enter must commit one real Cloud");
    assert_eq!(created.clouds.len(), 1);
    assert_eq!(created.clouds[0].border_effect_intensity(), 2.);
    assert!(created.clouds[0].scallop_path().len() > created.clouds[0].points().len());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select),
    );

    let cloud_id = created.clouds[0].id.clone();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &cloud_id, cx)
    }));
    let moved_vertex = PdfPoint::new(288., 378.).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_cloud_point(document_id, 1, moved_vertex, cx)
        })
        .unwrap();
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.clouds[0].id, cloud_id);
    assert_eq!(edited.clouds[0].points()[1], moved_vertex);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real Cloud document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(!saved.dirty);
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path).unwrap();
    let persisted = independent
        .clouds()
        .iter()
        .find(|cloud| cloud.id == cloud_id)
        .expect("the stable Cloud identity must survive Save As");
    assert!(persisted.same_persisted_state_as(&edited.clouds[0]));
    assert!(independent.cloud_has_canonical_native_identity(&cloud_id));

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_075),
            generation: 1,
            path: saved_path.clone(),
        })
        .unwrap();
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .unwrap();
    let annotation_free_page = pixel_proof.render_page(0, 320).unwrap();
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "the persisted Cloud must change real PDFium annotation pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .unwrap()
            .success()
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert!(!reopened.dirty);
    assert!(reopened.selected_id.is_none());
    let fresh_cloud = reopened
        .clouds
        .iter()
        .find(|cloud| cloud.id == cloud_id)
        .unwrap();
    assert!(fresh_cloud.same_persisted_state_as(&edited.clouds[0]));
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all real Cloud sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_callout_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(fixture.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-callout-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-callout-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real fixture must own one live worker");
    assert!(PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_CALLOUT_TOOL_ID);
    let callout_button = cx
        .debug_bounds(DOCUMENT_CALLOUT_TOOL_ID)
        .expect("the real GPUI Component Callout button must render");
    cx.simulate_click(callout_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live PDF annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };
    cx.simulate_click(to_view(108., 312.), Modifiers::default());
    cx.simulate_mouse_move(to_view(252., 408.), None, Modifiers::default());
    cx.simulate_click(to_view(252., 408.), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    cx.simulate_keystrokes("f i e l d shift-enter n o t e enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the real Callout creation must update the retained session");
    assert_eq!(created.callouts.len(), 1);
    assert_eq!(created.callouts[0].content(), "field\nnote");
    assert!(created.clouds.is_empty());
    assert!(created.text_boxes.is_empty());
    let callout_id = created.callouts[0].id.clone();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &callout_id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.translate_selected_callout_text_box(document_id, 18., -12., cx)
        })
        .expect("the retained Callout text box must move independently");
    let moved_knee = PdfPoint::new(174., 426.).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_callout_leader_point(document_id, 1, moved_knee, cx)
        })
        .expect("the retained Callout knee must edit by stable identity");
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(edited.callouts[0].id, callout_id);
    assert_eq!(edited.callouts[0].leader_points()[1], moved_knee);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real Callout document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(!saved.dirty);
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path)
        .expect("the saved PDF must reopen through the typed parser");
    let persisted = independent
        .callouts()
        .iter()
        .find(|callout| callout.id == callout_id)
        .expect("the stable Callout identity must survive Save As");
    assert!(persisted.same_persisted_state_as(&edited.callouts[0]));
    assert!(independent.callout_has_canonical_native_identity(&callout_id));
    assert!(independent.clouds().is_empty());
    assert!(independent.text_boxes().is_empty());

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_076),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved PDF must reopen for a PDFium pixel oracle");
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .expect("the native Callout must render through PDFium");
    let annotation_free_page = pixel_proof
        .render_page(0, 320)
        .expect("the application base raster must remain annotation-free");
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "the persisted Callout must change real PDFium annotation pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .unwrap()
            .success()
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert!(!reopened.dirty);
    assert!(reopened.selected_id.is_none());
    let fresh_callout = reopened
        .callouts
        .iter()
        .find(|callout| callout.id == callout_id)
        .expect("a fresh workspace must reopen the logical Callout");
    assert!(fresh_callout.same_persisted_state_as(&edited.callouts[0]));
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all real Callout sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_cloud_plus_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b"
    );
    assert!(worker.is_file());
    assert!(library.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-cloud-plus-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-cloud-plus-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real fixture must own one live worker");
    assert!(PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_CLOUD_PLUS_TOOL_ID);
    let cloud_plus_button = cx
        .debug_bounds(DOCUMENT_CLOUD_PLUS_TOOL_ID)
        .expect("the real GPUI Component Cloud+ button must render");
    cx.simulate_click(cloud_plus_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live PDF annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };
    for vertex in [(108., 312.), (252., 312.), (252., 456.), (108., 456.)] {
        cx.simulate_click(to_view(vertex.0, vertex.1), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    let workspace_focus = workspace.read_with(cx, |workspace, _| workspace.focus_handle());
    cx.update(|window, cx| workspace_focus.focus(window, cx));
    cx.simulate_keystrokes("enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    cx.simulate_keystrokes(
        "f i e l d shift-enter n o t e shift-enter l e v e l shift-enter f o u r enter",
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the real Cloud+ creation must update the retained session");
    assert_eq!(created.cloud_pluses.len(), 1);
    assert_eq!(
        created.cloud_pluses[0].content(),
        "field\nnote\nlevel\nfour"
    );
    assert_eq!(created.cloud_pluses[0].text_box.height, 67.2);
    assert_eq!(created.cloud_pluses[0].leader_points().len(), 3);
    assert!(created.clouds.is_empty());
    assert!(created.callouts.is_empty());
    assert!(created.text_boxes.is_empty());
    let cloud_plus_id = created.cloud_pluses[0].id.clone();

    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &cloud_plus_id, cx)
    }));
    let moved_vertex = PdfPoint::new(270., 330.).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_cloud_plus_cloud_point(document_id, 1, moved_vertex, cx)
        })
        .expect("the retained Cloud+ vertex must edit by stable identity");
    workspace
        .update(cx, |workspace, cx| {
            workspace.translate_selected_cloud_plus_text_box(document_id, 18., -12., cx)
        })
        .expect("the retained Cloud+ text box must move and reroute its leader");
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let edited_cloud_plus = &edited.cloud_pluses[0];
    assert_eq!(edited_cloud_plus.id, cloud_plus_id);
    assert_eq!(edited_cloud_plus.cloud_points()[1], moved_vertex);
    assert_eq!(edited_cloud_plus.leader_points().len(), 3);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real Cloud+ document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let save_status = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .save_status()
            .clone()
    });
    assert!(
        !saved.dirty,
        "Cloud+ Save As must mark the accepted revision clean: {save_status:?}"
    );
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path)
        .expect("the saved PDF must reopen through the typed parser");
    assert_eq!(independent.cloud_pluses().len(), 1);
    let persisted = independent
        .cloud_pluses()
        .iter()
        .find(|cloud_plus| cloud_plus.id == cloud_plus_id)
        .expect("the stable Cloud+ identity must survive Save As");
    assert!(persisted.same_persisted_state_as(edited_cloud_plus));
    assert!(independent.cloud_plus_has_canonical_native_identity(&cloud_plus_id));
    assert!(independent.clouds().is_empty());
    assert!(independent.callouts().is_empty());
    assert!(independent.text_boxes().is_empty());

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_077),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved PDF must reopen for a PDFium pixel oracle");
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .expect("the paired native Cloud+ must render through PDFium");
    let annotation_free_page = pixel_proof
        .render_page(0, 320)
        .expect("the application base raster must remain annotation-free");
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "the persisted Cloud+ pair must change real PDFium annotation pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .unwrap()
            .success()
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert!(!reopened.dirty);
    assert!(reopened.selected_id.is_none());
    assert_eq!(reopened.cloud_pluses.len(), 1);
    assert!(reopened.clouds.is_empty());
    assert!(reopened.callouts.is_empty());
    assert!(reopened.text_boxes.is_empty());
    let fresh_cloud_plus = reopened
        .cloud_pluses
        .iter()
        .find(|cloud_plus| cloud_plus.id == cloud_plus_id)
        .expect("a fresh workspace must reopen one logical Cloud+");
    assert!(fresh_cloud_plus.same_persisted_state_as(edited_cloud_plus));
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all real Cloud+ sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_semantic_snapping_line_and_length_save_close_and_fresh_workspace_reopen(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b"
    );
    assert!(worker.is_file());
    assert!(library.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-semantic-snapping-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-semantic-snapping-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let opened_evidence = workspace
        .read_with(cx, |workspace, cx| workspace.evidence_snapshot(document_id, cx))
        .expect("the real fixture must expose document evidence");
    assert!(opened_evidence.ready);
    assert_eq!(opened_evidence.page_count, 100);
    assert!(opened_evidence.current_raster_has_spatial_variation);
    let original_worker_pid = opened_evidence
        .worker_pid
        .expect("the real fixture must own one live worker");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the real annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_RECTANGLE_TOOL_ID);
    let rectangle_button = cx
        .debug_bounds(DOCUMENT_RECTANGLE_TOOL_ID)
        .expect("the real GPUI Component Rectangle button must render");
    cx.simulate_click(rectangle_button.center(), Modifiers::default());
    let rectangle_start = to_view(100., 100.);
    let rectangle_end = to_view(200., 200.);
    cx.simulate_mouse_down(rectangle_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        rectangle_end,
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(rectangle_end, MouseButton::Left, Modifiers::default());
    let reference = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(reference.rectangles.len(), 1);
    let reference_rect = reference.rectangles[0].rect;
    for (actual, expected) in [
        (reference_rect.x, 100.),
        (reference_rect.y, 100.),
        (reference_rect.width, 100.),
        (reference_rect.height, 100.),
    ] {
        assert!((actual - expected).abs() <= 0.000_1);
    }
    let expected_line_start = PdfPoint::new(reference_rect.x, reference_rect.y).unwrap();
    let expected_line_end = PdfPoint::new(
        reference_rect.x + reference_rect.width * 0.5,
        reference_rect.y + reference_rect.height,
    )
    .unwrap();
    let reference_id = reference.rectangles[0].id.clone();
    assert_eq!((reference.revision, reference.undo_depth), (1, 1));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_LINE_TOOL_ID);
    let line_button = cx
        .debug_bounds(DOCUMENT_LINE_TOOL_ID)
        .expect("the real GPUI Component Line button must render");
    cx.simulate_click(line_button.center(), Modifiers::default());
    cx.simulate_mouse_down(
        to_view(102., 100.),
        MouseButton::Left,
        Modifiers::default(),
    );
    cx.simulate_mouse_move(
        to_view(150., 198.),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    let line_decision = workspace
        .read_with(cx, |workspace, cx| {
            workspace.semantic_snap_decision(document_id, cx)
        })
        .expect("the real Line gesture must resolve against retained markup");
    assert_eq!(line_decision.point, expected_line_end);
    assert_eq!(line_decision.owner_id.as_ref(), Some(&reference_id));
    assert_eq!(line_decision.role, SemanticSnapRole::Midpoint);
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(document_id, cx)
            })
            .unwrap()
            .revision,
        1,
        "the real Line preview must not create history",
    );
    cx.simulate_mouse_up(
        to_view(150., 198.),
        MouseButton::Left,
        Modifiers::default(),
    );
    let after_line = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!((after_line.revision, after_line.undo_depth), (2, 2));
    let snapped_line = after_line.straight_lines[0].clone();
    assert_eq!(snapped_line.start, expected_line_start);
    assert_eq!(snapped_line.end, expected_line_end);

    workspace
        .update(cx, |workspace, cx| {
            workspace.set_length_calibration(
                document_id,
                LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap(),
                cx,
            )
        })
        .expect("the real document must retain one page calibration");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_LENGTH_TOOL_ID);
    let length_button = cx
        .debug_bounds(DOCUMENT_LENGTH_TOOL_ID)
        .expect("the real GPUI Component Length button must render");
    cx.simulate_click(length_button.center(), Modifiers::default());
    cx.simulate_click(to_view(127., 150.), Modifiers::default());
    cx.simulate_mouse_move(to_view(148., 198.), None, Modifiers::default());
    let length_decision = workspace
        .read_with(cx, |workspace, cx| {
            workspace.semantic_snap_decision(document_id, cx)
        })
        .expect("the real Length hover must use the same semantic engine");
    assert_eq!(length_decision.point, snapped_line.end);
    assert_eq!(length_decision.owner_id.as_ref(), Some(&snapped_line.id));
    assert_eq!(length_decision.role, SemanticSnapRole::Endpoint);
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.annotation_snapshot(document_id, cx)
            })
            .unwrap()
            .revision,
        3,
        "the Length preview must not add a revision after page-scale setup",
    );
    cx.simulate_click(to_view(148., 198.), Modifiers::default());
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!((edited.revision, edited.undo_depth), (4, 4));
    assert_eq!(edited.lengths.len(), 1);
    let snapped_length = edited.lengths[0].clone();
    assert_eq!(
        snapped_length.start,
        PdfPoint::new(
            (snapped_line.start.x + snapped_line.end.x) * 0.5,
            (snapped_line.start.y + snapped_line.end.y) * 0.5,
        )
        .unwrap(),
    );
    assert_eq!(snapped_length.end, snapped_line.end);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the snapped real document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let semantic_snap_save_status = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .save_status()
            .clone()
    });
    assert!(
        !saved.dirty,
        "semantic-snapping Save As must accept the current revision: {semantic_snap_save_status:?}",
    );
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("validated Save As must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path)
        .expect("the snapped PDF must reopen through the typed parser");
    let persisted_line = independent
        .straight_lines()
        .iter()
        .find(|line| line.id == snapped_line.id)
        .expect("the snapped Line stable identity must survive Save As");
    assert!(persisted_line.same_persisted_state_as(&snapped_line));
    assert!(independent.straight_line_has_canonical_native_identity(&snapped_line.id));
    let persisted_length = independent
        .lengths()
        .iter()
        .find(|length| length.id == snapped_length.id)
        .expect("the snapped Length stable identity must survive Save As");
    assert!(persisted_length.same_persisted_state_as(&snapped_length));

    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_081),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the snapped PDF must reopen for a PDFium pixel oracle");
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .expect("the snapped native annotations must render through PDFium");
    let annotation_free_page = pixel_proof.render_page(0, 320).unwrap();
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "the persisted snapped annotations must change real PDFium pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&saved_path)
            .status()
            .expect("pdfinfo must be available for independent reopen validation")
            .success()
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert!(!reopened.dirty);
    assert!(
        reopened
            .straight_lines
            .iter()
            .any(|line| line.same_persisted_state_as(&snapped_line))
    );
    assert!(
        reopened
            .lengths
            .iter()
            .any(|length| length.same_persisted_state_as(&snapped_length)),
        "fresh workspace Length mismatch: expected {snapped_length:?}, reopened {:?}",
        reopened.lengths,
    );
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all semantic-snapping sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_dimension_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b"
    );
    assert!(worker.is_file());
    assert!(library.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-dimension-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-dimension-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real fixture must own one live worker");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_DIMENSION_TOOL_ID);
    let dimension_button = cx
        .debug_bounds(DOCUMENT_DIMENSION_TOOL_ID)
        .expect("the real GPUI Component Dimension button must render");
    cx.simulate_click(dimension_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };
    cx.simulate_click(to_view(108., 312.), Modifiers::default());
    cx.simulate_mouse_move(to_view(288., 312.), None, Modifiers::default());
    cx.simulate_click(to_view(288., 312.), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_TEXT_BOX_EDITOR_ID).is_some());
    cx.simulate_keystrokes("d o o r space c l e a r space w i d t h enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the real Dimension creation must update the retained session");
    assert_eq!(created.dimensions.len(), 1);
    assert_eq!(created.dimensions[0].content(), "door clear width");
    let dimension_id = created.dimensions[0].id.clone();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &dimension_id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_dimension_offset(document_id, 40., cx)
        })
        .expect("the retained Dimension offset must edit by stable identity");
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let edited_dimension = edited.dimensions[0].clone();
    assert_eq!(edited_dimension.dimension_line_offset(), 40.);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real Dimension document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(!saved.dirty);
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path).unwrap();
    let persisted = independent
        .dimensions()
        .iter()
        .find(|dimension| dimension.id == dimension_id)
        .expect("the stable Dimension identity must survive Save As");
    assert!(persisted.same_persisted_state_as(&edited_dimension));
    assert!(independent.dimension_has_canonical_native_identity(&dimension_id));
    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_078),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved Dimension PDF must reopen for a PDFium pixel oracle");
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .unwrap();
    let annotation_free_page = pixel_proof.render_page(0, 320).unwrap();
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "the persisted Dimension must change real PDFium annotation pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!(reopened.dimensions.len(), 1);
    assert!(reopened.dimensions[0].same_persisted_state_as(&edited_dimension));
    assert!(!reopened.dirty);
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all real Dimension sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_arc_edit_save_close_and_fresh_workspace_reopen(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b"
    );
    assert!(worker.is_file());
    assert!(library.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-arc-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-arc-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists());
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, _| window.activate_window());
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the real fixture must own one live worker");

    cx.update(|window, cx| window.draw(cx).clear(cx));
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_ARC_TOOL_ID);
    let arc_button = cx
        .debug_bounds(DOCUMENT_ARC_TOOL_ID)
        .expect("the real GPUI Component Arc button must render");
    cx.simulate_click(arc_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer = cx
        .debug_bounds(layer_id)
        .expect("the live annotation layer must render");
    let render_scale =
        (f32::from(layer.size.width) / 612.).min(f32::from(layer.size.height) / 792.);
    let origin = point(
        layer.origin.x + px((f32::from(layer.size.width) - 612. * render_scale) / 2.),
        layer.origin.y + px((f32::from(layer.size.height) - 792. * render_scale) / 2.),
    );
    let to_view = |pdf_x: f64, pdf_y: f64| {
        point(
            origin.x + px(pdf_x as f32 * render_scale),
            origin.y + px((792. - pdf_y as f32) * render_scale),
        )
    };
    cx.simulate_click(to_view(108., 312.), Modifiers::default());
    cx.simulate_click(to_view(288., 312.), Modifiers::default());
    cx.simulate_mouse_move(to_view(198., 348.), None, Modifiers::default());
    cx.simulate_click(to_view(198., 348.), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the real Arc creation must update the retained session");
    assert_eq!(created.arcs.len(), 1);
    let arc_id = created.arcs[0].id.clone();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &arc_id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_arc_control_point(
                document_id,
                ArcControlPoint::Mid,
                PdfPoint::new(198., 372.).unwrap(),
                false,
                cx,
            )
        })
        .expect("the retained Arc midpoint must edit by stable identity");
    let edited = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    let edited_arc = edited.arcs[0].clone();
    assert!((edited_arc.mid.x - 198.).abs() < 0.000_01);
    assert!((edited_arc.mid.y - 372.).abs() < 0.000_01);
    assert!(edited.dirty);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real Arc document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(!saved.dirty);
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independent = PdfPersistenceSession::open(&saved_path).unwrap();
    let persisted = independent
        .arcs()
        .iter()
        .find(|arc| arc.id == arc_id)
        .expect("the stable Arc identity must survive Save As");
    assert!(persisted.same_persisted_state_as(&edited_arc));
    assert!(independent.arc_has_canonical_native_identity(&arc_id));
    let pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_079),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved Arc PDF must reopen for a PDFium pixel oracle");
    let annotated_page = pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .unwrap();
    let annotation_free_page = pixel_proof.render_page(0, 320).unwrap();
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "the persisted Arc must change real PDFium annotation pixels",
    );
    let pixel_worker_pid = pixel_proof.worker_pid().unwrap();
    pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_worker_pid}")).exists());

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .unwrap();
    assert_eq!(reopened.arcs.len(), 1);
    assert!(reopened.arcs[0].same_persisted_state_as(&edited_arc));
    assert!(!reopened.dirty);
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "all real Arc sessions must release workers and mapped surfaces",
    );
}

#[gpui::test]
fn visible_toolbar_close_routes_a_dirty_document_through_confirmation(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("toolbar-close.pdf"), cx)
    });
    let released = Arc::new(AtomicBool::new(false));
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(&request, Ok(opened_document(released.clone())), cx)
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                request.document_id,
                0,
                MarkupId::new("workspace:rectangle:toolbar-close").unwrap(),
                PdfPoint::new(20., 20.).unwrap(),
                PdfPoint::new(80., 80.).unwrap(),
                cx,
            )
        })
        .unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let close = cx
        .debug_bounds(DOCUMENT_CLOSE_ID)
        .expect("the live document toolbar must expose its Close action");
    cx.simulate_click(close.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(request.document_id, cx).is_some()
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.pending_close_document_id()),
        Some(request.document_id)
    );
    assert!(cx.debug_bounds(DOCUMENT_DIRTY_CLOSE_ID).is_some());
    assert!(!released.load(Ordering::Acquire));
}

#[gpui::test]
fn two_documents_switch_and_dirty_close_targets_only_the_stable_session(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
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
    let first_request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("first.pdf"), cx)
    });
    let first_released = Arc::new(AtomicBool::new(false));
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &first_request,
            Ok(opened_document(first_released.clone())),
            cx,
        )
    });
    let second_request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("second.pdf"), cx)
    });
    let second_released = Arc::new(AtomicBool::new(false));
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &second_request,
            Ok(opened_document(second_released.clone())),
            cx,
        )
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(second_request.document_id)
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_SESSION_TABS_ID).is_some());
    assert!(
        cx.debug_bounds("document-workspace-native-session-tab-bar")
            .is_some(),
        "the runnable document journey must use the real GPUI Component TabBar"
    );
    assert!(cx.debug_bounds("document-1-session-tab").is_some());
    assert!(cx.debug_bounds("document-2-session-tab").is_some());
    let first_tab_center = cx.debug_bounds("document-1-session-tab").unwrap().center();
    cx.simulate_click(first_tab_center, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(first_request.document_id)
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                first_request.document_id,
                0,
                MarkupId::new("workspace:rectangle:protected-close").unwrap(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(72., 96.).unwrap(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first_close_center = cx
        .debug_bounds("document-1-session-close")
        .unwrap()
        .center();
    cx.simulate_click(first_close_center, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_DIRTY_CLOSE_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_DIRTY_CLOSE_CANCEL_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_DIRTY_CLOSE_DISCARD_ID).is_some());
    assert!(cx.debug_bounds(DOCUMENT_DIRTY_CLOSE_SAVE_ID).is_some());
    let cancel_center = cx
        .debug_bounds(DOCUMENT_DIRTY_CLOSE_CANCEL_ID)
        .unwrap()
        .center();
    cx.simulate_click(cancel_center, Modifiers::default());
    assert!(!first_released.load(Ordering::Acquire));
    assert!(!second_released.load(Ordering::Acquire));

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(first_request.document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired
    );
    let failed_save = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_dirty_close_save_as(workspace_save_target("failed-close-save.pdf"), cx)
        })
        .expect("Save must target the pending stable dirty document");
    workspace.update(cx, |workspace, cx| {
        workspace.apply_save_result(&failed_save, Err("injected close-save failure".into()), cx)
    });
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(first_request.document_id, cx).is_some()
    }));
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(first_request.document_id, cx)
            .unwrap()
            .dirty
    }));

    let successful_save = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_dirty_close_save_as(workspace_save_target("saved-close.pdf"), cx)
        })
        .expect("a failed close-save must remain retryable");
    let reopened_released = Arc::new(AtomicBool::new(false));
    workspace.update(cx, |workspace, cx| {
        workspace.apply_save_result(
            &successful_save,
            Ok(SavedNativeDocument::new(
                opened_document(reopened_released.clone()),
                successful_save.annotation_revision,
            )),
            cx,
        )
    });
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(first_request.document_id, cx).is_none()
    }));
    assert!(first_released.load(Ordering::Acquire));
    assert!(reopened_released.load(Ordering::Acquire));
    assert!(!second_released.load(Ordering::Acquire));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(second_request.document_id)
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                second_request.document_id,
                0,
                MarkupId::new("workspace:rectangle:discard-close").unwrap(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(48., 48.).unwrap(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(96., 96.).unwrap(),
                cx,
            )
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(second_request.document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        DirtyCloseResolution::Discarded
    );
    assert!(second_released.load(Ordering::Acquire));
}

#[gpui::test]
fn real_session_tab_strip_preserves_document_identity_through_selection_clean_dirty_close_and_keyboard_pointer_reorder(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let mut requests = Vec::new();
    let mut released = Vec::new();
    for name in ["first.pdf", "second.pdf", "third.pdf"] {
        let request = workspace.update(cx, |workspace, cx| {
            workspace.begin_open(PathBuf::from(name), cx)
        });
        let release = Arc::new(AtomicBool::new(false));
        workspace.update(cx, |workspace, cx| {
            workspace.apply_open_result(&request, Ok(opened_document(release.clone())), cx)
        });
        requests.push(request);
        released.push(release);
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds(DOCUMENT_TAB_REORDER_STATUS_ID).is_some());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        requests
            .iter()
            .map(|request| request.document_id)
            .collect::<Vec<_>>()
    );

    let second_close = cx
        .debug_bounds(Box::leak(
            document_session_close_id(requests[1].document_id).into_boxed_str(),
        ))
        .unwrap();
    let third_before_close_drag = cx
        .debug_bounds(Box::leak(
            document_session_tab_id(requests[2].document_id).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_mouse_move(second_close.center(), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: second_close.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(
        third_before_close_drag.center(),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: third_before_close_drag.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        requests
            .iter()
            .map(|request| request.document_id)
            .collect::<Vec<_>>(),
        "a pointer gesture that starts on Close must never reorder a tab"
    );

    let first_tab = cx
        .debug_bounds(Box::leak(
            document_session_tab_id(requests[0].document_id).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(first_tab.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(requests[0].document_id)
    );

    cx.simulate_keystrokes("alt-shift-right");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        [
            requests[1].document_id,
            requests[0].document_id,
            requests[2].document_id,
        ]
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace
            .session_tab_reorder_announcement()
            .to_owned()),
        "Moved bp-multi-page-v1.pdf to position 2 of 3."
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let source_id = document_session_tab_id(requests[0].document_id);
    let target_id = document_session_tab_id(requests[2].document_id);
    let source = cx
        .debug_bounds(Box::leak(source_id.into_boxed_str()))
        .unwrap();
    let target = cx
        .debug_bounds(Box::leak(target_id.into_boxed_str()))
        .unwrap();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: source.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(
        point(source.center().x + px(6.), source.center().y),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: point(source.center().x + px(6.), source.center().y),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        [
            requests[1].document_id,
            requests[0].document_id,
            requests[2].document_id,
        ],
        "the Electron distance sensor activates only after six pixels"
    );

    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: source.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_mouse_move(
        point(
            source.center().x + px(DOCUMENT_TAB_POINTER_DRAG_THRESHOLD as f32 + 1.),
            source.center().y,
        ),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(Box::leak(
            document_tab_drag_id(&requests[0].document_id.to_string()).into_boxed_str(),
        ))
        .is_some()
    );
    cx.simulate_mouse_move(
        target.center(),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(Box::leak(
            document_tab_drop_target_id(&requests[2].document_id.to_string()).into_boxed_str(),
        ))
        .is_some()
    );
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: target.center(),
        modifiers: Modifiers::default(),
        click_count: 1,
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.session_order(cx)),
        [
            requests[1].document_id,
            requests[2].document_id,
            requests[0].document_id,
        ]
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                requests[1].document_id,
                0,
                MarkupId::new("workspace:rectangle:tab-identity").unwrap(),
                PdfPoint::new(20., 20.).unwrap(),
                PdfPoint::new(80., 80.).unwrap(),
                cx,
            )
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let dirty_close = cx
        .debug_bounds(Box::leak(
            document_session_close_id(requests[1].document_id).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(dirty_close.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.pending_close_document_id()),
        Some(requests[1].document_id)
    );
    let cancel = cx.debug_bounds(DOCUMENT_DIRTY_CLOSE_CANCEL_ID).unwrap();
    cx.simulate_click(cancel.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.pending_close_document_id()),
        None
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(requests[1].document_id, cx)
            .is_some_and(|snapshot| snapshot.dirty && snapshot.rectangles.len() == 1)
    }));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let active_focus = workspace
        .read_with(cx, |workspace, _| {
            workspace.session_tab_focus_handle(requests[0].document_id)
        })
        .unwrap();
    let inactive_dirty_close = cx
        .debug_bounds(Box::leak(
            document_session_close_id(requests[1].document_id).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(inactive_dirty_close.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let discard = cx.debug_bounds(DOCUMENT_DIRTY_CLOSE_DISCARD_ID).unwrap();
    cx.simulate_click(discard.center(), Modifiers::default());
    assert!(released[1].load(Ordering::Acquire));
    assert!(!released[0].load(Ordering::Acquire));
    assert!(!released[2].load(Ordering::Acquire));
    assert!(cx.update(|window, _| active_focus.is_focused(window)));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let third_tab = cx
        .debug_bounds(Box::leak(
            document_session_tab_id(requests[2].document_id).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(third_tab.center(), Modifiers::default());
    let active_clean_close = cx
        .debug_bounds(Box::leak(
            document_session_close_id(requests[2].document_id).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(active_clean_close.center(), Modifiers::default());
    assert!(released[2].load(Ordering::Acquire));
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(requests[0].document_id),
        "closing the active clean tab must select its stable successor"
    );
    assert!(cx.update(|window, _| active_focus.is_focused(window)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let last_clean_close = cx
        .debug_bounds(Box::leak(
            document_session_close_id(requests[0].document_id).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(last_clean_close.center(), Modifiers::default());
    assert!(
        released
            .iter()
            .all(|released| released.load(Ordering::Acquire))
    );
    assert!(workspace.read_with(cx, |workspace, _| workspace.sessions().is_empty()));
}

#[gpui::test]
fn real_session_tab_strip_preserves_loading_and_failed_identity_without_a_ready_document(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let failed = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("failed.pdf"), cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds(Box::leak(
            document_session_tab_id(failed.document_id).into_boxed_str(),
        ))
        .is_some(),
        "an opening session must retain a visible stable tab"
    );

    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(&failed, Err("injected open failure".into()), cx)
    });
    let loading = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("loading.pdf"), cx)
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    for document_id in [failed.document_id, loading.document_id] {
        assert!(
            cx.debug_bounds(Box::leak(
                document_session_tab_id(document_id).into_boxed_str(),
            ))
            .is_some(),
            "failed and loading sessions must not disappear without a ready active document"
        );
        assert!(
            cx.debug_bounds(Box::leak(
                document_session_close_id(document_id).into_boxed_str(),
            ))
            .is_some()
        );
    }
}

#[gpui::test]
fn clean_close_selects_the_adjacent_successor_and_dirty_requests_keep_first_authority(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let mut requests = Vec::new();
    for name in ["first.pdf", "second.pdf", "third.pdf"] {
        let request = workspace.update(cx, |workspace, cx| {
            workspace.begin_open(PathBuf::from(name), cx)
        });
        workspace.update(cx, |workspace, cx| {
            workspace.apply_open_result(
                &request,
                Ok(opened_document(Arc::new(AtomicBool::new(false)))),
                cx,
            )
        });
        requests.push(request);
    }
    workspace.update(cx, |workspace, cx| {
        workspace.activate_document(requests[0].document_id, cx);
        workspace.close_document(requests[0].document_id, cx);
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(requests[1].document_id),
        "closing an active tab must choose the next tab at the closed index"
    );
    for request in &requests[1..] {
        workspace
            .update(cx, |workspace, cx| {
                workspace.create_rectangle(
                    request.document_id,
                    0,
                    MarkupId::new(format!("dirty:{}", request.document_id.value())).unwrap(),
                    PdfPoint::new(20., 20.).unwrap(),
                    PdfPoint::new(80., 80.).unwrap(),
                    cx,
                )
            })
            .unwrap();
    }
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .request_close_document(requests[1].document_id, cx)),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .request_close_document(requests[2].document_id, cx)),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.pending_close_document_id()),
        Some(requests[1].document_id),
        "a competing dirty-close request must not replace the first stable target"
    );
}

#[gpui::test]
fn retained_viewer_plans_continuous_and_single_page_work_with_bounded_tiles(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("viewer.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .evidence_snapshot(request.document_id, cx)
                .unwrap()
                .rendered_device_pixel_ratio
        }),
        None,
        "an open document without a viewer plan must not claim rendered DPR authority"
    );

    let first = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::Continuous,
                100.,
                1.,
                1280.,
                200.,
                0.,
                780.,
                cx,
            )
        })
        .unwrap();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .evidence_snapshot(request.document_id, cx)
                .unwrap()
                .rendered_device_pixel_ratio
        }),
        None,
        "a planned but uncached viewer must not claim rendered DPR authority"
    );
    assert_eq!(first.visible_pages, vec![0, 1]);
    assert_eq!(first.current_page, Some(1));
    assert!(!first.tiles.is_empty());
    assert!(first.tiles.len() <= 32);
    assert!(first.requested_bytes <= 32 * 1024 * 1024 * 4);
    let equivalent = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::Continuous,
                100.,
                1.,
                1280.,
                200.,
                0.,
                780.,
                cx,
            )
        })
        .unwrap();
    assert_eq!(equivalent.generation, first.generation);
    let rendered = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &first, cx)
        })
        .unwrap();
    assert_eq!(rendered.rendered_tiles, first.tiles.len());
    assert_eq!(rendered.cache_hits, 0);
    assert!(rendered.cache_entries <= 32);
    assert!(rendered.cache_bytes <= 256 * 1024 * 1024);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .evidence_snapshot(request.document_id, cx)
                .unwrap()
                .rendered_device_pixel_ratio
        }),
        Some(1.),
        "a fully cached current plan must publish its actual rendered DPR"
    );
    let warm = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &equivalent, cx)
        })
        .unwrap();
    assert_eq!(warm.rendered_tiles, 0);
    assert_eq!(warm.cache_hits, equivalent.tiles.len());
    assert_eq!(warm.cache_bytes, rendered.cache_bytes);

    let single = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::SinglePage,
                1600.,
                2.,
                1280.,
                800.,
                12000.,
                7000.,
                cx,
            )
        })
        .unwrap();
    assert_eq!(single.page_layouts.len(), 1);
    assert!(single.tiles.len() <= 32);
    assert_eq!(single.cache_max_bytes, 256 * 1024 * 1024);
    assert!(single.requested_bytes <= 32 * 1024 * 1024 * 4);
    assert_ne!(single.generation, first.generation);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .evidence_snapshot(request.document_id, cx)
                .unwrap()
                .rendered_device_pixel_ratio
        }),
        None,
        "a new current plan must reject cached DPR evidence from the prior generation"
    );
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.render_planned_tiles_for_evidence(request.document_id, &first, cx)
            })
            .is_err(),
        "a changed visible tile set must reject the prior generation"
    );
}

#[gpui::test]
fn visible_native_viewport_renders_async_tiles_and_replans_without_stale_application(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("visible-viewer.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    workspace.update(cx, |workspace, cx| {
        workspace.set_view_configuration(request.document_id, PageViewMode::Continuous, 400., cx);
        workspace
            .refresh_viewport_async(request.document_id, 640., 480., 1., cx)
            .unwrap();
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let initial = workspace
        .read_with(cx, |workspace, cx| {
            workspace.viewer_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(initial.mode, PageViewMode::Continuous);
    assert_eq!(initial.zoom_percent, 400.);
    assert!(initial.generation > 0);
    assert!(initial.cache_entries > 0);
    assert_eq!(initial.queued_tiles, 0);
    assert_eq!(initial.active_tiles, 0);
    assert!(initial.cache_bytes <= initial.cache_max_bytes);
    assert!(cx.debug_bounds(DOCUMENT_VIEWPORT_ID).is_some());
    let page_id = Box::leak(document_viewer_page_id(request.document_id, 0).into_boxed_str());
    assert!(cx.debug_bounds(page_id).is_some());
    let tile_id = Box::leak(
        document_viewer_tile_id(request.document_id, initial.generation, 0, 0).into_boxed_str(),
    );
    assert!(cx.debug_bounds(tile_id).is_some());

    workspace.update(cx, |workspace, cx| {
        workspace.set_view_configuration(request.document_id, PageViewMode::SinglePage, 800., cx);
        workspace
            .refresh_viewport_async(request.document_id, 640., 480., 1., cx)
            .unwrap();
    });
    cx.run_until_parked();
    let changed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.viewer_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(changed.mode, PageViewMode::SinglePage);
    assert_eq!(changed.zoom_percent, 800.);
    assert_ne!(changed.generation, initial.generation);
    assert_eq!(changed.queued_tiles, 0);
    assert_eq!(changed.active_tiles, 0);
    assert!(changed.rejected_stale_tiles <= initial.rejected_stale_tiles + 32);

    workspace.update(cx, |workspace, cx| {
        assert!(workspace.set_viewport_scroll(request.document_id, 0., 1_200., cx));
        workspace
            .refresh_viewport_async(request.document_id, 640., 480., 1., cx)
            .unwrap();
    });
    cx.run_until_parked();
    let scrolled = workspace
        .read_with(cx, |workspace, cx| {
            workspace.viewer_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_ne!(scrolled.generation, changed.generation);
    assert!(scrolled.cache_bytes <= scrolled.cache_max_bytes);
}

#[gpui::test]
fn painted_page_evidence_is_generation_scoped_and_advances_at_native_prepaint(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("painted-page-evidence.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });

    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let viewport = cx
        .debug_bounds(DOCUMENT_VIEWPORT_ID)
        .expect("the native viewport must resolve before its paint handshake");
    let device_scale = cx.update(|window, _| window.scale_factor());
    let plan = workspace
        .update(cx, |workspace, cx| {
            workspace.set_view_configuration(
                request.document_id,
                PageViewMode::SinglePage,
                100.,
                cx,
            );
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::SinglePage,
                100.,
                device_scale,
                f32::from(viewport.size.width),
                f32::from(viewport.size.height),
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &plan, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let first = workspace
        .read_with(cx, |workspace, cx| {
            workspace.painted_page_evidence(request.document_id, 0, cx)
        })
        .expect("a fully tiled page must publish its actual prepaint evidence");
    assert_eq!(first.document_id, request.document_id);
    assert_eq!(first.page_index, 0);
    assert_eq!(first.source_pdf_page_size_points, (612., 792.));
    assert_eq!(first.viewer_generation, plan.generation);
    let first_workspace_evidence = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        first.request_generation,
        first_workspace_evidence.request_generation
    );
    assert!(first.resource_generation > 0);
    assert!((first.rendered_dpr - device_scale).abs() < 0.001);
    assert!(first.contained_bounds.size.width > px(0.));
    assert!(first.contained_bounds.size.height > px(0.));
    assert!(first.painted_state_sequence > 0);
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.painted_page_evidence(request.document_id, 1, cx)
            })
            .is_none(),
        "a hidden page must not inherit another page's paint evidence"
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.painted_page_evidence(DocumentId::new(999), 0, cx)
            })
            .is_none(),
        "a foreign document identity must not resolve paint evidence"
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let second = workspace
        .read_with(cx, |workspace, cx| {
            workspace.painted_page_evidence(request.document_id, 0, cx)
        })
        .expect("a second real prepaint must refresh the paint evidence");
    assert_eq!(
        second, first,
        "an unchanged redraw must preserve the exact painted-state handshake"
    );

    let coincident_viewer_generation = workspace.read_with(cx, |workspace, cx| {
        workspace
            .viewer_snapshot(request.document_id, cx)
            .unwrap()
            .generation
    });
    let page_request = workspace.update(cx, |workspace, cx| {
        let request = workspace
            .begin_page_navigation(request.document_id, 0, cx)
            .unwrap();
        assert!(
            workspace
                .painted_page_evidence(request.document_id, 0, cx)
                .is_none(),
            "a newer document request must reject prior evidence before GPUI can prepaint again"
        );
        request
    });
    assert!(page_request.generation > second.request_generation);
    let request_authority = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(request.document_id, cx)
        })
        .unwrap();
    assert_eq!(
        request_authority.request_generation,
        page_request.generation
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .viewer_snapshot(request.document_id, cx)
                .unwrap()
                .generation
        }),
        coincident_viewer_generation,
        "request authority must be tested while viewer generation remains coincident"
    );
    if let Some(refreshed) = workspace.read_with(cx, |workspace, cx| {
        workspace.painted_page_evidence(request.document_id, 0, cx)
    }) {
        assert_eq!(refreshed.request_generation, page_request.generation);
        assert_eq!(refreshed.viewer_generation, coincident_viewer_generation);
        assert!(refreshed.painted_state_sequence > second.painted_state_sequence);
    }
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &page_request,
            Ok(raster(32, 40)),
            cx,
        )),
        ApplyDisposition::Applied
    );

    let changed = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::SinglePage,
                125.,
                device_scale,
                f32::from(viewport.size.width),
                f32::from(viewport.size.height),
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    assert_ne!(changed.generation, plan.generation);
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.painted_page_evidence(request.document_id, 0, cx)
            })
            .is_none(),
        "a layout generation change must invalidate the prior painted bounds"
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &changed, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let changed_evidence = workspace
        .read_with(cx, |workspace, cx| {
            workspace.painted_page_evidence(request.document_id, 0, cx)
        })
        .expect("the changed generation must publish replacement paint evidence");
    assert_eq!(changed_evidence.viewer_generation, changed.generation);
    assert!(
        changed_evidence.painted_state_sequence > second.painted_state_sequence,
        "a materially changed painted state must advance the sequence"
    );
    assert!(changed_evidence.request_generation > second.request_generation);

    let save_request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save_as(
                request.document_id,
                workspace_save_target("painted-page-evidence-saved.pdf"),
                cx,
            )
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_save_result(
            &save_request,
            Ok(SavedNativeDocument::new(
                opened_document(Arc::new(AtomicBool::new(false))),
                save_request.annotation_revision,
            )),
            cx,
        )),
        ApplyDisposition::Applied
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.painted_page_evidence(request.document_id, 0, cx)
            })
            .is_none(),
        "a validated resource replacement must clear evidence from the prior resource epoch"
    );
    let replacement_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::SinglePage,
                125.,
                device_scale,
                f32::from(viewport.size.width),
                f32::from(viewport.size.height),
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &replacement_plan, cx)
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let replacement_evidence = workspace
        .read_with(cx, |workspace, cx| {
            workspace.painted_page_evidence(request.document_id, 0, cx)
        })
        .expect("the replacement resource must publish new paint authority");
    assert!(
        replacement_evidence.resource_generation > changed_evidence.resource_generation,
        "replacement pixels must carry a newer resource epoch"
    );
    assert!(
        replacement_evidence.painted_state_sequence > changed_evidence.painted_state_sequence,
        "resource replacement is a material painted-state change"
    );
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(request.document_id, cx)
    }));
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.painted_page_evidence(request.document_id, 0, cx)
            })
            .is_none(),
        "closing the stable document identity must remove its paint evidence"
    );
    let reopened = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("painted-page-evidence.pdf"), cx)
    });
    assert_ne!(reopened.document_id, request.document_id);
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &reopened,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    assert!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.painted_page_evidence(reopened.document_id, 0, cx)
            })
            .is_none(),
        "reopening the same path under a new stable identity must not reuse old paint evidence"
    );
}

#[gpui::test]
fn painted_page_evidence_keeps_source_points_while_rotation_changes_contained_bounds(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("painted-page-rotation.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_painted_rotation_document(Arc::new(AtomicBool::new(
                false,
            )))),
            cx,
        )
    });
    let rotation = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_rotation(request.document_id, 0, PageRotationDirection::Right, cx)
        })
        .unwrap();
    let presentation = workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_rotation_for_evidence(&rotation, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.apply_page_rotation_result(&rotation, Ok(presentation), cx)
        }),
        ApplyDisposition::Applied
    );

    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    let viewport = cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let device_scale = cx.update(|window, _| window.scale_factor());
    let first_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.set_view_configuration(
                request.document_id,
                PageViewMode::SinglePage,
                100.,
                cx,
            );
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::SinglePage,
                100.,
                device_scale,
                f32::from(viewport.size.width),
                f32::from(viewport.size.height),
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    assert_eq!(first_plan.visible_pages, vec![0]);
    assert!(
        !first_plan.tiles.is_empty(),
        "the rotated page must request at least one real viewer tile"
    );
    let first_render = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &first_plan, cx)
        })
        .unwrap();
    assert_eq!(
        first_render.rendered_tiles + first_render.cache_hits,
        first_plan.tiles.len(),
        "every rotated-page request must be present before native prepaint"
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let settled_viewport = cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    let settled_device_scale = cx.update(|window, _| window.scale_factor());
    let plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                request.document_id,
                PageViewMode::SinglePage,
                100.,
                settled_device_scale,
                f32::from(settled_viewport.size.width),
                f32::from(settled_viewport.size.height),
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    let settled_render = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(request.document_id, &plan, cx)
        })
        .unwrap();
    assert_eq!(
        settled_render.rendered_tiles + settled_render.cache_hits,
        plan.tiles.len(),
        "the current rotated viewer generation must be fully cache-backed"
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| {
                workspace.viewer_snapshot(request.document_id, cx)
            })
            .unwrap()
            .generation,
        plan.generation,
        "native prepaint must not silently replace the settled viewer generation"
    );
    let annotation_bounds = cx
        .debug_bounds(Box::leak(
            document_annotation_layer_id(request.document_id, 0).into_boxed_str(),
        ))
        .expect("the rotated annotation canvas must resolve before querying its evidence");
    assert!(
        annotation_bounds.size.width > px(0.) && annotation_bounds.size.height > px(0.),
        "the rotated annotation canvas must retain positive prepaint geometry"
    );

    let evidence = workspace
        .read_with(cx, |workspace, cx| {
            workspace.painted_page_evidence(request.document_id, 0, cx)
        })
        .expect("the rotated fully tiled page must publish paint evidence");
    assert_eq!(evidence.source_pdf_page_size_points, (200., 300.));
    let width = f32::from(evidence.contained_bounds.size.width);
    let height = f32::from(evidence.contained_bounds.size.height);
    assert!(width > height);
    assert!((width / height - 1.5).abs() < 0.01);
}

#[test]
fn document_ids_are_stable_value_objects() {
    assert_eq!(DocumentId::new(7).to_string(), "document-7");
    assert_eq!(
        document_thumbnail_id(DocumentId::new(7), 3),
        "document-7-thumbnail-3"
    );
    assert!(px(1.) > px(0.));
}

#[test]
fn raster_variation_compares_complete_pixels_instead_of_color_channels() {
    let uniform = raster(2, 1);
    assert!(!uniform.has_spatial_variation());

    let varied = RasterSurface::new(2, 1, vec![0, 17, 31, 255, 1, 17, 31, 255]).unwrap();
    assert!(varied.has_spatial_variation());
}

#[test]
fn public_multi_page_fixture_retains_its_reviewed_digest() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    let bytes = std::fs::read(&fixture).expect("the provenance-controlled PDF fixture must exist");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b"
    );
}

#[test]
fn deleting_imported_rectangle_removes_only_its_native_pdf_annotation() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let target = manifest_dir.join(format!(
        ".prepared/annotation-delete-{}.pdf",
        std::process::id()
    ));
    assert!(!target.exists(), "the deletion evidence target must be new");
    let mut persistence =
        PdfPersistenceSession::open(&source).expect("annotation fixture must open");
    let deleted_id = persistence
        .rectangles()
        .first()
        .expect("annotation fixture must contain a native Rectangle")
        .id
        .clone();
    let untouched_before = persistence.untouched_annotations().to_vec();
    persistence
        .remove_rectangle(&deleted_id)
        .expect("stable-ID Rectangle deletion must remove its native page reference");
    assert!(
        persistence
            .rectangles()
            .iter()
            .all(|rectangle| rectangle.id != deleted_id)
    );
    persistence
        .save_as(&target)
        .expect("deleted PDF must save atomically");

    let reopened = PdfPersistenceSession::open(&target).expect("deleted PDF must reopen");
    assert!(
        reopened
            .rectangles()
            .iter()
            .all(|rectangle| rectangle.id != deleted_id)
    );
    assert!(!reopened.has_raw_annotation_name(&deleted_id));
    assert_eq!(reopened.untouched_annotations(), untouched_before);
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&target)
            .status()
            .expect("qpdf must be available for structural validation")
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&target)
            .status()
            .expect("pdfinfo must be available for independent reopen validation")
            .success()
    );
    std::fs::remove_file(target).expect("the disposable deletion evidence must be removable");
}

#[gpui::test]
fn native_view_navigation_routes_real_single_page_wheel_and_control_zoom(cx: &mut TestAppContext) {
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("native-navigation.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.set_page_view_mode(request.document_id, PageViewMode::SinglePage, cx));
        assert!(workspace.set_document_wheel_behavior(
            request.document_id,
            PageViewMode::SinglePage,
            WheelBehavior::Scroll,
            cx,
        ));
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let viewport = cx.debug_bounds(DOCUMENT_VIEWPORT_ID).unwrap();
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(79.))),
        ..Default::default()
    });
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(request.document_id, cx)
                .unwrap()
                .read(cx)
                .current_page()
        }),
        0
    );
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(1.))),
        ..Default::default()
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(request.document_id, cx)
                .unwrap()
                .read(cx)
                .current_page()
        }),
        1
    );

    let before_zoom = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(request.document_id, cx)
        })
        .unwrap()
        .zoom_percent();
    cx.simulate_event(ScrollWheelEvent {
        position: viewport.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(120.))),
        modifiers: Modifiers {
            control: true,
            ..Default::default()
        },
        ..Default::default()
    });
    cx.run_until_parked();
    let after = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(request.document_id, cx)
        })
        .unwrap();
    assert!(after.zoom_percent() < before_zoom);
    assert_eq!(
        after.zoom_preset(),
        butter_paper_gpui_component_compat::native_document_view_state::ViewerZoomPreset::Manual,
    );
}

#[gpui::test]
fn native_file_authority_opens_multiple_pdfs_and_rejects_a_stale_save_target(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace =
                cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(SuccessfulOpener), cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let open = cx.debug_bounds(VIEWPORT_OPEN_DOCUMENT_ID).unwrap();
    cx.simulate_click(open.center(), Modifiers::default());
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files);
        assert!(!options.directories);
        assert!(options.multiple);
        assert_eq!(options.prompt.as_deref(), Some("Open PDFs"));
        Some(vec![
            PathBuf::from("first-native.pdf"),
            PathBuf::from("first-native.pdf"),
            PathBuf::from("not-a-pdf.txt"),
        ])
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1
    );
    let document_id = workspace
        .read_with(cx, |workspace, _| workspace.active_document_id())
        .expect("the valid PDF must remain the active live document");
    assert!(workspace.read_with(cx, |workspace, cx| {
        matches!(
            workspace
                .session(document_id, cx)
                .unwrap()
                .read(cx)
                .status(),
            NativeDocumentStatus::Ready
        )
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.fit_zoom_percent(
            document_id,
            ViewerFitPreset::Width,
            1_000.,
            560.,
            cx,
        )),
        Some(150.),
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.fit_zoom_percent(
            document_id,
            ViewerFitPreset::Page,
            1_000.,
            560.,
            cx,
        )),
        Some(66.),
    );
    assert_eq!(
        save_as_prompt_spec(std::path::Path::new("first-native.pdf")),
        butter_paper_gpui_component_compat::document_workspace::SaveAsPromptSpec {
            directory: PathBuf::new(),
            suggested_name: "first-native-annotated.pdf".into(),
        }
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("native-file-authority:dirty").unwrap(),
                PdfPoint::new(20., 20.).unwrap(),
                PdfPoint::new(80., 80.).unwrap(),
                cx,
            )
        })
        .unwrap();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = cx.debug_bounds(DOCUMENT_SAVE_AS_ID).unwrap();
    cx.simulate_click(save.center(), Modifiers::default());
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|directory| {
        assert_eq!(directory, std::path::Path::new(""));
        None
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.rejected_stale_save_prompts()),
        0
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .dirty
    }));

    cx.simulate_click(save.center(), Modifiers::default());
    assert!(cx.did_prompt_for_new_path());
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(document_id, cx)
    }));
    cx.simulate_new_path_selection(|_| Some(PathBuf::from("stale-save.pdf")));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.rejected_stale_save_prompts()),
        1
    );
    assert!(!std::path::Path::new("stale-save.pdf").exists());
}

#[cfg(unix)]
#[test]
fn native_file_authority_binds_one_exact_non_utf8_pdf_target_and_is_consumed_once() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let root = manifest_dir
        .join(".prepared")
        .join(format!("save-target-authority-{}", std::process::id()));
    let selected_parent = root.join("selected");
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&selected_parent).unwrap();
    let _scratch = ScratchDirectories(vec![root]);
    let mut leaf = b"drawing-".to_vec();
    leaf.push(0xff);
    leaf.extend_from_slice(b".PdF");
    let target = selected_parent.join(OsString::from_vec(leaf));

    let authority = SaveAsTargetAuthority::bind(target.clone(), &source).unwrap();
    assert_eq!(authority.path(), target.as_path());
    let persistence = PdfPersistenceSession::open(&source).unwrap();
    let prepared = persistence.prepare_save_authorized(&authority).unwrap();
    assert_eq!(prepared.path().parent(), Some(selected_parent.as_path()));
    let second = match persistence.prepare_save_authorized(&authority) {
        Ok(_) => panic!("a one-shot save authority must reject a second preparation"),
        Err(error) => error,
    };
    assert_eq!(
        second.save_target_error().map(|error| error.kind()),
        Some(SaveTargetErrorKind::AlreadyConsumed)
    );
    assert_eq!(prepared.publish().unwrap(), PdfPublicationOutcome::Durable);
    assert!(target.is_file());
}

#[cfg(unix)]
#[test]
fn native_file_authority_rejects_parent_substitution_before_staging_or_publication() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let root = manifest_dir
        .join(".prepared")
        .join(format!("save-target-parent-race-{}", std::process::id()));
    let selected_parent = root.join("selected");
    let moved_parent = root.join("moved");
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&selected_parent).unwrap();
    let _scratch = ScratchDirectories(vec![root]);
    let target = selected_parent.join("safe.pdf");
    let authority = SaveAsTargetAuthority::bind(target.clone(), &source).unwrap();

    std::fs::rename(&selected_parent, &moved_parent).unwrap();
    std::fs::create_dir(&selected_parent).unwrap();
    let persistence = PdfPersistenceSession::open(&source).unwrap();
    let error = match persistence.prepare_save_authorized(&authority) {
        Ok(_) => panic!("a replaced parent directory must reject save preparation"),
        Err(error) => error,
    };
    assert_eq!(
        error.save_target_error().map(|error| error.kind()),
        Some(SaveTargetErrorKind::ParentChanged)
    );
    assert!(!target.exists());
    assert!(!moved_parent.join("safe.pdf").exists());
}

#[cfg(unix)]
#[test]
fn native_file_authority_loses_a_target_creation_race_without_overwriting_the_winner() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let root = manifest_dir
        .join(".prepared")
        .join(format!("save-target-leaf-race-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let _scratch = ScratchDirectories(vec![root.clone()]);
    let target = root.join("occupied.pdf");
    let authority = SaveAsTargetAuthority::bind(target.clone(), &source).unwrap();
    std::fs::write(&target, b"competitor-owned-bytes").unwrap();

    let persistence = PdfPersistenceSession::open(&source).unwrap();
    let error = match persistence.prepare_save_authorized(&authority) {
        Ok(_) => panic!("an occupied save target must reject save preparation"),
        Err(error) => error,
    };
    assert_eq!(
        error.save_target_error().map(|error| error.kind()),
        Some(SaveTargetErrorKind::TargetExists)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"competitor-owned-bytes");
}

#[cfg(unix)]
#[test]
fn native_file_authority_rejects_staging_name_substitution_before_publication() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let root = manifest_dir
        .join(".prepared")
        .join(format!("save-target-stage-race-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&root).unwrap();
    let _scratch = ScratchDirectories(vec![root.clone()]);
    let target = root.join("published.pdf");
    let authority = SaveAsTargetAuthority::bind(target.clone(), &source).unwrap();
    let persistence = PdfPersistenceSession::open(&source).unwrap();
    let prepared = persistence.prepare_save_authorized(&authority).unwrap();
    let stage_path = prepared.path().to_path_buf();
    let displaced_stage = root.join("displaced-stage");

    std::fs::rename(&stage_path, &displaced_stage).unwrap();
    std::fs::write(&stage_path, b"replacement bytes that were never validated").unwrap();
    let error = prepared
        .publish()
        .expect_err("publication must retain the exact prepared staging-file identity");
    assert!(
        error.to_string().contains("staging file changed"),
        "{error}"
    );
    assert!(!target.exists());
    assert_eq!(
        std::fs::read(&stage_path).unwrap(),
        b"replacement bytes that were never validated"
    );
}

#[cfg(unix)]
#[test]
fn native_file_authority_rejects_relative_non_pdf_same_source_and_symlink_parent_inputs() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let root = manifest_dir
        .join(".prepared")
        .join(format!("save-target-inputs-{}", std::process::id()));
    let real_parent = root.join("real");
    let linked_parent = root.join("linked");
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&real_parent).unwrap();
    std::os::unix::fs::symlink(&real_parent, &linked_parent).unwrap();
    let _scratch = ScratchDirectories(vec![root]);

    for (selected, expected) in [
        (
            PathBuf::from("relative.pdf"),
            SaveTargetErrorKind::NotAbsolute,
        ),
        (real_parent.join("drawing.txt"), SaveTargetErrorKind::NotPdf),
        (source.clone(), SaveTargetErrorKind::SameAsSource),
        (
            linked_parent.join("drawing.pdf"),
            SaveTargetErrorKind::UnsafeParent,
        ),
    ] {
        let error = SaveAsTargetAuthority::bind(selected, &source).unwrap_err();
        assert_eq!(error.kind(), expected);
    }
}

#[cfg(unix)]
#[gpui::test]
fn native_file_authority_workspace_save_as_rejects_parent_substitution_after_begin(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    let root = manifest_dir
        .join(".prepared")
        .join(format!("workspace-save-target-race-{}", std::process::id()));
    let selected_parent = root.join("selected");
    let moved_parent = root.join("moved");
    std::fs::remove_dir_all(&root).ok();
    std::fs::create_dir_all(&selected_parent).unwrap();
    let _scratch = ScratchDirectories(vec![root]);
    let target = selected_parent.join("workspace-save.pdf");

    let workspace = cx.new(DocumentWorkspace::new);
    let open_request =
        workspace.update(cx, |workspace, cx| workspace.begin_open(source.clone(), cx));
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &open_request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    let request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save_as(open_request.document_id, target.clone(), cx)
        })
        .expect("an absolute new PDF target must bind before background saving");

    std::fs::rename(&selected_parent, &moved_parent).unwrap();
    std::fs::create_dir(&selected_parent).unwrap();
    let error = match PdfDocumentSaver::new(Arc::new(SuccessfulOpener)).save(&request) {
        Ok(_) => panic!("a substituted selected directory must not receive published bytes"),
        Err(error) => error,
    };
    assert!(
        error.contains("selected parent directory changed"),
        "{error}"
    );
    assert!(!target.exists());
    assert!(!moved_parent.join("workspace-save.pdf").exists());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(open_request.document_id, cx).is_some()
    }));
}

#[gpui::test]
fn regular_png_native_picker_prepares_the_target_and_rejects_a_closed_session(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let checker =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-image-checker-v1.png");
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
    let request = workspace.update(cx, |workspace, cx| {
        workspace.begin_open(PathBuf::from("native-image-picker.pdf"), cx)
    });
    workspace.update(cx, |workspace, cx| {
        workspace.apply_open_result(
            &request,
            Ok(opened_document(Arc::new(AtomicBool::new(false)))),
            cx,
        )
    });
    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_IMAGE_TOOL_ID);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let button = cx.debug_bounds(DOCUMENT_IMAGE_TOOL_ID).unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response({
        let checker = checker.clone();
        move |options| {
            assert!(options.files);
            assert!(!options.directories);
            assert!(!options.multiple);
            assert_eq!(
                options.prompt.as_deref(),
                Some("Select a PNG or JPEG image")
            );
            Some(vec![checker])
        }
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(request.document_id, cx)),
        Some(AnnotationTool::Image),
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.rejected_stale_image_prepares()),
        0,
    );

    cx.simulate_click(button.center(), Modifiers::default());
    assert!(cx.did_prompt_for_paths());
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(request.document_id, cx)
    }));
    cx.simulate_path_prompt_response(move |_| Some(vec![checker]));
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.rejected_stale_image_prepares()),
        1,
    );
}

#[gpui::test]
fn regular_png_workspace_create_move_resize_save_reopen_and_release_is_exact(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_component::init);
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let checker =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-image-checker-v1.png");
    let target = manifest_dir.join(format!(
        ".prepared/regular-image-save-{}.pdf",
        std::process::id()
    ));
    assert!(!target.exists());

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
    let open_request =
        workspace.update(cx, |workspace, cx| workspace.begin_open(source.clone(), cx));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_open_result(
            &open_request,
            Ok(opened_annotation_all_document(Arc::new(AtomicBool::new(
                false,
            )))),
            cx,
        )),
        ApplyDisposition::Applied,
    );

    let image_id = MarkupId::new("workspace:image:regular-png-1").unwrap();
    let asset_id = workspace
        .update(cx, |workspace, cx| {
            workspace.insert_image_at(
                open_request.document_id,
                0,
                &checker,
                image_id.clone(),
                PdfPoint::new(432., 444.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let placed = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_snapshot(open_request.document_id, cx)
    });
    let placed = placed.unwrap();
    assert_eq!(placed.images.len(), 1);
    assert_eq!(placed.images[0].id, image_id);
    assert_eq!(
        placed.images[0].rect,
        PdfRect::new(294.3, 340.725, 275.4, 206.55).unwrap(),
    );
    assert!(!placed.images[0].aspect_locked);
    assert_eq!(placed.images[0].asset().id().as_str(), asset_id);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.image_asset_count(open_request.document_id, cx)
        }),
        1,
    );
    let render_asset = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .image_render_asset_weak(open_request.document_id, &asset_id, cx)
                .and_then(|asset| asset.upgrade())
        })
        .expect("one retained GPUI render resource must back page and thumbnail scenes");
    let page_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(open_request.document_id, 0, cx)
    });
    let thumbnail_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.thumbnail_annotation_scene(open_request.document_id, 0, cx)
    });
    assert_eq!(
        page_scene.images[0].asset_id,
        thumbnail_scene.images[0].asset_id
    );

    let moved = PdfRect::new(324.3, 350.725, 275.4, 206.55).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_image_rect(open_request.document_id, moved, cx)
        })
        .unwrap();
    let resized = PdfRect::new(324.3, 350.725, 305.4, 206.55).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_image_rect(open_request.document_id, resized, cx)
        })
        .unwrap();

    let save_request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_save_as(open_request.document_id, target.clone(), cx)
        })
        .unwrap();
    let saved = PdfDocumentSaver::new(Arc::new(AnnotationAllSuccessfulOpener))
        .save(&save_request)
        .expect("the regular PNG must pass staged typed reopen validation");
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.apply_save_result(&save_request, Ok(saved), cx)
        }),
        ApplyDisposition::Applied,
    );
    let clean = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(open_request.document_id, cx)
        })
        .unwrap();
    assert!(!clean.dirty);
    assert_eq!(clean.images[0].rect, resized);
    let reopened = PdfPersistenceSession::open(&target).unwrap();
    let reopened_image = reopened
        .images()
        .iter()
        .find(|image| image.id == image_id)
        .unwrap();
    assert!(
        [
            (reopened_image.rect.x, resized.x),
            (reopened_image.rect.y, resized.y),
            (reopened_image.rect.width, resized.width),
            (reopened_image.rect.height, resized.height),
        ]
        .into_iter()
        .all(|(actual, expected)| (actual - expected).abs() <= 0.000_1),
        "PDF Real serialization must preserve image geometry within 0.0001 point",
    );
    assert_eq!(reopened_image.asset().id().as_str(), asset_id);
    assert!(!reopened_image.aspect_locked);
    assert!(reopened.image_has_canonical_native_identity(&image_id));

    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert!(window.has_image_atlas_entry(&render_asset));
    });
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(open_request.document_id, cx)
        }),
        CloseRequestDisposition::Closed,
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.image_asset_count(open_request.document_id, cx)
        }),
        0,
    );
    cx.update(|window, _| assert!(!window.has_image_atlas_entry(&render_asset)));
    let weak = Arc::downgrade(&render_asset);
    drop(render_asset);
    assert!(weak.upgrade().is_none());
    std::fs::remove_file(target).unwrap();
}

#[test]
fn text_box_save_as_reconciles_create_edit_delete_and_reopens_exact_typed_state() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let edited_target = manifest_dir.join(format!(
        ".prepared/text-box-edit-save-{}.pdf",
        std::process::id()
    ));
    let deleted_target = manifest_dir.join(format!(
        ".prepared/text-box-delete-save-{}.pdf",
        std::process::id()
    ));
    assert!(!edited_target.exists() && !deleted_target.exists());
    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let mut text_boxes = source_session.text_boxes().to_vec();
    assert_eq!(text_boxes.len(), 1);
    text_boxes[0] = TextBoxAnnotation::new(
        text_boxes[0].id.clone(),
        text_boxes[0].page_index,
        text_boxes[0].layout_rect,
        "Beam B-12 / revision 5",
        text_boxes[0].style().clone(),
    )
    .unwrap();
    let created = TextBoxAnnotation::new(
        MarkupId::new("workspace:text:persistence-1").unwrap(),
        0,
        PdfRect::new(72., 560., 252., 72.).unwrap(),
        "GPUI text box\nUnicode 世界",
        text_boxes[0].style().clone(),
    )
    .unwrap();
    text_boxes.push(created.clone());
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(80),
            generation: 2,
            source_path: source.clone(),
            destination: save_as_destination(&source, &edited_target),
            current_page: 0,
            annotation_revision: 2,
            annotations: AnnotationSnapshot {
                revision: 2,
                saved_revision: 0,
                dirty: true,
                selected_id: Some(created.id.clone()),
                annotation_order: Vec::new(),
                rectangles: source_session.rectangles().to_vec(),
                ellipses: source_session.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: source_session.pens().to_vec(),
                straight_lines: Vec::new(),
                vertex_paths: source_session.vertex_paths().to_vec(),

                clouds: source_session.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: source_session.callouts().to_vec(),
                measurement_paths: source_session.measurement_paths().to_vec(),
                text_boxes: text_boxes.clone(),
                lengths: source_session.lengths().to_vec(),
                images: source_session.images().to_vec(),
                snapshots: source_session.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 2,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Text Box create and edit must survive typed Save As validation");
    let edited = PdfPersistenceSession::open(&edited_target).unwrap();
    assert_eq!(edited.text_boxes(), text_boxes.as_slice());

    let deleted_id = edited.text_boxes()[0].id.clone();
    let remaining = edited
        .text_boxes()
        .iter()
        .filter(|text_box| text_box.id != deleted_id)
        .cloned()
        .collect::<Vec<_>>();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(80),
            generation: 3,
            source_path: edited_target.clone(),
            destination: save_as_destination(&edited_target, &deleted_target),
            current_page: 0,
            annotation_revision: 3,
            annotations: AnnotationSnapshot {
                revision: 3,
                saved_revision: 2,
                dirty: true,
                selected_id: Some(created.id.clone()),
                annotation_order: Vec::new(),
                rectangles: edited.rectangles().to_vec(),
                ellipses: edited.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: edited.pens().to_vec(),
                straight_lines: Vec::new(),
                vertex_paths: edited.vertex_paths().to_vec(),

                clouds: edited.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: edited.callouts().to_vec(),
                measurement_paths: edited.measurement_paths().to_vec(),
                text_boxes: remaining.clone(),
                lengths: edited.lengths().to_vec(),
                images: edited.images().to_vec(),
                snapshots: edited.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 3,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Text Box deletion must survive typed Save As validation");
    let deleted = PdfPersistenceSession::open(&deleted_target).unwrap();
    assert_eq!(deleted.text_boxes(), remaining.as_slice());
    assert!(!deleted.has_raw_annotation_name(&deleted_id));
    std::fs::remove_file(edited_target).unwrap();
    std::fs::remove_file(deleted_target).unwrap();
}

#[test]
fn length_save_as_reconciles_create_edit_delete_and_reopens_exact_typed_state() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let created_target = manifest_dir.join(format!(
        ".prepared/length-create-save-{}.pdf",
        std::process::id()
    ));
    let edited_target = manifest_dir.join(format!(
        ".prepared/length-edit-save-{}.pdf",
        std::process::id()
    ));
    let deleted_target = manifest_dir.join(format!(
        ".prepared/length-delete-save-{}.pdf",
        std::process::id()
    ));
    assert!(!created_target.exists() && !edited_target.exists() && !deleted_target.exists());
    let source_session = PdfPersistenceSession::open(&source).unwrap();
    assert!(source_session.lengths().is_empty());
    let created = LengthAnnotation::new(
        MarkupId::new("workspace:length:persistence-1").unwrap(),
        0,
        PdfPoint::new(72., 420.).unwrap(),
        PdfPoint::new(288., 420.).unwrap(),
        LengthCalibration::from_scale(72., 1., "m", 2, false)
            .unwrap()
            .with_label("Span")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(created.caption(), "Span: 3.00 m");
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(81),
            generation: 1,
            source_path: source.clone(),
            destination: save_as_destination(&source, &created_target),
            current_page: 0,
            annotation_revision: 1,
            annotations: AnnotationSnapshot {
                revision: 1,
                saved_revision: 0,
                dirty: true,
                selected_id: Some(created.id.clone()),
                annotation_order: Vec::new(),
                rectangles: source_session.rectangles().to_vec(),
                ellipses: source_session.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: source_session.pens().to_vec(),
                straight_lines: Vec::new(),
                vertex_paths: source_session.vertex_paths().to_vec(),

                clouds: source_session.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: source_session.callouts().to_vec(),
                measurement_paths: source_session.measurement_paths().to_vec(),
                text_boxes: source_session.text_boxes().to_vec(),
                lengths: vec![created.clone()],
                images: source_session.images().to_vec(),
                snapshots: source_session.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 1,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Length creation must survive typed Save As validation");
    let created_reopen = PdfPersistenceSession::open(&created_target).unwrap();
    assert_eq!(created_reopen.lengths(), &[created.clone()]);

    let edited_length = LengthAnnotation::new(
        created.id.clone(),
        created.page_index,
        created.start,
        PdfPoint::new(324., 420.).unwrap(),
        created.calibration().clone(),
    )
    .unwrap();
    assert_eq!(edited_length.caption(), "Span: 3.50 m");
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(81),
            generation: 2,
            source_path: created_target.clone(),
            destination: save_as_destination(&created_target, &edited_target),
            current_page: 0,
            annotation_revision: 2,
            annotations: AnnotationSnapshot {
                revision: 2,
                saved_revision: 1,
                dirty: true,
                selected_id: Some(edited_length.id.clone()),
                annotation_order: Vec::new(),
                rectangles: created_reopen.rectangles().to_vec(),
                ellipses: created_reopen.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: created_reopen.pens().to_vec(),
                straight_lines: Vec::new(),
                vertex_paths: created_reopen.vertex_paths().to_vec(),

                clouds: created_reopen.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: created_reopen.callouts().to_vec(),
                measurement_paths: created_reopen.measurement_paths().to_vec(),
                text_boxes: created_reopen.text_boxes().to_vec(),
                lengths: vec![edited_length.clone()],
                images: created_reopen.images().to_vec(),
                snapshots: created_reopen.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 2,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Length edit must survive typed Save As validation");
    let edited = PdfPersistenceSession::open(&edited_target).unwrap();
    assert_eq!(edited.lengths(), &[edited_length.clone()]);

    let deleted_id = edited_length.id.clone();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(81),
            generation: 3,
            source_path: edited_target.clone(),
            destination: save_as_destination(&edited_target, &deleted_target),
            current_page: 0,
            annotation_revision: 3,
            annotations: AnnotationSnapshot {
                revision: 3,
                saved_revision: 2,
                dirty: true,
                selected_id: None,
                annotation_order: Vec::new(),
                rectangles: edited.rectangles().to_vec(),
                ellipses: edited.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: edited.pens().to_vec(),
                straight_lines: Vec::new(),
                vertex_paths: edited.vertex_paths().to_vec(),

                clouds: edited.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: edited.callouts().to_vec(),
                measurement_paths: edited.measurement_paths().to_vec(),
                text_boxes: edited.text_boxes().to_vec(),
                lengths: Vec::new(),
                images: edited.images().to_vec(),
                snapshots: edited.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 3,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Length deletion must survive typed Save As validation");
    let deleted = PdfPersistenceSession::open(&deleted_target).unwrap();
    assert!(deleted.lengths().is_empty());
    assert!(!deleted.has_raw_annotation_name(&deleted_id));
    std::fs::remove_file(created_target).unwrap();
    std::fs::remove_file(edited_target).unwrap();
    std::fs::remove_file(deleted_target).unwrap();
}

#[test]
fn pen_save_as_reconciles_create_edit_delete_and_reopens_exact_typed_state() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let edited_target = manifest_dir.join(format!(
        ".prepared/pen-edit-save-{}.pdf",
        std::process::id()
    ));
    let deleted_target = manifest_dir.join(format!(
        ".prepared/pen-delete-save-{}.pdf",
        std::process::id()
    ));
    assert!(!edited_target.exists() && !deleted_target.exists());
    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let mut pens = source_session.pens().to_vec();
    assert_eq!(
        pens.len(),
        1,
        "the reviewed fixture contains one imported ink path"
    );
    pens[0].appearance = PenAppearance::new(
        pens[0].appearance.color(),
        pens[0].appearance.width_pt(),
        0.5,
    )
    .unwrap();
    let created = PenAnnotation::new(
        MarkupId::new("workspace:pen:persistence-1").unwrap(),
        0,
        vec![
            PdfPoint::new(72., 96.).unwrap(),
            PdfPoint::new(120., 144.).unwrap(),
            PdfPoint::new(180., 160.).unwrap(),
        ],
        PenAppearance::new("#ff0000", 1., 1.).unwrap(),
    )
    .unwrap();
    pens.push(created.clone());
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    let snapshot = AnnotationSnapshot {
        revision: 2,
        saved_revision: 0,
        dirty: true,
        selected_id: Some(created.id.clone()),
        annotation_order: Vec::new(),
        rectangles: source_session.rectangles().to_vec(),
        ellipses: source_session.ellipses().to_vec(),
        arcs: Vec::new(),
        redacts: Vec::new(),
        pens: pens.clone(),
        straight_lines: Vec::new(),
        vertex_paths: source_session.vertex_paths().to_vec(),

        clouds: source_session.clouds().to_vec(),
        cloud_pluses: Vec::new(),
        dimensions: Vec::new(),
        callouts: source_session.callouts().to_vec(),
        measurement_paths: source_session.measurement_paths().to_vec(),
        text_boxes: source_session.text_boxes().to_vec(),
        lengths: source_session.lengths().to_vec(),
        images: source_session.images().to_vec(),
        snapshots: source_session.snapshots().to_vec(),
        page_scales: Vec::new(),
        scale_presets: Vec::new(),
        page_length_calibrations: Vec::new(),
        page_rotations: Vec::new(),
        undo_depth: 2,
        redo_depth: 0,
    };
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(78),
            generation: 3,
            source_path: source.clone(),
            destination: save_as_destination(&source, &edited_target),
            current_page: 0,
            annotation_revision: snapshot.revision,
            annotations: snapshot.clone(),
            expected_source_sha256: None,
        })
        .expect("Pen create and edit must survive typed Save As validation");
    let edited = PdfPersistenceSession::open(&edited_target).unwrap();
    assert_eq!(edited.pens(), pens.as_slice());

    let deleted_id = edited.pens()[0].id.clone();
    let remaining = edited
        .pens()
        .iter()
        .filter(|pen| pen.id != deleted_id)
        .cloned()
        .collect::<Vec<_>>();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(78),
            generation: 4,
            source_path: edited_target.clone(),
            destination: save_as_destination(&edited_target, &deleted_target),
            current_page: 0,
            annotation_revision: 3,
            annotations: AnnotationSnapshot {
                revision: 3,
                saved_revision: 2,
                dirty: true,
                selected_id: Some(created.id.clone()),
                annotation_order: Vec::new(),
                rectangles: edited.rectangles().to_vec(),
                ellipses: edited.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: remaining.clone(),
                straight_lines: Vec::new(),
                vertex_paths: edited.vertex_paths().to_vec(),

                clouds: edited.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: edited.callouts().to_vec(),
                measurement_paths: edited.measurement_paths().to_vec(),
                text_boxes: edited.text_boxes().to_vec(),
                lengths: edited.lengths().to_vec(),
                images: edited.images().to_vec(),
                snapshots: edited.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 3,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Pen deletion must survive typed Save As validation");
    let deleted = PdfPersistenceSession::open(&deleted_target).unwrap();
    assert_eq!(deleted.pens(), remaining.as_slice());
    assert!(!deleted.has_raw_annotation_name(&deleted_id));
    std::fs::remove_file(edited_target).unwrap();
    std::fs::remove_file(deleted_target).unwrap();
}

#[test]
fn highlight_save_as_uses_canonical_identity_and_reconciles_create_edit_delete() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let created_target = manifest_dir.join(format!(
        ".prepared/highlight-create-save-{}.pdf",
        std::process::id()
    ));
    let edited_target = manifest_dir.join(format!(
        ".prepared/highlight-edit-save-{}.pdf",
        std::process::id()
    ));
    let deleted_target = manifest_dir.join(format!(
        ".prepared/highlight-delete-save-{}.pdf",
        std::process::id()
    ));
    assert!(!created_target.exists() && !edited_target.exists() && !deleted_target.exists());

    let source_session = PdfPersistenceSession::open(&source).unwrap();
    let highlight_id = MarkupId::new("workspace:highlight:persistence-1").unwrap();
    let canonical_native_id =
        MarkupId::new(format!("bp:{highlight_id}")).expect("canonical PDF identity is valid");
    let created = PenAnnotation::new_highlight(
        highlight_id.clone(),
        0,
        vec![
            PdfPoint::new(72., 96.).unwrap(),
            PdfPoint::new(108., 96.5).unwrap(),
            PdfPoint::new(144., 97.).unwrap(),
        ],
        PenAppearance::new("#ffff00", 12., 1.).unwrap(),
    )
    .unwrap();
    let mut created_pens = source_session.pens().to_vec();
    created_pens.push(created.clone());
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(80),
            generation: 1,
            source_path: source.clone(),
            destination: save_as_destination(&source, &created_target),
            current_page: 0,
            annotation_revision: 1,
            annotations: AnnotationSnapshot {
                revision: 1,
                saved_revision: 0,
                dirty: true,
                selected_id: Some(highlight_id.clone()),
                annotation_order: Vec::new(),
                rectangles: source_session.rectangles().to_vec(),
                ellipses: source_session.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: created_pens,
                straight_lines: Vec::new(),
                vertex_paths: source_session.vertex_paths().to_vec(),

                clouds: source_session.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: source_session.callouts().to_vec(),
                measurement_paths: source_session.measurement_paths().to_vec(),
                text_boxes: source_session.text_boxes().to_vec(),
                lengths: source_session.lengths().to_vec(),
                images: source_session.images().to_vec(),
                snapshots: source_session.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 1,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Highlight creation must survive typed Save As validation");

    let created_reopen = PdfPersistenceSession::open(&created_target).unwrap();
    let reopened_highlight = created_reopen
        .pens()
        .iter()
        .find(|pen| pen.id == highlight_id)
        .expect("the Highlight must reopen by its application-owned stable identity");
    assert_eq!(reopened_highlight, &created);
    assert_eq!(reopened_highlight.tool(), InkTool::Highlight);
    assert_eq!(reopened_highlight.blend_mode(), BlendMode::Multiply);
    assert!(!reopened_highlight.smooth_curves);
    assert!(created_reopen.has_raw_annotation_name(&canonical_native_id));
    assert!(!created_reopen.has_raw_annotation_name(&highlight_id));

    let mut edited_pens = created_reopen.pens().to_vec();
    let edited_highlight = edited_pens
        .iter_mut()
        .find(|pen| pen.id == highlight_id)
        .unwrap();
    edited_highlight.appearance = PenAppearance::new("#00ff00", 10., 0.75).unwrap();
    let expected_edit = edited_highlight.clone();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(80),
            generation: 2,
            source_path: created_target.clone(),
            destination: save_as_destination(&created_target, &edited_target),
            current_page: 0,
            annotation_revision: 2,
            annotations: AnnotationSnapshot {
                revision: 2,
                saved_revision: 1,
                dirty: true,
                selected_id: Some(highlight_id.clone()),
                annotation_order: Vec::new(),
                rectangles: created_reopen.rectangles().to_vec(),
                ellipses: created_reopen.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: edited_pens,
                straight_lines: Vec::new(),
                vertex_paths: created_reopen.vertex_paths().to_vec(),

                clouds: created_reopen.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: created_reopen.callouts().to_vec(),
                measurement_paths: created_reopen.measurement_paths().to_vec(),
                text_boxes: created_reopen.text_boxes().to_vec(),
                lengths: created_reopen.lengths().to_vec(),
                images: created_reopen.images().to_vec(),
                snapshots: created_reopen.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 2,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Highlight edit must survive typed Save As validation");
    let edited_reopen = PdfPersistenceSession::open(&edited_target).unwrap();
    assert_eq!(
        edited_reopen
            .pens()
            .iter()
            .find(|pen| pen.id == highlight_id),
        Some(&expected_edit)
    );
    assert!(edited_reopen.has_raw_annotation_name(&canonical_native_id));

    let remaining = edited_reopen
        .pens()
        .iter()
        .filter(|pen| pen.id != highlight_id)
        .cloned()
        .collect::<Vec<_>>();
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(80),
            generation: 3,
            source_path: edited_target.clone(),
            destination: save_as_destination(&edited_target, &deleted_target),
            current_page: 0,
            annotation_revision: 3,
            annotations: AnnotationSnapshot {
                revision: 3,
                saved_revision: 2,
                dirty: true,
                selected_id: None,
                annotation_order: Vec::new(),
                rectangles: edited_reopen.rectangles().to_vec(),
                ellipses: edited_reopen.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: remaining.clone(),
                straight_lines: Vec::new(),
                vertex_paths: edited_reopen.vertex_paths().to_vec(),

                clouds: edited_reopen.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: edited_reopen.callouts().to_vec(),
                measurement_paths: edited_reopen.measurement_paths().to_vec(),
                text_boxes: edited_reopen.text_boxes().to_vec(),
                lengths: edited_reopen.lengths().to_vec(),
                images: edited_reopen.images().to_vec(),
                snapshots: edited_reopen.snapshots().to_vec(),
                page_scales: Vec::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: Vec::new(),
                page_rotations: Vec::new(),
                undo_depth: 3,
                redo_depth: 0,
            },
            expected_source_sha256: None,
        })
        .expect("Highlight deletion must survive typed Save As validation");
    let deleted_reopen = PdfPersistenceSession::open(&deleted_target).unwrap();
    assert_eq!(deleted_reopen.pens(), remaining.as_slice());
    assert!(!deleted_reopen.has_raw_annotation_name(&canonical_native_id));

    std::fs::remove_file(created_target).unwrap();
    std::fs::remove_file(edited_target).unwrap();
    std::fs::remove_file(deleted_target).unwrap();
}

#[test]
fn save_as_refuses_source_identity_drift_before_writing_a_target() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reviewed =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let source = manifest_dir.join(format!(".prepared/source-drift-{}.pdf", std::process::id()));
    let target = manifest_dir.join(format!(
        ".prepared/source-drift-target-{}.pdf",
        std::process::id()
    ));
    std::fs::copy(&reviewed, &source).unwrap();
    let persistence = PdfPersistenceSession::open(&source).unwrap();
    let expected: [u8; 32] = Sha256::digest(std::fs::read(&source).unwrap()).into();
    let mut bytes = std::fs::read(&source).unwrap();
    bytes.extend_from_slice(b"\n% deterministic external source drift\n");
    std::fs::write(&source, bytes).unwrap();
    let saver = PdfDocumentSaver::new(Arc::new(RejectingOpener));
    let result = saver.save(&SaveDocumentRequest {
        document_id: DocumentId::new(79),
        generation: 1,
        source_path: source.clone(),
        destination: save_as_destination(&source, &target),
        current_page: 0,
        annotation_revision: 0,
        annotations: AnnotationSnapshot {
            revision: 0,
            saved_revision: 0,
            dirty: false,
            selected_id: None,
            annotation_order: Vec::new(),
            rectangles: persistence.rectangles().to_vec(),
            ellipses: persistence.ellipses().to_vec(),
            arcs: Vec::new(),
            redacts: Vec::new(),
            pens: persistence.pens().to_vec(),
            straight_lines: Vec::new(),
            vertex_paths: persistence.vertex_paths().to_vec(),

            clouds: persistence.clouds().to_vec(),
            cloud_pluses: Vec::new(),
            dimensions: Vec::new(),
            callouts: persistence.callouts().to_vec(),
            measurement_paths: persistence.measurement_paths().to_vec(),
            text_boxes: persistence.text_boxes().to_vec(),
            lengths: persistence.lengths().to_vec(),
            images: persistence.images().to_vec(),
            snapshots: persistence.snapshots().to_vec(),
            page_scales: Vec::new(),
            scale_presets: Vec::new(),
            page_length_calibrations: Vec::new(),
            page_rotations: Vec::new(),
            undo_depth: 0,
            redo_depth: 0,
        },
        expected_source_sha256: Some(expected),
    });
    let error = match result {
        Ok(_) => panic!("source identity drift must refuse Save As"),
        Err(error) => error,
    };
    assert!(error.contains("source PDF changed"));
    assert!(!target.exists());
    std::fs::remove_file(source).unwrap();
}

#[test]
fn in_place_save_atomically_replaces_the_regular_source_preserves_mode_and_removes_staging() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reviewed =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    let source = manifest_dir.join(format!(
        ".prepared/in-place-save-unit-{}.pdf",
        std::process::id()
    ));
    let original_inode = manifest_dir.join(format!(
        ".prepared/in-place-save-unit-original-{}.pdf",
        std::process::id()
    ));
    std::fs::copy(&reviewed, &source).unwrap();
    std::fs::hard_link(&source, &original_inode).unwrap();
    let _scratch_files = ScratchFiles(vec![source.clone(), original_inode.clone()]);
    #[cfg(unix)]
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o640)).unwrap();
    let original_bytes = std::fs::read(&source).unwrap();
    let expected: [u8; 32] = Sha256::digest(&original_bytes).into();
    let persistence = PdfPersistenceSession::open(&source).unwrap();
    let rectangle = RectangleAnnotation {
        id: MarkupId::new("workspace:rectangle:in-place-unit").unwrap(),
        page_index: 0,
        rect: PdfRect::new(72., 96., 144., 96.).unwrap(),
        rotation_degrees: 15.,
        appearance: RectangleAppearance::new("#dc2626", 3., Some("#abcdef"), 0.88)
            .unwrap()
            .with_fill_opacity(31. / 255.)
            .unwrap()
            .with_stroke_style(StrokeStyle::Dotted),
        locked: true,
    };
    let saver = PdfDocumentSaver::new(Arc::new(SuccessfulOpener));
    saver
        .save(&SaveDocumentRequest {
            document_id: DocumentId::new(8_001),
            generation: 1,
            source_path: source.clone(),
            destination: SaveDestination::OpenedSource,
            current_page: 0,
            annotation_revision: 1,
            annotations: AnnotationSnapshot {
                revision: 1,
                saved_revision: 0,
                dirty: true,
                selected_id: Some(rectangle.id.clone()),
                annotation_order: vec![rectangle.id.clone()],
                rectangles: vec![rectangle.clone()],
                ellipses: persistence.ellipses().to_vec(),
                arcs: Vec::new(),
                redacts: Vec::new(),
                pens: persistence.pens().to_vec(),
                straight_lines: persistence.straight_lines().to_vec(),
                vertex_paths: persistence.vertex_paths().to_vec(),

                clouds: persistence.clouds().to_vec(),
                cloud_pluses: Vec::new(),
                dimensions: Vec::new(),
                callouts: persistence.callouts().to_vec(),
                measurement_paths: persistence.measurement_paths().to_vec(),
                text_boxes: persistence.text_boxes().to_vec(),
                lengths: persistence.lengths().to_vec(),
                images: persistence.images().to_vec(),
                snapshots: persistence.snapshots().to_vec(),
                page_scales: persistence.page_scales().to_vec(),
                scale_presets: Vec::new(),
                page_length_calibrations: persistence
                    .page_length_calibrations()
                    .iter()
                    .map(|(page, calibration)| (*page, calibration.clone()))
                    .collect(),
                page_rotations: persistence
                    .page_rotations()
                    .iter()
                    .map(|(page, rotation)| (*page, *rotation))
                    .collect(),
                undo_depth: 1,
                redo_depth: 0,
            },
            expected_source_sha256: Some(expected),
        })
        .expect("in-place Save must replace the validated opened PDF");

    let replaced_bytes = std::fs::read(&source).unwrap();
    assert_ne!(replaced_bytes, original_bytes);
    assert_eq!(
        std::fs::read(&original_inode).unwrap(),
        original_bytes,
        "the old hard-linked inode must retain the pre-save bytes, proving replacement rather than truncation"
    );
    let reopened = PdfPersistenceSession::open(&source).unwrap();
    assert!(
        reopened
            .rectangles()
            .iter()
            .any(|candidate| candidate.same_persisted_state_as(&rectangle))
    );
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let staging_prefix = format!(
        ".{}.butter-paper-",
        source.file_name().unwrap().to_string_lossy()
    );
    assert!(
        std::fs::read_dir(source.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(&staging_prefix)),
        "successful in-place Save must remove every same-directory staging file"
    );
}

#[test]
fn in_place_save_reports_a_durable_publication_receipt_after_parent_sync() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reviewed =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    let source = manifest_dir.join(format!(
        ".prepared/in-place-save-receipt-{}.pdf",
        std::process::id()
    ));
    std::fs::copy(&reviewed, &source).unwrap();
    let _scratch_files = ScratchFiles(vec![source.clone()]);
    let expected: [u8; 32] = Sha256::digest(std::fs::read(&source).unwrap()).into();
    let persistence = PdfPersistenceSession::open_for_update(&source, expected).unwrap();
    let outcome = persistence
        .prepare_save_replacing(&source)
        .unwrap()
        .publish_replacing()
        .unwrap();

    assert_eq!(outcome, PdfPublicationOutcome::Durable);
}

#[cfg(unix)]
#[test]
fn in_place_save_refuses_a_final_symlink_source_without_writing_or_staging() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reviewed =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    let source_link = manifest_dir.join(format!(
        ".prepared/in-place-save-symlink-{}.pdf",
        std::process::id()
    ));
    std::os::unix::fs::symlink(&reviewed, &source_link).unwrap();
    let _scratch_files = ScratchFiles(vec![source_link.clone()]);
    let expected: [u8; 32] = Sha256::digest(std::fs::read(&reviewed).unwrap()).into();
    let result = PdfDocumentSaver::new(Arc::new(SuccessfulOpener)).save(&SaveDocumentRequest {
        document_id: DocumentId::new(8_002),
        generation: 1,
        source_path: source_link.clone(),
        destination: SaveDestination::OpenedSource,
        current_page: 0,
        annotation_revision: 0,
        annotations: AnnotationSnapshot {
            revision: 0,
            saved_revision: 0,
            dirty: false,
            selected_id: None,
            annotation_order: Vec::new(),
            rectangles: Vec::new(),
            ellipses: Vec::new(),
            arcs: Vec::new(),
            redacts: Vec::new(),
            pens: Vec::new(),
            straight_lines: Vec::new(),
            vertex_paths: Vec::new(),

            clouds: Vec::new(),
            cloud_pluses: Vec::new(),
            dimensions: Vec::new(),
            callouts: Vec::new(),
            measurement_paths: Vec::new(),
            text_boxes: Vec::new(),
            lengths: Vec::new(),
            images: Vec::new(),
            snapshots: Vec::new(),
            page_scales: Vec::new(),
            scale_presets: Vec::new(),
            page_length_calibrations: Vec::new(),
            page_rotations: Vec::new(),
            undo_depth: 0,
            redo_depth: 0,
        },
        expected_source_sha256: Some(expected),
    });
    let error = match result {
        Ok(_) => panic!("in-place Save must reject a final symlink source"),
        Err(error) => error,
    };
    assert!(error.contains("canonical"));
    assert!(
        std::fs::symlink_metadata(&source_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let staging_prefix = format!(
        ".{}.butter-paper-",
        source_link.file_name().unwrap().to_string_lossy()
    );
    assert!(
        std::fs::read_dir(source_link.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(&staging_prefix))
    );
}

#[test]
fn in_place_save_rejects_source_drift_after_prepare_and_removes_the_exact_stage() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let reviewed =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    let source = manifest_dir.join(format!(
        ".prepared/in-place-save-publish-drift-{}.pdf",
        std::process::id()
    ));
    std::fs::copy(&reviewed, &source).unwrap();
    let _scratch_files = ScratchFiles(vec![source.clone()]);
    let expected: [u8; 32] = Sha256::digest(std::fs::read(&source).unwrap()).into();
    let persistence = PdfPersistenceSession::open_for_update(&source, expected).unwrap();
    let prepared = persistence.prepare_save_replacing(&source).unwrap();
    let stage = prepared.path().to_path_buf();
    assert_eq!(stage.parent(), source.parent());
    let mut changed = std::fs::read(&source).unwrap();
    changed.extend_from_slice(b"\n% deterministic external change before publish\n");
    std::fs::write(&source, &changed).unwrap();

    assert!(
        prepared
            .publish_replacing()
            .unwrap_err()
            .to_string()
            .contains("source PDF changed")
    );
    assert_eq!(std::fs::read(&source).unwrap(), changed);
    assert!(
        !stage.exists(),
        "publish refusal must remove its exact stage"
    );
}

#[test]
fn save_as_does_not_publish_a_target_when_native_reopen_validation_fails() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf");
    let target = manifest_dir.join(format!(
        ".prepared/rejected-native-reopen-{}.pdf",
        std::process::id()
    ));
    assert!(!target.exists());
    let persistence = PdfPersistenceSession::open(&source).unwrap();
    let saver = PdfDocumentSaver::new(Arc::new(RejectingOpener));
    let result = saver.save(&SaveDocumentRequest {
        document_id: DocumentId::new(81),
        generation: 1,
        source_path: source.clone(),
        destination: save_as_destination(&source, &target),
        current_page: 0,
        annotation_revision: 0,
        annotations: AnnotationSnapshot {
            revision: 0,
            saved_revision: 0,
            dirty: false,
            selected_id: None,
            annotation_order: Vec::new(),
            rectangles: persistence.rectangles().to_vec(),
            ellipses: persistence.ellipses().to_vec(),
            arcs: Vec::new(),
            redacts: Vec::new(),
            pens: persistence.pens().to_vec(),
            straight_lines: Vec::new(),
            vertex_paths: persistence.vertex_paths().to_vec(),

            clouds: persistence.clouds().to_vec(),
            cloud_pluses: Vec::new(),
            dimensions: Vec::new(),
            callouts: persistence.callouts().to_vec(),
            measurement_paths: persistence.measurement_paths().to_vec(),
            text_boxes: persistence.text_boxes().to_vec(),
            lengths: persistence.lengths().to_vec(),
            images: persistence.images().to_vec(),
            snapshots: persistence.snapshots().to_vec(),
            page_scales: Vec::new(),
            scale_presets: Vec::new(),
            page_length_calibrations: Vec::new(),
            page_rotations: Vec::new(),
            undo_depth: 0,
            redo_depth: 0,
        },
        expected_source_sha256: None,
    });
    let error = match result {
        Ok(_) => panic!("a rejected native reopen must fail Save As"),
        Err(error) => error,
    };
    assert_eq!(error, "the unsupported-family save must fail before reopen");
    assert!(
        !target.exists(),
        "a rejected native reopen must never publish the user-visible target"
    );
    let target_name = target.file_name().unwrap().to_string_lossy();
    assert!(
        std::fs::read_dir(target.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .contains(target_name.as_ref())),
        "the rejected staging file must be removed"
    );
}

#[gpui::test]
fn generated_template_requires_save_as_and_releases_its_owned_source_after_reopen(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run_key = format!("generated-workspace-{}", std::process::id());
    let store_root = manifest_dir.join(".prepared").join(&run_key);
    let saved_path = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-saved.pdf"));
    let _ = std::fs::remove_dir_all(&store_root);
    let _ = std::fs::remove_file(&saved_path);
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    let opener = Arc::new(PathRecordingOpener::default());

    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(opener.clone(), cx));
    let document_id = workspace
        .update(cx, |workspace, cx| {
            workspace.create_generated_document(
                store.clone(),
                GeneratedDocumentRequest {
                    title: "Untitled".into(),
                    width_mm: 420.,
                    height_mm: 297.,
                    pattern: Some(GeneratedPattern::SquareGrid {
                        spacing_mm: 10.,
                        color: "#d1d5db".into(),
                    }),
                },
                cx,
            )
        })
        .expect("the owned template source must be created before opening");
    cx.run_until_parked();

    let source_path = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .path()
            .to_owned()
    });
    assert_eq!(source_path.file_name().unwrap(), "Untitled.pdf");
    assert!(source_path.exists());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.document_requires_save_as(document_id, cx)
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(document_id, cx)
        }),
        Some(0),
        "an unannotated generated document is still dirty until Save As succeeds"
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.resolve_dirty_close_cancel(cx)),
        DirtyCloseResolution::Cancelled
    );
    assert!(
        source_path.exists(),
        "Cancel must preserve the owned source"
    );

    let failed_path = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-missing-parent"))
        .join("failed.pdf");
    let failed_save_error = workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, failed_path, cx)
        })
        .expect_err("Save As must reject a missing parent before dispatching work");
    assert!(failed_save_error.contains("selected parent cannot be inspected"));
    assert!(
        source_path.exists(),
        "failed Save As must preserve the source"
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(document_id, cx)
        }),
        Some(0)
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .unwrap();
    cx.run_until_parked();

    let save_status = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .save_status()
            .clone()
    });
    assert_eq!(
        save_status,
        butter_paper_gpui_component_compat::document_workspace::NativeDocumentSaveStatus::Idle,
        "the generated PDF must pass persistence and validated reopen"
    );
    let opened = opener.opened.lock().unwrap();
    assert_eq!(
        opened.len(),
        2,
        "create and validated reopen must each open once"
    );
    assert_eq!(opened[0].0, source_path);
    assert!(opened[0].1.load(Ordering::Acquire));
    assert_eq!(opened[1].0.parent(), saved_path.parent());
    let reopened_stage_name = opened[1].0.file_name().unwrap().to_string_lossy();
    assert!(
        reopened_stage_name.starts_with(".butter-paper-") && reopened_stage_name.ends_with(".tmp"),
        "validated reopen must use the authority-owned staging file before atomic publication"
    );
    assert!(
        !opened[1].0.exists(),
        "publication must consume the staging path"
    );
    assert!(!opened[1].1.load(Ordering::Acquire));
    let saved_resource = opened[1].1.clone();
    drop(opened);
    assert!(saved_path.exists());
    assert!(!source_path.exists());
    assert!(!workspace.read_with(cx, |workspace, cx| {
        workspace.document_requires_save_as(document_id, cx)
    }));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(document_id, cx)
        }),
        None
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed
    );
    assert!(saved_resource.load(Ordering::Acquire));
    store.remove_if_empty().unwrap();
    assert!(!store_root.exists());
    std::fs::remove_file(saved_path).unwrap();
}

#[gpui::test]
fn generated_template_failed_open_preserves_source_until_explicit_discard(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run_key = format!("generated-open-failure-{}", std::process::id());
    let store_root = manifest_dir.join(".prepared").join(&run_key);
    let _ = std::fs::remove_dir_all(&store_root);
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(RejectingOpener), cx));
    let document_id = workspace
        .update(cx, |workspace, cx| {
            workspace.create_generated_document(
                store.clone(),
                GeneratedDocumentRequest::a3_landscape_blank(),
                cx,
            )
        })
        .unwrap();
    cx.run_until_parked();
    let source_path = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .path()
            .to_owned()
    });
    assert!(source_path.exists());
    assert!(workspace.read_with(cx, |workspace, cx| matches!(
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .status(),
        NativeDocumentStatus::Failed(_)
    )));
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        DirtyCloseResolution::Discarded
    );
    assert!(!source_path.exists());
    store.remove_if_empty().unwrap();
    assert!(!store_root.exists());
}

#[gpui::test]
fn imported_template_library_restart_materializes_an_independent_dirty_workspace_document(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run_key = format!("template-library-workspace-{}", std::process::id());
    let library_root = manifest_dir.join(".prepared").join(format!("{run_key}-library"));
    let store_root = manifest_dir.join(".prepared").join(format!("{run_key}-documents"));
    let invalid_source = manifest_dir.join(".prepared").join(format!("{run_key}-invalid.pdf"));
    let _scratch_directories = ScratchDirectories(vec![library_root.clone(), store_root.clone()]);
    let _scratch_files = ScratchFiles(vec![invalid_source.clone()]);
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the checksum-controlled multi-page fixture must exist");
    let imported_id = "imported-00000000-0000-4000-8000-000000000000";

    let mut library = PersistentTemplateManager::open(library_root.clone()).unwrap();
    library
        .import_pdf(
            imported_id,
            "Site Form.pdf",
            "2026-08-27T00:00:00.000Z",
            &fixture,
        )
        .unwrap();
    library.select(imported_id).unwrap();
    drop(library);

    let mut library = TemplateLibrary::open(library_root).unwrap();
    assert_eq!(library.last_template_id(), imported_id);
    let managed_source = library.managed_source_path(imported_id).unwrap();
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    let owned = library
        .materialize(imported_id, "imported-template-session", &store)
        .unwrap();
    let temporary_source = owned.path().to_owned();
    assert_ne!(temporary_source, managed_source);

    cx.update(gpui_component::init);
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener(Arc::new(PathRecordingOpener::default()), cx));
    let document_id = workspace
        .update(cx, |workspace, cx| {
            workspace.create_owned_template_document(store.clone(), owned, cx)
        })
        .unwrap();
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |workspace, cx| matches!(
        workspace.session(document_id, cx).unwrap().read(cx).status(),
        NativeDocumentStatus::Ready
    )));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .document_dirty_revision(document_id, cx)),
        Some(0)
    );

    std::fs::write(&invalid_source, b"not a PDF").unwrap();
    assert!(
        library
            .import_pdf(
                "imported-11111111-1111-4111-8111-111111111111",
                "Broken",
                "2026-08-27T00:00:01.000Z",
                &invalid_source,
            )
            .is_err()
    );
    assert_eq!(library.last_template_id(), imported_id);
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1
    );
    assert!(temporary_source.exists());

    library.remove(imported_id).unwrap();
    assert_eq!(library.last_template_id(), BUILT_IN_BLANK_ID);
    assert!(!managed_source.exists());
    assert!(
        temporary_source.exists(),
        "the open document owns an independent temporary copy"
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .request_close_document(document_id, cx)),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        DirtyCloseResolution::Discarded
    );
    assert!(!temporary_source.exists());
    store.remove_if_empty().unwrap();
    assert!(!store_root.exists());
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_imported_template_library_renders_saves_reopens_and_releases(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let pdfium = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the checksum-controlled multi-page fixture must exist");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        "517ebc78ee84071ce15040da05f2155ca0fe4b5d5871dc95cea1a95c97b1f57b"
    );
    assert!(worker.is_file());
    assert!(pdfium.is_file());

    let run_key = format!("real-template-library-{}", std::process::id());
    let library_root = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-library"));
    let store_root = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-documents"));
    let surface_root = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-surfaces"));
    let saved_path = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-saved.pdf"));
    let invalid_source = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-invalid.pdf"));
    let _scratch_directories = ScratchDirectories(vec![
        library_root.clone(),
        store_root.clone(),
        surface_root.clone(),
    ]);
    let _scratch_files = ScratchFiles(vec![saved_path.clone(), invalid_source.clone()]);
    let imported_id = "imported-00000000-0000-4000-8000-000000000000";
    let mut library = PersistentTemplateManager::open(library_root.clone()).unwrap();
    library
        .import_pdf(
            imported_id,
            "100 Page Site Form.pdf",
            "2026-08-27T00:00:00.000Z",
            &fixture,
        )
        .unwrap();
    library.select(imported_id).unwrap();
    drop(library);
    let mut library = PersistentTemplateManager::open(library_root).unwrap();
    let managed_source = library.managed_source_path(imported_id).unwrap();
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    let owned = library
        .materialize(imported_id, "real-imported-template", &store)
        .unwrap();
    let temporary_source = owned.path().to_owned();
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        pdfium,
        surface_root.clone(),
    ));

    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let document_id = workspace
        .update(cx, |workspace, cx| {
            workspace.create_owned_template_document(store.clone(), owned, cx)
        })
        .unwrap();
    cx.run_until_parked();
    let first_worker_pid = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert!(matches!(session.status(), NativeDocumentStatus::Ready));
        assert_eq!(session.page_count(), 100);
        assert!(
            session
                .current_base_raster()
                .unwrap()
                .has_spatial_variation()
        );
        assert!(
            session
                .thumbnail_base_raster(0)
                .unwrap()
                .has_spatial_variation()
        );
        session.worker_pid().unwrap()
    });
    assert!(managed_source.exists());
    assert!(temporary_source.exists());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .document_dirty_revision(document_id, cx)),
        Some(0)
    );

    std::fs::write(&invalid_source, b"not a PDF").unwrap();
    assert!(
        library
            .import_pdf(
                "imported-11111111-1111-4111-8111-111111111111",
                "Broken",
                "2026-08-27T00:00:01.000Z",
                &invalid_source,
            )
            .is_err()
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .unwrap()
                .read(cx)
                .current_page()
        }),
        0
    );

    library.remove(imported_id).unwrap();
    assert_eq!(
        library.snapshot().unwrap().last_used_id(),
        BUILT_IN_BLANK_ID
    );
    assert!(!managed_source.exists());
    assert!(temporary_source.exists());
    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .unwrap();
    cx.run_until_parked();
    let reopened_worker_pid = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert!(matches!(session.status(), NativeDocumentStatus::Ready));
        assert_eq!(session.path(), saved_path.as_path());
        assert_eq!(session.page_count(), 100);
        assert!(
            session
                .current_base_raster()
                .unwrap()
                .has_spatial_variation()
        );
        session.worker_pid().unwrap()
    });
    assert_ne!(first_worker_pid, reopened_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{first_worker_pid}")).exists());
    assert!(!temporary_source.exists());
    assert!(saved_path.exists());
    assert_eq!(
        PdfPersistenceSession::open(&saved_path)
            .unwrap()
            .page_count(),
        100
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .request_close_document(document_id, cx)),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    store.remove_if_empty().unwrap();
    assert!(!store_root.exists());
}

#[gpui::test]
fn generated_template_command_suppresses_a_duplicate_while_open_is_pending(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run_key = format!("generated-template-command-{}", std::process::id());
    let store_root = manifest_dir.join(".prepared").join(&run_key);
    let _ = std::fs::remove_dir_all(&store_root);
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    let opener = Arc::new(PathRecordingOpener::default());

    cx.update(gpui_component::init);
    let workspace =
        cx.new(|cx| DocumentWorkspace::with_opener_and_generated_store(opener, store.clone(), cx));
    let (first, second) = workspace.update(cx, |workspace, cx| {
        let first = workspace.request_generated_template("built-in-grid", cx);
        let second = workspace.request_generated_template("built-in-blank", cx);
        (first, second)
    });
    let document_id = match first {
        GeneratedTemplateRequestDisposition::Started(document_id) => document_id,
        other => panic!("the first template command must start: {other:?}"),
    };
    assert_eq!(
        second,
        GeneratedTemplateRequestDisposition::SuppressedPending
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1
    );
    assert!(workspace.read_with(cx, |workspace, _| workspace.is_template_creation_pending()));

    cx.run_until_parked();
    assert!(!workspace.read_with(cx, |workspace, _| workspace.is_template_creation_pending()));
    assert!(workspace.read_with(cx, |workspace, cx| matches!(
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .status(),
        NativeDocumentStatus::Ready
    )));
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        DirtyCloseResolution::Discarded
    );
    store.remove_if_empty().unwrap();
    assert!(!store_root.exists());
}

#[gpui::test]
fn generated_template_split_creates_one_real_workspace_session_without_mock_tab_state(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let run_key = format!("generated-template-split-{}", std::process::id());
    let store_root = manifest_dir.join(".prepared").join(&run_key);
    let _ = std::fs::remove_dir_all(&store_root);
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    let opener = Arc::new(PathRecordingOpener::default());
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));

    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let store = store.clone();
        move |window, cx| {
            let workspace =
                cx.new(|cx| DocumentWorkspace::with_opener_and_generated_store(opener, store, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_SESSION_TABS_ID).is_some());
    assert!(cx.debug_bounds(TEMPLATE_CONTROL_GROUP_ID).is_some());

    let picker = cx.debug_bounds(TEMPLATE_PICKER_ID).unwrap();
    cx.simulate_click(picker.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let grid = cx.debug_bounds(TEMPLATE_ITEM_IDS[2]).unwrap();
    cx.simulate_click(grid.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let create = cx.debug_bounds(TEMPLATE_CREATE_ID).unwrap();
    cx.simulate_click(create.center(), Modifiers::default());

    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.sessions().len()),
        1,
        "the template intent must create a real session immediately"
    );
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(!workspace.read_with(cx, |workspace, _| workspace.is_template_creation_pending()));
    assert!(
        cx.debug_bounds("document-tab-template-document-1")
            .is_none(),
        "the real workspace must not render the legacy mock template tab"
    );
    let document_id = workspace
        .read_with(cx, |workspace, _| workspace.active_document_id())
        .unwrap();
    let source_path = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert!(matches!(session.status(), NativeDocumentStatus::Ready));
        assert_eq!(session.title(), "Untitled.pdf");
        session.path().to_owned()
    });
    let source = std::fs::read_to_string(&source_path).unwrap();
    assert!(source.contains("butter-paper:page-grid"));
    assert!(source.contains(r#""type":"rectangular""#));
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.document_requires_save_as(document_id, cx)
    }));

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        DirtyCloseResolution::Discarded
    );
    store.remove_if_empty().unwrap();
    assert!(!store_root.exists());
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_generated_template_creates_opens_saves_reopens_and_releases(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    assert!(worker.is_file());
    assert!(library.is_file());
    let run_key = format!("real-generated-template-{}", std::process::id());
    let store_root = manifest_dir.join(".prepared").join(&run_key);
    let template_library_root = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-library"));
    let surface_root = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-surfaces"));
    let saved_path = manifest_dir
        .join(".prepared")
        .join(format!("{run_key}-saved.pdf"));
    let _ = std::fs::remove_dir_all(&store_root);
    let _ = std::fs::remove_dir_all(&template_library_root);
    let _ = std::fs::remove_dir_all(&surface_root);
    let _ = std::fs::remove_file(&saved_path);
    let store = GeneratedDocumentStore::new(store_root.clone()).unwrap();
    let mut template_manager =
        PersistentTemplateManager::open(template_library_root.clone()).unwrap();
    template_manager
        .save_generated(
            "custom-real-grid",
            "Real custom grid",
            GeneratedDocumentRequest {
                title: "Untitled".into(),
                width_mm: 420.,
                height_mm: 297.,
                pattern: Some(GeneratedPattern::SquareGrid {
                    spacing_mm: 10.,
                    color: "#d1d5db".into(),
                }),
            },
        )
        .unwrap();
    let owned = template_manager
        .materialize("custom-real-grid", "real-custom-grid", &store)
        .unwrap();
    assert_eq!(
        template_manager.snapshot().unwrap().last_used_id(),
        "custom-real-grid"
    );
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(gpui_component::init);
    let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let document_id = workspace
        .update(cx, |workspace, cx| {
            workspace.create_owned_template_document(store.clone(), owned, cx)
        })
        .unwrap();
    cx.run_until_parked();
    let (source_path, source_worker_pid) = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert!(matches!(session.status(), NativeDocumentStatus::Ready));
        assert!(
            session
                .current_base_raster()
                .unwrap()
                .has_spatial_variation()
        );
        assert!(
            session
                .thumbnail_base_raster(0)
                .unwrap()
                .has_spatial_variation()
        );
        (session.path().to_owned(), session.worker_pid().unwrap())
    });
    assert!(source_path.exists());
    assert!(PathBuf::from(format!("/proc/{source_worker_pid}")).exists());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(document_id, cx)
        }),
        Some(0)
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .unwrap();
    cx.run_until_parked();
    let reopened_worker_pid = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert_eq!(session.path(), saved_path.as_path());
        assert!(matches!(
            session.save_status(),
            butter_paper_gpui_component_compat::document_workspace::NativeDocumentSaveStatus::Idle
        ));
        assert!(
            session
                .current_base_raster()
                .unwrap()
                .has_spatial_variation()
        );
        session.worker_pid().unwrap()
    });
    assert_ne!(source_worker_pid, reopened_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{source_worker_pid}")).exists());
    assert!(PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(!source_path.exists());
    assert!(saved_path.exists());
    assert!(PdfPersistenceSession::open(&saved_path).is_ok());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace.document_dirty_revision(document_id, cx)
        }),
        None
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
    store.remove_if_empty().unwrap();
    assert!(!store_root.exists());
    std::fs::remove_file(saved_path).unwrap();
    let _ = std::fs::remove_dir(surface_root);
    drop(template_manager);
    std::fs::remove_dir_all(template_library_root).unwrap();
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_user_unit_coordinate_space_renders_edits_saves_reopens_and_releases(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-coordinate-space-v1.pdf")
        .canonicalize()
        .expect("the checksum-locked coordinate-space fixture must exist");
    assert_eq!(
        format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
        "dc450b09b502f23518ed361986d9a939ed6b9c2dc1fdb6890af30fae4b253a7d"
    );
    assert!(worker.is_file());
    assert!(library.is_file());

    let surface_root = manifest_dir
        .join(".prepared/real-user-unit-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-user-unit-save-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists(), "the Save As target must be new");
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));
    let opened = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(94),
            generation: 1,
            path: fixture.clone(),
        })
        .expect("the real worker must open the CropBox/Rotate/UserUnit fixture");
    let coordinate_space = opened
        .page_coordinate_space(0)
        .expect("the real page must retain its canonical coordinate space");
    assert_eq!(
        coordinate_space.media_box(),
        CoordinateRect::new(0., 0., 360., 240.).unwrap()
    );
    assert_eq!(
        coordinate_space.view_box(),
        CoordinateRect::new(18., 24., 324., 192.).unwrap()
    );
    assert_eq!(coordinate_space.rotation(), CoordinateRotation::Degrees90);
    assert_eq!(coordinate_space.user_unit(), 2.);
    assert_eq!(coordinate_space.display_size_points(), (384., 648.));

    let page = opened.current_page();
    assert_eq!(page.width(), 900);
    assert!(page.has_spatial_variation());
    let page_aspect = f64::from(page.width()) / f64::from(page.height());
    assert!((page_aspect - 384. / 648.).abs() < 0.01);
    let thumbnail = opened
        .thumbnails()
        .first()
        .expect("the real page must produce one thumbnail");
    assert_eq!(thumbnail.page_index, 0);
    assert_eq!(thumbnail.raster.width(), 104);
    assert!(thumbnail.raster.has_spatial_variation());
    let thumbnail_aspect =
        f64::from(thumbnail.raster.width()) / f64::from(thumbnail.raster.height());
    assert!((thumbnail_aspect - page_aspect).abs() < 0.01);

    let worker_pid = opened
        .worker_pid()
        .expect("the worker PID must be observable");
    assert!(PathBuf::from(format!("/proc/{worker_pid}")).exists());
    opened.close().expect("the real UserUnit worker must close");
    assert!(!PathBuf::from(format!("/proc/{worker_pid}")).exists());
    assert!(
        !surface_root.exists()
            || std::fs::read_dir(&surface_root)
                .expect("the mapped-surface root must remain readable")
                .next()
                .is_none(),
        "close must release every real UserUnit mapped surface"
    );

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the rendered UserUnit session must own a worker");

    let tile_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                document_id,
                PageViewMode::SinglePage,
                400.,
                1.,
                800.,
                600.,
                0.,
                0.,
                cx,
            )
        })
        .expect("the UserUnit page must produce a bounded viewport plan");
    assert!(!tile_plan.tiles.is_empty());
    let tile_evidence = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(document_id, &tile_plan, cx)
        })
        .expect("the real worker must render the visible UserUnit tiles");
    assert_eq!(tile_evidence.rendered_tiles, tile_plan.tiles.len());
    assert!(tile_evidence.non_uniform_tiles > 0);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rectangle_tool = cx
        .debug_bounds(DOCUMENT_RECTANGLE_TOOL_ID)
        .expect("the real GPUI Component Rectangle tool must render");
    cx.simulate_click(rectangle_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer_bounds = cx
        .debug_bounds(layer_id)
        .expect("the UserUnit annotation layer must render");
    let display_size = coordinate_space.display_size_points();
    let scale = (f64::from(layer_bounds.size.width) / display_size.0)
        .min(f64::from(layer_bounds.size.height) / display_size.1);
    let page_origin = point(
        layer_bounds.origin.x
            + px(((f64::from(layer_bounds.size.width) - display_size.0 * scale) / 2.) as f32),
        layer_bounds.origin.y
            + px(((f64::from(layer_bounds.size.height) - display_size.1 * scale) / 2.) as f32),
    );
    let transform = PageTransform::from_page_coordinate_space(coordinate_space, scale).unwrap();
    let to_view = |pdf_point: PdfPoint| {
        let local = transform.point_to_local_pixels(pdf_point);
        point(
            page_origin.x + px(local.x as f32),
            page_origin.y + px(local.y as f32),
        )
    };
    let create_start = to_view(PdfPoint::new(72., 60.).unwrap());
    let create_end = to_view(PdfPoint::new(144., 108.).unwrap());
    cx.simulate_mouse_down(create_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(create_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(create_end, MouseButton::Left, Modifiers::default());

    let rectangle_id = MarkupId::new("workspace:rectangle:1").unwrap();
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the UserUnit pointer gesture must create one Rectangle");
    assert_eq!(created.selected_id.as_ref(), Some(&rectangle_id));
    assert_eq!(created.rectangles.len(), 1);
    let expected_rect = PdfRect::new(72., 60., 72., 48.).unwrap();
    for (actual, expected) in [
        (created.rectangles[0].rect.x, expected_rect.x),
        (created.rectangles[0].rect.y, expected_rect.y),
        (created.rectangles[0].rect.width, expected_rect.width),
        (created.rectangles[0].rect.height, expected_rect.height),
    ] {
        assert!(
            (actual - expected).abs() < 0.001,
            "UserUnit pointer mapping drifted: actual={:?}, expected={expected_rect:?}",
            created.rectangles[0].rect
        );
    }

    let select_tool = cx
        .debug_bounds(DOCUMENT_SELECT_TOOL_ID)
        .expect("the GPUI Component Select tool must render");
    cx.simulate_click(select_tool.center(), Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_tool(document_id, cx)),
        Some(AnnotationTool::Select)
    );
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.toggle_annotation_selection(document_id, &rectangle_id, cx)
    }));
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .selected_id
            .is_none()
    }));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let current_layer_bounds = cx.debug_bounds(layer_id).unwrap();
    let current_scale = (f64::from(current_layer_bounds.size.width) / display_size.0)
        .min(f64::from(current_layer_bounds.size.height) / display_size.1);
    let current_origin = point(
        current_layer_bounds.origin.x
            + px(
                ((f64::from(current_layer_bounds.size.width) - display_size.0 * current_scale) / 2.)
                    as f32,
            ),
        current_layer_bounds.origin.y
            + px(
                ((f64::from(current_layer_bounds.size.height) - display_size.1 * current_scale)
                    / 2.) as f32,
            ),
    );
    let current_transform =
        PageTransform::from_page_coordinate_space(coordinate_space, current_scale).unwrap();
    let local_stroke = current_transform.point_to_local_pixels(PdfPoint::new(72., 84.).unwrap());
    let rectangle_stroke = point(
        current_origin.x + px(local_stroke.x as f32),
        current_origin.y + px(local_stroke.y as f32),
    );
    cx.simulate_click(rectangle_stroke, Modifiers::default());
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)
            .unwrap()
            .selected_id),
        Some(rectangle_id.clone()),
        "canonical pointer hit testing must select the same raw-PDF Rectangle"
    );

    let highlight_id = MarkupId::new("workspace:highlight:user-unit").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_highlight(
                document_id,
                0,
                highlight_id.clone(),
                &[
                    PdfPoint::new(72., 132.).unwrap(),
                    PdfPoint::new(144., 132.).unwrap(),
                ],
                cx,
            )
        })
        .expect("the canonical UserUnit page must accept a Highlight");
    let stable_highlight = workspace
        .read_with(cx, |workspace, cx| {
            workspace.highlight_composite_evidence(document_id, cx)
        })
        .unwrap();
    assert!(stable_highlight.current_page_pixels > 0);
    assert!(stable_highlight.thumbnail_pixels > 0);
    let highlight_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                document_id,
                PageViewMode::SinglePage,
                400.,
                1.,
                800.,
                600.,
                0.,
                0.,
                cx,
            )
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(document_id, &highlight_plan, cx)
        })
        .expect("the UserUnit Highlight must precompose into visible tiles");
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .highlight_composite_evidence(document_id, cx))
            .unwrap()
            .viewer_tile_pixels
            > 0
    );

    scroll_annotation_target_into_view(cx, &workspace, DOCUMENT_SNAPSHOT_TOOL_ID);
    let snapshot_button = cx
        .debug_bounds(DOCUMENT_SNAPSHOT_TOOL_ID)
        .expect("the real GPUI Component Snapshot tool must render");
    cx.simulate_click(snapshot_button.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let snapshot_layer = cx.debug_bounds(layer_id).unwrap();
    let snapshot_scale = (f64::from(snapshot_layer.size.width) / display_size.0)
        .min(f64::from(snapshot_layer.size.height) / display_size.1);
    let snapshot_origin = point(
        snapshot_layer.origin.x
            + px(
                ((f64::from(snapshot_layer.size.width) - display_size.0 * snapshot_scale) / 2.)
                    as f32,
            ),
        snapshot_layer.origin.y
            + px(
                ((f64::from(snapshot_layer.size.height) - display_size.1 * snapshot_scale) / 2.)
                    as f32,
            ),
    );
    let snapshot_transform =
        PageTransform::from_page_coordinate_space(coordinate_space, snapshot_scale).unwrap();
    let snapshot_to_view = |point: PdfPoint| {
        let local = snapshot_transform.point_to_local_pixels(point);
        gpui::point(
            snapshot_origin.x + px(local.x as f32),
            snapshot_origin.y + px(local.y as f32),
        )
    };
    let snapshot_start = snapshot_to_view(PdfPoint::new(180., 60.).unwrap());
    let snapshot_end = snapshot_to_view(PdfPoint::new(252., 108.).unwrap());
    cx.simulate_click(snapshot_start, Modifiers::default());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.snapshot_placement_pending(document_id, cx)
    }));
    cx.simulate_mouse_move(snapshot_end, None, Modifiers::default());
    cx.simulate_click(snapshot_end, Modifiers::default());
    let with_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(with_snapshot.snapshots.len(), 1);
    let captured = with_snapshot.snapshots[0].asset();
    assert!(captured.width_px() > 0 && captured.height_px() > 0);
    let first_pixel = &captured.rgba()[0..4];
    assert!(
        captured
            .rgba()
            .chunks_exact(4)
            .any(|pixel| pixel != first_pixel),
        "the canonical Snapshot crop must retain real spatially varied pixels"
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the UserUnit Rectangle must begin Save As");
    cx.run_until_parked();
    let saved_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the validated reopen must retain the Rectangle");
    assert!(!saved_snapshot.dirty);
    assert!(saved_snapshot.pens.iter().any(|pen| pen.id == highlight_id));
    assert_eq!(saved_snapshot.snapshots.len(), 1);
    assert!(saved_snapshot.rectangles[0]
        .rect
        .same_pdf_geometry_as(created.rectangles[0].rect));
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("Save As must replace the source worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let independently_reopened = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(95),
            generation: 2,
            path: saved_path.clone(),
        })
        .expect("the saved UserUnit PDF must independently reopen");
    assert_eq!(independently_reopened.page_coordinate_space(0), Some(coordinate_space));
    let annotated_page = independently_reopened
        .render_page_with_pdf_annotations(0, 320)
        .expect("the saved Rectangle must render through PDFium annotations");
    let base_page = independently_reopened
        .render_page(0, 320)
        .expect("the application base page must remain annotation-free");
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(base_page.pixels_bgra())
    );
    let proof_worker_pid = independently_reopened.worker_pid().unwrap();
    independently_reopened.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{proof_worker_pid}")).exists());

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened_snapshot = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .expect("a fresh workspace must hydrate the saved Rectangle");
    assert_eq!(reopened_snapshot.rectangles.len(), 1);
    assert!(reopened_snapshot.pens.iter().any(|pen| pen.id == highlight_id));
    assert_eq!(reopened_snapshot.snapshots.len(), 1);
    assert!(reopened_snapshot.rectangles[0]
        .rect
        .same_pdf_geometry_as(created.rectangles[0].rect));
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .unwrap();
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_pdfium_worker_opens_navigates_and_exits_without_an_orphan(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert!(
        worker.is_file(),
        "the all-targets gate must build the exact worker"
    );
    assert!(
        library.is_file(),
        "the reviewed development PDFium library must already exist; this test never downloads it"
    );
    let surface_root = manifest_dir
        .join(".prepared/real-document-spine-surfaces")
        .join(std::process::id().to_string());
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));
    let opened = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(91),
            generation: 1,
            path: fixture.clone(),
        })
        .expect("the pinned worker must open and rasterize the public fixture");
    assert_eq!(opened.page_count(), 100);
    assert_eq!(opened.page_coordinate_spaces().len(), 100);
    let first_coordinate_space = opened
        .page_coordinate_space(0)
        .expect("the real worker must retain canonical page metadata");
    assert_eq!(
        first_coordinate_space.view_box(),
        CoordinateRect::new(0., 0., 612., 792.).unwrap()
    );
    assert_eq!(first_coordinate_space.rotation(), CoordinateRotation::Degrees0);
    assert_eq!(first_coordinate_space.user_unit(), 1.);
    assert_eq!(opened.current_page().width(), 900);
    assert!(opened.current_page().height() > 0);
    assert!(
        opened.current_page().has_spatial_variation(),
        "the first real PDF page must contain non-uniform rendered pixels"
    );
    assert_eq!(opened.thumbnails().len(), 12);
    assert!(
        opened
            .thumbnails()
            .iter()
            .enumerate()
            .all(|(index, thumbnail)| {
                thumbnail.page_index == index as u32
                    && thumbnail.raster.width() == 104
                    && thumbnail.raster.height() > 0
                    && !thumbnail.raster.pixels_bgra().is_empty()
            })
    );
    assert!(
        opened
            .thumbnails()
            .iter()
            .any(|thumbnail| thumbnail.raster.has_spatial_variation())
    );
    let worker_pid = opened
        .worker_pid()
        .expect("the worker PID must be observable");
    assert!(PathBuf::from(format!("/proc/{worker_pid}")).exists());
    let second_page = opened
        .render_page(1, 320)
        .expect("thumbnail navigation must rasterize a second real page");
    assert_eq!(second_page.width(), 320);
    let first_page_at_navigation_width = opened
        .render_page(0, 320)
        .expect("the first page must rasterize at the comparison width");
    assert_ne!(
        Sha256::digest(second_page.pixels_bgra()),
        Sha256::digest(first_page_at_navigation_width.pixels_bgra()),
        "thumbnail navigation must return pixels for the selected page"
    );
    let failed_second_open = backend.open(&OpenDocumentRequest {
        document_id: DocumentId::new(92),
        generation: 2,
        path: manifest_dir.join("missing-second-document.pdf"),
    });
    assert!(failed_second_open.is_err());
    assert!(PathBuf::from(format!("/proc/{worker_pid}")).exists());
    let preserved_page = opened
        .render_page(2, 320)
        .expect("a failed second open must not invalidate the first live worker");
    assert_eq!(preserved_page.width(), 320);

    let coordinate_fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-annotation-all-v1.pdf")
        .canonicalize()
        .expect("the coordinate-space fixture path must canonicalize");
    let coordinate_opened = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(93),
            generation: 3,
            path: coordinate_fixture,
        })
        .expect("the worker must open the reviewed CropBox/Rotate fixture");
    assert_eq!(coordinate_opened.page_count(), 2);
    let rotated_coordinate_space = coordinate_opened
        .page_coordinate_space(1)
        .expect("the rotated page must retain canonical coordinate metadata");
    assert_eq!(
        rotated_coordinate_space.view_box(),
        CoordinateRect::new(18., 18., 756., 576.).unwrap()
    );
    assert_eq!(
        rotated_coordinate_space.rotation(),
        CoordinateRotation::Degrees90
    );
    assert_eq!(rotated_coordinate_space.user_unit(), 1.);
    assert_eq!(rotated_coordinate_space.display_size_points(), (576., 756.));
    assert!(coordinate_opened.current_page().has_spatial_variation());
    let rotated_page = coordinate_opened
        .render_page(1, 320)
        .expect("the rotated CropBox page must render real pixels");
    assert_eq!(rotated_page.width(), 320);
    assert!(rotated_page.height() > 0);
    assert!(rotated_page.has_spatial_variation());
    assert!(coordinate_opened
        .thumbnails()
        .iter()
        .any(|thumbnail| thumbnail.page_index == 1 && thumbnail.raster.has_spatial_variation()));
    let coordinate_worker_pid = coordinate_opened
        .worker_pid()
        .expect("the coordinate fixture worker PID must be observable");
    assert!(PathBuf::from(format!("/proc/{coordinate_worker_pid}")).exists());
    coordinate_opened
        .close()
        .expect("the coordinate fixture worker must close cleanly");
    assert!(!PathBuf::from(format!("/proc/{coordinate_worker_pid}")).exists());

    opened
        .close()
        .expect("close must release the worker protocol session");
    assert!(
        !PathBuf::from(format!("/proc/{worker_pid}")).exists(),
        "the worker client drop must kill and wait for the child"
    );

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot
        .borrow_mut()
        .take()
        .expect("the real workspace entity must be retained");
    let live_document =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let real_tile_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                live_document,
                PageViewMode::Continuous,
                400.,
                1.,
                1280.,
                800.,
                0.,
                0.,
                cx,
            )
        })
        .expect("the real 100-page document must produce a bounded tile plan");
    assert!(!real_tile_plan.tiles.is_empty());
    assert!(real_tile_plan.tiles.len() <= 32);
    let real_tiles = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(live_document, &real_tile_plan, cx)
        })
        .expect("the real PDFium worker must render the bounded visible tiles");
    assert_eq!(real_tiles.rendered_tiles, real_tile_plan.tiles.len());
    assert!(real_tiles.non_uniform_tiles > 0);
    assert!(real_tiles.cache_bytes <= 256 * 1024 * 1024);
    let warm_tiles = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(live_document, &real_tile_plan, cx)
        })
        .expect("an equivalent real plan must reuse the byte-accounted cache");
    assert_eq!(warm_tiles.rendered_tiles, 0);
    assert_eq!(warm_tiles.cache_hits, real_tile_plan.tiles.len());
    assert_eq!(warm_tiles.cache_bytes, real_tiles.cache_bytes);
    workspace.update(cx, |workspace, cx| {
        workspace.set_view_configuration(live_document, PageViewMode::SinglePage, 800., cx);
        workspace
            .refresh_viewport_async(live_document, 900., 600., 1., cx)
            .expect("the real visible viewer must queue its asynchronous tiles");
    });
    cx.run_until_parked();
    let first_visible = workspace
        .read_with(cx, |workspace, cx| {
            workspace.viewer_snapshot(live_document, cx)
        })
        .expect("the real visible viewer must retain a snapshot");
    assert_eq!(first_visible.mode, PageViewMode::SinglePage);
    assert_eq!(first_visible.zoom_percent, 800.);
    assert_eq!(first_visible.queued_tiles, 0);
    assert_eq!(first_visible.active_tiles, 0);
    assert!(first_visible.cache_entries > 0);
    assert!(first_visible.cache_bytes <= first_visible.cache_max_bytes);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first_visible_tile = Box::leak(
        document_viewer_tile_id(live_document, first_visible.generation, 0, 0).into_boxed_str(),
    );
    assert!(
        cx.debug_bounds(first_visible_tile).is_some(),
        "the real asynchronous PDFium tile must enter the visible GPUI tree"
    );
    let second_thumbnail = cx
        .debug_bounds("document-1-thumbnail-1")
        .expect("the real second thumbnail must render under its stable ID");
    let workspace_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(live_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the workspace worker PID must be observable");
    cx.simulate_click(second_thumbnail.center(), Modifiers::default());
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .session(live_document, cx)
            .unwrap()
            .read(cx)
            .current_page()),
        1,
        "selecting the real thumbnail must navigate the retained session"
    );
    let before_rotation_dimensions = workspace.read_with(cx, |workspace, cx| {
        let raster = workspace
            .session(live_document, cx)
            .unwrap()
            .read(cx)
            .current_base_raster()
            .unwrap();
        (raster.width(), raster.height())
    });
    let rotation_request = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_rotation(live_document, 1, PageRotationDirection::Right, cx)
        })
        .expect("the live second page must accept one quarter-turn revision");
    let rotation_pixels = workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_rotation_for_evidence(&rotation_request, cx)
        })
        .expect("the real worker pixels must feed the rotation presentation");
    assert!(rotation_pixels.current_page.has_spatial_variation());
    assert!(rotation_pixels.thumbnail.has_spatial_variation());
    assert_eq!(
        (
            rotation_pixels.current_page.width(),
            rotation_pixels.current_page.height()
        ),
        (before_rotation_dimensions.1, before_rotation_dimensions.0)
    );
    assert_ne!(rotation_pixels.thumbnail.width(), 104);
    assert_eq!(rotation_pixels.thumbnail.height(), 104);
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_rotation_result(
            &rotation_request,
            Ok(rotation_pixels),
            cx,
        )),
        ApplyDisposition::Applied
    );
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(live_document, cx))
            .unwrap()
            .dirty
    );
    let real_page_scale = PageScale::custom(
        1,
        "1 in = 2 ft / 9 ft Y",
        ScaleUnit::In,
        ScaleUnit::Ft,
        1.,
        2.,
        Some((2., 9.)),
        ScalePrecision::fraction(16).unwrap(),
    )
    .unwrap();
    assert!(
        workspace
            .update(cx, |workspace, cx| workspace.apply_page_scale(
                live_document,
                real_page_scale.clone(),
                PageScaleApplyTarget::Current(1),
                cx,
            ))
            .unwrap()
    );
    let rotation_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(live_document, cx)
        })
        .unwrap();
    let rotation_target = manifest_dir.join(format!(
        ".prepared/real-page-rotation-save-{}.pdf",
        std::process::id()
    ));
    assert!(!rotation_target.exists());
    let saved_rotation = PdfDocumentSaver::new(backend.clone())
        .save(&SaveDocumentRequest {
            document_id: live_document,
            generation: 77,
            source_path: fixture.clone(),
            destination: save_as_destination(&fixture, &rotation_target),
            current_page: 1,
            annotation_revision: rotation_snapshot.revision,
            annotations: rotation_snapshot.clone(),
            expected_source_sha256: None,
        })
        .expect("the real rotated document must save and independently reopen");
    let source_aspect = before_rotation_dimensions.0 as f64 / before_rotation_dimensions.1 as f64;
    let reopened_aspect = saved_rotation.opened().current_page().width() as f64
        / saved_rotation.opened().current_page().height() as f64;
    assert!(
        (reopened_aspect - source_aspect.recip()).abs() < 0.01,
        "the independently reopened page must swap the source aspect ratio"
    );
    assert!(
        saved_rotation
            .opened()
            .current_page()
            .has_spatial_variation()
    );
    let saved_thumbnail = saved_rotation
        .opened()
        .thumbnails()
        .iter()
        .find(|thumbnail| thumbnail.page_index == 1)
        .unwrap();
    let thumbnail_aspect =
        saved_thumbnail.raster.width() as f64 / saved_thumbnail.raster.height() as f64;
    assert!((thumbnail_aspect - reopened_aspect).abs() < 0.02);
    assert!(saved_thumbnail.raster.has_spatial_variation());
    assert_eq!(
        PdfPersistenceSession::open(&rotation_target)
            .unwrap()
            .page_rotation(1),
        rotation_snapshot
            .page_rotations
            .iter()
            .find(|(page_index, _)| *page_index == 1)
            .map(|(_, rotation)| *rotation)
    );
    assert_eq!(
        PdfPersistenceSession::open(&rotation_target)
            .unwrap()
            .page_scales(),
        &[real_page_scale],
        "the checksum-pinned real PDF journey must preserve the exact custom X/Y fraction scale"
    );
    saved_rotation.opened().close().unwrap();
    std::fs::remove_file(&rotation_target).unwrap();
    workspace.update(cx, |workspace, cx| {
        workspace
            .refresh_viewport_async(live_document, 900., 600., 1., cx)
            .expect("thumbnail navigation must schedule the selected page tiles");
    });
    cx.run_until_parked();
    let second_visible = workspace
        .read_with(cx, |workspace, cx| {
            workspace.viewer_snapshot(live_document, cx)
        })
        .unwrap();
    assert_ne!(second_visible.generation, first_visible.generation);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let second_visible_tile = Box::leak(
        document_viewer_tile_id(live_document, second_visible.generation, 1, 0).into_boxed_str(),
    );
    assert!(cx.debug_bounds(second_visible_tile).is_some());

    let failed_document = workspace.update(cx, |workspace, cx| {
        workspace.open_path(manifest_dir.join("missing-second-document.pdf"), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(live_document)
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        matches!(
            workspace
                .session(failed_document, cx)
                .unwrap()
                .read(cx)
                .status(),
            NativeDocumentStatus::Failed(_)
        ) && workspace
            .session(live_document, cx)
            .unwrap()
            .read(cx)
            .current_page()
            == 1
    }));
    let saved_path = manifest_dir.join(format!(
        ".prepared/document-spine-save-{}.pdf",
        std::process::id()
    ));
    assert!(
        !saved_path.exists(),
        "the exact Save As evidence path must be new"
    );
    let saved_rectangle_id = MarkupId::new("workspace:rectangle:real-save-1").unwrap();
    let saved_ellipse_id = MarkupId::new("workspace:ellipse:real-save-1").unwrap();
    let saved_pen_id = MarkupId::new("workspace:pen:real-save-1").unwrap();
    let saved_highlight_id = MarkupId::new("workspace:highlight:real-save-1").unwrap();
    let saved_text_box_id = MarkupId::new("workspace:text:real-save-1").unwrap();
    let saved_length_id = MarkupId::new("workspace:length:real-save-1").unwrap();
    let saved_line_id = MarkupId::new("workspace:line:real-save-1").unwrap();
    let saved_arrow_id = MarkupId::new("workspace:arrow:real-save-1").unwrap();
    let real_page_calibration = LengthCalibration::from_scale(72., 1., "m", 3, true).unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                live_document,
                1,
                saved_rectangle_id.clone(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(72., 96.).unwrap(),
                butter_paper_gpui_gallery::annotation_model::PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .expect("the real document must accept a retained Rectangle edit");
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_ellipse(
                live_document,
                1,
                saved_ellipse_id.clone(),
                PdfPoint::new(252., 96.).unwrap(),
                PdfPoint::new(396., 192.).unwrap(),
                false,
                cx,
            )
        })
        .expect("the real document must accept a retained Ellipse edit");
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_ellipse_rect(
                live_document,
                saved_ellipse_id.clone(),
                PdfRect::new(264., 90., 168., 108.).unwrap(),
                cx,
            )
        })
        .expect("the real Ellipse must retain edited bounds");
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_ellipse_rotation(live_document, saved_ellipse_id.clone(), 30., cx)
        })
        .expect("the real Ellipse must retain edited rotation");
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_ellipse_appearance(
                live_document,
                RectangleAppearance::new("#2563eb", 3., Some("#dbeafe"), 0.75).unwrap(),
                cx,
            )
        })
        .expect("the real Ellipse must retain edited appearance");
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_pen(
                live_document,
                1,
                saved_pen_id.clone(),
                &[
                    PdfPoint::new(72., 300.).unwrap(),
                    PdfPoint::new(120., 320.).unwrap(),
                    PdfPoint::new(180., 304.).unwrap(),
                ],
                cx,
            )
        })
        .expect("the real document must accept a retained Pen edit");
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_straight_line(
                live_document,
                1,
                saved_line_id.clone(),
                PdfPoint::new(72., 264.).unwrap(),
                PdfPoint::new(216., 300.).unwrap(),
                LineKind::Line,
                cx,
            )
        })
        .expect("the real document must accept a retained Line edit");
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_straight_line(
                live_document,
                1,
                saved_arrow_id.clone(),
                PdfPoint::new(252., 264.).unwrap(),
                PdfPoint::new(396., 300.).unwrap(),
                LineKind::Arrow,
                cx,
            )
        })
        .expect("the real document must accept a retained Arrow edit");
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(live_document, &saved_line_id, cx)
    }));
    for edit in [
        StraightLinePropertyEdit::StrokeColor("#2563eb".into()),
        StraightLinePropertyEdit::StrokeWidthPt(4.),
        StraightLinePropertyEdit::Opacity(0.5),
    ] {
        workspace
            .update(cx, |workspace, cx| {
                workspace.edit_selected_straight_line_property(live_document, edit, cx)
            })
            .expect("the real Line must accept retained appearance edits");
    }
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(live_document, &saved_arrow_id, cx)
    }));
    for edit in [
        StraightLinePropertyEdit::StrokeColor("#00aa00".into()),
        StraightLinePropertyEdit::StrokeWidthPt(2.),
        StraightLinePropertyEdit::Opacity(0.75),
    ] {
        workspace
            .update(cx, |workspace, cx| {
                workspace.edit_selected_straight_line_property(live_document, edit, cx)
            })
            .expect("the real Arrow must accept independent retained appearance edits");
    }
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_annotation_locked(live_document, true, cx)
        })
        .expect("the real Arrow must accept the frozen locked-group branch");
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(live_document, &saved_line_id, cx)
    }));
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.toggle_annotation_selection(live_document, &saved_arrow_id, cx)
    }));
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .copy_selected_annotations(live_document, cx)),
        2,
    );
    let pasted_group_ids = workspace
        .update(cx, |workspace, cx| {
            workspace.paste_annotations(live_document, 1, cx)
        })
        .expect("the selected real Line and locked Arrow must paste transactionally");
    assert_eq!(pasted_group_ids.len(), 2);
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(live_document, cx)
        })
        .expect("mixed group delete must remove only the unlocked pasted Line");
    let surviving_pasted_arrow_id = pasted_group_ids[1].clone();
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .selected_annotation_ids(live_document, cx)),
        vec![surviving_pasted_arrow_id.clone()],
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_highlight(
                live_document,
                1,
                saved_highlight_id.clone(),
                &[
                    PdfPoint::new(72., 340.).unwrap(),
                    PdfPoint::new(108., 340.5).unwrap(),
                    PdfPoint::new(144., 341.).unwrap(),
                ],
                cx,
            )
        })
        .expect("the real document must accept a retained Highlight edit");
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_text_box(
                live_document,
                TextBoxAnnotation::new(
                    saved_text_box_id.clone(),
                    1,
                    PdfRect::new(72., 360., 156., 36.).unwrap(),
                    "Native Text Box",
                    TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                )
                .unwrap(),
                cx,
            )
        })
        .expect("the real document must accept a retained Text Box edit");
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_page_length_calibration(
                live_document,
                1,
                real_page_calibration.clone(),
                cx,
            )
        })
        .expect("the real current page must retain its calibrated scale");
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_length(
                live_document,
                1,
                saved_length_id.clone(),
                PdfPoint::new(72., 432.).unwrap(),
                PdfPoint::new(288., 432.).unwrap(),
                real_page_calibration.clone(),
                cx,
            )
        })
        .expect("the real document must accept a retained Length edit");
    let expected_annotation_order = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(live_document, cx)
        })
        .unwrap()
        .annotation_order;
    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(live_document, saved_path.clone(), cx)
        })
        .expect("the real dirty document must begin Save As");
    cx.run_until_parked();
    let saved_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(live_document, cx)
        })
        .expect("the saved real document must retain annotation state");
    assert!(!saved_snapshot.dirty);
    assert_eq!(saved_snapshot.saved_revision, saved_snapshot.revision);
    assert_eq!(saved_snapshot.annotation_order, expected_annotation_order);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace.page_length_calibration(
            live_document,
            1,
            cx
        )),
        Some(real_page_calibration.clone()),
        "the page calibration must survive the validated native reopen"
    );
    assert!(saved_path.is_file());
    let reopened_session =
        PdfPersistenceSession::open(&saved_path).expect("the saved PDF must reopen through lopdf");
    assert_eq!(reopened_session.page_count(), 100);
    assert_eq!(
        reopened_session.annotation_order(),
        expected_annotation_order
    );
    assert_eq!(
        reopened_session
            .rectangles()
            .iter()
            .find(|rectangle| rectangle.id == saved_rectangle_id),
        saved_snapshot.rectangles.first()
    );
    assert_eq!(
        reopened_session
            .ellipses()
            .iter()
            .find(|ellipse| ellipse.id == saved_ellipse_id),
        saved_snapshot
            .ellipses
            .iter()
            .find(|ellipse| ellipse.id == saved_ellipse_id)
    );
    assert_eq!(
        reopened_session
            .pens()
            .iter()
            .find(|pen| pen.id == saved_pen_id),
        saved_snapshot.pens.first()
    );
    let reopened_highlight = reopened_session
        .pens()
        .iter()
        .find(|pen| pen.id == saved_highlight_id)
        .expect("the real saved Highlight must reopen by stable identity");
    assert_eq!(reopened_highlight.tool(), InkTool::Highlight);
    assert_eq!(reopened_highlight.blend_mode(), BlendMode::Multiply);
    assert!(!reopened_highlight.smooth_curves);
    assert_eq!(
        Some(reopened_highlight),
        saved_snapshot
            .pens
            .iter()
            .find(|pen| pen.id == saved_highlight_id)
    );
    assert_eq!(
        reopened_session
            .text_boxes()
            .iter()
            .find(|text_box| text_box.id == saved_text_box_id),
        saved_snapshot
            .text_boxes
            .iter()
            .find(|text_box| text_box.id == saved_text_box_id)
    );
    assert_eq!(
        reopened_session
            .lengths()
            .iter()
            .find(|length| length.id == saved_length_id),
        saved_snapshot
            .lengths
            .iter()
            .find(|length| length.id == saved_length_id)
    );
    assert_eq!(
        reopened_session
            .straight_lines()
            .iter()
            .find(|line| line.id == saved_line_id),
        saved_snapshot
            .straight_lines
            .iter()
            .find(|line| line.id == saved_line_id)
    );
    assert_eq!(
        reopened_session
            .straight_lines()
            .iter()
            .find(|line| line.id == saved_arrow_id),
        saved_snapshot
            .straight_lines
            .iter()
            .find(|line| line.id == saved_arrow_id)
    );
    let reopened_pasted_arrow = reopened_session
        .straight_lines()
        .iter()
        .find(|line| line.id == surviving_pasted_arrow_id)
        .expect("the locked pasted Arrow must survive typed reopen");
    assert_eq!(reopened_pasted_arrow.kind, LineKind::Arrow);
    assert!(reopened_pasted_arrow.locked);
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .expect("qpdf must be available for structural validation")
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&saved_path)
            .status()
            .expect("pdfinfo must be available for independent reopen validation")
            .success()
    );
    let reopened_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            let session = workspace.session(live_document, cx).unwrap().read(cx);
            assert_eq!(session.path(), saved_path.as_path());
            assert_eq!(session.current_page(), 1);
            session.worker_pid()
        })
        .expect("the saved document must own its independently reopened worker");
    assert_ne!(reopened_worker_pid, workspace_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{workspace_worker_pid}")).exists());
    assert!(PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    let saved_render = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(93),
            generation: 3,
            path: saved_path.clone(),
        })
        .expect("the independently written PDF must reopen through PDFium");
    let saved_second_page = saved_render
        .render_page_with_pdf_annotations(1, 320)
        .expect("the saved Rectangle page must render");
    let saved_annotation_free_second_page = saved_render
        .render_page(1, 320)
        .expect("the saved page must retain an annotation-free application base raster");
    assert!(saved_annotation_free_second_page.has_spatial_variation());
    let saved_base_aspect = f64::from(saved_annotation_free_second_page.width())
        / f64::from(saved_annotation_free_second_page.height());
    assert!(
        (saved_base_aspect - (792. / 612.)).abs() < 0.02,
        "the annotation-free PDFium base must retain the effective rotated page aspect"
    );
    let saved_highlight = saved_snapshot
        .pens
        .iter()
        .find(|pen| pen.id == saved_highlight_id)
        .unwrap();
    let mut cpu_composited_unrotated = second_page.pixels_bgra().to_vec();
    assert!(
        precompose_highlights_multiply_rgba(
            &mut cpu_composited_unrotated,
            second_page.width(),
            second_page.height(),
            612.,
            792.,
            std::slice::from_ref(saved_highlight),
        )
        .unwrap()
            > 0
    );
    let source_sample_x = (108. / 612. * f64::from(second_page.width())).floor() as u32;
    let source_sample_y = ((792. - 340.5) / 792. * f64::from(second_page.height())).floor() as u32;
    let source_sample_offset =
        ((source_sample_y * second_page.width() + source_sample_x) * 4) as usize;
    let cpu_sample = &cpu_composited_unrotated[source_sample_offset..source_sample_offset + 3];
    let sample_x = ((1. - f64::from(source_sample_y) / f64::from(second_page.height()))
        * f64::from(saved_second_page.width()))
    .floor() as u32;
    let sample_y = (f64::from(source_sample_x) / f64::from(second_page.width())
        * f64::from(saved_second_page.height()))
    .floor() as u32;
    let (pdfium_distance, pdfium_offset) = saved_second_page
        .pixels_bgra()
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, pixel)| pixel[0] >= 247 && pixel[1] >= 247 && pixel[2] <= 8)
        .map(|(index, _)| {
            let x = index as u32 % saved_second_page.width();
            let y = index as u32 / saved_second_page.width();
            let distance = x.abs_diff(sample_x) + y.abs_diff(sample_y);
            (distance, index * 4)
        })
        .min_by_key(|(distance, _)| *distance)
        .expect("PDFium must render an opaque yellow Multiply center pixel");
    assert!(
        pdfium_distance <= 8,
        "PDFium Highlight center drifted {pdfium_distance} raster pixels from the CPU oracle"
    );
    let pdfium_sample = &saved_second_page.pixels_bgra()[pdfium_offset..pdfium_offset + 3];
    for (cpu, pdfium) in cpu_sample.iter().zip(pdfium_sample) {
        assert!(
            i16::from(*cpu).abs_diff(i16::from(*pdfium)) <= 8,
            "CPU Multiply oracle {cpu_sample:?} must match PDFium {pdfium_sample:?} at the isolated Highlight center"
        );
    }
    assert_ne!(
        Sha256::digest(saved_second_page.pixels_bgra()),
        Sha256::digest(saved_annotation_free_second_page.pixels_bgra()),
        "the saved Rectangle must change real PDFium page pixels"
    );
    saved_render.close().unwrap();
    let second_live_document = workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let imported_saved_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(second_live_document, cx)
        })
        .expect("the independently opened saved PDF must hydrate imported annotations");
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .selected_annotation_ids(second_live_document, cx))
            .is_empty(),
        "selection is application interaction state and must not persist in the PDF",
    );
    let imported_saved_rectangle = imported_saved_snapshot
        .rectangles
        .iter()
        .find(|rectangle| rectangle.id == saved_rectangle_id)
        .expect("the stable Rectangle identity must survive real PDF reopen");
    let imported_saved_ellipse = imported_saved_snapshot
        .ellipses
        .iter()
        .find(|ellipse| ellipse.id == saved_ellipse_id)
        .expect("the stable Ellipse identity must survive real PDF reopen");
    assert_eq!(imported_saved_ellipse.rotation_degrees, 30.);
    assert_eq!(imported_saved_ellipse.appearance.stroke_color(), "#2563eb");
    assert!(
        imported_saved_snapshot
            .pens
            .iter()
            .any(|pen| pen.id == saved_pen_id)
    );
    assert!(
        imported_saved_snapshot
            .text_boxes
            .iter()
            .any(|text_box| text_box.id == saved_text_box_id)
    );
    assert!(
        imported_saved_snapshot
            .lengths
            .iter()
            .any(|length| length.id == saved_length_id)
    );
    for (id, kind, expected) in [
        (
            &saved_line_id,
            LineKind::Line,
            StraightLineAppearance::new("#2563eb", 4., 0.5, StrokeStyle::Solid).unwrap(),
        ),
        (
            &saved_arrow_id,
            LineKind::Arrow,
            StraightLineAppearance::new("#00aa00", 2., 0.75, StrokeStyle::Solid).unwrap(),
        ),
    ] {
        let imported = imported_saved_snapshot
            .straight_lines
            .iter()
            .find(|line| line.id == *id)
            .expect("the stable straight-line identity must survive real PDF reopen");
        assert_eq!(imported.kind, kind);
        assert_eq!(imported.appearance, expected);
    }
    let imported_pasted_arrow = imported_saved_snapshot
        .straight_lines
        .iter()
        .find(|line| line.id == surviving_pasted_arrow_id)
        .expect("the locked pasted Arrow must survive the real workspace reopen");
    assert_eq!(imported_pasted_arrow.kind, LineKind::Arrow);
    assert!(imported_pasted_arrow.locked);
    let imported_stroke_width = imported_saved_rectangle.appearance.stroke_width_pt();
    assert!(!imported_saved_snapshot.dirty);
    assert_eq!(imported_saved_snapshot.revision, 0);
    assert_eq!(
        (
            imported_saved_snapshot.undo_depth,
            imported_saved_snapshot.redo_depth
        ),
        (0, 0)
    );
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(second_live_document, &saved_rectangle_id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.set_selected_rectangle_stroke_width(second_live_document, 4., cx)
        })
        .expect("the real imported Rectangle must accept a retained property edit");
    let imported_edited_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(second_live_document, cx)
        })
        .unwrap();
    assert_eq!(
        imported_edited_snapshot
            .rectangles
            .iter()
            .find(|rectangle| rectangle.id == saved_rectangle_id)
            .unwrap()
            .appearance
            .stroke_width_pt(),
        4.
    );
    assert!(imported_edited_snapshot.dirty);
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(second_live_document, cx)
        })
        .unwrap();
    let imported_undone_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(second_live_document, cx)
        })
        .unwrap();
    assert_eq!(
        imported_undone_snapshot
            .rectangles
            .iter()
            .find(|rectangle| rectangle.id == saved_rectangle_id)
            .unwrap()
            .appearance
            .stroke_width_pt(),
        imported_stroke_width
    );
    assert!(!imported_undone_snapshot.dirty);
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(second_live_document, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(second_live_document, cx)
        })
        .unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(second_live_document, cx)
        })
        .expect("the imported Rectangle must delete from retained document state");
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(second_live_document, &saved_ellipse_id, cx)
    }));
    workspace
        .update(cx, |workspace, cx| {
            workspace.delete_selected_annotation(second_live_document, cx)
        })
        .expect("the imported Ellipse must delete from retained document state");
    let deleted_saved_path = manifest_dir.join(format!(
        ".prepared/document-spine-delete-{}.pdf",
        std::process::id()
    ));
    assert!(
        !deleted_saved_path.exists(),
        "the deletion Save As target must be new"
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(second_live_document, deleted_saved_path.clone(), cx)
        })
        .expect("the imported Rectangle deletion must begin a real Save As");
    cx.run_until_parked();
    let deleted_saved_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(second_live_document, cx)
        })
        .unwrap();
    assert!(!deleted_saved_snapshot.dirty);
    assert!(
        deleted_saved_snapshot
            .rectangles
            .iter()
            .all(|rectangle| rectangle.id != saved_rectangle_id)
    );
    assert!(
        deleted_saved_snapshot
            .ellipses
            .iter()
            .all(|ellipse| ellipse.id != saved_ellipse_id)
    );
    let independently_deleted = PdfPersistenceSession::open(&deleted_saved_path)
        .expect("the deletion Save As output must reopen independently");
    assert!(
        independently_deleted
            .rectangles()
            .iter()
            .all(|rectangle| rectangle.id != saved_rectangle_id),
        "a deleted imported Rectangle must not reappear after Save As"
    );
    assert!(
        independently_deleted
            .ellipses()
            .iter()
            .all(|ellipse| ellipse.id != saved_ellipse_id),
        "a deleted imported Ellipse must not reappear after Save As"
    );
    let second_live_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(second_live_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the second real live session must own a distinct worker");
    assert_ne!(second_live_worker_pid, reopened_worker_pid);
    assert!(PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(PathBuf::from(format!("/proc/{second_live_worker_pid}")).exists());
    assert_eq!(
        workspace.read_with(cx, |workspace, _| workspace.active_document_id()),
        Some(second_live_document)
    );
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.activate_document(live_document, cx)
    }));
    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(second_live_document, cx)
        }),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{second_live_worker_pid}")).exists());
    assert!(PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(live_document, cx)
    }));
    assert!(
        !PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists(),
        "closing the real retained session must kill and wait for its worker"
    );
    assert!(
        !surface_root.exists()
            || std::fs::read_dir(surface_root)
                .expect("the remaining evidence root must be readable")
                .next()
                .is_none(),
        "close must not leave a mapped surface behind"
    );
}

fn apply_real_rectangle_property_inspector_edits(
    cx: &mut gpui::VisualTestContext,
    workspace: &gpui::Entity<DocumentWorkspace>,
    document_id: DocumentId,
    rectangle_id: &MarkupId,
) -> AnnotationSnapshot {
    scroll_annotation_target_into_view(cx, workspace, DOCUMENT_RECTANGLE_PROPERTIES_ID);
    let properties_trigger = cx
        .debug_bounds(DOCUMENT_RECTANGLE_PROPERTIES_ID)
        .expect("the selected Rectangle must expose its real Properties control");
    cx.simulate_click(properties_trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let inspector = cx
        .debug_bounds(RECTANGLE_PROPERTY_INSPECTOR_ID)
        .expect("the real retained Rectangle inspector must render over the live PDF");
    let viewport = cx.update(|window, _| window.viewport_size());
    assert_eq!(
        f32::from(inspector.size.width),
        RECTANGLE_INSPECTOR_WIDTH_PX
    );
    assert!(inspector.right() <= viewport.width && inspector.bottom() <= viewport.height);
    for id in [
        RECTANGLE_INSPECTOR_LOCKED_ID,
        RECTANGLE_INSPECTOR_STROKE_COLOR_ID,
        RECTANGLE_INSPECTOR_OPACITY_ID,
        RECTANGLE_INSPECTOR_STROKE_WIDTH_ID,
        RECTANGLE_INSPECTOR_STROKE_STYLE_ID,
        RECTANGLE_INSPECTOR_FILL_ENABLED_ID,
        RECTANGLE_INSPECTOR_FILL_COLOR_ID,
        RECTANGLE_INSPECTOR_FILL_OPACITY_ID,
        RECTANGLE_INSPECTOR_X_ID,
        RECTANGLE_INSPECTOR_Y_ID,
        RECTANGLE_INSPECTOR_WIDTH_ID,
        RECTANGLE_INSPECTOR_HEIGHT_ID,
        RECTANGLE_INSPECTOR_ROTATION_ID,
    ] {
        assert!(
            cx.debug_bounds(id).is_some(),
            "{id} must render in the real journey"
        );
    }

    for patch in [
        RectanglePropertyPatch::StrokeColor("#dc2626".into()),
        RectanglePropertyPatch::Opacity(0.88),
        RectanglePropertyPatch::StrokeWidthPt(3.),
        RectanglePropertyPatch::StrokeStyle(StrokeStyle::Dotted),
        RectanglePropertyPatch::FillColor(Some("#abcdef".into())),
        RectanglePropertyPatch::FillOpacity(31. / 255.),
        RectanglePropertyPatch::X(12.),
        RectanglePropertyPatch::Y(24.),
        RectanglePropertyPatch::Width(80.),
        RectanglePropertyPatch::Height(40.),
        RectanglePropertyPatch::RotationDegrees(375.),
        RectanglePropertyPatch::Locked(true),
    ] {
        assert!(
            workspace
                .update(cx, |workspace, cx| workspace
                    .apply_rectangle_property_event(
                        &RectanglePropertyEvent {
                            document_id,
                            annotation_id: rectangle_id.clone(),
                            expected_kind: RectangularShapePropertyKind::Rectangle,
                            patch,
                        },
                        cx,
                    ))
                .expect("the identity-bound inspector edit must be accepted")
        );
    }
    workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the real inspector edits must retain an annotation snapshot")
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_rectangle_property_inspector_save_close_and_fresh_workspace_reopen(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file(), "the exact worker must already be built");
    assert!(
        library.is_file(),
        "the reviewed development PDFium library must already exist"
    );
    let surface_root = manifest_dir
        .join(".prepared/real-rectangle-cutover-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-rectangle-cutover-{}.pdf",
        std::process::id()
    ));
    assert!(!saved_path.exists(), "the Save As target must be new");
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot
        .borrow_mut()
        .take()
        .expect("the rendered workspace must be retained");
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();

    let original_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the opened fixture must own a real worker");
    assert!(PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let rectangle_tool = cx
        .debug_bounds(DOCUMENT_RECTANGLE_TOOL_ID)
        .expect("the real GPUI Component Rectangle tool must render");
    cx.simulate_click(rectangle_tool.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let layer_id = Box::leak(document_annotation_layer_id(document_id, 0).into_boxed_str());
    let layer_bounds = cx
        .debug_bounds(layer_id)
        .expect("the real PDF annotation layer must render");
    let scale =
        (f32::from(layer_bounds.size.width) / 612.).min(f32::from(layer_bounds.size.height) / 792.);
    let page_origin = point(
        layer_bounds.origin.x + px((f32::from(layer_bounds.size.width) - 612. * scale) / 2.),
        layer_bounds.origin.y + px((f32::from(layer_bounds.size.height) - 792. * scale) / 2.),
    );
    let to_view = |pdf_x: f32, pdf_y: f32| {
        point(
            page_origin.x + px(pdf_x * scale),
            page_origin.y + px((792. - pdf_y) * scale),
        )
    };
    let create_start = to_view(72., 96.);
    let create_end = to_view(216., 192.);
    cx.simulate_mouse_down(create_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(create_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(create_end, MouseButton::Left, Modifiers::default());

    let rectangle_id = MarkupId::new("workspace:rectangle:1").unwrap();
    let created = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the native pointer gesture must create a retained Rectangle");
    assert_eq!(created.selected_id.as_ref(), Some(&rectangle_id));
    assert_eq!(created.rectangles.len(), 1);
    assert_eq!(
        created.rectangles[0].appearance,
        RectangleAppearance::default()
    );
    assert_eq!((created.revision, created.saved_revision), (1, 0));
    assert!(created.dirty);

    let select_tool = cx
        .debug_bounds(DOCUMENT_SELECT_TOOL_ID)
        .expect("the real GPUI Component Select tool must render");
    cx.simulate_click(select_tool.center(), Modifiers::default());
    let move_start = to_view(72., 120.);
    let move_end = to_view(90., 108.);
    cx.simulate_mouse_down(move_start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(move_end, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(move_end, MouseButton::Left, Modifiers::default());

    let edited =
        apply_real_rectangle_property_inspector_edits(cx, &workspace, document_id, &rectangle_id);
    assert_eq!(
        (edited.revision, edited.undo_depth, edited.redo_depth),
        (14, 14, 0)
    );
    assert!(edited.dirty);
    assert_eq!(edited.selected_id.as_ref(), Some(&rectangle_id));
    let edited_rectangle = &edited.rectangles[0];
    for (actual, expected) in [
        (edited_rectangle.rect.x, 12.),
        (edited_rectangle.rect.y, 24.),
        (edited_rectangle.rect.width, 80.),
        (edited_rectangle.rect.height, 40.),
    ] {
        assert!((actual - expected).abs() < 0.001);
    }
    assert_eq!(edited_rectangle.rotation_degrees, 15.);
    assert!(edited_rectangle.locked);
    assert_eq!(edited_rectangle.appearance.stroke_color(), "#dc2626");
    assert_eq!(edited_rectangle.appearance.stroke_width_pt(), 3.);
    assert_eq!(
        edited_rectangle.appearance.stroke_style(),
        StrokeStyle::Dotted
    );
    assert_eq!(edited_rectangle.appearance.fill_color(), Some("#abcdef"));
    assert!((edited_rectangle.appearance.fill_opacity() - 31. / 255.).abs() < 0.000_001);
    assert_eq!(edited_rectangle.appearance.opacity(), 0.88);
    let page_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(document_id, 0, cx)
    });
    let thumbnail_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.thumbnail_annotation_scene(document_id, 0, cx)
    });
    assert_eq!(page_scene.rectangles[0].id, rectangle_id);
    assert_eq!(
        page_scene.rectangles[0].rect,
        thumbnail_scene.rectangles[0].rect
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .expect("the real Rectangle document must begin Save As");
    cx.run_until_parked();
    let saved = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .expect("the validated reopen must retain the saved Rectangle");
    let save_status = workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(document_id, cx)
            .unwrap()
            .read(cx)
            .save_status()
            .clone()
    });
    assert!(
        !saved.dirty,
        "Save As must mark the current Rectangle revision clean: {save_status:?}"
    );
    assert_eq!(saved.saved_revision, saved.revision);
    assert!(saved_path.is_file());
    let saved_worker_pid = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the validated Save As reopen must own a replacement worker");
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());
    let saved_pixel_proof = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(9_001),
            generation: 1,
            path: saved_path.clone(),
        })
        .expect("the saved PDF must reopen for an independent PDFium pixel oracle");
    let annotated_page = saved_pixel_proof
        .render_page_with_pdf_annotations(0, 320)
        .expect("the saved Rectangle must render through PDFium annotation flags");
    let annotation_free_page = saved_pixel_proof
        .render_page(0, 320)
        .expect("the application base raster must remain annotation-free");
    assert_ne!(
        Sha256::digest(annotated_page.pixels_bgra()),
        Sha256::digest(annotation_free_page.pixels_bgra()),
        "the persisted Rectangle must change real PDFium pixels while the workspace keeps its base raster annotation-free"
    );
    let pixel_proof_worker_pid = saved_pixel_proof.worker_pid().unwrap();
    saved_pixel_proof.close().unwrap();
    assert!(!PathBuf::from(format!("/proc/{pixel_proof_worker_pid}")).exists());
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&saved_path)
            .status()
            .expect("qpdf must be available for structural validation")
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&saved_path)
            .status()
            .expect("pdfinfo must be available for independent validation")
            .success()
    );

    assert_eq!(
        workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(document_id, cx)
        }),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());
    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.session(document_id, cx).is_none()
    }));

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_document = fresh_workspace.update(cx, |workspace, cx| {
        workspace.open_path(saved_path.clone(), cx)
    });
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_document, cx)
        })
        .expect("a distinct workspace must hydrate the saved Rectangle");
    assert_eq!((reopened.revision, reopened.saved_revision), (0, 0));
    assert_eq!((reopened.undo_depth, reopened.redo_depth), (0, 0));
    assert!(!reopened.dirty);
    assert!(reopened.selected_id.is_none());
    let reopened_rectangle = reopened
        .rectangles
        .iter()
        .find(|rectangle| rectangle.id == rectangle_id)
        .expect("the stable Rectangle identity must survive a fresh workspace reopen");
    assert!(
        reopened_rectangle
            .rect
            .same_pdf_geometry_as(edited_rectangle.rect),
        "PDF edge reconstruction must preserve Rectangle geometry within 0.00001 pt"
    );
    assert_eq!(reopened_rectangle.appearance, edited_rectangle.appearance);
    let reopened_worker_pid = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(reopened_document, cx)
                .and_then(|session| session.read(cx).worker_pid())
        })
        .expect("the fresh workspace must own its own worker");
    assert_ne!(reopened_worker_pid, saved_worker_pid);
    assert!(PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert_eq!(
        fresh_workspace.update(cx, |workspace, cx| {
            workspace.request_close_document(reopened_document, cx)
        }),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{reopened_worker_pid}")).exists());
    assert!(
        !surface_root.exists()
            || std::fs::read_dir(&surface_root)
                .expect("the mapped-surface root must remain readable")
                .next()
                .is_none(),
        "both clean closes must release every mapped surface"
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_native_shell_preserves_independent_view_state_through_fit_scroll_thumbnail_zoom_and_document_switch(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(fixture.is_file());

    let owned_root = manifest_dir
        .join(".prepared/real-viewer-state")
        .join(std::process::id().to_string());
    std::fs::remove_dir_all(&owned_root).ok();
    std::fs::create_dir_all(&owned_root).unwrap();
    let _scratch = ScratchDirectories(vec![owned_root.clone()]);
    let first_path = owned_root.join("first.pdf");
    let second_path = owned_root.join("second.pdf");
    let surface_root = owned_root.join("surfaces");
    std::fs::copy(&fixture, &first_path).unwrap();
    std::fs::copy(&fixture, &second_path).unwrap();
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(|cx| {
        gpui_component::init(cx);
        init_document_workspace_actions(cx);
    });
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let first_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path(first_path.clone(), cx)
    });
    cx.run_until_parked();
    let second_id = workspace.update(cx, |workspace, cx| {
        workspace.open_path(second_path.clone(), cx)
    });
    cx.run_until_parked();
    assert_ne!(first_id, second_id);

    let (first_worker, second_worker) = workspace.read_with(cx, |workspace, cx| {
        (
            workspace
                .session(first_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
                .unwrap(),
            workspace
                .session(second_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
                .unwrap(),
        )
    });
    assert!(PathBuf::from(format!("/proc/{first_worker}")).exists());
    assert!(PathBuf::from(format!("/proc/{second_worker}")).exists());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first_tab_id = Box::leak(document_session_tab_id(first_id).into_boxed_str());
    let first_tab = cx.debug_bounds(first_tab_id).unwrap();
    cx.simulate_click(first_tab.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let toolbar = cx
        .debug_bounds(VIEWER_TOOLBAR_ID)
        .expect("the real GPUI Component viewer toolbar must render in the workspace");
    let toolbar_scroll = cx.debug_bounds(VIEWER_TOOLBAR_SCROLL_ID).unwrap();
    let toolbar_content = cx.debug_bounds(VIEWER_TOOLBAR_CONTENT_ID).unwrap();
    assert!(toolbar.size.width > px(0.) && toolbar.size.height > px(0.));
    assert!(toolbar_scroll.size.width > px(0.) && toolbar_content.size.width > px(0.));
    let first_targets = [
        ZOOM_MENU_ID,
        FIT_WIDTH_ID,
        FIT_PAGE_ID,
        CONTINUOUS_PRIMARY_ID,
        SINGLE_PAGE_PRIMARY_ID,
    ]
    .map(|id| cx.debug_bounds(id).unwrap());
    assert!(
        first_targets
            .iter()
            .all(|target| { target.size.width > px(0.) && target.size.height > px(0.) })
    );
    for pair in first_targets.windows(2) {
        assert!(
            pair[0].right() <= pair[1].left(),
            "viewer controls must not overlap"
        );
    }

    let single_page = cx.debug_bounds(SINGLE_PAGE_PRIMARY_ID).unwrap();
    cx.simulate_click(single_page.center(), Modifiers::default());
    let fit_page = cx.debug_bounds(FIT_PAGE_ID).unwrap();
    cx.simulate_click(fit_page.center(), Modifiers::default());
    let zoom_menu = cx.debug_bounds(ZOOM_MENU_ID).unwrap();
    cx.simulate_click(zoom_menu.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("down down down down down down down down down down enter");
    cx.run_until_parked();

    let first_thumbnail_id = Box::leak(document_thumbnail_id(first_id, 1).into_boxed_str());
    let first_thumbnail = cx.debug_bounds(first_thumbnail_id).unwrap();
    cx.simulate_click(first_thumbnail.center(), Modifiers::default());
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.set_viewport_scroll(first_id, 120., 700., cx));
    });
    let first_state = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(first_id, cx)
        })
        .unwrap();
    assert_eq!(first_state.mode(), PageViewMode::SinglePage);
    assert_eq!(first_state.zoom_percent(), 400.);
    assert_eq!(first_state.scroll(), (120., 700.));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(first_id, cx)
                .unwrap()
                .read(cx)
                .current_page()
        }),
        1
    );
    let first_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                first_id,
                first_state.mode(),
                first_state.zoom_percent(),
                1.,
                640.,
                480.,
                first_state.scroll().0,
                first_state.scroll().1,
                cx,
            )
        })
        .unwrap();
    let first_render = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(first_id, &first_plan, cx)
        })
        .unwrap();
    assert!(first_render.rendered_tiles > 0);
    assert!(first_render.cache_bytes <= first_plan.cache_max_bytes);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let second_tab_id = Box::leak(document_session_tab_id(second_id).into_boxed_str());
    let second_tab = cx.debug_bounds(second_tab_id).unwrap();
    cx.simulate_click(second_tab.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let continuous = cx.debug_bounds(CONTINUOUS_PRIMARY_ID).unwrap();
    cx.simulate_click(continuous.center(), Modifiers::default());
    let fit_width = cx.debug_bounds(FIT_WIDTH_ID).unwrap();
    cx.simulate_click(fit_width.center(), Modifiers::default());
    let zoom_menu = cx.debug_bounds(ZOOM_MENU_ID).unwrap();
    cx.simulate_click(zoom_menu.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.simulate_keystrokes("down down down down down down down down down down down down enter");
    cx.run_until_parked();
    let second_thumbnail_id = Box::leak(document_thumbnail_id(second_id, 1).into_boxed_str());
    let second_thumbnail = cx.debug_bounds(second_thumbnail_id).unwrap();
    cx.simulate_click(second_thumbnail.center(), Modifiers::default());
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        assert!(workspace.set_viewport_scroll(second_id, 0., 1_000., cx));
    });
    let second_state = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(second_id, cx)
        })
        .unwrap();
    assert_eq!(second_state.mode(), PageViewMode::Continuous);
    assert_eq!(second_state.zoom_percent(), 1600.);
    assert_eq!(second_state.scroll(), (0., 1_000.));
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| {
            workspace
                .session(second_id, cx)
                .unwrap()
                .read(cx)
                .current_page()
        }),
        1
    );

    let stale_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                second_id,
                PageViewMode::Continuous,
                400.,
                1.,
                640.,
                480.,
                0.,
                1_000.,
                cx,
            )
        })
        .unwrap();
    let current_plan = workspace
        .update(cx, |workspace, cx| {
            workspace.plan_viewport(
                second_id,
                second_state.mode(),
                second_state.zoom_percent(),
                1.,
                640.,
                480.,
                second_state.scroll().0,
                second_state.scroll().1,
                cx,
            )
        })
        .unwrap();
    assert_ne!(stale_plan.generation, current_plan.generation);
    assert!(
        workspace
            .update(cx, |workspace, cx| {
                workspace.render_planned_tiles_for_evidence(second_id, &stale_plan, cx)
            })
            .is_err()
    );
    let second_render = workspace
        .update(cx, |workspace, cx| {
            workspace.render_planned_tiles_for_evidence(second_id, &current_plan, cx)
        })
        .unwrap();
    assert!(second_render.rendered_tiles > 0);
    assert!(second_render.cache_bytes <= current_plan.cache_max_bytes);
    assert!(current_plan.tiles.len() <= 32);
    assert_eq!(current_plan.cache_max_bytes, 256 * 1024 * 1024);

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let first_tab = cx.debug_bounds(first_tab_id).unwrap();
    cx.simulate_click(first_tab.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let restored_first = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(first_id, cx)
        })
        .unwrap();
    assert_eq!(restored_first.mode(), first_state.mode());
    assert_eq!(restored_first.zoom_preset(), first_state.zoom_preset());
    assert_eq!(restored_first.zoom_percent(), first_state.zoom_percent());
    assert_eq!(restored_first.scroll(), first_state.scroll());
    assert_eq!(
        restored_first.wheel_behavior(PageViewMode::Continuous),
        first_state.wheel_behavior(PageViewMode::Continuous)
    );
    assert_eq!(
        restored_first.wheel_behavior(PageViewMode::SinglePage),
        first_state.wheel_behavior(PageViewMode::SinglePage)
    );
    let restored_second = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(second_id, cx)
        })
        .unwrap();
    assert_eq!(restored_second.mode(), second_state.mode());
    assert_eq!(restored_second.zoom_preset(), second_state.zoom_preset());
    assert_eq!(restored_second.zoom_percent(), second_state.zoom_percent());
    assert_eq!(restored_second.scroll(), second_state.scroll());
    assert_eq!(
        restored_second.wheel_behavior(PageViewMode::Continuous),
        second_state.wheel_behavior(PageViewMode::Continuous)
    );
    assert_eq!(
        restored_second.wheel_behavior(PageViewMode::SinglePage),
        second_state.wheel_behavior(PageViewMode::SinglePage)
    );
    assert_eq!(
        cx.debug_bounds(ZOOM_MENU_ID).unwrap().size,
        first_targets[0].size
    );

    assert!(workspace.update(cx, |workspace, cx| workspace.close_document(first_id, cx)));
    assert!(workspace.update(cx, |workspace, cx| workspace.close_document(second_id, cx)));
    assert!(!PathBuf::from(format!("/proc/{first_worker}")).exists());
    assert!(!PathBuf::from(format!("/proc/{second_worker}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_in_place_save_replaces_the_opened_pdf_and_reopens_cleanly(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file(), "the exact worker must already be built");
    assert!(
        library.is_file(),
        "the reviewed development PDFium must exist"
    );
    let source = manifest_dir.join(format!(
        ".prepared/real-in-place-save-{}.pdf",
        std::process::id()
    ));
    let original_inode = manifest_dir.join(format!(
        ".prepared/real-in-place-save-original-{}.pdf",
        std::process::id()
    ));
    std::fs::copy(&fixture, &source).unwrap();
    std::fs::hard_link(&source, &original_inode).unwrap();
    #[cfg(unix)]
    std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o640)).unwrap();
    let original_bytes = std::fs::read(&source).unwrap();
    let surface_root = manifest_dir
        .join(".prepared/real-in-place-save-surfaces")
        .join(std::process::id().to_string());
    let _scratch_files = ScratchFiles(vec![source.clone(), original_inode.clone()]);
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id = workspace.update(cx, |workspace, cx| workspace.open_path(source.clone(), cx));
    cx.run_until_parked();
    let navigation = workspace
        .update(cx, |workspace, cx| {
            workspace.begin_page_navigation(document_id, 1, cx)
        })
        .unwrap();
    let page = workspace
        .read_with(cx, |workspace, cx| {
            workspace.render_page_request_for_evidence(&navigation, cx)
        })
        .unwrap();
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &navigation,
            Ok(page),
            cx,
        )),
        ApplyDisposition::Applied
    );
    let rectangle_id = MarkupId::new("workspace:rectangle:real-in-place-save").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                1,
                rectangle_id.clone(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let before_save = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(before_save.dirty);
    let (title_before, worker_before) = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        (session.title().to_owned(), session.worker_pid().unwrap())
    });
    assert!(PathBuf::from(format!("/proc/{worker_before}")).exists());

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save = cx.debug_bounds(DOCUMENT_SAVE_ID).expect("Save must render");
    cx.simulate_click(save.center(), Modifiers::default());
    cx.run_until_parked();

    let (path_after, title_after, page_after, worker_after, save_status) =
        workspace.read_with(cx, |workspace, cx| {
            let session = workspace.session(document_id, cx).unwrap().read(cx);
            (
                session.path().to_path_buf(),
                session.title().to_owned(),
                session.current_page(),
                session.worker_pid().unwrap(),
                session.save_status().clone(),
            )
        });
    let saved_snapshot = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert_eq!(
        save_status,
        butter_paper_gpui_component_compat::document_workspace::NativeDocumentSaveStatus::Idle
    );
    assert_eq!(path_after, source);
    assert_eq!(title_after, title_before);
    assert_eq!(page_after, 1);
    assert_eq!(saved_snapshot.selected_id.as_ref(), Some(&rectangle_id));
    assert_eq!(saved_snapshot.saved_revision, saved_snapshot.revision);
    assert!(!saved_snapshot.dirty);
    assert_ne!(worker_after, worker_before);
    assert!(!PathBuf::from(format!("/proc/{worker_before}")).exists());
    assert!(PathBuf::from(format!("/proc/{worker_after}")).exists());
    let saved_bytes = std::fs::read(&source).unwrap();
    assert_ne!(saved_bytes, original_bytes);
    assert_eq!(std::fs::read(&original_inode).unwrap(), original_bytes);
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&source).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    assert!(
        std::process::Command::new("pdfinfo")
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let persisted = PdfPersistenceSession::open(&source).unwrap();
    assert!(
        persisted
            .rectangles()
            .iter()
            .any(|rectangle| rectangle.id == rectangle_id)
    );

    workspace
        .update(cx, |workspace, cx| {
            workspace.undo_annotations(document_id, cx)
        })
        .unwrap();
    assert!(
        workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(document_id, cx))
            .unwrap()
            .dirty
    );
    workspace
        .update(cx, |workspace, cx| {
            workspace.redo_annotations(document_id, cx)
        })
        .unwrap();
    assert!(
        !workspace
            .read_with(cx, |workspace, cx| workspace
                .annotation_snapshot(document_id, cx))
            .unwrap()
            .dirty
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .request_close_document(document_id, cx)),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{worker_after}")).exists());

    let fresh_workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let reopened_id =
        fresh_workspace.update(cx, |workspace, cx| workspace.open_path(source.clone(), cx));
    cx.run_until_parked();
    let reopened = fresh_workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(reopened_id, cx)
        })
        .unwrap();
    assert!(
        reopened
            .rectangles
            .iter()
            .any(|rectangle| rectangle.id == rectangle_id)
    );
    let reopened_worker = fresh_workspace.read_with(cx, |workspace, cx| {
        workspace
            .session(reopened_id, cx)
            .unwrap()
            .read(cx)
            .worker_pid()
            .unwrap()
    });
    assert!(fresh_workspace.update(cx, |workspace, cx| {
        workspace.close_document(reopened_id, cx)
    }));
    assert!(!PathBuf::from(format!("/proc/{reopened_worker}")).exists());
    assert!(
        !surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none(),
        "both workspaces must release every mapped surface"
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_save_as_collision_recovers_to_fresh_target_and_reopens(cx: &mut TestAppContext) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file());
    assert!(library.is_file());
    assert!(fixture.is_file());

    let owned_root = manifest_dir
        .join(".prepared/real-save-as-collision")
        .join(std::process::id().to_string());
    std::fs::remove_dir_all(&owned_root).ok();
    std::fs::create_dir_all(&owned_root).unwrap();
    let _scratch = ScratchDirectories(vec![owned_root.clone()]);
    let source = owned_root.join("source.pdf");
    let occupied = owned_root.join("occupied.pdf");
    let fresh_target = owned_root.join("recovered.pdf");
    let surface_root = owned_root.join("surfaces");
    std::fs::copy(&fixture, &source).unwrap();
    std::fs::copy(&fixture, &occupied).unwrap();
    let occupied_before = Sha256::digest(std::fs::read(&occupied).unwrap());
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id = workspace.update(cx, |workspace, cx| workspace.open_path(source.clone(), cx));
    cx.run_until_parked();
    let rectangle_id = MarkupId::new("workspace:rectangle:save-as-collision").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                0,
                rectangle_id.clone(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let worker_before = workspace
        .read_with(cx, |workspace, cx| {
            workspace
                .session(document_id, cx)
                .unwrap()
                .read(cx)
                .worker_pid()
        })
        .unwrap();

    cx.update(|window, cx| window.draw(cx).clear(cx));
    let save_as = cx.debug_bounds(DOCUMENT_SAVE_AS_ID).unwrap();
    cx.simulate_click(save_as.center(), Modifiers::default());
    assert!(cx.did_prompt_for_new_path());
    let occupied_selection = occupied.clone();
    cx.simulate_new_path_selection(move |_| Some(occupied_selection));
    cx.run_until_parked();

    let failure = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_save_failure(document_id, cx)
        })
        .expect("an occupied Save As target must become visible typed recovery state");
    assert_eq!(failure.operation, DocumentSaveFailureOperation::SaveAs);
    assert_eq!(
        failure.message,
        "Save As will not replace an existing destination."
    );
    assert_eq!(
        Sha256::digest(std::fs::read(&occupied).unwrap()),
        occupied_before
    );
    assert!(workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        session.path() == source.as_path()
            && session.worker_pid() == Some(worker_before)
            && workspace
                .annotation_snapshot(document_id, cx)
                .unwrap()
                .dirty
    }));

    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds(DOCUMENT_SAVE_ERROR_RETRY_ID).is_none());
    let choose_another = cx.debug_bounds(DOCUMENT_SAVE_ERROR_SAVE_AS_ID).unwrap();
    cx.simulate_click(choose_another.center(), Modifiers::default());
    assert!(cx.did_prompt_for_new_path());
    let fresh_selection = fresh_target.clone();
    cx.simulate_new_path_selection(move |_| Some(fresh_selection));
    cx.run_until_parked();

    assert!(workspace.read_with(cx, |workspace, cx| {
        workspace.document_save_failure(document_id, cx).is_none()
    }));
    let worker_after = workspace
        .read_with(cx, |workspace, cx| {
            let session = workspace.session(document_id, cx).unwrap().read(cx);
            assert_eq!(session.path(), fresh_target.as_path());
            assert_eq!(session.save_status(), &NativeDocumentSaveStatus::Idle);
            assert!(
                !workspace
                    .annotation_snapshot(document_id, cx)
                    .unwrap()
                    .dirty
            );
            session.worker_pid()
        })
        .unwrap();
    assert_ne!(worker_after, worker_before);
    assert!(!Path::new(&format!("/proc/{worker_before}")).exists());
    assert!(fresh_target.is_file());
    assert_eq!(
        Sha256::digest(std::fs::read(&occupied).unwrap()),
        occupied_before
    );
    assert!(
        std::process::Command::new("qpdf")
            .arg("--check")
            .arg(&fresh_target)
            .status()
            .unwrap()
            .success()
    );
    let reopened = PdfPersistenceSession::open(&fresh_target).unwrap();
    assert!(
        reopened
            .rectangles()
            .iter()
            .any(|rectangle| rectangle.id == rectangle_id)
    );
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.close_document(document_id, cx)
    }));
    assert!(!Path::new(&format!("/proc/{worker_after}")).exists());
    assert!(!surface_root.exists() || std::fs::read_dir(&surface_root).unwrap().next().is_none());
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_worker_crash_recovery_preserves_dirty_document_and_releases_resources(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture =
        manifest_dir.join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf");
    assert!(worker.is_file(), "the exact worker must already be built");
    assert!(library.is_file(), "the reviewed PDFium library must exist");
    let surface_root = manifest_dir
        .join(".prepared/real-worker-recovery-surfaces")
        .join(std::process::id().to_string());
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    workspace.update(cx, |workspace, cx| {
        workspace.set_view_configuration(document_id, PageViewMode::SinglePage, 125., cx)
    });
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_rectangle(
                document_id,
                0,
                MarkupId::new("workspace:rectangle:worker-recovery").unwrap(),
                PdfPoint::new(72., 96.).unwrap(),
                PdfPoint::new(216., 192.).unwrap(),
                cx,
            )
        })
        .unwrap();
    let before_annotations = workspace
        .read_with(cx, |workspace, cx| {
            workspace.annotation_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(before_annotations.dirty);
    let before_view = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(document_id, cx)
        })
        .unwrap();
    let before = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(document_id, cx)
        })
        .unwrap();
    let crashed_pid = before
        .worker_pid
        .expect("the real worker PID must be observable");
    assert!(PathBuf::from(format!("/proc/{crashed_pid}")).exists());

    let kill = std::process::Command::new("kill")
        .args(["-KILL", &crashed_pid.to_string()])
        .status()
        .expect("the test must be able to terminate its owned worker");
    assert!(kill.success());
    let failed_navigation = workspace.update(cx, |workspace, cx| {
        workspace.begin_page_navigation(document_id, 1, cx).unwrap()
    });
    let render_result = workspace.read_with(cx, |workspace, cx| {
        workspace.render_page_request_for_evidence(&failed_navigation, cx)
    });
    assert!(
        render_result.is_err(),
        "the killed worker must fail a real render"
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.apply_page_result(
            &failed_navigation,
            render_result,
            cx,
        )),
        ApplyDisposition::Applied
    );
    let failed = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(document_id, cx)
        })
        .unwrap();
    assert!(failed.ready);
    assert_eq!(failed.current_page, before.current_page);
    assert_eq!(failed.requested_page, before.current_page);
    assert_eq!(failed.current_raster_bytes, before.current_raster_bytes);
    assert!(
        failed
            .presentation_error
            .as_deref()
            .is_some_and(|error| { error.contains("WorkerCrashed") || error.contains("worker") })
    );
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)),
        Some(before_annotations.clone())
    );
    cx.update(|window, cx| {
        window.activate_window();
        window.draw(cx).clear(cx);
    });
    let retry = cx
        .debug_bounds(DOCUMENT_RECOVERY_RETRY_ID)
        .expect("the real GPUI Component Retry button must render");
    cx.simulate_click(retry.center(), Modifiers::default());

    let recovered = workspace
        .read_with(cx, |workspace, cx| {
            workspace.evidence_snapshot(document_id, cx)
        })
        .unwrap();
    let recovered_pid = recovered
        .worker_pid
        .expect("recovery must own a replacement worker");
    assert_ne!(recovered_pid, crashed_pid);
    assert!(!PathBuf::from(format!("/proc/{crashed_pid}")).exists());
    assert!(PathBuf::from(format!("/proc/{recovered_pid}")).exists());
    assert!(recovered.ready);
    assert!(recovered.presentation_error.is_none());
    assert_eq!(recovered.current_page, before.current_page);
    assert!(recovered.current_raster_has_spatial_variation);
    assert_eq!(
        workspace.read_with(cx, |workspace, cx| workspace
            .annotation_snapshot(document_id, cx)),
        Some(before_annotations)
    );
    let recovered_view = workspace
        .read_with(cx, |workspace, cx| {
            workspace.document_view_state(document_id, cx)
        })
        .unwrap();
    assert_eq!(recovered_view.mode(), before_view.mode());
    assert_eq!(recovered_view.zoom_preset(), before_view.zoom_preset());
    assert_eq!(recovered_view.zoom_percent(), before_view.zoom_percent());
    assert_eq!(recovered_view.scroll(), before_view.scroll());

    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .request_close_document(document_id, cx)),
        CloseRequestDisposition::ConfirmationRequired
    );
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace
            .resolve_dirty_close_discard(cx)),
        DirtyCloseResolution::Discarded
    );
    assert!(!PathBuf::from(format!("/proc/{recovered_pid}")).exists());
    assert!(
        !surface_root.exists()
            || std::fs::read_dir(&surface_root)
                .expect("the recovery surface root must be readable")
                .next()
                .is_none(),
        "recovery and close must release every mapped worker surface"
    );
}

#[gpui::test]
#[ignore = "requires the checksum-pinned development PDFium library; production redistribution remains blocked"]
fn real_shared_shape_property_inspector_save_close_and_fresh_workspace_reopen(
    cx: &mut TestAppContext,
) {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let test_executable = std::env::current_exe().expect("the test executable path must exist");
    let worker = test_executable
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the Cargo target layout must have a debug directory")
        .join(if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        });
    let library = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            manifest_dir.join(
                "../gpui-gallery/target/pdfium-development/x86_64-unknown-linux-gnu/lib/libpdfium.so",
            )
        });
    let fixture = manifest_dir
        .join("../performance/results/public-fixtures-v1/bp-multi-page-v1.pdf")
        .canonicalize()
        .expect("the provenance-controlled fixture path must canonicalize");
    assert!(worker.is_file() && library.is_file() && fixture.is_file());
    let surface_root = manifest_dir
        .join(".prepared/real-shared-shape-inspector-surfaces")
        .join(std::process::id().to_string());
    let saved_path = manifest_dir.join(format!(
        ".prepared/real-shared-shape-inspector-{}.pdf",
        std::process::id()
    ));
    let _scratch_directories = ScratchDirectories(vec![surface_root.clone()]);
    let _scratch_files = ScratchFiles(vec![saved_path.clone()]);
    assert!(!saved_path.exists());
    let backend = Arc::new(PdfiumWorkerBackend::new(
        worker,
        library,
        surface_root.clone(),
    ));

    cx.update(gpui_component::init);
    let workspace_slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (_, cx) = cx.add_window_view({
        let workspace_slot = workspace_slot.clone();
        let backend = backend.clone();
        move |window, cx| {
            let workspace = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
            workspace_slot.replace(Some(workspace.clone()));
            Root::new(workspace, window, cx)
        }
    });
    let workspace = workspace_slot.borrow_mut().take().unwrap();
    let document_id =
        workspace.update(cx, |workspace, cx| workspace.open_path(fixture.clone(), cx));
    cx.run_until_parked();
    let original_worker_pid = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert!(matches!(session.status(), NativeDocumentStatus::Ready));
        assert!(session.current_base_raster().unwrap().has_spatial_variation());
        assert!(session.thumbnail_base_raster(0).unwrap().has_spatial_variation());
        session.worker_pid().unwrap()
    });
    assert!(PathBuf::from(format!("/proc/{original_worker_pid}")).exists());

    let ellipse_id = MarkupId::new("workspace:ellipse:shared-inspector-real").unwrap();
    workspace
        .update(cx, |workspace, cx| {
            workspace.create_ellipse(
                document_id,
                0,
                ellipse_id.clone(),
                PdfPoint { x: 72., y: 144. },
                PdfPoint { x: 252., y: 240. },
                false,
                cx,
            )
        })
        .unwrap();
    assert!(workspace.update(cx, |workspace, cx| {
        workspace.select_annotation(document_id, &ellipse_id, cx)
    }));
    for patch in [
        RectanglePropertyPatch::StrokeColor("#2563eb".into()),
        RectanglePropertyPatch::Opacity(0.72),
        RectanglePropertyPatch::StrokeWidthPt(3.),
        RectanglePropertyPatch::StrokeStyle(StrokeStyle::Dashed),
        RectanglePropertyPatch::FillColor(Some("#fef3c7".into())),
        RectanglePropertyPatch::FillOpacity(0.4),
        RectanglePropertyPatch::X(96.),
        RectanglePropertyPatch::Y(168.),
        RectanglePropertyPatch::Width(216.),
        RectanglePropertyPatch::Height(120.),
        RectanglePropertyPatch::RotationDegrees(375.),
        RectanglePropertyPatch::Locked(true),
    ] {
        assert!(
            workspace
                .update(cx, |workspace, cx| workspace
                    .apply_rectangular_shape_property_event(
                        &RectanglePropertyEvent {
                            document_id,
                            annotation_id: ellipse_id.clone(),
                            expected_kind: RectangularShapePropertyKind::Ellipse,
                            patch,
                        },
                        cx,
                    ))
                .unwrap()
        );
    }
    let edited = workspace
        .read_with(cx, |workspace, cx| workspace.annotation_snapshot(document_id, cx))
        .unwrap();
    assert_eq!(edited.selected_id.as_ref(), Some(&ellipse_id));
    assert!(edited.dirty);
    assert_eq!(edited.ellipses.len(), 1);
    let expected = edited.ellipses[0].clone();
    let page_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(document_id, 0, cx)
    });
    let thumbnail_scene = workspace.read_with(cx, |workspace, cx| {
        workspace.thumbnail_annotation_scene(document_id, 0, cx)
    });
    assert_eq!(page_scene.ellipses[0].id, ellipse_id);
    assert_eq!(page_scene.ellipses[0].rect, thumbnail_scene.ellipses[0].rect);

    workspace
        .update(cx, |workspace, cx| {
            workspace.save_as_path(document_id, saved_path.clone(), cx)
        })
        .unwrap();
    cx.run_until_parked();
    let saved_worker_pid = workspace.read_with(cx, |workspace, cx| {
        let session = workspace.session(document_id, cx).unwrap().read(cx);
        assert_eq!(
            session.path(),
            saved_path.as_path(),
            "Save As must replace the source after validation: {:?}",
            session.save_status()
        );
        assert!(matches!(session.save_status(), NativeDocumentSaveStatus::Idle));
        assert!(session.current_base_raster().unwrap().has_spatial_variation());
        session.worker_pid().unwrap()
    });
    assert_eq!(
        workspace
            .read_with(cx, |workspace, cx| workspace.annotation_snapshot(document_id, cx))
            .unwrap()
            .ellipses[0],
        expected
    );
    assert_ne!(saved_worker_pid, original_worker_pid);
    assert!(!PathBuf::from(format!("/proc/{original_worker_pid}")).exists());
    assert!(PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());
    assert_eq!(
        workspace.update(cx, |workspace, cx| workspace.request_close_document(document_id, cx)),
        CloseRequestDisposition::Closed
    );
    assert!(!PathBuf::from(format!("/proc/{saved_worker_pid}")).exists());

    let fresh = cx.new(|cx| DocumentWorkspace::with_opener(backend, cx));
    let fresh_id = fresh.update(cx, |workspace, cx| workspace.open_path(saved_path.clone(), cx));
    cx.run_until_parked();
    let fresh_worker_pid = fresh.read_with(cx, |workspace, cx| {
        let session = workspace.session(fresh_id, cx).unwrap().read(cx);
        assert!(session.current_base_raster().unwrap().has_spatial_variation());
        assert!(session.thumbnail_base_raster(0).unwrap().has_spatial_variation());
        session.worker_pid().unwrap()
    });
    assert_eq!(
        fresh
            .read_with(cx, |workspace, cx| workspace.annotation_snapshot(fresh_id, cx))
            .unwrap()
            .ellipses,
        vec![expected]
    );
    assert!(PathBuf::from(format!("/proc/{fresh_worker_pid}")).exists());
    let fresh_scene = fresh.read_with(cx, |workspace, cx| {
        workspace.annotation_scene(fresh_id, 0, cx)
    });
    assert_eq!(fresh_scene.ellipses.len(), 1);
    assert_eq!(fresh_scene.ellipses[0].id, ellipse_id);
    assert!(fresh.update(cx, |workspace, cx| workspace.close_document(fresh_id, cx)));
    assert!(!PathBuf::from(format!("/proc/{fresh_worker_pid}")).exists());
    assert!(
        !surface_root.exists()
            || std::fs::read_dir(&surface_root)
                .expect("the real Ellipse surface root must be readable")
                .next()
                .is_none(),
        "both real workers must release every mapped surface"
    );
}
