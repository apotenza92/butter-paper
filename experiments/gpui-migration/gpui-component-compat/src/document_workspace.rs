use std::{
    cell::Cell,
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

pub use crate::document_resource::{
    DEFAULT_PAGE_RENDER_WIDTH, DEFAULT_THUMBNAIL_COUNT, DEFAULT_THUMBNAIL_WIDTH, DocumentId,
    DocumentRecoveryRequest, NativeDocumentOpener, NativeDocumentResource, OpenDocumentRequest,
    OpenedNativeDocument, PageRenderRequest, PageRotationPresentation, PageRotationRequest,
    PdfiumWorkerBackend, RasterSurface, ThumbnailSurface,
};
pub use crate::document_session::{
    DocumentSaveFailure, DocumentSaveFailureOperation, DocumentSaveRoute,
    HighlightCompositeEvidence, NativeDocumentSaveStatus, NativeDocumentSession,
    NativeDocumentStatus, PenAnnotationDefaults, SaveDestination, SaveDocumentRequest,
    SavedNativeDocument, resolve_document_save_route,
};
pub use crate::document_viewer::{DocumentViewerSnapshot, ViewerFitPreset, ViewerRenderQuality};
use crate::{
    accessible_button::accessible_icon_button,
    adaptive_performance::{AdaptivePerformanceSnapshot, ViewerRenderDiagnostics},
    cad_view_control::{
        CadViewControl, CadViewControlEvent, CadViewOrganisation as ControlCadOrganisation,
    },
    continuous_view_control::ContinuousViewControl,
    document_session::ThumbnailPresentation,
    document_tab_bar::{
        DOCUMENT_TAB_OPEN_ID, DOCUMENT_TAB_POINTER_DRAG_THRESHOLD,
        DOCUMENT_TAB_REORDER_DESCRIPTION, DOCUMENT_TAB_REORDER_KEYSHORTCUTS,
        DOCUMENT_TAB_REORDER_STATUS_ID, DocumentTabReorderEvent, DocumentTabReorderOrigin,
        TemplateCatalogItem, TemplateCreationEvent, TemplateSplitControl, TemplateSplitEvent,
        document_tab_close_accessible_label, document_tab_drag_id, document_tab_drop_target_id,
    },
    document_viewer::{ViewerTileJob, resolve_fit_zoom_percent},
    dimension_property_inspector::{
        DimensionPropertyEvent, DimensionPropertyInspector, DimensionPropertyPatch,
        DimensionPropertySnapshot,
    },
    engineering_visual_property_inspector::{
        EngineeringVisualPropertyEvent, EngineeringVisualPropertyInspector,
        EngineeringVisualPropertyKind, EngineeringVisualPropertyPatch,
        EngineeringVisualPropertySnapshot, EngineeringVisualPropertyValues,
    },
    ink_property_inspector::{
        InkPropertyEvent, InkPropertyInspector, InkPropertyPatch, InkPropertySnapshot,
    },
    local_signature::{DrawnSignature, NormalizedSignaturePoint},
    measurement_property_inspector::{
        MeasurementPropertyAction, MeasurementPropertyEvent, MeasurementPropertyInspector,
        MeasurementPropertySnapshot,
    },
    native_document_view_state::{
        CadViewOrganisation, DocumentNavigationAction, DocumentNavigationOutcome,
        NativeDocumentViewState, RestartView, ViewerZoomPreset, WheelOutcome,
    },
    page_scale_control::{
        CalibrationPointDisposition, PAGE_SCALE_PICK_ALERT_ID, PAGE_SCALE_PICK_CANCEL_ID,
        PAGE_SCALE_TRIGGER_ID, PageScaleControl,
    },
    page_view_control::{PageViewControl, PageViewControlEvent, PageViewMode},
    rectangle_property_inspector::{
        EllipsePropertyInspector, RectanglePropertyEvent, RectanglePropertyInspector,
        RectanglePropertyPatch, RectanglePropertySnapshot, RectangularShapePropertyKind,
    },
    session_manifest::{SessionRestorePlan, SessionSnapshot, normalized_path_key},
    straight_line_property_inspector::{
        StraightLinePropertyEvent, StraightLinePropertyInspector, StraightLinePropertyPatch,
        StraightLinePropertySnapshot,
    },
    vertex_path_property_inspector::{PathPropertyKind, VertexPathPropertyEvent, VertexPathPropertyInspector, VertexPathPropertyPatch, VertexPathPropertySnapshot},
    text_box_property_inspector::{
        TextBoxPropertyEvent, TextBoxPropertyInspector, TextBoxPropertyPatch,
        TextBoxPropertySnapshot,
    },
    viewer_toolbar_strip::{FitPreset, ViewerToolbarStrip, ViewerToolbarStripEvent},
    zoom_control::{
        DEFAULT_VIEWER_ZOOM, MAX_VIEWER_ZOOM, MIN_VIEWER_ZOOM, ZOOM_STEP_FACTOR, ZoomControl,
        ZoomControlEvent,
    },
};
use butter_paper_gpui_gallery::viewer::CadOrganisation as PlannerCadOrganisation;
use butter_paper_gpui_gallery::{
    annotation_adapter::{
        AnnotationAdapter, AnnotationTool, CALLOUT_BODY_ID, CALLOUT_TEXT_BOX_ID, CLOUD_BODY_ID,
        DIMENSION_BODY_ID, DIMENSION_END_HANDLE_ID, DIMENSION_OFFSET_HANDLE_ID,
        DIMENSION_START_HANDLE_ID, LENGTH_SCALE_REQUIRED_MESSAGE, PointerInputModifiers,
        PointerPhaseOutcome, StraightLinePropertyEdit, VertexPathPropertyEdit, ellipse_resize_handle_point_for_rect,
        ellipse_rotation_handle_point_for_rect, redact_resize_handle_id, snapshot_resize_handle_id,
    },
    annotation_model::{
        Annotation, AnnotationError, AnnotationKind, AnnotationScene, AnnotationSnapshot,
        ArcControlPoint, InkTool, LengthCalibration, LengthEndpoint, LineKind, MarkupId,
        DimensionAnnotation,
        MeasurementPathKind, PENDING_REDACTION_STATUS, PageRotation, PageRotationDirection,
        PageScale, PageScaleApplyTarget, PageTransform, PdfPoint, PdfRect, PenAppearance,
        PointerCancelReason, RectangleAppearance, RectangleResizeHandle, ScalePreset, SceneArc,
        SceneCloudPlus, SceneDimension, SceneRectangle, SceneRedact,
        StrokeStyle, TextAlignment, TextBoxAnnotation, TextBoxStyle, VertexPathKind, built_in_scale_presets,
        ellipse_cubic_bezier_points, rectangle_world_corners,
    },
    annotation_paint_path::{InkPaintPathSegment, build_ink_paint_path},
    generated_document::{
        GeneratedDocumentRequest, GeneratedDocumentStore, GeneratedPattern, OwnedGeneratedDocument,
    },
    image_asset_decode::{
        DecodedImageFile, SanitizedSignatureFile, decode_image_path, sanitize_signature_path,
    },
    page_geometry::PageCoordinateSpace,
    pdf_engine::{InPlacePublicationCapability, PdfPersistenceSession, PdfPublicationOutcome},
    pdf_file_authority::{SaveAsTargetAuthority, SaveTargetErrorKind},
    selection_geometry::{SelectionMarquee, SelectionPoint, SelectionShape},
    semantic_snapping::{
        SemanticSnapDecision, SemanticSnapRole, SemanticSnapSettings, SemanticSnapTarget,
    },
    viewer::{PageLayout, TileRequest},
};
use gpui::{
    Anchor, App, AppContext as _, BorderStyle, Bounds, ClickEvent, ContentMask, Context,
    DispatchPhase, Entity, EventEmitter, FocusHandle, Focusable as _, InteractiveElement as _,
    IntoElement, KeyBinding, KeyDownEvent, Modifiers, MouseButton, MouseDownEvent, MouseExitEvent,
    MouseMoveEvent, MouseUpEvent, ObjectFit, ParentElement as _, PathBuilder, PathPromptOptions,
    Pixels, Point, Render, RenderImage, Role, ScrollHandle, ScrollStrategy, ScrollWheelEvent,
    SharedString, StatefulInteractiveElement as _, Styled as _, StyledImage as _, Subscription,
    Task, TextAlign, TextRun, UniformListScrollHandle, WeakEntity, Window, accesskit::Live, canvas,
    fill, font, img, outline, point, prelude::FluentBuilder as _, px, relative, size, uniform_list,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, IconName, Selectable as _, Sizable as _, StyledExt as _,
    WindowExt as _,
    alert::Alert,
    button::{Button, ButtonGroup, ButtonVariants as _},
    checkbox::Checkbox,
    h_flex,
    input::{
        Copy, Cut, Delete, Escape, InputEvent, Paste, Redo, SelectAll,
        Textarea, TextareaState, Undo,
    },
    popover::Popover,
    progress::Progress,
    resizable::{ResizableState, h_resizable, resizable_panel},
    scroll::ScrollableElement as _,
    spinner::Spinner,
    tab::{Tab, TabBar},
    try_parse_color, v_flex,
};
use image::{Frame, ImageBuffer, Rgba};

gpui::actions!(
    document_workspace,
    [
        OpenPdf,
        NewFromTemplate,
        SaveDocumentAsTemplate,
        Save,
        SaveAs,
        NavigateHome,
        NavigateEnd,
        NavigatePreviousPage,
        NavigateNextPage,
        NavigateUp,
        NavigateDown,
        NavigateLeft,
        NavigateRight,
        NavigatePageUp,
        NavigatePageDown,
        CloseDocument,
        RotatePageLeft,
        RotatePageRight,
        ZoomIn,
        ZoomOut,
        ActualSize,
        FitWidth,
        FitPage,
        ContinuousView,
        SinglePageView,
        SelectLineTool,
        SelectArcTool,
        SelectArrowTool,
        SelectPolylineTool,
        SelectPolygonTool,
        SelectPolylengthTool,
        SelectAreaTool,
        SelectCloudPlusTool,
        SelectDimensionTool,
        SelectHighlightTool,
        SelectImageTool,
        SelectSnapshotTool,
        SelectLengthTool,
        FinishVertexPath
    ]
);

const DOCUMENT_WORKSPACE_CONTEXT: &str = "DocumentWorkspace";

pub fn init_document_workspace_actions(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenPdf, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("ctrl-o", OpenPdf, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("cmd-n", NewFromTemplate, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("ctrl-n", NewFromTemplate, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("cmd-s", Save, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("ctrl-s", Save, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("cmd-shift-s", SaveAs, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("ctrl-shift-s", SaveAs, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("home", NavigateHome, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("end", NavigateEnd, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new(
            "cmd-left",
            NavigatePreviousPage,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        KeyBinding::new(
            "cmd-right",
            NavigateNextPage,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        KeyBinding::new("up", NavigateUp, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("down", NavigateDown, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("left", NavigateLeft, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("right", NavigateRight, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("pageup", NavigatePageUp, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new(
            "pagedown",
            NavigatePageDown,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-w", CloseDocument, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-w", CloseDocument, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new(
            "cmd-shift-w",
            crate::application_close_workspace::RequestApplicationClose,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new(
            "ctrl-shift-w",
            crate::application_close_workspace::RequestApplicationClose,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-=", ZoomIn, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-=", ZoomIn, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd--", ZoomOut, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl--", ZoomOut, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-0", ActualSize, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-0", ActualSize, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("l", SelectLineTool, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("shift-c", SelectArcTool, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("a", SelectArrowTool, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new(
            "shift-n",
            SelectPolylineTool,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        KeyBinding::new(
            "shift-p",
            SelectPolygonTool,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        KeyBinding::new(
            "shift-alt-q",
            SelectPolylengthTool,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        KeyBinding::new(
            "shift-alt-a",
            SelectAreaTool,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        KeyBinding::new("h", SelectHighlightTool, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("k", SelectCloudPlusTool, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new(
            "shift-l",
            SelectDimensionTool,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        KeyBinding::new("i", SelectImageTool, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("g", SelectSnapshotTool, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new(
            "shift-alt-l",
            SelectLengthTool,
            Some(DOCUMENT_WORKSPACE_CONTEXT),
        ),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", SelectAll, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", SelectAll, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", Copy, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", Copy, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", Paste, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", Paste, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", Cut, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", Cut, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-z", Undo, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-z", Undo, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-z", Redo, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-y", Redo, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("backspace", Delete, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("enter", FinishVertexPath, Some(DOCUMENT_WORKSPACE_CONTEXT)),
        KeyBinding::new("escape", Escape, Some(DOCUMENT_WORKSPACE_CONTEXT)),
    ]);
}

pub fn register_document_workspace_global_actions(
    workspace: &Entity<DocumentWorkspace>,
    cx: &mut App,
) {
    let open_workspace = workspace.downgrade();
    cx.on_action(move |_: &OpenPdf, cx| {
        if let Some(workspace) = open_workspace.upgrade() {
            workspace.update(cx, |workspace, cx| workspace.prompt_to_open_documents(cx));
        }
    });
    let template_workspace = workspace.downgrade();
    cx.on_action(move |_: &NewFromTemplate, cx| {
        if let Some(workspace) = template_workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.template_manage_requests =
                    workspace.template_manage_requests.saturating_add(1);
                cx.emit(DocumentWorkspaceTemplateCommand::Manage);
                cx.notify();
            });
        }
    });
    let save_template_workspace = workspace.downgrade();
    cx.on_action(move |_: &SaveDocumentAsTemplate, cx| {
        if let Some(workspace) = save_template_workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                workspace.handle_template_split_event(
                    TemplateSplitEvent::SaveDocumentAsTemplateRequested,
                    cx,
                );
            });
        }
    });
    let save_workspace = workspace.downgrade();
    cx.on_action(move |_: &Save, cx| {
        if let Some(workspace) = save_workspace.upgrade() {
            workspace.update(cx, |workspace, cx| workspace.save_active_document(cx));
        }
    });
    let save_as_workspace = workspace.downgrade();
    cx.on_action(move |_: &SaveAs, cx| {
        if let Some(workspace) = save_as_workspace.upgrade() {
            workspace.update(cx, |workspace, cx| {
                if let Some(document_id) = workspace.active_document_id {
                    workspace.prompt_to_save_as(document_id, cx);
                }
            });
        }
    });
}

pub const DOCUMENT_WORKSPACE_ID: &str = "document-workspace";
pub const DOCUMENT_THUMBNAIL_STRIP_ID: &str = "document-thumbnail-strip";
pub const DOCUMENT_PAGE_ID: &str = "document-current-page";
pub const DOCUMENT_VIEWPORT_ID: &str = "document-native-viewport";
pub const DOCUMENT_OPEN_STATUS_ID: &str = "document-open-status";
pub const DOCUMENT_OPEN_PROGRESS_ID: &str = "document-open-progress";
pub const DOCUMENT_VIEWER_STATUS_ID: &str = "document-viewer-status";
pub const DOCUMENT_VIEWER_PROGRESS_ID: &str = "document-viewer-progress";
pub const DOCUMENT_RECOVERY_ALERT_ID: &str = "document-worker-recovery-alert";
pub const DOCUMENT_RECOVERY_RETRY_ID: &str = "document-worker-recovery-retry";
pub const DOCUMENT_CLOSE_ID: &str = "document-workspace-close";
pub const DOCUMENT_RECTANGLE_TOOL_ID: &str = "document-workspace-rectangle-tool";
pub const DOCUMENT_ELLIPSE_TOOL_ID: &str = "document-workspace-ellipse-tool";
pub const DOCUMENT_ARC_TOOL_ID: &str = "tool-arc";
pub const DOCUMENT_ARC_PREVIEW_MARKER_ID: &str = "document-workspace-arc-preview-marker";
pub const DOCUMENT_LINE_TOOL_ID: &str = "document-workspace-line-tool";
pub const DOCUMENT_ARROW_TOOL_ID: &str = "document-workspace-arrow-tool";
pub const DOCUMENT_POLYLINE_TOOL_ID: &str = "document-workspace-polyline-tool";
pub const DOCUMENT_POLYGON_TOOL_ID: &str = "document-workspace-polygon-tool";
pub const DOCUMENT_POLYLENGTH_TOOL_ID: &str = "tool-polylength";
pub const DOCUMENT_AREA_TOOL_ID: &str = "tool-area";
pub const DOCUMENT_CLOUD_TOOL_ID: &str = "tool-cloud";
pub const DOCUMENT_CLOUD_PLUS_TOOL_ID: &str = "tool-cloud-plus";
pub const DOCUMENT_DIMENSION_TOOL_ID: &str = "tool-dimension";
pub const DOCUMENT_CALLOUT_TOOL_ID: &str = "tool-callout";
pub const DOCUMENT_REDACT_TOOL_ID: &str = "tool-redact";
pub const DOCUMENT_REDACT_PENDING_ALERT_ID: &str = "document-workspace-redact-pending-alert";
pub const DOCUMENT_TOOLBAR_SCROLL_ID: &str = "document-workspace-toolbar-scroll";
pub const DOCUMENT_TOOLBAR_CONTENT_ID: &str = "document-workspace-toolbar-content";
pub const DOCUMENT_ACTIVE_INSPECTOR_SLOT_ID: &str = "document-workspace-active-inspector-slot";
pub const DOCUMENT_PEN_TOOL_ID: &str = "document-workspace-pen-tool";
pub const DOCUMENT_INK_PROPERTIES_ID: &str = "document-workspace-ink-properties";
pub const DOCUMENT_ENGINEERING_VISUAL_PROPERTIES_ID: &str =
    "document-workspace-engineering-visual-properties";
pub const DOCUMENT_TEXT_BOX_PROPERTIES_ID: &str = "document-workspace-text-box-properties";
pub const DOCUMENT_TEXT_BOX_TOOL_ID: &str = "document-workspace-text-box-tool";
pub const DOCUMENT_HIGHLIGHT_TOOL_ID: &str = "document-workspace-highlight-tool";
pub const DOCUMENT_HIGHLIGHT_SETTINGS_ID: &str = "document-workspace-highlight-settings";
pub const DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID: &str = "document-workspace-highlight-color-yellow";
pub const DOCUMENT_HIGHLIGHT_COLOR_GREEN_ID: &str = "document-workspace-highlight-color-green";
pub const DOCUMENT_HIGHLIGHT_WIDTH_12_ID: &str = "document-workspace-highlight-width-12";
pub const DOCUMENT_HIGHLIGHT_WIDTH_18_ID: &str = "document-workspace-highlight-width-18";
pub const DOCUMENT_HIGHLIGHT_OPACITY_50_ID: &str = "document-workspace-highlight-opacity-50";
pub const DOCUMENT_HIGHLIGHT_OPACITY_100_ID: &str = "document-workspace-highlight-opacity-100";
pub const DOCUMENT_IMAGE_TOOL_ID: &str = "document-workspace-image-tool";
pub const DOCUMENT_SIGNATURE_TOOL_ID: &str = "document-workspace-signature-tool";
pub const DOCUMENT_SIGNATURE_POPOVER_ID: &str = "document-workspace-signature-popover";
pub const DOCUMENT_SIGNATURE_CHOOSE_IMAGE_ID: &str = "document-workspace-signature-choose-image";
pub const DOCUMENT_SIGNATURE_CANVAS_ID: &str = "document-workspace-signature-canvas";
pub const DOCUMENT_SIGNATURE_CLEAR_ID: &str = "document-workspace-signature-clear";
pub const DOCUMENT_SIGNATURE_PREVIEW_ID: &str = "document-workspace-signature-preview";
pub const DOCUMENT_SIGNATURE_ADD_ID: &str = "document-workspace-signature-add";
pub const DOCUMENT_SIGNATURE_ERROR_ALERT_ID: &str = "document-workspace-signature-error-alert";
pub const DOCUMENT_SIGNATURE_LOADING_ID: &str = "document-workspace-signature-loading";
pub const DOCUMENT_SNAPSHOT_TOOL_ID: &str = "tool-snapshot";
pub const DOCUMENT_SNAP_SETTINGS_ID: &str = "viewer-snap-target-menu";
pub const DOCUMENT_SNAP_POPOVER_ID: &str = "viewer-snap-popover";
pub const DOCUMENT_SNAP_MARKUP_ID: &str = "viewer-snap-markup";
pub const DOCUMENT_SNAP_ENDPOINT_ID: &str = "viewer-snap-target-endpoint";
pub const DOCUMENT_SNAP_MIDPOINT_ID: &str = "viewer-snap-target-midpoint";
pub const DOCUMENT_SNAP_CENTER_ID: &str = "viewer-snap-target-center";
pub const DOCUMENT_SNAP_INTERSECTION_ID: &str = "viewer-snap-target-intersection";
pub const DOCUMENT_SNAP_NEAREST_ID: &str = "viewer-snap-target-nearest";
pub const DOCUMENT_LENGTH_TOOL_ID: &str = "document-workspace-length-tool";
pub const DOCUMENT_MEASUREMENT_PROPERTIES_ID: &str = "document-workspace-measurement-properties";
pub const DOCUMENT_ANNOTATION_STATUS_ID: &str = "document-workspace-annotation-status";
pub const DOCUMENT_TEXT_BOX_EDITOR_ID: &str = "document-workspace-text-box-editor";
pub const DOCUMENT_TEXT_BOX_COMMIT_ERROR_ALERT_ID: &str =
    "document-workspace-text-box-commit-error-alert";
pub const DOCUMENT_TEXT_BOX_RETURN_FOCUS_ID: &str = "document-workspace-text-box-return-focus";
pub const DOCUMENT_SELECT_TOOL_ID: &str = "document-workspace-select-tool";
pub const DOCUMENT_ANNOTATION_UNDO_ID: &str = "document-workspace-annotation-undo";
pub const DOCUMENT_ANNOTATION_REDO_ID: &str = "document-workspace-annotation-redo";
pub const DOCUMENT_ANNOTATION_DELETE_ID: &str = "document-workspace-annotation-delete";
pub const DOCUMENT_ANNOTATION_LOCK_ID: &str = "document-workspace-annotation-lock";
pub const DOCUMENT_RECTANGLE_STROKE_ID: &str = "document-workspace-rectangle-stroke";
pub const DOCUMENT_RECTANGLE_PROPERTIES_ID: &str = "document-workspace-rectangle-properties";
pub const DOCUMENT_ELLIPSE_PROPERTIES_ID: &str = "document-workspace-ellipse-properties";
pub const DOCUMENT_DIMENSION_PROPERTIES_ID: &str = "document-workspace-dimension-properties";
pub const DIMENSION_PROPERTY_OFFSET_ID: &str = "dimension-property-offset";
pub const DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID: &str =
    "document-workspace-straight-line-properties";
pub const DOCUMENT_VERTEX_PATH_PROPERTIES_ID: &str = "document-workspace-vertex-path-properties";
pub const DOCUMENT_STRAIGHT_LINE_COLOR_BLUE_ID: &str =
    "document-workspace-straight-line-color-blue";
pub const DOCUMENT_STRAIGHT_LINE_WIDTH_ID: &str = "document-workspace-straight-line-width";
pub const DOCUMENT_STRAIGHT_LINE_WIDTH_4_ID: &str = "document-workspace-straight-line-width-4";
pub const DOCUMENT_STRAIGHT_LINE_OPACITY_ID: &str = "document-workspace-straight-line-opacity";
pub const DOCUMENT_STRAIGHT_LINE_OPACITY_50_ID: &str =
    "document-workspace-straight-line-opacity-50";
pub const DOCUMENT_SAVE_ID: &str = "document-workspace-save";
pub const DOCUMENT_SAVE_AS_ID: &str = "document-workspace-save-as";
pub const DOCUMENT_SAVE_ERROR_ALERT_ID: &str = "document-workspace-save-error-alert";
pub const DOCUMENT_SAVE_ERROR_RETRY_ID: &str = "document-workspace-save-error-retry";
pub const DOCUMENT_SAVE_ERROR_SAVE_AS_ID: &str = "document-workspace-save-error-save-as";
pub const DOCUMENT_SAVE_ERROR_DISMISS_ID: &str = "document-workspace-save-error-dismiss";
pub const DOCUMENT_ROTATE_LEFT_ID: &str = "document-workspace-rotate-left";
pub const DOCUMENT_ROTATE_RIGHT_ID: &str = "document-workspace-rotate-right";
pub const DOCUMENT_SESSION_TABS_ID: &str = "document-workspace-session-tabs";
pub const DOCUMENT_DIRTY_CLOSE_ID: &str = "document-workspace-dirty-close";
pub const DOCUMENT_DIRTY_CLOSE_CANCEL_ID: &str = "document-workspace-dirty-close-cancel";
pub const DOCUMENT_DIRTY_CLOSE_DISCARD_ID: &str = "document-workspace-dirty-close-discard";
pub const DOCUMENT_DIRTY_CLOSE_SAVE_ID: &str = "document-workspace-dirty-close-save";
pub const DOCUMENT_EMPTY_ID: &str = "document-workspace-empty";
pub const DOCUMENT_ERROR_ID: &str = "document-workspace-error";
pub const DOCUMENT_OPEN_ERROR_ALERT_ID: &str = "document-workspace-open-feedback-alert";
pub const DOCUMENT_OPEN_ERROR_DISMISS_ID: &str = "document-workspace-open-feedback-dismiss";
pub const VIEWPORT_OPEN_DOCUMENT_ID: &str = "viewport-open-document";

fn is_pdf_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.as_encoded_bytes().eq_ignore_ascii_case(b"pdf"))
}

fn normalized_document_path(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .as_encoded_bytes()
        .iter()
        .copied()
        .map(|byte| {
            let byte = if cfg!(target_os = "windows") && byte == b'\\' {
                b'/'
            } else {
                byte
            };
            if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
                byte.to_ascii_lowercase()
            } else {
                byte
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentOpenOrigin {
    Picker,
    Menu,
    System,
    Drop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentOpenBatchRequest {
    origin: DocumentOpenOrigin,
    paths: Vec<PathBuf>,
    cancelled: bool,
}

impl DocumentOpenBatchRequest {
    pub fn new(origin: DocumentOpenOrigin, paths: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            origin,
            paths: paths.into_iter().collect(),
            cancelled: false,
        }
    }

    pub fn cancelled(origin: DocumentOpenOrigin) -> Self {
        Self {
            origin,
            paths: Vec::new(),
            cancelled: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentOpenBatchDisposition {
    Cancelled,
    NoAcceptedPaths,
    Started {
        batch_id: u64,
        candidate_count: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentOpenBatchStatus {
    Idle,
    Opening {
        batch_id: u64,
        origin: DocumentOpenOrigin,
        candidate_count: usize,
    },
    Completed {
        batch_id: u64,
        opened: Vec<DocumentId>,
        focused_existing: Option<DocumentId>,
        failed_count: usize,
        status_message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentOpenFailure {
    pub path: PathBuf,
    pub message: String,
}

impl DocumentOpenFailure {
    fn presentation(&self) -> String {
        let name = self
            .path
            .file_name()
            .unwrap_or_else(|| self.path.as_os_str())
            .to_string_lossy();
        format!("{name}: {}", self.message)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SaveAsPromptSpec {
    pub directory: PathBuf,
    pub suggested_name: String,
}

pub fn save_as_prompt_spec(source_path: &Path) -> SaveAsPromptSpec {
    let directory = source_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .to_owned();
    let stem = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .unwrap_or("butter-paper");
    SaveAsPromptSpec {
        directory,
        suggested_name: format!("{stem}-annotated.pdf"),
    }
}

pub const fn save_as_command_label(save_in_progress: bool) -> &'static str {
    if save_in_progress {
        "Saving…"
    } else {
        "Save As…"
    }
}

fn generated_document_request_for_template(
    template_id: &str,
) -> Result<GeneratedDocumentRequest, String> {
    let pattern = match template_id {
        "built-in-blank" => None,
        "built-in-dots" => Some(GeneratedPattern::Dots {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-grid" => Some(GeneratedPattern::SquareGrid {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-lined" => Some(GeneratedPattern::Ruled {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-isometric" => Some(GeneratedPattern::Isometric {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-triangle" => Some(GeneratedPattern::Triangle {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        _ => return Err(format!("Unknown built-in template: {template_id}")),
    };
    Ok(GeneratedDocumentRequest {
        title: "Untitled".into(),
        width_mm: 420.,
        height_mm: 297.,
        pattern,
    })
}

pub fn document_thumbnail_id(document_id: DocumentId, page_index: u32) -> String {
    format!("{document_id}-thumbnail-{page_index}")
}

pub fn document_annotation_layer_id(document_id: DocumentId, page_index: u32) -> String {
    format!("{document_id}-annotation-layer-{page_index}")
}

pub const DOCUMENT_SNAP_INDICATOR_ID: &str = "snap-indicator";

pub fn document_viewer_page_id(document_id: DocumentId, page_index: usize) -> String {
    format!("{document_id}-viewer-page-{page_index}")
}

pub fn document_viewer_quality_id(
    document_id: DocumentId,
    page_index: usize,
    quality: ViewerRenderQuality,
) -> String {
    let quality = match quality {
        ViewerRenderQuality::Preview => "preview",
        ViewerRenderQuality::Full => "full",
        ViewerRenderQuality::Detail => "detail",
    };
    format!("{document_id}-viewer-page-{page_index}-quality-{quality}")
}

pub fn document_viewer_error_id(document_id: DocumentId, page_index: usize) -> String {
    format!("{document_id}-viewer-page-{page_index}-render-error")
}

pub fn document_viewer_retry_id(document_id: DocumentId, page_index: usize) -> String {
    format!("{document_id}-viewer-page-{page_index}-render-retry")
}

pub fn document_viewer_tile_id(
    document_id: DocumentId,
    generation: u64,
    page_index: usize,
    tile_index: usize,
) -> String {
    format!("{document_id}-viewer-{generation}-page-{page_index}-tile-{tile_index}")
}

pub fn document_session_tab_id(document_id: DocumentId) -> String {
    format!("{document_id}-session-tab")
}

pub fn document_session_close_id(document_id: DocumentId) -> String {
    format!("{document_id}-session-close")
}

#[derive(Debug)]
pub struct ViewerPlanEvidence {
    pub generation: u64,
    pub page_layouts: Vec<PageLayout>,
    pub visible_pages: Vec<usize>,
    pub current_page: Option<usize>,
    pub total_height: f32,
    pub total_width: f32,
    pub tiles: Vec<TileRequest>,
    pub requested_bytes: usize,
    pub cache_max_bytes: usize,
}

/// Copyable proof that a current viewer page reached the native prepaint boundary.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintedPageEvidence {
    pub document_id: DocumentId,
    pub page_index: u32,
    pub source_pdf_page_size_points: (f32, f32),
    pub contained_bounds: Bounds<Pixels>,
    pub viewer_generation: u64,
    pub request_generation: u64,
    pub resource_generation: u64,
    pub painted_state_sequence: u64,
    pub rendered_dpr: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileRenderEvidence {
    pub generation: u64,
    pub rendered_tiles: usize,
    pub cache_hits: usize,
    pub cache_entries: usize,
    pub cache_bytes: usize,
    pub non_uniform_tiles: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentWorkspaceEvidenceSnapshot {
    pub document_id: DocumentId,
    pub request_generation: u64,
    pub ready: bool,
    pub failure: Option<String>,
    pub current_page: u32,
    pub requested_page: u32,
    pub page_count: usize,
    pub current_raster_width: u32,
    pub current_raster_height: u32,
    pub current_raster_bytes: usize,
    pub current_raster_has_spatial_variation: bool,
    pub thumbnail_count: usize,
    pub worker_pid: Option<u32>,
    pub resource_present: bool,
    pub viewer_generation: u64,
    pub viewer_tile_count: usize,
    pub viewer_cache_bytes: usize,
    pub rendered_device_pixel_ratio: Option<f32>,
    pub annotation_revision: u64,
    pub annotation_dirty: bool,
    pub presentation_error: Option<String>,
    pub recovery_pending: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApplyDisposition {
    Applied,
    RejectedStale,
    RejectedClosed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeneratedTemplateRequestDisposition {
    Started(DocumentId),
    SuppressedPending,
    Rejected(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentWorkspaceTemplateCommand {
    Create(TemplateCreationEvent),
    Manage,
    SaveDocumentAsTemplate {
        document_id: DocumentId,
        document_name: String,
        authorized_source: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseRequestDisposition {
    Closed,
    ConfirmationRequired,
    ReleaseFailed,
    NotFound,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirtyCloseResolution {
    Cancelled,
    Discarded,
    ReleaseFailed,
    NoPendingDocument,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DocumentCommandState {
    pub can_close_document: bool,
    pub document_ready: bool,
    pub save_busy: bool,
    pub can_previous_page: bool,
    pub can_next_page: bool,
    pub rotation_busy: bool,
    pub can_zoom_out: bool,
    pub can_zoom_in: bool,
    pub actual_size_checked: bool,
    pub fit_width_checked: bool,
    pub fit_page_checked: bool,
    pub continuous_view_checked: bool,
    pub single_page_view_checked: bool,
}

pub trait NativeDocumentSaver: Send + Sync {
    fn save(&self, request: &SaveDocumentRequest) -> Result<SavedNativeDocument, String>;
}

pub struct PdfDocumentSaver {
    opener: Arc<dyn NativeDocumentOpener>,
}

impl PdfDocumentSaver {
    pub fn new(opener: Arc<dyn NativeDocumentOpener>) -> Self {
        Self { opener }
    }
}

impl NativeDocumentSaver for PdfDocumentSaver {
    fn save(&self, request: &SaveDocumentRequest) -> Result<SavedNativeDocument, String> {
        if request.is_in_place()
            && PdfPersistenceSession::in_place_publication_capability()
                == InPlacePublicationCapability::NewTargetRequired
        {
            return Err("in-place Save requires a new target on this platform".into());
        }
        let mut persistence = match request.expected_source_sha256 {
            Some(expected) => {
                PdfPersistenceSession::open_for_update(&request.source_path, expected)
            }
            None if request.is_in_place() => {
                return Err("in-place Save requires a verified source digest".into());
            }
            None => PdfPersistenceSession::open(&request.source_path),
        }
        .map_err(|error| error.to_string())?;
        let deleted_snapshot_ids = persistence
            .snapshots()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .snapshots
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|snapshot| snapshot.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_snapshot_ids {
            persistence
                .remove_snapshot(id)
                .map_err(|error| error.to_string())?;
        }
        for snapshot in &request.annotations.snapshots {
            if persistence
                .snapshots()
                .iter()
                .any(|imported| imported.id == snapshot.id)
            {
                persistence
                    .replace_snapshot(snapshot.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_snapshot(snapshot.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_image_ids = persistence
            .images()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .images
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|image| image.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_image_ids {
            persistence
                .remove_image(id)
                .map_err(|error| error.to_string())?;
        }
        for image in &request.annotations.images {
            if persistence
                .images()
                .iter()
                .any(|imported| imported.id == image.id)
            {
                persistence
                    .replace_image(image.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_image(image.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_rectangle_ids = persistence
            .rectangles()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .rectangles
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|rectangle| rectangle.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_rectangle_ids {
            persistence
                .remove_rectangle(id)
                .map_err(|error| error.to_string())?;
        }
        for rectangle in &request.annotations.rectangles {
            if persistence
                .rectangles()
                .iter()
                .any(|imported| imported.id == rectangle.id)
            {
                persistence
                    .replace_rectangle(rectangle.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_rectangle(rectangle.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_ellipse_ids = persistence
            .ellipses()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .ellipses
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|ellipse| ellipse.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_ellipse_ids {
            persistence
                .remove_ellipse(id)
                .map_err(|error| error.to_string())?;
        }
        for ellipse in &request.annotations.ellipses {
            if persistence
                .ellipses()
                .iter()
                .any(|imported| imported.id == ellipse.id)
            {
                persistence
                    .replace_ellipse(ellipse.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_ellipse(ellipse.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_arc_ids = persistence
            .arcs()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .arcs
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|arc| arc.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_arc_ids {
            persistence
                .remove_arc(id)
                .map_err(|error| error.to_string())?;
        }
        for arc in &request.annotations.arcs {
            if persistence
                .arcs()
                .iter()
                .any(|imported| imported.id == arc.id)
            {
                persistence
                    .replace_arc(arc.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_arc(arc.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_redact_ids = persistence
            .redacts()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .redacts
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|redact| redact.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_redact_ids {
            persistence
                .remove_redact(id)
                .map_err(|error| error.to_string())?;
        }
        for redact in &request.annotations.redacts {
            if persistence
                .redacts()
                .iter()
                .any(|imported| imported.id == redact.id)
            {
                persistence
                    .replace_redact(redact.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_redact(redact.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_straight_line_ids = persistence
            .straight_lines()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .straight_lines
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|line| line.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_straight_line_ids {
            persistence
                .remove_straight_line(id)
                .map_err(|error| error.to_string())?;
        }
        for line in &request.annotations.straight_lines {
            if persistence
                .straight_lines()
                .iter()
                .any(|imported| imported.id == line.id)
            {
                persistence
                    .replace_straight_line(line.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_straight_line(line.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_vertex_path_ids = persistence
            .vertex_paths()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .vertex_paths
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|path| path.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_vertex_path_ids {
            persistence
                .remove_vertex_path(id)
                .map_err(|error| error.to_string())?;
        }
        for path in &request.annotations.vertex_paths {
            if persistence
                .vertex_paths()
                .iter()
                .any(|imported| imported.id == path.id)
            {
                persistence
                    .replace_vertex_path(path.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_vertex_path(path.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_cloud_ids = persistence
            .clouds()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .clouds
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|cloud| cloud.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_cloud_ids {
            persistence
                .remove_cloud(id)
                .map_err(|error| error.to_string())?;
        }
        for cloud in &request.annotations.clouds {
            if persistence
                .clouds()
                .iter()
                .any(|imported| imported.id == cloud.id)
            {
                persistence
                    .replace_cloud(cloud.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_cloud(cloud.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_cloud_plus_ids = persistence
            .cloud_pluses()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .cloud_pluses
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|cloud_plus| cloud_plus.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_cloud_plus_ids {
            persistence
                .remove_cloud_plus(id)
                .map_err(|error| error.to_string())?;
        }
        for cloud_plus in &request.annotations.cloud_pluses {
            if persistence
                .cloud_pluses()
                .iter()
                .any(|imported| imported.id == cloud_plus.id)
            {
                persistence
                    .replace_cloud_plus(cloud_plus.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_cloud_plus(cloud_plus.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_dimension_ids = persistence
            .dimensions()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .dimensions
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|dimension| dimension.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_dimension_ids {
            persistence
                .remove_dimension(id)
                .map_err(|error| error.to_string())?;
        }
        for dimension in &request.annotations.dimensions {
            if persistence
                .dimensions()
                .iter()
                .any(|imported| imported.id == dimension.id)
            {
                persistence
                    .replace_dimension(dimension.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_dimension(dimension.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_callout_ids = persistence
            .callouts()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .callouts
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|callout| callout.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_callout_ids {
            persistence
                .remove_callout(id)
                .map_err(|error| error.to_string())?;
        }
        for callout in &request.annotations.callouts {
            if persistence
                .callouts()
                .iter()
                .any(|imported| imported.id == callout.id)
            {
                persistence
                    .replace_callout(callout.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_callout(callout.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_measurement_path_ids = persistence
            .measurement_paths()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .measurement_paths
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|path| path.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_measurement_path_ids {
            persistence
                .remove_measurement_path(id)
                .map_err(|error| error.to_string())?;
        }
        for path in &request.annotations.measurement_paths {
            if persistence
                .measurement_paths()
                .iter()
                .any(|imported| imported.id == path.id)
            {
                persistence
                    .replace_measurement_path(path.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_measurement_path(path.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_pen_ids = persistence
            .pens()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .pens
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|pen| pen.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_pen_ids {
            persistence
                .remove_pen(id)
                .map_err(|error| error.to_string())?;
        }
        for pen in &request.annotations.pens {
            if persistence
                .pens()
                .iter()
                .any(|imported| imported.id == pen.id)
            {
                persistence
                    .replace_pen(pen.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_pen(pen.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_text_box_ids = persistence
            .text_boxes()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .text_boxes
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|text_box| text_box.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_text_box_ids {
            persistence
                .remove_text_box(id)
                .map_err(|error| error.to_string())?;
        }
        for text_box in &request.annotations.text_boxes {
            if persistence
                .text_boxes()
                .iter()
                .any(|imported| imported.id == text_box.id)
            {
                persistence
                    .replace_text_box(text_box.clone())
                    .map_err(|error| error.to_string())?;
            } else {
                persistence
                    .add_text_box(text_box.clone())
                    .map_err(|error| error.to_string())?;
            }
        }
        let deleted_length_ids = persistence
            .lengths()
            .iter()
            .filter(|imported| {
                request
                    .annotations
                    .lengths
                    .iter()
                    .all(|current| current.id != imported.id)
            })
            .map(|length| length.id.clone())
            .collect::<Vec<_>>();
        for id in &deleted_length_ids {
            persistence
                .remove_length(id)
                .map_err(|error| error.to_string())?;
        }
        let mut length_save_expectations = Vec::with_capacity(request.annotations.lengths.len());
        for length in &request.annotations.lengths {
            let expectation = if persistence
                .lengths()
                .iter()
                .any(|imported| imported.id == length.id)
            {
                persistence
                    .replace_length(length.clone())
                    .map_err(|error| error.to_string())?
            } else {
                persistence
                    .add_length(length.clone())
                    .map_err(|error| error.to_string())?
            };
            length_save_expectations.push((length.id.clone(), expectation));
        }
        if request.annotations.page_scales.is_empty()
            && !request.annotations.page_length_calibrations.is_empty()
        {
            for (page_index, calibration) in &request.annotations.page_length_calibrations {
                persistence
                    .set_page_length_calibration(*page_index, calibration.clone())
                    .map_err(|error| error.to_string())?;
            }
        } else {
            persistence
                .replace_page_scales(&request.annotations.page_scales)
                .map_err(|error| error.to_string())?;
        }
        for (page_index, rotation) in &request.annotations.page_rotations {
            persistence
                .set_page_rotation(*page_index, *rotation)
                .map_err(|error| error.to_string())?;
        }
        if !request.annotations.annotation_order.is_empty() {
            persistence
                .reorder_managed_annotations(&request.annotations.annotation_order)
                .map_err(|error| error.to_string())?;
        }
        let prepared = match &request.destination {
            SaveDestination::OpenedSource => {
                persistence.prepare_save_replacing(request.target_path())
            }
            SaveDestination::NewTarget(authority) => persistence.prepare_save_authorized(authority),
        }
        .map_err(|error| error.to_string())?;
        let validation_path = prepared.path().to_path_buf();
        let reopened =
            PdfPersistenceSession::open(&validation_path).map_err(|error| error.to_string())?;
        let page_scales_match = if request.annotations.page_scales.is_empty() {
            let reopened_page_scales = reopened
                .page_length_calibrations()
                .iter()
                .map(|(page_index, calibration)| (*page_index, calibration.clone()))
                .collect::<Vec<_>>();
            reopened_page_scales.len() == request.annotations.page_length_calibrations.len()
                && request.annotations.page_length_calibrations.iter().all(
                    |(page_index, expected)| {
                        reopened_page_scales.iter().any(|(candidate_page, actual)| {
                            candidate_page == page_index && actual.same_scale_as(expected)
                        })
                    },
                )
        } else {
            reopened.page_scales() == request.annotations.page_scales
        };
        if !page_scales_match {
            return Err("saved PDF page scales failed independent reopen validation".into());
        }
        let reopened_page_rotations = reopened
            .page_rotations()
            .iter()
            .map(|(page_index, rotation)| (*page_index, *rotation))
            .collect::<Vec<_>>();
        if !request.annotations.page_rotations.iter().all(|expected| {
            reopened_page_rotations
                .iter()
                .any(|actual| actual == expected)
        }) {
            return Err("saved PDF page rotations failed independent reopen validation".into());
        }
        if !request.annotations.annotation_order.is_empty()
            && reopened.annotation_order() != request.annotations.annotation_order
        {
            return Err(
                "saved PDF managed annotation order failed independent reopen validation".into(),
            );
        }
        for deleted_id in &deleted_rectangle_ids {
            if reopened
                .rectangles()
                .iter()
                .any(|rectangle| rectangle.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted rectangle {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted rectangle {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.rectangles {
            let Some(actual) = reopened
                .rectangles()
                .iter()
                .find(|rectangle| rectangle.id == expected.id)
            else {
                return Err(format!("saved PDF is missing rectangle {}", expected.id));
            };
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF rectangle {} failed typed reopen validation: expected {expected:?}, reopened {actual:?}",
                    expected.id,
                ));
            }
        }
        for deleted_id in &deleted_ellipse_ids {
            if reopened
                .ellipses()
                .iter()
                .any(|ellipse| ellipse.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted ellipse {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted ellipse {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.ellipses {
            let Some(actual) = reopened
                .ellipses()
                .iter()
                .find(|ellipse| ellipse.id == expected.id)
            else {
                return Err(format!("saved PDF is missing ellipse {}", expected.id));
            };
            if !actual.same_persisted_state_as(expected)
                || !reopened.ellipse_has_canonical_native_identity(&expected.id)
            {
                return Err(format!(
                    "saved PDF ellipse {} failed typed reopen validation",
                    expected.id
                ));
            }
        }
        for deleted_id in &deleted_arc_ids {
            if reopened.arcs().iter().any(|arc| arc.id == *deleted_id) {
                return Err(format!("saved PDF still contains deleted Arc {deleted_id}"));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted Arc {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.arcs {
            let Some(actual) = reopened.arcs().iter().find(|arc| arc.id == expected.id) else {
                return Err(format!("saved PDF is missing Arc {}", expected.id));
            };
            if !actual.same_persisted_state_as(expected)
                || !reopened.arc_has_canonical_native_identity(&expected.id)
            {
                return Err(format!(
                    "saved PDF Arc {} failed typed reopen validation",
                    expected.id
                ));
            }
        }
        for deleted_id in &deleted_redact_ids {
            if reopened
                .redacts()
                .iter()
                .any(|redact| redact.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains pending redaction {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains pending redaction {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.redacts {
            let Some(actual) = reopened
                .redacts()
                .iter()
                .find(|redact| redact.id == expected.id)
            else {
                return Err(format!(
                    "saved PDF is missing pending redaction {}",
                    expected.id
                ));
            };
            if !actual.same_persisted_state_as(expected)
                || !reopened.redact_has_canonical_native_identity(&expected.id)
            {
                return Err(format!(
                    "saved PDF pending redaction {} failed typed reopen validation: expected {expected:?}, reopened {actual:?}, canonical_identity={}",
                    expected.id,
                    reopened.redact_has_canonical_native_identity(&expected.id),
                ));
            }
        }
        for deleted_id in &deleted_straight_line_ids {
            if reopened
                .straight_lines()
                .iter()
                .any(|line| line.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted straight line {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted straight line {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.straight_lines {
            let Some(actual) = reopened
                .straight_lines()
                .iter()
                .find(|line| line.id == expected.id)
            else {
                return Err(format!(
                    "saved PDF is missing straight line {}",
                    expected.id
                ));
            };
            if !reopened.straight_line_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF straight line {} does not have canonical native identity",
                    expected.id
                ));
            }
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF straight line {} failed typed reopen validation: expected {expected:?}, reopened {actual:?}",
                    expected.id,
                ));
            }
        }
        for deleted_id in &deleted_vertex_path_ids {
            if reopened
                .vertex_paths()
                .iter()
                .any(|path| path.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted vertex path {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted vertex path {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.vertex_paths {
            let Some(actual) = reopened
                .vertex_paths()
                .iter()
                .find(|path| path.id == expected.id)
            else {
                return Err(format!("saved PDF is missing vertex path {}", expected.id));
            };
            if !reopened.vertex_path_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF vertex path {} does not have canonical native identity",
                    expected.id
                ));
            }
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF vertex path {} failed typed reopen validation",
                    expected.id
                ));
            }
        }
        for deleted_id in &deleted_cloud_ids {
            if reopened
                .clouds()
                .iter()
                .any(|cloud| cloud.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted cloud {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted cloud {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.clouds {
            let Some(actual) = reopened
                .clouds()
                .iter()
                .find(|cloud| cloud.id == expected.id)
            else {
                return Err(format!("saved PDF is missing cloud {}", expected.id));
            };
            if !reopened.cloud_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF cloud {} does not have canonical native identity",
                    expected.id
                ));
            }
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF cloud {} failed typed reopen validation",
                    expected.id
                ));
            }
        }
        for deleted_id in &deleted_cloud_plus_ids {
            if reopened
                .cloud_pluses()
                .iter()
                .any(|cloud_plus| cloud_plus.id == *deleted_id)
                || reopened.has_cloud_plus_native_fragment_names(deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted Cloud+ pair {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.cloud_pluses {
            let Some(actual) = reopened
                .cloud_pluses()
                .iter()
                .find(|cloud_plus| cloud_plus.id == expected.id)
            else {
                return Err(format!("saved PDF is missing Cloud+ {}", expected.id));
            };
            if !reopened.cloud_plus_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF Cloud+ {} does not have canonical paired native identity",
                    expected.id
                ));
            }
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF Cloud+ {} failed typed reopen validation: expected={expected:?} actual={actual:?}",
                    expected.id,
                ));
            }
        }
        for deleted_id in &deleted_dimension_ids {
            if reopened
                .dimensions()
                .iter()
                .any(|dimension| dimension.id == *deleted_id)
                || reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted Dimension {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.dimensions {
            let Some(actual) = reopened
                .dimensions()
                .iter()
                .find(|dimension| dimension.id == expected.id)
            else {
                return Err(format!("saved PDF is missing Dimension {}", expected.id));
            };
            if !reopened.dimension_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF Dimension {} does not have canonical native identity",
                    expected.id
                ));
            }
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF Dimension {} failed typed reopen validation: expected={expected:?} actual={actual:?}",
                    expected.id,
                ));
            }
        }
        for deleted_id in &deleted_measurement_path_ids {
            if reopened
                .measurement_paths()
                .iter()
                .any(|path| path.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted measurement path {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted measurement path {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.measurement_paths {
            let Some(actual) = reopened
                .measurement_paths()
                .iter()
                .find(|path| path.id == expected.id)
            else {
                return Err(format!(
                    "saved PDF is missing measurement path {}",
                    expected.id
                ));
            };
            if !reopened.measurement_path_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF measurement path {} does not have canonical native identity",
                    expected.id
                ));
            }
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF measurement path {} failed typed reopen validation",
                    expected.id
                ));
            }
        }
        for deleted_id in &deleted_pen_ids {
            if reopened.pens().iter().any(|pen| pen.id == *deleted_id) {
                return Err(format!("saved PDF still contains deleted pen {deleted_id}"));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted pen {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.pens {
            let Some(actual) = reopened.pens().iter().find(|pen| pen.id == expected.id) else {
                return Err(format!("saved PDF is missing pen {}", expected.id));
            };
            if !reopened.pen_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF pen {} does not have canonical native identity",
                    expected.id
                ));
            }
            if actual != expected {
                return Err(format!(
                    "saved PDF pen {} failed typed reopen validation",
                    expected.id
                ));
            }
        }
        for deleted_id in &deleted_text_box_ids {
            if reopened
                .text_boxes()
                .iter()
                .any(|text_box| text_box.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted text box {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id) {
                return Err(format!(
                    "saved PDF object graph still contains deleted text box {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.text_boxes {
            let Some(actual) = reopened
                .text_boxes()
                .iter()
                .find(|text_box| text_box.id == expected.id)
            else {
                return Err(format!("saved PDF is missing text box {}", expected.id));
            };
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF text box {} failed typed reopen validation: expected {expected:?}, reopened {actual:?}",
                    expected.id,
                ));
            }
        }
        for deleted_id in &deleted_length_ids {
            if reopened
                .lengths()
                .iter()
                .any(|length| length.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted length {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id) {
                return Err(format!(
                    "saved PDF object graph still contains deleted length {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.lengths {
            let Some(actual) = reopened
                .lengths()
                .iter()
                .find(|length| length.id == expected.id)
            else {
                return Err(format!("saved PDF is missing length {}", expected.id));
            };
            if !actual.same_persisted_state_as(expected) {
                return Err(format!(
                    "saved PDF length {} failed typed reopen validation: expected {expected:?}, reopened {actual:?}",
                    expected.id,
                ));
            }
            let Some((_, identity_expectation)) = length_save_expectations
                .iter()
                .find(|(id, _)| id == &expected.id)
            else {
                return Err(format!(
                    "saved PDF length {} has no retained identity expectation",
                    expected.id,
                ));
            };
            if !reopened.length_matches_save_expectation(identity_expectation) {
                return Err(format!(
                    "saved PDF length {} failed native identity validation",
                    expected.id,
                ));
            }
        }
        for deleted_id in &deleted_snapshot_ids {
            if reopened
                .snapshots()
                .iter()
                .any(|snapshot| snapshot.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted Snapshot {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted Snapshot {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.snapshots {
            let Some(actual) = reopened
                .snapshots()
                .iter()
                .find(|snapshot| snapshot.id == expected.id)
            else {
                return Err(format!("saved PDF is missing Snapshot {}", expected.id));
            };
            let geometry_matches = [
                (actual.rect.x, expected.rect.x),
                (actual.rect.y, expected.rect.y),
                (actual.rect.width, expected.rect.width),
                (actual.rect.height, expected.rect.height),
                (actual.rotation_degrees(), expected.rotation_degrees()),
                (actual.opacity(), expected.opacity()),
            ]
            .into_iter()
            .all(|(actual, expected)| (actual - expected).abs() <= 0.000_1);
            if actual.id != expected.id
                || actual.page_index != expected.page_index
                || !geometry_matches
                || actual.asset() != expected.asset()
                || actual.locked != expected.locked
                || !reopened.snapshot_has_canonical_native_identity(&expected.id)
            {
                return Err(format!(
                    "saved PDF Snapshot {} failed typed reopen validation",
                    expected.id
                ));
            }
        }
        for deleted_id in &deleted_image_ids {
            if reopened
                .images()
                .iter()
                .any(|image| image.id == *deleted_id)
            {
                return Err(format!(
                    "saved PDF still contains deleted image {deleted_id}"
                ));
            }
            if reopened.has_raw_annotation_name(deleted_id)
                || reopened.has_canonical_raw_annotation_name(deleted_id)
            {
                return Err(format!(
                    "saved PDF object graph still contains deleted image {deleted_id}"
                ));
            }
        }
        for expected in &request.annotations.images {
            let Some(actual) = reopened
                .images()
                .iter()
                .find(|image| image.id == expected.id)
            else {
                return Err(format!("saved PDF is missing image {}", expected.id));
            };
            if !reopened.image_has_canonical_native_identity(&expected.id) {
                return Err(format!(
                    "saved PDF image {} does not have canonical native identity",
                    expected.id
                ));
            }
            let geometry_matches = [
                (actual.rect.x, expected.rect.x),
                (actual.rect.y, expected.rect.y),
                (actual.rect.width, expected.rect.width),
                (actual.rect.height, expected.rect.height),
            ]
            .into_iter()
            .all(|(actual, expected)| (actual - expected).abs() <= 0.000_1);
            if actual.id != expected.id
                || actual.page_index != expected.page_index
                || !geometry_matches
                || actual.asset() != expected.asset()
                || actual.aspect_locked != expected.aspect_locked
                || actual.locked != expected.locked
            {
                return Err(format!(
                    "saved PDF image {} failed typed reopen validation: expected page {} rect {:?} asset {} aspect_locked={} locked={}; reopened page {} rect {:?} asset {} aspect_locked={} locked={}",
                    expected.id,
                    expected.page_index,
                    expected.rect,
                    expected.asset().id().as_str(),
                    expected.aspect_locked,
                    expected.locked,
                    actual.page_index,
                    actual.rect,
                    actual.asset().id().as_str(),
                    actual.aspect_locked,
                    actual.locked,
                ));
            }
        }
        let reopen_request = OpenDocumentRequest {
            document_id: request.document_id,
            generation: request.generation,
            path: validation_path,
        };
        let mut opened = self.opener.open(&reopen_request)?;
        opened.scale_presets = request.annotations.scale_presets.clone();
        if request.current_page != 0 {
            opened.current_page = match opened
                .resource
                .render_page(request.current_page, DEFAULT_PAGE_RENDER_WIDTH)
            {
                Ok(surface) => surface,
                Err(error) => {
                    return Err(match opened.resource.close() {
                        Ok(()) => error,
                        Err(cleanup) => {
                            format!("{error}; failed to release rejected save resource: {cleanup}")
                        }
                    });
                }
            };
        }
        let publish_result = if request.is_in_place() {
            let expected_source_sha256 = request
                .expected_source_sha256
                .ok_or_else(|| "in-place Save requires a verified source digest".to_owned())?;
            let _ = expected_source_sha256;
            prepared.publish_replacing()
        } else {
            prepared.publish()
        };
        let publication_outcome = match publish_result {
            Ok(outcome) => outcome,
            Err(error) => {
                return Err(match opened.resource.close() {
                    Ok(()) => error.to_string(),
                    Err(cleanup) => {
                        format!("{error}; failed to release unpublished save resource: {cleanup}")
                    }
                });
            }
        };
        opened.title = request
            .target_path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled PDF")
            .to_owned();
        let saved = SavedNativeDocument::new(opened, request.annotation_revision);
        Ok(match publication_outcome {
            PdfPublicationOutcome::Durable => saved,
            PdfPublicationOutcome::PublishedWithWarning { warning } => {
                saved.with_publication_warning(warning)
            }
        })
    }
}

#[derive(Clone, Copy)]
enum ActiveInspectorKind {
    Rectangle,
    Ellipse,
    StraightLine,
    VertexPath,
    Ink,
    EngineeringVisual,
    TextBox,
    Measurement,
    Dimension,
}

struct ActiveInspector {
    kind: ActiveInspectorKind,
    initial_width: Pixels,
    width_range: Range<Pixels>,
}

fn active_inspector_shell() -> gpui::Stateful<gpui::Div> {
    gpui::div()
        .id(DOCUMENT_ACTIVE_INSPECTOR_SLOT_ID)
        .debug_selector(|| DOCUMENT_ACTIVE_INSPECTOR_SLOT_ID.into())
        .w_full()
        .h_full()
        .min_h_0()
        .flex_none()
        .relative()
}

pub struct DocumentWorkspace {
    sessions: Vec<Entity<NativeDocumentSession>>,
    active_document_id: Option<DocumentId>,
    next_document_id: u64,
    next_generation: u64,
    next_open_batch_id: u64,
    latest_open_batch_id: Option<u64>,
    pending_session_restore: Option<PendingSessionRestore>,
    document_open_status: DocumentOpenBatchStatus,
    document_open_failures: Vec<DocumentOpenFailure>,
    next_annotation_sequence: u64,
    annotation_clipboard: Vec<Annotation>,
    annotation_paste_sequence: u64,
    next_pointer_id: u64,
    next_painted_state_sequence: u64,
    page_interactions: HashMap<(DocumentId, u32), PageInteraction>,
    last_painted_page_evidence: HashMap<(DocumentId, u32), PaintedPageEvidence>,
    viewport_bounds: HashMap<DocumentId, Bounds<Pixels>>,
    active_annotation_pointer: Option<ActiveAnnotationPointer>,
    pending_close_document_id: Option<DocumentId>,
    close_after_save_document_id: Option<DocumentId>,
    session_tab_focus_handles: HashMap<DocumentId, FocusHandle>,
    session_tab_bounds: HashMap<DocumentId, Rc<Cell<Bounds<Pixels>>>>,
    session_tab_close_bounds: HashMap<DocumentId, Rc<Cell<Bounds<Pixels>>>>,
    session_tab_pointer_drag: Option<DocumentSessionTabPointerDragState>,
    suppress_session_tab_click_id: Option<DocumentId>,
    session_tab_reorder_events: Vec<DocumentTabReorderEvent>,
    session_tab_reorder_announcement: String,
    viewport_refresh_scheduled: Option<DocumentId>,
    viewer_quality_tasks: HashMap<DocumentId, Task<()>>,
    annotation_stroke_menu_open: bool,
    annotation_highlight_settings_open: bool,
    semantic_snap_settings_open: bool,
    semantic_snap_settings: SemanticSnapSettings,
    toolbar_scroll: ScrollHandle,
    workspace_focus: FocusHandle,
    text_box_return_focus: FocusHandle,
    pending_text_box_editor: Option<PendingTextBoxEditor>,
    pending_text_box_subscriptions: Vec<Subscription>,
    text_box_commit_error: Option<String>,
    annotation_statuses: HashMap<DocumentId, String>,
    last_file_error: Option<String>,
    rejected_stale_image_prepares: u64,
    signature_popover_open: bool,
    signature_prepare_state: SignaturePrepareState,
    drawn_signature: DrawnSignature,
    pending_save_prompt: Option<SavePromptAuthority>,
    rejected_stale_save_prompts: u64,
    page_scale_control: Option<Entity<PageScaleControl>>,
    rectangle_property_inspector: Option<Entity<RectanglePropertyInspector>>,
    rectangle_property_subscription: Option<Subscription>,
    ellipse_property_inspector: Option<Entity<EllipsePropertyInspector>>,
    ellipse_property_subscription: Option<Subscription>,
    rectangular_shape_property_inspector_open: bool,
    ink_property_inspector: Option<Entity<InkPropertyInspector>>,
    ink_property_subscription: Option<Subscription>,
    ink_property_inspector_open: bool,
    engineering_visual_property_inspector: Option<Entity<EngineeringVisualPropertyInspector>>,
    engineering_visual_property_subscription: Option<Subscription>,
    engineering_visual_property_inspector_open: bool,
    straight_line_property_inspector: Option<Entity<StraightLinePropertyInspector>>,
    straight_line_property_subscription: Option<Subscription>,
    straight_line_property_inspector_open: bool,
    vertex_path_property_inspector: Option<Entity<VertexPathPropertyInspector>>,
    vertex_path_property_subscription: Option<Subscription>,
    vertex_path_property_inspector_open: bool,
    text_box_property_inspector: Option<Entity<TextBoxPropertyInspector>>,
    text_box_property_subscription: Option<Subscription>,
    text_box_property_inspector_open: bool,
    measurement_property_inspector: Option<Entity<MeasurementPropertyInspector>>,
    measurement_property_subscription: Option<Subscription>,
    measurement_property_inspector_open: bool,
    dimension_property_inspector: Option<Entity<DimensionPropertyInspector>>,
    dimension_property_subscription: Option<Subscription>,
    dimension_property_inspector_open: bool,
    template_control: Entity<TemplateSplitControl>,
    _template_subscription: Subscription,
    viewer_toolbar: Entity<ViewerToolbarStrip>,
    viewer_resizable: Entity<ResizableState>,
    thumbnail_scroll: UniformListScrollHandle,
    _viewer_toolbar_subscriptions: Vec<Subscription>,
    viewer_session_subscriptions: HashMap<DocumentId, Subscription>,
    generated_document_store: Option<GeneratedDocumentStore>,
    pending_template_id: Option<String>,
    pending_template_document_id: Option<DocumentId>,
    template_manage_requests: u64,
    template_save_requests: u64,
    external_template_authority: bool,
    opener: Option<Arc<dyn NativeDocumentOpener>>,
    saver: Option<Arc<dyn NativeDocumentSaver>>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DocumentEditCapabilities {
    pub can_undo: bool,
    pub can_redo: bool,
    pub can_cut: bool,
    pub can_copy: bool,
    pub can_paste: bool,
    pub can_select_all: bool,
    pub can_delete: bool,
}

impl EventEmitter<DocumentWorkspaceTemplateCommand> for DocumentWorkspace {}

#[derive(Clone, Copy)]
struct PageInteraction {
    document_id: DocumentId,
    page_index: u32,
    bounds: Bounds<Pixels>,
    transform: PageTransform,
    painted_evidence: Option<PaintedPageEvidence>,
}

#[derive(Clone, Copy)]
struct PaintedViewerAuthority {
    viewer_generation: u64,
    request_generation: u64,
    resource_generation: u64,
    rendered_dpr: f32,
}

#[derive(Clone, Copy)]
struct ActiveAnnotationPointer {
    document_id: DocumentId,
    page_index: u32,
    pointer_id: u64,
    placement_pending: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct DocumentSessionTabPointerDragState {
    document_id: DocumentId,
    start: Point<Pixels>,
    current: Point<Pixels>,
    activated: bool,
    over_document_id: DocumentId,
}

#[derive(Clone, Copy)]
struct ImagePrepareAuthority {
    document_id: DocumentId,
    document_generation: u64,
    prepare_generation: u64,
}

#[derive(Clone)]
struct SignaturePreview {
    asset: butter_paper_gpui_gallery::annotation_model::DecodedRgbaAsset,
    image: Arc<RenderImage>,
}

#[derive(Clone, Default)]
enum SignaturePrepareState {
    #[default]
    Idle,
    Loading,
    Preview(SignaturePreview),
    Error(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SavePromptAuthority {
    document_id: DocumentId,
    document_generation: u64,
    close_after_save: bool,
}

enum OpenSelectionDecision {
    Existing(DocumentId),
    Begin(OpenDocumentRequest),
}

struct PendingSessionRestore {
    batch_id: u64,
    intended_path: Option<PathBuf>,
    restart_views: Vec<(PathBuf, RestartView)>,
}

struct PendingTextBoxEditor {
    document_id: DocumentId,
    page_index: u32,
    target: PendingTextEditorTarget,
    authority: Option<PendingTextEditorAuthority>,
    input: Entity<TextareaState>,
}

#[derive(Clone)]
struct PendingTextEditorAuthority {
    resource_generation: u64,
    baseline_revision: u64,
    baseline_text: String,
}

fn validate_existing_text_editor_authority(
    authority: &PendingTextEditorAuthority,
    current_resource_generation: u64,
    current_revision: u64,
    target: Option<(&str, bool)>,
) -> Result<(), String> {
    if current_resource_generation != authority.resource_generation {
        return Err("Text Box editor belongs to a stale document resource".into());
    }
    let Some((current_text, locked)) = target else {
        return Err("Text Box is no longer available".into());
    };
    if locked {
        return Err("Text Box is locked".into());
    }
    if current_text != authority.baseline_text {
        return Err("Text Box content changed while its editor was open".into());
    }
    if current_revision != authority.baseline_revision {
        return Err("Text Box changed while its editor was open".into());
    }
    Ok(())
}

enum PendingTextEditorTarget {
    NewTextBox { id: MarkupId, anchor: PdfPoint },
    ExistingTextBox { id: MarkupId },
    Callout { id: MarkupId },
    CloudPlus { id: MarkupId },
    NewDimension { id: MarkupId },
    ExistingDimension { id: MarkupId },
}

fn defer_drop_images(images: Vec<Arc<RenderImage>>, cx: &mut App) {
    if images.is_empty() {
        return;
    }
    cx.defer(move |cx| {
        for image in images {
            cx.drop_image(image, None);
        }
    });
}

impl DocumentWorkspace {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let template_control = cx.new(TemplateSplitControl::new);
        let template_subscription = cx.subscribe(
            &template_control,
            |workspace, _, event: &TemplateSplitEvent, cx| {
                workspace.handle_template_split_event(event.clone(), cx);
            },
        );
        let continuous_control = cx.new(|_| ContinuousViewControl::continuous());
        let single_page_control = cx.new(|_| PageViewControl::single_page());
        let zoom_control = cx.new(|_| ZoomControl::new());
        let cad_view_control = cx.new(CadViewControl::new_deferred);
        let viewer_toolbar = cx.new(|cx| {
            ViewerToolbarStrip::new_with_cad_view(
                continuous_control.clone(),
                single_page_control.clone(),
                zoom_control.clone(),
                cad_view_control.clone(),
                cx,
            )
        });
        let viewer_resizable = cx.new(|_| ResizableState::default());
        viewer_toolbar.update(cx, |toolbar, cx| toolbar.set_disabled(true, cx));
        let viewer_toolbar_subscriptions = vec![
            cx.subscribe(
                &continuous_control,
                |workspace, _, event: &PageViewControlEvent, cx| {
                    workspace.handle_page_view_control_event(*event, cx);
                },
            ),
            cx.subscribe(
                &single_page_control,
                |workspace, _, event: &PageViewControlEvent, cx| {
                    workspace.handle_page_view_control_event(*event, cx);
                },
            ),
            cx.subscribe(
                &zoom_control,
                |workspace, _, event: &ZoomControlEvent, cx| {
                    workspace.handle_zoom_control_event(*event, cx);
                },
            ),
            cx.subscribe(
                &cad_view_control,
                |workspace, _, event: &CadViewControlEvent, cx| {
                    workspace.handle_cad_view_control_event(*event, cx);
                },
            ),
            cx.subscribe(
                &viewer_toolbar,
                |workspace, _, event: &ViewerToolbarStripEvent, cx| {
                    workspace.handle_viewer_toolbar_event(*event, cx);
                },
            ),
        ];
        Self {
            sessions: Vec::new(),
            active_document_id: None,
            next_document_id: 1,
            next_generation: 1,
            next_open_batch_id: 1,
            latest_open_batch_id: None,
            pending_session_restore: None,
            document_open_status: DocumentOpenBatchStatus::Idle,
            document_open_failures: Vec::new(),
            next_annotation_sequence: 1,
            annotation_clipboard: Vec::new(),
            annotation_paste_sequence: 0,
            next_pointer_id: 1,
            next_painted_state_sequence: 1,
            page_interactions: HashMap::new(),
            last_painted_page_evidence: HashMap::new(),
            viewport_bounds: HashMap::new(),
            active_annotation_pointer: None,
            pending_close_document_id: None,
            close_after_save_document_id: None,
            session_tab_focus_handles: HashMap::new(),
            session_tab_bounds: HashMap::new(),
            session_tab_close_bounds: HashMap::new(),
            session_tab_pointer_drag: None,
            suppress_session_tab_click_id: None,
            session_tab_reorder_events: Vec::new(),
            session_tab_reorder_announcement: String::new(),
            viewport_refresh_scheduled: None,
            viewer_quality_tasks: HashMap::new(),
            annotation_stroke_menu_open: false,
            annotation_highlight_settings_open: false,
            semantic_snap_settings_open: false,
            semantic_snap_settings: SemanticSnapSettings::default(),
            toolbar_scroll: ScrollHandle::new(),
            workspace_focus: cx.focus_handle(),
            text_box_return_focus: cx.focus_handle(),
            pending_text_box_editor: None,
            pending_text_box_subscriptions: Vec::new(),
            text_box_commit_error: None,
            annotation_statuses: HashMap::new(),
            last_file_error: None,
            rejected_stale_image_prepares: 0,
            signature_popover_open: false,
            signature_prepare_state: SignaturePrepareState::Idle,
            drawn_signature: DrawnSignature::default(),
            pending_save_prompt: None,
            rejected_stale_save_prompts: 0,
            page_scale_control: None,
            rectangle_property_inspector: None,
            rectangle_property_subscription: None,
            ellipse_property_inspector: None,
            ellipse_property_subscription: None,
            rectangular_shape_property_inspector_open: false,
            ink_property_inspector: None,
            ink_property_subscription: None,
            ink_property_inspector_open: false,
            engineering_visual_property_inspector: None,
            engineering_visual_property_subscription: None,
            engineering_visual_property_inspector_open: false,
            straight_line_property_inspector: None,
            straight_line_property_subscription: None,
            straight_line_property_inspector_open: false,
            vertex_path_property_inspector: None,
            vertex_path_property_subscription: None,
            vertex_path_property_inspector_open: false,
            text_box_property_inspector: None,
            text_box_property_subscription: None,
            text_box_property_inspector_open: false,
            measurement_property_inspector: None,
            measurement_property_subscription: None,
            measurement_property_inspector_open: false,
            dimension_property_inspector: None,
            dimension_property_subscription: None,
            dimension_property_inspector_open: false,
            template_control,
            _template_subscription: template_subscription,
            viewer_toolbar,
            viewer_resizable,
            thumbnail_scroll: UniformListScrollHandle::new(),
            _viewer_toolbar_subscriptions: viewer_toolbar_subscriptions,
            viewer_session_subscriptions: HashMap::new(),
            generated_document_store: None,
            pending_template_id: None,
            pending_template_document_id: None,
            template_manage_requests: 0,
            template_save_requests: 0,
            external_template_authority: false,
            opener: None,
            saver: None,
        }
    }

    pub fn with_opener(opener: Arc<dyn NativeDocumentOpener>, cx: &mut Context<Self>) -> Self {
        let saver = Arc::new(PdfDocumentSaver::new(opener.clone()));
        let mut workspace = Self::new(cx);
        workspace.opener = Some(opener);
        workspace.saver = Some(saver);
        workspace
    }

    pub fn with_opener_and_generated_store(
        opener: Arc<dyn NativeDocumentOpener>,
        store: GeneratedDocumentStore,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut workspace = Self::with_opener(opener, cx);
        workspace.generated_document_store = Some(store);
        workspace
    }

    pub fn sessions(&self) -> &[Entity<NativeDocumentSession>] {
        &self.sessions
    }

    pub fn session_snapshot(&self, cx: &App) -> SessionSnapshot {
        let mut paths = Vec::new();
        let mut restart_views = Vec::new();
        let mut active_index = None;
        let mut retained_indices = HashMap::new();
        for session in &self.sessions {
            let session = session.read(cx);
            if !matches!(session.status, NativeDocumentStatus::Ready)
                || !session.path.is_absolute()
                || !is_pdf_path(&session.path)
                || session.save_as_required
                || session.temporary_source.is_some()
            {
                continue;
            }
            let key = normalized_path_key(&session.path);
            if let Some(&retained_index) = retained_indices.get(&key) {
                if Some(session.id) == self.active_document_id {
                    active_index = Some(retained_index);
                }
                continue;
            }
            retained_indices.insert(key, paths.len());
            if Some(session.id) == self.active_document_id {
                active_index = Some(paths.len());
            }
            paths.push(session.path.clone());
            restart_views.push(session.view_state.restart_view(session.current_page));
        }
        SessionSnapshot::new(paths, active_index).with_restart_views(restart_views)
    }

    pub fn restore_session(
        &mut self,
        plan: SessionRestorePlan,
        cx: &mut Context<Self>,
    ) -> DocumentOpenBatchDisposition {
        let (documents, active_index) = plan.into_documents();
        let intended_path = active_index
            .and_then(|index| documents.get(index))
            .map(|document| document.path().to_owned());
        let restart_views = documents
            .iter()
            .map(|document| (document.path().to_owned(), document.view()))
            .collect();
        let paths: Vec<PathBuf> = documents
            .into_iter()
            .map(|document| document.into_path())
            .collect();
        let disposition = self.open_documents(
            DocumentOpenBatchRequest::new(DocumentOpenOrigin::System, paths),
            cx,
        );
        if let DocumentOpenBatchDisposition::Started { batch_id, .. } = &disposition {
            self.pending_session_restore = Some(PendingSessionRestore {
                batch_id: *batch_id,
                intended_path,
                restart_views,
            });
        }
        disposition
    }

    pub fn session_order(&self, cx: &App) -> Vec<DocumentId> {
        self.sessions
            .iter()
            .map(|session| session.read(cx).id)
            .collect()
    }

    pub fn session_tab_reorder_events(&self) -> &[DocumentTabReorderEvent] {
        &self.session_tab_reorder_events
    }

    pub fn session_tab_reorder_announcement(&self) -> &str {
        &self.session_tab_reorder_announcement
    }

    pub fn session_tab_focus_handle(&self, document_id: DocumentId) -> Option<FocusHandle> {
        self.session_tab_focus_handles.get(&document_id).cloned()
    }

    fn register_session_tab(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        self.session_tab_focus_handles
            .entry(document_id)
            .or_insert_with(|| cx.focus_handle());
        self.session_tab_bounds
            .entry(document_id)
            .or_insert_with(|| Rc::new(Cell::new(Bounds::default())));
        self.session_tab_close_bounds
            .entry(document_id)
            .or_insert_with(|| Rc::new(Cell::new(Bounds::default())));
    }

    pub fn move_document_session_by_keyboard(
        &mut self,
        document_id: DocumentId,
        direction: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(from_ix) = self
            .sessions
            .iter()
            .position(|session| session.read(cx).id == document_id)
        else {
            return false;
        };
        let Some(to_ix) = from_ix.checked_add_signed(direction) else {
            return false;
        };
        if to_ix >= self.sessions.len() {
            return false;
        }
        let target_id = self.sessions[to_ix].read(cx).id;
        self.reorder_document_session(
            document_id,
            target_id,
            DocumentTabReorderOrigin::Keyboard,
            cx,
        )
    }

    fn reorder_document_session(
        &mut self,
        document_id: DocumentId,
        target_id: DocumentId,
        origin: DocumentTabReorderOrigin,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(from_ix) = self
            .sessions
            .iter()
            .position(|session| session.read(cx).id == document_id)
        else {
            return false;
        };
        let Some(to_ix) = self
            .sessions
            .iter()
            .position(|session| session.read(cx).id == target_id)
        else {
            return false;
        };
        if from_ix == to_ix {
            return false;
        }
        let title = self.sessions[from_ix].read(cx).title.clone();
        let session = self.sessions.remove(from_ix);
        self.sessions.insert(to_ix, session);
        let announcement = format!(
            "Moved {title} to position {} of {}.",
            to_ix + 1,
            self.sessions.len()
        );
        self.session_tab_reorder_announcement = announcement.clone();
        self.session_tab_reorder_events
            .push(DocumentTabReorderEvent {
                tab_id: document_id.to_string(),
                from_ix,
                to_ix,
                origin,
                announcement,
            });
        cx.notify();
        true
    }

    fn closest_session_tab_to_dragged_center(
        &self,
        document_id: DocumentId,
        delta: Point<Pixels>,
        cx: &App,
    ) -> Option<DocumentId> {
        let source = self.session_tab_bounds.get(&document_id)?.get();
        let dragged_center = source.center() + delta;
        self.sessions
            .iter()
            .filter_map(|session| {
                let id = session.read(cx).id;
                let bounds = self.session_tab_bounds.get(&id)?.get();
                Some((id, (bounds.center() - dragged_center).magnitude()))
            })
            .min_by(|left, right| left.1.total_cmp(&right.1))
            .map(|(document_id, _)| document_id)
    }

    fn begin_session_tab_pointer_drag(
        &mut self,
        document_id: DocumentId,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.session_tab_bounds.contains_key(&document_id) {
            return false;
        }
        self.suppress_session_tab_click_id = None;
        self.session_tab_pointer_drag = Some(DocumentSessionTabPointerDragState {
            document_id,
            start: position,
            current: position,
            activated: false,
            over_document_id: document_id,
        });
        cx.notify();
        true
    }

    fn update_session_tab_pointer_drag(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(snapshot) = self.session_tab_pointer_drag.clone() else {
            return false;
        };
        let delta = position - snapshot.start;
        if !snapshot.activated {
            if delta.magnitude() <= DOCUMENT_TAB_POINTER_DRAG_THRESHOLD {
                return false;
            }
            if let Some(drag) = self.session_tab_pointer_drag.as_mut() {
                drag.activated = true;
            }
            cx.notify();
            return true;
        }
        let over_document_id = self
            .closest_session_tab_to_dragged_center(snapshot.document_id, delta, cx)
            .unwrap_or(snapshot.document_id);
        if let Some(drag) = self.session_tab_pointer_drag.as_mut() {
            drag.current = position;
            drag.over_document_id = over_document_id;
        }
        cx.notify();
        true
    }

    fn finish_session_tab_pointer_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.session_tab_pointer_drag.take() else {
            return false;
        };
        if !drag.activated {
            return false;
        }
        self.suppress_session_tab_click_id = Some(drag.document_id);
        let reordered = self.reorder_document_session(
            drag.document_id,
            drag.over_document_id,
            DocumentTabReorderOrigin::Pointer,
            cx,
        );
        if !reordered {
            cx.notify();
        }
        true
    }

    fn cancel_session_tab_pointer_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.session_tab_pointer_drag.take() else {
            return false;
        };
        if drag.activated {
            self.suppress_session_tab_click_id = Some(drag.document_id);
        }
        cx.notify();
        true
    }

    fn take_suppressed_session_tab_click(&mut self, document_id: DocumentId) -> bool {
        if self.suppress_session_tab_click_id != Some(document_id) {
            return false;
        }
        self.suppress_session_tab_click_id = None;
        true
    }

    pub const fn active_document_id(&self) -> Option<DocumentId> {
        self.active_document_id
    }

    pub fn document_edit_capabilities(&self, cx: &App) -> DocumentEditCapabilities {
        let Some(document_id) = self.active_document_id else {
            return DocumentEditCapabilities::default();
        };
        let Some(session) = self.session(document_id, cx) else {
            return DocumentEditCapabilities::default();
        };
        let session = session.read(cx);
        if !matches!(session.status, NativeDocumentStatus::Ready) {
            return DocumentEditCapabilities::default();
        }
        let saving = session.save_status == NativeDocumentSaveStatus::Saving;
        let (undo_depth, redo_depth) = session.annotations.history_depths(document_id.value());
        let selected = session
            .annotations
            .selected_annotations_in_document_order(document_id.value());
        let has_selection = !selected.is_empty();
        let has_unlocked_selection = session
            .annotations
            .selected_has_unlocked(document_id.value());
        let current_page = session.current_page();
        let scene = session
            .annotations
            .document_scene(document_id.value(), current_page);
        let page_has_annotations = !scene.rectangles.is_empty()
            || !scene.redacts.is_empty()
            || !scene.ellipses.is_empty()
            || !scene.arcs.is_empty()
            || !scene.straight_lines.is_empty()
            || !scene.vertex_paths.is_empty()
            || !scene.clouds.is_empty()
            || !scene.cloud_pluses.is_empty()
            || !scene.callouts.is_empty()
            || !scene.measurement_paths.is_empty()
            || !scene.pens.is_empty()
            || !scene.text_boxes.is_empty()
            || !scene.dimensions.is_empty()
            || !scene.lengths.is_empty()
            || !scene.images.is_empty()
            || !scene.snapshots.is_empty();
        DocumentEditCapabilities {
            can_undo: undo_depth > 0 && !saving,
            can_redo: redo_depth > 0 && !saving,
            can_cut: has_selection && !saving,
            can_copy: has_selection,
            can_paste: !self.annotation_clipboard.is_empty() && !saving,
            can_select_all: page_has_annotations,
            can_delete: has_unlocked_selection && !saving,
        }
    }

    pub fn document_open_status(&self) -> &DocumentOpenBatchStatus {
        &self.document_open_status
    }

    pub fn last_document_open_failure(&self) -> Option<&DocumentOpenFailure> {
        self.document_open_failures.first()
    }

    pub fn document_open_failures(&self) -> &[DocumentOpenFailure] {
        &self.document_open_failures
    }

    pub fn dismiss_document_open_failure(&mut self, cx: &mut Context<Self>) {
        if !self.document_open_failures.is_empty() {
            self.document_open_failures.clear();
            cx.notify();
        }
    }

    pub const fn pending_close_document_id(&self) -> Option<DocumentId> {
        self.pending_close_document_id
    }

    pub const fn is_template_creation_pending(&self) -> bool {
        self.pending_template_id.is_some()
    }

    pub const fn template_manage_requests(&self) -> u64 {
        self.template_manage_requests
    }

    pub const fn template_save_requests(&self) -> u64 {
        self.template_save_requests
    }

    pub fn use_external_template_authority(&mut self, enabled: bool) {
        self.external_template_authority = enabled;
    }

    pub fn apply_template_catalog(
        &mut self,
        templates: Vec<TemplateCatalogItem>,
        durable_last_used_id: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        self.template_control.update(cx, |control, cx| {
            control.apply_template_catalog(templates, durable_last_used_id, cx);
        });
        cx.notify();
    }

    pub fn set_template_operation_state(&mut self, storage_busy: bool, cx: &mut Context<Self>) {
        let save_document_enabled = self.active_document_id.is_some() && !storage_busy;
        self.template_control.update(cx, |control, cx| {
            control.set_creating(storage_busy, cx);
            control.set_save_document_enabled(save_document_enabled, cx);
        });
    }

    fn handle_template_split_event(&mut self, event: TemplateSplitEvent, cx: &mut Context<Self>) {
        match event {
            TemplateSplitEvent::CreateRequested {
                template_id,
                origin,
            } => {
                if self.external_template_authority {
                    cx.emit(DocumentWorkspaceTemplateCommand::Create(
                        TemplateCreationEvent {
                            template_id,
                            origin,
                        },
                    ));
                    return;
                }
                if self.pending_template_id.is_some() {
                    return;
                }
                self.pending_template_id = Some(template_id.clone());
                let workspace = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = workspace.update(cx, |workspace, cx| {
                        workspace.start_reserved_template_creation(template_id, cx);
                    });
                });
                cx.notify();
            }
            TemplateSplitEvent::ManageRequested => {
                self.template_manage_requests = self.template_manage_requests.saturating_add(1);
                cx.emit(DocumentWorkspaceTemplateCommand::Manage);
                cx.notify();
            }
            TemplateSplitEvent::SaveDocumentAsTemplateRequested => {
                self.template_save_requests = self.template_save_requests.saturating_add(1);
                if let Some(document_id) = self.active_document_id
                    && let Some(session) = self.session(document_id, cx)
                {
                    let session = session.read(cx);
                    cx.emit(DocumentWorkspaceTemplateCommand::SaveDocumentAsTemplate {
                        document_id,
                        document_name: session.title().to_owned(),
                        authorized_source: session.path().to_owned(),
                    });
                }
                cx.notify();
            }
            TemplateSplitEvent::OpenChanged(_) | TemplateSplitEvent::SelectionChanged(_) => {}
        }
    }

    pub fn request_generated_template(
        &mut self,
        template_id: &'static str,
        cx: &mut Context<Self>,
    ) -> GeneratedTemplateRequestDisposition {
        if self.pending_template_id.is_some() {
            return GeneratedTemplateRequestDisposition::SuppressedPending;
        }
        self.pending_template_id = Some(template_id.to_owned());
        self.start_reserved_template_creation(template_id.to_owned(), cx)
    }

    fn start_reserved_template_creation(
        &mut self,
        template_id: String,
        cx: &mut Context<Self>,
    ) -> GeneratedTemplateRequestDisposition {
        if self.pending_template_id.as_deref() != Some(template_id.as_str())
            || self.pending_template_document_id.is_some()
        {
            return GeneratedTemplateRequestDisposition::SuppressedPending;
        }
        self.template_control.update(cx, |control, cx| {
            control.set_creating(true, cx);
        });
        let Some(store) = self.generated_document_store.clone() else {
            let error =
                "Generated template storage is not configured for this workspace.".to_owned();
            self.last_file_error = Some(error.clone());
            self.finish_template_creation(cx);
            return GeneratedTemplateRequestDisposition::Rejected(error);
        };
        let request = match generated_document_request_for_template(&template_id) {
            Ok(request) => request,
            Err(error) => {
                self.last_file_error = Some(error.clone());
                self.finish_template_creation(cx);
                return GeneratedTemplateRequestDisposition::Rejected(error);
            }
        };
        match self.create_generated_document(store, request, cx) {
            Ok(document_id) => {
                self.pending_template_document_id = Some(document_id);
                GeneratedTemplateRequestDisposition::Started(document_id)
            }
            Err(error) => {
                self.last_file_error = Some(error.clone());
                self.finish_template_creation(cx);
                GeneratedTemplateRequestDisposition::Rejected(error)
            }
        }
    }

    fn finish_template_creation(&mut self, cx: &mut Context<Self>) {
        self.pending_template_id = None;
        self.pending_template_document_id = None;
        self.template_control.update(cx, |control, cx| {
            control.set_creating(false, cx);
        });
        cx.notify();
    }

    pub const fn close_after_save_document_id(&self) -> Option<DocumentId> {
        self.close_after_save_document_id
    }

    pub fn last_file_error(&self) -> Option<&str> {
        self.last_file_error.as_deref()
    }

    pub fn document_save_failure(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<DocumentSaveFailure> {
        let session = self.session(document_id, cx)?;
        match session.read(cx).save_status() {
            NativeDocumentSaveStatus::Failed(failure) => Some(failure.clone()),
            NativeDocumentSaveStatus::Idle | NativeDocumentSaveStatus::Saving => None,
        }
    }

    pub fn pending_save_prompt_document_id(&self) -> Option<DocumentId> {
        self.pending_save_prompt
            .map(|authority| authority.document_id)
    }

    pub fn dismiss_document_save_failure(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let dismissed = session.update(cx, |session, cx| {
            if !matches!(session.save_status, NativeDocumentSaveStatus::Failed(_)) {
                return false;
            }
            session.save_status = NativeDocumentSaveStatus::Idle;
            cx.notify();
            true
        });
        if dismissed {
            cx.notify();
        }
        dismissed
    }

    fn record_document_save_failure(
        &mut self,
        document_id: DocumentId,
        operation: DocumentSaveFailureOperation,
        message: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let recorded = session.update(cx, |session, cx| {
            if !matches!(session.status, NativeDocumentStatus::Ready)
                || session.save_status == NativeDocumentSaveStatus::Saving
            {
                return false;
            }
            session.save_generation = session.save_generation.saturating_add(1);
            session.save_status = NativeDocumentSaveStatus::Failed(DocumentSaveFailure {
                generation: session.save_generation,
                operation,
                message,
            });
            cx.notify();
            true
        });
        if recorded {
            cx.notify();
        }
        recorded
    }

    pub fn page_scale_control(&self) -> Option<Entity<PageScaleControl>> {
        self.page_scale_control.clone()
    }

    fn ensure_page_scale_control(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<PageScaleControl> {
        if let Some(control) = &self.page_scale_control {
            return control.clone();
        }
        let owner = cx.entity().downgrade();
        let control = cx.new(|cx| PageScaleControl::new(owner, window, cx));
        self.page_scale_control = Some(control.clone());
        control
    }

    pub fn rectangle_property_inspector(&self) -> Option<Entity<RectanglePropertyInspector>> {
        self.rectangle_property_inspector.clone()
    }

    pub fn ellipse_property_inspector(&self) -> Option<Entity<EllipsePropertyInspector>> {
        self.ellipse_property_inspector.clone()
    }

    fn ensure_rectangle_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<RectanglePropertyInspector> {
        if let Some(inspector) = &self.rectangle_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| RectanglePropertyInspector::new(window, cx));
        let subscription = cx.subscribe(
            &inspector,
            |workspace, _, event: &RectanglePropertyEvent, cx| {
                if let Err(error) = workspace.apply_rectangle_property_event(event, cx) {
                    workspace
                        .annotation_statuses
                        .insert(event.document_id, error);
                    cx.notify();
                }
            },
        );
        self.rectangle_property_subscription = Some(subscription);
        self.rectangle_property_inspector = Some(inspector.clone());
        inspector
    }

    fn ensure_ellipse_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EllipsePropertyInspector> {
        if let Some(inspector) = &self.ellipse_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| EllipsePropertyInspector::new_ellipse(window, cx));
        let subscription = cx.subscribe(
            &inspector,
            |workspace, _, event: &RectanglePropertyEvent, cx| {
                if let Err(error) = workspace.apply_rectangular_shape_property_event(event, cx) {
                    workspace
                        .annotation_statuses
                        .insert(event.document_id, error);
                    cx.notify();
                }
            },
        );
        self.ellipse_property_subscription = Some(subscription);
        self.ellipse_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn ink_property_inspector(&self) -> Option<Entity<InkPropertyInspector>> {
        self.ink_property_inspector.clone()
    }

    fn ensure_ink_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InkPropertyInspector> {
        if let Some(inspector) = &self.ink_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| InkPropertyInspector::new(window, cx));
        let subscription =
            cx.subscribe(&inspector, |workspace, _, event: &InkPropertyEvent, cx| {
                if let Err(error) = workspace.apply_ink_property_event(event, cx) {
                    workspace
                        .annotation_statuses
                        .insert(event.document_id, error);
                    cx.notify();
                }
            });
        self.ink_property_subscription = Some(subscription);
        self.ink_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn set_ink_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ink_property_inspector_open = open;
        self.ensure_ink_property_inspector(window, cx)
            .update(cx, |inspector, cx| inspector.set_open(open, cx));
        cx.notify();
    }

    pub fn apply_ink_property_event(
        &mut self,
        event: &InkPropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.pending_close_document_id == Some(event.document_id)
            || self.close_after_save_document_id == Some(event.document_id)
        {
            return Ok(false);
        }
        let Some(session) = self.session(event.document_id, cx).cloned() else {
            return Ok(false);
        };
        let session_state = session.read(cx);
        if !matches!(session_state.status, NativeDocumentStatus::Ready)
            || session_state.save_status == NativeDocumentSaveStatus::Saving
            || session_state.pending_rotation_generation.is_some()
        {
            return Ok(false);
        }
        let Some(snapshot) = session_state
            .annotations
            .snapshot(event.document_id.value())
        else {
            return Ok(false);
        };
        let Some(current) = session_state
            .annotations
            .exact_selected_ink(event.document_id.value())
        else {
            return Ok(false);
        };
        if snapshot.revision != event.expected_revision
            || current.id != event.annotation_id
            || current.tool() != event.expected_tool
        {
            return Ok(false);
        }
        if ink_patch_matches(&event.patch, &current.appearance, current.locked) {
            return Ok(false);
        }
        if current.locked && !matches!(event.patch, InkPropertyPatch::Locked(_)) {
            return Err(format!("markup {} is locked", event.annotation_id));
        }
        let current_appearance = current.appearance.clone();
        match &event.patch {
            InkPropertyPatch::Locked(locked) => self.update_annotation_history(
                event.document_id,
                cx,
                |annotations, document_id| annotations.set_selected_locked(document_id, *locked),
            )?,
            InkPropertyPatch::Appearance(appearance) => {
                let appearance = appearance.clone();
                self.update_annotation_history(event.document_id, cx, move |annotations, id| {
                    annotations.set_exact_selected_ink_appearance(id, appearance)
                })?;
            }
            InkPropertyPatch::WidthPt(width) => {
                let appearance = PenAppearance::new(
                    current_appearance.color(),
                    *width,
                    current_appearance.opacity(),
                )
                .map_err(|error| error.to_string())?;
                self.update_annotation_history(event.document_id, cx, move |annotations, id| {
                    annotations.set_exact_selected_ink_appearance(id, appearance)
                })?;
            }
            InkPropertyPatch::Opacity(opacity) => {
                let appearance = PenAppearance::new(
                    current_appearance.color(),
                    current_appearance.width_pt(),
                    *opacity,
                )
                .map_err(|error| error.to_string())?;
                self.update_annotation_history(event.document_id, cx, move |annotations, id| {
                    annotations.set_exact_selected_ink_appearance(id, appearance)
                })?;
            }
        }
        Ok(true)
    }

    pub fn engineering_visual_property_inspector(
        &self,
    ) -> Option<Entity<EngineeringVisualPropertyInspector>> {
        self.engineering_visual_property_inspector.clone()
    }

    fn ensure_engineering_visual_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<EngineeringVisualPropertyInspector> {
        if let Some(inspector) = &self.engineering_visual_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| EngineeringVisualPropertyInspector::new(window, cx));
        let subscription = cx.subscribe(
            &inspector,
            |workspace, _, event: &EngineeringVisualPropertyEvent, cx| {
                if let Err(error) = workspace.apply_engineering_visual_property_event(event, cx) {
                    workspace.annotation_statuses.insert(event.document_id, error);
                    cx.notify();
                }
            },
        );
        self.engineering_visual_property_subscription = Some(subscription);
        self.engineering_visual_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn set_engineering_visual_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.engineering_visual_property_inspector_open = open;
        self.ensure_engineering_visual_property_inspector(window, cx)
            .update(cx, |inspector, cx| inspector.set_open(open, cx));
        cx.notify();
    }

    pub fn apply_engineering_visual_property_event(
        &mut self,
        event: &EngineeringVisualPropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.pending_close_document_id == Some(event.document_id)
            || self.close_after_save_document_id == Some(event.document_id)
            || self.pending_save_prompt_document_id() == Some(event.document_id)
            || self.pending_text_box_editor.as_ref().is_some_and(|editor| {
                editor.document_id == event.document_id
            })
        {
            return Ok(false);
        }
        let Some(session) = self.session(event.document_id, cx).cloned() else {
            return Ok(false);
        };
        let session_state = session.read(cx);
        if !matches!(session_state.status, NativeDocumentStatus::Ready)
            || session_state.save_status == NativeDocumentSaveStatus::Saving
            || session_state.pending_rotation_generation.is_some()
        {
            return Ok(false);
        }
        let Some(revision) = session_state
            .annotations
            .snapshot(event.document_id.value())
            .map(|snapshot| snapshot.revision)
        else {
            return Ok(false);
        };
        if revision != event.expected_revision {
            return Ok(false);
        }
        let current = match event.expected_kind {
            EngineeringVisualPropertyKind::Arc => session_state
                .annotations
                .exact_selected_arc(event.document_id.value())
                .map(|value| (value.id.clone(), value.appearance.clone(), None, value.locked)),
            EngineeringVisualPropertyKind::Cloud => session_state
                .annotations
                .exact_selected_cloud(event.document_id.value())
                .map(|value| {
                    (
                        value.id.clone(),
                        value.appearance.clone(),
                        Some(value.border_effect_intensity()),
                        value.locked,
                    )
                }),
            EngineeringVisualPropertyKind::Snapshot => session_state
                .annotations
                .exact_selected_snapshot(event.document_id.value())
                .map(|value| {
                    (
                        value.id.clone(),
                        RectangleAppearance::new("#000000", 0., None::<String>, value.opacity())
                            .expect("validated snapshot opacity must form a transient appearance"),
                        None,
                        value.locked,
                    )
                }),
        };
        let Some((current_id, current_appearance, current_intensity, locked)) = current else {
            return Ok(false);
        };
        if current_id != event.annotation_id
            || engineering_visual_patch_matches(
                &event.patch,
                event.expected_kind,
                &current_appearance,
                current_intensity,
                locked,
            )
        {
            return Ok(false);
        }
        if locked && !matches!(event.patch, EngineeringVisualPropertyPatch::Locked(_)) {
            return Err(format!("markup {} is locked", event.annotation_id));
        }
        match &event.patch {
            EngineeringVisualPropertyPatch::Locked(locked) => self.update_annotation_history(
                event.document_id,
                cx,
                |annotations, document_id| annotations.set_selected_locked(document_id, *locked),
            )?,
            EngineeringVisualPropertyPatch::CloudIntensity(intensity)
                if event.expected_kind == EngineeringVisualPropertyKind::Cloud =>
            {
                let intensity = *intensity;
                self.update_annotation_history(event.document_id, cx, move |annotations, id| {
                    annotations.set_exact_selected_cloud_intensity(id, intensity)
                })?;
            }
            EngineeringVisualPropertyPatch::Color(color)
                if matches!(
                    event.expected_kind,
                    EngineeringVisualPropertyKind::Arc | EngineeringVisualPropertyKind::Cloud
                ) =>
            {
                let appearance = engineering_visual_appearance(
                    &current_appearance,
                    color,
                    current_appearance.stroke_width_pt(),
                    current_appearance.opacity(),
                )?;
                self.update_engineering_visual_appearance(
                    event.document_id,
                    event.expected_kind,
                    appearance,
                    cx,
                )?;
            }
            EngineeringVisualPropertyPatch::WidthPt(width)
                if matches!(event.expected_kind, EngineeringVisualPropertyKind::Arc | EngineeringVisualPropertyKind::Cloud)
                    && (0.25..=24.).contains(width) =>
            {
                let appearance = engineering_visual_appearance(
                    &current_appearance,
                    current_appearance.stroke_color(),
                    *width,
                    current_appearance.opacity(),
                )?;
                self.update_engineering_visual_appearance(
                    event.document_id,
                    event.expected_kind,
                    appearance,
                    cx,
                )?;
            }
            EngineeringVisualPropertyPatch::Opacity(opacity) if (0.0..=1.).contains(opacity) => {
                if event.expected_kind == EngineeringVisualPropertyKind::Snapshot {
                    let opacity = *opacity;
                    self.update_annotation_history(event.document_id, cx, move |annotations, id| {
                        annotations.set_exact_selected_snapshot_opacity(id, opacity)
                    })?;
                } else {
                    let appearance = engineering_visual_appearance(
                        &current_appearance,
                        current_appearance.stroke_color(),
                        current_appearance.stroke_width_pt(),
                        *opacity,
                    )?;
                    self.update_engineering_visual_appearance(
                        event.document_id,
                        event.expected_kind,
                        appearance,
                        cx,
                    )?;
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn update_engineering_visual_appearance(
        &mut self,
        document_id: DocumentId,
        kind: EngineeringVisualPropertyKind,
        appearance: RectangleAppearance,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, id| match kind {
            EngineeringVisualPropertyKind::Arc => {
                annotations.set_exact_selected_arc_appearance(id, appearance)
            }
            EngineeringVisualPropertyKind::Cloud => {
                annotations.set_exact_selected_cloud_appearance(id, appearance)
            }
            EngineeringVisualPropertyKind::Snapshot => Err(AnnotationError::NoSelection),
        })
    }

    pub fn straight_line_property_inspector(
        &self,
    ) -> Option<Entity<StraightLinePropertyInspector>> {
        self.straight_line_property_inspector.clone()
    }

    fn ensure_straight_line_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<StraightLinePropertyInspector> {
        if let Some(inspector) = &self.straight_line_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| StraightLinePropertyInspector::new(window, cx));
        let subscription = cx.subscribe(
            &inspector,
            |workspace, _, event: &StraightLinePropertyEvent, cx| {
                if let Err(error) = workspace.apply_straight_line_property_event(event, cx) {
                    workspace.annotation_statuses.insert(event.document_id, error);
                    cx.notify();
                }
            },
        );
        self.straight_line_property_subscription = Some(subscription);
        self.straight_line_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn set_straight_line_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.straight_line_property_inspector_open = open;
        self.ensure_straight_line_property_inspector(window, cx)
            .update(cx, |inspector, cx| inspector.set_open(open, cx));
        cx.notify();
    }

    pub fn apply_straight_line_property_event(
        &mut self,
        event: &StraightLinePropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.pending_close_document_id == Some(event.document_id)
            || self.close_after_save_document_id == Some(event.document_id)
            || self.pending_save_prompt_document_id() == Some(event.document_id)
            || self.pending_text_box_editor.as_ref().is_some_and(|editor| {
                editor.document_id == event.document_id
            })
        {
            return Ok(false);
        }
        let Some(session) = self.session(event.document_id, cx).cloned() else {
            return Ok(false);
        };
        let session_state = session.read(cx);
        if !matches!(session_state.status, NativeDocumentStatus::Ready)
            || session_state.save_status == NativeDocumentSaveStatus::Saving
            || session_state.pending_rotation_generation.is_some()
        {
            return Ok(false);
        }
        let Some(revision) = session_state
            .annotations
            .snapshot(event.document_id.value())
            .map(|snapshot| snapshot.revision)
        else {
            return Ok(false);
        };
        let selected = session_state
            .annotations
            .selected_annotations_in_document_order(event.document_id.value());
        let [Annotation::StraightLine(current)] = selected.as_slice() else {
            return Ok(false);
        };
        if revision != event.expected_revision
            || current.id != event.annotation_id
            || current.kind != event.expected_kind
            || straight_line_property_patch_matches(&event.patch, current)
        {
            return Ok(false);
        }
        if current.locked && !matches!(event.patch, StraightLinePropertyPatch::Locked(_)) {
            return Err(format!("markup {} is locked", event.annotation_id));
        }
        match &event.patch {
            StraightLinePropertyPatch::Locked(locked) => self.update_annotation_history(
                event.document_id,
                cx,
                |annotations, document_id| annotations.set_selected_locked(document_id, *locked),
            )?,
            StraightLinePropertyPatch::Color(color) => self.edit_selected_straight_line_property(
                event.document_id,
                StraightLinePropertyEdit::StrokeColor(color.clone()),
                cx,
            )?,
            StraightLinePropertyPatch::WidthPt(width) if (0.25..=24.).contains(width) => self
                .edit_selected_straight_line_property(
                    event.document_id,
                    StraightLinePropertyEdit::StrokeWidthPt(*width),
                    cx,
                )?,
            StraightLinePropertyPatch::Opacity(opacity) if (0.0..=1.).contains(opacity) => self
                .edit_selected_straight_line_property(
                    event.document_id,
                    StraightLinePropertyEdit::Opacity(*opacity),
                    cx,
                )?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn vertex_path_property_inspector(&self) -> Option<Entity<VertexPathPropertyInspector>> {
        self.vertex_path_property_inspector.clone()
    }

    fn ensure_vertex_path_property_inspector(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<VertexPathPropertyInspector> {
        if let Some(inspector) = &self.vertex_path_property_inspector { return inspector.clone(); }
        let inspector = cx.new(|cx| VertexPathPropertyInspector::new(window, cx));
        let subscription = cx.subscribe(&inspector, |workspace, _, event: &VertexPathPropertyEvent, cx| {
            if let Err(error) = workspace.apply_vertex_path_property_event(event, cx) { workspace.last_file_error = Some(error); cx.notify(); }
        });
        self.vertex_path_property_subscription = Some(subscription);
        self.vertex_path_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn set_vertex_path_property_inspector_open(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.vertex_path_property_inspector_open = open;
        self.ensure_vertex_path_property_inspector(window, cx).update(cx, |inspector, cx| inspector.set_open(open, cx));
        cx.notify();
    }

    pub fn apply_vertex_path_property_event(&mut self, event: &VertexPathPropertyEvent, cx: &mut Context<Self>) -> Result<bool, String> {
        if self.active_document_id != Some(event.document_id) { return Ok(false); }
        if self.pending_close_document_id == Some(event.document_id)
            || self.close_after_save_document_id == Some(event.document_id)
            || self.pending_save_prompt_document_id() == Some(event.document_id)
            || self.pending_text_box_editor.as_ref().is_some_and(|editor| editor.document_id == event.document_id) { return Ok(false); }
        let Some(session) = self.session(event.document_id, cx).cloned() else { return Ok(false); };
        let state = session.read(cx);
        if !matches!(state.status, NativeDocumentStatus::Ready) || state.save_status == NativeDocumentSaveStatus::Saving || state.pending_rotation_generation.is_some() { return Ok(false); }
        let Some(revision) = state.annotations.snapshot(event.document_id.value()).map(|snapshot| snapshot.revision) else { return Ok(false); };
        let selected = state.annotations.selected_annotations_in_document_order(event.document_id.value());
        let (current_id, current_kind, appearance, locked, measurement_path) =
            match selected.as_slice() {
                [Annotation::VertexPath(current)] => (
                    &current.id,
                    PathPropertyKind::from(current.kind),
                    &current.appearance,
                    current.locked,
                    false,
                ),
                [Annotation::MeasurementPath(current)] => (
                    &current.id,
                    PathPropertyKind::from(current.kind),
                    &current.appearance,
                    current.locked,
                    true,
                ),
                _ => return Ok(false),
            };
        if revision != event.expected_revision
            || current_id != &event.annotation_id
            || current_kind != event.expected_kind
            || path_property_patch_matches(&event.patch, appearance, locked)
        {
            return Ok(false);
        }
        if locked && !matches!(event.patch, VertexPathPropertyPatch::Locked(_)) { return Ok(false); }
        if !current_kind.supports_fill() && matches!(event.patch, VertexPathPropertyPatch::FillColor(_)) { return Ok(false); }
        let edit_path = |workspace: &mut Self, edit, cx: &mut Context<Self>| {
            if measurement_path {
                workspace.edit_selected_measurement_path_property(event.document_id, edit, cx)
            } else {
                workspace.edit_selected_vertex_path_property(event.document_id, edit, cx)
            }
        };
        match &event.patch {
            VertexPathPropertyPatch::Locked(value) => self.update_annotation_history(event.document_id, cx, |annotations, document_id| annotations.set_selected_locked(document_id, *value))?,
            VertexPathPropertyPatch::StrokeColor(value) => edit_path(self, VertexPathPropertyEdit::StrokeColor(value.clone()), cx)?,
            VertexPathPropertyPatch::StrokeWidthPt(value) if value.is_finite() && (0.25..=24.).contains(value) => edit_path(self, VertexPathPropertyEdit::StrokeWidthPt(*value), cx)?,
            VertexPathPropertyPatch::Opacity(value) if value.is_finite() && (0.0..=1.).contains(value) => edit_path(self, VertexPathPropertyEdit::Opacity(*value), cx)?,
            VertexPathPropertyPatch::FillColor(value) => edit_path(self, VertexPathPropertyEdit::FillColor(value.clone()), cx)?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn text_box_property_inspector(&self) -> Option<Entity<TextBoxPropertyInspector>> {
        self.text_box_property_inspector.clone()
    }

    fn ensure_text_box_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextBoxPropertyInspector> {
        if let Some(inspector) = &self.text_box_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| TextBoxPropertyInspector::new(window, cx));
        let subscription = cx.subscribe(
            &inspector,
            |workspace, _, event: &TextBoxPropertyEvent, cx| {
                if let Err(error) = workspace.apply_text_box_property_event(event, cx) {
                    workspace
                        .annotation_statuses
                        .insert(event.document_id, error);
                    cx.notify();
                }
            },
        );
        self.text_box_property_subscription = Some(subscription);
        self.text_box_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn set_text_box_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text_box_property_inspector_open = open;
        self.ensure_text_box_property_inspector(window, cx)
            .update(cx, |inspector, cx| inspector.set_open(open, cx));
        cx.notify();
    }

    pub fn apply_text_box_property_event(
        &mut self,
        event: &TextBoxPropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.pending_close_document_id == Some(event.document_id)
            || self.close_after_save_document_id == Some(event.document_id)
        {
            return Ok(false);
        }
        let Some(session) = self.session(event.document_id, cx).cloned() else {
            return Ok(false);
        };
        let session_state = session.read(cx);
        if !matches!(session_state.status, NativeDocumentStatus::Ready)
            || session_state.save_status == NativeDocumentSaveStatus::Saving
            || session_state.pending_rotation_generation.is_some()
        {
            return Ok(false);
        }
        let Some(snapshot) = session_state
            .annotations
            .snapshot(event.document_id.value())
        else {
            return Ok(false);
        };
        let Some(current) = session_state
            .annotations
            .exact_selected_text_box(event.document_id.value())
        else {
            return Ok(false);
        };
        if snapshot.revision != event.expected_revision || current.id != event.annotation_id {
            return Ok(false);
        }
        if matches!(&event.patch, TextBoxPropertyPatch::Locked(value) if *value == current.locked)
            || matches!(&event.patch, TextBoxPropertyPatch::Style(style) if style == current.style())
        {
            return Ok(false);
        }
        if current.locked && !matches!(event.patch, TextBoxPropertyPatch::Locked(_)) {
            return Err(format!("markup {} is locked", event.annotation_id));
        }
        match &event.patch {
            TextBoxPropertyPatch::Locked(locked) => {
                self.update_annotation_history(event.document_id, cx, |annotations, id| {
                    annotations.set_selected_locked(id, *locked)
                })?
            }
            TextBoxPropertyPatch::Style(style) => {
                let style = style.clone();
                self.update_annotation_history(event.document_id, cx, move |annotations, id| {
                    annotations.set_exact_selected_text_box_style(id, style)
                })?;
            }
        }
        Ok(true)
    }

    pub fn measurement_property_inspector(&self) -> Option<Entity<MeasurementPropertyInspector>> {
        self.measurement_property_inspector.clone()
    }

    fn ensure_measurement_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<MeasurementPropertyInspector> {
        if let Some(inspector) = &self.measurement_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|_| MeasurementPropertyInspector::new());
        let subscription = cx.subscribe_in(
            &inspector,
            window,
            |workspace, _, event: &MeasurementPropertyEvent, window, cx| {
                if let Err(error) = workspace.apply_measurement_property_event(event, window, cx) {
                    workspace
                        .annotation_statuses
                        .insert(event.document_id, error);
                    cx.notify();
                }
            },
        );
        self.measurement_property_subscription = Some(subscription);
        self.measurement_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn set_measurement_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.measurement_property_inspector_open = open;
        self.ensure_measurement_property_inspector(window, cx)
            .update(cx, |inspector, cx| inspector.set_open(open, cx));
        cx.notify();
    }

    pub fn apply_measurement_property_event(
        &mut self,
        event: &MeasurementPropertyEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.pending_close_document_id == Some(event.document_id)
            || self.close_after_save_document_id == Some(event.document_id)
        {
            return Ok(false);
        }
        let Some(session) = self.session(event.document_id, cx).cloned() else {
            return Ok(false);
        };
        let session_state = session.read(cx);
        if !matches!(session_state.status, NativeDocumentStatus::Ready)
            || session_state.save_status == NativeDocumentSaveStatus::Saving
            || session_state.pending_rotation_generation.is_some()
        {
            return Ok(false);
        }
        let Some(snapshot) = session_state
            .annotations
            .snapshot(event.document_id.value())
        else {
            return Ok(false);
        };
        let selected = session_state
            .annotations
            .selected_annotations_in_document_order(event.document_id.value());
        let (current_id, current_kind, page_index, show_caption, locked) = match selected.as_slice()
        {
            [Annotation::Length(annotation)] => (
                annotation.id.clone(),
                AnnotationKind::Length,
                annotation.page_index,
                annotation.calibration().show_caption(),
                annotation.locked,
            ),
            [Annotation::MeasurementPath(annotation)] => (
                annotation.id.clone(),
                annotation.kind.into(),
                annotation.page_index,
                annotation.calibration().show_caption(),
                annotation.locked,
            ),
            _ => return Ok(false),
        };
        if snapshot.revision != event.expected_revision
            || current_id != event.annotation_id
            || current_kind != event.annotation_kind
            || page_index != event.page_index
        {
            return Ok(false);
        }
        match event.action {
            MeasurementPropertyAction::ShowCaption(value) => {
                if value == show_caption {
                    return Ok(false);
                }
                if locked {
                    return Err(format!("markup {} is locked", event.annotation_id));
                }
                self.update_annotation_history(event.document_id, cx, move |annotations, id| {
                    annotations.set_exact_selected_measurement_show_caption(id, value)
                })?;
            }
            MeasurementPropertyAction::OpenPageScale => {
                let current_scale = session_state
                    .annotations
                    .document_page_scale(event.document_id.value(), page_index)
                    .cloned();
                let mut presets = session_state
                    .annotations
                    .document_scale_presets(event.document_id.value())
                    .map(ToOwned::to_owned)
                    .unwrap_or_default();
                presets.extend(built_in_scale_presets());
                self.ensure_page_scale_control(window, cx)
                    .update(cx, |control, cx| {
                        control.open_for_state(
                            event.document_id,
                            page_index,
                            current_scale,
                            presets,
                            window,
                            cx,
                        )
                    });
            }
        }
        Ok(true)
    }

    pub fn set_rectangle_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_rectangular_shape_property_inspector_open(open, window, cx);
    }

    pub fn set_rectangular_shape_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.rectangular_shape_property_inspector_open = open;
        let rectangle = self.ensure_rectangle_property_inspector(window, cx);
        let ellipse = self.ensure_ellipse_property_inspector(window, cx);
        for inspector in [rectangle, ellipse] {
            inspector.update(cx, |inspector, cx| {
                if open {
                    inspector.open(cx);
                } else {
                    inspector.close(cx);
                }
            });
        }
        cx.notify();
    }

    pub fn apply_rectangle_property_event(
        &mut self,
        event: &RectanglePropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if event.expected_kind != RectangularShapePropertyKind::Rectangle {
            return Ok(false);
        }
        self.apply_rectangular_shape_property_event(event, cx)
    }

    pub fn apply_ellipse_property_event(
        &mut self,
        event: &RectanglePropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if event.expected_kind != RectangularShapePropertyKind::Ellipse {
            return Ok(false);
        }
        self.apply_rectangular_shape_property_event(event, cx)
    }

    pub fn apply_rectangular_shape_property_event(
        &mut self,
        event: &RectanglePropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let Some(session) = self.session(event.document_id, cx).cloned() else {
            return Ok(false);
        };
        let selected = session
            .read(cx)
            .annotations
            .selected_annotations_in_document_order(event.document_id.value());
        let current = match selected.as_slice() {
            [Annotation::Rectangle(annotation)]
                if event.expected_kind == RectangularShapePropertyKind::Rectangle =>
            {
                Some((
                    annotation.id.clone(),
                    annotation.rect,
                    annotation.rotation_degrees,
                    annotation.appearance.clone(),
                    annotation.locked,
                ))
            }
            [Annotation::Ellipse(annotation)]
                if event.expected_kind == RectangularShapePropertyKind::Ellipse =>
            {
                Some((
                    annotation.id.clone(),
                    annotation.rect,
                    annotation.rotation_degrees,
                    annotation.appearance.clone(),
                    annotation.locked,
                ))
            }
            _ => None,
        };
        let Some((current_id, current_rect, current_rotation, current_appearance, current_locked)) =
            current
        else {
            return Ok(false);
        };
        if current_id != event.annotation_id {
            return Ok(false);
        }
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Ok(false);
        }
        if rectangular_shape_patch_matches(
            &event.patch,
            current_rect,
            current_rotation,
            &current_appearance,
            current_locked,
        ) {
            return Ok(false);
        }
        if current_locked && !matches!(&event.patch, RectanglePropertyPatch::Locked(_)) {
            return Err(format!("markup {} is locked", event.annotation_id));
        }

        session.update(cx, |session, cx| {
            let annotations = &mut session.annotations;
            match &event.patch {
                RectanglePropertyPatch::Locked(locked) => annotations
                    .set_selected_locked(event.document_id.value(), *locked)
                    .map_err(|error| error.to_string())?,
                RectanglePropertyPatch::StrokeColor(color) => {
                    let appearance = RectangleAppearance::new(
                        color,
                        current_appearance.stroke_width_pt(),
                        current_appearance.fill_color(),
                        current_appearance.opacity(),
                    )
                    .and_then(|appearance| {
                        appearance.with_fill_opacity(current_appearance.fill_opacity())
                    })
                    .map(|appearance| {
                        appearance.with_stroke_style(current_appearance.stroke_style())
                    })
                    .map_err(|error| error.to_string())?;
                    annotations
                        .set_selected_rectangle_appearance(event.document_id.value(), appearance)
                        .map_err(|error| error.to_string())?;
                }
                RectanglePropertyPatch::Opacity(opacity) => {
                    let appearance = RectangleAppearance::new(
                        current_appearance.stroke_color(),
                        current_appearance.stroke_width_pt(),
                        current_appearance.fill_color(),
                        *opacity,
                    )
                    .and_then(|appearance| {
                        appearance.with_fill_opacity(current_appearance.fill_opacity())
                    })
                    .map(|appearance| {
                        appearance.with_stroke_style(current_appearance.stroke_style())
                    })
                    .map_err(|error| error.to_string())?;
                    annotations
                        .set_selected_rectangle_appearance(event.document_id.value(), appearance)
                        .map_err(|error| error.to_string())?;
                }
                RectanglePropertyPatch::StrokeWidthPt(width) => {
                    let appearance = RectangleAppearance::new(
                        current_appearance.stroke_color(),
                        *width,
                        current_appearance.fill_color(),
                        current_appearance.opacity(),
                    )
                    .and_then(|appearance| {
                        appearance.with_fill_opacity(current_appearance.fill_opacity())
                    })
                    .map(|appearance| {
                        appearance.with_stroke_style(current_appearance.stroke_style())
                    })
                    .map_err(|error| error.to_string())?;
                    annotations
                        .set_selected_rectangle_appearance(event.document_id.value(), appearance)
                        .map_err(|error| error.to_string())?;
                }
                RectanglePropertyPatch::StrokeStyle(style) => {
                    let appearance = current_appearance.clone().with_stroke_style(*style);
                    annotations
                        .set_selected_rectangle_appearance(event.document_id.value(), appearance)
                        .map_err(|error| error.to_string())?;
                }
                RectanglePropertyPatch::FillColor(fill) => {
                    let appearance = RectangleAppearance::new(
                        current_appearance.stroke_color(),
                        current_appearance.stroke_width_pt(),
                        fill.as_deref(),
                        current_appearance.opacity(),
                    )
                    .and_then(|appearance| {
                        appearance.with_fill_opacity(current_appearance.fill_opacity())
                    })
                    .map(|appearance| {
                        appearance.with_stroke_style(current_appearance.stroke_style())
                    })
                    .map_err(|error| error.to_string())?;
                    annotations
                        .set_selected_rectangle_appearance(event.document_id.value(), appearance)
                        .map_err(|error| error.to_string())?;
                }
                RectanglePropertyPatch::FillOpacity(opacity) => {
                    let appearance = current_appearance
                        .clone()
                        .with_fill_opacity(*opacity)
                        .map_err(|error| error.to_string())?;
                    annotations
                        .set_selected_rectangle_appearance(event.document_id.value(), appearance)
                        .map_err(|error| error.to_string())?;
                }
                RectanglePropertyPatch::X(x) => {
                    let rect =
                        PdfRect::new(*x, current_rect.y, current_rect.width, current_rect.height)
                            .map_err(|error| error.to_string())?;
                    set_rectangular_shape_rect(
                        annotations,
                        event.document_id,
                        event.annotation_id.clone(),
                        event.expected_kind,
                        rect,
                    )?;
                }
                RectanglePropertyPatch::Y(y) => {
                    let rect =
                        PdfRect::new(current_rect.x, *y, current_rect.width, current_rect.height)
                            .map_err(|error| error.to_string())?;
                    set_rectangular_shape_rect(
                        annotations,
                        event.document_id,
                        event.annotation_id.clone(),
                        event.expected_kind,
                        rect,
                    )?;
                }
                RectanglePropertyPatch::Width(width) => {
                    let rect =
                        PdfRect::new(current_rect.x, current_rect.y, *width, current_rect.height)
                            .map_err(|error| error.to_string())?;
                    set_rectangular_shape_rect(
                        annotations,
                        event.document_id,
                        event.annotation_id.clone(),
                        event.expected_kind,
                        rect,
                    )?;
                }
                RectanglePropertyPatch::Height(height) => {
                    let rect =
                        PdfRect::new(current_rect.x, current_rect.y, current_rect.width, *height)
                            .map_err(|error| error.to_string())?;
                    set_rectangular_shape_rect(
                        annotations,
                        event.document_id,
                        event.annotation_id.clone(),
                        event.expected_kind,
                        rect,
                    )?;
                }
                RectanglePropertyPatch::RotationDegrees(rotation) => {
                    set_rectangular_shape_rotation(
                        annotations,
                        event.document_id,
                        event.annotation_id.clone(),
                        event.expected_kind,
                        *rotation,
                    )?;
                }
            }
            cx.notify();
            Ok::<(), String>(())
        })?;
        self.annotation_statuses.remove(&event.document_id);
        cx.notify();
        Ok(true)
    }

    pub fn annotation_tool(&self, document_id: DocumentId, cx: &App) -> Option<AnnotationTool> {
        self.session(document_id, cx)
            .map(|session| session.read(cx).annotations.tool())
    }

    pub fn redact_pending_status_text(&self, cx: &App) -> Option<&'static str> {
        let document_id = self.active_document_id?;
        let session = self.session(document_id, cx)?.read(cx);
        if session.annotations.tool() == AnnotationTool::Redact {
            return Some(PENDING_REDACTION_STATUS);
        }
        let snapshot = session.annotations.snapshot(document_id.value())?;
        let selected_id = snapshot.selected_id.as_ref()?;
        snapshot
            .redacts
            .iter()
            .any(|redact| redact.id == *selected_id)
            .then_some(PENDING_REDACTION_STATUS)
    }

    pub fn pending_text_box_focus(&self, cx: &App) -> Option<FocusHandle> {
        self.pending_text_box_editor
            .as_ref()
            .map(|editor| editor.input.read(cx).focus_handle(cx))
    }

    pub fn pending_text_box_input(&self) -> Option<Entity<TextareaState>> {
        self.pending_text_box_editor
            .as_ref()
            .map(|editor| editor.input.clone())
    }

    pub fn annotation_toolbar_scroll_offset(&self) -> gpui::Point<Pixels> {
        self.toolbar_scroll.offset()
    }

    pub fn pending_text_box_value(&self, cx: &App) -> Option<String> {
        self.pending_text_box_editor
            .as_ref()
            .map(|editor| editor.input.read(cx).value().to_string())
    }

    pub fn text_box_return_focus(&self) -> FocusHandle {
        self.text_box_return_focus.clone()
    }

    pub fn text_box_commit_error(&self) -> Option<&str> {
        self.text_box_commit_error.as_deref()
    }

    pub fn annotation_status(&self) -> Option<String> {
        self.active_document_id
            .and_then(|document_id| self.annotation_statuses.get(&document_id))
            .cloned()
    }

    pub const fn rejected_stale_image_prepares(&self) -> u64 {
        self.rejected_stale_image_prepares
    }

    pub const fn rejected_stale_save_prompts(&self) -> u64 {
        self.rejected_stale_save_prompts
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.workspace_focus.clone()
    }

    pub fn set_length_calibration(
        &mut self,
        document_id: DocumentId,
        calibration: LengthCalibration,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let page_index = session.read(cx).current_page;
        self.set_page_length_calibration(document_id, page_index, calibration, cx)
    }

    pub fn set_page_length_calibration(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        calibration: LengthCalibration,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("page scale target is outside the document".into());
        }
        session.update(cx, |session, cx| {
            session
                .annotations
                .set_document_page_length_calibration(document_id.value(), page_index, calibration)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        self.annotation_statuses.insert(
            document_id,
            format!("Applied page scale to page {}.", page_index + 1),
        );
        cx.notify();
        Ok(())
    }

    pub fn length_placement_pending(&self, document_id: DocumentId, cx: &App) -> bool {
        self.session(document_id, cx).is_some_and(|session| {
            session
                .read(cx)
                .annotations
                .length_placement_pending(document_id.value())
        })
    }

    pub fn dimension_placement_pending(&self, document_id: DocumentId, cx: &App) -> bool {
        self.session(document_id, cx).is_some_and(|session| {
            session
                .read(cx)
                .annotations
                .dimension_placement_pending(document_id.value())
        })
    }

    pub fn arc_placement_pending(&self, document_id: DocumentId, cx: &App) -> bool {
        self.session(document_id, cx).is_some_and(|session| {
            session
                .read(cx)
                .annotations
                .arc_placement_pending(document_id.value())
        })
    }

    pub fn page_length_calibration(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Option<LengthCalibration> {
        self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .document_page_length_calibration(document_id.value(), page_index)
                .cloned()
        })
    }

    pub fn page_scale(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Option<PageScale> {
        self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .document_page_scale(document_id.value(), page_index)
                .cloned()
        })
    }

    pub fn scale_presets(&self, document_id: DocumentId, cx: &App) -> Vec<ScalePreset> {
        self.session(document_id, cx)
            .and_then(|session| {
                session
                    .read(cx)
                    .annotations
                    .document_scale_presets(document_id.value())
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_default()
    }

    pub fn page_count(&self, document_id: DocumentId, cx: &App) -> Option<u32> {
        self.session(document_id, cx)
            .and_then(|session| u32::try_from(session.read(cx).page_sizes.len()).ok())
    }

    pub fn apply_page_scale(
        &mut self,
        document_id: DocumentId,
        scale: PageScale,
        target: PageScaleApplyTarget,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let page_count = u32::try_from(session.read(cx).page_sizes.len())
            .map_err(|_| "document page count exceeds the page-scale limit".to_owned())?;
        let changed = session.update(cx, |session, _| {
            session
                .annotations
                .apply_document_page_scale(document_id.value(), scale, target, page_count)
                .map_err(|error| error.to_string())
        })?;
        if changed {
            cx.notify();
        }
        Ok(changed)
    }

    pub fn apply_page_scale_with_preset(
        &mut self,
        document_id: DocumentId,
        scale: PageScale,
        target: PageScaleApplyTarget,
        saved_preset: Option<ScalePreset>,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let page_count = u32::try_from(session.read(cx).page_sizes.len())
            .map_err(|_| "document page count exceeds the page-scale limit".to_owned())?;
        let changed = session.update(cx, |session, _| {
            session
                .annotations
                .apply_document_page_scale_with_preset(
                    document_id.value(),
                    scale,
                    target,
                    page_count,
                    saved_preset,
                )
                .map_err(|error| error.to_string())
        })?;
        if changed {
            cx.notify();
        }
        Ok(changed)
    }

    pub fn delete_scale_preset(
        &mut self,
        document_id: DocumentId,
        preset_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let changed = session.update(cx, |session, _| {
            session
                .annotations
                .delete_document_scale_preset(document_id.value(), preset_id)
                .map_err(|error| error.to_string())
        })?;
        if changed {
            cx.notify();
        }
        Ok(changed)
    }

    fn begin_pending_text_box(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        anchor: PdfPoint,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pending_text_box_editor.is_some() {
            return false;
        }
        let sequence = self.next_annotation_sequence;
        self.next_annotation_sequence = self.next_annotation_sequence.saturating_add(1);
        let Ok(id) = MarkupId::new(format!("workspace:text:{sequence}")) else {
            return false;
        };
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .soft_wrap(false)
                .placeholder("Text box content")
        });
        self.text_box_commit_error = None;
        let input_subscription = cx.subscribe_in(
            &input,
            window,
            |workspace, _, event: &InputEvent, _, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::Blur => {
                    if let Err(error) = workspace.commit_pending_text_box(cx) {
                        workspace.text_box_commit_error = Some(error);
                        cx.notify();
                    }
                }
                InputEvent::Focus | InputEvent::PressEnter { .. } => {}
            },
        );
        let input_focus = input.read(cx).focus_handle(cx);
        self.pending_text_box_editor = Some(PendingTextBoxEditor {
            document_id,
            page_index,
            target: PendingTextEditorTarget::NewTextBox { id, anchor },
            authority: None,
            input: input.clone(),
        });
        self.pending_text_box_subscriptions.push(input_subscription);
        input_focus.focus(window, cx);
        cx.notify();
        true
    }

    fn begin_pending_composite_text_editor(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        target: PendingTextEditorTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pending_text_box_editor.is_some() {
            return false;
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let (resource_generation, snapshot) = {
            let session_state = session.read(cx);
            let Some(snapshot) = session_state.annotations.snapshot(document_id.value()) else {
                return false;
            };
            (session_state.resource_epoch, snapshot)
        };
        let baseline_revision = snapshot.revision;
        let Some(content) = (match &target {
            PendingTextEditorTarget::Callout { id } => snapshot
                .callouts
                .into_iter()
                .find(|callout| &callout.id == id)
                .map(|callout| callout.content().to_owned()),
            PendingTextEditorTarget::CloudPlus { id } => snapshot
                .cloud_pluses
                .into_iter()
                .find(|cloud_plus| &cloud_plus.id == id)
                .map(|cloud_plus| cloud_plus.content().to_owned()),
            PendingTextEditorTarget::NewDimension { id }
            | PendingTextEditorTarget::ExistingDimension { id } => snapshot
                .dimensions
                .into_iter()
                .find(|dimension| &dimension.id == id && !dimension.locked)
                .map(|dimension| dimension.content().to_owned()),
            PendingTextEditorTarget::ExistingTextBox { id } => snapshot
                .text_boxes
                .into_iter()
                .find(|text_box| &text_box.id == id && !text_box.locked)
                .map(|text_box| text_box.content().to_owned()),
            PendingTextEditorTarget::NewTextBox { .. } => None,
        }) else {
            return false;
        };
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .soft_wrap(false)
                .default_value(content.clone())
        });
        input.update(cx, |input, cx| input.set_submit_on_enter(true, cx));
        self.text_box_commit_error = None;
        let input_subscription = cx.subscribe_in(
            &input,
            window,
            |workspace, _, event: &InputEvent, window, cx| match event {
                InputEvent::Change => cx.notify(),
                InputEvent::Blur => {
                    if let Err(error) = workspace.commit_pending_text_box(cx) {
                        workspace.text_box_commit_error = Some(error);
                        cx.notify();
                    }
                }
                InputEvent::PressEnter { shift: false, .. } => {
                    if let Err(error) = workspace.commit_pending_text_box_from_enter(cx) {
                        workspace.text_box_commit_error = Some(error);
                        cx.notify();
                    } else {
                        workspace.workspace_focus.focus(window, cx);
                    }
                }
                InputEvent::Focus | InputEvent::PressEnter { shift: true, .. } => {}
            },
        );
        let input_focus = input.read(cx).focus_handle(cx);
        self.pending_text_box_editor = Some(PendingTextBoxEditor {
            document_id,
            page_index,
            target,
            authority: Some(PendingTextEditorAuthority {
                resource_generation,
                baseline_revision,
                baseline_text: content,
            }),
            input: input.clone(),
        });
        self.pending_text_box_subscriptions.push(input_subscription);
        input.update(cx, |input, cx| input.select_all(window, cx));
        input_focus.focus(window, cx);
        cx.notify();
        true
    }

    fn cancel_pending_composite_text_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.pending_text_box_editor.as_ref().is_some_and(|editor| {
            matches!(
                editor.target,
                PendingTextEditorTarget::ExistingTextBox { .. }
                    | PendingTextEditorTarget::Callout { .. }
                    | PendingTextEditorTarget::CloudPlus { .. }
                    | PendingTextEditorTarget::NewDimension { .. }
                    | PendingTextEditorTarget::ExistingDimension { .. }
            )
        }) {
            return false;
        }
        let editor = self
            .pending_text_box_editor
            .take()
            .expect("the checked composite editor remains retained");
        self.pending_text_box_subscriptions.clear();
        self.text_box_commit_error = None;
        if !matches!(
            editor.target,
            PendingTextEditorTarget::ExistingTextBox { .. }
                | PendingTextEditorTarget::ExistingDimension { .. }
        ) && let Some(session) = self.session(editor.document_id, cx).cloned()
        {
            session.update(cx, |session, cx| {
                session
                    .annotations
                    .clear_selection(editor.document_id.value());
                cx.notify();
            });
        }
        self.workspace_focus.focus(window, cx);
        cx.notify();
        true
    }

    fn commit_pending_text_box(&mut self, cx: &mut Context<Self>) -> Result<bool, String> {
        self.commit_pending_text_box_with_policy(false, cx)
    }

    fn commit_pending_text_box_from_enter(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        self.commit_pending_text_box_with_policy(true, cx)
    }

    fn commit_pending_text_box_with_policy(
        &mut self,
        trim_composite_submit_newline: bool,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.pending_text_box_editor.as_ref().is_some_and(|editor| {
            matches!(
                editor.target,
                PendingTextEditorTarget::ExistingTextBox { .. }
            )
        }) {
            return self.commit_pending_existing_text_box(cx);
        }
        if self.pending_text_box_editor.as_ref().is_some_and(|editor| {
            matches!(editor.target, PendingTextEditorTarget::ExistingDimension { .. })
        }) {
            return self.commit_pending_existing_dimension(trim_composite_submit_newline, cx);
        }
        if self.pending_text_box_editor.as_ref().is_some_and(|editor| {
            matches!(editor.target, PendingTextEditorTarget::NewDimension { .. })
                && !editor.input.read(cx).value().is_ascii()
        }) {
            return Err("Dimension captions currently support ASCII text only".into());
        }
        let Some(editor) = self.pending_text_box_editor.take() else {
            return Ok(false);
        };
        let mut content = editor.input.read(cx).value().to_string();
        self.pending_text_box_subscriptions.clear();
        if trim_composite_submit_newline
            && matches!(
                &editor.target,
                PendingTextEditorTarget::Callout { .. }
                    | PendingTextEditorTarget::CloudPlus { .. }
                    | PendingTextEditorTarget::NewDimension { .. }
                    | PendingTextEditorTarget::ExistingDimension { .. }
            )
            && content.ends_with('\n')
        {
            content.pop();
        }
        match editor.target {
            PendingTextEditorTarget::NewTextBox { id, anchor } => {
                if content.is_empty() {
                    cx.notify();
                    return Ok(false);
                }
                let lines = content.split('\n').collect::<Vec<_>>();
                let widest = lines
                    .iter()
                    .map(|line| line.chars().count())
                    .max()
                    .unwrap_or(0) as f64;
                let width = (widest * 7.2 + 10.).max(11.);
                let height = lines.len().max(1) as f64 * 13.8 + 4.2;
                let initial_left = anchor.x - 5.5;
                let initial_top = anchor.y + 9.;
                let annotation = TextBoxAnnotation::new(
                    id,
                    editor.page_index,
                    PdfRect::new(initial_left, initial_top - height, width, height)
                        .map_err(|error| error.to_string())?,
                    content,
                    TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                self.create_text_box(editor.document_id, annotation, cx)?;
                Ok(true)
            }
            PendingTextEditorTarget::ExistingTextBox { id } => {
                let Some(session) = self.session(editor.document_id, cx).cloned() else {
                    return Err("document session is closed".into());
                };
                session.update(cx, |session, cx| {
                    if !session
                        .annotations
                        .select_id(editor.document_id.value(), &id)
                    {
                        return Err("Text Box is no longer available".to_owned());
                    }
                    session
                        .annotations
                        .replace_selected_text(editor.document_id.value(), content)
                        .map_err(|error| error.to_string())?;
                    cx.notify();
                    Ok::<(), String>(())
                })?;
                cx.notify();
                Ok(true)
            }
            PendingTextEditorTarget::Callout { id } => {
                let Some(session) = self.session(editor.document_id, cx).cloned() else {
                    return Err("document session is closed".into());
                };
                session.update(cx, |session, cx| {
                    session
                        .annotations
                        .replace_callout_text_in_create_transaction(
                            editor.document_id.value(),
                            &id,
                            content,
                        )
                        .map_err(|error| error.to_string())?;
                    session
                        .annotations
                        .clear_selection(editor.document_id.value());
                    cx.notify();
                    Ok::<(), String>(())
                })?;
                cx.notify();
                Ok(true)
            }
            PendingTextEditorTarget::CloudPlus { id } => {
                let Some(session) = self.session(editor.document_id, cx).cloned() else {
                    return Err("document session is closed".into());
                };
                session.update(cx, |session, cx| {
                    session
                        .annotations
                        .replace_cloud_plus_text_in_create_transaction(
                            editor.document_id.value(),
                            &id,
                            content,
                        )
                        .map_err(|error| error.to_string())?;
                    session
                        .annotations
                        .clear_selection(editor.document_id.value());
                    cx.notify();
                    Ok::<(), String>(())
                })?;
                cx.notify();
                Ok(true)
            }
            PendingTextEditorTarget::NewDimension { id } => {
                let Some(session) = self.session(editor.document_id, cx).cloned() else {
                    return Err("document session is closed".into());
                };
                session.update(cx, |session, cx| {
                    session
                        .annotations
                        .replace_dimension_content_in_create_transaction(
                            editor.document_id.value(),
                            &id,
                            content,
                        )
                        .map_err(|error| error.to_string())?;
                    session
                        .annotations
                        .clear_selection(editor.document_id.value());
                    cx.notify();
                    Ok::<(), String>(())
                })?;
                cx.notify();
                Ok(true)
            }
            PendingTextEditorTarget::ExistingDimension { .. } => {
                unreachable!("existing Dimension captions commit through their authority path")
            }
        }
    }

    fn commit_pending_existing_text_box(&mut self, cx: &mut Context<Self>) -> Result<bool, String> {
        let Some(editor) = self.pending_text_box_editor.as_ref() else {
            return Ok(false);
        };
        let PendingTextEditorTarget::ExistingTextBox { id } = &editor.target else {
            return Ok(false);
        };
        let authority = editor
            .authority
            .as_ref()
            .ok_or_else(|| "Text Box editor authority is missing".to_owned())?;
        let document_id = editor.document_id;
        let page_index = editor.page_index;
        let id = id.clone();
        let content = editor.input.read(cx).value().to_string();
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        {
            let session = session.read(cx);
            if !matches!(session.status, NativeDocumentStatus::Ready) {
                return Err("document session is not ready".into());
            }
            if session.save_status == NativeDocumentSaveStatus::Saving {
                return Err("document save is already in progress".into());
            }
            if session.pending_rotation_generation.is_some() {
                return Err("page rotation pixels are still pending".into());
            }
            let snapshot = session
                .annotations
                .snapshot(document_id.value())
                .ok_or_else(|| "document has no annotation state".to_owned())?;
            let target = snapshot
                .text_boxes
                .iter()
                .find(|text_box| text_box.id == id && text_box.page_index == page_index);
            validate_existing_text_editor_authority(
                authority,
                session.resource_epoch,
                snapshot.revision,
                target.map(|target| (target.content(), target.locked)),
            )?;
        }
        if content == authority.baseline_text {
            self.pending_text_box_editor = None;
            self.pending_text_box_subscriptions.clear();
            cx.notify();
            return Ok(false);
        }
        session.update(cx, |session, cx| {
            if !session.annotations.select_id(document_id.value(), &id) {
                return Err("Text Box is no longer available".to_owned());
            }
            session
                .annotations
                .replace_selected_text(document_id.value(), content)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        self.pending_text_box_editor = None;
        self.pending_text_box_subscriptions.clear();
        cx.notify();
        Ok(true)
    }

    fn commit_pending_existing_dimension(
        &mut self,
        trim_submit_newline: bool,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let Some(editor) = self.pending_text_box_editor.as_ref() else {
            return Ok(false);
        };
        let PendingTextEditorTarget::ExistingDimension { id } = &editor.target else {
            return Ok(false);
        };
        let authority = editor
            .authority
            .as_ref()
            .ok_or_else(|| "Dimension editor authority is missing".to_owned())?;
        let document_id = editor.document_id;
        let page_index = editor.page_index;
        let id = id.clone();
        let mut content = editor.input.read(cx).value().to_string();
        if trim_submit_newline && content.ends_with('\n') {
            content.pop();
        }
        if !content.is_ascii() {
            return Err("Dimension captions currently support ASCII text only".into());
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        {
            let session = session.read(cx);
            if !matches!(session.status, NativeDocumentStatus::Ready)
                || session.save_status == NativeDocumentSaveStatus::Saving
                || session.pending_rotation_generation.is_some()
            {
                return Err("Dimension editor is no longer writable".into());
            }
            let snapshot = session
                .annotations
                .snapshot(document_id.value())
                .ok_or_else(|| "document has no annotation state".to_owned())?;
            let target = snapshot
                .dimensions
                .iter()
                .find(|dimension| dimension.id == id && dimension.page_index == page_index);
            validate_existing_text_editor_authority(
                authority,
                session.resource_epoch,
                snapshot.revision,
                target.map(|target| (target.content(), target.locked)),
            )?;
        }
        if content == authority.baseline_text {
            self.pending_text_box_editor = None;
            self.pending_text_box_subscriptions.clear();
            self.text_box_commit_error = None;
            cx.notify();
            return Ok(false);
        }
        session.update(cx, |session, cx| {
            if !session.annotations.select_id(document_id.value(), &id) {
                return Err("Dimension is no longer available".to_owned());
            }
            session
                .annotations
                .replace_selected_dimension_content(document_id.value(), content)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        self.pending_text_box_editor = None;
        self.pending_text_box_subscriptions.clear();
        self.text_box_commit_error = None;
        cx.notify();
        Ok(true)
    }

    pub fn create_text_box(
        &mut self,
        document_id: DocumentId,
        annotation: TextBoxAnnotation,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("document session is not ready".into());
        }
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        if annotation.page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("text box page is outside the document".into());
        }
        session.update(cx, |session, cx| {
            session
                .annotations
                .create_text_box(document_id.value(), annotation)
                .map_err(|error| error.to_string())?;
            session.annotations.clear_selection(document_id.value());
            session
                .annotations
                .set_tool(AnnotationTool::Select)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn replace_selected_text(
        &mut self,
        document_id: DocumentId,
        content: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let content = content.into();
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.replace_selected_text(document_id, content)
        })
    }

    pub fn resize_selected_text(
        &mut self,
        document_id: DocumentId,
        width: f64,
        height: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.resize_selected_text(document_id, width, height)
        })
    }

    pub fn move_selected_text(
        &mut self,
        document_id: DocumentId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.move_selected_text(document_id, delta_x, delta_y)
        })
    }

    pub fn session(&self, id: DocumentId, cx: &App) -> Option<&Entity<NativeDocumentSession>> {
        self.sessions
            .iter()
            .find(|session| session.read(cx).id == id)
    }

    pub fn semantic_snap_settings(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<SemanticSnapSettings> {
        self.session(document_id, cx)
            .map(|session| session.read(cx).annotations.semantic_snap_settings())
    }

    pub fn semantic_snap_decision(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<SemanticSnapDecision> {
        self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .semantic_snap_decision()
                .cloned()
        })
    }

    fn apply_semantic_snap_settings(
        &mut self,
        settings: SemanticSnapSettings,
        cx: &mut Context<Self>,
    ) {
        self.semantic_snap_settings = settings;
        for session in self.sessions.clone() {
            session.update(cx, |session, _| {
                session
                    .annotations
                    .set_semantic_snap_settings(settings)
                    .expect("controlled semantic snap settings are valid");
            });
        }
        cx.notify();
    }

    fn set_semantic_snap_annotations_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        self.apply_semantic_snap_settings(
            self.semantic_snap_settings.with_annotation_source(enabled),
            cx,
        );
    }

    fn set_semantic_snap_target(
        &mut self,
        target: SemanticSnapTarget,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.apply_semantic_snap_settings(
            self.semantic_snap_settings.with_target(target, enabled),
            cx,
        );
    }

    fn next_generation(&mut self) -> u64 {
        let generation = self.next_generation;
        self.next_generation = self.next_generation.saturating_add(1);
        generation
    }

    pub fn begin_open(&mut self, path: PathBuf, cx: &mut Context<Self>) -> OpenDocumentRequest {
        let document_id = DocumentId::new(self.next_document_id);
        self.next_document_id = self.next_document_id.saturating_add(1);
        let generation = self.next_generation();
        let semantic_snap_settings = self.semantic_snap_settings;
        let session = cx.new(|_| {
            let mut session = NativeDocumentSession::opening(document_id, path.clone(), generation);
            session
                .annotations
                .set_semantic_snap_settings(semantic_snap_settings)
                .expect("controlled semantic snap settings are valid");
            session
        });
        self.observe_viewer_session(&session, cx);
        self.register_session_tab(document_id, cx);
        self.sessions.push(session);
        cx.notify();
        OpenDocumentRequest {
            document_id,
            generation,
            path,
        }
    }

    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) -> DocumentId {
        let request = self.begin_open(path, cx);
        let document_id = request.document_id;
        let Some(opener) = self.opener.clone() else {
            let _ = self.apply_open_result(
                &request,
                Err("no document opener is configured".into()),
                cx,
            );
            return document_id;
        };
        let task = cx.background_executor().spawn(async move {
            let result = opener.open(&request);
            (request, result)
        });
        cx.spawn(async move |entity, cx| {
            let (request, result) = task.await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.apply_open_result(&request, result, cx);
            });
        })
        .detach();
        document_id
    }

    pub fn open_documents(
        &mut self,
        request: DocumentOpenBatchRequest,
        cx: &mut Context<Self>,
    ) -> DocumentOpenBatchDisposition {
        if request.cancelled {
            return DocumentOpenBatchDisposition::Cancelled;
        }
        let mut seen = std::collections::HashSet::new();
        let candidates = request
            .paths
            .into_iter()
            .filter(|path| !path.as_os_str().as_encoded_bytes().is_empty() && is_pdf_path(path))
            .filter(|path| seen.insert(normalized_document_path(path)))
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return DocumentOpenBatchDisposition::NoAcceptedPaths;
        }

        self.pending_session_restore = None;
        let batch_id = self.next_open_batch_id;
        self.next_open_batch_id = self.next_open_batch_id.saturating_add(1);
        self.latest_open_batch_id = Some(batch_id);
        self.document_open_status = DocumentOpenBatchStatus::Opening {
            batch_id,
            origin: request.origin,
            candidate_count: candidates.len(),
        };
        self.document_open_failures.clear();
        let candidate_count = candidates.len();
        let force_new_tabs = request.origin == DocumentOpenOrigin::Drop;
        let opener = self.opener.clone();
        let background = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            let mut opened = Vec::new();
            let mut opened_titles = Vec::new();
            let mut duplicate_to_focus = None;
            let mut failures = Vec::new();
            let mut failed_count = 0usize;

            for path in candidates {
                let decision = match entity.update(cx, |workspace, cx| {
                    if !force_new_tabs
                        && let Some(existing) = workspace.sessions.iter().find_map(|session| {
                            let session = session.read(cx);
                            (!matches!(session.status, NativeDocumentStatus::Failed(_))
                                && normalized_document_path(&session.path)
                                    == normalized_document_path(&path))
                            .then_some(session.id)
                        })
                    {
                        OpenSelectionDecision::Existing(existing)
                    } else {
                        OpenSelectionDecision::Begin(workspace.begin_open(path.clone(), cx))
                    }
                }) {
                    Ok(decision) => decision,
                    Err(_) => return,
                };
                let open_request = match decision {
                    OpenSelectionDecision::Existing(document_id) => {
                        duplicate_to_focus.get_or_insert(document_id);
                        continue;
                    }
                    OpenSelectionDecision::Begin(request) => request,
                };
                let result = if let Some(opener) = opener.clone() {
                    let request_for_open = open_request.clone();
                    background
                        .spawn(async move { opener.open(&request_for_open) })
                        .await
                } else {
                    Err("no document opener is configured".into())
                };
                let restart_view = match entity.update(cx, |workspace, _| {
                    workspace
                        .pending_session_restore
                        .as_ref()
                        .filter(|restore| restore.batch_id == batch_id)
                        .and_then(|restore| {
                            restore
                                .restart_views
                                .iter()
                                .find_map(|(restored_path, view)| {
                                    (normalized_path_key(restored_path)
                                        == normalized_path_key(&path))
                                    .then_some(*view)
                                })
                        })
                }) {
                    Ok(view) => view,
                    Err(_) => return,
                };
                let restored_page = match (&result, restart_view) {
                    (Ok(opened), Some(view)) => {
                        let page_index = view
                            .current_page()
                            .min(opened.page_sizes.len().saturating_sub(1) as u32);
                        if page_index == 0 {
                            None
                        } else {
                            let resource = opened.resource.clone();
                            Some(
                                background
                                    .spawn(async move {
                                        resource.render_page(page_index, DEFAULT_PAGE_RENDER_WIDTH)
                                    })
                                    .await,
                            )
                        }
                    }
                    _ => None,
                };
                let document_id = open_request.document_id;
                let applied = entity.update(cx, |workspace, cx| {
                    let preserved_active = workspace.active_document_id;
                    workspace.apply_open_result(&open_request, result, cx);
                    if let Some(view) = restart_view {
                        workspace.apply_restart_view(document_id, view, restored_page, cx);
                    }
                    let outcome = workspace.session(document_id, cx).map(|session| {
                        let session = session.read(cx);
                        match &session.status {
                            NativeDocumentStatus::Ready => Ok(session.title.clone()),
                            NativeDocumentStatus::Failed(error) => Err(error.clone()),
                            NativeDocumentStatus::Opening => {
                                Err("the document did not finish opening".into())
                            }
                        }
                    });
                    workspace.active_document_id = preserved_active;
                    if !matches!(outcome, Some(Ok(_))) {
                        let _ = workspace.close_document(document_id, cx);
                        workspace.active_document_id = preserved_active;
                    }
                    workspace.sync_active_viewer_toolbar(cx);
                    outcome
                });
                match applied {
                    Ok(Some(Ok(title))) => {
                        opened.push(document_id);
                        opened_titles.push(title);
                    }
                    Ok(Some(Err(error))) => {
                        failed_count = failed_count.saturating_add(1);
                        failures.push(DocumentOpenFailure {
                            path,
                            message: error,
                        });
                    }
                    Ok(None) | Err(_) => return,
                }
            }

            let _ = entity.update(cx, |workspace, cx| {
                if workspace.latest_open_batch_id != Some(batch_id) {
                    return;
                }
                let intended_path = workspace
                    .pending_session_restore
                    .take()
                    .filter(|restore| restore.batch_id == batch_id)
                    .and_then(|restore| restore.intended_path);
                let focused_existing = if opened.is_empty() {
                    duplicate_to_focus
                } else {
                    None
                };
                if let Some(document_id) = opened.first().copied().or(focused_existing) {
                    workspace.activate_document(document_id, cx);
                }
                if let Some(intended_path) = intended_path {
                    let intended_document_id = workspace.sessions.iter().find_map(|session| {
                        let session = session.read(cx);
                        (matches!(session.status, NativeDocumentStatus::Ready)
                            && normalized_document_path(&session.path)
                                == normalized_document_path(&intended_path))
                        .then_some(session.id)
                    });
                    if let Some(document_id) = intended_document_id {
                        workspace.activate_document(document_id, cx);
                    }
                }
                let status_message = if opened.len() > 1 {
                    format!("Loaded {} documents", opened.len())
                } else if let Some(title) = opened_titles.first() {
                    format!("Loaded {title}")
                } else {
                    "Focused existing document".into()
                };
                workspace.document_open_failures = failures;
                workspace.document_open_status = DocumentOpenBatchStatus::Completed {
                    batch_id,
                    opened,
                    focused_existing,
                    failed_count,
                    status_message,
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        DocumentOpenBatchDisposition::Started {
            batch_id,
            candidate_count,
        }
    }

    pub fn create_generated_document(
        &mut self,
        store: GeneratedDocumentStore,
        request: GeneratedDocumentRequest,
        cx: &mut Context<Self>,
    ) -> Result<DocumentId, String> {
        let document_id = DocumentId::new(self.next_document_id);
        let source = store
            .create(&format!("document-{}", document_id.value()), &request)
            .map_err(|error| error.to_string())?;
        self.create_owned_template_document(store, source, cx)
    }

    pub fn create_owned_template_document(
        &mut self,
        store: GeneratedDocumentStore,
        source: OwnedGeneratedDocument,
        cx: &mut Context<Self>,
    ) -> Result<DocumentId, String> {
        let document_id = DocumentId::new(self.next_document_id);
        self.next_document_id = self.next_document_id.saturating_add(1);
        let generation = self.next_generation();
        let path = source.path().to_owned();
        let semantic_snap_settings = self.semantic_snap_settings;
        let session = cx.new(|_| {
            let mut session =
                NativeDocumentSession::opening_generated(document_id, source, store, generation);
            session
                .annotations
                .set_semantic_snap_settings(semantic_snap_settings)
                .expect("controlled semantic snap settings are valid");
            session
        });
        self.observe_viewer_session(&session, cx);
        self.register_session_tab(document_id, cx);
        self.sessions.push(session);
        cx.notify();
        let open_request = OpenDocumentRequest {
            document_id,
            generation,
            path,
        };
        let Some(opener) = self.opener.clone() else {
            let _ = self.apply_open_result(
                &open_request,
                Err("no document opener is configured".into()),
                cx,
            );
            return Ok(document_id);
        };
        let task = cx.background_executor().spawn(async move {
            let result = opener.open(&open_request);
            (open_request, result)
        });
        cx.spawn(async move |entity, cx| {
            let (request, result) = task.await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.apply_open_result(&request, result, cx);
            });
        })
        .detach();
        Ok(document_id)
    }

    fn prompt_to_open_documents(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open PDFs".into()),
        });
        cx.spawn(async move |entity, cx| {
            let paths = match picker.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.open_documents(
                            DocumentOpenBatchRequest::cancelled(DocumentOpenOrigin::Picker),
                            cx,
                        );
                    });
                    return;
                }
                Ok(Err(error)) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.last_file_error =
                            Some(format!("Could not open the file picker: {error}"));
                        cx.notify();
                    });
                    return;
                }
                Err(error) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.last_file_error =
                            Some(format!("The file picker closed unexpectedly: {error}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = entity.update(cx, |workspace, cx| {
                workspace.open_documents(
                    DocumentOpenBatchRequest::new(DocumentOpenOrigin::Picker, paths),
                    cx,
                );
            });
        })
        .detach();
    }

    fn open_pdf_from_action(&mut self, _: &OpenPdf, _: &mut Window, cx: &mut Context<Self>) {
        self.prompt_to_open_documents(cx);
    }

    fn save_as_from_action(&mut self, _: &SaveAs, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(document_id) = self.active_document_id {
            self.prompt_to_save_as(document_id, cx);
        }
    }

    fn save_from_action(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        let had_pending_editor = self.pending_text_box_editor.is_some();
        self.save_active_document(cx);
        if had_pending_editor
            && self.pending_text_box_editor.is_none()
            && self.text_box_commit_error.is_none()
        {
            self.workspace_focus.focus(window, cx);
        }
    }

    fn save_active_document(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        if self.document_requires_save_as(document_id, cx) {
            self.prompt_to_save_as(document_id, cx);
            return;
        }
        let _ = self.save_path(document_id, cx);
    }

    fn retry_document_save_failure(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let operation = self
            .document_save_failure(document_id, cx)
            .map(|failure| failure.operation);
        match operation {
            Some(DocumentSaveFailureOperation::InPlace) => {
                if self.document_requires_save_as(document_id, cx) {
                    self.prompt_to_save_as(document_id, cx);
                } else {
                    let _ = self.save_path(document_id, cx);
                }
            }
            Some(DocumentSaveFailureOperation::SaveAs) => {
                self.prompt_to_save_as(document_id, cx);
            }
            None => {}
        }
    }

    fn prompt_to_save_as(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        self.prompt_to_save_as_with_intent(document_id, false, cx);
    }

    fn prompt_to_save_pending_close(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.pending_close_document_id else {
            return;
        };
        if self.document_requires_save_as(document_id, cx) {
            self.prompt_to_save_as_with_intent(document_id, true, cx);
        } else if let Err(error) = self.dirty_close_save_path(cx) {
            self.annotation_statuses.insert(document_id, error);
            cx.notify();
        }
    }

    fn prompt_to_save_as_with_intent(
        &mut self,
        document_id: DocumentId,
        close_after_save: bool,
        cx: &mut Context<Self>,
    ) {
        if self.pending_save_prompt.is_some() {
            return;
        }
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let (spec, authority) = {
            let session = session.read(cx);
            if !matches!(session.status, NativeDocumentStatus::Ready)
                || session.save_status == NativeDocumentSaveStatus::Saving
            {
                return;
            }
            (
                save_as_prompt_spec(&session.path),
                SavePromptAuthority {
                    document_id,
                    document_generation: session.generation,
                    close_after_save,
                },
            )
        };
        self.pending_save_prompt = Some(authority);
        let picker = cx.prompt_for_new_path(&spec.directory, Some(&spec.suggested_name));
        cx.spawn(async move |entity, cx| {
            let selected = match picker.await {
                Ok(Ok(path)) => path,
                Ok(Err(error)) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        if !workspace.finish_save_prompt(authority) {
                            return;
                        }
                        let message = format!("Could not open the Save As picker: {error}");
                        if authority.close_after_save {
                            workspace
                                .annotation_statuses
                                .insert(authority.document_id, message);
                        } else {
                            workspace.record_document_save_failure(
                                authority.document_id,
                                DocumentSaveFailureOperation::SaveAs,
                                message,
                                cx,
                            );
                        }
                        cx.notify();
                    });
                    return;
                }
                Err(error) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        if !workspace.finish_save_prompt(authority) {
                            return;
                        }
                        let message = format!("The Save As picker closed unexpectedly: {error}");
                        if authority.close_after_save {
                            workspace
                                .annotation_statuses
                                .insert(authority.document_id, message);
                        } else {
                            workspace.record_document_save_failure(
                                authority.document_id,
                                DocumentSaveFailureOperation::SaveAs,
                                message,
                                cx,
                            );
                        }
                        cx.notify();
                    });
                    return;
                }
            };
            let Some(path) = selected else {
                let _ = entity.update(cx, |workspace, cx| {
                    if workspace.finish_save_prompt(authority) {
                        cx.notify();
                    }
                });
                return;
            };
            let _ = entity.update(cx, |workspace, cx| {
                if !workspace.finish_save_prompt(authority) {
                    workspace.rejected_stale_save_prompts =
                        workspace.rejected_stale_save_prompts.saturating_add(1);
                    cx.notify();
                    return;
                }
                let current = workspace
                    .session(authority.document_id, cx)
                    .is_some_and(|session| {
                        let session = session.read(cx);
                        session.generation == authority.document_generation
                            && matches!(session.status, NativeDocumentStatus::Ready)
                            && session.save_status != NativeDocumentSaveStatus::Saving
                    })
                    && (!authority.close_after_save
                        || workspace.pending_close_document_id == Some(authority.document_id));
                if !current {
                    workspace.rejected_stale_save_prompts =
                        workspace.rejected_stale_save_prompts.saturating_add(1);
                    cx.notify();
                    return;
                }
                if !path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
                {
                    let message = "Save As requires a .pdf file name.".to_owned();
                    if authority.close_after_save {
                        workspace
                            .annotation_statuses
                            .insert(authority.document_id, message);
                    } else {
                        workspace.record_document_save_failure(
                            authority.document_id,
                            DocumentSaveFailureOperation::SaveAs,
                            message,
                            cx,
                        );
                    }
                    cx.notify();
                    return;
                }
                let result = if authority.close_after_save {
                    workspace.dirty_close_save_as_path(path, cx)
                } else {
                    workspace.save_as_path(authority.document_id, path, cx)
                };
                if let Err(error) = result {
                    if authority.close_after_save {
                        workspace
                            .annotation_statuses
                            .insert(authority.document_id, error);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn finish_save_prompt(&mut self, authority: SavePromptAuthority) -> bool {
        if self.pending_save_prompt != Some(authority) {
            return false;
        }
        self.pending_save_prompt = None;
        true
    }

    pub fn save_as_path(
        &mut self,
        document_id: DocumentId,
        target_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let request = match self.begin_save_as(document_id, target_path, cx) {
            Ok(request) => request,
            Err(error) => {
                self.record_document_save_failure(
                    document_id,
                    DocumentSaveFailureOperation::SaveAs,
                    error.clone(),
                    cx,
                );
                return Err(error);
            }
        };
        self.dispatch_save_request(request, cx)
    }

    pub fn save_path(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let request = match self.begin_save(document_id, cx) {
            Ok(request) => request,
            Err(error) => {
                self.record_document_save_failure(
                    document_id,
                    DocumentSaveFailureOperation::InPlace,
                    error.clone(),
                    cx,
                );
                return Err(error);
            }
        };
        self.dispatch_save_request(request, cx)
    }

    pub fn dirty_close_save_as_path(
        &mut self,
        target_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let request = self.begin_dirty_close_save_as(target_path, cx)?;
        self.dispatch_save_request(request, cx)
    }

    pub fn dirty_close_save_path(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let request = self.begin_dirty_close_save(cx)?;
        self.dispatch_save_request(request, cx)
    }

    fn dispatch_save_request(
        &mut self,
        request: SaveDocumentRequest,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(saver) = self.saver.clone() else {
            self.apply_save_result(&request, Err("no document saver is configured".into()), cx);
            return Err("no document saver is configured".into());
        };
        let task = cx.background_executor().spawn(async move {
            let result = saver.save(&request);
            (request, result)
        });
        cx.spawn(async move |entity, cx| {
            let (request, result) = task.await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.apply_save_result(&request, result, cx);
            });
        })
        .detach();
        Ok(())
    }

    fn record_detached_release(
        &mut self,
        context: &'static str,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = result {
            self.last_file_error = Some(format!("Failed to release {context}: {error}"));
            cx.notify();
        }
    }

    pub fn apply_open_result(
        &mut self,
        request: &OpenDocumentRequest,
        result: Result<OpenedNativeDocument, String>,
        cx: &mut Context<Self>,
    ) -> ApplyDisposition {
        if self.pending_template_document_id == Some(request.document_id) {
            self.finish_template_creation(cx);
        }
        let Some(session) = self.session(request.document_id, cx).cloned() else {
            if let Ok(opened) = result {
                self.record_detached_release(
                    "closed-document open result",
                    opened.resource.close(),
                    cx,
                );
            }
            return ApplyDisposition::RejectedClosed;
        };
        let is_current = session.read(cx).generation == request.generation;
        if !is_current {
            if let Ok(opened) = result {
                self.record_detached_release("stale open result", opened.resource.close(), cx);
            }
            return ApplyDisposition::RejectedStale;
        }
        match result {
            Ok(opened) => {
                let source_sha256 = opened.source_sha256;
                let page_rotations = opened.page_rotations.clone();
                let page_coordinate_spaces = opened.page_coordinate_spaces.clone();
                self.next_annotation_sequence = self
                    .next_annotation_sequence
                    .max(next_workspace_annotation_sequence(&opened.annotations));
                if let Err(error) = session.update(cx, |session, cx| {
                    if opened.page_scales.is_empty() {
                        session
                            .annotations
                            .load_imported_annotations_with_document_state(
                                request.document_id.value(),
                                opened.annotations,
                                opened.page_length_calibrations,
                                page_rotations
                                    .iter()
                                    .copied()
                                    .enumerate()
                                    .map(|(page_index, rotation)| (page_index as u32, rotation))
                                    .collect(),
                            )?;
                    } else {
                        session
                            .annotations
                            .load_imported_annotations_with_page_scale_state(
                                request.document_id.value(),
                                opened.annotations,
                                opened
                                    .page_scales
                                    .into_iter()
                                    .map(|scale| (scale.page_index, scale))
                                    .collect(),
                                opened.scale_presets,
                                page_rotations
                                    .iter()
                                    .copied()
                                    .enumerate()
                                    .map(|(page_index, rotation)| (page_index as u32, rotation))
                                    .collect(),
                            )?;
                    }
                    let removed = session
                        .sync_image_assets()
                        .map_err(AnnotationError::InvalidGeometry)?;
                    defer_drop_images(removed, cx);
                    Ok::<(), AnnotationError>(())
                }) {
                    self.record_detached_release(
                        "annotation-import open result",
                        opened.resource.close(),
                        cx,
                    );
                    session.update(cx, |session, cx| {
                        session.status = NativeDocumentStatus::Failed(error.to_string());
                        cx.notify();
                    });
                    cx.notify();
                    return ApplyDisposition::Applied;
                }
                let current_base_raster = opened.current_page;
                let page_image = match current_base_raster.clone().into_render_image() {
                    Ok(image) => image,
                    Err(error) => {
                        self.record_detached_release(
                            "invalid-raster open result",
                            opened.resource.close(),
                            cx,
                        );
                        session.update(cx, |session, cx| {
                            session.status = NativeDocumentStatus::Failed(error);
                            cx.notify();
                        });
                        cx.notify();
                        return ApplyDisposition::Applied;
                    }
                };
                let thumbnails = opened
                    .thumbnails
                    .into_iter()
                    .filter_map(|thumbnail| {
                        let base_raster = thumbnail.raster;
                        base_raster.clone().into_render_image().ok().map(|image| {
                            ThumbnailPresentation {
                                page_index: thumbnail.page_index,
                                base_raster,
                                image,
                                highlight_pixels: 0,
                            }
                        })
                    })
                    .collect();
                session.update(cx, |session, cx| {
                    session.title = opened.title;
                    session.source_page_sizes = opened.page_sizes;
                    session.source_page_rotations = page_rotations;
                    session.source_page_coordinate_spaces = page_coordinate_spaces;
                    session.sync_rotation_geometry();
                    session.presentation_error = None;
                    session.current_page = 0;
                    session.requested_page = 0;
                    session.current_base_raster = Some(current_base_raster);
                    session.current_image = Some(page_image);
                    session.thumbnails = thumbnails;
                    session.resource = Some(opened.resource);
                    session.resource_epoch = session.resource_epoch.saturating_add(1);
                    session.source_sha256 = source_sha256;
                    session.status = NativeDocumentStatus::Ready;
                    if let Err(error) = session.rebuild_stable_highlight_presentations() {
                        session.status = NativeDocumentStatus::Failed(error);
                    }
                    cx.notify();
                });
                self.active_document_id = Some(request.document_id);
            }
            Err(error) => {
                session.update(cx, |session, cx| {
                    session.status = NativeDocumentStatus::Failed(error);
                    cx.notify();
                });
            }
        }
        self.sync_active_viewer_toolbar(cx);
        cx.notify();
        ApplyDisposition::Applied
    }

    fn apply_restart_view(
        &mut self,
        document_id: DocumentId,
        view: RestartView,
        restored_page: Option<Result<RasterSurface, String>>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return;
        }
        session.update(cx, |session, cx| {
            let page_index = view
                .current_page()
                .min(session.page_sizes.len().saturating_sub(1) as u32);
            let restored_page = if page_index == 0 {
                None
            } else {
                match restored_page {
                    Some(Ok(surface)) => match surface.clone().into_render_image() {
                        Ok(image) => Some((surface, image)),
                        Err(error) => {
                            session.presentation_error = Some(format!(
                                "Could not restore page {}: {error}",
                                page_index + 1
                            ));
                            cx.notify();
                            return;
                        }
                    },
                    Some(Err(error)) => {
                        session.presentation_error = Some(format!(
                            "Could not restore page {}: {error}",
                            page_index + 1
                        ));
                        cx.notify();
                        return;
                    }
                    None => {
                        session.presentation_error = Some(format!(
                            "Could not restore page {}: restored page raster was not prepared",
                            page_index + 1
                        ));
                        cx.notify();
                        return;
                    }
                }
            };
            session
                .view_state
                .apply_restart_view(view, session.page_sizes.len());
            if let Some((surface, image)) = restored_page {
                session.current_page = page_index;
                session.requested_page = page_index;
                session.current_base_raster = Some(surface);
                session.current_image = Some(image);
                session.viewer.invalidate_layout();
                if let Err(error) = session.rebuild_stable_highlight_presentations() {
                    session.presentation_error = Some(error);
                }
            }
            session
                .viewer
                .configure(session.view_state.mode(), session.view_state.zoom_percent());
            let (scroll_x, scroll_y) = session.view_state.scroll();
            session.viewer.set_scroll(scroll_x, scroll_y);
            cx.notify();
        });
        cx.notify();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn plan_viewport(
        &mut self,
        document_id: DocumentId,
        mode: PageViewMode,
        zoom_percent: f32,
        device_scale: f32,
        viewport_width: f32,
        viewport_height: f32,
        scroll_x: f32,
        scroll_y: f32,
        cx: &mut Context<Self>,
    ) -> Result<ViewerPlanEvidence, String> {
        if !viewport_width.is_finite()
            || !viewport_height.is_finite()
            || viewport_width <= 0.
            || viewport_height <= 0.
        {
            return Err("viewer viewport must have finite positive dimensions".into());
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        session.update(cx, |session, _| {
            if !matches!(session.status, NativeDocumentStatus::Ready) {
                return Err("document session is not ready".into());
            }
            session.viewer.configure(mode, zoom_percent);
            let cad_layout = session.view_state.cad_view_active().then(|| {
                (
                    match session.view_state.cad_organisation() {
                        CadViewOrganisation::Columns => PlannerCadOrganisation::Columns,
                        CadViewOrganisation::Rows => PlannerCadOrganisation::Rows,
                    },
                    session.view_state.pages_per_lane(),
                )
            });
            session.viewer.configure_cad(cad_layout);
            let rotations = session.page_rotation_quarter_turns();
            let plan = session.viewer.plan(
                document_id.value(),
                &session.page_sizes,
                &rotations,
                session.current_page as usize,
                viewport_width,
                viewport_height,
                scroll_x,
                scroll_y,
                device_scale,
            )?;
            let evidence = ViewerPlanEvidence {
                generation: plan.generation,
                page_layouts: plan.page_layouts,
                visible_pages: plan.visible_pages,
                current_page: plan.current_page,
                total_height: plan.total_height,
                total_width: plan.total_width,
                tiles: plan.tiles,
                requested_bytes: plan.requested_bytes,
                cache_max_bytes: plan.cache_max_bytes,
            };
            Ok(evidence)
        })
    }

    pub fn set_view_configuration(
        &mut self,
        document_id: DocumentId,
        mode: PageViewMode,
        zoom_percent: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        if session.read(cx).view_state.mode() == mode
            && (session.read(cx).view_state.zoom_percent() - zoom_percent).abs() < 0.001
        {
            return false;
        }
        session.update(cx, |session, cx| {
            session.view_state.set_mode(mode);
            if (session.view_state.zoom_percent() - zoom_percent).abs() >= 0.001 {
                session.view_state.set_manual_zoom(zoom_percent);
            }
            session
                .viewer
                .configure(session.view_state.mode(), session.view_state.zoom_percent());
            cx.notify();
        });
        cx.notify();
        true
    }

    fn active_ready_document_id(&self, cx: &App) -> Option<DocumentId> {
        let document_id = self.active_document_id?;
        self.session(document_id, cx)
            .is_some_and(|session| matches!(session.read(cx).status, NativeDocumentStatus::Ready))
            .then_some(document_id)
    }

    pub fn document_command_state(&self, cx: &App) -> DocumentCommandState {
        let Some(document_id) = self.active_document_id else {
            return DocumentCommandState::default();
        };
        let Some(session) = self.session(document_id, cx) else {
            return DocumentCommandState::default();
        };
        let session = session.read(cx);
        let document_ready = matches!(session.status, NativeDocumentStatus::Ready);
        let save_busy = session.save_status == NativeDocumentSaveStatus::Saving
            || self.pending_save_prompt.is_some_and(|pending| {
                pending.document_id == document_id
                    && pending.document_generation == session.generation
            });
        let rotation_busy = session.pending_rotation_generation.is_some();
        let mut state = DocumentCommandState {
            can_close_document: true,
            document_ready,
            save_busy,
            rotation_busy,
            ..Default::default()
        };
        if !document_ready {
            return state;
        }

        let view = &session.view_state;
        let zoom = f64::from(view.zoom_percent()) / 100.;
        state.can_previous_page = session.current_page > 0;
        state.can_next_page = usize::try_from(session.current_page)
            .ok()
            .is_some_and(|page| page + 1 < session.page_sizes.len());
        state.can_zoom_out = zoom > MIN_VIEWER_ZOOM + f64::EPSILON;
        state.can_zoom_in = zoom < MAX_VIEWER_ZOOM - f64::EPSILON;
        state.actual_size_checked = view.zoom_preset() == ViewerZoomPreset::Manual
            && (zoom - DEFAULT_VIEWER_ZOOM).abs() < 0.000_001;
        state.fit_width_checked = view.zoom_preset() == ViewerZoomPreset::FitWidth;
        state.fit_page_checked = view.zoom_preset() == ViewerZoomPreset::FitPage;
        state.continuous_view_checked = view.mode() == PageViewMode::Continuous;
        state.single_page_view_checked = view.mode() == PageViewMode::SinglePage;
        state
    }

    fn close_active_document_from_action(
        &mut self,
        _: &CloseDocument,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(document_id) = self.active_document_id {
            self.request_close_document(document_id, cx);
        }
    }

    fn rotate_active_page_from_action(
        &mut self,
        direction: PageRotationDirection,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        if self.document_command_state(cx).save_busy
            || self.document_command_state(cx).rotation_busy
        {
            return;
        }
        self.rotate_page_async(document_id, direction, cx);
    }

    fn set_active_manual_zoom(&mut self, zoom: f64, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        let Some(mode) = self
            .document_view_state(document_id, cx)
            .map(|state| state.mode())
        else {
            return;
        };
        self.set_view_configuration(document_id, mode, (zoom * 100.) as f32, cx);
        self.sync_active_viewer_toolbar(cx);
    }

    fn zoom_active_document(&mut self, factor: f64, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        let Some(zoom) = self
            .document_view_state(document_id, cx)
            .map(|state| f64::from(state.zoom_percent()) / 100.)
        else {
            return;
        };
        self.set_active_manual_zoom(zoom * factor, cx);
    }

    fn set_active_fit_preset(&mut self, preset: ViewerFitPreset, cx: &mut Context<Self>) {
        if let Some(document_id) = self.active_ready_document_id(cx) {
            self.set_fit_preset(document_id, preset, cx);
            self.sync_active_viewer_toolbar(cx);
        }
    }

    fn set_active_page_view_mode(&mut self, mode: PageViewMode, cx: &mut Context<Self>) {
        if let Some(document_id) = self.active_ready_document_id(cx) {
            self.set_page_view_mode(document_id, mode, cx);
            self.sync_active_viewer_toolbar(cx);
        }
    }

    fn handle_page_view_control_event(
        &mut self,
        event: PageViewControlEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        match event {
            PageViewControlEvent::Activated(mode) => {
                self.set_page_view_mode(document_id, mode, cx);
            }
            PageViewControlEvent::FitActivated(mode) => {
                self.set_page_view_mode(document_id, mode, cx);
                let preset = match mode {
                    PageViewMode::Continuous => ViewerFitPreset::Width,
                    PageViewMode::SinglePage => ViewerFitPreset::Page,
                };
                self.set_fit_preset(document_id, preset, cx);
            }
            PageViewControlEvent::WheelBehaviorChanged(mode, behavior) => {
                self.set_document_wheel_behavior(document_id, mode, behavior, cx);
            }
        }
    }

    fn handle_zoom_control_event(&mut self, event: ZoomControlEvent, cx: &mut Context<Self>) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        let Some(mode) = self
            .document_view_state(document_id, cx)
            .map(|state| state.mode())
        else {
            return;
        };
        let ZoomControlEvent::Changed(zoom) = event;
        self.set_view_configuration(document_id, mode, (zoom * 100.) as f32, cx);
    }

    fn handle_cad_view_control_event(
        &mut self,
        event: CadViewControlEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        session.update(cx, |session, cx| {
            match event {
                CadViewControlEvent::Activated => session.view_state.activate_cad_view(),
                CadViewControlEvent::OrganisationChanged(organisation) => {
                    session.view_state.set_cad_organisation(match organisation {
                        ControlCadOrganisation::Columns => CadViewOrganisation::Columns,
                        ControlCadOrganisation::Rows => CadViewOrganisation::Rows,
                    });
                }
                CadViewControlEvent::PagesPerLaneChanged(count) => {
                    session.view_state.set_pages_per_lane(count as f64);
                }
            }
            let layout = session.view_state.cad_view_active().then(|| {
                (
                    match session.view_state.cad_organisation() {
                        CadViewOrganisation::Columns => PlannerCadOrganisation::Columns,
                        CadViewOrganisation::Rows => PlannerCadOrganisation::Rows,
                    },
                    session.view_state.pages_per_lane(),
                )
            });
            session
                .viewer
                .configure(session.view_state.mode(), session.view_state.zoom_percent());
            session.viewer.configure_cad(layout);
            cx.notify();
        });
        self.sync_active_viewer_toolbar(cx);
        cx.notify();
    }

    fn handle_viewer_toolbar_event(
        &mut self,
        event: ViewerToolbarStripEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.active_ready_document_id(cx) else {
            return;
        };
        let ViewerToolbarStripEvent::FitPresetChanged(preset) = event;
        self.set_fit_preset(
            document_id,
            match preset {
                FitPreset::Width => ViewerFitPreset::Width,
                FitPreset::Page => ViewerFitPreset::Page,
            },
            cx,
        );
    }

    fn sync_active_viewer_toolbar(&mut self, cx: &mut Context<Self>) {
        let state = self
            .active_ready_document_id(cx)
            .and_then(|document_id| self.document_view_state(document_id, cx));
        self.viewer_toolbar.update(cx, |toolbar, cx| {
            let Some(state) = state else {
                toolbar.set_disabled(true, cx);
                return;
            };
            let fit_preset = match state.zoom_preset() {
                ViewerZoomPreset::Manual => None,
                ViewerZoomPreset::FitWidth => Some(FitPreset::Width),
                ViewerZoomPreset::FitPage => Some(FitPreset::Page),
            };
            toolbar.sync_document_state(
                state.mode(),
                fit_preset,
                state.wheel_behavior(PageViewMode::Continuous),
                state.wheel_behavior(PageViewMode::SinglePage),
                f64::from(state.zoom_percent()) / 100.,
                false,
                cx,
            );
            toolbar.sync_cad_document_state(
                state.cad_view_active(),
                match state.cad_organisation() {
                    CadViewOrganisation::Columns => ControlCadOrganisation::Columns,
                    CadViewOrganisation::Rows => ControlCadOrganisation::Rows,
                },
                state.pages_per_lane(),
                cx,
            );
        });
    }

    fn observe_viewer_session(
        &mut self,
        session: &Entity<NativeDocumentSession>,
        cx: &mut Context<Self>,
    ) {
        let document_id = session.read(cx).id;
        let subscription = cx.observe(session, move |workspace, _, cx| {
            if workspace.active_document_id == Some(document_id) {
                workspace.sync_active_viewer_toolbar(cx);
            }
        });
        self.viewer_session_subscriptions
            .insert(document_id, subscription);
    }

    pub fn document_view_state(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<NativeDocumentViewState> {
        self.session(document_id, cx)
            .map(|session| session.read(cx).view_state.clone())
    }

    pub fn set_page_view_mode(
        &mut self,
        document_id: DocumentId,
        mode: PageViewMode,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        if session.read(cx).view_state.mode() == mode {
            return false;
        }
        session.update(cx, |session, cx| {
            session.view_state.set_mode(mode);
            session
                .viewer
                .configure(mode, session.view_state.zoom_percent());
            cx.notify();
        });
        cx.notify();
        true
    }

    pub fn set_fit_preset(
        &mut self,
        document_id: DocumentId,
        preset: ViewerFitPreset,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        session.update(cx, |session, cx| {
            session.view_state.set_fit_preset(preset);
            if let Some((width, height)) = session.view_state.viewport_size()
                && let Some(page_size) = session
                    .page_sizes
                    .get(session.current_page as usize)
                    .copied()
            {
                session.view_state.update_viewport(width, height, page_size);
            }
            session
                .viewer
                .configure(session.view_state.mode(), session.view_state.zoom_percent());
            cx.notify();
        });
        cx.notify();
        true
    }

    pub fn set_document_wheel_behavior(
        &mut self,
        document_id: DocumentId,
        mode: PageViewMode,
        behavior: crate::page_view_control::WheelBehavior,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        if session.read(cx).view_state.wheel_behavior(mode) == behavior {
            return false;
        }
        session.update(cx, |session, cx| {
            session.view_state.set_wheel_behavior(mode, behavior);
            cx.notify();
        });
        cx.notify();
        true
    }

    pub fn viewer_snapshot(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<DocumentViewerSnapshot> {
        self.session(document_id, cx)
            .map(|session| session.read(cx).viewer.snapshot())
    }

    pub fn adaptive_performance_snapshot(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<AdaptivePerformanceSnapshot> {
        self.session(document_id, cx)
            .map(|session| session.read(cx).adaptive_performance.current())
    }

    pub fn observe_viewer_frame_at(
        &mut self,
        document_id: DocumentId,
        frame_at: Instant,
        cx: &mut Context<Self>,
    ) -> Option<AdaptivePerformanceSnapshot> {
        let session = self.session(document_id, cx).cloned()?;
        let (snapshot, level_changed) = session.update(cx, |session, _| {
            session.adaptive_performance.observe_frame(frame_at);
            let should_evaluate = session.adaptive_last_evaluated_at.is_some_and(|previous| {
                frame_at.saturating_duration_since(previous) >= Duration::from_millis(250)
            });
            if session.adaptive_last_evaluated_at.is_none() {
                session.adaptive_last_evaluated_at = Some(frame_at);
            }
            if !should_evaluate {
                return (session.adaptive_performance.current(), false);
            }
            session.adaptive_last_evaluated_at = Some(frame_at);
            let viewer = session.viewer.snapshot();
            let snapshot = session
                .adaptive_performance
                .evaluate(ViewerRenderDiagnostics {
                    queued_page_renders: viewer.queued_tiles,
                    queued_thumbnail_renders: 0,
                    inflight_page_renders: viewer.active_tiles,
                    inflight_thumbnail_renders: 0,
                });
            let changed = session.viewer.set_adaptive_level(snapshot.level, frame_at);
            (snapshot, changed)
        });
        if level_changed {
            self.arm_viewer_quality_timer(document_id, cx);
            cx.notify();
        }
        Some(snapshot)
    }

    fn observe_viewer_input_at(
        &mut self,
        document_id: DocumentId,
        input_at: Instant,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.session(document_id, cx).cloned() {
            session.update(cx, |session, _| {
                session.adaptive_performance.observe_input(input_at);
            });
        }
    }

    pub fn painted_page_evidence(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> Option<PaintedPageEvidence> {
        let interaction = self.page_interactions.get(&(document_id, page_index))?;
        let evidence = interaction.painted_evidence?;
        if self.active_document_id != Some(document_id)
            || interaction.document_id != document_id
            || interaction.page_index != page_index
            || interaction.bounds != evidence.contained_bounds
            || evidence.document_id != document_id
            || evidence.page_index != page_index
        {
            return None;
        }
        let session = self.session(document_id, cx)?.read(cx);
        if !matches!(session.status, NativeDocumentStatus::Ready) {
            return None;
        }
        if session.generation != evidence.request_generation
            || session.resource_epoch != evidence.resource_generation
        {
            return None;
        }
        let plan = session.viewer.plan_snapshot()?;
        if plan.generation != evidence.viewer_generation
            || !plan.visible_pages.contains(&(page_index as usize))
            || session.annotation_page_geometry(page_index)?.0
                != evidence.source_pdf_page_size_points
        {
            return None;
        }
        let mut requests = plan
            .tiles
            .iter()
            .filter(|request| request.page == page_index as usize);
        let first = requests.next()?;
        let expected_tile_count = plan
            .tiles
            .iter()
            .filter(|request| request.page == page_index as usize)
            .count();
        let painted_tiles = session.viewer.visible_tiles(page_index as usize);
        if first.generation != evidence.viewer_generation
            || painted_tiles.len() != expected_tile_count
            || (first.device_scale_millis as f32 / 1_000. - evidence.rendered_dpr).abs() >= 0.001
            || requests.any(|request| {
                request.generation != evidence.viewer_generation
                    || request.device_scale_millis != first.device_scale_millis
            })
        {
            return None;
        }
        Some(evidence)
    }

    pub fn evidence_snapshot(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<DocumentWorkspaceEvidenceSnapshot> {
        let session = self.session(document_id, cx)?.read(cx);
        let viewer = session.viewer.snapshot();
        let rendered_device_pixel_ratio = session.viewer.plan_snapshot().and_then(|plan| {
            let first = plan.tiles.first()?;
            let rendered_tile_count = plan
                .visible_pages
                .iter()
                .map(|page| session.viewer.visible_tiles(*page).len())
                .sum::<usize>();
            (first.generation == plan.generation
                && rendered_tile_count == plan.tiles.len()
                && plan.tiles.iter().all(|request| {
                    request.generation == plan.generation
                        && request.device_scale_millis == first.device_scale_millis
                }))
            .then_some(first.device_scale_millis as f32 / 1_000.)
        });
        let annotations = session.annotations.snapshot(document_id.value());
        let (current_raster_width, current_raster_height, current_raster_bytes, variation) =
            session
                .current_base_raster
                .as_ref()
                .map_or((0, 0, 0, false), |raster| {
                    (
                        raster.width(),
                        raster.height(),
                        raster.pixels_bgra().len(),
                        raster.has_spatial_variation(),
                    )
                });
        Some(DocumentWorkspaceEvidenceSnapshot {
            document_id,
            request_generation: session.generation,
            ready: matches!(session.status, NativeDocumentStatus::Ready),
            failure: match &session.status {
                NativeDocumentStatus::Failed(error) => Some(error.clone()),
                NativeDocumentStatus::Opening | NativeDocumentStatus::Ready => None,
            },
            current_page: session.current_page,
            requested_page: session.requested_page,
            page_count: session.page_sizes.len(),
            current_raster_width,
            current_raster_height,
            current_raster_bytes,
            current_raster_has_spatial_variation: variation,
            thumbnail_count: session.thumbnails.len(),
            worker_pid: session.worker_pid(),
            resource_present: session.resource.is_some(),
            viewer_generation: viewer.generation,
            viewer_tile_count: viewer
                .queued_tiles
                .saturating_add(viewer.active_tiles)
                .saturating_add(viewer.cache_entries),
            viewer_cache_bytes: viewer.cache_bytes,
            rendered_device_pixel_ratio,
            annotation_revision: annotations.as_ref().map_or(0, |snapshot| snapshot.revision),
            annotation_dirty: session.is_dirty(),
            presentation_error: session.presentation_error.clone(),
            recovery_pending: session.recovery_generation,
        })
    }

    pub fn fit_zoom_percent(
        &self,
        document_id: DocumentId,
        preset: ViewerFitPreset,
        viewport_width: f32,
        viewport_height: f32,
        cx: &App,
    ) -> Option<f32> {
        let session = self.session(document_id, cx)?.read(cx);
        let (page_width, page_height) = *session.page_sizes.get(session.current_page as usize)?;
        Some(resolve_fit_zoom_percent(
            preset,
            viewport_width,
            viewport_height,
            page_width,
            page_height,
        ))
    }

    pub fn observed_fit_zoom_percent(
        &self,
        document_id: DocumentId,
        preset: ViewerFitPreset,
        cx: &App,
    ) -> Option<f32> {
        let session = self.session(document_id, cx)?.read(cx);
        let (viewport_width, viewport_height) = session.viewport_size?;
        let (page_width, page_height) = *session.page_sizes.get(session.current_page as usize)?;
        Some(resolve_fit_zoom_percent(
            preset,
            viewport_width,
            viewport_height,
            page_width,
            page_height,
        ))
    }

    pub fn set_viewport_scroll(
        &mut self,
        document_id: DocumentId,
        scroll_x: f32,
        scroll_y: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        session.update(cx, |session, _| {
            session.view_state.set_scroll(scroll_x, scroll_y);
            session.viewer.set_scroll(scroll_x, scroll_y);
        });
        cx.notify();
        true
    }

    fn handle_viewport_wheel(
        &mut self,
        document_id: DocumentId,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let input_at = cx.background_executor().now();
        self.observe_viewer_input_at(document_id, input_at, cx);
        let delta = event.delta.pixel_delta(px(16.));
        let old_zoom = session.read(cx).view_state.zoom_percent();
        let outcome = session.update(cx, |session, _| {
            session.view_state.wheel(
                session.page_sizes.len(),
                session.current_page as usize,
                f32::from(delta.x),
                f32::from(delta.y),
                event.modifiers.control,
            )
        });
        match outcome {
            WheelOutcome::NativeScroll => false,
            WheelOutcome::Consumed => true,
            WheelOutcome::Page(target) => {
                if let Ok(target) = u32::try_from(target) {
                    self.navigate_async(document_id, target, cx);
                }
                true
            }
            WheelOutcome::Zoom(zoom_percent) => {
                let bounds = self.viewport_bounds.get(&document_id).copied();
                session.update(cx, |session, cx| {
                    let offset = session.viewer.scroll_handle().offset();
                    let old_scroll = (
                        (-f32::from(offset.x)).max(0.),
                        (-f32::from(offset.y)).max(0.),
                    );
                    if let Some(bounds) = bounds {
                        let local_x = f32::from(event.position.x - bounds.origin.x);
                        let local_y = f32::from(event.position.y - bounds.origin.y);
                        let ratio = zoom_percent / old_zoom.max(0.001);
                        let scroll_x = (old_scroll.0 + local_x) * ratio - local_x;
                        let scroll_y = (old_scroll.1 + local_y) * ratio - local_y;
                        session.view_state.set_scroll(scroll_x, scroll_y);
                        session.viewer.set_scroll(scroll_x, scroll_y);
                    }
                    session
                        .viewer
                        .configure(session.view_state.mode(), zoom_percent);
                    cx.notify();
                });
                if let Some((width, height)) = session.read(cx).view_state.viewport_size() {
                    let _ = self.refresh_viewport_async(
                        document_id,
                        width,
                        height,
                        window.scale_factor(),
                        cx,
                    );
                }
                cx.notify();
                true
            }
        }
    }

    fn apply_document_navigation(
        &mut self,
        action: DocumentNavigationAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_text_box_editor.is_some() || self.pending_close_document_id.is_some() {
            return;
        }
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let outcome = {
            let session = session.read(cx);
            let viewport_height = session.view_state.viewport_size().map_or(0., |size| size.1);
            session.view_state.keyboard(
                action,
                session.page_sizes.len(),
                session.current_page as usize,
                viewport_height,
            )
        };
        match outcome {
            DocumentNavigationOutcome::None => return,
            DocumentNavigationOutcome::Page(target) => {
                if matches!(
                    action,
                    DocumentNavigationAction::Home | DocumentNavigationAction::End
                ) {
                    let y = if action == DocumentNavigationAction::Home {
                        0.
                    } else {
                        session
                            .read(cx)
                            .viewer
                            .plan_snapshot()
                            .map_or(0., |plan| plan.total_height)
                    };
                    session.update(cx, |session, _| {
                        session.view_state.set_scroll(0., y);
                        session.viewer.set_scroll(0., y);
                    });
                }
                if target != session.read(cx).current_page as usize
                    && let Ok(target) = u32::try_from(target)
                {
                    self.navigate_async(document_id, target, cx);
                }
            }
            DocumentNavigationOutcome::Scroll { x, y } => {
                session.update(cx, |session, _| {
                    let offset = session.viewer.scroll_handle().offset();
                    let scroll_x = (-f32::from(offset.x) + x).max(0.);
                    let scroll_y = (-f32::from(offset.y) + y).max(0.);
                    session.view_state.set_scroll(scroll_x, scroll_y);
                    session.viewer.set_scroll(scroll_x, scroll_y);
                });
            }
        }
        if let Some((width, height)) = session.read(cx).view_state.viewport_size() {
            let _ =
                self.refresh_viewport_async(document_id, width, height, window.scale_factor(), cx);
        }
        cx.notify();
    }

    pub fn refresh_viewport_async(
        &mut self,
        document_id: DocumentId,
        viewport_width: f32,
        viewport_height: f32,
        device_scale: f32,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let scroll = session.read(cx).viewer.scroll_handle().offset();
        let scroll_x = (-f32::from(scroll.x)).max(0.);
        let scroll_y = (-f32::from(scroll.y)).max(0.);
        let now = cx.background_executor().now();
        session.update(cx, |session, _| {
            if !matches!(session.status, NativeDocumentStatus::Ready) {
                return Err("document session is not ready".into());
            }
            let rotations = session.page_rotation_quarter_turns();
            let cad_layout = session.view_state.cad_view_active().then(|| {
                (
                    match session.view_state.cad_organisation() {
                        CadViewOrganisation::Columns => PlannerCadOrganisation::Columns,
                        CadViewOrganisation::Rows => PlannerCadOrganisation::Rows,
                    },
                    session.view_state.pages_per_lane(),
                )
            });
            session.viewer.configure_cad(cad_layout);
            session.view_state.set_scroll(scroll_x, scroll_y);
            session.viewer.observe_motion(scroll_x, scroll_y, now);
            let plan = session.viewer.plan_at(
                document_id.value(),
                &session.page_sizes,
                &rotations,
                session.current_page as usize,
                viewport_width,
                viewport_height,
                scroll_x,
                scroll_y,
                device_scale,
                now,
            )?;
            if session.view_state.mode() == PageViewMode::Continuous
                && let Some(visible_page) = plan.current_page
                && visible_page != session.current_page as usize
            {
                session.current_page = u32::try_from(visible_page)
                    .map_err(|_| "visible page index overflowed".to_owned())?;
                if session.view_state.zoom_preset() != ViewerZoomPreset::Manual
                    && let Some(page_size) = session.page_sizes.get(visible_page).copied()
                {
                    session
                        .view_state
                        .update_viewport(viewport_width, viewport_height, page_size);
                    session
                        .viewer
                        .configure(session.view_state.mode(), session.view_state.zoom_percent());
                    session.viewer.plan_at(
                        document_id.value(),
                        &session.page_sizes,
                        &rotations,
                        visible_page,
                        viewport_width,
                        viewport_height,
                        scroll_x,
                        scroll_y,
                        device_scale,
                        now,
                    )?;
                }
            }
            Ok::<(), String>(())
        })?;
        self.dispatch_viewer_jobs(document_id, cx);
        self.arm_viewer_quality_timer(document_id, cx);
        cx.notify();
        Ok(())
    }

    fn observe_viewport(
        &mut self,
        document_id: DocumentId,
        viewport_width: f32,
        viewport_height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let frame_at = cx.background_executor().now();
        self.observe_viewer_frame_at(document_id, frame_at, cx);
        if let Some(session) = self.session(document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                let next = Some((viewport_width, viewport_height));
                if session.viewport_size != next {
                    session.viewport_size = next;
                    if let Some(page_size) = session
                        .page_sizes
                        .get(session.current_page as usize)
                        .copied()
                    {
                        session.view_state.update_viewport(
                            viewport_width,
                            viewport_height,
                            page_size,
                        );
                        session.viewer.configure(
                            session.view_state.mode(),
                            session.view_state.zoom_percent(),
                        );
                    }
                    cx.notify();
                }
            });
        }
        let needs_plan = self.session(document_id, cx).is_some_and(|session| {
            let session = session.read(cx);
            matches!(session.status, NativeDocumentStatus::Ready)
                && session.viewer.needs_plan(
                    viewport_width,
                    viewport_height,
                    window.scale_factor(),
                    session.current_page as usize,
                )
        });
        if !needs_plan || self.viewport_refresh_scheduled == Some(document_id) {
            return;
        }
        self.viewport_refresh_scheduled = Some(document_id);
        cx.on_next_frame(window, move |workspace, window, cx| {
            if workspace.viewport_refresh_scheduled != Some(document_id) {
                return;
            }
            workspace.viewport_refresh_scheduled = None;
            let _ = workspace.refresh_viewport_async(
                document_id,
                viewport_width,
                viewport_height,
                window.scale_factor(),
                cx,
            );
        });
    }

    fn dispatch_viewer_jobs(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let (resource, page_sizes, coordinate_spaces, pens) = {
            let session = session.read(cx);
            let Some(resource) = session.resource.clone() else {
                return;
            };
            let pens = session
                .annotations
                .snapshot(document_id.value())
                .map(|snapshot| snapshot.pens)
                .unwrap_or_default();
            let coordinate_spaces = (0..session.page_sizes.len())
                .map(|page_index| session.annotation_page_coordinate_space(page_index as u32))
                .collect::<Option<Vec<_>>>();
            let Some(coordinate_spaces) = coordinate_spaces else {
                return;
            };
            (
                resource,
                session.page_sizes.clone(),
                coordinate_spaces,
                pens,
            )
        };
        let jobs = session.update(cx, |session, _| session.viewer.claim_jobs());
        for job in jobs {
            let resource = resource.clone();
            let page_sizes = page_sizes.clone();
            let coordinate_spaces = coordinate_spaces.clone();
            let pens = pens.clone();
            let request = job.raster;
            let task = cx.background_executor().spawn(async move {
                let mut raster = resource.render_tile(request)?;
                let page_size = *page_sizes
                    .get(request.page)
                    .ok_or_else(|| "viewer tile page geometry is unavailable".to_owned())?;
                let zoom = f64::from(request.zoom_tenths) / 1_000.;
                let device_scale = f64::from(request.device_scale_millis) / 1_000.;
                let scale = zoom * device_scale;
                let highlight_pixels = raster.precompose_highlights(
                    u32::try_from(request.page)
                        .map_err(|_| "viewer tile page index overflowed".to_owned())?,
                    coordinate_spaces[request.page],
                    request.crop.x as f64,
                    request.crop.y as f64,
                    (
                        f64::from(page_size.0) * scale,
                        f64::from(page_size.1) * scale,
                    ),
                    &pens,
                )?;
                let bytes = raster.pixels_bgra().len();
                let image = raster.into_render_image()?;
                Ok::<_, String>((image, bytes, highlight_pixels))
            });
            cx.spawn(async move |entity, cx| {
                let result = task.await;
                let _ = entity.update(cx, |workspace, cx| {
                    workspace.finish_viewer_job(document_id, job, result, cx);
                });
            })
            .detach();
        }
    }

    fn arm_viewer_quality_timer(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        self.viewer_quality_tasks.remove(&document_id);
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let Some((deadline, revision)) = session.read_with(cx, |session, _| {
            session
                .viewer
                .next_promotion_deadline()
                .map(|deadline| (deadline, session.viewer.scheduler_revision()))
        }) else {
            return;
        };
        let executor = cx.background_executor().clone();
        let delay = deadline.saturating_duration_since(executor.now());
        let task = cx.spawn(async move |entity, cx| {
            executor.timer(delay).await;
            let _ = entity.update(cx, |workspace, cx| {
                let Some(session) = workspace.session(document_id, cx).cloned() else {
                    workspace.viewer_quality_tasks.remove(&document_id);
                    return;
                };
                let current_revision =
                    session.read_with(cx, |session, _| session.viewer.scheduler_revision());
                if current_revision != revision {
                    workspace.arm_viewer_quality_timer(document_id, cx);
                    return;
                }
                let now = cx.background_executor().now();
                session.update(cx, |session, cx| {
                    if session.viewer.release_due_promotions(now) > 0 {
                        cx.notify();
                    }
                });
                workspace.dispatch_viewer_jobs(document_id, cx);
                workspace.arm_viewer_quality_timer(document_id, cx);
                cx.notify();
            });
        });
        self.viewer_quality_tasks.insert(document_id, task);
    }

    fn finish_viewer_job(
        &mut self,
        document_id: DocumentId,
        job: ViewerTileJob,
        result: Result<(Arc<RenderImage>, usize, usize), String>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        session.update(cx, |session, cx| {
            let result =
                result.map(|(image, bytes, highlight_pixels)| (image, bytes, highlight_pixels));
            let accepted = session.viewer.finish_at(
                job,
                result
                    .as_ref()
                    .map(|(image, bytes, _)| (image.clone(), *bytes))
                    .map_err(Clone::clone),
                cx.background_executor().now(),
            );
            if accepted && let Ok((_, _, highlight_pixels)) = result {
                session.highlight_composite.viewer_tile_pixels = session
                    .highlight_composite
                    .viewer_tile_pixels
                    .saturating_add(highlight_pixels);
            }
            cx.notify();
        });
        self.dispatch_viewer_jobs(document_id, cx);
        self.arm_viewer_quality_timer(document_id, cx);
        cx.notify();
    }

    pub fn retry_viewer_page(
        &mut self,
        document_id: DocumentId,
        page_index: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let queued = session.update(cx, |session, cx| {
            let queued = session.viewer.retry_page(page_index);
            if queued {
                cx.notify();
            }
            queued
        });
        if queued {
            self.dispatch_viewer_jobs(document_id, cx);
            cx.notify();
        }
        queued
    }

    pub fn render_planned_tiles_for_evidence(
        &mut self,
        document_id: DocumentId,
        plan: &ViewerPlanEvidence,
        cx: &mut Context<Self>,
    ) -> Result<TileRenderEvidence, String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let resource = session
            .read(cx)
            .resource
            .clone()
            .ok_or_else(|| "document resource is unavailable".to_owned())?;
        session.update(cx, |session, _| {
            if !session.viewer.accepts(plan.generation) {
                return Err("viewer plan generation is stale".into());
            }
            if plan
                .tiles
                .iter()
                .any(|tile| tile.source.document_id != document_id.value())
            {
                return Err("viewer plan belongs to another document".into());
            }
            let mut rendered_tiles = 0;
            let mut cache_hits = 0;
            let mut non_uniform_tiles = 0;
            for request in &plan.tiles {
                if session.viewer.touch_cached(*request) {
                    cache_hits += 1;
                    continue;
                }
                let mut raster = resource.render_tile(*request)?;
                let page_size = session.page_sizes[request.page];
                let coordinate_space = session
                    .annotation_page_coordinate_space(request.page as u32)
                    .ok_or_else(|| "viewer tile coordinate space is unavailable".to_owned())?;
                let scale = f64::from(request.zoom_tenths) / 1_000.
                    * f64::from(request.device_scale_millis)
                    / 1_000.;
                let pens = session
                    .annotations
                    .snapshot(document_id.value())
                    .map(|snapshot| snapshot.pens)
                    .unwrap_or_default();
                let highlight_pixels = raster.precompose_highlights(
                    u32::try_from(request.page)
                        .map_err(|_| "viewer tile page index overflowed".to_owned())?,
                    coordinate_space,
                    request.crop.x as f64,
                    request.crop.y as f64,
                    (
                        f64::from(page_size.0) * scale,
                        f64::from(page_size.1) * scale,
                    ),
                    &pens,
                )?;
                if raster.has_spatial_variation() {
                    non_uniform_tiles += 1;
                }
                let bytes = raster.pixels_bgra().len();
                let image = raster.into_render_image()?;
                if !session.viewer.insert_direct(*request, image, bytes) {
                    return Err("viewer tile exceeds the byte-accounted cache limit".into());
                }
                rendered_tiles += 1;
                session.highlight_composite.viewer_tile_pixels = session
                    .highlight_composite
                    .viewer_tile_pixels
                    .saturating_add(highlight_pixels);
            }
            let evidence = TileRenderEvidence {
                generation: plan.generation,
                rendered_tiles,
                cache_hits,
                cache_entries: session.viewer.cache_len(),
                cache_bytes: session.viewer.cache_bytes(),
                non_uniform_tiles,
            };
            Ok(evidence)
        })
    }

    pub fn begin_page_navigation(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Result<PageRenderRequest, String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("requested page is outside the document".into());
        }
        if session.read(cx).pending_rotation_generation.is_some() {
            return Err("page rotation pixels are still pending".into());
        }
        let source_rotation = session.read(cx).source_page_rotations[page_index as usize];
        let target_rotation = session
            .read(cx)
            .annotations
            .document_page_rotation(document_id.value(), page_index)
            .unwrap_or(source_rotation);
        let generation = self.next_generation();
        session.update(cx, |session, cx| {
            session.generation = generation;
            session.requested_page = page_index;
            cx.notify();
        });
        Ok(PageRenderRequest {
            document_id,
            generation,
            page_index,
            source_rotation,
            target_rotation,
        })
    }

    pub fn render_page_request_for_evidence(
        &self,
        request: &PageRenderRequest,
        cx: &App,
    ) -> Result<RasterSurface, String> {
        let resource = self
            .session(request.document_id, cx)
            .and_then(|session| session.read(cx).resource.clone())
            .ok_or_else(|| "document resource is unavailable".to_owned())?;
        resource
            .render_page(request.page_index, DEFAULT_PAGE_RENDER_WIDTH)
            .and_then(|surface| {
                surface.rotated(request.target_rotation.delta_from(request.source_rotation))
            })
    }

    pub fn begin_page_rotation(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        direction: PageRotationDirection,
        cx: &mut Context<Self>,
    ) -> Result<PageRotationRequest, String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        let source_rotation = *session
            .read(cx)
            .source_page_rotations
            .get(page_index as usize)
            .ok_or_else(|| "page rotation target is outside the document".to_owned())?;
        let generation = self.next_generation();
        let target_rotation = session.update(cx, |session, cx| {
            let rotation = session
                .annotations
                .rotate_document_page(document_id.value(), page_index, direction)
                .map_err(|error| error.to_string())?;
            session.generation = generation;
            session.pending_rotation_generation = Some(generation);
            session.requested_page = page_index;
            session.presentation_error = None;
            session.sync_rotation_geometry();
            cx.notify();
            Ok::<_, String>(rotation)
        })?;
        let (document_revision, resource_epoch) = {
            let session = session.read(cx);
            (
                session
                    .annotations
                    .snapshot(document_id.value())
                    .map(|snapshot| snapshot.revision)
                    .unwrap_or(0),
                session.resource_epoch,
            )
        };
        cx.notify();
        Ok(PageRotationRequest {
            document_id,
            generation,
            page_index,
            source_rotation,
            target_rotation,
            document_revision,
            resource_epoch,
        })
    }

    pub fn render_page_rotation_for_evidence(
        &self,
        request: &PageRotationRequest,
        cx: &App,
    ) -> Result<PageRotationPresentation, String> {
        let session = self
            .session(request.document_id, cx)
            .ok_or_else(|| "document session is closed".to_owned())?
            .read(cx);
        let resource = session
            .resource
            .as_ref()
            .ok_or_else(|| "document resource is unavailable".to_owned())?;
        let delta = request.target_rotation.delta_from(request.source_rotation);
        Ok(PageRotationPresentation {
            current_page: resource
                .render_page(request.page_index, DEFAULT_PAGE_RENDER_WIDTH)?
                .rotated(delta)?,
            thumbnail: resource
                .render_page(request.page_index, DEFAULT_THUMBNAIL_WIDTH)?
                .rotated(delta)?,
        })
    }

    fn rotate_page_async(
        &mut self,
        document_id: DocumentId,
        direction: PageRotationDirection,
        cx: &mut Context<Self>,
    ) {
        let Some(page_index) = self
            .session(document_id, cx)
            .map(|session| session.read(cx).current_page)
        else {
            return;
        };
        let Ok(request) = self.begin_page_rotation(document_id, page_index, direction, cx) else {
            return;
        };
        let Some(resource) = self
            .session(document_id, cx)
            .and_then(|session| session.read(cx).resource.clone())
        else {
            return;
        };
        let task = cx.background_executor().spawn(async move {
            let delta = request.target_rotation.delta_from(request.source_rotation);
            let result = (|| {
                Ok(PageRotationPresentation {
                    current_page: resource
                        .render_page(request.page_index, DEFAULT_PAGE_RENDER_WIDTH)?
                        .rotated(delta)?,
                    thumbnail: resource
                        .render_page(request.page_index, DEFAULT_THUMBNAIL_WIDTH)?
                        .rotated(delta)?,
                })
            })();
            (request, result)
        });
        cx.spawn(async move |entity, cx| {
            let (request, result) = task.await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.apply_page_rotation_result(&request, result, cx);
            });
        })
        .detach();
    }

    pub fn apply_page_rotation_result(
        &mut self,
        request: &PageRotationRequest,
        result: Result<PageRotationPresentation, String>,
        cx: &mut Context<Self>,
    ) -> ApplyDisposition {
        let Some(session) = self.session(request.document_id, cx).cloned() else {
            return ApplyDisposition::RejectedClosed;
        };
        let is_current = {
            let session = session.read(cx);
            session.generation == request.generation
                && session.resource_epoch == request.resource_epoch
                && session
                    .annotations
                    .snapshot(request.document_id.value())
                    .is_some_and(|snapshot| snapshot.revision == request.document_revision)
        };
        if !is_current {
            return ApplyDisposition::RejectedStale;
        }
        session.update(cx, |session, cx| {
            session.pending_rotation_generation = None;
            match result {
                Ok(presentation) => {
                    let page_image = presentation.current_page.clone().into_render_image();
                    let thumbnail_image = presentation.thumbnail.clone().into_render_image();
                    match (page_image, thumbnail_image) {
                        (Ok(page_image), Ok(thumbnail_image)) => {
                            session.current_page = request.page_index;
                            session.current_base_raster = Some(presentation.current_page);
                            session.current_image = Some(page_image);
                            if let Some(thumbnail) = session
                                .thumbnails
                                .iter_mut()
                                .find(|thumbnail| thumbnail.page_index == request.page_index)
                            {
                                thumbnail.base_raster = presentation.thumbnail;
                                thumbnail.image = thumbnail_image;
                            }
                            session.presentation_error = None;
                            if let Err(error) = session.rebuild_stable_highlight_presentations() {
                                if session
                                    .annotations
                                    .undo(request.document_id.value())
                                    .is_ok()
                                {
                                    session.sync_rotation_geometry();
                                    let _ = session.refresh_rotation_presentations();
                                }
                                session.presentation_error = Some(error);
                            }
                        }
                        (Err(error), _) | (_, Err(error)) => {
                            if session
                                .annotations
                                .undo(request.document_id.value())
                                .is_ok()
                            {
                                session.sync_rotation_geometry();
                            }
                            session.presentation_error = Some(error);
                        }
                    }
                }
                Err(error) => {
                    if session
                        .annotations
                        .undo(request.document_id.value())
                        .is_ok()
                    {
                        session.sync_rotation_geometry();
                    }
                    session.presentation_error = Some(error);
                }
            }
            cx.notify();
        });
        cx.notify();
        ApplyDisposition::Applied
    }

    fn navigate_async(&mut self, document_id: DocumentId, page_index: u32, cx: &mut Context<Self>) {
        let Ok(request) = self.begin_page_navigation(document_id, page_index, cx) else {
            return;
        };
        let Some(resource) = self
            .session(document_id, cx)
            .and_then(|session| session.read(cx).resource.clone())
        else {
            return;
        };
        let task = cx.background_executor().spawn(async move {
            let page = resource
                .render_page(request.page_index, DEFAULT_PAGE_RENDER_WIDTH)
                .and_then(|surface| {
                    surface.rotated(request.target_rotation.delta_from(request.source_rotation))
                })?;
            let thumbnail = resource
                .render_page(request.page_index, DEFAULT_THUMBNAIL_WIDTH)
                .and_then(|surface| {
                    surface.rotated(request.target_rotation.delta_from(request.source_rotation))
                })?;
            Ok::<_, String>((page, thumbnail))
        });
        cx.spawn(async move |entity, cx| {
            let result = task.await;
            let _ = entity.update(cx, |workspace, cx| match result {
                Ok((page, thumbnail)) => {
                    if workspace.apply_page_result(&request, Ok(page), cx)
                        == ApplyDisposition::Applied
                    {
                        workspace.apply_navigation_thumbnail(&request, thumbnail, cx);
                    }
                }
                Err(error) => {
                    workspace.apply_page_result(&request, Err(error), cx);
                }
            });
        })
        .detach();
    }

    fn apply_navigation_thumbnail(
        &mut self,
        request: &PageRenderRequest,
        surface: RasterSurface,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(request.document_id, cx).cloned() else {
            return;
        };
        if session.read(cx).generation != request.generation {
            return;
        }
        session.update(cx, |session, cx| {
            let Ok(image) = surface.clone().into_render_image() else {
                return;
            };
            if let Some(thumbnail) = session
                .thumbnails
                .iter_mut()
                .find(|thumbnail| thumbnail.page_index == request.page_index)
            {
                thumbnail.base_raster = surface;
                thumbnail.image = image;
            } else {
                session.thumbnails.push(ThumbnailPresentation {
                    page_index: request.page_index,
                    base_raster: surface,
                    image,
                    highlight_pixels: 0,
                });
                session
                    .thumbnails
                    .sort_by_key(|thumbnail| thumbnail.page_index);
            }
            if let Err(error) = session.rebuild_stable_highlight_presentations() {
                session.presentation_error = Some(error);
            }
            cx.notify();
        });
    }

    pub fn scroll_thumbnail_to_page(&mut self, page_index: u32, cx: &mut Context<Self>) -> bool {
        let Some(document_id) = self.active_document_id else {
            return false;
        };
        let in_range = self
            .session(document_id, cx)
            .is_some_and(|session| (page_index as usize) < session.read(cx).page_sizes.len());
        if !in_range {
            return false;
        }
        self.thumbnail_scroll
            .scroll_to_item(page_index as usize, ScrollStrategy::Nearest);
        cx.notify();
        true
    }

    pub fn activate_thumbnail_page(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> bool {
        let in_range = self
            .session(document_id, cx)
            .is_some_and(|session| (page_index as usize) < session.read(cx).page_sizes.len());
        if !in_range {
            return false;
        }
        if let Some(session) = self.session(document_id, cx).cloned() {
            let now = cx.background_executor().now();
            session.update(cx, |session, _| {
                session
                    .viewer
                    .mark_thumbnail_navigation_target(page_index as usize, now);
            });
        }
        self.navigate_async(document_id, page_index, cx);
        true
    }

    pub fn apply_page_result(
        &mut self,
        request: &PageRenderRequest,
        result: Result<RasterSurface, String>,
        cx: &mut Context<Self>,
    ) -> ApplyDisposition {
        let Some(session) = self.session(request.document_id, cx).cloned() else {
            return ApplyDisposition::RejectedClosed;
        };
        if session.read(cx).generation != request.generation {
            return ApplyDisposition::RejectedStale;
        }
        let mut page_applied = false;
        session.update(cx, |session, cx| {
            match result {
                Ok(surface) => match surface.clone().into_render_image() {
                    Ok(image) => {
                        session.current_page = request.page_index;
                        session.current_base_raster = Some(surface);
                        session.current_image = Some(image);
                        session.viewer.invalidate_layout();
                        session.status = NativeDocumentStatus::Ready;
                        session.presentation_error = None;
                        if let Err(error) = session.rebuild_stable_highlight_presentations() {
                            session.presentation_error = Some(error);
                        }
                        page_applied = true;
                    }
                    Err(error) => {
                        session.requested_page = session.current_page;
                        session.presentation_error = Some(error);
                    }
                },
                Err(error) => {
                    session.requested_page = session.current_page;
                    session.presentation_error = Some(error);
                }
            }
            cx.notify();
        });
        if page_applied {
            self.thumbnail_scroll
                .scroll_to_item(request.page_index as usize, ScrollStrategy::Nearest);
        }
        cx.notify();
        ApplyDisposition::Applied
    }

    pub fn begin_document_recovery(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<DocumentRecoveryRequest, String> {
        if self.opener.is_none() {
            return Err("no document opener is configured".into());
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let generation = self.next_generation();
        session.update(cx, |session, cx| {
            if !matches!(session.status, NativeDocumentStatus::Ready) {
                return Err("document session is not ready".into());
            }
            if session.presentation_error.is_none() {
                return Err("document recovery requires a presentation failure".into());
            }
            if session.recovery_generation.is_some() {
                return Err("document recovery is already in progress".into());
            }
            let target_rotations = session
                .page_rotation_quarter_turns()
                .into_iter()
                .map(|turns| {
                    PageRotation::from_degrees(i64::from(turns) * 90)
                        .expect("quarter-turn state must be canonical")
                })
                .collect();
            session.generation = generation;
            session.recovery_generation = Some(generation);
            let request = DocumentRecoveryRequest {
                document_id,
                generation,
                resource_epoch: session.resource_epoch,
                path: session.path.clone(),
                expected_source_sha256: session.source_sha256,
                current_page: session.current_page,
                expected_page_sizes: session.source_page_sizes.clone(),
                expected_source_rotations: session.source_page_rotations.clone(),
                expected_source_coordinate_spaces: session.source_page_coordinate_spaces.clone(),
                target_rotations,
            };
            cx.notify();
            Ok(request)
        })
    }

    pub fn prepare_document_recovery(
        &self,
        request: &DocumentRecoveryRequest,
    ) -> Result<OpenedNativeDocument, String> {
        let opener = self
            .opener
            .as_ref()
            .ok_or_else(|| "no document opener is configured".to_owned())?;
        Self::prepare_document_recovery_with(opener.as_ref(), request)
    }

    fn prepare_document_recovery_with(
        opener: &dyn NativeDocumentOpener,
        request: &DocumentRecoveryRequest,
    ) -> Result<OpenedNativeDocument, String> {
        let mut opened = opener.open(&OpenDocumentRequest {
            document_id: request.document_id,
            generation: request.generation,
            path: request.path.clone(),
        })?;
        let validate = || -> Result<(), String> {
            if opened.source_sha256 != request.expected_source_sha256 {
                return Err("document source changed while recovering its renderer".into());
            }
            if opened.page_sizes != request.expected_page_sizes
                || opened.page_rotations != request.expected_source_rotations
                || opened.page_coordinate_spaces != request.expected_source_coordinate_spaces
                || opened.page_sizes.len() != request.target_rotations.len()
            {
                return Err("document geometry changed while recovering its renderer".into());
            }
            if request.current_page as usize >= opened.page_sizes.len() {
                return Err("the retained current page is outside the recovered document".into());
            }
            Ok(())
        };
        if let Err(error) = validate() {
            let _ = opened.resource.close();
            return Err(error);
        }
        let source_rotation = opened.page_rotations[request.current_page as usize];
        let target_rotation = request.target_rotations[request.current_page as usize];
        let current_page = opened
            .resource
            .render_page(request.current_page, DEFAULT_PAGE_RENDER_WIDTH)
            .and_then(|surface| surface.rotated(target_rotation.delta_from(source_rotation)));
        let current_page = match current_page {
            Ok(surface) => surface,
            Err(error) => {
                let _ = opened.resource.close();
                return Err(error);
            }
        };
        let mut thumbnails = Vec::with_capacity(opened.thumbnails.len());
        for thumbnail in opened.thumbnails {
            let page = thumbnail.page_index as usize;
            let Some((&source_rotation, &target_rotation)) = opened
                .page_rotations
                .get(page)
                .zip(request.target_rotations.get(page))
            else {
                let _ = opened.resource.close();
                return Err("a recovered thumbnail is outside the retained document".into());
            };
            match thumbnail
                .raster
                .rotated(target_rotation.delta_from(source_rotation))
            {
                Ok(raster) => thumbnails.push(ThumbnailSurface::new(thumbnail.page_index, raster)),
                Err(error) => {
                    let _ = opened.resource.close();
                    return Err(error);
                }
            }
        }
        opened.current_page = current_page;
        opened.thumbnails = thumbnails;
        Ok(opened)
    }

    pub fn retry_document_recovery_async(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let request = self.begin_document_recovery(document_id, cx)?;
        let opener = self
            .opener
            .clone()
            .ok_or_else(|| "no document opener is configured".to_owned())?;
        let task = cx.background_executor().spawn(async move {
            let result = Self::prepare_document_recovery_with(opener.as_ref(), &request);
            (request, result)
        });
        cx.spawn(async move |entity, cx| {
            let (request, result) = task.await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.apply_document_recovery_result(&request, result, cx);
            });
        })
        .detach();
        Ok(())
    }

    pub fn apply_document_recovery_result(
        &mut self,
        request: &DocumentRecoveryRequest,
        result: Result<OpenedNativeDocument, String>,
        cx: &mut Context<Self>,
    ) -> ApplyDisposition {
        let Some(session) = self.session(request.document_id, cx).cloned() else {
            if let Ok(opened) = result {
                self.record_detached_release(
                    "closed-document recovery result",
                    opened.resource.close(),
                    cx,
                );
            }
            return ApplyDisposition::RejectedClosed;
        };
        let is_current = {
            let session = session.read(cx);
            session.generation == request.generation
                && session.resource_epoch == request.resource_epoch
                && session.recovery_generation == Some(request.generation)
        };
        if !is_current {
            if let Ok(opened) = result {
                self.record_detached_release(
                    "stale document recovery result",
                    opened.resource.close(),
                    cx,
                );
            }
            return ApplyDisposition::RejectedStale;
        }
        let mut opened = match result {
            Ok(opened) => opened,
            Err(error) => {
                session.update(cx, |session, cx| {
                    session.recovery_generation = None;
                    session.presentation_error = Some(error);
                    cx.notify();
                });
                cx.notify();
                return ApplyDisposition::Applied;
            }
        };
        if opened.source_sha256 != request.expected_source_sha256
            || opened.page_sizes != request.expected_page_sizes
            || opened.page_rotations != request.expected_source_rotations
            || opened.page_coordinate_spaces != request.expected_source_coordinate_spaces
        {
            self.record_detached_release(
                "invalid document recovery result",
                opened.resource.close(),
                cx,
            );
            session.update(cx, |session, cx| {
                session.recovery_generation = None;
                session.presentation_error =
                    Some("document source changed while recovering its renderer".into());
                cx.notify();
            });
            cx.notify();
            return ApplyDisposition::Applied;
        }
        let current_base_raster = opened.current_page;
        let current_image = match current_base_raster.clone().into_render_image() {
            Ok(image) => image,
            Err(error) => {
                self.record_detached_release(
                    "invalid document recovery raster",
                    opened.resource.close(),
                    cx,
                );
                session.update(cx, |session, cx| {
                    session.recovery_generation = None;
                    session.presentation_error = Some(error);
                    cx.notify();
                });
                cx.notify();
                return ApplyDisposition::Applied;
            }
        };
        let thumbnails = opened
            .thumbnails
            .drain(..)
            .map(|thumbnail| {
                let base_raster = thumbnail.raster;
                base_raster
                    .clone()
                    .into_render_image()
                    .map(|image| ThumbnailPresentation {
                        page_index: thumbnail.page_index,
                        base_raster,
                        image,
                        highlight_pixels: 0,
                    })
            })
            .collect::<Result<Vec<_>, _>>();
        let thumbnails = match thumbnails {
            Ok(thumbnails) => thumbnails,
            Err(error) => {
                self.record_detached_release(
                    "invalid document recovery thumbnail",
                    opened.resource.close(),
                    cx,
                );
                session.update(cx, |session, cx| {
                    session.recovery_generation = None;
                    session.presentation_error = Some(error);
                    cx.notify();
                });
                cx.notify();
                return ApplyDisposition::Applied;
            }
        };
        let old_resource = session.read(cx).resource.clone();
        if let Some(old_resource) = old_resource
            && let Err(error) = old_resource.close()
        {
            self.record_detached_release(
                "replacement document recovery result",
                opened.resource.close(),
                cx,
            );
            session.update(cx, |session, cx| {
                session.recovery_generation = None;
                session.presentation_error =
                    Some(format!("failed to release crashed PDF worker: {error}"));
                cx.notify();
            });
            cx.notify();
            return ApplyDisposition::Applied;
        }
        session.update(cx, |session, cx| {
            let old_current = session.current_image.replace(current_image);
            let old_thumbnails = std::mem::replace(&mut session.thumbnails, thumbnails);
            session.current_base_raster = Some(current_base_raster);
            session.source_page_coordinate_spaces = opened.page_coordinate_spaces.clone();
            session.resource = Some(opened.resource);
            session.resource_epoch = session.resource_epoch.saturating_add(1);
            session.requested_page = session.current_page;
            session.recovery_generation = None;
            session.presentation_error = None;
            session.viewer.invalidate_raster();
            session.status = NativeDocumentStatus::Ready;
            if let Some(image) = old_current {
                cx.drop_image(image, None);
            }
            for thumbnail in old_thumbnails {
                cx.drop_image(thumbnail.image, None);
            }
            if let Err(error) = session.rebuild_stable_highlight_presentations() {
                session.presentation_error = Some(error);
            }
            cx.notify();
        });
        self.page_interactions
            .retain(|(owner, _), _| *owner != request.document_id);
        self.last_painted_page_evidence
            .retain(|(owner, _), _| *owner != request.document_id);
        cx.notify();
        ApplyDisposition::Applied
    }

    fn begin_save_request(
        &mut self,
        document_id: DocumentId,
        destination: SaveDestination,
        cx: &mut Context<Self>,
    ) -> Result<SaveDocumentRequest, String> {
        if self
            .pending_text_box_editor
            .as_ref()
            .is_some_and(|editor| editor.document_id == document_id)
        {
            self.commit_pending_text_box(cx)?;
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let (annotations, source_path, current_page, generation, expected_source_sha256) = {
            let session_state = session.read(cx);
            if !matches!(session_state.status, NativeDocumentStatus::Ready) {
                return Err("document session is not ready".into());
            }
            if session_state.save_status == NativeDocumentSaveStatus::Saving {
                return Err("document save is already in progress".into());
            }
            if session_state.pending_rotation_generation.is_some() {
                return Err("page rotation pixels are still pending; Save As is blocked".into());
            }
            let annotations = session_state
                .annotations
                .snapshot(document_id.value())
                .ok_or_else(|| "document has no annotation state to save".to_owned())?;
            (
                annotations,
                session_state.path.clone(),
                session_state.current_page,
                session_state.save_generation.saturating_add(1),
                session_state.source_sha256,
            )
        };
        session.update(cx, |session, cx| {
            session.save_generation = generation;
            session.save_status = NativeDocumentSaveStatus::Saving;
            cx.notify();
        });
        if self
            .active_annotation_pointer
            .is_some_and(|active| active.document_id == document_id)
            && let Some(active) = self.active_annotation_pointer.take()
        {
            self.cancel_retained_annotation_pointer(active, cx);
        }
        if self.signature_popover_open && self.active_document_id == Some(document_id) {
            self.dismiss_signature_popover(document_id, None, cx);
        }
        cx.notify();
        Ok(SaveDocumentRequest {
            document_id,
            generation,
            source_path,
            destination,
            current_page,
            annotation_revision: annotations.revision,
            annotations,
            expected_source_sha256,
        })
    }

    pub fn begin_save_as(
        &mut self,
        document_id: DocumentId,
        target_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Result<SaveDocumentRequest, String> {
        let session = self
            .session(document_id, cx)
            .ok_or_else(|| "document session is closed".to_owned())?;
        let source_path = {
            let session = session.read(cx);
            if !matches!(session.status, NativeDocumentStatus::Ready) {
                return Err("document session is not ready".into());
            }
            if session.save_status == NativeDocumentSaveStatus::Saving {
                return Err("document save is already in progress".into());
            }
            if session.pending_rotation_generation.is_some() {
                return Err("page rotation pixels are still pending; Save As is blocked".into());
            }
            session.path.clone()
        };
        let authority =
            SaveAsTargetAuthority::bind(target_path, &source_path).map_err(|error| {
                match error.kind() {
                    SaveTargetErrorKind::NotPdf => {
                        "Save As requires a .pdf file name.".to_owned()
                    }
                    SaveTargetErrorKind::TargetExists => {
                        "Save As will not replace an existing destination.".to_owned()
                    }
                    SaveTargetErrorKind::SameAsSource => "Save As will not replace the source PDF. Choose a new destination so the original remains preserved.".to_owned(),
                    _ => error.to_string(),
                }
            })?;
        self.begin_save_request(document_id, SaveDestination::NewTarget(authority), cx)
    }

    pub fn begin_save(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<SaveDocumentRequest, String> {
        if self.session(document_id, cx).is_none() {
            return Err("document session is closed".into());
        }
        if self.document_requires_save_as(document_id, cx) {
            return Err("document requires Save As".into());
        }
        self.begin_save_request(document_id, SaveDestination::OpenedSource, cx)
    }

    pub fn apply_save_result(
        &mut self,
        request: &SaveDocumentRequest,
        result: Result<SavedNativeDocument, String>,
        cx: &mut Context<Self>,
    ) -> ApplyDisposition {
        let Some(session) = self.session(request.document_id, cx).cloned() else {
            if let Ok(saved) = result {
                self.record_detached_release(
                    "closed-document save result",
                    saved.opened.resource.close(),
                    cx,
                );
            }
            return ApplyDisposition::RejectedClosed;
        };
        if session.read(cx).save_generation != request.generation {
            if let Ok(saved) = result {
                self.record_detached_release(
                    "stale save result",
                    saved.opened.resource.close(),
                    cx,
                );
            }
            return ApplyDisposition::RejectedStale;
        }
        let mut saved = match result {
            Ok(saved) => saved,
            Err(error) => {
                session.update(cx, |session, cx| {
                    session.save_status = NativeDocumentSaveStatus::Failed(DocumentSaveFailure {
                        generation: request.generation,
                        operation: if request.is_in_place() {
                            DocumentSaveFailureOperation::InPlace
                        } else {
                            DocumentSaveFailureOperation::SaveAs
                        },
                        message: error,
                    });
                    cx.notify();
                });
                if self.close_after_save_document_id == Some(request.document_id) {
                    self.close_after_save_document_id = None;
                    self.pending_close_document_id = Some(request.document_id);
                }
                cx.notify();
                return ApplyDisposition::Applied;
            }
        };
        if saved.validated_revision != request.annotation_revision {
            self.record_detached_release(
                "revision-rejected save result",
                saved.opened.resource.close(),
                cx,
            );
            session.update(cx, |session, cx| {
                session.save_status = NativeDocumentSaveStatus::Failed(DocumentSaveFailure {
                    generation: request.generation,
                    operation: if request.is_in_place() {
                        DocumentSaveFailureOperation::InPlace
                    } else {
                        DocumentSaveFailureOperation::SaveAs
                    },
                    message: "saved document validation returned the wrong annotation revision"
                        .into(),
                });
                cx.notify();
            });
            cx.notify();
            return ApplyDisposition::Applied;
        }
        let publication_warning = saved.publication_warning.take();
        let has_publication_warning = publication_warning.is_some();
        let current_base_raster = saved.opened.current_page;
        let page_image = match current_base_raster.clone().into_render_image() {
            Ok(image) => image,
            Err(error) => {
                self.record_detached_release(
                    "invalid-raster save result",
                    saved.opened.resource.close(),
                    cx,
                );
                session.update(cx, |session, cx| {
                    session.save_status = NativeDocumentSaveStatus::Failed(DocumentSaveFailure {
                        generation: request.generation,
                        operation: if request.is_in_place() {
                            DocumentSaveFailureOperation::InPlace
                        } else {
                            DocumentSaveFailureOperation::SaveAs
                        },
                        message: error,
                    });
                    cx.notify();
                });
                cx.notify();
                return ApplyDisposition::Applied;
            }
        };
        let thumbnails =
            saved
                .opened
                .thumbnails
                .drain(..)
                .filter_map(|thumbnail| {
                    let base_raster = thumbnail.raster;
                    base_raster.clone().into_render_image().ok().map(|image| {
                        ThumbnailPresentation {
                            page_index: thumbnail.page_index,
                            base_raster,
                            image,
                            highlight_pixels: 0,
                        }
                    })
                })
                .collect();
        let source_sha256 = saved.opened.source_sha256;
        let source_page_rotations = saved.opened.page_rotations;
        let source_page_coordinate_spaces = saved.opened.page_coordinate_spaces;
        let new_resource = saved.opened.resource;
        let reopened_generation = self.next_generation();
        let old_resource = session.update(cx, |session, cx| {
            let old_resource = session.resource.replace(new_resource);
            session.path = request.target_path().to_path_buf();
            session.title = saved.opened.title;
            session.source_page_sizes = saved.opened.page_sizes;
            session.source_page_rotations = source_page_rotations;
            session.source_page_coordinate_spaces = source_page_coordinate_spaces;
            session.sync_rotation_geometry();
            session.generation = reopened_generation;
            session.pending_rotation_generation = None;
            session.resource_epoch = session.resource_epoch.saturating_add(1);
            session.current_page = request.current_page;
            session.requested_page = request.current_page;
            session.current_base_raster = Some(current_base_raster);
            session.current_image = Some(page_image);
            session.thumbnails = thumbnails;
            session.source_sha256 = source_sha256;
            // The validated reopen owns a different PDF resource. Pixels from
            // the prior resource must never survive the atomic swap, even when
            // document identity, annotation revision, zoom, and crop match.
            session.viewer.invalidate_raster();
            session.status = NativeDocumentStatus::Ready;
            session.save_status = NativeDocumentSaveStatus::Idle;
            session.save_as_required = false;
            if session
                .annotations
                .snapshot(request.document_id.value())
                .is_some_and(|snapshot| snapshot.revision == request.annotation_revision)
            {
                let _ = session.annotations.mark_saved(request.document_id.value());
            }
            if let Err(error) = session.rebuild_stable_highlight_presentations() {
                session.status = NativeDocumentStatus::Failed(error);
            }
            cx.notify();
            old_resource
        });
        let old_resource_released = if let Some(old_resource) = old_resource {
            match old_resource.close() {
                Ok(()) => true,
                Err(error) => {
                    self.record_detached_release("replaced save resource", Err(error), cx);
                    false
                }
            }
        } else {
            true
        };
        if old_resource_released {
            let temporary_release =
                session.update(cx, |session, _| session.release_temporary_source());
            self.record_detached_release("replaced generated source", temporary_release, cx);
        }
        if let Some(warning) = publication_warning {
            self.annotation_statuses
                .insert(request.document_id, warning);
        }
        self.page_interactions
            .retain(|(document_id, _), _| *document_id != request.document_id);
        self.last_painted_page_evidence
            .retain(|(document_id, _), _| *document_id != request.document_id);
        if self.close_after_save_document_id == Some(request.document_id) {
            let saved_current_revision = self
                .annotation_snapshot(request.document_id, cx)
                .is_some_and(|snapshot| !snapshot.dirty);
            if saved_current_revision && !has_publication_warning {
                self.pending_close_document_id = Some(request.document_id);
                self.close_document(request.document_id, cx);
            } else {
                self.close_after_save_document_id = None;
                self.pending_close_document_id = Some(request.document_id);
            }
        }
        cx.notify();
        ApplyDisposition::Applied
    }

    pub fn create_rectangle(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("document session is not ready".into());
        }
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("rectangle page is outside the document".into());
        }

        session.update(cx, |session, cx| {
            const POINTER_ID: u64 = 1;
            session
                .annotations
                .set_tool(AnnotationTool::Rectangle)
                .map_err(|error| error.to_string())?;
            session.annotations.queue_next_annotation_id(id.clone());
            session
                .annotations
                .pointer_down(document_id.value(), page_index, POINTER_ID, start, 0.)
                .map_err(|error| error.to_string())?;
            session
                .annotations
                .pointer_move(POINTER_ID, end)
                .map_err(|error| error.to_string())?;
            let outcome = session
                .annotations
                .pointer_up(POINTER_ID, end)
                .map_err(|error| error.to_string())?;
            if outcome != PointerPhaseOutcome::AnnotationCreated(id) {
                return Err(format!(
                    "rectangle gesture did not create the requested annotation: {outcome:?}"
                ));
            }
            cx.notify();
            Ok(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn create_ellipse(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
        constrain_circle: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("document session is not ready".into());
        }
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("ellipse page is outside the document".into());
        }

        session.update(cx, |session, cx| {
            const POINTER_ID: u64 = 2;
            session
                .annotations
                .set_tool(AnnotationTool::Ellipse)
                .map_err(|error| error.to_string())?;
            session.annotations.queue_next_annotation_id(id.clone());
            session
                .annotations
                .pointer_down_with_input(
                    document_id.value(),
                    page_index,
                    POINTER_ID,
                    0,
                    start,
                    0.,
                    constrain_circle,
                )
                .map_err(|error| error.to_string())?;
            session
                .annotations
                .pointer_move_with_constraint(POINTER_ID, end, constrain_circle)
                .map_err(|error| error.to_string())?;
            let outcome = session
                .annotations
                .pointer_up_with_constraint(POINTER_ID, end, constrain_circle)
                .map_err(|error| error.to_string())?;
            if outcome != PointerPhaseOutcome::AnnotationCreated(id) {
                return Err(format!(
                    "ellipse gesture did not create the requested annotation: {outcome:?}"
                ));
            }
            cx.notify();
            Ok(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn translate_ellipse(
        &mut self,
        document_id: DocumentId,
        id: MarkupId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let session = self
            .session(document_id, cx)
            .cloned()
            .ok_or_else(|| "document session is closed".to_owned())?;
        session.update(cx, |session, cx| {
            session
                .annotations
                .translate_ellipse(document_id.value(), id, delta_x, delta_y)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn set_ellipse_rect(
        &mut self,
        document_id: DocumentId,
        id: MarkupId,
        rect: PdfRect,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let session = self
            .session(document_id, cx)
            .cloned()
            .ok_or_else(|| "document session is closed".to_owned())?;
        session.update(cx, |session, cx| {
            session
                .annotations
                .set_ellipse_rect(document_id.value(), id, rect)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn set_ellipse_rotation(
        &mut self,
        document_id: DocumentId,
        id: MarkupId,
        rotation_degrees: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let session = self
            .session(document_id, cx)
            .cloned()
            .ok_or_else(|| "document session is closed".to_owned())?;
        session.update(cx, |session, cx| {
            session
                .annotations
                .set_ellipse_rotation(document_id.value(), id, rotation_degrees)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn set_selected_ellipse_appearance(
        &mut self,
        document_id: DocumentId,
        appearance: RectangleAppearance,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let session = self
            .session(document_id, cx)
            .cloned()
            .ok_or_else(|| "document session is closed".to_owned())?;
        session.update(cx, |session, cx| {
            session
                .annotations
                .set_selected_rectangle_appearance(document_id.value(), appearance)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn create_pen(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        id: MarkupId,
        points: &[PdfPoint],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some((start, remaining)) = points.split_first() else {
            return Err("pen path requires at least two points".into());
        };
        if remaining.is_empty() {
            return Err("pen path requires at least two points".into());
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("document session is not ready".into());
        }
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("pen page is outside the document".into());
        }
        session.update(cx, |session, cx| {
            const POINTER_ID: u64 = 2;
            session
                .annotations
                .set_tool(AnnotationTool::Pen)
                .map_err(|error| error.to_string())?;
            session.annotations.queue_next_annotation_id(id.clone());
            session
                .annotations
                .pointer_down(document_id.value(), page_index, POINTER_ID, *start, 0.)
                .map_err(|error| error.to_string())?;
            for point in remaining.iter().copied() {
                session
                    .annotations
                    .pointer_move(POINTER_ID, point)
                    .map_err(|error| error.to_string())?;
            }
            let end = *remaining
                .last()
                .expect("a validated Pen path has an endpoint");
            let outcome = session
                .annotations
                .pointer_up(POINTER_ID, end)
                .map_err(|error| error.to_string())?;
            if outcome != PointerPhaseOutcome::AnnotationCreated(id) {
                return Err(format!(
                    "pen gesture did not create the requested annotation: {outcome:?}"
                ));
            }
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn create_straight_line(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
        kind: LineKind,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("document session is not ready".into());
        }
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("straight-line page is outside the document".into());
        }
        let tool = match kind {
            LineKind::Line => AnnotationTool::Line,
            LineKind::Arrow => AnnotationTool::Arrow,
        };
        session.update(cx, |session, cx| {
            const POINTER_ID: u64 = 5;
            session
                .annotations
                .set_tool(tool)
                .map_err(|error| error.to_string())?;
            session.annotations.queue_next_annotation_id(id.clone());
            session
                .annotations
                .pointer_down_with_input(
                    document_id.value(),
                    page_index,
                    POINTER_ID,
                    0,
                    start,
                    0.,
                    false,
                )
                .map_err(|error| error.to_string())?;
            session
                .annotations
                .pointer_move_with_constraint(POINTER_ID, end, false)
                .map_err(|error| error.to_string())?;
            let outcome = session
                .annotations
                .pointer_up_with_constraint(POINTER_ID, end, false)
                .map_err(|error| error.to_string())?;
            if outcome != PointerPhaseOutcome::AnnotationCreated(id) {
                return Err(format!(
                    "straight-line gesture did not create the requested annotation: {outcome:?}"
                ));
            }
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn create_highlight(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        id: MarkupId,
        points: &[PdfPoint],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some((start, remaining)) = points.split_first() else {
            return Err("highlight path requires at least two points".into());
        };
        if remaining.is_empty() {
            return Err("highlight path requires at least two points".into());
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("document session is not ready".into());
        }
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("highlight page is outside the document".into());
        }
        session.update(cx, |session, cx| {
            const POINTER_ID: u64 = 4;
            session
                .annotations
                .set_tool(AnnotationTool::Highlight)
                .map_err(|error| error.to_string())?;
            session.annotations.queue_next_annotation_id(id.clone());
            session
                .annotations
                .pointer_down(document_id.value(), page_index, POINTER_ID, *start, 0.)
                .map_err(|error| error.to_string())?;
            for point in remaining.iter().copied() {
                session
                    .annotations
                    .pointer_move(POINTER_ID, point)
                    .map_err(|error| error.to_string())?;
            }
            let end = *remaining
                .last()
                .expect("a validated Highlight path has an endpoint");
            let outcome = session
                .annotations
                .pointer_up(POINTER_ID, end)
                .map_err(|error| error.to_string())?;
            if outcome != PointerPhaseOutcome::AnnotationCreated(id) {
                return Err(format!(
                    "highlight gesture did not create the requested annotation: {outcome:?}"
                ));
            }
            session.rebuild_stable_highlight_presentations()?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn create_length(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
        calibration: LengthCalibration,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready) {
            return Err("document session is not ready".into());
        }
        if page_index as usize >= session.read(cx).page_sizes.len() {
            return Err("length page is outside the document".into());
        }
        session.update(cx, |session, cx| {
            session
                .annotations
                .set_tool(AnnotationTool::Length)
                .map_err(|error| error.to_string())?;
            session
                .annotations
                .set_document_page_length_calibration(document_id.value(), page_index, calibration)
                .map_err(|error| error.to_string())?;
            session
                .annotations
                .begin_length_placement(document_id.value(), page_index, id.clone(), start)
                .map_err(|error| error.to_string())?;
            let outcome = session
                .annotations
                .commit_length_placement(document_id.value(), page_index, end, false)
                .map_err(|error| error.to_string())?;
            if outcome != PointerPhaseOutcome::AnnotationCreated(id) {
                return Err(format!(
                    "length gesture did not create the requested annotation: {outcome:?}"
                ));
            }
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn select_annotation(
        &mut self,
        document_id: DocumentId,
        annotation_id: &MarkupId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let selected = session.update(cx, |session, cx| {
            let selected = session
                .annotations
                .select_id(document_id.value(), annotation_id);
            if selected {
                cx.notify();
            }
            selected
        });
        if selected {
            cx.notify();
        }
        selected
    }

    pub fn toggle_annotation_selection(
        &mut self,
        document_id: DocumentId,
        annotation_id: &MarkupId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return false;
        };
        let selected = session.update(cx, |session, cx| {
            let selected = session
                .annotations
                .toggle_selection(document_id.value(), annotation_id);
            if selected {
                cx.notify();
            }
            selected
        });
        if selected {
            cx.notify();
        }
        selected
    }

    pub fn set_selected_rectangle_stroke_width(
        &mut self,
        document_id: DocumentId,
        stroke_width_pt: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        session.update(cx, |session, cx| {
            session
                .annotations
                .commit_selected_rectangle_stroke_width(document_id.value(), stroke_width_pt)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn move_selected_pen(
        &mut self,
        document_id: DocumentId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, |annotations, document_id| {
            annotations.move_selected_ink(document_id, delta_x, delta_y)
        })
    }

    pub fn move_selected_length(
        &mut self,
        document_id: DocumentId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.move_selected_length(document_id, delta_x, delta_y)
        })
    }

    pub fn set_selected_length_endpoint(
        &mut self,
        document_id: DocumentId,
        endpoint: LengthEndpoint,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_length_endpoint(document_id, endpoint, point)
        })
    }

    pub fn set_selected_pen_opacity(
        &mut self,
        document_id: DocumentId,
        opacity: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, |annotations, document_id| {
            annotations.set_selected_ink_opacity(document_id, opacity)
        })
    }

    pub fn edit_selected_straight_line_property(
        &mut self,
        document_id: DocumentId,
        edit: StraightLinePropertyEdit,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.edit_selected_straight_line_property(document_id, edit)
        })?;
        cx.notify();
        Ok(())
    }

    pub fn edit_selected_vertex_path_property(&mut self, document_id: DocumentId, edit: VertexPathPropertyEdit, cx: &mut Context<Self>) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| annotations.edit_selected_vertex_path_property(document_id, edit))?;
        cx.notify();
        Ok(())
    }

    pub fn edit_selected_measurement_path_property(&mut self, document_id: DocumentId, edit: VertexPathPropertyEdit, cx: &mut Context<Self>) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| annotations.edit_selected_measurement_path_property(document_id, edit))?;
        cx.notify();
        Ok(())
    }

    pub fn set_highlight_defaults(
        &mut self,
        document_id: DocumentId,
        color: &str,
        width_pt: f64,
        opacity: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let appearance = butter_paper_gpui_gallery::annotation_model::PenAppearance::new(
            color, width_pt, opacity,
        )
        .map_err(|error| error.to_string())?;
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        session.update(cx, |session, cx| {
            session
                .annotations
                .set_highlight_appearance(appearance)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn highlight_defaults(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<PenAnnotationDefaults> {
        self.session(document_id, cx).map(|session| {
            let appearance = session.read(cx).annotations.highlight_appearance();
            PenAnnotationDefaults {
                color: appearance.color().to_owned(),
                width_pt: appearance.width_pt(),
                opacity: appearance.opacity(),
            }
        })
    }

    pub fn delete_selected_annotation(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, |annotations, document_id| {
            annotations
                .delete_selected_unlocked(document_id)
                .map(|_| ())
        })
    }

    pub fn select_all_annotations_on_page(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Vec<MarkupId> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Vec::new();
        };
        let selected = session.update(cx, |session, cx| {
            let selected = session
                .annotations
                .select_all_on_page(document_id.value(), page_index)
                .to_vec();
            cx.notify();
            selected
        });
        cx.notify();
        selected
    }

    pub fn copy_selected_annotations(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> usize {
        let copied = self
            .session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .selected_annotations_in_document_order(document_id.value())
            })
            .unwrap_or_default();
        let count = copied.len();
        if count > 0 {
            self.annotation_clipboard = copied;
            self.annotation_paste_sequence = 0;
            self.annotation_statuses.insert(
                document_id,
                if count == 1 {
                    "Copied annotation".into()
                } else {
                    format!("Copied {count} annotations")
                },
            );
            cx.notify();
        }
        count
    }

    pub fn cut_selected_annotations(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<usize, String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let (copied, has_unlocked) = {
            let session = session.read(cx);
            if session.save_status == NativeDocumentSaveStatus::Saving {
                return Err("document save is in progress".into());
            }
            (
                session
                    .annotations
                    .selected_annotations_in_document_order(document_id.value()),
                session
                    .annotations
                    .selected_has_unlocked(document_id.value()),
            )
        };
        if copied.is_empty() {
            return Ok(0);
        }
        self.update_annotation_history(document_id, cx, |annotations, document_id| {
            annotations
                .delete_selected_unlocked(document_id)
                .map(|_| ())
        })?;
        let count = copied.len();
        self.annotation_clipboard = copied;
        self.annotation_paste_sequence = 0;
        self.annotation_statuses.insert(
            document_id,
            if has_unlocked {
                if count == 1 {
                    "Cut annotation".into()
                } else {
                    format!("Cut {count} annotations")
                }
            } else if count == 1 {
                "Copied locked annotation".into()
            } else {
                format!("Copied {count} locked annotations")
            },
        );
        cx.notify();
        Ok(count)
    }

    pub fn paste_annotations(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        cx: &mut Context<Self>,
    ) -> Result<Vec<MarkupId>, String> {
        if self.annotation_clipboard.is_empty() {
            return Err("annotation clipboard is empty".into());
        }
        self.annotation_paste_sequence = self.annotation_paste_sequence.saturating_add(1);
        let offset = 12. * self.annotation_paste_sequence as f64;
        let mut pasted = Vec::with_capacity(self.annotation_clipboard.len());
        for source in &self.annotation_clipboard {
            let family = match source {
                Annotation::Rectangle(_) => "rectangle",
                Annotation::Ellipse(_) => "ellipse",
                Annotation::Arc(_) => "arc",
                Annotation::Redact(_) => "redact",
                Annotation::StraightLine(annotation) => match annotation.kind {
                    LineKind::Line => "line",
                    LineKind::Arrow => "arrow",
                },
                Annotation::VertexPath(annotation) => match annotation.kind {
                    butter_paper_gpui_gallery::annotation_model::VertexPathKind::Polyline => {
                        "polyline"
                    }
                    butter_paper_gpui_gallery::annotation_model::VertexPathKind::Polygon => {
                        "polygon"
                    }
                },
                Annotation::MeasurementPath(annotation) => match annotation.kind {
                    MeasurementPathKind::Polylength => "polylength",
                    MeasurementPathKind::Area => "area",
                },
                Annotation::Cloud(_) => "cloud",
                Annotation::CloudPlus(_) => "cloud-plus",
                Annotation::Callout(_) => "callout",
                Annotation::Pen(annotation) => match annotation.tool() {
                    butter_paper_gpui_gallery::annotation_model::InkTool::Pen => "pen",
                    butter_paper_gpui_gallery::annotation_model::InkTool::Highlight => "highlight",
                },
                Annotation::TextBox(_) => "text",
                Annotation::Length(_) => "length",
                Annotation::Dimension(_) => "dimension",
                Annotation::Image(_) => "image",
                Annotation::Snapshot(_) => "snapshot",
            };
            let sequence = self.next_annotation_sequence;
            self.next_annotation_sequence = self.next_annotation_sequence.saturating_add(1);
            let id = MarkupId::new(format!("workspace:paste:{family}:{sequence}"))
                .map_err(|error| error.to_string())?;
            pasted.push(
                source
                    .translated_copy(id, page_index, offset, -offset)
                    .map_err(|error| error.to_string())?,
            );
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        let inserted = session.update(cx, |session, cx| {
            let inserted = session
                .annotations
                .insert_annotations(document_id.value(), pasted)
                .map_err(|error| error.to_string())?;
            let removed = session.sync_image_assets()?;
            defer_drop_images(removed, cx);
            cx.notify();
            Ok::<Vec<MarkupId>, String>(inserted)
        })?;
        self.annotation_statuses.insert(
            document_id,
            if inserted.len() == 1 {
                "Pasted annotation".into()
            } else {
                format!("Pasted {} annotations", inserted.len())
            },
        );
        cx.notify();
        Ok(inserted)
    }

    pub fn set_selected_annotation_locked(
        &mut self,
        document_id: DocumentId,
        locked: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, |annotations, document_id| {
            annotations.set_selected_locked(document_id, locked)
        })
    }

    pub fn undo_annotations(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.apply_history_with_rotation_refresh(document_id, true, cx)
    }

    pub fn redo_annotations(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.apply_history_with_rotation_refresh(document_id, false, cx)
    }

    fn apply_history_with_rotation_refresh(
        &mut self,
        document_id: DocumentId,
        undo: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        if session.read(cx).save_status == NativeDocumentSaveStatus::Saving {
            return Err("document save is in progress".into());
        }
        let generation = self.next_generation();
        session.update(cx, |session, cx| {
            let before = session
                .annotations
                .snapshot(document_id.value())
                .map(|snapshot| snapshot.page_rotations)
                .unwrap_or_default();
            if undo {
                session.annotations.undo(document_id.value())
            } else {
                session.annotations.redo(document_id.value())
            }
            .map_err(|error| error.to_string())?;
            let after = session
                .annotations
                .snapshot(document_id.value())
                .map(|snapshot| snapshot.page_rotations)
                .unwrap_or_default();
            session.generation = generation;
            session.pending_rotation_generation = None;
            if before != after {
                session.sync_rotation_geometry();
                if let Err(error) = session.refresh_rotation_presentations() {
                    let rollback = if undo {
                        session.annotations.redo(document_id.value())
                    } else {
                        session.annotations.undo(document_id.value())
                    };
                    rollback.map_err(|rollback| {
                        format!("{error}; history rollback also failed: {rollback}")
                    })?;
                    session.sync_rotation_geometry();
                    return Err(error);
                }
            } else {
                let removed = session.sync_image_assets()?;
                defer_drop_images(removed, cx);
                session.rebuild_stable_highlight_presentations()?;
            }
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    fn update_annotation_history(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
        command: impl FnOnce(
            &mut AnnotationAdapter,
            u64,
        )
            -> Result<(), butter_paper_gpui_gallery::annotation_model::AnnotationError>,
    ) -> Result<(), String> {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        session.update(cx, |session, cx| {
            if session.save_status == NativeDocumentSaveStatus::Saving {
                return Err("document save is in progress".into());
            }
            if session.pending_rotation_generation.is_some() {
                return Err("page rotation pixels are still pending".into());
            }
            command(&mut session.annotations, document_id.value())
                .map_err(|error| error.to_string())?;
            let removed = session.sync_image_assets()?;
            defer_drop_images(removed, cx);
            session.rebuild_stable_highlight_presentations()?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn annotation_snapshot(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<AnnotationSnapshot> {
        self.session(document_id, cx)
            .and_then(|session| session.read(cx).annotations.snapshot(document_id.value()))
    }

    pub fn snapshot_placement_pending(&self, document_id: DocumentId, cx: &App) -> bool {
        self.session(document_id, cx).is_some_and(|session| {
            session
                .read(cx)
                .annotations
                .snapshot_placement_pending(document_id.value())
        })
    }

    pub fn set_selected_vertex_path_point(
        &mut self,
        document_id: DocumentId,
        vertex_index: usize,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_vertex_path_point(document_id, vertex_index, point)
        })
    }

    pub fn set_selected_measurement_path_point(
        &mut self,
        document_id: DocumentId,
        vertex_index: usize,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_measurement_path_point(document_id, vertex_index, point)
        })
    }

    pub fn set_selected_cloud_point(
        &mut self,
        document_id: DocumentId,
        vertex_index: usize,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_cloud_point(document_id, vertex_index, point)
        })
    }

    pub fn set_selected_cloud_plus_cloud_point(
        &mut self,
        document_id: DocumentId,
        vertex_index: usize,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_cloud_plus_cloud_point(document_id, vertex_index, point)
        })
    }

    pub fn translate_selected_cloud_plus_text_box(
        &mut self,
        document_id: DocumentId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.translate_selected_cloud_plus_text_box(document_id, delta_x, delta_y)
        })
    }

    pub fn translate_selected_cloud_plus_group(
        &mut self,
        document_id: DocumentId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.translate_selected_cloud_plus_group(document_id, delta_x, delta_y)
        })
    }

    pub fn set_selected_dimension_offset(
        &mut self,
        document_id: DocumentId,
        offset: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_dimension_offset(document_id, offset)
        })
    }

    fn ensure_dimension_property_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DimensionPropertyInspector> {
        if let Some(inspector) = &self.dimension_property_inspector {
            return inspector.clone();
        }
        let inspector = cx.new(|cx| DimensionPropertyInspector::new(window, cx));
        let subscription = cx.subscribe(
            &inspector,
            |workspace, _, event: &DimensionPropertyEvent, cx| {
                if let Err(error) = workspace.apply_dimension_property_event(event, cx) {
                    workspace.annotation_statuses.insert(event.document_id, error);
                    cx.notify();
                }
            },
        );
        self.dimension_property_subscription = Some(subscription);
        self.dimension_property_inspector = Some(inspector.clone());
        inspector
    }

    pub fn set_dimension_property_inspector_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dimension_property_inspector_open = open;
        self.ensure_dimension_property_inspector(window, cx)
            .update(cx, |inspector, cx| inspector.set_open(open, cx));
        cx.notify();
    }

    pub fn apply_dimension_property_event(
        &mut self,
        event: &DimensionPropertyEvent,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if self.active_document_id != Some(event.document_id)
            || self.pending_close_document_id == Some(event.document_id)
            || self.close_after_save_document_id == Some(event.document_id)
            || self.pending_save_prompt_document_id() == Some(event.document_id)
            || self.pending_text_box_editor.as_ref().is_some_and(|editor| editor.document_id == event.document_id)
        {
            return Ok(false);
        }
        let Some(session) = self.session(event.document_id, cx).cloned() else { return Ok(false); };
        let state = session.read(cx);
        if !matches!(state.status, NativeDocumentStatus::Ready)
            || state.save_status == NativeDocumentSaveStatus::Saving
            || state.pending_rotation_generation.is_some()
        {
            return Ok(false);
        }
        let Some(snapshot) = state.annotations.snapshot(event.document_id.value()) else { return Ok(false); };
        let Some(current) = state.annotations.exact_selected_dimension(event.document_id.value()) else { return Ok(false); };
        if snapshot.revision != event.expected_revision
            || current.id != event.annotation_id
            || dimension_property_patch_matches(&event.patch, current)
        {
            return Ok(false);
        }
        if current.locked && !matches!(event.patch, DimensionPropertyPatch::Locked(_)) {
            return Ok(false);
        }
        match &event.patch {
            DimensionPropertyPatch::Locked(value) => self.update_annotation_history(event.document_id, cx, |annotations, id| annotations.set_selected_locked(id, *value))?,
            DimensionPropertyPatch::OffsetPt(value) if value.is_finite() => self.set_selected_dimension_offset(event.document_id, *value, cx)?,
            DimensionPropertyPatch::Appearance(appearance) => {
                let appearance = appearance.clone();
                self.update_annotation_history(event.document_id, cx, move |annotations, id| annotations.set_exact_selected_dimension_appearance(id, appearance))?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub fn set_selected_callout_leader_point(
        &mut self,
        document_id: DocumentId,
        point_index: usize,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_callout_leader_point(document_id, point_index, point)
        })
    }

    pub fn translate_selected_callout_text_box(
        &mut self,
        document_id: DocumentId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.translate_selected_callout_text_box(document_id, delta_x, delta_y)
        })
    }

    pub fn translate_selected_callout_group(
        &mut self,
        document_id: DocumentId,
        delta_x: f64,
        delta_y: f64,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.translate_selected_callout_group(document_id, delta_x, delta_y)
        })
    }

    pub fn set_selected_arc_control_point(
        &mut self,
        document_id: DocumentId,
        control: ArcControlPoint,
        point: PdfPoint,
        snap_quarter_turn: bool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_arc_control_point(
                document_id,
                control,
                point,
                snap_quarter_turn,
            )
        })
    }

    pub fn document_requires_save_as(&self, document_id: DocumentId, cx: &App) -> bool {
        self.document_save_route(document_id, cx) == Some(DocumentSaveRoute::NewTargetRequired)
    }

    pub fn document_save_route(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<DocumentSaveRoute> {
        self.session(document_id, cx).map(|session| {
            resolve_document_save_route(
                session.read(cx).save_as_required,
                PdfPersistenceSession::in_place_publication_capability(),
            )
        })
    }

    pub fn document_dirty_revision(&self, document_id: DocumentId, cx: &App) -> Option<u64> {
        self.session(document_id, cx)
            .and_then(|session| session.read(cx).dirty_revision())
    }

    pub fn selected_annotation_ids(&self, document_id: DocumentId, cx: &App) -> Vec<MarkupId> {
        self.session(document_id, cx)
            .map(|session| {
                session
                    .read(cx)
                    .annotations
                    .selected_ids(document_id.value())
                    .to_vec()
            })
            .unwrap_or_default()
    }

    pub fn image_asset_count(&self, document_id: DocumentId, cx: &App) -> usize {
        self.session(document_id, cx)
            .map(|session| session.read(cx).image_assets.len())
            .unwrap_or_default()
    }

    pub fn image_render_asset_weak(
        &self,
        document_id: DocumentId,
        asset_id: &str,
        cx: &App,
    ) -> Option<std::sync::Weak<RenderImage>> {
        self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .image_assets
                .get(asset_id)
                .map(Arc::downgrade)
        })
    }

    pub fn set_selected_image_rect(
        &mut self,
        document_id: DocumentId,
        rect: PdfRect,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        self.update_annotation_history(document_id, cx, move |annotations, document_id| {
            annotations.set_selected_image_rect(document_id, rect)
        })
    }

    pub fn highlight_composite_evidence(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<HighlightCompositeEvidence> {
        self.session(document_id, cx)
            .map(|session| session.read(cx).highlight_composite)
    }

    pub fn annotation_scene(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> AnnotationScene {
        self.session(document_id, cx).map_or_else(
            || AnnotationAdapter::default().document_scene(document_id.value(), page_index),
            |session| {
                let session = session.read(cx);
                self.annotation_scene_for_session(document_id, page_index, &session)
            },
        )
    }

    fn annotation_scene_for_session(
        &self,
        document_id: DocumentId,
        page_index: u32,
        session: &NativeDocumentSession,
    ) -> AnnotationScene {
        let preview_blocked = self.active_document_id != Some(document_id)
            || self.pending_text_box_editor.is_some()
            || self.pending_close_document_id == Some(document_id)
            || self.close_after_save_document_id == Some(document_id)
            || !matches!(session.status, NativeDocumentStatus::Ready)
            || session.save_status == NativeDocumentSaveStatus::Saving
            || session.pending_rotation_generation.is_some()
            || self.pending_save_prompt.is_some_and(|pending| {
                pending.document_id == document_id
                    && pending.document_generation == session.generation
            });
        if preview_blocked {
            session
                .annotations
                .canonical_document_scene(document_id.value(), page_index)
        } else {
            session
                .annotations
                .document_scene(document_id.value(), page_index)
        }
    }

    pub fn thumbnail_annotation_scene(
        &self,
        document_id: DocumentId,
        page_index: u32,
        cx: &App,
    ) -> AnnotationScene {
        self.session(document_id, cx).map_or_else(
            || AnnotationAdapter::default().thumbnail_scene(document_id.value(), page_index),
            |session| {
                session
                    .read(cx)
                    .annotations
                    .thumbnail_scene(document_id.value(), page_index)
            },
        )
    }

    pub fn active_selection_marquee(
        &self,
        document_id: DocumentId,
        cx: &App,
    ) -> Option<(u32, SelectionMarquee)> {
        self.session(document_id, cx).and_then(|session| {
            session
                .read(cx)
                .annotations
                .active_selection_marquee(document_id.value())
        })
    }

    pub fn set_annotation_tool(
        &mut self,
        document_id: DocumentId,
        tool: AnnotationTool,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self
            .active_annotation_pointer
            .is_some_and(|active| active.document_id == document_id)
            && self.annotation_tool(document_id, cx) != Some(tool)
            && let Some(active) = self.active_annotation_pointer.take()
        {
            self.cancel_retained_annotation_pointer(active, cx);
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        session.update(cx, |session, cx| {
            session
                .annotations
                .set_tool(tool)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(())
    }

    pub fn prepare_image_from_path(
        &mut self,
        document_id: DocumentId,
        path: &Path,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let decoded = decode_image_path(path).map_err(|error| error.to_string())?;
        self.apply_decoded_image(document_id, decoded, cx)
    }

    fn apply_decoded_image(
        &mut self,
        document_id: DocumentId,
        decoded: DecodedImageFile,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let asset_id = decoded.asset().id().as_str().to_owned();
        let asset = decoded.into_asset();
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        session.update(cx, |session, cx| {
            let page_size = session
                .annotation_page_geometry(session.current_page)
                .map(|(page_size, _)| page_size)
                .ok_or_else(|| "image placement page geometry is unavailable".to_owned())?;
            session.annotations.set_image_asset(asset);
            session
                .annotations
                .set_image_placement_page(f64::from(page_size.0), f64::from(page_size.1), 0.45)
                .map_err(|error| error.to_string())?;
            session
                .annotations
                .set_tool(AnnotationTool::Image)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(asset_id)
    }

    fn begin_image_selection(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        if self.pending_text_box_editor.is_some() || self.pending_close_document_id.is_some() {
            return;
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let authority = session.update(cx, |session, _| {
            if !matches!(session.status, NativeDocumentStatus::Ready)
                || session.save_status == NativeDocumentSaveStatus::Saving
            {
                return None;
            }
            session.image_prepare_generation = session.image_prepare_generation.saturating_add(1);
            Some(ImagePrepareAuthority {
                document_id,
                document_generation: session.generation,
                prepare_generation: session.image_prepare_generation,
            })
        });
        let Some(authority) = authority else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a PNG or JPEG image".into()),
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            let selected = match picker.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.apply_image_prepare_result(
                            authority,
                            Err(format!("Could not open the image picker: {error}")),
                            cx,
                        );
                    });
                    return;
                }
                Err(error) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.apply_image_prepare_result(
                            authority,
                            Err(format!("The image picker closed unexpectedly: {error}")),
                            cx,
                        );
                    });
                    return;
                }
            };
            let Some(path) = selected else {
                return;
            };
            let decoded = background
                .spawn(async move { decode_image_path(path).map_err(|error| error.to_string()) })
                .await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.apply_image_prepare_result(authority, decoded, cx);
            });
        })
        .detach();
    }

    fn apply_image_prepare_result(
        &mut self,
        authority: ImagePrepareAuthority,
        result: Result<DecodedImageFile, String>,
        cx: &mut Context<Self>,
    ) {
        let current = self.active_document_id == Some(authority.document_id)
            && self
                .session(authority.document_id, cx)
                .is_some_and(|session| {
                    let session = session.read(cx);
                    session.generation == authority.document_generation
                        && session.image_prepare_generation == authority.prepare_generation
                        && matches!(session.status, NativeDocumentStatus::Ready)
                        && session.save_status != NativeDocumentSaveStatus::Saving
                });
        if !current {
            self.rejected_stale_image_prepares =
                self.rejected_stale_image_prepares.saturating_add(1);
            cx.notify();
            return;
        }
        match result
            .and_then(|decoded| self.apply_decoded_image(authority.document_id, decoded, cx))
        {
            Ok(_) => {
                self.annotation_statuses.remove(&authority.document_id);
            }
            Err(error) => {
                self.annotation_statuses
                    .insert(authority.document_id, error);
            }
        }
        cx.notify();
    }

    fn begin_signature_selection(&mut self, document_id: DocumentId, cx: &mut Context<Self>) {
        if self.pending_text_box_editor.is_some() || self.pending_close_document_id.is_some() {
            return;
        }
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let authority = session.update(cx, |session, _| {
            if !matches!(session.status, NativeDocumentStatus::Ready)
                || session.save_status == NativeDocumentSaveStatus::Saving
            {
                return None;
            }
            session.image_prepare_generation = session.image_prepare_generation.saturating_add(1);
            Some(ImagePrepareAuthority {
                document_id,
                document_generation: session.generation,
                prepare_generation: session.image_prepare_generation,
            })
        });
        let Some(authority) = authority else {
            return;
        };
        self.drawn_signature.clear();
        self.signature_prepare_state = SignaturePrepareState::Loading;
        cx.notify();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select a PNG or JPEG image".into()),
        });
        let background = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            let selected = match picker.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.apply_signature_prepare_result(authority, None, cx);
                    });
                    return;
                }
                Ok(Err(error)) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.apply_signature_prepare_result(
                            authority,
                            Some(Err(format!("Could not open the image picker: {error}"))),
                            cx,
                        );
                    });
                    return;
                }
                Err(error) => {
                    let _ = entity.update(cx, |workspace, cx| {
                        workspace.apply_signature_prepare_result(
                            authority,
                            Some(Err(format!(
                                "The image picker closed unexpectedly: {error}"
                            ))),
                            cx,
                        );
                    });
                    return;
                }
            };
            let Some(path) = selected else {
                return;
            };
            let result = background
                .spawn(
                    async move { sanitize_signature_path(path).map_err(|error| error.to_string()) },
                )
                .await;
            let _ = entity.update(cx, |workspace, cx| {
                workspace.apply_signature_prepare_result(authority, Some(result), cx);
            });
        })
        .detach();
    }

    fn signature_authority_is_current(&self, authority: ImagePrepareAuthority, cx: &App) -> bool {
        self.signature_popover_open
            && self.active_document_id == Some(authority.document_id)
            && self
                .session(authority.document_id, cx)
                .is_some_and(|session| {
                    let session = session.read(cx);
                    session.generation == authority.document_generation
                        && session.image_prepare_generation == authority.prepare_generation
                        && matches!(session.status, NativeDocumentStatus::Ready)
                        && session.save_status != NativeDocumentSaveStatus::Saving
                })
    }

    fn apply_signature_prepare_result(
        &mut self,
        authority: ImagePrepareAuthority,
        result: Option<Result<SanitizedSignatureFile, String>>,
        cx: &mut Context<Self>,
    ) {
        if !self.signature_authority_is_current(authority, cx) {
            self.rejected_stale_image_prepares =
                self.rejected_stale_image_prepares.saturating_add(1);
            cx.notify();
            return;
        }
        self.signature_prepare_state = match result {
            None => SignaturePrepareState::Idle,
            Some(Err(error)) => SignaturePrepareState::Error(error),
            Some(Ok(sanitized)) => {
                let asset = sanitized.into_asset();
                let pixels = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
                    asset.width_px(),
                    asset.height_px(),
                    asset.rgba().to_vec(),
                );
                match pixels {
                    Some(pixels) => SignaturePrepareState::Preview(SignaturePreview {
                        asset,
                        image: Arc::new(RenderImage::new(smallvec::smallvec![Frame::new(pixels)])),
                    }),
                    None => SignaturePrepareState::Error("Unable to process this image.".into()),
                }
            }
        };
        cx.notify();
    }

    fn dismiss_signature_popover(
        &mut self,
        document_id: DocumentId,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.session(document_id, cx).cloned() {
            session.update(cx, |session, _| {
                session.image_prepare_generation =
                    session.image_prepare_generation.saturating_add(1);
            });
        }
        self.signature_popover_open = false;
        self.signature_prepare_state = SignaturePrepareState::Idle;
        self.drawn_signature.clear();
        if let Some(window) = window {
            self.workspace_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn arm_signature_placement(
        &mut self,
        document_id: DocumentId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let asset = match &self.signature_prepare_state {
            SignaturePrepareState::Preview(preview) => preview.asset.clone(),
            _ => self
                .drawn_signature
                .rasterize()
                .map_err(|error| error.to_string())?,
        };
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        let page_size = {
            let session = session.read(cx);
            if session.save_status == NativeDocumentSaveStatus::Saving {
                return Err("document save is in progress".to_owned());
            }
            session
                .annotation_page_geometry(session.current_page)
                .map(|(page_size, _)| page_size)
                .ok_or_else(|| "image placement page geometry is unavailable".to_owned())?
        };
        self.set_annotation_tool(document_id, AnnotationTool::Image, cx)?;
        session.update(cx, |session, cx| {
            session.annotations.set_signature_asset(asset);
            session
                .annotations
                .set_image_placement_page(f64::from(page_size.0), f64::from(page_size.1), 0.45)
                .map_err(|error| error.to_string())?;
            cx.notify();
            Ok::<(), String>(())
        })?;
        self.signature_popover_open = false;
        self.signature_prepare_state = SignaturePrepareState::Idle;
        self.drawn_signature.clear();
        self.annotation_statuses
            .insert(document_id, "Click the page to place the signature".into());
        self.workspace_focus.focus(window, cx);
        cx.notify();
        Ok(())
    }

    fn begin_drawn_signature_stroke(
        &mut self,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !bounds.contains(&position) {
            return false;
        }
        self.signature_prepare_state = SignaturePrepareState::Idle;
        let point = normalized_signature_point(position, bounds);
        if let Err(error) = self.drawn_signature.begin_stroke(point) {
            self.signature_prepare_state = SignaturePrepareState::Error(error.to_string());
            cx.notify();
            return false;
        }
        cx.notify();
        true
    }

    fn append_drawn_signature_point(
        &mut self,
        position: Point<Pixels>,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.drawn_signature.has_active_stroke() {
            return false;
        }
        let point = normalized_signature_point(position, bounds);
        if let Err(error) = self.drawn_signature.append_point(point) {
            self.signature_prepare_state = SignaturePrepareState::Error(error.to_string());
            self.drawn_signature.end_stroke();
        }
        cx.notify();
        true
    }

    fn end_drawn_signature_stroke(&mut self, cx: &mut Context<Self>) {
        self.drawn_signature.end_stroke();
        cx.notify();
    }

    fn clear_signature_input(&mut self, cx: &mut Context<Self>) {
        self.signature_prepare_state = SignaturePrepareState::Idle;
        self.drawn_signature.clear();
        cx.notify();
    }

    pub fn drawn_signature_point_count(&self) -> usize {
        self.drawn_signature.point_count()
    }

    pub fn drawn_signature(&self) -> &DrawnSignature {
        &self.drawn_signature
    }

    pub fn insert_image_at(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        path: &Path,
        id: MarkupId,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let asset_id = self.prepare_image_from_path(document_id, path, cx)?;
        let Some(session) = self.session(document_id, cx).cloned() else {
            return Err("document session is closed".into());
        };
        session.update(cx, |session, cx| {
            session.annotations.queue_next_annotation_id(id.clone());
            let outcome = session
                .annotations
                .pointer_down(document_id.value(), page_index, 5, point, 0.)
                .map_err(|error| error.to_string())?;
            if outcome != PointerPhaseOutcome::AnnotationCreated(id) {
                return Err(format!(
                    "image placement did not create its stable annotation: {outcome:?}"
                ));
            }
            let removed = session.sync_image_assets()?;
            defer_drop_images(removed, cx);
            cx.notify();
            Ok::<(), String>(())
        })?;
        cx.notify();
        Ok(asset_id)
    }

    fn select_available_annotation_tool(&mut self, tool: AnnotationTool, cx: &mut Context<Self>) {
        if self.pending_text_box_editor.is_some() || self.pending_close_document_id.is_some() {
            cx.propagate();
            return;
        }
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let available = self.session(document_id, cx).is_some_and(|session| {
            let session = session.read(cx);
            matches!(session.status, NativeDocumentStatus::Ready)
                && session.save_status != NativeDocumentSaveStatus::Saving
        });
        if !available {
            return;
        }
        self.annotation_stroke_menu_open = false;
        let _ = self.set_annotation_tool(document_id, tool, cx);
    }

    fn select_line_tool_from_action(
        &mut self,
        _: &SelectLineTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Line, cx);
    }

    fn select_arc_tool_from_action(
        &mut self,
        _: &SelectArcTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Arc, cx);
    }

    fn select_arrow_tool_from_action(
        &mut self,
        _: &SelectArrowTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Arrow, cx);
    }

    fn select_polyline_tool_from_action(
        &mut self,
        _: &SelectPolylineTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Polyline, cx);
    }

    fn select_polygon_tool_from_action(
        &mut self,
        _: &SelectPolygonTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Polygon, cx);
    }

    fn select_polylength_tool_from_action(
        &mut self,
        _: &SelectPolylengthTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Polylength, cx);
    }

    fn select_area_tool_from_action(
        &mut self,
        _: &SelectAreaTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Area, cx);
    }

    fn select_cloud_plus_tool_from_action(
        &mut self,
        _: &SelectCloudPlusTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::CloudPlus, cx);
    }

    fn select_dimension_tool_from_action(
        &mut self,
        _: &SelectDimensionTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Dimension, cx);
    }

    fn finish_vertex_path_from_action(
        &mut self,
        _: &FinishVertexPath,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let (vertex_path_pending, measurement_path_pending, cloud_pending, cloud_plus_pending) = {
            let session = session.read(cx);
            (
                session.annotations.vertex_path_pending(document_id.value()),
                session
                    .annotations
                    .measurement_path_pending(document_id.value()),
                session.annotations.cloud_pending(document_id.value()),
                session.annotations.cloud_plus_pending(document_id.value()),
            )
        };
        if !vertex_path_pending
            && !measurement_path_pending
            && !cloud_pending
            && !cloud_plus_pending
        {
            cx.propagate();
            return;
        }
        let outcome = session.update(cx, |session, cx| {
            let outcome = if measurement_path_pending {
                session
                    .annotations
                    .finish_measurement_path(document_id.value())
            } else if cloud_pending {
                session.annotations.finish_cloud(document_id.value())
            } else if cloud_plus_pending {
                session.annotations.finish_cloud_plus(document_id.value())
            } else {
                session.annotations.finish_vertex_path(document_id.value())
            };
            if matches!(outcome, Ok(PointerPhaseOutcome::AnnotationCreated(_))) {
                session.annotations.set_tool(AnnotationTool::Select)?;
            }
            cx.notify();
            outcome
        });
        let created_cloud_plus_id = if cloud_plus_pending {
            match &outcome {
                Ok(PointerPhaseOutcome::AnnotationCreated(id)) => Some(id.clone()),
                _ => None,
            }
        } else {
            None
        };
        if cloud_plus_pending
            && matches!(
                outcome,
                Ok(PointerPhaseOutcome::AnnotationCreated(_)) | Ok(PointerPhaseOutcome::Ignored)
            )
        {
            self.active_annotation_pointer = None;
        }
        match outcome {
            Ok(PointerPhaseOutcome::AnnotationCreated(_)) | Ok(PointerPhaseOutcome::Ignored) => {
                self.annotation_statuses.remove(&document_id);
            }
            Ok(_) => {}
            Err(error) => {
                self.annotation_statuses
                    .insert(document_id, error.to_string());
            }
        }
        if let Some(id) = created_cloud_plus_id {
            let page_index = session.read(cx).current_page;
            let _ = self.begin_pending_composite_text_editor(
                document_id,
                page_index,
                PendingTextEditorTarget::CloudPlus { id },
                window,
                cx,
            );
        }
        cx.notify();
    }

    fn select_length_tool_from_action(
        &mut self,
        _: &SelectLengthTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Length, cx);
    }

    fn select_highlight_tool_from_action(
        &mut self,
        _: &SelectHighlightTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_text_box_editor.is_some() || self.pending_close_document_id.is_some() {
            cx.propagate();
            return;
        }
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let available = self.session(document_id, cx).is_some_and(|session| {
            let session = session.read(cx);
            matches!(session.status, NativeDocumentStatus::Ready)
                && session.save_status != NativeDocumentSaveStatus::Saving
        });
        if !available {
            return;
        }
        self.annotation_stroke_menu_open = false;
        let _ = self.set_annotation_tool(document_id, AnnotationTool::Highlight, cx);
    }

    fn select_image_tool_from_action(
        &mut self,
        _: &SelectImageTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_text_box_editor.is_some() || self.pending_close_document_id.is_some() {
            cx.propagate();
            return;
        }
        let Some(document_id) = self.active_document_id else {
            return;
        };
        self.begin_image_selection(document_id, cx);
    }

    fn select_snapshot_tool_from_action(
        &mut self,
        _: &SelectSnapshotTool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_available_annotation_tool(AnnotationTool::Snapshot, cx);
    }

    fn select_all_annotations_from_action(
        &mut self,
        _: &SelectAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(page_index) = self
            .session(document_id, cx)
            .map(|session| session.read(cx).current_page())
        else {
            return;
        };
        self.select_all_annotations_on_page(document_id, page_index, cx);
    }

    fn copy_annotations_from_action(
        &mut self,
        _: &Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        if let Some(document_id) = self.active_document_id {
            self.copy_selected_annotations(document_id, cx);
        }
    }

    fn cut_annotations_from_action(
        &mut self,
        _: &Cut,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        if let Some(document_id) = self.active_document_id {
            if let Err(error) = self.cut_selected_annotations(document_id, cx) {
                self.annotation_statuses.insert(document_id, error);
                cx.notify();
            }
        }
    }

    fn paste_annotations_from_action(
        &mut self,
        _: &Paste,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        let Some(document_id) = self.active_document_id else {
            return;
        };
        let Some(page_index) = self
            .session(document_id, cx)
            .map(|session| session.read(cx).current_page())
        else {
            return;
        };
        if let Err(error) = self.paste_annotations(document_id, page_index, cx) {
            self.annotation_statuses.insert(document_id, error);
            cx.notify();
        }
    }

    fn delete_annotations_from_action(
        &mut self,
        _: &Delete,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        if let Some(document_id) = self.active_document_id {
            let _ = self.delete_selected_annotation(document_id, cx);
        }
    }

    fn undo_annotations_from_action(
        &mut self,
        _: &Undo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        if let Some(document_id) = self.active_document_id {
            let _ = self.undo_annotations(document_id, cx);
        }
    }

    fn redo_annotations_from_action(
        &mut self,
        _: &Redo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_focused_input(cx) {
            return;
        }
        if let Some(document_id) = self.active_document_id {
            let _ = self.redo_annotations(document_id, cx);
        }
    }

    fn record_page_interaction(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        bounds: Bounds<Pixels>,
        transform: PageTransform,
        source_pdf_page_size_points: (f32, f32),
        painted_viewer: Option<PaintedViewerAuthority>,
    ) {
        let painted_evidence = painted_viewer
            .filter(|authority| authority.rendered_dpr.is_finite() && authority.rendered_dpr > 0.)
            .map(|authority| {
                let previous = self
                    .last_painted_page_evidence
                    .get(&(document_id, page_index))
                    .copied();
                let painted_state_sequence = previous
                    .filter(|previous| {
                        previous.source_pdf_page_size_points == source_pdf_page_size_points
                            && previous.contained_bounds == bounds
                            && previous.viewer_generation == authority.viewer_generation
                            && previous.request_generation == authority.request_generation
                            && previous.resource_generation == authority.resource_generation
                            && previous.rendered_dpr == authority.rendered_dpr
                    })
                    .map_or_else(
                        || {
                            let next = self.next_painted_state_sequence;
                            self.next_painted_state_sequence = self
                                .next_painted_state_sequence
                                .checked_add(1)
                                .expect("painted page state sequence exhausted");
                            next
                        },
                        |previous| previous.painted_state_sequence,
                    );
                let evidence = PaintedPageEvidence {
                    document_id,
                    page_index,
                    source_pdf_page_size_points,
                    contained_bounds: bounds,
                    viewer_generation: authority.viewer_generation,
                    request_generation: authority.request_generation,
                    resource_generation: authority.resource_generation,
                    painted_state_sequence,
                    rendered_dpr: authority.rendered_dpr,
                };
                self.last_painted_page_evidence
                    .insert((document_id, page_index), evidence);
                evidence
            });
        self.page_interactions.insert(
            (document_id, page_index),
            PageInteraction {
                document_id,
                page_index,
                bounds,
                transform,
                painted_evidence,
            },
        );
    }

    fn interaction_point(
        interaction: PageInteraction,
        position: gpui::Point<Pixels>,
        require_inside: bool,
    ) -> Option<PdfPoint> {
        if require_inside && !interaction.bounds.contains(&position) {
            return None;
        }
        interaction
            .transform
            .point_from_local_pixels(
                f64::from(f32::from(position.x - interaction.bounds.origin.x)),
                f64::from(f32::from(position.y - interaction.bounds.origin.y)),
            )
            .ok()
    }

    fn interaction_viewport_point(
        interaction: PageInteraction,
        position: gpui::Point<Pixels>,
    ) -> SelectionPoint {
        SelectionPoint::new(
            f64::from(f32::from(position.x - interaction.bounds.origin.x)),
            f64::from(f32::from(position.y - interaction.bounds.origin.y)),
        )
    }

    fn begin_annotation_pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
        click_count: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.pending_text_box_editor.is_some()
            || self.pending_close_document_id.is_some()
            || self
                .active_document_id
                .and_then(|document_id| self.session(document_id, cx))
                .is_some_and(|session| session.read(cx).pending_rotation_generation.is_some())
        {
            return false;
        }
        let Some(interaction) = self
            .page_interactions
            .values()
            .copied()
            .find(|interaction| {
                Some(interaction.document_id) == self.active_document_id
                    && interaction.bounds.contains(&position)
            })
        else {
            return false;
        };
        let Some(point) = Self::interaction_point(interaction, position, true) else {
            return false;
        };
        let viewport_point = Self::interaction_viewport_point(interaction, position);
        if self
            .page_scale_control
            .as_ref()
            .is_some_and(|control| control.read(cx).is_picking())
        {
            let control = self
                .page_scale_control
                .as_ref()
                .expect("the picking calibration control is retained")
                .clone();
            let disposition = control.update(cx, |control, cx| {
                control.record_point(
                    interaction.document_id,
                    interaction.page_index,
                    point,
                    modifiers.shift,
                    window,
                    cx,
                )
            });
            if disposition == CalibrationPointDisposition::Ignored {
                self.annotation_statuses.insert(
                    interaction.document_id,
                    "Pick both calibration points on the selected page.".into(),
                );
            } else {
                self.annotation_statuses.remove(&interaction.document_id);
            }
            cx.notify();
            return true;
        }
        let Some(session) = self.session(interaction.document_id, cx).cloned() else {
            return false;
        };
        if !matches!(session.read(cx).status, NativeDocumentStatus::Ready)
            || session.read(cx).save_status == NativeDocumentSaveStatus::Saving
            || self.pending_save_prompt.is_some_and(|pending| {
                pending.document_id == interaction.document_id
                    && pending.document_generation == session.read(cx).generation
            })
        {
            return false;
        }
        let tool = session.read(cx).annotations.tool();
        if matches!(tool, AnnotationTool::Polylength | AnnotationTool::Area)
            && click_count >= 2
            && session
                .read(cx)
                .annotations
                .measurement_path_pending(interaction.document_id.value())
        {
            let tolerance = match interaction.transform.tolerance_points(4.) {
                Ok(tolerance) => tolerance,
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                    cx.notify();
                    return true;
                }
            };
            let outcome = session.update(cx, |session, cx| {
                let outcome = session.annotations.pointer_double_click(
                    interaction.document_id.value(),
                    interaction.page_index,
                    point,
                    tolerance,
                )?;
                if matches!(outcome, PointerPhaseOutcome::AnnotationCreated(_)) {
                    session.annotations.set_tool(AnnotationTool::Select)?;
                }
                cx.notify();
                Ok::<PointerPhaseOutcome, AnnotationError>(outcome)
            });
            match outcome {
                Ok(PointerPhaseOutcome::AnnotationCreated(_)) => {
                    self.annotation_statuses.remove(&interaction.document_id);
                }
                Ok(PointerPhaseOutcome::Ignored) => {}
                Ok(_) => {}
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                }
            }
            cx.notify();
            return true;
        }
        if tool == AnnotationTool::Cloud
            && click_count >= 2
            && session
                .read(cx)
                .annotations
                .cloud_pending(interaction.document_id.value())
        {
            let outcome = session.update(cx, |session, cx| {
                session.annotations.pointer_down(
                    interaction.document_id.value(),
                    interaction.page_index,
                    self.next_pointer_id,
                    point,
                    interaction.transform.tolerance_points(4.)?,
                )?;
                let outcome = session
                    .annotations
                    .finish_cloud(interaction.document_id.value())?;
                if matches!(outcome, PointerPhaseOutcome::AnnotationCreated(_)) {
                    session.annotations.set_tool(AnnotationTool::Select)?;
                }
                cx.notify();
                Ok::<PointerPhaseOutcome, AnnotationError>(outcome)
            });
            match outcome {
                Ok(PointerPhaseOutcome::AnnotationCreated(_)) => {
                    self.annotation_statuses.remove(&interaction.document_id);
                }
                Ok(_) => {}
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                }
            }
            cx.notify();
            return true;
        }
        if tool == AnnotationTool::CloudPlus
            && click_count >= 2
            && session
                .read(cx)
                .annotations
                .cloud_plus_pending(interaction.document_id.value())
        {
            let tolerance = match interaction.transform.tolerance_points(4.) {
                Ok(tolerance) => tolerance,
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                    cx.notify();
                    return true;
                }
            };
            let outcome = session.update(cx, |session, cx| {
                let outcome = session.annotations.pointer_double_click(
                    interaction.document_id.value(),
                    interaction.page_index,
                    point,
                    tolerance,
                )?;
                if matches!(outcome, PointerPhaseOutcome::AnnotationCreated(_)) {
                    session.annotations.set_tool(AnnotationTool::Select)?;
                }
                cx.notify();
                Ok::<PointerPhaseOutcome, AnnotationError>(outcome)
            });
            match outcome {
                Ok(PointerPhaseOutcome::AnnotationCreated(id)) => {
                    self.active_annotation_pointer = None;
                    self.annotation_statuses.remove(&interaction.document_id);
                    let _ = self.begin_pending_composite_text_editor(
                        interaction.document_id,
                        interaction.page_index,
                        PendingTextEditorTarget::CloudPlus { id },
                        window,
                        cx,
                    );
                }
                Ok(PointerPhaseOutcome::Ignored) => {
                    self.active_annotation_pointer = None;
                }
                Ok(_) => {}
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                }
            }
            cx.notify();
            return true;
        }
        if tool == AnnotationTool::Select && click_count >= 2 {
            let tolerance = match interaction.transform.tolerance_points(4.) {
                Ok(tolerance) => tolerance,
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                    cx.notify();
                    return true;
                }
            };
            let rectangle_hit = session
                .read(cx)
                .annotations
                .hit_rectangle_id(
                    interaction.document_id.value(),
                    interaction.page_index,
                    point,
                    tolerance,
                )
                .ok()
                .flatten();
            if let Some(id) = rectangle_hit {
                let already_selected = session
                    .read(cx)
                    .annotations
                    .selected_rectangle(interaction.document_id.value())
                    .is_some_and(|rectangle| rectangle.id == id);
                let selected = session.update(cx, |session, cx| {
                    let selected = session
                        .annotations
                        .select_id(interaction.document_id.value(), &id);
                    cx.notify();
                    selected
                });
                if selected {
                    let inspector = self.ensure_rectangle_property_inspector(window, cx);
                    inspector.update(cx, |inspector, cx| {
                        if already_selected && inspector.is_open() {
                            inspector.close(cx);
                        } else {
                            inspector.open(cx);
                        }
                    });
                    self.annotation_statuses.remove(&interaction.document_id);
                    cx.notify();
                    return true;
                }
            }
            let outcome = session.update(cx, |session, cx| {
                session
                    .annotations
                    .set_observed_pixels_per_point(interaction.transform.pixels_per_point())
                    .map_err(|error| error.to_string())?;
                let outcome = session
                    .annotations
                    .pointer_double_click(
                        interaction.document_id.value(),
                        interaction.page_index,
                        point,
                        tolerance,
                    )
                    .map_err(|error| error.to_string())?;
                cx.notify();
                Ok::<PointerPhaseOutcome, String>(outcome)
            });
            match outcome {
                Ok(PointerPhaseOutcome::AnnotationEdited(_)) => {
                    self.annotation_statuses.remove(&interaction.document_id);
                    cx.notify();
                    return true;
                }
                Ok(PointerPhaseOutcome::SelectionChanged(Some(id))) => {
                    self.active_annotation_pointer = None;
                    self.annotation_statuses.remove(&interaction.document_id);
                    let target = if session
                        .read(cx)
                        .annotations
                        .exact_selected_dimension(interaction.document_id.value())
                        .is_some()
                    {
                        PendingTextEditorTarget::ExistingDimension { id }
                    } else {
                        PendingTextEditorTarget::ExistingTextBox { id }
                    };
                    let _ = self.begin_pending_composite_text_editor(
                        interaction.document_id,
                        interaction.page_index,
                        target,
                        window,
                        cx,
                    );
                    cx.notify();
                    return true;
                }
                Ok(PointerPhaseOutcome::Ignored) => {}
                Ok(_) => {
                    return true;
                }
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error);
                    cx.notify();
                    return true;
                }
            }
        }
        if tool == AnnotationTool::TextBox {
            return self.begin_pending_text_box(
                interaction.document_id,
                interaction.page_index,
                point,
                window,
                cx,
            );
        }
        if tool == AnnotationTool::Length {
            if session
                .read(cx)
                .annotations
                .length_placement_pending(interaction.document_id.value())
            {
                let result = session.update(cx, |session, cx| {
                    let outcome = session.annotations.commit_length_placement(
                        interaction.document_id.value(),
                        interaction.page_index,
                        point,
                        modifiers.shift,
                    )?;
                    session.annotations.clear_semantic_snap_decision();
                    cx.notify();
                    Ok::<PointerPhaseOutcome, AnnotationError>(outcome)
                });
                match result {
                    Ok(_) => {
                        self.annotation_statuses.remove(&interaction.document_id);
                    }
                    Err(error) => {
                        self.annotation_statuses
                            .insert(interaction.document_id, error.to_string());
                    }
                }
                cx.notify();
                return true;
            }

            let annotation_sequence = self.next_annotation_sequence;
            let Ok(id) = MarkupId::new(format!("workspace:length:{annotation_sequence}")) else {
                return false;
            };
            let result = session.update(cx, |session, cx| {
                let outcome = session.annotations.begin_length_placement(
                    interaction.document_id.value(),
                    interaction.page_index,
                    id,
                    point,
                )?;
                cx.notify();
                Ok::<PointerPhaseOutcome, AnnotationError>(outcome)
            });
            match result {
                Ok(PointerPhaseOutcome::PlacementPending) => {
                    self.next_annotation_sequence = self.next_annotation_sequence.saturating_add(1);
                    self.annotation_statuses.remove(&interaction.document_id);
                }
                Ok(_) => {}
                Err(AnnotationError::InvalidGeometry(message))
                    if message == LENGTH_SCALE_REQUIRED_MESSAGE =>
                {
                    self.annotation_statuses
                        .insert(interaction.document_id, message);
                }
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                }
            }
            cx.notify();
            return true;
        }
        if tool == AnnotationTool::Dimension {
            if session
                .read(cx)
                .annotations
                .dimension_placement_pending(interaction.document_id.value())
            {
                let result = session.update(cx, |session, cx| {
                    let outcome = session.annotations.commit_dimension_placement(
                        interaction.document_id.value(),
                        interaction.page_index,
                        point,
                        modifiers.shift,
                    )?;
                    session.annotations.clear_semantic_snap_decision();
                    cx.notify();
                    Ok::<PointerPhaseOutcome, AnnotationError>(outcome)
                });
                match result {
                    Ok(PointerPhaseOutcome::AnnotationCreated(id)) => {
                        self.annotation_statuses.remove(&interaction.document_id);
                        let _ = self.begin_pending_composite_text_editor(
                            interaction.document_id,
                            interaction.page_index,
                            PendingTextEditorTarget::NewDimension { id },
                            window,
                            cx,
                        );
                    }
                    Ok(PointerPhaseOutcome::Ignored) => {
                        self.annotation_statuses.remove(&interaction.document_id);
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.annotation_statuses
                            .insert(interaction.document_id, error.to_string());
                    }
                }
                cx.notify();
                return true;
            }

            let annotation_sequence = self.next_annotation_sequence;
            let Ok(id) = MarkupId::new(format!("workspace:dimension:{annotation_sequence}")) else {
                return false;
            };
            let result = session.update(cx, |session, cx| {
                let outcome = session.annotations.begin_dimension_placement(
                    interaction.document_id.value(),
                    interaction.page_index,
                    id,
                    point,
                )?;
                cx.notify();
                Ok::<PointerPhaseOutcome, AnnotationError>(outcome)
            });
            match result {
                Ok(PointerPhaseOutcome::PlacementPending) => {
                    self.next_annotation_sequence = self.next_annotation_sequence.saturating_add(1);
                    self.annotation_statuses.remove(&interaction.document_id);
                }
                Ok(_) => {}
                Err(error) => {
                    self.annotation_statuses
                        .insert(interaction.document_id, error.to_string());
                }
            }
            cx.notify();
            return true;
        }
        if !matches!(
            tool,
            AnnotationTool::Rectangle
                | AnnotationTool::Ellipse
                | AnnotationTool::Arc
                | AnnotationTool::Line
                | AnnotationTool::Arrow
                | AnnotationTool::Polyline
                | AnnotationTool::Polygon
                | AnnotationTool::Polylength
                | AnnotationTool::Area
                | AnnotationTool::Cloud
                | AnnotationTool::CloudPlus
                | AnnotationTool::Callout
                | AnnotationTool::Redact
                | AnnotationTool::Pen
                | AnnotationTool::Highlight
                | AnnotationTool::Image
                | AnnotationTool::Snapshot
                | AnnotationTool::Select
        ) {
            return false;
        }
        let retained_pointer = self.active_annotation_pointer.filter(|active| {
            active.placement_pending
                && active.document_id == interaction.document_id
                && active.page_index == interaction.page_index
        });
        if self.active_annotation_pointer.is_some() && retained_pointer.is_none() {
            return false;
        }
        let pointer_id = retained_pointer.map_or_else(
            || {
                let pointer_id = self.next_pointer_id;
                self.next_pointer_id = self.next_pointer_id.saturating_add(1);
                pointer_id
            },
            |active| active.pointer_id,
        );
        let path_draft_pending = {
            let session = session.read(cx);
            session
                .annotations
                .vertex_path_pending(interaction.document_id.value())
                || session
                    .annotations
                    .measurement_path_pending(interaction.document_id.value())
                || session
                    .annotations
                    .cloud_pending(interaction.document_id.value())
                || session
                    .annotations
                    .cloud_plus_pending(interaction.document_id.value())
                || session
                    .annotations
                    .arc_placement_pending(interaction.document_id.value())
                || session
                    .annotations
                    .snapshot_placement_pending(interaction.document_id.value())
        };
        let next_annotation_id = if retained_pointer.is_none()
            && !path_draft_pending
            && matches!(
                tool,
                AnnotationTool::Rectangle
                    | AnnotationTool::Ellipse
                    | AnnotationTool::Arc
                    | AnnotationTool::Line
                    | AnnotationTool::Arrow
                    | AnnotationTool::Polyline
                    | AnnotationTool::Polygon
                    | AnnotationTool::Polylength
                    | AnnotationTool::Area
                    | AnnotationTool::Cloud
                    | AnnotationTool::CloudPlus
                    | AnnotationTool::Callout
                    | AnnotationTool::Redact
                    | AnnotationTool::Pen
                    | AnnotationTool::Highlight
                    | AnnotationTool::Image
                    | AnnotationTool::Snapshot
            ) {
            let annotation_sequence = self.next_annotation_sequence;
            self.next_annotation_sequence = self.next_annotation_sequence.saturating_add(1);
            let family = match tool {
                AnnotationTool::Rectangle => "rectangle",
                AnnotationTool::Ellipse => "ellipse",
                AnnotationTool::Arc => "arc",
                AnnotationTool::Line => "line",
                AnnotationTool::Arrow => "arrow",
                AnnotationTool::Polyline => "polyline",
                AnnotationTool::Polygon => "polygon",
                AnnotationTool::Polylength => "polylength",
                AnnotationTool::Area => "area",
                AnnotationTool::Cloud => "cloud",
                AnnotationTool::CloudPlus => "cloud-plus",
                AnnotationTool::Callout => "callout",
                AnnotationTool::Redact => "redact",
                AnnotationTool::Pen => "pen",
                AnnotationTool::Highlight => "highlight",
                AnnotationTool::Image => "image",
                AnnotationTool::Snapshot => "snapshot",
                _ => unreachable!("only drawing tools allocate workspace IDs"),
            };
            let Ok(id) = MarkupId::new(format!("workspace:{family}:{annotation_sequence}")) else {
                return false;
            };
            Some(id)
        } else {
            None
        };
        let snapshot_capture = if tool == AnnotationTool::Snapshot {
            let session = session.read(cx);
            session
                .annotations
                .snapshot_pending_rect_to(
                    interaction.document_id.value(),
                    interaction.page_index,
                    point,
                )
                .filter(|rect| rect.width > 2. && rect.height > 2.)
                .map(|rect| {
                    let coordinate_space = session
                        .annotation_page_coordinate_space(interaction.page_index)
                        .ok_or_else(|| {
                            "Snapshot page coordinate space is unavailable".to_owned()
                        })?;
                    session
                        .current_base_raster()
                        .ok_or_else(|| "Snapshot base raster is unavailable".to_owned())?
                        .snapshot_asset(rect, coordinate_space)
                })
                .transpose()
        } else {
            Ok(None)
        };
        let snapshot_capture = match snapshot_capture {
            Ok(capture) => capture,
            Err(error) => {
                self.annotation_statuses
                    .insert(interaction.document_id, error);
                cx.notify();
                return true;
            }
        };
        let accepted = session
            .update(cx, |session, cx| {
                session
                    .annotations
                    .set_observed_pixels_per_point(interaction.transform.pixels_per_point())
                    .map_err(|error| error.to_string())?;
                if let Some(id) = next_annotation_id {
                    session.annotations.queue_next_annotation_id(id);
                }
                if let Some(asset) = snapshot_capture {
                    session.annotations.set_snapshot_capture_asset(asset);
                }
                let outcome = session
                    .annotations
                    .pointer_down_with_viewport_input(
                        interaction.document_id.value(),
                        interaction.page_index,
                        pointer_id,
                        0,
                        point,
                        viewport_point,
                        interaction
                            .transform
                            .tolerance_points(4.)
                            .map_err(|error| error.to_string())?,
                        PointerInputModifiers {
                            shift: modifiers.shift,
                            alt: modifiers.alt,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                cx.notify();
                Ok::<PointerPhaseOutcome, String>(outcome)
            })
            .ok();
        if matches!(
            accepted.as_ref(),
            Some(PointerPhaseOutcome::GestureStarted) | Some(PointerPhaseOutcome::PlacementPending)
        ) {
            self.active_annotation_pointer = Some(ActiveAnnotationPointer {
                document_id: interaction.document_id,
                page_index: interaction.page_index,
                pointer_id,
                placement_pending: matches!(
                    accepted.as_ref(),
                    Some(PointerPhaseOutcome::PlacementPending)
                ),
            });
            cx.notify();
        }
        if retained_pointer.is_some()
            && matches!(accepted, Some(PointerPhaseOutcome::SelectionChanged(_)))
        {
            self.active_annotation_pointer = None;
            cx.notify();
        }
        if let Some(PointerPhaseOutcome::AnnotationCreated(created_id)) = accepted.clone() {
            self.active_annotation_pointer = None;
            let _ = session.update(cx, |session, cx| {
                session.annotations.clear_semantic_snap_decision();
                if matches!(
                    tool,
                    AnnotationTool::Polyline
                        | AnnotationTool::Polygon
                        | AnnotationTool::Polylength
                        | AnnotationTool::Area
                        | AnnotationTool::CloudPlus
                        | AnnotationTool::Image
                ) {
                    session
                        .annotations
                        .set_tool(AnnotationTool::Select)
                        .map_err(|error| error.to_string())?;
                }
                let removed = session.sync_image_assets()?;
                defer_drop_images(removed, cx);
                cx.notify();
                Ok::<(), String>(())
            });
            if matches!(tool, AnnotationTool::Callout | AnnotationTool::CloudPlus) {
                let target = match tool {
                    AnnotationTool::Callout => PendingTextEditorTarget::Callout {
                        id: created_id.clone(),
                    },
                    AnnotationTool::CloudPlus => PendingTextEditorTarget::CloudPlus {
                        id: created_id.clone(),
                    },
                    _ => unreachable!("only composite text tools open the retained editor"),
                };
                let _ = self.begin_pending_composite_text_editor(
                    interaction.document_id,
                    interaction.page_index,
                    target,
                    window,
                    cx,
                );
            }
            if tool == AnnotationTool::Image {
                self.annotation_statuses.remove(&interaction.document_id);
            }
            cx.notify();
            return true;
        }
        accepted.is_some_and(|outcome| {
            matches!(
                outcome,
                PointerPhaseOutcome::GestureStarted
                    | PointerPhaseOutcome::PlacementPending
                    | PointerPhaseOutcome::SelectionChanged(_)
            )
        })
    }

    fn update_annotation_hover(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(interaction) = self
            .page_interactions
            .values()
            .copied()
            .find(|interaction| {
                Some(interaction.document_id) == self.active_document_id
                    && interaction.bounds.contains(&position)
            })
        else {
            return false;
        };
        let Some(point) = Self::interaction_point(interaction, position, true) else {
            return false;
        };
        let Some(session) = self.session(interaction.document_id, cx).cloned() else {
            return false;
        };
        let length_pending = session
            .read(cx)
            .annotations
            .length_placement_pending(interaction.document_id.value());
        let dimension_pending = session
            .read(cx)
            .annotations
            .dimension_placement_pending(interaction.document_id.value());
        let vertex_path_pending = session
            .read(cx)
            .annotations
            .vertex_path_pending(interaction.document_id.value());
        let measurement_path_pending = session
            .read(cx)
            .annotations
            .measurement_path_pending(interaction.document_id.value());
        let cloud_pending = session
            .read(cx)
            .annotations
            .cloud_pending(interaction.document_id.value());
        let cloud_plus_pending = session
            .read(cx)
            .annotations
            .cloud_plus_pending(interaction.document_id.value());
        let arc_pending = session
            .read(cx)
            .annotations
            .arc_placement_pending(interaction.document_id.value());
        if !length_pending
            && !dimension_pending
            && !vertex_path_pending
            && !measurement_path_pending
            && !cloud_pending
            && !cloud_plus_pending
            && !arc_pending
        {
            return false;
        }
        let updated = session
            .update(cx, |session, cx| {
                if length_pending {
                    session
                        .annotations
                        .update_length_placement(point, modifiers.shift)
                        .map_err(|error| error.to_string())?;
                } else if dimension_pending {
                    session
                        .annotations
                        .update_dimension_placement(point, modifiers.shift)
                        .map_err(|error| error.to_string())?;
                } else if vertex_path_pending {
                    session
                        .annotations
                        .update_vertex_path_hover(
                            interaction.document_id.value(),
                            interaction.page_index,
                            point,
                        )
                        .map_err(|error| error.to_string())?;
                } else if cloud_pending {
                    session
                        .annotations
                        .update_cloud_hover(
                            interaction.document_id.value(),
                            interaction.page_index,
                            point,
                        )
                        .map_err(|error| error.to_string())?;
                } else if cloud_plus_pending {
                    session
                        .annotations
                        .update_cloud_plus_hover(
                            interaction.document_id.value(),
                            interaction.page_index,
                            point,
                        )
                        .map_err(|error| error.to_string())?;
                } else if arc_pending {
                    session
                        .annotations
                        .update_arc_hover(
                            interaction.document_id.value(),
                            interaction.page_index,
                            point,
                            modifiers.shift,
                        )
                        .map_err(|error| error.to_string())?;
                } else {
                    session
                        .annotations
                        .update_measurement_path_hover(
                            interaction.document_id.value(),
                            interaction.page_index,
                            point,
                        )
                        .map_err(|error| error.to_string())?;
                }
                cx.notify();
                Ok::<(), String>(())
            })
            .is_ok();
        if updated {
            cx.notify();
        }
        updated
    }

    fn update_annotation_pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = self.active_annotation_pointer else {
            return false;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(active.document_id, active.page_index))
            .copied()
        else {
            return false;
        };
        let Some(point) = Self::interaction_point(interaction, position, false) else {
            return false;
        };
        let viewport_point = Self::interaction_viewport_point(interaction, position);
        let Some(session) = self.session(active.document_id, cx).cloned() else {
            self.active_annotation_pointer = None;
            return false;
        };
        let updated = session
            .update(cx, |session, cx| {
                session
                    .annotations
                    .pointer_move_with_viewport_input(
                        active.pointer_id,
                        point,
                        viewport_point,
                        PointerInputModifiers {
                            shift: modifiers.shift,
                            alt: modifiers.alt,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                cx.notify();
                Ok::<(), String>(())
            })
            .is_ok();
        if updated {
            cx.notify();
        }
        updated
    }

    fn finish_annotation_pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = self.active_annotation_pointer.take() else {
            return false;
        };
        let Some(interaction) = self
            .page_interactions
            .get(&(active.document_id, active.page_index))
            .copied()
        else {
            self.cancel_retained_annotation_pointer(active, cx);
            return false;
        };
        let Some(point) = Self::interaction_point(interaction, position, false) else {
            self.cancel_retained_annotation_pointer(active, cx);
            return false;
        };
        let viewport_point = Self::interaction_viewport_point(interaction, position);
        let Some(session) = self.session(active.document_id, cx).cloned() else {
            return false;
        };
        let commit_blocked = self.active_document_id != Some(active.document_id)
            || self.pending_text_box_editor.is_some()
            || self.pending_close_document_id == Some(active.document_id)
            || self.close_after_save_document_id == Some(active.document_id)
            || !matches!(session.read(cx).status, NativeDocumentStatus::Ready)
            || session.read(cx).save_status == NativeDocumentSaveStatus::Saving
            || session.read(cx).pending_rotation_generation.is_some()
            || self.pending_save_prompt.is_some_and(|pending| {
                pending.document_id == active.document_id
                    && pending.document_generation == session.read(cx).generation
            });
        if commit_blocked {
            self.cancel_retained_annotation_pointer(active, cx);
            return false;
        }
        let tool = session.read(cx).annotations.tool();
        let outcome = session
            .update(cx, |session, cx| {
                let outcome = session
                    .annotations
                    .pointer_up_with_viewport_input(
                        active.pointer_id,
                        point,
                        viewport_point,
                        PointerInputModifiers {
                            shift: modifiers.shift,
                            alt: modifiers.alt,
                        },
                    )
                    .map_err(|error| error.to_string())?;
                if outcome != PointerPhaseOutcome::PlacementPending {
                    session.annotations.clear_semantic_snap_decision();
                }
                cx.notify();
                let created_highlight =
                    matches!(outcome, PointerPhaseOutcome::AnnotationCreated(_))
                        && session.annotations.tool() == AnnotationTool::Highlight;
                let created_cloud_plus =
                    matches!(outcome, PointerPhaseOutcome::AnnotationCreated(_))
                        && session.annotations.tool() == AnnotationTool::CloudPlus;
                if created_highlight || created_cloud_plus {
                    session
                        .annotations
                        .set_tool(AnnotationTool::Select)
                        .map_err(|error| error.to_string())?;
                }
                if matches!(
                    outcome,
                    PointerPhaseOutcome::AnnotationCreated(_)
                        | PointerPhaseOutcome::AnnotationEdited(_)
                ) {
                    session.rebuild_stable_highlight_presentations()?;
                }
                Ok::<PointerPhaseOutcome, String>(outcome)
            })
            .ok();
        if tool == AnnotationTool::CloudPlus
            && let Some(PointerPhaseOutcome::AnnotationCreated(id)) = outcome.as_ref()
        {
            let _ = self.begin_pending_composite_text_editor(
                active.document_id,
                active.page_index,
                PendingTextEditorTarget::CloudPlus { id: id.clone() },
                window,
                cx,
            );
        }
        if outcome == Some(PointerPhaseOutcome::PlacementPending) {
            self.active_annotation_pointer = Some(ActiveAnnotationPointer {
                placement_pending: true,
                ..active
            });
        }
        cx.notify();
        outcome.is_some_and(|outcome| {
            matches!(
                outcome,
                PointerPhaseOutcome::AnnotationCreated(_)
                    | PointerPhaseOutcome::AnnotationEdited(_)
                    | PointerPhaseOutcome::SelectionChanged(_)
                    | PointerPhaseOutcome::PlacementPending
            )
        })
    }

    fn cancel_retained_annotation_pointer(
        &mut self,
        active: ActiveAnnotationPointer,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = self.session(active.document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                let _ = session.annotations.cancel(PointerCancelReason::CaptureLost);
                cx.notify();
            });
        }
        cx.notify();
    }

    fn cancel_active_annotation_pointer(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(active) = self.active_annotation_pointer.take() else {
            return false;
        };
        self.cancel_retained_annotation_pointer(active, cx);
        cx.notify();
        true
    }

    fn cancel_annotation_pointer(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(active) = self.active_annotation_pointer.take() else {
            return false;
        };
        if let Some(session) = self.session(active.document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                let _ = session.annotations.cancel(PointerCancelReason::CaptureLost);
                cx.notify();
            });
        }
        cx.notify();
        true
    }

    pub fn activate_document(&mut self, document_id: DocumentId, cx: &mut Context<Self>) -> bool {
        let ready = self
            .session(document_id, cx)
            .is_some_and(|session| matches!(session.read(cx).status, NativeDocumentStatus::Ready));
        if !ready {
            return false;
        }
        if self.active_document_id != Some(document_id) {
            if self.signature_popover_open
                && let Some(previous) = self.active_document_id
            {
                self.dismiss_signature_popover(previous, None, cx);
            }
            if let (Some(previous), Some(control)) =
                (self.active_document_id, self.page_scale_control.clone())
            {
                control.update(cx, |control, cx| {
                    control.cancel_for_document(previous, cx);
                });
            }
            self.cancel_annotation_pointer(cx);
            self.active_document_id = Some(document_id);
            cx.notify();
        }
        self.sync_active_viewer_toolbar(cx);
        true
    }

    pub fn request_close_document(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> CloseRequestDisposition {
        if self
            .pending_text_box_editor
            .as_ref()
            .is_some_and(|editor| editor.document_id == document_id)
            && let Err(error) = self.commit_pending_text_box(cx)
        {
            self.text_box_commit_error = Some(error);
            cx.notify();
            return CloseRequestDisposition::ConfirmationRequired;
        }
        let Some(session) = self.session(document_id, cx) else {
            return CloseRequestDisposition::NotFound;
        };
        if session.read(cx).is_dirty() {
            if self.pending_close_document_id.is_some() {
                return CloseRequestDisposition::ConfirmationRequired;
            }
            self.pending_close_document_id = Some(document_id);
            cx.notify();
            CloseRequestDisposition::ConfirmationRequired
        } else {
            if self.close_document(document_id, cx) {
                CloseRequestDisposition::Closed
            } else {
                CloseRequestDisposition::ReleaseFailed
            }
        }
    }

    pub(crate) fn commit_pending_text_editor_before_application_close(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if self.pending_text_box_editor.is_some() {
            if let Err(error) = self.commit_pending_text_box(cx) {
                self.text_box_commit_error = Some(error.clone());
                cx.notify();
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn resolve_dirty_close_cancel(&mut self, cx: &mut Context<Self>) -> DirtyCloseResolution {
        if self.pending_close_document_id.take().is_none() {
            return DirtyCloseResolution::NoPendingDocument;
        }
        self.close_after_save_document_id = None;
        cx.notify();
        DirtyCloseResolution::Cancelled
    }

    pub fn resolve_dirty_close_discard(&mut self, cx: &mut Context<Self>) -> DirtyCloseResolution {
        let Some(document_id) = self.pending_close_document_id else {
            return DirtyCloseResolution::NoPendingDocument;
        };
        if self.close_document(document_id, cx) {
            DirtyCloseResolution::Discarded
        } else {
            DirtyCloseResolution::ReleaseFailed
        }
    }

    pub fn begin_dirty_close_save_as(
        &mut self,
        target_path: PathBuf,
        cx: &mut Context<Self>,
    ) -> Result<SaveDocumentRequest, String> {
        let document_id = self
            .pending_close_document_id
            .ok_or_else(|| "no dirty document is awaiting close confirmation".to_owned())?;
        let request = self.begin_save_as(document_id, target_path, cx)?;
        self.close_after_save_document_id = Some(document_id);
        cx.notify();
        Ok(request)
    }

    pub fn begin_dirty_close_save(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<SaveDocumentRequest, String> {
        let document_id = self
            .pending_close_document_id
            .ok_or_else(|| "no dirty document is awaiting close confirmation".to_owned())?;
        let request = self.begin_save(document_id, cx)?;
        self.close_after_save_document_id = Some(document_id);
        cx.notify();
        Ok(request)
    }

    pub fn close_document(&mut self, document_id: DocumentId, cx: &mut Context<Self>) -> bool {
        match self.close_document_checked(document_id, cx) {
            Ok(closed) => closed,
            Err(error) => {
                self.last_file_error = Some(format!("Failed to close document: {error}"));
                cx.notify();
                false
            }
        }
    }

    pub fn close_document_checked(
        &mut self,
        document_id: DocumentId,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        let Some(index) = self
            .sessions
            .iter()
            .position(|session| session.read(cx).id == document_id)
        else {
            return Ok(false);
        };
        if self.signature_popover_open && self.active_document_id == Some(document_id) {
            self.dismiss_signature_popover(document_id, None, cx);
        }
        let images = self.sessions[index].update(cx, |session, _| session.release())?;
        if self
            .active_annotation_pointer
            .is_some_and(|active| active.document_id == document_id)
        {
            self.cancel_annotation_pointer(cx);
        }
        if let Some(control) = self.page_scale_control.clone() {
            control.update(cx, |control, cx| {
                control.cancel_for_document(document_id, cx);
            });
        }
        if self.pending_close_document_id == Some(document_id) {
            self.pending_close_document_id = None;
        }
        if self.close_after_save_document_id == Some(document_id) {
            self.close_after_save_document_id = None;
        }
        self.viewer_session_subscriptions.remove(&document_id);
        self.viewer_quality_tasks.remove(&document_id);
        self.session_tab_focus_handles.remove(&document_id);
        self.session_tab_bounds.remove(&document_id);
        self.session_tab_close_bounds.remove(&document_id);
        if self.session_tab_pointer_drag.as_ref().is_some_and(|drag| {
            drag.document_id == document_id || drag.over_document_id == document_id
        }) {
            self.session_tab_pointer_drag = None;
        }
        if self.suppress_session_tab_click_id == Some(document_id) {
            self.suppress_session_tab_click_id = None;
        }
        let session = self.sessions.remove(index);
        self.page_interactions
            .retain(|(owner, _), _| *owner != document_id);
        self.last_painted_page_evidence
            .retain(|(owner, _), _| *owner != document_id);
        self.viewport_bounds.remove(&document_id);
        drop(session);
        defer_drop_images(images, cx);
        if self.active_document_id == Some(document_id) {
            self.active_document_id = self
                .sessions
                .iter()
                .skip(index)
                .chain(self.sessions[..index].iter().rev())
                .find_map(|session| {
                    matches!(session.read(cx).status, NativeDocumentStatus::Ready)
                        .then_some(session.read(cx).id)
                });
        }
        self.sync_active_viewer_toolbar(cx);
        self.last_file_error = None;
        cx.notify();
        Ok(true)
    }
}

fn contained_page_bounds_for_space(
    container: Bounds<Pixels>,
    page_size: (f32, f32),
    coordinate_space: PageCoordinateSpace,
) -> Option<(Bounds<Pixels>, PageTransform)> {
    let container_width = f32::from(container.size.width);
    let container_height = f32::from(container.size.height);
    let (page_width, page_height) = page_size;
    if container_width <= 0. || container_height <= 0. || page_width <= 0. || page_height <= 0. {
        return None;
    }
    let scale = (container_width / page_width).min(container_height / page_height);
    let rendered_width = page_width * scale;
    let rendered_height = page_height * scale;
    let page_bounds = Bounds::new(
        point(
            container.origin.x + px((container_width - rendered_width) / 2.),
            container.origin.y + px((container_height - rendered_height) / 2.),
        ),
        size(px(rendered_width), px(rendered_height)),
    );
    let transform =
        PageTransform::from_page_coordinate_space(coordinate_space, f64::from(scale)).ok()?;
    Some((page_bounds, transform))
}

pub fn straight_line_arrowhead_points(
    start: PdfPoint,
    end: PdfPoint,
    stroke_width_pt: f64,
) -> Option<[PdfPoint; 3]> {
    butter_paper_gpui_gallery::annotation_model::straight_line_arrowhead_points(
        start,
        end,
        stroke_width_pt,
    )
}

fn paint_ellipse_annotations(
    annotations: Vec<SceneRectangle>,
    page_bounds: Bounds<Pixels>,
    page_size: (f32, f32),
    transform: &PageTransform,
    selection_color: gpui::Hsla,
    window: &mut Window,
) {
    for annotation in annotations {
        let project = |sample: PdfPoint| {
            let local = transform.point_to_local_pixels(sample);
            point(
                page_bounds.origin.x + px(local.x as f32),
                page_bounds.origin.y + px(local.y as f32),
            )
        };
        let (start, segments) =
            ellipse_cubic_bezier_points(annotation.rect, annotation.rotation_degrees);
        if let Some(fill_color) = annotation.appearance.fill_color()
            && let Ok(color) = try_parse_color(fill_color)
        {
            let mut builder = PathBuilder::fill();
            builder.move_to(project(start));
            for (control_a, control_b, to) in segments {
                builder.cubic_bezier_to(project(to), project(control_a), project(control_b));
            }
            builder.close();
            if let Ok(path) = builder.build() {
                window.paint_path(
                    path,
                    color.opacity(
                        (annotation.appearance.opacity() * annotation.appearance.fill_opacity())
                            as f32,
                    ),
                );
            }
        }
        let scale = f32::from(page_bounds.size.width) / page_size.0;
        let stroke_width = px((annotation.appearance.stroke_width_pt() as f32 * scale).max(1.));
        let mut builder = PathBuilder::stroke(stroke_width);
        builder = match annotation.appearance.stroke_style() {
            StrokeStyle::Solid => builder,
            StrokeStyle::Dashed => builder.dash_array(&[stroke_width * 4., stroke_width * 2.]),
            StrokeStyle::Dotted => builder.dash_array(&[stroke_width, stroke_width * 2.]),
        };
        builder.move_to(project(start));
        for (control_a, control_b, to) in segments {
            builder.cubic_bezier_to(project(to), project(control_a), project(control_b));
        }
        builder.close();
        if let Ok(path) = builder.build() {
            window.paint_path(
                path,
                try_parse_color(annotation.appearance.stroke_color())
                    .unwrap_or(selection_color)
                    .opacity(annotation.appearance.opacity() as f32),
            );
        }
        if annotation.selected {
            let mut builder = PathBuilder::stroke(px(2.));
            builder.move_to(project(start));
            for (control_a, control_b, to) in segments {
                builder.cubic_bezier_to(project(to), project(control_a), project(control_b));
            }
            builder.close();
            if let Ok(path) = builder.build() {
                window.paint_path(path, selection_color);
            }
            let handle_color = if annotation.locked {
                selection_color.opacity(0.55)
            } else {
                selection_color
            };
            for center in RectangleResizeHandle::ALL
                .map(|handle| {
                    ellipse_resize_handle_point_for_rect(
                        annotation.rect,
                        annotation.rotation_degrees,
                        handle,
                    )
                })
                .map(project)
            {
                window.paint_quad(fill(
                    Bounds::new(
                        point(center.x - px(4.), center.y - px(4.)),
                        size(px(8.), px(8.)),
                    ),
                    handle_color,
                ));
            }
            if let Ok(rotation_handle) = ellipse_rotation_handle_point_for_rect(
                annotation.rect,
                annotation.rotation_degrees,
                transform.pixels_per_point(),
            ) {
                let north = ellipse_resize_handle_point_for_rect(
                    annotation.rect,
                    annotation.rotation_degrees,
                    RectangleResizeHandle::North,
                );
                let mut connector = PathBuilder::stroke(px(2.));
                connector.move_to(project(north));
                connector.line_to(project(rotation_handle));
                if let Ok(path) = connector.build() {
                    window.paint_path(path, handle_color);
                }
                let center = project(rotation_handle);
                window.paint_quad(fill(
                    Bounds::new(
                        point(center.x - px(4.), center.y - px(4.)),
                        size(px(8.), px(8.)),
                    ),
                    handle_color,
                ));
            }
        }
    }
}

fn paint_cloud_plus_annotation(
    annotation: SceneCloudPlus,
    transform: &PageTransform,
    page_bounds: Bounds<Pixels>,
    page_size: (f32, f32),
    selection_color: gpui::Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    if annotation.scallop_path.len() < 2 {
        return;
    }
    let project = |sample: PdfPoint| {
        let local = transform.point_to_local_pixels(sample);
        point(
            page_bounds.origin.x + px(local.x as f32),
            page_bounds.origin.y + px(local.y as f32),
        )
    };
    let scale = f32::from(page_bounds.size.width) / page_size.0;
    let cloud = annotation.appearance.cloud();
    let stroke_width = px((cloud.stroke_width_pt() as f32 * scale).max(1.));
    let stroke_color = try_parse_color(cloud.stroke_color())
        .unwrap_or(selection_color)
        .opacity(cloud.opacity() as f32);
    let mut scallop = PathBuilder::stroke(stroke_width);
    scallop.move_to(project(annotation.scallop_path[0]));
    for sample in annotation.scallop_path.iter().copied().skip(1) {
        scallop.line_to(project(sample));
    }
    if !annotation.draft {
        scallop.close();
    }
    if let Ok(path) = scallop.build() {
        window.paint_path(path, stroke_color);
    }

    let projected_leader = annotation
        .leader_points
        .iter()
        .copied()
        .map(project)
        .collect::<Vec<_>>();
    if let Some(first) = projected_leader.first().copied() {
        let leader_appearance = annotation.appearance.leader();
        let leader_width = px((leader_appearance.stroke_width_pt() as f32 * scale).max(1.));
        let leader_color = try_parse_color(leader_appearance.stroke_color())
            .unwrap_or(selection_color)
            .opacity(leader_appearance.opacity() as f32);
        let mut leader = PathBuilder::stroke(leader_width);
        leader = match leader_appearance.stroke_style() {
            StrokeStyle::Solid => leader,
            StrokeStyle::Dashed => leader.dash_array(&[leader_width * 4., leader_width * 2.]),
            StrokeStyle::Dotted => leader.dash_array(&[leader_width, leader_width * 2.]),
        };
        leader.move_to(first);
        for sample in projected_leader.iter().copied().skip(1) {
            leader.line_to(sample);
        }
        if let Ok(path) = leader.build() {
            window.paint_path(path, leader_color);
        }
    }

    let local = transform.rect_to_local_pixels(annotation.text_box);
    let text_box_bounds = Bounds::new(
        point(
            page_bounds.origin.x + px(local.x as f32),
            page_bounds.origin.y + px(local.y as f32),
        ),
        size(px(local.width as f32), px(local.height as f32)),
    );
    let text_style = annotation.appearance.text();
    let font_size = px(text_style.font_size_pt() as f32 * scale);
    let line_height = px(text_style.font_size_pt() as f32 * 1.15 * scale);
    let inset = px(3. * scale);
    let line_count = annotation.content.split('\n').count().max(1) as f32;
    let text_height = line_height * line_count;
    let content_bounds = Bounds::new(
        point(
            text_box_bounds.origin.x + inset,
            text_box_bounds.origin.y
                + ((text_box_bounds.size.height - text_height) / 2.).max(inset),
        ),
        size(
            (text_box_bounds.size.width - inset * 2.).max(px(0.)),
            text_height,
        ),
    );
    let text_color = try_parse_color(text_style.color())
        .unwrap_or(selection_color)
        .opacity(text_style.opacity() as f32);
    let align = match text_style.alignment() {
        TextAlignment::Left => TextAlign::Left,
        TextAlignment::Center => TextAlign::Center,
        TextAlignment::Right => TextAlign::Right,
    };
    window.with_content_mask(
        Some(ContentMask {
            bounds: text_box_bounds,
        }),
        |window| {
            for (line_index, line) in annotation.content.split('\n').enumerate() {
                let text: SharedString = line.to_owned().into();
                let run = TextRun {
                    len: text.len(),
                    font: font(text_style.font_family().to_owned()),
                    color: text_color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window
                    .text_system()
                    .shape_line(text, font_size, &[run], None);
                let _ = shaped.paint(
                    point(
                        content_bounds.origin.x,
                        content_bounds.origin.y + line_height * line_index as f32,
                    ),
                    line_height,
                    align,
                    Some(content_bounds.size.width),
                    window,
                    cx,
                );
            }
        },
    );
    if annotation.selected {
        let handle_color = if annotation.locked {
            selection_color.opacity(0.55)
        } else {
            selection_color
        };
        window.paint_quad(
            outline(text_box_bounds, handle_color, BorderStyle::Solid)
                .border_widths(px(if annotation.locked { 1. } else { 2. })),
        );
        for center in annotation
            .cloud_points
            .into_iter()
            .map(project)
            .chain(projected_leader)
        {
            window.paint_quad(fill(
                Bounds::new(
                    point(center.x - px(4.), center.y - px(4.)),
                    size(px(8.), px(8.)),
                ),
                handle_color,
            ));
        }
    }
}

fn paint_dimension_annotation(
    annotation: SceneDimension,
    transform: &PageTransform,
    page_bounds: Bounds<Pixels>,
    page_size: (f32, f32),
    selection_color: gpui::Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    let delta_x = annotation.end.x - annotation.start.x;
    let delta_y = annotation.end.y - annotation.start.y;
    let length = delta_x.hypot(delta_y);
    if !length.is_finite() || length <= f64::EPSILON {
        return;
    }
    let normal_x = -delta_y / length;
    let normal_y = delta_x / length;
    let offset_x = normal_x * annotation.dimension_line_offset;
    let offset_y = normal_y * annotation.dimension_line_offset;
    let dimension_start = PdfPoint {
        x: annotation.start.x + offset_x,
        y: annotation.start.y + offset_y,
    };
    let dimension_end = PdfPoint {
        x: annotation.end.x + offset_x,
        y: annotation.end.y + offset_y,
    };
    let caption_center = PdfPoint {
        x: (dimension_start.x + dimension_end.x) * 0.5,
        y: (dimension_start.y + dimension_end.y) * 0.5,
    };
    let project = |sample: PdfPoint| {
        let local = transform.point_to_local_pixels(sample);
        point(
            page_bounds.origin.x + px(local.x as f32),
            page_bounds.origin.y + px(local.y as f32),
        )
    };
    let scale = f32::from(page_bounds.size.width) / page_size.0;
    let line = annotation.appearance.line();
    let stroke_width = px((line.stroke_width_pt() as f32 * scale).max(1.));
    let stroke_color = try_parse_color(line.stroke_color())
        .unwrap_or(selection_color)
        .opacity(line.opacity() as f32);
    let sign = if annotation.dimension_line_offset >= 0. {
        1.
    } else {
        -1.
    };
    let overhang = 4. * sign;
    let extension_start_outer = PdfPoint {
        x: dimension_start.x + normal_x * overhang,
        y: dimension_start.y + normal_y * overhang,
    };
    let extension_end_outer = PdfPoint {
        x: dimension_end.x + normal_x * overhang,
        y: dimension_end.y + normal_y * overhang,
    };
    for (from, to) in [
        (extension_start_outer, annotation.start),
        (annotation.start, dimension_start),
        (extension_end_outer, annotation.end),
        (annotation.end, dimension_end),
    ] {
        let mut builder = PathBuilder::stroke(stroke_width);
        builder.move_to(project(from));
        builder.line_to(project(to));
        if let Ok(path) = builder.build() {
            window.paint_path(path, stroke_color);
        }
    }

    let text_style = annotation.appearance.text();
    let caption_width_pt =
        (annotation.content.chars().count() as f64 * text_style.font_size_pt() * 0.6 + 8.).max(16.);
    let half_gap = (caption_width_pt * 0.5 + 4.).min(length * 0.45);
    let unit_x = delta_x / length;
    let unit_y = delta_y / length;
    for (from, to) in [
        (
            dimension_start,
            PdfPoint {
                x: caption_center.x - unit_x * half_gap,
                y: caption_center.y - unit_y * half_gap,
            },
        ),
        (
            PdfPoint {
                x: caption_center.x + unit_x * half_gap,
                y: caption_center.y + unit_y * half_gap,
            },
            dimension_end,
        ),
    ] {
        let mut builder = PathBuilder::stroke(stroke_width);
        builder.move_to(project(from));
        builder.line_to(project(to));
        if let Ok(path) = builder.build() {
            window.paint_path(path, stroke_color);
        }
    }
    for (from, to) in [
        (dimension_end, dimension_start),
        (dimension_start, dimension_end),
    ] {
        if let Some(points) = straight_line_arrowhead_points(from, to, line.stroke_width_pt()) {
            let points = points.map(project);
            let mut builder = PathBuilder::fill();
            builder.move_to(points[0]);
            builder.line_to(points[1]);
            builder.line_to(points[2]);
            builder.close();
            if let Ok(path) = builder.build() {
                window.paint_path(path, stroke_color);
            }
        }
    }

    let caption: SharedString = annotation.content.into();
    let text_color = try_parse_color(text_style.color())
        .unwrap_or(selection_color)
        .opacity(text_style.opacity() as f32);
    let run = TextRun {
        len: caption.len(),
        font: font(text_style.font_family().to_owned()),
        color: text_color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let font_size = px(text_style.font_size_pt() as f32 * scale);
    let line_height = px(text_style.font_size_pt() as f32 * (13. / 12.) * scale);
    let shaped = window
        .text_system()
        .shape_line(caption, font_size, &[run], None);
    let center = project(caption_center);
    let _ = shaped.paint(
        point(
            center.x - px(caption_width_pt as f32 * scale * 0.5),
            center.y - line_height / 2.,
        ),
        line_height,
        TextAlign::Center,
        Some(px(caption_width_pt as f32 * scale)),
        window,
        cx,
    );

    if annotation.selected {
        let handle_color = if annotation.locked {
            selection_color.opacity(0.55)
        } else {
            selection_color
        };
        for center in [
            project(annotation.start),
            project(annotation.end),
            project(caption_center),
        ] {
            window.paint_quad(fill(
                Bounds::new(
                    point(center.x - px(4.), center.y - px(4.)),
                    size(px(8.), px(8.)),
                ),
                handle_color,
            ));
        }
    }
}

fn paint_arc_annotations(
    annotations: Vec<SceneArc>,
    page_bounds: Bounds<Pixels>,
    page_size: (f32, f32),
    transform: &PageTransform,
    selection_color: gpui::Hsla,
    window: &mut Window,
) {
    for annotation in annotations {
        let project = |sample: PdfPoint| {
            let local = transform.point_to_local_pixels(sample);
            point(
                page_bounds.origin.x + px(local.x as f32),
                page_bounds.origin.y + px(local.y as f32),
            )
        };
        let Some(first) = annotation.sampled_path.first().copied() else {
            continue;
        };
        let scale = f32::from(page_bounds.size.width) / page_size.0;
        let stroke_width = px((annotation.appearance.stroke_width_pt() as f32 * scale).max(1.));
        let mut builder = PathBuilder::stroke(stroke_width);
        builder = match annotation.appearance.stroke_style() {
            StrokeStyle::Solid => builder,
            StrokeStyle::Dashed => builder.dash_array(&[stroke_width * 4., stroke_width * 2.]),
            StrokeStyle::Dotted => builder.dash_array(&[stroke_width, stroke_width * 2.]),
        };
        builder.move_to(project(first));
        for sample in annotation.sampled_path.iter().skip(1) {
            builder.line_to(project(*sample));
        }
        if let Ok(path) = builder.build() {
            window.paint_path(
                path,
                try_parse_color(annotation.appearance.stroke_color())
                    .unwrap_or(selection_color)
                    .opacity(annotation.appearance.opacity() as f32),
            );
        }
        if annotation.selected {
            let mut selection = PathBuilder::stroke(px(2.));
            selection.move_to(project(first));
            for sample in annotation.sampled_path.iter().skip(1) {
                selection.line_to(project(*sample));
            }
            if let Ok(path) = selection.build() {
                window.paint_path(path, selection_color);
            }
            let handle_color = if annotation.locked {
                selection_color.opacity(0.55)
            } else {
                selection_color
            };
            for center in [
                project(annotation.start),
                project(annotation.mid),
                project(annotation.end),
            ] {
                window.paint_quad(fill(
                    Bounds::new(
                        point(center.x - px(4.), center.y - px(4.)),
                        size(px(8.), px(8.)),
                    ),
                    handle_color,
                ));
            }
        }
    }
}

fn redact_resize_point(rect: PdfRect, handle: RectangleResizeHandle) -> PdfPoint {
    let left = rect.x;
    let right = rect.x + rect.width;
    let bottom = rect.y;
    let top = rect.y + rect.height;
    let center_x = (left + right) * 0.5;
    let center_y = (bottom + top) * 0.5;
    match handle {
        RectangleResizeHandle::NorthWest => PdfPoint { x: left, y: top },
        RectangleResizeHandle::North => PdfPoint {
            x: center_x,
            y: top,
        },
        RectangleResizeHandle::NorthEast => PdfPoint { x: right, y: top },
        RectangleResizeHandle::East => PdfPoint {
            x: right,
            y: center_y,
        },
        RectangleResizeHandle::SouthEast => PdfPoint {
            x: right,
            y: bottom,
        },
        RectangleResizeHandle::South => PdfPoint {
            x: center_x,
            y: bottom,
        },
        RectangleResizeHandle::SouthWest => PdfPoint { x: left, y: bottom },
        RectangleResizeHandle::West => PdfPoint {
            x: left,
            y: center_y,
        },
    }
}

fn paint_redact_annotations(
    annotations: Vec<SceneRedact>,
    page_bounds: Bounds<Pixels>,
    page_size: (f32, f32),
    transform: &PageTransform,
    selection_color: gpui::Hsla,
    window: &mut Window,
) {
    for annotation in annotations {
        let local = transform.rect_to_local_pixels(annotation.rect);
        let annotation_bounds = Bounds::new(
            point(
                page_bounds.origin.x + px(local.x as f32),
                page_bounds.origin.y + px(local.y as f32),
            ),
            size(px(local.width as f32), px(local.height as f32)),
        );
        if let Some(fill_color) = annotation.appearance.fill_color()
            && let Ok(color) = try_parse_color(fill_color)
        {
            window.paint_quad(fill(
                annotation_bounds,
                color.opacity(annotation.appearance.opacity() as f32),
            ));
        }
        let stroke_color = try_parse_color(annotation.appearance.stroke_color())
            .unwrap_or(selection_color)
            .opacity(annotation.appearance.opacity() as f32);
        let stroke_width = px((annotation.appearance.stroke_width_pt() as f32
            * f32::from(page_bounds.size.width)
            / page_size.0)
            .max(1.));
        window.paint_quad(
            outline(annotation_bounds, stroke_color, BorderStyle::Solid)
                .border_widths(stroke_width),
        );
        if !annotation.selected {
            continue;
        }
        window.paint_quad(
            outline(annotation_bounds, selection_color, BorderStyle::Solid).border_widths(px(2.)),
        );
        let handle_color = if annotation.locked {
            selection_color.opacity(0.55)
        } else {
            selection_color
        };
        for handle in RectangleResizeHandle::ALL {
            let local =
                transform.point_to_local_pixels(redact_resize_point(annotation.rect, handle));
            let center = point(
                page_bounds.origin.x + px(local.x as f32),
                page_bounds.origin.y + px(local.y as f32),
            );
            window.paint_quad(fill(
                Bounds::new(
                    point(center.x - px(4.), center.y - px(4.)),
                    size(px(8.), px(8.)),
                ),
                handle_color,
            ));
        }
    }
}

fn selected_redact_debug_markers(
    scene: &AnnotationScene,
    page_size: (f32, f32),
    pdf_page_size: (f32, f32),
    rotation: PageRotation,
) -> Vec<gpui::AnyElement> {
    let Some(annotation) = scene.redacts.iter().find(|redact| redact.selected) else {
        return Vec::new();
    };
    let Ok(transform) = PageTransform::new_rotated(
        f64::from(pdf_page_size.0),
        f64::from(pdf_page_size.1),
        1.,
        rotation,
    ) else {
        return Vec::new();
    };
    if page_size.0 <= 0. || page_size.1 <= 0. {
        return Vec::new();
    }
    let body = transform.rect_to_local_pixels(annotation.rect);
    let body_id = annotation.body_id;
    let body_selector = body_id;
    let mut markers = vec![
        gpui::div()
            .id(body_id)
            .debug_selector(move || body_selector.into())
            .absolute()
            .left(relative((body.x as f32 / page_size.0).clamp(0., 1.)))
            .top(relative((body.y as f32 / page_size.1).clamp(0., 1.)))
            .w(relative((body.width as f32 / page_size.0).clamp(0., 1.)))
            .h(relative((body.height as f32 / page_size.1).clamp(0., 1.)))
            .into_any_element(),
    ];
    markers.extend(RectangleResizeHandle::ALL.into_iter().map(|handle| {
        let id = redact_resize_handle_id(handle);
        let selector = id;
        let local = transform.point_to_local_pixels(redact_resize_point(annotation.rect, handle));
        gpui::div()
            .id(id)
            .debug_selector(move || selector.into())
            .absolute()
            .left(relative((local.x as f32 / page_size.0).clamp(0., 1.)))
            .top(relative((local.y as f32 / page_size.1).clamp(0., 1.)))
            .ml(px(-4.))
            .mt(px(-4.))
            .size(px(8.))
            .into_any_element()
    }));
    markers
}

fn selected_snapshot_debug_markers(
    scene: &AnnotationScene,
    page_size: (f32, f32),
    pdf_page_size: (f32, f32),
    rotation: PageRotation,
) -> Vec<gpui::AnyElement> {
    let Some(annotation) = scene.snapshots.iter().find(|snapshot| snapshot.selected) else {
        return Vec::new();
    };
    let Ok(transform) = PageTransform::new_rotated(
        f64::from(pdf_page_size.0),
        f64::from(pdf_page_size.1),
        1.,
        rotation,
    ) else {
        return Vec::new();
    };
    if page_size.0 <= 0. || page_size.1 <= 0. {
        return Vec::new();
    }
    let body = transform.rect_to_local_pixels(annotation.rect);
    let body_id = annotation.body_id;
    let body_selector = body_id;
    let mut markers = vec![
        gpui::div()
            .id(body_id)
            .debug_selector(move || body_selector.into())
            .absolute()
            .left(relative((body.x as f32 / page_size.0).clamp(0., 1.)))
            .top(relative((body.y as f32 / page_size.1).clamp(0., 1.)))
            .w(relative((body.width as f32 / page_size.0).clamp(0., 1.)))
            .h(relative((body.height as f32 / page_size.1).clamp(0., 1.)))
            .into_any_element(),
    ];
    if annotation.draft {
        return markers;
    }
    markers.extend(RectangleResizeHandle::ALL.into_iter().map(|handle| {
        let id = snapshot_resize_handle_id(handle);
        let selector = id;
        let local = transform.point_to_local_pixels(
            handle.world_point(annotation.rect, annotation.rotation_degrees),
        );
        gpui::div()
            .id(id)
            .debug_selector(move || selector.into())
            .absolute()
            .left(relative((local.x as f32 / page_size.0).clamp(0., 1.)))
            .top(relative((local.y as f32 / page_size.1).clamp(0., 1.)))
            .ml(px(-4.))
            .mt(px(-4.))
            .size(px(8.))
            .into_any_element()
    }));
    let rotate_id = "snapshot.rotate";
    let rotate_selector = rotate_id;
    let observed_pixels_per_point = (f64::from(page_size.0) / f64::from(pdf_page_size.0))
        .min(f64::from(page_size.1) / f64::from(pdf_page_size.1));
    if let Ok(rotation_handle) = ellipse_rotation_handle_point_for_rect(
        annotation.rect,
        annotation.rotation_degrees,
        observed_pixels_per_point,
    ) {
        let local = transform.point_to_local_pixels(rotation_handle);
        markers.push(
            gpui::div()
            .id(rotate_id)
            .debug_selector(move || rotate_selector.into())
            .absolute()
            .left(relative((local.x as f32 / page_size.0).clamp(0., 1.)))
            .top(relative((local.y as f32 / page_size.1).clamp(0., 1.)))
            .ml(px(-4.))
            .mt(px(-4.))
            .size(px(8.))
            .into_any_element(),
        );
    }
    markers
}

fn pointer_debug_marker(
    id: impl Into<SharedString>,
    point: PdfPoint,
    transform: PageTransform,
    page_size: (f32, f32),
) -> gpui::AnyElement {
    let id = id.into();
    let selector = id.clone();
    let local = transform.point_to_local_pixels(point);
    gpui::div()
        .id(id)
        .debug_selector(move || selector.to_string())
        .absolute()
        .left(relative((local.x as f32 / page_size.0).clamp(0., 1.)))
        .top(relative((local.y as f32 / page_size.1).clamp(0., 1.)))
        .ml(px(-4.))
        .mt(px(-4.))
        .size(px(8.))
        .into_any_element()
}

fn pointer_body_debug_marker(
    id: &'static str,
    points: impl IntoIterator<Item = PdfPoint>,
    transform: PageTransform,
    page_size: (f32, f32),
) -> Option<gpui::AnyElement> {
    let mut points = points.into_iter().map(|point| transform.point_to_local_pixels(point));
    let first = points.next()?;
    let (mut left, mut top, mut right, mut bottom) = (first.x, first.y, first.x, first.y);
    for point in points {
        left = left.min(point.x);
        top = top.min(point.y);
        right = right.max(point.x);
        bottom = bottom.max(point.y);
    }
    let selector = id;
    Some(
        gpui::div()
            .id(id)
            .debug_selector(move || selector.into())
            .absolute()
            .left(relative((left as f32 / page_size.0).clamp(0., 1.)))
            .top(relative((top as f32 / page_size.1).clamp(0., 1.)))
            .w(relative(
                (((right - left).max(8.)) as f32 / page_size.0).clamp(0., 1.),
            ))
            .h(relative(
                (((bottom - top).max(8.)) as f32 / page_size.1).clamp(0., 1.),
            ))
            .into_any_element(),
    )
}

fn selected_engineering_debug_markers(
    scene: &AnnotationScene,
    page_size: (f32, f32),
    coordinate_space: PageCoordinateSpace,
) -> Vec<gpui::AnyElement> {
    if page_size.0 <= 0. || page_size.1 <= 0. {
        return Vec::new();
    }
    let Ok(transform) = PageTransform::from_page_coordinate_space(coordinate_space, 1.) else {
        return Vec::new();
    };
    let mut markers = Vec::new();
    if let Some(annotation) = scene.dimensions.iter().find(|annotation| annotation.selected) {
        let delta_x = annotation.end.x - annotation.start.x;
        let delta_y = annotation.end.y - annotation.start.y;
        let length = delta_x.hypot(delta_y);
        if length > f64::EPSILON {
            let offset_x = -delta_y / length * annotation.dimension_line_offset;
            let offset_y = delta_x / length * annotation.dimension_line_offset;
            let offset_start = PdfPoint {
                x: annotation.start.x + offset_x,
                y: annotation.start.y + offset_y,
            };
            let offset_end = PdfPoint {
                x: annotation.end.x + offset_x,
                y: annotation.end.y + offset_y,
            };
            markers.push(pointer_debug_marker(
                DIMENSION_START_HANDLE_ID,
                annotation.start,
                transform,
                page_size,
            ));
            markers.push(pointer_debug_marker(
                DIMENSION_END_HANDLE_ID,
                annotation.end,
                transform,
                page_size,
            ));
            markers.push(pointer_debug_marker(
                DIMENSION_OFFSET_HANDLE_ID,
                PdfPoint {
                    x: (offset_start.x + offset_end.x) * 0.5,
                    y: (offset_start.y + offset_end.y) * 0.5,
                },
                transform,
                page_size,
            ));
            markers.push(pointer_debug_marker(
                DIMENSION_BODY_ID,
                PdfPoint {
                    x: offset_start.x + (offset_end.x - offset_start.x) * 0.25,
                    y: offset_start.y + (offset_end.y - offset_start.y) * 0.25,
                },
                transform,
                page_size,
            ));
        }
    }
    if let Some(annotation) = scene.callouts.iter().find(|annotation| annotation.selected) {
        markers.extend(annotation.leader_points.iter().enumerate().map(|(index, point)| {
            pointer_debug_marker(
                format!("callout.leader.{index}"),
                *point,
                transform,
                page_size,
            )
        }));
        let rect = annotation.text_box;
        if let Some(text_box) = pointer_body_debug_marker(
            CALLOUT_TEXT_BOX_ID,
            [
                PdfPoint { x: rect.x, y: rect.y },
                PdfPoint {
                    x: rect.x + rect.width,
                    y: rect.y + rect.height,
                },
            ],
            transform,
            page_size,
        ) {
            markers.push(text_box);
        }
        if let Some(segment) = annotation.leader_points.windows(2).next() {
            markers.push(pointer_debug_marker(
                CALLOUT_BODY_ID,
                PdfPoint {
                    x: (segment[0].x + segment[1].x) * 0.5,
                    y: (segment[0].y + segment[1].y) * 0.5,
                },
                transform,
                page_size,
            ));
        }
    }
    if let Some(annotation) = scene.clouds.iter().find(|annotation| annotation.selected) {
        markers.extend(annotation.points.iter().enumerate().map(|(index, point)| {
            pointer_debug_marker(
                format!("cloud.vertex.{index}"),
                *point,
                transform,
                page_size,
            )
        }));
        if let Some(point) = annotation.scallop_path.iter().copied().find(|candidate| {
            annotation
                .points
                .iter()
                .all(|vertex| (candidate.x - vertex.x).hypot(candidate.y - vertex.y) > 12.)
        }) {
            markers.push(pointer_debug_marker(
                CLOUD_BODY_ID,
                point,
                transform,
                page_size,
            ));
        }
    }
    markers
}

fn semantic_snap_debug_marker(
    decision: &SemanticSnapDecision,
    page_size: (f32, f32),
    pdf_page_size: (f32, f32),
    rotation: PageRotation,
) -> Option<gpui::AnyElement> {
    let transform = PageTransform::new_rotated(
        f64::from(pdf_page_size.0),
        f64::from(pdf_page_size.1),
        1.,
        rotation,
    )
    .ok()?;
    if page_size.0 <= 0. || page_size.1 <= 0. {
        return None;
    }
    let local = transform.point_to_local_pixels(decision.point);
    Some(
        gpui::div()
            .id(DOCUMENT_SNAP_INDICATOR_ID)
            .debug_selector(|| DOCUMENT_SNAP_INDICATOR_ID.into())
            .absolute()
            .left(relative((local.x as f32 / page_size.0).clamp(0., 1.)))
            .top(relative((local.y as f32 / page_size.1).clamp(0., 1.)))
            .ml(px(-0.5))
            .mt(px(-0.5))
            .size(px(1.))
            .into_any_element(),
    )
}

fn paint_semantic_snap_indicator(
    decision: &SemanticSnapDecision,
    page_bounds: Bounds<Pixels>,
    transform: &PageTransform,
    window: &mut Window,
) {
    let local = transform.point_to_local_pixels(decision.point);
    let center = point(
        page_bounds.origin.x + px(local.x as f32),
        page_bounds.origin.y + px(local.y as f32),
    );
    let color = gpui::rgb(0x22c55e);
    let half = px(5.);
    match decision.role {
        SemanticSnapRole::Endpoint => {
            window.paint_quad(
                outline(
                    Bounds::new(
                        point(center.x - half, center.y - half),
                        size(half * 2., half * 2.),
                    ),
                    color,
                    BorderStyle::Solid,
                )
                .border_widths(px(2.)),
            );
        }
        SemanticSnapRole::Midpoint => {
            let mut builder = PathBuilder::stroke(px(2.));
            builder.move_to(point(center.x, center.y - half));
            builder.line_to(point(center.x + half, center.y + half));
            builder.line_to(point(center.x - half, center.y + half));
            builder.line_to(point(center.x, center.y - half));
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        SemanticSnapRole::Center => {
            let mut builder = PathBuilder::stroke(px(2.));
            builder.move_to(point(center.x - half, center.y));
            builder.line_to(point(center.x + half, center.y));
            builder.move_to(point(center.x, center.y - half));
            builder.line_to(point(center.x, center.y + half));
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        SemanticSnapRole::Intersection => {
            let mut builder = PathBuilder::stroke(px(2.));
            builder.move_to(point(center.x - half, center.y - half));
            builder.line_to(point(center.x + half, center.y + half));
            builder.move_to(point(center.x + half, center.y - half));
            builder.line_to(point(center.x - half, center.y + half));
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        SemanticSnapRole::Nearest => {
            let mut builder = PathBuilder::stroke(px(2.));
            builder.move_to(point(center.x, center.y - half));
            builder.line_to(point(center.x + half, center.y));
            builder.line_to(point(center.x, center.y + half));
            builder.line_to(point(center.x - half, center.y));
            builder.line_to(point(center.x, center.y - half));
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
    }
}

fn annotation_layer(
    document_id: DocumentId,
    page_index: u32,
    page_size: (f32, f32),
    pdf_page_size: (f32, f32),
    rotation: PageRotation,
    coordinate_space: PageCoordinateSpace,
    scene: AnnotationScene,
    highlights_precomposed: bool,
    image_assets: Arc<HashMap<String, Arc<RenderImage>>>,
    selection_color: gpui::Hsla,
    semantic_snap_decision: Option<SemanticSnapDecision>,
    selection_marquee: Option<SelectionMarquee>,
    interaction_control: Option<WeakEntity<DocumentWorkspace>>,
    painted_viewer: Option<PaintedViewerAuthority>,
) -> impl IntoElement {
    let stable_id = document_annotation_layer_id(document_id, page_index);
    let selector = stable_id.clone();
    let redact_debug_markers = if interaction_control.is_some() {
        selected_redact_debug_markers(&scene, page_size, pdf_page_size, rotation)
    } else {
        Vec::new()
    };
    let snapshot_debug_markers = if interaction_control.is_some() {
        selected_snapshot_debug_markers(&scene, page_size, pdf_page_size, rotation)
    } else {
        Vec::new()
    };
    let engineering_debug_markers = if interaction_control.is_some() {
        selected_engineering_debug_markers(&scene, page_size, coordinate_space)
    } else {
        Vec::new()
    };
    let arc_preview_debug_marker = interaction_control
        .is_some()
        .then(|| {
            scene
                .arcs
                .iter()
                .any(|annotation| annotation.selected && annotation.draft)
                .then(|| {
                    gpui::div()
                        .id(DOCUMENT_ARC_PREVIEW_MARKER_ID)
                        .debug_selector(|| DOCUMENT_ARC_PREVIEW_MARKER_ID.into())
                        .absolute()
                        .left_0()
                        .top_0()
                        .size(px(1.))
                })
        })
        .flatten();
    let semantic_snap_debug_marker = interaction_control
        .is_some()
        .then(|| {
            semantic_snap_decision.as_ref().and_then(|decision| {
                semantic_snap_debug_marker(decision, page_size, pdf_page_size, rotation)
            })
        })
        .flatten();
    let painted_semantic_snap_decision = semantic_snap_decision.clone();
    gpui::div()
        .id(stable_id)
        .debug_selector(move || selector.clone().into())
        .absolute()
        .inset_0()
        .child(
            canvas(
                move |bounds, _, cx| {
                    if let Some(control) = interaction_control
                        && let Some((page_bounds, transform)) =
                            contained_page_bounds_for_space(bounds, page_size, coordinate_space)
                    {
                        let _ = control.update(cx, |workspace, _| {
                            workspace.record_page_interaction(
                                document_id,
                                page_index,
                                page_bounds,
                                transform,
                                pdf_page_size,
                                painted_viewer,
                            );
                        });
                    }
                },
                move |bounds, _, window, cx| {
                    let Some((page_bounds, transform)) =
                        contained_page_bounds_for_space(bounds, page_size, coordinate_space)
                    else {
                        return;
                    };
                    paint_redact_annotations(
                        scene.redacts,
                        page_bounds,
                        page_size,
                        &transform,
                        selection_color,
                        window,
                    );
                    for annotation in scene.rectangles {
                        if annotation.rotation_degrees != 0. {
                            let points = rectangle_world_corners(
                                annotation.rect,
                                annotation.rotation_degrees,
                            )
                            .map(|sample| {
                                let local = transform.point_to_local_pixels(sample);
                                point(
                                    page_bounds.origin.x + px(local.x as f32),
                                    page_bounds.origin.y + px(local.y as f32),
                                )
                            });
                            if let Some(fill_color) = annotation.appearance.fill_color()
                                && let Ok(color) = try_parse_color(fill_color)
                            {
                                let mut builder = PathBuilder::fill();
                                builder.move_to(points[0]);
                                for point in points.iter().skip(1) {
                                    builder.line_to(*point);
                                }
                                builder.close();
                                if let Ok(path) = builder.build() {
                                    window.paint_path(
                                        path,
                                        color.opacity(
                                            (annotation.appearance.opacity()
                                                * annotation.appearance.fill_opacity())
                                                as f32,
                                        ),
                                    );
                                }
                            }
                            let stroke_width = px((annotation.appearance.stroke_width_pt() as f32
                                * f32::from(page_bounds.size.width)
                                / page_size.0)
                                .max(1.));
                            let mut builder = PathBuilder::stroke(stroke_width);
                            builder = match annotation.appearance.stroke_style() {
                                StrokeStyle::Solid => builder,
                                StrokeStyle::Dashed => {
                                    builder.dash_array(&[stroke_width * 4., stroke_width * 2.])
                                }
                                StrokeStyle::Dotted => {
                                    builder.dash_array(&[stroke_width, stroke_width * 2.])
                                }
                            };
                            builder.move_to(points[0]);
                            for point in points.iter().skip(1) {
                                builder.line_to(*point);
                            }
                            builder.close();
                            if let Ok(path) = builder.build() {
                                let color = try_parse_color(annotation.appearance.stroke_color())
                                    .unwrap_or(selection_color)
                                    .opacity(annotation.appearance.opacity() as f32);
                                window.paint_path(path, color);
                            }
                            if annotation.selected {
                                let mut builder = PathBuilder::stroke(px(2.));
                                builder.move_to(points[0]);
                                for point in points.iter().skip(1) {
                                    builder.line_to(*point);
                                }
                                builder.close();
                                if let Ok(path) = builder.build() {
                                    window.paint_path(path, selection_color);
                                }
                                for center in points {
                                    window.paint_quad(fill(
                                        Bounds::new(
                                            point(center.x - px(4.), center.y - px(4.)),
                                            size(px(8.), px(8.)),
                                        ),
                                        selection_color,
                                    ));
                                }
                            }
                            continue;
                        }
                        let local = transform.rect_to_local_pixels(annotation.rect);
                        let annotation_bounds = Bounds::new(
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            ),
                            size(px(local.width as f32), px(local.height as f32)),
                        );
                        if let Some(fill_color) = annotation.appearance.fill_color()
                            && let Ok(color) = try_parse_color(fill_color)
                        {
                            window.paint_quad(fill(
                                annotation_bounds,
                                color.opacity(
                                    (annotation.appearance.opacity()
                                        * annotation.appearance.fill_opacity())
                                        as f32,
                                ),
                            ));
                        }
                        let stroke_color = try_parse_color(annotation.appearance.stroke_color())
                            .unwrap_or(selection_color)
                            .opacity(annotation.appearance.opacity() as f32);
                        let stroke_width = px((annotation.appearance.stroke_width_pt() as f32
                            * f32::from(page_bounds.size.width)
                            / page_size.0)
                            .max(1.));
                        match annotation.appearance.stroke_style() {
                            StrokeStyle::Solid | StrokeStyle::Dashed => {
                                window.paint_quad(
                                    outline(
                                        annotation_bounds,
                                        stroke_color,
                                        if annotation.appearance.stroke_style()
                                            == StrokeStyle::Solid
                                        {
                                            BorderStyle::Solid
                                        } else {
                                            BorderStyle::Dashed
                                        },
                                    )
                                    .border_widths(stroke_width),
                                );
                            }
                            StrokeStyle::Dotted => {
                                let left = annotation_bounds.origin.x;
                                let top = annotation_bounds.origin.y;
                                let right = left + annotation_bounds.size.width;
                                let bottom = top + annotation_bounds.size.height;
                                let mut builder = PathBuilder::stroke(stroke_width)
                                    .dash_array(&[stroke_width, stroke_width * 2.]);
                                builder.move_to(point(left, top));
                                builder.line_to(point(right, top));
                                builder.line_to(point(right, bottom));
                                builder.line_to(point(left, bottom));
                                builder.close();
                                if let Ok(path) = builder.build() {
                                    window.paint_path(path, stroke_color);
                                }
                            }
                        }
                        if annotation.selected {
                            window.paint_quad(
                                outline(annotation_bounds, selection_color, BorderStyle::Solid)
                                    .border_widths(px(2.)),
                            );
                            let left = annotation_bounds.origin.x;
                            let top = annotation_bounds.origin.y;
                            let right = left + annotation_bounds.size.width;
                            let bottom = top + annotation_bounds.size.height;
                            let center_x = left + annotation_bounds.size.width / 2.;
                            let center_y = top + annotation_bounds.size.height / 2.;
                            let handle_size = px(8.);
                            let handle_half = handle_size / 2.;
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            for center in [
                                point(left, top),
                                point(center_x, top),
                                point(right, top),
                                point(right, center_y),
                                point(right, bottom),
                                point(center_x, bottom),
                                point(left, bottom),
                                point(left, center_y),
                            ] {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - handle_half, center.y - handle_half),
                                        size(handle_size, handle_size),
                                    ),
                                    handle_color,
                                ));
                            }
                            let rotation_center = point(center_x, top - px(12.));
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(center_x - px(1.), rotation_center.y),
                                    size(px(2.), px(12.)),
                                ),
                                handle_color,
                            ));
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(
                                        rotation_center.x - handle_half,
                                        rotation_center.y - handle_half,
                                    ),
                                    size(handle_size, handle_size),
                                ),
                                handle_color,
                            ));
                        }
                    }
                    paint_ellipse_annotations(
                        scene.ellipses,
                        page_bounds,
                        page_size,
                        &transform,
                        selection_color,
                        window,
                    );
                    paint_arc_annotations(
                        scene.arcs,
                        page_bounds,
                        page_size,
                        &transform,
                        selection_color,
                        window,
                    );
                    for annotation in scene.straight_lines {
                        let project = |sample: PdfPoint| {
                            let local = transform.point_to_local_pixels(sample);
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            )
                        };
                        let start = project(annotation.start);
                        let end = project(annotation.end);
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let stroke_width =
                            px((annotation.appearance.stroke_width_pt() as f32 * scale).max(1.));
                        let color = try_parse_color(annotation.appearance.stroke_color())
                            .unwrap_or(selection_color)
                            .opacity(annotation.appearance.opacity() as f32);
                        let mut builder = PathBuilder::stroke(stroke_width);
                        builder = match annotation.appearance.stroke_style() {
                            StrokeStyle::Solid => builder,
                            StrokeStyle::Dashed => {
                                builder.dash_array(&[stroke_width * 4., stroke_width * 2.])
                            }
                            StrokeStyle::Dotted => {
                                builder.dash_array(&[stroke_width, stroke_width * 2.])
                            }
                        };
                        builder.move_to(start);
                        builder.line_to(end);
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, color);
                        }
                        if annotation.kind == LineKind::Arrow
                            && let Some(points) = straight_line_arrowhead_points(
                                annotation.start,
                                annotation.end,
                                annotation.appearance.stroke_width_pt(),
                            )
                        {
                            let points = points.map(project);
                            let mut fill_builder = PathBuilder::fill();
                            fill_builder.move_to(points[0]);
                            fill_builder.line_to(points[1]);
                            fill_builder.line_to(points[2]);
                            fill_builder.close();
                            if let Ok(path) = fill_builder.build() {
                                window.paint_path(path, color);
                            }
                            let mut outline_builder = PathBuilder::stroke(stroke_width);
                            outline_builder.move_to(points[0]);
                            outline_builder.line_to(points[1]);
                            outline_builder.line_to(points[2]);
                            outline_builder.close();
                            if let Ok(path) = outline_builder.build() {
                                window.paint_path(path, color);
                            }
                        }
                        if annotation.selected {
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            for center in [start, end] {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - px(4.), center.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    handle_color,
                                ));
                            }
                        }
                    }
                    for annotation in scene.vertex_paths {
                        if annotation.points.len() < 2 {
                            continue;
                        }
                        let project = |sample: PdfPoint| {
                            let local = transform.point_to_local_pixels(sample);
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            )
                        };
                        let projected = annotation
                            .points
                            .iter()
                            .copied()
                            .map(project)
                            .collect::<Vec<_>>();
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let stroke_width =
                            px((annotation.appearance.stroke_width_pt() as f32 * scale).max(1.));
                        let stroke_color = try_parse_color(annotation.appearance.stroke_color())
                            .unwrap_or(selection_color)
                            .opacity(annotation.appearance.opacity() as f32);
                        let closes = annotation.kind
                            == butter_paper_gpui_gallery::annotation_model::VertexPathKind::Polygon
                            && !annotation.draft;
                        if closes && let Some(fill_color) = annotation.appearance.fill_color() {
                            let mut fill_builder = PathBuilder::fill();
                            fill_builder.move_to(projected[0]);
                            for sample in projected.iter().copied().skip(1) {
                                fill_builder.line_to(sample);
                            }
                            fill_builder.close();
                            if let Ok(path) = fill_builder.build() {
                                let color = try_parse_color(fill_color)
                                    .unwrap_or(selection_color)
                                    .opacity(
                                        (annotation.appearance.opacity()
                                            * annotation.appearance.fill_opacity())
                                            as f32,
                                    );
                                window.paint_path(path, color);
                            }
                        }
                        let mut stroke_builder = PathBuilder::stroke(stroke_width);
                        stroke_builder = match annotation.appearance.stroke_style() {
                            StrokeStyle::Solid => stroke_builder,
                            StrokeStyle::Dashed => {
                                stroke_builder.dash_array(&[stroke_width * 4., stroke_width * 2.])
                            }
                            StrokeStyle::Dotted => {
                                stroke_builder.dash_array(&[stroke_width, stroke_width * 2.])
                            }
                        };
                        stroke_builder.move_to(projected[0]);
                        for sample in projected.iter().copied().skip(1) {
                            stroke_builder.line_to(sample);
                        }
                        if closes {
                            stroke_builder.close();
                        }
                        if let Ok(path) = stroke_builder.build() {
                            window.paint_path(path, stroke_color);
                        }
                        if annotation.selected {
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            for center in projected {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - px(4.), center.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    handle_color,
                                ));
                            }
                        }
                    }
                    for annotation in scene.clouds {
                        if annotation.scallop_path.len() < 2 {
                            continue;
                        }
                        let project = |sample: PdfPoint| {
                            let local = transform.point_to_local_pixels(sample);
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            )
                        };
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let stroke_width =
                            px((annotation.appearance.stroke_width_pt() as f32 * scale).max(1.));
                        let stroke_color = try_parse_color(annotation.appearance.stroke_color())
                            .unwrap_or(selection_color)
                            .opacity(annotation.appearance.opacity() as f32);
                        let mut builder = PathBuilder::stroke(stroke_width);
                        let first = project(annotation.scallop_path[0]);
                        builder.move_to(first);
                        for sample in annotation.scallop_path.iter().copied().skip(1) {
                            builder.line_to(project(sample));
                        }
                        if !annotation.draft {
                            builder.close();
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, stroke_color);
                        }
                        if annotation.selected {
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            for center in annotation.points.into_iter().map(project) {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - px(4.), center.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    handle_color,
                                ));
                            }
                        }
                    }
                    for annotation in scene.cloud_pluses {
                        paint_cloud_plus_annotation(
                            annotation,
                            &transform,
                            page_bounds,
                            page_size,
                            selection_color,
                            window,
                            cx,
                        );
                    }
                    for annotation in scene.callouts {
                        if annotation.leader_points.len() < 2 {
                            continue;
                        }
                        let project = |sample: PdfPoint| {
                            let local = transform.point_to_local_pixels(sample);
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            )
                        };
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let line = annotation.appearance.line();
                        let stroke_width = px((line.stroke_width_pt() as f32 * scale).max(1.));
                        let stroke_color = try_parse_color(line.stroke_color())
                            .unwrap_or(selection_color)
                            .opacity(line.opacity() as f32);
                        let projected_leader = annotation
                            .leader_points
                            .iter()
                            .copied()
                            .map(project)
                            .collect::<Vec<_>>();
                        let mut leader = PathBuilder::stroke(stroke_width);
                        leader = match line.stroke_style() {
                            StrokeStyle::Solid => leader,
                            StrokeStyle::Dashed => {
                                leader.dash_array(&[stroke_width * 4., stroke_width * 2.])
                            }
                            StrokeStyle::Dotted => {
                                leader.dash_array(&[stroke_width, stroke_width * 2.])
                            }
                        };
                        leader.move_to(projected_leader[0]);
                        for sample in projected_leader.iter().copied().skip(1) {
                            leader.line_to(sample);
                        }
                        if let Ok(path) = leader.build() {
                            window.paint_path(path, stroke_color);
                        }
                        if let Some(arrow) = straight_line_arrowhead_points(
                            annotation.leader_points[1],
                            annotation.leader_points[0],
                            line.stroke_width_pt(),
                        ) {
                            let arrow = arrow.map(project);
                            let mut open_arrow = PathBuilder::stroke(stroke_width);
                            open_arrow.move_to(arrow[1]);
                            open_arrow.line_to(arrow[0]);
                            open_arrow.line_to(arrow[2]);
                            if let Ok(path) = open_arrow.build() {
                                window.paint_path(path, stroke_color);
                            }
                        }

                        let local = transform.rect_to_local_pixels(annotation.text_box);
                        let text_box_bounds = Bounds::new(
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            ),
                            size(px(local.width as f32), px(local.height as f32)),
                        );
                        let text_style = annotation.appearance.text();
                        let font_size = px(text_style.font_size_pt() as f32 * scale);
                        let line_height = px(text_style.font_size_pt() as f32 * 1.15 * scale);
                        let inset = px(3. * scale);
                        let line_count = annotation.content.split('\n').count().max(1) as f32;
                        let text_height = line_height * line_count;
                        let content_bounds = Bounds::new(
                            point(
                                text_box_bounds.origin.x + inset,
                                text_box_bounds.origin.y
                                    + ((text_box_bounds.size.height - text_height) / 2.).max(inset),
                            ),
                            size(
                                (text_box_bounds.size.width - inset * 2.).max(px(0.)),
                                text_height,
                            ),
                        );
                        let text_color = try_parse_color(text_style.color())
                            .unwrap_or(selection_color)
                            .opacity(text_style.opacity() as f32);
                        let align = match text_style.alignment() {
                            TextAlignment::Left => TextAlign::Left,
                            TextAlignment::Center => TextAlign::Center,
                            TextAlignment::Right => TextAlign::Right,
                        };
                        window.with_content_mask(
                            Some(ContentMask {
                                bounds: text_box_bounds,
                            }),
                            |window| {
                                for (line_index, line) in annotation.content.split('\n').enumerate()
                                {
                                    let text: SharedString = line.to_owned().into();
                                    let run = TextRun {
                                        len: text.len(),
                                        font: font(text_style.font_family().to_owned()),
                                        color: text_color,
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    };
                                    let shaped = window.text_system().shape_line(
                                        text,
                                        font_size,
                                        &[run],
                                        None,
                                    );
                                    let _ = shaped.paint(
                                        point(
                                            content_bounds.origin.x,
                                            content_bounds.origin.y
                                                + line_height * line_index as f32,
                                        ),
                                        line_height,
                                        align,
                                        Some(content_bounds.size.width),
                                        window,
                                        cx,
                                    );
                                }
                            },
                        );
                        if annotation.selected {
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            window.paint_quad(
                                outline(text_box_bounds, handle_color, BorderStyle::Solid)
                                    .border_widths(px(if annotation.locked { 1. } else { 2. })),
                            );
                            for center in projected_leader {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - px(4.), center.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    handle_color,
                                ));
                            }
                        }
                    }
                    for annotation in scene.measurement_paths {
                        if annotation.points.len() < 2 {
                            continue;
                        }
                        let project = |sample: PdfPoint| {
                            let local = transform.point_to_local_pixels(sample);
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            )
                        };
                        let projected = annotation
                            .points
                            .iter()
                            .copied()
                            .map(project)
                            .collect::<Vec<_>>();
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let stroke_width =
                            px((annotation.appearance.stroke_width_pt() as f32 * scale).max(1.));
                        let stroke_color = try_parse_color(annotation.appearance.stroke_color())
                            .unwrap_or(selection_color)
                            .opacity(annotation.appearance.opacity() as f32);
                        let closes =
                            annotation.kind == MeasurementPathKind::Area && !annotation.draft;
                        if closes && let Some(fill_color) = annotation.appearance.fill_color() {
                            let mut fill_builder = PathBuilder::fill();
                            fill_builder.move_to(projected[0]);
                            for sample in projected.iter().copied().skip(1) {
                                fill_builder.line_to(sample);
                            }
                            fill_builder.close();
                            if let Ok(path) = fill_builder.build() {
                                let color = try_parse_color(fill_color)
                                    .unwrap_or(selection_color)
                                    .opacity(
                                        (annotation.appearance.opacity()
                                            * annotation.appearance.fill_opacity())
                                            as f32,
                                    );
                                window.paint_path(path, color);
                            }
                        }
                        let mut stroke_builder = PathBuilder::stroke(stroke_width);
                        stroke_builder = match annotation.appearance.stroke_style() {
                            StrokeStyle::Solid => stroke_builder,
                            StrokeStyle::Dashed => {
                                stroke_builder.dash_array(&[stroke_width * 4., stroke_width * 2.])
                            }
                            StrokeStyle::Dotted => {
                                stroke_builder.dash_array(&[stroke_width, stroke_width * 2.])
                            }
                        };
                        stroke_builder.move_to(projected[0]);
                        for sample in projected.iter().copied().skip(1) {
                            stroke_builder.line_to(sample);
                        }
                        if closes {
                            stroke_builder.close();
                        }
                        if let Ok(path) = stroke_builder.build() {
                            window.paint_path(path, stroke_color);
                        }
                        if annotation.show_caption && !annotation.caption.is_empty() {
                            let center = projected
                                .iter()
                                .copied()
                                .fold(point(px(0.), px(0.)), |sum, sample| {
                                    point(sum.x + sample.x, sum.y + sample.y)
                                });
                            let count = projected.len() as f32;
                            let caption: SharedString = annotation.caption.into();
                            let run = TextRun {
                                len: caption.len(),
                                font: font("Helvetica"),
                                color: stroke_color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped = window.text_system().shape_line(
                                caption,
                                px(12. * scale),
                                &[run],
                                None,
                            );
                            let _ = shaped.paint(
                                point(center.x / count, center.y / count - px(14. * scale)),
                                px(14. * scale),
                                TextAlign::Center,
                                None,
                                window,
                                cx,
                            );
                        }
                        if annotation.selected {
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            for center in projected.iter().copied() {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - px(4.), center.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    handle_color,
                                ));
                            }
                        }
                    }
                    for annotation in scene.pens {
                        let project = |sample: PdfPoint| {
                            let local = transform.point_to_local_pixels(sample);
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            )
                        };
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let paint_body = annotation.tool
                            != butter_paper_gpui_gallery::annotation_model::InkTool::Highlight
                            || annotation.draft
                            || !highlights_precomposed;
                        for points in &annotation.paths {
                            if !paint_body {
                                continue;
                            }
                            let paint_path = build_ink_paint_path(points, annotation.smooth_curves);
                            if paint_path.is_empty() {
                                continue;
                            }
                            let mut builder =
                                PathBuilder::stroke(px((annotation.appearance.width_pt() as f32
                                    * scale)
                                    .max(1.)));
                            for segment in paint_path {
                                match segment {
                                    InkPaintPathSegment::MoveTo(to) => builder.move_to(project(to)),
                                    InkPaintPathSegment::LineTo(to) => builder.line_to(project(to)),
                                    InkPaintPathSegment::CubicTo {
                                        control_a,
                                        control_b,
                                        to,
                                    } => builder.cubic_bezier_to(
                                        project(to),
                                        project(control_a),
                                        project(control_b),
                                    ),
                                }
                            }
                            if let Ok(path) = builder.build() {
                                let color = try_parse_color(annotation.appearance.color())
                                    .unwrap_or(selection_color)
                                    .opacity(annotation.appearance.opacity() as f32);
                                window.paint_path(path, color);
                            }
                        }
                        if annotation.selected {
                            let projected = annotation
                                .paths
                                .iter()
                                .flatten()
                                .copied()
                                .map(project)
                                .collect::<Vec<_>>();
                            if let Some(first) = projected.first().copied() {
                                let (mut left, mut top, mut right, mut bottom) =
                                    (first.x, first.y, first.x, first.y);
                                for sample in projected.iter().skip(1) {
                                    left = left.min(sample.x);
                                    top = top.min(sample.y);
                                    right = right.max(sample.x);
                                    bottom = bottom.max(sample.y);
                                }
                                let padding = px(4.);
                                window.paint_quad(
                                    outline(
                                        Bounds::new(
                                            point(left - padding, top - padding),
                                            size(
                                                right - left + padding * 2.,
                                                bottom - top + padding * 2.,
                                            ),
                                        ),
                                        selection_color,
                                        BorderStyle::Solid,
                                    )
                                    .border_widths(px(1.)),
                                );
                            }
                        }
                    }
                    for annotation in scene.text_boxes {
                        let local = transform.rect_to_local_pixels(annotation.layout_rect);
                        let annotation_bounds = Bounds::new(
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            ),
                            size(px(local.width as f32), px(local.height as f32)),
                        );
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let font_size = px(annotation.style.font_size_pt() as f32 * scale);
                        let line_height = px(annotation.style.font_size_pt() as f32 * 1.15 * scale);
                        let inset = px(5. * scale);
                        let color = try_parse_color(annotation.style.color())
                            .unwrap_or(selection_color)
                            .opacity(annotation.style.opacity() as f32);
                        let align = match annotation.style.alignment() {
                            TextAlignment::Left => TextAlign::Left,
                            TextAlignment::Center => TextAlign::Center,
                            TextAlignment::Right => TextAlign::Right,
                        };
                        let content_bounds = Bounds::new(
                            point(
                                annotation_bounds.origin.x + inset,
                                annotation_bounds.origin.y + inset,
                            ),
                            size(
                                (annotation_bounds.size.width - inset * 2.).max(px(0.)),
                                (annotation_bounds.size.height - inset * 2.).max(px(0.)),
                            ),
                        );
                        window.with_content_mask(
                            Some(ContentMask {
                                bounds: annotation_bounds,
                            }),
                            |window| {
                                for (line_index, line) in annotation.content.split('\n').enumerate()
                                {
                                    let text: SharedString = line.to_owned().into();
                                    let run = TextRun {
                                        len: text.len(),
                                        font: font(annotation.style.font_family().to_owned()),
                                        color,
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    };
                                    let shaped = window.text_system().shape_line(
                                        text,
                                        font_size,
                                        &[run],
                                        None,
                                    );
                                    let _ = shaped.paint(
                                        point(
                                            content_bounds.origin.x,
                                            content_bounds.origin.y
                                                + line_height * line_index as f32,
                                        ),
                                        line_height,
                                        align,
                                        Some(content_bounds.size.width),
                                        window,
                                        cx,
                                    );
                                }
                            },
                        );
                        if annotation.selected {
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            window.paint_quad(
                                outline(annotation_bounds, handle_color, BorderStyle::Solid)
                                    .border_widths(px(if annotation.locked { 1. } else { 2. })),
                            );
                            let left = annotation_bounds.origin.x;
                            let top = annotation_bounds.origin.y;
                            let right = left + annotation_bounds.size.width;
                            let bottom = top + annotation_bounds.size.height;
                            let center_x = left + annotation_bounds.size.width / 2.;
                            let center_y = top + annotation_bounds.size.height / 2.;
                            for center in [
                                point(left, top),
                                point(center_x, top),
                                point(right, top),
                                point(right, center_y),
                                point(right, bottom),
                                point(center_x, bottom),
                                point(left, bottom),
                                point(left, center_y),
                            ] {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - px(4.), center.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    handle_color,
                                ));
                            }
                        }
                    }
                    for annotation in scene.dimensions {
                        paint_dimension_annotation(
                            annotation,
                            &transform,
                            page_bounds,
                            page_size,
                            selection_color,
                            window,
                            cx,
                        );
                    }
                    for annotation in scene.lengths {
                        let project = |sample: PdfPoint| {
                            let local = transform.point_to_local_pixels(sample);
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            )
                        };
                        let start = project(annotation.start);
                        let end = project(annotation.end);
                        let scale = f32::from(page_bounds.size.width) / page_size.0;
                        let color = try_parse_color("#ff0000").unwrap_or(selection_color);
                        let mut builder = PathBuilder::stroke(px(scale.max(1.)));
                        builder.move_to(start);
                        builder.line_to(end);
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, color);
                        }
                        if annotation.show_caption {
                            let caption: SharedString = annotation.caption.into();
                            let run = TextRun {
                                len: caption.len(),
                                font: font("Helvetica"),
                                color,
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            };
                            let shaped = window.text_system().shape_line(
                                caption,
                                px(12. * scale),
                                &[run],
                                None,
                            );
                            let _ = shaped.paint(
                                point(
                                    start.x + (end.x - start.x) / 2.,
                                    start.y + (end.y - start.y) / 2. - px(14. * scale),
                                ),
                                px(14. * scale),
                                TextAlign::Center,
                                None,
                                window,
                                cx,
                            );
                        }
                        if annotation.selected {
                            let handle_size = px(8.);
                            let handle_half = handle_size / 2.;
                            let handle_color = if annotation.locked {
                                selection_color.opacity(0.55)
                            } else {
                                selection_color
                            };
                            for center in [start, end] {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - handle_half, center.y - handle_half),
                                        size(handle_size, handle_size),
                                    ),
                                    handle_color,
                                ));
                            }
                        }
                    }
                    for annotation in scene.snapshots {
                        let local = transform.rect_to_local_pixels(annotation.rect);
                        let image_bounds = Bounds::new(
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            ),
                            size(px(local.width as f32), px(local.height as f32)),
                        );
                        if let Some(image) = image_assets.get(annotation.asset_id.as_str()) {
                            let _ = window.paint_image(
                                image_bounds,
                                image_bounds,
                                Default::default(),
                                image.clone(),
                                0,
                                false,
                            );
                        }
                        if annotation.selected {
                            window.paint_quad(
                                outline(image_bounds, selection_color, BorderStyle::Solid)
                                    .border_widths(px(if annotation.locked { 1. } else { 2. })),
                            );
                        }
                    }
                    for annotation in scene.images {
                        let local = transform.rect_to_local_pixels(annotation.rect);
                        let image_bounds = Bounds::new(
                            point(
                                page_bounds.origin.x + px(local.x as f32),
                                page_bounds.origin.y + px(local.y as f32),
                            ),
                            size(px(local.width as f32), px(local.height as f32)),
                        );
                        if let Some(image) = image_assets.get(annotation.asset_id.as_str()) {
                            let _ = window.paint_image(
                                image_bounds,
                                image_bounds,
                                Default::default(),
                                image.clone(),
                                0,
                                false,
                            );
                        }
                        if annotation.selected {
                            window.paint_quad(
                                outline(image_bounds, selection_color, BorderStyle::Solid)
                                    .border_widths(px(if annotation.locked { 1. } else { 2. })),
                            );
                            let left = image_bounds.origin.x;
                            let top = image_bounds.origin.y;
                            let right = left + image_bounds.size.width;
                            let bottom = top + image_bounds.size.height;
                            let center_x = left + image_bounds.size.width / 2.;
                            let center_y = top + image_bounds.size.height / 2.;
                            for center in [
                                point(left, top),
                                point(center_x, top),
                                point(right, top),
                                point(right, center_y),
                                point(right, bottom),
                                point(center_x, bottom),
                                point(left, bottom),
                                point(left, center_y),
                            ] {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(center.x - px(4.), center.y - px(4.)),
                                        size(px(8.), px(8.)),
                                    ),
                                    selection_color,
                                ));
                            }
                        }
                    }
                    if let Some(marquee) = selection_marquee
                        && marquee.points.len() >= 2
                    {
                        let project = |sample: SelectionPoint| {
                            point(
                                page_bounds.origin.x + px(sample.x as f32),
                                page_bounds.origin.y + px(sample.y as f32),
                            )
                        };
                        let color = selection_color.opacity(0.9);
                        match marquee.shape {
                            SelectionShape::Box => {
                                let start = project(marquee.start);
                                let current = project(marquee.current);
                                let left = start.x.min(current.x);
                                let top = start.y.min(current.y);
                                let right = start.x.max(current.x);
                                let bottom = start.y.max(current.y);
                                window.paint_quad(
                                    outline(
                                        Bounds::new(
                                            point(left, top),
                                            size(right - left, bottom - top),
                                        ),
                                        color,
                                        BorderStyle::Dashed,
                                    )
                                    .border_widths(px(1.)),
                                );
                            }
                            SelectionShape::Lasso => {
                                let mut builder = PathBuilder::stroke(px(1.));
                                builder.move_to(project(marquee.points[0]));
                                for sample in marquee.points.iter().skip(1) {
                                    builder.line_to(project(*sample));
                                }
                                if let Ok(path) = builder.build() {
                                    window.paint_path(path, color);
                                }
                            }
                        }
                    }
                    if let Some(decision) = painted_semantic_snap_decision.as_ref() {
                        paint_semantic_snap_indicator(decision, page_bounds, &transform, window);
                    }
                },
            )
            .size_full(),
        )
        .children(redact_debug_markers)
        .children(snapshot_debug_markers)
        .children(engineering_debug_markers)
        .when_some(arc_preview_debug_marker, |layer, marker| {
            layer.child(marker)
        })
        .when_some(semantic_snap_debug_marker, |layer, marker| {
            layer.child(marker)
        })
}

fn normalized_signature_point(
    position: Point<Pixels>,
    bounds: Bounds<Pixels>,
) -> NormalizedSignaturePoint {
    let width = f32::from(bounds.size.width).max(1.);
    let height = f32::from(bounds.size.height).max(1.);
    let x = ((f32::from(position.x - bounds.origin.x) / width).clamp(0., 1.) * f32::from(u16::MAX))
        .round() as u16;
    let y = ((f32::from(position.y - bounds.origin.y) / height).clamp(0., 1.) * f32::from(u16::MAX))
        .round() as u16;
    NormalizedSignaturePoint::new(x, y)
}

fn drawn_signature_canvas(
    signature: DrawnSignature,
    control: WeakEntity<DocumentWorkspace>,
    border_color: gpui::Hsla,
    background: gpui::Hsla,
) -> gpui::AnyElement {
    let down_control = control.clone();
    let move_control = control.clone();
    let up_control = control;
    let painted_signature = signature.clone();
    gpui::div()
        .id(DOCUMENT_SIGNATURE_CANVAS_ID)
        .debug_selector(|| DOCUMENT_SIGNATURE_CANVAS_ID.into())
        .role(Role::Canvas)
        .aria_label("Draw signature")
        .relative()
        .h_24()
        .w_full()
        .overflow_hidden()
        .border_1()
        .border_color(border_color)
        .bg(background)
        .rounded_lg()
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, cx| {
                    let ink = cx.theme().foreground;
                    for stroke in painted_signature.strokes() {
                        let mut builder = PathBuilder::stroke(px(3.));
                        for (point_ix, sample) in stroke.iter().enumerate() {
                            let position = point(
                                bounds.origin.x
                                    + bounds.size.width
                                        * (f32::from(sample.x) / f32::from(u16::MAX)),
                                bounds.origin.y
                                    + bounds.size.height
                                        * (f32::from(sample.y) / f32::from(u16::MAX)),
                            );
                            if point_ix == 0 {
                                builder.move_to(position);
                                builder.line_to(point(position.x + px(0.1), position.y));
                            } else {
                                builder.line_to(position);
                            }
                        }
                        if let Ok(path) = builder.build() {
                            window.paint_path(path, ink);
                        }
                    }
                    window.on_mouse_event({
                        let down_control = down_control.clone();
                        move |event: &MouseDownEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture || event.button != MouseButton::Left
                            {
                                return;
                            }
                            let started = down_control
                                .update(cx, |workspace, cx| {
                                    workspace.begin_drawn_signature_stroke(
                                        event.position,
                                        bounds,
                                        cx,
                                    )
                                })
                                .unwrap_or(false);
                            if started {
                                window.prevent_default();
                            }
                        }
                    });
                    window.on_mouse_event({
                        let move_control = move_control.clone();
                        move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture
                                || event.pressed_button != Some(MouseButton::Left)
                            {
                                return;
                            }
                            if move_control
                                .update(cx, |workspace, cx| {
                                    workspace.append_drawn_signature_point(
                                        event.position,
                                        bounds,
                                        cx,
                                    )
                                })
                                .unwrap_or(false)
                            {
                                window.prevent_default();
                            }
                        }
                    });
                    window.on_mouse_event({
                        let up_control = up_control.clone();
                        move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                let _ = up_control.update(cx, |workspace, cx| {
                                    workspace.end_drawn_signature_stroke(cx);
                                });
                            }
                        }
                    });
                },
            )
            .absolute()
            .inset_0(),
        )
        .into_any_element()
}

fn annotation_tool_group(
    document_id: DocumentId,
    current_page: u32,
    annotation_tool: AnnotationTool,
    save_busy: bool,
    signature_popover_open: bool,
    signature_prepare_state: SignaturePrepareState,
    drawn_signature: DrawnSignature,
    page_scale_control: WeakEntity<PageScaleControl>,
    cx: &mut Context<DocumentWorkspace>,
) -> gpui::AnyElement {
    let signature_open_control = cx.entity().downgrade();
    let signature_content_control = cx.entity().downgrade();
    let signature_canvas_border = cx.theme().border;
    let signature_canvas_background = cx.theme().background;
    let signature_control = Popover::new(DOCUMENT_SIGNATURE_POPOVER_ID)
        .anchor(Anchor::BottomLeft)
        .open(signature_popover_open)
        .on_open_change(move |open, window, cx| {
            let _ = signature_open_control.update(cx, |workspace, cx| {
                if *open {
                    workspace.signature_popover_open = true;
                    workspace.signature_prepare_state = SignaturePrepareState::Idle;
                    workspace.drawn_signature.clear();
                    cx.notify();
                } else {
                    workspace.dismiss_signature_popover(document_id, Some(window), cx);
                }
            });
        })
        .trigger(
            Button::new(DOCUMENT_SIGNATURE_TOOL_ID)
                .debug_selector(|| DOCUMENT_SIGNATURE_TOOL_ID.into())
                .label("Signature")
                .disabled(save_busy),
        )
        .content(move |_, _, _| {
            let choose_control = signature_content_control.clone();
            let add_control = signature_content_control.clone();
            let clear_control = signature_content_control.clone();
            let loading = matches!(signature_prepare_state, SignaturePrepareState::Loading);
            let has_signature =
                matches!(signature_prepare_state, SignaturePrepareState::Preview(_))
                    || !drawn_signature.is_empty();
            let mut content = v_flex().w_64().gap_2();
            content = match &signature_prepare_state {
                SignaturePrepareState::Idle => content.child(drawn_signature_canvas(
                    drawn_signature.clone(),
                    signature_content_control.clone(),
                    signature_canvas_border,
                    signature_canvas_background,
                )),
                SignaturePrepareState::Loading => content.child(
                    h_flex()
                        .id(DOCUMENT_SIGNATURE_LOADING_ID)
                        .debug_selector(|| DOCUMENT_SIGNATURE_LOADING_ID.into())
                        .role(Role::Status)
                        .aria_label("Processing signature…")
                        .gap_2()
                        .child(Spinner::new().small())
                        .child("Processing signature…"),
                ),
                SignaturePrepareState::Error(error) => content
                    .child(drawn_signature_canvas(
                        drawn_signature.clone(),
                        signature_content_control.clone(),
                        signature_canvas_border,
                        signature_canvas_background,
                    ))
                    .child(
                        gpui::div()
                            .id(DOCUMENT_SIGNATURE_ERROR_ALERT_ID)
                            .debug_selector(|| DOCUMENT_SIGNATURE_ERROR_ALERT_ID.into())
                            .child(Alert::error(
                                "document-workspace-signature-error-alert-component",
                                error.clone(),
                            )),
                    ),
                SignaturePrepareState::Preview(preview) => content.child(
                    gpui::div()
                        .id(DOCUMENT_SIGNATURE_PREVIEW_ID)
                        .debug_selector(|| DOCUMENT_SIGNATURE_PREVIEW_ID.into())
                        .role(Role::Image)
                        .aria_label("Signature preview")
                        .h_24()
                        .w_full()
                        .child(
                            img(preview.image.clone())
                                .size_full()
                                .object_fit(ObjectFit::Contain),
                        ),
                ),
            };
            content
                .child(
                    Button::new(DOCUMENT_SIGNATURE_CHOOSE_IMAGE_ID)
                        .debug_selector(|| DOCUMENT_SIGNATURE_CHOOSE_IMAGE_ID.into())
                        .label("Choose file")
                        .disabled(loading)
                        .on_click(move |_, _, cx| {
                            let _ = choose_control.update(cx, |workspace, cx| {
                                workspace.begin_signature_selection(document_id, cx);
                            });
                        }),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .child(
                            Button::new(DOCUMENT_SIGNATURE_CLEAR_ID)
                                .debug_selector(|| DOCUMENT_SIGNATURE_CLEAR_ID.into())
                                .label("Clear")
                                .disabled(loading || !has_signature)
                                .on_click(move |_, _, cx| {
                                    let _ = clear_control.update(cx, |workspace, cx| {
                                        workspace.clear_signature_input(cx);
                                    });
                                }),
                        )
                        .child(
                            Button::new(DOCUMENT_SIGNATURE_ADD_ID)
                                .debug_selector(|| DOCUMENT_SIGNATURE_ADD_ID.into())
                                .label("Add signature")
                                .primary()
                                .disabled(loading || !has_signature)
                                .on_click(move |_, window, cx| {
                                    let _ = add_control.update(cx, |workspace, cx| {
                                        if let Err(error) = workspace.arm_signature_placement(
                                            document_id,
                                            window,
                                            cx,
                                        ) {
                                            workspace.signature_prepare_state =
                                                SignaturePrepareState::Error(error);
                                            cx.notify();
                                        }
                                    });
                                }),
                        ),
                )
        });
    h_flex()
        .gap_1()
        .child(signature_control)
        .child(
            ButtonGroup::new("document-workspace-annotation-tools")
                .child(
                    Button::new(DOCUMENT_SELECT_TOOL_ID)
                        .debug_selector(|| DOCUMENT_SELECT_TOOL_ID.into())
                        .label("Select")
                        .selected(annotation_tool == AnnotationTool::Select)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Select,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_RECTANGLE_TOOL_ID)
                        .debug_selector(|| DOCUMENT_RECTANGLE_TOOL_ID.into())
                        .label("Rectangle")
                        .selected(annotation_tool == AnnotationTool::Rectangle)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Rectangle,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_ELLIPSE_TOOL_ID)
                        .debug_selector(|| DOCUMENT_ELLIPSE_TOOL_ID.into())
                        .label("Ellipse")
                        .tooltip(AnnotationTool::Ellipse.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Ellipse)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Ellipse,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_ARC_TOOL_ID)
                        .debug_selector(|| DOCUMENT_ARC_TOOL_ID.into())
                        .label("Arc")
                        .tooltip(AnnotationTool::Arc.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Arc)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ =
                                workspace.set_annotation_tool(document_id, AnnotationTool::Arc, cx);
                        })),
                )
                .child(
                    Button::new(DOCUMENT_LINE_TOOL_ID)
                        .debug_selector(|| DOCUMENT_LINE_TOOL_ID.into())
                        .label("Line")
                        .tooltip(AnnotationTool::Line.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Line)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Line,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_ARROW_TOOL_ID)
                        .debug_selector(|| DOCUMENT_ARROW_TOOL_ID.into())
                        .label("Arrow")
                        .tooltip(AnnotationTool::Arrow.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Arrow)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Arrow,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_POLYLINE_TOOL_ID)
                        .debug_selector(|| DOCUMENT_POLYLINE_TOOL_ID.into())
                        .label("Polyline")
                        .tooltip(AnnotationTool::Polyline.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Polyline)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Polyline,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_POLYGON_TOOL_ID)
                        .debug_selector(|| DOCUMENT_POLYGON_TOOL_ID.into())
                        .label("Polygon")
                        .tooltip(AnnotationTool::Polygon.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Polygon)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Polygon,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_POLYLENGTH_TOOL_ID)
                        .debug_selector(|| DOCUMENT_POLYLENGTH_TOOL_ID.into())
                        .label("Polylength")
                        .tooltip(AnnotationTool::Polylength.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Polylength)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Polylength,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_AREA_TOOL_ID)
                        .debug_selector(|| DOCUMENT_AREA_TOOL_ID.into())
                        .label("Area")
                        .tooltip(AnnotationTool::Area.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Area)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Area,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_CLOUD_TOOL_ID)
                        .debug_selector(|| DOCUMENT_CLOUD_TOOL_ID.into())
                        .label("Cloud")
                        .tooltip(AnnotationTool::Cloud.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Cloud)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Cloud,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_CLOUD_PLUS_TOOL_ID)
                        .debug_selector(|| DOCUMENT_CLOUD_PLUS_TOOL_ID.into())
                        .label("Cloud+")
                        .tooltip(AnnotationTool::CloudPlus.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::CloudPlus)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::CloudPlus,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_DIMENSION_TOOL_ID)
                        .debug_selector(|| DOCUMENT_DIMENSION_TOOL_ID.into())
                        .label("Dimension")
                        .tooltip(AnnotationTool::Dimension.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Dimension)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Dimension,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_CALLOUT_TOOL_ID)
                        .debug_selector(|| DOCUMENT_CALLOUT_TOOL_ID.into())
                        .label("Callout")
                        .tooltip(AnnotationTool::Callout.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Callout)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Callout,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_REDACT_TOOL_ID)
                        .debug_selector(|| DOCUMENT_REDACT_TOOL_ID.into())
                        .label("Redact")
                        .tooltip(AnnotationTool::Redact.tooltip_label())
                        .selected(annotation_tool == AnnotationTool::Redact)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Redact,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_PEN_TOOL_ID)
                        .debug_selector(|| DOCUMENT_PEN_TOOL_ID.into())
                        .label("Pen")
                        .selected(annotation_tool == AnnotationTool::Pen)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ =
                                workspace.set_annotation_tool(document_id, AnnotationTool::Pen, cx);
                        })),
                )
                .child(
                    Button::new(DOCUMENT_TEXT_BOX_TOOL_ID)
                        .debug_selector(|| DOCUMENT_TEXT_BOX_TOOL_ID.into())
                        .label("Text Box")
                        .selected(annotation_tool == AnnotationTool::TextBox)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::TextBox,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_IMAGE_TOOL_ID)
                        .debug_selector(|| DOCUMENT_IMAGE_TOOL_ID.into())
                        .label("Insert Image")
                        .tooltip("Insert Image (I)")
                        .selected(annotation_tool == AnnotationTool::Image)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            workspace.begin_image_selection(document_id, cx);
                        })),
                )
                .child(
                    Button::new(DOCUMENT_SNAPSHOT_TOOL_ID)
                        .debug_selector(|| DOCUMENT_SNAPSHOT_TOOL_ID.into())
                        .label("Snapshot")
                        .tooltip("Snapshot (G)")
                        .selected(annotation_tool == AnnotationTool::Snapshot)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Snapshot,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(DOCUMENT_LENGTH_TOOL_ID)
                        .debug_selector(|| DOCUMENT_LENGTH_TOOL_ID.into())
                        .label("Length")
                        .selected(annotation_tool == AnnotationTool::Length)
                        .disabled(save_busy)
                        .on_click(cx.listener(move |workspace, _, _, cx| {
                            let _ = workspace.set_annotation_tool(
                                document_id,
                                AnnotationTool::Length,
                                cx,
                            );
                        })),
                )
                .child(
                    Button::new(PAGE_SCALE_TRIGGER_ID)
                        .debug_selector(|| PAGE_SCALE_TRIGGER_ID.into())
                        .label("Set Page Scale")
                        .disabled(save_busy)
                        .on_click(move |_, window, cx| {
                            let _ = page_scale_control.update(cx, |control, cx| {
                                control.open_for(document_id, current_page, window, cx);
                            });
                        }),
                ),
        )
        .into_any_element()
}

impl Render for DocumentWorkspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.page_interactions.clear();
        let page_scale_control = self.ensure_page_scale_control(window, cx);
        let rectangle_property_inspector = self.ensure_rectangle_property_inspector(window, cx);
        let ellipse_property_inspector = self.ensure_ellipse_property_inspector(window, cx);
        let ink_property_inspector = self.ensure_ink_property_inspector(window, cx);
        let engineering_visual_property_inspector =
            self.ensure_engineering_visual_property_inspector(window, cx);
        let straight_line_property_inspector =
            self.ensure_straight_line_property_inspector(window, cx);
        let vertex_path_property_inspector = self.ensure_vertex_path_property_inspector(window, cx);
        let text_box_property_inspector = self.ensure_text_box_property_inspector(window, cx);
        let measurement_property_inspector = self.ensure_measurement_property_inspector(window, cx);
        let dimension_property_inspector = self.ensure_dimension_property_inspector(window, cx);
        let page_scale_pick_instruction = page_scale_control.read(cx).pick_instruction();
        let session_tabs = self
            .sessions
            .iter()
            .map(|session| {
                let session = session.read(cx);
                (
                    session.id,
                    session.title.clone(),
                    session.is_dirty(),
                    session.save_status == NativeDocumentSaveStatus::Saving,
                )
            })
            .collect::<Vec<_>>();
        let active = self
            .active_document_id
            .and_then(|id| self.session(id, cx))
            .map(|session| session.read(cx));
        let root = v_flex()
            .id(DOCUMENT_WORKSPACE_ID)
            .debug_selector(|| DOCUMENT_WORKSPACE_ID.into())
            .size_full()
            .min_h_0()
            .track_focus(&self.workspace_focus)
            .key_context(DOCUMENT_WORKSPACE_CONTEXT)
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
            .on_action(cx.listener(Self::select_length_tool_from_action))
            .on_action(cx.listener(Self::select_all_annotations_from_action))
            .on_action(cx.listener(Self::copy_annotations_from_action))
            .on_action(cx.listener(Self::cut_annotations_from_action))
            .on_action(cx.listener(Self::paste_annotations_from_action))
            .on_action(cx.listener(Self::delete_annotations_from_action))
            .on_action(cx.listener(Self::undo_annotations_from_action))
            .on_action(cx.listener(Self::redo_annotations_from_action))
            .on_action(cx.listener(|workspace, _: &Escape, window, cx| {
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
                } else if workspace.pending_text_box_editor.is_some() {
                    if !workspace.cancel_pending_composite_text_editor(window, cx) {
                        match workspace.commit_pending_text_box(cx) {
                            Ok(_) => workspace.text_box_return_focus.focus(window, cx),
                            Err(error) => {
                                workspace.text_box_commit_error = Some(error);
                                cx.notify();
                            }
                        }
                    }
                } else if let Some(document_id) = workspace.active_document_id
                    && workspace.session(document_id, cx).is_some_and(|session| {
                        session
                            .read(cx)
                            .annotations
                            .vertex_path_pending(document_id.value())
                            || session
                                .read(cx)
                                .annotations
                                .cloud_pending(document_id.value())
                            || session
                                .read(cx)
                                .annotations
                                .cloud_plus_pending(document_id.value())
                    })
                {
                    workspace.finish_vertex_path_from_action(&FinishVertexPath, window, cx);
                } else if let Some(document_id) = workspace.active_document_id
                    && workspace.session(document_id, cx).is_some_and(|session| {
                        session.read(cx).annotations.tool() != AnnotationTool::Select
                            || workspace.active_annotation_pointer.is_some()
                    })
                {
                    workspace.active_annotation_pointer = None;
                    if let Some(session) = workspace.session(document_id, cx).cloned() {
                        session.update(cx, |session, cx| {
                            let _ = session.annotations.cancel(PointerCancelReason::ToolChanged);
                            let _ = session.annotations.set_tool(AnnotationTool::Select);
                            cx.notify();
                        });
                    }
                    workspace.annotation_statuses.remove(&document_id);
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .size_full()
            .min_h(px(360.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground);

        let Some(session) = active else {
            let opening_title = self.sessions.iter().find_map(|session| {
                let session = session.read(cx);
                matches!(session.status, NativeDocumentStatus::Opening)
                    .then(|| session.title.clone())
            });
            let open_failure = self.last_document_open_failure().cloned();
            let error = self.last_file_error.clone().or_else(|| {
                self.sessions.iter().rev().find_map(|session| {
                    let session = session.read(cx);
                    if let NativeDocumentStatus::Failed(error) = &session.status {
                        Some(error.clone())
                    } else {
                        None
                    }
                })
            });
            let open_document = Button::new(DOCUMENT_TAB_OPEN_ID)
                .debug_selector(|| DOCUMENT_TAB_OPEN_ID.into())
                .label("Open PDF")
                .on_click(cx.listener(|workspace, _, _, cx| {
                    workspace.prompt_to_open_documents(cx);
                }));
            let document_actions = h_flex()
                .flex_shrink_0()
                .gap_1()
                .child(open_document)
                .child(self.template_control.clone());
            let inactive_session_tabs = session_tabs
                .iter()
                .enumerate()
                .map(|(ix, (document_id, title, dirty, saving))| {
                    let tab_selector = document_session_tab_id(*document_id);
                    let debug_selector = tab_selector.clone();
                    let tab_focus = self
                        .session_tab_focus_handles
                        .get(document_id)
                        .expect("every retained loading or failed session tab must own focus")
                        .clone();
                    let click_focus = tab_focus.clone();
                    let click_control = cx.entity().downgrade();
                    let close_id = document_session_close_id(*document_id);
                    let close_selector = close_id.clone();
                    let close_accessibility_id = close_id.clone();
                    let close_control = cx.entity().downgrade();
                    let document_id = *document_id;
                    let close_label = document_tab_close_accessible_label(title);
                    let close = accessible_icon_button(
                        Button::new(close_id)
                            .debug_selector(move || close_selector.clone().into())
                            .accessibility_id(close_accessibility_id)
                            .ghost()
                            .xsmall()
                            .size_6()
                            .icon(IconName::Close)
                            .tooltip(close_label.clone())
                            .disabled(*saving)
                            .on_click(move |_: &ClickEvent, _, cx| {
                                let _ = close_control.update(cx, |workspace, cx| {
                                    workspace.request_close_document(document_id, cx);
                                });
                                cx.stop_propagation();
                            }),
                        close_label,
                    );
                    Tab::new()
                        .debug_selector(move || debug_selector.clone().into())
                        .label(if *dirty {
                            format!("* {title}")
                        } else {
                            title.clone()
                        })
                        .aria_label(title.clone())
                        .track_focus(&tab_focus.tab_stop(ix == 0))
                        .on_click(move |_, window, cx| {
                            let _ = click_control.update(cx, |workspace, cx| {
                                workspace.activate_document(document_id, cx);
                            });
                            click_focus.focus(window, cx);
                        })
                        .suffix(close)
                })
                .collect::<Vec<_>>();
            let session_tab_strip = h_flex()
                .id(DOCUMENT_SESSION_TABS_ID)
                .debug_selector(|| DOCUMENT_SESSION_TABS_ID.into())
                .w_full()
                .child(
                    gpui::div()
                        .id("document-workspace-native-session-tab-bar")
                        .debug_selector(|| "document-workspace-native-session-tab-bar".into())
                        .w_full()
                        .child(
                            TabBar::new("document-workspace-session-tabs-component")
                                .max_width(px(190.))
                                .children(inactive_session_tabs)
                                .suffix(document_actions),
                        ),
                );
            return root
                .child(session_tab_strip)
                .child(self.viewer_toolbar.clone())
                .child(
                    v_flex()
                        .id(DOCUMENT_EMPTY_ID)
                        .debug_selector(|| DOCUMENT_EMPTY_ID.into())
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .gap_2()
                        .when_some(opening_title, |view, title| {
                            let status = format!("Opening {title}");
                            view.child(
                                v_flex()
                                    .id(DOCUMENT_OPEN_STATUS_ID)
                                    .debug_selector(|| DOCUMENT_OPEN_STATUS_ID.into())
                                    .role(Role::Status)
                                    .aria_label(status.clone())
                                    .a11y_synthetic_children(|builder| {
                                        builder.parent_node().set_live(Live::Polite)
                                    })
                                    .w(px(240.))
                                    .items_center()
                                    .gap_2()
                                    .child(status)
                                    .child(
                                        gpui::div()
                                            .id(DOCUMENT_OPEN_PROGRESS_ID)
                                            .debug_selector(|| DOCUMENT_OPEN_PROGRESS_ID.into())
                                            .w_full()
                                            .child(
                                                Progress::new("document-open-progress-component")
                                                    .loading(true),
                                            ),
                                    ),
                            )
                        })
                        .when(
                            !self.sessions.iter().any(|session| {
                                matches!(session.read(cx).status, NativeDocumentStatus::Opening)
                            }),
                            |view| view.child("Open a PDF to start"),
                        )
                        .child(
                            Button::new(VIEWPORT_OPEN_DOCUMENT_ID)
                                .debug_selector(|| VIEWPORT_OPEN_DOCUMENT_ID.into())
                                .label("Open")
                                .on_click(cx.listener(|workspace, _, _, cx| {
                                    workspace.prompt_to_open_documents(cx);
                                })),
                        )
                        .when_some(open_failure, |view, failure| {
                            view.child(
                                v_flex()
                                    .w(px(520.))
                                    .gap_2()
                                    .child(
                                        gpui::div()
                                            .id(DOCUMENT_OPEN_ERROR_ALERT_ID)
                                            .debug_selector(|| DOCUMENT_OPEN_ERROR_ALERT_ID.into())
                                            .child(
                                                Alert::error(
                                                    "document-workspace-open-feedback-message",
                                                    failure.presentation(),
                                                )
                                                .title("Some PDFs couldn’t be opened"),
                                            ),
                                    )
                                    .child(
                                        Button::new(DOCUMENT_OPEN_ERROR_DISMISS_ID)
                                            .debug_selector(|| {
                                                DOCUMENT_OPEN_ERROR_DISMISS_ID.into()
                                            })
                                            .accessibility_id(DOCUMENT_OPEN_ERROR_DISMISS_ID)
                                            .outline()
                                            .label("Dismiss")
                                            .on_click(cx.listener(|workspace, _, _, cx| {
                                                workspace.dismiss_document_open_failure(cx);
                                            })),
                                    ),
                            )
                        })
                        .when_some(error, |view, error| {
                            view.child(
                                gpui::div()
                                    .id(DOCUMENT_ERROR_ID)
                                    .debug_selector(|| DOCUMENT_ERROR_ID.into())
                                    .text_sm()
                                    .text_color(cx.theme().danger)
                                    .child(error),
                            )
                        }),
                );
        };

        let (
            document_id,
            title,
            current_page,
            current_page_size,
            current_pdf_page_size,
            current_page_rotation,
            current_coordinate_space,
            current_image,
            annotation_tool,
            _annotation_dirty,
            annotation_undo_depth,
            annotation_redo_depth,
            selected_annotation_id,
            selected_annotation_locked,
            selected_has_unlocked_annotation,
            selected_rectangle,
            selected_ellipse,
            selected_dimension,
            selected_rectangle_stroke_width,
            selected_straight_line,
            selected_vertex_path,
            selected_ink,
            selected_engineering_visual,
            selected_text_box,
            selected_measurement,
            highlight_defaults,
            save_in_progress,
            save_busy,
            ink_mutation_disabled,
            save_failure,
            presentation_error,
            recovery_pending,
            annotation_scene,
            semantic_snap_decision,
            active_selection_marquee,
            current_highlights_precomposed,
            image_assets,
            thumbnails,
            viewer_plan,
            viewer_scroll,
            viewer_pages,
            viewer_snapshot,
        ) = {
            let document_id = session.id;
            let title = session.title.clone();
            let current_page = session.current_page;
            let current_page_size = session.page_sizes[current_page as usize];
            let (current_pdf_page_size, current_page_rotation) = session
                .annotation_page_geometry(current_page)
                .expect("a ready page must retain annotation geometry");
            let current_coordinate_space = session
                .annotation_page_coordinate_space(current_page)
                .expect("a ready page must retain coordinate-space metadata");
            let current_image = session.current_image.clone();
            let annotation_tool = session.annotations.tool();
            let annotation_dirty = session.is_dirty();
            let (annotation_undo_depth, annotation_redo_depth) =
                session.annotations.history_depths(document_id.value());
            let annotation_snapshot = session.annotations.snapshot(document_id.value());
            let selected_annotation_id = annotation_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.selected_id.clone());
            let selected_annotation_locked = annotation_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.selected_id.as_ref())
                .map(|_| session.annotations.selected_is_locked(document_id.value()))
                .unwrap_or(false);
            let selected_has_unlocked_annotation = session
                .annotations
                .selected_has_unlocked(document_id.value());
            let selected_annotations = session
                .annotations
                .selected_annotations_in_document_order(document_id.value());
            let selected_rectangle = match selected_annotations.as_slice() {
                [Annotation::Rectangle(annotation)] => Some(annotation.clone()),
                _ => None,
            };
            let selected_ellipse = match selected_annotations.as_slice() {
                [Annotation::Ellipse(annotation)] => Some(annotation.clone()),
                _ => None,
            };
            let selected_ink = match selected_annotations.as_slice() {
                [Annotation::Pen(annotation)] => Some(annotation.clone()),
                _ => None,
            };
            let selected_engineering_visual = match selected_annotations.as_slice() {
                [Annotation::Arc(annotation)] => Some((
                    annotation.id.clone(),
                    EngineeringVisualPropertyValues::Arc {
                        appearance: annotation.appearance.clone(),
                    },
                    annotation.locked,
                )),
                [Annotation::Cloud(annotation)] => Some((
                    annotation.id.clone(),
                    EngineeringVisualPropertyValues::Cloud {
                        appearance: annotation.appearance.clone(),
                        intensity: annotation.border_effect_intensity(),
                    },
                    annotation.locked,
                )),
                [Annotation::Snapshot(annotation)] => Some((
                    annotation.id.clone(),
                    EngineeringVisualPropertyValues::Snapshot {
                        opacity: annotation.opacity(),
                    },
                    annotation.locked,
                )),
                _ => None,
            };
            let selected_text_box = match selected_annotations.as_slice() {
                [Annotation::TextBox(annotation)] => Some(annotation.clone()),
                _ => None,
            };
            let selected_measurement = match selected_annotations.as_slice() {
                [Annotation::Length(annotation)] => Some((
                    annotation.id.clone(),
                    AnnotationKind::Length,
                    annotation.page_index,
                    annotation.caption(),
                    annotation.calibration().clone(),
                    annotation.locked,
                    session
                        .annotations
                        .document_page_scale(document_id.value(), annotation.page_index)
                        .cloned(),
                )),
                [Annotation::MeasurementPath(annotation)] => Some((
                    annotation.id.clone(),
                    annotation.kind.into(),
                    annotation.page_index,
                    annotation.caption(),
                    annotation.calibration().clone(),
                    annotation.locked,
                    session
                        .annotations
                        .document_page_scale(document_id.value(), annotation.page_index)
                        .cloned(),
                )),
                _ => None,
            };
            let selected_dimension = match selected_annotations.as_slice() {
                [Annotation::Dimension(annotation)] => Some(annotation.clone()),
                _ => None,
            };
            let selected_rectangle_stroke_width = session
                .annotations
                .selected_rectangle_appearance(document_id.value())
                .map(|appearance| appearance.stroke_width_pt());
            let selected_straight_line = match selected_annotations.as_slice() {
                [Annotation::StraightLine(annotation)] => Some(annotation.clone()),
                _ => None,
            };
            let selected_vertex_path = match selected_annotations.as_slice() {
                [Annotation::VertexPath(annotation)] => Some((
                    annotation.id.clone(),
                    PathPropertyKind::from(annotation.kind),
                    annotation.appearance.clone(),
                    annotation.locked,
                )),
                [Annotation::MeasurementPath(annotation)] => Some((
                    annotation.id.clone(),
                    PathPropertyKind::from(annotation.kind),
                    annotation.appearance.clone(),
                    annotation.locked,
                )),
                _ => None,
            };
            let highlight_appearance = session.annotations.highlight_appearance();
            let highlight_defaults = PenAnnotationDefaults {
                color: highlight_appearance.color().to_owned(),
                width_pt: highlight_appearance.width_pt(),
                opacity: highlight_appearance.opacity(),
            };
            let save_in_progress = session.save_status == NativeDocumentSaveStatus::Saving;
            let save_busy = save_in_progress
                || self.pending_save_prompt.is_some_and(|pending| {
                    pending.document_id == document_id
                        && pending.document_generation == session.generation
                });
            let ink_mutation_disabled = save_busy
                || session.pending_rotation_generation.is_some()
                || self.pending_close_document_id == Some(document_id)
                || self.close_after_save_document_id == Some(document_id);
            let save_failure = match &session.save_status {
                NativeDocumentSaveStatus::Failed(failure) => Some(failure.clone()),
                NativeDocumentSaveStatus::Idle | NativeDocumentSaveStatus::Saving => None,
            };
            let presentation_error = session.presentation_error.clone();
            let recovery_pending = session.recovery_generation.is_some();
            let annotation_scene =
                self.annotation_scene_for_session(document_id, current_page, &session);
            let semantic_snap_decision = session.annotations.semantic_snap_decision().cloned();
            let active_selection_marquee = session
                .annotations
                .active_selection_marquee(document_id.value());
            let current_highlights_precomposed =
                session.highlight_composite.annotation_revision == annotation_scene.revision;
            let image_assets = Arc::new(session.image_assets.clone());
            let thumbnails = session
                .page_sizes
                .iter()
                .copied()
                .enumerate()
                .map(|(page_index, page_size)| {
                    let page_index = page_index as u32;
                    let scene = session
                        .annotations
                        .thumbnail_scene(document_id.value(), page_index);
                    (
                        page_index,
                        page_size,
                        session
                            .annotation_page_geometry(page_index)
                            .expect("a thumbnail row must retain annotation geometry"),
                        session
                            .annotation_page_coordinate_space(page_index)
                            .expect("a thumbnail row must retain coordinate-space metadata"),
                        session
                            .thumbnails
                            .iter()
                            .find(|thumbnail| thumbnail.page_index == page_index)
                            .map(|thumbnail| thumbnail.image.clone()),
                        session.highlight_composite.annotation_revision == scene.revision,
                        scene,
                    )
                })
                .collect::<Vec<_>>();
            let viewer_plan = session.viewer.plan_snapshot().cloned();
            let viewer_snapshot = session.viewer.snapshot();
            let viewer_scroll = session.viewer.scroll_handle();
            let viewer_pages = viewer_plan
                .as_ref()
                .map(|plan| {
                    plan.page_layouts
                        .iter()
                        .filter(|layout| plan.visible_pages.contains(&layout.page))
                        .map(|layout| {
                            let scene = self.annotation_scene_for_session(
                                document_id,
                                layout.page as u32,
                                &session,
                            );
                            let tiles = session.viewer.visible_tiles(layout.page);
                            let quality = session.viewer.page_quality(layout.page);
                            let render_error =
                                session.viewer.page_error(layout.page).map(str::to_owned);
                            let expected_tiles = plan
                                .tiles
                                .iter()
                                .filter(|request| request.page == layout.page)
                                .count();
                            let painted_viewer = tiles.first().and_then(|(first, _)| {
                                (expected_tiles > 0
                                    && tiles.len() == expected_tiles
                                    && first.generation == plan.generation
                                    && tiles.iter().all(|(request, _)| {
                                        request.generation == plan.generation
                                            && request.device_scale_millis
                                                == first.device_scale_millis
                                    }))
                                .then_some(
                                    PaintedViewerAuthority {
                                        viewer_generation: plan.generation,
                                        request_generation: session.generation,
                                        resource_generation: session.resource_epoch,
                                        rendered_dpr: first.device_scale_millis as f32 / 1_000.,
                                    },
                                )
                            });
                            (
                                *layout,
                                session.page_sizes[layout.page],
                                session
                                    .annotation_page_geometry(layout.page as u32)
                                    .expect("a viewer page must retain annotation geometry"),
                                session
                                    .annotation_page_coordinate_space(layout.page as u32)
                                    .expect("a viewer page must retain coordinate-space metadata"),
                                scene,
                                layout.page == current_page as usize
                                    || (expected_tiles > 0 && tiles.len() == expected_tiles),
                                tiles,
                                painted_viewer,
                                quality,
                                render_error,
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (
                document_id,
                title,
                current_page,
                current_page_size,
                current_pdf_page_size,
                current_page_rotation,
                current_coordinate_space,
                current_image,
                annotation_tool,
                annotation_dirty,
                annotation_undo_depth,
                annotation_redo_depth,
                selected_annotation_id,
                selected_annotation_locked,
                selected_has_unlocked_annotation,
                selected_rectangle,
                selected_ellipse,
                selected_dimension,
                selected_rectangle_stroke_width,
                selected_straight_line,
                selected_vertex_path,
                selected_ink,
                selected_engineering_visual,
                selected_text_box,
                selected_measurement,
                highlight_defaults,
                save_in_progress,
                save_busy,
                ink_mutation_disabled,
                save_failure,
                presentation_error,
                recovery_pending,
                annotation_scene,
                semantic_snap_decision,
                active_selection_marquee,
                current_highlights_precomposed,
                image_assets,
                thumbnails,
                viewer_plan,
                viewer_scroll,
                viewer_pages,
                viewer_snapshot,
            )
        };
        if let Some(dimension) = selected_dimension.as_ref() {
            let snapshot = DimensionPropertySnapshot {
                document_id,
                annotation_id: dimension.id.clone(),
                expected_revision: annotation_scene.revision,
                offset_pt: dimension.dimension_line_offset(),
                appearance: dimension.appearance.clone(),
                locked: dimension.locked,
                mutation_disabled: save_busy,
            };
            if dimension_property_inspector.read(cx).snapshot().is_none_or(|current| current != &snapshot) {
                dimension_property_inspector.update(cx, |inspector, cx| inspector.sync(snapshot, window, cx));
            }
        } else if dimension_property_inspector.read(cx).snapshot().is_some() {
            self.dimension_property_inspector_open = false;
            dimension_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        let thumbnail_scroll = self.thumbnail_scroll.clone();
        let redact_pending = annotation_tool == AnnotationTool::Redact
            || annotation_scene
                .redacts
                .iter()
                .any(|redact| redact.selected);
        if let Some(rectangle) = selected_rectangle.as_ref() {
            let snapshot = RectanglePropertySnapshot {
                document_id,
                annotation_id: rectangle.id.clone(),
                rect: rectangle.rect,
                rotation_degrees: rectangle.rotation_degrees,
                appearance: rectangle.appearance.clone(),
                locked: rectangle.locked,
                mutation_disabled: save_busy,
            };
            let needs_sync = rectangle_property_inspector
                .read(cx)
                .snapshot()
                .is_none_or(|current| current != &snapshot);
            if needs_sync {
                rectangle_property_inspector.update(cx, |inspector, cx| {
                    inspector.sync(snapshot, window, cx);
                });
            }
        } else if rectangle_property_inspector.read(cx).snapshot().is_some() {
            rectangle_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        if let Some(ellipse) = selected_ellipse.as_ref() {
            let snapshot = RectanglePropertySnapshot {
                document_id,
                annotation_id: ellipse.id.clone(),
                rect: ellipse.rect,
                rotation_degrees: ellipse.rotation_degrees,
                appearance: ellipse.appearance.clone(),
                locked: ellipse.locked,
                mutation_disabled: save_busy,
            };
            let needs_sync = ellipse_property_inspector
                .read(cx)
                .snapshot()
                .is_none_or(|current| current != &snapshot);
            if needs_sync {
                ellipse_property_inspector.update(cx, |inspector, cx| {
                    inspector.sync(snapshot, window, cx);
                });
            }
        } else if ellipse_property_inspector.read(cx).snapshot().is_some() {
            ellipse_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        if let Some(ink) = selected_ink.as_ref() {
            let snapshot = InkPropertySnapshot {
                document_id,
                annotation_id: ink.id.clone(),
                expected_revision: annotation_scene.revision,
                tool: ink.tool(),
                appearance: ink.appearance.clone(),
                smooth_curves: ink.smooth_curves,
                blend_mode: ink.blend_mode(),
                locked: ink.locked,
                mutation_disabled: ink_mutation_disabled,
            };
            let needs_sync = ink_property_inspector
                .read(cx)
                .snapshot()
                .is_none_or(|current| current != &snapshot);
            if needs_sync {
                ink_property_inspector.update(cx, |inspector, cx| {
                    inspector.sync(snapshot, window, cx);
                });
            }
        } else if ink_property_inspector.read(cx).snapshot().is_some() {
            self.ink_property_inspector_open = false;
            ink_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        if let Some((annotation_id, values, locked)) = selected_engineering_visual.as_ref() {
            let snapshot = EngineeringVisualPropertySnapshot {
                document_id,
                annotation_id: annotation_id.clone(),
                expected_revision: annotation_scene.revision,
                values: values.clone(),
                locked: *locked,
                mutation_disabled: ink_mutation_disabled
                    || self.pending_text_box_editor.as_ref().is_some_and(|editor| {
                        editor.document_id == document_id
                    }),
            };
            let needs_sync = engineering_visual_property_inspector
                .read(cx)
                .snapshot()
                .is_none_or(|current| current != &snapshot);
            if needs_sync {
                engineering_visual_property_inspector.update(cx, |inspector, cx| {
                    inspector.sync(snapshot, window, cx);
                });
            }
        } else if engineering_visual_property_inspector
            .read(cx)
            .snapshot()
            .is_some()
        {
            self.engineering_visual_property_inspector_open = false;
            engineering_visual_property_inspector
                .update(cx, |inspector, cx| inspector.clear(cx));
        }
        if let Some(annotation) = selected_straight_line.as_ref() {
            let snapshot = StraightLinePropertySnapshot {
                document_id,
                annotation_id: annotation.id.clone(),
                expected_revision: annotation_scene.revision,
                kind: annotation.kind,
                appearance: annotation.appearance.clone(),
                locked: annotation.locked,
                mutation_disabled: ink_mutation_disabled,
            };
            let needs_sync = straight_line_property_inspector
                .read(cx)
                .snapshot()
                .is_none_or(|current| current != &snapshot);
            if needs_sync {
                straight_line_property_inspector.update(cx, |inspector, cx| {
                    inspector.sync(snapshot, window, cx);
                });
            }
        } else if straight_line_property_inspector
            .read(cx)
            .snapshot()
            .is_some()
        {
            self.straight_line_property_inspector_open = false;
            straight_line_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        if let Some((annotation_id, kind, appearance, locked)) = selected_vertex_path.as_ref() {
            let snapshot = VertexPathPropertySnapshot { document_id, annotation_id: annotation_id.clone(), expected_revision: annotation_scene.revision, kind: *kind, appearance: appearance.clone(), locked: *locked, mutation_disabled: ink_mutation_disabled };
            let needs_sync = vertex_path_property_inspector.read(cx).snapshot().is_none_or(|current| current != &snapshot);
            if needs_sync { vertex_path_property_inspector.update(cx, |inspector, cx| inspector.sync(snapshot, window, cx)); }
        } else if vertex_path_property_inspector.read(cx).snapshot().is_some() {
            self.vertex_path_property_inspector_open = false;
            vertex_path_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        if let Some(text_box) = selected_text_box.as_ref() {
            let snapshot = TextBoxPropertySnapshot {
                document_id,
                annotation_id: text_box.id.clone(),
                expected_revision: annotation_scene.revision,
                style: text_box.style().clone(),
                locked: text_box.locked,
                mutation_disabled: ink_mutation_disabled,
            };
            let needs_sync = text_box_property_inspector
                .read(cx)
                .snapshot()
                .is_none_or(|current| current != &snapshot);
            if needs_sync {
                text_box_property_inspector.update(cx, |inspector, cx| {
                    inspector.sync(snapshot, window, cx);
                });
            }
        } else if text_box_property_inspector.read(cx).snapshot().is_some() {
            self.text_box_property_inspector_open = false;
            text_box_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        if let Some((id, kind, page_index, caption, calibration, locked, page_scale)) =
            selected_measurement.as_ref()
        {
            let snapshot = MeasurementPropertySnapshot {
                document_id,
                annotation_id: id.clone(),
                annotation_kind: *kind,
                expected_revision: annotation_scene.revision,
                page_index: *page_index,
                caption: caption.clone(),
                page_scale: page_scale.clone(),
                unit: calibration.unit().to_owned(),
                precision: calibration.scale_precision(),
                show_caption: calibration.show_caption(),
                locked: *locked,
                mutation_disabled: ink_mutation_disabled,
            };
            let needs_sync = measurement_property_inspector
                .read(cx)
                .snapshot()
                .is_none_or(|current| current != &snapshot);
            if needs_sync {
                measurement_property_inspector.update(cx, |inspector, cx| {
                    inspector.sync(snapshot, cx);
                });
            }
        } else if measurement_property_inspector.read(cx).snapshot().is_some() {
            self.measurement_property_inspector_open = false;
            measurement_property_inspector.update(cx, |inspector, cx| inspector.clear(cx));
        }
        let has_viewer_pages = !viewer_pages.is_empty();
        let viewer_busy = viewer_snapshot.queued_tiles > 0 || viewer_snapshot.active_tiles > 0;
        let viewer_status_text = if viewer_busy {
            format!(
                "Rendering page {} · {} queued · {} active",
                current_page + 1,
                viewer_snapshot.queued_tiles,
                viewer_snapshot.active_tiles
            )
        } else {
            let quality = match viewer_snapshot.current_quality {
                Some(ViewerRenderQuality::Preview) => "Preview",
                Some(ViewerRenderQuality::Full) => "Full quality",
                Some(ViewerRenderQuality::Detail) => "Detail quality",
                None => "Waiting for page",
            };
            format!("Page {} · {quality}", current_page + 1)
        };
        let viewer_status_surface = v_flex()
            .id(DOCUMENT_VIEWER_STATUS_ID)
            .debug_selector(|| DOCUMENT_VIEWER_STATUS_ID.into())
            .role(Role::Status)
            .aria_label(viewer_status_text.clone())
            .a11y_synthetic_children(|builder| builder.parent_node().set_live(Live::Polite))
            .absolute()
            .right_2()
            .bottom_2()
            .w(px(220.))
            .gap_1()
            .p_2()
            .rounded(cx.theme().radius)
            .bg(cx.theme().popover)
            .text_xs()
            .child(viewer_status_text)
            .when(viewer_busy, |status| {
                status.child(
                    gpui::div()
                        .id(DOCUMENT_VIEWER_PROGRESS_ID)
                        .debug_selector(|| DOCUMENT_VIEWER_PROGRESS_ID.into())
                        .w_full()
                        .child(Progress::new("document-viewer-progress-component").loading(true)),
                )
            });
        let viewport_control = cx.entity().downgrade();
        let viewport_observer = canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let _ = viewport_control.update(cx, |workspace, cx| {
                    workspace.viewport_bounds.insert(document_id, bounds);
                    workspace.observe_viewport(
                        document_id,
                        f32::from(bounds.size.width),
                        f32::from(bounds.size.height),
                        window,
                        cx,
                    );
                });
            },
        )
        .absolute()
        .inset_0();
        let selection_color = cx.theme().primary;
        let pending_text_box_input = self
            .pending_text_box_editor
            .as_ref()
            .filter(|editor| editor.document_id == document_id)
            .map(|editor| editor.input.clone());
        let highlight_open_control = cx.entity().downgrade();
        let highlight_color_control = cx.entity().downgrade();
        let highlight_width_control = cx.entity().downgrade();
        let highlight_opacity_control = cx.entity().downgrade();
        let highlight_tool_control = Popover::new(DOCUMENT_HIGHLIGHT_SETTINGS_ID)
            .anchor(Anchor::BottomLeft)
            .open(self.annotation_highlight_settings_open)
            .on_open_change(move |open, _, cx| {
                let _ = highlight_open_control.update(cx, |workspace, cx| {
                    workspace.annotation_highlight_settings_open = *open;
                    if *open {
                        let _ = workspace.set_annotation_tool(
                            document_id,
                            AnnotationTool::Highlight,
                            cx,
                        );
                    }
                    cx.notify();
                });
            })
            .trigger(
                Button::new(DOCUMENT_HIGHLIGHT_TOOL_ID)
                    .debug_selector(|| DOCUMENT_HIGHLIGHT_TOOL_ID.into())
                    .label("Highlight")
                    .tooltip("Highlight (H)")
                    .selected(annotation_tool == AnnotationTool::Highlight)
                    .disabled(save_busy),
            )
            .content(move |_, _, _| {
                let selected_color = highlight_defaults.color.clone();
                let selected_width = highlight_defaults.width_pt;
                let selected_opacity = highlight_defaults.opacity;
                let color_for_width_12 = selected_color.clone();
                let color_for_width_18 = selected_color.clone();
                let color_for_opacity_50 = selected_color.clone();
                let color_for_opacity_100 = selected_color.clone();
                let yellow = highlight_color_control.clone();
                let green = highlight_color_control.clone();
                let width_12 = highlight_width_control.clone();
                let width_18 = highlight_width_control.clone();
                let opacity_50 = highlight_opacity_control.clone();
                let opacity_100 = highlight_opacity_control.clone();
                v_flex()
                    .gap_2()
                    .child(
                        gpui::div()
                            .text_sm()
                            .font_semibold()
                            .child("Highlight defaults"),
                    )
                    .child(
                        ButtonGroup::new("document-workspace-highlight-colors")
                            .child(
                                Button::new(DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID)
                                    .debug_selector(|| DOCUMENT_HIGHLIGHT_COLOR_YELLOW_ID.into())
                                    .label("Yellow")
                                    .selected(selected_color == "#ffff00")
                                    .on_click(move |_, _, cx| {
                                        let _ = yellow.update(cx, |workspace, cx| {
                                            workspace.set_highlight_defaults(
                                                document_id,
                                                "#ffff00",
                                                selected_width,
                                                selected_opacity,
                                                cx,
                                            )
                                        });
                                    }),
                            )
                            .child(
                                Button::new(DOCUMENT_HIGHLIGHT_COLOR_GREEN_ID)
                                    .debug_selector(|| DOCUMENT_HIGHLIGHT_COLOR_GREEN_ID.into())
                                    .label("Green")
                                    .selected(selected_color == "#00ff00")
                                    .on_click(move |_, _, cx| {
                                        let _ = green.update(cx, |workspace, cx| {
                                            workspace.set_highlight_defaults(
                                                document_id,
                                                "#00ff00",
                                                selected_width,
                                                selected_opacity,
                                                cx,
                                            )
                                        });
                                    }),
                            ),
                    )
                    .child(
                        ButtonGroup::new("document-workspace-highlight-widths")
                            .child(
                                Button::new(DOCUMENT_HIGHLIGHT_WIDTH_12_ID)
                                    .debug_selector(|| DOCUMENT_HIGHLIGHT_WIDTH_12_ID.into())
                                    .label("12 pt")
                                    .selected(selected_width == 12.)
                                    .on_click(move |_, _, cx| {
                                        let _ = width_12.update(cx, |workspace, cx| {
                                            workspace.set_highlight_defaults(
                                                document_id,
                                                &color_for_width_12,
                                                12.,
                                                selected_opacity,
                                                cx,
                                            )
                                        });
                                    }),
                            )
                            .child(
                                Button::new(DOCUMENT_HIGHLIGHT_WIDTH_18_ID)
                                    .debug_selector(|| DOCUMENT_HIGHLIGHT_WIDTH_18_ID.into())
                                    .label("18 pt")
                                    .selected(selected_width == 18.)
                                    .on_click(move |_, _, cx| {
                                        let _ = width_18.update(cx, |workspace, cx| {
                                            workspace.set_highlight_defaults(
                                                document_id,
                                                &color_for_width_18,
                                                18.,
                                                selected_opacity,
                                                cx,
                                            )
                                        });
                                    }),
                            ),
                    )
                    .child(
                        ButtonGroup::new("document-workspace-highlight-opacities")
                            .child(
                                Button::new(DOCUMENT_HIGHLIGHT_OPACITY_50_ID)
                                    .debug_selector(|| DOCUMENT_HIGHLIGHT_OPACITY_50_ID.into())
                                    .label("50%")
                                    .selected(selected_opacity == 0.5)
                                    .on_click(move |_, _, cx| {
                                        let _ = opacity_50.update(cx, |workspace, cx| {
                                            workspace.set_highlight_defaults(
                                                document_id,
                                                &color_for_opacity_50,
                                                selected_width,
                                                0.5,
                                                cx,
                                            )
                                        });
                                    }),
                            )
                            .child(
                                Button::new(DOCUMENT_HIGHLIGHT_OPACITY_100_ID)
                                    .debug_selector(|| DOCUMENT_HIGHLIGHT_OPACITY_100_ID.into())
                                    .label("100%")
                                    .selected(selected_opacity == 1.)
                                    .on_click(move |_, _, cx| {
                                        let _ = opacity_100.update(cx, |workspace, cx| {
                                            workspace.set_highlight_defaults(
                                                document_id,
                                                &color_for_opacity_100,
                                                selected_width,
                                                1.,
                                                cx,
                                            )
                                        });
                                    }),
                            ),
                    )
            });
        let semantic_settings = self.semantic_snap_settings;
        let snap_open_control = cx.entity().downgrade();
        let snap_markup_control = cx.entity().downgrade();
        let snap_endpoint_control = cx.entity().downgrade();
        let snap_midpoint_control = cx.entity().downgrade();
        let snap_center_control = cx.entity().downgrade();
        let snap_intersection_control = cx.entity().downgrade();
        let snap_nearest_control = cx.entity().downgrade();
        let semantic_snap_control = Popover::new("document-workspace-snap-settings-owner")
            .anchor(Anchor::BottomLeft)
            .open(self.semantic_snap_settings_open)
            .on_open_change(move |open, _, cx| {
                let _ = snap_open_control.update(cx, |workspace, cx| {
                    workspace.semantic_snap_settings_open = *open;
                    cx.notify();
                });
            })
            .trigger(
                Button::new(DOCUMENT_SNAP_SETTINGS_ID)
                    .debug_selector(|| DOCUMENT_SNAP_SETTINGS_ID.into())
                    .label("Snap")
                    .tooltip("Snap settings")
                    .selected(self.semantic_snap_settings_open)
                    .disabled(save_busy),
            )
            .content(move |_, _, _| {
                let snap_markup_control = snap_markup_control.clone();
                let snap_endpoint_control = snap_endpoint_control.clone();
                let snap_midpoint_control = snap_midpoint_control.clone();
                let snap_center_control = snap_center_control.clone();
                let snap_intersection_control = snap_intersection_control.clone();
                let snap_nearest_control = snap_nearest_control.clone();
                v_flex()
                    .id(DOCUMENT_SNAP_POPOVER_ID)
                    .debug_selector(|| DOCUMENT_SNAP_POPOVER_ID.into())
                    .gap_2()
                    .p_3()
                    .child(gpui::div().text_sm().font_semibold().child("Snap to"))
                    .child(
                        Checkbox::new(DOCUMENT_SNAP_MARKUP_ID)
                            .debug_selector(|| DOCUMENT_SNAP_MARKUP_ID.into())
                            .label("Markup")
                            .checked(semantic_settings.annotations_enabled())
                            .on_click(move |checked, _, cx| {
                                let _ = snap_markup_control.update(cx, |workspace, cx| {
                                    workspace.set_semantic_snap_annotations_enabled(*checked, cx);
                                });
                            }),
                    )
                    .child(gpui::div().text_sm().font_semibold().child("Snap points"))
                    .child(
                        Checkbox::new(DOCUMENT_SNAP_ENDPOINT_ID)
                            .debug_selector(|| DOCUMENT_SNAP_ENDPOINT_ID.into())
                            .label("Ends")
                            .checked(
                                semantic_settings.is_target_selected(SemanticSnapTarget::Endpoint),
                            )
                            .disabled(!semantic_settings.annotations_enabled())
                            .on_click(move |checked, _, cx| {
                                let _ = snap_endpoint_control.update(cx, |workspace, cx| {
                                    workspace.set_semantic_snap_target(
                                        SemanticSnapTarget::Endpoint,
                                        *checked,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(
                        Checkbox::new(DOCUMENT_SNAP_MIDPOINT_ID)
                            .debug_selector(|| DOCUMENT_SNAP_MIDPOINT_ID.into())
                            .label("Midpoints")
                            .checked(
                                semantic_settings.is_target_selected(SemanticSnapTarget::Midpoint),
                            )
                            .disabled(!semantic_settings.annotations_enabled())
                            .on_click(move |checked, _, cx| {
                                let _ = snap_midpoint_control.update(cx, |workspace, cx| {
                                    workspace.set_semantic_snap_target(
                                        SemanticSnapTarget::Midpoint,
                                        *checked,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(
                        Checkbox::new(DOCUMENT_SNAP_CENTER_ID)
                            .debug_selector(|| DOCUMENT_SNAP_CENTER_ID.into())
                            .label("Centers")
                            .checked(
                                semantic_settings.is_target_selected(SemanticSnapTarget::Center),
                            )
                            .disabled(!semantic_settings.annotations_enabled())
                            .on_click(move |checked, _, cx| {
                                let _ = snap_center_control.update(cx, |workspace, cx| {
                                    workspace.set_semantic_snap_target(
                                        SemanticSnapTarget::Center,
                                        *checked,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(
                        Checkbox::new(DOCUMENT_SNAP_INTERSECTION_ID)
                            .debug_selector(|| DOCUMENT_SNAP_INTERSECTION_ID.into())
                            .label("Intersections")
                            .checked(
                                semantic_settings
                                    .is_target_selected(SemanticSnapTarget::Intersection),
                            )
                            .disabled(!semantic_settings.annotations_enabled())
                            .on_click(move |checked, _, cx| {
                                let _ = snap_intersection_control.update(cx, |workspace, cx| {
                                    workspace.set_semantic_snap_target(
                                        SemanticSnapTarget::Intersection,
                                        *checked,
                                        cx,
                                    );
                                });
                            }),
                    )
                    .child(
                        Checkbox::new(DOCUMENT_SNAP_NEAREST_ID)
                            .debug_selector(|| DOCUMENT_SNAP_NEAREST_ID.into())
                            .label("Nearest")
                            .checked(
                                semantic_settings.is_target_selected(SemanticSnapTarget::Nearest),
                            )
                            .disabled(!semantic_settings.annotations_enabled())
                            .on_click(move |checked, _, cx| {
                                let _ = snap_nearest_control.update(cx, |workspace, cx| {
                                    workspace.set_semantic_snap_target(
                                        SemanticSnapTarget::Nearest,
                                        *checked,
                                        cx,
                                    );
                                });
                            }),
                    )
            });
        let tool_group = annotation_tool_group(
            document_id,
            current_page,
            annotation_tool,
            save_busy,
            self.signature_popover_open,
            self.signature_prepare_state.clone(),
            self.drawn_signature.clone(),
            page_scale_control.downgrade(),
            cx,
        );
        let history_group = ButtonGroup::new("document-workspace-annotation-history")
            .child(
                Button::new(DOCUMENT_ANNOTATION_UNDO_ID)
                    .debug_selector(|| DOCUMENT_ANNOTATION_UNDO_ID.into())
                    .label("Undo")
                    .disabled(annotation_undo_depth == 0 || save_busy)
                    .on_click(cx.listener(move |workspace, _, _, cx| {
                        let _ = workspace.undo_annotations(document_id, cx);
                    })),
            )
            .child(
                Button::new(DOCUMENT_ANNOTATION_REDO_ID)
                    .debug_selector(|| DOCUMENT_ANNOTATION_REDO_ID.into())
                    .label("Redo")
                    .disabled(annotation_redo_depth == 0 || save_busy)
                    .on_click(cx.listener(move |workspace, _, _, cx| {
                        let _ = workspace.redo_annotations(document_id, cx);
                    })),
            );
        let rectangle_properties_control = cx.entity().downgrade();
        let rectangle_properties_button = Button::new(DOCUMENT_RECTANGLE_PROPERTIES_ID)
            .debug_selector(|| DOCUMENT_RECTANGLE_PROPERTIES_ID.into())
            .label("Properties")
            .tooltip("Show or hide Rectangle properties")
            .selected(self.rectangular_shape_property_inspector_open)
            .disabled(selected_rectangle.is_none())
            .on_click(move |_, window, cx| {
                let _ = rectangle_properties_control.update(cx, |workspace, cx| {
                    workspace.set_rectangular_shape_property_inspector_open(
                        !workspace.rectangular_shape_property_inspector_open,
                        window,
                        cx,
                    );
                });
            });
        let ellipse_properties_control = cx.entity().downgrade();
        let ellipse_properties_button = Button::new(DOCUMENT_ELLIPSE_PROPERTIES_ID)
            .debug_selector(|| DOCUMENT_ELLIPSE_PROPERTIES_ID.into())
            .label("Properties")
            .tooltip("Show or hide Ellipse properties")
            .selected(self.rectangular_shape_property_inspector_open)
            .disabled(selected_ellipse.is_none())
            .on_click(move |_, window, cx| {
                let _ = ellipse_properties_control.update(cx, |workspace, cx| {
                    workspace.set_rectangular_shape_property_inspector_open(
                        !workspace.rectangular_shape_property_inspector_open,
                        window,
                        cx,
                    );
                });
            });
        let stroke_update_control = cx.entity().downgrade();
        let stroke_open_control = cx.entity().downgrade();
        let stroke_control = Popover::new("document-workspace-rectangle-stroke-popover")
            .anchor(Anchor::BottomLeft)
            .open(self.annotation_stroke_menu_open)
            .on_open_change(move |open, _, cx| {
                let _ = stroke_open_control.update(cx, |workspace, cx| {
                    workspace.annotation_stroke_menu_open = *open;
                    cx.notify();
                });
            })
            .trigger(
                Button::new(DOCUMENT_RECTANGLE_STROKE_ID)
                    .debug_selector(|| DOCUMENT_RECTANGLE_STROKE_ID.into())
                    .label(
                        selected_rectangle_stroke_width
                            .map(|width| format!("Line {width} pt"))
                            .unwrap_or_else(|| "Line width".to_owned()),
                    )
                    .disabled(selected_rectangle_stroke_width.is_none() || save_busy),
            )
            .content(move |_, _, cx| {
                let popover = cx.entity();
                h_flex().gap_1().children([1., 1.5, 3., 4.].map(|width| {
                    let control = stroke_update_control.clone();
                    let popover = popover.clone();
                    let id = format!("document-workspace-stroke-{width}");
                    let selector = id.clone();
                    Button::new(id)
                        .debug_selector(move || selector.clone().into())
                        .label(format!("{width} pt"))
                        .selected(selected_rectangle_stroke_width == Some(width))
                        .on_click(move |_, window, cx| {
                            let _ = control.update(cx, |workspace, cx| {
                                workspace.set_selected_rectangle_stroke_width(
                                    document_id,
                                    width,
                                    cx,
                                )
                            });
                            popover.update(cx, |popover, cx| popover.dismiss(window, cx));
                        })
                }))
            });
        let straight_line_property_control = cx.entity().downgrade();
        let straight_line_properties = selected_straight_line.as_ref().map(|annotation| {
            Button::new(DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID)
                .debug_selector(|| DOCUMENT_STRAIGHT_LINE_PROPERTIES_ID.into())
                .label("Properties")
                .tooltip(format!(
                    "Show or hide {} properties",
                    match annotation.kind {
                        LineKind::Line => "Line",
                        LineKind::Arrow => "Arrow",
                    }
                ))
                .selected(self.straight_line_property_inspector_open)
                .on_click(move |_, window, cx| {
                    let _ = straight_line_property_control.update(cx, |workspace, cx| {
                        workspace.set_straight_line_property_inspector_open(
                            !workspace.straight_line_property_inspector_open,
                            window,
                            cx,
                        );
                    });
                })
        });
        let vertex_path_properties_control = cx.entity().downgrade();
        let vertex_path_properties = selected_vertex_path.as_ref().map(|(_, kind, _, _)| {
            Button::new(DOCUMENT_VERTEX_PATH_PROPERTIES_ID)
                .debug_selector(|| DOCUMENT_VERTEX_PATH_PROPERTIES_ID.into())
                .label("Properties")
                .tooltip(format!("Show or hide {kind:?} properties"))
                .selected(self.vertex_path_property_inspector_open)
                .on_click(move |_, window, cx| { let _ = vertex_path_properties_control.update(cx, |workspace, cx| workspace.set_vertex_path_property_inspector_open(!workspace.vertex_path_property_inspector_open, window, cx)); })
        });
        let ink_properties_control = cx.entity().downgrade();
        let ink_properties_button = selected_ink.as_ref().map(|ink| {
            Button::new(DOCUMENT_INK_PROPERTIES_ID)
                .debug_selector(|| DOCUMENT_INK_PROPERTIES_ID.into())
                .label("Properties")
                .tooltip(match ink.tool() {
                    InkTool::Pen => "Show or hide Pen properties",
                    InkTool::Highlight => "Show or hide Highlight properties",
                })
                .selected(self.ink_property_inspector_open)
                .on_click(move |_, window, cx| {
                    let _ = ink_properties_control.update(cx, |workspace, cx| {
                        workspace.set_ink_property_inspector_open(
                            !workspace.ink_property_inspector_open,
                            window,
                            cx,
                        );
                    });
                })
        });
        let engineering_visual_properties_control = cx.entity().downgrade();
        let engineering_visual_properties_button =
            selected_engineering_visual.as_ref().map(|(_, values, _)| {
                let kind = values.kind();
                Button::new(DOCUMENT_ENGINEERING_VISUAL_PROPERTIES_ID)
                    .debug_selector(|| DOCUMENT_ENGINEERING_VISUAL_PROPERTIES_ID.into())
                    .label("Properties")
                    .tooltip(format!("Show or hide {} properties", kind.label()))
                    .selected(self.engineering_visual_property_inspector_open)
                    .on_click(move |_, window, cx| {
                        let _ = engineering_visual_properties_control.update(
                            cx,
                            |workspace, cx| {
                                workspace.set_engineering_visual_property_inspector_open(
                                    !workspace.engineering_visual_property_inspector_open,
                                    window,
                                    cx,
                                );
                            },
                        );
                    })
            });
        let text_box_properties_control = cx.entity().downgrade();
        let text_box_properties_button = selected_text_box.as_ref().map(|_| {
            Button::new(DOCUMENT_TEXT_BOX_PROPERTIES_ID)
                .debug_selector(|| DOCUMENT_TEXT_BOX_PROPERTIES_ID.into())
                .label("Properties")
                .tooltip("Show or hide Text Box properties")
                .selected(self.text_box_property_inspector_open)
                .on_click(move |_, window, cx| {
                    let _ = text_box_properties_control.update(cx, |workspace, cx| {
                        workspace.set_text_box_property_inspector_open(
                            !workspace.text_box_property_inspector_open,
                            window,
                            cx,
                        );
                    });
                })
        });
        let measurement_properties_control = cx.entity().downgrade();
        let measurement_properties_button = selected_measurement.as_ref().map(|_| {
            Button::new(DOCUMENT_MEASUREMENT_PROPERTIES_ID)
                .debug_selector(|| DOCUMENT_MEASUREMENT_PROPERTIES_ID.into())
                .label("Measurement Properties")
                .tooltip("Show or hide measurement properties")
                .selected(self.measurement_property_inspector_open)
                .on_click(move |_, window, cx| {
                    let _ = measurement_properties_control.update(cx, |workspace, cx| {
                        workspace.set_measurement_property_inspector_open(
                            !workspace.measurement_property_inspector_open,
                            window,
                            cx,
                        );
                    });
                })
        });
        let dimension_properties = selected_dimension.as_ref().map(|_| {
            let control = cx.entity().downgrade();
            Button::new(DOCUMENT_DIMENSION_PROPERTIES_ID)
                .debug_selector(|| DOCUMENT_DIMENSION_PROPERTIES_ID.into())
                .label("Dimension Properties")
                .tooltip("Show or hide Dimension properties")
                .selected(self.dimension_property_inspector_open)
                .on_click(move |_, window, cx| {
                    let _ = control.update(cx, |workspace, cx| workspace.set_dimension_property_inspector_open(!workspace.dimension_property_inspector_open, window, cx));
                })
        });
        let fixed_inspector_width = px(300.);
        let fixed_inspector_range = px(220.)..px(420.);
        let scaled_inspector_width = window.rem_size() * 18.75;
        let scaled_inspector_range =
            window.rem_size() * 13.75..window.rem_size() * 26.25;
        let active_inspector = if self.rectangular_shape_property_inspector_open
            && (selected_rectangle.is_some() || selected_ellipse.is_some())
        {
            Some(ActiveInspector {
                kind: if selected_ellipse.is_some() {
                    ActiveInspectorKind::Ellipse
                } else {
                    ActiveInspectorKind::Rectangle
                },
                initial_width: fixed_inspector_width,
                width_range: fixed_inspector_range.clone(),
            })
        } else if self.straight_line_property_inspector_open && selected_straight_line.is_some() {
            Some(ActiveInspector {
                kind: ActiveInspectorKind::StraightLine,
                initial_width: scaled_inspector_width,
                width_range: scaled_inspector_range.clone(),
            })
        } else if self.vertex_path_property_inspector_open && selected_vertex_path.is_some() {
            Some(ActiveInspector {
                kind: ActiveInspectorKind::VertexPath,
                initial_width: scaled_inspector_width,
                width_range: scaled_inspector_range.clone(),
            })
        } else if self.ink_property_inspector_open && selected_ink.is_some() {
            Some(ActiveInspector {
                kind: ActiveInspectorKind::Ink,
                initial_width: fixed_inspector_width,
                width_range: fixed_inspector_range.clone(),
            })
        } else if self.engineering_visual_property_inspector_open
            && selected_engineering_visual.is_some()
        {
            Some(ActiveInspector {
                kind: ActiveInspectorKind::EngineeringVisual,
                initial_width: fixed_inspector_width,
                width_range: fixed_inspector_range.clone(),
            })
        } else if self.text_box_property_inspector_open && selected_text_box.is_some() {
            Some(ActiveInspector {
                kind: ActiveInspectorKind::TextBox,
                initial_width: fixed_inspector_width,
                width_range: fixed_inspector_range.clone(),
            })
        } else if self.measurement_property_inspector_open && selected_measurement.is_some() {
            Some(ActiveInspector {
                kind: ActiveInspectorKind::Measurement,
                initial_width: fixed_inspector_width,
                width_range: fixed_inspector_range,
            })
        } else if self.dimension_property_inspector_open && selected_dimension.is_some() {
            Some(ActiveInspector {
                kind: ActiveInspectorKind::Dimension,
                initial_width: scaled_inspector_width,
                width_range: scaled_inspector_range,
            })
        } else {
            None
        };
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
                    let is_dragged = self.session_tab_pointer_drag.as_ref().is_some_and(|drag| {
                        drag.activated && drag.document_id == tab_document_id
                    });
                    let is_drop_target = self.session_tab_pointer_drag.as_ref().is_some_and(|drag| {
                        drag.activated && drag.over_document_id == tab_document_id
                    });
                    let accessibility_label = if dirty {
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
                            .debug_selector(move || drag_selector.clone().into())
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
                            .debug_selector(move || drop_selector.clone().into())
                            .absolute()
                            .left_0()
                            .right_0()
                            .top_0()
                            .bottom_0()
                    });
                    let click_control = cx.entity().downgrade();
                    let keyboard_control = cx.entity().downgrade();
                    let keyboard_ids = session_tab_ids.clone();
                    let keyboard_focus_handles = self.session_tab_focus_handles.clone();
                    let keyboard_tab_focus = tab_focus.clone();
                    let click_tab_focus = tab_focus.clone();
                    let tab = Tab::new()
                        .debug_selector(move || tab_selector.clone().into())
                        .label(if dirty {
                            format!("* {tab_title}")
                        } else {
                            tab_title.clone()
                        })
                        .aria_label(accessibility_label)
                        .aria_description(DOCUMENT_TAB_REORDER_DESCRIPTION)
                        .aria_keyshortcuts(DOCUMENT_TAB_REORDER_KEYSHORTCUTS)
                        .child(bounds_trace)
                        .children(drag_trace)
                        .children(drop_trace)
                        .track_focus(&tab_focus.tab_stop(is_active))
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
                        .on_click(move |_, window, cx| {
                            let suppressed = click_control
                                .update(cx, |workspace, _| {
                                    let active_drag = workspace
                                        .session_tab_pointer_drag
                                        .as_ref()
                                        .is_some_and(|drag| {
                                            drag.activated
                                                && drag.document_id == tab_document_id
                                        });
                                    active_drag
                                        || workspace
                                            .take_suppressed_session_tab_click(tab_document_id)
                                })
                                .unwrap_or(false);
                            if suppressed {
                                cx.stop_propagation();
                                return;
                            }
                            let _ = click_control.update(cx, |workspace, cx| {
                                workspace.activate_document(tab_document_id, cx);
                            });
                            click_tab_focus.focus(window, cx);
                        });
                    let close_id = document_session_close_id(tab_document_id);
                    let close_selector = close_id.clone();
                    let close_accessibility_id = close_id.clone();
                    let close_label = document_tab_close_accessible_label(&tab_title);
                    let close = accessible_icon_button(
                        Button::new(close_id)
                            .debug_selector(move || close_selector.clone().into())
                            .accessibility_id(close_accessibility_id)
                            .ghost()
                            .xsmall()
                            .size_6()
                            .icon(IconName::Close)
                            .tooltip(close_label.clone())
                            .disabled(saving)
                            .on_click(move |_: &ClickEvent, _, cx| {
                                cx.stop_propagation();
                            }),
                        close_label,
                    );
                    let close = if dirty {
                        let confirmation_open =
                            self.pending_close_document_id == Some(tab_document_id);
                        let open_control = cx.entity().downgrade();
                        let content_control = cx.entity().downgrade();
                        Popover::new(format!("{tab_document_id}-dirty-close-popover"))
                            .anchor(Anchor::TopRight)
                            .open(confirmation_open)
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
                                    .gap_3()
                                    .child(
                                        v_flex()
                                            .gap_1()
                                            .child(
                                                gpui::div()
                                                    .font_semibold()
                                                    .child(format!(
                                                        "Save changes to {tab_title}?"
                                                    )),
                                            )
                                            .child(
                                                gpui::div()
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
                    tab.suffix(
                        h_flex()
                            .relative()
                            .child(close)
                            .child(close_bounds_trace),
                    )
                },
            )
            .collect::<Vec<_>>();
        let open_document = Button::new(DOCUMENT_TAB_OPEN_ID)
            .debug_selector(|| DOCUMENT_TAB_OPEN_ID.into())
            .label("Open PDF")
            .on_click(cx.listener(|workspace, _, _, cx| {
                workspace.prompt_to_open_documents(cx);
            }));
        let document_actions = h_flex()
            .flex_shrink_0()
            .gap_1()
            .child(open_document)
            .child(self.template_control.clone());
        let native_session_tabs = TabBar::new("document-workspace-session-tabs-component")
            .max_width(px(190.))
            .children(rendered_session_tabs)
            .suffix(document_actions)
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
        let pointer_event_bridge = canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let focus = down_control
                        .update(cx, |workspace, cx| {
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
        let session_tab_strip = h_flex()
            .id(DOCUMENT_SESSION_TABS_ID)
            .debug_selector(|| DOCUMENT_SESSION_TABS_ID.into())
            .w_full()
            .relative()
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
                    .w_full()
                    .child(native_session_tabs),
            )
            .child(reorder_status)
            .child(pointer_event_bridge);
        let down_control = cx.entity().downgrade();
        let move_control = cx.entity().downgrade();
        let up_control = cx.entity().downgrade();
        let exit_control = cx.entity().downgrade();
        let pointer_event_bridge = canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
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
                    if event.pressed_button != Some(MouseButton::Left) {
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
                            workspace.finish_annotation_pointer(
                                event.position,
                                event.modifiers,
                                window,
                                cx,
                            )
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
                            if workspace
                                .active_annotation_pointer
                                .is_some_and(|active| active.placement_pending)
                            {
                                false
                            } else {
                                workspace.cancel_active_annotation_pointer(cx)
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
        .size(px(1.));

        let (inspector_kind, inspector_initial_width, inspector_width_range) =
            match active_inspector {
                Some(ActiveInspector {
                    kind,
                    initial_width,
                    width_range,
                }) => (Some(kind), initial_width, width_range),
                None => (
                    None,
                    window.rem_size() * 18.75,
                    window.rem_size() * 13.75..window.rem_size() * 26.25,
                ),
            };
        let inspector_visible = inspector_kind.is_some();
        let inspector_shell = match inspector_kind {
            Some(ActiveInspectorKind::Rectangle) => active_inspector_shell().child(
                self.rectangle_property_inspector
                    .as_ref()
                    .expect("the ensured Rectangle inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::Ellipse) => active_inspector_shell().child(
                self.ellipse_property_inspector
                    .as_ref()
                    .expect("the ensured Ellipse inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::StraightLine) => active_inspector_shell().child(
                self.straight_line_property_inspector
                    .as_ref()
                    .expect("the ensured straight-line inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::VertexPath) => active_inspector_shell().child(
                self.vertex_path_property_inspector
                    .as_ref()
                    .expect("the ensured vertex-path inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::Ink) => active_inspector_shell().child(
                self.ink_property_inspector
                    .as_ref()
                    .expect("the ensured Ink inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::EngineeringVisual) => active_inspector_shell().child(
                self.engineering_visual_property_inspector
                    .as_ref()
                    .expect("the ensured engineering-visual inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::TextBox) => active_inspector_shell().child(
                self.text_box_property_inspector
                    .as_ref()
                    .expect("the ensured Text Box inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::Measurement) => active_inspector_shell().child(
                self.measurement_property_inspector
                    .as_ref()
                    .expect("the ensured measurement inspector must be retained")
                    .clone(),
            ),
            Some(ActiveInspectorKind::Dimension) => active_inspector_shell().child(
                self.dimension_property_inspector
                    .as_ref()
                    .expect("the ensured Dimension inspector must be retained")
                    .clone(),
            ),
            None => active_inspector_shell(),
        };
        let page_scale_cancel_control = page_scale_control.downgrade();
        let save_failure_title = format!("Couldn’t save “{title}”");
        let text_box_commit_error = self.text_box_commit_error.clone();
        root.child(session_tab_strip)
        .child(self.viewer_toolbar.clone())
        .when(redact_pending, |root| {
            root.child(
                gpui::div()
                    .id(DOCUMENT_REDACT_PENDING_ALERT_ID)
                    .debug_selector(|| DOCUMENT_REDACT_PENDING_ALERT_ID.into())
                    .w_full()
                    .px_3()
                    .py_2()
                    .child(
                        Alert::warning(
                            "document-workspace-redact-pending-message",
                            PENDING_REDACTION_STATUS,
                        )
                        .title("Pending redaction mark"),
                ),
            )
        })
        .when_some(text_box_commit_error, |root, error| {
            root.child(
                gpui::div()
                    .id(DOCUMENT_TEXT_BOX_COMMIT_ERROR_ALERT_ID)
                    .debug_selector(|| DOCUMENT_TEXT_BOX_COMMIT_ERROR_ALERT_ID.into())
                    .w_full()
                    .px_3()
                    .py_2()
                    .child(
                        Alert::error("document-workspace-text-box-commit-error-message", error)
                            .title("Couldn’t finish editing"),
                    ),
            )
        })
        .when_some(self.last_document_open_failure().cloned(), |root, failure| {
            root.child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .p_2()
                    .child(
                        gpui::div()
                            .id(DOCUMENT_OPEN_ERROR_ALERT_ID)
                            .debug_selector(|| DOCUMENT_OPEN_ERROR_ALERT_ID.into())
                            .flex_1()
                            .child(
                                Alert::error(
                                    "document-workspace-open-feedback-message",
                                    failure.presentation(),
                                )
                                .title("Some PDFs couldn’t be opened"),
                            ),
                    )
                    .child(
                        Button::new(DOCUMENT_OPEN_ERROR_DISMISS_ID)
                            .debug_selector(|| DOCUMENT_OPEN_ERROR_DISMISS_ID.into())
                            .accessibility_id(DOCUMENT_OPEN_ERROR_DISMISS_ID)
                            .outline()
                            .label("Dismiss")
                            .on_click(cx.listener(|workspace, _, _, cx| {
                                workspace.dismiss_document_open_failure(cx);
                            })),
                    ),
            )
        })
        .when_some(save_failure, |root, failure| {
            root.child(
                h_flex()
                    .debug_selector(|| DOCUMENT_SAVE_ERROR_ALERT_ID.into())
                    .w_full()
                    .gap_2()
                    .p_2()
                    .child(
                        Alert::error(DOCUMENT_SAVE_ERROR_ALERT_ID, failure.message)
                            .title(save_failure_title)
                            .flex_1(),
                    )
                    .when(
                        failure.operation == DocumentSaveFailureOperation::InPlace,
                        |actions| {
                            actions.child(
                                Button::new(DOCUMENT_SAVE_ERROR_RETRY_ID)
                                    .debug_selector(|| DOCUMENT_SAVE_ERROR_RETRY_ID.into())
                                    .accessibility_id(DOCUMENT_SAVE_ERROR_RETRY_ID)
                                    .label("Retry")
                                    .on_click(cx.listener(move |workspace, _, _, cx| {
                                        workspace.retry_document_save_failure(document_id, cx);
                                    })),
                            )
                        },
                    )
                    .child(
                        Button::new(DOCUMENT_SAVE_ERROR_SAVE_AS_ID)
                            .debug_selector(|| DOCUMENT_SAVE_ERROR_SAVE_AS_ID.into())
                            .accessibility_id(DOCUMENT_SAVE_ERROR_SAVE_AS_ID)
                            .outline()
                            .label("Save As…")
                            .on_click(cx.listener(move |workspace, _, _, cx| {
                                workspace.prompt_to_save_as(document_id, cx);
                            })),
                    )
                    .child(
                        Button::new(DOCUMENT_SAVE_ERROR_DISMISS_ID)
                            .debug_selector(|| DOCUMENT_SAVE_ERROR_DISMISS_ID.into())
                            .accessibility_id(DOCUMENT_SAVE_ERROR_DISMISS_ID)
                            .ghost()
                            .label("Dismiss")
                            .on_click(cx.listener(move |workspace, _, _, cx| {
                                workspace.dismiss_document_save_failure(document_id, cx);
                            })),
                    ),
            )
        })
        .when_some(self.last_file_error.clone(), |root, error| {
            root.child(
                gpui::div()
                    .id(DOCUMENT_ERROR_ID)
                    .debug_selector(|| DOCUMENT_ERROR_ID.into())
                    .w_full()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(cx.theme().danger)
                    .child(error),
            )
        })
        .when_some(presentation_error, |root, error| {
            root.child(
                h_flex()
                    .id(DOCUMENT_RECOVERY_ALERT_ID)
                    .debug_selector(|| DOCUMENT_RECOVERY_ALERT_ID.into())
                    .w_full()
                    .gap_2()
                    .p_2()
                    .child(
                        Alert::info("document-worker-recovery-message", error)
                            .title("PDF rendering stopped")
                            .w(px(520.)),
                    )
                    .child(
                        Button::new(DOCUMENT_RECOVERY_RETRY_ID)
                            .debug_selector(|| DOCUMENT_RECOVERY_RETRY_ID.into())
                            .label(if recovery_pending { "Retrying…" } else { "Retry" })
                            .disabled(recovery_pending)
                            .on_click(cx.listener(move |workspace, _, _, cx| {
                                if let Err(error) =
                                    workspace.retry_document_recovery_async(document_id, cx)
                                {
                                    workspace.last_file_error = Some(error);
                                    cx.notify();
                                }
                            })),
                    ),
            )
        })
        .when_some(page_scale_pick_instruction, |root, instruction| {
            root.child(
                h_flex()
                    .id(PAGE_SCALE_PICK_ALERT_ID)
                    .debug_selector(|| PAGE_SCALE_PICK_ALERT_ID.into())
                    .w_full()
                    .gap_2()
                    .p_2()
                    .child(
                        Alert::info("page-scale-pick-instructions", instruction)
                            .title("Set Page Scale")
                            .flex_1(),
                    )
                    .child(
                        Button::new(PAGE_SCALE_PICK_CANCEL_ID)
                            .debug_selector(|| PAGE_SCALE_PICK_CANCEL_ID.into())
                            .label("Cancel")
                            .outline()
                            .on_click(move |_, _, cx| {
                                let _ = page_scale_cancel_control.update(cx, |control, cx| {
                                    control.cancel_pick(cx);
                                });
                            }),
                    ),
            )
        })
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .px_3()
                .py_2()
                .border_b_1()
                .border_color(cx.theme().border)
                .child(gpui::div().flex_shrink_0().font_semibold().child(title))
                .child(
                    gpui::div()
                        .ml_auto()
                        .flex_shrink_0()
                        .text_sm()
                        .child(format!("Page {}", current_page + 1)),
                )
                .child(
                    h_flex()
                        .id(DOCUMENT_TOOLBAR_SCROLL_ID)
                        .debug_selector(|| DOCUMENT_TOOLBAR_SCROLL_ID.into())
                        .w_full()
                        .flex_1()
                        .min_w_0()
                        .overflow_x_scroll()
                        .track_scroll(&self.toolbar_scroll)
                        .child(
                            h_flex()
                                .id(DOCUMENT_TOOLBAR_CONTENT_ID)
                                .debug_selector(|| DOCUMENT_TOOLBAR_CONTENT_ID.into())
                                .w_auto()
                                .min_w_full()
                                .flex_shrink_0()
                                .child(h_flex().flex_shrink_0().child(tool_group))
                                .child(h_flex().flex_shrink_0().child(semantic_snap_control))
                                .child(h_flex().flex_shrink_0().child(highlight_tool_control))
                                .child(h_flex().flex_shrink_0().child(history_group))
                                .child(
                                    h_flex()
                                        .flex_shrink_0()
                                        .child(rectangle_properties_button),
                                )
                                .child(
                                    h_flex()
                                        .flex_shrink_0()
                                        .child(ellipse_properties_button),
                                )
                                .child(h_flex().flex_shrink_0().child(stroke_control))
                                .when_some(straight_line_properties, |toolbar, button| {
                                    toolbar.child(h_flex().flex_shrink_0().child(button))
                                })
                                .when_some(vertex_path_properties, |toolbar, button| {
                                    toolbar.child(h_flex().flex_shrink_0().child(button))
                                })
                                .when_some(ink_properties_button, |toolbar, button| {
                                    toolbar.child(h_flex().flex_shrink_0().child(button))
                                })
                                .when_some(
                                    engineering_visual_properties_button,
                                    |toolbar, button| {
                                        toolbar.child(h_flex().flex_shrink_0().child(button))
                                    },
                                )
                                .when_some(text_box_properties_button, |toolbar, button| {
                                    toolbar.child(h_flex().flex_shrink_0().child(button))
                                })
                                .when_some(measurement_properties_button, |toolbar, button| {
                                    toolbar.child(h_flex().flex_shrink_0().child(button))
                                })
                                .when_some(dimension_properties, |toolbar, properties| {
                                    toolbar.child(h_flex().flex_shrink_0().child(properties))
                                })
                                .when_some(
                                    self.annotation_statuses.get(&document_id).cloned(),
                                    |toolbar, status| {
                                        toolbar.child(
                                            gpui::div()
                                                .id(DOCUMENT_ANNOTATION_STATUS_ID)
                                                .debug_selector(|| {
                                                    DOCUMENT_ANNOTATION_STATUS_ID.into()
                                                })
                                                .text_sm()
                                                .text_color(cx.theme().danger)
                                                .child(status),
                                        )
                                    },
                                )
                                .when_some(pending_text_box_input, |toolbar, input| {
                                    toolbar.child(
                                        gpui::div()
                                            .id(DOCUMENT_TEXT_BOX_EDITOR_ID)
                                            .debug_selector(|| DOCUMENT_TEXT_BOX_EDITOR_ID.into())
                                            .w(px(220.))
                                            .h(px(72.))
                                            .child(
                                                Textarea::new(&input)
                                                    .aria_label("Text box content")
                                                    .h_full(),
                                            ),
                                    )
                                })
                                .child(
                                    gpui::div()
                                        .id(DOCUMENT_TEXT_BOX_RETURN_FOCUS_ID)
                                        .track_focus(&self.text_box_return_focus)
                                        .w(px(1.))
                                        .h(px(1.)),
                                )
                                .child(
                                    Button::new(DOCUMENT_ANNOTATION_LOCK_ID)
                                        .debug_selector(|| DOCUMENT_ANNOTATION_LOCK_ID.into())
                                        .label(if selected_annotation_locked {
                                            "Unlock"
                                        } else {
                                            "Lock"
                                        })
                                        .disabled(selected_annotation_id.is_none() || save_busy)
                                        .on_click(cx.listener(move |workspace, _, _, cx| {
                                            let _ = workspace.set_selected_annotation_locked(
                                                document_id,
                                                !selected_annotation_locked,
                                                cx,
                                            );
                                        })),
                                )
                                .child(
                                    Button::new(DOCUMENT_ANNOTATION_DELETE_ID)
                                        .debug_selector(|| DOCUMENT_ANNOTATION_DELETE_ID.into())
                                        .danger()
                                        .label("Delete")
                                        .disabled(
                                            selected_annotation_id.is_none()
                                                || !selected_has_unlocked_annotation
                                                || save_busy,
                                        )
                                        .on_click(cx.listener(move |workspace, _, _, cx| {
                                            let _ = workspace
                                                .delete_selected_annotation(document_id, cx);
                                        })),
                                ),
                        ),
                )
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .child(
                            Button::new(DOCUMENT_ROTATE_LEFT_ID)
                                .debug_selector(|| DOCUMENT_ROTATE_LEFT_ID.into())
                                .label("Rotate Left")
                                .disabled(save_busy)
                                .on_click(cx.listener(move |workspace, _, _, cx| {
                                    workspace.rotate_page_async(
                                        document_id,
                                        PageRotationDirection::Left,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(DOCUMENT_ROTATE_RIGHT_ID)
                                .debug_selector(|| DOCUMENT_ROTATE_RIGHT_ID.into())
                                .label("Rotate Right")
                                .disabled(save_busy)
                                .on_click(cx.listener(move |workspace, _, _, cx| {
                                    workspace.rotate_page_async(
                                        document_id,
                                        PageRotationDirection::Right,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            Button::new(DOCUMENT_SAVE_ID)
                                .debug_selector(|| DOCUMENT_SAVE_ID.into())
                                .label(if save_in_progress { "Saving…" } else { "Save" })
                                .disabled(save_busy)
                                .on_click(cx.listener(move |workspace, _, _, cx| {
                                    workspace.save_active_document(cx);
                                })),
                        )
                        .child(
                            Button::new(DOCUMENT_SAVE_AS_ID)
                                .debug_selector(|| DOCUMENT_SAVE_AS_ID.into())
                                .label(save_as_command_label(save_in_progress))
                                .disabled(save_busy)
                                .on_click(cx.listener(move |workspace, _, _, cx| {
                                    workspace.prompt_to_save_as(document_id, cx);
                                })),
                        )
                        .child(
                            Button::new(DOCUMENT_CLOSE_ID)
                                .debug_selector(|| DOCUMENT_CLOSE_ID.into())
                                .ghost()
                                .label("Close")
                                .on_click(cx.listener(move |workspace, _, _, cx| {
                                    workspace.request_close_document(document_id, cx);
                                })),
                        ),
                ),
        )
        .child(
            h_resizable("document-workspace-viewer-panels")
                .with_state(&self.viewer_resizable)
                .child(
                    resizable_panel()
                        .size(px(300.))
                        .size_range(px(180.)..px(360.))
                        .flex_none()
                        .child({
                            let thumbnail_rows = Arc::new(thumbnails);
                            let thumbnail_count = thumbnail_rows.len();
                            let thumbnail_control = cx.entity().downgrade();
                            let thumbnail_images = image_assets.clone();
                            gpui::div()
                                .id(DOCUMENT_THUMBNAIL_STRIP_ID)
                                .debug_selector(|| DOCUMENT_THUMBNAIL_STRIP_ID.into())
                                .role(Role::List)
                                .aria_label("Document pages")
                                .relative()
                                .w_full()
                                .h_full()
                                .p_2()
                                .child(
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
                                                        highlights_precomposed,
                                                        scene,
                                                    ) = thumbnail_rows[row_index].clone();
                                                    let stable_id = document_thumbnail_id(
                                                        document_id,
                                                        page_index,
                                                    );
                                                    let selector = stable_id.clone();
                                                    let click_control =
                                                        thumbnail_control.clone();
                                                    let page_label =
                                                        format!("Page {}", page_index + 1);
                                                    v_flex()
                                                        .id(stable_id.clone())
                                                        .debug_selector(move || {
                                                            selector.clone().into()
                                                        })
                                                        .role(Role::ListItem)
                                                        .accessibility_id(stable_id)
                                                        .aria_label(page_label.clone())
                                                        .aria_position_in_set(row_index + 1)
                                                        .aria_size_of_set(thumbnail_count)
                                                        .aria_selected(page_index == current_page)
                                                        .when(page_index == current_page, |row| {
                                                            row.aria_active_descendant()
                                                        })
                                                        .h(px(160.))
                                                        .flex_none()
                                                        .p_1()
                                                        .gap_1()
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
                                                        .child(
                                                            gpui::div()
                                                                .relative()
                                                                .w_full()
                                                                .h(px(128.))
                                                                .when_some(
                                                                    image.clone(),
                                                                    |page, image| {
                                                                        page.child(
                                                                            img(image)
                                                                                .size_full()
                                                                                .object_fit(
                                                                                    ObjectFit::Contain,
                                                                                ),
                                                                        )
                                                                        .child(annotation_layer(
                                                                            document_id,
                                                                            page_index,
                                                                            page_size,
                                                                            pdf_page_size,
                                                                            rotation,
                                                                            coordinate_space,
                                                                            scene,
                                                                            highlights_precomposed,
                                                                            thumbnail_images.clone(),
                                                                            selection_color,
                                                                            None,
                                                                            None,
                                                                            None,
                                                                            None,
                                                                        ))
                                                                    },
                                                                )
                                                                .when(
                                                                    image.is_none(),
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
                                                                ),
                                                        )
                                                        .child(
                                                            gpui::div()
                                                                .text_xs()
                                                                .child(page_label),
                                                        )
                                                })
                                                .collect::<Vec<_>>()
                                        },
                                    )
                                    .size_full()
                                    .track_scroll(&thumbnail_scroll),
                                )
                                .vertical_scrollbar(&thumbnail_scroll)
                        }),
                )
                .child(
                    resizable_panel().child(gpui::div()
                        .id(DOCUMENT_PAGE_ID)
                        .debug_selector(|| DOCUMENT_PAGE_ID.into())
                        .flex_1()
                        .size_full()
                        .p_4()
                        .bg(cx.theme().secondary)
                        .child(
                            gpui::div()
                                .id(DOCUMENT_VIEWPORT_ID)
                                .debug_selector(|| DOCUMENT_VIEWPORT_ID.into())
                                .relative()
                                .size_full()
                                .overflow_scroll()
                                .track_scroll(&viewer_scroll)
                                .on_scroll_wheel(cx.listener(
                                    move |workspace, event: &ScrollWheelEvent, window, cx| {
                                        if workspace.handle_viewport_wheel(document_id, event, window, cx) {
                                            window.prevent_default();
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
                                    viewport.child(
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
                                                                quality_selector.clone().into()
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
                                                                    error_selector.clone().into()
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
                                                                                .into()
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
                                                                    tile_selector.clone().into()
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
                                                    gpui::div()
                                                        .id(page_id)
                                                        .debug_selector(move || {
                                                            page_selector.clone().into()
                                                        })
                                                        .absolute()
                                                        .left(px(layout.logical_rect.x))
                                                        .top(px(layout.logical_rect.y))
                                                        .w(px(layout.logical_rect.width))
                                                        .h(px(layout.logical_rect.height))
                                                        .bg(gpui::rgb(0xffffff))
                                                        .border_1()
                                                        .border_color(if page_index == current_page {
                                                            selection_color
                                                        } else {
                                                            cx.theme().border
                                                        })
                                                        .when(
                                                            page_index == current_page,
                                                            |page| {
                                                                page.when_some(
                                                                    current_image.clone(),
                                                                    |page, image| {
                                                                        page.child(
                                                                            img(image)
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
                                                            highlights_precomposed,
                                                            image_assets.clone(),
                                                            selection_color,
                                                            (page_index == current_page)
                                                                .then(|| semantic_snap_decision.clone())
                                                                .flatten(),
                                                            active_selection_marquee
                                                                .as_ref()
                                                                .filter(|(active_page, _)| {
                                                                    *active_page == page_index
                                                                })
                                                                .map(|(_, marquee)| marquee.clone()),
                                                            Some(cx.entity().downgrade()),
                                                            painted_viewer,
                                                        ))
                                                        .children(render_error_surface)
                                                },
                                            )),
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
                                                    current_highlights_precomposed,
                                                    image_assets,
                                                    selection_color,
                                                    semantic_snap_decision.clone(),
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
                                .child(viewer_status_surface)
                                .vertical_scrollbar(&viewer_scroll)
                                .horizontal_scrollbar(&viewer_scroll),
                        ),
                ))
                .child(
                    resizable_panel()
                        .visible(inspector_visible)
                        .size(inspector_initial_width)
                        .size_range(inspector_width_range)
                        .flex_none()
                        .child(inspector_shell),
                ),
        )
        .child(pointer_event_bridge)
    }
}

fn rectangular_shape_patch_matches(
    patch: &RectanglePropertyPatch,
    rect: PdfRect,
    rotation_degrees: f64,
    appearance: &RectangleAppearance,
    locked: bool,
) -> bool {
    match patch {
        RectanglePropertyPatch::Locked(value) => locked == *value,
        RectanglePropertyPatch::StrokeColor(value) => {
            appearance.stroke_color().eq_ignore_ascii_case(value)
        }
        RectanglePropertyPatch::Opacity(value) => appearance.opacity() == *value,
        RectanglePropertyPatch::StrokeWidthPt(value) => appearance.stroke_width_pt() == *value,
        RectanglePropertyPatch::StrokeStyle(value) => appearance.stroke_style() == *value,
        RectanglePropertyPatch::FillColor(value) => match (appearance.fill_color(), value) {
            (None, None) => true,
            (Some(current), Some(value)) => current.eq_ignore_ascii_case(value),
            _ => false,
        },
        RectanglePropertyPatch::FillOpacity(value) => appearance.fill_opacity() == *value,
        RectanglePropertyPatch::X(value) => rect.x == *value,
        RectanglePropertyPatch::Y(value) => rect.y == *value,
        RectanglePropertyPatch::Width(value) => rect.width == *value,
        RectanglePropertyPatch::Height(value) => rect.height == *value,
        RectanglePropertyPatch::RotationDegrees(value) => {
            rotation_degrees.rem_euclid(360.) == value.rem_euclid(360.)
        }
    }
}

fn ink_patch_matches(patch: &InkPropertyPatch, appearance: &PenAppearance, locked: bool) -> bool {
    match patch {
        InkPropertyPatch::Locked(value) => locked == *value,
        InkPropertyPatch::Appearance(value) => appearance == value,
        InkPropertyPatch::WidthPt(value) => appearance.width_pt() == *value,
        InkPropertyPatch::Opacity(value) => appearance.opacity() == *value,
    }
}

fn engineering_visual_appearance(
    current: &RectangleAppearance,
    color: impl Into<String>,
    width: f64,
    opacity: f64,
) -> Result<RectangleAppearance, String> {
    RectangleAppearance::new(
        color,
        width,
        current.fill_color().map(str::to_owned),
        opacity,
    )
    .and_then(|appearance| appearance.with_fill_opacity(current.fill_opacity()))
    .map(|appearance| appearance.with_stroke_style(current.stroke_style()))
    .map_err(|error| error.to_string())
}

fn straight_line_property_patch_matches(
    patch: &StraightLinePropertyPatch,
    line: &butter_paper_gpui_gallery::annotation_model::StraightLineAnnotation,
) -> bool {
    match patch {
        StraightLinePropertyPatch::Locked(value) => line.locked == *value,
        StraightLinePropertyPatch::Color(value) => line.appearance.stroke_color() == value,
        StraightLinePropertyPatch::WidthPt(value) => {
            line.appearance.stroke_width_pt() == *value
        }
        StraightLinePropertyPatch::Opacity(value) => line.appearance.opacity() == *value,
    }
}

fn dimension_property_patch_matches(
    patch: &DimensionPropertyPatch,
    dimension: &DimensionAnnotation,
) -> bool {
    match patch {
        DimensionPropertyPatch::Locked(value) => dimension.locked == *value,
        DimensionPropertyPatch::OffsetPt(value) => dimension.dimension_line_offset() == *value,
        DimensionPropertyPatch::Appearance(value) => &dimension.appearance == value,
    }
}

fn path_property_patch_matches(
    patch: &VertexPathPropertyPatch,
    appearance: &RectangleAppearance,
    locked: bool,
) -> bool {
    match patch {
        VertexPathPropertyPatch::Locked(value) => locked == *value,
        VertexPathPropertyPatch::StrokeColor(value) => appearance.stroke_color() == value,
        VertexPathPropertyPatch::StrokeWidthPt(value) => appearance.stroke_width_pt() == *value,
        VertexPathPropertyPatch::Opacity(value) => appearance.opacity() == *value,
        VertexPathPropertyPatch::FillColor(value) => appearance.fill_color() == value.as_deref(),
    }
}

fn engineering_visual_patch_matches(
    patch: &EngineeringVisualPropertyPatch,
    kind: EngineeringVisualPropertyKind,
    appearance: &RectangleAppearance,
    intensity: Option<f64>,
    locked: bool,
) -> bool {
    match patch {
        EngineeringVisualPropertyPatch::Locked(value) => locked == *value,
        EngineeringVisualPropertyPatch::Color(value) => appearance.stroke_color() == value,
        EngineeringVisualPropertyPatch::WidthPt(value) => appearance.stroke_width_pt() == *value,
        EngineeringVisualPropertyPatch::Opacity(value) => appearance.opacity() == *value,
        EngineeringVisualPropertyPatch::CloudIntensity(value) => {
            kind == EngineeringVisualPropertyKind::Cloud && intensity == Some(*value)
        }
    }
}

fn set_rectangular_shape_rect(
    annotations: &mut AnnotationAdapter,
    document_id: DocumentId,
    annotation_id: MarkupId,
    kind: RectangularShapePropertyKind,
    rect: PdfRect,
) -> Result<(), String> {
    match kind {
        RectangularShapePropertyKind::Rectangle => annotations
            .set_selected_rectangle_rect(document_id.value(), rect)
            .map_err(|error| error.to_string()),
        RectangularShapePropertyKind::Ellipse => annotations
            .set_ellipse_rect(document_id.value(), annotation_id, rect)
            .map_err(|error| error.to_string()),
    }
}

fn set_rectangular_shape_rotation(
    annotations: &mut AnnotationAdapter,
    document_id: DocumentId,
    annotation_id: MarkupId,
    kind: RectangularShapePropertyKind,
    rotation_degrees: f64,
) -> Result<(), String> {
    match kind {
        RectangularShapePropertyKind::Rectangle => annotations
            .set_selected_rectangle_rotation(document_id.value(), rotation_degrees)
            .map_err(|error| error.to_string()),
        RectangularShapePropertyKind::Ellipse => annotations
            .set_ellipse_rotation(
                document_id.value(),
                annotation_id,
                rotation_degrees.rem_euclid(360.),
            )
            .map_err(|error| error.to_string()),
    }
}

fn next_workspace_annotation_sequence(annotations: &[Annotation]) -> u64 {
    annotations
        .iter()
        .filter_map(|annotation| {
            let value = annotation.id().as_str();
            value
                .strip_prefix("workspace:")?
                .rsplit_once(':')?
                .1
                .parse::<u64>()
                .ok()
        })
        .max()
        .map_or(1, |sequence| sequence.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::{PendingTextEditorAuthority, validate_existing_text_editor_authority};

    #[test]
    fn existing_text_editor_authority_rejects_stale_resource_without_mutation() {
        let authority = PendingTextEditorAuthority {
            resource_generation: 7,
            baseline_revision: 11,
            baseline_text: "original".into(),
        };
        let before = authority.clone();

        let error =
            validate_existing_text_editor_authority(&authority, 8, 11, Some(("original", false)))
                .expect_err("a replacement resource must invalidate the open editor");

        assert!(error.contains("stale document resource"));
        assert_eq!(authority.resource_generation, before.resource_generation);
        assert_eq!(authority.baseline_revision, before.baseline_revision);
        assert_eq!(authority.baseline_text, before.baseline_text);
    }
}
