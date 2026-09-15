//! Native-input adapter for the GPUI annotation surface.
//!
//! The interface accepts page-local PDF points and returns authoritative
//! document/thumbnail scenes. GPUI event capture, focus, and paint stay in the
//! binary; command ordering, typed tools, gesture lifetime, frozen defaults,
//! history, and selection live here.

use std::collections::HashMap;

#[path = "tool_properties.rs"]
pub mod tool_properties;
pub use tool_properties::{
    TOOL_FONT_FAMILY_OPTIONS, ToolFontFamilyOption, ToolProperties, ToolPropertyField,
    ToolPropertyRange,
};

use crate::annotation_model::{
    Annotation, AnnotationCommand, AnnotationDocument, AnnotationEdit, AnnotationError,
    AnnotationKind, AnnotationScene, AnnotationSnapshot, ArcAnnotation, ArcControlPoint,
    CalloutAnnotation, CalloutAppearance, CloudAnnotation, CloudPlusAnnotation,
    CloudPlusAppearance, CommandOutcome, DecodedRgbaAsset, DimensionAnnotation,
    DimensionAppearance, EllipseAnnotation, GestureKind, ImageAnnotation, InkTool,
    LengthAnnotation, LengthCalibration, LengthEndpoint, LineEndpoint, LineKind, MarkupId,
    MeasurementPathAnnotation, MeasurementPathKind, PageRotation, PageRotationDirection, PageScale,
    PdfPoint, PdfRect, PenAnnotation, PenAppearance, PointerCancelReason, PointerTool,
    RectangleAnnotation, RectangleAppearance, RectangleResizeHandle, RedactAnnotation, ScalePreset,
    SceneArc, SceneCallout, SceneCloud, SceneCloudPlus, SceneDimension, SceneLength,
    SceneMeasurementPath, SceneRedact, SceneSnapshot, SceneStraightLine, SceneVertexPath,
    SnapshotAnnotation, SpatialQueryWork, StraightLineAnnotation, StraightLineAppearance,
    StrokeStyle, TextBoxAnnotation, TextBoxStyle, VertexPathAnnotation, VertexPathKind,
};
use crate::cloud_plus_routing::{
    CloudPlusRoutingContext, place_initial_cloud_plus_text_box, route_cloud_plus_leader,
};
use crate::density_fixture::{DensityFixtureImportOutcome, materialize_density_fixture};
use crate::native_editing_v5::{
    InclusiveLInfGridSnap, NativeEditingV5Error, PropertyEditCommit, PropertyEditPlan,
    SnapGestureCommit, SnapResolution, SnapTransformPlan, StrokeWidthEditTransaction, Translation,
};
use crate::selection_geometry::{
    SelectionMarquee, SelectionOperation, SelectionPoint, SelectionShape,
};
use crate::semantic_snapping::{
    SemanticSnapDecision, SemanticSnapIndex, SemanticSnapSettings,
    quantize_pdf_distance_to_mm_increment, resolve_construction_grid_point,
};

pub const FROZEN_TEXT_CREATE: &str = "Beam B-12 / revision 3";
pub const NATURAL_IMAGE_MAX_PAGE_FRACTION: f64 = 0.45;
const TEXT_WIDTH_PT: f64 = 240.0;
const TEXT_HEIGHT_PT: f64 = 72.0;
const HIGHLIGHT_MIN_DISTANCE_PT: f64 = 0.5;
const POINTER_DRAG_THRESHOLD_CSS_PX: f64 = 3.0;
const CLOUD_PLUS_TEXT_WIDTH_PT: f64 = 150.0;
const CLOUD_PLUS_TEXT_HEIGHT_PT: f64 = 44.0;
const CLOUD_PLUS_TEXT_GAP_PT: f64 = 24.0;
const LENGTH_MINIMUM_PDF_DISTANCE: f64 = 2.0;
const ARC_MINIMUM_BULGE_CSS_PX: f64 = 8.0;
pub const ROTATION_HANDLE_OFFSET_CSS_PX: f64 = 12.0;
pub const ELLIPSE_ROTATION_HANDLE_ID: &str = "ellipse.rotate";
pub const LENGTH_SCALE_REQUIRED_MESSAGE: &str =
    "Set page scale before placing measurement markups.";
pub const ARC_START_HANDLE_ID: &str = "arc.point.start";
pub const ARC_MID_HANDLE_ID: &str = "arc.point.mid";
pub const ARC_END_HANDLE_ID: &str = "arc.point.end";
pub const ARC_BODY_ID: &str = "arc.body";
pub const REDACT_BODY_ID: &str = "redact.body";
pub const SNAPSHOT_BODY_ID: &str = "snapshot.body";
pub const DIMENSION_START_HANDLE_ID: &str = "dimension.endpoint.start";
pub const DIMENSION_END_HANDLE_ID: &str = "dimension.endpoint.end";
pub const DIMENSION_OFFSET_HANDLE_ID: &str = "dimension.offset";
pub const DIMENSION_BODY_ID: &str = "dimension.body";
pub const CALLOUT_TEXT_BOX_ID: &str = "callout.text-box";
pub const CALLOUT_BODY_ID: &str = "callout.body";
pub const CLOUD_BODY_ID: &str = "cloud.body";
pub const PENDING_REDACTION_STATUS: &str = "Pending redaction mark — saving keeps the underlying PDF content; this mark does not securely remove text or graphics.";

pub const fn redact_resize_handle_id(handle: RectangleResizeHandle) -> &'static str {
    match handle {
        RectangleResizeHandle::NorthWest => "redact.resize.nw",
        RectangleResizeHandle::North => "redact.resize.n",
        RectangleResizeHandle::NorthEast => "redact.resize.ne",
        RectangleResizeHandle::East => "redact.resize.e",
        RectangleResizeHandle::SouthEast => "redact.resize.se",
        RectangleResizeHandle::South => "redact.resize.s",
        RectangleResizeHandle::SouthWest => "redact.resize.sw",
        RectangleResizeHandle::West => "redact.resize.w",
    }
}

pub const fn snapshot_resize_handle_id(handle: RectangleResizeHandle) -> &'static str {
    match handle {
        RectangleResizeHandle::NorthWest => "snapshot.resize.nw",
        RectangleResizeHandle::North => "snapshot.resize.n",
        RectangleResizeHandle::NorthEast => "snapshot.resize.ne",
        RectangleResizeHandle::East => "snapshot.resize.e",
        RectangleResizeHandle::SouthEast => "snapshot.resize.se",
        RectangleResizeHandle::South => "snapshot.resize.s",
        RectangleResizeHandle::SouthWest => "snapshot.resize.sw",
        RectangleResizeHandle::West => "snapshot.resize.w",
    }
}

pub fn snapshot_resize_handle_point(
    annotation: &SnapshotAnnotation,
    handle: RectangleResizeHandle,
) -> PdfPoint {
    handle.world_point(annotation.rect, annotation.rotation_degrees())
}

pub fn snapshot_rotation_handle_point(
    annotation: &SnapshotAnnotation,
    observed_pixels_per_point: f64,
) -> Result<PdfPoint, AnnotationError> {
    ellipse_rotation_handle_point_for_rect(
        annotation.rect,
        annotation.rotation_degrees(),
        observed_pixels_per_point,
    )
}

pub fn redact_resize_handle_point(
    annotation: &RedactAnnotation,
    handle: RectangleResizeHandle,
) -> PdfPoint {
    axis_aligned_resize_handle_point(annotation.rect, handle)
}

fn axis_aligned_resize_handle_point(rect: PdfRect, handle: RectangleResizeHandle) -> PdfPoint {
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

pub const fn ellipse_resize_handle_id(handle: RectangleResizeHandle) -> &'static str {
    match handle {
        RectangleResizeHandle::NorthWest => "ellipse.resize.nw",
        RectangleResizeHandle::North => "ellipse.resize.n",
        RectangleResizeHandle::NorthEast => "ellipse.resize.ne",
        RectangleResizeHandle::East => "ellipse.resize.e",
        RectangleResizeHandle::SouthEast => "ellipse.resize.se",
        RectangleResizeHandle::South => "ellipse.resize.s",
        RectangleResizeHandle::SouthWest => "ellipse.resize.sw",
        RectangleResizeHandle::West => "ellipse.resize.w",
    }
}

pub fn ellipse_resize_handle_point(
    annotation: &EllipseAnnotation,
    handle: RectangleResizeHandle,
) -> PdfPoint {
    ellipse_resize_handle_point_for_rect(annotation.rect, annotation.rotation_degrees, handle)
}

pub fn ellipse_resize_handle_point_for_rect(
    rect: PdfRect,
    rotation_degrees: f64,
    handle: RectangleResizeHandle,
) -> PdfPoint {
    let center = PdfPoint {
        x: rect.x + rect.width / 2.,
        y: rect.y + rect.height / 2.,
    };
    let diagonal_x = rect.width * 0.5 * std::f64::consts::FRAC_1_SQRT_2;
    let diagonal_y = rect.height * 0.5 * std::f64::consts::FRAC_1_SQRT_2;
    let local = match handle {
        RectangleResizeHandle::NorthWest => PdfPoint {
            x: center.x - diagonal_x,
            y: center.y + diagonal_y,
        },
        RectangleResizeHandle::NorthEast => PdfPoint {
            x: center.x + diagonal_x,
            y: center.y + diagonal_y,
        },
        RectangleResizeHandle::SouthEast => PdfPoint {
            x: center.x + diagonal_x,
            y: center.y - diagonal_y,
        },
        RectangleResizeHandle::SouthWest => PdfPoint {
            x: center.x - diagonal_x,
            y: center.y - diagonal_y,
        },
        _ => handle.point(rect),
    };
    rotate_point_around_rect_center(local, rect, -rotation_degrees)
}

pub fn ellipse_rotation_handle_point(
    annotation: &EllipseAnnotation,
    observed_pixels_per_point: f64,
) -> Result<PdfPoint, AnnotationError> {
    ellipse_rotation_handle_point_for_rect(
        annotation.rect,
        annotation.rotation_degrees,
        observed_pixels_per_point,
    )
}

pub fn ellipse_rotation_handle_point_for_rect(
    rect: PdfRect,
    rotation_degrees: f64,
    observed_pixels_per_point: f64,
) -> Result<PdfPoint, AnnotationError> {
    if !observed_pixels_per_point.is_finite() || observed_pixels_per_point <= 0. {
        return Err(AnnotationError::InvalidGeometry(
            "ellipse handle scale must be positive and finite".into(),
        ));
    }
    let local = PdfPoint {
        x: rect.x + rect.width / 2.,
        y: rect.y + rect.height + ROTATION_HANDLE_OFFSET_CSS_PX / observed_pixels_per_point,
    };
    Ok(rotate_point_around_rect_center(
        local,
        rect,
        -rotation_degrees,
    ))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectangleSnapSettings {
    enabled: bool,
    grid_spacing_pt: f64,
    sensitivity_css_px: f64,
}

impl RectangleSnapSettings {
    pub fn new(
        enabled: bool,
        grid_spacing_pt: f64,
        sensitivity_css_px: f64,
    ) -> Result<Self, AnnotationError> {
        if !grid_spacing_pt.is_finite()
            || !sensitivity_css_px.is_finite()
            || grid_spacing_pt <= 0.0
            || sensitivity_css_px < 0.0
        {
            return Err(AnnotationError::InvalidGeometry(
                "rectangle snap grid spacing must be positive and sensitivity must be nonnegative"
                    .into(),
            ));
        }
        Ok(Self {
            enabled,
            grid_spacing_pt,
            sensitivity_css_px,
        })
    }

    pub fn enabled(self) -> bool {
        self.enabled
    }

    pub fn grid_spacing_pt(self) -> f64 {
        self.grid_spacing_pt
    }

    pub fn sensitivity_css_px(self) -> f64 {
        self.sensitivity_css_px
    }
}

impl Default for RectangleSnapSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            grid_spacing_pt: 18.0,
            sensitivity_css_px: 8.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ObservedPixelsPerPoint(f64);

impl Default for ObservedPixelsPerPoint {
    fn default() -> Self {
        Self(1.0)
    }
}

#[derive(Clone, Copy, Debug)]
struct ImagePlacementPage {
    width_pt: f64,
    height_pt: f64,
    max_fraction: f64,
}

#[derive(Clone)]
struct PendingImageAsset {
    asset: DecodedRgbaAsset,
    aspect_locked: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum AnnotationTool {
    #[default]
    Select,
    Rectangle,
    Ellipse,
    Arc,
    Redact,
    Line,
    Arrow,
    Polyline,
    Polygon,
    Polylength,
    Area,
    Cloud,
    CloudPlus,
    Callout,
    Dimension,
    Pen,
    Highlight,
    TextBox,
    Length,
    Image,
    Snapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightPaintCapability {
    /// The domain preserves Multiply, but GPUI CE's public path paint API only
    /// exposes source-alpha color. The current migration app therefore cannot
    /// prove Electron/PDF Multiply compositing parity.
    SourceAlphaFallback,
}

impl AnnotationTool {
    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Arc => "Arc",
            Self::Redact => "Redact",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
            Self::Polyline => "Polyline",
            Self::Polygon => "Polygon",
            Self::Polylength => "Polylength",
            Self::Area => "Area",
            Self::Cloud => "Cloud",
            Self::CloudPlus => "Cloud+",
            Self::Callout => "Callout",
            Self::Dimension => "Dimension",
            Self::Pen => "Pen",
            Self::Highlight => "Highlight",
            Self::TextBox => "Text Box",
            Self::Length => "Length",
            Self::Image => "Insert Image",
            Self::Snapshot => "Snapshot",
        }
    }

    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            Self::Select => Some("V"),
            Self::Rectangle => Some("R"),
            Self::Ellipse => Some("E"),
            Self::Arc => Some("Shift+C"),
            Self::Line => Some("L"),
            Self::Arrow => Some("A"),
            Self::Polyline => Some("Shift+N"),
            Self::Polygon => Some("Shift+P"),
            Self::Polylength => Some("Shift+Alt+Q"),
            Self::Area => Some("Shift+Alt+A"),
            Self::Cloud => Some("C"),
            Self::CloudPlus => Some("K"),
            Self::Callout => Some("Q"),
            Self::Dimension => Some("Shift+L"),
            Self::Pen => Some("P"),
            Self::Highlight => Some("H"),
            Self::Snapshot => Some("G"),
            Self::Redact | Self::TextBox | Self::Length | Self::Image => None,
        }
    }

    pub fn tooltip_label(self) -> String {
        self.shortcut().map_or_else(
            || self.label().to_owned(),
            |shortcut| format!("{} ({shortcut})", self.label()),
        )
    }

    pub fn from_plain_shortcut(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "v" => Some(Self::Select),
            "r" => Some(Self::Rectangle),
            "e" => Some(Self::Ellipse),
            "l" => Some(Self::Line),
            "a" => Some(Self::Arrow),
            "c" => Some(Self::Cloud),
            "k" => Some(Self::CloudPlus),
            "q" => Some(Self::Callout),
            "p" => Some(Self::Pen),
            "h" => Some(Self::Highlight),
            "g" => Some(Self::Snapshot),
            _ => None,
        }
    }

    pub fn toolbar_id(self) -> &'static str {
        match self {
            Self::Select => "general-mouse-pointer-2",
            Self::Rectangle => "draw-square",
            Self::Ellipse => "draw-ellipse",
            Self::Arc => "tool-arc",
            Self::Redact => "tool-redact",
            Self::Line => "markup-line",
            Self::Arrow => "markup-arrow",
            Self::Polyline => "markup-polyline",
            Self::Polygon => "markup-polygon",
            Self::Polylength => "tool-polylength",
            Self::Area => "tool-area",
            Self::Cloud => "tool-cloud",
            Self::CloudPlus => "tool-cloud-plus",
            Self::Callout => "tool-callout",
            Self::Dimension => "tool-dimension",
            Self::Pen => "markup-pen",
            Self::Highlight => "markup-highlighter",
            Self::TextBox => "markup-type",
            Self::Length => "measure-ruler-dimension-line",
            Self::Image => "markup-image",
            Self::Snapshot => "tool-snapshot",
        }
    }

    pub fn from_toolbar_id(value: &str) -> Option<Self> {
        match value {
            "general-mouse-pointer-2" => Some(Self::Select),
            "draw-square" => Some(Self::Rectangle),
            "draw-ellipse" => Some(Self::Ellipse),
            "tool-arc" => Some(Self::Arc),
            "tool-redact" => Some(Self::Redact),
            "markup-line" => Some(Self::Line),
            "markup-arrow" => Some(Self::Arrow),
            "markup-polyline" => Some(Self::Polyline),
            "markup-polygon" => Some(Self::Polygon),
            "tool-polylength" => Some(Self::Polylength),
            "tool-area" => Some(Self::Area),
            "tool-cloud" => Some(Self::Cloud),
            "tool-cloud-plus" => Some(Self::CloudPlus),
            "tool-callout" => Some(Self::Callout),
            "tool-dimension" => Some(Self::Dimension),
            "markup-pen" => Some(Self::Pen),
            "markup-highlighter" => Some(Self::Highlight),
            "markup-type" => Some(Self::TextBox),
            "measure-ruler-dimension-line" => Some(Self::Length),
            "markup-image" => Some(Self::Image),
            "tool-snapshot" => Some(Self::Snapshot),
            _ => None,
        }
    }

    pub fn uses_crosshair(self) -> bool {
        !matches!(self, Self::Select)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PointerPhaseOutcome {
    GestureStarted,
    PlacementPending,
    SelectionChanged(Option<MarkupId>),
    AnnotationCreated(MarkupId),
    AnnotationEdited(MarkupId),
    Ignored,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StraightLinePropertyEdit {
    StrokeColor(String),
    StrokeWidthPt(f64),
    Opacity(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub enum VertexPathPropertyEdit {
    StrokeColor(String),
    StrokeWidthPt(f64),
    Opacity(f64),
    FillColor(Option<String>),
}

fn vertex_path_property_rgb(color: String) -> String {
    if color.len() == 9
        && color.starts_with('#')
        && color[1..].chars().all(|digit| digit.is_ascii_hexdigit())
    {
        color[..7].to_owned()
    } else {
        color
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PointerInputModifiers {
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Debug)]
enum ActivePointer {
    Marquee {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        marquee: SelectionMarquee,
        pdf_points: Vec<PdfPoint>,
    },
    GroupMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        start: PdfPoint,
        current: PdfPoint,
    },
    Domain {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        ink: bool,
        ink_start: Option<PdfPoint>,
        rectangle_translation_start: Option<PdfPoint>,
        rectangle_create_start: Option<PdfPoint>,
        click_placement_pending: bool,
    },
    EllipseCreate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        appearance: RectangleAppearance,
        start: PdfPoint,
        current: PdfPoint,
        click_placement_pending: bool,
    },
    EllipseMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    EllipseResize {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        handle: RectangleResizeHandle,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
        original_rotation_degrees: f64,
    },
    EllipseRotate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
        original_rotation_degrees: f64,
    },
    RedactCreate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        viewport_start: SelectionPoint,
        current: PdfPoint,
        click_placement_pending: bool,
    },
    RedactMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    RedactResize {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        handle: RectangleResizeHandle,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    ArcMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        expected_revision: u64,
        start: PdfPoint,
        current: PdfPoint,
        original: ArcAnnotation,
    },
    ArcControlPoint {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        expected_revision: u64,
        control: ArcControlPoint,
        start: PdfPoint,
        current: PdfPoint,
        original: ArcAnnotation,
        snap_quarter_turn: bool,
    },
    StraightLineCreate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        kind: LineKind,
        appearance: StraightLineAppearance,
        start: PdfPoint,
        current: PdfPoint,
        click_placement_pending: bool,
    },
    CalloutCreate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        click_placement_pending: bool,
    },
    CloudPlusCreate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
    },
    StraightLineMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_start: PdfPoint,
        original_end: PdfPoint,
    },
    StraightLineEndpoint {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        endpoint: LineEndpoint,
        start: PdfPoint,
        current: PdfPoint,
    },
    VertexPathPoint {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        vertex_index: usize,
        start: PdfPoint,
        current: PdfPoint,
    },
    MeasurementPathPoint {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        vertex_index: usize,
        start: PdfPoint,
        current: PdfPoint,
    },
    InkMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_paths: Vec<Vec<PdfPoint>>,
    },
    TextBoxMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    TextBoxResize {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        handle: RectangleResizeHandle,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    ImageMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    ImageResize {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        handle: ImageResizeHandle,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    SnapshotMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
    },
    SnapshotResize {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        handle: RectangleResizeHandle,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
        original_rotation_degrees: f64,
    },
    SnapshotRotate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
        original_rotation_degrees: f64,
    },
    LengthCreate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
    },
    DimensionCreate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
    },
    DimensionEdit {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        expected_revision: u64,
        kind: DimensionPointerEditKind,
        start: PdfPoint,
        current: PdfPoint,
        original: DimensionAnnotation,
    },
    CalloutEdit {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        expected_revision: u64,
        kind: CalloutPointerEditKind,
        start: PdfPoint,
        current: PdfPoint,
        original: CalloutAnnotation,
    },
    CloudEdit {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        expected_revision: u64,
        kind: CloudPointerEditKind,
        start: PdfPoint,
        current: PdfPoint,
        original: CloudAnnotation,
    },
    LengthEndpoint {
        document_id: u64,
        pointer_id: u64,
        id: MarkupId,
        endpoint: LengthEndpoint,
        current: PdfPoint,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DimensionPointerEditKind {
    Start,
    End,
    Offset,
    Body,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CalloutPointerEditKind {
    LeaderPoint(usize),
    TextBox,
    Body,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudPointerEditKind {
    Vertex(usize),
    Body,
}

#[derive(Clone, Debug)]
struct VertexPathDraft {
    document_id: u64,
    page_index: u32,
    id: MarkupId,
    kind: VertexPathKind,
    points: Vec<PdfPoint>,
    hover: PdfPoint,
}

#[derive(Clone, Debug)]
struct MeasurementPathDraft {
    document_id: u64,
    page_index: u32,
    id: MarkupId,
    kind: MeasurementPathKind,
    calibration: LengthCalibration,
    points: Vec<PdfPoint>,
    hover: PdfPoint,
}

#[derive(Clone, Debug)]
struct ArcDraft {
    document_id: u64,
    page_index: u32,
    id: MarkupId,
    start: PdfPoint,
    end: Option<PdfPoint>,
    mid: PdfPoint,
    appearance: RectangleAppearance,
}

#[derive(Clone, Debug)]
struct SnapshotDraft {
    document_id: u64,
    page_index: u32,
    pointer_id: u64,
    id: MarkupId,
    start: PdfPoint,
    current: PdfPoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ImageResizeHandle {
    SouthWest,
    South,
    SouthEast,
    East,
    NorthEast,
    North,
    NorthWest,
    West,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EllipseHandleKind {
    Resize(RectangleResizeHandle),
    Rotate,
}

/// Evidence bookkeeping for the frozen benchmark replay.
///
/// The gesture itself must remain in `ActivePointer` and flow through the
/// ordinary `pointer_down`/`pointer_move`/`pointer_up` product path. This
/// structure records only the facts needed to build the benchmark receipt.
#[derive(Clone, Debug)]
struct NativeV5SnapObservation {
    document_id: u64,
    plan: SnapTransformPlan,
    pointer_id: u64,
    history_before: (usize, usize),
    appearance: RectangleAppearance,
    observed_pixels_per_point: f64,
    sample_count: usize,
    latest: Option<SnapResolution>,
}

#[derive(Default)]
pub struct AnnotationAdapter {
    tool: AnnotationTool,
    documents: HashMap<u64, AnnotationDocument>,
    active: Option<ActivePointer>,
    vertex_path_draft: Option<VertexPathDraft>,
    cloud_draft: Option<VertexPathDraft>,
    cloud_plus_draft: Option<VertexPathDraft>,
    measurement_path_draft: Option<MeasurementPathDraft>,
    arc_draft: Option<ArcDraft>,
    snapshot_draft: Option<SnapshotDraft>,
    next_sequence: u64,
    queued_id: Option<MarkupId>,
    queued_rectangle_appearance: Option<RectangleAppearance>,
    tool_properties: HashMap<AnnotationTool, ToolProperties>,
    queued_text_content: Option<String>,
    image_asset: Option<PendingImageAsset>,
    snapshot_capture_asset: Option<DecodedRgbaAsset>,
    image_placement_page: Option<ImagePlacementPage>,
    native_v5_property_receipt: Option<(u64, PropertyEditCommit)>,
    native_v5_snap_observation: Option<NativeV5SnapObservation>,
    native_v5_snap_receipt: Option<(u64, SnapGestureCommit)>,
    rectangle_snap_settings: RectangleSnapSettings,
    semantic_snap_settings: SemanticSnapSettings,
    semantic_snap_decision: Option<SemanticSnapDecision>,
    semantic_snap_page_sizes: HashMap<(u64, u32), (f64, f64)>,
    observed_pixels_per_point: ObservedPixelsPerPoint,
}

impl AnnotationAdapter {
    pub fn load_imported_annotations(
        &mut self,
        document_id: u64,
        annotations: Vec<Annotation>,
    ) -> Result<(), AnnotationError> {
        self.image_asset = None;
        self.image_placement_page = None;
        let imported_length_calibrations = annotations
            .iter()
            .filter_map(|annotation| match annotation {
                Annotation::Length(length) => {
                    Some((length.page_index, length.calibration().clone()))
                }
                Annotation::MeasurementPath(measurement) => {
                    Some((measurement.page_index, measurement.calibration().clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut document = AnnotationDocument::default();
        document.load_imported_annotations(annotations, imported_length_calibrations)?;
        if self
            .active_surface()
            .is_some_and(|(active_document_id, _)| active_document_id == document_id)
        {
            self.cancel(PointerCancelReason::PageChanged)?;
        }
        self.documents.insert(document_id, document);
        Ok(())
    }

    pub fn load_imported_annotations_with_page_scales(
        &mut self,
        document_id: u64,
        annotations: Vec<Annotation>,
        page_length_calibrations: Vec<(u32, LengthCalibration)>,
    ) -> Result<(), AnnotationError> {
        self.load_imported_annotations_with_document_state(
            document_id,
            annotations,
            page_length_calibrations,
            Vec::new(),
        )
    }

    pub fn load_imported_annotations_with_document_state(
        &mut self,
        document_id: u64,
        annotations: Vec<Annotation>,
        page_length_calibrations: Vec<(u32, LengthCalibration)>,
        page_rotations: Vec<(u32, PageRotation)>,
    ) -> Result<(), AnnotationError> {
        let mut imported_length_calibrations = annotations
            .iter()
            .filter_map(|annotation| match annotation {
                Annotation::Length(length) => {
                    Some((length.page_index, length.calibration().clone()))
                }
                Annotation::MeasurementPath(measurement) => {
                    Some((measurement.page_index, measurement.calibration().clone()))
                }
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        for (page_index, calibration) in page_length_calibrations {
            imported_length_calibrations
                .entry(page_index)
                .and_modify(|imported| {
                    if !imported.same_scale_as(&calibration) {
                        *imported = calibration.clone();
                    }
                })
                .or_insert(calibration);
        }
        let mut document = AnnotationDocument::default();
        document.load_imported_document_state(
            annotations,
            imported_length_calibrations.into_iter().collect(),
            page_rotations,
        )?;
        if self
            .active_surface()
            .is_some_and(|(active_document_id, _)| active_document_id == document_id)
        {
            self.cancel(PointerCancelReason::PageChanged)?;
        }
        self.documents.insert(document_id, document);
        Ok(())
    }

    pub fn load_imported_annotations_with_page_scale_state(
        &mut self,
        document_id: u64,
        annotations: Vec<Annotation>,
        page_scales: Vec<(u32, PageScale)>,
        scale_presets: Vec<ScalePreset>,
        page_rotations: Vec<(u32, PageRotation)>,
    ) -> Result<(), AnnotationError> {
        let mut document = AnnotationDocument::default();
        document.load_imported_page_scale_state(
            annotations,
            page_scales,
            scale_presets,
            page_rotations,
        )?;
        if self
            .active_surface()
            .is_some_and(|(active_document_id, _)| active_document_id == document_id)
        {
            self.cancel(PointerCancelReason::PageChanged)?;
        }
        self.documents.insert(document_id, document);
        Ok(())
    }

    pub fn rotate_document_page(
        &mut self,
        document_id: u64,
        page_index: u32,
        direction: PageRotationDirection,
    ) -> Result<PageRotation, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .rotate_page(page_index, direction)
    }

    pub fn document_page_rotation(
        &self,
        document_id: u64,
        page_index: u32,
    ) -> Option<PageRotation> {
        self.documents
            .get(&document_id)
            .and_then(|document| document.page_rotation(page_index))
    }

    pub fn load_density_fixture(
        &mut self,
        document_id: u64,
        fixture_json: &str,
    ) -> Result<DensityFixtureImportOutcome, AnnotationError> {
        let materialized = materialize_density_fixture(fixture_json)?;
        self.load_imported_annotations(document_id, materialized.annotations)?;
        Ok(materialized.outcome)
    }

    fn prepare_native_v5_rectangle(
        &mut self,
        document_id: u64,
        id: MarkupId,
        page_index: u32,
        rect: PdfRect,
        stroke_width_pt: f64,
    ) -> Result<(usize, usize), NativeEditingV5Error> {
        self.cancel(PointerCancelReason::ToolChanged)?;
        self.native_v5_property_receipt = None;
        self.native_v5_snap_observation = None;
        self.native_v5_snap_receipt = None;
        let document = self.documents.entry(document_id).or_default();
        if document
            .rectangles()
            .iter()
            .any(|rectangle| rectangle.id == id)
        {
            return Err(NativeEditingV5Error::TargetChanged(id));
        }
        let appearance =
            RectangleAppearance::new("#2563eb", stroke_width_pt, Some("#dbeafe"), 1.0)?
                .with_fill_opacity(0.2)?;
        document.apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
            RectangleAnnotation {
                id: id.clone(),
                page_index,
                rect,
                rotation_degrees: 0.0,
                appearance,
                locked: false,
            },
        )))?;
        if !document.select(&id) {
            return Err(NativeEditingV5Error::TargetNotFound(id));
        }
        Ok(document.history_depths())
    }

    pub fn prepare_native_v5_property(
        &mut self,
        document_id: u64,
        plan: &PropertyEditPlan,
    ) -> Result<(usize, usize), NativeEditingV5Error> {
        self.prepare_native_v5_rectangle(
            document_id,
            plan.target_id.clone(),
            0,
            plan.setup_rect,
            plan.original_stroke_width_pt,
        )
    }

    pub fn commit_native_v5_property(
        &mut self,
        document_id: u64,
        plan: &PropertyEditPlan,
    ) -> Result<PropertyEditCommit, NativeEditingV5Error> {
        {
            let document = self
                .documents
                .get(&document_id)
                .ok_or_else(|| NativeEditingV5Error::TargetNotFound(plan.target_id.clone()))?;
            let transaction = plan.begin_transaction(document)?;
            if transaction.staged_appearance().stroke_width_pt() != plan.edited_stroke_width_pt
                || document.selected_id() != Some(&plan.target_id)
            {
                return Err(NativeEditingV5Error::TargetChanged(plan.target_id.clone()));
            }
        }
        let receipt =
            self.commit_selected_rectangle_stroke_width(document_id, plan.edited_stroke_width_pt)?;
        self.native_v5_property_receipt = Some((document_id, receipt.clone()));
        Ok(receipt)
    }

    pub fn undo_native_v5_property(
        &mut self,
        document_id: u64,
    ) -> Result<PropertyEditCommit, NativeEditingV5Error> {
        let (receipt_document_id, receipt) =
            self.native_v5_property_receipt.as_ref().ok_or_else(|| {
                NativeEditingV5Error::HistoryInvariant("property receipt is missing".into())
            })?;
        if *receipt_document_id != document_id {
            return Err(NativeEditingV5Error::HistoryInvariant(
                "property receipt belongs to another document".into(),
            ));
        }
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or_else(|| NativeEditingV5Error::TargetNotFound(receipt.target_id.clone()))?;
        if !document.undo()? {
            return Err(NativeEditingV5Error::HistoryInvariant(
                "property undo did not apply".into(),
            ));
        }
        receipt.verify_undone(document)?;
        Ok(receipt.clone())
    }

    pub fn prepare_native_v5_snap(
        &mut self,
        document_id: u64,
        plan: &SnapTransformPlan,
    ) -> Result<(usize, usize), NativeEditingV5Error> {
        self.prepare_native_v5_rectangle(
            document_id,
            plan.target_id.clone(),
            plan.page_index,
            plan.setup_rect,
            1.5,
        )
    }

    pub fn begin_native_v5_snap(
        &mut self,
        document_id: u64,
        plan: &SnapTransformPlan,
        pointer_id: u64,
        tolerance_pt: f64,
        observed_pixels_per_point: f64,
    ) -> Result<(), NativeEditingV5Error> {
        if self.native_v5_snap_observation.is_some() {
            return Err(NativeEditingV5Error::GestureInvariant(
                "snap gesture is already active".into(),
            ));
        }
        let (history_before, appearance) = {
            let document = self
                .documents
                .get(&document_id)
                .ok_or_else(|| NativeEditingV5Error::TargetNotFound(plan.target_id.clone()))?;
            let rectangle = document
                .rectangles()
                .iter()
                .find(|rectangle| rectangle.id == plan.target_id)
                .ok_or_else(|| NativeEditingV5Error::TargetNotFound(plan.target_id.clone()))?;
            if rectangle.page_index != plan.page_index || rectangle.rect != plan.setup_rect {
                return Err(NativeEditingV5Error::TargetChanged(plan.target_id.clone()));
            }
            (document.history_depths(), rectangle.appearance.clone())
        };

        self.set_rectangle_snap_settings(RectangleSnapSettings::new(
            true,
            plan.grid_spacing_pt,
            plan.sensitivity_css_px,
        )?)?;
        self.set_observed_pixels_per_point(observed_pixels_per_point)?;
        self.set_tool(AnnotationTool::Select)?;
        let outcome = self.pointer_down(
            document_id,
            plan.page_index,
            pointer_id,
            plan.start,
            tolerance_pt,
        )?;
        if outcome != PointerPhaseOutcome::GestureStarted
            || self
                .documents
                .get(&document_id)
                .and_then(AnnotationDocument::selected_id)
                != Some(&plan.target_id)
            || self.active_surface() != Some((document_id, plan.page_index))
        {
            self.cancel(PointerCancelReason::AdapterError)?;
            return Err(NativeEditingV5Error::GestureInvariant(
                "ordinary pointer down did not acquire the frozen snap target".into(),
            ));
        }
        self.native_v5_snap_observation = Some(NativeV5SnapObservation {
            document_id,
            plan: plan.clone(),
            pointer_id,
            history_before,
            appearance,
            observed_pixels_per_point,
            sample_count: 0,
            latest: None,
        });
        Ok(())
    }

    pub fn update_native_v5_snap(
        &mut self,
        document_id: u64,
        point: PdfPoint,
    ) -> Result<SnapResolution, NativeEditingV5Error> {
        let (observation_document_id, pointer_id, start, settings, observed_pixels_per_point) = {
            let observation = self.native_v5_snap_observation.as_ref().ok_or_else(|| {
                NativeEditingV5Error::GestureInvariant("snap gesture is missing".into())
            })?;
            (
                observation.document_id,
                observation.pointer_id,
                observation.plan.start,
                self.rectangle_snap_settings,
                observation.observed_pixels_per_point,
            )
        };
        if observation_document_id != document_id {
            return Err(NativeEditingV5Error::GestureInvariant(
                "snap gesture belongs to another document".into(),
            ));
        }
        let resolution = rectangle_translation_snap_resolution(
            start,
            point,
            settings,
            observed_pixels_per_point,
        )
        .ok_or_else(|| {
            NativeEditingV5Error::GestureInvariant("ordinary rectangle snap is disabled".into())
        })?;
        self.pointer_move(pointer_id, point)?;
        let observation = self
            .native_v5_snap_observation
            .as_mut()
            .expect("the snap observation remains active during pointer move");
        observation.sample_count = observation.sample_count.saturating_add(1);
        observation.latest = Some(resolution);
        Ok(resolution)
    }

    pub fn commit_native_v5_snap(
        &mut self,
        document_id: u64,
        point: PdfPoint,
    ) -> Result<SnapGestureCommit, NativeEditingV5Error> {
        let observation = self.native_v5_snap_observation.take().ok_or_else(|| {
            NativeEditingV5Error::GestureInvariant("snap gesture is missing".into())
        })?;
        if observation.document_id != document_id {
            return Err(NativeEditingV5Error::GestureInvariant(
                "snap gesture belongs to another document".into(),
            ));
        }
        let Some(resolution) = observation.latest else {
            self.cancel(PointerCancelReason::AdapterError)?;
            return Err(NativeEditingV5Error::GestureInvariant(
                "snap gesture has no pointer samples".into(),
            ));
        };
        self.pointer_up(observation.pointer_id, point)?;
        let history_after = self.history_depths(document_id);
        if history_after != (observation.history_before.0 + 1, 0) {
            return Err(NativeEditingV5Error::HistoryInvariant(
                "ordinary snap gesture did not commit exactly once".into(),
            ));
        }
        let final_rectangle = self
            .document_scene(document_id, observation.plan.page_index)
            .rectangles
            .into_iter()
            .find(|rectangle| rectangle.id == observation.plan.target_id)
            .ok_or_else(|| {
                NativeEditingV5Error::TargetNotFound(observation.plan.target_id.clone())
            })?;
        if final_rectangle.rect != observation.plan.expected_final_rect
            || final_rectangle.appearance != observation.appearance
        {
            return Err(NativeEditingV5Error::TargetChanged(
                observation.plan.target_id.clone(),
            ));
        }
        let receipt = SnapGestureCommit {
            target_id: observation.plan.target_id,
            original_rect: observation.plan.setup_rect,
            final_rect: final_rectangle.rect,
            appearance: observation.appearance,
            resolution,
            sample_count: observation.sample_count,
            sensitivity_css_px: observation.plan.sensitivity_css_px,
            observed_pixels_per_point: observation.observed_pixels_per_point,
            derived_threshold_pt: observation.plan.sensitivity_css_px
                / observation.observed_pixels_per_point,
            history_before: observation.history_before,
            history_after,
        };
        self.native_v5_snap_receipt = Some((document_id, receipt.clone()));
        Ok(receipt)
    }

    pub fn undo_native_v5_snap(
        &mut self,
        document_id: u64,
    ) -> Result<SnapGestureCommit, NativeEditingV5Error> {
        let (receipt_document_id, receipt) =
            self.native_v5_snap_receipt.as_ref().ok_or_else(|| {
                NativeEditingV5Error::HistoryInvariant("snap receipt is missing".into())
            })?;
        if *receipt_document_id != document_id {
            return Err(NativeEditingV5Error::HistoryInvariant(
                "snap receipt belongs to another document".into(),
            ));
        }
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or_else(|| NativeEditingV5Error::TargetNotFound(receipt.target_id.clone()))?;
        if !document.undo()? {
            return Err(NativeEditingV5Error::HistoryInvariant(
                "snap undo did not apply".into(),
            ));
        }
        receipt.verify_undone(document)?;
        Ok(receipt.clone())
    }

    pub fn redo_native_v5_snap(
        &mut self,
        document_id: u64,
    ) -> Result<SnapGestureCommit, NativeEditingV5Error> {
        let (receipt_document_id, receipt) =
            self.native_v5_snap_receipt.as_ref().ok_or_else(|| {
                NativeEditingV5Error::HistoryInvariant("snap receipt is missing".into())
            })?;
        if *receipt_document_id != document_id {
            return Err(NativeEditingV5Error::HistoryInvariant(
                "snap receipt belongs to another document".into(),
            ));
        }
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or_else(|| NativeEditingV5Error::TargetNotFound(receipt.target_id.clone()))?;
        if !document.redo()? {
            return Err(NativeEditingV5Error::HistoryInvariant(
                "snap redo did not apply".into(),
            ));
        }
        receipt.verify_redone(document)?;
        Ok(receipt.clone())
    }

    pub fn highlight_paint_capability(&self) -> HighlightPaintCapability {
        HighlightPaintCapability::SourceAlphaFallback
    }

    pub fn tool_properties(&self, tool: AnnotationTool) -> ToolProperties {
        self.tool_properties
            .get(&tool)
            .cloned()
            .unwrap_or_else(|| ToolProperties::for_tool(tool))
    }

    pub fn set_tool_properties(
        &mut self,
        tool: AnnotationTool,
        properties: ToolProperties,
    ) -> Result<(), AnnotationError> {
        let properties = properties.canonicalized_for_tool(tool)?;
        if self.tool_properties(tool) != properties {
            self.cancel(PointerCancelReason::ToolChanged)?;
            self.tool_properties.insert(tool, properties);
        }
        Ok(())
    }

    pub fn highlight_appearance(&self) -> PenAppearance {
        let properties = self.tool_properties(AnnotationTool::Highlight);
        PenAppearance::new(properties.colour, properties.width_pt, properties.opacity)
            .expect("stored Highlight tool properties are validated")
    }

    pub fn set_highlight_appearance(
        &mut self,
        appearance: PenAppearance,
    ) -> Result<(), AnnotationError> {
        let mut properties = self.tool_properties(AnnotationTool::Highlight);
        properties.colour = appearance.color().to_owned();
        properties.width_pt = appearance.width_pt();
        properties.opacity = appearance.opacity();
        self.set_tool_properties(AnnotationTool::Highlight, properties)
    }

    pub fn tool(&self) -> AnnotationTool {
        self.tool
    }

    pub fn set_rectangle_snap_settings(
        &mut self,
        settings: RectangleSnapSettings,
    ) -> Result<(), AnnotationError> {
        if self.rectangle_snap_settings != settings {
            self.cancel(PointerCancelReason::ToolChanged)?;
            self.rectangle_snap_settings = settings;
        }
        Ok(())
    }

    pub fn rectangle_snap_settings(&self) -> RectangleSnapSettings {
        self.rectangle_snap_settings
    }

    pub fn set_semantic_snap_settings(
        &mut self,
        settings: SemanticSnapSettings,
    ) -> Result<(), AnnotationError> {
        self.semantic_snap_settings = settings;
        self.semantic_snap_decision = None;
        Ok(())
    }

    pub fn semantic_snap_settings(&self) -> SemanticSnapSettings {
        self.semantic_snap_settings
    }

    pub fn semantic_snap_decision(&self) -> Option<&SemanticSnapDecision> {
        self.semantic_snap_decision.as_ref()
    }

    pub fn clear_semantic_snap_decision(&mut self) {
        self.semantic_snap_decision = None;
    }

    pub fn set_semantic_snap_page_size(
        &mut self,
        document_id: u64,
        page_index: u32,
        width_pdf_points: f64,
        height_pdf_points: f64,
    ) {
        self.semantic_snap_page_sizes.insert(
            (document_id, page_index),
            (width_pdf_points, height_pdf_points),
        );
    }

    fn resolve_semantic_creation_point(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> PdfPoint {
        if !matches!(
            self.tool,
            AnnotationTool::Line
                | AnnotationTool::Arrow
                | AnnotationTool::Length
                | AnnotationTool::Dimension
        ) {
            self.semantic_snap_decision = None;
            return point;
        }
        let (excluded_ids, creation_anchor) = match self.active.as_ref() {
            Some(ActivePointer::StraightLineCreate {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            })
            | Some(ActivePointer::LengthCreate {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            })
            | Some(ActivePointer::DimensionCreate {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                (vec![id.clone()], Some(*start))
            }
            _ => (Vec::new(), None),
        };
        let scene = self.document_scene(document_id, page_index);
        let annotation_decision = SemanticSnapIndex::from_annotation_scene(&scene, &excluded_ids)
            .resolve_point_with_orthogonal_anchor(
                point,
                &self.semantic_snap_settings,
                self.observed_pixels_per_point.0,
                constrain_orthogonal.then_some(creation_anchor).flatten(),
            );
        let construction_grid_decision = self
            .semantic_snap_page_sizes
            .get(&(document_id, page_index))
            .and_then(|(width, height)| {
                resolve_construction_grid_point(
                    point,
                    *width,
                    *height,
                    &self.semantic_snap_settings,
                    self.observed_pixels_per_point.0,
                )
            });
        let decision = match (annotation_decision, construction_grid_decision) {
            (Some(annotation), Some(grid)) => Some(
                if annotation.distance_window_px <= grid.distance_window_px {
                    annotation
                } else {
                    grid
                },
            ),
            (annotation @ Some(_), None) => annotation,
            (None, grid) => grid,
        };
        let mut resolved = decision.as_ref().map_or(point, |decision| decision.point);
        if decision.is_none()
            && self.tool == AnnotationTool::Dimension
            && self.semantic_snap_settings.dimension_increment_enabled()
            && let Some(anchor) = creation_anchor
        {
            let delta_x = resolved.x - anchor.x;
            let delta_y = resolved.y - anchor.y;
            let distance = delta_x.hypot(delta_y);
            if distance > 0.
                && let Ok(quantized) = quantize_pdf_distance_to_mm_increment(
                    distance,
                    self.semantic_snap_settings.dimension_increment_mm(),
                )
            {
                let scale = quantized / distance;
                resolved = PdfPoint {
                    x: anchor.x + delta_x * scale,
                    y: anchor.y + delta_y * scale,
                };
            }
        }
        self.semantic_snap_decision = decision;
        resolved
    }

    pub fn set_observed_pixels_per_point(
        &mut self,
        observed_pixels_per_point: f64,
    ) -> Result<(), AnnotationError> {
        if !observed_pixels_per_point.is_finite() || observed_pixels_per_point <= 0.0 {
            return Err(AnnotationError::InvalidGeometry(
                "observed pixels per PDF point must be finite and positive".into(),
            ));
        }
        let observed_pixels_per_point = ObservedPixelsPerPoint(observed_pixels_per_point);
        if self.observed_pixels_per_point != observed_pixels_per_point {
            self.cancel(PointerCancelReason::ToolChanged)?;
            self.observed_pixels_per_point = observed_pixels_per_point;
        }
        Ok(())
    }

    pub fn set_tool(&mut self, tool: AnnotationTool) -> Result<(), AnnotationError> {
        if self.tool != tool {
            self.cancel(PointerCancelReason::ToolChanged)?;
            if self.tool == AnnotationTool::Image && tool != AnnotationTool::Image {
                self.image_asset = None;
                self.image_placement_page = None;
            }
            self.tool = tool;
        }
        Ok(())
    }

    pub fn set_image_asset(&mut self, asset: DecodedRgbaAsset) {
        self.image_asset = Some(PendingImageAsset {
            asset,
            aspect_locked: false,
        });
    }

    pub fn set_signature_asset(&mut self, asset: DecodedRgbaAsset) {
        self.image_asset = Some(PendingImageAsset {
            asset,
            aspect_locked: true,
        });
    }

    /// Supplies the synchronous page capture used by the pending Snapshot's
    /// second click. The capture belongs to the current pending rectangle and
    /// is consumed only after a successful commit.
    pub fn set_snapshot_capture_asset(&mut self, asset: DecodedRgbaAsset) {
        self.snapshot_capture_asset = Some(asset);
    }

    pub fn snapshot_placement_pending(&self, document_id: u64) -> bool {
        self.snapshot_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
    }

    pub fn snapshot_pending_rect(&self, document_id: u64, page_index: u32) -> Option<PdfRect> {
        self.snapshot_draft.as_ref().and_then(|draft| {
            ((draft.document_id, draft.page_index) == (document_id, page_index))
                .then(|| PdfRect::from_corners(draft.start, draft.current))
        })
    }

    pub fn snapshot_pending_rect_to(
        &self,
        document_id: u64,
        page_index: u32,
        second_click: PdfPoint,
    ) -> Option<PdfRect> {
        self.snapshot_draft.as_ref().and_then(|draft| {
            ((draft.document_id, draft.page_index) == (document_id, page_index))
                .then(|| PdfRect::from_corners(draft.start, second_click))
        })
    }

    /// Sets the page-local placement boundary used by the image tool.
    ///
    /// The decoded image keeps its natural aspect ratio and is never enlarged.
    /// Either dimension is reduced when it exceeds this fraction of the page.
    pub fn set_image_placement_page(
        &mut self,
        width_pt: f64,
        height_pt: f64,
        max_fraction: f64,
    ) -> Result<(), AnnotationError> {
        if !width_pt.is_finite()
            || !height_pt.is_finite()
            || !max_fraction.is_finite()
            || width_pt <= 0.0
            || height_pt <= 0.0
            || max_fraction <= 0.0
            || max_fraction > 1.0
        {
            return Err(AnnotationError::InvalidGeometry(
                "image placement page and maximum fraction must be finite and positive; the fraction must not exceed one"
                    .into(),
            ));
        }
        self.image_placement_page = Some(ImagePlacementPage {
            width_pt,
            height_pt,
            max_fraction,
        });
        Ok(())
    }

    /// Supplies the next deterministic ID for manifest-backed comparison replay.
    pub fn queue_next_annotation_id(&mut self, id: MarkupId) {
        self.queued_id = Some(id);
    }

    /// Supplies the initial appearance for the next rectangle placement.
    pub fn queue_next_rectangle_appearance(&mut self, appearance: RectangleAppearance) {
        self.queued_rectangle_appearance = Some(appearance);
    }

    /// Supplies the initial content for the next text placement. Native input
    /// replay uses a one-character seed that the first delivered key replaces,
    /// so the frozen command text cannot be present before keyboard delivery.
    pub fn queue_next_text_content(&mut self, content: impl Into<String>) {
        self.queued_text_content = Some(content.into());
    }

    pub fn image_asset(&self) -> Option<&DecodedRgbaAsset> {
        self.image_asset.as_ref().map(|pending| &pending.asset)
    }

    pub fn set_length_calibration(
        &mut self,
        calibration: LengthCalibration,
    ) -> Result<(), AnnotationError> {
        self.set_page_length_calibration(0, calibration)
    }

    pub fn set_page_length_calibration(
        &mut self,
        page_index: u32,
        calibration: LengthCalibration,
    ) -> Result<(), AnnotationError> {
        self.set_document_page_length_calibration(0, page_index, calibration)
    }

    pub fn set_document_page_length_calibration(
        &mut self,
        document_id: u64,
        page_index: u32,
        calibration: LengthCalibration,
    ) -> Result<(), AnnotationError> {
        self.cancel(PointerCancelReason::ToolChanged)?;
        self.documents
            .entry(document_id)
            .or_default()
            .set_page_length_calibration(page_index, calibration)?;
        Ok(())
    }

    pub fn apply_document_page_scale(
        &mut self,
        document_id: u64,
        scale: PageScale,
        target: crate::annotation_model::PageScaleApplyTarget,
        page_count: u32,
    ) -> Result<bool, AnnotationError> {
        self.cancel(PointerCancelReason::ToolChanged)?;
        self.documents
            .entry(document_id)
            .or_default()
            .apply_page_scale(scale, target, page_count)
    }

    pub fn apply_document_page_scale_with_preset(
        &mut self,
        document_id: u64,
        scale: PageScale,
        target: crate::annotation_model::PageScaleApplyTarget,
        page_count: u32,
        saved_preset: Option<ScalePreset>,
    ) -> Result<bool, AnnotationError> {
        self.cancel(PointerCancelReason::ToolChanged)?;
        self.documents
            .entry(document_id)
            .or_default()
            .apply_page_scale_with_preset(scale, target, page_count, saved_preset)
    }

    pub fn delete_document_scale_preset(
        &mut self,
        document_id: u64,
        preset_id: &str,
    ) -> Result<bool, AnnotationError> {
        self.cancel(PointerCancelReason::ToolChanged)?;
        self.documents
            .entry(document_id)
            .or_default()
            .delete_scale_preset(preset_id)
    }

    pub fn length_calibration(&self) -> Option<&LengthCalibration> {
        self.page_length_calibration(0)
    }

    pub fn page_length_calibration(&self, page_index: u32) -> Option<&LengthCalibration> {
        self.document_page_length_calibration(0, page_index)
    }

    pub fn document_page_length_calibration(
        &self,
        document_id: u64,
        page_index: u32,
    ) -> Option<&LengthCalibration> {
        self.documents
            .get(&document_id)
            .and_then(|document| document.page_length_calibration(page_index))
    }

    pub fn document_page_scale(&self, document_id: u64, page_index: u32) -> Option<&PageScale> {
        self.documents
            .get(&document_id)
            .and_then(|document| document.page_scale(page_index))
    }

    pub fn document_scale_presets(&self, document_id: u64) -> Option<&[ScalePreset]> {
        self.documents
            .get(&document_id)
            .map(AnnotationDocument::scale_presets)
    }

    pub fn begin_length_placement(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        self.cancel(PointerCancelReason::AdapterError)?;
        if self
            .document_page_length_calibration(document_id, page_index)
            .is_none()
        {
            return Err(AnnotationError::InvalidGeometry(
                LENGTH_SCALE_REQUIRED_MESSAGE.into(),
            ));
        }
        let start = self.resolve_semantic_creation_point(document_id, page_index, start, false);
        self.documents
            .entry(document_id)
            .or_default()
            .clear_selection();
        self.active = Some(ActivePointer::LengthCreate {
            document_id,
            page_index,
            pointer_id: 0,
            id,
            start,
            current: start,
        });
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn begin_dimension_placement(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        self.cancel(PointerCancelReason::AdapterError)?;
        self.documents
            .entry(document_id)
            .or_default()
            .clear_selection();
        self.active = Some(ActivePointer::DimensionCreate {
            document_id,
            page_index,
            pointer_id: 0,
            id,
            start,
            current: start,
        });
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn length_placement_pending(&self, document_id: u64) -> bool {
        matches!(
            self.active,
            Some(ActivePointer::LengthCreate {
                document_id: active_document_id,
                ..
            }) if active_document_id == document_id
        )
    }

    pub fn dimension_placement_pending(&self, document_id: u64) -> bool {
        matches!(
            self.active,
            Some(ActivePointer::DimensionCreate {
                document_id: active_document_id,
                ..
            }) if active_document_id == document_id
        )
    }

    pub fn arc_placement_pending(&self, document_id: u64) -> bool {
        self.arc_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
    }

    pub fn update_arc_hover(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        snap_quarter_turn: bool,
    ) -> Result<(), AnnotationError> {
        let draft = self
            .arc_draft
            .as_mut()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if (draft.document_id, draft.page_index) != (document_id, page_index) {
            return Err(AnnotationError::NoActiveGesture);
        }
        if let Some(end) = draft.end {
            draft.mid = ArcAnnotation::constrained_midpoint(
                draft.start,
                end,
                point,
                ARC_MINIMUM_BULGE_CSS_PX / self.observed_pixels_per_point.0,
                snap_quarter_turn,
            )?;
        }
        Ok(())
    }

    pub fn vertex_path_pending(&self, document_id: u64) -> bool {
        self.vertex_path_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
    }

    pub fn finish_vertex_path(
        &mut self,
        document_id: u64,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .vertex_path_draft
            .take()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if draft.document_id != document_id {
            self.vertex_path_draft = Some(draft);
            return Err(AnnotationError::NoActiveGesture);
        }
        if draft.points.len() < draft.kind.minimum_points() {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let id = draft.id.clone();
        let tool = match draft.kind {
            VertexPathKind::Polyline => AnnotationTool::Polyline,
            VertexPathKind::Polygon => AnnotationTool::Polygon,
        };
        let properties = self.tool_properties(tool);
        let appearance =
            rectangle_tool_appearance(&properties, draft.kind == VertexPathKind::Polygon)?;
        self.documents
            .entry(document_id)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::VertexPath(
                VertexPathAnnotation::new(
                    draft.id,
                    draft.page_index,
                    draft.points,
                    draft.kind,
                    appearance,
                )?,
            )))?;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    pub fn cloud_pending(&self, document_id: u64) -> bool {
        self.cloud_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
    }

    pub fn finish_cloud(
        &mut self,
        document_id: u64,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .cloud_draft
            .take()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if draft.document_id != document_id {
            self.cloud_draft = Some(draft);
            return Err(AnnotationError::NoActiveGesture);
        }
        if draft.points.len() < 3 {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let id = draft.id.clone();
        let properties = self.tool_properties(AnnotationTool::Cloud);
        self.documents
            .entry(document_id)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Cloud(
                CloudAnnotation::new(
                    draft.id,
                    draft.page_index,
                    draft.points,
                    properties.cloud_intensity,
                    rectangle_tool_appearance(&properties, false)?,
                )?,
            )))?;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    pub fn update_cloud_hover(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .cloud_draft
            .as_mut()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if (draft.document_id, draft.page_index) != (document_id, page_index) {
            return Err(AnnotationError::NoActiveGesture);
        }
        draft.hover = point;
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn set_selected_cloud_point(
        &mut self,
        document_id: u64,
        vertex_index: usize,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetCloudPoint {
                vertex_index,
                point,
            },
        })?;
        Ok(())
    }

    pub fn translate_selected_cloud(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateCloud { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn cloud_plus_pending(&self, document_id: u64) -> bool {
        self.cloud_plus_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
    }

    pub fn finish_cloud_plus(
        &mut self,
        document_id: u64,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .cloud_plus_draft
            .take()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if draft.document_id != document_id {
            self.cloud_plus_draft = Some(draft);
            return Err(AnnotationError::NoActiveGesture);
        }
        if draft.points.len() < 3 {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        self.commit_cloud_plus(document_id, draft.page_index, draft.id, draft.points)
    }

    pub fn update_cloud_plus_hover(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .cloud_plus_draft
            .as_mut()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if (draft.document_id, draft.page_index) != (document_id, page_index) {
            return Err(AnnotationError::NoActiveGesture);
        }
        draft.hover = point;
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn set_selected_cloud_plus_cloud_point(
        &mut self,
        document_id: u64,
        vertex_index: usize,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let annotation = document
            .cloud_pluses()
            .iter()
            .find(|annotation| annotation.id == id)
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if vertex_index >= annotation.cloud_points().len() {
            return Err(AnnotationError::InvalidGeometry(
                "Cloud+ point index is out of range".into(),
            ));
        }
        let mut cloud_points = annotation.cloud_points().to_vec();
        cloud_points[vertex_index] = point;
        let visible_path = cloud_visible_path(&cloud_points, annotation.border_effect_intensity())?;
        let leader = route_cloud_plus_leader(
            &cloud_points,
            &visible_path,
            annotation.text_box,
            annotation.leader_points(),
            &CloudPlusRoutingContext::default(),
        )?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetCloudPlusCloudPoint {
                vertex_index,
                point,
                leader_points: leader.points,
            },
        })?;
        Ok(())
    }

    pub fn translate_selected_cloud_plus_text_box(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let annotation = document
            .cloud_pluses()
            .iter()
            .find(|annotation| annotation.id == id)
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let text_box = PdfRect::new(
            annotation.text_box.x + delta_x,
            annotation.text_box.y + delta_y,
            annotation.text_box.width,
            annotation.text_box.height,
        )?;
        let leader = route_cloud_plus_leader(
            annotation.cloud_points(),
            &annotation.scallop_path(),
            text_box,
            annotation.leader_points(),
            &CloudPlusRoutingContext::default(),
        )?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetCloudPlusTextBox {
                text_box,
                leader_points: leader.points,
            },
        })?;
        Ok(())
    }

    pub fn translate_selected_cloud_plus_group(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateCloudPlusGroup { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn set_selected_callout_leader_point(
        &mut self,
        document_id: u64,
        point_index: usize,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetCalloutLeaderPoint { point_index, point },
        })?;
        Ok(())
    }

    pub fn translate_selected_callout_text_box(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateCalloutTextBox { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn translate_selected_callout_group(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateCalloutGroup { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn measurement_path_pending(&self, document_id: u64) -> bool {
        self.measurement_path_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
    }

    pub fn finish_measurement_path(
        &mut self,
        document_id: u64,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .measurement_path_draft
            .take()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if draft.document_id != document_id {
            self.measurement_path_draft = Some(draft);
            return Err(AnnotationError::NoActiveGesture);
        }
        if draft.points.len() < draft.kind.minimum_points() {
            self.measurement_path_draft = Some(draft);
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let id = draft.id.clone();
        let tool = match draft.kind {
            MeasurementPathKind::Polylength => AnnotationTool::Polylength,
            MeasurementPathKind::Area => AnnotationTool::Area,
        };
        let properties = self.tool_properties(tool);
        self.documents
            .entry(document_id)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::MeasurementPath(MeasurementPathAnnotation::new_with_text_style(
                    draft.id,
                    draft.page_index,
                    draft.points,
                    draft.kind,
                    draft.calibration,
                    rectangle_tool_appearance(&properties, false)?,
                    text_box_tool_style(&properties)?,
                )?),
            ))?;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    pub fn cancel_measurement_path(
        &mut self,
        document_id: u64,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .measurement_path_draft
            .take()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if draft.document_id != document_id {
            self.measurement_path_draft = Some(draft);
            return Err(AnnotationError::NoActiveGesture);
        }
        Ok(PointerPhaseOutcome::Ignored)
    }

    pub fn update_measurement_path_hover(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .measurement_path_draft
            .as_mut()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if (draft.document_id, draft.page_index) != (document_id, page_index) {
            return Err(AnnotationError::NoActiveGesture);
        }
        draft.hover = point;
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn update_vertex_path_hover(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let draft = self
            .vertex_path_draft
            .as_mut()
            .ok_or(AnnotationError::NoActiveGesture)?;
        if (draft.document_id, draft.page_index) != (document_id, page_index) {
            return Err(AnnotationError::NoActiveGesture);
        }
        draft.hover = point;
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn set_selected_vertex_path_point(
        &mut self,
        document_id: u64,
        vertex_index: usize,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetVertexPathPoint {
                vertex_index,
                point,
            },
        })?;
        Ok(())
    }

    pub fn move_selected_vertex_path(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateVertexPath { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn set_selected_arc_control_point(
        &mut self,
        document_id: u64,
        control: ArcControlPoint,
        point: PdfPoint,
        snap_quarter_turn: bool,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let annotation = document
            .arcs()
            .iter()
            .find(|annotation| annotation.id == id)
            .ok_or(AnnotationError::NoSelection)?;
        let resolved = resolve_arc_control_point(
            annotation,
            control,
            point,
            ARC_MINIMUM_BULGE_CSS_PX / self.observed_pixels_per_point.0,
            snap_quarter_turn,
        )?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetArcControlPoint {
                control,
                point: resolved,
                snap_quarter_turn,
            },
        })?;
        Ok(())
    }

    pub fn set_selected_measurement_path_point(
        &mut self,
        document_id: u64,
        vertex_index: usize,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetMeasurementPathPoint {
                vertex_index,
                point,
            },
        })?;
        Ok(())
    }

    pub fn move_selected_measurement_path(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateMeasurementPath { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn update_length_placement(
        &mut self,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let (document_id, page_index) = match self.active.as_ref() {
            Some(ActivePointer::LengthCreate {
                document_id,
                page_index,
                ..
            }) => (*document_id, *page_index),
            _ => return Err(AnnotationError::NoActiveGesture),
        };
        let point = self.resolve_semantic_creation_point(
            document_id,
            page_index,
            point,
            constrain_orthogonal,
        );
        let Some(ActivePointer::LengthCreate { start, current, .. }) = self.active.as_mut() else {
            return Err(AnnotationError::NoActiveGesture);
        };
        *current = constrained_length_point(*start, point, constrain_orthogonal);
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn update_dimension_placement(
        &mut self,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let (document_id, page_index) = match self.active.as_ref() {
            Some(ActivePointer::DimensionCreate {
                document_id,
                page_index,
                ..
            }) => (*document_id, *page_index),
            _ => return Err(AnnotationError::NoActiveGesture),
        };
        let point = self.resolve_semantic_creation_point(
            document_id,
            page_index,
            point,
            constrain_orthogonal,
        );
        let Some(ActivePointer::DimensionCreate { start, current, .. }) = self.active.as_mut()
        else {
            return Err(AnnotationError::NoActiveGesture);
        };
        *current = constrained_line_point(*start, point, constrain_orthogonal);
        Ok(PointerPhaseOutcome::PlacementPending)
    }

    pub fn commit_dimension_placement(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let point = self.resolve_semantic_creation_point(
            document_id,
            page_index,
            point,
            constrain_orthogonal,
        );
        let active = self.active.take().ok_or(AnnotationError::NoActiveGesture)?;
        let ActivePointer::DimensionCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            ..
        } = active
        else {
            self.active = Some(active);
            return Err(AnnotationError::NoActiveGesture);
        };
        if (active_document_id, active_page_index) != (document_id, page_index) {
            return Err(AnnotationError::NoActiveGesture);
        }
        let end = constrained_line_point(start, point, constrain_orthogonal);
        if (end.x - start.x).hypot(end.y - start.y) <= LENGTH_MINIMUM_PDF_DISTANCE + 0.001 {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let properties = self.tool_properties(AnnotationTool::Dimension);
        let annotation = DimensionAnnotation::new(
            id.clone(),
            page_index,
            start,
            end,
            DimensionAnnotation::default_offset(start, end),
            "Dimension",
            dimension_tool_appearance(&properties)?,
        )?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoActiveGesture)?
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Dimension(
                annotation,
            )))?;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    pub fn replace_dimension_content_in_create_transaction(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .replace_dimension_content_in_create_transaction(id, content)?;
        Ok(())
    }

    pub fn replace_selected_dimension_content(
        &mut self,
        document_id: u64,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetDimensionContent(content.into()),
        })?;
        Ok(())
    }

    pub fn set_exact_selected_dimension_appearance(
        &mut self,
        document_id: u64,
        appearance: DimensionAppearance,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::Dimension(annotation)] = selected.as_slice() else {
            return Err(AnnotationError::NoSelection);
        };
        document.apply_command(AnnotationCommand::EditAnnotation {
            id: annotation.id.clone(),
            edit: AnnotationEdit::SetDimensionAppearance(appearance),
        })?;
        Ok(())
    }

    pub fn edit_selected_dimension_endpoint(
        &mut self,
        document_id: u64,
        endpoint: LineEndpoint,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetDimensionEndpoint { endpoint, point },
        })?;
        Ok(())
    }

    pub fn set_selected_dimension_offset(
        &mut self,
        document_id: u64,
        offset: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetDimensionOffset(offset),
        })?;
        Ok(())
    }

    pub fn move_selected_dimension(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateDimension { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn commit_length_placement(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let point = self.resolve_semantic_creation_point(
            document_id,
            page_index,
            point,
            constrain_orthogonal,
        );
        let active = self.active.take().ok_or(AnnotationError::NoActiveGesture)?;
        let ActivePointer::LengthCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            ..
        } = active
        else {
            self.active = Some(active);
            return Err(AnnotationError::NoActiveGesture);
        };
        if (active_document_id, active_page_index) != (document_id, page_index) {
            return Err(AnnotationError::NoActiveGesture);
        }
        let end = constrained_length_point(start, point, constrain_orthogonal);
        // GPUI converts through f32 pixel bounds before returning PDF points.
        // Keep the exact two-point product threshold stable across layout sizes.
        if ((end.x - start.x).powi(2) + (end.y - start.y).powi(2)).sqrt()
            <= LENGTH_MINIMUM_PDF_DISTANCE + 0.001
        {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let calibration = self
            .document_page_length_calibration(document_id, page_index)
            .cloned()
            .ok_or_else(|| {
                AnnotationError::InvalidGeometry(LENGTH_SCALE_REQUIRED_MESSAGE.into())
            })?;
        let properties = self.tool_properties(AnnotationTool::Length);
        let annotation = LengthAnnotation::new_with_appearance(
            id.clone(),
            page_index,
            start,
            end,
            calibration,
            dimension_tool_appearance(&properties)?,
        )?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoActiveGesture)?
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
                annotation,
            )))?;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    pub fn active_surface(&self) -> Option<(u64, u32)> {
        if let Some(draft) = &self.measurement_path_draft {
            return Some((draft.document_id, draft.page_index));
        }
        if let Some(draft) = &self.vertex_path_draft {
            return Some((draft.document_id, draft.page_index));
        }
        if let Some(draft) = &self.cloud_plus_draft {
            return Some((draft.document_id, draft.page_index));
        }
        if let Some(draft) = &self.snapshot_draft {
            return Some((draft.document_id, draft.page_index));
        }
        match self.active.as_ref()? {
            ActivePointer::Marquee {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::GroupMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::Domain {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::EllipseCreate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::CloudPlusCreate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::EllipseMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::EllipseResize {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::EllipseRotate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::RedactCreate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::RedactMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::RedactResize {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::ArcMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::ArcControlPoint {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::StraightLineCreate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::CalloutCreate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::StraightLineMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::StraightLineEndpoint {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::VertexPathPoint {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::MeasurementPathPoint {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::LengthCreate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::DimensionCreate {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::DimensionEdit {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::CalloutEdit {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::CloudEdit {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::InkMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::TextBoxMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::TextBoxResize {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::ImageMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::ImageResize {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::SnapshotMove {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::SnapshotResize {
                document_id,
                page_index,
                ..
            }
            | ActivePointer::SnapshotRotate {
                document_id,
                page_index,
                ..
            } => Some((*document_id, *page_index)),
            ActivePointer::LengthEndpoint { document_id, .. } => {
                let document = self.documents.get(document_id)?;
                let page_index = document
                    .selected_id()
                    .and_then(|id| document.lengths().iter().find(|length| &length.id == id))
                    .map(|length| length.page_index)?;
                Some((*document_id, page_index))
            }
        }
    }

    pub fn active_selection_marquee(&self, document_id: u64) -> Option<(u32, SelectionMarquee)> {
        match self.active.as_ref()? {
            ActivePointer::Marquee {
                document_id: active_document_id,
                page_index,
                marquee,
                ..
            } if *active_document_id == document_id => Some((*page_index, marquee.clone())),
            _ => None,
        }
    }

    pub fn is_click_placement_pending(&self) -> bool {
        matches!(
            self.active,
            Some(ActivePointer::Marquee {
                marquee: SelectionMarquee {
                    shape: SelectionShape::Box,
                    ..
                },
                ..
            }) | Some(ActivePointer::Domain {
                click_placement_pending: true,
                ..
            }) | Some(ActivePointer::EllipseCreate {
                click_placement_pending: true,
                ..
            }) | Some(ActivePointer::RedactCreate {
                click_placement_pending: true,
                ..
            }) | Some(ActivePointer::StraightLineCreate {
                click_placement_pending: true,
                ..
            }) | Some(ActivePointer::CalloutCreate {
                click_placement_pending: true,
                ..
            })
        )
    }

    pub fn remove_document(&mut self, document_id: u64) {
        if self
            .active_surface()
            .is_some_and(|(active_document_id, _)| active_document_id == document_id)
        {
            let _ = self.cancel(PointerCancelReason::PageChanged);
        }
        if self
            .arc_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
        {
            self.arc_draft = None;
        }
        if self
            .snapshot_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
        {
            self.snapshot_draft = None;
            self.snapshot_capture_asset = None;
        }
        self.documents.remove(&document_id);
    }

    pub fn has_selection(&self, document_id: u64) -> bool {
        self.documents
            .get(&document_id)
            .and_then(AnnotationDocument::selected_id)
            .is_some()
    }

    pub fn selected_is_locked(&self, document_id: u64) -> bool {
        self.documents
            .get(&document_id)
            .is_some_and(AnnotationDocument::selected_is_locked)
    }

    pub fn selected_kind(&self, document_id: u64) -> Option<AnnotationKind> {
        let document = self.documents.get(&document_id)?;
        let id = document.selected_id()?;
        if document
            .rectangles()
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            Some(AnnotationKind::Rectangle)
        } else if let Some(annotation) = document
            .straight_lines()
            .iter()
            .find(|annotation| &annotation.id == id)
        {
            Some(match annotation.kind {
                crate::annotation_model::LineKind::Line => AnnotationKind::Line,
                crate::annotation_model::LineKind::Arrow => AnnotationKind::Arrow,
            })
        } else if let Some(annotation) = document
            .vertex_paths()
            .iter()
            .find(|annotation| &annotation.id == id)
        {
            Some(annotation.kind.into())
        } else if document
            .clouds()
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            Some(AnnotationKind::Cloud)
        } else if document
            .callouts()
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            Some(AnnotationKind::Callout)
        } else if let Some(annotation) = document
            .measurement_paths()
            .iter()
            .find(|annotation| &annotation.id == id)
        {
            Some(annotation.kind.into())
        } else if document
            .pens()
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            Some(AnnotationKind::Pen)
        } else if document
            .text_boxes()
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            Some(AnnotationKind::TextBox)
        } else if document
            .lengths()
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            Some(AnnotationKind::Length)
        } else if document
            .images()
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            Some(AnnotationKind::Image)
        } else {
            None
        }
    }

    pub fn selected_rectangle_appearance(&self, document_id: u64) -> Option<&RectangleAppearance> {
        let document = self.documents.get(&document_id)?;
        let id = document.selected_id()?;
        document
            .rectangles()
            .iter()
            .find(|annotation| &annotation.id == id)
            .map(|annotation| &annotation.appearance)
    }

    pub fn selected_rectangle(&self, document_id: u64) -> Option<&RectangleAnnotation> {
        let document = self.documents.get(&document_id)?;
        let id = document.selected_id()?;
        document
            .rectangles()
            .iter()
            .find(|annotation| &annotation.id == id)
    }

    pub fn hit_rectangle_id(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let Some(target) = document.hit_test(page_index, point, tolerance_pt)? else {
            return Ok(None);
        };
        let id = target.markup_id();
        Ok(document
            .rectangles()
            .iter()
            .any(|annotation| &annotation.id == id)
            .then(|| id.clone()))
    }

    pub fn selected_ellipse_appearance(&self, document_id: u64) -> Option<&RectangleAppearance> {
        let document = self.documents.get(&document_id)?;
        let id = document.selected_id()?;
        document
            .ellipses()
            .iter()
            .find(|annotation| &annotation.id == id)
            .map(|annotation| &annotation.appearance)
    }

    pub fn selected_pen_appearance(&self, document_id: u64) -> Option<&PenAppearance> {
        let document = self.documents.get(&document_id)?;
        let id = document.selected_id()?;
        document
            .pens()
            .iter()
            .find(|annotation| &annotation.id == id)
            .map(|annotation| &annotation.appearance)
    }

    /// Returns Ink only when the current selection contains exactly one Pen
    /// or Highlight. Property inspectors must not silently target the first
    /// item in a mixed or multi-selection.
    pub fn exact_selected_ink(&self, document_id: u64) -> Option<&PenAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::Pen(selected)] = selected.as_slice() else {
            return None;
        };
        document.pens().iter().find(|pen| pen.id == selected.id)
    }

    /// Returns a Text Box only when the current selection contains exactly one.
    pub fn exact_selected_text_box(&self, document_id: u64) -> Option<&TextBoxAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::TextBox(selected)] = selected.as_slice() else {
            return None;
        };
        document
            .text_boxes()
            .iter()
            .find(|text_box| text_box.id == selected.id)
    }

    /// Returns a Dimension only when the current selection contains exactly one.
    pub fn exact_selected_dimension(&self, document_id: u64) -> Option<&DimensionAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::Dimension(selected)] = selected.as_slice() else {
            return None;
        };
        document
            .dimensions()
            .iter()
            .find(|dimension| dimension.id == selected.id)
    }

    /// Returns an Arc only when the current selection contains exactly one.
    pub fn exact_selected_arc(&self, document_id: u64) -> Option<&ArcAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::Arc(selected)] = selected.as_slice() else {
            return None;
        };
        document.arcs().iter().find(|arc| arc.id == selected.id)
    }

    /// Returns a Cloud only when the current selection contains exactly one.
    pub fn exact_selected_cloud(&self, document_id: u64) -> Option<&CloudAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::Cloud(selected)] = selected.as_slice() else {
            return None;
        };
        document
            .clouds()
            .iter()
            .find(|cloud| cloud.id == selected.id)
    }

    /// Returns a Snapshot only when the current selection contains exactly one.
    pub fn exact_selected_snapshot(&self, document_id: u64) -> Option<&SnapshotAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::Snapshot(selected)] = selected.as_slice() else {
            return None;
        };
        document
            .snapshots()
            .iter()
            .find(|snapshot| snapshot.id == selected.id)
    }

    /// Changes caption visibility only when the selection is exactly one
    /// Length, Polylength, or Area. The annotation model remains the sole
    /// owner of calibration state and history.
    pub fn set_exact_selected_measurement_show_caption(
        &mut self,
        document_id: u64,
        show_caption: bool,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let selected = document.selected_annotations_in_document_order();
        let (id, edit) = match selected.as_slice() {
            [Annotation::Length(annotation)] => (
                annotation.id.clone(),
                AnnotationEdit::SetLengthCalibration(
                    annotation
                        .calibration()
                        .clone()
                        .with_show_caption(show_caption),
                ),
            ),
            [Annotation::MeasurementPath(annotation)] => (
                annotation.id.clone(),
                AnnotationEdit::SetMeasurementPathCalibration(
                    annotation
                        .calibration()
                        .clone()
                        .with_show_caption(show_caption),
                ),
            ),
            _ => return Err(AnnotationError::NoSelection),
        };
        document.apply_command(AnnotationCommand::EditAnnotation { id, edit })?;
        Ok(())
    }

    pub fn selected_straight_line_appearance(
        &self,
        document_id: u64,
    ) -> Option<&StraightLineAppearance> {
        let document = self.documents.get(&document_id)?;
        let id = document.selected_id()?;
        document
            .straight_lines()
            .iter()
            .find(|annotation| &annotation.id == id)
            .map(|annotation| &annotation.appearance)
    }

    pub fn selected_vertex_path(&self, document_id: u64) -> Option<&VertexPathAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::VertexPath(annotation)] = selected.as_slice() else {
            return None;
        };
        document
            .vertex_paths()
            .iter()
            .find(|candidate| candidate.id == annotation.id)
    }

    /// Commits the ordinary rectangle-properties command. Benchmark input
    /// drives this same product command and only adds observations around it.
    pub fn commit_selected_rectangle_stroke_width(
        &mut self,
        document_id: u64,
        stroke_width_pt: f64,
    ) -> Result<PropertyEditCommit, NativeEditingV5Error> {
        let document = self.documents.get_mut(&document_id).ok_or_else(|| {
            NativeEditingV5Error::GestureInvariant("annotation document is missing".into())
        })?;
        let target_id = document.selected_id().cloned().ok_or_else(|| {
            NativeEditingV5Error::GestureInvariant("rectangle selection is missing".into())
        })?;
        let mut transaction = StrokeWidthEditTransaction::begin(document, &target_id)?;
        transaction.stage_stroke_width(stroke_width_pt)?;
        transaction.commit(document)
    }

    pub fn pointer_down(
        &mut self,
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        self.pointer_down_with_input(
            document_id,
            page_index,
            pointer_id,
            0,
            point,
            tolerance_pt,
            false,
        )
    }

    pub fn pointer_down_with_input(
        &mut self,
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        button: u8,
        point: PdfPoint,
        tolerance_pt: f64,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let viewport_point = SelectionPoint::new(
            point.x * self.observed_pixels_per_point.0,
            point.y * self.observed_pixels_per_point.0,
        );
        self.pointer_down_with_viewport_input(
            document_id,
            page_index,
            pointer_id,
            button,
            point,
            viewport_point,
            tolerance_pt,
            PointerInputModifiers {
                shift: constrain_orthogonal,
                alt: false,
            },
        )
    }

    pub fn pointer_down_with_viewport_input(
        &mut self,
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        button: u8,
        point: PdfPoint,
        viewport_point: SelectionPoint,
        tolerance_pt: f64,
        modifiers: PointerInputModifiers,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let constrain_orthogonal = modifiers.shift;
        if button != 0 {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let point = self.resolve_semantic_creation_point(
            document_id,
            page_index,
            point,
            constrain_orthogonal,
        );
        if self.tool == AnnotationTool::Snapshot {
            if let Some(mut draft) = self.snapshot_draft.take() {
                if (draft.document_id, draft.page_index) != (document_id, page_index) {
                    self.snapshot_draft = Some(draft);
                    return Err(AnnotationError::NoActiveGesture);
                }
                draft.current = point;
                let rect = PdfRect::from_corners(draft.start, point);
                if rect.width <= 2. || rect.height <= 2. {
                    self.snapshot_draft = Some(draft);
                    return Ok(PointerPhaseOutcome::Ignored);
                }
                let Some(asset) = self.snapshot_capture_asset.clone() else {
                    self.snapshot_draft = Some(draft);
                    return Err(AnnotationError::InvalidFixture(
                        "Snapshot second click requires a synchronous decoded page capture".into(),
                    ));
                };
                let opacity = self.tool_properties(AnnotationTool::Snapshot).opacity;
                let annotation =
                    SnapshotAnnotation::new(draft.id.clone(), page_index, rect, asset, opacity)?;
                let id = annotation.id.clone();
                self.documents
                    .entry(document_id)
                    .or_default()
                    .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Snapshot(
                        annotation,
                    )))?;
                self.snapshot_capture_asset = None;
                self.tool = AnnotationTool::Select;
                return Ok(PointerPhaseOutcome::AnnotationCreated(id));
            }

            let id = self.next_id(AnnotationTool::Snapshot)?;
            self.documents
                .entry(document_id)
                .or_default()
                .clear_selection();
            self.snapshot_capture_asset = None;
            self.snapshot_draft = Some(SnapshotDraft {
                document_id,
                page_index,
                pointer_id,
                id,
                start: point,
                current: point,
            });
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        if matches!(
            self.active,
            Some(ActivePointer::Marquee {
                marquee: SelectionMarquee {
                    shape: SelectionShape::Box,
                    ..
                },
                ..
            })
        ) {
            let active = self.active.take().ok_or(AnnotationError::NoActiveGesture)?;
            let ActivePointer::Marquee {
                document_id: active_document_id,
                page_index: active_page_index,
                pointer_id: active_pointer_id,
                mut marquee,
                mut pdf_points,
            } = active
            else {
                unreachable!("the pending marquee branch retains a marquee")
            };
            if (active_document_id, active_page_index, active_pointer_id)
                != (document_id, page_index, pointer_id)
            {
                return Err(AnnotationError::NoActiveGesture);
            }
            marquee.update(viewport_point);
            pdf_points.push(point);
            let mut pdf_marquee = marquee;
            pdf_marquee.start = pdf_points
                .first()
                .copied()
                .map(selection_point_from_pdf)
                .unwrap_or_else(|| selection_point_from_pdf(point));
            pdf_marquee.current = selection_point_from_pdf(point);
            pdf_marquee.points = pdf_points
                .into_iter()
                .map(selection_point_from_pdf)
                .collect();
            let document = self
                .documents
                .get_mut(&document_id)
                .ok_or(AnnotationError::NoActiveGesture)?;
            document.apply_marquee_selection(page_index, &pdf_marquee, selection_point_from_pdf);
            return Ok(PointerPhaseOutcome::SelectionChanged(
                document.selected_id().cloned(),
            ));
        }
        if self.is_click_placement_pending() {
            let (active_document_id, active_page_index, active_pointer_id) = match self.active {
                Some(ActivePointer::Domain {
                    document_id,
                    page_index,
                    pointer_id,
                    ..
                }) => (document_id, page_index, pointer_id),
                Some(ActivePointer::EllipseCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    ..
                }) => (document_id, page_index, pointer_id),
                Some(ActivePointer::RedactCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    ..
                }) => (document_id, page_index, pointer_id),
                Some(ActivePointer::StraightLineCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    ..
                }) => (document_id, page_index, pointer_id),
                Some(ActivePointer::CalloutCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    ..
                }) => (document_id, page_index, pointer_id),
                _ => unreachable!("pending click placement has a supported pointer"),
            };
            if (active_document_id, active_page_index, active_pointer_id)
                != (document_id, page_index, pointer_id)
            {
                self.cancel(PointerCancelReason::AdapterError)?;
                return Err(AnnotationError::NoActiveGesture);
            }
            self.pointer_move_with_constraint(pointer_id, point, constrain_orthogonal)?;
            return self.commit_pending_click(pointer_id, point, constrain_orthogonal);
        }
        if matches!(self.tool, AnnotationTool::Polylength | AnnotationTool::Area) {
            let kind = match self.tool {
                AnnotationTool::Polylength => MeasurementPathKind::Polylength,
                AnnotationTool::Area => MeasurementPathKind::Area,
                _ => unreachable!("the measurement-path branch receives a measurement tool"),
            };
            if let Some(draft) = self.measurement_path_draft.as_mut() {
                if (draft.document_id, draft.page_index, draft.kind)
                    != (document_id, page_index, kind)
                {
                    self.measurement_path_draft = None;
                    return Err(AnnotationError::NoActiveGesture);
                }
                let last = *draft
                    .points
                    .last()
                    .expect("a measurement draft has a first point");
                if (point.x - last.x).hypot(point.y - last.y) >= 0.5 {
                    draft.points.push(point);
                }
                draft.hover = point;
            } else {
                let calibration = self
                    .document_page_length_calibration(document_id, page_index)
                    .cloned()
                    .ok_or_else(|| {
                        AnnotationError::InvalidGeometry(LENGTH_SCALE_REQUIRED_MESSAGE.into())
                    })?;
                let id = self.next_id(self.tool)?;
                self.documents
                    .entry(document_id)
                    .or_default()
                    .clear_selection();
                self.measurement_path_draft = Some(MeasurementPathDraft {
                    document_id,
                    page_index,
                    id,
                    kind,
                    calibration,
                    points: vec![point],
                    hover: point,
                });
            }
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        if self.tool == AnnotationTool::Cloud {
            if let Some(draft) = self.cloud_draft.as_mut() {
                if (draft.document_id, draft.page_index) != (document_id, page_index) {
                    self.cloud_draft = None;
                    return Err(AnnotationError::NoActiveGesture);
                }
                let closes_cloud = draft.points.len() >= 3
                    && point_distance_css_px(
                        draft.points[0],
                        point,
                        self.observed_pixels_per_point.0,
                    ) <= 10.0;
                if closes_cloud {
                    draft.hover = draft.points[0];
                    return self.finish_cloud(document_id);
                }
                if point_distance_css_px(
                    *draft
                        .points
                        .last()
                        .expect("a cloud draft has a first point"),
                    point,
                    self.observed_pixels_per_point.0,
                ) >= 0.5 * self.observed_pixels_per_point.0
                {
                    draft.points.push(point);
                }
                draft.hover = point;
            } else {
                let id = self.next_id(self.tool)?;
                self.documents
                    .entry(document_id)
                    .or_default()
                    .clear_selection();
                self.cloud_draft = Some(VertexPathDraft {
                    document_id,
                    page_index,
                    id,
                    kind: VertexPathKind::Polygon,
                    points: vec![point],
                    hover: point,
                });
            }
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        if self.tool == AnnotationTool::CloudPlus
            && let Some(draft) = self.cloud_plus_draft.as_mut()
        {
            if (draft.document_id, draft.page_index) != (document_id, page_index) {
                self.cloud_plus_draft = None;
                return Err(AnnotationError::NoActiveGesture);
            }
            let closes_cloud = draft.points.len() >= 3
                && point_distance_css_px(draft.points[0], point, self.observed_pixels_per_point.0)
                    <= 10.0;
            if closes_cloud {
                draft.hover = draft.points[0];
                return self.finish_cloud_plus(document_id);
            }
            if point_distance_css_px(
                *draft
                    .points
                    .last()
                    .expect("a Cloud+ draft has a first point"),
                point,
                self.observed_pixels_per_point.0,
            ) >= 0.5 * self.observed_pixels_per_point.0
            {
                let point = if constrain_orthogonal {
                    constrained_line_point(*draft.points.last().unwrap(), point, true)
                } else {
                    point
                };
                draft.points.push(point);
            }
            draft.hover = point;
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        if matches!(
            self.tool,
            AnnotationTool::Polyline | AnnotationTool::Polygon
        ) {
            let kind = match self.tool {
                AnnotationTool::Polyline => VertexPathKind::Polyline,
                AnnotationTool::Polygon => VertexPathKind::Polygon,
                _ => unreachable!("the vertex-path branch receives a vertex-path tool"),
            };
            if let Some(draft) = self.vertex_path_draft.as_mut() {
                if (draft.document_id, draft.page_index, draft.kind)
                    != (document_id, page_index, kind)
                {
                    self.vertex_path_draft = None;
                    return Err(AnnotationError::NoActiveGesture);
                }
                let closes_polygon = draft.kind == VertexPathKind::Polygon
                    && draft.points.len() >= draft.kind.minimum_points()
                    && point_distance_css_px(
                        draft.points[0],
                        point,
                        self.observed_pixels_per_point.0,
                    ) <= 10.0;
                if closes_polygon {
                    draft.hover = draft.points[0];
                    return self.finish_vertex_path(document_id);
                }
                if point_distance_css_px(
                    *draft
                        .points
                        .last()
                        .expect("a vertex draft has a first point"),
                    point,
                    self.observed_pixels_per_point.0,
                ) >= 0.5 * self.observed_pixels_per_point.0
                {
                    draft.points.push(point);
                }
                draft.hover = point;
            } else {
                let id = self.next_id(self.tool)?;
                self.documents
                    .entry(document_id)
                    .or_default()
                    .clear_selection();
                self.vertex_path_draft = Some(VertexPathDraft {
                    document_id,
                    page_index,
                    id,
                    kind,
                    points: vec![point],
                    hover: point,
                });
            }
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        if self.tool == AnnotationTool::Arc {
            if let Some(mut draft) = self.arc_draft.take() {
                if (draft.document_id, draft.page_index) != (document_id, page_index) {
                    self.arc_draft = Some(draft);
                    return Err(AnnotationError::NoActiveGesture);
                }
                if let Some(end) = draft.end {
                    let mid = ArcAnnotation::constrained_midpoint(
                        draft.start,
                        end,
                        point,
                        ARC_MINIMUM_BULGE_CSS_PX / self.observed_pixels_per_point.0,
                        constrain_orthogonal,
                    )?;
                    let annotation = ArcAnnotation::new(
                        draft.id.clone(),
                        page_index,
                        draft.start,
                        end,
                        mid,
                        draft.appearance,
                    )?;
                    let id = annotation.id.clone();
                    self.documents
                        .entry(document_id)
                        .or_default()
                        .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Arc(
                            annotation,
                        )))?;
                    self.tool = AnnotationTool::Select;
                    return Ok(PointerPhaseOutcome::AnnotationCreated(id));
                }
                if (point.x - draft.start.x).hypot(point.y - draft.start.y)
                    > LENGTH_MINIMUM_PDF_DISTANCE
                {
                    draft.end = Some(point);
                    draft.mid = ArcAnnotation::constrained_midpoint(
                        draft.start,
                        point,
                        draft.start,
                        ARC_MINIMUM_BULGE_CSS_PX / self.observed_pixels_per_point.0,
                        false,
                    )?;
                }
                self.arc_draft = Some(draft);
                return Ok(PointerPhaseOutcome::PlacementPending);
            }
            let id = self.next_id(self.tool)?;
            let appearance = match self.queued_rectangle_appearance.take() {
                Some(appearance) => appearance,
                None => {
                    let properties = self.tool_properties(AnnotationTool::Arc);
                    rectangle_tool_appearance(&properties, false)?
                }
            };
            self.documents
                .entry(document_id)
                .or_default()
                .clear_selection();
            self.arc_draft = Some(ArcDraft {
                document_id,
                page_index,
                id,
                start: point,
                end: None,
                mid: point,
                appearance,
            });
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        self.cancel(PointerCancelReason::AdapterError)?;
        let tool = self.tool;
        let id = if tool == AnnotationTool::Select {
            None
        } else {
            Some(self.next_id(tool)?)
        };
        let text_content = (tool == AnnotationTool::TextBox).then(|| {
            self.queued_text_content
                .take()
                .unwrap_or_else(|| FROZEN_TEXT_CREATE.to_owned())
        });
        let tool_properties = self.tool_properties(tool);
        let document = self.documents.entry(document_id).or_default();
        match tool {
            AnnotationTool::Select => {
                let direct_hit = document.hit_test(page_index, point, tolerance_pt)?;
                let selectable_hit_id = direct_hit
                    .as_ref()
                    .map(|hit| hit.markup_id().clone())
                    .or_else(|| hit_non_rectangle(document, page_index, point, tolerance_pt));
                if let Some((id, control)) = hit_selected_arc_control_point(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .arcs()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("an Arc control-point hit must retain its annotation");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::ArcControlPoint {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        expected_revision: document.snapshot().revision,
                        control,
                        start: point,
                        current: point,
                        original: annotation.clone(),
                        snap_quarter_turn: constrain_orthogonal,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if constrain_orthogonal && let Some(id) = selectable_hit_id.clone() {
                    document.toggle_selection(&id);
                    return Ok(PointerPhaseOutcome::SelectionChanged(
                        document.selected_id().cloned(),
                    ));
                }
                if let Some((id, handle)) = hit_selected_ellipse_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .ellipses()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("an Ellipse handle hit must retain its annotation")
                        .clone();
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(match handle {
                        EllipseHandleKind::Resize(handle) => ActivePointer::EllipseResize {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            handle,
                            start: point,
                            current: point,
                            original_rect: annotation.rect,
                            original_rotation_degrees: annotation.rotation_degrees,
                        },
                        EllipseHandleKind::Rotate => ActivePointer::EllipseRotate {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_rect: annotation.rect,
                            original_rotation_degrees: annotation.rotation_degrees,
                        },
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, handle)) = hit_selected_redact_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .redacts()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a Redact handle hit must retain its annotation")
                        .clone();
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::RedactResize {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        handle,
                        start: point,
                        current: point,
                        original_rect: annotation.rect,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some(id) = selectable_hit_id.clone()
                    && document.selected_ids().len() > 1
                    && document.selected_ids().contains(&id)
                {
                    self.active = Some(ActivePointer::GroupMove {
                        document_id,
                        page_index,
                        pointer_id,
                        start: point,
                        current: point,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, kind)) = hit_selected_dimension_control(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let expected_revision = document.snapshot().revision;
                    let original = document
                        .dimensions()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a Dimension control hit must retain its annotation")
                        .clone();
                    if original.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::DimensionEdit {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        expected_revision,
                        kind,
                        start: point,
                        current: point,
                        original,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, kind)) = hit_selected_callout_control(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let expected_revision = document.snapshot().revision;
                    let original = document
                        .callouts()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a Callout control hit must retain its annotation")
                        .clone();
                    if original.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::CalloutEdit {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        expected_revision,
                        kind,
                        start: point,
                        current: point,
                        original,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, kind)) = hit_selected_cloud_control(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let expected_revision = document.snapshot().revision;
                    let original = document
                        .clouds()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a Cloud control hit must retain its annotation")
                        .clone();
                    if original.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::CloudEdit {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        expected_revision,
                        kind,
                        start: point,
                        current: point,
                        original,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, endpoint)) =
                    hit_straight_line_endpoint(document, page_index, point, tolerance_pt)
                {
                    document.select(&id);
                    let annotation = document
                        .straight_lines()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("an endpoint hit must retain its straight line");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::StraightLineEndpoint {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        endpoint,
                        start: point,
                        current: point,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, vertex_index)) =
                    hit_selected_vertex_path_point(document, page_index, point, tolerance_pt)
                {
                    document.select(&id);
                    let annotation = document
                        .vertex_paths()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a vertex handle hit must retain its vertex path");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::VertexPathPoint {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        vertex_index,
                        start: point,
                        current: point,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, vertex_index)) =
                    hit_selected_measurement_path_point(document, page_index, point, tolerance_pt)
                {
                    document.select(&id);
                    let annotation = document
                        .measurement_paths()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a vertex handle hit must retain its measurement path");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::MeasurementPathPoint {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        vertex_index,
                        start: point,
                        current: point,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, endpoint)) =
                    hit_length_endpoint(document, page_index, point, tolerance_pt)
                {
                    document.select(&id);
                    self.active = Some(ActivePointer::LengthEndpoint {
                        document_id,
                        pointer_id,
                        id: id.clone(),
                        endpoint,
                        current: point,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, handle)) = hit_selected_text_box_resize_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .text_boxes()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a resize-handle hit must retain its Text Box");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::TextBoxResize {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        handle,
                        start: point,
                        current: point,
                        original_rect: annotation.layout_rect,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, handle)) =
                    hit_selected_image_resize_handle(document, page_index, point, tolerance_pt)
                {
                    document.select(&id);
                    let annotation = document
                        .images()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a resize-handle hit must retain its image");
                    if annotation.locked || annotation.aspect_locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::ImageResize {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        handle,
                        start: point,
                        current: point,
                        original_rect: annotation.rect,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, handle)) = hit_selected_snapshot_resize_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .snapshots()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a resize-handle hit must retain its Snapshot");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::SnapshotResize {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        handle,
                        start: point,
                        current: point,
                        original_rect: annotation.rect,
                        original_rotation_degrees: annotation.rotation_degrees(),
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some(id) = hit_selected_snapshot_rotation_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .snapshots()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a rotation-handle hit must retain its Snapshot");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::SnapshotRotate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start: point,
                        current: point,
                        original_rect: annotation.rect,
                        original_rotation_degrees: annotation.rotation_degrees(),
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some(id) = hit_straight_line(document, page_index, point, tolerance_pt) {
                    document.select(&id);
                    let annotation = document
                        .straight_lines()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a segment hit must retain its straight line");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    self.active = Some(ActivePointer::StraightLineMove {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start: point,
                        current: point,
                        original_start: annotation.start,
                        original_end: annotation.end,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some(id) = hit_non_rectangle(document, page_index, point, tolerance_pt) {
                    document.select(&id);
                    if let Some(annotation) = document
                        .redacts()
                        .iter()
                        .find(|annotation| annotation.id == id)
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        self.active = Some(ActivePointer::RedactMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_rect: annotation.rect,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .arcs()
                        .iter()
                        .find(|annotation| annotation.id == id)
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        self.active = Some(ActivePointer::ArcMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            expected_revision: document.snapshot().revision,
                            start: point,
                            current: point,
                            original: annotation.clone(),
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .ellipses()
                        .iter()
                        .find(|annotation| annotation.id == id)
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        self.active = Some(ActivePointer::EllipseMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_rect: annotation.rect,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .pens()
                        .iter()
                        .find(|annotation| annotation.id == id && !annotation.locked)
                    {
                        self.active = Some(ActivePointer::InkMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_paths: annotation.paths().map(|path| path.to_vec()).collect(),
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .text_boxes()
                        .iter()
                        .find(|annotation| annotation.id == id && !annotation.locked)
                    {
                        self.active = Some(ActivePointer::TextBoxMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_rect: annotation.layout_rect,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if document
                        .vertex_paths()
                        .iter()
                        .find(|annotation| annotation.id == id && !annotation.locked)
                        .is_some()
                    {
                        self.active = Some(ActivePointer::GroupMove {
                            document_id,
                            page_index,
                            pointer_id,
                            start: point,
                            current: point,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if document
                        .measurement_paths()
                        .iter()
                        .any(|annotation| annotation.id == id && !annotation.locked)
                    {
                        self.active = Some(ActivePointer::GroupMove {
                            document_id,
                            page_index,
                            pointer_id,
                            start: point,
                            current: point,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .images()
                        .iter()
                        .find(|annotation| annotation.id == id && !annotation.locked)
                    {
                        self.active = Some(ActivePointer::ImageMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_rect: annotation.rect,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .snapshots()
                        .iter()
                        .find(|annotation| annotation.id == id)
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        self.active = Some(ActivePointer::SnapshotMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_rect: annotation.rect,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .dimensions()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .cloned()
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        self.active = Some(ActivePointer::DimensionEdit {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            expected_revision: document.snapshot().revision,
                            kind: DimensionPointerEditKind::Body,
                            start: point,
                            current: point,
                            original: annotation,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .callouts()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .cloned()
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        let kind = if rect_contains(annotation.text_box, point, tolerance_pt) {
                            CalloutPointerEditKind::TextBox
                        } else {
                            CalloutPointerEditKind::Body
                        };
                        self.active = Some(ActivePointer::CalloutEdit {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            expected_revision: document.snapshot().revision,
                            kind,
                            start: point,
                            current: point,
                            original: annotation,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .clouds()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .cloned()
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        self.active = Some(ActivePointer::CloudEdit {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            expected_revision: document.snapshot().revision,
                            kind: CloudPointerEditKind::Body,
                            start: point,
                            current: point,
                            original: annotation,
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                if selectable_hit_id.is_none() && (modifiers.shift || modifiers.alt) {
                    self.active = Some(ActivePointer::Marquee {
                        document_id,
                        page_index,
                        pointer_id,
                        marquee: SelectionMarquee::lasso(
                            pointer_id,
                            viewport_point,
                            SelectionOperation::from_modifiers(modifiers.shift, modifiers.alt),
                        ),
                        pdf_points: vec![point],
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                let had_selection = !document.selected_ids().is_empty();
                let outcome = document.apply_command(AnnotationCommand::PointerDown {
                    pointer_id,
                    page_index,
                    point,
                    tolerance_pt,
                    tool: PointerTool::Select {
                        rotation_handle_offset_pt: ROTATION_HANDLE_OFFSET_CSS_PX
                            / self.observed_pixels_per_point.0,
                    },
                })?;
                if matches!(outcome, CommandOutcome::GestureStarted { .. }) {
                    let rectangle_translation_start = matches!(
                        outcome,
                        CommandOutcome::GestureStarted {
                            kind: GestureKind::Move,
                            ..
                        }
                    )
                    .then_some(point);
                    self.active = Some(ActivePointer::Domain {
                        document_id,
                        page_index,
                        pointer_id,
                        ink: false,
                        ink_start: None,
                        rectangle_translation_start,
                        rectangle_create_start: None,
                        click_placement_pending: false,
                    });
                    Ok(PointerPhaseOutcome::GestureStarted)
                } else if selectable_hit_id.is_none() && !had_selection {
                    self.active = Some(ActivePointer::Marquee {
                        document_id,
                        page_index,
                        pointer_id,
                        marquee: SelectionMarquee::lasso(
                            pointer_id,
                            viewport_point,
                            SelectionOperation::Replace,
                        ),
                        pdf_points: vec![point],
                    });
                    Ok(PointerPhaseOutcome::GestureStarted)
                } else {
                    Ok(PointerPhaseOutcome::SelectionChanged(
                        document.selected_id().cloned(),
                    ))
                }
            }
            AnnotationTool::Rectangle => {
                let id = id.expect("drawing tools allocate an annotation ID");
                let appearance = self
                    .queued_rectangle_appearance
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| rectangle_tool_appearance(&tool_properties, true))?;
                document.apply_command(AnnotationCommand::PointerDown {
                    pointer_id,
                    page_index,
                    point,
                    tolerance_pt,
                    tool: PointerTool::Rectangle { id, appearance },
                })?;
                self.active = Some(ActivePointer::Domain {
                    document_id,
                    page_index,
                    pointer_id,
                    ink: false,
                    ink_start: None,
                    rectangle_translation_start: None,
                    rectangle_create_start: Some(point),
                    click_placement_pending: false,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::Ellipse => {
                let id = id.expect("drawing tools allocate an annotation ID");
                let appearance = self
                    .queued_rectangle_appearance
                    .take()
                    .map(Ok)
                    .unwrap_or_else(|| rectangle_tool_appearance(&tool_properties, true))?;
                document.clear_selection();
                self.active = Some(ActivePointer::EllipseCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    id,
                    appearance,
                    start: point,
                    current: point,
                    click_placement_pending: false,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::Redact => {
                let id = id.expect("drawing tools allocate an annotation ID");
                document.clear_selection();
                self.active = Some(ActivePointer::RedactCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    id,
                    start: point,
                    viewport_start: viewport_point,
                    current: point,
                    click_placement_pending: false,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::Arc => unreachable!("Arc placement is handled before pointer capture"),
            AnnotationTool::Line | AnnotationTool::Arrow => {
                let id = id.expect("drawing tools allocate an annotation ID");
                let kind = match tool {
                    AnnotationTool::Line => LineKind::Line,
                    AnnotationTool::Arrow => LineKind::Arrow,
                    _ => unreachable!("the straight-line arm receives a straight-line tool"),
                };
                document.clear_selection();
                self.active = Some(ActivePointer::StraightLineCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    id,
                    kind,
                    appearance: straight_line_tool_appearance(&tool_properties)?,
                    start: point,
                    current: point,
                    click_placement_pending: false,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::Callout => {
                let id = id.expect("drawing tools allocate an annotation ID");
                document.clear_selection();
                self.active = Some(ActivePointer::CalloutCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    id,
                    start: point,
                    current: point,
                    click_placement_pending: false,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::CloudPlus => {
                let id = id.expect("drawing tools allocate an annotation ID");
                document.clear_selection();
                self.active = Some(ActivePointer::CloudPlusCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    id,
                    start: point,
                    current: point,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::Pen => {
                let id = id.expect("drawing tools allocate an annotation ID");
                document.apply_command(AnnotationCommand::BeginInk {
                    pointer_id,
                    id,
                    page_index,
                    start: point,
                    appearance: pen_tool_appearance(&tool_properties)?,
                    smooth_curves: tool_properties.smooth_curves,
                    tool: InkTool::Pen,
                })?;
                self.active = Some(ActivePointer::Domain {
                    document_id,
                    page_index,
                    pointer_id,
                    ink: true,
                    ink_start: Some(point),
                    rectangle_translation_start: None,
                    rectangle_create_start: None,
                    click_placement_pending: false,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::Highlight => {
                let id = id.expect("drawing tools allocate an annotation ID");
                document.apply_command(AnnotationCommand::BeginInk {
                    pointer_id,
                    id,
                    page_index,
                    start: point,
                    appearance: pen_tool_appearance(&tool_properties)?,
                    smooth_curves: false,
                    tool: InkTool::Highlight,
                })?;
                self.active = Some(ActivePointer::Domain {
                    document_id,
                    page_index,
                    pointer_id,
                    ink: true,
                    ink_start: Some(point),
                    rectangle_translation_start: None,
                    rectangle_create_start: None,
                    click_placement_pending: false,
                });
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            AnnotationTool::TextBox => {
                let id = id.expect("drawing tools allocate an annotation ID");
                let annotation = TextBoxAnnotation::new(
                    id.clone(),
                    page_index,
                    PdfRect::new(point.x, point.y, TEXT_WIDTH_PT, TEXT_HEIGHT_PT)?,
                    text_content.expect("text tool prepares initial content"),
                    text_box_tool_style(&tool_properties)?,
                )?;
                document.apply_command(AnnotationCommand::CreateAnnotation(
                    Annotation::TextBox(annotation),
                ))?;
                Ok(PointerPhaseOutcome::AnnotationCreated(id))
            }
            AnnotationTool::Polyline
            | AnnotationTool::Polygon
            | AnnotationTool::Polylength
            | AnnotationTool::Area
            | AnnotationTool::Cloud => {
                unreachable!("vertex-path tools return before ordinary pointer dispatch")
            }
            AnnotationTool::Length => Err(AnnotationError::InvalidGeometry(
                "length creation requires the two-click placement interface".into(),
            )),
            AnnotationTool::Dimension => Err(AnnotationError::InvalidGeometry(
                "dimension creation requires the two-click placement interface".into(),
            )),
            AnnotationTool::Image => {
                let id = id.expect("drawing tools allocate an annotation ID");
                let pending = self.image_asset.clone().ok_or_else(|| {
                    AnnotationError::InvalidFixture(
                        "image tool requires a decoded bounded PNG or JPEG asset".into(),
                    )
                })?;
                if pending.aspect_locked {
                    self.image_asset = None;
                }
                let placement_page = self.image_placement_page.ok_or_else(|| {
                    AnnotationError::InvalidGeometry(
                        "image tool requires the current page dimensions".into(),
                    )
                })?;
                let source_width = f64::from(pending.asset.width_px());
                let source_height = f64::from(pending.asset.height_px());
                let aspect_ratio = (source_width / source_height).max(0.01);
                let natural_width = source_width.max(24.0);
                let natural_height = natural_width / aspect_ratio;
                let scale = 1.0_f64
                    .min(placement_page.width_pt * placement_page.max_fraction / natural_width)
                    .min(placement_page.height_pt * placement_page.max_fraction / natural_height);
                let (width, height) = if pending.aspect_locked {
                    (natural_width * scale, natural_height * scale)
                } else {
                    let width = (natural_width * scale).max(24.0);
                    let height = (width / aspect_ratio).max(24.0);
                    (width, height)
                };
                let x =
                    (point.x - width / 2.0).clamp(0.0, (placement_page.width_pt - width).max(0.0));
                let y = (point.y - height / 2.0)
                    .clamp(0.0, (placement_page.height_pt - height).max(0.0));
                let annotation = ImageAnnotation::new_with_opacity(
                    id.clone(),
                    page_index,
                    PdfRect::new(x, y, width, height)?,
                    pending.asset,
                    pending.aspect_locked,
                    tool_properties.opacity,
                )?;
                document.apply_command(AnnotationCommand::CreateAnnotation(Annotation::Image(
                    annotation,
                )))?;
                Ok(PointerPhaseOutcome::AnnotationCreated(id))
            }
            AnnotationTool::Snapshot => {
                unreachable!("Snapshot placement returns before ordinary pointer dispatch")
            }
        }
    }

    pub fn pointer_double_click(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        if self.active.is_some() {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        if let Some(draft) = self.cloud_plus_draft.as_mut() {
            if (draft.document_id, draft.page_index) != (document_id, page_index) {
                return Ok(PointerPhaseOutcome::Ignored);
            }
            let last = *draft
                .points
                .last()
                .expect("a Cloud+ draft has a first point");
            if point_distance_css_px(last, point, self.observed_pixels_per_point.0)
                >= 0.5 * self.observed_pixels_per_point.0
            {
                draft.points.push(point);
            }
            draft.hover = point;
            return self.finish_cloud_plus(document_id);
        }
        if let Some(draft) = self.measurement_path_draft.as_mut() {
            if (draft.document_id, draft.page_index) != (document_id, page_index) {
                return Ok(PointerPhaseOutcome::Ignored);
            }
            let last = *draft
                .points
                .last()
                .expect("a measurement draft has a first point");
            if (point.x - last.x).hypot(point.y - last.y) >= 0.5 {
                draft.points.push(point);
            }
            draft.hover = point;
            return self.finish_measurement_path(document_id);
        }
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if let Some(selected) = document.selected_id().cloned()
            && document.text_boxes().iter().any(|annotation| {
                annotation.id == selected
                    && annotation.page_index == page_index
                    && !annotation.locked
                    && point.x >= annotation.layout_rect.x - tolerance_pt
                    && point.x
                        <= annotation.layout_rect.x + annotation.layout_rect.width + tolerance_pt
                    && point.y >= annotation.layout_rect.y - tolerance_pt
                    && point.y
                        <= annotation.layout_rect.y + annotation.layout_rect.height + tolerance_pt
            })
        {
            return Ok(PointerPhaseOutcome::SelectionChanged(Some(selected)));
        }
        if let Some(selected) = document.selected_id().cloned()
            && document.dimensions().iter().any(|annotation| {
                if annotation.id != selected
                    || annotation.page_index != page_index
                    || annotation.locked
                {
                    return false;
                }
                let (start, end) = annotation.dimension_line_points();
                point_segment_distance(point, start, end)
                    <= tolerance_pt.max(annotation.appearance.line().stroke_width_pt() / 2.)
            })
        {
            return Ok(PointerPhaseOutcome::SelectionChanged(Some(selected)));
        }
        if let Some(id) = hit_selected_snapshot_rotation_handle(
            document,
            page_index,
            point,
            tolerance_pt,
            self.observed_pixels_per_point.0,
        ) {
            let annotation = document
                .snapshots()
                .iter()
                .find(|annotation| annotation.id == id)
                .expect("a Snapshot rotation-handle hit must retain its annotation");
            if annotation.locked {
                return Ok(PointerPhaseOutcome::Ignored);
            }
            document.apply_command(AnnotationCommand::EditAnnotation {
                id: id.clone(),
                edit: AnnotationEdit::SetSnapshotRotation(0.),
            })?;
            return Ok(PointerPhaseOutcome::AnnotationEdited(id));
        }
        let Some((id, EllipseHandleKind::Rotate)) = hit_selected_ellipse_handle(
            document,
            page_index,
            point,
            tolerance_pt,
            self.observed_pixels_per_point.0,
        ) else {
            return Ok(PointerPhaseOutcome::Ignored);
        };
        let annotation = document
            .ellipses()
            .iter()
            .find(|annotation| annotation.id == id)
            .expect("an Ellipse rotation-handle hit must retain its annotation");
        if annotation.locked {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id: id.clone(),
            edit: AnnotationEdit::SetEllipseRotation(0.),
        })?;
        Ok(PointerPhaseOutcome::AnnotationEdited(id))
    }

    pub fn pointer_move(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        self.pointer_move_with_constraint(pointer_id, point, false)
    }

    pub fn pointer_move_with_constraint(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let viewport_point = SelectionPoint::new(
            point.x * self.observed_pixels_per_point.0,
            point.y * self.observed_pixels_per_point.0,
        );
        self.pointer_move_with_viewport_input(
            pointer_id,
            point,
            viewport_point,
            PointerInputModifiers {
                shift: constrain_orthogonal,
                alt: false,
            },
        )
    }

    pub fn pointer_move_with_viewport_input(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
        viewport_point: SelectionPoint,
        modifiers: PointerInputModifiers,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let constrain_orthogonal = modifiers.shift;
        let point = self
            .active_surface()
            .map_or(point, |(document_id, page_index)| {
                self.resolve_semantic_creation_point(
                    document_id,
                    page_index,
                    point,
                    constrain_orthogonal,
                )
            });
        if let Some(draft) = self.snapshot_draft.as_mut() {
            require_pointer(draft.pointer_id, pointer_id)?;
            draft.current = point;
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        let Some(active) = self.active.as_mut() else {
            return Ok(PointerPhaseOutcome::Ignored);
        };
        match active {
            ActivePointer::Marquee {
                pointer_id: active_pointer,
                marquee,
                pdf_points,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                marquee.update(viewport_point);
                pdf_points.push(point);
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::GroupMove {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::Domain {
                document_id,
                pointer_id: active_pointer,
                ink,
                rectangle_translation_start,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                let document = self
                    .documents
                    .get_mut(document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                if *ink {
                    document.apply_command(AnnotationCommand::AppendPenSamples {
                        pointer_id,
                        samples: vec![point],
                        min_distance_pt: HIGHLIGHT_MIN_DISTANCE_PT,
                    })?;
                } else {
                    let point = resolve_rectangle_translation_endpoint(
                        *rectangle_translation_start,
                        point,
                        self.rectangle_snap_settings,
                        self.observed_pixels_per_point.0,
                    );
                    document.apply_command(AnnotationCommand::PointerMove { pointer_id, point })?;
                }
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::EllipseCreate {
                pointer_id: active_pointer,
                start,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = if constrain_orthogonal {
                    EllipseAnnotation::constrained_end(*start, point)
                } else {
                    point
                };
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::EllipseMove {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::EllipseResize {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::EllipseRotate {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::RedactCreate {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::RedactMove {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::RedactResize {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::ArcMove {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::ArcControlPoint {
                pointer_id: active_pointer,
                current,
                snap_quarter_turn,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                *snap_quarter_turn = constrain_orthogonal;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::LengthCreate {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::DimensionCreate {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::LengthEndpoint {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::DimensionEdit {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::CalloutEdit {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::CloudEdit {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::StraightLineCreate {
                pointer_id: active_pointer,
                start,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = constrained_line_point(*start, point, constrain_orthogonal);
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::CalloutCreate {
                pointer_id: active_pointer,
                start,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = constrained_line_point(*start, point, constrain_orthogonal);
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::CloudPlusCreate {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::StraightLineMove {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::StraightLineEndpoint {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::VertexPathPoint {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::MeasurementPathPoint {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::InkMove {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::TextBoxMove {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::TextBoxResize {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
            ActivePointer::ImageMove {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::ImageResize {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::SnapshotMove {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::SnapshotResize {
                pointer_id: active_pointer,
                current,
                ..
            }
            | ActivePointer::SnapshotRotate {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
        }
    }

    pub fn pointer_up(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        if let Some(draft) = self.snapshot_draft.as_mut() {
            require_pointer(draft.pointer_id, pointer_id)?;
            draft.current = point;
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        self.pointer_up_with_constraint(pointer_id, point, false)
    }

    pub fn pointer_up_with_constraint(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let viewport_point = SelectionPoint::new(
            point.x * self.observed_pixels_per_point.0,
            point.y * self.observed_pixels_per_point.0,
        );
        self.pointer_up_with_viewport_input(
            pointer_id,
            point,
            viewport_point,
            PointerInputModifiers {
                shift: constrain_orthogonal,
                alt: false,
            },
        )
    }

    pub fn pointer_up_with_viewport_input(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
        viewport_point: SelectionPoint,
        modifiers: PointerInputModifiers,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let constrain_orthogonal = modifiers.shift;
        let point = self
            .active_surface()
            .map_or(point, |(document_id, page_index)| {
                self.resolve_semantic_creation_point(
                    document_id,
                    page_index,
                    point,
                    constrain_orthogonal,
                )
            });
        if let Some(draft) = self.snapshot_draft.as_mut() {
            require_pointer(draft.pointer_id, pointer_id)?;
            draft.current = point;
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        let active = self.active.take().ok_or(AnnotationError::NoActiveGesture)?;
        match active {
            ActivePointer::Marquee {
                document_id,
                page_index,
                pointer_id: active_pointer,
                mut marquee,
                mut pdf_points,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                marquee.update(viewport_point);
                pdf_points.push(point);
                if !marquee.active {
                    self.active = Some(ActivePointer::Marquee {
                        document_id,
                        page_index,
                        pointer_id,
                        marquee: SelectionMarquee::armed_box(marquee.start, marquee.operation),
                        pdf_points: pdf_points.first().copied().into_iter().collect(),
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                let mut pdf_marquee = marquee;
                pdf_marquee.start = pdf_points
                    .first()
                    .copied()
                    .map(selection_point_from_pdf)
                    .unwrap_or_else(|| selection_point_from_pdf(point));
                pdf_marquee.current = selection_point_from_pdf(point);
                pdf_marquee.points = pdf_points
                    .into_iter()
                    .map(selection_point_from_pdf)
                    .collect();
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                document.apply_marquee_selection(
                    page_index,
                    &pdf_marquee,
                    selection_point_from_pdf,
                );
                Ok(PointerPhaseOutcome::SelectionChanged(
                    document.selected_id().cloned(),
                ))
            }
            ActivePointer::GroupMove {
                document_id,
                page_index,
                pointer_id: active_pointer,
                start,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(
                        self.documents
                            .get(&document_id)
                            .and_then(AnnotationDocument::selected_id)
                            .cloned(),
                    ));
                }
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                let primary = document
                    .selected_id()
                    .cloned()
                    .ok_or(AnnotationError::NoSelection)?;
                let changed = document.translate_selection_on_page(
                    page_index,
                    point.x - start.x,
                    point.y - start.y,
                )?;
                if changed {
                    Ok(PointerPhaseOutcome::AnnotationEdited(primary))
                } else {
                    Ok(PointerPhaseOutcome::SelectionChanged(Some(primary)))
                }
            }
            ActivePointer::Domain {
                document_id,
                pointer_id: active_pointer,
                ink,
                ink_start,
                rectangle_translation_start,
                rectangle_create_start,
                click_placement_pending,
                page_index,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if click_placement_pending {
                    self.active = Some(ActivePointer::Domain {
                        document_id,
                        page_index,
                        pointer_id,
                        ink,
                        ink_start,
                        rectangle_translation_start,
                        rectangle_create_start,
                        click_placement_pending,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                if let Some(start) = ink_start
                    && point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                        < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    document.apply_command(AnnotationCommand::PointerCancel {
                        pointer_id,
                        reason: PointerCancelReason::AdapterError,
                    })?;
                    return Ok(PointerPhaseOutcome::Ignored);
                }
                if let Some(start) = rectangle_create_start
                    && point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                        <= POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    document.apply_command(AnnotationCommand::PointerMove { pointer_id, point })?;
                    self.active = Some(ActivePointer::Domain {
                        document_id,
                        page_index,
                        pointer_id,
                        ink,
                        ink_start: None,
                        rectangle_translation_start,
                        rectangle_create_start: Some(start),
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                let outcome = if ink {
                    document.apply_command(AnnotationCommand::AppendPenSamples {
                        pointer_id,
                        samples: vec![point],
                        min_distance_pt: HIGHLIGHT_MIN_DISTANCE_PT,
                    })?;
                    document.apply_command(AnnotationCommand::CommitPen { pointer_id })?
                } else {
                    let point = resolve_rectangle_translation_endpoint(
                        rectangle_translation_start,
                        point,
                        self.rectangle_snap_settings,
                        self.observed_pixels_per_point.0,
                    );
                    document.apply_command(AnnotationCommand::PointerUp { pointer_id, point })?
                };
                Ok(pointer_phase_outcome(outcome))
            }
            ActivePointer::EllipseCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                appearance,
                start,
                current,
                click_placement_pending,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if click_placement_pending {
                    self.active = Some(ActivePointer::EllipseCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        appearance,
                        start,
                        current,
                        click_placement_pending,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                let end = if constrain_orthogonal {
                    EllipseAnnotation::constrained_end(start, point)
                } else {
                    point
                };
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    self.active = Some(ActivePointer::EllipseCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        appearance,
                        start,
                        current: end,
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                self.commit_ellipse(document_id, page_index, id, appearance, start, end)
            }
            ActivePointer::EllipseMove {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetEllipseRect(PdfRect::new(
                            original_rect.x + point.x - start.x,
                            original_rect.y + point.y - start.y,
                            original_rect.width,
                            original_rect.height,
                        )?),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::EllipseResize {
                document_id,
                pointer_id: active_pointer,
                id,
                handle,
                start,
                original_rect,
                original_rotation_degrees,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetEllipseRect(ellipse_resized_rect(
                            original_rect,
                            original_rotation_degrees,
                            handle,
                            point,
                        )),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::RedactCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                viewport_start,
                current,
                click_placement_pending,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if click_placement_pending {
                    self.active = Some(ActivePointer::RedactCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start,
                        viewport_start,
                        current,
                        click_placement_pending,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                if (viewport_point.x - viewport_start.x).hypot(viewport_point.y - viewport_start.y)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    self.active = Some(ActivePointer::RedactCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start,
                        viewport_start,
                        current: point,
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                self.commit_redact(document_id, page_index, id, start, point)
            }
            ActivePointer::RedactMove {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetRedactRect(PdfRect::new(
                            original_rect.x + point.x - start.x,
                            original_rect.y + point.y - start.y,
                            original_rect.width,
                            original_rect.height,
                        )?),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::RedactResize {
                document_id,
                pointer_id: active_pointer,
                id,
                handle,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetRedactRect(redact_resized_rect(
                            original_rect,
                            handle,
                            point,
                        )?),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::EllipseRotate {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                original_rotation_degrees,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetEllipseRotation(ellipse_rotation_from_drag(
                            original_rect,
                            original_rotation_degrees,
                            start,
                            point,
                        )),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::ArcMove {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                expected_revision,
                start,
                original,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                validate_arc_pointer_target(
                    document,
                    page_index,
                    &id,
                    expected_revision,
                    &original,
                )?;
                document.apply_command(AnnotationCommand::EditAnnotation {
                    id: id.clone(),
                    edit: AnnotationEdit::TranslateArc {
                        delta_x: point.x - start.x,
                        delta_y: point.y - start.y,
                    },
                })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::ArcControlPoint {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                expected_revision,
                control,
                start,
                original,
                snap_quarter_turn: _,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                validate_arc_pointer_target(
                    document,
                    page_index,
                    &id,
                    expected_revision,
                    &original,
                )?;
                let snap_quarter_turn = constrain_orthogonal;
                let resolved = resolve_arc_control_point(
                    &original,
                    control,
                    point,
                    ARC_MINIMUM_BULGE_CSS_PX / self.observed_pixels_per_point.0,
                    snap_quarter_turn,
                )?;
                document.apply_command(AnnotationCommand::EditAnnotation {
                    id: id.clone(),
                    edit: AnnotationEdit::SetArcControlPoint {
                        control,
                        point: resolved,
                        snap_quarter_turn,
                    },
                })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::StraightLineCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                kind,
                appearance,
                start,
                current,
                click_placement_pending,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if click_placement_pending {
                    self.active = Some(ActivePointer::StraightLineCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        kind,
                        appearance,
                        start,
                        current,
                        click_placement_pending,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                let end = constrained_line_point(start, point, constrain_orthogonal);
                if point_distance_css_px(start, end, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    self.active = Some(ActivePointer::StraightLineCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        kind,
                        appearance,
                        start,
                        current: end,
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                self.commit_straight_line(document_id, page_index, id, kind, appearance, start, end)
            }
            ActivePointer::CalloutCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                current,
                click_placement_pending,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if click_placement_pending {
                    self.active = Some(ActivePointer::CalloutCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start,
                        current,
                        click_placement_pending,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                let end = constrained_line_point(start, point, constrain_orthogonal);
                if point_distance_css_px(start, end, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    self.active = Some(ActivePointer::CalloutCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start,
                        current: end,
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                self.commit_callout(document_id, page_index, id, start, end)
            }
            ActivePointer::CloudPlusCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                let drag_css_px =
                    point_distance_css_px(start, point, self.observed_pixels_per_point.0);
                if drag_css_px < POINTER_DRAG_THRESHOLD_CSS_PX {
                    self.cloud_plus_draft = Some(VertexPathDraft {
                        document_id,
                        page_index,
                        id,
                        kind: VertexPathKind::Polygon,
                        points: vec![start],
                        hover: point,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                if (point.x - start.x).hypot(point.y - start.y) <= LENGTH_MINIMUM_PDF_DISTANCE {
                    return Ok(PointerPhaseOutcome::Ignored);
                }
                let rect = PdfRect::from_corners(start, point);
                self.commit_cloud_plus(
                    document_id,
                    page_index,
                    id,
                    vec![
                        PdfPoint::new(rect.x, rect.y)?,
                        PdfPoint::new(rect.x + rect.width, rect.y)?,
                        PdfPoint::new(rect.x + rect.width, rect.y + rect.height)?,
                        PdfPoint::new(rect.x, rect.y + rect.height)?,
                    ],
                )
            }
            ActivePointer::StraightLineMove {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_start: _,
                original_end: _,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::TranslateStraightLine {
                            delta_x: point.x - start.x,
                            delta_y: point.y - start.y,
                        },
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::StraightLineEndpoint {
                document_id,
                pointer_id: active_pointer,
                id,
                endpoint,
                start,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetStraightLineEndpoint { endpoint, point },
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::VertexPathPoint {
                document_id,
                pointer_id: active_pointer,
                id,
                vertex_index,
                start,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetVertexPathPoint {
                            vertex_index,
                            point,
                        },
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::MeasurementPathPoint {
                document_id,
                pointer_id: active_pointer,
                id,
                vertex_index,
                start,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetMeasurementPathPoint {
                            vertex_index,
                            point,
                        },
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::LengthCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                current,
            } => {
                self.active = Some(ActivePointer::LengthCreate {
                    document_id,
                    page_index,
                    pointer_id: active_pointer,
                    id,
                    start,
                    current,
                });
                require_pointer(active_pointer, pointer_id)?;
                Ok(PointerPhaseOutcome::PlacementPending)
            }
            ActivePointer::DimensionCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                current,
            } => {
                self.active = Some(ActivePointer::DimensionCreate {
                    document_id,
                    page_index,
                    pointer_id: active_pointer,
                    id,
                    start,
                    current,
                });
                require_pointer(active_pointer, pointer_id)?;
                Ok(PointerPhaseOutcome::PlacementPending)
            }
            ActivePointer::DimensionEdit {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                expected_revision,
                kind,
                start,
                original,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                validate_dimension_pointer_target(
                    document,
                    page_index,
                    &id,
                    expected_revision,
                    &original,
                )?;
                let edit = match kind {
                    DimensionPointerEditKind::Start => AnnotationEdit::SetDimensionEndpoint {
                        endpoint: LineEndpoint::Start,
                        point,
                    },
                    DimensionPointerEditKind::End => AnnotationEdit::SetDimensionEndpoint {
                        endpoint: LineEndpoint::End,
                        point,
                    },
                    DimensionPointerEditKind::Offset => {
                        let delta_x = original.end.x - original.start.x;
                        let delta_y = original.end.y - original.start.y;
                        let length = delta_x.hypot(delta_y);
                        let projected_delta = (point.x - start.x) * (-delta_y / length)
                            + (point.y - start.y) * (delta_x / length);
                        AnnotationEdit::SetDimensionOffset(
                            original.dimension_line_offset() + projected_delta,
                        )
                    }
                    DimensionPointerEditKind::Body => AnnotationEdit::TranslateDimension {
                        delta_x: point.x - start.x,
                        delta_y: point.y - start.y,
                    },
                };
                document.apply_command(AnnotationCommand::EditAnnotation {
                    id: id.clone(),
                    edit,
                })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::CalloutEdit {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                expected_revision,
                kind,
                start,
                original,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                validate_callout_pointer_target(
                    document,
                    page_index,
                    &id,
                    expected_revision,
                    &original,
                )?;
                let edit = match kind {
                    CalloutPointerEditKind::LeaderPoint(point_index) => {
                        AnnotationEdit::SetCalloutLeaderPoint { point_index, point }
                    }
                    CalloutPointerEditKind::TextBox => AnnotationEdit::TranslateCalloutTextBox {
                        delta_x: point.x - start.x,
                        delta_y: point.y - start.y,
                    },
                    CalloutPointerEditKind::Body => AnnotationEdit::TranslateCalloutGroup {
                        delta_x: point.x - start.x,
                        delta_y: point.y - start.y,
                    },
                };
                document.apply_command(AnnotationCommand::EditAnnotation {
                    id: id.clone(),
                    edit,
                })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::CloudEdit {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                expected_revision,
                kind,
                start,
                original,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                validate_cloud_pointer_target(
                    document,
                    page_index,
                    &id,
                    expected_revision,
                    &original,
                )?;
                let edit = match kind {
                    CloudPointerEditKind::Vertex(vertex_index) => AnnotationEdit::SetCloudPoint {
                        vertex_index,
                        point,
                    },
                    CloudPointerEditKind::Body => AnnotationEdit::TranslateCloud {
                        delta_x: point.x - start.x,
                        delta_y: point.y - start.y,
                    },
                };
                document.apply_command(AnnotationCommand::EditAnnotation {
                    id: id.clone(),
                    edit,
                })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::LengthEndpoint {
                document_id,
                pointer_id: active_pointer,
                id,
                endpoint,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetLengthEndpoint { endpoint, point },
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::InkMove {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_paths,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                let delta_x = point.x - start.x;
                let delta_y = point.y - start.y;
                if delta_x == 0. && delta_y == 0. {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let paths = original_paths
                    .into_iter()
                    .map(|path| {
                        path.into_iter()
                            .map(|sample| PdfPoint::new(sample.x + delta_x, sample.y + delta_y))
                            .collect::<Result<Vec<_>, _>>()
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::ReplacePenPaths(paths),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::TextBoxMove {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rect = PdfRect::new(
                    original_rect.x + point.x - start.x,
                    original_rect.y + point.y - start.y,
                    original_rect.width,
                    original_rect.height,
                )?;
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetTextBoxLayoutRect(rect),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::TextBoxResize {
                document_id,
                pointer_id: active_pointer,
                id,
                handle,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rect = original_rect.rotated_resize_from_handle(0., handle, point);
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetTextBoxLayoutRect(rect),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::ImageMove {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rect = PdfRect::new(
                    original_rect.x + point.x - start.x,
                    original_rect.y + point.y - start.y,
                    original_rect.width,
                    original_rect.height,
                )?;
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetImageRect(rect),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::ImageResize {
                document_id,
                pointer_id: active_pointer,
                id,
                handle,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rect = resized_image_rect(original_rect, handle, start, point)?;
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetImageRect(rect),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::SnapshotMove {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rect = PdfRect::new(
                    original_rect.x + point.x - start.x,
                    original_rect.y + point.y - start.y,
                    original_rect.width,
                    original_rect.height,
                )?;
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetSnapshotRect(rect),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::SnapshotResize {
                document_id,
                pointer_id: active_pointer,
                id,
                handle,
                start,
                original_rect,
                original_rotation_degrees,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rect = original_rect.rotated_resize_from_handle(
                    original_rotation_degrees,
                    handle,
                    point,
                );
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetSnapshotRect(rect),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::SnapshotRotate {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                original_rotation_degrees,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                self.documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetSnapshotRotation(ellipse_rotation_from_drag(
                            original_rect,
                            original_rotation_degrees,
                            start,
                            point,
                        )),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
        }
    }

    pub fn cancel(&mut self, reason: PointerCancelReason) -> Result<(), AnnotationError> {
        if reason != PointerCancelReason::AdapterError {
            self.semantic_snap_decision = None;
        }
        self.vertex_path_draft = None;
        self.cloud_draft = None;
        self.cloud_plus_draft = None;
        self.measurement_path_draft = None;
        self.arc_draft = None;
        self.snapshot_draft = None;
        self.snapshot_capture_asset = None;
        let Some(active) = self.active.take() else {
            return Ok(());
        };
        if let ActivePointer::Domain {
            document_id,
            pointer_id,
            ..
        } = active
        {
            self.documents
                .get_mut(&document_id)
                .ok_or(AnnotationError::NoActiveGesture)?
                .apply_command(AnnotationCommand::PointerCancel { pointer_id, reason })?;
        }
        Ok(())
    }

    fn commit_pending_click(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let active = self.active.take().ok_or(AnnotationError::NoActiveGesture)?;
        match active {
            ActivePointer::Domain {
                document_id,
                pointer_id: active_pointer,
                click_placement_pending: true,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                let outcome = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::PointerUp { pointer_id, point })?;
                Ok(pointer_phase_outcome(outcome))
            }
            ActivePointer::EllipseCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                appearance,
                start,
                click_placement_pending: true,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                let end = if constrain_orthogonal {
                    EllipseAnnotation::constrained_end(start, point)
                } else {
                    point
                };
                self.commit_ellipse(document_id, page_index, id, appearance, start, end)
            }
            ActivePointer::RedactCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                click_placement_pending: true,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                self.commit_redact(document_id, page_index, id, start, point)
            }
            ActivePointer::StraightLineCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                kind,
                appearance,
                start,
                click_placement_pending: true,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                let end = constrained_line_point(start, point, constrain_orthogonal);
                self.commit_straight_line(document_id, page_index, id, kind, appearance, start, end)
            }
            ActivePointer::CalloutCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                click_placement_pending: true,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                let end = constrained_line_point(start, point, constrain_orthogonal);
                self.commit_callout(document_id, page_index, id, start, end)
            }
            active => {
                self.active = Some(active);
                Err(AnnotationError::NoActiveGesture)
            }
        }
    }

    fn commit_straight_line(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        kind: LineKind,
        appearance: StraightLineAppearance,
        start: PdfPoint,
        end: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let annotation =
            match StraightLineAnnotation::new(id.clone(), page_index, start, end, kind, appearance)
            {
                Ok(annotation) => annotation,
                Err(AnnotationError::InvalidGeometry(_)) => {
                    return Ok(PointerPhaseOutcome::Ignored);
                }
                Err(error) => return Err(error),
            };
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoActiveGesture)?
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(annotation),
            ))?;
        self.tool = AnnotationTool::Select;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    fn commit_callout(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let text_box = PdfRect::new(end.x, end.y - 22., 150., 44.)?;
        let connection = PdfPoint::new(text_box.x, text_box.y + text_box.height * 0.5)?;
        let knee = PdfPoint::new((start.x + connection.x) * 0.5, connection.y)?;
        let properties = self.tool_properties(AnnotationTool::Callout);
        let appearance = callout_tool_appearance(&properties)?;
        let annotation = match CalloutAnnotation::new(
            id.clone(),
            page_index,
            vec![start, knee, connection],
            text_box,
            "Callout",
            appearance,
        ) {
            Ok(annotation) => annotation,
            Err(AnnotationError::InvalidGeometry(_)) => {
                return Ok(PointerPhaseOutcome::Ignored);
            }
            Err(error) => return Err(error),
        };
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoActiveGesture)?
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Callout(
                annotation,
            )))?;
        self.tool = AnnotationTool::Select;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    fn commit_cloud_plus(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        cloud_points: Vec<PdfPoint>,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let properties = self.tool_properties(AnnotationTool::CloudPlus);
        let visible_path = cloud_visible_path(&cloud_points, properties.cloud_intensity)?;
        let placement = place_initial_cloud_plus_text_box(
            &cloud_points,
            &visible_path,
            CLOUD_PLUS_TEXT_WIDTH_PT,
            CLOUD_PLUS_TEXT_HEIGHT_PT,
            CLOUD_PLUS_TEXT_GAP_PT,
            &CloudPlusRoutingContext::default(),
        )?;
        let annotation = CloudPlusAnnotation::new(
            id.clone(),
            page_index,
            cloud_points,
            properties.cloud_intensity,
            placement.leader.points,
            placement.text_box,
            "Cloud+",
            cloud_plus_tool_appearance(&properties)?,
        )?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoActiveGesture)?
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::CloudPlus(
                annotation,
            )))?;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    fn commit_ellipse(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        appearance: RectangleAppearance,
        start: PdfPoint,
        end: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let annotation = match EllipseAnnotation::new(
            id.clone(),
            page_index,
            PdfRect::from_corners(start, end),
            appearance,
        ) {
            Ok(annotation) => annotation,
            Err(AnnotationError::InvalidGeometry(_)) => {
                return Ok(PointerPhaseOutcome::Ignored);
            }
            Err(error) => return Err(error),
        };
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoActiveGesture)?
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Ellipse(
                annotation,
            )))?;
        self.tool = AnnotationTool::Select;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    fn commit_redact(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let rect = PdfRect::from_corners(start, end);
        if rect.width <= 2. || rect.height <= 2. {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)?
            .with_fill_opacity(0.35)?;
        let annotation = RedactAnnotation::new(
            id.clone(),
            page_index,
            rect,
            "#000000",
            None::<String>,
            appearance,
        )?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoActiveGesture)?
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Redact(
                annotation,
            )))?;
        self.tool = AnnotationTool::Select;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    pub fn replace_selected_text(
        &mut self,
        document_id: u64,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetTextBoxContent(content.into()),
        })?;
        Ok(())
    }

    pub fn create_text_box(
        &mut self,
        document_id: u64,
        annotation: TextBoxAnnotation,
    ) -> Result<(), AnnotationError> {
        self.cancel(PointerCancelReason::ToolChanged)?;
        self.documents
            .entry(document_id)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::TextBox(
                annotation,
            )))?;
        Ok(())
    }

    pub fn clear_selection(&mut self, document_id: u64) {
        if let Some(document) = self.documents.get_mut(&document_id) {
            document.clear_selection();
        }
    }

    pub fn replace_selected_text_in_create_transaction(
        &mut self,
        document_id: u64,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.replace_text_box_content_in_create_transaction(&id, content)?;
        Ok(())
    }

    pub fn replace_callout_text_in_create_transaction(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        document.replace_callout_content_in_create_transaction(id, content)?;
        Ok(())
    }

    pub fn replace_cloud_plus_text_in_create_transaction(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let annotation = document
            .cloud_pluses()
            .iter()
            .find(|annotation| &annotation.id == id)
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let content = content.into();
        let normalized_content = content.replace("\r\n", "\n").replace('\r', "\n");
        let line_count = normalized_content.split('\n').count().max(1) as f64;
        let line_height = annotation.appearance.text().font_size_pt() * 1.15;
        let height = annotation
            .text_box
            .height
            .max(line_count * line_height + 12.);
        let connection = annotation.leader_points().last().copied();
        let existing_center_y = annotation.text_box.y + annotation.text_box.height * 0.5;
        let connects_to_vertical_side = connection.is_some_and(|connection| {
            (connection.x - annotation.text_box.x)
                .abs()
                .min((connection.x - (annotation.text_box.x + annotation.text_box.width)).abs())
                <= (connection.y - annotation.text_box.y).abs().min(
                    (connection.y - (annotation.text_box.y + annotation.text_box.height)).abs(),
                )
        });
        let center_y = if connects_to_vertical_side {
            connection
                .expect("a vertical-side connection was checked above")
                .y
        } else {
            existing_center_y
        };
        let text_box = PdfRect::new(
            annotation.text_box.x,
            center_y - height * 0.5,
            annotation.text_box.width,
            height,
        )?;
        let leader = route_cloud_plus_leader(
            annotation.cloud_points(),
            &annotation.scallop_path(),
            text_box,
            annotation.leader_points(),
            &CloudPlusRoutingContext::default(),
        )?;
        document.replace_cloud_plus_content_and_layout_in_create_transaction(
            id,
            content,
            text_box,
            leader.points,
        )?;
        Ok(())
    }

    pub fn select_id(&mut self, document_id: u64, id: &MarkupId) -> bool {
        self.documents
            .get_mut(&document_id)
            .is_some_and(|document| document.select(id))
    }

    pub fn toggle_selection(&mut self, document_id: u64, id: &MarkupId) -> bool {
        self.documents
            .get_mut(&document_id)
            .is_some_and(|document| document.toggle_selection(id))
    }

    pub fn selected_ids(&self, document_id: u64) -> &[MarkupId] {
        self.documents
            .get(&document_id)
            .map(AnnotationDocument::selected_ids)
            .unwrap_or_default()
    }

    /// Returns the current primary annotation without requiring the rest of
    /// the selection to be empty or changing its order.
    pub fn primary_selected_annotation(&self, document_id: u64) -> Option<Annotation> {
        let document = self.documents.get(&document_id)?;
        let primary_id = document.selected_id()?;
        document
            .selected_annotations_in_document_order()
            .into_iter()
            .find(|annotation| annotation.id() == primary_id)
    }

    /// Applies one ordinary history edit only when `expected_id` is still the
    /// primary selection. A stale target is reported as `NoSelection`.
    pub fn edit_primary_selected_annotation(
        &mut self,
        document_id: u64,
        expected_id: &MarkupId,
        edit: AnnotationEdit,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if document.selected_id() != Some(expected_id) {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id: expected_id.clone(),
            edit,
        })?;
        Ok(())
    }

    /// Changes lock state only when `expected_id` is still the primary
    /// selection, without temporarily narrowing a multi-selection.
    pub fn set_primary_selected_locked(
        &mut self,
        document_id: u64,
        expected_id: &MarkupId,
        locked: bool,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if document.selected_id() != Some(expected_id) {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::SetLocked {
            id: expected_id.clone(),
            locked,
        })?;
        Ok(())
    }

    pub fn selected_annotations_in_document_order(&self, document_id: u64) -> Vec<Annotation> {
        self.documents
            .get(&document_id)
            .map(AnnotationDocument::selected_annotations_in_document_order)
            .unwrap_or_default()
    }

    pub fn selected_has_unlocked(&self, document_id: u64) -> bool {
        self.documents
            .get(&document_id)
            .is_some_and(AnnotationDocument::selected_has_unlocked)
    }

    pub fn select_all_on_page(&mut self, document_id: u64, page_index: u32) -> &[MarkupId] {
        self.documents
            .entry(document_id)
            .or_default()
            .select_all_on_page(page_index)
    }

    pub fn insert_annotations(
        &mut self,
        document_id: u64,
        annotations: Vec<Annotation>,
    ) -> Result<Vec<MarkupId>, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .insert_annotations(annotations)
    }

    pub fn delete_selected_unlocked(
        &mut self,
        document_id: u64,
    ) -> Result<Vec<MarkupId>, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .delete_selected_unlocked()
    }

    pub fn set_selected_rectangle_appearance(
        &mut self,
        document_id: u64,
        appearance: RectangleAppearance,
    ) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::SetSelectedAppearance(appearance))?;
        Ok(())
    }

    pub fn set_selected_rectangle_rect(
        &mut self,
        document_id: u64,
        rect: PdfRect,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if !document
            .rectangles()
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetRectangleRect(rect),
        })?;
        Ok(())
    }

    pub fn set_selected_rectangle_rotation(
        &mut self,
        document_id: u64,
        rotation_degrees: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if !document
            .rectangles()
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetRectangleRotation(rotation_degrees),
        })?;
        Ok(())
    }

    pub fn set_ellipse_rect(
        &mut self,
        document_id: u64,
        id: MarkupId,
        rect: PdfRect,
    ) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::SetEllipseRect(rect),
            })?;
        Ok(())
    }

    pub fn translate_ellipse(
        &mut self,
        document_id: u64,
        id: MarkupId,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::TranslateEllipse { delta_x, delta_y },
            })?;
        Ok(())
    }

    pub fn set_ellipse_rotation(
        &mut self,
        document_id: u64,
        id: MarkupId,
        rotation_degrees: f64,
    ) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::SetEllipseRotation(rotation_degrees),
            })?;
        Ok(())
    }

    pub fn edit_selected_straight_line_property(
        &mut self,
        document_id: u64,
        edit: StraightLinePropertyEdit,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let current = document
            .straight_lines()
            .iter()
            .find(|annotation| annotation.id == id)
            .ok_or(AnnotationError::NoSelection)?
            .appearance
            .clone();
        let appearance = match edit {
            StraightLinePropertyEdit::StrokeColor(color) => StraightLineAppearance::new(
                color,
                current.stroke_width_pt(),
                current.opacity(),
                current.stroke_style(),
            )?,
            StraightLinePropertyEdit::StrokeWidthPt(width) => StraightLineAppearance::new(
                current.stroke_color(),
                width,
                current.opacity(),
                current.stroke_style(),
            )?,
            StraightLinePropertyEdit::Opacity(opacity) => StraightLineAppearance::new(
                current.stroke_color(),
                current.stroke_width_pt(),
                opacity,
                current.stroke_style(),
            )?,
        };
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetStraightLineAppearance(appearance),
        })?;
        Ok(())
    }

    pub fn edit_selected_vertex_path_property(
        &mut self,
        document_id: u64,
        edit: VertexPathPropertyEdit,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::VertexPath(annotation)] = selected.as_slice() else {
            return Err(AnnotationError::NoSelection);
        };
        let id = annotation.id.clone();
        let current = annotation.appearance.clone();
        match &edit {
            VertexPathPropertyEdit::StrokeWidthPt(value)
                if !value.is_finite() || !(0.25..=24.).contains(value) =>
            {
                return Err(AnnotationError::InvalidAppearance(
                    "vertex-path stroke width must be between 0.25 and 24 points".into(),
                ));
            }
            VertexPathPropertyEdit::Opacity(value)
                if !value.is_finite() || !(0.0..=1.0).contains(value) =>
            {
                return Err(AnnotationError::InvalidAppearance(
                    "vertex-path opacity must be between 0 and 1".into(),
                ));
            }
            VertexPathPropertyEdit::FillColor(_) if annotation.kind == VertexPathKind::Polyline => {
                return Err(AnnotationError::InvalidAppearance(
                    "polyline annotations do not support fill property edits".into(),
                ));
            }
            _ => {}
        }
        let (stroke_color, stroke_width, fill_color, opacity) = match edit {
            VertexPathPropertyEdit::StrokeColor(value) => (
                vertex_path_property_rgb(value),
                current.stroke_width_pt(),
                current.fill_color().map(str::to_owned),
                current.opacity(),
            ),
            VertexPathPropertyEdit::StrokeWidthPt(value) => (
                current.stroke_color().to_owned(),
                value,
                current.fill_color().map(str::to_owned),
                current.opacity(),
            ),
            VertexPathPropertyEdit::Opacity(value) => (
                current.stroke_color().to_owned(),
                current.stroke_width_pt(),
                current.fill_color().map(str::to_owned),
                value,
            ),
            VertexPathPropertyEdit::FillColor(value) => (
                current.stroke_color().to_owned(),
                current.stroke_width_pt(),
                value.map(vertex_path_property_rgb),
                current.opacity(),
            ),
        };
        let appearance = RectangleAppearance::new(stroke_color, stroke_width, fill_color, opacity)?
            .with_fill_opacity(current.fill_opacity())?
            .with_stroke_style(current.stroke_style());
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetVertexPathAppearance(appearance),
        })?;
        Ok(())
    }

    pub fn edit_selected_measurement_path_property(
        &mut self,
        document_id: u64,
        edit: VertexPathPropertyEdit,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::MeasurementPath(annotation)] = selected.as_slice() else {
            return Err(AnnotationError::NoSelection);
        };
        let current = annotation.appearance.clone();
        match &edit {
            VertexPathPropertyEdit::StrokeWidthPt(value)
                if !value.is_finite() || !(0.25..=24.).contains(value) =>
            {
                return Err(AnnotationError::InvalidAppearance(
                    "measurement-path stroke width must be between 0.25 and 24 points".into(),
                ));
            }
            VertexPathPropertyEdit::Opacity(value)
                if !value.is_finite() || !(0.0..=1.0).contains(value) =>
            {
                return Err(AnnotationError::InvalidAppearance(
                    "measurement-path opacity must be between 0 and 1".into(),
                ));
            }
            VertexPathPropertyEdit::FillColor(_)
                if annotation.kind == MeasurementPathKind::Polylength =>
            {
                return Err(AnnotationError::InvalidAppearance(
                    "polylength annotations do not support fill property edits".into(),
                ));
            }
            _ => {}
        }
        let (stroke_color, stroke_width, fill_color, opacity) = match edit {
            VertexPathPropertyEdit::StrokeColor(value) => (
                vertex_path_property_rgb(value),
                current.stroke_width_pt(),
                current.fill_color().map(str::to_owned),
                current.opacity(),
            ),
            VertexPathPropertyEdit::StrokeWidthPt(value) => (
                current.stroke_color().to_owned(),
                value,
                current.fill_color().map(str::to_owned),
                current.opacity(),
            ),
            VertexPathPropertyEdit::Opacity(value) => (
                current.stroke_color().to_owned(),
                current.stroke_width_pt(),
                current.fill_color().map(str::to_owned),
                value,
            ),
            VertexPathPropertyEdit::FillColor(value) => (
                current.stroke_color().to_owned(),
                current.stroke_width_pt(),
                value.map(vertex_path_property_rgb),
                current.opacity(),
            ),
        };
        let appearance = RectangleAppearance::new(stroke_color, stroke_width, fill_color, opacity)?
            .with_fill_opacity(current.fill_opacity())?
            .with_stroke_style(current.stroke_style());
        document.apply_command(AnnotationCommand::SetSelectedAppearance(appearance))?;
        Ok(())
    }

    pub fn move_selected_ink(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let paths = document
            .pens()
            .iter()
            .find(|annotation| annotation.id == id)
            .ok_or(AnnotationError::NoSelection)?
            .paths()
            .map(|path| {
                path.iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::ReplacePenPaths(paths),
        })?;
        Ok(())
    }

    pub fn set_selected_ink_opacity(
        &mut self,
        document_id: u64,
        opacity: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let pen = document
            .pens()
            .iter()
            .find(|annotation| annotation.id == id)
            .ok_or(AnnotationError::NoSelection)?;
        let appearance =
            PenAppearance::new(pen.appearance.color(), pen.appearance.width_pt(), opacity)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetInkAppearance(appearance),
        })?;
        Ok(())
    }

    /// Replaces only the appearance of exactly one selected Pen or Highlight.
    /// The model command retains every path plus tool, smoothing, blend, and
    /// lock state, and records at most one history entry.
    pub fn set_exact_selected_ink_appearance(
        &mut self,
        document_id: u64,
        appearance: PenAppearance,
    ) -> Result<(), AnnotationError> {
        let id = self
            .exact_selected_ink(document_id)
            .map(|pen| pen.id.clone())
            .ok_or(AnnotationError::NoSelection)?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::SetInkAppearance(appearance),
            })?;
        Ok(())
    }

    /// Replaces the complete style of exactly one selected Text Box.
    pub fn set_exact_selected_text_box_style(
        &mut self,
        document_id: u64,
        style: TextBoxStyle,
    ) -> Result<(), AnnotationError> {
        let id = self
            .exact_selected_text_box(document_id)
            .map(|text_box| text_box.id.clone())
            .ok_or(AnnotationError::NoSelection)?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::SetTextBoxStyle(style),
            })?;
        Ok(())
    }

    /// Replaces the complete appearance of exactly one selected Arc.
    pub fn set_exact_selected_arc_appearance(
        &mut self,
        document_id: u64,
        appearance: RectangleAppearance,
    ) -> Result<(), AnnotationError> {
        self.exact_selected_arc(document_id)
            .ok_or(AnnotationError::NoSelection)?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::SetSelectedAppearance(appearance))?;
        Ok(())
    }

    /// Replaces the complete appearance of exactly one selected Cloud.
    pub fn set_exact_selected_cloud_appearance(
        &mut self,
        document_id: u64,
        appearance: RectangleAppearance,
    ) -> Result<(), AnnotationError> {
        let id = self
            .exact_selected_cloud(document_id)
            .map(|cloud| cloud.id.clone())
            .ok_or(AnnotationError::NoSelection)?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::SetCloudAppearance(appearance),
            })?;
        Ok(())
    }

    /// Replaces intensity on exactly one selected Cloud.
    pub fn set_exact_selected_cloud_intensity(
        &mut self,
        document_id: u64,
        intensity: f64,
    ) -> Result<(), AnnotationError> {
        let id = self
            .exact_selected_cloud(document_id)
            .map(|cloud| cloud.id.clone())
            .ok_or(AnnotationError::NoSelection)?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::SetCloudIntensity(intensity),
            })?;
        Ok(())
    }

    /// Replaces opacity on exactly one selected Snapshot.
    pub fn set_exact_selected_snapshot_opacity(
        &mut self,
        document_id: u64,
        opacity: f64,
    ) -> Result<(), AnnotationError> {
        let id = self
            .exact_selected_snapshot(document_id)
            .map(|snapshot| snapshot.id.clone())
            .ok_or(AnnotationError::NoSelection)?;
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id,
                edit: AnnotationEdit::SetSnapshotOpacity(opacity),
            })?;
        Ok(())
    }

    pub fn resize_selected_text(
        &mut self,
        document_id: u64,
        width: f64,
        height: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let current = document
            .text_boxes()
            .iter()
            .find(|annotation| annotation.id == id)
            .ok_or(AnnotationError::NoSelection)?
            .layout_rect;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetTextBoxLayoutRect(PdfRect::new(
                current.x, current.y, width, height,
            )?),
        })?;
        Ok(())
    }

    pub fn move_selected_text(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let current = document
            .text_boxes()
            .iter()
            .find(|annotation| annotation.id == id)
            .ok_or(AnnotationError::NoSelection)?
            .layout_rect;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetTextBoxLayoutRect(PdfRect::new(
                current.x + delta_x,
                current.y + delta_y,
                current.width,
                current.height,
            )?),
        })?;
        Ok(())
    }

    pub fn move_selected_length(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if !document
            .lengths()
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::TranslateLength { delta_x, delta_y },
        })?;
        Ok(())
    }

    pub fn set_selected_length_endpoint(
        &mut self,
        document_id: u64,
        endpoint: LengthEndpoint,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if !document
            .lengths()
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetLengthEndpoint { endpoint, point },
        })?;
        Ok(())
    }

    pub fn resize_selected_image(
        &mut self,
        document_id: u64,
        width: f64,
        height: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let current = document
            .images()
            .iter()
            .find(|annotation| annotation.id == id)
            .ok_or(AnnotationError::NoSelection)?
            .rect;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetImageRect(PdfRect::new(current.x, current.y, width, height)?),
        })?;
        Ok(())
    }

    pub fn set_selected_image_rect(
        &mut self,
        document_id: u64,
        rect: PdfRect,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if !document
            .images()
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetImageRect(rect),
        })?;
        Ok(())
    }

    pub fn set_selected_locked(
        &mut self,
        document_id: u64,
        locked: bool,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::SetLocked { id, locked })?;
        Ok(())
    }

    pub fn set_selected_snapshot_rotation(
        &mut self,
        document_id: u64,
        rotation_degrees: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if !document
            .snapshots()
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetSnapshotRotation(rotation_degrees),
        })?;
        Ok(())
    }

    pub fn set_selected_snapshot_opacity(
        &mut self,
        document_id: u64,
        opacity: f64,
    ) -> Result<(), AnnotationError> {
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let id = document
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if !document
            .snapshots()
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return Err(AnnotationError::NoSelection);
        }
        document.apply_command(AnnotationCommand::EditAnnotation {
            id,
            edit: AnnotationEdit::SetSnapshotOpacity(opacity),
        })?;
        Ok(())
    }

    pub fn delete_selected(&mut self, document_id: u64) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::DeleteSelected)?;
        Ok(())
    }

    pub fn undo(&mut self, document_id: u64) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::Undo)?;
        Ok(())
    }

    pub fn redo(&mut self, document_id: u64) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::Redo)?;
        Ok(())
    }

    pub fn document_scene(&self, document_id: u64, page_index: u32) -> AnnotationScene {
        let mut scene = self
            .documents
            .get(&document_id)
            .map(|document| document.document_scene(page_index))
            .unwrap_or_else(|| empty_scene(page_index));
        if let Some(draft) = &self.arc_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
            && let Some(end) = draft.end
            && let Ok(annotation) = ArcAnnotation::new(
                draft.id.clone(),
                page_index,
                draft.start,
                end,
                draft.mid,
                draft.appearance.clone(),
            )
        {
            scene.arcs.push(SceneArc {
                id: annotation.id.clone(),
                start: annotation.start,
                end: annotation.end,
                mid: annotation.mid,
                sampled_path: annotation.sampled_path(64),
                appearance: annotation.appearance,
                selected: true,
                locked: false,
                draft: true,
            });
        }
        if let Some(draft) = &self.snapshot_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
        {
            let preview_asset = self.snapshot_capture_asset.clone().unwrap_or_else(|| {
                DecodedRgbaAsset::new(1, 1, vec![0; 4])
                    .expect("the transparent Snapshot draft marker is a valid RGBA asset")
            });
            scene.snapshots.push(SceneSnapshot {
                id: draft.id.clone(),
                body_id: SNAPSHOT_BODY_ID,
                rect: PdfRect::from_corners(draft.start, draft.current),
                asset_id: preview_asset.id().clone(),
                width_px: preview_asset.width_px(),
                height_px: preview_asset.height_px(),
                opacity: self.tool_properties(AnnotationTool::Snapshot).opacity,
                rotation_degrees: 0.,
                selected: true,
                locked: false,
                draft: true,
            });
        }
        if let Some(ActivePointer::RedactCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
        {
            scene.redacts.push(SceneRedact {
                id: id.clone(),
                rect: PdfRect::from_corners(*start, *current),
                appearance: RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
                    .expect("the frozen pending Redact preview appearance is valid")
                    .with_fill_opacity(0.35)
                    .expect("the frozen pending Redact fill opacity is valid"),
                selected: true,
                locked: false,
                draft: true,
                body_id: REDACT_BODY_ID,
            });
        }
        if let Some(ActivePointer::GroupMove {
            document_id: active_document_id,
            page_index: active_page_index,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
        {
            let delta_x = current.x - start.x;
            let delta_y = current.y - start.y;
            for annotation in scene
                .rectangles
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.rect = PdfRect::new(
                    annotation.rect.x + delta_x,
                    annotation.rect.y + delta_y,
                    annotation.rect.width,
                    annotation.rect.height,
                )
                .expect("validated pointer points produce a finite group Rectangle preview");
                annotation.preview = true;
            }
            for annotation in scene
                .ellipses
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.rect = PdfRect::new(
                    annotation.rect.x + delta_x,
                    annotation.rect.y + delta_y,
                    annotation.rect.width,
                    annotation.rect.height,
                )
                .expect("validated pointer points produce a finite group Ellipse preview");
                annotation.preview = true;
            }
            for annotation in scene
                .redacts
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.rect = PdfRect::new(
                    annotation.rect.x + delta_x,
                    annotation.rect.y + delta_y,
                    annotation.rect.width,
                    annotation.rect.height,
                )
                .expect("validated pointer points produce a finite group Redact preview");
                annotation.draft = true;
            }
            for annotation in scene
                .arcs
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                for point in [
                    &mut annotation.start,
                    &mut annotation.end,
                    &mut annotation.mid,
                ] {
                    point.x += delta_x;
                    point.y += delta_y;
                }
                for point in &mut annotation.sampled_path {
                    point.x += delta_x;
                    point.y += delta_y;
                }
                annotation.draft = true;
            }
            for annotation in scene
                .straight_lines
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.start =
                    PdfPoint::new(annotation.start.x + delta_x, annotation.start.y + delta_y)
                        .expect("validated pointer points produce a finite group Line preview");
                annotation.end =
                    PdfPoint::new(annotation.end.x + delta_x, annotation.end.y + delta_y)
                        .expect("validated pointer points produce a finite group Line preview");
                annotation.draft = true;
            }
            for annotation in scene
                .vertex_paths
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                for point in &mut annotation.points {
                    *point = PdfPoint::new(point.x + delta_x, point.y + delta_y)
                        .expect("validated pointer points produce a finite vertex-path preview");
                }
                annotation.draft = true;
            }
            for annotation in scene
                .measurement_paths
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                for point in &mut annotation.points {
                    *point = PdfPoint::new(point.x + delta_x, point.y + delta_y).expect(
                        "validated pointer points produce a finite measurement-path preview",
                    );
                }
                annotation.draft = true;
            }
            for annotation in scene
                .pens
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                for point in &mut annotation.points {
                    *point = PdfPoint::new(point.x + delta_x, point.y + delta_y)
                        .expect("validated pointer points produce a finite group Ink preview");
                }
                for path in &mut annotation.paths {
                    for point in path {
                        *point = PdfPoint::new(point.x + delta_x, point.y + delta_y).expect(
                            "validated pointer points produce a finite group Ink-path preview",
                        );
                    }
                }
                annotation.draft = true;
            }
            for annotation in scene
                .text_boxes
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.layout_rect = PdfRect::new(
                    annotation.layout_rect.x + delta_x,
                    annotation.layout_rect.y + delta_y,
                    annotation.layout_rect.width,
                    annotation.layout_rect.height,
                )
                .expect("validated pointer points produce a finite group Text preview");
            }
            for annotation in scene
                .lengths
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.start =
                    PdfPoint::new(annotation.start.x + delta_x, annotation.start.y + delta_y)
                        .expect("validated pointer points produce a finite group Length preview");
                annotation.end =
                    PdfPoint::new(annotation.end.x + delta_x, annotation.end.y + delta_y)
                        .expect("validated pointer points produce a finite group Length preview");
            }
            for annotation in scene
                .images
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.rect = PdfRect::new(
                    annotation.rect.x + delta_x,
                    annotation.rect.y + delta_y,
                    annotation.rect.width,
                    annotation.rect.height,
                )
                .expect("validated pointer points produce a finite group Image preview");
            }
            for annotation in scene
                .snapshots
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                annotation.rect = PdfRect::new(
                    annotation.rect.x + delta_x,
                    annotation.rect.y + delta_y,
                    annotation.rect.width,
                    annotation.rect.height,
                )
                .expect("validated pointer points produce a finite group Snapshot preview");
                annotation.draft = true;
            }
        }
        if let Some(ActivePointer::EllipseMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .ellipses
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rect = PdfRect::new(
                original_rect.x + current.x - start.x,
                original_rect.y + current.y - start.y,
                original_rect.width,
                original_rect.height,
            )
            .expect("validated pointer points produce a finite Ellipse move preview");
            annotation.preview = true;
        }
        if let Some(ActivePointer::RedactMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .redacts
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rect = PdfRect::new(
                original_rect.x + current.x - start.x,
                original_rect.y + current.y - start.y,
                original_rect.width,
                original_rect.height,
            )
            .expect("validated pointer points produce a finite Redact move preview");
            annotation.draft = true;
        }
        if let Some(ActivePointer::RedactResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .redacts
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            if let Ok(rect) = redact_resized_rect(*original_rect, *handle, *current) {
                annotation.rect = rect;
                annotation.draft = true;
            }
        }
        if let Some(ActivePointer::EllipseResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            current,
            original_rect,
            original_rotation_degrees,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .ellipses
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rect = ellipse_resized_rect(
                *original_rect,
                *original_rotation_degrees,
                *handle,
                *current,
            );
            annotation.preview = true;
        }
        if let Some(ActivePointer::EllipseRotate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_rect,
            original_rotation_degrees,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .ellipses
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rotation_degrees = ellipse_rotation_from_drag(
                *original_rect,
                *original_rotation_degrees,
                *start,
                *current,
            );
            annotation.preview = true;
        }
        if let Some(ActivePointer::ArcMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            expected_revision,
            start,
            current,
            original,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && self.documents.get(&document_id).is_some_and(|document| {
                validate_arc_pointer_target(document, page_index, id, *expected_revision, original)
                    .is_ok()
            })
            && let Some(annotation) = scene
                .arcs
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let delta_x = current.x - start.x;
            let delta_y = current.y - start.y;
            annotation.start =
                PdfPoint::new(original.start.x + delta_x, original.start.y + delta_y)
                    .expect("validated pointer points produce a finite Arc preview");
            annotation.end = PdfPoint::new(original.end.x + delta_x, original.end.y + delta_y)
                .expect("validated pointer points produce a finite Arc preview");
            annotation.mid = PdfPoint::new(original.mid.x + delta_x, original.mid.y + delta_y)
                .expect("validated pointer points produce a finite Arc preview");
            annotation.sampled_path = ArcAnnotation::new(
                annotation.id.clone(),
                page_index,
                annotation.start,
                annotation.end,
                annotation.mid,
                annotation.appearance.clone(),
            )
            .expect("translated Arc preview geometry remains valid")
            .sampled_path(64);
            annotation.draft = true;
        }
        if let Some(ActivePointer::ArcControlPoint {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            expected_revision,
            control,
            current,
            original,
            snap_quarter_turn,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && self.documents.get(&document_id).is_some_and(|document| {
                validate_arc_pointer_target(document, page_index, id, *expected_revision, original)
                    .is_ok()
            })
            && let Some(annotation) = scene
                .arcs
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let resolved = resolve_arc_control_point(
                original,
                *control,
                *current,
                ARC_MINIMUM_BULGE_CSS_PX / self.observed_pixels_per_point.0,
                *snap_quarter_turn,
            )
            .unwrap_or(*current);
            let (start, end, mid) = match *control {
                ArcControlPoint::Start => (resolved, original.end, original.mid),
                ArcControlPoint::Mid => (original.start, original.end, resolved),
                ArcControlPoint::End => (original.start, resolved, original.mid),
            };
            if let Ok(mut preview) = ArcAnnotation::new(
                annotation.id.clone(),
                page_index,
                start,
                end,
                mid,
                original.appearance.clone(),
            ) {
                preview.locked = original.locked;
                annotation.start = preview.start;
                annotation.end = preview.end;
                annotation.mid = preview.mid;
                annotation.sampled_path = preview.sampled_path(64);
                annotation.draft = true;
            }
        }
        if let Some(ActivePointer::StraightLineCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            kind,
            appearance,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && start != current
        {
            scene.straight_lines.push(SceneStraightLine {
                id: id.clone(),
                start: *start,
                end: *current,
                kind: *kind,
                appearance: appearance.clone(),
                selected: true,
                locked: false,
                draft: true,
            });
        }
        if let Some(ActivePointer::CalloutCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && start != current
        {
            let properties = self.tool_properties(AnnotationTool::Callout);
            let text_box = PdfRect {
                x: current.x,
                y: current.y - 22.,
                width: 150.,
                height: 44.,
            };
            let connection = PdfPoint {
                x: text_box.x,
                y: text_box.y + text_box.height * 0.5,
            };
            scene.callouts.push(SceneCallout {
                id: id.clone(),
                leader_points: vec![
                    *start,
                    PdfPoint {
                        x: (start.x + connection.x) * 0.5,
                        y: connection.y,
                    },
                    connection,
                ],
                text_box,
                content: "Callout".into(),
                appearance: callout_tool_appearance(&properties)
                    .expect("stored Callout tool properties are validated"),
                selected: true,
                locked: false,
                draft: true,
            });
        }
        if let Some(ActivePointer::CloudPlusCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && start != current
        {
            let properties = self.tool_properties(AnnotationTool::CloudPlus);
            let rect = PdfRect::from_corners(*start, *current);
            let cloud_points = vec![
                PdfPoint {
                    x: rect.x,
                    y: rect.y,
                },
                PdfPoint {
                    x: rect.x + rect.width,
                    y: rect.y,
                },
                PdfPoint {
                    x: rect.x + rect.width,
                    y: rect.y + rect.height,
                },
                PdfPoint {
                    x: rect.x,
                    y: rect.y + rect.height,
                },
            ];
            if let Ok(visible_path) = cloud_visible_path(&cloud_points, properties.cloud_intensity)
                && let Ok(placement) = place_initial_cloud_plus_text_box(
                    &cloud_points,
                    &visible_path,
                    CLOUD_PLUS_TEXT_WIDTH_PT,
                    CLOUD_PLUS_TEXT_HEIGHT_PT,
                    CLOUD_PLUS_TEXT_GAP_PT,
                    &CloudPlusRoutingContext::default(),
                )
                && let Ok(appearance) = cloud_plus_tool_appearance(&properties)
            {
                scene.cloud_pluses.push(SceneCloudPlus {
                    id: id.clone(),
                    cloud_points,
                    scallop_path: visible_path,
                    border_effect_intensity: properties.cloud_intensity,
                    leader_points: placement.leader.points,
                    text_box: placement.text_box,
                    content: "Cloud+".into(),
                    appearance,
                    selected: true,
                    locked: false,
                    draft: true,
                });
            }
        }
        if let Some(ActivePointer::StraightLineMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_start,
            original_end,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .straight_lines
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let delta_x = current.x - start.x;
            let delta_y = current.y - start.y;
            annotation.start =
                PdfPoint::new(original_start.x + delta_x, original_start.y + delta_y)
                    .expect("validated pointer points produce a finite line preview");
            annotation.end = PdfPoint::new(original_end.x + delta_x, original_end.y + delta_y)
                .expect("validated pointer points produce a finite line preview");
            annotation.draft = true;
        }
        if let Some(draft) = &self.vertex_path_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
        {
            let tool = match draft.kind {
                VertexPathKind::Polyline => AnnotationTool::Polyline,
                VertexPathKind::Polygon => AnnotationTool::Polygon,
            };
            let properties = self.tool_properties(tool);
            let mut points = draft.points.clone();
            if point_distance_css_px(
                *points
                    .last()
                    .expect("a vertex draft retains its first point"),
                draft.hover,
                self.observed_pixels_per_point.0,
            ) >= 0.5 * self.observed_pixels_per_point.0
            {
                points.push(draft.hover);
            }
            scene.vertex_paths.push(SceneVertexPath {
                id: draft.id.clone(),
                points,
                kind: draft.kind,
                appearance: rectangle_tool_appearance(
                    &properties,
                    draft.kind == VertexPathKind::Polygon,
                )
                .expect("stored vertex-path tool properties are validated"),
                selected: true,
                locked: false,
                draft: true,
            });
        }
        if let Some(draft) = &self.cloud_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
        {
            let properties = self.tool_properties(AnnotationTool::Cloud);
            let mut points = draft.points.clone();
            if point_distance_css_px(
                *points
                    .last()
                    .expect("a cloud draft retains its first point"),
                draft.hover,
                self.observed_pixels_per_point.0,
            ) >= 0.5 * self.observed_pixels_per_point.0
            {
                points.push(draft.hover);
            }
            let appearance = rectangle_tool_appearance(&properties, false)
                .expect("stored Cloud tool properties are validated");
            let scallop_path = if points.len() >= 3 {
                CloudAnnotation::new(
                    draft.id.clone(),
                    draft.page_index,
                    points.clone(),
                    properties.cloud_intensity,
                    appearance.clone(),
                )
                .expect("a cloud draft with three validated points must build")
                .scallop_path()
            } else {
                points.clone()
            };
            scene.clouds.push(SceneCloud {
                id: draft.id.clone(),
                points,
                scallop_path,
                border_effect_intensity: properties.cloud_intensity,
                appearance,
                selected: true,
                locked: false,
                draft: true,
            });
        }
        if let Some(draft) = &self.cloud_plus_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
        {
            let properties = self.tool_properties(AnnotationTool::CloudPlus);
            let mut cloud_points = draft.points.clone();
            if point_distance_css_px(
                *cloud_points
                    .last()
                    .expect("a Cloud+ draft retains its first point"),
                draft.hover,
                self.observed_pixels_per_point.0,
            ) >= 0.5 * self.observed_pixels_per_point.0
            {
                cloud_points.push(draft.hover);
            }
            if cloud_points.len() >= 3
                && let Ok(visible_path) =
                    cloud_visible_path(&cloud_points, properties.cloud_intensity)
                && let Ok(placement) = place_initial_cloud_plus_text_box(
                    &cloud_points,
                    &visible_path,
                    CLOUD_PLUS_TEXT_WIDTH_PT,
                    CLOUD_PLUS_TEXT_HEIGHT_PT,
                    CLOUD_PLUS_TEXT_GAP_PT,
                    &CloudPlusRoutingContext::default(),
                )
                && let Ok(appearance) = cloud_plus_tool_appearance(&properties)
            {
                scene.cloud_pluses.push(SceneCloudPlus {
                    id: draft.id.clone(),
                    cloud_points,
                    scallop_path: visible_path,
                    border_effect_intensity: properties.cloud_intensity,
                    leader_points: placement.leader.points,
                    text_box: placement.text_box,
                    content: "Cloud+".into(),
                    appearance,
                    selected: true,
                    locked: false,
                    draft: true,
                });
            }
        }
        if let Some(draft) = &self.measurement_path_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
        {
            let tool = match draft.kind {
                MeasurementPathKind::Polylength => AnnotationTool::Polylength,
                MeasurementPathKind::Area => AnnotationTool::Area,
            };
            let appearance = rectangle_tool_appearance(&self.tool_properties(tool), false)
                .expect("stored measurement-path tool properties are validated");
            let text_style = text_box_tool_style(&self.tool_properties(tool))
                .expect("stored measurement-path tool properties are validated");
            let mut points = draft.points.clone();
            let last = *points
                .last()
                .expect("a measurement draft retains its first point");
            if (draft.hover.x - last.x).hypot(draft.hover.y - last.y) >= 0.5 {
                points.push(draft.hover);
            }
            let measured = MeasurementPathAnnotation::new_with_text_style(
                draft.id.clone(),
                draft.page_index,
                points.clone(),
                draft.kind,
                draft.calibration.clone(),
                appearance.clone(),
                text_style.clone(),
            )
            .ok();
            scene.measurement_paths.push(SceneMeasurementPath {
                id: draft.id.clone(),
                points,
                kind: draft.kind,
                appearance,
                text_style,
                caption: measured.map_or_else(String::new, |annotation| annotation.caption()),
                show_caption: draft.calibration.show_caption(),
                selected: true,
                locked: false,
                draft: true,
            });
        }
        if let Some(ActivePointer::VertexPathPoint {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            vertex_index,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .vertex_paths
                .iter_mut()
                .find(|annotation| annotation.id == *id)
            && let Some(vertex) = annotation.points.get_mut(*vertex_index)
        {
            *vertex = *current;
            annotation.draft = true;
        }
        if let Some(ActivePointer::MeasurementPathPoint {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            vertex_index,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .measurement_paths
                .iter_mut()
                .find(|annotation| annotation.id == *id)
            && let Some(vertex) = annotation.points.get_mut(*vertex_index)
        {
            *vertex = *current;
            annotation.draft = true;
        }
        if let Some(ActivePointer::StraightLineEndpoint {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            endpoint,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .straight_lines
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            match endpoint {
                LineEndpoint::Start => annotation.start = *current,
                LineEndpoint::End => annotation.end = *current,
            }
            annotation.draft = true;
        }
        if let Some(ActivePointer::InkMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_paths,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .pens
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let delta_x = current.x - start.x;
            let delta_y = current.y - start.y;
            annotation.paths = original_paths
                .iter()
                .map(|path| {
                    path.iter()
                        .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y).unwrap())
                        .collect()
                })
                .collect();
            annotation.points = annotation.paths.first().cloned().unwrap_or_default();
        }
        if let Some(ActivePointer::TextBoxMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .text_boxes
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.layout_rect = PdfRect::new(
                original_rect.x + current.x - start.x,
                original_rect.y + current.y - start.y,
                original_rect.width,
                original_rect.height,
            )
            .expect("validated pointer points produce a finite Text Box preview");
        }
        if let Some(ActivePointer::TextBoxResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .text_boxes
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.layout_rect =
                original_rect.rotated_resize_from_handle(0., *handle, *current);
        }
        if let Some(ActivePointer::ImageMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .images
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rect.x = original_rect.x + current.x - start.x;
            annotation.rect.y = original_rect.y + current.y - start.y;
        }
        if let Some(ActivePointer::ImageResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            start,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .images
                .iter_mut()
                .find(|annotation| annotation.id == *id)
            && let Ok(rect) = resized_image_rect(*original_rect, *handle, *start, *current)
        {
            annotation.rect = rect;
        }
        if let Some(ActivePointer::SnapshotMove {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_rect,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .snapshots
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rect = PdfRect::new(
                original_rect.x + current.x - start.x,
                original_rect.y + current.y - start.y,
                original_rect.width,
                original_rect.height,
            )
            .expect("validated pointer points produce a finite Snapshot move preview");
            annotation.draft = true;
        }
        if let Some(ActivePointer::SnapshotResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            current,
            original_rect,
            original_rotation_degrees,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .snapshots
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rect = original_rect.rotated_resize_from_handle(
                *original_rotation_degrees,
                *handle,
                *current,
            );
            annotation.draft = true;
        }
        if let Some(ActivePointer::SnapshotRotate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            original_rect,
            original_rotation_degrees,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .snapshots
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rotation_degrees = ellipse_rotation_from_drag(
                *original_rect,
                *original_rotation_degrees,
                *start,
                *current,
            );
            annotation.draft = true;
        }
        if let Some(ActivePointer::DimensionEdit {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            kind,
            start,
            current,
            original,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .dimensions
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let mut preview_start = original.start;
            let mut preview_end = original.end;
            let mut preview_offset = original.dimension_line_offset();
            match kind {
                DimensionPointerEditKind::Start => preview_start = *current,
                DimensionPointerEditKind::End => preview_end = *current,
                DimensionPointerEditKind::Offset => {
                    let delta_x = original.end.x - original.start.x;
                    let delta_y = original.end.y - original.start.y;
                    let length = delta_x.hypot(delta_y);
                    preview_offset += (current.x - start.x) * (-delta_y / length)
                        + (current.y - start.y) * (delta_x / length);
                }
                DimensionPointerEditKind::Body => {
                    let delta_x = current.x - start.x;
                    let delta_y = current.y - start.y;
                    preview_start.x += delta_x;
                    preview_start.y += delta_y;
                    preview_end.x += delta_x;
                    preview_end.y += delta_y;
                }
            }
            if DimensionAnnotation::new(
                original.id.clone(),
                original.page_index,
                preview_start,
                preview_end,
                preview_offset,
                original.content(),
                original.appearance.clone(),
            )
            .is_ok()
            {
                annotation.start = preview_start;
                annotation.end = preview_end;
                annotation.dimension_line_offset = preview_offset;
                annotation.draft = true;
            }
        }
        if let Some(ActivePointer::CalloutEdit {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            kind,
            start,
            current,
            original,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .callouts
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let delta_x = current.x - start.x;
            let delta_y = current.y - start.y;
            let mut leader_points = original.leader_points().to_vec();
            let mut text_box = original.text_box;
            match kind {
                CalloutPointerEditKind::LeaderPoint(index) => {
                    if let Some(point) = leader_points.get_mut(*index) {
                        *point = *current;
                    }
                }
                CalloutPointerEditKind::TextBox => {
                    text_box.x += delta_x;
                    text_box.y += delta_y;
                    if let Some(connection) = leader_points.last_mut() {
                        connection.x += delta_x;
                        connection.y += delta_y;
                    }
                }
                CalloutPointerEditKind::Body => {
                    text_box.x += delta_x;
                    text_box.y += delta_y;
                    for point in &mut leader_points {
                        point.x += delta_x;
                        point.y += delta_y;
                    }
                }
            }
            if CalloutAnnotation::new(
                original.id.clone(),
                original.page_index,
                leader_points.clone(),
                text_box,
                original.content(),
                original.appearance.clone(),
            )
            .is_ok()
            {
                annotation.leader_points = leader_points;
                annotation.text_box = text_box;
                annotation.draft = true;
            }
        }
        if let Some(ActivePointer::CloudEdit {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            kind,
            start,
            current,
            original,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .clouds
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let mut points = original.points().to_vec();
            match kind {
                CloudPointerEditKind::Vertex(index) => {
                    if let Some(point) = points.get_mut(*index) {
                        *point = *current;
                    }
                }
                CloudPointerEditKind::Body => {
                    let delta_x = current.x - start.x;
                    let delta_y = current.y - start.y;
                    for point in &mut points {
                        point.x += delta_x;
                        point.y += delta_y;
                    }
                }
            }
            if let Ok(preview) = CloudAnnotation::new(
                original.id.clone(),
                original.page_index,
                points,
                original.border_effect_intensity(),
                original.appearance.clone(),
            ) {
                annotation.points = preview.points().to_vec();
                annotation.scallop_path = preview.scallop_path();
                annotation.draft = true;
            }
        }
        if let Some(ActivePointer::DimensionCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && start != current
        {
            let properties = self.tool_properties(AnnotationTool::Dimension);
            if let Ok(annotation) = DimensionAnnotation::new(
                id.clone(),
                page_index,
                *start,
                *current,
                DimensionAnnotation::default_offset(*start, *current),
                "Dimension",
                dimension_tool_appearance(&properties)
                    .expect("stored Dimension tool properties are validated"),
            ) {
                scene.dimensions.push(SceneDimension {
                    id: annotation.id.clone(),
                    start: annotation.start,
                    end: annotation.end,
                    dimension_line_offset: annotation.dimension_line_offset(),
                    content: annotation.content().into(),
                    appearance: annotation.appearance,
                    selected: true,
                    locked: false,
                    draft: true,
                });
            }
        }
        if let Some(ActivePointer::LengthCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && start != current
            && let Some(calibration) = self
                .document_page_length_calibration(document_id, page_index)
                .cloned()
            && let Ok(annotation) = LengthAnnotation::new_with_appearance(
                id.clone(),
                page_index,
                *start,
                *current,
                calibration,
                dimension_tool_appearance(&self.tool_properties(AnnotationTool::Length))
                    .expect("stored Length tool properties are validated"),
            )
        {
            let caption = annotation.caption();
            let show_caption = annotation.calibration().show_caption();
            scene.lengths.push(SceneLength {
                id: annotation.id,
                start: annotation.start,
                end: annotation.end,
                caption,
                show_caption,
                appearance: annotation.appearance,
                selected: true,
                locked: false,
            });
        }
        if let Some(ActivePointer::LengthEndpoint {
            document_id: active_document_id,
            id,
            endpoint,
            current,
            ..
        }) = &self.active
            && *active_document_id == document_id
            && let Some(retained) = self.documents.get(&document_id).and_then(|document| {
                document
                    .lengths()
                    .iter()
                    .find(|annotation| annotation.id == *id && annotation.page_index == page_index)
            })
            && let Some(annotation) = scene
                .lengths
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let (start, end) = match endpoint {
                LengthEndpoint::Start => (*current, retained.end),
                LengthEndpoint::End => (retained.start, *current),
            };
            if let Ok(preview) = LengthAnnotation::new_with_appearance(
                id.clone(),
                page_index,
                start,
                end,
                retained.calibration().clone(),
                retained.appearance.clone(),
            ) {
                annotation.start = preview.start;
                annotation.end = preview.end;
                annotation.caption = preview.caption();
                annotation.show_caption = preview.calibration().show_caption();
            }
        }
        scene
    }

    pub fn canonical_document_scene(&self, document_id: u64, page_index: u32) -> AnnotationScene {
        self.documents
            .get(&document_id)
            .map(|document| document.document_scene(page_index))
            .unwrap_or_else(|| empty_scene(page_index))
    }

    pub fn thumbnail_scene(&self, document_id: u64, page_index: u32) -> AnnotationScene {
        self.documents
            .get(&document_id)
            .map(|document| document.thumbnail_scene(page_index))
            .unwrap_or_else(|| empty_scene(page_index))
    }

    pub fn selected_text(&self, document_id: u64) -> Option<&str> {
        let document = self.documents.get(&document_id)?;
        let id = document.selected_id()?;
        document
            .text_boxes()
            .iter()
            .find(|annotation| &annotation.id == id)
            .map(TextBoxAnnotation::content)
    }

    pub fn history_depths(&self, document_id: u64) -> (usize, usize) {
        self.documents
            .get(&document_id)
            .map(AnnotationDocument::history_depths)
            .unwrap_or_default()
    }

    pub fn snapshot(&self, document_id: u64) -> Option<AnnotationSnapshot> {
        self.documents
            .get(&document_id)
            .map(AnnotationDocument::snapshot)
    }

    pub fn canonical_json_snapshot(&self, document_id: u64) -> Option<serde_json::Value> {
        self.documents
            .get(&document_id)
            .map(AnnotationDocument::canonical_json_snapshot)
    }

    pub fn is_dirty(&self, document_id: u64) -> bool {
        self.documents
            .get(&document_id)
            .is_some_and(|document| document.snapshot().dirty)
    }

    pub fn spatial_query_work(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<SpatialQueryWork, AnnotationError> {
        self.documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .spatial_query_work(page_index, point, tolerance_pt)
    }

    pub fn mark_saved(&mut self, document_id: u64) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::MarkSaved)?;
        Ok(())
    }

    fn next_id(&mut self, tool: AnnotationTool) -> Result<MarkupId, AnnotationError> {
        if let Some(id) = self.queued_id.take() {
            return Ok(id);
        }
        self.next_sequence = self.next_sequence.saturating_add(1);
        let family = match tool {
            AnnotationTool::Select => "selection",
            AnnotationTool::Rectangle => "rectangle",
            AnnotationTool::Ellipse => "ellipse",
            AnnotationTool::Arc => "arc",
            AnnotationTool::Redact => "redact",
            AnnotationTool::Line => "line",
            AnnotationTool::Arrow => "arrow",
            AnnotationTool::Polyline => "polyline",
            AnnotationTool::Polygon => "polygon",
            AnnotationTool::Polylength => "polylength",
            AnnotationTool::Area => "area",
            AnnotationTool::Cloud => "cloud",
            AnnotationTool::CloudPlus => "cloud-plus",
            AnnotationTool::Callout => "callout",
            AnnotationTool::Pen => "pen",
            AnnotationTool::Highlight => "highlight",
            AnnotationTool::TextBox => "text",
            AnnotationTool::Length => "length",
            AnnotationTool::Dimension => "dimension",
            AnnotationTool::Image => "image",
            AnnotationTool::Snapshot => "snapshot",
        };
        MarkupId::new(format!("comparison:{family}:{}", self.next_sequence))
    }
}

fn require_pointer(active: u64, received: u64) -> Result<(), AnnotationError> {
    if active == received {
        Ok(())
    } else {
        Err(AnnotationError::PointerMismatch {
            expected: active,
            received,
        })
    }
}

fn point_distance_css_px(start: PdfPoint, end: PdfPoint, observed_pixels_per_point: f64) -> f64 {
    (end.x - start.x).hypot(end.y - start.y) * observed_pixels_per_point
}

fn rectangle_tool_appearance(
    properties: &ToolProperties,
    supports_fill: bool,
) -> Result<RectangleAppearance, AnnotationError> {
    RectangleAppearance::new(
        properties.colour.clone(),
        properties.width_pt,
        supports_fill
            .then(|| properties.fill_colour.clone())
            .flatten(),
        properties.opacity,
    )
    .and_then(|appearance| appearance.with_fill_opacity(properties.fill_opacity))
}

fn straight_line_tool_appearance(
    properties: &ToolProperties,
) -> Result<StraightLineAppearance, AnnotationError> {
    StraightLineAppearance::new(
        properties.colour.clone(),
        properties.width_pt,
        properties.opacity,
        StrokeStyle::Solid,
    )
}

fn pen_tool_appearance(properties: &ToolProperties) -> Result<PenAppearance, AnnotationError> {
    PenAppearance::new(
        properties.colour.clone(),
        properties.width_pt,
        properties.opacity,
    )
}

fn text_box_tool_style(properties: &ToolProperties) -> Result<TextBoxStyle, AnnotationError> {
    TextBoxStyle::new(
        properties.font_family.clone(),
        properties.font_size_pt,
        properties.colour.clone(),
        properties.opacity,
    )
}

fn callout_tool_appearance(
    properties: &ToolProperties,
) -> Result<CalloutAppearance, AnnotationError> {
    let line_properties = ToolProperties {
        width_pt: 1.0,
        ..properties.clone()
    };
    CalloutAppearance::new(
        straight_line_tool_appearance(&line_properties)?,
        text_box_tool_style(properties)?,
    )
}

fn cloud_plus_tool_appearance(
    properties: &ToolProperties,
) -> Result<CloudPlusAppearance, AnnotationError> {
    let line_properties = ToolProperties {
        width_pt: 1.0,
        fill_colour: None,
        ..properties.clone()
    };
    CloudPlusAppearance::new(
        rectangle_tool_appearance(&line_properties, false)?,
        straight_line_tool_appearance(&line_properties)?,
        text_box_tool_style(properties)?,
    )
}

fn dimension_tool_appearance(
    properties: &ToolProperties,
) -> Result<DimensionAppearance, AnnotationError> {
    DimensionAppearance::new(
        straight_line_tool_appearance(properties)?,
        text_box_tool_style(properties)?,
    )
}

fn default_cloud_plus_appearance() -> Result<CloudPlusAppearance, AnnotationError> {
    let properties = ToolProperties::for_tool(AnnotationTool::CloudPlus);
    cloud_plus_tool_appearance(&properties)
}

fn default_dimension_appearance() -> Result<DimensionAppearance, AnnotationError> {
    let properties = ToolProperties::for_tool(AnnotationTool::Dimension);
    dimension_tool_appearance(&properties)
}

fn cloud_visible_path(
    points: &[PdfPoint],
    border_effect_intensity: f64,
) -> Result<Vec<PdfPoint>, AnnotationError> {
    Ok(CloudAnnotation::new(
        MarkupId::new("cloud-plus:visible-path")?,
        0,
        points.to_vec(),
        border_effect_intensity,
        RectangleAppearance::new("#ff0000", 1., None::<String>, 1.)?,
    )?
    .scallop_path())
}

fn pointer_phase_outcome(outcome: CommandOutcome) -> PointerPhaseOutcome {
    match outcome {
        CommandOutcome::AnnotationCreated { id, .. }
        | CommandOutcome::GestureCommitted(crate::annotation_model::CommitOutcome::Created(id)) => {
            PointerPhaseOutcome::AnnotationCreated(id)
        }
        CommandOutcome::GestureCommitted(crate::annotation_model::CommitOutcome::Updated(id)) => {
            PointerPhaseOutcome::AnnotationEdited(id)
        }
        _ => PointerPhaseOutcome::Ignored,
    }
}

fn hit_straight_line_endpoint(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
) -> Option<(MarkupId, LineEndpoint)> {
    let selected = document.selected_id()?;
    let annotation = document
        .straight_lines()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    if distance(annotation.end, point) <= tolerance {
        Some((annotation.id.clone(), LineEndpoint::End))
    } else if distance(annotation.start, point) <= tolerance {
        Some((annotation.id.clone(), LineEndpoint::Start))
    } else {
        None
    }
}

fn hit_selected_vertex_path_point(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
) -> Option<(MarkupId, usize)> {
    let selected = document.selected_id()?;
    let annotation = document
        .vertex_paths()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    annotation
        .points()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, vertex)| distance(**vertex, point) <= tolerance)
        .map(|(index, _)| (annotation.id.clone(), index))
}

fn hit_selected_measurement_path_point(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
) -> Option<(MarkupId, usize)> {
    let selected = document.selected_id()?;
    let annotation = document
        .measurement_paths()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    annotation
        .points()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, vertex)| distance(**vertex, point) <= tolerance)
        .map(|(index, _)| (annotation.id.clone(), index))
}

fn hit_straight_line(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
) -> Option<MarkupId> {
    document
        .straight_lines()
        .iter()
        .rev()
        .find(|annotation| {
            annotation.page_index == page_index
                && point_segment_distance(point, annotation.start, annotation.end)
                    <= tolerance.max(annotation.appearance.stroke_width_pt() / 2.0)
        })
        .map(|annotation| annotation.id.clone())
}

fn hit_length_endpoint(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
) -> Option<(MarkupId, LengthEndpoint)> {
    let selected = document.selected_id()?;
    let annotation = document
        .lengths()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    if distance(annotation.end, point) <= tolerance {
        Some((annotation.id.clone(), LengthEndpoint::End))
    } else if distance(annotation.start, point) <= tolerance {
        Some((annotation.id.clone(), LengthEndpoint::Start))
    } else {
        None
    }
}

fn hit_selected_image_resize_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
) -> Option<(MarkupId, ImageResizeHandle)> {
    let selected = document.selected_id()?;
    let annotation = document
        .images()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let rect = annotation.rect;
    let left = rect.x;
    let center_x = rect.x + rect.width / 2.0;
    let right = rect.x + rect.width;
    let bottom = rect.y;
    let center_y = rect.y + rect.height / 2.0;
    let top = rect.y + rect.height;
    [
        (
            ImageResizeHandle::SouthWest,
            PdfPoint { x: left, y: bottom },
        ),
        (
            ImageResizeHandle::South,
            PdfPoint {
                x: center_x,
                y: bottom,
            },
        ),
        (
            ImageResizeHandle::SouthEast,
            PdfPoint {
                x: right,
                y: bottom,
            },
        ),
        (
            ImageResizeHandle::East,
            PdfPoint {
                x: right,
                y: center_y,
            },
        ),
        (ImageResizeHandle::NorthEast, PdfPoint { x: right, y: top }),
        (
            ImageResizeHandle::North,
            PdfPoint {
                x: center_x,
                y: top,
            },
        ),
        (ImageResizeHandle::NorthWest, PdfPoint { x: left, y: top }),
        (
            ImageResizeHandle::West,
            PdfPoint {
                x: left,
                y: center_y,
            },
        ),
    ]
    .into_iter()
    .find(|(_, center)| distance(*center, point) <= tolerance)
    .map(|(handle, _)| (annotation.id.clone(), handle))
}

fn hit_selected_snapshot_resize_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, RectangleResizeHandle)> {
    let selected = document.selected_id()?;
    let annotation = document
        .snapshots()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    if snapshot_rotation_handle_point(annotation, observed_pixels_per_point)
        .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
    {
        return None;
    }
    RectangleResizeHandle::ALL
        .into_iter()
        .rev()
        .find(|handle| {
            distance(snapshot_resize_handle_point(annotation, *handle), point) <= handle_tolerance
        })
        .map(|handle| (annotation.id.clone(), handle))
}

fn hit_selected_snapshot_rotation_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<MarkupId> {
    let selected = document.selected_id()?;
    let annotation = document
        .snapshots()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    snapshot_rotation_handle_point(annotation, observed_pixels_per_point)
        .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
        .then(|| annotation.id.clone())
}

fn resized_image_rect(
    original: PdfRect,
    handle: ImageResizeHandle,
    start: PdfPoint,
    current: PdfPoint,
) -> Result<PdfRect, AnnotationError> {
    const MIN_IMAGE_SIZE_PT: f64 = 24.0;
    let delta_x = current.x - start.x;
    let delta_y = current.y - start.y;
    let left_moves = matches!(
        handle,
        ImageResizeHandle::SouthWest | ImageResizeHandle::NorthWest | ImageResizeHandle::West
    );
    let right_moves = matches!(
        handle,
        ImageResizeHandle::SouthEast | ImageResizeHandle::NorthEast | ImageResizeHandle::East
    );
    let bottom_moves = matches!(
        handle,
        ImageResizeHandle::SouthWest | ImageResizeHandle::South | ImageResizeHandle::SouthEast
    );
    let top_moves = matches!(
        handle,
        ImageResizeHandle::NorthWest | ImageResizeHandle::North | ImageResizeHandle::NorthEast
    );

    let right = original.x + original.width;
    let top = original.y + original.height;
    let mut x = original.x;
    let mut y = original.y;
    let mut width = original.width;
    let mut height = original.height;
    if left_moves {
        x = (original.x + delta_x).min(right - MIN_IMAGE_SIZE_PT);
        width = right - x;
    } else if right_moves {
        width = (original.width + delta_x).max(MIN_IMAGE_SIZE_PT);
    }
    if bottom_moves {
        y = (original.y + delta_y).min(top - MIN_IMAGE_SIZE_PT);
        height = top - y;
    } else if top_moves {
        height = (original.height + delta_y).max(MIN_IMAGE_SIZE_PT);
    }
    PdfRect::new(x, y, width, height)
}

fn hit_non_rectangle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
) -> Option<MarkupId> {
    document
        .redacts()
        .iter()
        .rev()
        .find(|annotation| {
            annotation.page_index == page_index && point_in_rect(point, annotation.rect, tolerance)
        })
        .map(|annotation| annotation.id.clone())
        .or_else(|| {
            document
                .arcs()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index && arc_hit(annotation, point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .ellipses()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index && ellipse_hit(annotation, point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .vertex_paths()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index
                        && vertex_path_hit(annotation, point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .measurement_paths()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index
                        && measurement_path_hit(annotation, point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .clouds()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index && cloud_hit(annotation, point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .images()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index
                        && rect_contains(annotation.rect, point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .snapshots()
                .iter()
                .rev()
                .find(|annotation| {
                    if annotation.page_index != page_index {
                        return false;
                    }
                    let local_point = rotate_point_around_rect_center(
                        point,
                        annotation.rect,
                        annotation.rotation_degrees(),
                    );
                    rect_contains(annotation.rect, local_point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .text_boxes()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index
                        && rect_contains(annotation.layout_rect, point, tolerance)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .dimensions()
                .iter()
                .rev()
                .find(|annotation| {
                    if annotation.page_index != page_index {
                        return false;
                    }
                    let (start, end) = annotation.dimension_line_points();
                    point_segment_distance(point, start, end)
                        <= tolerance.max(annotation.appearance.line().stroke_width_pt() / 2.)
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .callouts()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index
                        && (rect_contains(annotation.text_box, point, tolerance)
                            || annotation.leader_points().windows(2).any(|segment| {
                                point_segment_distance(point, segment[0], segment[1]) <= tolerance
                            }))
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .lengths()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index
                        && point_segment_distance(point, annotation.start, annotation.end)
                            <= tolerance
                })
                .map(|annotation| annotation.id.clone())
        })
        .or_else(|| {
            document
                .pens()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == page_index
                        && annotation.paths().any(|path| {
                            path.windows(2).any(|segment| {
                                point_segment_distance(point, segment[0], segment[1])
                                    <= tolerance.max(annotation.appearance.width_pt() / 2.0)
                            })
                        })
                })
                .map(|annotation| annotation.id.clone())
        })
}

fn hit_selected_dimension_control(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, DimensionPointerEditKind)> {
    let selected = document.selected_id()?;
    let annotation = document
        .dimensions()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    for (handle, handle_point) in [
        (DimensionPointerEditKind::Start, annotation.start),
        (DimensionPointerEditKind::End, annotation.end),
        (
            DimensionPointerEditKind::Offset,
            annotation.caption_center(),
        ),
    ] {
        if distance(handle_point, point) <= handle_tolerance {
            return Some((annotation.id.clone(), handle));
        }
    }
    let (offset_start, offset_end) = annotation.dimension_line_points();
    (point_segment_distance(point, offset_start, offset_end) <= tolerance
        || point_segment_distance(point, annotation.start, annotation.end) <= tolerance)
        .then(|| (annotation.id.clone(), DimensionPointerEditKind::Body))
}

fn hit_selected_callout_control(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, CalloutPointerEditKind)> {
    let selected = document.selected_id()?;
    let annotation = document
        .callouts()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    if let Some((index, _)) = annotation
        .leader_points()
        .iter()
        .enumerate()
        .find(|(_, candidate)| distance(**candidate, point) <= handle_tolerance)
    {
        return Some((
            annotation.id.clone(),
            CalloutPointerEditKind::LeaderPoint(index),
        ));
    }
    if rect_contains(annotation.text_box, point, tolerance) {
        return Some((annotation.id.clone(), CalloutPointerEditKind::TextBox));
    }
    annotation
        .leader_points()
        .windows(2)
        .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= tolerance)
        .then(|| (annotation.id.clone(), CalloutPointerEditKind::Body))
}

fn hit_selected_cloud_control(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, CloudPointerEditKind)> {
    let selected = document.selected_id()?;
    let annotation = document
        .clouds()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    if let Some((index, _)) = annotation
        .points()
        .iter()
        .enumerate()
        .find(|(_, candidate)| distance(**candidate, point) <= handle_tolerance)
    {
        return Some((annotation.id.clone(), CloudPointerEditKind::Vertex(index)));
    }
    cloud_hit(annotation, point, tolerance)
        .then(|| (annotation.id.clone(), CloudPointerEditKind::Body))
}

fn validate_pointer_identity(
    document: &AnnotationDocument,
    id: &MarkupId,
    expected_revision: u64,
) -> Result<(), AnnotationError> {
    if document.snapshot().revision != expected_revision
        || document.selected_ids() != std::slice::from_ref(id)
    {
        return Err(AnnotationError::NoActiveGesture);
    }
    Ok(())
}

fn resolve_arc_control_point(
    original: &ArcAnnotation,
    control: ArcControlPoint,
    point: PdfPoint,
    minimum_bulge_pt: f64,
    snap_quarter_turn: bool,
) -> Result<PdfPoint, AnnotationError> {
    if control == ArcControlPoint::Mid && snap_quarter_turn {
        ArcAnnotation::constrained_midpoint(
            original.start,
            original.end,
            point,
            minimum_bulge_pt,
            true,
        )
    } else {
        Ok(point)
    }
}

fn validate_dimension_pointer_target(
    document: &AnnotationDocument,
    page_index: u32,
    id: &MarkupId,
    expected_revision: u64,
    original: &DimensionAnnotation,
) -> Result<(), AnnotationError> {
    validate_pointer_identity(document, id, expected_revision)?;
    let retained = document
        .dimensions()
        .iter()
        .find(|annotation| &annotation.id == id && annotation.page_index == page_index)
        .ok_or(AnnotationError::NoSelection)?;
    if retained.locked {
        return Err(AnnotationError::LockedMarkup(id.clone()));
    }
    if !retained.same_persisted_state_as(original) {
        return Err(AnnotationError::NoActiveGesture);
    }
    Ok(())
}

fn validate_arc_pointer_target(
    document: &AnnotationDocument,
    page_index: u32,
    id: &MarkupId,
    expected_revision: u64,
    original: &ArcAnnotation,
) -> Result<(), AnnotationError> {
    validate_pointer_identity(document, id, expected_revision)?;
    let retained = document
        .arcs()
        .iter()
        .find(|annotation| &annotation.id == id && annotation.page_index == page_index)
        .ok_or(AnnotationError::NoSelection)?;
    if retained.locked {
        return Err(AnnotationError::LockedMarkup(id.clone()));
    }
    if !retained.same_persisted_state_as(original) {
        return Err(AnnotationError::NoActiveGesture);
    }
    Ok(())
}

fn validate_callout_pointer_target(
    document: &AnnotationDocument,
    page_index: u32,
    id: &MarkupId,
    expected_revision: u64,
    original: &CalloutAnnotation,
) -> Result<(), AnnotationError> {
    validate_pointer_identity(document, id, expected_revision)?;
    let retained = document
        .callouts()
        .iter()
        .find(|annotation| &annotation.id == id && annotation.page_index == page_index)
        .ok_or(AnnotationError::NoSelection)?;
    if retained.locked {
        return Err(AnnotationError::LockedMarkup(id.clone()));
    }
    if !retained.same_persisted_state_as(original) {
        return Err(AnnotationError::NoActiveGesture);
    }
    Ok(())
}

fn validate_cloud_pointer_target(
    document: &AnnotationDocument,
    page_index: u32,
    id: &MarkupId,
    expected_revision: u64,
    original: &CloudAnnotation,
) -> Result<(), AnnotationError> {
    validate_pointer_identity(document, id, expected_revision)?;
    let retained = document
        .clouds()
        .iter()
        .find(|annotation| &annotation.id == id && annotation.page_index == page_index)
        .ok_or(AnnotationError::NoSelection)?;
    if retained.locked {
        return Err(AnnotationError::LockedMarkup(id.clone()));
    }
    if !retained.same_persisted_state_as(original) {
        return Err(AnnotationError::NoActiveGesture);
    }
    Ok(())
}

fn point_in_rect(point: PdfPoint, rect: PdfRect, tolerance: f64) -> bool {
    point.x >= rect.x - tolerance
        && point.x <= rect.x + rect.width + tolerance
        && point.y >= rect.y - tolerance
        && point.y <= rect.y + rect.height + tolerance
}

fn hit_selected_redact_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, RectangleResizeHandle)> {
    let selected = document.selected_id()?;
    let annotation = document
        .redacts()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    RectangleResizeHandle::ALL
        .into_iter()
        .rev()
        .find(|handle| {
            distance(redact_resize_handle_point(annotation, *handle), point) <= handle_tolerance
        })
        .map(|handle| (annotation.id.clone(), handle))
}

fn redact_resized_rect(
    original: PdfRect,
    handle: RectangleResizeHandle,
    point: PdfPoint,
) -> Result<PdfRect, AnnotationError> {
    // Electron clamps resize geometry to the two-point product minimum while
    // creation remains strictly greater than two points. Keep the retained
    // canonical value one micro-point above the open creation boundary.
    const MINIMUM: f64 = 2.000_001;
    let left = original.x;
    let bottom = original.y;
    let right = original.x + original.width;
    let top = original.y + original.height;
    let west = matches!(
        handle,
        RectangleResizeHandle::NorthWest
            | RectangleResizeHandle::West
            | RectangleResizeHandle::SouthWest
    );
    let east = matches!(
        handle,
        RectangleResizeHandle::NorthEast
            | RectangleResizeHandle::East
            | RectangleResizeHandle::SouthEast
    );
    let north = matches!(
        handle,
        RectangleResizeHandle::NorthWest
            | RectangleResizeHandle::North
            | RectangleResizeHandle::NorthEast
    );
    let south = matches!(
        handle,
        RectangleResizeHandle::SouthWest
            | RectangleResizeHandle::South
            | RectangleResizeHandle::SouthEast
    );
    let next_left = if west {
        point.x.min(right - MINIMUM)
    } else {
        left
    };
    let next_right = if east {
        point.x.max(left + MINIMUM)
    } else {
        right
    };
    let next_bottom = if south {
        point.y.min(top - MINIMUM)
    } else {
        bottom
    };
    let next_top = if north {
        point.y.max(bottom + MINIMUM)
    } else {
        top
    };
    PdfRect::new(
        next_left,
        next_bottom,
        next_right - next_left,
        next_top - next_bottom,
    )
}

fn hit_selected_arc_control_point(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, ArcControlPoint)> {
    let selected = document.selected_id()?;
    let annotation = document
        .arcs()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    [
        (ArcControlPoint::Start, annotation.start),
        (ArcControlPoint::Mid, annotation.mid),
        (ArcControlPoint::End, annotation.end),
    ]
    .into_iter()
    .find(|(_, control_point)| {
        distance(*control_point, point)
            <= tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON))
    })
    .map(|(control, _)| (annotation.id.clone(), control))
}

fn arc_hit(annotation: &ArcAnnotation, point: PdfPoint, tolerance: f64) -> bool {
    let edge_tolerance = tolerance.max(annotation.appearance.stroke_width_pt() / 2.);
    annotation
        .sampled_path(64)
        .windows(2)
        .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= edge_tolerance)
}

fn vertex_path_hit(annotation: &VertexPathAnnotation, point: PdfPoint, tolerance: f64) -> bool {
    let points = annotation.points();
    let edge_tolerance = tolerance.max(annotation.appearance.stroke_width_pt() / 2.0);
    if points
        .windows(2)
        .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= edge_tolerance)
    {
        return true;
    }
    if annotation.kind == VertexPathKind::Polygon
        && points.len() >= 3
        && point_segment_distance(point, *points.last().unwrap(), points[0]) <= edge_tolerance
    {
        return true;
    }
    annotation.kind == VertexPathKind::Polygon
        && annotation.appearance.fill_color().is_some()
        && point_in_polygon(point, points)
}

fn measurement_path_hit(
    annotation: &MeasurementPathAnnotation,
    point: PdfPoint,
    tolerance: f64,
) -> bool {
    let points = annotation.points();
    let edge_tolerance = tolerance.max(annotation.appearance.stroke_width_pt() / 2.0);
    if points
        .windows(2)
        .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= edge_tolerance)
    {
        return true;
    }
    if annotation.kind == MeasurementPathKind::Area
        && points.len() >= 3
        && point_segment_distance(point, *points.last().unwrap(), points[0]) <= edge_tolerance
    {
        return true;
    }
    annotation.kind == MeasurementPathKind::Area && point_in_polygon(point, points)
}

fn cloud_hit(annotation: &CloudAnnotation, point: PdfPoint, tolerance: f64) -> bool {
    let points = annotation.scallop_path();
    let edge_tolerance = tolerance.max(annotation.appearance.stroke_width_pt() / 2.0);
    points
        .windows(2)
        .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= edge_tolerance)
}

fn point_in_polygon(point: PdfPoint, vertices: &[PdfPoint]) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut previous = *vertices.last().unwrap();
    for &current in vertices {
        let crosses = (current.y > point.y) != (previous.y > point.y)
            && point.x
                < (previous.x - current.x) * (point.y - current.y) / (previous.y - current.y)
                    + current.x;
        if crosses {
            inside = !inside;
        }
        previous = current;
    }
    inside
}

fn hit_selected_ellipse_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, EllipseHandleKind)> {
    let selected = document.selected_id()?;
    let annotation = document
        .ellipses()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    if ellipse_rotation_handle_point(annotation, observed_pixels_per_point)
        .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
    {
        return Some((annotation.id.clone(), EllipseHandleKind::Rotate));
    }
    RectangleResizeHandle::ALL
        .into_iter()
        .rev()
        .find(|handle| {
            distance(ellipse_resize_handle_point(annotation, *handle), point) <= handle_tolerance
        })
        .map(|handle| (annotation.id.clone(), EllipseHandleKind::Resize(handle)))
}

fn hit_selected_text_box_resize_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, RectangleResizeHandle)> {
    let selected = document.selected_id()?;
    let annotation = document
        .text_boxes()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(9. / observed_pixels_per_point.max(f64::EPSILON));
    RectangleResizeHandle::ALL
        .into_iter()
        .rev()
        .find(|handle| {
            distance(
                axis_aligned_resize_handle_point(annotation.layout_rect, *handle),
                point,
            ) <= handle_tolerance
        })
        .map(|handle| (annotation.id.clone(), handle))
}

fn ellipse_resize_point_from_handle(
    original: PdfRect,
    rotation_degrees: f64,
    handle: RectangleResizeHandle,
    point: PdfPoint,
) -> PdfPoint {
    if matches!(
        handle,
        RectangleResizeHandle::North
            | RectangleResizeHandle::East
            | RectangleResizeHandle::South
            | RectangleResizeHandle::West
    ) {
        return point;
    }
    let local_point = rotate_point_around_rect_center(point, original, rotation_degrees);
    let opposite = match handle {
        RectangleResizeHandle::NorthWest => PdfPoint {
            x: original.x + original.width,
            y: original.y,
        },
        RectangleResizeHandle::NorthEast => PdfPoint {
            x: original.x,
            y: original.y,
        },
        RectangleResizeHandle::SouthEast => PdfPoint {
            x: original.x,
            y: original.y + original.height,
        },
        RectangleResizeHandle::SouthWest => PdfPoint {
            x: original.x + original.width,
            y: original.y + original.height,
        },
        _ => unreachable!("cardinal Ellipse handles return before diagonal projection"),
    };
    let opposite_factor = (1. + std::f64::consts::FRAC_1_SQRT_2) * 0.5;
    let bounds_point = PdfPoint {
        x: opposite.x + (local_point.x - opposite.x) / opposite_factor,
        y: opposite.y + (local_point.y - opposite.y) / opposite_factor,
    };
    rotate_point_around_rect_center(bounds_point, original, -rotation_degrees)
}

fn ellipse_resized_rect(
    original: PdfRect,
    rotation_degrees: f64,
    handle: RectangleResizeHandle,
    point: PdfPoint,
) -> PdfRect {
    original.rotated_resize_from_handle(
        rotation_degrees,
        handle,
        ellipse_resize_point_from_handle(original, rotation_degrees, handle, point),
    )
}

fn ellipse_rotation_from_drag(
    original_rect: PdfRect,
    original_rotation_degrees: f64,
    start: PdfPoint,
    current: PdfPoint,
) -> f64 {
    let center_x = original_rect.x + original_rect.width * 0.5;
    let center_y = original_rect.y + original_rect.height * 0.5;
    let start_angle = (start.y - center_y).atan2(start.x - center_x);
    let current_angle = (current.y - center_y).atan2(current.x - center_x);
    (original_rotation_degrees + (start_angle - current_angle).to_degrees()).rem_euclid(360.)
}

fn rotate_point_around_rect_center(
    point: PdfPoint,
    rect: PdfRect,
    rotation_degrees: f64,
) -> PdfPoint {
    let center_x = rect.x + rect.width * 0.5;
    let center_y = rect.y + rect.height * 0.5;
    let radians = rotation_degrees.to_radians();
    let delta_x = point.x - center_x;
    let delta_y = point.y - center_y;
    PdfPoint {
        x: center_x + delta_x * radians.cos() - delta_y * radians.sin(),
        y: center_y + delta_x * radians.sin() + delta_y * radians.cos(),
    }
}

fn ellipse_hit(annotation: &EllipseAnnotation, point: PdfPoint, tolerance: f64) -> bool {
    let center_x = annotation.rect.x + annotation.rect.width / 2.;
    let center_y = annotation.rect.y + annotation.rect.height / 2.;
    let radians = annotation.rotation_degrees.to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let dx = point.x - center_x;
    let dy = point.y - center_y;
    let local_x = dx * cosine + dy * sine;
    let local_y = -dx * sine + dy * cosine;
    let radius_x = annotation.rect.width / 2.;
    let radius_y = annotation.rect.height / 2.;
    if radius_x <= 0. || radius_y <= 0. {
        return false;
    }
    let normalized = ((local_x / radius_x).powi(2) + (local_y / radius_y).powi(2)).sqrt();
    if annotation.appearance.fill_color().is_some() && normalized <= 1. {
        return true;
    }
    (normalized - 1.).abs() * radius_x.min(radius_y)
        <= tolerance.max(annotation.appearance.stroke_width_pt() / 2.)
}

fn rect_contains(rect: PdfRect, point: PdfPoint, tolerance: f64) -> bool {
    point.x >= rect.x - tolerance
        && point.x <= rect.x + rect.width + tolerance
        && point.y >= rect.y - tolerance
        && point.y <= rect.y + rect.height + tolerance
}

fn resolve_rectangle_translation_endpoint(
    start: Option<PdfPoint>,
    raw_endpoint: PdfPoint,
    settings: RectangleSnapSettings,
    observed_pixels_per_point: f64,
) -> PdfPoint {
    let Some(start) = start else {
        return raw_endpoint;
    };
    let Some(resolution) = rectangle_translation_snap_resolution(
        start,
        raw_endpoint,
        settings,
        observed_pixels_per_point,
    ) else {
        return raw_endpoint;
    };
    PdfPoint::new(
        start.x + resolution.applied.x,
        start.y + resolution.applied.y,
    )
    .unwrap_or(raw_endpoint)
}

fn rectangle_translation_snap_resolution(
    start: PdfPoint,
    raw_endpoint: PdfPoint,
    settings: RectangleSnapSettings,
    observed_pixels_per_point: f64,
) -> Option<SnapResolution> {
    settings.enabled.then(|| {
        let raw = Translation::new(raw_endpoint.x - start.x, raw_endpoint.y - start.y)
            .expect("validated PDF points produce a finite translation");
        InclusiveLInfGridSnap::from_css_pixels(
            settings.grid_spacing_pt,
            settings.sensitivity_css_px,
            observed_pixels_per_point,
        )
        .expect("the adapter stores only validated snap settings")
        .resolve(raw)
        .expect("validated PDF points produce a finite snap resolution")
    })
}

fn distance(left: PdfPoint, right: PdfPoint) -> f64 {
    (left.x - right.x).hypot(left.y - right.y)
}

fn selection_point_from_pdf(point: PdfPoint) -> SelectionPoint {
    SelectionPoint::new(point.x, point.y)
}

fn constrained_length_point(
    start: PdfPoint,
    point: PdfPoint,
    constrain_orthogonal: bool,
) -> PdfPoint {
    if !constrain_orthogonal {
        return point;
    }
    if (point.x - start.x).abs() >= (point.y - start.y).abs() {
        PdfPoint {
            x: point.x,
            y: start.y,
        }
    } else {
        PdfPoint {
            x: start.x,
            y: point.y,
        }
    }
}

fn constrained_line_point(
    start: PdfPoint,
    point: PdfPoint,
    constrain_orthogonal: bool,
) -> PdfPoint {
    constrained_length_point(start, point, constrain_orthogonal)
}

fn point_segment_distance(point: PdfPoint, start: PdfPoint, end: PdfPoint) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f64::EPSILON {
        return distance(point, start);
    }
    let projection =
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    distance(
        point,
        PdfPoint {
            x: start.x + projection * dx,
            y: start.y + projection * dy,
        },
    )
}

fn empty_scene(page_index: u32) -> AnnotationScene {
    AnnotationScene {
        page_index,
        revision: 0,
        rectangles: Vec::new(),
        ellipses: Vec::new(),
        arcs: Vec::new(),
        redacts: Vec::new(),
        straight_lines: Vec::new(),
        vertex_paths: Vec::new(),
        clouds: Vec::new(),
        cloud_pluses: Vec::new(),
        callouts: Vec::new(),
        measurement_paths: Vec::new(),
        pens: Vec::new(),
        text_boxes: Vec::new(),
        dimensions: Vec::new(),
        lengths: Vec::new(),
        images: Vec::new(),
        snapshots: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> PdfPoint {
        PdfPoint::new(x, y).unwrap()
    }

    fn seed_dimension(adapter: &mut AnnotationAdapter) -> MarkupId {
        let id = MarkupId::new("dimension:pointer-edit").unwrap();
        let annotation = DimensionAnnotation::new(
            id.clone(),
            0,
            point(20., 40.),
            point(120., 40.),
            24.,
            "100 mm",
            default_dimension_appearance().unwrap(),
        )
        .unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Dimension(
                annotation,
            )))
            .unwrap();
        id
    }

    fn seed_callout(adapter: &mut AnnotationAdapter) -> MarkupId {
        let id = MarkupId::new("callout:pointer-edit").unwrap();
        let appearance = CalloutAppearance::new(
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap();
        let annotation = CalloutAnnotation::new(
            id.clone(),
            0,
            vec![point(20., 20.), point(60., 40.), point(100., 40.)],
            PdfRect::new(100., 20., 80., 40.).unwrap(),
            "Note",
            appearance,
        )
        .unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Callout(
                annotation,
            )))
            .unwrap();
        id
    }

    fn seed_cloud(adapter: &mut AnnotationAdapter) -> MarkupId {
        let id = MarkupId::new("cloud:pointer-edit").unwrap();
        let annotation = CloudAnnotation::new(
            id.clone(),
            0,
            vec![
                point(20., 20.),
                point(100., 20.),
                point(100., 80.),
                point(20., 80.),
            ],
            3.,
            RectangleAppearance::new("#ff0000", 2., None::<String>, 0.8).unwrap(),
        )
        .unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Cloud(
                annotation,
            )))
            .unwrap();
        id
    }

    #[test]
    fn ordinary_rectangle_property_command_commits_once_and_preserves_other_appearance() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        adapter
            .pointer_down(7, 0, 1, PdfPoint::new(10.0, 20.0).unwrap(), 4.0)
            .unwrap();
        adapter
            .pointer_move(1, PdfPoint::new(110.0, 80.0).unwrap())
            .unwrap();
        adapter
            .pointer_up(1, PdfPoint::new(110.0, 80.0).unwrap())
            .unwrap();

        let before = adapter.selected_rectangle_appearance(7).unwrap().clone();
        let history_before = adapter.history_depths(7);
        let receipt = adapter
            .commit_selected_rectangle_stroke_width(7, 4.0)
            .unwrap();

        let after = adapter.selected_rectangle_appearance(7).unwrap();
        assert_eq!(after.stroke_width_pt(), 4.0);
        assert_eq!(after.stroke_color(), before.stroke_color());
        assert_eq!(after.fill_color(), before.fill_color());
        assert_eq!(after.opacity(), before.opacity());
        assert_eq!(receipt.history_before, history_before);
        assert_eq!(receipt.history_after, (history_before.0 + 1, 0));
    }

    #[test]
    fn dimension_pointer_edits_preview_then_commit_once_and_reject_stale_or_invalid_release() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_dimension(&mut adapter);
        let before = adapter.snapshot(7).unwrap();
        let original = before.dimensions[0].clone();

        assert_eq!(
            adapter.pointer_down(7, 0, 1, original.start, 4.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(1, point(30., 50.)).unwrap();
        let preview = adapter.document_scene(7, 0).dimensions.remove(0);
        assert_eq!(preview.start, point(30., 50.));
        assert!(preview.draft);
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        adapter.cancel(PointerCancelReason::FocusLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        adapter.pointer_down(7, 0, 2, original.end, 4.).unwrap();
        assert_eq!(
            adapter.pointer_up(2, point(140., 50.)).unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id.clone())
        );
        let committed = adapter.snapshot(7).unwrap();
        assert_eq!(committed.revision, before.revision + 1);
        assert_eq!(committed.dimensions[0].end, point(140., 50.));
        assert_eq!(committed.dimensions[0].content(), original.content());
        assert_eq!(committed.dimensions[0].appearance, original.appearance);
        adapter.undo(7).unwrap();

        let offset_handle = original.caption_center();
        adapter.pointer_down(7, 0, 3, offset_handle, 4.).unwrap();
        adapter.set_selected_dimension_offset(7, 30.).unwrap();
        let stale_revision = adapter.snapshot(7).unwrap().revision;
        assert_eq!(
            adapter.pointer_up(3, point(offset_handle.x, offset_handle.y + 20.)),
            Err(AnnotationError::NoActiveGesture)
        );
        assert_eq!(adapter.snapshot(7).unwrap().revision, stale_revision);
        adapter.undo(7).unwrap();

        let revision_before_invalid = adapter.snapshot(7).unwrap().revision;
        adapter.pointer_down(7, 0, 4, original.start, 4.).unwrap();
        assert!(matches!(
            adapter.pointer_up(4, original.end),
            Err(AnnotationError::InvalidGeometry(_))
        ));
        assert_eq!(
            adapter.snapshot(7).unwrap().revision,
            revision_before_invalid
        );
    }

    #[test]
    fn callout_pointer_edits_keep_leader_order_and_distinguish_text_box_from_group() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_callout(&mut adapter);
        let original = adapter.snapshot(7).unwrap().callouts[0].clone();
        let base_revision = adapter.snapshot(7).unwrap().revision;

        adapter
            .pointer_down(7, 0, 1, original.leader_points()[1], 4.)
            .unwrap();
        adapter.pointer_move(1, point(64., 52.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).callouts[0].leader_points[1],
            point(64., 52.)
        );
        assert_eq!(adapter.snapshot(7).unwrap().revision, base_revision);
        assert_eq!(
            adapter.pointer_up(1, point(64., 52.)).unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id.clone())
        );
        assert_eq!(adapter.snapshot(7).unwrap().revision, base_revision + 1);
        adapter.undo(7).unwrap();

        let text_center = point(
            original.text_box.x + original.text_box.width * 0.5,
            original.text_box.y + original.text_box.height * 0.5,
        );
        adapter.pointer_down(7, 0, 2, text_center, 4.).unwrap();
        adapter
            .pointer_up(2, point(text_center.x + 10., text_center.y + 5.))
            .unwrap();
        let text_moved = adapter.snapshot(7).unwrap().callouts[0].clone();
        assert_eq!(text_moved.leader_points()[0], original.leader_points()[0]);
        assert_eq!(text_moved.leader_points()[1], original.leader_points()[1]);
        assert_eq!(text_moved.leader_points()[2], point(110., 45.));
        adapter.undo(7).unwrap();

        let leader_body = point(40., 30.);
        adapter.pointer_down(7, 0, 3, leader_body, 4.).unwrap();
        adapter.pointer_up(3, point(50., 40.)).unwrap();
        let group_moved = adapter.snapshot(7).unwrap().callouts[0].clone();
        assert_eq!(group_moved.leader_points()[0], point(30., 30.));
        assert_eq!(group_moved.text_box.x, original.text_box.x + 10.);
        assert_eq!(group_moved.content(), original.content());
        assert_eq!(group_moved.appearance, original.appearance);

        let tip = group_moved.leader_points()[0];
        adapter.pointer_down(7, 0, 4, tip, 4.).unwrap();
        adapter
            .set_selected_callout_leader_point(7, 1, point(75., 55.))
            .unwrap();
        let stale_revision = adapter.snapshot(7).unwrap().revision;
        assert_eq!(
            adapter.pointer_up(4, point(tip.x + 12., tip.y + 8.)),
            Err(AnnotationError::NoActiveGesture)
        );
        assert_eq!(adapter.snapshot(7).unwrap().revision, stale_revision);
    }

    #[test]
    fn cloud_pointer_vertex_and_body_edits_preserve_scallop_authority_and_cancel_exactly() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_cloud(&mut adapter);
        let before = adapter.snapshot(7).unwrap();
        let original = before.clouds[0].clone();

        adapter
            .pointer_down(7, 0, 1, original.points()[0], 4.)
            .unwrap();
        adapter.pointer_move(1, point(25., 30.)).unwrap();
        let preview = adapter.document_scene(7, 0).clouds.remove(0);
        assert_eq!(preview.points[0], point(25., 30.));
        assert_ne!(preview.scallop_path, original.scallop_path());
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        let body = original.scallop_path()[0];
        adapter.pointer_down(7, 0, 2, body, 4.).unwrap();
        assert_eq!(
            adapter
                .pointer_up(2, point(body.x + 10., body.y + 8.))
                .unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id)
        );
        let moved = adapter.snapshot(7).unwrap();
        assert_eq!(moved.revision, before.revision + 1);
        assert_eq!(moved.clouds[0].points()[0], point(30., 28.));
        assert_eq!(
            moved.clouds[0].border_effect_intensity(),
            original.border_effect_intensity()
        );
        assert_eq!(moved.clouds[0].appearance, original.appearance);
    }

    fn customised_tool_properties(tool: AnnotationTool) -> ToolProperties {
        let mut properties = ToolProperties::for_tool(tool);
        properties.colour = "#336699".into();
        properties.width_pt = if tool == AnnotationTool::Highlight {
            4.0
        } else {
            3.25
        };
        properties.fill_colour = Some("#abcdef".into());
        properties.fill_opacity = 0.35;
        properties.opacity = 0.55;
        properties.font_size_pt = 18.0;
        properties.font_family = "Arimo".into();
        properties.smooth_curves = false;
        properties.cloud_intensity = 3.0;
        properties
    }

    fn create_drag_annotation(adapter: &mut AnnotationAdapter, tool: AnnotationTool) {
        adapter.set_tool(tool).unwrap();
        adapter.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        adapter.pointer_move(1, point(120., 80.)).unwrap();
        adapter.pointer_up(1, point(120., 80.)).unwrap();
    }

    fn assert_line_properties(appearance: &StraightLineAppearance, expected_width: f64) {
        assert_eq!(appearance.stroke_color(), "#336699");
        assert_eq!(appearance.stroke_width_pt(), expected_width);
        assert_eq!(appearance.opacity(), 0.55);
    }

    fn assert_rectangle_properties(appearance: &RectangleAppearance, expected_fill: Option<&str>) {
        assert_eq!(appearance.stroke_color(), "#336699");
        assert_eq!(appearance.stroke_width_pt(), 3.25);
        assert_eq!(appearance.fill_color(), expected_fill);
        assert_eq!(
            appearance.fill_opacity(),
            if expected_fill.is_some() { 0.35 } else { 1.0 }
        );
        assert_eq!(appearance.opacity(), 0.55);
    }

    #[test]
    fn tool_defaults_drive_shape_line_ink_and_text_creation() {
        let mut rectangle = AnnotationAdapter::default();
        rectangle
            .set_tool_properties(
                AnnotationTool::Rectangle,
                customised_tool_properties(AnnotationTool::Rectangle),
            )
            .unwrap();
        create_drag_annotation(&mut rectangle, AnnotationTool::Rectangle);
        assert_rectangle_properties(
            &rectangle.snapshot(7).unwrap().rectangles[0].appearance,
            Some("#abcdef"),
        );

        let mut ellipse = AnnotationAdapter::default();
        ellipse
            .set_tool_properties(
                AnnotationTool::Ellipse,
                customised_tool_properties(AnnotationTool::Ellipse),
            )
            .unwrap();
        create_drag_annotation(&mut ellipse, AnnotationTool::Ellipse);
        assert_rectangle_properties(
            &ellipse.snapshot(7).unwrap().ellipses[0].appearance,
            Some("#abcdef"),
        );

        for tool in [AnnotationTool::Line, AnnotationTool::Arrow] {
            let mut adapter = AnnotationAdapter::default();
            adapter
                .set_tool_properties(tool, customised_tool_properties(tool))
                .unwrap();
            create_drag_annotation(&mut adapter, tool);
            assert_line_properties(
                &adapter.snapshot(7).unwrap().straight_lines[0].appearance,
                3.25,
            );
        }

        for tool in [AnnotationTool::Pen, AnnotationTool::Highlight] {
            let mut adapter = AnnotationAdapter::default();
            adapter
                .set_tool_properties(tool, customised_tool_properties(tool))
                .unwrap();
            create_drag_annotation(&mut adapter, tool);
            let pen = &adapter.snapshot(7).unwrap().pens[0];
            assert_eq!(pen.appearance.color(), "#336699");
            assert_eq!(
                pen.appearance.width_pt(),
                if tool == AnnotationTool::Highlight {
                    4.0
                } else {
                    3.25
                }
            );
            assert_eq!(pen.appearance.opacity(), 0.55);
            assert!(!pen.smooth_curves);
        }

        let mut text_box = AnnotationAdapter::default();
        text_box
            .set_tool_properties(
                AnnotationTool::TextBox,
                customised_tool_properties(AnnotationTool::TextBox),
            )
            .unwrap();
        text_box.set_tool(AnnotationTool::TextBox).unwrap();
        text_box.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        let snapshot = text_box.snapshot(7).unwrap();
        let style = snapshot.text_boxes[0].style();
        assert_eq!(style.color(), "#336699");
        assert_eq!(style.font_size_pt(), 18.0);
        assert_eq!(style.font_family(), "Arimo");
        assert_eq!(style.opacity(), 0.55);
    }

    #[test]
    fn tool_defaults_drive_vertex_cloud_and_measurement_path_creation() {
        for tool in [AnnotationTool::Polyline, AnnotationTool::Polygon] {
            let mut adapter = AnnotationAdapter::default();
            adapter
                .set_tool_properties(tool, customised_tool_properties(tool))
                .unwrap();
            adapter.set_tool(tool).unwrap();
            for (pointer_id, vertex) in [
                (1, point(20., 20.)),
                (2, point(100., 20.)),
                (3, point(100., 80.)),
            ] {
                adapter.pointer_down(7, 0, pointer_id, vertex, 4.).unwrap();
            }
            adapter.finish_vertex_path(7).unwrap();
            assert_rectangle_properties(
                &adapter.snapshot(7).unwrap().vertex_paths[0].appearance,
                (tool == AnnotationTool::Polygon).then_some("#abcdef"),
            );
        }

        let mut arc = AnnotationAdapter::default();
        arc.set_tool_properties(
            AnnotationTool::Arc,
            customised_tool_properties(AnnotationTool::Arc),
        )
        .unwrap();
        arc.set_tool(AnnotationTool::Arc).unwrap();
        arc.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        arc.pointer_down(7, 0, 2, point(120., 20.), 4.).unwrap();
        arc.pointer_down(7, 0, 3, point(70., 80.), 4.).unwrap();
        assert_rectangle_properties(&arc.snapshot(7).unwrap().arcs[0].appearance, None);

        let mut cloud = AnnotationAdapter::default();
        cloud
            .set_tool_properties(
                AnnotationTool::Cloud,
                customised_tool_properties(AnnotationTool::Cloud),
            )
            .unwrap();
        cloud.set_tool(AnnotationTool::Cloud).unwrap();
        for (pointer_id, vertex) in [
            (1, point(20., 20.)),
            (2, point(100., 20.)),
            (3, point(100., 80.)),
        ] {
            cloud.pointer_down(7, 0, pointer_id, vertex, 4.).unwrap();
        }
        cloud.finish_cloud(7).unwrap();
        let cloud = &cloud.snapshot(7).unwrap().clouds[0];
        assert_rectangle_properties(&cloud.appearance, None);
        assert_eq!(cloud.border_effect_intensity(), 3.0);

        for tool in [AnnotationTool::Polylength, AnnotationTool::Area] {
            let mut adapter = AnnotationAdapter::default();
            adapter
                .set_document_page_length_calibration(
                    7,
                    0,
                    LengthCalibration::new(1.0, "mm", "Scale", true).unwrap(),
                )
                .unwrap();
            adapter
                .set_tool_properties(tool, customised_tool_properties(tool))
                .unwrap();
            adapter.set_tool(tool).unwrap();
            for (pointer_id, vertex) in [
                (1, point(20., 20.)),
                (2, point(100., 20.)),
                (3, point(100., 80.)),
            ] {
                adapter.pointer_down(7, 0, pointer_id, vertex, 4.).unwrap();
            }
            adapter.finish_measurement_path(7).unwrap();
            assert_rectangle_properties(
                &adapter.snapshot(7).unwrap().measurement_paths[0].appearance,
                None,
            );
        }
    }

    #[test]
    fn tool_defaults_drive_composite_dimension_and_snapshot_creation() {
        let mut callout_adapter = AnnotationAdapter::default();
        callout_adapter
            .set_tool_properties(
                AnnotationTool::Callout,
                customised_tool_properties(AnnotationTool::Callout),
            )
            .unwrap();
        create_drag_annotation(&mut callout_adapter, AnnotationTool::Callout);
        let callout = &callout_adapter.snapshot(7).unwrap().callouts[0];
        assert_line_properties(callout.appearance.line(), 1.0);
        assert_eq!(callout.appearance.text().color(), "#336699");
        assert_eq!(callout.appearance.text().font_size_pt(), 18.0);

        let mut cloud_plus = AnnotationAdapter::default();
        cloud_plus
            .set_tool_properties(
                AnnotationTool::CloudPlus,
                customised_tool_properties(AnnotationTool::CloudPlus),
            )
            .unwrap();
        cloud_plus.set_tool(AnnotationTool::CloudPlus).unwrap();
        cloud_plus
            .pointer_down(7, 0, 1, point(20., 20.), 4.)
            .unwrap();
        cloud_plus.pointer_up(1, point(20., 20.)).unwrap();
        cloud_plus
            .pointer_down(7, 0, 2, point(100., 20.), 4.)
            .unwrap();
        cloud_plus
            .pointer_down(7, 0, 3, point(100., 80.), 4.)
            .unwrap();
        cloud_plus.finish_cloud_plus(7).unwrap();
        let cloud_plus = &cloud_plus.snapshot(7).unwrap().cloud_pluses[0];
        assert_eq!(cloud_plus.appearance.cloud().stroke_color(), "#336699");
        assert_eq!(cloud_plus.appearance.cloud().stroke_width_pt(), 1.0);
        assert_eq!(cloud_plus.appearance.cloud().fill_color(), None);
        assert_eq!(cloud_plus.appearance.cloud().opacity(), 0.55);
        assert_eq!(cloud_plus.appearance.text().font_size_pt(), 18.0);
        assert_eq!(cloud_plus.border_effect_intensity(), 3.0);

        let mut dimension = AnnotationAdapter::default();
        dimension
            .set_tool_properties(
                AnnotationTool::Dimension,
                customised_tool_properties(AnnotationTool::Dimension),
            )
            .unwrap();
        dimension
            .begin_dimension_placement(
                7,
                0,
                MarkupId::new("dimension:tool-defaults").unwrap(),
                point(20., 20.),
            )
            .unwrap();
        dimension
            .commit_dimension_placement(7, 0, point(120., 20.), false)
            .unwrap();
        let dimension = &dimension.snapshot(7).unwrap().dimensions[0];
        assert_line_properties(dimension.appearance.line(), 3.25);
        assert_eq!(dimension.appearance.text().color(), "#336699");
        assert_eq!(dimension.appearance.text().font_size_pt(), 18.0);

        let mut snapshot = AnnotationAdapter::default();
        snapshot
            .set_tool_properties(
                AnnotationTool::Snapshot,
                customised_tool_properties(AnnotationTool::Snapshot),
            )
            .unwrap();
        snapshot.set_tool(AnnotationTool::Snapshot).unwrap();
        snapshot.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        snapshot.set_snapshot_capture_asset(
            DecodedRgbaAsset::new(1, 1, vec![255, 255, 255, 255]).unwrap(),
        );
        snapshot
            .pointer_down(7, 0, 1, point(120., 80.), 4.)
            .unwrap();
        assert_eq!(snapshot.snapshot(7).unwrap().snapshots[0].opacity(), 0.55);
    }

    #[test]
    fn drag_creation_previews_match_committed_tool_defaults_without_history() {
        let mut callout = AnnotationAdapter::default();
        callout
            .set_tool_properties(
                AnnotationTool::Callout,
                customised_tool_properties(AnnotationTool::Callout),
            )
            .unwrap();
        callout.set_tool(AnnotationTool::Callout).unwrap();
        let callout_history_before = callout.history_depths(7);
        callout.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        callout.pointer_move(1, point(120., 80.)).unwrap();
        let callout_preview = callout.document_scene(7, 0).callouts.remove(0);
        assert!(callout_preview.draft);
        assert_eq!(callout.history_depths(7), callout_history_before);
        callout.pointer_up(1, point(120., 80.)).unwrap();
        assert_eq!(
            callout.snapshot(7).unwrap().callouts[0].appearance,
            callout_preview.appearance
        );

        let mut cloud_plus = AnnotationAdapter::default();
        cloud_plus
            .set_tool_properties(
                AnnotationTool::CloudPlus,
                customised_tool_properties(AnnotationTool::CloudPlus),
            )
            .unwrap();
        cloud_plus.set_tool(AnnotationTool::CloudPlus).unwrap();
        let cloud_plus_history_before = cloud_plus.history_depths(7);
        cloud_plus
            .pointer_down(7, 0, 1, point(20., 20.), 4.)
            .unwrap();
        cloud_plus.pointer_move(1, point(120., 80.)).unwrap();
        let cloud_plus_preview = cloud_plus.document_scene(7, 0).cloud_pluses.remove(0);
        assert!(cloud_plus_preview.draft);
        assert_eq!(cloud_plus_preview.border_effect_intensity, 3.0);
        assert_eq!(cloud_plus.history_depths(7), cloud_plus_history_before);
        cloud_plus.pointer_up(1, point(120., 80.)).unwrap();
        let cloud_plus_committed = &cloud_plus.snapshot(7).unwrap().cloud_pluses[0];
        assert_eq!(
            cloud_plus_committed.appearance,
            cloud_plus_preview.appearance
        );
        assert_eq!(
            cloud_plus_committed.border_effect_intensity(),
            cloud_plus_preview.border_effect_intensity
        );

        let mut dimension = AnnotationAdapter::default();
        dimension
            .set_tool_properties(
                AnnotationTool::Dimension,
                customised_tool_properties(AnnotationTool::Dimension),
            )
            .unwrap();
        let dimension_history_before = dimension.history_depths(7);
        dimension
            .begin_dimension_placement(
                7,
                0,
                MarkupId::new("dimension:preview-defaults").unwrap(),
                point(20., 20.),
            )
            .unwrap();
        dimension
            .update_dimension_placement(point(120., 20.), false)
            .unwrap();
        let dimension_preview = dimension.document_scene(7, 0).dimensions.remove(0);
        assert!(dimension_preview.draft);
        assert_eq!(dimension.history_depths(7), dimension_history_before);
        dimension
            .commit_dimension_placement(7, 0, point(120., 20.), false)
            .unwrap();
        assert_eq!(
            dimension.snapshot(7).unwrap().dimensions[0].appearance,
            dimension_preview.appearance
        );

        let mut snapshot = AnnotationAdapter::default();
        snapshot
            .set_tool_properties(
                AnnotationTool::Snapshot,
                customised_tool_properties(AnnotationTool::Snapshot),
            )
            .unwrap();
        snapshot.set_tool(AnnotationTool::Snapshot).unwrap();
        let snapshot_history_before = snapshot.history_depths(7);
        snapshot.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        snapshot.pointer_move(1, point(120., 80.)).unwrap();
        let snapshot_preview = snapshot.document_scene(7, 0).snapshots.remove(0);
        assert!(snapshot_preview.draft);
        assert_eq!(snapshot_preview.opacity, 0.55);
        assert_eq!(snapshot.history_depths(7), snapshot_history_before);
        snapshot.set_snapshot_capture_asset(
            DecodedRgbaAsset::new(1, 1, vec![255, 255, 255, 255]).unwrap(),
        );
        snapshot
            .pointer_down(7, 0, 1, point(120., 80.), 4.)
            .unwrap();
        assert_eq!(
            snapshot.snapshot(7).unwrap().snapshots[0].opacity(),
            snapshot_preview.opacity
        );
    }

    #[test]
    fn multi_click_previews_match_committed_tool_defaults_without_history() {
        let mut polygon = AnnotationAdapter::default();
        polygon
            .set_tool_properties(
                AnnotationTool::Polygon,
                customised_tool_properties(AnnotationTool::Polygon),
            )
            .unwrap();
        polygon.set_tool(AnnotationTool::Polygon).unwrap();
        let polygon_history_before = polygon.history_depths(7);
        for (pointer_id, vertex) in [
            (1, point(20., 20.)),
            (2, point(100., 20.)),
            (3, point(100., 80.)),
        ] {
            polygon.pointer_down(7, 0, pointer_id, vertex, 4.).unwrap();
        }
        let polygon_preview = polygon.document_scene(7, 0).vertex_paths.remove(0);
        assert!(polygon_preview.draft);
        assert_eq!(polygon.history_depths(7), polygon_history_before);
        polygon.finish_vertex_path(7).unwrap();
        assert_eq!(
            polygon.snapshot(7).unwrap().vertex_paths[0].appearance,
            polygon_preview.appearance
        );

        let mut cloud = AnnotationAdapter::default();
        cloud
            .set_tool_properties(
                AnnotationTool::Cloud,
                customised_tool_properties(AnnotationTool::Cloud),
            )
            .unwrap();
        cloud.set_tool(AnnotationTool::Cloud).unwrap();
        let cloud_history_before = cloud.history_depths(7);
        for (pointer_id, vertex) in [
            (1, point(20., 20.)),
            (2, point(100., 20.)),
            (3, point(100., 80.)),
        ] {
            cloud.pointer_down(7, 0, pointer_id, vertex, 4.).unwrap();
        }
        let cloud_preview = cloud.document_scene(7, 0).clouds.remove(0);
        assert!(cloud_preview.draft);
        assert_eq!(cloud_preview.border_effect_intensity, 3.0);
        assert_eq!(cloud.history_depths(7), cloud_history_before);
        cloud.finish_cloud(7).unwrap();
        let cloud_committed = &cloud.snapshot(7).unwrap().clouds[0];
        assert_eq!(cloud_committed.appearance, cloud_preview.appearance);
        assert_eq!(
            cloud_committed.border_effect_intensity(),
            cloud_preview.border_effect_intensity
        );

        let mut cloud_plus = AnnotationAdapter::default();
        cloud_plus
            .set_tool_properties(
                AnnotationTool::CloudPlus,
                customised_tool_properties(AnnotationTool::CloudPlus),
            )
            .unwrap();
        cloud_plus.set_tool(AnnotationTool::CloudPlus).unwrap();
        let cloud_plus_history_before = cloud_plus.history_depths(7);
        cloud_plus
            .pointer_down(7, 0, 1, point(20., 20.), 4.)
            .unwrap();
        cloud_plus.pointer_up(1, point(20., 20.)).unwrap();
        cloud_plus
            .pointer_down(7, 0, 2, point(100., 20.), 4.)
            .unwrap();
        cloud_plus
            .pointer_down(7, 0, 3, point(100., 80.), 4.)
            .unwrap();
        let cloud_plus_preview = cloud_plus.document_scene(7, 0).cloud_pluses.remove(0);
        assert!(cloud_plus_preview.draft);
        assert_eq!(cloud_plus_preview.border_effect_intensity, 3.0);
        assert_eq!(cloud_plus.history_depths(7), cloud_plus_history_before);
        cloud_plus.finish_cloud_plus(7).unwrap();
        let cloud_plus_committed = &cloud_plus.snapshot(7).unwrap().cloud_pluses[0];
        assert_eq!(
            cloud_plus_committed.appearance,
            cloud_plus_preview.appearance
        );
        assert_eq!(
            cloud_plus_committed.border_effect_intensity(),
            cloud_plus_preview.border_effect_intensity
        );

        let mut area = AnnotationAdapter::default();
        area.set_document_page_length_calibration(
            7,
            0,
            LengthCalibration::new(1.0, "mm", "Scale", true).unwrap(),
        )
        .unwrap();
        area.set_tool_properties(
            AnnotationTool::Area,
            customised_tool_properties(AnnotationTool::Area),
        )
        .unwrap();
        area.set_tool(AnnotationTool::Area).unwrap();
        let area_history_before = area.history_depths(7);
        for (pointer_id, vertex) in [
            (1, point(20., 20.)),
            (2, point(100., 20.)),
            (3, point(100., 80.)),
        ] {
            area.pointer_down(7, 0, pointer_id, vertex, 4.).unwrap();
        }
        let area_preview = area.document_scene(7, 0).measurement_paths.remove(0);
        assert!(area_preview.draft);
        assert_eq!(area.history_depths(7), area_history_before);
        area.finish_measurement_path(7).unwrap();
        assert_eq!(
            area.snapshot(7).unwrap().measurement_paths[0].appearance,
            area_preview.appearance
        );
    }

    #[test]
    fn changing_tool_defaults_preserves_document_history_and_adapter_isolation() {
        let mut adapter = AnnotationAdapter::default();
        create_drag_annotation(&mut adapter, AnnotationTool::Rectangle);
        let before = adapter.snapshot(7).unwrap();

        for tool in [
            AnnotationTool::Rectangle,
            AnnotationTool::Pen,
            AnnotationTool::Highlight,
            AnnotationTool::CloudPlus,
            AnnotationTool::TextBox,
            AnnotationTool::Snapshot,
        ] {
            adapter
                .set_tool_properties(tool, customised_tool_properties(tool))
                .unwrap();
        }
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(adapter.history_depths(7), (1, 0));

        let mut invalid = adapter.tool_properties(AnnotationTool::Highlight);
        invalid.width_pt = 0.5;
        assert!(
            adapter
                .set_tool_properties(AnnotationTool::Highlight, invalid)
                .is_err()
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        let other = AnnotationAdapter::default();
        assert_eq!(
            other.tool_properties(AnnotationTool::Highlight),
            ToolProperties::for_tool(AnnotationTool::Highlight)
        );
    }

    #[test]
    fn primary_selection_edits_preserve_non_primary_annotations_and_selection_order() {
        let mut adapter = AnnotationAdapter::default();
        create_drag_annotation(&mut adapter, AnnotationTool::Rectangle);
        create_drag_annotation(&mut adapter, AnnotationTool::Rectangle);
        let initial = adapter.snapshot(7).unwrap();
        let primary_id = initial.rectangles[0].id.clone();
        let secondary_id = initial.rectangles[1].id.clone();
        let secondary_before = initial.rectangles[1].clone();
        {
            let document = adapter.documents.get_mut(&7).unwrap();
            assert!(document.select(&primary_id));
            assert!(document.toggle_selection(&secondary_id));
        }
        let selection_before = adapter.selected_ids(7).to_vec();
        let order_before = adapter.snapshot(7).unwrap().annotation_order;
        let history_before = adapter.history_depths(7);

        assert_eq!(
            adapter
                .primary_selected_annotation(7)
                .map(|annotation| annotation.id().clone()),
            Some(primary_id.clone())
        );
        let replacement_rect = PdfRect::new(30., 40., 120., 90.).unwrap();
        adapter
            .edit_primary_selected_annotation(
                7,
                &primary_id,
                AnnotationEdit::SetRectangleRect(replacement_rect),
            )
            .unwrap();
        let edited = adapter.snapshot(7).unwrap();
        assert_eq!(edited.rectangles[0].rect, replacement_rect);
        assert_eq!(edited.rectangles[1], secondary_before);
        assert_eq!(adapter.selected_ids(7), selection_before);
        assert_eq!(edited.annotation_order, order_before);
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));

        let before_stale = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter.edit_primary_selected_annotation(
                7,
                &secondary_id,
                AnnotationEdit::SetRectangleRect(PdfRect::new(1., 1., 20., 20.).unwrap()),
            ),
            Err(AnnotationError::NoSelection)
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before_stale);

        adapter
            .set_primary_selected_locked(7, &primary_id, true)
            .unwrap();
        let locked = adapter.snapshot(7).unwrap();
        assert!(locked.rectangles[0].locked);
        assert!(!locked.rectangles[1].locked);
        assert_eq!(adapter.selected_ids(7), selection_before);
        assert_eq!(locked.annotation_order, order_before);
        assert_eq!(adapter.history_depths(7), (history_before.0 + 2, 0));
    }

    #[test]
    fn construction_grid_and_dimension_increment_change_creation_geometry() {
        use crate::semantic_snapping::{SemanticSnapRole, SemanticSnapSource};

        let points_per_mm = 72. / 25.4;
        let base_settings = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::PageGrid, false);

        let mut grid = AnnotationAdapter::default();
        grid.set_tool(AnnotationTool::Line).unwrap();
        grid.set_semantic_snap_settings(
            base_settings
                .with_source(SemanticSnapSource::ConstructionGrid, true)
                .with_construction_grid_spacing_mm(10.),
        )
        .unwrap();
        grid.set_semantic_snap_page_size(7, 0, 300., 200.);
        grid.pointer_down(7, 0, 1, point(3. * points_per_mm, 3. * points_per_mm), 4.)
            .unwrap();
        grid.pointer_move(1, point(10.4 * points_per_mm, 20.3 * points_per_mm))
            .unwrap();
        let line = grid.document_scene(7, 0).straight_lines.remove(0);
        assert!((line.end.x - 10. * points_per_mm).abs() < 0.000_001);
        assert!((line.end.y - 20. * points_per_mm).abs() < 0.000_001);
        assert_eq!(
            grid.semantic_snap_decision().map(|decision| decision.role),
            Some(SemanticSnapRole::Intersection)
        );

        let mut dimension = AnnotationAdapter::default();
        dimension.set_tool(AnnotationTool::Dimension).unwrap();
        dimension
            .set_semantic_snap_settings(
                base_settings
                    .with_source(SemanticSnapSource::ConstructionGrid, false)
                    .with_dimension_increment_enabled(true)
                    .with_dimension_increment_mm(5.),
            )
            .unwrap();
        dimension
            .begin_dimension_placement(
                9,
                0,
                MarkupId::new("dimension:increment").unwrap(),
                point(0., 0.),
            )
            .unwrap();
        dimension
            .update_dimension_placement(point(13.2 * points_per_mm, 0.), false)
            .unwrap();
        let draft = dimension.document_scene(9, 0).dimensions.remove(0);
        assert!((draft.end.x - 15. * points_per_mm).abs() < 0.000_001);
        assert_eq!(draft.end.y, 0.);
    }
}
