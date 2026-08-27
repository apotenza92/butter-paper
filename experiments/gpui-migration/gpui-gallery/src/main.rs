use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsString,
    fs,
    ops::Range,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::Result;
use gpui::{
    AnyElement, AnyWindowHandle, App, AssetSource, BorderStyle, Bounds, ColorExt, Context, Corners,
    ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable, IntoElement,
    KeyBinding, KeyDownEvent, Menu, MenuItem, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ObjectFit, PathBuilder, PathPromptOptions, Pixels, Point, QuitMode, Render,
    RenderImage, ScrollHandle, ScrollStrategy, ScrollWheelEvent, SharedString, StyledImage,
    TextRun, TitlebarOptions, UTF16Selection, UniformListScrollHandle, Window, WindowBounds,
    WindowOptions, actions, canvas, div, fill, img, outline, point, prelude::*, px, rgb, size, svg,
    uniform_list,
};
use image::{Frame, ImageBuffer, Rgba};
use smallvec::smallvec;

mod butter_ui;
mod nova_theme;
mod pdf_document;
mod perf;
use butter_paper_gpui_gallery::annotation_adapter::{
    AnnotationAdapter, AnnotationTool, FROZEN_TEXT_CREATE, NATURAL_IMAGE_MAX_PAGE_FRACTION,
    ROTATION_HANDLE_OFFSET_CSS_PX, RectangleSnapSettings,
};
use butter_paper_gpui_gallery::annotation_model::{
    AnnotationError, AnnotationKind, AnnotationScene, BlendMode, DecodedRgbaAsset, InkTool,
    MarkupId, PageTransform, PdfPoint, PointerCancelReason, RectangleAppearance,
    RectangleResizeHandle, StrokeStyle, rectangle_rotation_handle_world_point,
    rectangle_world_corners,
};
use butter_paper_gpui_gallery::annotation_paint_path::{InkPaintPathSegment, build_ink_paint_path};
use butter_paper_gpui_gallery::comparison_scenario::{
    ComparisonScenarioKind, ComparisonScenarioPlan, MilestoneGate,
    V4_VISIBLE_RASTER_READINESS_MILESTONE, compare_highlight_geometry,
};
use butter_paper_gpui_gallery::dynamic_fidelity_v5_scenario::{
    COMMAND_ID as DYNAMIC_FIDELITY_COMMAND_ID,
    MAX_VISIBLE_PAGE_WINDOW as DYNAMIC_FIDELITY_MAX_VISIBLE_PAGES, queue_state_for_paint,
    visible_page_raster_states,
};
use butter_paper_gpui_gallery::editor_comparison_scenario::{
    EditorComparisonScenario, RecordingEditorObserver,
};
use butter_paper_gpui_gallery::engineering_v4_scenario::{
    AppResourceObservation, CacheRecoveryObservation, CacheRecoveryPlan, FitMode,
    FitModeObservation, FitModesPlan, assess_cache_recovery, assess_fit_mode,
    embedded_cache_recovery_plan, embedded_fit_modes_plan,
};
use butter_paper_gpui_gallery::image_asset_ingestion::ingest_image_asset_from_path;
use butter_paper_gpui_gallery::multi_document_v5_scenario::{
    CLOSE_COMMAND_ID as MULTI_CLOSE_COMMAND_ID, CLOSE_MILESTONES as MULTI_CLOSE_MILESTONES,
    CLOSE_SEQUENCE as MULTI_CLOSE_SEQUENCE, ClosedDocumentObservation, DENSE_FIXTURE_ID,
    DENSE_RECTANGLE_ID, DocumentResourceObservation, EDIT_COMMAND_ID as MULTI_EDIT_COMMAND_ID,
    EDIT_MILESTONES as MULTI_EDIT_MILESTONES, FIXTURE_IDS as MULTI_FIXTURE_IDS,
    OPEN_COMMAND_ID as MULTI_OPEN_COMMAND_ID, OPEN_MILESTONES as MULTI_OPEN_MILESTONES,
    SWITCH_COMMAND_ID as MULTI_SWITCH_COMMAND_ID, SWITCH_MILESTONES as MULTI_SWITCH_MILESTONES,
    SWITCH_SEQUENCE as MULTI_SWITCH_SEQUENCE, validate_closed_observations,
    validate_open_observations, validate_switch_observations,
};
use butter_paper_gpui_gallery::native_editing_v5::{
    NativeEditingV5Plan, PropertyEditCommit, SnapGestureCommit, SnapResolution,
};
use butter_paper_gpui_gallery::persistence_comparison_scenario::PersistenceComparisonScenario;
use butter_paper_gpui_gallery::presentation_evidence::{
    AnnotationOverlayPaintObservation, GpuiFinalFrameObservation, GpuiSubmissionObservation,
    IMAGE_CREATE_ID, ImageDecodeObservation, LENGTH_CREATE_ID, TEXT_CREATE_ID,
    TextShapeObservation, build_dense_rectangle_live_report, build_editor_final_live_report,
    build_representative_live_report, build_representative_semantic_report,
    prepare_representative_create_scene, qualify_representative_create,
};
use butter_paper_gpui_gallery::rectangle_interaction_scenario::{
    NativeRectangleTransformObservation, NativeRectangleTransformPlan, RectangleInteractionScenario,
};
use butter_paper_gpui_gallery::viewer::{
    CachePolicy, PageLayout, Rect as ViewerRect, RenderInput, RenderPlanner, TileCache,
    TileRequest, ViewportGeometry,
};
use butter_paper_gpui_gallery::viewer_presentation_evidence::{
    DocumentOpenObservation, NativeShellObservation, NativeShellPresentationProbe,
    NativeShellProbeProgress, VIEWER_LAUNCH_COMMAND_ID, VIEWER_OPEN_COMMAND_ID,
    build_viewer_launch_open_evidence,
};
use butter_ui::{
    ButterTheme, Button, ButtonGroup, ButtonGroupPosition, ButtonSize, ButtonVariant, PopupMenu,
    PopupMenuItem, Separator, SeparatorOrientation, SplitButton,
};
use nova_theme::*;
use pdf_document::{PdfDocument, RenderedTile, RenderedViewport, create_blank_pdf};
use serde_json::{Map, Value, json};

actions!(
    butter_paper,
    [
        OpenPdf,
        NextPage,
        PreviousPage,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        FitWidth,
        FitPage,
    ]
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ZoomPreset {
    Manual,
    FitWidth,
    FitPage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollMode {
    Continuous,
    SinglePage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollWheelMode {
    Scroll,
    Zoom,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenPopup {
    TemplatePicker,
    ContinuousWheel,
    SinglePageWheel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TemplateChoice {
    LetterPortrait,
    LetterLandscape,
    A4Portrait,
}

impl TemplateChoice {
    fn label(self) -> &'static str {
        match self {
            Self::LetterPortrait => "Blank US Letter",
            Self::LetterLandscape => "Blank US Letter landscape",
            Self::A4Portrait => "Blank A4",
        }
    }

    fn page_size(self) -> (f32, f32) {
        match self {
            Self::LetterPortrait => (612.0, 792.0),
            Self::LetterLandscape => (792.0, 612.0),
            Self::A4Portrait => (595.0, 842.0),
        }
    }
}

const MIN_ZOOM_PERCENT: f32 = 6.25;
const MAX_ZOOM_PERCENT: f32 = 6400.0;
const PAGE_LAYOUT_GAP: f32 = 24.0;
const FIT_ZOOM_STEP_PERCENT: f32 = 2.0;
const VIEWPORT_TOP_BORDER_WIDTH: f32 = 1.0;
const VIEWPORT_SCROLLBAR_WIDTH: f32 = 14.0;
const VIEWPORT_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 32.0;
const MAX_CONCURRENT_THUMBNAIL_JOBS: usize = 2;
const MAX_CONCURRENT_PAGE_SURFACE_JOBS: usize = 2;
const MAX_CONCURRENT_TILE_JOBS: usize = 4;
const TILED_RENDER_THRESHOLD_PX: usize = 4_096;

fn uses_tiled_rendering(viewport_pixel_width: usize) -> bool {
    viewport_pixel_width >= TILED_RENDER_THRESHOLD_PX
}

fn tile_job_can_start(
    request: TileRequest,
    pending: &HashSet<TileRequest>,
    active: &HashSet<TileRequest>,
    planner: &RenderPlanner,
) -> bool {
    pending.contains(&request) && planner.accepts(request.generation) && !active.contains(&request)
}
const SINGLE_PAGE_WHEEL_DELTA_PER_PAGE: f32 = 80.0;
const WHEEL_ZOOM_MAX_DELTA_PER_FRAME: f32 = 120.0;
const BLUEBEAM_TRACKPAD_ZOOM_RATE: f32 = 0.00165;
const ANNOTATION_MOUSE_POINTER_ID: u64 = 1;
const ANNOTATION_HIT_TOLERANCE_PX: f64 = 8.0;
fn perf_page_sequence(page_count: usize) -> Vec<usize> {
    if page_count == 0 {
        return Vec::new();
    }
    let page_count_f32 = page_count as f32;
    let candidates = [
        page_count,
        (page_count_f32 * 0.08).ceil() as usize,
        (page_count_f32 * 0.72).ceil() as usize,
        (page_count_f32 * 0.25).ceil() as usize,
        (page_count_f32 * 0.90).ceil() as usize,
        (page_count_f32 * 0.50).ceil() as usize,
        11,
        (page_count_f32 * 0.958).ceil() as usize,
        (page_count_f32 * 0.33).ceil() as usize,
        1,
    ];
    let mut sequence = Vec::with_capacity(candidates.len());
    for page in candidates {
        let page = page.clamp(1, page_count);
        if !sequence.contains(&page) {
            sequence.push(page);
        }
    }
    sequence
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerfScenarioKind {
    EmptyShell,
    OpenPdf,
    ViewerLayout,
    PageNavigation,
    Zoom,
    HighZoomPan,
    CachePressure,
    CloseReopen,
    AnnotationCreate,
    AnnotationTransform,
    AnnotationPropertiesHistory,
    EditorCreate,
    EditorWorkload,
    PersistenceWorkload,
    ContinuousScroll,
    DynamicFidelity,
    FitModes,
    CachePressureRecovery,
    MultiDocumentSession,
    NativePropertyEditUndo,
    NativeSnapTransform,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerfInputLane {
    SemanticDiagnostic,
    NativeX11Xtest,
}

impl PerfInputLane {
    fn from_environment() -> Self {
        match std::env::var("BP_GPUI_INPUT_LANE").as_deref() {
            Ok("native-x11-xtest") => Self::NativeX11Xtest,
            _ => Self::SemanticDiagnostic,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::SemanticDiagnostic => "semantic-diagnostic",
            Self::NativeX11Xtest => "native-x11-xtest",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeAnnotationStage {
    Rectangle,
    Highlight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeTransformStage {
    PrerequisiteCreate,
    Move,
    EastResize,
}

#[derive(Clone, Copy, Debug, Default)]
struct NativeTransformProgress {
    create_history_delta: usize,
    move_history_delta: usize,
    resize_history_delta: usize,
    hit_test_selected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeEditorStage {
    Text,
    Scale,
    Length,
    Image,
}

#[derive(Clone, Debug)]
struct NativeTextEntryProbe {
    content_before: String,
    history_before: usize,
    scene_revision_before: u64,
    submitted_text: String,
}

#[derive(Clone, Debug)]
struct AnnotationImageAtlasPaintObservation {
    document_id: u64,
    page_index: u32,
    scene_revision: u64,
    render_image_id: usize,
    atlas_entry_observed: bool,
    decoded_bgra_bytes: usize,
}

impl PerfScenarioKind {
    fn from_environment() -> Option<Self> {
        match perf::scenario()? {
            "empty-shell" => Some(Self::EmptyShell),
            "open-pdf" | "open-first-page" => Some(Self::OpenPdf),
            "viewer-layout" => Some(Self::ViewerLayout),
            "page-navigation" => Some(Self::PageNavigation),
            "zoom" => Some(Self::Zoom),
            "high-zoom-pan" => Some(Self::HighZoomPan),
            "cache-pressure" => Some(Self::CachePressure),
            "close-reopen" => Some(Self::CloseReopen),
            "annotation-create" => Some(Self::AnnotationCreate),
            "annotation-transform" => Some(Self::AnnotationTransform),
            "annotation-properties-history" => Some(Self::AnnotationPropertiesHistory),
            "editor-create" => Some(Self::EditorCreate),
            "editor-workload" => Some(Self::EditorWorkload),
            "persistence-workload" => Some(Self::PersistenceWorkload),
            "continuous-scroll" => Some(Self::ContinuousScroll),
            "viewer-dynamic-fidelity" => Some(Self::DynamicFidelity),
            "fit-modes" => Some(Self::FitModes),
            "cache-pressure-recovery" => Some(Self::CachePressureRecovery),
            "multi-document-session" => Some(Self::MultiDocumentSession),
            "native-property-edit-undo" => Some(Self::NativePropertyEditUndo),
            "native-snap-transform-120hz" => Some(Self::NativeSnapTransform),
            other => {
                perf::emit(
                    "scenario-error",
                    perf::fields([("message", json!(format!("Unsupported scenario: {other}")))]),
                );
                None
            }
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::EmptyShell => "empty-shell",
            Self::OpenPdf => "open-pdf",
            Self::ViewerLayout => "viewer-layout",
            Self::PageNavigation => "page-navigation",
            Self::Zoom => "zoom",
            Self::HighZoomPan => "high-zoom-pan",
            Self::CachePressure => "cache-pressure",
            Self::CloseReopen => "close-reopen",
            Self::AnnotationCreate => "annotation-create",
            Self::AnnotationTransform => "annotation-transform",
            Self::AnnotationPropertiesHistory => "annotation-properties-history",
            Self::EditorCreate => "editor-create",
            Self::EditorWorkload => "editor-workload",
            Self::PersistenceWorkload => "persistence-workload",
            Self::ContinuousScroll => "continuous-scroll",
            Self::DynamicFidelity => "viewer-dynamic-fidelity",
            Self::FitModes => "fit-modes",
            Self::CachePressureRecovery => "cache-pressure-recovery",
            Self::MultiDocumentSession => "multi-document-session",
            Self::NativePropertyEditUndo => "native-property-edit-undo",
            Self::NativeSnapTransform => "native-snap-transform-120hz",
        }
    }

    const fn benefit_metrics_eligible(self) -> bool {
        !matches!(self, Self::EmptyShell | Self::NativePropertyEditUndo)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ComparisonViewStateObservation {
    component: &'static str,
    checkpoint: &'static str,
    window_bounds_window_logical: ViewerRect,
    viewport_bounds_window_logical: ViewerRect,
    display_scale_factor: f32,
    layout_mode: &'static str,
    zoom_mode: &'static str,
    zoom_percent: f32,
    left_sidebar_visible: bool,
    left_sidebar_width_logical: f32,
    right_sidebar_visible: bool,
    right_sidebar_width_logical: f32,
    active_fixture_id: Option<String>,
    active_document_index: Option<usize>,
    open_document_count: usize,
}

impl ComparisonViewStateObservation {
    #[allow(clippy::too_many_arguments)]
    fn from_live_state(
        component: &'static str,
        checkpoint: &'static str,
        window_width: f32,
        window_height: f32,
        display_scale_factor: f32,
        scroll_mode: ScrollMode,
        zoom_preset: ZoomPreset,
        zoom_percent: f32,
        left_sidebar_visible: bool,
        active_fixture_id: Option<String>,
        active_document_index: Option<usize>,
        open_document_count: usize,
    ) -> Self {
        let left_sidebar_width_logical = if left_sidebar_visible {
            SIDEBAR_WIDTH
        } else {
            0.0
        };
        let viewport = viewport_content_rect(
            window_width,
            window_height,
            left_sidebar_visible,
            scroll_mode,
            open_document_count,
        );
        Self {
            component,
            checkpoint,
            window_bounds_window_logical: ViewerRect::new(0.0, 0.0, window_width, window_height),
            viewport_bounds_window_logical: viewport,
            display_scale_factor,
            layout_mode: match scroll_mode {
                ScrollMode::SinglePage => "single-page",
                ScrollMode::Continuous => "continuous",
            },
            zoom_mode: match zoom_preset {
                ZoomPreset::FitPage => "fit-page",
                ZoomPreset::FitWidth => "fit-width",
                ZoomPreset::Manual => "manual",
            },
            zoom_percent,
            left_sidebar_visible,
            left_sidebar_width_logical,
            // GPUI currently has an 88 px annotation rail, not the separate
            // 300 px document-properties sidebar used by Electron. Report the
            // real sidebar state instead of relabeling the rail for parity.
            right_sidebar_visible: false,
            right_sidebar_width_logical: 0.0,
            active_fixture_id,
            active_document_index,
            open_document_count,
        }
    }

    fn into_fields(self) -> Map<String, Value> {
        perf::fields([
            ("component", json!(self.component)),
            ("checkpoint", json!(self.checkpoint)),
            ("observation_source", json!("live-application-render-state")),
            ("live", json!(true)),
            (
                "window_bounds_window_logical",
                json!({
                    "x": self.window_bounds_window_logical.x,
                    "y": self.window_bounds_window_logical.y,
                    "width": self.window_bounds_window_logical.width,
                    "height": self.window_bounds_window_logical.height,
                }),
            ),
            (
                "viewport_bounds_window_logical",
                json!({
                    "x": self.viewport_bounds_window_logical.x,
                    "y": self.viewport_bounds_window_logical.y,
                    "width": self.viewport_bounds_window_logical.width,
                    "height": self.viewport_bounds_window_logical.height,
                }),
            ),
            ("display_scale_factor", json!(self.display_scale_factor)),
            ("layout_mode", json!(self.layout_mode)),
            ("zoom_mode", json!(self.zoom_mode)),
            ("zoom_percent", json!(self.zoom_percent)),
            ("left_sidebar_visible", json!(self.left_sidebar_visible)),
            (
                "left_sidebar_width_logical",
                json!(self.left_sidebar_width_logical),
            ),
            ("right_sidebar_visible", json!(self.right_sidebar_visible)),
            (
                "right_sidebar_width_logical",
                json!(self.right_sidebar_width_logical),
            ),
            ("active_fixture_id", json!(self.active_fixture_id)),
            ("active_document_index", json!(self.active_document_index)),
            ("open_document_count", json!(self.open_document_count)),
        ])
    }
}

#[derive(Clone, Copy, Debug)]
enum ComparisonPhase {
    Idle,
    ViewerLayoutSingle,
    ViewerLayoutContinuous,
    AwaitingViewerSettle {
        operation: ViewerOperation,
        ready_at_ms: f64,
    },
    Panning {
        started_ms: f64,
        last_sample: usize,
        max_tiles: usize,
    },
    CachePressure {
        operation_index: usize,
        tile_cache_insert_bytes: usize,
        atlas_upload_checkpoint_bytes: Option<usize>,
    },
    FitModesStart,
    EngineeringCachePressure {
        operation_index: usize,
        tile_cache_insert_bytes: usize,
        atlas_upload_checkpoint_bytes: Option<usize>,
    },
    CloseReopen {
        stage: CloseReopenStage,
    },
    AwaitingNativeInputSurface {
        stage: NativeAnnotationStage,
        history_before: usize,
    },
    NativeAnnotationInput {
        stage: NativeAnnotationStage,
        coordinate_samples: usize,
        history_before: usize,
    },
    AwaitingNativeTransformSurface {
        stage: NativeTransformStage,
        history_before: usize,
        progress: NativeTransformProgress,
    },
    NativeTransformInput {
        stage: NativeTransformStage,
        coordinate_samples: usize,
        history_before: usize,
        progress: NativeTransformProgress,
        pixels_per_point: f64,
    },
    AwaitingNativeTransformPaint {
        scene_revision: u64,
        progress: NativeTransformProgress,
        observed_final_rect: [f64; 4],
        pixels_per_point: f64,
    },
    AwaitingNativeV5SnapSurface,
    NativeV5SnapInput {
        pixels_per_point: f64,
    },
    AwaitingNativeEditorInput {
        stage: NativeEditorStage,
        history_before: usize,
    },
    NativeEditorInput {
        stage: NativeEditorStage,
        coordinate_samples: usize,
        history_before: usize,
    },
    AwaitingAnnotationPaint,
    AwaitingEditorCreatePaint,
    AwaitingEditorWorkloadPaint {
        persistence: bool,
    },
    AwaitingNativeScrollSurface,
    AwaitingNativeWheelCalibration {
        start_offset_y: f32,
        viewport_height: f32,
    },
    NativeScrollInput {
        forward_events: usize,
        reverse_events: usize,
        first_direction: Option<i8>,
        wheel_unit_delta: Option<f32>,
        last_forward_ms: Option<f64>,
        first_reverse_ms: Option<f64>,
        start_offset_y: f32,
        peak_distance: f32,
        viewport_height: f32,
        raster_observations: usize,
        missing_raster_observations: usize,
        max_visible_pages: usize,
    },
    Scrolling {
        started_ms: f64,
        last_sample: usize,
        raster_observations: usize,
        missing_raster_observations: usize,
        max_visible_pages: usize,
    },
    AwaitingScrollSettle {
        input_samples: usize,
        expected_samples: usize,
        native_peak_viewport_heights: Option<f32>,
        native_settle_at_ms: Option<f64>,
        raster_observations: usize,
        missing_raster_observations: usize,
        max_visible_pages: usize,
    },
    AwaitingDynamicRunnerResult,
}

const DYNAMIC_RUNNER_RESULT_FILE: &str = "dynamic-fidelity-runner-result.json";

fn validate_dynamic_runner_result(contents: &str) -> Result<bool, String> {
    let receipt: Value = serde_json::from_str(contents)
        .map_err(|error| format!("dynamic runner result is not valid JSON: {error}"))?;
    if receipt.get("schema_version").and_then(Value::as_u64) != Some(1)
        || receipt.get("command_id").and_then(Value::as_str) != Some(DYNAMIC_FIDELITY_COMMAND_ID)
        || receipt.get("crop_source").and_then(Value::as_str)
            != Some("XGetImage-presented-client-drawable")
    {
        return Err(
            "dynamic runner result does not match the frozen presented-drawable protocol".into(),
        );
    }
    match receipt.get("status").and_then(Value::as_str) {
        Some("passed") if receipt.get("error").is_none_or(Value::is_null) => Ok(true),
        Some("failed") if receipt.get("error").and_then(Value::as_str).is_some() => Ok(false),
        _ => Err("dynamic runner result status or error field is invalid".into()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ViewerOperation {
    Navigation,
    Zoom,
    HighZoomPanPrime,
    Reopen,
    FitPage { expected_zoom_percent: f32 },
    FitWidth { expected_zoom_percent: f32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloseReopenStage {
    Start,
    AwaitingReopen,
}

#[derive(Clone, Copy)]
struct PerfOperation {
    kind: &'static str,
    value: f64,
    started_ms: f64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum MultiDocumentStage {
    #[default]
    Opening,
    Switching,
    Editing,
    Closing,
    Complete,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum NativeEditingV5Stage {
    #[default]
    Setup,
    PropertyTrigger,
    PropertyChoice,
    PropertyAwaitingPaint,
    PropertyUndo,
    SnapAwaitingSurface,
    SnapInput,
    SnapAwaitingPaint,
    SnapUndo,
    SnapRedo,
    Complete,
}

struct NativeEditingV5PerfState {
    plan: NativeEditingV5Plan,
    stage: NativeEditingV5Stage,
    history_before: (usize, usize),
    property_commit: Option<PropertyEditCommit>,
    snap_commit: Option<SnapGestureCommit>,
    snap_sample_timestamps_ms: Vec<f64>,
    snap_first_sample_point: Option<PdfPoint>,
    snap_last_sample_point: Option<PdfPoint>,
    snap_pointer_up_t_ms: Option<f64>,
    snap_guide_active: bool,
    snap_guide_painted: bool,
    snap_target_acquired_count: usize,
    snap_resolution: Option<SnapResolution>,
    native_ready_emitted: bool,
    #[cfg(feature = "benchmark-evidence")]
    input_latency_samples_before: Option<u64>,
}

impl NativeEditingV5PerfState {
    fn new() -> Self {
        Self {
            plan: NativeEditingV5Plan::embedded()
                .expect("the checked-in v5 native editing workload must remain valid"),
            stage: NativeEditingV5Stage::Setup,
            history_before: (0, 0),
            property_commit: None,
            snap_commit: None,
            snap_sample_timestamps_ms: Vec::new(),
            snap_first_sample_point: None,
            snap_last_sample_point: None,
            snap_pointer_up_t_ms: None,
            snap_guide_active: false,
            snap_guide_painted: false,
            snap_target_acquired_count: 0,
            snap_resolution: None,
            native_ready_emitted: false,
            #[cfg(feature = "benchmark-evidence")]
            input_latency_samples_before: None,
        }
    }
}

#[derive(Default)]
struct MultiDocumentPerfState {
    stage: MultiDocumentStage,
    open_requested: usize,
    open_observations: Vec<DocumentResourceObservation>,
    switch_index: usize,
    switch_observations: Vec<String>,
    switch_rasters: Vec<bool>,
    native_ready_emitted: bool,
    native_input_samples_before: Option<u64>,
    native_action_started_ms: Option<f64>,
    edit_setup_complete: bool,
    edit_committed: bool,
    close_observations: Vec<ClosedDocumentObservation>,
    stage_started_ms: f64,
}

struct PerfScenario {
    kind: PerfScenarioKind,
    input_lane: PerfInputLane,
    step_index: usize,
    render_enter_emitted: bool,
    first_frame_emitted: bool,
    frame_callback_scheduled: bool,
    initial_viewport_visible: bool,
    open_started_ms: Option<f64>,
    active_operation: Option<PerfOperation>,
    pending_visible_operation: Option<PerfOperation>,
    pending_initial_visible: bool,
    initial_open_requested: Option<RequestToken>,
    initial_open_completed: Option<RequestToken>,
    initial_preview_current: Option<RequestToken>,
    native_shell_probe_in_flight: bool,
    native_shell_observation: Option<NativeShellObservation>,
    native_open_settle_started_ms: Option<f64>,
    last_frame_ms: Option<f64>,
    comparison_plan: Option<ComparisonScenarioPlan>,
    comparison_gate: Option<MilestoneGate>,
    comparison_phase: ComparisonPhase,
    engineering_fit_plan: Option<FitModesPlan>,
    engineering_cache_plan: Option<CacheRecoveryPlan>,
    engineering_fit_observations: Vec<FitModeObservation>,
    engineering_fit_shell_size: Option<[f32; 4]>,
    multi_document: Option<MultiDocumentPerfState>,
    native_editing_v5: Option<NativeEditingV5PerfState>,
    #[cfg(feature = "benchmark-evidence")]
    comparison_input_latency_samples_before: Option<u64>,
    #[cfg(feature = "benchmark-evidence")]
    viewer_input_latency_samples_before: Option<u64>,
    comparison_completion_pending: bool,
    comparison_view_state_start_emitted: bool,
    comparison_view_state_end_emitted: bool,
}

impl PerfScenario {
    fn new(kind: PerfScenarioKind) -> Self {
        let input_lane = PerfInputLane::from_environment();
        let comparison_kind = match kind {
            PerfScenarioKind::ViewerLayout => Some(ComparisonScenarioKind::ViewerLayout),
            PerfScenarioKind::PageNavigation => Some(ComparisonScenarioKind::PageNavigation),
            PerfScenarioKind::Zoom => Some(ComparisonScenarioKind::Zoom),
            PerfScenarioKind::HighZoomPan => Some(ComparisonScenarioKind::HighZoomPan),
            PerfScenarioKind::CachePressure => Some(ComparisonScenarioKind::CachePressure),
            PerfScenarioKind::CloseReopen => Some(ComparisonScenarioKind::CloseReopen),
            PerfScenarioKind::AnnotationCreate => Some(ComparisonScenarioKind::AnnotationCreate),
            PerfScenarioKind::AnnotationTransform => {
                Some(ComparisonScenarioKind::AnnotationTransform)
            }
            PerfScenarioKind::AnnotationPropertiesHistory => {
                Some(ComparisonScenarioKind::AnnotationPropertiesHistory)
            }
            PerfScenarioKind::EditorCreate => Some(ComparisonScenarioKind::EditorCreate),
            PerfScenarioKind::ContinuousScroll | PerfScenarioKind::DynamicFidelity => {
                Some(ComparisonScenarioKind::ContinuousScroll)
            }
            _ => None,
        };
        let mut comparison_plan = comparison_kind.map(|kind| {
            ComparisonScenarioPlan::embedded(kind)
                .expect("the checked-in comparison workload must remain valid")
        });
        if matches!(
            kind,
            PerfScenarioKind::ContinuousScroll | PerfScenarioKind::DynamicFidelity
        ) && std::env::var("BP_GPUI_V4_MANIFEST_ID").as_deref() == Ok("bp-perf-v4-decision-1")
        {
            comparison_plan
                .as_mut()
                .expect("continuous scroll has a comparison plan")
                .use_v4_continuous_raster_readiness()
                .expect("the frozen v3 scroll milestone must map to v4 readiness");
        }
        let engineering_fit_plan = (kind == PerfScenarioKind::FitModes)
            .then(|| embedded_fit_modes_plan().expect("the v4 Fit modes plan must remain valid"));
        let engineering_cache_plan = (kind == PerfScenarioKind::CachePressureRecovery).then(|| {
            embedded_cache_recovery_plan().expect("the v4 cache recovery plan must remain valid")
        });
        let comparison_gate = comparison_plan
            .as_ref()
            .map(MilestoneGate::new)
            .or_else(|| {
                engineering_fit_plan.as_ref().map(|plan| {
                    MilestoneGate::from_command(&plan.command_id, &plan.expected_milestones)
                })
            })
            .or_else(|| {
                engineering_cache_plan.as_ref().map(|plan| {
                    MilestoneGate::from_command(&plan.command_id, &plan.expected_milestones)
                })
            })
            .filter(|_| kind != PerfScenarioKind::DynamicFidelity);
        let multi_document =
            (kind == PerfScenarioKind::MultiDocumentSession).then(MultiDocumentPerfState::default);
        let native_editing_v5 = matches!(
            kind,
            PerfScenarioKind::NativePropertyEditUndo | PerfScenarioKind::NativeSnapTransform
        )
        .then(NativeEditingV5PerfState::new);
        if comparison_plan.is_some()
            || matches!(
                kind,
                PerfScenarioKind::EditorCreate
                    | PerfScenarioKind::EditorWorkload
                    | PerfScenarioKind::PersistenceWorkload
                    | PerfScenarioKind::FitModes
                    | PerfScenarioKind::CachePressureRecovery
                    | PerfScenarioKind::MultiDocumentSession
                    | PerfScenarioKind::NativePropertyEditUndo
                    | PerfScenarioKind::NativeSnapTransform
            )
        {
            perf::emit(
                "scenario-lane",
                perf::fields([("input_lane", json!(input_lane.as_str()))]),
            );
        }
        Self {
            kind,
            input_lane,
            step_index: 0,
            render_enter_emitted: false,
            first_frame_emitted: false,
            frame_callback_scheduled: false,
            initial_viewport_visible: false,
            open_started_ms: None,
            active_operation: None,
            pending_visible_operation: None,
            pending_initial_visible: false,
            initial_open_requested: None,
            initial_open_completed: None,
            initial_preview_current: None,
            native_shell_probe_in_flight: false,
            native_shell_observation: None,
            native_open_settle_started_ms: None,
            last_frame_ms: None,
            comparison_plan,
            comparison_gate,
            comparison_phase: ComparisonPhase::Idle,
            engineering_fit_plan,
            engineering_cache_plan,
            engineering_fit_observations: Vec::new(),
            engineering_fit_shell_size: None,
            multi_document,
            native_editing_v5,
            #[cfg(feature = "benchmark-evidence")]
            comparison_input_latency_samples_before: None,
            #[cfg(feature = "benchmark-evidence")]
            viewer_input_latency_samples_before: None,
            comparison_completion_pending: false,
            comparison_view_state_start_emitted: false,
            comparison_view_state_end_emitted: false,
        }
    }
}

fn finish_viewer_settle(scenario: &mut PerfScenario) {
    // A completed operation can schedule the next raster immediately. Clear
    // the prior settle phase first so its validation cannot run again before
    // the next raster is promoted to `pending_visible_operation`.
    scenario.comparison_phase = ComparisonPhase::Idle;
}

fn native_pending_annotation_id(scenario: &PerfScenario) -> Option<String> {
    match scenario.comparison_phase {
        ComparisonPhase::NativeAnnotationInput { stage, .. } => {
            let plan = scenario.comparison_plan.as_ref()?.annotation_create()?;
            Some(match stage {
                NativeAnnotationStage::Rectangle => plan.rectangle.annotation_id.clone(),
                NativeAnnotationStage::Highlight => plan.highlight.annotation_id.clone(),
            })
        }
        ComparisonPhase::NativeTransformInput {
            stage: NativeTransformStage::PrerequisiteCreate,
            ..
        } => Gallery::native_transform_plan()
            .ok()
            .map(|plan| plan.annotation_id),
        ComparisonPhase::NativeEditorInput { stage, .. } => {
            Gallery::native_editor_annotation_id(stage).map(str::to_owned)
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RequestToken {
    document_id: u64,
    generation: u64,
}

#[derive(Clone, Copy)]
struct AnnotationPointerLocation {
    document_id: u64,
    page: usize,
    bounds: Bounds<Pixels>,
    transform: PageTransform,
}

#[derive(Default)]
struct AnnotationPointerButtonState {
    pressed: bool,
}

impl AnnotationPointerButtonState {
    fn begin_press(&mut self) -> bool {
        if self.pressed {
            return false;
        }
        self.pressed = true;
        true
    }

    fn end_press(&mut self) -> bool {
        if !self.pressed {
            return false;
        }
        self.pressed = false;
        true
    }

    fn cancel(&mut self) {
        self.pressed = false;
    }
}

struct ThumbnailCardArgs {
    page: usize,
    selected: bool,
    image: Option<Arc<RenderImage>>,
    loading: bool,
    failed: bool,
    annotation_scene: AnnotationScene,
    page_width: f32,
    page_height: f32,
    annotation_image: Arc<RenderImage>,
    entity: Entity<Gallery>,
}

fn is_current_request(current: Option<RequestToken>, completed: RequestToken) -> bool {
    current == Some(completed)
}

fn should_refresh_equal_zoom(scenario: Option<&PerfScenario>) -> bool {
    scenario.is_some_and(|scenario| {
        scenario
            .active_operation
            .is_some_and(|operation| matches!(operation.kind, "zoom" | "fit-page" | "fit-width"))
    })
}

#[derive(Clone, Copy)]
struct ViewportRequest {
    token: RequestToken,
    page: usize,
    zoom_percent: f32,
    scale_factor: f32,
    scroll_y: Option<f32>,
    started_ms: f64,
}

#[derive(Clone, Copy)]
struct ThumbnailRequest {
    token: RequestToken,
    page: usize,
}

#[derive(Clone, Copy)]
struct PageSurfaceRequest {
    token: RequestToken,
    page: usize,
    zoom_percent: f32,
    scale_factor: f32,
    pixel_width: usize,
}

#[derive(Clone, Copy, Debug)]
struct DynamicFidelityPaintObservation {
    capture_sequence: u64,
    page_number: usize,
    outer_bounds_window_logical: ViewerRect,
    page_width_points: f32,
    page_height_points: f32,
    render_generation: u64,
    current_raster_ready: bool,
}

fn prune_page_surface_work_to_visible_pages(
    document_id: u64,
    visible_pages: &HashSet<usize>,
    pending: &mut HashMap<(u64, usize, usize), u64>,
    queue: &mut VecDeque<PageSurfaceRequest>,
) {
    pending.retain(|(candidate_document_id, page, _), _| {
        *candidate_document_id != document_id || visible_pages.contains(page)
    });
    queue.retain(|request| {
        request.token.document_id != document_id || visible_pages.contains(&request.page)
    });
}

#[derive(Clone, Copy)]
struct TileJob {
    document_id: u64,
    zoom_percent: f32,
    scale_factor: f32,
    request: TileRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TilePlanKey {
    document_id: u64,
    revision: u64,
    page: usize,
    zoom_tenths: i32,
    scale_millis: i32,
    scroll_x: i32,
    scroll_y: i32,
    viewport_width: i32,
    viewport_height: i32,
}

fn clamp_zoom_percent(zoom_percent: f32) -> f32 {
    if zoom_percent <= MIN_ZOOM_PERCENT {
        return MIN_ZOOM_PERCENT;
    }
    if zoom_percent >= MAX_ZOOM_PERCENT {
        return MAX_ZOOM_PERCENT;
    }
    (zoom_percent * 10.0).round() / 10.0
}

fn initial_pdf_paths(args: impl IntoIterator<Item = OsString>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg.as_os_str() == "-ApplePersistenceIgnoreState" {
            let _ = args.next();
            continue;
        }
        paths.push(PathBuf::from(arg));
    }
    paths
}

fn quantize_fit_zoom_down(zoom_percent: f32) -> f32 {
    ((zoom_percent / FIT_ZOOM_STEP_PERCENT).floor() * FIT_ZOOM_STEP_PERCENT)
        .clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT)
}

fn resolve_fit_width_zoom(viewport_width: f32, page_width: f32) -> f32 {
    quantize_fit_zoom_down(
        viewport_width.max(1.0) / (page_width + PAGE_LAYOUT_GAP * 2.0).max(1.0) * 100.0,
    )
}

fn resolve_fit_page_zoom(
    viewport_width: f32,
    viewport_height: f32,
    page_width: f32,
    page_height: f32,
) -> f32 {
    let width_zoom = resolve_fit_width_zoom(viewport_width, page_width);
    let height_zoom =
        viewport_height.max(1.0) / (page_height + PAGE_LAYOUT_GAP * 2.0).max(1.0) * 100.0;
    quantize_fit_zoom_down(width_zoom.min(height_zoom))
}

fn viewport_content_width(
    window_width: f32,
    left_sidebar_visible: bool,
    scroll_mode: ScrollMode,
    open_document_count: usize,
) -> f32 {
    let left_sidebar_width = if left_sidebar_visible {
        SIDEBAR_WIDTH
    } else {
        0.0
    };
    let viewport_x = RAIL_WIDTH + left_sidebar_width;
    let scrollbar_width = if scroll_mode == ScrollMode::Continuous && open_document_count > 0 {
        VIEWPORT_SCROLLBAR_WIDTH
    } else {
        0.0
    };
    (window_width - viewport_x - RIGHT_RAIL_WIDTH - scrollbar_width).max(1.0)
}

fn viewport_content_rect(
    window_width: f32,
    window_height: f32,
    left_sidebar_visible: bool,
    scroll_mode: ScrollMode,
    open_document_count: usize,
) -> ViewerRect {
    let left_sidebar_width = if left_sidebar_visible {
        SIDEBAR_WIDTH
    } else {
        0.0
    };
    let viewport_x = RAIL_WIDTH + left_sidebar_width;
    let viewport_y = WINDOW_TITLE_BAR_HEIGHT
        + MENU_BAR_HEIGHT
        + DOCUMENT_TAB_BAR_HEIGHT
        + PRIMARY_BAND_HEIGHT
        + VIEWPORT_TOP_BORDER_WIDTH;
    ViewerRect::new(
        viewport_x,
        viewport_y,
        viewport_content_width(
            window_width,
            left_sidebar_visible,
            scroll_mode,
            open_document_count,
        ),
        (window_height - viewport_y).max(1.0),
    )
}

fn open_pdf_default_zoom(
    perf_scenario: Option<PerfScenarioKind>,
    window_width: f32,
    page_width: f32,
) -> Option<(f32, ZoomPreset)> {
    (perf_scenario == Some(PerfScenarioKind::OpenPdf)).then(|| {
        (
            resolve_fit_width_zoom(
                viewport_content_width(window_width, true, ScrollMode::Continuous, 1),
                page_width,
            ),
            ZoomPreset::FitWidth,
        )
    })
}

fn continuous_scrollbar_thumb(
    viewport_height: f32,
    content_height: f32,
    scroll_offset_y: f32,
) -> (f32, f32) {
    let viewport_height = viewport_height.max(1.0);
    let content_height = content_height.max(viewport_height);
    let thumb_height = (viewport_height * viewport_height / content_height).clamp(
        VIEWPORT_SCROLLBAR_MIN_THUMB_HEIGHT.min(viewport_height),
        viewport_height,
    );
    let maximum_scroll = (content_height - viewport_height).max(0.0);
    let thumb_travel = (viewport_height - thumb_height).max(0.0);
    let progress = if maximum_scroll > 0.0 {
        (-scroll_offset_y).clamp(0.0, maximum_scroll) / maximum_scroll
    } else {
        0.0
    };
    (thumb_travel * progress, thumb_height)
}

#[cfg(test)]
fn viewport_page_numbers(
    scroll_mode: ScrollMode,
    page_count: usize,
    current_page: usize,
) -> Vec<usize> {
    match scroll_mode {
        ScrollMode::Continuous => (1..=page_count).collect(),
        ScrollMode::SinglePage => (page_count > 0)
            .then(|| current_page.clamp(1, page_count))
            .into_iter()
            .collect(),
    }
}

fn initial_scroll_mode(perf_scenario: Option<PerfScenarioKind>) -> ScrollMode {
    if matches!(
        perf_scenario,
        Some(
            PerfScenarioKind::EditorCreate
                | PerfScenarioKind::EditorWorkload
                | PerfScenarioKind::PersistenceWorkload
                | PerfScenarioKind::FitModes
                | PerfScenarioKind::CachePressureRecovery
                | PerfScenarioKind::MultiDocumentSession
                | PerfScenarioKind::NativePropertyEditUndo
                | PerfScenarioKind::NativeSnapTransform
        )
    ) {
        ScrollMode::SinglePage
    } else {
        ScrollMode::Continuous
    }
}

fn initial_sidebar_visible(perf_scenario: Option<PerfScenarioKind>) -> bool {
    !matches!(
        perf_scenario,
        Some(
            PerfScenarioKind::OpenPdf
                | PerfScenarioKind::EditorCreate
                | PerfScenarioKind::EditorWorkload
                | PerfScenarioKind::PersistenceWorkload
                | PerfScenarioKind::MultiDocumentSession
                | PerfScenarioKind::NativePropertyEditUndo
                | PerfScenarioKind::NativeSnapTransform
        )
    )
}

fn sidebar_visible_after_document_open(
    current: bool,
    perf_scenario: Option<PerfScenarioKind>,
) -> bool {
    if perf_scenario == Some(PerfScenarioKind::OpenPdf) {
        true
    } else {
        current
    }
}

fn should_scroll_viewport_wheel(mode: ScrollWheelMode, control_pressed: bool) -> bool {
    match mode {
        ScrollWheelMode::Scroll => !control_pressed,
        ScrollWheelMode::Zoom => control_pressed,
    }
}

fn native_scroll_expected_events(
    forward_duration_ms: u64,
    reverse_duration_ms: u64,
    rate_hz: u32,
) -> Option<(usize, usize)> {
    let reverse_clicks_per_interval = forward_duration_ms.checked_div(reverse_duration_ms)?;
    if reverse_clicks_per_interval == 0
        || reverse_clicks_per_interval.saturating_mul(reverse_duration_ms) != forward_duration_ms
    {
        return None;
    }
    let forward = forward_duration_ms
        .checked_mul(u64::from(rate_hz))?
        .checked_div(1_000)?;
    let reverse_intervals = reverse_duration_ms
        .checked_mul(u64::from(rate_hz))?
        .checked_div(1_000)?;
    if forward.saturating_mul(1_000) != forward_duration_ms.saturating_mul(u64::from(rate_hz))
        || reverse_intervals.saturating_mul(1_000)
            != reverse_duration_ms.saturating_mul(u64::from(rate_hz))
    {
        return None;
    }
    Some((
        usize::try_from(forward).ok()?,
        usize::try_from(reverse_intervals.checked_mul(reverse_clicks_per_interval)?).ok()?,
    ))
}

fn native_distance_bounded_expected_events(
    viewport_height: f32,
    forward_viewport_heights: f64,
    wheel_delta: f32,
) -> Option<usize> {
    if !viewport_height.is_finite()
        || viewport_height <= 0.0
        || !forward_viewport_heights.is_finite()
        || forward_viewport_heights <= 0.0
        || !wheel_delta.is_finite()
        || wheel_delta <= 0.0
    {
        return None;
    }
    let requested_distance = viewport_height * forward_viewport_heights as f32;
    let events = (requested_distance / wheel_delta).round() as usize;
    if events == 0 {
        return None;
    }
    let scheduled_distance = events as f32 * wheel_delta;
    ((scheduled_distance - requested_distance).abs() <= wheel_delta / 2.0 + f32::EPSILON)
        .then_some(events)
}

fn native_distance_bounded_offset_y(
    start_offset_y: f32,
    wheel_delta: f32,
    forward_events: usize,
    reverse_events: usize,
    expected_events: usize,
    forward_direction: i8,
) -> Option<f32> {
    if !start_offset_y.is_finite()
        || !wheel_delta.is_finite()
        || wheel_delta <= 0.0
        || expected_events == 0
        || forward_events > expected_events
        || reverse_events > expected_events
        || reverse_events > forward_events
        || !matches!(forward_direction, -1 | 1)
    {
        return None;
    }
    Some(
        start_offset_y
            + f32::from(forward_direction)
                * wheel_delta
                * forward_events.saturating_sub(reverse_events) as f32,
    )
}

fn native_wheel_delta_is_unit(unit_delta: f32, observed_delta: f32) -> bool {
    if !unit_delta.is_finite() || !observed_delta.is_finite() || unit_delta <= 0.0 {
        return false;
    }
    let tolerance = (unit_delta * 0.01).max(f32::EPSILON * 16.0);
    (observed_delta.abs() - unit_delta).abs() <= tolerance
}

fn native_wheel_calibration_delta(observed_delta: f32) -> Option<f32> {
    let delta = observed_delta.abs();
    (delta.is_finite() && delta > f32::EPSILON).then_some(delta)
}

fn native_peak_distance_matches(actual: f32, expected: f32) -> bool {
    actual.is_finite()
        && expected.is_finite()
        && expected > 0.0
        && (actual - expected).abs() <= (expected * 0.05).max(0.5)
}

fn native_scroll_batched_offset_y(
    start_offset_y: f32,
    viewport_height: f32,
    forward_viewport_heights: f64,
    forward_events: usize,
    expected_forward_events: usize,
    reverse_events: usize,
    expected_reverse_events: usize,
    forward_direction: i8,
) -> Option<f32> {
    if !start_offset_y.is_finite()
        || !viewport_height.is_finite()
        || viewport_height <= 0.0
        || !forward_viewport_heights.is_finite()
        || forward_viewport_heights <= 0.0
        || expected_forward_events == 0
        || expected_reverse_events == 0
        || forward_events > expected_forward_events
        || reverse_events > expected_reverse_events
        || !matches!(forward_direction, -1 | 1)
    {
        return None;
    }
    let forward_progress = forward_events as f32 / expected_forward_events as f32;
    let reverse_progress = reverse_events as f32 / expected_reverse_events as f32;
    let path_progress = (forward_progress - reverse_progress).clamp(0.0, 1.0);
    Some(
        start_offset_y
            + f32::from(forward_direction)
                * forward_viewport_heights as f32
                * viewport_height
                * path_progress,
    )
}

fn resolve_wheel_zoom(zoom_percent: f32, delta: f32) -> f32 {
    let delta = delta.clamp(
        -WHEEL_ZOOM_MAX_DELTA_PER_FRAME,
        WHEEL_ZOOM_MAX_DELTA_PER_FRAME,
    );
    clamp_zoom_percent(zoom_percent * (-delta * BLUEBEAM_TRACKPAD_ZOOM_RATE).exp())
}

fn resolve_single_page_wheel(
    current_page: usize,
    page_count: usize,
    accumulated_delta: f32,
    delta: f32,
) -> (Option<usize>, f32) {
    if page_count <= 1 || !delta.is_finite() || delta.abs() <= f32::EPSILON {
        return (None, 0.0);
    }
    let accumulated = accumulated_delta + delta;
    if accumulated.abs() < SINGLE_PAGE_WHEEL_DELTA_PER_PAGE {
        return (None, accumulated);
    }
    let next_page = if accumulated.is_sign_positive() {
        current_page.saturating_add(1).min(page_count)
    } else {
        current_page.saturating_sub(1).max(1)
    };
    ((next_page != current_page).then_some(next_page), 0.0)
}

struct Assets {
    base: PathBuf,
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        fs::read(self.base.join(path))
            .map(|data| Some(Cow::Owned(data)))
            .map_err(Into::into)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        fs::read_dir(self.base.join(path))
            .map(|entries| {
                entries
                    .filter_map(|entry| {
                        entry
                            .ok()
                            .and_then(|entry| entry.file_name().into_string().ok())
                            .map(SharedString::from)
                    })
                    .collect()
            })
            .map_err(Into::into)
    }
}

fn asset_base() -> PathBuf {
    if let Some(path) = std::env::var_os("BP_GPUI_ASSET_DIR") {
        return path.into();
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(macos_directory) = executable.parent()
        && macos_directory
            .file_name()
            .is_some_and(|name| name == "MacOS")
    {
        return macos_directory.join("../Resources/assets");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn load_fonts(cx: &App) -> Result<()> {
    let geist = cx
        .asset_source()
        .load("fonts/geist/Geist-Variable.ttf")?
        .ok_or_else(|| anyhow::anyhow!("Geist font asset is missing"))?;
    cx.text_system().add_fonts(vec![geist])
}

fn annotation_color(value: &str) -> u32 {
    u32::from_str_radix(&value[1..], 16)
        .expect("annotation appearance colors are validated six-digit hex values")
}

fn presentation_font_family(model_font_family: &str) -> &str {
    if model_font_family == "Helvetica" {
        "Geist"
    } else {
        model_font_family
    }
}

fn annotation_tool_shortcut(
    event: &KeyDownEvent,
    text_input_active: bool,
) -> Option<AnnotationTool> {
    if text_input_active || event.is_held || event.keystroke.modifiers.modified() {
        return None;
    }
    if event.keystroke.key.eq_ignore_ascii_case("escape") {
        return Some(AnnotationTool::Select);
    }
    AnnotationTool::from_plain_shortcut(&event.keystroke.key)
}

fn should_activate_page_from_surface(current_page: usize, target_page: usize) -> bool {
    current_page != target_page
}

fn comparison_checker_asset() -> (DecodedRgbaAsset, Arc<RenderImage>) {
    const WIDTH: usize = 512;
    const HEIGHT: usize = 384;
    let mut rgba = Vec::with_capacity(WIDTH * HEIGHT * 4);
    let mut bgra = Vec::with_capacity(WIDTH * HEIGHT * 4);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut color = if (x / 32 + y / 32).is_multiple_of(2) {
                [29, 110, 216]
            } else {
                [245, 238, 218]
            };
            if x.abs_diff(WIDTH / 2) < 3 || y.abs_diff(HEIGHT / 2) < 3 {
                color = [220, 38, 38];
            }
            rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
            bgra.extend_from_slice(&[color[2], color[1], color[0], 255]);
        }
    }
    let asset = DecodedRgbaAsset::new(WIDTH as u32, HEIGHT as u32, rgba)
        .expect("the generated checker has valid bounded RGBA geometry");
    let pixels = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(WIDTH as u32, HEIGHT as u32, bgra)
        .expect("the generated checker has exact BGRA dimensions");
    let image = Arc::new(RenderImage::new(smallvec![Frame::new(pixels)]));
    (asset, image)
}

struct Gallery {
    active_tab: usize,
    capture_shell: bool,
    sidebar_visible: bool,
    annotation_adapter: AnnotationAdapter,
    annotation_focus: FocusHandle,
    annotation_pointer_button: AnnotationPointerButtonState,
    annotation_text_cursor: usize,
    native_editor_text_input: Option<NativeTextEntryProbe>,
    annotation_image_asset: DecodedRgbaAsset,
    annotation_image: Arc<RenderImage>,
    rectangle_snap_enabled: bool,
    open_popup: Option<OpenPopup>,
    popup_selection: usize,
    selected_template: TemplateChoice,
    continuous_wheel_mode: ScrollWheelMode,
    single_page_wheel_mode: ScrollWheelMode,
    single_page_wheel_delta: f32,
    documents: Vec<PdfDocument>,
    active_document: Option<usize>,
    document_fixture_ids: HashMap<u64, String>,
    document_error: Option<String>,
    document_scroll: ScrollHandle,
    continuous_scroll: ScrollHandle,
    native_scroll_pending_offset_y: Option<f32>,
    native_scroll_flush_scheduled: bool,
    thumbnail_scroll: UniformListScrollHandle,
    zoom_percent: f32,
    display_scale_factor: f32,
    zoom_preset: ZoomPreset,
    scroll_mode: ScrollMode,
    next_document_id: u64,
    next_request_generation: u64,
    latest_open_request: Option<u64>,
    pending_open_requests: HashSet<u64>,
    pending_open_fixture_ids: HashMap<u64, String>,
    pending_viewport_requests: HashMap<u64, ViewportRequest>,
    active_viewport_jobs: HashSet<u64>,
    pending_thumbnail_requests: HashMap<(u64, usize), u64>,
    thumbnail_queue: VecDeque<ThumbnailRequest>,
    active_thumbnail_jobs: usize,
    thumbnail_failures: HashMap<(u64, usize), String>,
    pending_page_surface_requests: HashMap<(u64, usize, usize), u64>,
    page_surface_queue: VecDeque<PageSurfaceRequest>,
    active_page_surface_jobs: usize,
    page_surface_failures: HashMap<(u64, usize, usize), String>,
    render_planner: RenderPlanner,
    tile_plan_key: Option<TilePlanKey>,
    visible_tile_requests: Vec<TileRequest>,
    pending_tile_requests: HashSet<TileRequest>,
    tile_queue: VecDeque<TileJob>,
    active_tile_requests: HashSet<TileRequest>,
    active_tile_jobs: usize,
    tile_cache: TileCache<Arc<RenderImage>>,
    continuous_page_layouts: Vec<PageLayout>,
    continuous_visible_pages: Vec<usize>,
    continuous_total_height: f32,
    continuous_plan_generation: u64,
    dynamic_fidelity_state_sequence: u64,
    dynamic_fidelity_paint_capture_sequence: u64,
    dynamic_fidelity_pending_state: Option<Map<String, Value>>,
    dynamic_fidelity_ready_state: Option<Map<String, Value>>,
    dynamic_fidelity_painted_pages:
        Arc<Mutex<HashMap<(u64, usize), DynamicFidelityPaintObservation>>>,
    dynamic_fidelity_exported_pages: HashSet<usize>,
    initial_page: usize,
    diagnostics: bool,
    perf_scenario: Option<PerfScenario>,
    perf_initial_pdfs: Vec<PathBuf>,
    perf_reopen_path: Option<PathBuf>,
    editor_presentation_pending: Option<bool>,
    editor_dense_presentation_pending: bool,
    editor_overlay_document_id: Option<u64>,
    perf_window_handle: Option<AnyWindowHandle>,
    last_window_logical_size: Option<[f32; 2]>,
    annotation_overlay_paint: Arc<Mutex<Option<AnnotationOverlayPaintObservation>>>,
    annotation_image_atlas_paint: Arc<Mutex<Option<AnnotationImageAtlasPaintObservation>>>,
    tile_atlas_uploads: Arc<Mutex<HashMap<usize, usize>>>,
}

impl Gallery {
    fn native_editor_annotation_id(stage: NativeEditorStage) -> Option<&'static str> {
        match stage {
            NativeEditorStage::Text => Some(TEXT_CREATE_ID),
            NativeEditorStage::Length => Some(LENGTH_CREATE_ID),
            NativeEditorStage::Image => Some(IMAGE_CREATE_ID),
            NativeEditorStage::Scale => None,
        }
    }

    fn document(&self) -> Option<&PdfDocument> {
        self.active_document
            .and_then(|index| self.documents.get(index))
    }

    fn active_fixture_id(&self) -> Option<String> {
        let document_id = self.document()?.id;
        self.document_fixture_ids.get(&document_id).cloned()
    }

    fn emit_comparison_view_state(&mut self, checkpoint: &'static str, window_size: [f32; 2]) {
        let Some((kind, already_emitted)) = self.perf_scenario.as_ref().map(|scenario| {
            (
                scenario.kind,
                match checkpoint {
                    "measurement-start" => scenario.comparison_view_state_start_emitted,
                    "measurement-end" => scenario.comparison_view_state_end_emitted,
                    _ => true,
                },
            )
        }) else {
            return;
        };
        if !kind.benefit_metrics_eligible() || already_emitted {
            return;
        }
        let observation = ComparisonViewStateObservation::from_live_state(
            kind.as_str(),
            checkpoint,
            window_size[0],
            window_size[1],
            self.display_scale_factor,
            self.scroll_mode,
            self.zoom_preset,
            self.zoom_percent,
            self.sidebar_visible,
            self.active_fixture_id(),
            self.active_document,
            self.documents.len(),
        );
        perf::emit("comparison-view-state", observation.into_fields());
        if let Some(scenario) = self.perf_scenario.as_mut() {
            match checkpoint {
                "measurement-start" => scenario.comparison_view_state_start_emitted = true,
                "measurement-end" => scenario.comparison_view_state_end_emitted = true,
                _ => {}
            }
        }
    }

    fn next_request(&mut self, document_id: u64) -> RequestToken {
        let token = RequestToken {
            document_id,
            generation: self.next_request_generation,
        };
        self.next_request_generation = self.next_request_generation.saturating_add(1);
        token
    }

    fn select_tool(&mut self, tool: AnnotationTool, cx: &mut Context<Self>) {
        if tool == AnnotationTool::Image && self.perf_scenario.is_none() {
            self.select_image_asset_dialog(cx);
            return;
        }
        if let Err(error) = self.annotation_adapter.set_tool(tool) {
            self.report_annotation_error(error, cx);
            return;
        }
        self.document_error = None;
        cx.notify();
    }

    fn select_image_asset_dialog(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose PNG or JPEG".into()),
        });
        cx.spawn(async move |entity, cx| {
            let result = picker.await;
            let _ = entity.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => {
                    let Some(path) = paths.into_iter().next() else {
                        return;
                    };
                    match ingest_image_asset_from_path(&path) {
                        Ok(ingested) => {
                            let (annotation_asset, render_image) = ingested.into_parts();
                            this.annotation_adapter
                                .set_image_asset(annotation_asset.clone());
                            this.annotation_image_asset = annotation_asset;
                            this.annotation_image = render_image;
                            if let Err(error) =
                                this.annotation_adapter.set_tool(AnnotationTool::Image)
                            {
                                this.report_annotation_error(error, cx);
                                return;
                            }
                            this.document_error = None;
                            cx.notify();
                        }
                        Err(error) => {
                            this.document_error =
                                Some(format!("Could not load annotation image: {error}"));
                            cx.notify();
                        }
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.document_error = Some(format!("Could not open the file picker: {error}"));
                    cx.notify();
                }
                Err(error) => {
                    this.document_error =
                        Some(format!("The file picker closed unexpectedly: {error}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn cancel_active_annotation_gesture(&mut self, reason: PointerCancelReason) {
        self.annotation_pointer_button.cancel();
        let _ = self.annotation_adapter.cancel(reason);
    }

    fn annotation_transform(page_height: f32, zoom_percent: f32) -> PageTransform {
        let pixels_per_point = f64::from(zoom_percent) / 100.0;
        PageTransform::new(f64::from(page_height) / pixels_per_point, pixels_per_point)
            .expect("the rendered PDF page has positive finite geometry")
    }

    fn canonicalize_pdf_coordinate(value: f64) -> f64 {
        let integer = value.round();
        if (value - integer).abs() <= 0.02 {
            integer
        } else {
            value
        }
    }

    fn annotation_point(
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        transform: PageTransform,
    ) -> Result<PdfPoint, AnnotationError> {
        let point = transform.point_from_local_pixels(
            f64::from(position.x - bounds.origin.x),
            f64::from(position.y - bounds.origin.y),
        )?;
        PdfPoint::new(
            Self::canonicalize_pdf_coordinate(point.x),
            Self::canonicalize_pdf_coordinate(point.y),
        )
    }

    fn report_annotation_error(&mut self, error: AnnotationError, cx: &mut Context<Self>) {
        self.cancel_active_annotation_gesture(PointerCancelReason::AdapterError);
        self.document_error = Some(format!("Annotation interaction failed: {error}"));
        cx.notify();
    }

    fn undo_annotation(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            return;
        };
        match self.annotation_adapter.undo(document_id) {
            Ok(()) => cx.notify(),
            Err(error) => self.report_annotation_error(error, cx),
        }
    }

    fn redo_annotation(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            return;
        };
        match self.annotation_adapter.redo(document_id) {
            Ok(()) => cx.notify(),
            Err(error) => self.report_annotation_error(error, cx),
        }
    }

    fn toggle_annotation_lock(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            return;
        };
        let locked = self.annotation_adapter.selected_is_locked(document_id);
        match self
            .annotation_adapter
            .set_selected_locked(document_id, !locked)
        {
            Ok(()) => cx.notify(),
            Err(error) => self.report_annotation_error(error, cx),
        }
    }

    fn apply_comparison_layout(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            return;
        };
        let result = match self.annotation_adapter.selected_kind(document_id) {
            Some(AnnotationKind::TextBox) => {
                self.annotation_adapter
                    .resize_selected_text(document_id, 300.0, 84.0)
            }
            Some(AnnotationKind::Image) => {
                self.annotation_adapter
                    .resize_selected_image(document_id, 180.0, 135.0)
            }
            _ => return,
        };
        match result {
            Ok(()) => cx.notify(),
            Err(error) => self.report_annotation_error(error, cx),
        }
    }

    fn commit_selected_rectangle_stroke_width(
        &mut self,
        stroke_width_pt: f64,
    ) -> Result<PropertyEditCommit, String> {
        let document_id = self
            .document()
            .map(|document| document.id)
            .ok_or_else(|| "rectangle properties require an open document".to_owned())?;
        self.annotation_adapter
            .commit_selected_rectangle_stroke_width(document_id, stroke_width_pt)
            .map_err(|error| error.to_string())
    }

    fn apply_selected_rectangle_stroke_width(
        &mut self,
        stroke_width_pt: f64,
        cx: &mut Context<Self>,
    ) {
        match self.commit_selected_rectangle_stroke_width(stroke_width_pt) {
            Ok(_) => {
                if (stroke_width_pt - 4.0).abs() <= f64::EPSILON
                    && let Some(state) = self
                        .perf_scenario
                        .as_mut()
                        .and_then(|scenario| scenario.multi_document.as_mut())
                    && state.stage == MultiDocumentStage::Editing
                    && state.edit_setup_complete
                {
                    state.edit_committed = true;
                    state.native_action_started_ms = Some(perf::elapsed_ms());
                }
                self.document_error = None;
                cx.notify();
            }
            Err(error) => {
                self.document_error = Some(format!("Rectangle property change failed: {error}"));
                cx.notify();
            }
        }
    }

    fn toggle_rectangle_snap(&mut self, cx: &mut Context<Self>) {
        let enabled = !self.rectangle_snap_enabled;
        let settings = RectangleSnapSettings::new(enabled, 18.0, 8.0)
            .expect("the product rectangle snap defaults are valid");
        match self
            .annotation_adapter
            .set_rectangle_snap_settings(settings)
        {
            Ok(()) => {
                self.rectangle_snap_enabled = enabled;
                self.document_error = None;
                cx.notify();
            }
            Err(error) => self.report_annotation_error(error, cx),
        }
    }

    fn annotation_mouse_down(
        &mut self,
        location: AnnotationPointerLocation,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.annotation_pointer_button.begin_press() {
            return;
        }
        if let Err(error) = self
            .annotation_adapter
            .set_observed_pixels_per_point(f64::from(self.zoom_percent) / 100.0)
        {
            self.report_annotation_error(error, cx);
            return;
        }
        let point = match Self::annotation_point(location.bounds, position, location.transform) {
            Ok(point) => point,
            Err(error) => {
                self.report_annotation_error(error, cx);
                return;
            }
        };
        let page_index = u32::try_from(location.page.saturating_sub(1)).unwrap_or(u32::MAX);
        let tolerance = location
            .transform
            .tolerance_points(ANNOTATION_HIT_TOLERANCE_PX)
            .expect("the annotation hit tolerance is finite and nonnegative");
        let native_v5_snap = self.perf_scenario.as_ref().and_then(|scenario| {
            let ComparisonPhase::NativeV5SnapInput {
                pixels_per_point, ..
            } = scenario.comparison_phase
            else {
                return None;
            };
            let state = scenario.native_editing_v5.as_ref()?;
            (state.snap_sample_timestamps_ms.is_empty())
                .then(|| (state.plan.snap_transform.clone(), pixels_per_point))
        });
        if let Some((plan, pixels_per_point)) = native_v5_snap {
            if point != plan.start {
                self.fail_comparison_scenario(
                    format!(
                        "native v5 snap pointer down was ({:.3}, {:.3}); expected ({:.3}, {:.3})",
                        point.x, point.y, plan.start.x, plan.start.y
                    ),
                    cx,
                );
                return;
            }
            if let Err(error) = self.annotation_adapter.begin_native_v5_snap(
                location.document_id,
                &plan,
                ANNOTATION_MOUSE_POINTER_ID,
                tolerance,
                pixels_per_point,
            ) {
                self.fail_comparison_scenario(error.to_string(), cx);
                return;
            }
            match self
                .annotation_adapter
                .update_native_v5_snap(location.document_id, point)
            {
                Ok(resolution) => {
                    self.record_native_v5_snap_sample(point, perf::elapsed_ms(), resolution);
                    self.annotation_focus.focus(window, cx);
                    cx.notify();
                }
                Err(error) => self.fail_comparison_scenario(error.to_string(), cx),
            }
            return;
        }
        let text_tool = self.annotation_adapter.tool() == AnnotationTool::TextBox;
        if self.annotation_adapter.tool() == AnnotationTool::Image {
            let Some((page_width, page_height)) = self
                .document()
                .map(|document| document.page_size(location.page))
            else {
                self.report_annotation_error(
                    AnnotationError::InvalidGeometry("image tool requires an open page".into()),
                    cx,
                );
                return;
            };
            if let Err(error) = self.annotation_adapter.set_image_placement_page(
                f64::from(page_width),
                f64::from(page_height),
                NATURAL_IMAGE_MAX_PAGE_FRACTION,
            ) {
                self.report_annotation_error(error, cx);
                return;
            }
        }
        if let Some(id) = self
            .perf_scenario
            .as_ref()
            .and_then(native_pending_annotation_id)
            .and_then(|id| MarkupId::new(id).ok())
        {
            // Requeue at the verified page event boundary. A stale click during
            // a native phase transition must not consume the frozen command ID.
            self.annotation_adapter.queue_next_annotation_id(id);
        }
        match self.annotation_adapter.pointer_down(
            location.document_id,
            page_index,
            ANNOTATION_MOUSE_POINTER_ID,
            point,
            tolerance,
        ) {
            Ok(outcome) => {
                self.record_native_annotation_coordinate();
                if text_tool {
                    let content = self
                        .annotation_adapter
                        .selected_text(location.document_id)
                        .unwrap_or_default()
                        .to_owned();
                    let native_text_create = self.perf_scenario.as_ref().is_some_and(|scenario| {
                        matches!(
                            scenario.comparison_phase,
                            ComparisonPhase::NativeEditorInput {
                                stage: NativeEditorStage::Text,
                                ..
                            }
                        )
                    });
                    if native_text_create {
                        self.annotation_text_cursor = 0;
                        self.native_editor_text_input = Some(NativeTextEntryProbe {
                            content_before: content,
                            history_before: self
                                .annotation_adapter
                                .history_depths(location.document_id)
                                .0,
                            scene_revision_before: self
                                .annotation_adapter
                                .document_scene(location.document_id, page_index)
                                .revision,
                            submitted_text: String::new(),
                        });
                    } else {
                        self.annotation_text_cursor = content.len();
                    }
                    self.annotation_focus.focus(window, cx);
                }
                self.document_error = None;
                if matches!(outcome, butter_paper_gpui_gallery::annotation_adapter::PointerPhaseOutcome::AnnotationCreated(_))
                    && self.perf_scenario.as_ref().is_some_and(|scenario| {
                        matches!(
                            scenario.comparison_phase,
                            ComparisonPhase::NativeEditorInput {
                                stage: NativeEditorStage::Image,
                                ..
                            }
                        )
                    })
                {
                    self.finish_native_editor_input(false, cx);
                }
                cx.notify();
            }
            Err(error) => self.report_annotation_error(error, cx),
        }
    }

    fn annotation_mouse_move(
        &mut self,
        document_id: u64,
        page: usize,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        transform: PageTransform,
        cx: &mut Context<Self>,
    ) {
        let page_index = u32::try_from(page.saturating_sub(1)).unwrap_or(u32::MAX);
        if self.perf_scenario.as_ref().is_some_and(|scenario| {
            matches!(
                scenario.comparison_phase,
                ComparisonPhase::NativeV5SnapInput { .. }
            )
        }) {
            let point = match Self::annotation_point(bounds, position, transform) {
                Ok(point) => point,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            let repeated_position = self
                .perf_scenario
                .as_ref()
                .and_then(|scenario| scenario.native_editing_v5.as_ref())
                .is_some_and(|state| state.snap_last_sample_point == Some(point));
            if repeated_position {
                return;
            }
            let sample_t_ms = perf::elapsed_ms();
            match self
                .annotation_adapter
                .update_native_v5_snap(document_id, point)
            {
                Ok(resolution) => {
                    self.record_native_v5_snap_sample(point, sample_t_ms, resolution);
                    cx.notify();
                }
                Err(error) => self.fail_comparison_scenario(error.to_string(), cx),
            }
            return;
        }
        if self.annotation_adapter.active_surface() != Some((document_id, page_index)) {
            return;
        }
        let result = Self::annotation_point(bounds, position, transform).and_then(|point| {
            self.annotation_adapter
                .pointer_move(ANNOTATION_MOUSE_POINTER_ID, point)
                .map(|_| ())
        });
        if let Err(error) = result {
            self.report_annotation_error(error, cx);
        } else {
            self.record_native_annotation_coordinate();
            cx.notify();
        }
    }

    fn annotation_capture_lost(&mut self, document_id: u64, page: usize, cx: &mut Context<Self>) {
        let page_index = u32::try_from(page.saturating_sub(1)).unwrap_or(u32::MAX);
        if self.annotation_adapter.active_surface() == Some((document_id, page_index)) {
            self.cancel_active_annotation_gesture(PointerCancelReason::CaptureLost);
            cx.notify();
        } else {
            self.annotation_pointer_button.cancel();
        }
    }

    fn annotation_mouse_up(
        &mut self,
        document_id: u64,
        page: usize,
        bounds: Bounds<Pixels>,
        position: Point<Pixels>,
        transform: PageTransform,
        cx: &mut Context<Self>,
    ) {
        if !self.annotation_pointer_button.end_press() {
            return;
        }
        let page_index = u32::try_from(page.saturating_sub(1)).unwrap_or(u32::MAX);
        if self.perf_scenario.as_ref().is_some_and(|scenario| {
            matches!(
                scenario.comparison_phase,
                ComparisonPhase::NativeV5SnapInput { .. }
            )
        }) {
            let point = match Self::annotation_point(bounds, position, transform) {
                Ok(point) => point,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            let expected_end = self
                .perf_scenario
                .as_ref()
                .and_then(|scenario| scenario.native_editing_v5.as_ref())
                .map(|state| state.plan.snap_transform.unsnapped_end);
            if expected_end != Some(point) {
                self.fail_comparison_scenario(
                    format!(
                        "native v5 snap pointer up was ({:.3}, {:.3}); expected the frozen endpoint",
                        point.x, point.y
                    ),
                    cx,
                );
                return;
            }
            if let Some(state) = self
                .perf_scenario
                .as_mut()
                .and_then(|scenario| scenario.native_editing_v5.as_mut())
            {
                state.snap_pointer_up_t_ms = Some(perf::elapsed_ms());
            }
            let receipt = match self
                .annotation_adapter
                .commit_native_v5_snap(document_id, point)
            {
                Ok(receipt) => receipt,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            *self
                .annotation_overlay_paint
                .lock()
                .expect("annotation presentation marker lock") = None;
            if let Some(scenario) = self.perf_scenario.as_mut() {
                scenario.comparison_phase = ComparisonPhase::Idle;
                if let Some(state) = scenario.native_editing_v5.as_mut() {
                    state.snap_commit = Some(receipt);
                    state.stage = NativeEditingV5Stage::SnapAwaitingPaint;
                    state.native_ready_emitted = false;
                }
            }
            cx.notify();
            return;
        }
        if self.annotation_adapter.active_surface() != Some((document_id, page_index)) {
            return;
        }
        let result = Self::annotation_point(bounds, position, transform).and_then(|point| {
            self.annotation_adapter
                .pointer_up(ANNOTATION_MOUSE_POINTER_ID, point)
                .map(|_| ())
        });
        if let Err(error) = result {
            self.report_annotation_error(error, cx);
        } else {
            self.document_error = None;
            if self.perf_scenario.as_ref().is_some_and(|scenario| {
                matches!(
                    scenario.comparison_phase,
                    ComparisonPhase::NativeEditorInput {
                        stage: NativeEditorStage::Length,
                        ..
                    }
                )
            }) {
                self.finish_native_editor_input(false, cx);
            } else if self.perf_scenario.as_ref().is_some_and(|scenario| {
                matches!(
                    scenario.comparison_phase,
                    ComparisonPhase::NativeTransformInput { .. }
                )
            }) {
                self.finish_native_transform_input(cx);
            } else {
                self.finish_native_annotation_input(cx);
            }
            cx.notify();
        }
    }

    fn record_native_v5_snap_sample(
        &mut self,
        point: PdfPoint,
        sample_t_ms: f64,
        resolution: SnapResolution,
    ) {
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.native_editing_v5.as_mut())
        {
            state.snap_sample_timestamps_ms.push(sample_t_ms);
            state.snap_first_sample_point.get_or_insert(point);
            state.snap_last_sample_point = Some(point);
            state.snap_guide_active = resolution.acquired;
            if resolution.acquired {
                state.snap_target_acquired_count =
                    state.snap_target_acquired_count.saturating_add(1);
            }
            state.snap_resolution = Some(resolution);
        }
    }

    fn toggle_popup(&mut self, popup: OpenPopup, cx: &mut Context<Self>) {
        self.open_popup = (self.open_popup != Some(popup)).then_some(popup);
        self.popup_selection = 0;
        cx.notify();
    }

    fn close_popup(&mut self, cx: &mut Context<Self>) {
        self.open_popup = None;
        self.popup_selection = 0;
        cx.notify();
    }

    fn popup_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(popup) = self.open_popup else {
            return;
        };
        let item_count = match popup {
            OpenPopup::TemplatePicker => 3,
            OpenPopup::ContinuousWheel | OpenPopup::SinglePageWheel => 2,
        };
        match event.keystroke.key.as_str() {
            "down" | "arrowdown" => {
                self.popup_selection = (self.popup_selection + 1) % item_count;
            }
            "up" | "arrowup" => {
                self.popup_selection = (self.popup_selection + item_count - 1) % item_count;
            }
            "enter" => {
                match popup {
                    OpenPopup::TemplatePicker => {
                        self.selected_template = [
                            TemplateChoice::LetterPortrait,
                            TemplateChoice::LetterLandscape,
                            TemplateChoice::A4Portrait,
                        ][self.popup_selection];
                    }
                    OpenPopup::ContinuousWheel => {
                        self.continuous_wheel_mode =
                            [ScrollWheelMode::Scroll, ScrollWheelMode::Zoom][self.popup_selection];
                        self.single_page_wheel_delta = 0.0;
                    }
                    OpenPopup::SinglePageWheel => {
                        self.single_page_wheel_mode =
                            [ScrollWheelMode::Scroll, ScrollWheelMode::Zoom][self.popup_selection];
                        self.single_page_wheel_delta = 0.0;
                    }
                }
                self.close_popup(cx);
                cx.stop_propagation();
                return;
            }
            "escape" => {
                self.close_popup(cx);
                cx.stop_propagation();
                return;
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn create_selected_template(&mut self, cx: &mut Context<Self>) {
        let template = self.selected_template;
        let (width, height) = template.page_size();
        match create_blank_pdf(self.next_document_id, template.label(), width, height) {
            Ok(path) => {
                self.open_popup = None;
                self.open_path(path, cx);
            }
            Err(error) => {
                self.document_error = Some(format!("Could not create template PDF: {error}"));
                self.open_popup = None;
                cx.notify();
            }
        }
    }

    fn template_picker_menu(&self, cx: &mut Context<Self>) -> PopupMenu {
        let menu = PopupMenu::new("template-picker-menu", "New from template")
            .on_dismiss(cx.listener(|this, _, _, cx| this.close_popup(cx)));
        [
            TemplateChoice::LetterPortrait,
            TemplateChoice::LetterLandscape,
            TemplateChoice::A4Portrait,
        ]
        .into_iter()
        .enumerate()
        .fold(menu, |menu, (index, choice)| {
            menu.child(
                PopupMenuItem::new(format!("template-choice-{choice:?}"), choice.label())
                    .selected(self.selected_template == choice)
                    .active(self.popup_selection == index)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_template = choice;
                        this.close_popup(cx);
                    })),
            )
        })
    }

    fn wheel_mode_menu(
        &self,
        popup: OpenPopup,
        current_mode: ScrollWheelMode,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        let prefix = match popup {
            OpenPopup::ContinuousWheel => "continuous",
            OpenPopup::SinglePageWheel => "single-page",
            OpenPopup::TemplatePicker => unreachable!("template picker is not a wheel menu"),
        };
        [ScrollWheelMode::Scroll, ScrollWheelMode::Zoom]
            .into_iter()
            .enumerate()
            .fold(
                PopupMenu::new(format!("{prefix}-wheel-menu"), "Mousewheel Behaviour")
                    .note("Ctrl + mousewheel does the opposite.")
                    .on_dismiss(cx.listener(|this, _, _, cx| this.close_popup(cx))),
                |menu, (index, mode)| {
                    menu.child(
                        PopupMenuItem::new(
                            format!("{prefix}-wheel-{mode:?}"),
                            match mode {
                                ScrollWheelMode::Scroll => "Scroll",
                                ScrollWheelMode::Zoom => "Zoom",
                            },
                        )
                        .selected(current_mode == mode)
                        .active(self.popup_selection == index)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            match popup {
                                OpenPopup::ContinuousWheel => this.continuous_wheel_mode = mode,
                                OpenPopup::SinglePageWheel => this.single_page_wheel_mode = mode,
                                OpenPopup::TemplatePicker => unreachable!(),
                            }
                            this.single_page_wheel_delta = 0.0;
                            this.close_popup(cx);
                        })),
                    )
                },
            )
    }

    fn document_index_by_id(&self, document_id: u64) -> Option<usize> {
        self.documents
            .iter()
            .position(|document| document.id == document_id)
    }

    fn open_path(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        self.open_path_with_fixture(path, None, cx);
    }

    fn open_path_with_fixture(
        &mut self,
        path: std::path::PathBuf,
        fixture_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let document_id = self.next_document_id;
        self.next_document_id = self.next_document_id.saturating_add(1);
        let token = self.next_request(document_id);
        self.latest_open_request = Some(token.generation);
        self.pending_open_requests.insert(token.generation);
        if let Some(fixture_id) = fixture_id {
            self.pending_open_fixture_ids
                .insert(document_id, fixture_id.to_owned());
        }
        self.document_error = None;
        if let Some(scenario) = self.perf_scenario.as_mut()
            && scenario.input_lane == PerfInputLane::NativeX11Xtest
            && scenario.initial_open_requested.is_none()
        {
            scenario.initial_open_requested = Some(token);
        }
        if let Some(scenario) = self.perf_scenario.as_mut()
            && scenario.open_started_ms.is_none()
        {
            let started_ms = perf::elapsed_ms();
            scenario.open_started_ms = Some(started_ms);
            perf::emit(
                "pdf-open-requested",
                perf::fields([("document_id", json!(document_id)), ("path", json!(path))]),
            );
        }
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { PdfDocument::open(document_id, path) });
        cx.spawn(async move |entity, cx| {
            let result = task.await;
            let _ = entity.update(cx, |this, cx| {
                this.finish_open(token, result, cx);
            });
        })
        .detach();
    }

    fn finish_open(
        &mut self,
        token: RequestToken,
        result: Result<PdfDocument, String>,
        cx: &mut Context<Self>,
    ) {
        if !self.pending_open_requests.remove(&token.generation) {
            return;
        }
        match result {
            Ok(document) => {
                self.sidebar_visible = sidebar_visible_after_document_open(
                    self.sidebar_visible,
                    self.perf_scenario.as_ref().map(|scenario| scenario.kind),
                );
                let fixture_id = self.pending_open_fixture_ids.remove(&document.id);
                if fixture_id.as_deref() == Some(DENSE_FIXTURE_ID) {
                    let imported = match self.annotation_adapter.load_density_fixture(
                        document.id,
                        include_str!(
                            "../../performance/fixtures/bp-annotation-density-v1.fixture.json"
                        ),
                    ) {
                        Ok(imported) => imported,
                        Err(error) => {
                            self.document_error =
                                Some(format!("Density annotation import failed: {error}"));
                            self.fail_comparison_scenario(error.to_string(), cx);
                            return;
                        }
                    };
                    if imported.page_count as usize != document.page_count
                        || imported.annotation_count != 1_000
                    {
                        self.fail_comparison_scenario(
                            "density annotation fixture does not match the opened PDF",
                            cx,
                        );
                        return;
                    }
                    perf::emit(
                        "density-annotations-imported",
                        perf::fields([
                            ("fixture_id", json!(imported.fixture_id)),
                            ("document_id", json!(document.id)),
                            ("annotation_count", json!(imported.annotation_count)),
                            (
                                "dense_page_annotation_count",
                                json!(
                                    self.annotation_adapter
                                        .document_scene(document.id, 1)
                                        .rectangles
                                        .len()
                                ),
                            ),
                        ]),
                    );
                }
                if let Some(fixture_id) = fixture_id {
                    self.document_fixture_ids.insert(document.id, fixture_id);
                }
                if let Some(scenario) = self.perf_scenario.as_mut()
                    && scenario.initial_open_requested == Some(token)
                {
                    scenario.initial_open_completed = Some(token);
                }
                if let Some(scenario) = self.perf_scenario.as_mut()
                    && let Some(started_ms) = scenario.open_started_ms
                {
                    perf::emit(
                        "pdf-open-completed",
                        perf::fields([
                            ("duration_ms", json!(perf::elapsed_ms() - started_ms)),
                            ("document_id", json!(document.id)),
                            ("pages", json!(document.page_count)),
                        ]),
                    );
                }
                let previously_active_id = self.document().map(|document| document.id);
                let insert_at = self
                    .documents
                    .partition_point(|existing| existing.id < document.id);
                self.documents.insert(insert_at, document);
                let should_activate = self.latest_open_request == Some(token.generation)
                    || self.active_document.is_none();
                if should_activate {
                    let index = self.document_index_by_id(token.document_id).unwrap();
                    self.active_document = Some(index);
                    self.document_error = None;
                    if let Some(window_width) = self
                        .last_window_logical_size
                        .map(|window_size| window_size[0])
                        && let Some((zoom_percent, zoom_preset)) = open_pdf_default_zoom(
                            self.perf_scenario.as_ref().map(|scenario| scenario.kind),
                            window_width,
                            self.documents[index].page_width,
                        )
                    {
                        self.zoom_percent = zoom_percent;
                        self.zoom_preset = zoom_preset;
                    }
                    let page = self.initial_page;
                    let scroll_y = Some(if self.capture_shell { -255.0 } else { 0.0 });
                    self.request_viewport(token.document_id, page, self.zoom_percent, scroll_y, cx);
                } else {
                    self.active_document = previously_active_id
                        .and_then(|document_id| self.document_index_by_id(document_id));
                }
            }
            Err(error) => {
                self.pending_open_fixture_ids.remove(&token.document_id);
                if self.latest_open_request == Some(token.generation) {
                    self.document_error = Some(error);
                }
            }
        }
        if self.diagnostics {
            let titles = self
                .documents
                .iter()
                .map(|document| document.title.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            eprintln!(
                "BP_GPUI_DIAGNOSTICS tabs={} active={:?} pending_opens={} titles={titles}",
                self.documents.len(),
                self.active_document,
                self.pending_open_requests.len(),
            );
        }
        cx.notify();
    }

    fn select_document(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.documents.len() {
            return;
        }
        self.cancel_active_annotation_gesture(PointerCancelReason::PageChanged);
        self.active_document = Some(index);
        self.document_error = None;
        let document_id = self.documents[index].id;
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.multi_document.as_mut())
            && state.stage == MultiDocumentStage::Switching
            && state.native_ready_emitted
            && state
                .open_observations
                .iter()
                .position(|observation| observation.document_id == document_id)
                .is_some_and(|fixture_index| {
                    MULTI_FIXTURE_IDS[fixture_index] == MULTI_SWITCH_SEQUENCE[state.switch_index]
                })
        {
            state.native_action_started_ms = Some(perf::elapsed_ms());
        }
        let page = self.documents[index].current_page;
        self.document_scroll.set_offset(point(px(0.0), px(0.0)));
        self.scroll_continuous_to_page(page);
        if let Some(document) = self.document() {
            self.thumbnail_scroll.scroll_to_item(
                document.current_page.saturating_sub(1),
                ScrollStrategy::Center,
            );
        }
        self.request_viewport(document_id, page, self.zoom_percent, Some(0.0), cx);
    }

    fn announce_multi_document_tab(
        &mut self,
        document_id: u64,
        bounds: Bounds<Pixels>,
        window: &Window,
    ) {
        let Some((input_lane, target_fixture, ready)) =
            self.perf_scenario.as_ref().and_then(|scenario| {
                let state = scenario.multi_document.as_ref()?;
                (state.stage == MultiDocumentStage::Switching
                    && state.switch_index < MULTI_SWITCH_SEQUENCE.len())
                .then(|| {
                    (
                        scenario.input_lane,
                        MULTI_SWITCH_SEQUENCE[state.switch_index],
                        state.native_ready_emitted,
                    )
                })
            })
        else {
            return;
        };
        if input_lane != PerfInputLane::NativeX11Xtest || ready {
            return;
        }
        let Some(target_document_id) = self.multi_document_id_for_fixture(target_fixture) else {
            return;
        };
        if target_document_id != document_id {
            return;
        }
        #[cfg(feature = "benchmark-evidence")]
        let samples_before = window.input_latency_snapshot().latency_histogram.len();
        #[cfg(not(feature = "benchmark-evidence"))]
        let samples_before = 0;
        let viewport = window.viewport_size();
        perf::emit(
            "native-input-ready",
            perf::fields([
                ("command_id", json!(MULTI_SWITCH_COMMAND_ID)),
                (
                    "stage",
                    json!(format!(
                        "switch-{}",
                        self.perf_scenario
                            .as_ref()
                            .and_then(|scenario| scenario.multi_document.as_ref())
                            .map(|state| state.switch_index)
                            .unwrap_or_default()
                    )),
                ),
                ("fixture_id", json!(target_fixture)),
                (
                    "control",
                    json!({
                        "control_id": format!("multi-document-tab-{target_fixture}"),
                        "window_logical_size": {
                            "width": f32::from(viewport.width),
                            "height": f32::from(viewport.height),
                        },
                        "bounds": {
                            "x": f32::from(bounds.origin.x),
                            "y": f32::from(bounds.origin.y),
                            "width": f32::from(bounds.size.width),
                            "height": f32::from(bounds.size.height),
                        },
                        "point": {
                            "x": f32::from(bounds.origin.x + bounds.size.width / 2.0),
                            "y": f32::from(bounds.origin.y + bounds.size.height / 2.0),
                        },
                    }),
                ),
            ]),
        );
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.multi_document.as_mut())
        {
            state.native_ready_emitted = true;
            state.native_input_samples_before = Some(samples_before);
        }
    }

    fn announce_multi_document_property_control(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &Window,
    ) {
        let Some((input_lane, ready)) = self.perf_scenario.as_ref().and_then(|scenario| {
            let state = scenario.multi_document.as_ref()?;
            (state.stage == MultiDocumentStage::Editing && state.edit_setup_complete)
                .then_some((scenario.input_lane, state.native_ready_emitted))
        }) else {
            return;
        };
        if input_lane != PerfInputLane::NativeX11Xtest || ready {
            return;
        }
        #[cfg(feature = "benchmark-evidence")]
        let samples_before = window.input_latency_snapshot().latency_histogram.len();
        #[cfg(not(feature = "benchmark-evidence"))]
        let samples_before = 0;
        let viewport = window.viewport_size();
        perf::emit(
            "native-input-ready",
            perf::fields([
                ("command_id", json!(MULTI_EDIT_COMMAND_ID)),
                ("stage", json!("property-stroke-width-4pt")),
                ("fixture_id", json!(DENSE_FIXTURE_ID)),
                (
                    "control",
                    json!({
                        "control_id": "rectangle-stroke-width-4",
                        "window_logical_size": {
                            "width": f32::from(viewport.width),
                            "height": f32::from(viewport.height),
                        },
                        "bounds": {
                            "x": f32::from(bounds.origin.x),
                            "y": f32::from(bounds.origin.y),
                            "width": f32::from(bounds.size.width),
                            "height": f32::from(bounds.size.height),
                        },
                        "point": {
                            "x": f32::from(bounds.origin.x + bounds.size.width / 2.0),
                            "y": f32::from(bounds.origin.y + bounds.size.height / 2.0),
                        },
                    }),
                ),
            ]),
        );
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.multi_document.as_mut())
        {
            state.native_ready_emitted = true;
            state.native_input_samples_before = Some(samples_before);
        }
    }

    fn close_document(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.documents.len() {
            return;
        }
        self.cancel_active_annotation_gesture(PointerCancelReason::PageChanged);
        let was_active = self.active_document == Some(index);
        let document_id = self.documents[index].id;
        self.documents.remove(index);
        self.document_fixture_ids.remove(&document_id);
        self.annotation_adapter.remove_document(document_id);
        self.active_document = match self.active_document {
            None => None,
            Some(_) if self.documents.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) if active == index => Some(index.min(self.documents.len() - 1)),
            Some(active) => Some(active),
        };
        self.document_error = None;
        self.pending_viewport_requests.remove(&document_id);
        self.pending_thumbnail_requests
            .retain(|(id, _), _| *id != document_id);
        self.thumbnail_queue
            .retain(|request| request.token.document_id != document_id);
        self.thumbnail_failures
            .retain(|(id, _), _| *id != document_id);
        self.pending_page_surface_requests
            .retain(|(id, _, _), _| *id != document_id);
        self.page_surface_queue
            .retain(|request| request.token.document_id != document_id);
        self.page_surface_failures
            .retain(|(id, _, _), _| *id != document_id);
        self.render_planner.cancel();
        self.tile_plan_key = None;
        self.visible_tile_requests
            .retain(|request| request.source.document_id != document_id);
        self.pending_tile_requests
            .retain(|request| request.source.document_id != document_id);
        self.tile_queue.retain(|job| job.document_id != document_id);
        self.active_tile_requests
            .retain(|request| request.source.document_id != document_id);
        // The current cache is intentionally document-local for the comparison
        // candidate. Releasing it here prevents a closed large sheet from
        // retaining decoded GPU upload resources through a later document.
        self.tile_cache.clear();
        self.document_scroll.set_offset(point(px(0.0), px(0.0)));
        if was_active {
            if let Some((document_id, page)) = self
                .document()
                .map(|document| (document.id, document.current_page))
            {
                self.request_viewport(document_id, page, self.zoom_percent, Some(0.0), cx);
            } else {
                cx.notify();
            }
        } else {
            cx.notify();
        }
    }

    fn open_pdf_dialog(&mut self, cx: &mut Context<Self>) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open PDF".into()),
        });
        cx.spawn(async move |entity, cx| {
            let result = picker.await;
            let _ = entity.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => {
                    let Some(path) = paths.into_iter().next() else {
                        return;
                    };
                    if path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
                    {
                        this.open_path(path, cx);
                    } else {
                        this.document_error = Some("Choose a PDF document.".to_string());
                        cx.notify();
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.document_error = Some(format!("Could not open the file picker: {error}"));
                    cx.notify();
                }
                Err(error) => {
                    this.document_error =
                        Some(format!("The file picker closed unexpectedly: {error}"));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn set_page(&mut self, page: usize, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            return;
        };
        self.cancel_active_annotation_gesture(PointerCancelReason::PageChanged);
        self.request_viewport(document_id, page, self.zoom_percent, Some(0.0), cx);
    }

    fn change_zoom(&mut self, delta: f32, cx: &mut Context<Self>) {
        let next_zoom = if delta.is_sign_positive() {
            self.zoom_percent * 1.1
        } else {
            self.zoom_percent / 1.1
        };
        self.set_zoom(next_zoom, ZoomPreset::Manual, cx);
    }

    fn set_zoom(&mut self, next_zoom: f32, preset: ZoomPreset, cx: &mut Context<Self>) {
        self.cancel_active_annotation_gesture(PointerCancelReason::PageChanged);
        let next_zoom = clamp_zoom_percent(next_zoom);
        if (next_zoom - self.zoom_percent).abs() < f32::EPSILON {
            self.zoom_preset = preset;
            // A deterministic performance sequence can intentionally revisit
            // the current zoom (the first step is 100%). Re-request the
            // viewport for an active benchmark operation so that equal-value
            // steps still produce a completion event instead of waiting
            // forever for a request that was skipped as a no-op.
            let refresh_equal_zoom = should_refresh_equal_zoom(self.perf_scenario.as_ref());
            if refresh_equal_zoom
                && let Some((document_id, page)) = self
                    .document()
                    .map(|document| (document.id, document.current_page))
            {
                self.request_viewport(document_id, page, next_zoom, None, cx);
            }
            cx.notify();
            return;
        }
        self.zoom_percent = next_zoom;
        self.zoom_preset = preset;
        if let Some(document) = self.document() {
            let document_id = document.id;
            let current_page = document.current_page;
            self.pending_page_surface_requests
                .retain(|(id, _, _), _| *id != document_id);
            self.page_surface_queue
                .retain(|request| request.token.document_id != document_id);
            self.request_viewport(document_id, current_page, next_zoom, None, cx);
        } else {
            cx.notify();
        }
    }

    fn request_viewport(
        &mut self,
        document_id: u64,
        page: usize,
        zoom_percent: f32,
        scroll_y: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.document_index_by_id(document_id) else {
            return;
        };
        let page = self.documents[index].select_page(page);
        let token = self.next_request(document_id);
        self.pending_viewport_requests.insert(
            document_id,
            ViewportRequest {
                token,
                page,
                zoom_percent,
                scale_factor: self.display_scale_factor,
                scroll_y,
                started_ms: perf::elapsed_ms(),
            },
        );
        if self.active_document == Some(index) {
            self.document_error = None;
        }
        self.start_viewport_job(document_id, cx);
        cx.notify();
    }

    fn start_viewport_job(&mut self, document_id: u64, cx: &mut Context<Self>) {
        if self.active_viewport_jobs.contains(&document_id) {
            return;
        }
        let Some(request) = self.pending_viewport_requests.get(&document_id).copied() else {
            return;
        };
        let Some(index) = self.document_index_by_id(document_id) else {
            self.pending_viewport_requests.remove(&document_id);
            return;
        };
        let document = self.documents[index].clone();
        self.active_viewport_jobs.insert(document_id);
        let task = cx.background_executor().spawn(async move {
            document.render_viewport(request.page, request.zoom_percent, request.scale_factor)
        });
        cx.spawn(async move |entity, cx| {
            let result = task.await;
            let _ = entity.update(cx, |this, cx| {
                this.finish_viewport(request, result, cx);
            });
        })
        .detach();
    }

    fn finish_viewport(
        &mut self,
        request: ViewportRequest,
        result: Result<RenderedViewport, String>,
        cx: &mut Context<Self>,
    ) {
        self.active_viewport_jobs.remove(&request.token.document_id);
        let current = self
            .pending_viewport_requests
            .get(&request.token.document_id)
            .map(|request| request.token);
        if is_current_request(current, request.token) {
            self.pending_viewport_requests
                .remove(&request.token.document_id);
            if let Some(index) = self.document_index_by_id(request.token.document_id) {
                match result {
                    Ok(rendered) => {
                        let operation = self
                            .perf_scenario
                            .as_mut()
                            .and_then(|scenario| scenario.active_operation.take());
                        let duration_ms = perf::elapsed_ms() - request.started_ms;
                        let displayed_logical_width =
                            rendered.page_width * request.zoom_percent / 100.0;
                        let rendered_device_pixel_ratio = rendered.pixel_width as f32
                            / (displayed_logical_width * request.scale_factor).max(1.0);
                        perf::emit(
                            if operation.is_some() {
                                "operation-raster-completed"
                            } else {
                                "viewport-raster-completed"
                            },
                            perf::fields([
                                ("duration_ms", json!(duration_ms)),
                                ("document_id", json!(request.token.document_id)),
                                ("page", json!(rendered.page)),
                                ("zoom_percent", json!(request.zoom_percent)),
                                ("pixel_width", json!(rendered.pixel_width)),
                                ("displayed_logical_width", json!(displayed_logical_width)),
                                ("display_scale_factor", json!(request.scale_factor)),
                                (
                                    "rendered_device_pixel_ratio",
                                    json!(rendered_device_pixel_ratio),
                                ),
                                ("surface_kind", json!("in-memory-bgra")),
                            ]),
                        );
                        self.documents[index].apply_rendered_viewport(rendered);
                        if let Some(scenario) = self.perf_scenario.as_mut() {
                            if scenario
                                .initial_open_requested
                                .is_some_and(|open| open.document_id == request.token.document_id)
                            {
                                scenario.initial_preview_current = Some(request.token);
                            }
                            if let Some(operation) = operation {
                                scenario.pending_visible_operation = Some(operation);
                            } else if !scenario.initial_viewport_visible {
                                scenario.pending_initial_visible = true;
                            }
                        }
                        if self.active_document == Some(index) {
                            self.document_error = None;
                            if let Some(scroll_y) = request.scroll_y {
                                if self.scroll_mode == ScrollMode::Continuous {
                                    self.scroll_continuous_to_page(request.page);
                                } else {
                                    self.document_scroll
                                        .set_offset(point(px(0.0), px(scroll_y)));
                                }
                                self.thumbnail_scroll.scroll_to_item(
                                    request.page.saturating_sub(1),
                                    ScrollStrategy::Center,
                                );
                            }
                        }
                    }
                    Err(error) if self.active_document == Some(index) => {
                        self.document_error = Some(error);
                    }
                    Err(_) => {}
                }
            }
        }
        if self
            .pending_viewport_requests
            .contains_key(&request.token.document_id)
        {
            self.start_viewport_job(request.token.document_id, cx);
        }
        cx.notify();
    }

    fn record_comparison_milestone(
        &mut self,
        command_id: &str,
        milestone: &str,
    ) -> Result<(), String> {
        let gate = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.comparison_gate.as_mut())
            .ok_or_else(|| "comparison milestone gate is unavailable".to_string())?;
        gate.record(command_id, milestone)
            .map_err(|error| error.to_string())?;
        perf::emit(
            "comparison-milestone",
            perf::fields([
                ("command_id", json!(command_id)),
                ("milestone", json!(milestone)),
            ]),
        );
        Ok(())
    }

    fn fail_comparison_scenario(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        perf::emit(
            "scenario-error",
            perf::fields([("message", json!(message.into()))]),
        );
        self.close_perf_window(cx);
    }

    fn prepare_native_annotation_input(
        &mut self,
        stage: NativeAnnotationStage,
        cx: &mut Context<Self>,
    ) {
        self.scroll_mode = ScrollMode::SinglePage;
        self.document_scroll.set_offset(point(px(0.0), px(-240.0)));
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::annotation_create)
            .cloned()
        else {
            self.fail_comparison_scenario("annotation-create plan is unavailable", cx);
            return;
        };
        let (tool, annotation_id) = match stage {
            NativeAnnotationStage::Rectangle => {
                (AnnotationTool::Rectangle, plan.rectangle.annotation_id)
            }
            NativeAnnotationStage::Highlight => {
                (AnnotationTool::Highlight, plan.highlight.annotation_id)
            }
        };
        let result = (|| -> Result<(), AnnotationError> {
            self.annotation_adapter.set_tool(tool)?;
            let id = MarkupId::new(&annotation_id)?;
            self.annotation_adapter.queue_next_annotation_id(id);
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error.to_string(), cx);
            return;
        }
        let history_before = self
            .document()
            .map(|document| self.annotation_adapter.history_depths(document.id).0)
            .unwrap_or_default();
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::AwaitingNativeInputSurface {
                stage,
                history_before,
            };
        }
        cx.notify();
    }

    fn prepare_native_editor_input(&mut self, stage: NativeEditorStage, cx: &mut Context<Self>) {
        self.scroll_mode = ScrollMode::SinglePage;
        self.document_scroll.set_offset(point(px(0.0), px(-240.0)));
        let result = (|| -> Result<(), AnnotationError> {
            match stage {
                NativeEditorStage::Text => {
                    self.native_editor_text_input = None;
                    self.annotation_adapter.set_tool(AnnotationTool::TextBox)?;
                    self.annotation_adapter
                        .queue_next_annotation_id(MarkupId::new(TEXT_CREATE_ID)?);
                    self.annotation_adapter.queue_next_text_content(" ");
                }
                NativeEditorStage::Scale => {}
                NativeEditorStage::Length => {
                    self.annotation_adapter.set_tool(AnnotationTool::Length)?;
                    self.annotation_adapter
                        .queue_next_annotation_id(MarkupId::new(LENGTH_CREATE_ID)?);
                }
                NativeEditorStage::Image => {
                    self.annotation_adapter.set_tool(AnnotationTool::Image)?;
                    self.annotation_adapter
                        .queue_next_annotation_id(MarkupId::new(IMAGE_CREATE_ID)?);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error.to_string(), cx);
            return;
        }
        let history_before = self
            .document()
            .map(|document| self.annotation_adapter.history_depths(document.id).0)
            .unwrap_or_default();
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::AwaitingNativeEditorInput {
                stage,
                history_before,
            };
        }
        cx.notify();
    }

    fn native_transform_plan() -> Result<NativeRectangleTransformPlan, String> {
        RectangleInteractionScenario::embedded()
            .and_then(|scenario| scenario.native_transform_plan())
            .map_err(|error| error.to_string())
    }

    fn prepare_native_transform_input(
        &mut self,
        stage: NativeTransformStage,
        progress: NativeTransformProgress,
        cx: &mut Context<Self>,
    ) {
        self.scroll_mode = ScrollMode::SinglePage;
        self.document_scroll.set_offset(point(px(0.0), px(-240.0)));
        let plan = match Self::native_transform_plan() {
            Ok(plan) => plan,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        let result = (|| -> Result<(), AnnotationError> {
            match stage {
                NativeTransformStage::PrerequisiteCreate => {
                    self.annotation_adapter
                        .set_tool(AnnotationTool::Rectangle)?;
                    self.annotation_adapter
                        .queue_next_annotation_id(MarkupId::new(&plan.annotation_id)?);
                }
                NativeTransformStage::Move | NativeTransformStage::EastResize => {
                    self.annotation_adapter.set_tool(AnnotationTool::Select)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error.to_string(), cx);
            return;
        }
        let history_before = self
            .document()
            .map(|document| self.annotation_adapter.history_depths(document.id).0)
            .unwrap_or_default();
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::AwaitingNativeTransformSurface {
                stage,
                history_before,
                progress,
            };
        }
        cx.notify();
    }

    fn prepare_native_v5_property(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("native v5 property edit requires an open document", cx);
            return;
        };
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.native_editing_v5.as_ref())
            .map(|state| state.plan.property_edit.clone())
        else {
            self.fail_comparison_scenario("native v5 property plan is missing", cx);
            return;
        };
        let history_before = match self
            .annotation_adapter
            .prepare_native_v5_property(document_id, &plan)
        {
            Ok(history) => history,
            Err(error) => {
                self.fail_comparison_scenario(error.to_string(), cx);
                return;
            }
        };
        self.editor_overlay_document_id = Some(document_id);
        self.annotation_adapter
            .set_tool(AnnotationTool::Select)
            .expect("select is always a valid annotation tool");
        *self
            .annotation_overlay_paint
            .lock()
            .expect("annotation presentation marker lock") = None;
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.native_editing_v5.as_mut())
        {
            state.stage = NativeEditingV5Stage::PropertyTrigger;
            state.history_before = history_before;
            state.native_ready_emitted = false;
        }
        cx.notify();
    }

    fn prepare_native_v5_snap(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("native v5 snap transform requires an open document", cx);
            return;
        };
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.native_editing_v5.as_ref())
            .map(|state| state.plan.snap_transform.clone())
        else {
            self.fail_comparison_scenario("native v5 snap plan is missing", cx);
            return;
        };
        let history_before = match self
            .annotation_adapter
            .prepare_native_v5_snap(document_id, &plan)
        {
            Ok(history) => history,
            Err(error) => {
                self.fail_comparison_scenario(error.to_string(), cx);
                return;
            }
        };
        self.editor_overlay_document_id = Some(document_id);
        self.annotation_adapter
            .set_tool(AnnotationTool::Select)
            .expect("select is always a valid annotation tool");
        *self
            .annotation_overlay_paint
            .lock()
            .expect("annotation presentation marker lock") = None;
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::AwaitingNativeV5SnapSurface;
            if let Some(state) = scenario.native_editing_v5.as_mut() {
                state.stage = NativeEditingV5Stage::SnapAwaitingSurface;
                state.history_before = history_before;
                state.snap_sample_timestamps_ms.clear();
                state.snap_first_sample_point = None;
                state.snap_last_sample_point = None;
                state.snap_pointer_up_t_ms = None;
                state.snap_guide_active = false;
                state.snap_guide_painted = false;
                state.snap_target_acquired_count = 0;
                state.snap_resolution = None;
                state.native_ready_emitted = false;
            }
        }
        cx.notify();
    }

    fn apply_native_v5_property(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("native v5 property document is missing", cx);
            return;
        };
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.native_editing_v5.as_ref())
            .filter(|state| state.stage == NativeEditingV5Stage::PropertyChoice)
            .map(|state| state.plan.property_edit.clone())
        else {
            return;
        };
        let receipt = match self
            .annotation_adapter
            .commit_native_v5_property(document_id, &plan)
        {
            Ok(receipt) => receipt,
            Err(error) => {
                self.fail_comparison_scenario(error.to_string(), cx);
                return;
            }
        };
        *self
            .annotation_overlay_paint
            .lock()
            .expect("annotation presentation marker lock") = None;
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.native_editing_v5.as_mut())
        {
            state.property_commit = Some(receipt);
            state.stage = NativeEditingV5Stage::PropertyAwaitingPaint;
            state.native_ready_emitted = false;
        }
        cx.notify();
    }

    fn announce_native_input_surface(
        &mut self,
        document_id: u64,
        page: usize,
        bounds: Bounds<Pixels>,
        page_height_points: f32,
        zoom_percent: f32,
        window: &Window,
    ) {
        if page != 1 || self.document().map(|document| document.id) != Some(document_id) {
            return;
        }
        let phase = self
            .perf_scenario
            .as_ref()
            .map(|scenario| scenario.comparison_phase)
            .unwrap_or(ComparisonPhase::Idle);
        let command_id = match phase {
            ComparisonPhase::AwaitingNativeInputSurface { stage, .. } => match stage {
                NativeAnnotationStage::Rectangle => "rectangle:create-sparse",
                NativeAnnotationStage::Highlight => "highlight:create",
            },
            ComparisonPhase::AwaitingNativeScrollSurface => {
                if self
                    .perf_scenario
                    .as_ref()
                    .is_some_and(|scenario| scenario.kind == PerfScenarioKind::DynamicFidelity)
                {
                    DYNAMIC_FIDELITY_COMMAND_ID
                } else {
                    "viewer:continuous-scroll"
                }
            }
            ComparisonPhase::AwaitingNativeEditorInput { stage, .. } => match stage {
                NativeEditorStage::Text => "text:create",
                NativeEditorStage::Length => "length:create",
                NativeEditorStage::Image => "image:create",
                NativeEditorStage::Scale => return,
            },
            ComparisonPhase::AwaitingNativeTransformSurface { stage, .. } => match stage {
                NativeTransformStage::PrerequisiteCreate => "rectangle:create-sparse",
                NativeTransformStage::Move | NativeTransformStage::EastResize => {
                    "rectangle:select-move-resize"
                }
            },
            ComparisonPhase::AwaitingNativeV5SnapSurface => {
                "annotation:native-snap-transform-120hz"
            }
            _ => return,
        };
        #[cfg(feature = "benchmark-evidence")]
        if let Some(scenario) = self.perf_scenario.as_mut()
            && scenario.comparison_input_latency_samples_before.is_none()
        {
            let samples_before = window.input_latency_snapshot().latency_histogram.len();
            scenario.comparison_input_latency_samples_before = Some(samples_before);
            if matches!(phase, ComparisonPhase::AwaitingNativeV5SnapSurface)
                && let Some(state) = scenario.native_editing_v5.as_mut()
            {
                state.input_latency_samples_before = Some(samples_before);
            }
        }
        let viewport = window.viewport_size();
        let transform_stage = match phase {
            ComparisonPhase::AwaitingNativeTransformSurface { stage, .. } => Some(stage),
            _ => None,
        };
        let handle_point = if transform_stage == Some(NativeTransformStage::EastResize) {
            let Some(plan) = Self::native_transform_plan().ok() else {
                return;
            };
            let Some(rectangle) = self
                .annotation_adapter
                .document_scene(document_id, 0)
                .rectangles
                .into_iter()
                .find(|rectangle| rectangle.id.as_str() == plan.annotation_id)
            else {
                return;
            };
            json!({
                "x": rectangle.rect.x + rectangle.rect.width,
                "y": rectangle.rect.y + rectangle.rect.height / 2.0,
            })
        } else {
            Value::Null
        };
        let dynamic_fidelity = command_id == DYNAMIC_FIDELITY_COMMAND_ID;
        let scroll_offset = self.continuous_scroll.offset();
        let initial_scroll_x = (-f32::from(scroll_offset.x)).max(0.0);
        let initial_scroll_y = (-f32::from(scroll_offset.y)).max(0.0);
        let viewport_bounds = ViewerRect::new(
            RAIL_WIDTH + SIDEBAR_WIDTH,
            WINDOW_TITLE_BAR_HEIGHT
                + MENU_BAR_HEIGHT
                + DOCUMENT_TAB_BAR_HEIGHT
                + PRIMARY_BAND_HEIGHT,
            (f32::from(viewport.width) - RAIL_WIDTH - SIDEBAR_WIDTH - RIGHT_RAIL_WIDTH).max(1.0),
            (f32::from(viewport.height)
                - WINDOW_TITLE_BAR_HEIGHT
                - MENU_BAR_HEIGHT
                - DOCUMENT_TAB_BAR_HEIGHT
                - PRIMARY_BAND_HEIGHT)
                .max(1.0),
        );
        let checkpoint_page_geometries = if dynamic_fidelity {
            let geometries = [1, 15, 29]
                .into_iter()
                .filter_map(|checkpoint_page| {
                    let layout = self
                        .continuous_page_layouts
                        .iter()
                        .find(|layout| layout.page == checkpoint_page)?;
                    let (page_width_points, page_height_points) =
                        self.document()?.page_size(checkpoint_page);
                    Some(json!({
                        "page_number": checkpoint_page,
                        "page_size_points": {
                            "width": page_width_points,
                            "height": page_height_points,
                        },
                        "painted_outer_page_bounds_at_initial_scroll_window_logical": {
                            "x": viewport_bounds.x + layout.logical_rect.x - initial_scroll_x,
                            "y": viewport_bounds.y + layout.logical_rect.y - initial_scroll_y,
                            "width": layout.logical_rect.width,
                            "height": layout.logical_rect.height,
                        },
                    }))
                })
                .collect::<Vec<_>>();
            if geometries.len() != 3 {
                return;
            }
            Some(geometries)
        } else {
            None
        };
        perf::emit(
            "native-input-ready",
            perf::fields([
                ("command_id", json!(command_id)),
                (
                    "stage",
                    match transform_stage {
                        Some(NativeTransformStage::PrerequisiteCreate) => {
                            json!("prerequisite-create")
                        }
                        Some(NativeTransformStage::Move) => json!("move"),
                        Some(NativeTransformStage::EastResize) => json!("east-resize"),
                        None => Value::Null,
                    },
                ),
                ("handle_point", handle_point),
                (
                    "checkpoint_page_geometries",
                    checkpoint_page_geometries.map_or(Value::Null, Value::Array),
                ),
                (
                    "initial_scroll_offset_css_px",
                    dynamic_fidelity
                        .then(|| json!({ "x": initial_scroll_x, "y": initial_scroll_y }))
                        .unwrap_or(Value::Null),
                ),
                (
                    "viewport_bounds_window_logical",
                    dynamic_fidelity
                        .then(|| {
                            json!({
                                "x": viewport_bounds.x,
                                "y": viewport_bounds.y,
                                "width": viewport_bounds.width,
                                "height": viewport_bounds.height,
                            })
                        })
                        .unwrap_or(Value::Null),
                ),
                (
                    "zoom_percent",
                    dynamic_fidelity
                        .then(|| json!(self.zoom_percent))
                        .unwrap_or(Value::Null),
                ),
                (
                    "display_scale_factor",
                    dynamic_fidelity
                        .then(|| json!(self.display_scale_factor))
                        .unwrap_or(Value::Null),
                ),
                ("calibrated_wheel_delta_css_px", Value::Null),
                ("wheel_calibration_required", json!(dynamic_fidelity)),
                (
                    "preloaded_asset_id",
                    if command_id == "image:create" {
                        json!("bp-image-checker-v1")
                    } else {
                        Value::Null
                    },
                ),
                (
                    "surface",
                    json!({
                        "page": page,
                        "window_logical_size": {
                            "width": f32::from(viewport.width),
                            "height": f32::from(viewport.height),
                        },
                        "bounds": {
                            "x": f32::from(bounds.origin.x),
                            "y": f32::from(bounds.origin.y),
                            "width": f32::from(bounds.size.width),
                            "height": f32::from(bounds.size.height),
                        },
                        "page_height_points": page_height_points,
                        "pixels_per_point": zoom_percent / 100.0,
                    }),
                ),
            ]),
        );
        let native_scroll_start_offset_y = f32::from(self.continuous_scroll.offset().y);
        let native_scroll_viewport_height = (f32::from(viewport.height)
            - WINDOW_TITLE_BAR_HEIGHT
            - MENU_BAR_HEIGHT
            - DOCUMENT_TAB_BAR_HEIGHT
            - PRIMARY_BAND_HEIGHT)
            .max(1.0);
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = match phase {
                ComparisonPhase::AwaitingNativeInputSurface {
                    stage,
                    history_before,
                } => ComparisonPhase::NativeAnnotationInput {
                    stage,
                    coordinate_samples: 0,
                    history_before,
                },
                ComparisonPhase::AwaitingNativeScrollSurface => {
                    if dynamic_fidelity {
                        ComparisonPhase::AwaitingNativeWheelCalibration {
                            start_offset_y: native_scroll_start_offset_y,
                            viewport_height: native_scroll_viewport_height,
                        }
                    } else {
                        ComparisonPhase::NativeScrollInput {
                            forward_events: 0,
                            reverse_events: 0,
                            first_direction: None,
                            wheel_unit_delta: None,
                            last_forward_ms: None,
                            first_reverse_ms: None,
                            start_offset_y: native_scroll_start_offset_y,
                            peak_distance: 0.0,
                            viewport_height: native_scroll_viewport_height,
                            raster_observations: 0,
                            missing_raster_observations: 0,
                            max_visible_pages: 0,
                        }
                    }
                }
                ComparisonPhase::AwaitingNativeEditorInput {
                    stage,
                    history_before,
                } => ComparisonPhase::NativeEditorInput {
                    stage,
                    coordinate_samples: 0,
                    history_before,
                },
                ComparisonPhase::AwaitingNativeTransformSurface {
                    stage,
                    history_before,
                    progress,
                } => ComparisonPhase::NativeTransformInput {
                    stage,
                    coordinate_samples: 0,
                    history_before,
                    progress,
                    pixels_per_point: f64::from(zoom_percent) / 100.0,
                },
                ComparisonPhase::AwaitingNativeV5SnapSurface => {
                    if let Some(state) = scenario.native_editing_v5.as_mut() {
                        state.stage = NativeEditingV5Stage::SnapInput;
                    }
                    ComparisonPhase::NativeV5SnapInput {
                        pixels_per_point: f64::from(zoom_percent) / 100.0,
                    }
                }
                _ => phase,
            };
        }
    }

    fn record_native_annotation_coordinate(&mut self) {
        if let Some(scenario) = self.perf_scenario.as_mut()
            && let ComparisonPhase::NativeAnnotationInput {
                stage,
                coordinate_samples,
                history_before,
            } = scenario.comparison_phase
        {
            scenario.comparison_phase = ComparisonPhase::NativeAnnotationInput {
                stage,
                coordinate_samples: coordinate_samples.saturating_add(1),
                history_before,
            };
        } else if let Some(scenario) = self.perf_scenario.as_mut()
            && let ComparisonPhase::NativeEditorInput {
                stage,
                coordinate_samples,
                history_before,
            } = scenario.comparison_phase
        {
            scenario.comparison_phase = ComparisonPhase::NativeEditorInput {
                stage,
                coordinate_samples: coordinate_samples.saturating_add(1),
                history_before,
            };
        } else if let Some(scenario) = self.perf_scenario.as_mut()
            && let ComparisonPhase::NativeTransformInput {
                stage,
                coordinate_samples,
                history_before,
                progress,
                pixels_per_point,
            } = scenario.comparison_phase
        {
            scenario.comparison_phase = ComparisonPhase::NativeTransformInput {
                stage,
                coordinate_samples: coordinate_samples.saturating_add(1),
                history_before,
                progress,
                pixels_per_point,
            };
        }
    }

    fn finish_native_annotation_input(&mut self, cx: &mut Context<Self>) {
        let Some((stage, coordinate_samples, history_before, document_id, plan)) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| match scenario.comparison_phase {
                ComparisonPhase::NativeAnnotationInput {
                    stage,
                    coordinate_samples,
                    history_before,
                } => Some((stage, coordinate_samples, history_before)),
                _ => None,
            })
            .and_then(|(stage, samples, history_before)| {
                Some((
                    stage,
                    samples,
                    history_before,
                    self.document()?.id,
                    self.perf_scenario
                        .as_ref()?
                        .comparison_plan
                        .as_ref()?
                        .annotation_create()?
                        .clone(),
                ))
            })
        else {
            return;
        };
        let (command_id, expected_samples) = match stage {
            NativeAnnotationStage::Rectangle => (
                plan.rectangle.command_id.as_str(),
                plan.rectangle.sample_count,
            ),
            NativeAnnotationStage::Highlight => (
                plan.highlight.command_id.as_str(),
                plan.highlight.sample_count,
            ),
        };
        // XTest submits every manifest coordinate, but X11 and GPUI may
        // coalesce consecutive integer-pixel MotionNotify events. Require an
        // observed drag through the normal handlers here; the external runner
        // independently proves the exact submitted sample count and timing.
        if coordinate_samples < 2 {
            self.fail_comparison_scenario(
                format!(
                    "{command_id} received only {coordinate_samples} native coordinate samples"
                ),
                cx,
            );
            return;
        }
        perf::emit(
            "comparison-input-observed",
            perf::fields([
                ("command_id", json!(command_id)),
                ("observed_coordinate_samples", json!(coordinate_samples)),
                ("submitted_coordinate_samples", json!(expected_samples)),
                ("delivery_may_coalesce", json!(true)),
            ]),
        );
        if self.annotation_adapter.history_depths(document_id).0 != history_before + 1 {
            self.fail_comparison_scenario(
                format!("{command_id} did not create exactly one history entry"),
                cx,
            );
            return;
        }
        let scene = self.annotation_adapter.document_scene(document_id, 0);
        let committed_highlight = scene.pens.iter().find(|annotation| {
            annotation.id.as_str() == plan.highlight.annotation_id
                && annotation.tool == InkTool::Highlight
                && annotation.blend_mode == BlendMode::Multiply
                && !annotation.draft
        });
        let committed = match stage {
            NativeAnnotationStage::Rectangle => scene
                .rectangles
                .iter()
                .any(|annotation| annotation.id.as_str() == plan.rectangle.annotation_id),
            NativeAnnotationStage::Highlight => committed_highlight.is_some(),
        };
        if !committed {
            self.fail_comparison_scenario(
                format!("{command_id} did not commit the expected typed annotation"),
                cx,
            );
            return;
        }
        let result = (|| -> Result<(), String> {
            self.record_comparison_milestone(command_id, "pointer-stream-received")?;
            if stage == NativeAnnotationStage::Highlight {
                let observed = committed_highlight
                    .expect("committed highlight was checked")
                    .points
                    .iter()
                    .map(|point| [point.x, point.y])
                    .collect::<Vec<_>>();
                let geometry = compare_highlight_geometry(
                    &plan.highlight.samples(),
                    &observed,
                    f64::from(self.zoom_percent) / 100.0,
                )
                .map_err(|error| error.to_string())?;
                perf::emit(
                    "comparison-highlight-geometry",
                    perf::fields([
                        ("command_id", json!(command_id)),
                        ("matched", json!(geometry.matched)),
                        (
                            "expected_input_point_count",
                            json!(geometry.expected_input_point_count),
                        ),
                        (
                            "observed_model_point_count",
                            json!(geometry.observed_model_point_count),
                        ),
                        (
                            "canonical_resample_count",
                            json!(geometry.canonical_resample_count),
                        ),
                        (
                            "maximum_centerline_deviation_pdf_points",
                            json!(geometry.maximum_centerline_deviation_pdf_points),
                        ),
                        ("tolerance_pdf_points", json!(geometry.tolerance_pdf_points)),
                        (
                            "smoothing_tolerance_pdf_points",
                            json!(geometry.smoothing_tolerance_pdf_points),
                        ),
                        (
                            "coordinate_quantization_allowance_pdf_points",
                            json!(geometry.coordinate_quantization_allowance_pdf_points),
                        ),
                        ("contract_version", json!(geometry.contract_version)),
                        ("canonicalization", json!(geometry.canonicalization)),
                    ]),
                );
                if !geometry.matched {
                    return Err(format!(
                        "{command_id} centerline deviation {} exceeded tolerance {} PDF points",
                        geometry.maximum_centerline_deviation_pdf_points,
                        geometry.tolerance_pdf_points,
                    ));
                }
                self.record_comparison_milestone(command_id, "path-smoothed")?;
            }
            self.record_comparison_milestone(command_id, "gesture-committed-once")?;
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error, cx);
            return;
        }
        match stage {
            NativeAnnotationStage::Rectangle => {
                self.prepare_native_annotation_input(NativeAnnotationStage::Highlight, cx)
            }
            NativeAnnotationStage::Highlight => {
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::AwaitingAnnotationPaint;
                }
                cx.notify();
            }
        }
    }

    fn finish_native_transform_input(&mut self, cx: &mut Context<Self>) {
        let Some((
            stage,
            coordinate_samples,
            history_before,
            mut progress,
            pixels_per_point,
            document_id,
        )) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| match scenario.comparison_phase {
                ComparisonPhase::NativeTransformInput {
                    stage,
                    coordinate_samples,
                    history_before,
                    progress,
                    pixels_per_point,
                } => Some((
                    stage,
                    coordinate_samples,
                    history_before,
                    progress,
                    pixels_per_point,
                )),
                _ => None,
            })
            .and_then(
                |(stage, coordinate_samples, history_before, progress, pixels_per_point)| {
                    Some((
                        stage,
                        coordinate_samples,
                        history_before,
                        progress,
                        pixels_per_point,
                        self.document()?.id,
                    ))
                },
            )
        else {
            return;
        };
        let plan = match Self::native_transform_plan() {
            Ok(plan) => plan,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        let (stage_name, submitted_samples) = match stage {
            NativeTransformStage::PrerequisiteCreate => {
                ("prerequisite-create", plan.create_sample_count)
            }
            NativeTransformStage::Move => ("move", plan.move_sample_count),
            NativeTransformStage::EastResize => ("east-resize", plan.resize_sample_count),
        };
        if coordinate_samples < 2 {
            self.fail_comparison_scenario(
                format!(
                    "{} stage {stage_name} received only {coordinate_samples} native coordinate samples",
                    plan.command_id,
                ),
                cx,
            );
            return;
        }
        let history_after = self.annotation_adapter.history_depths(document_id).0;
        let history_delta = history_after.saturating_sub(history_before);
        if history_delta != 1 {
            self.fail_comparison_scenario(
                format!(
                    "{} stage {stage_name} committed {history_delta} history transactions instead of one",
                    plan.command_id,
                ),
                cx,
            );
            return;
        }
        let scene = self.annotation_adapter.document_scene(document_id, 0);
        let Some(rectangle) = scene
            .rectangles
            .iter()
            .find(|rectangle| rectangle.id.as_str() == plan.annotation_id)
        else {
            self.fail_comparison_scenario("native transform rectangle is missing", cx);
            return;
        };
        perf::emit(
            "comparison-input-observed",
            perf::fields([
                ("command_id", json!(&plan.command_id)),
                ("stage", json!(stage_name)),
                ("observed_coordinate_samples", json!(coordinate_samples)),
                ("submitted_coordinate_samples", json!(submitted_samples)),
                ("delivery_may_coalesce", json!(true)),
                ("history_delta", json!(history_delta)),
            ]),
        );
        match stage {
            NativeTransformStage::PrerequisiteCreate => {
                progress.create_history_delta = history_delta;
                self.prepare_native_transform_input(NativeTransformStage::Move, progress, cx);
            }
            NativeTransformStage::Move => {
                if !rectangle.selected {
                    self.fail_comparison_scenario(
                        "decision-3 no-fill edge/body point did not select the rectangle",
                        cx,
                    );
                    return;
                }
                progress.hit_test_selected = true;
                progress.move_history_delta = history_delta;
                self.prepare_native_transform_input(NativeTransformStage::EastResize, progress, cx);
            }
            NativeTransformStage::EastResize => {
                if !rectangle.selected {
                    self.fail_comparison_scenario(
                        "east resize completed without the rectangle selected",
                        cx,
                    );
                    return;
                }
                progress.resize_history_delta = history_delta;
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::AwaitingNativeTransformPaint {
                        scene_revision: scene.revision,
                        progress,
                        observed_final_rect: [
                            rectangle.rect.x,
                            rectangle.rect.y,
                            rectangle.rect.width,
                            rectangle.rect.height,
                        ],
                        pixels_per_point,
                    };
                }
                cx.notify();
            }
        }
    }

    fn advance_native_transform_paint(
        &mut self,
        scene_revision: u64,
        progress: NativeTransformProgress,
        observed_final_rect: [f64; 4],
        pixels_per_point: f64,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("native transform document closed before draw", cx);
            return;
        };
        let plan = match Self::native_transform_plan() {
            Ok(plan) => plan,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        let paint = self
            .annotation_overlay_paint
            .lock()
            .expect("annotation presentation marker lock")
            .clone();
        let Some(paint) = paint.filter(|paint| {
            paint.document_id == document_id
                && paint.page_index == 0
                && paint.scene_revision == scene_revision
                && paint
                    .rectangle_ids
                    .iter()
                    .any(|id| id == &plan.annotation_id)
        }) else {
            cx.notify();
            return;
        };
        let evidence = match plan.assess_native_observation(NativeRectangleTransformObservation {
            hit_test_selected: progress.hit_test_selected,
            create_history_delta: progress.create_history_delta,
            move_history_delta: progress.move_history_delta,
            resize_history_delta: progress.resize_history_delta,
            observed_final_rect,
            pixels_per_point,
            gpui_platform_draw_submitted: true,
        }) {
            Ok(evidence) => evidence,
            Err(error) => {
                self.fail_comparison_scenario(error.to_string(), cx);
                return;
            }
        };
        perf::emit(
            "comparison-native-transform-evidence",
            perf::fields([
                ("command_id", json!(&plan.command_id)),
                ("select_semantics", json!(evidence.select_semantics)),
                ("hit_test_selected", json!(progress.hit_test_selected)),
                ("create_history_delta", json!(progress.create_history_delta)),
                ("move_history_delta", json!(progress.move_history_delta)),
                ("resize_history_delta", json!(progress.resize_history_delta)),
                ("observed_final_rect", json!(observed_final_rect)),
                ("expected_final_rect", json!(evidence.expected_final_rect)),
                ("pixels_per_point", json!(pixels_per_point)),
                (
                    "maximum_geometry_error_device_px",
                    json!(evidence.maximum_geometry_error_device_px),
                ),
                (
                    "geometry_tolerance_device_px",
                    json!(evidence.geometry_tolerance_device_px),
                ),
                ("scene_revision", json!(paint.scene_revision)),
                ("gpui_platform_draw_submitted", json!(true)),
                ("physical_scanout_observed", json!(false)),
                ("decision_timing_eligible", json!(false)),
            ]),
        );
        for milestone in &plan.expected_milestones {
            if let Err(error) = self.record_comparison_milestone(&plan.command_id, milestone) {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        }
        self.complete_comparison_if_ready(cx);
    }

    fn finish_native_editor_input(
        &mut self,
        native_text_entry_observed: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((stage, coordinate_samples, history_before, document_id)) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| match scenario.comparison_phase {
                ComparisonPhase::NativeEditorInput {
                    stage,
                    coordinate_samples,
                    history_before,
                } => Some((stage, coordinate_samples, history_before)),
                _ => None,
            })
            .and_then(|(stage, coordinate_samples, history_before)| {
                Some((
                    stage,
                    coordinate_samples,
                    history_before,
                    self.document()?.id,
                ))
            })
        else {
            return;
        };
        let (command_id, minimum_samples) = match stage {
            NativeEditorStage::Text => ("text:create", 1),
            NativeEditorStage::Length => ("length:create", 2),
            NativeEditorStage::Image => ("image:create", 1),
            NativeEditorStage::Scale => return,
        };
        if stage == NativeEditorStage::Text && !native_text_entry_observed {
            self.fail_comparison_scenario(
                "text:create cannot finish without an exact native key-entry receipt",
                cx,
            );
            return;
        }
        if coordinate_samples < minimum_samples {
            self.fail_comparison_scenario(
                format!(
                    "{command_id} received only {coordinate_samples} native coordinate samples"
                ),
                cx,
            );
            return;
        }
        if self.annotation_adapter.history_depths(document_id).0 != history_before + 1 {
            self.fail_comparison_scenario(
                format!("{command_id} did not create exactly one history entry"),
                cx,
            );
            return;
        }
        let scene = self.annotation_adapter.document_scene(document_id, 0);
        let committed = match stage {
            NativeEditorStage::Text => scene.text_boxes.iter().any(|annotation| {
                annotation.id.as_str() == TEXT_CREATE_ID && annotation.content == FROZEN_TEXT_CREATE
            }),
            NativeEditorStage::Length => scene.lengths.iter().any(|annotation| {
                annotation.id.as_str() == LENGTH_CREATE_ID
                    && annotation.start.x == 90.0
                    && annotation.start.y == 510.0
                    && annotation.end.x == 306.0
                    && annotation.end.y == 510.0
                    && annotation.caption == "3.00 m"
            }),
            NativeEditorStage::Image => scene
                .images
                .iter()
                .any(|annotation| annotation.id.as_str() == IMAGE_CREATE_ID),
            NativeEditorStage::Scale => false,
        };
        if !committed {
            let observed = match stage {
                NativeEditorStage::Length => scene
                    .lengths
                    .iter()
                    .map(|annotation| {
                        format!(
                            "{}:({:.3},{:.3})->({:.3},{:.3}) {}",
                            annotation.id.as_str(),
                            annotation.start.x,
                            annotation.start.y,
                            annotation.end.x,
                            annotation.end.y,
                            annotation.caption,
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                NativeEditorStage::Text => scene
                    .text_boxes
                    .iter()
                    .map(|annotation| {
                        format!("{}:{:?}", annotation.id.as_str(), annotation.content)
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
                NativeEditorStage::Image => scene
                    .images
                    .iter()
                    .map(|annotation| annotation.id.as_str().to_owned())
                    .collect::<Vec<_>>()
                    .join(", "),
                NativeEditorStage::Scale => String::new(),
            };
            self.fail_comparison_scenario(
                format!(
                    "{command_id} did not commit the expected typed annotation; observed [{observed}]"
                ),
                cx,
            );
            return;
        }
        if let Err(error) = self.record_comparison_milestone(command_id, "gesture-committed-once") {
            self.fail_comparison_scenario(error, cx);
            return;
        }
        if stage == NativeEditorStage::Text
            && let Err(error) = self.record_comparison_milestone(command_id, "text-input-committed")
        {
            self.fail_comparison_scenario(error, cx);
            return;
        }
        perf::emit(
            "comparison-native-input-evidence",
            perf::fields([
                ("command_id", json!(command_id)),
                ("input_api", json!("XTEST")),
                ("page_gesture_observed", json!(true)),
                ("observed_coordinate_samples", json!(coordinate_samples)),
                (
                    "native_text_entry_observed",
                    json!(native_text_entry_observed),
                ),
                ("document_content_prepopulated", json!(false)),
                ("native_asset_selection_observed", json!(false)),
                ("gpu_present_observed", json!(false)),
                ("gpu_upload_bytes", Value::Null),
                ("decision_timing_eligible", json!(false)),
            ]),
        );
        match stage {
            NativeEditorStage::Text => {
                self.prepare_native_editor_input(NativeEditorStage::Scale, cx)
            }
            NativeEditorStage::Length => {
                self.prepare_native_editor_input(NativeEditorStage::Image, cx)
            }
            NativeEditorStage::Image => {
                let semantic = match build_representative_semantic_report(
                    document_id,
                    &scene,
                    &self.annotation_adapter,
                ) {
                    Ok(report) => report,
                    Err(error) => {
                        self.fail_comparison_scenario(error, cx);
                        return;
                    }
                };
                for evidence in &semantic.commands {
                    perf::emit(
                        "comparison-command-evidence",
                        perf::fields([
                            ("command_id", json!(evidence.command_id)),
                            ("evidence_scope", json!("native-input-plus-domain-semantic")),
                            (
                                "all_manifest_milestones_proven",
                                json!(evidence.blocked_manifest_milestones.is_empty()),
                            ),
                            ("decision_timing_eligible", json!(false)),
                            ("evidence", json!(evidence)),
                        ]),
                    );
                }
                perf::emit(
                    "editor-create-semantic-evidence",
                    perf::fields([("report", json!(semantic))]),
                );
                *self
                    .annotation_overlay_paint
                    .lock()
                    .expect("annotation presentation marker lock") = None;
                if let Some(perf) = self.perf_scenario.as_mut() {
                    perf.comparison_phase = ComparisonPhase::AwaitingEditorCreatePaint;
                }
                cx.notify();
            }
            NativeEditorStage::Scale => {}
        }
    }

    fn apply_native_editor_scale(&mut self, cx: &mut Context<Self>) {
        let ready = self.perf_scenario.as_ref().is_some_and(|scenario| {
            matches!(
                scenario.comparison_phase,
                ComparisonPhase::NativeEditorInput {
                    stage: NativeEditorStage::Scale,
                    ..
                }
            )
        });
        if !ready {
            return;
        }
        let calibration =
            match butter_paper_gpui_gallery::annotation_model::LengthCalibration::from_scale(
                72.0, 1.0, "m", 2, true,
            ) {
                Ok(calibration) => calibration,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
        if let Err(error) = self.annotation_adapter.set_length_calibration(calibration) {
            self.fail_comparison_scenario(error.to_string(), cx);
            return;
        }
        if let Err(error) =
            self.record_comparison_milestone("length:set-scale", "measurement-scale-current")
        {
            self.fail_comparison_scenario(error, cx);
            return;
        }
        perf::emit(
            "comparison-native-input-evidence",
            perf::fields([
                ("command_id", json!("length:set-scale")),
                ("input_api", json!("XTEST")),
                ("control_id", json!("comparison-length-scale")),
                ("control_click_observed", json!(true)),
                ("gpu_present_observed", json!(false)),
                ("gpu_upload_bytes", Value::Null),
                ("decision_timing_eligible", json!(false)),
            ]),
        );
        self.prepare_native_editor_input(NativeEditorStage::Length, cx);
    }

    fn start_annotation_create_comparison(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("annotation-create requires an open document", cx);
            return;
        };
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::annotation_create)
            .cloned()
        else {
            self.fail_comparison_scenario("annotation-create plan is unavailable", cx);
            return;
        };
        let before_history = self.annotation_adapter.history_depths(document_id).0;
        let result = (|| -> Result<(), String> {
            let rectangle_samples = plan.rectangle.samples();
            self.annotation_adapter
                .set_tool(AnnotationTool::Rectangle)
                .map_err(|error| error.to_string())?;
            self.annotation_adapter.queue_next_annotation_id(
                MarkupId::new(&plan.rectangle.annotation_id).map_err(|error| error.to_string())?,
            );
            let first = rectangle_samples
                .first()
                .copied()
                .ok_or_else(|| "rectangle pointer stream is empty".to_string())?;
            let last = rectangle_samples
                .last()
                .copied()
                .ok_or_else(|| "rectangle pointer stream is empty".to_string())?;
            self.annotation_adapter
                .pointer_down(
                    document_id,
                    0,
                    10_001,
                    PdfPoint::new(first[0], first[1]).map_err(|error| error.to_string())?,
                    4.0,
                )
                .map_err(|error| error.to_string())?;
            for sample in &rectangle_samples[1..rectangle_samples.len() - 1] {
                self.annotation_adapter
                    .pointer_move(
                        10_001,
                        PdfPoint::new(sample[0], sample[1]).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
            }
            self.annotation_adapter
                .pointer_up(
                    10_001,
                    PdfPoint::new(last[0], last[1]).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            if rectangle_samples.len() != plan.rectangle.sample_count {
                return Err("rectangle pointer sample count drifted".into());
            }
            if self.annotation_adapter.history_depths(document_id).0 != before_history + 1 {
                return Err("rectangle gesture did not create exactly one history entry".into());
            }
            self.record_comparison_milestone(
                &plan.rectangle.command_id,
                "pointer-stream-received",
            )?;
            self.record_comparison_milestone(&plan.rectangle.command_id, "gesture-committed-once")?;

            let highlight_samples = plan.highlight.samples();
            self.annotation_adapter
                .set_tool(AnnotationTool::Highlight)
                .map_err(|error| error.to_string())?;
            self.annotation_adapter.queue_next_annotation_id(
                MarkupId::new(&plan.highlight.annotation_id).map_err(|error| error.to_string())?,
            );
            let first = highlight_samples
                .first()
                .copied()
                .ok_or_else(|| "highlight pointer stream is empty".to_string())?;
            let last = highlight_samples
                .last()
                .copied()
                .ok_or_else(|| "highlight pointer stream is empty".to_string())?;
            self.annotation_adapter
                .pointer_down(
                    document_id,
                    0,
                    10_002,
                    PdfPoint::new(first[0], first[1]).map_err(|error| error.to_string())?,
                    4.0,
                )
                .map_err(|error| error.to_string())?;
            for sample in &highlight_samples[1..highlight_samples.len() - 1] {
                self.annotation_adapter
                    .pointer_move(
                        10_002,
                        PdfPoint::new(sample[0], sample[1]).map_err(|error| error.to_string())?,
                    )
                    .map_err(|error| error.to_string())?;
            }
            self.annotation_adapter
                .pointer_up(
                    10_002,
                    PdfPoint::new(last[0], last[1]).map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
            if highlight_samples.len() != plan.highlight.sample_count {
                return Err("highlight pointer sample count drifted".into());
            }
            if self.annotation_adapter.history_depths(document_id).0 != before_history + 2 {
                return Err("highlight gesture did not create exactly one history entry".into());
            }
            let highlight = self
                .annotation_adapter
                .document_scene(document_id, 0)
                .pens
                .into_iter()
                .find(|pen| pen.id.as_str() == plan.highlight.annotation_id)
                .ok_or_else(|| "committed highlight is missing".to_string())?;
            if highlight.tool != InkTool::Highlight
                || highlight.blend_mode != BlendMode::Multiply
                || highlight.draft
            {
                return Err("highlight did not keep its typed multiply-blend state".into());
            }
            let observed = highlight
                .points
                .iter()
                .map(|point| [point.x, point.y])
                .collect::<Vec<_>>();
            let geometry = compare_highlight_geometry(
                &highlight_samples,
                &observed,
                f64::from(self.zoom_percent) / 100.0,
            )
            .map_err(|error| error.to_string())?;
            perf::emit(
                "comparison-highlight-geometry",
                perf::fields([
                    ("command_id", json!(&plan.highlight.command_id)),
                    ("matched", json!(geometry.matched)),
                    (
                        "expected_input_point_count",
                        json!(geometry.expected_input_point_count),
                    ),
                    (
                        "observed_model_point_count",
                        json!(geometry.observed_model_point_count),
                    ),
                    (
                        "canonical_resample_count",
                        json!(geometry.canonical_resample_count),
                    ),
                    (
                        "maximum_centerline_deviation_pdf_points",
                        json!(geometry.maximum_centerline_deviation_pdf_points),
                    ),
                    ("tolerance_pdf_points", json!(geometry.tolerance_pdf_points)),
                    (
                        "smoothing_tolerance_pdf_points",
                        json!(geometry.smoothing_tolerance_pdf_points),
                    ),
                    (
                        "coordinate_quantization_allowance_pdf_points",
                        json!(geometry.coordinate_quantization_allowance_pdf_points),
                    ),
                    ("contract_version", json!(geometry.contract_version)),
                    ("canonicalization", json!(geometry.canonicalization)),
                ]),
            );
            if !geometry.matched {
                return Err(format!(
                    "highlight centerline deviation {} exceeded tolerance {} PDF points",
                    geometry.maximum_centerline_deviation_pdf_points, geometry.tolerance_pdf_points,
                ));
            }
            self.record_comparison_milestone(
                &plan.highlight.command_id,
                "pointer-stream-received",
            )?;
            self.record_comparison_milestone(&plan.highlight.command_id, "path-smoothed")?;
            self.record_comparison_milestone(&plan.highlight.command_id, "gesture-committed-once")?;
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error, cx);
            return;
        }
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::AwaitingAnnotationPaint;
        }
        cx.notify();
    }

    fn start_continuous_scroll_comparison(&mut self, cx: &mut Context<Self>) {
        self.scroll_mode = ScrollMode::Continuous;
        self.continuous_scroll.set_offset(point(px(0.0), px(0.0)));
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::Scrolling {
                started_ms: perf::elapsed_ms(),
                last_sample: 0,
                raster_observations: 0,
                missing_raster_observations: 0,
                max_visible_pages: 0,
            };
        }
        perf::emit(
            "operation-started",
            perf::fields([("operation", json!("continuous-scroll"))]),
        );
        cx.notify();
    }

    fn advance_viewer_layout(&mut self, phase: ComparisonPhase, cx: &mut Context<Self>) {
        let Some((document_id, page_count, current_page, zoom)) = self.document().map(|document| {
            (
                document.id,
                document.page_count,
                document.current_page,
                self.zoom_percent / 100.0,
            )
        }) else {
            self.fail_comparison_scenario("viewer-layout requires an open document", cx);
            return;
        };
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::viewer_layout)
            .cloned()
        else {
            self.fail_comparison_scenario("viewer-layout plan is unavailable", cx);
            return;
        };
        let result = (|| -> Result<(), String> {
            match phase {
                ComparisonPhase::ViewerLayoutSingle => {
                    if self.scroll_mode != ScrollMode::SinglePage
                        || !self.continuous_page_layouts.is_empty()
                    {
                        return Err("single-page layout retained continuous page members".into());
                    }
                    let (width, height) = self
                        .document()
                        .expect("document was checked")
                        .page_size(current_page);
                    if width <= 0.0 || height <= 0.0 {
                        return Err("single-page geometry is not positive".into());
                    }
                    self.record_comparison_milestone(
                        &plan.single.command_id,
                        "per-page-geometry-matched",
                    )?;
                    let page_index = u32::try_from(current_page - 1).map_err(|_| {
                        "current page exceeds the annotation page index".to_string()
                    })?;
                    let scene = self
                        .annotation_adapter
                        .document_scene(document_id, page_index);
                    let thumbnail = self
                        .annotation_adapter
                        .thumbnail_scene(document_id, page_index);
                    if scene.revision != thumbnail.revision {
                        return Err("annotation thumbnail revision is stale".into());
                    }
                    self.record_comparison_milestone(
                        &plan.single.command_id,
                        "annotation-thumbnail-current",
                    )?;
                    self.set_scroll_mode(ScrollMode::Continuous, cx);
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.comparison_phase = ComparisonPhase::ViewerLayoutContinuous;
                    }
                }
                ComparisonPhase::ViewerLayoutContinuous => {
                    if self.scroll_mode != ScrollMode::Continuous
                        || self.continuous_page_layouts.len() != page_count
                    {
                        return Err(format!(
                            "continuous layout has {} page members for {page_count} pages",
                            self.continuous_page_layouts.len()
                        ));
                    }
                    for layout in &self.continuous_page_layouts {
                        let (width, height) = self
                            .document()
                            .expect("document was checked")
                            .page_size(layout.page);
                        if (layout.logical_rect.width - width * zoom).abs() > 0.1
                            || (layout.logical_rect.height - height * zoom).abs() > 0.1
                        {
                            return Err(format!("page {} geometry drifted", layout.page));
                        }
                    }
                    self.record_comparison_milestone(
                        &plan.continuous.command_id,
                        "per-page-geometry-matched",
                    )?;
                    if self.continuous_visible_pages.is_empty()
                        || self.continuous_visible_pages.len() > 8
                        || self.continuous_visible_pages.len() >= page_count
                    {
                        return Err("continuous visible page window is not bounded".into());
                    }
                    self.record_comparison_milestone(
                        &plan.continuous.command_id,
                        "virtual-page-window-bounded",
                    )?;
                    self.complete_comparison_if_ready(cx);
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error, cx);
        }
    }

    fn advance_cache_pressure(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::cache_pressure)
            .cloned()
        else {
            self.fail_comparison_scenario("cache-pressure plan is unavailable", cx);
            return;
        };
        let Some((operation_index, tile_cache_insert_bytes, atlas_upload_checkpoint_bytes)) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| match scenario.comparison_phase {
                ComparisonPhase::CachePressure {
                    operation_index,
                    tile_cache_insert_bytes,
                    atlas_upload_checkpoint_bytes,
                } => Some((
                    operation_index,
                    tile_cache_insert_bytes,
                    atlas_upload_checkpoint_bytes,
                )),
                _ => None,
            })
        else {
            return;
        };
        if !self.pending_viewport_requests.is_empty()
            || !self.pending_tile_requests.is_empty()
            || self.active_tile_jobs > 0
        {
            cx.notify();
            return;
        }
        if let Some(checkpoint) = atlas_upload_checkpoint_bytes {
            let current = self
                .tile_atlas_uploads
                .lock()
                .expect("tile atlas upload receipt lock")
                .values()
                .copied()
                .sum::<usize>();
            if current <= checkpoint {
                cx.notify();
                return;
            }
        }
        let total = plan.cycles.saturating_mul(plan.sequence.len());
        if operation_index >= total {
            let decoded_bytes = self
                .document()
                .map(PdfDocument::cached_image_bytes)
                .unwrap_or(usize::MAX);
            let policy = CachePolicy::default();
            if self.tile_cache.bytes() > policy.max_bytes {
                self.fail_comparison_scenario("tile cache exceeded its declared byte limit", cx);
                return;
            }
            if decoded_bytes > 128 * 1_024 * 1_024 {
                self.fail_comparison_scenario("decoded page cache exceeded its byte limit", cx);
                return;
            }
            let atlas_upload_bytes = self
                .tile_atlas_uploads
                .lock()
                .expect("tile atlas upload receipt lock")
                .values()
                .copied()
                .sum::<usize>();
            if atlas_upload_bytes == 0 {
                cx.notify();
                return;
            }
            let result = (|| -> Result<(), String> {
                self.record_comparison_milestone(
                    &plan.command_id,
                    "declared-cache-byte-limit-held",
                )?;
                self.record_comparison_milestone(&plan.command_id, "decoded-byte-limit-held")?;
                if tile_cache_insert_bytes == 0 {
                    return Err(
                        "cache pressure did not record any decoded tile-cache insert bytes".into(),
                    );
                }
                perf::emit(
                    "comparison-tile-cache-insert-bytes",
                    perf::fields([
                        ("bytes", json!(tile_cache_insert_bytes)),
                        ("evidence_kind", json!("cpu-decoded-tile-cache-insertion")),
                        ("gpu_upload_observed", json!(false)),
                    ]),
                );
                perf::emit(
                    "comparison-tile-atlas-upload-bytes",
                    perf::fields([
                        ("bytes", json!(atlas_upload_bytes)),
                        (
                            "evidence_kind",
                            json!("gpui-wgpu-paint-image-atlas-upload-queued"),
                        ),
                        ("physical_bus_upload_bytes", Value::Null),
                    ]),
                );
                self.record_comparison_milestone(&plan.command_id, "upload-byte-count-recorded")?;
                Ok(())
            })();
            if let Err(error) = result {
                self.fail_comparison_scenario(error, cx);
            } else {
                self.complete_comparison_if_ready(cx);
            }
            return;
        }
        let step = plan.sequence[operation_index % plan.sequence.len()].as_str();
        let mut next_atlas_upload_checkpoint = None;
        match step {
            "navigate" => {
                let page_count = self
                    .document()
                    .map(|document| document.page_count)
                    .unwrap_or(1);
                self.set_page(page_count, cx);
            }
            "zoom" => {
                if self
                    .tile_atlas_uploads
                    .lock()
                    .expect("tile atlas upload receipt lock")
                    .is_empty()
                {
                    next_atlas_upload_checkpoint = Some(0);
                }
                self.set_zoom(800.0, ZoomPreset::Manual, cx);
            }
            "pan" => {
                self.document_scroll
                    .set_offset(point(px(-512.0), px(-384.0)));
                self.tile_plan_key = None;
                cx.notify();
            }
            "return-page-1" => {
                self.document_scroll.set_offset(point(px(0.0), px(0.0)));
                self.set_zoom(100.0, ZoomPreset::Manual, cx);
                self.set_page(1, cx);
            }
            other => {
                self.fail_comparison_scenario(format!("unsupported cache step {other}"), cx);
                return;
            }
        }
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::CachePressure {
                operation_index: operation_index + 1,
                tile_cache_insert_bytes,
                atlas_upload_checkpoint_bytes: next_atlas_upload_checkpoint,
            };
        }
    }

    fn start_fit_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((mode, command_id)) = self.perf_scenario.as_ref().and_then(|scenario| {
            let plan = scenario.engineering_fit_plan.as_ref()?;
            Some((
                plan.modes.get(scenario.step_index).copied()?,
                plan.command_id.clone(),
            ))
        }) else {
            self.fail_comparison_scenario("Fit modes plan is exhausted or unavailable", cx);
            return;
        };
        let Some(document) = self.document() else {
            self.fail_comparison_scenario("Fit modes requires an open engineering sheet", cx);
            return;
        };
        let viewport = window.viewport_size();
        let shell_width = f32::from(viewport.width);
        let shell_height = f32::from(viewport.height);
        let available_width = shell_width - RAIL_WIDTH - SIDEBAR_WIDTH - RIGHT_RAIL_WIDTH;
        let available_height = shell_height
            - WINDOW_TITLE_BAR_HEIGHT
            - MENU_BAR_HEIGHT
            - DOCUMENT_TAB_BAR_HEIGHT
            - PRIMARY_BAND_HEIGHT
            - 32.0;
        let expected_zoom_percent = match mode {
            FitMode::FitPage => resolve_fit_page_zoom(
                available_width,
                available_height,
                document.page_width,
                document.page_height,
            ),
            FitMode::FitWidth => resolve_fit_width_zoom(available_width, document.page_width),
        };
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.engineering_fit_shell_size = Some([1_200.0, 800.0, shell_width, shell_height]);
            scenario.active_operation = Some(PerfOperation {
                kind: mode.as_str(),
                value: f64::from(expected_zoom_percent),
                started_ms: perf::elapsed_ms(),
            });
            scenario.comparison_phase = ComparisonPhase::Idle;
        }
        perf::emit(
            "operation-started",
            perf::fields([
                ("operation", json!(mode.as_str())),
                ("command_id", json!(command_id)),
                ("expected_zoom_percent", json!(expected_zoom_percent)),
                ("requested_shell_width", json!(1_200.0)),
                ("requested_shell_height", json!(800.0)),
                ("client_width", json!(shell_width)),
                ("client_height", json!(shell_height)),
            ]),
        );
        match mode {
            FitMode::FitPage => self.fit_page(window, cx),
            FitMode::FitWidth => self.fit_width(window, cx),
        }
    }

    fn advance_engineering_cache_pressure(&mut self, cx: &mut Context<Self>) {
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.engineering_cache_plan.clone())
        else {
            self.fail_comparison_scenario("engineering cache recovery plan is unavailable", cx);
            return;
        };
        let Some((operation_index, tile_cache_insert_bytes, atlas_upload_checkpoint_bytes)) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| match scenario.comparison_phase {
                ComparisonPhase::EngineeringCachePressure {
                    operation_index,
                    tile_cache_insert_bytes,
                    atlas_upload_checkpoint_bytes,
                } => Some((
                    operation_index,
                    tile_cache_insert_bytes,
                    atlas_upload_checkpoint_bytes,
                )),
                _ => None,
            })
        else {
            return;
        };
        if !self.pending_viewport_requests.is_empty()
            || !self.pending_tile_requests.is_empty()
            || self.active_tile_jobs > 0
            || self.active_page_surface_jobs > 0
        {
            cx.notify();
            return;
        }
        if let Some(checkpoint) = atlas_upload_checkpoint_bytes {
            let submitted = self
                .tile_atlas_uploads
                .lock()
                .expect("tile atlas upload receipt lock")
                .values()
                .copied()
                .sum::<usize>();
            if submitted <= checkpoint {
                cx.notify();
                return;
            }
        }
        let steps_per_cycle = 4;
        let total = plan.cycles.saturating_mul(steps_per_cycle);
        if operation_index < total {
            let mut next_atlas_upload_checkpoint = None;
            match operation_index % steps_per_cycle {
                0 => {
                    let submitted: usize = self
                        .tile_atlas_uploads
                        .lock()
                        .expect("tile atlas upload receipt lock")
                        .values()
                        .copied()
                        .sum();
                    if submitted == 0 {
                        next_atlas_upload_checkpoint = Some(0);
                    }
                    self.set_zoom(800.0, ZoomPreset::Manual, cx);
                }
                1 => {
                    self.document_scroll
                        .set_offset(point(px(-512.0), px(-384.0)));
                    self.tile_plan_key = None;
                    cx.notify();
                }
                2 => self.set_zoom(100.0, ZoomPreset::Manual, cx),
                3 => {
                    self.document_scroll.set_offset(point(px(0.0), px(0.0)));
                    self.tile_plan_key = None;
                    cx.notify();
                }
                _ => unreachable!(),
            }
            if let Some(scenario) = self.perf_scenario.as_mut() {
                scenario.comparison_phase = ComparisonPhase::EngineeringCachePressure {
                    operation_index: operation_index + 1,
                    tile_cache_insert_bytes,
                    atlas_upload_checkpoint_bytes: next_atlas_upload_checkpoint,
                };
            }
            return;
        }

        let renderer_resource_submission_bytes = self
            .tile_atlas_uploads
            .lock()
            .expect("tile atlas upload receipt lock")
            .values()
            .copied()
            .sum::<usize>();
        let before = AppResourceObservation {
            document_count: self.documents.len(),
            tile_cache_bytes: self.tile_cache.bytes(),
            decoded_page_bytes: self
                .document()
                .map(PdfDocument::cached_image_bytes)
                .unwrap_or(0),
            renderer_resource_submission_bytes,
        };
        if renderer_resource_submission_bytes == 0 {
            cx.notify();
            return;
        }
        if tile_cache_insert_bytes == 0 {
            self.fail_comparison_scenario(
                "engineering cache cycles recorded no decoded tile insertions",
                cx,
            );
            return;
        }
        let Some(active) = self.active_document else {
            self.fail_comparison_scenario("engineering cache recovery has no open document", cx);
            return;
        };
        self.close_document(active, cx);
        self.tile_atlas_uploads
            .lock()
            .expect("tile atlas upload receipt lock")
            .clear();
        let after = AppResourceObservation {
            document_count: self.documents.len(),
            tile_cache_bytes: self.tile_cache.bytes(),
            decoded_page_bytes: self
                .document()
                .map(PdfDocument::cached_image_bytes)
                .unwrap_or(0),
            renderer_resource_submission_bytes: self
                .tile_atlas_uploads
                .lock()
                .expect("tile atlas upload receipt lock")
                .values()
                .copied()
                .sum(),
        };
        let receipt = match assess_cache_recovery(
            &plan,
            CacheRecoveryObservation {
                cycles_completed: plan.cycles,
                cache_limit_bytes: CachePolicy::default().max_bytes,
                decoded_limit_bytes: 128 * 1_024 * 1_024,
                before,
                after,
            },
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        perf::emit(
            "comparison-v4-command-receipt",
            perf::fields([
                ("command_id", json!(&receipt.command_id)),
                ("component_scenario", json!("cache-pressure-recovery")),
                ("passed", json!(true)),
                ("observation", json!(&receipt.observation)),
                (
                    "released_render_bytes",
                    json!(receipt.released_render_bytes),
                ),
                ("tile_cache_insert_bytes", json!(tile_cache_insert_bytes)),
                ("milestone_ids", json!(&receipt.milestones)),
            ]),
        );
        perf::emit(
            "comparison-memory-recovery",
            perf::fields([
                (
                    "released_render_bytes",
                    json!(receipt.released_render_bytes),
                ),
                ("before", json!(before)),
                ("after", json!(after)),
            ]),
        );
        for milestone in &receipt.milestones {
            if let Err(error) = self.record_comparison_milestone(&receipt.command_id, milestone) {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        }
        self.complete_comparison_if_ready(cx);
    }

    fn start_next_perf_operation(&mut self, cx: &mut Context<Self>) {
        let Some((scenario_kind, step_index)) = self
            .perf_scenario
            .as_ref()
            .map(|scenario| (scenario.kind, scenario.step_index))
        else {
            return;
        };
        let page_sequence = self
            .document()
            .map(|document| perf_page_sequence(document.page_count));
        let next = match scenario_kind {
            PerfScenarioKind::ViewerLayout => {
                self.set_scroll_mode(ScrollMode::SinglePage, cx);
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::ViewerLayoutSingle;
                }
                return;
            }
            PerfScenarioKind::PageNavigation => page_sequence
                .as_ref()
                .and_then(|sequence| sequence.get(step_index))
                .map(|page| ("page", *page as f64)),
            PerfScenarioKind::Zoom => self
                .perf_scenario
                .as_ref()
                .and_then(|scenario| scenario.comparison_plan.as_ref())
                .and_then(ComparisonScenarioPlan::zoom)
                .and_then(|plan| plan.percent.get(step_index))
                .map(|zoom| ("zoom", f64::from(*zoom))),
            PerfScenarioKind::HighZoomPan => (step_index == 0).then(|| {
                self.scroll_mode = ScrollMode::SinglePage;
                ("zoom-pan-prime", 1600.0)
            }),
            PerfScenarioKind::CachePressure => {
                self.tile_atlas_uploads
                    .lock()
                    .expect("tile atlas upload receipt lock")
                    .clear();
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::CachePressure {
                        operation_index: 0,
                        tile_cache_insert_bytes: 0,
                        atlas_upload_checkpoint_bytes: None,
                    };
                }
                self.advance_cache_pressure(cx);
                return;
            }
            PerfScenarioKind::FitModes => {
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::FitModesStart;
                }
                cx.notify();
                return;
            }
            PerfScenarioKind::CachePressureRecovery => {
                self.tile_atlas_uploads
                    .lock()
                    .expect("tile atlas upload receipt lock")
                    .clear();
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::EngineeringCachePressure {
                        operation_index: 0,
                        tile_cache_insert_bytes: 0,
                        atlas_upload_checkpoint_bytes: None,
                    };
                }
                self.advance_engineering_cache_pressure(cx);
                return;
            }
            PerfScenarioKind::CloseReopen => {
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::CloseReopen {
                        stage: CloseReopenStage::Start,
                    };
                }
                cx.notify();
                return;
            }
            PerfScenarioKind::AnnotationCreate => {
                let native = self
                    .perf_scenario
                    .as_ref()
                    .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest);
                if native {
                    self.prepare_native_annotation_input(NativeAnnotationStage::Rectangle, cx);
                } else {
                    self.start_annotation_create_comparison(cx);
                }
                return;
            }
            PerfScenarioKind::AnnotationTransform => {
                let native = self
                    .perf_scenario
                    .as_ref()
                    .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest);
                if native {
                    self.prepare_native_transform_input(
                        NativeTransformStage::PrerequisiteCreate,
                        NativeTransformProgress::default(),
                        cx,
                    );
                    return;
                }
                let Some(document_id) = self.document().map(|document| document.id) else {
                    self.fail_comparison_scenario(
                        "annotation transform requires an open document",
                        cx,
                    );
                    return;
                };
                let report = match RectangleInteractionScenario::embedded().and_then(|scenario| {
                    scenario.execute_transform(document_id, &mut self.annotation_adapter)
                }) {
                    Ok(report) => report,
                    Err(error) => {
                        self.fail_comparison_scenario(error.to_string(), cx);
                        return;
                    }
                };
                for evidence in report.command_evidence {
                    for milestone in evidence.proven_milestones {
                        if let Err(error) =
                            self.record_comparison_milestone(&evidence.command_id, &milestone)
                        {
                            self.fail_comparison_scenario(error, cx);
                            return;
                        }
                    }
                }
                self.complete_comparison_if_ready(cx);
                return;
            }
            PerfScenarioKind::AnnotationPropertiesHistory => {
                let Some(document_id) = self.document().map(|document| document.id) else {
                    self.fail_comparison_scenario(
                        "annotation properties/history requires an open document",
                        cx,
                    );
                    return;
                };
                let report = match RectangleInteractionScenario::embedded().and_then(|scenario| {
                    scenario.execute_properties_history(document_id, &mut self.annotation_adapter)
                }) {
                    Ok(report) => report,
                    Err(error) => {
                        self.fail_comparison_scenario(error.to_string(), cx);
                        return;
                    }
                };
                for evidence in report.command_evidence {
                    for milestone in evidence.proven_milestones {
                        if let Err(error) =
                            self.record_comparison_milestone(&evidence.command_id, &milestone)
                        {
                            self.fail_comparison_scenario(error, cx);
                            return;
                        }
                    }
                }
                self.complete_comparison_if_ready(cx);
                return;
            }
            PerfScenarioKind::EditorCreate => {
                let native_lane = self
                    .perf_scenario
                    .as_ref()
                    .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest);
                if native_lane {
                    self.prepare_native_editor_input(NativeEditorStage::Text, cx);
                    return;
                }
                let Some(document_id) = self.document().map(|document| document.id) else {
                    self.fail_comparison_scenario("editor-create requires an open document", cx);
                    return;
                };
                let scene = match prepare_representative_create_scene(
                    document_id,
                    self.annotation_image_asset.clone(),
                    &mut self.annotation_adapter,
                ) {
                    Ok(scene) => scene,
                    Err(error) => {
                        self.fail_comparison_scenario(error.to_string(), cx);
                        return;
                    }
                };
                let semantic = match build_representative_semantic_report(
                    document_id,
                    &scene,
                    &self.annotation_adapter,
                ) {
                    Ok(report) => report,
                    Err(error) => {
                        self.fail_comparison_scenario(error, cx);
                        return;
                    }
                };
                for evidence in &semantic.commands {
                    perf::emit(
                        "comparison-command-evidence",
                        perf::fields([
                            ("command_id", json!(evidence.command_id)),
                            ("evidence_scope", json!("domain-semantic")),
                            (
                                "all_manifest_milestones_proven",
                                json!(evidence.blocked_manifest_milestones.is_empty()),
                            ),
                            ("decision_timing_eligible", json!(false)),
                            ("evidence", json!(evidence)),
                        ]),
                    );
                }
                perf::emit(
                    "editor-create-semantic-evidence",
                    perf::fields([("report", json!(semantic))]),
                );
                *self
                    .annotation_overlay_paint
                    .lock()
                    .expect("annotation presentation marker lock") = None;
                if let Some(perf) = self.perf_scenario.as_mut() {
                    perf.comparison_phase = ComparisonPhase::AwaitingEditorCreatePaint;
                }
                cx.notify();
                return;
            }
            PerfScenarioKind::EditorWorkload | PerfScenarioKind::PersistenceWorkload => {
                let persistence = scenario_kind == PerfScenarioKind::PersistenceWorkload;
                let Some(document_id) = self.document().map(|document| document.id) else {
                    self.fail_comparison_scenario("editor workload requires an open document", cx);
                    return;
                };
                if let Err(error) = prepare_representative_create_scene(
                    document_id,
                    self.annotation_image_asset.clone(),
                    &mut self.annotation_adapter,
                ) {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
                self.editor_presentation_pending = Some(persistence);
                *self
                    .annotation_overlay_paint
                    .lock()
                    .expect("annotation presentation marker lock") = None;
                if let Some(perf) = self.perf_scenario.as_mut() {
                    perf.comparison_phase =
                        ComparisonPhase::AwaitingEditorWorkloadPaint { persistence };
                }
                cx.notify();
                return;
            }
            PerfScenarioKind::ContinuousScroll | PerfScenarioKind::DynamicFidelity => {
                if scenario_kind == PerfScenarioKind::DynamicFidelity {
                    self.zoom_percent = 100.0;
                }
                let native = self
                    .perf_scenario
                    .as_ref()
                    .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest);
                if native {
                    self.scroll_mode = ScrollMode::Continuous;
                    self.continuous_scroll.set_offset(point(px(0.0), px(0.0)));
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.comparison_phase = ComparisonPhase::AwaitingNativeScrollSurface;
                    }
                    cx.notify();
                } else {
                    self.start_continuous_scroll_comparison(cx);
                }
                return;
            }
            PerfScenarioKind::MultiDocumentSession => {
                cx.notify();
                return;
            }
            PerfScenarioKind::NativePropertyEditUndo => {
                self.prepare_native_v5_property(cx);
                return;
            }
            PerfScenarioKind::NativeSnapTransform => {
                self.prepare_native_v5_snap(cx);
                return;
            }
            PerfScenarioKind::EmptyShell | PerfScenarioKind::OpenPdf => None,
        };
        let Some((kind, value)) = next else {
            if self
                .perf_scenario
                .as_ref()
                .is_some_and(|scenario| scenario.comparison_plan.is_some())
            {
                self.complete_comparison_if_ready(cx);
            } else {
                self.complete_perf_scenario(cx);
            }
            return;
        };
        let operation = PerfOperation {
            kind,
            value,
            started_ms: perf::elapsed_ms(),
        };
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.active_operation = Some(operation);
        }
        perf::emit(
            "operation-started",
            perf::fields([
                ("operation", json!(kind)),
                ("value", json!(value)),
                ("step", json!(step_index)),
            ]),
        );
        match kind {
            "page" => self.set_page(value as usize, cx),
            "zoom" => self.set_zoom(value as f32, ZoomPreset::Manual, cx),
            "zoom-pan-prime" => self.set_zoom(value as f32, ZoomPreset::Manual, cx),
            _ => unreachable!(),
        }
    }

    fn complete_perf_scenario(&mut self, cx: &mut Context<Self>) {
        if let Some(window_size) = self.last_window_logical_size {
            self.emit_comparison_view_state("measurement-end", window_size);
        }
        perf::emit("scenario-complete", Default::default());
        self.close_perf_window(cx);
    }

    fn await_dynamic_runner_result(&mut self, cx: &mut Context<Self>) {
        if !self.perf_scenario.as_ref().is_some_and(|scenario| {
            scenario.input_lane == PerfInputLane::NativeX11Xtest
                && scenario.kind == PerfScenarioKind::DynamicFidelity
        }) {
            self.complete_perf_scenario(cx);
            return;
        }
        let Some(directory) =
            std::env::var_os("BP_GPUI_EVIDENCE_DIR").map(std::path::PathBuf::from)
        else {
            self.fail_comparison_scenario(
                "dynamic native runner acknowledgement requires BP_GPUI_EVIDENCE_DIR",
                cx,
            );
            return;
        };
        let path = directory.join(DYNAMIC_RUNNER_RESULT_FILE);
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::AwaitingDynamicRunnerResult;
        }
        perf::emit(
            "dynamic-fidelity-runner-result-waiting",
            perf::fields([("path", json!(path))]),
        );
        let executor = cx.background_executor().clone();
        let task = executor.clone().spawn(async move {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                match fs::read_to_string(&path) {
                    Ok(contents) => {
                        return validate_dynamic_runner_result(&contents)
                            .map(|passed| (passed, path));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(format!(
                            "could not read dynamic runner result {}: {error}",
                            path.display()
                        ));
                    }
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "timed out waiting for dynamic runner result {}",
                        path.display()
                    ));
                }
                executor.timer(Duration::from_millis(10)).await;
            }
        });
        cx.spawn(async move |entity, cx| {
            let result = task.await;
            let _ = entity.update(cx, |this, cx| {
                if !this.perf_scenario.as_ref().is_some_and(|scenario| {
                    matches!(
                        scenario.comparison_phase,
                        ComparisonPhase::AwaitingDynamicRunnerResult
                    )
                }) {
                    return;
                }
                match result {
                    Ok((true, path)) => {
                        perf::emit(
                            "dynamic-fidelity-runner-result-acknowledged",
                            perf::fields([("path", json!(path)), ("status", json!("passed"))]),
                        );
                        this.complete_perf_scenario(cx);
                    }
                    Ok((false, path)) => this.fail_comparison_scenario(
                        format!(
                            "dynamic runner reported failed evidence at {}",
                            path.display()
                        ),
                        cx,
                    ),
                    Err(error) => this.fail_comparison_scenario(error, cx),
                }
            });
        })
        .detach();
    }

    fn emit_native_v5_milestones(command_id: &str, milestones: &[String]) {
        for milestone in milestones {
            perf::emit(
                "comparison-milestone",
                perf::fields([
                    ("command_id", json!(command_id)),
                    ("milestone", json!(milestone)),
                ]),
            );
        }
    }

    fn advance_native_v5_presentation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((kind, stage, document_id)) = self.perf_scenario.as_ref().and_then(|scenario| {
            let state = scenario.native_editing_v5.as_ref()?;
            matches!(
                state.stage,
                NativeEditingV5Stage::PropertyAwaitingPaint
                    | NativeEditingV5Stage::SnapAwaitingPaint
            )
            .then(|| {
                (
                    scenario.kind,
                    state.stage,
                    self.document().map(|document| document.id),
                )
            })
        }) else {
            return false;
        };
        let Some(document_id) = document_id else {
            self.fail_comparison_scenario("native v5 presentation document is missing", cx);
            return true;
        };
        let scene = self.annotation_adapter.document_scene(document_id, 0);
        let thumbnail = self.annotation_adapter.thumbnail_scene(document_id, 0);
        let paint = self
            .annotation_overlay_paint
            .lock()
            .expect("annotation presentation marker lock")
            .clone();
        let Some(paint) = paint.filter(|paint| {
            paint.document_id == document_id
                && paint.page_index == 0
                && paint.scene_revision == scene.revision
        }) else {
            window.refresh();
            cx.notify();
            return true;
        };
        #[cfg(feature = "benchmark-evidence")]
        let input_latency_samples_after = window.input_latency_snapshot().latency_histogram.len();
        #[cfg(not(feature = "benchmark-evidence"))]
        let input_latency_samples_after = 0;
        match (kind, stage) {
            (
                PerfScenarioKind::NativePropertyEditUndo,
                NativeEditingV5Stage::PropertyAwaitingPaint,
            ) => {
                let Some((target_id, committed_width, history_delta, input_before)) = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.native_editing_v5.as_ref())
                    .and_then(|state| {
                        let receipt = state.property_commit.as_ref()?;
                        Some((
                            receipt.target_id.clone(),
                            receipt.after.stroke_width_pt(),
                            receipt
                                .history_after
                                .0
                                .saturating_sub(receipt.history_before.0),
                            {
                                #[cfg(feature = "benchmark-evidence")]
                                {
                                    state.input_latency_samples_before.unwrap_or_default()
                                }
                                #[cfg(not(feature = "benchmark-evidence"))]
                                {
                                    0
                                }
                            },
                        ))
                    })
                else {
                    self.fail_comparison_scenario("native v5 property receipt is missing", cx);
                    return true;
                };
                let scene_rectangle = scene
                    .rectangles
                    .iter()
                    .find(|rectangle| rectangle.id == target_id);
                let thumbnail_current = thumbnail.revision == scene.revision
                    && thumbnail.rectangles.iter().any(|rectangle| {
                        scene_rectangle.is_some_and(|current| {
                            rectangle.id == current.id
                                && rectangle.rect == current.rect
                                && rectangle.appearance == current.appearance
                                && rectangle.locked == current.locked
                                && !rectangle.preview
                        })
                    });
                let scene_current = scene_rectangle.is_some_and(|rectangle| {
                    rectangle.appearance.stroke_width_pt() == committed_width
                });
                let overlay_current = paint
                    .rectangle_ids
                    .iter()
                    .any(|id| id == target_id.as_str());
                let input_acknowledged = input_latency_samples_after > input_before;
                if history_delta != 1
                    || !scene_current
                    || !overlay_current
                    || !thumbnail_current
                    || !input_acknowledged
                {
                    self.fail_comparison_scenario(
                        format!(
                            "native v5 property presentation mismatch: history_delta={history_delta}, scene_current={scene_current}, overlay_current={overlay_current}, thumbnail_current={thumbnail_current}, input_before={input_before}, input_after={input_latency_samples_after}"
                        ),
                        cx,
                    );
                    return true;
                }
                perf::emit(
                    "native-v5-property-presentation-evidence",
                    perf::fields([
                        ("command_id", json!("annotation:native-property-edit-undo")),
                        ("stroke_width_points", json!(committed_width)),
                        ("history_revision_delta", json!(history_delta)),
                        ("scene_revision", json!(scene.revision)),
                        ("thumbnail_current", json!(thumbnail_current)),
                        ("input_latency_samples_before", json!(input_before)),
                        (
                            "input_latency_samples_after",
                            json!(input_latency_samples_after),
                        ),
                        ("gpui_platform_draw_submitted", json!(true)),
                        ("physical_scanout_observed", json!(false)),
                    ]),
                );
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.native_editing_v5.as_mut())
                {
                    state.stage = NativeEditingV5Stage::PropertyUndo;
                    state.native_ready_emitted = false;
                }
                cx.notify();
            }
            (PerfScenarioKind::NativeSnapTransform, NativeEditingV5Stage::SnapAwaitingPaint) => {
                let Some((
                    receipt,
                    expected_rect,
                    samples,
                    guide_painted,
                    acquired_count,
                    first_point,
                    last_point,
                    pointer_up_t_ms,
                    input_before,
                )) = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.native_editing_v5.as_ref())
                    .and_then(|state| {
                        Some((
                            state.snap_commit.clone()?,
                            state.plan.snap_transform.expected_final_rect,
                            state.snap_sample_timestamps_ms.clone(),
                            state.snap_guide_painted,
                            state.snap_target_acquired_count,
                            state.snap_first_sample_point,
                            state.snap_last_sample_point,
                            state.snap_pointer_up_t_ms?,
                            {
                                #[cfg(feature = "benchmark-evidence")]
                                {
                                    state.input_latency_samples_before.unwrap_or_default()
                                }
                                #[cfg(not(feature = "benchmark-evidence"))]
                                {
                                    0
                                }
                            },
                        ))
                    })
                else {
                    self.fail_comparison_scenario("native v5 snap receipt is missing", cx);
                    return true;
                };
                let scene_rectangle = scene
                    .rectangles
                    .iter()
                    .find(|rectangle| rectangle.id == receipt.target_id);
                let thumbnail_current = thumbnail.revision == scene.revision
                    && thumbnail.rectangles.iter().any(|rectangle| {
                        scene_rectangle.is_some_and(|current| {
                            rectangle.id == current.id
                                && rectangle.rect == current.rect
                                && rectangle.appearance == current.appearance
                                && rectangle.locked == current.locked
                                && !rectangle.preview
                        })
                    });
                let timestamps_monotonic = samples.windows(2).all(|pair| pair[0] < pair[1]);
                if samples.len() < 3
                    || !receipt.resolution.acquired
                    || receipt.sample_count != samples.len()
                    || first_point
                        != Some(
                            self.perf_scenario
                                .as_ref()
                                .and_then(|scenario| scenario.native_editing_v5.as_ref())
                                .expect("native v5 state exists")
                                .plan
                                .snap_transform
                                .start,
                        )
                    || last_point
                        != Some(
                            self.perf_scenario
                                .as_ref()
                                .and_then(|scenario| scenario.native_editing_v5.as_ref())
                                .expect("native v5 state exists")
                                .plan
                                .snap_transform
                                .unsnapped_end,
                        )
                    || !timestamps_monotonic
                    || receipt.final_rect != expected_rect
                    || !guide_painted
                    || acquired_count == 0
                    || scene_rectangle.is_none_or(|rectangle| rectangle.rect != receipt.final_rect)
                    || !paint
                        .rectangle_ids
                        .iter()
                        .any(|id| id == receipt.target_id.as_str())
                    || !thumbnail_current
                    || input_latency_samples_after <= input_before
                {
                    self.fail_comparison_scenario(
                        "native v5 snap commit, guide, or GPUI frame is not exact",
                        cx,
                    );
                    return true;
                }
                perf::emit(
                    "native-v5-snap-presentation-evidence",
                    perf::fields([
                        (
                            "command_id",
                            json!("annotation:native-snap-transform-120hz"),
                        ),
                        ("observed_application_update_timestamps_ms", json!(samples)),
                        (
                            "observed_application_update_count",
                            json!(receipt.sample_count),
                        ),
                        ("duration_ms", json!(perf::elapsed_ms() - pointer_up_t_ms)),
                        ("first_position_observed", json!(true)),
                        ("final_position_observed", json!(true)),
                        ("snap_guide_presented", json!(true)),
                        ("snap_target_acquired_count", json!(acquired_count)),
                        ("scene_revision", json!(scene.revision)),
                        ("thumbnail_current", json!(thumbnail_current)),
                        ("input_latency_samples_before", json!(input_before)),
                        (
                            "input_latency_samples_after",
                            json!(input_latency_samples_after),
                        ),
                        ("gpui_platform_draw_submitted", json!(true)),
                        ("physical_scanout_observed", json!(false)),
                    ]),
                );
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.native_editing_v5.as_mut())
                {
                    state.stage = NativeEditingV5Stage::SnapUndo;
                    state.native_ready_emitted = false;
                }
                cx.notify();
            }
            _ => {}
        }
        true
    }

    fn native_v5_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text_input_active = self
            .document()
            .is_some_and(|document| self.annotation_adapter.selected_text(document.id).is_some());
        if let Some(tool) = annotation_tool_shortcut(event, text_input_active) {
            self.select_tool(tool, cx);
            return;
        }
        if event.keystroke.key.as_str() != "z" || !event.keystroke.modifiers.control {
            return;
        }
        let Some((kind, stage, document_id)) = self.perf_scenario.as_ref().and_then(|scenario| {
            let state = scenario.native_editing_v5.as_ref()?;
            Some((scenario.kind, state.stage, self.document()?.id))
        }) else {
            return;
        };
        match (kind, stage, event.keystroke.modifiers.shift) {
            (
                PerfScenarioKind::NativePropertyEditUndo,
                NativeEditingV5Stage::PropertyUndo,
                false,
            ) => {
                let receipt = match self.annotation_adapter.undo_native_v5_property(document_id) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        self.fail_comparison_scenario(error.to_string(), cx);
                        return;
                    }
                };
                let scene = self.annotation_adapter.document_scene(document_id, 0);
                let thumbnail = self.annotation_adapter.thumbnail_scene(document_id, 0);
                let canonical = scene.rectangles.iter().any(|rectangle| {
                    rectangle.id == receipt.target_id
                        && rectangle.rect == receipt.rect
                        && rectangle.appearance == receipt.before
                });
                let thumbnail_current = thumbnail.revision == scene.revision
                    && thumbnail.rectangles.iter().any(|rectangle| {
                        rectangle.id == receipt.target_id
                            && rectangle.rect == receipt.rect
                            && rectangle.appearance == receipt.before
                    });
                perf::emit(
                    "native-v5-property-application-evidence",
                    perf::fields([
                        ("command_id", json!("annotation:native-property-edit-undo")),
                        ("property", json!("stroke_width_points")),
                        ("before", json!(receipt.before.stroke_width_pt())),
                        ("committed", json!(receipt.after.stroke_width_pt())),
                        ("after_undo", json!(receipt.before.stroke_width_pt())),
                        (
                            "effective_history_revision_delta",
                            json!(
                                receipt
                                    .history_after
                                    .0
                                    .saturating_sub(receipt.history_before.0)
                            ),
                        ),
                        ("application_undo_count", json!(1)),
                        ("canonical_state_restored", json!(canonical)),
                        ("known_baseline_defect_id", Value::Null),
                        ("native_presentation_acknowledged", json!(true)),
                        ("thumbnail_current", json!(thumbnail_current)),
                    ]),
                );
                let milestones = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.native_editing_v5.as_ref())
                    .map(|state| state.plan.property_edit.expected_milestones.clone())
                    .unwrap_or_default();
                Self::emit_native_v5_milestones(
                    "annotation:native-property-edit-undo",
                    &milestones,
                );
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.native_editing_v5.as_mut())
                {
                    state.stage = NativeEditingV5Stage::Complete;
                }
                cx.stop_propagation();
                self.complete_perf_scenario(cx);
            }
            (PerfScenarioKind::NativeSnapTransform, NativeEditingV5Stage::SnapUndo, false) => {
                if let Err(error) = self.annotation_adapter.undo_native_v5_snap(document_id) {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.native_editing_v5.as_mut())
                {
                    state.stage = NativeEditingV5Stage::SnapRedo;
                    state.native_ready_emitted = false;
                }
                cx.stop_propagation();
                cx.notify();
            }
            (PerfScenarioKind::NativeSnapTransform, NativeEditingV5Stage::SnapRedo, true) => {
                let receipt = match self.annotation_adapter.redo_native_v5_snap(document_id) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        self.fail_comparison_scenario(error.to_string(), cx);
                        return;
                    }
                };
                let Some((plan, samples, acquired_count, guide_count)) = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.native_editing_v5.as_ref())
                    .map(|state| {
                        (
                            state.plan.snap_transform.clone(),
                            state.snap_sample_timestamps_ms.clone(),
                            state.snap_target_acquired_count,
                            usize::from(state.snap_guide_painted),
                        )
                    })
                else {
                    return;
                };
                let scene = self.annotation_adapter.document_scene(document_id, 0);
                let thumbnail = self.annotation_adapter.thumbnail_scene(document_id, 0);
                let thumbnail_current = thumbnail.revision == scene.revision
                    && thumbnail.rectangles.iter().any(|rectangle| {
                        rectangle.id == receipt.target_id
                            && rectangle.rect == receipt.final_rect
                            && rectangle.appearance == receipt.appearance
                    });
                perf::emit(
                    "native-v5-snap-application-evidence",
                    perf::fields([
                        (
                            "command_id",
                            json!("annotation:native-snap-transform-120hz"),
                        ),
                        ("input_rate_hz", json!(plan.rate_hz)),
                        ("expected_injected_sample_count", json!(plan.sample_count)),
                        (
                            "observed_application_update_count",
                            json!(receipt.sample_count),
                        ),
                        ("observed_application_update_timestamps_ms", json!(samples)),
                        ("first_position_observed", json!(true)),
                        ("final_position_observed", json!(true)),
                        ("snap_enabled", json!(true)),
                        ("sensitivity_css_px", json!(receipt.sensitivity_css_px)),
                        (
                            "observed_pixels_per_point",
                            json!(receipt.observed_pixels_per_point),
                        ),
                        (
                            "derived_threshold_points",
                            json!(receipt.derived_threshold_pt),
                        ),
                        (
                            "observed_raw_delta_points",
                            json!({
                                "x": receipt.resolution.raw.x,
                                "y": receipt.resolution.raw.y,
                            }),
                        ),
                        (
                            "observed_snap_correction_points",
                            json!({
                                "x": receipt.resolution.correction.x,
                                "y": receipt.resolution.correction.y,
                            }),
                        ),
                        ("snap_target_acquired_count", json!(acquired_count)),
                        ("snap_guide_presented_count", json!(guide_count)),
                        (
                            "observed_final_rectangle",
                            json!({
                                "x1": receipt.final_rect.x,
                                "y1": receipt.final_rect.y,
                                "x2": receipt.final_rect.x + receipt.final_rect.width,
                                "y2": receipt.final_rect.y + receipt.final_rect.height,
                            }),
                        ),
                        ("maximum_geometry_deviation_points", json!(0.0)),
                        ("gesture_commit_count", json!(1)),
                        ("undo_redo_exact", json!(true)),
                        ("thumbnail_current", json!(thumbnail_current)),
                    ]),
                );
                Self::emit_native_v5_milestones(
                    "annotation:native-snap-transform-120hz",
                    &plan.expected_milestones,
                );
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.native_editing_v5.as_mut())
                {
                    state.stage = NativeEditingV5Stage::Complete;
                }
                cx.stop_propagation();
                self.complete_perf_scenario(cx);
            }
            _ => {}
        }
    }

    fn close_perf_window(&mut self, cx: &mut Context<Self>) {
        let window_handle = self.perf_window_handle.take();
        cx.defer(move |cx| {
            if let Some(window_handle) = window_handle {
                let _ = window_handle.update(cx, |_, window, _| window.remove_window());
            } else {
                cx.quit();
            }
        });
    }

    fn complete_comparison_if_ready(&mut self, cx: &mut Context<Self>) {
        let missing = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_gate.as_ref())
            .map(MilestoneGate::missing)
            .unwrap_or_else(|| vec![("scenario".into(), "milestone-gate-missing".into())]);
        if missing.is_empty() {
            let native_lane = self
                .perf_scenario
                .as_ref()
                .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest);
            if native_lane {
                #[cfg(feature = "benchmark-evidence")]
                {
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.comparison_completion_pending = true;
                    }
                    cx.notify();
                }
                #[cfg(not(feature = "benchmark-evidence"))]
                self.fail_comparison_scenario(
                    "native application draw acknowledgement requires benchmark-evidence",
                    cx,
                );
            } else {
                self.complete_perf_scenario(cx);
            }
        } else {
            self.fail_comparison_scenario(
                format!("comparison milestone gate is incomplete: {missing:?}"),
                cx,
            );
        }
    }

    fn advance_annotation_create_comparison(&mut self, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("annotation document closed before paint gate", cx);
            return;
        };
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::annotation_create)
            .cloned()
        else {
            self.fail_comparison_scenario("annotation-create plan is unavailable", cx);
            return;
        };
        let scene = self.annotation_adapter.document_scene(document_id, 0);
        let thumbnail = self.annotation_adapter.thumbnail_scene(document_id, 0);
        let result = (|| -> Result<(), String> {
            let rectangle = scene
                .rectangles
                .iter()
                .find(|rectangle| rectangle.id.as_str() == plan.rectangle.annotation_id)
                .ok_or_else(|| "rectangle was not present in the painted scene".to_string())?;
            if rectangle.preview
                || rectangle.rect.x != plan.rectangle.start[0]
                || rectangle.rect.y != plan.rectangle.start[1]
                || rectangle.rect.width
                    != (plan.rectangle.finish[0] - plan.rectangle.start[0]).abs()
                || rectangle.rect.height
                    != (plan.rectangle.finish[1] - plan.rectangle.start[1]).abs()
            {
                return Err("painted rectangle geometry is not the manifest geometry".into());
            }
            let highlight = scene
                .pens
                .iter()
                .find(|pen| pen.id.as_str() == plan.highlight.annotation_id)
                .ok_or_else(|| "highlight was not present in the painted scene".to_string())?;
            if highlight.draft
                || highlight.tool != InkTool::Highlight
                || highlight.blend_mode != BlendMode::Multiply
            {
                return Err("painted highlight is not committed multiply-blend ink".into());
            }
            if thumbnail.revision != scene.revision
                || !thumbnail
                    .rectangles
                    .iter()
                    .any(|rectangle| rectangle.id.as_str() == plan.rectangle.annotation_id)
                || !thumbnail
                    .pens
                    .iter()
                    .any(|pen| pen.id.as_str() == plan.highlight.annotation_id)
            {
                return Err("thumbnail scene is not current with the document scene".into());
            }
            self.record_comparison_milestone(&plan.rectangle.command_id, "annotation-painted")?;
            self.record_comparison_milestone(&plan.rectangle.command_id, "thumbnail-current")?;
            self.record_comparison_milestone(&plan.highlight.command_id, "annotation-painted")?;
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error, cx);
            return;
        }
        self.complete_comparison_if_ready(cx);
    }

    fn advance_editor_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("editor-create document closed before paint", cx);
            return;
        };
        let scene = self.annotation_adapter.document_scene(document_id, 0);
        let paint = self
            .annotation_overlay_paint
            .lock()
            .expect("annotation presentation marker lock")
            .clone();
        let Some(paint) = paint.filter(|paint| {
            paint.document_id == document_id && paint.scene_revision == scene.revision
        }) else {
            cx.notify();
            return;
        };
        let Some(text) = scene
            .text_boxes
            .iter()
            .find(|annotation| annotation.id.as_str() == TEXT_CREATE_ID)
        else {
            self.fail_comparison_scenario("representative text create is missing", cx);
            return;
        };
        let Some(length) = scene
            .lengths
            .iter()
            .find(|annotation| annotation.id.as_str() == LENGTH_CREATE_ID)
        else {
            self.fail_comparison_scenario("representative length create is missing", cx);
            return;
        };
        if !scene
            .images
            .iter()
            .any(|annotation| annotation.id.as_str() == IMAGE_CREATE_ID)
        {
            self.fail_comparison_scenario("representative image create is missing", cx);
            return;
        }
        let shape = |annotation_id: &str,
                     content: &str,
                     font_family: &str,
                     font_size_px: f32,
                     window: &mut Window| {
            let text: SharedString = content.to_owned().into();
            let run = TextRun {
                len: text.len(),
                font: gpui::font(font_family.to_owned()),
                color: gpui::hsla(0.0, 0.0, 0.0, 1.0),
                background_color: None,
                underline: None,
                strikethrough: None,
                letter_spacing: None,
            };
            let shaped =
                window
                    .text_system()
                    .shape_line(text.clone(), px(font_size_px), &[run], None);
            TextShapeObservation {
                annotation_id: annotation_id.to_owned(),
                text: content.to_owned(),
                font_family: font_family.to_owned(),
                font_size_px,
                shaped_utf8_bytes: shaped.len(),
                shaped_width_px: f32::from(shaped.width()),
            }
        };
        let text_shape = shape(
            TEXT_CREATE_ID,
            &text.content,
            presentation_font_family(text.style.font_family()),
            (text.style.font_size_pt() as f32 * self.zoom_percent / 100.0).max(8.0),
            window,
        );
        let length_shape = shape(LENGTH_CREATE_ID, &length.caption, "Geist", 12.0, window);
        let render_image = self.annotation_image.clone();
        let render_size = render_image.size(0);
        let image_decode = ImageDecodeObservation {
            annotation_id: IMAGE_CREATE_ID.into(),
            render_image_id: render_image.id.0,
            width_px: u32::try_from(render_size.width.0).unwrap_or_default(),
            height_px: u32::try_from(render_size.height.0).unwrap_or_default(),
            decoded_bgra_bytes: render_image.as_bytes(0).map_or(0, <[u8]>::len),
        };
        #[cfg(feature = "benchmark-evidence")]
        let atlas_paint = self
            .annotation_image_atlas_paint
            .lock()
            .expect("annotation image atlas marker lock")
            .clone()
            .filter(|observation| {
                observation.document_id == document_id
                    && observation.page_index == 0
                    && observation.scene_revision == scene.revision
                    && observation.render_image_id == image_decode.render_image_id
                    && observation.decoded_bgra_bytes == image_decode.decoded_bgra_bytes
                    && observation.atlas_entry_observed
            });
        let native_lane = self
            .perf_scenario
            .as_ref()
            .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest);
        #[cfg(feature = "benchmark-evidence")]
        if native_lane && atlas_paint.is_none() {
            // The final overlay canvas runs after the annotation image child.
            // Wait until that callback records atlas residency for this exact
            // document revision and RenderImage.
            cx.notify();
            return;
        }
        #[cfg(feature = "benchmark-evidence")]
        let gpui_submission = if native_lane {
            self.perf_scenario
                .as_ref()
                .and_then(|scenario| scenario.comparison_input_latency_samples_before)
                .map(|input_latency_samples_before| {
                    let snapshot = window.input_latency_snapshot();
                    let atlas_paint = atlas_paint
                        .as_ref()
                        .expect("native lane waited for the matching atlas paint receipt");
                    GpuiSubmissionObservation {
                        input_latency_samples_before,
                        input_latency_samples_after: snapshot.latency_histogram.len(),
                        input_to_present_p50_ns: snapshot.latency_histogram.value_at_quantile(0.5),
                        input_to_present_p95_ns: snapshot.latency_histogram.value_at_quantile(0.95),
                        image_atlas_entry_observed: atlas_paint.atlas_entry_observed,
                        atlas_upload_bytes: atlas_paint.decoded_bgra_bytes,
                    }
                })
        } else {
            None
        };
        #[cfg(not(feature = "benchmark-evidence"))]
        let gpui_submission: Option<GpuiSubmissionObservation> = None;
        if let Some(observation) = gpui_submission.as_ref() {
            perf::emit(
                "gpui-native-present-submission-evidence",
                perf::fields([
                    (
                        "receipt_scope",
                        json!("platform-draw-submission-not-physical-scanout"),
                    ),
                    ("physical_scanout_observed", json!(false)),
                    ("physical_bus_upload_bytes", Value::Null),
                    ("observation", json!(observation)),
                    ("decision_timing_eligible", json!(false)),
                ]),
            );
        }
        let report = match build_representative_live_report(
            &scene,
            &paint,
            &text_shape,
            &length_shape,
            &image_decode,
            true,
            native_lane,
            gpui_submission,
        ) {
            Ok(report) => report,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        for evidence in &report.commands {
            perf::emit(
                "comparison-live-presentation-evidence",
                perf::fields([
                    ("command_id", json!(evidence.command_id)),
                    ("evidence_scope", json!("gpui-live-frame")),
                    ("decision_timing_eligible", json!(false)),
                    ("evidence", json!(evidence)),
                ]),
            );
        }
        perf::emit(
            "editor-create-presentation-frame-observed",
            perf::fields([("report", json!(&report))]),
        );
        let semantic = match build_representative_semantic_report(
            document_id,
            &scene,
            &self.annotation_adapter,
        ) {
            Ok(report) => report,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        let qualification = match qualify_representative_create(&semantic, &report) {
            Ok(qualification) => qualification,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        for evidence in &qualification.commands {
            perf::emit(
                "comparison-command-evidence",
                perf::fields([
                    ("command_id", json!(evidence.command_id)),
                    ("evidence_scope", json!("combined-native-gpui")),
                    (
                        "all_manifest_milestones_proven",
                        json!(evidence.blocked_manifest_milestones.is_empty()),
                    ),
                    ("decision_timing_eligible", json!(false)),
                    ("evidence", json!(evidence)),
                ]),
            );
            for milestone in &evidence.proven_manifest_milestones {
                if let Err(error) =
                    self.record_comparison_milestone(&evidence.command_id, milestone)
                {
                    self.fail_comparison_scenario(error, cx);
                    return;
                }
            }
        }
        if qualification.missing_requirements.is_empty() {
            perf::emit(
                "editor-create-qualified",
                perf::fields([("report", json!(qualification))]),
            );
            self.complete_comparison_if_ready(cx);
        } else {
            perf::emit(
                "editor-create-blocked",
                perf::fields([
                    ("decision_timing_eligible", json!(false)),
                    (
                        "missing_milestones",
                        json!(qualification.missing_requirements),
                    ),
                ]),
            );
            self.fail_comparison_scenario(
                format!(
                    "editor-create diagnostic is incomplete and cannot pass: {}",
                    qualification.missing_requirements.join(", ")
                ),
                cx,
            );
        }
    }

    fn advance_editor_workload(
        &mut self,
        persistence: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            self.fail_comparison_scenario("editor workload document closed before paint", cx);
            return;
        };
        if self.editor_dense_presentation_pending {
            let dense_document_id = document_id.saturating_add(1);
            let dense_scene = self.annotation_adapter.document_scene(dense_document_id, 0);
            let dense_thumbnail = self
                .annotation_adapter
                .thumbnail_scene(dense_document_id, 0);
            let paint = self
                .annotation_overlay_paint
                .lock()
                .expect("annotation presentation marker lock")
                .clone();
            let Some(paint) = paint.filter(|paint| {
                paint.document_id == dense_document_id
                    && paint.scene_revision == dense_scene.revision
            }) else {
                cx.notify();
                return;
            };
            let report = match build_dense_rectangle_live_report(
                dense_document_id,
                &dense_scene,
                &dense_thumbnail,
                &paint,
                true,
            ) {
                Ok(report) => report,
                Err(error) => {
                    self.fail_comparison_scenario(error, cx);
                    return;
                }
            };
            perf::emit(
                "comparison-command-evidence",
                perf::fields([
                    ("command_id", json!(report.evidence.command_id)),
                    ("evidence_scope", json!("gpui-dense-overlay-frame")),
                    ("all_manifest_milestones_proven", json!(true)),
                    ("decision_timing_eligible", json!(false)),
                    ("evidence", json!(&report.evidence)),
                ]),
            );
            perf::emit(
                "dense-rectangle-presentation-evidence",
                perf::fields([("report", json!(report))]),
            );
            self.editor_dense_presentation_pending = false;
            self.editor_overlay_document_id = None;
            *self
                .annotation_overlay_paint
                .lock()
                .expect("annotation presentation marker lock") = None;
            cx.notify();
            return;
        }
        let scene = self.annotation_adapter.document_scene(document_id, 0);
        if self.editor_presentation_pending.is_some() {
            let paint = self
                .annotation_overlay_paint
                .lock()
                .expect("annotation presentation marker lock")
                .clone();
            let Some(paint) = paint.filter(|paint| {
                paint.document_id == document_id && paint.scene_revision == scene.revision
            }) else {
                cx.notify();
                return;
            };
            let Some(text) = scene
                .text_boxes
                .iter()
                .find(|annotation| annotation.id.as_str() == TEXT_CREATE_ID)
            else {
                self.fail_comparison_scenario("representative text create is missing", cx);
                return;
            };
            let Some(length) = scene
                .lengths
                .iter()
                .find(|annotation| annotation.id.as_str() == LENGTH_CREATE_ID)
            else {
                self.fail_comparison_scenario("representative length create is missing", cx);
                return;
            };
            if !scene
                .images
                .iter()
                .any(|annotation| annotation.id.as_str() == IMAGE_CREATE_ID)
            {
                self.fail_comparison_scenario("representative image create is missing", cx);
                return;
            }
            let shape = |annotation_id: &str,
                         content: &str,
                         font_family: &str,
                         font_size_px: f32,
                         window: &mut Window| {
                let text: SharedString = content.to_owned().into();
                let run = TextRun {
                    len: text.len(),
                    font: gpui::font(font_family.to_owned()),
                    color: gpui::hsla(0.0, 0.0, 0.0, 1.0),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                    letter_spacing: None,
                };
                let shaped =
                    window
                        .text_system()
                        .shape_line(text.clone(), px(font_size_px), &[run], None);
                TextShapeObservation {
                    annotation_id: annotation_id.to_owned(),
                    text: content.to_owned(),
                    font_family: font_family.to_owned(),
                    font_size_px,
                    shaped_utf8_bytes: shaped.len(),
                    shaped_width_px: f32::from(shaped.width()),
                }
            };
            let text_shape = shape(
                TEXT_CREATE_ID,
                &text.content,
                presentation_font_family(text.style.font_family()),
                (text.style.font_size_pt() as f32 * self.zoom_percent / 100.0).max(8.0),
                window,
            );
            let length_shape = shape(LENGTH_CREATE_ID, &length.caption, "Geist", 12.0, window);
            let render_image = self.annotation_image.clone();
            let render_size = render_image.size(0);
            let image_decode = ImageDecodeObservation {
                annotation_id: IMAGE_CREATE_ID.into(),
                render_image_id: render_image.id.0,
                width_px: u32::try_from(render_size.width.0).unwrap_or_default(),
                height_px: u32::try_from(render_size.height.0).unwrap_or_default(),
                decoded_bgra_bytes: render_image.as_bytes(0).map_or(0, <[u8]>::len),
            };
            let report = match build_representative_live_report(
                &scene,
                &paint,
                &text_shape,
                &length_shape,
                &image_decode,
                true,
                false,
                None,
            ) {
                Ok(report) => report,
                Err(error) => {
                    self.fail_comparison_scenario(error, cx);
                    return;
                }
            };
            for evidence in &report.commands {
                perf::emit(
                    "comparison-live-presentation-evidence",
                    perf::fields([
                        ("command_id", json!(evidence.command_id)),
                        ("evidence_scope", json!("gpui-live-frame")),
                        ("decision_timing_eligible", json!(false)),
                        ("evidence", json!(evidence)),
                    ]),
                );
            }
            perf::emit(
                "editor-create-presentation-frame-observed",
                perf::fields([("report", json!(report))]),
            );

            self.annotation_adapter.remove_document(document_id);
            let scenario = match EditorComparisonScenario::embedded() {
                Ok(scenario) => scenario,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            let report = match scenario.execute(
                document_id,
                self.annotation_image_asset.clone(),
                &mut self.annotation_adapter,
                &mut RecordingEditorObserver::default(),
            ) {
                Ok(report) => report,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            for evidence in report.command_evidence {
                let all_manifest_milestones_proven =
                    evidence.blocked_manifest_milestones.is_empty();
                perf::emit(
                    "comparison-command-evidence",
                    perf::fields([
                        ("command_id", json!(evidence.command_id)),
                        ("evidence_scope", json!("domain-semantic")),
                        (
                            "all_manifest_milestones_proven",
                            json!(all_manifest_milestones_proven),
                        ),
                        ("decision_timing_eligible", json!(false)),
                        ("evidence", json!(evidence)),
                    ]),
                );
            }
            self.editor_presentation_pending = None;
            self.editor_dense_presentation_pending = true;
            self.editor_overlay_document_id = Some(document_id.saturating_add(1));
            *self
                .annotation_overlay_paint
                .lock()
                .expect("annotation presentation marker lock") = None;
            cx.notify();
            return;
        }
        let thumbnail = self.annotation_adapter.thumbnail_scene(document_id, 0);
        if scene.rectangles.len() != 1
            || scene.pens.len() != 1
            || scene.text_boxes.len() != 1
            || scene.lengths.len() != 1
            || scene.images.len() != 1
            || thumbnail.revision != scene.revision
        {
            self.fail_comparison_scenario(
                "editor workload did not reach a visible current scene",
                cx,
            );
            return;
        }
        let paint = self
            .annotation_overlay_paint
            .lock()
            .expect("annotation presentation marker lock")
            .clone();
        let Some(paint) = paint.filter(|paint| {
            paint.document_id == document_id && paint.scene_revision == scene.revision
        }) else {
            cx.notify();
            return;
        };
        let render_image = self.annotation_image.clone();
        let render_size = render_image.size(0);
        let image_decode = ImageDecodeObservation {
            annotation_id: IMAGE_CREATE_ID.into(),
            render_image_id: render_image.id.0,
            width_px: u32::try_from(render_size.width.0).unwrap_or_default(),
            height_px: u32::try_from(render_size.height.0).unwrap_or_default(),
            decoded_bgra_bytes: render_image.as_bytes(0).map_or(0, <[u8]>::len),
        };
        #[cfg(feature = "benchmark-evidence")]
        let atlas_paint = self
            .annotation_image_atlas_paint
            .lock()
            .expect("annotation image atlas marker lock")
            .clone()
            .filter(|observation| {
                observation.document_id == document_id
                    && observation.page_index == 0
                    && observation.scene_revision == scene.revision
                    && observation.render_image_id == image_decode.render_image_id
                    && observation.decoded_bgra_bytes == image_decode.decoded_bgra_bytes
                    && observation.atlas_entry_observed
            });
        #[cfg(feature = "benchmark-evidence")]
        let Some(atlas_paint) = atlas_paint else {
            cx.notify();
            return;
        };
        #[cfg(feature = "benchmark-evidence")]
        let gpui_frame = Some(GpuiFinalFrameObservation {
            frame_callback_after_submission: true,
            image_atlas_entry_observed: atlas_paint.atlas_entry_observed,
            atlas_upload_bytes: atlas_paint.decoded_bgra_bytes,
        });
        #[cfg(not(feature = "benchmark-evidence"))]
        let gpui_frame: Option<GpuiFinalFrameObservation> = None;
        let final_live_report = match build_editor_final_live_report(
            &scene,
            &thumbnail,
            &paint,
            &image_decode,
            gpui_frame,
        ) {
            Ok(report) => report,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return;
            }
        };
        for evidence in &final_live_report.commands {
            perf::emit(
                "comparison-command-evidence",
                perf::fields([
                    ("command_id", json!(evidence.command_id)),
                    ("evidence_scope", json!("gpui-final-editor-frame")),
                    (
                        "all_manifest_milestones_proven",
                        json!(evidence.blocked_manifest_milestones.is_empty()),
                    ),
                    ("decision_timing_eligible", json!(false)),
                    ("evidence", json!(evidence)),
                ]),
            );
        }
        perf::emit(
            "editor-final-presentation-evidence",
            perf::fields([("report", json!(&final_live_report))]),
        );
        let final_live_blockers = final_live_report
            .commands
            .iter()
            .flat_map(|evidence| {
                evidence
                    .blocked_manifest_milestones
                    .iter()
                    .map(move |blocked| format!("{}:{}", evidence.command_id, blocked.milestone))
            })
            .collect::<Vec<_>>();
        if !persistence && !final_live_blockers.is_empty() {
            self.fail_comparison_scenario(
                format!(
                    "editor final presentation is incomplete: {}",
                    final_live_blockers.join(", ")
                ),
                cx,
            );
            return;
        }
        perf::emit(
            "editor-scene-settled",
            perf::fields([
                ("document_revision", json!(scene.revision)),
                ("thumbnail_revision", json!(thumbnail.revision)),
            ]),
        );
        if !persistence {
            let editor_commands = match EditorComparisonScenario::embedded() {
                Ok(scenario) => scenario.command_ids(),
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            for command_id in editor_commands {
                perf::emit(
                    "comparison-command-complete",
                    perf::fields([("command_id", json!(command_id))]),
                );
            }
        }
        if persistence {
            let Some(source) = self.perf_reopen_path.clone() else {
                self.fail_comparison_scenario("persistence source closed", cx);
                return;
            };
            let report_result = if let Some(evidence_directory) =
                std::env::var_os("BP_GPUI_EVIDENCE_DIR").map(std::path::PathBuf::from)
            {
                PersistenceComparisonScenario::execute_with_evidence_directory(
                    &source,
                    &evidence_directory,
                    document_id,
                    &mut self.annotation_adapter,
                )
            } else {
                let output_dir = std::env::var_os("BP_GPUI_CACHE_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(std::env::temp_dir);
                let cycle_1 = output_dir.join("cycle-1.pdf");
                let cycle_2 = output_dir.join("cycle-2.pdf");
                PersistenceComparisonScenario::execute(
                    &source,
                    &cycle_1,
                    &cycle_2,
                    document_id,
                    &mut self.annotation_adapter,
                )
            };
            let report = match report_result {
                Ok(report) => report,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            let receipt = match report.exact_receipt() {
                Ok(receipt) => receipt,
                Err(error) => {
                    self.fail_comparison_scenario(error.to_string(), cx);
                    return;
                }
            };
            perf::emit(
                "persistence-evidence-complete",
                perf::fields([
                    ("receipt_status", json!(receipt.status)),
                    ("exact_receipt", json!(&receipt)),
                    ("cycle_1_sha256", json!(report.cycle_1_sha256)),
                    ("cycle_2_sha256", json!(report.cycle_2_sha256)),
                    (
                        "cycle_1_crop_sha256",
                        json!(report.raster_oracle.cycle_1_crop_sha256),
                    ),
                    (
                        "cycle_2_crop_sha256",
                        json!(report.raster_oracle.cycle_2_crop_sha256),
                    ),
                    (
                        "source_crop_sha256",
                        json!(report.raster_oracle.source_crop_sha256),
                    ),
                    ("typed_state_exact", json!(receipt.typed_state_exact)),
                    (
                        "untouched_annotation_count",
                        json!(report.untouched_annotation_count),
                    ),
                    ("unknown_graphs_exact", json!(receipt.unknown_probes_exact)),
                    (
                        "independent_pdf_validation_passed",
                        json!(receipt.independent_pdf_validation_passed),
                    ),
                    (
                        "independent_visual_validation_passed",
                        json!(receipt.independent_visual_validation_passed),
                    ),
                    (
                        "validator_receipt_count",
                        json!(receipt.validator_receipt_count),
                    ),
                    ("artifacts_retained", json!(receipt.artifacts_retained)),
                    ("decision_timing_eligible", json!(false)),
                ]),
            );
            for command_id in receipt.completed_command_ids {
                perf::emit(
                    "comparison-command-complete",
                    perf::fields([("command_id", json!(command_id))]),
                );
            }
        }
        self.complete_perf_scenario(cx);
    }

    fn continuous_visible_raster_is_ready(&self) -> Option<bool> {
        let Some(document) = self.document() else {
            return None;
        };
        (!self.continuous_visible_pages.is_empty()).then(|| {
            self.continuous_visible_pages.iter().all(|page| {
                document
                    .cached_viewport_image(*page, self.zoom_percent, self.display_scale_factor)
                    .is_some()
            })
        })
    }

    fn capture_dynamic_fidelity_state(&self, window: &Window) -> Option<Map<String, Value>> {
        if !self
            .perf_scenario
            .as_ref()
            .is_some_and(|scenario| scenario.kind == PerfScenarioKind::DynamicFidelity)
        {
            return None;
        }
        let viewport_width = (f32::from(window.viewport_size().width)
            - RAIL_WIDTH
            - SIDEBAR_WIDTH
            - RIGHT_RAIL_WIDTH)
            .max(1.0);
        let viewport_height = (f32::from(window.viewport_size().height)
            - WINDOW_TITLE_BAR_HEIGHT
            - MENU_BAR_HEIGHT
            - DOCUMENT_TAB_BAR_HEIGHT
            - PRIMARY_BAND_HEIGHT)
            .max(1.0);
        let offset = self.continuous_scroll.offset();
        let scroll_x = (-f32::from(offset.x)).max(0.0);
        let scroll_y = (-f32::from(offset.y)).max(0.0);
        let Some(document) = self.document() else {
            return None;
        };
        let current_page = document.current_page;
        let page_count = document.page_count;
        let zoom_percent = self.zoom_percent;
        if (zoom_percent - 100.0).abs() > f32::EPSILON {
            return None;
        }
        let scale_factor = self.display_scale_factor;
        let states = visible_page_raster_states(
            &self.continuous_page_layouts,
            scroll_x,
            scroll_y,
            viewport_width,
            viewport_height,
            RAIL_WIDTH + SIDEBAR_WIDTH,
            WINDOW_TITLE_BAR_HEIGHT
                + MENU_BAR_HEIGHT
                + DOCUMENT_TAB_BAR_HEIGHT
                + PRIMARY_BAND_HEIGHT,
            |page, logical_width| {
                let ready = document
                    .cached_viewport_image(page, zoom_percent, scale_factor)
                    .is_some();
                let linear_density =
                    document.viewport_pixel_width_for(page, zoom_percent, scale_factor) as f64
                        / f64::from(logical_width.max(1.0));
                (ready, linear_density * linear_density)
            },
        );
        if states.is_empty() {
            return None;
        }
        let visible_pages = states
            .iter()
            .map(|state| {
                let (page_width, page_height) = document.page_size(state.page);
                json!({
                    "page_number": state.page,
                    "page_size_points": { "width": page_width, "height": page_height },
                    "visible_intersection_area_css_px2": state.visible_intersection_area_css_px2,
                    "current_raster_generation": self.continuous_plan_generation,
                    "current_raster_ready_area_fraction": state.current_raster_ready_area_fraction,
                    "current_raster_device_pixels_per_css_pixel": state.current_raster_device_pixels_per_css_pixel,
                    "page_bounds_window_logical": {
                        "x": state.page_bounds_window_logical.x,
                        "y": state.page_bounds_window_logical.y,
                        "width": state.page_bounds_window_logical.width,
                        "height": state.page_bounds_window_logical.height,
                    },
                })
            })
            .collect::<Vec<_>>();
        Some(perf::fields([
            ("command_id", json!(DYNAMIC_FIDELITY_COMMAND_ID)),
            ("zoom_percent", json!(zoom_percent)),
            ("render_generation", json!(self.continuous_plan_generation)),
            ("active_page", json!(current_page)),
            ("page_count", json!(page_count)),
            (
                "scroll_offset_css_px",
                json!({ "x": scroll_x, "y": scroll_y }),
            ),
            (
                "viewport_size_css_px",
                json!({ "width": viewport_width, "height": viewport_height }),
            ),
            (
                "viewport_bounds_window_logical",
                json!({
                    "x": RAIL_WIDTH + SIDEBAR_WIDTH,
                    "y": WINDOW_TITLE_BAR_HEIGHT + MENU_BAR_HEIGHT + DOCUMENT_TAB_BAR_HEIGHT + PRIMARY_BAND_HEIGHT,
                    "width": viewport_width,
                    "height": viewport_height,
                }),
            ),
            ("visible_page_count", json!(visible_pages.len())),
            ("visible_pages", json!(visible_pages)),
        ]))
    }

    fn queue_dynamic_fidelity_state(&mut self, window: &Window) {
        let Some(mut current) = self.capture_dynamic_fidelity_state(window) else {
            return;
        };
        self.dynamic_fidelity_paint_capture_sequence = self
            .dynamic_fidelity_paint_capture_sequence
            .saturating_add(1);
        current.insert(
            "paint_capture_sequence".into(),
            json!(self.dynamic_fidelity_paint_capture_sequence),
        );
        queue_state_for_paint(
            &mut self.dynamic_fidelity_pending_state,
            &mut self.dynamic_fidelity_ready_state,
            current,
        );
    }

    fn attach_dynamic_fidelity_paint_evidence(
        state: &mut Map<String, Value>,
        observations: &HashMap<(u64, usize), DynamicFidelityPaintObservation>,
    ) -> Option<u64> {
        let capture_sequence = state.get("paint_capture_sequence")?.as_u64()?;
        let render_generation = state.get("render_generation")?.as_u64()?;
        let visible_pages = state.get_mut("visible_pages")?.as_array_mut()?;
        for page in visible_pages {
            let page = page.as_object_mut()?;
            let page_number = usize::try_from(page.get("page_number")?.as_u64()?).ok()?;
            let observation = observations.get(&(capture_sequence, page_number))?;
            if observation.capture_sequence != capture_sequence
                || observation.page_number != page_number
                || observation.render_generation != render_generation
            {
                return None;
            }
            let ready_fraction = page.get("current_raster_ready_area_fraction")?.as_f64()?;
            if observation.current_raster_ready != (ready_fraction == 1.0) {
                return None;
            }
            let bounds = observation.outer_bounds_window_logical;
            let pixels_per_point_x = f64::from(bounds.width)
                / f64::from(observation.page_width_points.max(f32::EPSILON));
            let pixels_per_point_y = f64::from(bounds.height)
                / f64::from(observation.page_height_points.max(f32::EPSILON));
            let painted_bounds = json!({
                "x": bounds.x,
                "y": bounds.y,
                "width": bounds.width,
                "height": bounds.height,
            });
            page.insert("page_bounds_window_logical".into(), painted_bounds.clone());
            page.insert(
                "painted_outer_page_bounds_window_logical".into(),
                painted_bounds,
            );
            page.insert(
                "page_size_points".into(),
                json!({
                    "width": observation.page_width_points,
                    "height": observation.page_height_points,
                }),
            );
            page.insert(
                "pixels_per_point".into(),
                json!({ "x": pixels_per_point_x, "y": pixels_per_point_y }),
            );
            page.insert(
                "painted_render_generation".into(),
                json!(observation.render_generation),
            );
            page.insert("painted_generation_current".into(), json!(true));
        }
        state.remove("paint_capture_sequence");
        state.insert("painted_generation_current".into(), json!(true));
        state.insert("platform_draw_submitted".into(), json!(true));
        Some(capture_sequence)
    }

    fn export_dynamic_fidelity_backing_rasters(&mut self) {
        if !self
            .perf_scenario
            .as_ref()
            .is_some_and(|scenario| scenario.kind == PerfScenarioKind::DynamicFidelity)
        {
            return;
        }
        let Some(directory) = std::env::var_os("BP_GPUI_EVIDENCE_DIR").map(PathBuf::from) else {
            return;
        };
        for page in [1, 15, 29] {
            if self.dynamic_fidelity_exported_pages.contains(&page)
                || !self.continuous_visible_pages.contains(&page)
            {
                continue;
            }
            let path = directory.join(format!("dynamic-fidelity-page-{page}-current.png"));
            let result = self.document().and_then(|document| {
                document
                    .export_cached_viewport_png(
                        page,
                        self.zoom_percent,
                        self.display_scale_factor,
                        &path,
                    )
                    .ok()
            });
            let Some((width, height)) = result else {
                continue;
            };
            self.dynamic_fidelity_exported_pages.insert(page);
            perf::emit(
                "dynamic-fidelity-backing-raster-exported",
                perf::fields([
                    ("command_id", json!(DYNAMIC_FIDELITY_COMMAND_ID)),
                    ("page_number", json!(page)),
                    ("path", json!(path)),
                    ("pixel_width", json!(width)),
                    ("pixel_height", json!(height)),
                    ("render_generation", json!(self.continuous_plan_generation)),
                    ("current_raster", json!(true)),
                ]),
            );
        }
    }

    fn emit_painted_dynamic_fidelity_state(&mut self) {
        let Some(mut state) = self.dynamic_fidelity_ready_state.take() else {
            return;
        };
        let capture_sequence = {
            let observations = self
                .dynamic_fidelity_painted_pages
                .lock()
                .expect("dynamic fidelity paint observation lock");
            Self::attach_dynamic_fidelity_paint_evidence(&mut state, &observations)
        };
        let Some(capture_sequence) = capture_sequence else {
            self.dynamic_fidelity_ready_state = Some(state);
            return;
        };
        self.dynamic_fidelity_state_sequence =
            self.dynamic_fidelity_state_sequence.saturating_add(1);
        state.insert(
            "state_sequence".into(),
            json!(self.dynamic_fidelity_state_sequence),
        );
        state.insert(
            "painted_state_sequence".into(),
            json!(self.dynamic_fidelity_state_sequence),
        );
        perf::emit("dynamic-fidelity-state", state);
        self.dynamic_fidelity_painted_pages
            .lock()
            .expect("dynamic fidelity paint observation lock")
            .retain(|(sequence, _), _| *sequence > capture_sequence);
    }

    fn advance_continuous_scroll_comparison(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        phase: ComparisonPhase,
    ) {
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::continuous_scroll)
            .cloned()
        else {
            self.fail_comparison_scenario("continuous-scroll plan is unavailable", cx);
            return;
        };
        match phase {
            ComparisonPhase::Scrolling {
                started_ms,
                last_sample,
                mut raster_observations,
                mut missing_raster_observations,
                mut max_visible_pages,
            } => {
                let now = perf::elapsed_ms();
                let elapsed = (now - started_ms).max(0.0);
                let forward_end = plan.forward_duration_ms as f64;
                let pause_end = forward_end + plan.pause_duration_ms as f64;
                let total_ms = pause_end + plan.reverse_duration_ms as f64;
                let expected_samples = usize::try_from(
                    (plan.forward_duration_ms + plan.pause_duration_ms + plan.reverse_duration_ms)
                        .saturating_mul(u64::from(plan.input_rate_hz))
                        / 1_000
                        + 1,
                )
                .unwrap_or(usize::MAX);
                let current_sample = (((elapsed.min(total_ms) * f64::from(plan.input_rate_hz))
                    / 1_000.0)
                    .floor() as usize
                    + 1)
                .min(expected_samples);
                let viewport_height = (f32::from(window.viewport_size().height)
                    - WINDOW_TITLE_BAR_HEIGHT
                    - MENU_BAR_HEIGHT
                    - DOCUMENT_TAB_BAR_HEIGHT
                    - PRIMARY_BAND_HEIGHT)
                    .max(1.0);
                let maximum_offset = plan.forward_viewport_heights as f32 * viewport_height;
                if current_sample > last_sample {
                    for sample_index in last_sample..current_sample {
                        let sample_ms =
                            sample_index as f64 * 1_000.0 / f64::from(plan.input_rate_hz);
                        let sample_offset = if sample_ms < forward_end {
                            maximum_offset * (sample_ms / forward_end) as f32
                        } else if sample_ms < pause_end {
                            maximum_offset
                        } else if sample_ms < total_ms {
                            maximum_offset
                                * (1.0
                                    - ((sample_ms - pause_end) / plan.reverse_duration_ms as f64)
                                        as f32)
                        } else {
                            0.0
                        };
                        self.continuous_scroll
                            .set_offset(point(px(0.0), px(-sample_offset)));
                    }
                    perf::emit(
                        "comparison-input-batch",
                        perf::fields([
                            ("command_id", json!(plan.command_id)),
                            ("first_sample", json!(last_sample)),
                            ("last_sample", json!(current_sample - 1)),
                            ("coalesced_count", json!(current_sample - last_sample)),
                            (
                                "scheduled_first_ms",
                                json!(last_sample as f64 * 1_000.0 / f64::from(plan.input_rate_hz)),
                            ),
                            (
                                "scheduled_last_ms",
                                json!(
                                    (current_sample - 1) as f64 * 1_000.0
                                        / f64::from(plan.input_rate_hz)
                                ),
                            ),
                        ]),
                    );
                }
                max_visible_pages = max_visible_pages.max(self.continuous_visible_pages.len());
                if let Some(ready) = self.continuous_visible_raster_is_ready() {
                    raster_observations = raster_observations.saturating_add(1);
                    if !ready {
                        missing_raster_observations = missing_raster_observations.saturating_add(1);
                    }
                }
                if elapsed >= total_ms {
                    self.continuous_scroll.set_offset(point(px(0.0), px(0.0)));
                    self.set_page(plan.finish_page, cx);
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.comparison_phase = ComparisonPhase::AwaitingScrollSettle {
                            input_samples: current_sample,
                            expected_samples,
                            native_peak_viewport_heights: None,
                            native_settle_at_ms: None,
                            raster_observations,
                            missing_raster_observations,
                            max_visible_pages,
                        };
                    }
                } else if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::Scrolling {
                        started_ms,
                        last_sample: current_sample,
                        raster_observations,
                        missing_raster_observations,
                        max_visible_pages,
                    };
                }
                window.refresh();
                cx.notify();
            }
            ComparisonPhase::AwaitingScrollSettle {
                input_samples,
                expected_samples,
                native_peak_viewport_heights,
                native_settle_at_ms,
                raster_observations,
                missing_raster_observations,
                max_visible_pages,
            } => {
                let Some(document) = self.document() else {
                    self.fail_comparison_scenario("document closed before scroll settled", cx);
                    return;
                };
                if native_settle_at_ms.is_some_and(|settle_at_ms| perf::elapsed_ms() < settle_at_ms)
                {
                    window.refresh();
                    cx.notify();
                    return;
                }
                if self.pending_viewport_requests.contains_key(&document.id) {
                    if native_settle_at_ms.is_some() {
                        self.fail_comparison_scenario(
                            "native continuous scroll did not settle its viewport request",
                            cx,
                        );
                        return;
                    }
                    window.refresh();
                    cx.notify();
                    return;
                }
                if document.current_page != plan.finish_page {
                    if native_settle_at_ms.is_some() {
                        self.fail_comparison_scenario(
                            format!(
                                "native continuous scroll ended on page {}; expected page {} from native input",
                                document.current_page, plan.finish_page
                            ),
                            cx,
                        );
                        return;
                    }
                    window.refresh();
                    cx.notify();
                    return;
                }
                let page_count = document.page_count;
                let dynamic_fidelity = self
                    .perf_scenario
                    .as_ref()
                    .is_some_and(|scenario| scenario.kind == PerfScenarioKind::DynamicFidelity);
                if dynamic_fidelity {
                    if input_samples != expected_samples
                        || input_samples == 0
                        || !input_samples.is_multiple_of(2)
                    {
                        self.fail_comparison_scenario(
                            format!(
                                "dynamic fidelity native input ended at {input_samples}/{expected_samples} events"
                            ),
                            cx,
                        );
                        return;
                    }
                    let expected_direction_events = expected_samples / 2;
                    if max_visible_pages == 0
                        || max_visible_pages > DYNAMIC_FIDELITY_MAX_VISIBLE_PAGES
                        || max_visible_pages >= page_count
                    {
                        self.fail_comparison_scenario(
                            format!(
                                "dynamic fidelity visible page window was not bounded: max={max_visible_pages}, pages={page_count}"
                            ),
                            cx,
                        );
                        return;
                    }
                    if let Some(peak_viewport_heights) = native_peak_viewport_heights
                        && !native_peak_distance_matches(
                            peak_viewport_heights,
                            plan.forward_viewport_heights as f32,
                        )
                    {
                        self.fail_comparison_scenario(
                            format!(
                                "dynamic fidelity peak was {peak_viewport_heights:.2} viewport heights; expected {}",
                                plan.forward_viewport_heights
                            ),
                            cx,
                        );
                        return;
                    }
                    perf::emit(
                        "dynamic-fidelity-application-evidence",
                        perf::fields([
                            ("command_id", json!(DYNAMIC_FIDELITY_COMMAND_ID)),
                            (
                                "native_forward_event_count",
                                json!(expected_direction_events),
                            ),
                            (
                                "native_reverse_event_count",
                                json!(expected_direction_events),
                            ),
                            ("max_visible_page_count", json!(max_visible_pages)),
                            ("page_count", json!(page_count)),
                            ("finish_page", json!(plan.finish_page)),
                            ("current_page", json!(plan.finish_page)),
                            (
                                "current_render_generation",
                                json!(self.continuous_plan_generation),
                            ),
                            ("raster_observation_count", json!(raster_observations)),
                            (
                                "missing_raster_observation_count",
                                json!(missing_raster_observations),
                            ),
                            ("gpui_platform_draw_submitted", json!(true)),
                            ("physical_scanout_observed", json!(false)),
                        ]),
                    );
                    for milestone in ["virtual-page-window-bounded", "finish-page-current"] {
                        perf::emit(
                            "comparison-milestone",
                            perf::fields([
                                ("command_id", json!(DYNAMIC_FIDELITY_COMMAND_ID)),
                                ("milestone", json!(milestone)),
                            ]),
                        );
                    }
                    self.await_dynamic_runner_result(cx);
                    return;
                }
                let result = (|| -> Result<(), String> {
                    if input_samples != expected_samples {
                        return Err(format!(
                            "continuous input stream ended at {input_samples}/{expected_samples} samples"
                        ));
                    }
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "timestamped-input-complete",
                    )?;
                    if let Some(peak_viewport_heights) = native_peak_viewport_heights {
                        let expected_peak = plan.forward_viewport_heights as f32;
                        let tolerance = (expected_peak * 0.05).max(0.5);
                        if !native_peak_distance_matches(peak_viewport_heights, expected_peak) {
                            return Err(format!(
                                "native continuous scroll peak was {peak_viewport_heights:.2} viewport heights; expected {} +/- {tolerance:.2}",
                                plan.forward_viewport_heights
                            ));
                        }
                    }
                    if max_visible_pages == 0
                        || max_visible_pages > 8
                        || max_visible_pages >= page_count
                    {
                        return Err(format!(
                            "continuous virtual page window was not bounded: max={max_visible_pages}, pages={page_count}"
                        ));
                    }
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "virtual-page-window-bounded",
                    )?;
                    self.record_comparison_milestone(&plan.command_id, "finish-page-current")?;
                    if plan
                        .expected_milestones
                        .iter()
                        .any(|milestone| milestone == V4_VISIBLE_RASTER_READINESS_MILESTONE)
                    {
                        if raster_observations == 0 {
                            return Err(
                                "continuous scroll did not observe visible raster readiness".into(),
                            );
                        }
                        let ready_observations =
                            raster_observations.saturating_sub(missing_raster_observations);
                        let readiness_rate = ready_observations as f64 / raster_observations as f64;
                        perf::emit(
                            "comparison-raster-readiness-observed",
                            perf::fields([
                                ("command_id", json!(plan.command_id)),
                                ("raster_observation_count", json!(raster_observations)),
                                (
                                    "missing_raster_observation_count",
                                    json!(missing_raster_observations),
                                ),
                                ("ready_raster_observation_count", json!(ready_observations)),
                                ("readiness_rate", json!(readiness_rate)),
                                ("acceptance_role", json!("diagnostic-counts-and-rate")),
                            ]),
                        );
                        self.record_comparison_milestone(
                            &plan.command_id,
                            V4_VISIBLE_RASTER_READINESS_MILESTONE,
                        )?;
                    } else {
                        if missing_raster_observations != 0 {
                            return Err(format!(
                                "continuous scroll presented {missing_raster_observations} blank current-generation frames"
                            ));
                        }
                        self.record_comparison_milestone(
                            &plan.command_id,
                            "blank-current-generation-frames-zero",
                        )?;
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    self.fail_comparison_scenario(error, cx);
                    return;
                }
                self.complete_comparison_if_ready(cx);
            }
            _ => {}
        }
    }

    fn advance_viewer_settle(
        &mut self,
        operation: ViewerOperation,
        ready_at_ms: f64,
        cx: &mut Context<Self>,
    ) {
        if perf::elapsed_ms() < ready_at_ms
            || !self.pending_viewport_requests.is_empty()
            || !self.pending_tile_requests.is_empty()
            || self.active_tile_jobs > 0
        {
            cx.notify();
            return;
        }
        let result = (|| -> Result<(), String> {
            match operation {
                ViewerOperation::Navigation => {
                    let plan = self
                        .perf_scenario
                        .as_ref()
                        .and_then(|scenario| scenario.comparison_plan.as_ref())
                        .and_then(ComparisonScenarioPlan::page_navigation)
                        .cloned()
                        .ok_or_else(|| "navigation plan is unavailable".to_string())?;
                    let document = self
                        .document()
                        .ok_or_else(|| "navigation document closed".to_string())?;
                    let completed_step = self
                        .perf_scenario
                        .as_ref()
                        .map(|scenario| scenario.step_index.saturating_sub(1))
                        .unwrap_or_default();
                    let expected_page = perf_page_sequence(document.page_count)
                        .get(completed_step)
                        .copied()
                        .ok_or_else(|| {
                            "navigation step exceeds the manifest sequence".to_string()
                        })?;
                    if document.current_page != expected_page {
                        return Err(format!(
                            "navigation current page {} does not match {expected_page}",
                            document.current_page
                        ));
                    }
                    if document.viewport_image().is_none() {
                        return Err("navigation preview is not current".into());
                    }
                    self.record_comparison_milestone(&plan.command_id, "target-page-current")?;
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "preview-current-generation",
                    )?;
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "settled-current-generation-250ms",
                    )?;
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        finish_viewer_settle(scenario);
                    }
                    self.start_next_perf_operation(cx);
                }
                ViewerOperation::Zoom => {
                    let plan = self
                        .perf_scenario
                        .as_ref()
                        .and_then(|scenario| scenario.comparison_plan.as_ref())
                        .and_then(ComparisonScenarioPlan::zoom)
                        .cloned()
                        .ok_or_else(|| "zoom plan is unavailable".to_string())?;
                    let completed_step = self
                        .perf_scenario
                        .as_ref()
                        .map(|scenario| scenario.step_index.saturating_sub(1))
                        .unwrap_or_default();
                    let expected_zoom = plan
                        .percent
                        .get(completed_step)
                        .copied()
                        .ok_or_else(|| "zoom step exceeds the manifest sequence".to_string())?;
                    if (self.zoom_percent - expected_zoom as f32).abs() > 0.1 {
                        return Err(format!(
                            "zoom state {} does not match {expected_zoom}",
                            self.zoom_percent
                        ));
                    }
                    if self.visible_tile_requests.len() > CachePolicy::default().max_tiles_per_plan
                    {
                        return Err("zoom tile plan is not bounded".into());
                    }
                    let document = self
                        .document()
                        .ok_or_else(|| "zoom document closed".to_string())?;
                    if document.viewport_image().is_none() {
                        return Err("zoom preview is not current".into());
                    }
                    let tiles_current = !self.visible_tile_requests.is_empty()
                        && self
                            .visible_tile_requests
                            .iter()
                            .all(|request| self.tile_cache.contains(*request));
                    let stale_visible_generation_count = self
                        .visible_tile_requests
                        .iter()
                        .filter(|request| !self.render_planner.accepts(request.generation))
                        .count();
                    if stale_visible_generation_count != 0 {
                        return Err(format!(
                            "zoom presented {stale_visible_generation_count} stale visible tile generations"
                        ));
                    }
                    if !tiles_current
                        && document
                            .viewport_render_density(self.zoom_percent, self.display_scale_factor)
                            < 1.0
                    {
                        return Err(
                            "zoom settled below device density without current tiles".into()
                        );
                    }
                    self.record_comparison_milestone(&plan.command_id, "zoom-state-current")?;
                    self.record_comparison_milestone(&plan.command_id, "visible-tiles-bounded")?;
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "preview-current-generation",
                    )?;
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "settled-density-at-least-1",
                    )?;
                    perf::emit(
                        "comparison-v4-zoom-generation-evidence",
                        perf::fields([
                            ("command_id", json!(&plan.command_id)),
                            ("step_index", json!(completed_step)),
                            ("zoom_percent", json!(expected_zoom)),
                            (
                                "visible_tile_count",
                                json!(self.visible_tile_requests.len()),
                            ),
                            (
                                "maximum_visible_tiles",
                                json!(CachePolicy::default().max_tiles_per_plan),
                            ),
                            (
                                "stale_visible_generation_count",
                                json!(stale_visible_generation_count),
                            ),
                        ]),
                    );
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        finish_viewer_settle(scenario);
                    }
                    self.start_next_perf_operation(cx);
                }
                ViewerOperation::HighZoomPanPrime => {
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.comparison_phase = ComparisonPhase::Panning {
                            started_ms: perf::elapsed_ms(),
                            last_sample: 0,
                            max_tiles: 0,
                        };
                    }
                    cx.notify();
                }
                ViewerOperation::FitPage {
                    expected_zoom_percent,
                }
                | ViewerOperation::FitWidth {
                    expected_zoom_percent,
                } => {
                    let mode = match operation {
                        ViewerOperation::FitPage { .. } => FitMode::FitPage,
                        ViewerOperation::FitWidth { .. } => FitMode::FitWidth,
                        _ => unreachable!(),
                    };
                    let (plan, shell_size) = self
                        .perf_scenario
                        .as_ref()
                        .and_then(|scenario| {
                            Some((
                                scenario.engineering_fit_plan.clone()?,
                                scenario.engineering_fit_shell_size?,
                            ))
                        })
                        .ok_or_else(|| "Fit modes plan or shell size is unavailable".to_string())?;
                    let tiles_current = self
                        .visible_tile_requests
                        .iter()
                        .all(|request| self.tile_cache.contains(*request));
                    let (preview_current, preview_density) = self
                        .document()
                        .map(|document| {
                            (
                                document.viewport_image().is_some(),
                                document.viewport_render_density(
                                    self.zoom_percent,
                                    self.display_scale_factor,
                                ),
                            )
                        })
                        .ok_or_else(|| "Fit modes document closed before settle".to_string())?;
                    let settled_density = if !self.visible_tile_requests.is_empty() && tiles_current
                    {
                        1.0
                    } else {
                        preview_density
                    };
                    let preset_current = matches!(
                        (mode, self.zoom_preset),
                        (FitMode::FitPage, ZoomPreset::FitPage)
                            | (FitMode::FitWidth, ZoomPreset::FitWidth)
                    );
                    let observation = FitModeObservation {
                        mode,
                        shell_width: shell_size[0],
                        shell_height: shell_size[1],
                        client_width: shell_size[2],
                        client_height: shell_size[3],
                        expected_zoom_percent,
                        applied_zoom_percent: self.zoom_percent,
                        preset_current,
                        current_generation_presented: preview_current
                            && (self.visible_tile_requests.is_empty() || tiles_current),
                        settled_for_ms: (perf::elapsed_ms() - (ready_at_ms - 250.0)).max(0.0),
                        visible_tile_count: self.visible_tile_requests.len(),
                        maximum_visible_tiles: CachePolicy::default().max_tiles_per_plan,
                        settled_density,
                    };
                    let receipt = assess_fit_mode(&plan, observation)?;
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario
                            .engineering_fit_observations
                            .push(receipt.observation);
                    }
                    let complete = self.perf_scenario.as_ref().is_some_and(|scenario| {
                        scenario.engineering_fit_observations.len() == plan.modes.len()
                    });
                    if complete {
                        let observations = self
                            .perf_scenario
                            .as_ref()
                            .map(|scenario| scenario.engineering_fit_observations.clone())
                            .unwrap_or_default();
                        perf::emit(
                            "comparison-v4-command-receipt",
                            perf::fields([
                                ("command_id", json!(&plan.command_id)),
                                ("component_scenario", json!("fit-modes")),
                                ("passed", json!(true)),
                                ("observations", json!(observations)),
                                ("milestone_ids", json!(&plan.expected_milestones)),
                            ]),
                        );
                        for milestone in &plan.expected_milestones {
                            self.record_comparison_milestone(&plan.command_id, milestone)?;
                        }
                        self.complete_comparison_if_ready(cx);
                    } else {
                        self.start_next_perf_operation(cx);
                    }
                }
                ViewerOperation::Reopen => {
                    let plan = self
                        .perf_scenario
                        .as_ref()
                        .and_then(|scenario| scenario.comparison_plan.as_ref())
                        .and_then(ComparisonScenarioPlan::close_reopen)
                        .cloned()
                        .ok_or_else(|| "close-reopen plan is unavailable".to_string())?;
                    if self
                        .document()
                        .and_then(PdfDocument::viewport_image)
                        .is_none()
                    {
                        return Err("reopened document has no current viewport".into());
                    }
                    self.record_comparison_milestone(&plan.command_id, "document-reopened")?;
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "settled-current-generation-250ms",
                    )?;
                    self.complete_comparison_if_ready(cx);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error, cx);
        }
    }

    fn advance_high_zoom_pan(
        &mut self,
        window: &mut Window,
        started_ms: f64,
        last_sample: usize,
        mut max_tiles: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::high_zoom_pan)
            .cloned()
        else {
            self.fail_comparison_scenario("high-zoom-pan plan is unavailable", cx);
            return;
        };
        let elapsed = (perf::elapsed_ms() - started_ms).max(0.0);
        let expected_samples =
            usize::try_from(plan.duration_ms.saturating_mul(u64::from(plan.rate_hz)) / 1_000 + 1)
                .unwrap_or(usize::MAX);
        let current_sample = (((elapsed.min(plan.duration_ms as f64) * f64::from(plan.rate_hz))
            / 1_000.0)
            .floor() as usize
            + 1)
        .min(expected_samples);
        if current_sample > last_sample {
            let viewport = window.viewport_size();
            let viewport_width = f32::from(viewport.width);
            let viewport_height = f32::from(viewport.height);
            let (page_width, page_height) = self
                .document()
                .map(|document| document.page_size(1))
                .unwrap_or((1.0, 1.0));
            let page_width = page_width * plan.zoom_percent as f32 / 100.0;
            let page_height = page_height * plan.zoom_percent as f32 / 100.0;
            for sample_index in last_sample..current_sample {
                let fraction = if expected_samples <= 1 {
                    0.0
                } else {
                    sample_index as f64 / (expected_samples - 1) as f64
                };
                let segments = plan.normalized_viewport_points.len().saturating_sub(1);
                let global = fraction * segments as f64;
                let segment = (global.floor() as usize).min(segments.saturating_sub(1));
                let local = (global - segment as f64) as f32;
                let start = plan.normalized_viewport_points[segment];
                let finish = plan.normalized_viewport_points[segment + 1];
                let nx = start[0] as f32 + (finish[0] as f32 - start[0] as f32) * local;
                let ny = start[1] as f32 + (finish[1] as f32 - start[1] as f32) * local;
                let x = (nx * page_width - viewport_width / 2.0)
                    .clamp(0.0, (page_width - viewport_width).max(0.0));
                let y = (ny * page_height - viewport_height / 2.0)
                    .clamp(0.0, (page_height - viewport_height).max(0.0));
                self.document_scroll.set_offset(point(px(-x), px(-y)));
                self.tile_plan_key = None;
            }
        }
        max_tiles = max_tiles.max(self.visible_tile_requests.len());
        if elapsed < plan.duration_ms as f64 {
            if let Some(scenario) = self.perf_scenario.as_mut() {
                scenario.comparison_phase = ComparisonPhase::Panning {
                    started_ms,
                    last_sample: current_sample,
                    max_tiles,
                };
            }
            window.refresh();
            cx.notify();
            return;
        }
        if !self.pending_tile_requests.is_empty() || self.active_tile_jobs > 0 {
            window.refresh();
            cx.notify();
            return;
        }
        if current_sample != expected_samples {
            self.fail_comparison_scenario(
                format!("high-zoom pan input ended at {current_sample}/{expected_samples} samples"),
                cx,
            );
            return;
        }
        perf::emit(
            "comparison-v4-pan-input-evidence",
            perf::fields([
                ("command_id", json!(&plan.command_id)),
                ("timestamped_input_complete", json!(true)),
                ("input_samples", json!(current_sample)),
                ("expected_input_samples", json!(expected_samples)),
                ("input_rate_hz", json!(plan.rate_hz)),
                ("duration_ms", json!(plan.duration_ms)),
            ]),
        );
        let result = (|| -> Result<(), String> {
            if max_tiles == 0 || max_tiles > CachePolicy::default().max_tiles_per_plan {
                return Err(format!("high-zoom pan tile window was {max_tiles}"));
            }
            if self
                .visible_tile_requests
                .iter()
                .any(|request| !self.render_planner.accepts(request.generation))
            {
                return Err("stale tile generation reached the visible request set".into());
            }
            if self.visible_tile_requests.is_empty()
                || self
                    .visible_tile_requests
                    .iter()
                    .any(|request| !self.tile_cache.contains(*request))
            {
                return Err("high-zoom pan did not settle every visible tile".into());
            }
            self.record_comparison_milestone(&plan.command_id, "visible-tiles-bounded")?;
            self.record_comparison_milestone(&plan.command_id, "stale-generations-presented-zero")?;
            self.record_comparison_milestone(&plan.command_id, "settled-density-at-least-1")?;
            Ok(())
        })();
        if let Err(error) = result {
            self.fail_comparison_scenario(error, cx);
        } else {
            self.complete_comparison_if_ready(cx);
        }
    }

    fn advance_close_reopen(&mut self, stage: CloseReopenStage, cx: &mut Context<Self>) {
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::close_reopen)
            .cloned()
        else {
            self.fail_comparison_scenario("close-reopen plan is unavailable", cx);
            return;
        };
        match stage {
            CloseReopenStage::Start => {
                if !self.active_viewport_jobs.is_empty()
                    || self.active_thumbnail_jobs > 0
                    || self.active_page_surface_jobs > 0
                    || self.active_tile_jobs > 0
                    || !self.pending_viewport_requests.is_empty()
                    || !self.thumbnail_queue.is_empty()
                    || !self.page_surface_queue.is_empty()
                    || !self.tile_queue.is_empty()
                {
                    cx.notify();
                    return;
                }
                let before = self.tile_cache.bytes()
                    + self
                        .document()
                        .map(PdfDocument::cached_image_bytes)
                        .unwrap_or(0);
                if self.documents.is_empty() {
                    self.fail_comparison_scenario("close-reopen has no open document", cx);
                    return;
                }
                let reopen_fixture_id = self
                    .document()
                    .and_then(|document| self.document_fixture_ids.get(&document.id))
                    .cloned();
                self.close_document(self.active_document.unwrap_or(0), cx);
                if !self.documents.is_empty() || !self.tile_cache.is_empty() {
                    self.fail_comparison_scenario(
                        "document resources were retained after close",
                        cx,
                    );
                    return;
                }
                let result = (|| -> Result<(), String> {
                    self.record_comparison_milestone(
                        &plan.command_id,
                        "document-resources-released",
                    )?;
                    perf::emit(
                        "comparison-memory-recovery",
                        perf::fields([("released_render_bytes", json!(before))]),
                    );
                    self.record_comparison_milestone(&plan.command_id, "memory-recovery-recorded")?;
                    Ok(())
                })();
                if let Err(error) = result {
                    self.fail_comparison_scenario(error, cx);
                    return;
                }
                let Some(path) = self.perf_reopen_path.clone() else {
                    self.fail_comparison_scenario("close-reopen path was not retained", cx);
                    return;
                };
                self.open_path_with_fixture(path, reopen_fixture_id.as_deref(), cx);
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::CloseReopen {
                        stage: CloseReopenStage::AwaitingReopen,
                    };
                }
            }
            CloseReopenStage::AwaitingReopen => {
                if self.pending_open_requests.is_empty()
                    && self.pending_viewport_requests.is_empty()
                    && self
                        .document()
                        .and_then(PdfDocument::viewport_image)
                        .is_some()
                    && let Some(scenario) = self.perf_scenario.as_mut()
                {
                    scenario.comparison_phase = ComparisonPhase::AwaitingViewerSettle {
                        operation: ViewerOperation::Reopen,
                        ready_at_ms: perf::elapsed_ms() + 250.0,
                    };
                }
                cx.notify();
            }
        }
    }

    fn advance_multi_document_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((stage, input_lane)) = self.perf_scenario.as_ref().and_then(|scenario| {
            Some((scenario.multi_document.as_ref()?.stage, scenario.input_lane))
        }) else {
            return false;
        };
        match stage {
            MultiDocumentStage::Opening => {
                let Some((requested, observed)) = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.multi_document.as_ref())
                    .map(|state| (state.open_requested, state.open_observations.len()))
                else {
                    return true;
                };
                if requested == 0
                    || observed >= requested
                    || !self.pending_open_requests.is_empty()
                    || !self.pending_viewport_requests.is_empty()
                    || !self.active_viewport_jobs.is_empty()
                {
                    return true;
                }
                let Some(document) = self.document() else {
                    self.fail_comparison_scenario("opened fixture has no active document", cx);
                    return true;
                };
                let observation = DocumentResourceObservation {
                    fixture_id: MULTI_FIXTURE_IDS[observed].into(),
                    document_id: document.id,
                    page_count: document.page_count,
                    decoded_page_bytes: document.cached_image_bytes(),
                    current_raster: document.viewport_image().is_some(),
                    annotation_count: self
                        .annotation_adapter
                        .snapshot(document.id)
                        .map_or(0, |snapshot| snapshot.rectangles.len()),
                    dense_page_annotation_count: self
                        .annotation_adapter
                        .document_scene(document.id, 1)
                        .rectangles
                        .len(),
                };
                if !observation.current_raster || observation.decoded_page_bytes == 0 {
                    window.refresh();
                    cx.notify();
                    return true;
                }
                perf::emit(
                    "multi-document-open-raster-observed",
                    perf::fields([
                        ("fixture_id", json!(observation.fixture_id)),
                        ("document_id", json!(observation.document_id)),
                        ("page_count", json!(observation.page_count)),
                        ("decoded_page_bytes", json!(observation.decoded_page_bytes)),
                        ("current_raster", json!(observation.current_raster)),
                        ("annotation_count", json!(observation.annotation_count)),
                        (
                            "dense_page_annotation_count",
                            json!(observation.dense_page_annotation_count),
                        ),
                        ("process_id", json!(std::process::id())),
                    ]),
                );
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.multi_document.as_mut())
                {
                    state.open_observations.push(observation);
                }
                if observed + 1 < MULTI_FIXTURE_IDS.len() {
                    self.open_next_multi_document(cx);
                    return true;
                }
                let observations = &self
                    .perf_scenario
                    .as_ref()
                    .unwrap()
                    .multi_document
                    .as_ref()
                    .unwrap()
                    .open_observations;
                if let Err(error) = validate_open_observations(observations) {
                    self.fail_comparison_scenario(error, cx);
                    return true;
                }
                let stage_started_ms = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.multi_document.as_ref())
                    .map(|state| state.stage_started_ms)
                    .unwrap_or_default();
                perf::emit(
                    "multi-document-command-evidence",
                    perf::fields([
                        ("command_id", json!(MULTI_OPEN_COMMAND_ID)),
                        ("process_id", json!(std::process::id())),
                        ("documents", json!(observations)),
                    ]),
                );
                Self::emit_multi_operation_visible(MULTI_OPEN_COMMAND_ID, stage_started_ms);
                Self::record_multi_milestones(MULTI_OPEN_COMMAND_ID, &MULTI_OPEN_MILESTONES);
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.multi_document.as_mut())
                {
                    state.stage = MultiDocumentStage::Switching;
                    state.stage_started_ms = perf::elapsed_ms();
                }
                cx.notify();
            }
            MultiDocumentStage::Switching => {
                let Some((index, ready, started)) = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.multi_document.as_ref())
                    .map(|state| {
                        (
                            state.switch_index,
                            state.native_ready_emitted,
                            state.native_action_started_ms,
                        )
                    })
                else {
                    return true;
                };
                if index >= MULTI_SWITCH_SEQUENCE.len() {
                    return true;
                }
                let target_fixture = MULTI_SWITCH_SEQUENCE[index];
                if input_lane == PerfInputLane::SemanticDiagnostic && !ready {
                    let Some(document_id) = self.multi_document_id_for_fixture(target_fixture)
                    else {
                        self.fail_comparison_scenario("switch target document is missing", cx);
                        return true;
                    };
                    let Some(document_index) = self.document_index_by_id(document_id) else {
                        self.fail_comparison_scenario("switch target tab is missing", cx);
                        return true;
                    };
                    if let Some(state) = self
                        .perf_scenario
                        .as_mut()
                        .and_then(|scenario| scenario.multi_document.as_mut())
                    {
                        state.native_ready_emitted = true;
                        state.native_action_started_ms = Some(perf::elapsed_ms());
                    }
                    self.select_document(document_index, cx);
                    return true;
                }
                if started.is_none()
                    || self.active_multi_fixture_id() != Some(target_fixture)
                    || !self.pending_viewport_requests.is_empty()
                    || self
                        .document()
                        .and_then(PdfDocument::viewport_image)
                        .is_none()
                {
                    return true;
                }
                #[cfg(feature = "benchmark-evidence")]
                if input_lane == PerfInputLane::NativeX11Xtest {
                    let before = self
                        .perf_scenario
                        .as_ref()
                        .and_then(|scenario| scenario.multi_document.as_ref())
                        .and_then(|state| state.native_input_samples_before)
                        .unwrap_or(u64::MAX);
                    let snapshot = window.input_latency_snapshot();
                    let after = snapshot.latency_histogram.len();
                    if after <= before {
                        window.refresh();
                        cx.notify();
                        return true;
                    }
                    perf::emit(
                        "multi-document-native-frame-evidence",
                        perf::fields([
                            ("command_id", json!(MULTI_SWITCH_COMMAND_ID)),
                            ("action_index", json!(index)),
                            ("fixture_id", json!(target_fixture)),
                            ("input_latency_samples_before", json!(before)),
                            ("input_latency_samples_after", json!(after)),
                            (
                                "input_to_application_draw_ack_p95_ns",
                                json!(snapshot.latency_histogram.value_at_quantile(0.95)),
                            ),
                            ("gpui_platform_draw_submitted", json!(true)),
                            ("physical_scanout_observed", json!(false)),
                        ]),
                    );
                }
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.multi_document.as_mut())
                {
                    state.switch_observations.push(target_fixture.into());
                    state.switch_rasters.push(true);
                    state.switch_index += 1;
                    state.native_ready_emitted = false;
                    state.native_input_samples_before = None;
                    state.native_action_started_ms = None;
                }
                if index + 1 == MULTI_SWITCH_SEQUENCE.len() {
                    let state = self
                        .perf_scenario
                        .as_ref()
                        .unwrap()
                        .multi_document
                        .as_ref()
                        .unwrap();
                    let other_states_unchanged =
                        state.open_observations.iter().all(|observation| {
                            observation.fixture_id == DENSE_FIXTURE_ID
                                || (self
                                    .annotation_adapter
                                    .history_depths(observation.document_id)
                                    == (0, 0)
                                    && !self.annotation_adapter.is_dirty(observation.document_id))
                        });
                    if let Err(error) = validate_switch_observations(
                        &state.switch_observations,
                        &state.switch_rasters,
                        other_states_unchanged,
                    ) {
                        self.fail_comparison_scenario(error, cx);
                        return true;
                    }
                    let stage_started_ms = state.stage_started_ms;
                    perf::emit(
                        "multi-document-command-evidence",
                        perf::fields([
                            ("command_id", json!(MULTI_SWITCH_COMMAND_ID)),
                            ("process_id", json!(std::process::id())),
                            ("switch_sequence", json!(state.switch_observations)),
                            (
                                "native_input",
                                json!(input_lane == PerfInputLane::NativeX11Xtest),
                            ),
                        ]),
                    );
                    Self::emit_multi_operation_visible(MULTI_SWITCH_COMMAND_ID, stage_started_ms);
                    Self::record_multi_milestones(
                        MULTI_SWITCH_COMMAND_ID,
                        &MULTI_SWITCH_MILESTONES,
                    );
                    if let Err(error) = self.setup_multi_dense_rectangle(cx) {
                        self.fail_comparison_scenario(error, cx);
                        return true;
                    }
                    if let Some(state) = self
                        .perf_scenario
                        .as_mut()
                        .and_then(|scenario| scenario.multi_document.as_mut())
                    {
                        state.stage = MultiDocumentStage::Editing;
                        state.stage_started_ms = perf::elapsed_ms();
                    }
                }
                cx.notify();
            }
            MultiDocumentStage::Editing => {
                let Some((ready, committed, started)) = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.multi_document.as_ref())
                    .map(|state| {
                        (
                            state.native_ready_emitted,
                            state.edit_committed,
                            state.native_action_started_ms,
                        )
                    })
                else {
                    return true;
                };
                if input_lane == PerfInputLane::SemanticDiagnostic && !ready {
                    if let Some(state) = self
                        .perf_scenario
                        .as_mut()
                        .and_then(|scenario| scenario.multi_document.as_mut())
                    {
                        state.native_ready_emitted = true;
                    }
                    self.apply_selected_rectangle_stroke_width(4.0, cx);
                    return true;
                }
                if !committed || started.is_none() {
                    return true;
                }
                #[cfg(feature = "benchmark-evidence")]
                if input_lane == PerfInputLane::NativeX11Xtest {
                    let before = self
                        .perf_scenario
                        .as_ref()
                        .and_then(|scenario| scenario.multi_document.as_ref())
                        .and_then(|state| state.native_input_samples_before)
                        .unwrap_or(u64::MAX);
                    let snapshot = window.input_latency_snapshot();
                    let after = snapshot.latency_histogram.len();
                    if after <= before {
                        window.refresh();
                        cx.notify();
                        return true;
                    }
                    perf::emit(
                        "multi-document-native-frame-evidence",
                        perf::fields([
                            ("command_id", json!(MULTI_EDIT_COMMAND_ID)),
                            ("action_index", json!(0)),
                            ("fixture_id", json!(DENSE_FIXTURE_ID)),
                            ("input_latency_samples_before", json!(before)),
                            ("input_latency_samples_after", json!(after)),
                            (
                                "input_to_application_draw_ack_p95_ns",
                                json!(snapshot.latency_histogram.value_at_quantile(0.95)),
                            ),
                            ("gpui_platform_draw_submitted", json!(true)),
                            ("physical_scanout_observed", json!(false)),
                        ]),
                    );
                }
                let Some(document_id) = self.multi_document_id_for_fixture(DENSE_FIXTURE_ID) else {
                    self.fail_comparison_scenario("dense document disappeared after edit", cx);
                    return true;
                };
                let scene = self.annotation_adapter.document_scene(document_id, 0);
                let Some(rectangle) = scene
                    .rectangles
                    .iter()
                    .find(|rectangle| rectangle.id.as_str() == DENSE_RECTANGLE_ID)
                else {
                    self.fail_comparison_scenario(
                        "dense rectangle is missing after native edit",
                        cx,
                    );
                    return true;
                };
                let other_states_unchanged = self
                    .perf_scenario
                    .as_ref()
                    .unwrap()
                    .multi_document
                    .as_ref()
                    .unwrap()
                    .open_observations
                    .iter()
                    .filter(|observation| observation.document_id != document_id)
                    .all(|observation| {
                        self.annotation_adapter
                            .history_depths(observation.document_id)
                            == (0, 0)
                            && !self.annotation_adapter.is_dirty(observation.document_id)
                    });
                let thumbnail_current = self
                    .annotation_adapter
                    .thumbnail_scene(document_id, 0)
                    .rectangles
                    .iter()
                    .any(|candidate| {
                        candidate.id.as_str() == DENSE_RECTANGLE_ID
                            && (candidate.appearance.stroke_width_pt() - 4.0).abs() <= f64::EPSILON
                    });
                if (rectangle.appearance.stroke_width_pt() - 4.0).abs() > f64::EPSILON
                    || self.annotation_adapter.history_depths(document_id) != (2, 0)
                    || !self.annotation_adapter.is_dirty(document_id)
                    || !other_states_unchanged
                    || !thumbnail_current
                {
                    self.fail_comparison_scenario(
                        "dense rectangle native property state or isolation proof failed",
                        cx,
                    );
                    return true;
                }
                perf::emit(
                    "multi-document-command-evidence",
                    perf::fields([
                        ("command_id", json!(MULTI_EDIT_COMMAND_ID)),
                        ("process_id", json!(std::process::id())),
                        ("document_id", json!(document_id)),
                        ("annotation_id", json!(DENSE_RECTANGLE_ID)),
                        ("stroke_width_points", json!(4.0)),
                        ("history_depth", json!(2)),
                        ("dirty", json!(true)),
                        ("other_document_states_unchanged", json!(true)),
                        ("thumbnail_current", json!(true)),
                    ]),
                );
                let stage_started_ms = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.multi_document.as_ref())
                    .map(|state| state.stage_started_ms)
                    .unwrap_or_default();
                Self::emit_multi_operation_visible(MULTI_EDIT_COMMAND_ID, stage_started_ms);
                Self::record_multi_milestones(MULTI_EDIT_COMMAND_ID, &MULTI_EDIT_MILESTONES);
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.multi_document.as_mut())
                {
                    state.stage = MultiDocumentStage::Closing;
                    state.stage_started_ms = perf::elapsed_ms();
                }
                cx.notify();
            }
            MultiDocumentStage::Closing => {
                let closed = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.multi_document.as_ref())
                    .map(|state| state.close_observations.len())
                    .unwrap_or_default();
                if closed < MULTI_CLOSE_SEQUENCE.len() {
                    let fixture_id = MULTI_CLOSE_SEQUENCE[closed];
                    let Some(document_id) = self.multi_document_id_for_fixture(fixture_id) else {
                        self.fail_comparison_scenario("close target document is missing", cx);
                        return true;
                    };
                    let Some(index) = self.document_index_by_id(document_id) else {
                        self.fail_comparison_scenario("close target tab is missing", cx);
                        return true;
                    };
                    let released_decoded_page_bytes = self.documents[index].cached_image_bytes();
                    self.close_document(index, cx);
                    let render_requests_removed =
                        !self.pending_viewport_requests.contains_key(&document_id)
                            && !self
                                .pending_thumbnail_requests
                                .keys()
                                .any(|(candidate, _)| *candidate == document_id)
                            && !self
                                .pending_page_surface_requests
                                .keys()
                                .any(|(candidate, _, _)| *candidate == document_id)
                            && !self
                                .pending_tile_requests
                                .iter()
                                .any(|request| request.source.document_id == document_id);
                    let observation = ClosedDocumentObservation {
                        fixture_id: fixture_id.into(),
                        document_id,
                        released_decoded_page_bytes,
                        document_removed: self.document_index_by_id(document_id).is_none(),
                        render_requests_removed,
                        annotation_state_removed: self
                            .annotation_adapter
                            .snapshot(document_id)
                            .is_none(),
                    };
                    perf::emit(
                        "multi-document-resource-release-observed",
                        perf::fields([
                            ("fixture_id", json!(fixture_id)),
                            ("document_id", json!(document_id)),
                            (
                                "released_decoded_page_bytes",
                                json!(released_decoded_page_bytes),
                            ),
                            ("document_removed", json!(observation.document_removed)),
                            (
                                "render_requests_removed",
                                json!(observation.render_requests_removed),
                            ),
                            (
                                "annotation_state_removed",
                                json!(observation.annotation_state_removed),
                            ),
                        ]),
                    );
                    if let Some(state) = self
                        .perf_scenario
                        .as_mut()
                        .and_then(|scenario| scenario.multi_document.as_mut())
                    {
                        state.close_observations.push(observation);
                    }
                    cx.notify();
                    return true;
                }
                let Some(dense_document_id) = self.multi_document_id_for_fixture(DENSE_FIXTURE_ID)
                else {
                    self.fail_comparison_scenario(
                        "remaining dense document identity is missing",
                        cx,
                    );
                    return true;
                };
                let scene = self.annotation_adapter.document_scene(dense_document_id, 0);
                let width = scene
                    .rectangles
                    .iter()
                    .find(|rectangle| rectangle.id.as_str() == DENSE_RECTANGLE_ID)
                    .map(|rectangle| rectangle.appearance.stroke_width_pt())
                    .unwrap_or(f64::NAN);
                let observations = &self
                    .perf_scenario
                    .as_ref()
                    .unwrap()
                    .multi_document
                    .as_ref()
                    .unwrap()
                    .close_observations;
                if let Err(error) = validate_closed_observations(
                    observations,
                    DENSE_FIXTURE_ID,
                    self.documents.len(),
                    self.document().map(|document| document.id) == Some(dense_document_id),
                    width,
                ) {
                    self.fail_comparison_scenario(error, cx);
                    return true;
                }
                let released_bytes = observations
                    .iter()
                    .map(|observation| observation.released_decoded_page_bytes)
                    .sum::<usize>();
                let stage_started_ms = self
                    .perf_scenario
                    .as_ref()
                    .and_then(|scenario| scenario.multi_document.as_ref())
                    .map(|state| state.stage_started_ms)
                    .unwrap_or_default();
                perf::emit(
                    "multi-document-command-evidence",
                    perf::fields([
                        ("command_id", json!(MULTI_CLOSE_COMMAND_ID)),
                        ("process_id", json!(std::process::id())),
                        ("closed_documents", json!(observations)),
                        ("released_decoded_page_bytes", json!(released_bytes)),
                        ("remaining_fixture_id", json!(DENSE_FIXTURE_ID)),
                        ("remaining_document_id", json!(dense_document_id)),
                        ("remaining_document_count", json!(self.documents.len())),
                        ("dense_rectangle_stroke_width_points", json!(width)),
                        ("interactive_document_shell", json!(true)),
                    ]),
                );
                Self::emit_multi_operation_visible(MULTI_CLOSE_COMMAND_ID, stage_started_ms);
                Self::record_multi_milestones(MULTI_CLOSE_COMMAND_ID, &MULTI_CLOSE_MILESTONES);
                if let Some(state) = self
                    .perf_scenario
                    .as_mut()
                    .and_then(|scenario| scenario.multi_document.as_mut())
                {
                    state.stage = MultiDocumentStage::Complete;
                }
                self.complete_perf_scenario(cx);
            }
            MultiDocumentStage::Complete => {}
        }
        true
    }

    fn advance_comparison_scenario(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self
            .perf_scenario
            .as_ref()
            .is_some_and(|scenario| scenario.kind == PerfScenarioKind::MultiDocumentSession)
        {
            return self.advance_multi_document_session(window, cx);
        }
        let phase = self
            .perf_scenario
            .as_ref()
            .map(|scenario| scenario.comparison_phase)
            .unwrap_or(ComparisonPhase::Idle);
        match phase {
            ComparisonPhase::FitModesStart => {
                self.start_fit_mode(window, cx);
                true
            }
            ComparisonPhase::ViewerLayoutSingle | ComparisonPhase::ViewerLayoutContinuous => {
                self.advance_viewer_layout(phase, cx);
                true
            }
            ComparisonPhase::AwaitingViewerSettle {
                operation,
                ready_at_ms,
            } => {
                self.advance_viewer_settle(operation, ready_at_ms, cx);
                true
            }
            ComparisonPhase::Panning {
                started_ms,
                last_sample,
                max_tiles,
            } => {
                self.advance_high_zoom_pan(window, started_ms, last_sample, max_tiles, cx);
                true
            }
            ComparisonPhase::CachePressure { .. } => {
                self.advance_cache_pressure(cx);
                true
            }
            ComparisonPhase::EngineeringCachePressure { .. } => {
                self.advance_engineering_cache_pressure(cx);
                true
            }
            ComparisonPhase::CloseReopen { stage } => {
                self.advance_close_reopen(stage, cx);
                true
            }
            ComparisonPhase::AwaitingNativeInputSurface { .. }
            | ComparisonPhase::NativeAnnotationInput { .. }
            | ComparisonPhase::AwaitingNativeTransformSurface { .. }
            | ComparisonPhase::NativeTransformInput { .. }
            | ComparisonPhase::AwaitingNativeV5SnapSurface
            | ComparisonPhase::NativeV5SnapInput { .. }
            | ComparisonPhase::AwaitingNativeEditorInput { .. }
            | ComparisonPhase::NativeEditorInput { .. }
            | ComparisonPhase::AwaitingNativeScrollSurface
            | ComparisonPhase::AwaitingNativeWheelCalibration { .. }
            | ComparisonPhase::AwaitingDynamicRunnerResult => true,
            ComparisonPhase::AwaitingAnnotationPaint => {
                self.advance_annotation_create_comparison(cx);
                true
            }
            ComparisonPhase::AwaitingNativeTransformPaint {
                scene_revision,
                progress,
                observed_final_rect,
                pixels_per_point,
            } => {
                self.advance_native_transform_paint(
                    scene_revision,
                    progress,
                    observed_final_rect,
                    pixels_per_point,
                    cx,
                );
                true
            }
            ComparisonPhase::AwaitingEditorCreatePaint => {
                self.advance_editor_create(window, cx);
                true
            }
            ComparisonPhase::AwaitingEditorWorkloadPaint { persistence } => {
                self.advance_editor_workload(persistence, window, cx);
                true
            }
            ComparisonPhase::NativeScrollInput { .. } => true,
            ComparisonPhase::Scrolling { .. } | ComparisonPhase::AwaitingScrollSettle { .. } => {
                self.advance_continuous_scroll_comparison(window, cx, phase);
                true
            }
            ComparisonPhase::Idle => false,
        }
    }

    fn open_initial_perf_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self
            .perf_scenario
            .as_ref()
            .is_some_and(|scenario| scenario.kind == PerfScenarioKind::MultiDocumentSession)
        {
            if self.perf_initial_pdfs.len() != MULTI_FIXTURE_IDS.len() {
                self.fail_comparison_scenario(
                    format!(
                        "multi-document-session requires exactly {} ordered PDF fixtures; received {}",
                        MULTI_FIXTURE_IDS.len(),
                        self.perf_initial_pdfs.len()
                    ),
                    cx,
                );
                return;
            }
            self.open_next_multi_document(cx);
            window.refresh();
            cx.notify();
            return;
        }
        let paths = std::mem::take(&mut self.perf_initial_pdfs);
        if paths.is_empty() {
            self.fail_comparison_scenario(
                "native viewer evidence requires an initial PDF fixture",
                cx,
            );
            return;
        }
        let fixture_ids = std::env::var("BP_GPUI_FIXTURE_IDS")
            .ok()
            .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
            .unwrap_or_default();
        for (index, path) in paths.into_iter().enumerate() {
            self.open_path_with_fixture(path, fixture_ids.get(index).map(String::as_str), cx);
        }
        window.refresh();
        cx.notify();
    }

    fn open_next_multi_document(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.multi_document.as_ref())
            .map(|state| state.open_requested)
        else {
            return;
        };
        if index >= MULTI_FIXTURE_IDS.len() {
            return;
        }
        let Some(path) = self.perf_initial_pdfs.get(index).cloned() else {
            self.fail_comparison_scenario("ordered multi-document fixture path is missing", cx);
            return;
        };
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.multi_document.as_mut())
        {
            if index == 0 {
                state.stage_started_ms = perf::elapsed_ms();
            }
            state.open_requested += 1;
        }
        perf::emit(
            "multi-document-open-requested",
            perf::fields([
                ("fixture_id", json!(MULTI_FIXTURE_IDS[index])),
                ("order_index", json!(index)),
                ("path", json!(path)),
                ("process_id", json!(std::process::id())),
            ]),
        );
        self.open_path_with_fixture(path, Some(MULTI_FIXTURE_IDS[index]), cx);
    }

    fn record_multi_milestones(command_id: &str, milestones: &[&str]) {
        for milestone in milestones {
            perf::emit(
                "comparison-milestone",
                perf::fields([
                    ("command_id", json!(command_id)),
                    ("milestone", json!(milestone)),
                ]),
            );
        }
    }

    fn emit_multi_operation_visible(command_id: &str, started_ms: f64) {
        perf::emit(
            "operation-visible",
            perf::fields([
                ("operation", json!(command_id)),
                ("duration_ms", json!(perf::elapsed_ms() - started_ms)),
            ]),
        );
    }

    fn multi_document_id_for_fixture(&self, fixture_id: &str) -> Option<u64> {
        self.perf_scenario
            .as_ref()?
            .multi_document
            .as_ref()?
            .open_observations
            .iter()
            .find(|observation| observation.fixture_id == fixture_id)
            .map(|observation| observation.document_id)
    }

    fn active_multi_fixture_id(&self) -> Option<&'static str> {
        let document_id = self.document()?.id;
        let state = self.perf_scenario.as_ref()?.multi_document.as_ref()?;
        state
            .open_observations
            .iter()
            .position(|observation| observation.document_id == document_id)
            .map(|index| MULTI_FIXTURE_IDS[index])
    }

    fn setup_multi_dense_rectangle(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let document_id = self
            .multi_document_id_for_fixture(DENSE_FIXTURE_ID)
            .ok_or_else(|| "dense multi-document fixture is missing".to_string())?;
        if self.document().map(|document| document.id) != Some(document_id) {
            return Err("dense fixture must be active before rectangle setup".into());
        }
        self.annotation_adapter
            .set_tool(AnnotationTool::Rectangle)
            .map_err(|error| error.to_string())?;
        self.annotation_adapter.queue_next_annotation_id(
            MarkupId::new(DENSE_RECTANGLE_ID).map_err(|error| error.to_string())?,
        );
        self.annotation_adapter.queue_next_rectangle_appearance(
            RectangleAppearance::new("#ff0000", 1.5, None::<String>, 1.0)
                .map_err(|error| error.to_string())?,
        );
        let pointer_id = 91_001;
        self.annotation_adapter
            .pointer_down(
                document_id,
                0,
                pointer_id,
                PdfPoint::new(72.0, 144.0).map_err(|error| error.to_string())?,
                3.0,
            )
            .map_err(|error| error.to_string())?;
        self.annotation_adapter
            .pointer_move(
                pointer_id,
                PdfPoint::new(252.0, 360.0).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        self.annotation_adapter
            .pointer_up(
                pointer_id,
                PdfPoint::new(252.0, 360.0).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        self.annotation_adapter
            .set_tool(AnnotationTool::Select)
            .map_err(|error| error.to_string())?;
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.multi_document.as_mut())
        {
            state.edit_setup_complete = true;
        }
        cx.notify();
        Ok(())
    }

    fn observe_native_viewer_shell_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scenario) = self.perf_scenario.as_mut() else {
            return;
        };
        if scenario.input_lane != PerfInputLane::NativeX11Xtest
            || scenario.native_shell_probe_in_flight
            || scenario.native_shell_observation.is_some()
        {
            return;
        }
        #[cfg(feature = "benchmark-evidence")]
        let Some(input_latency_samples_before) = scenario.viewer_input_latency_samples_before
        else {
            self.fail_comparison_scenario(
                "native shell input arrived before its GPUI latency baseline",
                cx,
            );
            return;
        };
        #[cfg(not(feature = "benchmark-evidence"))]
        {
            self.fail_comparison_scenario(
                "native shell presentation evidence requires benchmark-evidence",
                cx,
            );
            return;
        }
        scenario.native_shell_probe_in_flight = true;
        perf::emit(
            "viewer-native-shell-input-observed",
            perf::fields([
                ("command_id", json!(VIEWER_LAUNCH_COMMAND_ID)),
                ("input_api", json!("XTEST-pointer")),
            ]),
        );
        cx.notify();
        let probe = NativeShellPresentationProbe::new(input_latency_samples_before);
        cx.on_next_frame(window, move |this, window, cx| {
            #[cfg(feature = "benchmark-evidence")]
            {
                let NativeShellProbeProgress::AwaitingPostPresentSample(probe) = probe
                    .observe_frame_callback(
                        this.perf_scenario
                            .as_ref()
                            .is_some_and(|scenario| scenario.render_enter_emitted),
                        window.input_latency_snapshot().latency_histogram.len(),
                    )
                    .expect("the first shell-probe callback only arms the post-present sample")
                else {
                    unreachable!("the first shell-probe callback cannot complete the probe");
                };
                cx.on_next_frame(window, move |this, window, cx| {
                    let snapshot = window.input_latency_snapshot();
                    let progress = probe.observe_frame_callback(
                        this.perf_scenario
                            .as_ref()
                            .is_some_and(|scenario| scenario.render_enter_emitted),
                        snapshot.latency_histogram.len(),
                    );
                    let observation = match progress {
                        Ok(NativeShellProbeProgress::Complete(observation)) => observation,
                        Ok(NativeShellProbeProgress::AwaitingPostPresentSample(_)) => {
                            unreachable!("the second shell-probe callback must finish the probe")
                        }
                        Err(message) => {
                            this.fail_comparison_scenario(&message, cx);
                            return;
                        }
                    };
                    if let Some(scenario) = this.perf_scenario.as_mut() {
                        scenario.native_shell_probe_in_flight = false;
                        scenario.native_shell_observation = Some(observation);
                    }
                    perf::emit(
                        "viewer-native-launch-evidence",
                        perf::fields([
                            ("command_id", json!(VIEWER_LAUNCH_COMMAND_ID)),
                            ("native_input_observed", json!(true)),
                            ("input_api", json!("XTEST-pointer")),
                            (
                                "input_latency_samples_before",
                                json!(observation.input_latency_samples_before),
                            ),
                            (
                                "input_latency_samples_after",
                                json!(observation.input_latency_samples_after),
                            ),
                            (
                                "receipt_scope",
                                json!("gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout"),
                            ),
                            (
                                "input_to_application_draw_ack_p50_ns",
                                json!(snapshot.latency_histogram.value_at_quantile(0.5)),
                            ),
                            (
                                "input_to_application_draw_ack_p95_ns",
                                json!(snapshot.latency_histogram.value_at_quantile(0.95)),
                            ),
                            ("gpui_platform_draw_submitted", json!(true)),
                            ("physical_scanout_observed", json!(false)),
                            ("interactive_shell", json!(true)),
                            ("decision_timing_eligible", json!(false)),
                        ]),
                    );
                    this.open_initial_perf_documents(window, cx);
                });
                window.refresh();
                cx.notify();
            }
        });
    }

    fn advance_native_viewer_open_settle(
        &mut self,
        now: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((started_ms, shell, requested_open, completed_open, preview)) =
            self.perf_scenario.as_ref().and_then(|scenario| {
                Some((
                    scenario.native_open_settle_started_ms?,
                    scenario.native_shell_observation?,
                    scenario.initial_open_requested?,
                    scenario.initial_open_completed,
                    scenario.initial_preview_current,
                ))
            })
        else {
            return false;
        };
        let settled_ms = now - started_ms;
        if settled_ms < 250.0 {
            window.refresh();
            cx.notify();
            return true;
        }
        let preview_document_id = self
            .document()
            .and_then(|document| document.viewport_image().is_some().then_some(document.id));
        let observation = DocumentOpenObservation {
            requested_open_generation: requested_open.generation,
            completed_open_generation: completed_open.map(|token| token.generation),
            requested_document_id: requested_open.document_id,
            active_document_id: self.document().map(|document| document.id),
            preview_document_id,
            preview_generation: preview.map(|token| token.generation),
            pending_preview_generation: self
                .pending_viewport_requests
                .get(&requested_open.document_id)
                .map(|request| request.token.generation),
            preview_available: preview_document_id.is_some(),
            settled_ms,
        };
        let report = match build_viewer_launch_open_evidence(shell, observation) {
            Ok(report) => report,
            Err(error) => {
                self.fail_comparison_scenario(error, cx);
                return true;
            }
        };
        perf::emit(
            "viewer-native-open-evidence",
            perf::fields([
                ("command_id", json!(VIEWER_OPEN_COMMAND_ID)),
                ("document_opened", json!(true)),
                ("preview_current_generation", json!(true)),
                ("settled_current_generation_ms", json!(settled_ms)),
                (
                    "requested_open_generation",
                    json!(requested_open.generation),
                ),
                (
                    "completed_open_generation",
                    json!(completed_open.map(|token| token.generation)),
                ),
                (
                    "preview_generation",
                    json!(preview.map(|token| token.generation)),
                ),
                ("decision_timing_eligible", json!(false)),
            ]),
        );
        perf::emit(
            "viewer-native-launch-open-evidence",
            perf::fields([
                ("commands", json!(report)),
                ("physical_scanout_observed", json!(false)),
                ("decision_timing_eligible", json!(false)),
            ]),
        );
        let scenario_kind = self
            .perf_scenario
            .as_mut()
            .map(|scenario| {
                scenario.native_open_settle_started_ms = None;
                scenario.kind
            })
            .unwrap_or(PerfScenarioKind::OpenPdf);
        if scenario_kind == PerfScenarioKind::OpenPdf {
            self.complete_perf_scenario(cx);
        } else {
            self.start_next_perf_operation(cx);
        }
        true
    }

    fn on_perf_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = perf::elapsed_ms();
        let Some((first_frame, scenario_kind)) = self.perf_scenario.as_mut().map(|scenario| {
            scenario.frame_callback_scheduled = false;
            if let Some(previous) = scenario.last_frame_ms.replace(now) {
                perf::emit(
                    "frame",
                    perf::fields([("interval_ms", json!(now - previous))]),
                );
            }
            let first_frame = !scenario.first_frame_emitted;
            scenario.first_frame_emitted = true;
            (first_frame, scenario.kind)
        }) else {
            return;
        };
        if first_frame {
            let viewport = window.viewport_size();
            self.emit_comparison_view_state(
                "measurement-start",
                [f32::from(viewport.width), f32::from(viewport.height)],
            );
        }
        self.emit_painted_dynamic_fidelity_state();
        if self.advance_native_v5_presentation(window, cx) {
            return;
        }
        #[cfg(feature = "benchmark-evidence")]
        if self
            .perf_scenario
            .as_ref()
            .is_some_and(|scenario| scenario.comparison_completion_pending)
        {
            let Some(input_latency_samples_before) = self
                .perf_scenario
                .as_ref()
                .and_then(|scenario| scenario.comparison_input_latency_samples_before)
            else {
                self.fail_comparison_scenario(
                    "native comparison completed without an input-latency baseline",
                    cx,
                );
                return;
            };
            let snapshot = window.input_latency_snapshot();
            let input_latency_samples_after = snapshot.latency_histogram.len();
            if input_latency_samples_after <= input_latency_samples_before {
                self.fail_comparison_scenario(
                    "native comparison completed without a GPUI input-latency sample",
                    cx,
                );
                return;
            }
            perf::emit(
                "native-application-draw-acknowledgement",
                perf::fields([
                    (
                        "receipt_scope",
                        json!(
                            "gpui-input-latency-histogram-to-platform-draw-submission-not-physical-scanout"
                        ),
                    ),
                    (
                        "input_latency_samples_before",
                        json!(input_latency_samples_before),
                    ),
                    (
                        "input_latency_samples_after",
                        json!(input_latency_samples_after),
                    ),
                    (
                        "input_to_application_draw_ack_p50_ns",
                        json!(snapshot.latency_histogram.value_at_quantile(0.5)),
                    ),
                    (
                        "input_to_application_draw_ack_p95_ns",
                        json!(snapshot.latency_histogram.value_at_quantile(0.95)),
                    ),
                    ("gpui_platform_draw_submitted", json!(true)),
                    ("physical_scanout_observed", json!(false)),
                ]),
            );
            if let Some(scenario) = self.perf_scenario.as_mut() {
                scenario.comparison_completion_pending = false;
            }
            self.complete_perf_scenario(cx);
            return;
        }
        if first_frame {
            let gpu_specs = window.gpu_specs();
            perf::emit(
                "gpu-adapter-selected",
                perf::fields([
                    ("available", json!(gpu_specs.is_some())),
                    (
                        "is_software_emulated",
                        gpu_specs
                            .as_ref()
                            .map_or(Value::Null, |specs| json!(specs.is_software_emulated)),
                    ),
                    (
                        "device_name",
                        gpu_specs
                            .as_ref()
                            .map_or(Value::Null, |specs| json!(specs.device_name)),
                    ),
                    (
                        "driver_name",
                        gpu_specs
                            .as_ref()
                            .map_or(Value::Null, |specs| json!(specs.driver_name)),
                    ),
                    (
                        "driver_info",
                        gpu_specs
                            .as_ref()
                            .map_or(Value::Null, |specs| json!(specs.driver_info)),
                    ),
                ]),
            );
            perf::emit("first-frame-callback-fired", Default::default());
            perf::emit("first-frame", Default::default());
            perf::emit("shell-ready", Default::default());
            if scenario_kind == PerfScenarioKind::EmptyShell {
                self.complete_perf_scenario(cx);
                return;
            }
            let native_lane = self
                .perf_scenario
                .as_ref()
                .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest);
            if native_lane {
                #[cfg(feature = "benchmark-evidence")]
                {
                    let viewport = window.viewport_size();
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.viewer_input_latency_samples_before =
                            Some(window.input_latency_snapshot().latency_histogram.len());
                    }
                    perf::emit(
                        "native-viewer-shell-ready",
                        perf::fields([
                            ("command_id", json!(VIEWER_LAUNCH_COMMAND_ID)),
                            (
                                "control",
                                json!({
                                    "window_logical_size": {
                                        "width": f32::from(viewport.width),
                                        "height": f32::from(viewport.height),
                                    },
                                    "point": {
                                        "x": f32::from(viewport.width) / 2.0,
                                        "y": f32::from(viewport.height) / 2.0,
                                    },
                                }),
                            ),
                        ]),
                    );
                    window.refresh();
                    cx.notify();
                    return;
                }
                #[cfg(not(feature = "benchmark-evidence"))]
                {
                    self.fail_comparison_scenario(
                        "native shell presentation evidence requires benchmark-evidence",
                        cx,
                    );
                    return;
                }
            }
            self.open_initial_perf_documents(window, cx);
            return;
        }

        if self.advance_native_viewer_open_settle(now, window, cx) {
            return;
        }

        if self.advance_comparison_scenario(window, cx) {
            return;
        }

        let Some(scenario) = self.perf_scenario.as_mut() else {
            return;
        };
        let initial_visible = scenario.pending_initial_visible;
        let operation = scenario.pending_visible_operation.take();
        if initial_visible {
            scenario.pending_initial_visible = false;
            scenario.initial_viewport_visible = true;
        }
        if operation.is_some() {
            scenario.step_index += 1;
        }
        let scenario_kind = scenario.kind;
        if !initial_visible && operation.is_none() {
            // The async viewport completion notifies the entity. Do not drive
            // an unbounded software-render frame loop while it is pending.
            return;
        }

        if initial_visible {
            let duration_ms = self
                .perf_scenario
                .as_ref()
                .and_then(|scenario| scenario.open_started_ms)
                .map(|started_ms| now - started_ms);
            perf::emit(
                "viewport-visible",
                perf::fields([("duration_ms", json!(duration_ms))]),
            );
            if scenario_kind == PerfScenarioKind::OpenPdf {
                if self
                    .perf_scenario
                    .as_ref()
                    .is_some_and(|scenario| scenario.input_lane == PerfInputLane::NativeX11Xtest)
                {
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.native_open_settle_started_ms = Some(now);
                    }
                    window.refresh();
                    cx.notify();
                } else {
                    self.complete_perf_scenario(cx);
                }
            } else {
                if self.perf_scenario.as_ref().is_some_and(|scenario| {
                    scenario.input_lane == PerfInputLane::NativeX11Xtest
                        && scenario.native_open_settle_started_ms.is_none()
                }) {
                    if let Some(scenario) = self.perf_scenario.as_mut() {
                        scenario.native_open_settle_started_ms = Some(now);
                    }
                    window.refresh();
                    cx.notify();
                } else {
                    self.start_next_perf_operation(cx);
                }
            }
        } else if let Some(operation) = operation {
            perf::emit(
                "operation-visible",
                perf::fields([
                    ("duration_ms", json!(now - operation.started_ms)),
                    ("operation", json!(operation.kind)),
                    ("value", json!(operation.value)),
                ]),
            );
            let viewer_operation = match (scenario_kind, operation.kind) {
                (PerfScenarioKind::PageNavigation, "page") => Some(ViewerOperation::Navigation),
                (PerfScenarioKind::Zoom, "zoom") => Some(ViewerOperation::Zoom),
                (PerfScenarioKind::HighZoomPan, "zoom-pan-prime") => {
                    Some(ViewerOperation::HighZoomPanPrime)
                }
                (PerfScenarioKind::FitModes, "fit-page") => Some(ViewerOperation::FitPage {
                    expected_zoom_percent: operation.value as f32,
                }),
                (PerfScenarioKind::FitModes, "fit-width") => Some(ViewerOperation::FitWidth {
                    expected_zoom_percent: operation.value as f32,
                }),
                _ => None,
            };
            if let Some(viewer_operation) = viewer_operation {
                if let Some(scenario) = self.perf_scenario.as_mut() {
                    scenario.comparison_phase = ComparisonPhase::AwaitingViewerSettle {
                        operation: viewer_operation,
                        ready_at_ms: now + 250.0,
                    };
                }
            } else {
                self.start_next_perf_operation(cx);
            }
        }
        window.refresh();
        cx.notify();
    }

    fn schedule_perf_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scenario) = self.perf_scenario.as_mut() else {
            return;
        };
        if !scenario.frame_callback_scheduled {
            scenario.frame_callback_scheduled = true;
            if !scenario.first_frame_emitted {
                perf::emit("first-frame-callback-scheduled", Default::default());
            }
            cx.on_next_frame(window, |this, window, cx| {
                this.on_perf_frame(window, cx);
            });
        }
    }

    fn request_thumbnail(&mut self, document_id: u64, page: usize, cx: &mut Context<Self>) {
        let key = (document_id, page);
        if self.pending_thumbnail_requests.contains_key(&key)
            || self.thumbnail_failures.contains_key(&key)
        {
            return;
        }
        let token = self.next_request(document_id);
        self.pending_thumbnail_requests
            .insert(key, token.generation);
        self.thumbnail_queue
            .push_back(ThumbnailRequest { token, page });
        self.pump_thumbnail_queue(cx);
    }

    fn pump_thumbnail_queue(&mut self, cx: &mut Context<Self>) {
        while self.active_thumbnail_jobs < MAX_CONCURRENT_THUMBNAIL_JOBS {
            let Some(request) = self.thumbnail_queue.pop_front() else {
                break;
            };
            let key = (request.token.document_id, request.page);
            if self.pending_thumbnail_requests.get(&key).copied() != Some(request.token.generation)
            {
                continue;
            }
            let Some(index) = self.document_index_by_id(request.token.document_id) else {
                self.pending_thumbnail_requests.remove(&key);
                continue;
            };
            let document = self.documents[index].clone();
            self.active_thumbnail_jobs += 1;
            let task = cx
                .background_executor()
                .spawn(async move { document.render_thumbnail(request.page) });
            cx.spawn(async move |entity, cx| {
                let result = task.await;
                let _ = entity.update(cx, |this, cx| {
                    this.finish_thumbnail(request, result, cx);
                });
            })
            .detach();
        }
    }

    fn finish_thumbnail(
        &mut self,
        request: ThumbnailRequest,
        result: Result<Arc<RenderImage>, String>,
        cx: &mut Context<Self>,
    ) {
        self.active_thumbnail_jobs = self.active_thumbnail_jobs.saturating_sub(1);
        let key = (request.token.document_id, request.page);
        if self.pending_thumbnail_requests.get(&key).copied() == Some(request.token.generation) {
            self.pending_thumbnail_requests.remove(&key);
            if let Err(error) = result {
                self.thumbnail_failures.insert(key, error.clone());
                if self
                    .document()
                    .is_some_and(|document| document.id == request.token.document_id)
                {
                    self.document_error = Some(format!(
                        "Could not render thumbnail for page {}: {error}",
                        request.page
                    ));
                }
            }
        }
        self.pump_thumbnail_queue(cx);
        cx.notify();
    }

    fn request_page_surface(
        &mut self,
        document_id: u64,
        page: usize,
        zoom_percent: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.document_index_by_id(document_id) else {
            return;
        };
        let document = &self.documents[index];
        let page = page.clamp(1, document.page_count);
        let scale_factor = self.display_scale_factor;
        let pixel_width = document.viewport_pixel_width_for(page, zoom_percent, scale_factor);
        let key = (document_id, page, pixel_width);
        if document
            .cached_viewport_image(page, zoom_percent, scale_factor)
            .is_some()
            || self.pending_page_surface_requests.contains_key(&key)
            || self.page_surface_failures.contains_key(&key)
            || self
                .pending_viewport_requests
                .get(&document_id)
                .is_some_and(|request| {
                    request.page == page
                        && self.documents[index].viewport_pixel_width_for(
                            request.page,
                            request.zoom_percent,
                            request.scale_factor,
                        ) == pixel_width
                })
        {
            return;
        }
        let token = self.next_request(document_id);
        self.pending_page_surface_requests
            .insert(key, token.generation);
        self.page_surface_queue.push_back(PageSurfaceRequest {
            token,
            page,
            zoom_percent,
            scale_factor,
            pixel_width,
        });
        self.pump_page_surface_queue(cx);
    }

    fn pump_page_surface_queue(&mut self, cx: &mut Context<Self>) {
        while self.active_page_surface_jobs < MAX_CONCURRENT_PAGE_SURFACE_JOBS {
            let Some(request) = self.page_surface_queue.pop_front() else {
                break;
            };
            let key = (request.token.document_id, request.page, request.pixel_width);
            if self.pending_page_surface_requests.get(&key).copied()
                != Some(request.token.generation)
            {
                continue;
            }
            let Some(index) = self.document_index_by_id(request.token.document_id) else {
                self.pending_page_surface_requests.remove(&key);
                continue;
            };
            let document = self.documents[index].clone();
            self.active_page_surface_jobs += 1;
            let task = cx.background_executor().spawn(async move {
                document.render_viewport(request.page, request.zoom_percent, request.scale_factor)
            });
            cx.spawn(async move |entity, cx| {
                let result = task.await;
                let _ = entity.update(cx, |this, cx| {
                    this.finish_page_surface(request, result, cx);
                });
            })
            .detach();
        }
    }

    fn finish_page_surface(
        &mut self,
        request: PageSurfaceRequest,
        result: Result<RenderedViewport, String>,
        cx: &mut Context<Self>,
    ) {
        self.active_page_surface_jobs = self.active_page_surface_jobs.saturating_sub(1);
        let key = (request.token.document_id, request.page, request.pixel_width);
        if self.pending_page_surface_requests.get(&key).copied() == Some(request.token.generation) {
            self.pending_page_surface_requests.remove(&key);
            if let Err(error) = result {
                self.page_surface_failures.insert(key, error.clone());
                if self
                    .document()
                    .is_some_and(|document| document.id == request.token.document_id)
                {
                    self.document_error = Some(format!(
                        "Could not render page {} for continuous view: {error}",
                        request.page
                    ));
                }
            }
        }
        self.pump_page_surface_queue(cx);
        cx.notify();
    }

    fn refresh_tile_plan(&mut self, window: &Window, cx: &mut Context<Self>) {
        let viewport_width = (f32::from(window.viewport_size().width)
            - RAIL_WIDTH
            - SIDEBAR_WIDTH
            - RIGHT_RAIL_WIDTH)
            .max(1.0);
        let viewport_height = (f32::from(window.viewport_size().height)
            - WINDOW_TITLE_BAR_HEIGHT
            - MENU_BAR_HEIGHT
            - DOCUMENT_TAB_BAR_HEIGHT
            - PRIMARY_BAND_HEIGHT)
            .max(1.0);
        let Some((source, document_id, current_page, pages)) = self.document().map(|document| {
            let current_page = document.current_page;
            let pages = match self.scroll_mode {
                ScrollMode::SinglePage => {
                    let (width, height) = document.page_size(current_page);
                    vec![butter_paper_gpui_gallery::viewer::PageGeometry::new(
                        current_page,
                        width,
                        height,
                    )]
                }
                ScrollMode::Continuous => (1..=document.page_count)
                    .map(|page| {
                        let (width, height) = document.page_size(page);
                        butter_paper_gpui_gallery::viewer::PageGeometry::new(page, width, height)
                    })
                    .collect(),
            };
            (document.render_source(), document.id, current_page, pages)
        }) else {
            self.continuous_page_layouts.clear();
            self.continuous_visible_pages.clear();
            self.continuous_total_height = 0.0;
            self.continuous_plan_generation = 0;
            return;
        };
        let offset = match self.scroll_mode {
            ScrollMode::SinglePage => self.document_scroll.offset(),
            ScrollMode::Continuous => self.continuous_scroll.offset(),
        };
        let scroll_x = (-f32::from(offset.x)).max(0.0);
        let scroll_y = (-f32::from(offset.y)).max(0.0);
        let key = TilePlanKey {
            document_id,
            revision: source.revision,
            page: match self.scroll_mode {
                ScrollMode::SinglePage => current_page,
                ScrollMode::Continuous => 0,
            },
            zoom_tenths: (self.zoom_percent * 10.0).round() as i32,
            scale_millis: (self.display_scale_factor * 1_000.0).round() as i32,
            scroll_x: scroll_x.round() as i32,
            scroll_y: scroll_y.round() as i32,
            viewport_width: viewport_width.ceil() as i32,
            viewport_height: viewport_height.ceil() as i32,
        };
        if self.tile_plan_key == Some(key) {
            return;
        }

        let plan = self.render_planner.plan(RenderInput {
            source,
            pages: &pages,
            zoom_percent: self.zoom_percent,
            device_scale: self.display_scale_factor,
            page_gap: PAGE_LAYOUT_GAP,
            viewport: ViewportGeometry {
                width: viewport_width,
                height: viewport_height,
                scroll_x,
                scroll_y,
                visible_rect: ViewerRect::new(0.0, 0.0, viewport_width, viewport_height),
            },
        });
        if let Some(document) = self.document() {
            document.set_render_generation(plan.generation);
        }
        if self.scroll_mode == ScrollMode::Continuous {
            self.continuous_page_layouts = plan.page_layouts.clone();
            self.continuous_visible_pages = plan.visible_pages.clone();
            self.continuous_total_height = plan.total_height;
            self.continuous_plan_generation = plan.generation;
            if let Some(page) = plan.current_page
                && let Some(index) = self.active_document
            {
                self.documents[index].select_page(page);
            }
        } else {
            self.continuous_page_layouts.clear();
            self.continuous_visible_pages.clear();
            self.continuous_total_height = 0.0;
            self.continuous_plan_generation = 0;
        }
        self.tile_plan_key = Some(key);
        if self.scroll_mode == ScrollMode::Continuous {
            let visible_pages = self
                .continuous_visible_pages
                .iter()
                .copied()
                .collect::<HashSet<_>>();
            prune_page_surface_work_to_visible_pages(
                document_id,
                &visible_pages,
                &mut self.pending_page_surface_requests,
                &mut self.page_surface_queue,
            );
        }
        self.pending_tile_requests.clear();
        self.tile_queue.clear();
        self.visible_tile_requests = plan
            .tiles
            .into_iter()
            .filter(|request| {
                self.document().is_some_and(|document| {
                    uses_tiled_rendering(document.viewport_pixel_width_for(
                        request.page,
                        self.zoom_percent,
                        self.display_scale_factor,
                    ))
                })
            })
            .collect();
        for request in self.visible_tile_requests.iter().copied() {
            if !self.tile_cache.contains(request) {
                self.pending_tile_requests.insert(request);
                self.tile_queue.push_back(TileJob {
                    document_id,
                    zoom_percent: self.zoom_percent,
                    scale_factor: self.display_scale_factor,
                    request,
                });
            }
        }
        if self.scroll_mode == ScrollMode::Continuous {
            for page in self.continuous_visible_pages.clone() {
                let needs_page_surface = self.document().is_some_and(|document| {
                    !uses_tiled_rendering(document.viewport_pixel_width_for(
                        page,
                        self.zoom_percent,
                        self.display_scale_factor,
                    ))
                });
                if needs_page_surface {
                    self.request_page_surface(document_id, page, self.zoom_percent, cx);
                }
            }
        }
        self.pump_tile_queue(cx);
    }

    fn pump_tile_queue(&mut self, cx: &mut Context<Self>) {
        while self.active_tile_jobs < MAX_CONCURRENT_TILE_JOBS {
            let Some(job) = self.tile_queue.pop_front() else {
                break;
            };
            if !tile_job_can_start(
                job.request,
                &self.pending_tile_requests,
                &self.active_tile_requests,
                &self.render_planner,
            ) {
                continue;
            }
            let Some(index) = self.document_index_by_id(job.document_id) else {
                self.pending_tile_requests.remove(&job.request);
                continue;
            };
            let document = self.documents[index].clone();
            self.active_tile_requests.insert(job.request);
            self.active_tile_jobs += 1;
            let task = cx.background_executor().spawn(async move {
                document.render_tile(job.request, job.zoom_percent, job.scale_factor)
            });
            cx.spawn(async move |entity, cx| {
                let result = task.await;
                let _ = entity.update(cx, |this, cx| this.finish_tile(job, result, cx));
            })
            .detach();
        }
    }

    fn finish_tile(
        &mut self,
        job: TileJob,
        result: Result<RenderedTile, String>,
        cx: &mut Context<Self>,
    ) {
        self.active_tile_jobs = self.active_tile_jobs.saturating_sub(1);
        self.active_tile_requests.remove(&job.request);
        self.pending_tile_requests.remove(&job.request);
        if self.render_planner.accepts(job.request.generation) {
            match result {
                Ok(rendered) => {
                    let bytes = rendered.request.crop.width
                        * rendered.request.crop.height
                        * std::mem::size_of::<u32>();
                    self.tile_cache
                        .insert(rendered.request, rendered.image, bytes);
                    if let Some(scenario) = self.perf_scenario.as_mut()
                        && let ComparisonPhase::CachePressure {
                            operation_index,
                            tile_cache_insert_bytes,
                            atlas_upload_checkpoint_bytes,
                        } = scenario.comparison_phase
                    {
                        scenario.comparison_phase = ComparisonPhase::CachePressure {
                            operation_index,
                            tile_cache_insert_bytes: tile_cache_insert_bytes.saturating_add(bytes),
                            atlas_upload_checkpoint_bytes,
                        };
                    }
                    if let Some(scenario) = self.perf_scenario.as_mut()
                        && let ComparisonPhase::EngineeringCachePressure {
                            operation_index,
                            tile_cache_insert_bytes,
                            atlas_upload_checkpoint_bytes,
                        } = scenario.comparison_phase
                    {
                        scenario.comparison_phase = ComparisonPhase::EngineeringCachePressure {
                            operation_index,
                            tile_cache_insert_bytes: tile_cache_insert_bytes.saturating_add(bytes),
                            atlas_upload_checkpoint_bytes,
                        };
                    }
                }
                Err(error) if !error.contains("cancel") => {
                    self.document_error = Some(format!(
                        "Could not render a high-zoom tile on page {}: {error}",
                        job.request.page
                    ));
                }
                Err(_) => {}
            }
        }
        self.pump_tile_queue(cx);
        cx.notify();
    }

    fn visible_tiles(&self, page: usize) -> Vec<AnyElement> {
        let tile_atlas_uploads = self.tile_atlas_uploads.clone();
        self.visible_tile_requests
            .iter()
            .filter(|request| request.page == page)
            .filter_map(|request| {
                let render_image = self.tile_cache.peek(*request)?.clone();
                let crop = request.crop;
                let scale = (request.device_scale_millis as f32 / 1_000.0).max(0.1);
                let upload_receipts = tile_atlas_uploads.clone();
                Some(
                    div()
                        .absolute()
                        .left(px(crop.x as f32 / scale))
                        .top(px(crop.y as f32 / scale))
                        .w(px(crop.width as f32 / scale))
                        .h(px(crop.height as f32 / scale))
                        .child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let decoded_bytes =
                                        render_image.as_bytes(0).map_or(0, <[u8]>::len);
                                    match window.paint_image(
                                        bounds,
                                        bounds,
                                        Corners::default(),
                                        render_image.clone(),
                                        0,
                                        false,
                                    ) {
                                        Ok(()) if decoded_bytes > 0 => {
                                            upload_receipts
                                                .lock()
                                                .expect("tile atlas upload receipt lock")
                                                .insert(render_image.id.0, decoded_bytes);
                                        }
                                        Ok(()) => {}
                                        Err(error) => perf::emit(
                                            "tile-image-paint-failed",
                                            perf::fields([("error", json!(error.to_string()))]),
                                        ),
                                    }
                                },
                            )
                            .size_full(),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }

    fn fit_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.document() else {
            return;
        };
        let available_width =
            f32::from(window.viewport_size().width) - RAIL_WIDTH - SIDEBAR_WIDTH - RIGHT_RAIL_WIDTH;
        let zoom = resolve_fit_width_zoom(available_width, document.page_width);
        self.set_zoom(zoom, ZoomPreset::FitWidth, cx);
    }

    fn fit_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.document() else {
            return;
        };
        let available_width =
            f32::from(window.viewport_size().width) - RAIL_WIDTH - SIDEBAR_WIDTH - RIGHT_RAIL_WIDTH;
        let available_height = f32::from(window.viewport_size().height)
            - WINDOW_TITLE_BAR_HEIGHT
            - MENU_BAR_HEIGHT
            - DOCUMENT_TAB_BAR_HEIGHT
            - PRIMARY_BAND_HEIGHT
            - 32.0;
        self.set_zoom(
            resolve_fit_page_zoom(
                available_width,
                available_height,
                document.page_width,
                document.page_height,
            ),
            ZoomPreset::FitPage,
            cx,
        );
    }

    fn set_scroll_mode(&mut self, scroll_mode: ScrollMode, cx: &mut Context<Self>) {
        self.scroll_mode = scroll_mode;
        self.single_page_wheel_delta = 0.0;
        if let Some(current_page) = self.document().map(|document| document.current_page) {
            if scroll_mode == ScrollMode::Continuous {
                self.scroll_continuous_to_page(current_page);
            } else {
                self.document_scroll.set_offset(point(px(0.0), px(0.0)));
            }
        }
        cx.notify();
    }

    fn scroll_continuous_to_page(&self, page: usize) {
        let Some(document) = self.document() else {
            return;
        };
        let page = page.clamp(1, document.page_count);
        let zoom = self.zoom_percent / 100.0;
        let preceding_height = (1..page)
            .map(|candidate| document.page_size(candidate).1 * zoom + PAGE_LAYOUT_GAP)
            .sum::<f32>();
        self.continuous_scroll
            .set_offset(point(px(0.0), px(-(PAGE_LAYOUT_GAP + preceding_height))));
    }

    fn viewport_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());
        let dominant_delta = if f32::from(delta.y).abs() >= f32::from(delta.x).abs() {
            f32::from(delta.y)
        } else {
            f32::from(delta.x)
        };
        if dominant_delta.abs() <= f32::EPSILON {
            return;
        }
        let calibration_pending = self.perf_scenario.as_ref().is_some_and(|scenario| {
            matches!(
                scenario.comparison_phase,
                ComparisonPhase::AwaitingNativeWheelCalibration { .. }
            )
        });
        self.observe_native_scroll_input(dominant_delta, cx);
        if calibration_pending {
            cx.stop_propagation();
            window.refresh();
            cx.notify();
            return;
        }
        if self.native_scroll_pending_offset_y.is_some() {
            self.schedule_native_scroll_flush(window, cx);
            cx.stop_propagation();
            return;
        }
        let configured_mode = match self.scroll_mode {
            ScrollMode::Continuous => self.continuous_wheel_mode,
            ScrollMode::SinglePage => self.single_page_wheel_mode,
        };
        if should_scroll_viewport_wheel(configured_mode, event.modifiers.control) {
            if self.scroll_mode == ScrollMode::SinglePage {
                let Some((current_page, page_count)) = self
                    .document()
                    .map(|document| (document.current_page, document.page_count))
                else {
                    return;
                };
                let (next_page, accumulated_delta) = resolve_single_page_wheel(
                    current_page,
                    page_count,
                    self.single_page_wheel_delta,
                    dominant_delta,
                );
                self.single_page_wheel_delta = accumulated_delta;
                if let Some(next_page) = next_page {
                    self.set_page(next_page, cx);
                } else {
                    cx.notify();
                }
                cx.stop_propagation();
            }
            return;
        }
        self.single_page_wheel_delta = 0.0;
        self.set_zoom(
            resolve_wheel_zoom(self.zoom_percent, dominant_delta),
            ZoomPreset::Manual,
            cx,
        );
        cx.stop_propagation();
    }

    fn schedule_native_scroll_flush(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.native_scroll_flush_scheduled {
            return;
        }
        self.native_scroll_flush_scheduled = true;
        cx.on_next_frame(window, |this, window, cx| {
            this.native_scroll_flush_scheduled = false;
            let Some(offset_y) = this.native_scroll_pending_offset_y.take() else {
                return;
            };
            this.continuous_scroll
                .set_offset(point(px(0.0), px(offset_y)));
            this.observe_native_scroll_flush(offset_y);
            window.refresh();
            cx.notify();
        });
        window.refresh();
        cx.notify();
    }

    fn observe_native_scroll_flush(&mut self, offset_y: f32) {
        let phase = self
            .perf_scenario
            .as_ref()
            .map(|scenario| scenario.comparison_phase)
            .unwrap_or(ComparisonPhase::Idle);
        let ComparisonPhase::NativeScrollInput {
            forward_events,
            reverse_events,
            first_direction,
            wheel_unit_delta,
            last_forward_ms,
            first_reverse_ms,
            start_offset_y,
            mut peak_distance,
            viewport_height,
            mut raster_observations,
            mut missing_raster_observations,
            mut max_visible_pages,
        } = phase
        else {
            return;
        };
        peak_distance = peak_distance.max((offset_y - start_offset_y).abs());
        max_visible_pages = max_visible_pages.max(self.continuous_visible_pages.len());
        if let Some(ready) = self.continuous_visible_raster_is_ready() {
            raster_observations = raster_observations.saturating_add(1);
            if !ready {
                missing_raster_observations = missing_raster_observations.saturating_add(1);
            }
        }
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::NativeScrollInput {
                forward_events,
                reverse_events,
                first_direction,
                wheel_unit_delta,
                last_forward_ms,
                first_reverse_ms,
                start_offset_y,
                peak_distance,
                viewport_height,
                raster_observations,
                missing_raster_observations,
                max_visible_pages,
            };
        }
        perf::emit(
            "native-scroll-frame-batch-applied",
            perf::fields([
                ("forward_receipts", json!(forward_events)),
                ("reverse_receipts", json!(reverse_events)),
                ("offset_y", json!(offset_y)),
                ("peak_distance", json!(peak_distance)),
                ("raster_observation_count", json!(raster_observations)),
                (
                    "missing_raster_observation_count",
                    json!(missing_raster_observations),
                ),
            ]),
        );
    }

    fn open_pdf_action(&mut self, _: &OpenPdf, _: &mut Window, cx: &mut Context<Self>) {
        self.open_pdf_dialog(cx);
    }

    fn next_page_action(&mut self, _: &NextPage, _: &mut Window, cx: &mut Context<Self>) {
        let page = self
            .document()
            .map(|document| (document.current_page + 1).min(document.page_count));
        if let Some(page) = page {
            self.set_page(page, cx);
        }
    }

    fn observe_native_scroll_input(&mut self, delta: f32, cx: &mut Context<Self>) {
        let Some(plan) = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.comparison_plan.as_ref())
            .and_then(ComparisonScenarioPlan::continuous_scroll)
            .cloned()
        else {
            return;
        };
        let phase = self
            .perf_scenario
            .as_ref()
            .map(|scenario| scenario.comparison_phase)
            .unwrap_or(ComparisonPhase::Idle);
        if let ComparisonPhase::AwaitingNativeWheelCalibration {
            start_offset_y,
            viewport_height,
        } = phase
        {
            let Some(calibrated_delta) = native_wheel_calibration_delta(delta) else {
                self.fail_comparison_scenario(
                    "native wheel calibration produced a zero or non-finite delta",
                    cx,
                );
                return;
            };
            perf::emit(
                "native-wheel-calibrated",
                perf::fields([
                    ("command_id", json!(DYNAMIC_FIDELITY_COMMAND_ID)),
                    ("candidate_runtime", json!("gpui")),
                    ("input_api", json!("XTEST-single-wheel-notch")),
                    ("calibration_event_count", json!(1)),
                    ("observed_wheel_delta_css_px", json!(calibrated_delta)),
                    ("initial_scroll_offset_css_px", json!(-start_offset_y)),
                    (
                        "post_calibration_scroll_offset_css_px",
                        json!(-start_offset_y),
                    ),
                    ("scroll_applied", json!(false)),
                    ("timed_trajectory_excluded", json!(true)),
                ]),
            );
            if let Some(scenario) = self.perf_scenario.as_mut() {
                scenario.comparison_phase = ComparisonPhase::NativeScrollInput {
                    forward_events: 0,
                    reverse_events: 0,
                    first_direction: None,
                    wheel_unit_delta: Some(calibrated_delta),
                    last_forward_ms: None,
                    first_reverse_ms: None,
                    start_offset_y,
                    peak_distance: 0.0,
                    viewport_height,
                    raster_observations: 0,
                    missing_raster_observations: 0,
                    max_visible_pages: 0,
                };
            }
            return;
        }
        let ComparisonPhase::NativeScrollInput {
            mut forward_events,
            mut reverse_events,
            mut first_direction,
            mut wheel_unit_delta,
            mut last_forward_ms,
            mut first_reverse_ms,
            start_offset_y,
            peak_distance,
            viewport_height,
            raster_observations,
            missing_raster_observations,
            max_visible_pages,
        } = phase
        else {
            return;
        };
        let delta_magnitude = delta.abs();
        let expected_delta = *wheel_unit_delta.get_or_insert(delta_magnitude);
        if !native_wheel_delta_is_unit(expected_delta, delta_magnitude) {
            self.fail_comparison_scenario(
                format!(
                    "native continuous scroll coalesced or changed wheel delta: {delta_magnitude:.3} versus unit {expected_delta:.3}"
                ),
                cx,
            );
            return;
        }
        let direction = if delta.is_sign_positive() { 1 } else { -1 };
        let now = perf::elapsed_ms();
        let initial_direction = *first_direction.get_or_insert(direction);
        if direction == initial_direction && reverse_events == 0 {
            forward_events = forward_events.saturating_add(1);
            last_forward_ms = Some(now);
        } else if direction == -initial_direction {
            reverse_events = reverse_events.saturating_add(1);
            first_reverse_ms.get_or_insert(now);
        } else {
            self.fail_comparison_scenario(
                "native continuous scroll changed back to the forward direction",
                cx,
            );
            return;
        }
        let dynamic_fidelity = self
            .perf_scenario
            .as_ref()
            .is_some_and(|scenario| scenario.kind == PerfScenarioKind::DynamicFidelity);
        let expected_events = if dynamic_fidelity {
            native_distance_bounded_expected_events(
                viewport_height,
                plan.forward_viewport_heights,
                expected_delta,
            )
            .map(|events| (events, events))
        } else {
            native_scroll_expected_events(
                plan.forward_duration_ms,
                plan.reverse_duration_ms,
                plan.input_rate_hz,
            )
        };
        let Some((expected_forward, expected_reverse)) = expected_events else {
            self.fail_comparison_scenario(
                "native continuous scroll could not derive its expected event counts",
                cx,
            );
            return;
        };
        if forward_events > expected_forward || reverse_events > expected_reverse {
            self.fail_comparison_scenario(
                format!(
                    "native continuous scroll received too many events: {forward_events}/{expected_forward} forward, {reverse_events}/{expected_reverse} reverse"
                ),
                cx,
            );
            return;
        }
        let received_events = forward_events.saturating_add(reverse_events);
        if received_events.is_multiple_of(120)
            || (forward_events == expected_forward && reverse_events == expected_reverse)
        {
            perf::emit(
                "native-scroll-input-progress",
                perf::fields([
                    ("forward_receipts", json!(forward_events)),
                    ("expected_forward_receipts", json!(expected_forward)),
                    ("reverse_receipts", json!(reverse_events)),
                    ("expected_reverse_receipts", json!(expected_reverse)),
                    ("total_receipts", json!(received_events)),
                ]),
            );
        }
        let target_offset_y = if dynamic_fidelity {
            native_distance_bounded_offset_y(
                start_offset_y,
                expected_delta,
                forward_events,
                reverse_events,
                expected_forward,
                initial_direction,
            )
        } else {
            native_scroll_batched_offset_y(
                start_offset_y,
                viewport_height,
                plan.forward_viewport_heights,
                forward_events,
                expected_forward,
                reverse_events,
                expected_reverse,
                initial_direction,
            )
        };
        let Some(target_offset_y) = target_offset_y else {
            self.fail_comparison_scenario(
                "native continuous scroll could not calculate its frame-batched offset",
                cx,
            );
            return;
        };
        self.native_scroll_pending_offset_y = Some(target_offset_y);
        if forward_events == expected_forward && reverse_events == expected_reverse {
            let pause_ms = first_reverse_ms.unwrap_or(now) - last_forward_ms.unwrap_or(now);
            if pause_ms + 100.0 < plan.pause_duration_ms as f64 {
                self.fail_comparison_scenario(
                    format!(
                        "native continuous scroll pause was {pause_ms:.1} ms; expected at least {} ms",
                        plan.pause_duration_ms
                    ),
                    cx,
                );
                return;
            }
            if let Some(scenario) = self.perf_scenario.as_mut() {
                scenario.comparison_phase = ComparisonPhase::AwaitingScrollSettle {
                    input_samples: forward_events + reverse_events,
                    expected_samples: expected_forward + expected_reverse,
                    native_peak_viewport_heights: Some(peak_distance / viewport_height.max(1.0)),
                    native_settle_at_ms: Some(now + 250.0),
                    raster_observations,
                    missing_raster_observations,
                    max_visible_pages,
                };
            }
        } else if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.comparison_phase = ComparisonPhase::NativeScrollInput {
                forward_events,
                reverse_events,
                first_direction,
                wheel_unit_delta,
                last_forward_ms,
                first_reverse_ms,
                start_offset_y,
                peak_distance,
                viewport_height,
                raster_observations,
                missing_raster_observations,
                max_visible_pages,
            };
        }
    }

    fn previous_page_action(&mut self, _: &PreviousPage, _: &mut Window, cx: &mut Context<Self>) {
        let page = self
            .document()
            .map(|document| document.current_page.saturating_sub(1).max(1));
        if let Some(page) = page {
            self.set_page(page, cx);
        }
    }

    fn zoom_in_action(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.change_zoom(1.0, cx);
    }

    fn zoom_out_action(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.change_zoom(-1.0, cx);
    }

    fn zoom_reset_action(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(100.0, ZoomPreset::Manual, cx);
    }

    fn fit_width_action(&mut self, _: &FitWidth, window: &mut Window, cx: &mut Context<Self>) {
        self.fit_width(window, cx);
    }

    fn fit_page_action(&mut self, _: &FitPage, window: &mut Window, cx: &mut Context<Self>) {
        self.fit_page(window, cx);
    }

    fn icon(name: &'static str, size: f32) -> AnyElement {
        svg()
            .path(SharedString::from(format!("icons/{name}.svg")))
            .size(px(size))
            .text_color(rgb(TEXT))
            .into_any_element()
    }

    fn section_title(title: &'static str, note: &'static str) -> AnyElement {
        div()
            .flex()
            .items_end()
            .justify_between()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
            .child(div().text_color(rgb(MUTED)).text_xs().child(note))
            .into_any_element()
    }

    fn comparison_card(&self, title: &'static str, butter_paper_skin: bool) -> AnyElement {
        if butter_paper_skin {
            let theme = ButterTheme::light();
            return div()
                .flex_1()
                .min_w(px(0.0))
                .p_4()
                .rounded(px(BASE_RADIUS))
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .flex()
                .flex_col()
                .gap_4()
                .child(Self::section_title(
                    title,
                    "owned raw-GPUI components · Nova defaults",
                ))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(Button::new("gallery-default", "Default").theme(theme))
                        .child(
                            Button::new("gallery-outline", "Outline")
                                .variant(ButtonVariant::Outline),
                        )
                        .child(
                            Button::new("gallery-secondary", "Secondary")
                                .variant(ButtonVariant::Secondary),
                        )
                        .child(
                            Button::new("gallery-ghost", "Ghost")
                                .variant(ButtonVariant::Ghost),
                        )
                        .child(
                            Button::new("gallery-destructive", "Delete")
                                .variant(ButtonVariant::Destructive)
                                .leading_icon("trash-2"),
                        )
                        .child(Button::new("gallery-disabled", "Disabled").disabled(true)),
                )
                .child(Separator::new("gallery-separator"))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            Button::icon("gallery-command", "command", "Command palette")
                                .variant(ButtonVariant::Outline),
                        )
                        .child(
                            Button::icon("gallery-toggle-off", "mouse-pointer-2", "Select tool")
                                .toggled(false),
                        )
                        .child(
                            Button::icon("gallery-toggle-on", "highlighter", "Highlight tool")
                                .toggled(true),
                        )
                        .child(
                            Button::icon("gallery-icon-xs", "minus", "Extra-small icon")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::IconXSmall),
                        )
                        .child(
                            Button::icon("gallery-icon-sm", "minus", "Small icon")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::IconSmall),
                        )
                        .child(
                            Button::icon("gallery-icon-lg", "plus", "Large icon")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::IconLarge),
                        )
                        .child(
                            Button::new("gallery-open-menu", "Open")
                                .variant(ButtonVariant::Outline)
                                .trailing_icon("chevron-down"),
                        )
                        .child(
                            Button::new("gallery-small", "Small")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small),
                        )
                        .child(
                            Button::new("gallery-extra-small", "Extra small")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::XSmall),
                        )
                        .child(
                            Button::new("gallery-large", "Large")
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Large),
                        ),
                )
                .child(
                    div()
                        .h(px(32.0))
                        .flex()
                        .items_stretch()
                        .gap_3()
                        .child(
                            ButtonGroup::new("gallery-zoom-group", "Zoom controls")
                                .child(
                                    Button::icon("gallery-zoom-out", "zoom-out", "Zoom out")
                                        .variant(ButtonVariant::Outline)
                                        .group_position(ButtonGroupPosition::First),
                                )
                                .child(
                                    Button::new("gallery-zoom-value", "100%")
                                        .variant(ButtonVariant::Outline)
                                        .trailing_icon("chevron-down")
                                        .group_position(ButtonGroupPosition::Middle),
                                )
                                .child(
                                    Button::icon("gallery-zoom-in", "zoom-in", "Zoom in")
                                        .variant(ButtonVariant::Outline)
                                        .group_position(ButtonGroupPosition::Last),
                                ),
                        )
                        .child(
                            Separator::new("gallery-vertical-separator")
                                .orientation(SeparatorOrientation::Vertical),
                        )
                        .child(
                            Button::new("gallery-link", "Keyboard shortcut")
                                .variant(ButtonVariant::Link),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("24–36 px sizes · 10 px radius · native focus and keyboard activation · accessible roles and labels"),
                )
                .into_any_element();
        }

        // GPUI does not provide a default widget density or visual theme. Keep
        // the reviewed Butter Paper metrics in the thinnest possible wrapper.
        let radius = px(CONTROL_RADIUS);
        let gap = px(RAIL_BUTTON_GAP);
        let height = px(CONTROL_HEIGHT);
        let focus = FOCUS;

        let base = |text: &'static str| {
            div()
                .flex()
                .h(height)
                .px_3()
                .items_center()
                .justify_center()
                .rounded(radius)
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE))
                .child(text)
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .p_4()
            .rounded(px(BASE_RADIUS))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .flex()
            .flex_col()
            .gap_4()
            .child(Self::section_title(
                title,
                if butter_paper_skin {
                    "Butter Paper skin on GPUI primitives"
                } else {
                    "div · focus · actions · input handler"
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(gap)
                    .child(base("Button"))
                    .child(
                        div()
                            .flex()
                            .size(height)
                            .items_center()
                            .justify_center()
                            .rounded(radius)
                            .border_1()
                            .border_color(rgb(focus))
                            .bg(rgb(ACCENT))
                            .child(Self::icon("command", CONTROL_ICON_SIZE)),
                    )
                    .child(
                        div()
                            .flex()
                            .h(height)
                            .items_center()
                            .rounded(radius)
                            .border_1()
                            .border_color(rgb(BORDER))
                            .overflow_hidden()
                            .child(div().h_full().px_3().flex().items_center().child("Open"))
                            .child(
                                div()
                                    .h_full()
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .border_l_1()
                                    .border_color(rgb(BORDER))
                                    .child(Self::icon("chevron-down", 14.0)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(gap)
                    .items_center()
                    .child(base("Input value").w(px(140.0)).justify_start())
                    .child(base("Select  Solid"))
                    .child(base("✓  Locked")),
            )
            .child(
                div()
                    .flex()
                    .gap(gap)
                    .items_center()
                    .child(
                        div()
                            .w(px(140.0))
                            .h(px(4.0))
                            .rounded_full()
                            .bg(rgb(BORDER))
                            .child(div().w(px(92.0)).h_full().rounded_full().bg(rgb(TEXT))),
                    )
                    .child(div().text_color(rgb(MUTED)).child("64%"))
                    .child(base("Tooltip target")),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_b_2()
                            .border_color(rgb(focus))
                            .child("Document A"),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .text_color(rgb(MUTED))
                            .child("Document B"),
                    ),
            )
            .into_any_element()
    }

    fn window_title_bar(&self) -> AnyElement {
        let title = self
            .document()
            .map(|document| document.title.as_str())
            .unwrap_or("Butter Paper");
        div()
            .h(px(WINDOW_TITLE_BAR_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .text_xs()
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(MUTED))
            .child(format!("{title} — Butter Paper Dev · GPUI spike"))
            .into_any_element()
    }

    fn app_menu_bar() -> AnyElement {
        div()
            .h(px(MENU_BAR_HEIGHT))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .children(["Butter Paper", "File", "Edit", "View"].map(|label| {
                div()
                    .h(px(24.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .child(label)
            }))
            .into_any_element()
    }

    fn document_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let tabs = self
            .documents
            .iter()
            .enumerate()
            .map(|(index, document)| {
                let selected = self.active_document == Some(index);
                let label = document
                    .title
                    .strip_suffix(".pdf")
                    .unwrap_or(&document.title)
                    .to_owned();
                let width = (label.chars().count() as f32 * 7.0 + 44.0).clamp(96.0, 450.0);
                let select_entity = entity.clone();
                let close_entity = entity.clone();
                let ready_entity = entity.clone();
                let document_id = document.id;
                div()
                    .id(SharedString::from(format!("document-tab-{index}")))
                    .h(px(TAB_HEIGHT))
                    .w(px(width))
                    .flex_none()
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(CONTROL_RADIUS))
                    .bg(rgb(if selected { ACCENT } else { SURFACE }))
                    .cursor_pointer()
                    .relative()
                    .on_click(move |_, _, cx| {
                        select_entity.update(cx, |this, cx| this.select_document(index, cx));
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(label),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("close-document-{index}")))
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                close_entity.update(cx, |this, cx| this.close_document(index, cx));
                            })
                            .child(Self::icon("x", 14.0)),
                    )
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, cx| {
                                ready_entity.update(cx, |this, _| {
                                    this.announce_multi_document_tab(document_id, bounds, window);
                                });
                            },
                        )
                        .absolute()
                        .inset_0(),
                    )
            })
            .collect::<Vec<_>>();
        let template_menu_open = self.open_popup == Some(OpenPopup::TemplatePicker);
        let template_split = SplitButton::new(
            "new-template-controls",
            "New from template controls",
            Button::icon(
                "new-from-selected-template",
                "file-plus",
                format!("New from {}", self.selected_template.label()),
            )
            .variant(ButtonVariant::Outline)
            .on_click(cx.listener(|this, _, _, cx| this.create_selected_template(cx))),
            Button::icon("open-template-picker", "chevron-down", "New from template")
                .variant(ButtonVariant::Outline)
                .toggled(template_menu_open)
                .on_click(
                    cx.listener(|this, _, _, cx| this.toggle_popup(OpenPopup::TemplatePicker, cx)),
                ),
        )
        .on_popup_key(cx.listener(Self::popup_key_down));
        let template_split = if template_menu_open {
            template_split.popup(self.template_picker_menu(cx))
        } else {
            template_split
        };
        let document_actions = div()
            .h(px(TAB_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .child(
                Button::icon("open-pdf", "plus", "Open PDF")
                    .variant(ButtonVariant::Outline)
                    .on_click(cx.listener(|this, _, _, cx| this.open_pdf_dialog(cx))),
            )
            .child(template_split);
        div()
            .h(px(DOCUMENT_TAB_BAR_HEIGHT))
            .flex_none()
            .p_2()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .id("document-tab-scroll")
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(TAB_HEIGHT))
                    .flex()
                    .items_center()
                    .gap_2()
                    .overflow_x_scroll()
                    .children(tabs)
                    .when(!self.documents.is_empty(), |element| {
                        element.child(div().w(px(1.0)).h(px(20.0)).flex_none().bg(rgb(BORDER)))
                    })
                    .child(document_actions),
            )
            .into_any_element()
    }

    fn left_rail(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .p_2()
            .flex()
            .flex_col()
            .items_center()
            .border_r_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                Button::icon(
                    "toggle-thumbnail-sidebar",
                    "rail/files",
                    "Toggle page thumbnails",
                )
                .variant(ButtonVariant::Outline)
                .toggled(self.sidebar_visible)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.sidebar_visible = !this.sidebar_visible;
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn thumbnail_annotation_layer(
        scene: AnnotationScene,
        page_width: f32,
        page_height: f32,
        annotation_image: Arc<RenderImage>,
    ) -> AnyElement {
        let scale = (114.0 / page_width).min(132.0 / page_height);
        let offset_x = (114.0 - page_width * scale) / 2.0;
        let offset_y = (132.0 - page_height * scale) / 2.0;
        let transform = PageTransform::new(f64::from(page_height), f64::from(scale))
            .expect("thumbnail page geometry is positive");
        let text_layers = scene.text_boxes.iter().map(|annotation| {
            let local = transform.rect_to_local_pixels(annotation.layout_rect);
            div()
                .absolute()
                .left(px(offset_x + local.x as f32))
                .top(px(offset_y + local.y as f32))
                .w(px(local.width as f32))
                .h(px(local.height as f32))
                .overflow_hidden()
                .text_size(px(4.0))
                .text_color(rgb(annotation_color(annotation.style.color())))
                .child(annotation.content.clone())
        });
        let image_layers = scene.images.iter().map(|annotation| {
            let local = transform.rect_to_local_pixels(annotation.rect);
            div()
                .absolute()
                .left(px(offset_x + local.x as f32))
                .top(px(offset_y + local.y as f32))
                .w(px(local.width as f32))
                .h(px(local.height as f32))
                .child(
                    img(annotation_image.clone())
                        .size_full()
                        .object_fit(ObjectFit::Fill),
                )
        });
        div()
            .absolute()
            .inset_0()
            .children(image_layers)
            .children(text_layers)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let project = |pdf_point: PdfPoint| {
                            let local = transform.point_to_local_pixels(pdf_point);
                            point(
                                bounds.origin.x + px(offset_x + local.x as f32),
                                bounds.origin.y + px(offset_y + local.y as f32),
                            )
                        };
                        for annotation in &scene.rectangles {
                            if annotation.rotation_degrees == 0.0 {
                                let local = transform.rect_to_local_pixels(annotation.rect);
                                let annotation_bounds = Bounds::new(
                                    point(
                                        bounds.origin.x + px(offset_x + local.x as f32),
                                        bounds.origin.y + px(offset_y + local.y as f32),
                                    ),
                                    size(px(local.width as f32), px(local.height as f32)),
                                );
                                if let Some(color) = annotation.appearance.fill_color() {
                                    window.paint_quad(fill(
                                        annotation_bounds,
                                        rgb(annotation_color(color)).opacity(
                                            (annotation.appearance.opacity()
                                                * annotation.appearance.fill_opacity())
                                                as f32,
                                        ),
                                    ));
                                }
                                window.paint_quad(
                                    outline(
                                        annotation_bounds,
                                        rgb(annotation_color(annotation.appearance.stroke_color()))
                                            .opacity(annotation.appearance.opacity() as f32),
                                        match annotation.appearance.stroke_style() {
                                            StrokeStyle::Solid => BorderStyle::Solid,
                                            StrokeStyle::Dashed => BorderStyle::Dashed,
                                            StrokeStyle::Dotted => BorderStyle::Dashed,
                                        },
                                    )
                                    .border_widths(px(1.0)),
                                );
                            } else {
                                let points = rectangle_world_corners(
                                    annotation.rect,
                                    annotation.rotation_degrees,
                                )
                                .map(project);
                                if let Some(color) = annotation.appearance.fill_color() {
                                    let mut builder = PathBuilder::fill();
                                    builder.move_to(points[0]);
                                    for point in points.iter().skip(1) {
                                        builder.line_to(*point);
                                    }
                                    builder.close();
                                    if let Ok(path) = builder.build() {
                                        window.paint_path(
                                            path,
                                            rgb(annotation_color(color)).opacity(
                                                (annotation.appearance.opacity()
                                                    * annotation.appearance.fill_opacity())
                                                    as f32,
                                            ),
                                        );
                                    }
                                }
                                let mut builder = PathBuilder::stroke(px(1.0));
                                if matches!(
                                    annotation.appearance.stroke_style(),
                                    StrokeStyle::Dashed | StrokeStyle::Dotted
                                ) {
                                    builder = builder.dash_array(&[px(3.0), px(2.0)]);
                                }
                                builder.move_to(points[0]);
                                for point in points.iter().skip(1) {
                                    builder.line_to(*point);
                                }
                                builder.close();
                                if let Ok(path) = builder.build() {
                                    window.paint_path(
                                        path,
                                        rgb(annotation_color(annotation.appearance.stroke_color()))
                                            .opacity(annotation.appearance.opacity() as f32),
                                    );
                                }
                            }
                        }
                        for annotation in &scene.pens {
                            let paint_path =
                                build_ink_paint_path(&annotation.points, annotation.smooth_curves);
                            if paint_path.is_empty() {
                                continue;
                            }
                            let mut builder =
                                PathBuilder::stroke(px((annotation.appearance.width_pt() as f32
                                    * scale)
                                    .max(1.0)));
                            for segment in paint_path {
                                match segment {
                                    InkPaintPathSegment::MoveTo(to) => {
                                        builder.move_to(project(to));
                                    }
                                    InkPaintPathSegment::LineTo(to) => {
                                        builder.line_to(project(to));
                                    }
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
                                window.paint_path(
                                    path,
                                    rgb(annotation_color(annotation.appearance.color()))
                                        .opacity(annotation.appearance.opacity() as f32),
                                );
                            }
                        }
                        for annotation in &scene.lengths {
                            let mut builder = PathBuilder::stroke(px(1.0));
                            builder.move_to(project(annotation.start));
                            builder.line_to(project(annotation.end));
                            if let Ok(path) = builder.build() {
                                window.paint_path(path, rgb(0x1d6ed8));
                            }
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    fn thumbnail_card(args: ThumbnailCardArgs) -> AnyElement {
        let ThumbnailCardArgs {
            page,
            selected,
            image: image_path,
            loading,
            failed,
            annotation_scene,
            page_width,
            page_height,
            annotation_image,
            entity,
        } = args;
        div()
            .id(SharedString::from(format!("thumbnail-{page}")))
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.set_page(page, cx));
            })
            .h(px(195.0))
            .mr_4()
            .p_2()
            .rounded(px(BASE_RADIUS))
            .border_1()
            .border_color(rgb(if selected { 0x60a5fa } else { BORDER }))
            .bg(rgb(if selected { ACCENT } else { SURFACE }))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("Page {page}"))
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_color(rgb(MUTED))
                            .child(Self::icon("scan-line", 14.0))
                            .child(Self::icon("rotate-ccw", 14.0))
                            .child(Self::icon("rotate-cw", 14.0)),
                    ),
            )
            .child(
                div()
                    .mt_2()
                    .mx_auto()
                    .w(px(114.0))
                    .h(px(132.0))
                    .bg(rgb(PAGE))
                    .border_1()
                    .border_color(rgb(0xbdbdbd))
                    .relative()
                    .when_some(image_path, |element, image_path| {
                        element.child(img(image_path).size_full().object_fit(ObjectFit::Contain))
                    })
                    .when(loading, |element| {
                        element
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().text_xs().text_color(rgb(MUTED)).child("Rendering…"))
                    })
                    .when(failed, |element| {
                        element.flex().items_center().justify_center().child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x991b1b))
                                .child("Unavailable"),
                        )
                    })
                    .child(Self::thumbnail_annotation_layer(
                        annotation_scene,
                        page_width,
                        page_height,
                        annotation_image,
                    )),
            )
            .into_any_element()
    }

    fn thumbnail_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let page_count = self
            .document()
            .map(|document| document.page_count)
            .unwrap_or(0);
        let entity = cx.entity();
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .h(px(PRIMARY_BAND_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Page Thumbnails"),
            )
            .child(
                uniform_list(
                    "page-thumbnails",
                    page_count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let mut items = Vec::with_capacity(range.end - range.start);
                        for index in range {
                            let page = index + 1;
                            let document_state = this.document().map(|document| {
                                let page_index =
                                    u32::try_from(page.saturating_sub(1)).unwrap_or(u32::MAX);
                                (
                                    document.id,
                                    document.current_page == page,
                                    document.thumbnail_image(page),
                                    document.page_width,
                                    document.page_height,
                                    this.annotation_adapter
                                        .thumbnail_scene(document.id, page_index),
                                )
                            });
                            let (
                                selected,
                                image_path,
                                loading,
                                failed,
                                page_width,
                                page_height,
                                annotation_scene,
                            ) = if let Some((
                                document_id,
                                selected,
                                image_path,
                                page_width,
                                page_height,
                                annotation_scene,
                            )) = document_state
                            {
                                let key = (document_id, page);
                                let failed = this.thumbnail_failures.contains_key(&key);
                                if image_path.is_none()
                                    && !this.pending_thumbnail_requests.contains_key(&key)
                                    && !failed
                                {
                                    this.request_thumbnail(document_id, page, cx);
                                }
                                (
                                    selected,
                                    image_path,
                                    this.pending_thumbnail_requests.contains_key(&key),
                                    failed,
                                    page_width,
                                    page_height,
                                    annotation_scene,
                                )
                            } else {
                                (
                                    false,
                                    None,
                                    false,
                                    false,
                                    612.0,
                                    792.0,
                                    this.annotation_adapter.thumbnail_scene(0, 0),
                                )
                            };
                            items.push(div().h(px(203.0)).pt_2().pl_2().child(
                                Self::thumbnail_card(ThumbnailCardArgs {
                                    page,
                                    selected,
                                    image: image_path,
                                    loading,
                                    failed,
                                    annotation_scene,
                                    page_width,
                                    page_height,
                                    annotation_image: this.annotation_image.clone(),
                                    entity: entity.clone(),
                                }),
                            ));
                        }
                        items
                    }),
                )
                .flex_1()
                .min_h(px(0.0))
                .track_scroll(&self.thumbnail_scroll),
            )
            .into_any_element()
    }

    fn viewer_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let zoom_out_enabled = self.document().is_some() && self.zoom_percent > MIN_ZOOM_PERCENT;
        let zoom_in_enabled = self.document().is_some() && self.zoom_percent < MAX_ZOOM_PERCENT;
        let document_open = self.document().is_some();
        let continuous_active = self.scroll_mode == ScrollMode::Continuous;
        let continuous_menu_open =
            document_open && self.open_popup == Some(OpenPopup::ContinuousWheel);
        let continuous_control = SplitButton::new(
            "continuous-view-controls",
            "Continuous View controls",
            Button::icon("continuous-view", "continuous", "Use continuous page view")
                .variant(ButtonVariant::Outline)
                .disabled(!document_open)
                .toggled(continuous_active)
                .on_click(
                    cx.listener(|this, _, _, cx| this.set_scroll_mode(ScrollMode::Continuous, cx)),
                ),
            Button::icon(
                "continuous-view-settings",
                "chevron-down",
                "Continuous View settings",
            )
            .variant(ButtonVariant::Outline)
            .disabled(!document_open)
            .toggled(continuous_active)
            .on_click(
                cx.listener(|this, _, _, cx| this.toggle_popup(OpenPopup::ContinuousWheel, cx)),
            ),
        )
        .on_popup_key(cx.listener(Self::popup_key_down));
        let continuous_control = if continuous_menu_open {
            continuous_control.popup(self.wheel_mode_menu(
                OpenPopup::ContinuousWheel,
                self.continuous_wheel_mode,
                cx,
            ))
        } else {
            continuous_control
        };
        let single_page_active = self.scroll_mode == ScrollMode::SinglePage;
        let single_page_menu_open =
            document_open && self.open_popup == Some(OpenPopup::SinglePageWheel);
        let single_page_control = SplitButton::new(
            "single-page-view-controls",
            "Single Page View controls",
            Button::icon(
                "single-page-view",
                "rectangle-vertical",
                "Use single-page view",
            )
            .variant(ButtonVariant::Outline)
            .disabled(!document_open)
            .toggled(single_page_active)
            .on_click(
                cx.listener(|this, _, _, cx| this.set_scroll_mode(ScrollMode::SinglePage, cx)),
            ),
            Button::icon(
                "single-page-view-settings",
                "chevron-down",
                "Single Page View settings",
            )
            .variant(ButtonVariant::Outline)
            .disabled(!document_open)
            .toggled(single_page_active)
            .on_click(
                cx.listener(|this, _, _, cx| this.toggle_popup(OpenPopup::SinglePageWheel, cx)),
            ),
        )
        .on_popup_key(cx.listener(Self::popup_key_down));
        let single_page_control = if single_page_menu_open {
            single_page_control.popup(self.wheel_mode_menu(
                OpenPopup::SinglePageWheel,
                self.single_page_wheel_mode,
                cx,
            ))
        } else {
            single_page_control
        };
        div()
            .h(px(PRIMARY_BAND_HEIGHT))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                ButtonGroup::new("viewer-zoom-controls", "Zoom controls")
                    .child(
                        Button::icon("zoom-out", "zoom-out", "Zoom out")
                            .variant(ButtonVariant::Outline)
                            .group_position(ButtonGroupPosition::First)
                            .disabled(!zoom_out_enabled)
                            .on_click(cx.listener(|this, _, _, cx| this.change_zoom(-10.0, cx))),
                    )
                    .child(
                        Button::icon("zoom-in", "zoom-in", "Zoom in")
                            .variant(ButtonVariant::Outline)
                            .group_position(ButtonGroupPosition::Middle)
                            .disabled(!zoom_in_enabled)
                            .on_click(cx.listener(|this, _, _, cx| this.change_zoom(10.0, cx))),
                    )
                    .child(
                        Button::new("zoom-reset", format!("{:.0}%", self.zoom_percent))
                            .variant(ButtonVariant::Outline)
                            .trailing_icon("chevron-down")
                            .group_position(ButtonGroupPosition::Last)
                            .disabled(!document_open)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_zoom(100.0, ZoomPreset::Manual, cx)
                            })),
                    ),
            )
            .child(
                ButtonGroup::new("viewer-fit-controls", "Fit controls")
                    .child(
                        Button::icon("fit-width", "move-horizontal", "Fit width")
                            .variant(ButtonVariant::Outline)
                            .group_position(ButtonGroupPosition::First)
                            .disabled(!document_open)
                            .toggled(self.zoom_preset == ZoomPreset::FitWidth)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.fit_width(window, cx)),
                            ),
                    )
                    .child(
                        Button::icon("fit-page", "expand", "Fit page")
                            .variant(ButtonVariant::Outline)
                            .group_position(ButtonGroupPosition::Last)
                            .disabled(!document_open)
                            .toggled(self.zoom_preset == ZoomPreset::FitPage)
                            .on_click(cx.listener(|this, _, window, cx| this.fit_page(window, cx))),
                    ),
            )
            .child(continuous_control)
            .child(single_page_control)
            .into_any_element()
    }

    fn annotation_overlay(
        &self,
        document_id: u64,
        page: usize,
        page_height: f32,
        zoom_percent: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let document_id = self.editor_overlay_document_id.unwrap_or(document_id);
        let transform = Self::annotation_transform(page_height, zoom_percent);
        let page_index = u32::try_from(page.saturating_sub(1)).unwrap_or(u32::MAX);
        let scene = self
            .annotation_adapter
            .document_scene(document_id, page_index);
        let text_layers = scene
            .text_boxes
            .iter()
            .map(|annotation| {
                let local = transform.rect_to_local_pixels(annotation.layout_rect);
                div()
                    .absolute()
                    .left(px(local.x as f32))
                    .top(px(local.y as f32))
                    .w(px(local.width as f32))
                    .h(px(local.height as f32))
                    .overflow_hidden()
                    .font_family(
                        presentation_font_family(annotation.style.font_family()).to_owned(),
                    )
                    .text_size(px((annotation.style.font_size_pt() as f32 * zoom_percent
                        / 100.0)
                        .max(8.0)))
                    .text_color(
                        rgb(annotation_color(annotation.style.color()))
                            .opacity(annotation.style.opacity() as f32),
                    )
                    .when(annotation.selected, |element| {
                        element.border_1().border_color(rgb(0x2563eb))
                    })
                    .child(annotation.content.clone())
            })
            .collect::<Vec<_>>();
        let length_labels = scene
            .lengths
            .iter()
            .filter(|annotation| annotation.show_caption)
            .map(|annotation| {
                let midpoint = PdfPoint {
                    x: (annotation.start.x + annotation.end.x) / 2.0,
                    y: (annotation.start.y + annotation.end.y) / 2.0,
                };
                let local = transform.point_to_local_pixels(midpoint);
                div()
                    .absolute()
                    .left(px(local.x as f32 - 28.0))
                    .top(px(local.y as f32 - 24.0))
                    .px_1()
                    .rounded(px(2.0))
                    .bg(rgb(0xffffff).opacity(0.9))
                    .text_xs()
                    .text_color(rgb(0x1e3a5f))
                    .child(annotation.caption.clone())
            })
            .collect::<Vec<_>>();
        let annotation_image = self.annotation_image.clone();
        let annotation_overlay_paint = self.annotation_overlay_paint.clone();
        let annotation_image_atlas_paint = self.annotation_image_atlas_paint.clone();
        let image_layers = scene
            .images
            .iter()
            .map(|annotation| {
                let local = transform.rect_to_local_pixels(annotation.rect);
                div()
                    .absolute()
                    .left(px(local.x as f32))
                    .top(px(local.y as f32))
                    .w(px(local.width as f32))
                    .h(px(local.height as f32))
                    .when(annotation.selected, |element| {
                        element.border_1().border_color(rgb(0x2563eb))
                    })
            })
            .collect::<Vec<_>>();
        let text_input_active = scene
            .text_boxes
            .iter()
            .any(|annotation| annotation.selected);
        let entity = cx.entity();
        let surface_entity = entity.clone();
        let down_entity = entity.clone();
        let move_entity = entity.clone();
        let up_entity = entity.clone();
        let input_entity = entity;
        let annotation_focus = self.annotation_focus.clone();
        let drawing_annotation = self.annotation_adapter.tool().uses_crosshair();
        let snap_guide = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.native_editing_v5.as_ref())
            .filter(|state| state.snap_guide_active)
            .map(|state| {
                let rect = state.plan.snap_transform.expected_final_rect;
                (rect.x, rect.y)
            });

        div()
            .absolute()
            .inset_0()
            .when(drawing_annotation, |element| element.cursor_crosshair())
            .children(image_layers)
            .children(text_layers)
            .children(length_labels)
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        surface_entity.update(cx, |this, _| {
                            this.announce_native_input_surface(
                                document_id,
                                page,
                                bounds,
                                page_height,
                                zoom_percent,
                                window,
                            );
                        });
                        window.on_mouse_event({
                            let entity = down_entity.clone();
                            move |event: &MouseDownEvent, _, window, cx| {
                                if event.button != MouseButton::Left
                                    || !bounds.contains(&event.position)
                                {
                                    return;
                                }
                                entity.update(cx, |this, cx| {
                                    this.annotation_mouse_down(
                                        AnnotationPointerLocation {
                                            document_id,
                                            page,
                                            bounds,
                                            transform,
                                        },
                                        event.position,
                                        window,
                                        cx,
                                    );
                                });
                            }
                        });
                        window.on_mouse_event({
                            let entity = move_entity.clone();
                            move |event: &MouseMoveEvent, _, _, cx| {
                                if !event.dragging() {
                                    entity.update(cx, |this, cx| {
                                        if this.annotation_adapter.is_click_placement_pending() {
                                            this.annotation_mouse_move(
                                                document_id,
                                                page,
                                                bounds,
                                                event.position,
                                                transform,
                                                cx,
                                            );
                                        } else {
                                            this.annotation_capture_lost(document_id, page, cx);
                                        }
                                    });
                                    return;
                                }
                                entity.update(cx, |this, cx| {
                                    this.annotation_mouse_move(
                                        document_id,
                                        page,
                                        bounds,
                                        event.position,
                                        transform,
                                        cx,
                                    );
                                });
                            }
                        });
                        window.on_mouse_event({
                            let entity = up_entity.clone();
                            move |event: &MouseUpEvent, _, _, cx| {
                                if event.button != MouseButton::Left {
                                    return;
                                }
                                entity.update(cx, |this, cx| {
                                    this.annotation_mouse_up(
                                        document_id,
                                        page,
                                        bounds,
                                        event.position,
                                        transform,
                                        cx,
                                    );
                                });
                            }
                        });

                        let annotation_render_image =
                            (!scene.images.is_empty()).then(|| annotation_image.clone());
                        let mut annotation_atlas_upload_queued = false;
                        if let Some(render_image) = annotation_render_image.as_ref() {
                            for annotation in &scene.images {
                                let local = transform.rect_to_local_pixels(annotation.rect);
                                let image_bounds = Bounds {
                                    origin: point(
                                        bounds.origin.x + px(local.x as f32),
                                        bounds.origin.y + px(local.y as f32),
                                    ),
                                    size: size(px(local.width as f32), px(local.height as f32)),
                                };
                                match window.paint_image(
                                    image_bounds,
                                    image_bounds,
                                    Corners::default(),
                                    render_image.clone(),
                                    0,
                                    false,
                                ) {
                                    Ok(()) => annotation_atlas_upload_queued = true,
                                    Err(error) => {
                                        perf::emit(
                                            "annotation-image-paint-failed",
                                            perf::fields([("error", json!(error.to_string()))]),
                                        );
                                    }
                                }
                            }
                        }

                        for annotation in &scene.rectangles {
                            let local = transform.rect_to_local_pixels(annotation.rect);
                            let annotation_bounds = Bounds {
                                origin: point(
                                    bounds.origin.x + px(local.x as f32),
                                    bounds.origin.y + px(local.y as f32),
                                ),
                                size: size(px(local.width as f32), px(local.height as f32)),
                            };
                            let fill_opacity = (annotation.appearance.opacity()
                                * annotation.appearance.fill_opacity())
                                as f32;
                            let stroke_width = px((annotation.appearance.stroke_width_pt() as f32
                                * zoom_percent
                                / 100.0)
                                .max(1.0));
                            if annotation.rotation_degrees == 0.0 {
                                if let Some(fill_color) = annotation.appearance.fill_color() {
                                    window.paint_quad(fill(
                                        annotation_bounds,
                                        rgb(annotation_color(fill_color)).opacity(fill_opacity),
                                    ));
                                }
                                window.paint_quad(
                                    outline(
                                        annotation_bounds,
                                        rgb(annotation_color(annotation.appearance.stroke_color()))
                                            .opacity(annotation.appearance.opacity() as f32),
                                        match annotation.appearance.stroke_style() {
                                            StrokeStyle::Solid => BorderStyle::Solid,
                                            StrokeStyle::Dashed => BorderStyle::Dashed,
                                            StrokeStyle::Dotted => BorderStyle::Dashed,
                                        },
                                    )
                                    .border_widths(stroke_width),
                                );
                            } else {
                                let points = rectangle_world_corners(
                                    annotation.rect,
                                    annotation.rotation_degrees,
                                )
                                .map(|point| {
                                    let local = transform.point_to_local_pixels(point);
                                    gpui::point(
                                        bounds.origin.x + px(local.x as f32),
                                        bounds.origin.y + px(local.y as f32),
                                    )
                                });
                                if let Some(fill_color) = annotation.appearance.fill_color() {
                                    let mut builder = PathBuilder::fill();
                                    builder.move_to(points[0]);
                                    for point in points.iter().skip(1) {
                                        builder.line_to(*point);
                                    }
                                    builder.close();
                                    if let Ok(path) = builder.build() {
                                        window.paint_path(
                                            path,
                                            rgb(annotation_color(fill_color)).opacity(fill_opacity),
                                        );
                                    }
                                }
                                let mut builder = PathBuilder::stroke(stroke_width);
                                if matches!(
                                    annotation.appearance.stroke_style(),
                                    StrokeStyle::Dashed | StrokeStyle::Dotted
                                ) {
                                    builder = builder.dash_array(&[px(5.0), px(4.0)]);
                                }
                                builder.move_to(points[0]);
                                for point in points.iter().skip(1) {
                                    builder.line_to(*point);
                                }
                                builder.close();
                                if let Ok(path) = builder.build() {
                                    window.paint_path(
                                        path,
                                        rgb(annotation_color(annotation.appearance.stroke_color()))
                                            .opacity(annotation.appearance.opacity() as f32),
                                    );
                                }
                            }

                            if annotation.selected {
                                if annotation.rotation_degrees == 0.0 {
                                    let chrome_bounds = Bounds {
                                        origin: point(
                                            annotation_bounds.origin.x - px(3.0),
                                            annotation_bounds.origin.y - px(3.0),
                                        ),
                                        size: size(
                                            annotation_bounds.size.width + px(6.0),
                                            annotation_bounds.size.height + px(6.0),
                                        ),
                                    };
                                    window.paint_quad(outline(
                                        chrome_bounds,
                                        rgb(0x2563eb),
                                        BorderStyle::default(),
                                    ));
                                } else {
                                    let points = rectangle_world_corners(
                                        annotation.rect,
                                        annotation.rotation_degrees,
                                    )
                                    .map(|point| {
                                        let local = transform.point_to_local_pixels(point);
                                        gpui::point(
                                            bounds.origin.x + px(local.x as f32),
                                            bounds.origin.y + px(local.y as f32),
                                        )
                                    });
                                    let mut builder = PathBuilder::stroke(px(1.0));
                                    builder.move_to(points[0]);
                                    for point in points.iter().skip(1) {
                                        builder.line_to(*point);
                                    }
                                    builder.close();
                                    if let Ok(path) = builder.build() {
                                        window.paint_path(path, rgb(0x2563eb));
                                    }
                                }
                                let handle_size = px(7.0);
                                for handle in RectangleResizeHandle::ALL {
                                    let local =
                                        transform.point_to_local_pixels(handle.world_point(
                                            annotation.rect,
                                            annotation.rotation_degrees,
                                        ));
                                    let handle_bounds = Bounds {
                                        origin: point(
                                            bounds.origin.x + px(local.x as f32)
                                                - handle_size / 2.0,
                                            bounds.origin.y + px(local.y as f32)
                                                - handle_size / 2.0,
                                        ),
                                        size: size(handle_size, handle_size),
                                    };
                                    window.paint_quad(
                                        fill(handle_bounds, rgb(0xffffff))
                                            .border_widths(px(1.0))
                                            .border_color(rgb(0x2563eb)),
                                    );
                                }
                                let north = transform.point_to_local_pixels(
                                    RectangleResizeHandle::North
                                        .world_point(annotation.rect, annotation.rotation_degrees),
                                );
                                let rotation = transform.point_to_local_pixels(
                                    rectangle_rotation_handle_world_point(
                                        annotation.rect,
                                        annotation.rotation_degrees,
                                        transform
                                            .tolerance_points(ROTATION_HANDLE_OFFSET_CSS_PX)
                                            .expect("rotation handle offset is valid"),
                                    ),
                                );
                                let mut connector = PathBuilder::stroke(px(1.0));
                                connector.move_to(point(
                                    bounds.origin.x + px(north.x as f32),
                                    bounds.origin.y + px(north.y as f32),
                                ));
                                connector.line_to(point(
                                    bounds.origin.x + px(rotation.x as f32),
                                    bounds.origin.y + px(rotation.y as f32),
                                ));
                                if let Ok(path) = connector.build() {
                                    window.paint_path(path, rgb(0x2563eb));
                                }
                                let rotation_size = px(9.0);
                                let rotation_bounds = Bounds {
                                    origin: point(
                                        bounds.origin.x + px(rotation.x as f32)
                                            - rotation_size / 2.0,
                                        bounds.origin.y + px(rotation.y as f32)
                                            - rotation_size / 2.0,
                                    ),
                                    size: size(rotation_size, rotation_size),
                                };
                                window.paint_quad(
                                    fill(rotation_bounds, rgb(0xffffff))
                                        .corner_radii(rotation_size / 2.0)
                                        .border_widths(px(1.0))
                                        .border_color(rgb(0x2563eb)),
                                );
                            }
                        }

                        if let Some((guide_x, guide_y)) = snap_guide {
                            let vertical =
                                transform.point_to_local_pixels(PdfPoint { x: guide_x, y: 0.0 });
                            let horizontal =
                                transform.point_to_local_pixels(PdfPoint { x: 0.0, y: guide_y });
                            let mut vertical_builder = PathBuilder::stroke(px(1.0));
                            vertical_builder.move_to(point(
                                bounds.origin.x + px(vertical.x as f32),
                                bounds.origin.y,
                            ));
                            vertical_builder.line_to(point(
                                bounds.origin.x + px(vertical.x as f32),
                                bounds.origin.y + bounds.size.height,
                            ));
                            if let Ok(path) = vertical_builder.build() {
                                window.paint_path(path, rgb(0x0ea5e9));
                            }
                            let mut horizontal_builder = PathBuilder::stroke(px(1.0));
                            horizontal_builder.move_to(point(
                                bounds.origin.x,
                                bounds.origin.y + px(horizontal.y as f32),
                            ));
                            horizontal_builder.line_to(point(
                                bounds.origin.x + bounds.size.width,
                                bounds.origin.y + px(horizontal.y as f32),
                            ));
                            if let Ok(path) = horizontal_builder.build() {
                                window.paint_path(path, rgb(0x0ea5e9));
                            }
                            surface_entity.update(cx, |this, _| {
                                if let Some(state) = this
                                    .perf_scenario
                                    .as_mut()
                                    .and_then(|scenario| scenario.native_editing_v5.as_mut())
                                {
                                    state.snap_guide_painted = true;
                                }
                            });
                        }

                        for annotation in &scene.pens {
                            let paint_path =
                                build_ink_paint_path(&annotation.points, annotation.smooth_curves);
                            if paint_path.is_empty() {
                                continue;
                            }
                            let mut builder =
                                PathBuilder::stroke(px((annotation.appearance.width_pt() as f32
                                    * zoom_percent
                                    / 100.0)
                                    .max(1.0)));
                            let project = |sample: PdfPoint| {
                                let sample = transform.point_to_local_pixels(sample);
                                point(
                                    bounds.origin.x + px(sample.x as f32),
                                    bounds.origin.y + px(sample.y as f32),
                                )
                            };
                            for segment in paint_path {
                                match segment {
                                    InkPaintPathSegment::MoveTo(to) => {
                                        builder.move_to(project(to));
                                    }
                                    InkPaintPathSegment::LineTo(to) => {
                                        builder.line_to(project(to));
                                    }
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
                                window.paint_path(
                                    path,
                                    rgb(annotation_color(annotation.appearance.color()))
                                        .opacity(annotation.appearance.opacity() as f32),
                                );
                            }
                        }

                        let mut submitted_length_path_ids = Vec::new();
                        for annotation in &scene.lengths {
                            let start = transform.point_to_local_pixels(annotation.start);
                            let end = transform.point_to_local_pixels(annotation.end);
                            let start = point(
                                bounds.origin.x + px(start.x as f32),
                                bounds.origin.y + px(start.y as f32),
                            );
                            let end = point(
                                bounds.origin.x + px(end.x as f32),
                                bounds.origin.y + px(end.y as f32),
                            );
                            let mut builder = PathBuilder::stroke(px(2.0));
                            builder.move_to(start);
                            builder.line_to(end);
                            if let Ok(path) = builder.build() {
                                window.paint_path(path, rgb(0x1d6ed8));
                                submitted_length_path_ids.push(annotation.id.as_str().to_owned());
                            }
                            if annotation.selected {
                                for endpoint in [start, end] {
                                    window.paint_quad(
                                        fill(
                                            Bounds::new(
                                                point(endpoint.x - px(4.0), endpoint.y - px(4.0)),
                                                size(px(8.0), px(8.0)),
                                            ),
                                            rgb(0xffffff),
                                        )
                                        .border_widths(px(1.0))
                                        .border_color(rgb(0x2563eb)),
                                    );
                                }
                            }
                        }

                        #[cfg(feature = "benchmark-evidence")]
                        let atlas_observation = annotation_render_image.map(|render_image| {
                            AnnotationImageAtlasPaintObservation {
                                document_id,
                                page_index,
                                scene_revision: scene.revision,
                                render_image_id: render_image.id.0,
                                // WGPU's `paint_image` returns only after
                                // `PlatformAtlas::get_or_insert_with` has
                                // accepted the exact frame bytes and queued
                                // the atlas upload. GPUI CE's test-only
                                // `contains` hook is not implemented by its
                                // WGPU atlas, so the successful production
                                // paint call is the portable Linux receipt.
                                atlas_entry_observed: annotation_atlas_upload_queued,
                                decoded_bgra_bytes: render_image.as_bytes(0).map_or(0, <[u8]>::len),
                            }
                        });
                        #[cfg(not(feature = "benchmark-evidence"))]
                        let atlas_observation: Option<
                            AnnotationImageAtlasPaintObservation,
                        > = None;
                        *annotation_image_atlas_paint
                            .lock()
                            .expect("annotation image atlas marker lock") = atlas_observation;

                        *annotation_overlay_paint
                            .lock()
                            .expect("annotation presentation marker lock") =
                            Some(AnnotationOverlayPaintObservation::from_scene(
                                document_id,
                                page_index,
                                &scene,
                                submitted_length_path_ids,
                            ));

                        if text_input_active {
                            window.handle_input(
                                &annotation_focus,
                                ElementInputHandler::new(bounds, input_entity.clone()),
                                cx,
                            );
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    fn native_editor_scale_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let visible = self.perf_scenario.as_ref().is_some_and(|scenario| {
            matches!(
                scenario.comparison_phase,
                ComparisonPhase::AwaitingNativeEditorInput {
                    stage: NativeEditorStage::Scale,
                    ..
                } | ComparisonPhase::NativeEditorInput {
                    stage: NativeEditorStage::Scale,
                    ..
                }
            )
        });
        if !visible {
            return None;
        }
        let entity = cx.entity();
        let paint_entity = entity.clone();
        Some(
            div()
                .id("comparison-length-scale")
                .absolute()
                .top_3()
                .left_3()
                .w(px(184.0))
                .h(px(34.0))
                .rounded(px(CONTROL_RADIUS))
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE))
                .text_sm()
                .text_color(rgb(TEXT))
                .flex()
                .items_center()
                .justify_center()
                .child("Set scale: 72 pt = 1 m")
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            paint_entity.update(cx, |this, _| {
                                let phase = this
                                    .perf_scenario
                                    .as_ref()
                                    .map(|scenario| scenario.comparison_phase)
                                    .unwrap_or(ComparisonPhase::Idle);
                                let ComparisonPhase::AwaitingNativeEditorInput {
                                    stage: NativeEditorStage::Scale,
                                    history_before,
                                } = phase
                                else {
                                    return;
                                };
                                let viewport = window.viewport_size();
                                perf::emit(
                                    "native-input-ready",
                                    perf::fields([
                                        ("command_id", json!("length:set-scale")),
                                        (
                                            "control",
                                            json!({
                                                "control_id": "comparison-length-scale",
                                                "window_logical_size": {
                                                    "width": f32::from(viewport.width),
                                                    "height": f32::from(viewport.height),
                                                },
                                                "bounds": {
                                                    "x": f32::from(bounds.origin.x),
                                                    "y": f32::from(bounds.origin.y),
                                                    "width": f32::from(bounds.size.width),
                                                    "height": f32::from(bounds.size.height),
                                                },
                                            }),
                                        ),
                                    ]),
                                );
                                if let Some(scenario) = this.perf_scenario.as_mut() {
                                    scenario.comparison_phase =
                                        ComparisonPhase::NativeEditorInput {
                                            stage: NativeEditorStage::Scale,
                                            coordinate_samples: 0,
                                            history_before,
                                        };
                                }
                            });
                            window.on_mouse_event({
                                let entity = entity.clone();
                                move |event: &MouseDownEvent, _, _, cx| {
                                    if event.button != MouseButton::Left
                                        || !bounds.contains(&event.position)
                                    {
                                        return;
                                    }
                                    entity.update(cx, |this, cx| {
                                        this.apply_native_editor_scale(cx);
                                    });
                                }
                            });
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element(),
        )
    }

    fn announce_native_v5_control(
        &mut self,
        bounds: Bounds<Pixels>,
        window: &Window,
        expected_stage: NativeEditingV5Stage,
        stage_name: &'static str,
        control_id: &'static str,
    ) {
        let Some((kind, ready)) = self.perf_scenario.as_ref().and_then(|scenario| {
            let state = scenario.native_editing_v5.as_ref()?;
            (state.stage == expected_stage).then_some((scenario.kind, state.native_ready_emitted))
        }) else {
            return;
        };
        if ready {
            return;
        }
        #[cfg(feature = "benchmark-evidence")]
        let samples_before = window.input_latency_snapshot().latency_histogram.len();
        let viewport = window.viewport_size();
        let command_id = match kind {
            PerfScenarioKind::NativePropertyEditUndo => "annotation:native-property-edit-undo",
            PerfScenarioKind::NativeSnapTransform => "annotation:native-snap-transform-120hz",
            _ => return,
        };
        perf::emit(
            "native-input-ready",
            perf::fields([
                ("command_id", json!(command_id)),
                ("stage", json!(stage_name)),
                (
                    "control",
                    json!({
                        "control_id": control_id,
                        "window_logical_size": {
                            "width": f32::from(viewport.width),
                            "height": f32::from(viewport.height),
                        },
                        "bounds": {
                            "x": f32::from(bounds.origin.x),
                            "y": f32::from(bounds.origin.y),
                            "width": f32::from(bounds.size.width),
                            "height": f32::from(bounds.size.height),
                        },
                        "point": {
                            "x": f32::from(bounds.origin.x + bounds.size.width / 2.0),
                            "y": f32::from(bounds.origin.y + bounds.size.height / 2.0),
                        },
                    }),
                ),
            ]),
        );
        if let Some(state) = self
            .perf_scenario
            .as_mut()
            .and_then(|scenario| scenario.native_editing_v5.as_mut())
        {
            state.native_ready_emitted = true;
            #[cfg(feature = "benchmark-evidence")]
            if state.input_latency_samples_before.is_none() {
                state.input_latency_samples_before = Some(samples_before);
            }
        }
    }

    fn native_v5_property_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let stage = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.native_editing_v5.as_ref())?
            .stage;
        let (label, stage_name, control_id, clickable) = match stage {
            NativeEditingV5Stage::PropertyTrigger => (
                "Rectangle properties",
                "properties-trigger",
                "native-v5-properties-trigger",
                true,
            ),
            NativeEditingV5Stage::PropertyChoice => (
                "Stroke width · 4 pt",
                "property-stroke-width-4pt",
                "native-v5-stroke-width-4pt",
                true,
            ),
            NativeEditingV5Stage::PropertyUndo => (
                "Undo property edit · Ctrl+Z",
                "undo-shortcut",
                "native-v5-property-undo-ready",
                false,
            ),
            _ => return None,
        };
        let entity = cx.entity();
        let ready_entity = entity.clone();
        Some(
            div()
                .id(control_id)
                .absolute()
                .top_3()
                .right_3()
                .w(px(210.0))
                .h(px(36.0))
                .rounded(px(CONTROL_RADIUS))
                .border_1()
                .border_color(rgb(if clickable { BORDER } else { 0x0ea5e9 }))
                .bg(rgb(SURFACE))
                .when(clickable, |element| element.cursor_pointer())
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .child(label)
                .when(clickable, |element| {
                    element.on_click(move |_, window, cx| {
                        entity.update(cx, |this, cx| {
                            this.annotation_focus.focus(window, cx);
                            match stage {
                                NativeEditingV5Stage::PropertyTrigger => {
                                    if let Some(state) = this
                                        .perf_scenario
                                        .as_mut()
                                        .and_then(|scenario| scenario.native_editing_v5.as_mut())
                                    {
                                        state.stage = NativeEditingV5Stage::PropertyChoice;
                                        state.native_ready_emitted = false;
                                    }
                                    cx.notify();
                                }
                                NativeEditingV5Stage::PropertyChoice => {
                                    this.apply_native_v5_property(cx)
                                }
                                _ => {}
                            }
                        });
                    })
                })
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            ready_entity.update(cx, |this, _| {
                                this.announce_native_v5_control(
                                    bounds, window, stage, stage_name, control_id,
                                );
                            });
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element(),
        )
    }

    fn native_v5_snap_keyboard_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let stage = self
            .perf_scenario
            .as_ref()
            .and_then(|scenario| scenario.native_editing_v5.as_ref())?
            .stage;
        let (label, stage_name, control_id) = match stage {
            NativeEditingV5Stage::SnapUndo => (
                "Undo snap · Ctrl+Z",
                "undo-shortcut",
                "native-v5-snap-undo-ready",
            ),
            NativeEditingV5Stage::SnapRedo => (
                "Redo snap · Ctrl+Shift+Z",
                "redo-shortcut",
                "native-v5-snap-redo-ready",
            ),
            _ => return None,
        };
        let ready_entity = cx.entity();
        Some(
            div()
                .id(control_id)
                .absolute()
                .top_3()
                .right_3()
                .w(px(220.0))
                .h(px(36.0))
                .rounded(px(CONTROL_RADIUS))
                .border_1()
                .border_color(rgb(0x0ea5e9))
                .bg(rgb(SURFACE))
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .child(label)
                .child(
                    canvas(
                        |_, _, _| (),
                        move |bounds, _, window, cx| {
                            ready_entity.update(cx, |this, _| {
                                this.announce_native_v5_control(
                                    bounds, window, stage, stage_name, control_id,
                                );
                            });
                        },
                    )
                    .absolute()
                    .inset_0(),
                )
                .into_any_element(),
        )
    }

    fn document_viewport(&self, cx: &mut Context<Self>) -> AnyElement {
        let image_path = self.document().and_then(PdfDocument::viewport_image);
        let document_state = self.document().map(|document| {
            (
                document.id,
                document.current_page,
                document.page_width * self.zoom_percent / 100.0,
                document.page_height * self.zoom_percent / 100.0,
            )
        });
        let (document_id, current_page, page_width, page_height) =
            document_state.unwrap_or((0, 1, 655.0, 758.0));
        let empty = self.document().is_none();
        let opening = !self.pending_open_requests.is_empty();
        let viewport_loading = self
            .document()
            .is_some_and(|document| self.pending_viewport_requests.contains_key(&document.id));
        let loading_label = self.document().map(|document| {
            format!(
                "Rendering page {} at {:.0}%…",
                document.current_page, self.zoom_percent
            )
        });
        let error = self.document_error.clone();
        let page_mode = self.scroll_mode;
        let zoom_percent = self.zoom_percent;
        let show_continuous_scrollbar = !empty && page_mode == ScrollMode::Continuous;
        let visible_tiles = self.visible_tiles(current_page);
        let entity = cx.entity();
        let page_surface = if empty || page_mode == ScrollMode::SinglePage {
            div()
                .id("document-scroll")
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .overflow_scroll()
                .track_scroll(&self.document_scroll)
                .flex()
                .items_start()
                .justify_center()
                .p(px(PAGE_LAYOUT_GAP))
                .child(
                    div()
                        .id("single-page-surface")
                        .relative()
                        .w(px(page_width))
                        .h(px(page_height))
                        .flex_none()
                        .bg(rgb(PAGE))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .shadow_lg()
                        .on_scroll_wheel(cx.listener(Self::viewport_wheel))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when_some(image_path, |element, image_path| {
                            element
                                .child(img(image_path).size_full().object_fit(ObjectFit::Contain))
                        })
                        .children(visible_tiles)
                        .when(empty, |element| {
                            element.child(div().text_color(rgb(MUTED)).child(if opening {
                                "Opening PDF…"
                            } else {
                                "Open a PDF with the + button"
                            }))
                        })
                        .when(!empty, |element| {
                            element.child(self.annotation_overlay(
                                document_id,
                                current_page,
                                page_height,
                                zoom_percent,
                                cx,
                            ))
                        }),
                )
                .into_any_element()
        } else {
            let dynamic_fidelity = self
                .perf_scenario
                .as_ref()
                .is_some_and(|scenario| scenario.kind == PerfScenarioKind::DynamicFidelity);
            let dynamic_paint_capture_sequence = self.dynamic_fidelity_paint_capture_sequence;
            let dynamic_render_generation = self.continuous_plan_generation;
            let dynamic_painted_pages = self.dynamic_fidelity_painted_pages.clone();
            let visible_pages = self
                .continuous_page_layouts
                .iter()
                .filter(|layout| self.continuous_visible_pages.contains(&layout.page))
                .map(|layout| {
                    let page = layout.page;
                    let page_width = layout.logical_rect.width;
                    let page_height = layout.logical_rect.height;
                    let image_path = self
                        .document()
                        .filter(|document| document.id == document_id)
                        .and_then(|document| {
                            document.cached_viewport_image(
                                page,
                                zoom_percent,
                                self.display_scale_factor,
                            )
                        });
                    let pixel_width = self
                        .document()
                        .map(|document| {
                            document.viewport_pixel_width_for(
                                page,
                                zoom_percent,
                                self.display_scale_factor,
                            )
                        })
                        .unwrap_or_default();
                    let failed =
                        self.page_surface_failures
                            .contains_key(&(document_id, page, pixel_width));
                    let selected = page == current_page;
                    let page_entity = entity.clone();
                    let tiles = self.visible_tiles(page);
                    let (page_width_points, page_height_points) = self
                        .document()
                        .map(|document| document.page_size(page))
                        .unwrap_or((page_width, page_height));
                    let current_raster_ready = image_path.is_some();
                    let painted_pages = dynamic_painted_pages.clone();
                    div()
                        .id(SharedString::from(format!(
                            "continuous-page-surface-{page}"
                        )))
                        .absolute()
                        .left(px(layout.logical_rect.x))
                        .top(px(layout.logical_rect.y))
                        .w(px(page_width))
                        .h(px(page_height))
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                            if should_activate_page_from_surface(current_page, page) {
                                page_entity.update(cx, |this, cx| this.set_page(page, cx));
                            }
                        })
                        .bg(rgb(PAGE))
                        .border_1()
                        .border_color(rgb(if selected { 0x60a5fa } else { BORDER }))
                        .shadow_lg()
                        .flex()
                        .items_center()
                        .justify_center()
                        .when_some(image_path.clone(), |element, image_path| {
                            element
                                .child(img(image_path).size_full().object_fit(ObjectFit::Contain))
                        })
                        .children(tiles)
                        .when(failed, |element| {
                            element.child(
                                div()
                                    .text_color(rgb(0x991b1b))
                                    .child(format!("Page {page} unavailable")),
                            )
                        })
                        .when(!failed && image_path.is_none(), |element| {
                            element.child(
                                div()
                                    .text_color(rgb(MUTED))
                                    .child(format!("Rendering page {page}…")),
                            )
                        })
                        .child(self.annotation_overlay(
                            document_id,
                            page,
                            page_height,
                            zoom_percent,
                            cx,
                        ))
                        .when(dynamic_fidelity, |element| {
                            element.child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, _, _, _| {
                                        painted_pages
                                            .lock()
                                            .expect("dynamic fidelity paint observation lock")
                                            .insert(
                                                (dynamic_paint_capture_sequence, page),
                                                DynamicFidelityPaintObservation {
                                                    capture_sequence:
                                                        dynamic_paint_capture_sequence,
                                                    page_number: page,
                                                    outer_bounds_window_logical: ViewerRect::new(
                                                        f32::from(bounds.origin.x),
                                                        f32::from(bounds.origin.y),
                                                        f32::from(bounds.size.width),
                                                        f32::from(bounds.size.height),
                                                    ),
                                                    page_width_points,
                                                    page_height_points,
                                                    render_generation: dynamic_render_generation,
                                                    current_raster_ready,
                                                },
                                            );
                                    },
                                )
                                .absolute()
                                .inset_0(),
                            )
                        })
                        .into_any_element()
                })
                .collect::<Vec<_>>();
            let content_width = self
                .continuous_page_layouts
                .iter()
                .map(|layout| layout.logical_rect.x + layout.logical_rect.width + PAGE_LAYOUT_GAP)
                .fold(1.0, f32::max);
            div()
                .id("continuous-pages-scroll")
                .flex_1()
                .min_w(px(0.0))
                .h_full()
                .overflow_scroll()
                .track_scroll(&self.continuous_scroll)
                // Observe the full viewport, including inter-page gaps and
                // not-yet-painted surfaces. A page-local listener can lose
                // physical wheel receipts as content moves under the fixed
                // native pointer.
                .on_scroll_wheel(cx.listener(Self::viewport_wheel))
                .child(
                    div()
                        .id("continuous-pages-content")
                        .relative()
                        .w(px(content_width))
                        .h(px(self.continuous_total_height.max(1.0)))
                        .flex_none()
                        .children(visible_pages),
                )
                .into_any_element()
        };
        let continuous_scrollbar = show_continuous_scrollbar.then(|| {
            let viewport_height = self
                .last_window_logical_size
                .map(|window_size| {
                    (window_size[1]
                        - WINDOW_TITLE_BAR_HEIGHT
                        - MENU_BAR_HEIGHT
                        - DOCUMENT_TAB_BAR_HEIGHT
                        - PRIMARY_BAND_HEIGHT
                        - VIEWPORT_TOP_BORDER_WIDTH)
                        .max(1.0)
                })
                .unwrap_or(1.0);
            let (thumb_top, thumb_height) = continuous_scrollbar_thumb(
                viewport_height,
                self.continuous_total_height,
                f32::from(self.continuous_scroll.offset().y),
            );
            div()
                .id("continuous-pages-scrollbar")
                .w(px(VIEWPORT_SCROLLBAR_WIDTH))
                .h_full()
                .flex_none()
                .relative()
                .border_l_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE))
                .on_scroll_wheel(cx.listener(Self::viewport_wheel))
                .child(
                    div()
                        .absolute()
                        .top(px(thumb_top))
                        .left(px(3.0))
                        .right(px(3.0))
                        .h(px(thumb_height))
                        .rounded_full()
                        .bg(rgb(FOCUS)),
                )
                .into_any_element()
        });
        div()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .relative()
            .flex()
            .border_t_1()
            .border_color(rgb(BORDER))
            .bg(rgb(VIEWPORT))
            .child(page_surface)
            .when_some(continuous_scrollbar, |element, scrollbar| {
                element.child(scrollbar)
            })
            .when_some(self.native_v5_property_control(cx), |element, control| {
                element.child(control)
            })
            .when_some(
                self.native_v5_snap_keyboard_control(cx),
                |element, control| element.child(control),
            )
            .when_some(self.native_editor_scale_control(cx), |element, control| {
                element.child(control)
            })
            .when(viewport_loading, |element| {
                element.child(
                    div()
                        .absolute()
                        .top_3()
                        .right_3()
                        .px_3()
                        .py_2()
                        .rounded(px(CONTROL_RADIUS))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE))
                        .text_color(rgb(MUTED))
                        .child(loading_label.unwrap_or_else(|| "Rendering…".into())),
                )
            })
            .when_some(error, |element, error| {
                element.child(
                    div()
                        .absolute()
                        .top_3()
                        .left_3()
                        .right_3()
                        .p_3()
                        .rounded(px(CONTROL_RADIUS))
                        .border_1()
                        .border_color(rgb(0xfca5a5))
                        .bg(rgb(0xfef2f2))
                        .text_color(rgb(0x991b1b))
                        .child(error),
                )
            })
            .into_any_element()
    }

    fn right_rail_group(
        &self,
        title: &'static str,
        group_id: &'static str,
        icons: &'static [&'static str],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .py_3()
            .px_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(title),
            )
            .child(
                div()
                    .w(px(72.0))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap_2()
                    .children(icons.iter().map(|icon| {
                        let id = SharedString::from(format!("{group_id}-{icon}"));
                        let tool = AnnotationTool::from_toolbar_id(id.as_ref());
                        let selected = tool == Some(self.annotation_adapter.tool());
                        let accessible_label = tool
                            .map(AnnotationTool::label)
                            .map(str::to_owned)
                            .unwrap_or_else(|| icon.replace('-', " "));
                        let mut button = Button::icon(
                            SharedString::from(format!("tool-{id}")),
                            SharedString::from(format!("rail/{icon}")),
                            accessible_label,
                        );
                        if let Some(tool) = tool {
                            button = button.tooltip(tool.tooltip_label());
                        }
                        button
                            .variant(ButtonVariant::Ghost)
                            .disabled(tool.is_none())
                            .toggled(selected)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(tool) = tool {
                                    this.select_tool(tool, cx);
                                }
                            }))
                    })),
            )
            .into_any_element()
    }

    fn rectangle_properties_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let document_id = self.document()?.id;
        let appearance = self
            .annotation_adapter
            .selected_rectangle_appearance(document_id)?;
        let current_width = appearance.stroke_width_pt();
        let ready_entity = cx.entity();
        Some(
            div()
                .w_full()
                .px_2()
                .py_2()
                .flex()
                .flex_col()
                .gap_2()
                .border_b_1()
                .border_color(rgb(BORDER))
                .child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(format!("Rectangle · {current_width} pt")),
                )
                .child(
                    div().flex().flex_col().gap_1().children(
                        [(1.5, "1.5"), (2.0, "2"), (4.0, "4")]
                            .into_iter()
                            .map(|(width, label)| {
                                let button = Button::new(
                                    SharedString::from(format!("rectangle-stroke-width-{label}")),
                                    SharedString::from(format!("{label} pt")),
                                )
                                .variant(ButtonVariant::Outline)
                                .size(ButtonSize::Small)
                                .toggled((current_width - width).abs() <= f64::EPSILON)
                                .disabled((current_width - width).abs() <= f64::EPSILON)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.apply_selected_rectangle_stroke_width(width, cx)
                                }));
                                div().relative().child(button).when(
                                    (width - 4.0).abs() <= f64::EPSILON,
                                    |element| {
                                        let ready_entity = ready_entity.clone();
                                        element.child(
                                            canvas(
                                                |_, _, _| (),
                                                move |bounds, _, window, cx| {
                                                    ready_entity.update(cx, |this, _| {
                                                    this.announce_multi_document_property_control(
                                                        bounds, window,
                                                    );
                                                });
                                                },
                                            )
                                            .absolute()
                                            .inset_0(),
                                        )
                                    },
                                )
                            }),
                    ),
                )
                .child(
                    Button::new("rectangle-snap-grid", "Snap · 18 pt grid")
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Small)
                        .toggled(self.rectangle_snap_enabled)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_rectangle_snap(cx))),
                )
                .into_any_element(),
        )
    }

    fn right_rail(&self, cx: &mut Context<Self>) -> AnyElement {
        let document_id = self.document().map(|document| document.id);
        let selected_annotation = document_id
            .is_some_and(|document_id| self.annotation_adapter.has_selection(document_id));
        let selected_locked = document_id
            .is_some_and(|document_id| self.annotation_adapter.selected_is_locked(document_id));
        let layout_editable = document_id.is_some_and(|document_id| {
            matches!(
                self.annotation_adapter.selected_kind(document_id),
                Some(AnnotationKind::TextBox | AnnotationKind::Image)
            )
        });
        let (undo_depth, redo_depth) = document_id
            .map(|document_id| self.annotation_adapter.history_depths(document_id))
            .unwrap_or_default();
        div()
            .w(px(RIGHT_RAIL_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .border_l_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .h(px(PRIMARY_BAND_HEIGHT))
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        Button::icon(
                            "delete-selected-annotation",
                            "trash-2",
                            "Delete selected annotation",
                        )
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::IconSmall)
                        .disabled(!selected_annotation)
                        .on_click(cx.listener(|this, _, _, cx| {
                            let Some(document_id) = this.document().map(|document| document.id)
                            else {
                                return;
                            };
                            match this.annotation_adapter.delete_selected(document_id) {
                                Ok(()) => {
                                    this.document_error = None;
                                    cx.notify();
                                }
                                Err(error) => this.report_annotation_error(error, cx),
                            }
                        })),
                    )
                    .child(
                        Button::icon(
                            "lock-selected-annotation",
                            if selected_locked { "lock-open" } else { "lock" },
                            if selected_locked {
                                "Unlock selected annotation"
                            } else {
                                "Lock selected annotation"
                            },
                        )
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::IconSmall)
                        .disabled(!selected_annotation)
                        .toggled(selected_locked)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_annotation_lock(cx))),
                    ),
            )
            .when_some(self.rectangle_properties_panel(cx), |rail, panel| {
                rail.child(panel)
            })
            .child(
                div()
                    .w_full()
                    .py_2()
                    .flex()
                    .justify_center()
                    .gap_1()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        Button::icon("undo-annotation", "undo-2", "Undo annotation change")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::IconSmall)
                            .disabled(undo_depth == 0)
                            .on_click(cx.listener(|this, _, _, cx| this.undo_annotation(cx))),
                    )
                    .child(
                        Button::icon("redo-annotation", "redo-2", "Redo annotation change")
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::IconSmall)
                            .disabled(redo_depth == 0)
                            .on_click(cx.listener(|this, _, _, cx| this.redo_annotation(cx))),
                    )
                    .child(
                        Button::icon(
                            "resize-comparison-annotation",
                            "maximize-2",
                            "Apply exact comparison layout",
                        )
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::IconSmall)
                        .disabled(!layout_editable)
                        .on_click(cx.listener(|this, _, _, cx| this.apply_comparison_layout(cx))),
                    ),
            )
            .child(self.right_rail_group("General", "general", &["mouse-pointer-2", "hand"], cx))
            .child(self.right_rail_group(
                "Markup",
                "markup",
                &[
                    "type",
                    "arrow-right",
                    "highlighter",
                    "cloud",
                    "message-square",
                    "shield-x",
                    "pen-line",
                    "image",
                    "scan-search",
                ],
                cx,
            ))
            .child(self.right_rail_group(
                "Draw",
                "draw",
                &[
                    "square",
                    "circle",
                    "minus",
                    "waypoints",
                    "pen-line",
                    "spline",
                    "pentagon",
                    "ruler",
                ],
                cx,
            ))
            .child(self.right_rail_group(
                "Measure",
                "measure",
                &["ruler", "ruler-dimension-line", "route", "chart-area"],
                cx,
            ))
            .child(
                div()
                    .mt_auto()
                    .p_2()
                    .text_xs()
                    .text_color(rgb(MUTED))
                    .child("Highlight preview uses source alpha; Multiply parity is unverified."),
            )
            .into_any_element()
    }

    fn shell_preview(&self, fill_window: bool, cx: &mut Context<Self>) -> AnyElement {
        let shell = div()
            .id("butter-paper-shell")
            .track_focus(&self.annotation_focus)
            .on_key_down(cx.listener(Self::native_v5_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.observe_native_viewer_shell_input(window, cx)
                }),
            )
            .on_action(cx.listener(Self::open_pdf_action))
            .on_action(cx.listener(Self::next_page_action))
            .on_action(cx.listener(Self::previous_page_action))
            .on_action(cx.listener(Self::zoom_in_action))
            .on_action(cx.listener(Self::zoom_out_action))
            .on_action(cx.listener(Self::zoom_reset_action))
            .on_action(cx.listener(Self::fit_width_action))
            .on_action(cx.listener(Self::fit_page_action))
            .rounded(px(BASE_RADIUS))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.window_title_bar())
            .child(Self::app_menu_bar())
            .child(self.document_tab_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .child(self.left_rail(cx))
                    .when(self.sidebar_visible, |element| {
                        element.child(self.thumbnail_sidebar(cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(self.viewer_toolbar(cx))
                            .child(self.document_viewport(cx)),
                    )
                    .child(self.right_rail(cx)),
            );

        if fill_window {
            shell.size_full().rounded(px(0.0)).into_any_element()
        } else {
            shell.h(px(360.0)).into_any_element()
        }
    }
}

fn utf16_to_utf8_offset(content: &str, offset: usize) -> usize {
    let mut utf8 = 0;
    let mut utf16 = 0;
    for character in content.chars() {
        if utf16 >= offset {
            break;
        }
        utf16 += character.len_utf16();
        utf8 += character.len_utf8();
    }
    utf8
}

fn utf8_to_utf16_offset(content: &str, offset: usize) -> usize {
    content[..offset.min(content.len())]
        .chars()
        .map(char::len_utf16)
        .sum()
}

impl Focusable for Gallery {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.annotation_focus.clone()
    }
}

impl EntityInputHandler for Gallery {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let document_id = self.document()?.id;
        let content = self
            .annotation_adapter
            .selected_text(document_id)?
            .to_string();
        let start = utf16_to_utf8_offset(&content, range.start);
        let end = utf16_to_utf8_offset(&content, range.end);
        adjusted_range
            .replace(utf8_to_utf16_offset(&content, start)..utf8_to_utf16_offset(&content, end));
        Some(content[start..end].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let document_id = self.document()?.id;
        let content = self.annotation_adapter.selected_text(document_id)?;
        if self
            .native_editor_text_input
            .as_ref()
            .is_some_and(|probe| probe.submitted_text.is_empty() && content == probe.content_before)
        {
            return Some(UTF16Selection {
                range: 0..utf8_to_utf16_offset(content, content.len()),
                reversed: false,
            });
        }
        let cursor = utf8_to_utf16_offset(content, self.annotation_text_cursor.min(content.len()));
        Some(UTF16Selection {
            range: cursor..cursor,
            reversed: false,
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            return;
        };
        let native_text_phase = self.perf_scenario.as_ref().is_some_and(|scenario| {
            matches!(
                scenario.comparison_phase,
                ComparisonPhase::NativeEditorInput {
                    stage: NativeEditorStage::Text,
                    ..
                }
            )
        });
        if native_text_phase {
            let Some(mut probe) = self.native_editor_text_input.take() else {
                self.fail_comparison_scenario(
                    "native Text key input arrived before the placed text state was captured",
                    cx,
                );
                return;
            };
            let Some(content) = self
                .annotation_adapter
                .selected_text(document_id)
                .map(str::to_owned)
            else {
                self.fail_comparison_scenario(
                    "native Text key input arrived without a selected text annotation",
                    cx,
                );
                return;
            };
            let range = if probe.submitted_text.is_empty() {
                0..content.len()
            } else {
                range
                    .map(|range| {
                        utf16_to_utf8_offset(&content, range.start)
                            ..utf16_to_utf8_offset(&content, range.end)
                    })
                    .unwrap_or_else(|| {
                        let cursor = self.annotation_text_cursor.min(content.len());
                        cursor..cursor
                    })
            };
            let mut replacement = content;
            replacement.replace_range(range.clone(), new_text);
            probe.submitted_text.push_str(new_text);
            if !FROZEN_TEXT_CREATE.starts_with(&probe.submitted_text)
                || !FROZEN_TEXT_CREATE.starts_with(&replacement)
            {
                self.fail_comparison_scenario(
                    "native Text key payload or resulting document content differs from the frozen command",
                    cx,
                );
                return;
            }
            self.annotation_text_cursor = range.start + new_text.len();
            if let Err(error) = self
                .annotation_adapter
                .replace_selected_text_in_create_transaction(document_id, replacement)
            {
                self.fail_comparison_scenario(error.to_string(), cx);
                return;
            }
            let content_after = self
                .annotation_adapter
                .selected_text(document_id)
                .unwrap_or_default()
                .to_owned();
            let history_after = self.annotation_adapter.history_depths(document_id).0;
            let scene_revision_after = self
                .annotation_adapter
                .document_scene(document_id, 0)
                .revision;
            let stage_history_before =
                self.perf_scenario
                    .as_ref()
                    .and_then(|scenario| match scenario.comparison_phase {
                        ComparisonPhase::NativeEditorInput {
                            stage: NativeEditorStage::Text,
                            history_before,
                            ..
                        } => Some(history_before),
                        _ => None,
                    });
            if content_after == FROZEN_TEXT_CREATE && probe.submitted_text == FROZEN_TEXT_CREATE {
                let content_changed_by_native_key_events = content_after != probe.content_before
                    && scene_revision_after > probe.scene_revision_before;
                let gesture_committed_once = stage_history_before
                    .is_some_and(|history_before| history_after == history_before + 1);
                perf::emit(
                    "native-text-entry-observed",
                    perf::fields([
                        ("command_id", json!("text:create")),
                        ("input_api", json!("XTEST-key-events")),
                        ("utf8_bytes", json!(probe.submitted_text.len())),
                        ("final_text_matches_document", json!(true)),
                        ("document_content_prepopulated", json!(false)),
                        (
                            "content_changed_by_native_key_events",
                            json!(content_changed_by_native_key_events),
                        ),
                        ("history_before_typing", json!(probe.history_before)),
                        ("history_after_typing", json!(history_after)),
                        (
                            "scene_revision_before_typing",
                            json!(probe.scene_revision_before),
                        ),
                        ("scene_revision_after_typing", json!(scene_revision_after)),
                        ("gesture_committed_once", json!(gesture_committed_once)),
                        ("decision_timing_eligible", json!(false)),
                    ]),
                );
                if !content_changed_by_native_key_events || !gesture_committed_once {
                    perf::emit(
                        "comparison-native-input-evidence",
                        perf::fields([
                            ("command_id", json!("text:create")),
                            ("input_api", json!("XTEST-key-events")),
                            ("native_text_entry_observed", json!(true)),
                            ("document_content_prepopulated", json!(false)),
                            (
                                "content_changed_by_native_key_events",
                                json!(content_changed_by_native_key_events),
                            ),
                            ("gesture_committed_once", json!(gesture_committed_once)),
                            ("history_before_typing", json!(probe.history_before)),
                            ("history_after_typing", json!(history_after)),
                            (
                                "scene_revision_before_typing",
                                json!(probe.scene_revision_before),
                            ),
                            ("scene_revision_after_typing", json!(scene_revision_after)),
                            ("gpu_present_observed", json!(false)),
                            ("gpu_upload_bytes", Value::Null),
                            ("decision_timing_eligible", json!(false)),
                        ]),
                    );
                    perf::emit(
                        "editor-create-blocked",
                        perf::fields([
                            ("decision_timing_eligible", json!(false)),
                            (
                                "missing_milestones",
                                json!(["text:create:gesture-committed-once"]),
                            ),
                        ]),
                    );
                    self.fail_comparison_scenario(
                        format!(
                            "native Text key events changed the document to the exact frozen text, but the command used {} history entries; placement and typing history coalescing is not implemented",
                            history_after.saturating_sub(stage_history_before.unwrap_or_default())
                        ),
                        cx,
                    );
                    return;
                }
                self.finish_native_editor_input(true, cx);
            } else {
                self.native_editor_text_input = Some(probe);
                self.document_error = None;
                cx.notify();
            }
            return;
        }
        let Some(content) = self
            .annotation_adapter
            .selected_text(document_id)
            .map(str::to_string)
        else {
            return;
        };
        let range = range
            .map(|range| {
                utf16_to_utf8_offset(&content, range.start)
                    ..utf16_to_utf8_offset(&content, range.end)
            })
            .unwrap_or_else(|| {
                let cursor = self.annotation_text_cursor.min(content.len());
                cursor..cursor
            });
        let mut replacement = content;
        replacement.replace_range(range.clone(), new_text);
        self.annotation_text_cursor = range.start + new_text.len();
        match self
            .annotation_adapter
            .replace_selected_text(document_id, replacement)
        {
            Ok(()) => {
                self.document_error = None;
                cx.notify();
            }
            Err(error) => self.report_annotation_error(error, cx),
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_range(range, new_text, window, cx);
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.perf_window_handle
            .get_or_insert(window.window_handle());
        if let Some(scenario) = self.perf_scenario.as_mut()
            && !scenario.render_enter_emitted
        {
            scenario.render_enter_emitted = true;
            perf::emit("first-render-enter", Default::default());
        }
        self.display_scale_factor = window.scale_factor();
        let viewport = window.viewport_size();
        self.last_window_logical_size =
            Some([f32::from(viewport.width), f32::from(viewport.height)]);
        self.refresh_tile_plan(window, cx);
        self.queue_dynamic_fidelity_state(window);
        self.export_dynamic_fidelity_backing_rasters();
        self.schedule_perf_frame(window, cx);
        if self.capture_shell {
            return div()
                .size_full()
                .font_family("Geist")
                .bg(rgb(BG))
                .text_color(rgb(TEXT))
                .text_size(px(BODY_FONT_SIZE))
                .child(self.shell_preview(true, cx))
                .into_any_element();
        }

        let active_tab = self.active_tab;
        div()
            .size_full()
            .font_family("Geist")
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(BODY_FONT_SIZE))
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .child(div().text_2xl().font_weight(gpui::FontWeight::BOLD).child("Butter Paper GPUI gallery"))
                            .child(div().mt_1().text_color(rgb(MUTED)).child("Pinned current Zed GPUI · Butter Paper-owned components with Nova defaults")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children(["Primitives", "Compound", "Shell"].into_iter().enumerate().map(|(index, label)| {
                        Button::new(format!("tab-{index}"), label)
                            .variant(if active_tab == index {
                                ButtonVariant::Secondary
                            } else {
                                ButtonVariant::Ghost
                            })
                            .toggled(active_tab == index)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_tab = index;
                                cx.notify();
                            }))
                    })),
            )
            .child(
                if active_tab == 2 {
                    self.shell_preview(false, cx)
                } else {
                    div()
                        .flex()
                        .gap_4()
                        .child(self.comparison_card("GPUI foundation", false))
                        .child(self.comparison_card("Butter Paper wrapper", true))
                        .into_any_element()
                },
            )
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_xs()
                    .child("Direct GPUI: div/flex/grid, focus, actions, native app menus, anchored/deferred layers, input plumbing, uniform lists, images/surfaces and canvas. Thin Butter Paper wrappers: ordinary controls and Nova styling. Domain UI: document viewport and annotation behavior."),
            )
            .into_any_element()
    }
}

fn main() {
    perf::init();
    perf::emit("process-main-enter", Default::default());
    let perf_scenario_kind = PerfScenarioKind::from_environment();
    let initial_pdfs = initial_pdf_paths(std::env::args_os().skip(1));
    gpui_platform::application()
        .with_assets(Assets { base: asset_base() })
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| {
            perf::emit("application-run-enter", Default::default());
            load_fonts(cx).expect("Geist should load from the bundled assets");
            perf::emit("fonts-loaded", Default::default());
            cx.bind_keys([
                KeyBinding::new("cmd-o", OpenPdf, None),
                KeyBinding::new("ctrl-o", OpenPdf, None),
                KeyBinding::new("pagedown", NextPage, None),
                KeyBinding::new("pageup", PreviousPage, None),
                KeyBinding::new("cmd-=", ZoomIn, None),
                KeyBinding::new("cmd--", ZoomOut, None),
                KeyBinding::new("cmd-0", ZoomReset, None),
                KeyBinding::new("ctrl-=", ZoomIn, None),
                KeyBinding::new("ctrl--", ZoomOut, None),
                KeyBinding::new("ctrl-0", ZoomReset, None),
            ]);
            cx.set_menus(vec![
                Menu {
                    name: "File".into(),
                    items: vec![MenuItem::action("Open…", OpenPdf)],
                    disabled: false,
                },
                Menu {
                    name: "View".into(),
                    items: vec![
                        MenuItem::action("Zoom In", ZoomIn),
                        MenuItem::action("Zoom Out", ZoomOut),
                        MenuItem::action("Actual Size", ZoomReset),
                        MenuItem::separator(),
                        MenuItem::action("Fit Width", FitWidth),
                        MenuItem::action("Fit Page", FitPage),
                    ],
                    disabled: false,
                },
            ]);
            let capture_shell =
                std::env::var_os("BP_GPUI_CAPTURE_SHELL").is_some() || perf_scenario_kind.is_some();
            let window_size = if capture_shell {
                // The Electron evidence was captured at 1200×800 logical pixels
                // and exported at 1152×768. Use the same 0.96 export scale.
                size(px(1200.0), px(800.0))
            } else {
                size(px(1120.0), px(760.0))
            };
            let bounds = Bounds::centered(None, window_size, cx);
            perf::emit("open-window-requested", Default::default());
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(
                            if perf_scenario_kind.is_some() {
                                "Butter Paper GPUI comparison"
                            } else {
                                "Butter Paper GPUI gallery"
                            }
                            .into(),
                        ),
                        appears_transparent: capture_shell,
                        traffic_light_position: capture_shell.then(|| point(px(10.0), px(16.0))),
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    perf::emit("window-created", Default::default());
                    let initial_pdfs = initial_pdfs.clone();
                    let gallery = cx.new(move |cx| {
                        perf::emit("gallery-entity-creation-enter", Default::default());
                        let zoom_percent = std::env::var("BP_GPUI_ZOOM")
                            .ok()
                            .and_then(|value| value.parse::<f32>().ok())
                            .unwrap_or(if perf_scenario_kind.is_some() {
                                100.0
                            } else {
                                194.0
                            });
                        let zoom_percent = clamp_zoom_percent(zoom_percent);
                        let initial_page = std::env::var("BP_GPUI_INITIAL_PAGE")
                            .ok()
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(1);
                        const CHECKER_SHA256: &str =
                            "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda";
                        let (checker_asset, annotation_image) =
                            if let Some(path) = std::env::var_os("BP_GPUI_IMAGE_ASSET_PATH") {
                                let ingested = ingest_image_asset_from_path(&path).expect(
                                    "the explicit annotation image must be a bounded PNG or JPEG",
                                );
                                assert_eq!(
                                    ingested.encoded_sha256(),
                                    CHECKER_SHA256,
                                    "the comparison image must match the frozen checker bytes"
                                );
                                perf::emit(
                                    "annotation-image-asset-ingested",
                                    perf::fields([
                                        ("asset_id", json!("bp-image-checker-v1")),
                                        ("encoded_sha256", json!(ingested.encoded_sha256())),
                                        ("encoded_bytes", json!(ingested.encoded_bytes())),
                                        ("source", json!("bounded-png-jpeg-file-ingestion")),
                                    ]),
                                );
                                ingested.into_parts()
                            } else {
                                assert!(
                                    perf_scenario_kind.is_none(),
                                    "performance scenarios require BP_GPUI_IMAGE_ASSET_PATH"
                                );
                                comparison_checker_asset()
                            };
                        let mut annotation_adapter = AnnotationAdapter::default();
                        annotation_adapter.set_image_asset(checker_asset.clone());
                        let mut gallery = Gallery {
                            active_tab: if capture_shell { 2 } else { 0 },
                            capture_shell,
                            sidebar_visible: initial_sidebar_visible(perf_scenario_kind),
                            annotation_adapter,
                            annotation_focus: cx.focus_handle().tab_stop(true),
                            annotation_pointer_button: AnnotationPointerButtonState::default(),
                            annotation_text_cursor: 0,
                            native_editor_text_input: None,
                            annotation_image_asset: checker_asset,
                            annotation_image,
                            rectangle_snap_enabled: false,
                            open_popup: None,
                            popup_selection: 0,
                            selected_template: TemplateChoice::LetterPortrait,
                            continuous_wheel_mode: ScrollWheelMode::Scroll,
                            single_page_wheel_mode: ScrollWheelMode::Zoom,
                            single_page_wheel_delta: 0.0,
                            documents: Vec::new(),
                            active_document: None,
                            document_fixture_ids: HashMap::new(),
                            document_error: None,
                            document_scroll: ScrollHandle::new(),
                            continuous_scroll: ScrollHandle::new(),
                            native_scroll_pending_offset_y: None,
                            native_scroll_flush_scheduled: false,
                            thumbnail_scroll: UniformListScrollHandle::new(),
                            zoom_percent,
                            display_scale_factor: 1.0,
                            zoom_preset: ZoomPreset::Manual,
                            scroll_mode: initial_scroll_mode(perf_scenario_kind),
                            next_document_id: 1,
                            next_request_generation: 1,
                            latest_open_request: None,
                            pending_open_requests: HashSet::new(),
                            pending_open_fixture_ids: HashMap::new(),
                            pending_viewport_requests: HashMap::new(),
                            active_viewport_jobs: HashSet::new(),
                            pending_thumbnail_requests: HashMap::new(),
                            thumbnail_queue: VecDeque::new(),
                            active_thumbnail_jobs: 0,
                            thumbnail_failures: HashMap::new(),
                            pending_page_surface_requests: HashMap::new(),
                            page_surface_queue: VecDeque::new(),
                            active_page_surface_jobs: 0,
                            page_surface_failures: HashMap::new(),
                            render_planner: RenderPlanner::default(),
                            tile_plan_key: None,
                            visible_tile_requests: Vec::new(),
                            pending_tile_requests: HashSet::new(),
                            tile_queue: VecDeque::new(),
                            active_tile_requests: HashSet::new(),
                            active_tile_jobs: 0,
                            tile_cache: TileCache::new(CachePolicy::default()),
                            continuous_page_layouts: Vec::new(),
                            continuous_visible_pages: Vec::new(),
                            continuous_total_height: 0.0,
                            continuous_plan_generation: 0,
                            dynamic_fidelity_state_sequence: 0,
                            dynamic_fidelity_paint_capture_sequence: 0,
                            dynamic_fidelity_pending_state: None,
                            dynamic_fidelity_ready_state: None,
                            dynamic_fidelity_painted_pages: Arc::new(Mutex::new(HashMap::new())),
                            dynamic_fidelity_exported_pages: HashSet::new(),
                            initial_page,
                            diagnostics: std::env::var_os("BP_GPUI_DIAGNOSTICS").is_some(),
                            perf_scenario: perf_scenario_kind.map(PerfScenario::new),
                            perf_initial_pdfs: Vec::new(),
                            perf_reopen_path: None,
                            editor_presentation_pending: None,
                            editor_dense_presentation_pending: false,
                            editor_overlay_document_id: None,
                            perf_window_handle: None,
                            last_window_logical_size: None,
                            annotation_overlay_paint: Arc::new(Mutex::new(None)),
                            annotation_image_atlas_paint: Arc::new(Mutex::new(None)),
                            tile_atlas_uploads: Arc::new(Mutex::new(HashMap::new())),
                        };
                        if perf_scenario_kind.is_some() {
                            if matches!(
                                perf_scenario_kind,
                                Some(
                                    PerfScenarioKind::CloseReopen
                                        | PerfScenarioKind::PersistenceWorkload
                                )
                            ) {
                                gallery.perf_reopen_path = initial_pdfs.first().cloned();
                            }
                            gallery.perf_initial_pdfs = initial_pdfs;
                        } else {
                            for path in initial_pdfs {
                                gallery.open_path(path, cx);
                            }
                        }
                        perf::emit("gallery-entity-created", Default::default());
                        gallery
                    });
                    gallery.update(cx, |this, cx| {
                        this.annotation_focus.focus(window, cx);
                    });
                    gallery
                },
            )
            .unwrap();
            cx.activate(true);
        });
}

#[cfg(test)]
mod shell_tests {
    use super::*;

    #[test]
    fn rectangle_shortcut_is_plain_and_suppressed_during_text_entry() {
        let plain = KeyDownEvent {
            keystroke: gpui::Keystroke::parse("r").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        let modified = KeyDownEvent {
            keystroke: gpui::Keystroke::parse("ctrl-r").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };
        let held = KeyDownEvent {
            keystroke: gpui::Keystroke::parse("r").unwrap(),
            is_held: true,
            prefer_character_input: false,
        };
        let escape = KeyDownEvent {
            keystroke: gpui::Keystroke::parse("escape").unwrap(),
            is_held: false,
            prefer_character_input: false,
        };

        assert_eq!(
            annotation_tool_shortcut(&plain, false),
            Some(AnnotationTool::Rectangle)
        );
        assert_eq!(annotation_tool_shortcut(&plain, true), None);
        assert_eq!(annotation_tool_shortcut(&modified, false), None);
        assert_eq!(annotation_tool_shortcut(&held, false), None);
        assert_eq!(
            annotation_tool_shortcut(&escape, false),
            Some(AnnotationTool::Select)
        );
        assert_eq!(annotation_tool_shortcut(&escape, true), None);
    }

    #[test]
    fn clicking_the_current_page_surface_does_not_restart_its_viewport() {
        assert!(!should_activate_page_from_surface(1, 1));
        assert!(should_activate_page_from_surface(1, 2));
    }

    #[test]
    fn native_pointer_button_transitions_are_delivered_once() {
        let mut state = AnnotationPointerButtonState::default();
        assert!(state.begin_press());
        assert!(!state.begin_press());
        assert!(state.end_press());
        assert!(!state.end_press());
        assert!(state.begin_press());
        state.cancel();
        assert!(!state.end_press());
    }

    #[test]
    fn dynamic_fidelity_state_uses_actual_painted_page_geometry() {
        let mut state = perf::fields([
            ("paint_capture_sequence", json!(7)),
            ("render_generation", json!(11)),
            (
                "visible_pages",
                json!([{
                    "page_number": 15,
                    "page_size_points": { "width": 612.0, "height": 792.0 },
                    "current_raster_ready_area_fraction": 1.0,
                    "page_bounds_window_logical": { "x": 0, "y": 0, "width": 1, "height": 1 },
                }]),
            ),
        ]);
        let observations = HashMap::from([(
            (7, 15),
            DynamicFidelityPaintObservation {
                capture_sequence: 7,
                page_number: 15,
                outer_bounds_window_logical: ViewerRect::new(210.0, 92.0, 612.0, 792.0),
                page_width_points: 612.0,
                page_height_points: 792.0,
                render_generation: 11,
                current_raster_ready: true,
            },
        )]);

        assert_eq!(
            Gallery::attach_dynamic_fidelity_paint_evidence(&mut state, &observations),
            Some(7)
        );
        let page = &state["visible_pages"][0];
        assert_eq!(
            page["painted_outer_page_bounds_window_logical"],
            json!({ "x": 210.0, "y": 92.0, "width": 612.0, "height": 792.0 })
        );
        assert_eq!(page["pixels_per_point"], json!({ "x": 1.0, "y": 1.0 }));
        assert_eq!(page["painted_render_generation"], json!(11));
        assert_eq!(state["painted_generation_current"], json!(true));
        assert_eq!(state["platform_draw_submitted"], json!(true));
    }

    #[test]
    fn dynamic_fidelity_state_rejects_stale_or_unready_paint_evidence() {
        let state = || {
            perf::fields([
                ("paint_capture_sequence", json!(7)),
                ("render_generation", json!(11)),
                (
                    "visible_pages",
                    json!([{
                        "page_number": 15,
                        "page_size_points": { "width": 612.0, "height": 792.0 },
                        "current_raster_ready_area_fraction": 1.0,
                    }]),
                ),
            ])
        };
        let observation = |render_generation, current_raster_ready| {
            HashMap::from([(
                (7, 15),
                DynamicFidelityPaintObservation {
                    capture_sequence: 7,
                    page_number: 15,
                    outer_bounds_window_logical: ViewerRect::new(210.0, 92.0, 612.0, 792.0),
                    page_width_points: 612.0,
                    page_height_points: 792.0,
                    render_generation,
                    current_raster_ready,
                },
            )])
        };

        assert_eq!(
            Gallery::attach_dynamic_fidelity_paint_evidence(&mut state(), &observation(10, true)),
            None
        );
        assert_eq!(
            Gallery::attach_dynamic_fidelity_paint_evidence(&mut state(), &observation(11, false)),
            None
        );
    }

    #[test]
    fn editor_evidence_starts_with_a_bounded_single_page_viewport() {
        for scenario in [
            PerfScenarioKind::EditorCreate,
            PerfScenarioKind::EditorWorkload,
            PerfScenarioKind::PersistenceWorkload,
        ] {
            assert_eq!(initial_scroll_mode(Some(scenario)), ScrollMode::SinglePage);
            assert!(!initial_sidebar_visible(Some(scenario)));
        }
        assert_eq!(initial_scroll_mode(None), ScrollMode::Continuous);
        assert!(initial_sidebar_visible(None));
    }

    #[test]
    fn native_open_benchmark_starts_without_a_document_sidebar() {
        assert!(!initial_sidebar_visible(Some(PerfScenarioKind::OpenPdf)));
        assert!(sidebar_visible_after_document_open(
            false,
            Some(PerfScenarioKind::OpenPdf)
        ));
        assert!(!sidebar_visible_after_document_open(
            false,
            Some(PerfScenarioKind::EditorCreate)
        ));
        assert!(initial_sidebar_visible(None));
    }

    #[test]
    fn comparison_view_state_maps_live_shell_state_to_the_frozen_contract() {
        let observation = ComparisonViewStateObservation::from_live_state(
            PerfScenarioKind::Zoom.as_str(),
            "measurement-end",
            1_200.0,
            800.0,
            1.0,
            ScrollMode::SinglePage,
            ZoomPreset::FitWidth,
            111.25,
            true,
            Some("bp-engineering-sheet-v1".into()),
            Some(0),
            1,
        );

        assert_eq!(observation.component, "zoom");
        assert_eq!(observation.layout_mode, "single-page");
        assert_eq!(observation.zoom_mode, "fit-width");
        assert_eq!(observation.left_sidebar_width_logical, SIDEBAR_WIDTH);
        assert!(!observation.right_sidebar_visible);
        assert_eq!(observation.right_sidebar_width_logical, 0.0);
        assert_eq!(observation.window_bounds_window_logical.x, 0.0);
        assert_eq!(observation.window_bounds_window_logical.y, 0.0);
        assert_eq!(observation.window_bounds_window_logical.width, 1_200.0);
        assert_eq!(observation.window_bounds_window_logical.height, 800.0);
        assert_eq!(
            observation.viewport_bounds_window_logical.x,
            RAIL_WIDTH + SIDEBAR_WIDTH
        );
        assert_eq!(
            observation.viewport_bounds_window_logical.y,
            WINDOW_TITLE_BAR_HEIGHT
                + MENU_BAR_HEIGHT
                + DOCUMENT_TAB_BAR_HEIGHT
                + PRIMARY_BAND_HEIGHT
                + VIEWPORT_TOP_BORDER_WIDTH
        );
        assert_eq!(
            observation.viewport_bounds_window_logical.width,
            1_200.0 - RAIL_WIDTH - SIDEBAR_WIDTH - RIGHT_RAIL_WIDTH
        );
        assert_eq!(observation.viewport_bounds_window_logical.height, 639.0);

        let fields = observation.into_fields();
        assert_eq!(fields.get("live"), Some(&json!(true)));
        assert_eq!(
            fields.get("observation_source"),
            Some(&json!("live-application-render-state"))
        );
        assert_eq!(
            fields.get("active_fixture_id"),
            Some(&json!("bp-engineering-sheet-v1"))
        );
        assert_eq!(fields.get("active_document_index"), Some(&json!(0)));
        assert_eq!(fields.get("open_document_count"), Some(&json!(1)));
    }

    #[test]
    fn comparison_view_state_reports_empty_documents_and_hidden_sidebar_exactly() {
        let observation = ComparisonViewStateObservation::from_live_state(
            PerfScenarioKind::OpenPdf.as_str(),
            "measurement-start",
            1_200.0,
            800.0,
            1.0,
            ScrollMode::Continuous,
            ZoomPreset::Manual,
            100.0,
            false,
            None,
            None,
            0,
        );

        assert_eq!(observation.layout_mode, "continuous");
        assert_eq!(observation.zoom_mode, "manual");
        assert!(!observation.left_sidebar_visible);
        assert_eq!(observation.left_sidebar_width_logical, 0.0);
        assert_eq!(observation.viewport_bounds_window_logical.x, RAIL_WIDTH);
        assert_eq!(
            observation.viewport_bounds_window_logical.width,
            1_200.0 - RAIL_WIDTH - RIGHT_RAIL_WIDTH
        );
        assert_eq!(observation.viewport_bounds_window_logical.y, 161.0);
        assert_eq!(observation.viewport_bounds_window_logical.height, 639.0);
        let fields = observation.into_fields();
        assert_eq!(fields.get("active_fixture_id"), Some(&Value::Null));
        assert_eq!(fields.get("active_document_index"), Some(&Value::Null));
        assert_eq!(fields.get("open_document_count"), Some(&json!(0)));
    }

    #[test]
    fn open_pdf_live_layout_uses_fit_width_and_reserves_the_continuous_scrollbar() {
        let (zoom_percent, zoom_preset) =
            open_pdf_default_zoom(Some(PerfScenarioKind::OpenPdf), 1_200.0, 612.0)
                .expect("open-pdf adopts the maintained fit-width default");
        assert_eq!(zoom_percent, 112.0);
        assert_eq!(zoom_preset, ZoomPreset::FitWidth);

        let observation = ComparisonViewStateObservation::from_live_state(
            PerfScenarioKind::OpenPdf.as_str(),
            "measurement-end",
            1_200.0,
            800.0,
            1.0,
            ScrollMode::Continuous,
            zoom_preset,
            zoom_percent,
            true,
            Some("bp-single-page-v1".into()),
            Some(0),
            1,
        );
        assert_eq!(observation.viewport_bounds_window_logical.x, 348.0);
        assert_eq!(observation.viewport_bounds_window_logical.y, 161.0);
        assert_eq!(observation.viewport_bounds_window_logical.width, 750.0);
        assert_eq!(observation.viewport_bounds_window_logical.height, 639.0);
        assert_eq!(observation.zoom_mode, "fit-width");
        assert_eq!(observation.zoom_percent, 112.0);
    }

    #[test]
    fn continuous_scrollbar_thumb_tracks_the_live_scroll_range() {
        let (top, height) = continuous_scrollbar_thumb(639.0, 1_278.0, 0.0);
        assert_eq!(top, 0.0);
        assert_eq!(height, 319.5);

        let (top, height) = continuous_scrollbar_thumb(639.0, 1_278.0, -639.0);
        assert_eq!(top, 319.5);
        assert_eq!(height, 319.5);
    }

    #[test]
    fn comparison_view_state_eligibility_excludes_only_non_timed_components() {
        assert!(!PerfScenarioKind::EmptyShell.benefit_metrics_eligible());
        assert!(!PerfScenarioKind::NativePropertyEditUndo.benefit_metrics_eligible());
        for scenario in [
            PerfScenarioKind::OpenPdf,
            PerfScenarioKind::ViewerLayout,
            PerfScenarioKind::EditorCreate,
            PerfScenarioKind::PersistenceWorkload,
            PerfScenarioKind::MultiDocumentSession,
            PerfScenarioKind::NativeSnapTransform,
            PerfScenarioKind::DynamicFidelity,
        ] {
            assert!(scenario.benefit_metrics_eligible(), "{scenario:?}");
        }
    }

    #[test]
    fn pdf_helvetica_uses_the_bundled_presentation_font_without_changing_the_model() {
        assert_eq!(presentation_font_family("Helvetica"), "Geist");
        assert_eq!(
            presentation_font_family("Times New Roman"),
            "Times New Roman"
        );
    }

    #[test]
    fn annotation_adapter_maps_window_pixels_to_pdf_bottom_left_points() {
        let transform = Gallery::annotation_transform(1_188.0, 150.0);
        let bounds = Bounds {
            origin: point(px(100.0), px(50.0)),
            size: size(px(918.0), px(1_188.0)),
        };

        assert_eq!(
            Gallery::annotation_point(bounds, point(px(208.0), px(266.0)), transform).unwrap(),
            PdfPoint::new(72.0, 648.0).unwrap()
        );
    }

    #[test]
    fn annotation_adapter_canonicalizes_only_subpixel_integer_noise() {
        assert_eq!(Gallery::canonicalize_pdf_coordinate(90.006), 90.0);
        assert_eq!(Gallery::canonicalize_pdf_coordinate(509.996), 510.0);
        assert_eq!(Gallery::canonicalize_pdf_coordinate(90.125), 90.125);
    }

    #[test]
    fn tiled_rendering_starts_at_the_full_page_raster_cap() {
        assert!(!uses_tiled_rendering(TILED_RENDER_THRESHOLD_PX - 1));
        assert!(uses_tiled_rendering(TILED_RENDER_THRESHOLD_PX));
    }

    #[test]
    fn an_active_current_tile_request_is_not_started_twice() {
        let pages = [butter_paper_gpui_gallery::viewer::PageGeometry::new(
            1, 1_584.0, 1_224.0,
        )];
        let mut planner = RenderPlanner::default();
        let plan = planner.plan(RenderInput {
            source: butter_paper_gpui_gallery::viewer::RenderSource {
                document_id: 1,
                revision: 1,
            },
            pages: &pages,
            zoom_percent: 1_600.0,
            device_scale: 1.0,
            page_gap: PAGE_LAYOUT_GAP,
            viewport: ViewportGeometry {
                width: 800.0,
                height: 600.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
                visible_rect: ViewerRect::new(0.0, 0.0, 800.0, 600.0),
            },
        });
        let request = plan.tiles[0];
        let pending = HashSet::from([request]);
        let mut active = HashSet::new();

        assert!(tile_job_can_start(request, &pending, &active, &planner));
        active.insert(request);
        assert!(!tile_job_can_start(request, &pending, &active, &planner));
    }

    #[test]
    fn native_editor_stages_keep_the_frozen_annotation_ids() {
        assert_eq!(
            Gallery::native_editor_annotation_id(NativeEditorStage::Text),
            Some(TEXT_CREATE_ID)
        );
        assert_eq!(
            Gallery::native_editor_annotation_id(NativeEditorStage::Length),
            Some(LENGTH_CREATE_ID)
        );
        assert_eq!(
            Gallery::native_editor_annotation_id(NativeEditorStage::Image),
            Some(IMAGE_CREATE_ID)
        );
        assert_eq!(
            Gallery::native_editor_annotation_id(NativeEditorStage::Scale),
            None
        );
    }

    #[test]
    fn native_page_gestures_requeue_the_frozen_annotation_id_at_pointer_down() {
        let mut annotation = PerfScenario::new(PerfScenarioKind::AnnotationCreate);
        annotation.comparison_phase = ComparisonPhase::NativeAnnotationInput {
            stage: NativeAnnotationStage::Rectangle,
            coordinate_samples: 0,
            history_before: 0,
        };
        assert_eq!(
            native_pending_annotation_id(&annotation).as_deref(),
            Some("comparison:rectangle:sparse:1")
        );

        let mut transform = PerfScenario::new(PerfScenarioKind::AnnotationTransform);
        transform.comparison_phase = ComparisonPhase::NativeTransformInput {
            stage: NativeTransformStage::PrerequisiteCreate,
            coordinate_samples: 0,
            history_before: 0,
            progress: NativeTransformProgress::default(),
            pixels_per_point: 1.0,
        };
        assert_eq!(
            native_pending_annotation_id(&transform).as_deref(),
            Some("comparison:rectangle:sparse:1")
        );
    }

    #[test]
    fn filters_appkit_persistence_arguments_from_document_paths() {
        let paths = initial_pdf_paths([
            OsString::from("-ApplePersistenceIgnoreState"),
            OsString::from("YES"),
            OsString::from("/tmp/Hibbeler.pdf"),
        ]);
        assert_eq!(paths, vec![PathBuf::from("/tmp/Hibbeler.pdf")]);
    }

    #[test]
    fn normalizes_the_performance_page_sequence_to_the_document() {
        assert_eq!(
            perf_page_sequence(526),
            vec![526, 43, 379, 132, 474, 263, 11, 504, 174, 1]
        );
        assert_eq!(perf_page_sequence(6), vec![6, 1, 5, 2, 3]);
        assert!(perf_page_sequence(0).is_empty());
    }

    #[test]
    fn matches_the_electron_fit_width_quantization_contract() {
        let zoom = resolve_fit_width_zoom(919.6, 952.0);
        assert_eq!(zoom, 90.0);
        assert!((952.0 + PAGE_LAYOUT_GAP * 2.0) * zoom / 100.0 <= 919.6);
    }

    #[test]
    fn fit_page_uses_the_more_constrained_axis() {
        assert_eq!(resolve_fit_page_zoom(1000.0, 500.0, 500.0, 1000.0), 46.0);
    }

    #[test]
    fn manual_zoom_uses_the_current_product_limits() {
        assert_eq!(clamp_zoom_percent(0.0), MIN_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(9000.0), MAX_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(194.04), 194.0);
    }

    #[test]
    fn page_view_modes_have_distinct_layout_membership() {
        assert_eq!(
            viewport_page_numbers(ScrollMode::Continuous, 4, 2),
            vec![1, 2, 3, 4],
        );
        assert_eq!(viewport_page_numbers(ScrollMode::SinglePage, 4, 2), vec![2],);
    }

    #[test]
    fn wheel_preferences_and_control_modifier_are_opposites() {
        assert!(should_scroll_viewport_wheel(ScrollWheelMode::Scroll, false));
        assert!(!should_scroll_viewport_wheel(ScrollWheelMode::Scroll, true));
        assert!(!should_scroll_viewport_wheel(ScrollWheelMode::Zoom, false));
        assert!(should_scroll_viewport_wheel(ScrollWheelMode::Zoom, true));
    }

    #[test]
    fn native_scroll_counts_physical_clicks_separately_from_timed_intervals() {
        assert_eq!(
            native_scroll_expected_events(20_000, 10_000, 120),
            Some((2_400, 2_400)),
        );
        assert_eq!(native_scroll_expected_events(20_000, 11_000, 120), None);
    }

    #[test]
    fn dynamic_scroll_derives_distance_bounded_counts_from_the_advertised_wheel_delta() {
        assert_eq!(
            native_distance_bounded_expected_events(639.0, 50.0, 120.0),
            Some(266),
        );
        assert_eq!(
            native_distance_bounded_expected_events(640.0, 50.0, 120.0),
            Some(267),
        );
        assert_eq!(
            native_distance_bounded_expected_events(0.0, 50.0, 120.0),
            None,
        );
        assert_eq!(
            native_distance_bounded_expected_events(640.0, 50.0, 78.0),
            Some(410),
        );
    }

    #[test]
    fn dynamic_scroll_calibration_rejects_zero_nonfinite_and_preserves_the_observed_unit() {
        assert_eq!(native_wheel_calibration_delta(-78.0), Some(78.0));
        assert_eq!(native_wheel_calibration_delta(0.0), None);
        assert_eq!(native_wheel_calibration_delta(f32::NAN), None);
        assert_eq!(native_wheel_calibration_delta(f32::INFINITY), None);
    }

    #[test]
    fn dynamic_scroll_applies_the_same_fixed_distance_per_forward_and_reverse_event() {
        assert_eq!(
            native_distance_bounded_offset_y(0.0, 120.0, 99, 0, 266, -1),
            Some(-11_880.0),
        );
        assert_eq!(
            native_distance_bounded_offset_y(0.0, 120.0, 266, 100, 266, -1),
            Some(-19_920.0),
        );
        assert_eq!(
            native_distance_bounded_offset_y(0.0, 120.0, 266, 266, 266, -1),
            Some(0.0),
        );
        assert_eq!(
            native_distance_bounded_offset_y(0.0, 120.0, 10, 11, 266, -1),
            None,
        );
    }

    #[test]
    fn native_scroll_rejects_coalesced_delta_and_wrong_peak_distance() {
        assert!(native_wheel_delta_is_unit(48.0, 48.0));
        assert!(!native_wheel_delta_is_unit(48.0, 96.0));
        assert!(native_peak_distance_matches(50.0, 50.0));
        assert!(native_peak_distance_matches(52.5, 50.0));
        assert!(!native_peak_distance_matches(53.0, 50.0));
    }

    #[test]
    fn native_scroll_frame_batch_preserves_the_frozen_fifty_viewport_path() {
        assert_eq!(
            native_scroll_batched_offset_y(0.0, 600.0, 50.0, 1_200, 2_400, 0, 2_400, -1),
            Some(-15_000.0),
        );
        assert_eq!(
            native_scroll_batched_offset_y(0.0, 600.0, 50.0, 2_400, 2_400, 0, 2_400, -1),
            Some(-30_000.0),
        );
        assert_eq!(
            native_scroll_batched_offset_y(0.0, 600.0, 50.0, 2_400, 2_400, 1_200, 2_400, -1,),
            Some(-15_000.0),
        );
        assert_eq!(
            native_scroll_batched_offset_y(0.0, 600.0, 50.0, 2_400, 2_400, 2_400, 2_400, -1,),
            Some(0.0),
        );
    }

    #[test]
    fn native_scroll_frame_batch_rejects_invalid_receipt_counts() {
        assert_eq!(
            native_scroll_batched_offset_y(0.0, 600.0, 50.0, 2_401, 2_400, 0, 2_400, -1),
            None,
        );
        assert_eq!(
            native_scroll_batched_offset_y(0.0, 600.0, 50.0, 1, 0, 0, 2_400, -1),
            None,
        );
        assert_eq!(
            native_scroll_batched_offset_y(0.0, 600.0, 50.0, 1, 2_400, 0, 2_400, 0),
            None,
        );
    }

    #[test]
    fn continuous_plan_prunes_stale_page_surface_work() {
        let mut pending = HashMap::from([
            ((7, 10, 612), 1),
            ((7, 11, 612), 2),
            ((7, 12, 612), 3),
            ((8, 99, 612), 4),
        ]);
        let request = |document_id, page, generation| PageSurfaceRequest {
            token: RequestToken {
                document_id,
                generation,
            },
            page,
            zoom_percent: 100.0,
            scale_factor: 1.0,
            pixel_width: 612,
        };
        let mut queue = VecDeque::from([
            request(7, 10, 1),
            request(7, 11, 2),
            request(7, 12, 3),
            request(8, 99, 4),
        ]);

        prune_page_surface_work_to_visible_pages(
            7,
            &HashSet::from([11, 12]),
            &mut pending,
            &mut queue,
        );

        assert_eq!(pending.len(), 3);
        assert!(!pending.contains_key(&(7, 10, 612)));
        assert!(pending.contains_key(&(7, 11, 612)));
        assert!(pending.contains_key(&(7, 12, 612)));
        assert!(pending.contains_key(&(8, 99, 612)));
        assert_eq!(
            queue
                .iter()
                .map(|request| (request.token.document_id, request.page))
                .collect::<Vec<_>>(),
            vec![(7, 11), (7, 12), (8, 99)],
        );
    }

    #[test]
    fn wheel_zoom_matches_the_electron_curve_and_limits() {
        assert!((resolve_wheel_zoom(100.0, -120.0) - 121.9).abs() < 0.1);
        assert!((resolve_wheel_zoom(100.0, 120.0) - 82.0).abs() < 0.1);
        assert_eq!(
            resolve_wheel_zoom(MAX_ZOOM_PERCENT, -120.0),
            MAX_ZOOM_PERCENT
        );
    }

    #[test]
    fn single_page_wheel_accumulates_before_navigation() {
        assert_eq!(resolve_single_page_wheel(2, 4, 0.0, 40.0), (None, 40.0));
        assert_eq!(resolve_single_page_wheel(2, 4, 40.0, 40.0), (Some(3), 0.0),);
        assert_eq!(resolve_single_page_wheel(1, 4, 0.0, -80.0), (None, 0.0));
    }

    #[test]
    fn stale_or_cross_document_results_are_rejected() {
        let current = RequestToken {
            document_id: 7,
            generation: 12,
        };
        assert!(is_current_request(Some(current), current));
        assert!(!is_current_request(
            Some(current),
            RequestToken {
                document_id: 7,
                generation: 11,
            }
        ));
        assert!(!is_current_request(
            Some(current),
            RequestToken {
                document_id: 8,
                generation: 12,
            }
        ));
        assert!(!is_current_request(None, current));
    }

    #[test]
    fn equal_zoom_refreshes_only_for_an_active_zoom_operation() {
        let mut scenario = PerfScenario::new(PerfScenarioKind::Zoom);
        assert!(!should_refresh_equal_zoom(Some(&scenario)));
        scenario.active_operation = Some(PerfOperation {
            kind: "zoom",
            value: 100.0,
            started_ms: 0.0,
        });
        assert!(should_refresh_equal_zoom(Some(&scenario)));
        scenario.active_operation = Some(PerfOperation {
            kind: "page",
            value: 1.0,
            started_ms: 0.0,
        });
        assert!(!should_refresh_equal_zoom(Some(&scenario)));
    }

    #[test]
    fn completed_viewer_settle_clears_the_previous_phase_before_the_next_step() {
        let mut scenario = PerfScenario::new(PerfScenarioKind::PageNavigation);
        scenario.step_index = 1;
        scenario.comparison_phase = ComparisonPhase::AwaitingViewerSettle {
            operation: ViewerOperation::Navigation,
            ready_at_ms: 250.0,
        };

        finish_viewer_settle(&mut scenario);

        assert!(matches!(scenario.comparison_phase, ComparisonPhase::Idle));
        assert_eq!(scenario.step_index, 1);
    }

    #[test]
    fn dynamic_runner_result_accepts_only_exact_presented_drawable_protocol() {
        assert_eq!(
            validate_dynamic_runner_result(
                r#"{"schema_version":1,"command_id":"viewer:dynamic-fidelity-scroll","status":"passed","crop_source":"XGetImage-presented-client-drawable","error":null}"#,
            ),
            Ok(true),
        );
        assert_eq!(
            validate_dynamic_runner_result(
                r#"{"schema_version":1,"command_id":"viewer:dynamic-fidelity-scroll","status":"failed","crop_source":"XGetImage-presented-client-drawable","error":"crop mismatch"}"#,
            ),
            Ok(false),
        );
        assert!(
            validate_dynamic_runner_result(
                r#"{"schema_version":1,"command_id":"viewer:dynamic-fidelity-scroll","status":"passed","crop_source":"internal-raster","error":null}"#,
            )
            .is_err()
        );
        assert!(
            validate_dynamic_runner_result(
                r#"{"schema_version":1,"command_id":"viewer:dynamic-fidelity-scroll","status":"passed","crop_source":"XGetImage-presented-client-drawable","error":"ignored"}"#,
            )
            .is_err()
        );
    }
}
