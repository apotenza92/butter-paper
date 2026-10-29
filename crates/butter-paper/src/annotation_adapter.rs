//! Native-input adapter for the GPUI annotation surface.
//!
//! The interface accepts page-local PDF points and returns authoritative
//! document/thumbnail scenes. GPUI event capture, focus, and paint stay in the
//! binary; command ordering, typed tools, gesture lifetime, frozen defaults,
//! history, and selection live here.

use std::{collections::HashMap, sync::Arc};

#[path = "tool_properties.rs"]
pub mod tool_properties;
pub use tool_properties::{
    TOOL_FONT_FAMILY_OPTIONS, ToolFontFamilyOption, ToolProperties, ToolPropertyField,
    ToolPropertyRange,
};

use crate::annotation_model::{
    Annotation, AnnotationCommand, AnnotationDocument, AnnotationEdit, AnnotationError,
    AnnotationKind, AnnotationScene, AnnotationSelectionSupplement, AnnotationSnapshot,
    ArcAnnotation, ArcControlPoint, CalloutAnnotation, CalloutAppearance, CloudAnnotation,
    CloudPlusAnnotation, CloudPlusAppearance, CommandOutcome, DecodedRgbaAsset,
    DimensionAnnotation, DimensionAppearance, EllipseAnnotation, GestureKind, HitTarget,
    ImageAnnotation, InkTool, LengthAnnotation, LengthCalibration, LengthEndpoint, LineEndpoint,
    LineKind, MarkupId, MeasurementPathAnnotation, MeasurementPathKind, PageRotation,
    PageRotationDirection, PageScale, PdfPoint, PdfRect, PenAnnotation, PenAppearance,
    PointerCancelReason, PointerTool, RectangleAnnotation, RectangleAppearance,
    RectangleResizeHandle, RedactAnnotation, RetainedAnnotationObstacle, SceneAnnotation, SceneArc,
    SceneCallout, SceneCloud, SceneCloudPlus, SceneDimension, SceneInteractionFeedback,
    SceneLength, SceneMeasurementPath, SceneRectangle, SceneRedact, SceneSnapshot,
    SceneStraightLine, SceneVertexPath, SnapshotAnnotation, SpatialQueryWork,
    StraightLineAnnotation, StraightLineAppearance, StrokeStyle, TextAlignment, TextBoxAnnotation,
    TextBoxStyle, VertexPathAnnotation, VertexPathKind, box_outline_resize_handle,
    ellipse_move_zone_contains, ellipse_outline_resize_handle,
};
use crate::cloud_plus_routing::{
    CloudPlusObstacle, CloudPlusRoutingContext, place_initial_cloud_plus_text_box,
    route_cloud_plus_leader, snap_cloud_plus_leader_tip,
};
use crate::density_fixture::{DensityFixtureImportOutcome, materialize_density_fixture};
use crate::native_editing_v5::{
    InclusiveLInfGridSnap, NativeEditingV5Error, PropertyEditCommit, PropertyEditPlan,
    SnapGestureCommit, SnapResolution, SnapTransformPlan, StrokeWidthEditTransaction, Translation,
};
use crate::pdf_content_geometry::PageSnapGeometry;
use crate::pdf_text_edit::PageTextEdit;
use crate::selection_geometry::{
    SelectionMarquee, SelectionOperation, SelectionPoint, SelectionShape,
};
use crate::semantic_snapping::{
    AcquiredTrackingPoint, ObjectSnapTrackingResult, OrthogonalAxis, PageGridDefinition,
    RelationshipSnapGuide, SemanticSnapDecision, SemanticSnapError, SemanticSnapIndex,
    SemanticSnapSettings, SnapGuideRect, annotation_guide_rects, combined_guide_bounds,
    find_equal_size_snap, find_equal_spacing_snap, find_object_snap_tracking_point,
    moving_annotation_snap_anchor_points,
    moving_annotation_snap_anchor_points_with_selection_supplement,
    quantize_pdf_distance_to_mm_increment, resolve_construction_grid_point,
    toggle_acquired_tracking_point,
};

pub const FROZEN_TEXT_CREATE: &str = "Beam B-12 / revision 3";
pub const NATURAL_IMAGE_MAX_PAGE_FRACTION: f64 = 0.45;
pub const IMAGE_PLACEMENT_PREVIEW_OPACITY: f64 = 0.45;
const TEXT_WIDTH_PT: f64 = 240.0;
const TEXT_HEIGHT_PT: f64 = 72.0;
const HIGHLIGHT_MIN_DISTANCE_PT: f64 = 0.5;
const POINTER_DRAG_THRESHOLD_CSS_PX: f64 = 3.0;
const CAPTION_MIN_WIDTH_PT: f64 = 24.0;
const CAPTION_HEIGHT_PT: f64 = 44.0;
const CALLOUT_DEFAULT_TEXT: &str = "Callout";
const CLOUD_PLUS_TEXT_HEIGHT_PT: f64 = 44.0;
const CLOUD_PLUS_TEXT_GAP_PT: f64 = 24.0;
const LENGTH_MINIMUM_PDF_DISTANCE: f64 = 2.0;
const ARC_MINIMUM_BULGE_CSS_PX: f64 = 8.0;
pub const ROTATION_HANDLE_OFFSET_CSS_PX: f64 = crate::annotation_model::ROTATION_HANDLE_OFFSET_PT;
pub const ELLIPSE_ROTATION_HANDLE_ID: &str = "ellipse.rotate";
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

pub const fn callout_resize_handle_id(handle: RectangleResizeHandle) -> &'static str {
    match handle {
        RectangleResizeHandle::NorthWest => "callout.textBox.resize.nw",
        RectangleResizeHandle::North => "callout.textBox.resize.n",
        RectangleResizeHandle::NorthEast => "callout.textBox.resize.ne",
        RectangleResizeHandle::East => "callout.textBox.resize.e",
        RectangleResizeHandle::SouthEast => "callout.textBox.resize.se",
        RectangleResizeHandle::South => "callout.textBox.resize.s",
        RectangleResizeHandle::SouthWest => "callout.textBox.resize.sw",
        RectangleResizeHandle::West => "callout.textBox.resize.w",
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

pub fn text_box_resize_handle_point(
    annotation: &TextBoxAnnotation,
    handle: RectangleResizeHandle,
) -> PdfPoint {
    handle.world_point(annotation.layout_rect, annotation.rotation_degrees())
}

pub fn text_box_rotation_handle_point(
    annotation: &TextBoxAnnotation,
    observed_pixels_per_point: f64,
) -> Result<PdfPoint, AnnotationError> {
    ellipse_rotation_handle_point_for_rect(
        annotation.layout_rect,
        annotation.rotation_degrees(),
        observed_pixels_per_point,
    )
}

pub fn image_rotation_handle_point(
    annotation: &ImageAnnotation,
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
    select_after_placement: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PendingImagePreview {
    pub document_id: u64,
    pub page_index: u32,
    pub rect: PdfRect,
    pub asset_id: String,
    pub opacity: f64,
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
        self != Self::Select
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
        snap_anchor_points: Vec<PdfPoint>,
        excluded_ids: Vec<MarkupId>,
        snap_caption_supplement: AnnotationSelectionSupplement,
    },
    Domain {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        ink: bool,
        ink_start: Option<PdfPoint>,
        rectangle_translation_start: Option<PdfPoint>,
        rectangle_resize_handle: Option<RectangleResizeHandle>,
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
        /// From the press to its handle, so the grabbed point of the curve
        /// follows the pointer.
        grab_offset: (f64, f64),
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
    ImageCreate {
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
        click_placement_pending: bool,
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
        snap_anchor_points: Vec<PdfPoint>,
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
    InkResize {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        handle: RectangleResizeHandle,
        start: PdfPoint,
        current: PdfPoint,
        original_paths: Vec<Vec<PdfPoint>>,
        original_box: InkResizeBox,
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
        original_rotation_degrees: f64,
    },
    TextBoxRotate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
        original_rotation_degrees: f64,
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
        original_rotation_degrees: f64,
        aspect_locked: bool,
    },
    ImageRotate {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_rect: PdfRect,
        original_rotation_degrees: f64,
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
    LengthMove {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        start: PdfPoint,
        current: PdfPoint,
        original_start: PdfPoint,
        original_end: PdfPoint,
        snap_anchor_points: Vec<PdfPoint>,
        snap_caption_supplement: AnnotationSelectionSupplement,
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
        snap_anchor_points: Vec<PdfPoint>,
        snap_caption_supplement: AnnotationSelectionSupplement,
        /// From the press to the measured point a Start or End edit grabbed.
        grab_offset: (f64, f64),
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
        snap_anchor_points: Vec<PdfPoint>,
    },
    CloudPlusEdit {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        expected_revision: u64,
        kind: CloudPlusPointerEditKind,
        start: PdfPoint,
        current: PdfPoint,
        original: CloudPlusAnnotation,
        snap_anchor_points: Vec<PdfPoint>,
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
        snap_anchor_points: Vec<PdfPoint>,
    },
    LengthEndpoint {
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        id: MarkupId,
        endpoint: LengthEndpoint,
        current: PdfPoint,
        /// From the press to the measured point it grabbed.
        grab_offset: (f64, f64),
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
    TextBoxResize(RectangleResizeHandle),
    LeaderPoint(usize),
    TextBox,
    Body,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudPlusPointerEditKind {
    CloudVertex(usize),
    TextBoxResize(RectangleResizeHandle),
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

impl ImageResizeHandle {
    const ALL: [Self; 8] = [
        Self::SouthWest,
        Self::South,
        Self::SouthEast,
        Self::East,
        Self::NorthEast,
        Self::North,
        Self::NorthWest,
        Self::West,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EllipseHandleKind {
    Resize(RectangleResizeHandle),
    Rotate,
}

#[derive(Clone, Copy, Debug)]
enum EqualSizeResizeGeometry {
    Ellipse {
        grab_offset: (f64, f64),
    },
    Redact,
    Rectangle,
    Image {
        handle: ImageResizeHandle,
        start: PdfPoint,
        aspect_locked: bool,
    },
}

#[derive(Clone, Debug)]
struct EqualSizeResizeTarget {
    document_id: u64,
    page_index: u32,
    id: MarkupId,
    handle: RectangleResizeHandle,
    original_rect: PdfRect,
    geometry: EqualSizeResizeGeometry,
}

impl EqualSizeResizeTarget {
    fn rect_at(&self, point: PdfPoint) -> Option<PdfRect> {
        match self.geometry {
            EqualSizeResizeGeometry::Ellipse { grab_offset } => Some(ellipse_resized_rect(
                self.original_rect,
                0.,
                self.handle,
                offset_point(point, grab_offset),
            )),
            EqualSizeResizeGeometry::Redact => {
                redact_resized_rect(self.original_rect, self.handle, point).ok()
            }
            EqualSizeResizeGeometry::Rectangle => Some(
                self.original_rect
                    .rotated_resize_from_handle(0., self.handle, point),
            ),
            EqualSizeResizeGeometry::Image {
                handle,
                start,
                aspect_locked,
            } => {
                resized_image_rect(self.original_rect, handle, start, point, 0., aspect_locked).ok()
            }
        }
    }
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

/// Page content an editor is changing, whose markups a scene shows moved
/// with it: content being dragged (moved or resized).
#[derive(Clone, Copy, Default)]
pub struct ContentPreview<'a> {
    pub dragged: Option<(
        &'a [Vec<crate::pdf_content_moves::MovedContent>],
        &'a [MarkupId],
        crate::pdf_content_moves::ContentChange,
    )>,
}

#[derive(Default)]
pub struct AnnotationAdapter {
    tool: AnnotationTool,
    documents: HashMap<u64, AnnotationDocument>,
    active: Option<ActivePointer>,
    vertex_path_draft: Option<VertexPathDraft>,
    cloud_draft: Option<VertexPathDraft>,
    cloud_drag_pointer: Option<u64>,
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
    acquired_tracking_points: Vec<AcquiredTrackingPoint>,
    tracking_hover_key: Option<(i64, i64)>,
    object_snap_tracking_result: Option<ObjectSnapTrackingResult>,
    relationship_snap_guides: Vec<RelationshipSnapGuide>,
    semantic_snap_page_sizes: HashMap<(u64, u32), (f64, f64)>,
    semantic_snap_page_grids: HashMap<(u64, u32), PageGridDefinition>,
    semantic_snap_page_content: HashMap<(u64, u32), Arc<SemanticSnapIndex>>,
    retained_annotation_obstacles: HashMap<u64, Vec<RetainedAnnotationObstacle>>,
    observed_pixels_per_point: ObservedPixelsPerPoint,
    /// Equal-size snap references for one committed page, keyed by document,
    /// page and revision, so a drag computes them once rather than per move.
    committed_guide_rects: Option<((u64, u32, u64), Arc<Vec<SnapGuideRect>>)>,
    /// Annotation snap data for the page under an active gesture; see
    /// `annotation_snap_cache`.
    annotation_snap: Option<AnnotationSnapCache>,
}

type AnnotationSnapKey = (u64, u32, u64, Vec<MarkupId>, AnnotationSelectionSupplement);

/// Annotation snap targets for one page and exclusion set. Built from the
/// page's document scene, whose in-flight previews and drafts are either
/// flagged (and so never snap targets) or excluded by id, so the data only
/// changes with the committed revision. A drag therefore builds it once
/// instead of on every pointer move.
struct AnnotationSnapCache {
    key: AnnotationSnapKey,
    index: Arc<SemanticSnapIndex>,
    /// Targets beside the markups' strokes for a line of the given width.
    stroke_offsets: Option<(f64, Arc<SemanticSnapIndex>)>,
    references: Option<Vec<SnapGuideRect>>,
    initial_bounds: Option<Option<PdfRect>>,
}

impl AnnotationAdapter {
    /// Guide rectangles of a page's committed annotations, excluding
    /// `excluded`. Matches `annotation_guide_rects` over the page's
    /// thumbnail scene with no selection supplement.
    fn committed_guide_rects(
        &mut self,
        document_id: u64,
        page_index: u32,
        excluded: &[MarkupId],
    ) -> Vec<SnapGuideRect> {
        let Some(document) = self.documents.get(&document_id) else {
            return Vec::new();
        };
        let key = (document_id, page_index, document.revision());
        let rects = match &self.committed_guide_rects {
            Some((cached, rects)) if *cached == key => rects.clone(),
            _ => {
                let rects = Arc::new(annotation_guide_rects(
                    &document.thumbnail_scene(page_index),
                    &[],
                    &AnnotationSelectionSupplement::new(),
                ));
                self.committed_guide_rects = Some((key, rects.clone()));
                rects
            }
        };
        rects
            .iter()
            .filter(|guide| !excluded.contains(&guide.owner_id))
            .cloned()
            .collect()
    }

    /// Encodes one document's committed annotation model and exact undo/redo
    /// timeline. Adapter-owned pointer and placement state is intentionally
    /// excluded from the recovery payload.
    pub fn encode_document_recovery_timeline(
        &self,
        document_id: u64,
    ) -> Result<Vec<u8>, AnnotationError> {
        self.documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .encode_recovery_timeline()
    }

    /// Atomically replaces one document from a validated recovery timeline.
    ///
    /// Decoding and adapter-specific identity validation complete before the
    /// existing document or any interaction state is changed. On success,
    /// stale pointer/draft/placement state for the restored document is
    /// discarded and generated comparison IDs advance beyond identities that
    /// survive only in undo or redo history.
    pub fn restore_document_recovery_timeline(
        &mut self,
        document_id: u64,
        bytes: &[u8],
    ) -> Result<(), AnnotationError> {
        let document = AnnotationDocument::hydrate_recovery_timeline(bytes)?;
        let recovered_sequence = self.replacement_sequence_floor(document_id, &document)?;

        self.clear_document_recovery_transients(document_id);
        self.next_sequence = self.next_sequence.max(recovered_sequence);
        self.documents.insert(document_id, document);
        Ok(())
    }

    fn replacement_sequence_floor(
        &self,
        document_id: u64,
        replacement: &AnnotationDocument,
    ) -> Result<u64, AnnotationError> {
        let max_sequence = self
            .documents
            .iter()
            .filter(|(existing_document_id, _)| **existing_document_id != document_id)
            .map(|(_, document)| document)
            .chain(std::iter::once(replacement))
            .flat_map(AnnotationDocument::recovery_markup_ids)
            .filter_map(|id| comparison_sequence(&id))
            .chain(self.queued_id.as_ref().and_then(comparison_sequence))
            .chain(std::iter::once(self.next_sequence))
            .max()
            .unwrap_or_default();
        if max_sequence == u64::MAX {
            return Err(AnnotationError::InvalidRecoveryTimeline(
                "comparison markup sequence is exhausted".into(),
            ));
        }
        Ok(max_sequence)
    }

    fn clear_document_recovery_transients(&mut self, document_id: u64) {
        if self
            .vertex_path_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
        {
            self.vertex_path_draft = None;
        }
        if self
            .cloud_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
        {
            self.cloud_draft = None;
            self.cloud_drag_pointer = None;
        }
        if self
            .measurement_path_draft
            .as_ref()
            .is_some_and(|draft| draft.document_id == document_id)
        {
            self.measurement_path_draft = None;
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
        } else if self.snapshot_draft.is_none() {
            self.snapshot_capture_asset = None;
        }

        // Pending Image placement has no document owner before its click is
        // handled. It cannot safely survive a model replacement because the
        // next click could apply pre-recovery state to the recovered document.
        self.image_asset = None;
        self.image_placement_page = None;
        self.queued_id = None;
        self.queued_rectangle_appearance = None;
        self.queued_text_content = None;
        self.semantic_snap_decision = None;

        if self
            .active
            .as_ref()
            .is_some_and(|active| active_pointer_document_id(active) == document_id)
        {
            self.active = None;
        }
        if self
            .native_v5_snap_observation
            .as_ref()
            .is_some_and(|observation| observation.document_id == document_id)
        {
            self.native_v5_snap_observation = None;
        }
        if self
            .native_v5_property_receipt
            .as_ref()
            .is_some_and(|(receipt_document_id, _)| *receipt_document_id == document_id)
        {
            self.native_v5_property_receipt = None;
        }
        if self
            .native_v5_snap_receipt
            .as_ref()
            .is_some_and(|(receipt_document_id, _)| *receipt_document_id == document_id)
        {
            self.native_v5_snap_receipt = None;
        }
        self.retained_annotation_obstacles.remove(&document_id);
    }

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
        let sequence_floor = self.replacement_sequence_floor(document_id, &document)?;
        if self
            .active_surface()
            .is_some_and(|(active_document_id, _)| active_document_id == document_id)
        {
            self.cancel(PointerCancelReason::PageChanged)?;
        }
        self.next_sequence = self.next_sequence.max(sequence_floor);
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
        let sequence_floor = self.replacement_sequence_floor(document_id, &document)?;
        if self
            .active_surface()
            .is_some_and(|(active_document_id, _)| active_document_id == document_id)
        {
            self.cancel(PointerCancelReason::PageChanged)?;
        }
        self.next_sequence = self.next_sequence.max(sequence_floor);
        self.documents.insert(document_id, document);
        Ok(())
    }

    pub fn load_imported_annotations_with_page_scale_state(
        &mut self,
        document_id: u64,
        annotations: Vec<Annotation>,
        page_scales: Vec<(u32, PageScale)>,
        page_rotations: Vec<(u32, PageRotation)>,
    ) -> Result<(), AnnotationError> {
        let mut document = AnnotationDocument::default();
        document.load_imported_page_scale_state(annotations, page_scales, page_rotations)?;
        let sequence_floor = self.replacement_sequence_floor(document_id, &document)?;
        if self
            .active_surface()
            .is_some_and(|(active_document_id, _)| active_document_id == document_id)
        {
            self.cancel(PointerCancelReason::PageChanged)?;
        }
        self.next_sequence = self.next_sequence.max(sequence_floor);
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

    /// Rearranges a document's pages ([`AnnotationDocument::arrange_pages`]).
    pub fn arrange_document_pages(
        &mut self,
        document_id: u64,
        page_count: u32,
        pages: &[crate::annotation_model::ArrangedPage],
    ) -> Result<Vec<MarkupId>, AnnotationError> {
        let copied = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .arrange_pages(page_count, pages)?;
        // Content read for snapping names pages by where they were.
        self.clear_semantic_snap_page_content(document_id);
        Ok(copied)
    }

    /// A document's pages as arranged; empty when they are the file's.
    pub fn document_page_slots(&self, document_id: u64) -> &[crate::pdf_page_structure::PageSlot] {
        self.documents
            .get(&document_id)
            .map_or(&[], |document| document.page_slots())
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

    pub fn document_page_text_edits(&self, document_id: u64) -> &[PageTextEdit] {
        self.documents
            .get(&document_id)
            .map_or(&[], |document| document.page_text_edits())
    }

    pub fn set_document_page_text_edit(
        &mut self,
        document_id: u64,
        edit: PageTextEdit,
    ) -> Result<bool, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .set_page_text_edit(edit)
    }

    pub fn set_document_page_text_edits(
        &mut self,
        document_id: u64,
        page_index: u32,
        anchors: &[crate::pdf_text_edit::TextLineAnchor],
        edits: Vec<PageTextEdit>,
    ) -> Result<bool, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .set_page_text_edits(page_index, anchors, edits)
    }

    /// [`AnnotationDocument::set_page_content_edits`] for a document.
    pub fn set_document_page_content_edits(
        &mut self,
        document_id: u64,
        page_index: u32,
        anchors: &[crate::pdf_text_edit::TextLineAnchor],
        edits: Vec<PageTextEdit>,
        tables: Vec<(
            crate::pdf_tables::TableKey,
            Option<crate::pdf_tables::PageTableEdit>,
        )>,
    ) -> Result<bool, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .set_page_content_edits(page_index, anchors, edits, tables)
    }

    /// [`AnnotationDocument::set_pages_content_edits`] for a document.
    pub fn set_document_pages_content_edits(
        &mut self,
        document_id: u64,
        pages: Vec<crate::annotation_model::PageContentEdits>,
    ) -> Result<bool, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .set_pages_content_edits(pages)
    }

    pub fn document_page_content_moves(
        &self,
        document_id: u64,
    ) -> &[crate::pdf_content_moves::ContentMove] {
        self.documents
            .get(&document_id)
            .map_or(&[], |document| document.page_content_moves())
    }

    /// [`AnnotationDocument::change_page_content`] for a document.
    pub fn change_document_page_content(
        &mut self,
        document_id: u64,
        page_index: u32,
        groups: &[Vec<crate::pdf_content_moves::MovedContent>],
        markups: &[MarkupId],
        change: crate::pdf_content_moves::ContentChange,
    ) -> Result<bool, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .change_page_content(page_index, groups, markups, change)
    }

    /// [`AnnotationDocument::paste_page_content`] for a document.
    pub fn paste_document_page_content(
        &mut self,
        document_id: u64,
        page_index: u32,
        groups: &[Vec<crate::pdf_content_moves::MovedContent>],
        offset: (f64, f64),
    ) -> Result<Vec<Vec<crate::pdf_content_moves::MovedContent>>, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .paste_page_content(page_index, groups, offset)
    }

    pub fn document_page_table_edits(
        &self,
        document_id: u64,
    ) -> &[crate::pdf_tables::PageTableEdit] {
        self.documents
            .get(&document_id)
            .map_or(&[], |document| document.page_table_edits())
    }

    pub fn document_form_field_edits(
        &self,
        document_id: u64,
    ) -> &[crate::pdf_forms::FormFieldEdit] {
        self.documents
            .get(&document_id)
            .map_or(&[], |document| document.form_field_edits())
    }

    pub fn set_document_form_field_edit(
        &mut self,
        document_id: u64,
        edit: crate::pdf_forms::FormFieldEdit,
    ) -> Result<bool, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .set_form_field_edit(edit)
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
        self.acquired_tracking_points.clear();
        self.tracking_hover_key = None;
        self.object_snap_tracking_result = None;
        self.relationship_snap_guides.clear();
        Ok(())
    }

    pub fn semantic_snap_settings(&self) -> SemanticSnapSettings {
        self.semantic_snap_settings
    }

    pub fn semantic_snap_decision(&self) -> Option<&SemanticSnapDecision> {
        self.semantic_snap_decision.as_ref()
    }

    pub fn object_snap_tracking_result(&self) -> Option<&ObjectSnapTrackingResult> {
        self.object_snap_tracking_result.as_ref()
    }

    pub fn relationship_snap_guides(&self) -> &[RelationshipSnapGuide] {
        &self.relationship_snap_guides
    }

    pub fn clear_semantic_snap_decision(&mut self) {
        self.semantic_snap_decision = None;
        self.object_snap_tracking_result = None;
        self.relationship_snap_guides.clear();
    }

    fn update_tracking_acquisition(&mut self, decision: Option<&SemanticSnapDecision>) {
        let Some(decision) = decision.filter(|decision| decision.point_candidate) else {
            self.tracking_hover_key = None;
            return;
        };
        let key = (
            (decision.point.x * 1_000.).round() as i64,
            (decision.point.y * 1_000.).round() as i64,
        );
        if self.tracking_hover_key == Some(key) {
            return;
        }
        self.tracking_hover_key = Some(key);
        self.acquired_tracking_points = toggle_acquired_tracking_point(
            &self.acquired_tracking_points,
            AcquiredTrackingPoint {
                point: decision.point,
                source: decision.source,
                role: decision.role,
                owner_id: decision.owner_id.clone(),
            },
        );
    }

    fn acquired_tracking_for_enabled_sources(&self) -> Vec<AcquiredTrackingPoint> {
        self.acquired_tracking_points
            .iter()
            .filter(|point| self.semantic_snap_settings.is_source_enabled(point.source))
            .cloned()
            .collect()
    }

    fn manipulation_chrome_visible(&self, _document_id: u64, _page_index: u32) -> bool {
        // Snap feedback supplements selection; it never replaces it.
        true
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

    pub fn set_semantic_snap_page_grid(
        &mut self,
        document_id: u64,
        page_index: u32,
        grid: Option<PageGridDefinition>,
    ) {
        let key = (document_id, page_index);
        if let Some(grid) = grid {
            self.semantic_snap_page_grids.insert(key, grid);
        } else {
            self.semantic_snap_page_grids.remove(&key);
        }
        self.semantic_snap_decision = None;
    }

    pub fn set_semantic_snap_page_content(
        &mut self,
        document_id: u64,
        geometry: PageSnapGeometry,
    ) -> Result<(), SemanticSnapError> {
        let key = (document_id, geometry.page_index);
        // Validate candidate expansion before replacing a previously usable
        // page cache. This also keeps malformed/oversized worker results from
        // affecting annotation interaction.
        let index = SemanticSnapIndex::default().with_page_content(&geometry)?;
        self.semantic_snap_page_content.insert(key, Arc::new(index));
        self.semantic_snap_decision = None;
        Ok(())
    }

    pub fn clear_semantic_snap_page_content(&mut self, document_id: u64) {
        self.semantic_snap_page_content
            .retain(|(owner, _), _| *owner != document_id);
        self.semantic_snap_decision = None;
    }

    pub fn clear_semantic_snap_page_content_page(&mut self, document_id: u64, page_index: u32) {
        self.semantic_snap_page_content
            .remove(&(document_id, page_index));
        self.semantic_snap_decision = None;
    }

    fn annotation_snap_cache(
        &mut self,
        document_id: u64,
        page_index: u32,
        excluded_owner_ids: &[MarkupId],
        supplement: &AnnotationSelectionSupplement,
    ) -> &mut AnnotationSnapCache {
        let revision = self
            .documents
            .get(&document_id)
            .map_or(0, AnnotationDocument::revision);
        let fresh =
            self.annotation_snap.as_ref().is_some_and(|cache| {
                let (
                    cached_document,
                    cached_page,
                    cached_revision,
                    cached_excluded,
                    cached_supplement,
                ) = &cache.key;
                (*cached_document, *cached_page, *cached_revision)
                    == (document_id, page_index, revision)
                    && cached_excluded.as_slice() == excluded_owner_ids
                    && cached_supplement == supplement
            });
        if !fresh {
            let scene = self.document_scene(document_id, page_index);
            let index = SemanticSnapIndex::from_annotation_scene_with_selection_supplement(
                &scene,
                excluded_owner_ids,
                supplement,
            );
            self.annotation_snap = Some(AnnotationSnapCache {
                key: (
                    document_id,
                    page_index,
                    revision,
                    excluded_owner_ids.to_vec(),
                    supplement.clone(),
                ),
                index: Arc::new(index),
                stroke_offsets: None,
                references: None,
                initial_bounds: None,
            });
        }
        self.annotation_snap
            .as_mut()
            .expect("the annotation snap cache was just filled")
    }

    /// The width of the stroke being drawn or moved, for snapping it beside
    /// other markups' strokes: the moving markup's own, or the drawing
    /// tool's.
    fn snap_stroke_width(
        &self,
        document_id: u64,
        page_index: u32,
        excluded_owner_ids: &[MarkupId],
    ) -> Option<f64> {
        if let Some(id) = excluded_owner_ids.first() {
            let scene = self
                .documents
                .get(&document_id)?
                .thumbnail_scene(page_index);
            return scene
                .rectangles
                .iter()
                .chain(&scene.ellipses)
                .find(|item| &item.id == id)
                .map(|item| item.appearance.stroke_width_pt())
                .or_else(|| {
                    scene
                        .straight_lines
                        .iter()
                        .find(|item| &item.id == id)
                        .map(|item| item.appearance.stroke_width_pt())
                })
                .or_else(|| {
                    scene
                        .vertex_paths
                        .iter()
                        .find(|item| &item.id == id)
                        .map(|item| item.appearance.stroke_width_pt())
                })
                .filter(|width| *width > 0.);
        }
        matches!(
            self.tool,
            AnnotationTool::Rectangle
                | AnnotationTool::Ellipse
                | AnnotationTool::Line
                | AnnotationTool::Arrow
                | AnnotationTool::Polyline
                | AnnotationTool::Polygon
                | AnnotationTool::Arc
                | AnnotationTool::Length
                | AnnotationTool::Polylength
                | AnnotationTool::Area
                | AnnotationTool::Dimension
        )
        .then(|| self.tool_properties(self.tool).width_pt)
        .filter(|width| *width > 0.)
    }

    /// Annotation targets, then the page grid, then PDF content: the same
    /// candidate order as one index holding them in that sequence.
    fn semantic_snap_index(
        &mut self,
        document_id: u64,
        page_index: u32,
        excluded_owner_ids: &[MarkupId],
        supplement: &AnnotationSelectionSupplement,
    ) -> SemanticSnapIndex {
        let stroke_width = self.snap_stroke_width(document_id, page_index, excluded_owner_ids);
        let cache =
            self.annotation_snap_cache(document_id, page_index, excluded_owner_ids, supplement);
        let annotations = cache.index.clone();
        // A stroked line also snaps beside other markups' strokes.
        let stroke_offsets = stroke_width.map(|width| match &cache.stroke_offsets {
            Some((cached, offsets)) if *cached == width => offsets.clone(),
            _ => {
                let offsets = Arc::new(annotations.stroke_offsets(width));
                cache.stroke_offsets = Some((width, offsets.clone()));
                offsets
            }
        });
        let mut index = SemanticSnapIndex::default().with_shared_index(annotations);
        if let Some(offsets) = stroke_offsets {
            index = index.with_shared_index(offsets);
        }
        if let Some(grid) = self
            .semantic_snap_page_grids
            .get(&(document_id, page_index))
        {
            index = index.with_shared_index(Arc::new(
                SemanticSnapIndex::default()
                    .with_page_grid(grid)
                    .expect("stored page-grid geometry was validated before installation"),
            ));
        }
        if let Some(content) = self
            .semantic_snap_page_content
            .get(&(document_id, page_index))
        {
            index = index.with_shared_index(content.clone());
        }
        index
    }

    /// Committed bounds of the moving annotations, from guide rectangles or,
    /// failing those, the anchors.
    fn moving_initial_bounds(
        &self,
        document_id: u64,
        page_index: u32,
        anchors: &[PdfPoint],
        excluded_ids: &[MarkupId],
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Option<PdfRect> {
        self.documents
            .get(&document_id)
            .and_then(|document| {
                let committed_scene = document.thumbnail_scene(page_index);
                let points = annotation_guide_rects(&committed_scene, &[], caption_supplement)
                    .into_iter()
                    .filter(|guide| excluded_ids.contains(&guide.owner_id))
                    .flat_map(|guide| {
                        let rect = guide.rect;
                        [
                            PdfPoint {
                                x: rect.x,
                                y: rect.y,
                            },
                            PdfPoint {
                                x: rect.x + rect.width,
                                y: rect.y + rect.height,
                            },
                        ]
                    })
                    .collect::<Vec<_>>();
                combined_guide_bounds(&points)
            })
            .or_else(|| combined_guide_bounds(anchors))
    }

    fn cached_moving_initial_bounds(
        &mut self,
        document_id: u64,
        page_index: u32,
        anchors: &[PdfPoint],
        excluded_ids: &[MarkupId],
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Option<PdfRect> {
        if let Some(bounds) = self
            .annotation_snap_cache(document_id, page_index, excluded_ids, caption_supplement)
            .initial_bounds
        {
            return bounds;
        }
        let bounds = self.moving_initial_bounds(
            document_id,
            page_index,
            anchors,
            excluded_ids,
            caption_supplement,
        );
        self.annotation_snap_cache(document_id, page_index, excluded_ids, caption_supplement)
            .initial_bounds = Some(bounds);
        bounds
    }

    fn cached_moving_guide_references(
        &mut self,
        document_id: u64,
        page_index: u32,
        excluded_ids: &[MarkupId],
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Vec<SnapGuideRect> {
        if let Some(references) = &self
            .annotation_snap_cache(document_id, page_index, excluded_ids, caption_supplement)
            .references
        {
            return references.clone();
        }
        let references = annotation_guide_rects(
            &self.document_scene(document_id, page_index),
            excluded_ids,
            caption_supplement,
        );
        self.annotation_snap_cache(document_id, page_index, excluded_ids, caption_supplement)
            .references = Some(references.clone());
        references
    }

    pub fn set_retained_annotation_obstacles(
        &mut self,
        document_id: u64,
        mut obstacles: Vec<RetainedAnnotationObstacle>,
    ) {
        obstacles.sort_by(|left, right| {
            (left.page_index, left.id.as_str()).cmp(&(right.page_index, right.id.as_str()))
        });
        self.retained_annotation_obstacles
            .insert(document_id, obstacles);
    }

    fn cloud_plus_routing_context(
        &self,
        document_id: u64,
        page_index: u32,
        exclude_id: Option<&MarkupId>,
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> CloudPlusRoutingContext {
        let page_bounds = self
            .semantic_snap_page_sizes
            .get(&(document_id, page_index))
            .and_then(|(width, height)| PdfRect::new(0., 0., *width, *height).ok());
        let mut obstacles = self
            .retained_annotation_obstacles
            .get(&document_id)
            .into_iter()
            .flatten()
            .filter(|obstacle| obstacle.page_index == page_index)
            .map(|obstacle| CloudPlusObstacle::Rect {
                id: Some(obstacle.id.clone()),
                rect: obstacle.rect,
            })
            .collect::<Vec<_>>();
        let Some(document) = self.documents.get(&document_id) else {
            return CloudPlusRoutingContext {
                page_bounds,
                obstacles,
            };
        };

        let mut annotations = document
            .document_scene(page_index)
            .into_ordered_annotations()
            .filter(|annotation| exclude_id.is_none_or(|id| annotation.id() != id))
            .collect::<Vec<_>>();
        annotations.sort_by(|left, right| left.id().as_str().cmp(right.id().as_str()));

        for annotation in annotations {
            let owner = annotation.id().as_str().to_owned();
            match annotation {
                SceneAnnotation::Rectangle(annotation) | SceneAnnotation::Ellipse(annotation) => {
                    obstacles.push(CloudPlusObstacle::Rect {
                        id: Some(owner),
                        rect: annotation.rect,
                    });
                }
                SceneAnnotation::Redact(annotation) => {
                    obstacles.push(CloudPlusObstacle::Rect {
                        id: Some(owner),
                        rect: annotation.rect,
                    });
                }
                SceneAnnotation::Arc(annotation) => {
                    obstacles.push(CloudPlusObstacle::Rect {
                        id: Some(owner),
                        rect: annotation.rect,
                    });
                }
                SceneAnnotation::StraightLine(annotation) => {
                    obstacles.push(CloudPlusObstacle::Polyline {
                        id: Some(owner),
                        points: vec![annotation.start, annotation.end],
                    });
                }
                SceneAnnotation::VertexPath(annotation) => match annotation.kind {
                    VertexPathKind::Polyline => obstacles.push(CloudPlusObstacle::Polyline {
                        id: Some(owner),
                        points: annotation.points,
                    }),
                    VertexPathKind::Polygon => obstacles.push(CloudPlusObstacle::Polygon {
                        id: Some(owner),
                        points: annotation.points,
                    }),
                },
                SceneAnnotation::Cloud(annotation) => {
                    obstacles.push(CloudPlusObstacle::Polygon {
                        id: Some(owner),
                        points: annotation.points,
                    });
                }
                SceneAnnotation::CloudPlus(annotation) => {
                    obstacles.extend([
                        CloudPlusObstacle::Rect {
                            id: Some(format!("{owner}:text")),
                            rect: annotation.text_box,
                        },
                        CloudPlusObstacle::Polyline {
                            id: Some(format!("{owner}:leader")),
                            points: annotation.leader_points,
                        },
                        CloudPlusObstacle::Polygon {
                            id: Some(format!("{owner}:cloud")),
                            points: annotation.scallop_path,
                        },
                    ]);
                }
                SceneAnnotation::Callout(annotation) => {
                    obstacles.extend([
                        CloudPlusObstacle::Rect {
                            id: Some(format!("{owner}:text")),
                            rect: annotation.text_box,
                        },
                        CloudPlusObstacle::Polyline {
                            id: Some(format!("{owner}:leader")),
                            points: annotation.leader_points,
                        },
                    ]);
                }
                SceneAnnotation::MeasurementPath(annotation) => match annotation.kind {
                    MeasurementPathKind::Polylength => {
                        obstacles.push(CloudPlusObstacle::Polyline {
                            id: Some(owner),
                            points: annotation.points,
                        });
                    }
                    MeasurementPathKind::Area => obstacles.push(CloudPlusObstacle::Polygon {
                        id: Some(owner),
                        points: annotation.points,
                    }),
                },
                SceneAnnotation::Pen(annotation) => {
                    obstacles.extend(annotation.paths.into_iter().enumerate().map(
                        |(index, points)| CloudPlusObstacle::Polyline {
                            id: Some(format!("{owner}:path:{index}")),
                            points,
                        },
                    ));
                }
                SceneAnnotation::TextBox(annotation) => {
                    obstacles.push(CloudPlusObstacle::Rect {
                        id: Some(owner),
                        rect: annotation.layout_rect,
                    });
                }
                SceneAnnotation::Dimension(annotation) => {
                    if let Some(rect) = caption_supplement
                        .get(&annotation.id)
                        .and_then(|corners| routing_points_bounds(corners))
                    {
                        obstacles.push(CloudPlusObstacle::Rect {
                            id: Some(format!("{owner}:caption")),
                            rect,
                        });
                    }
                    obstacles.push(CloudPlusObstacle::Polyline {
                        id: Some(format!("{owner}:line")),
                        points: vec![annotation.start, annotation.end],
                    });
                }
                SceneAnnotation::Length(annotation) => {
                    obstacles.push(CloudPlusObstacle::Polyline {
                        id: Some(owner),
                        points: vec![annotation.start, annotation.end],
                    });
                }
                SceneAnnotation::Image(annotation) => {
                    obstacles.push(CloudPlusObstacle::Rect {
                        id: Some(owner),
                        rect: annotation.rect,
                    });
                }
                SceneAnnotation::Snapshot(annotation) => {
                    obstacles.push(CloudPlusObstacle::Rect {
                        id: Some(owner),
                        rect: annotation.rect,
                    });
                }
            }
        }
        CloudPlusRoutingContext {
            page_bounds,
            obstacles,
        }
    }

    /// Snaps a moving selection, as a markup move does: `point` is the
    /// pointer, `start` where the move began and `anchors` the selection's
    /// points as they were. It snaps to the semantic targets (markups other
    /// than `excluded_ids`, page content outside `excluded_content`, grids),
    /// then to tracking and to equal spacing beside other markups, setting
    /// the guides the canvas shows. Returns the snapped pointer.
    #[allow(clippy::too_many_arguments)]
    fn resolve_moving_point(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        start: PdfPoint,
        anchors: &[PdfPoint],
        excluded_ids: &[MarkupId],
        caption_supplement: &AnnotationSelectionSupplement,
        initial_bounds: Option<PdfRect>,
        excluded_content: &[[f64; 4]],
    ) -> PdfPoint {
        let index = self
            .semantic_snap_index(document_id, page_index, excluded_ids, caption_supplement)
            .with_excluded_content(excluded_content);
        let delta_x = point.x - start.x;
        let delta_y = point.y - start.y;
        let page_size = self
            .semantic_snap_page_sizes
            .get(&(document_id, page_index))
            .copied();
        let mut best: Option<(SemanticSnapDecision, PdfPoint)> = None;
        for anchor in anchors {
            let prospective = PdfPoint {
                x: anchor.x + delta_x,
                y: anchor.y + delta_y,
            };
            let annotation = index.resolve_point(
                prospective,
                &self.semantic_snap_settings,
                self.observed_pixels_per_point.0,
            );
            let grid = page_size.and_then(|(width, height)| {
                resolve_construction_grid_point(
                    prospective,
                    width,
                    height,
                    &self.semantic_snap_settings,
                    self.observed_pixels_per_point.0,
                )
            });
            let decision = match (annotation, grid) {
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
            if let Some(decision) = decision
                && best.as_ref().is_none_or(|(current, _)| {
                    decision.distance_window_px < current.distance_window_px
                })
            {
                best = Some((decision, prospective));
            }
        }
        let mut resolved = point;
        if let Some((decision, prospective)) = best {
            resolved = PdfPoint {
                x: point.x + decision.point.x - prospective.x,
                y: point.y + decision.point.y - prospective.y,
            };
            self.update_tracking_acquisition(Some(&decision));
            self.object_snap_tracking_result = None;
            self.semantic_snap_decision = Some(decision);
        } else {
            self.update_tracking_acquisition(None);
            self.semantic_snap_decision = None;
            let acquired = self.acquired_tracking_for_enabled_sources();
            let mut best_tracking: Option<(ObjectSnapTrackingResult, PdfPoint)> = None;
            for anchor in anchors {
                let prospective = PdfPoint {
                    x: anchor.x + delta_x,
                    y: anchor.y + delta_y,
                };
                if let Some(result) = find_object_snap_tracking_point(
                    prospective,
                    &acquired,
                    self.observed_pixels_per_point.0,
                    self.semantic_snap_settings.sensitivity_window_px(),
                    &[OrthogonalAxis::Horizontal, OrthogonalAxis::Vertical],
                ) && best_tracking.as_ref().is_none_or(|(current, _)| {
                    result.distance_window_px < current.distance_window_px
                }) {
                    best_tracking = Some((result, prospective));
                }
            }
            if let Some((result, prospective)) = best_tracking {
                resolved = PdfPoint {
                    x: point.x + result.point.x - prospective.x,
                    y: point.y + result.point.y - prospective.y,
                };
                self.object_snap_tracking_result = Some(result);
            } else {
                self.object_snap_tracking_result = None;
            }
        }

        let initial_bounds = initial_bounds.or_else(|| {
            self.cached_moving_initial_bounds(
                document_id,
                page_index,
                anchors,
                excluded_ids,
                caption_supplement,
            )
        });
        if let Some(initial_bounds) = initial_bounds {
            let mut moving_bounds = PdfRect {
                x: initial_bounds.x + resolved.x - start.x,
                y: initial_bounds.y + resolved.y - start.y,
                ..initial_bounds
            };
            let references = self.cached_moving_guide_references(
                document_id,
                page_index,
                excluded_ids,
                caption_supplement,
            );
            if let Some(spacing) = find_equal_spacing_snap(
                moving_bounds,
                &references,
                self.observed_pixels_per_point.0,
                self.semantic_snap_settings.sensitivity_window_px(),
            ) {
                resolved.x += spacing.adjustment.x;
                resolved.y += spacing.adjustment.y;
                moving_bounds.x += spacing.adjustment.x;
                moving_bounds.y += spacing.adjustment.y;
                self.relationship_snap_guides.extend(spacing.guides);
                self.semantic_snap_decision = None;
                self.object_snap_tracking_result = None;
            }
            if let Some(size) = find_equal_size_snap(
                moving_bounds,
                &references,
                self.observed_pixels_per_point.0,
                0.5,
            ) {
                self.relationship_snap_guides.extend(size.guides);
            }
        }
        resolved
    }

    /// Snaps a drag of page content in Edit PDF mode as a markup move
    /// snaps. `anchors` and `bounds` are the content as shown when the drag
    /// began; markups `moving_ids` move with it. Page content inside
    /// `bounds` is not a target: it is what moves.
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_content_move(
        &mut self,
        document_id: u64,
        page_index: u32,
        start: PdfPoint,
        point: PdfPoint,
        anchors: &[PdfPoint],
        bounds: PdfRect,
        excluded_content: &[[f64; 4]],
        moving_ids: &[MarkupId],
        align_to: &[[f64; 4]],
    ) -> PdfPoint {
        self.relationship_snap_guides.clear();
        let resolved = self.resolve_moving_point(
            document_id,
            page_index,
            point,
            start,
            anchors,
            moving_ids,
            &AnnotationSelectionSupplement::new(),
            Some(bounds),
            excluded_content,
        );
        // Short of a point to snap to, its box lines up with the others'.
        let settings = self.semantic_snap_settings;
        if self.semantic_snap_decision.is_some()
            || self.object_snap_tracking_result.is_some()
            || !self.relationship_snap_guides.is_empty()
            || !settings.is_source_enabled(crate::semantic_snapping::SemanticSnapSource::Content)
            || !settings.guides_enabled()
            || !settings
                .is_guide_enabled(crate::semantic_snapping::SemanticSnapGuideType::Alignment)
        {
            return resolved;
        }
        let (dx, dy) = (resolved.x - start.x, resolved.y - start.y);
        let moving = [
            bounds.x + dx,
            bounds.y + dy,
            bounds.x + bounds.width + dx,
            bounds.y + bounds.height + dy,
        ];
        match crate::semantic_snapping::find_box_alignment(
            moving,
            align_to,
            self.observed_pixels_per_point.0,
            settings.sensitivity_window_px(),
        ) {
            Some(aligned) => {
                self.object_snap_tracking_result = Some(aligned.tracking);
                PdfPoint {
                    x: resolved.x + aligned.adjustment.x,
                    y: resolved.y + aligned.adjustment.y,
                }
            }
            None => resolved,
        }
    }

    /// Ends a content drag's snapping: its guides go.
    pub fn clear_content_move_snap(&mut self) {
        self.relationship_snap_guides.clear();
        self.semantic_snap_decision = None;
        self.object_snap_tracking_result = None;
    }

    /// A calibration line's end, snapped to the page's drawing and markups
    /// as a Length end is, whatever tool is armed. The snap shows on the
    /// page until [`Self::clear_semantic_snap_decision`].
    pub fn resolve_calibration_point(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
    ) -> PdfPoint {
        self.relationship_snap_guides.clear();
        self.object_snap_tracking_result = None;
        let decision = self
            .semantic_snap_index(
                document_id,
                page_index,
                &[],
                &AnnotationSelectionSupplement::new(),
            )
            .resolve_point(
                point,
                &self.semantic_snap_settings,
                self.observed_pixels_per_point.0,
            );
        let resolved = decision.as_ref().map_or(point, |decision| decision.point);
        self.semantic_snap_decision = decision;
        resolved
    }

    fn resolve_semantic_creation_point(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> PdfPoint {
        self.relationship_snap_guides.clear();
        let moving = match self.active.as_ref() {
            Some(ActivePointer::Domain {
                document_id: active_document_id,
                page_index: active_page_index,
                rectangle_translation_start: Some(start),
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => self
                .documents
                .get(active_document_id)
                .and_then(|document| document.selected_id().cloned().map(|id| (document, id)))
                .map(|(document, id)| {
                    // The domain Rectangle gesture installs a scene preview at
                    // pointer-down; freeze anchors from committed geometry.
                    let scene = document.thumbnail_scene(page_index);
                    let anchors = moving_annotation_snap_anchor_points_with_selection_supplement(
                        &scene,
                        std::slice::from_ref(&id),
                        128,
                        &AnnotationSelectionSupplement::new(),
                    );
                    (
                        *start,
                        anchors,
                        vec![id],
                        AnnotationSelectionSupplement::new(),
                    )
                }),
            Some(ActivePointer::EllipseMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                self.documents.get(active_document_id).map(|document| {
                    let scene = document.thumbnail_scene(page_index);
                    (
                        *start,
                        moving_annotation_snap_anchor_points_with_selection_supplement(
                            &scene,
                            std::slice::from_ref(id),
                            128,
                            &AnnotationSelectionSupplement::new(),
                        ),
                        vec![id.clone()],
                        AnnotationSelectionSupplement::new(),
                    )
                })
            }
            Some(ActivePointer::InkMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                original_paths,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                original_paths.iter().flatten().copied().take(128).collect(),
                vec![id.clone()],
                AnnotationSelectionSupplement::new(),
            )),
            Some(ActivePointer::RedactMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                self.documents.get(active_document_id).map(|document| {
                    let scene = document.thumbnail_scene(page_index);
                    (
                        *start,
                        moving_annotation_snap_anchor_points_with_selection_supplement(
                            &scene,
                            std::slice::from_ref(id),
                            128,
                            &AnnotationSelectionSupplement::new(),
                        ),
                        vec![id.clone()],
                        AnnotationSelectionSupplement::new(),
                    )
                })
            }
            Some(ActivePointer::StraightLineMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                snap_anchor_points,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                snap_anchor_points.clone(),
                vec![id.clone()],
                AnnotationSelectionSupplement::new(),
            )),
            Some(ActivePointer::LengthMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                snap_anchor_points,
                snap_caption_supplement,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                snap_anchor_points.clone(),
                vec![id.clone()],
                snap_caption_supplement.clone(),
            )),
            Some(ActivePointer::DimensionEdit {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                kind: DimensionPointerEditKind::Body,
                start,
                snap_anchor_points,
                snap_caption_supplement,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                snap_anchor_points.clone(),
                vec![id.clone()],
                snap_caption_supplement.clone(),
            )),
            Some(ActivePointer::TextBoxMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                self.documents.get(active_document_id).map(|document| {
                    let scene = document.thumbnail_scene(page_index);
                    (
                        *start,
                        moving_annotation_snap_anchor_points(&scene, std::slice::from_ref(id), 128),
                        vec![id.clone()],
                        AnnotationSelectionSupplement::new(),
                    )
                })
            }
            Some(ActivePointer::ImageMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                self.documents.get(active_document_id).map(|document| {
                    let scene = document.thumbnail_scene(page_index);
                    (
                        *start,
                        moving_annotation_snap_anchor_points(&scene, std::slice::from_ref(id), 128),
                        vec![id.clone()],
                        AnnotationSelectionSupplement::new(),
                    )
                })
            }
            Some(ActivePointer::SnapshotMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                self.documents.get(active_document_id).map(|document| {
                    let scene = document.thumbnail_scene(page_index);
                    (
                        *start,
                        moving_annotation_snap_anchor_points(&scene, std::slice::from_ref(id), 128),
                        vec![id.clone()],
                        AnnotationSelectionSupplement::new(),
                    )
                })
            }
            Some(ActivePointer::ArcMove {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                start,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                self.documents.get(active_document_id).map(|document| {
                    let scene = document.thumbnail_scene(page_index);
                    (
                        *start,
                        moving_annotation_snap_anchor_points(&scene, std::slice::from_ref(id), 128),
                        vec![id.clone()],
                        AnnotationSelectionSupplement::new(),
                    )
                })
            }
            Some(ActivePointer::GroupMove {
                document_id: active_document_id,
                page_index: active_page_index,
                start,
                snap_anchor_points,
                excluded_ids,
                snap_caption_supplement,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                snap_anchor_points.clone(),
                excluded_ids.clone(),
                snap_caption_supplement.clone(),
            )),
            Some(ActivePointer::CloudEdit {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                kind: CloudPointerEditKind::Body,
                start,
                snap_anchor_points,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                snap_anchor_points.clone(),
                vec![id.clone()],
                AnnotationSelectionSupplement::new(),
            )),
            Some(ActivePointer::CalloutEdit {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                kind: CalloutPointerEditKind::TextBox | CalloutPointerEditKind::Body,
                start,
                snap_anchor_points,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                snap_anchor_points.clone(),
                vec![id.clone()],
                AnnotationSelectionSupplement::new(),
            )),
            Some(ActivePointer::CloudPlusEdit {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                kind: CloudPlusPointerEditKind::TextBox | CloudPlusPointerEditKind::Body,
                start,
                snap_anchor_points,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => Some((
                *start,
                snap_anchor_points.clone(),
                vec![id.clone()],
                AnnotationSelectionSupplement::new(),
            )),
            _ => None,
        };
        if let Some((start, anchors, excluded_ids, caption_supplement)) = moving {
            return self.resolve_moving_point(
                document_id,
                page_index,
                point,
                start,
                &anchors,
                &excluded_ids,
                &caption_supplement,
                None,
                &[],
            );
        }
        let manipulated_id = match self.active.as_ref() {
            Some(ActivePointer::Domain {
                document_id: active_document_id,
                page_index: active_page_index,
                rectangle_resize_handle: Some(_),
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => self
                .documents
                .get(active_document_id)
                .and_then(|document| document.selected_id().cloned()),
            Some(ActivePointer::StraightLineEndpoint {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::LengthEndpoint {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::VertexPathPoint {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::MeasurementPathPoint {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::ArcControlPoint {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                Some(id.clone())
            }
            Some(ActivePointer::EllipseResize {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::RedactResize {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::TextBoxResize {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::ImageResize {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::SnapshotResize {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            })
            | Some(ActivePointer::InkResize {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                Some(id.clone())
            }
            Some(ActivePointer::CalloutEdit {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                kind:
                    CalloutPointerEditKind::TextBoxResize(_) | CalloutPointerEditKind::LeaderPoint(_),
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                Some(id.clone())
            }
            Some(ActivePointer::CloudPlusEdit {
                document_id: active_document_id,
                page_index: active_page_index,
                id,
                kind:
                    CloudPlusPointerEditKind::CloudVertex(_)
                    | CloudPlusPointerEditKind::TextBoxResize(_)
                    | CloudPlusPointerEditKind::LeaderPoint(_),
                ..
            }) if (*active_document_id, *active_page_index) == (document_id, page_index) => {
                Some(id.clone())
            }
            _ => None,
        };
        if let Some(manipulated_id) = manipulated_id {
            let annotation_decision = self
                .semantic_snap_index(
                    document_id,
                    page_index,
                    std::slice::from_ref(&manipulated_id),
                    &AnnotationSelectionSupplement::new(),
                )
                .resolve_point(
                    point,
                    &self.semantic_snap_settings,
                    self.observed_pixels_per_point.0,
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
            if let Some(decision) = decision {
                let resolved = decision.point;
                self.update_tracking_acquisition(Some(&decision));
                self.object_snap_tracking_result = None;
                self.semantic_snap_decision = Some(decision);
                return resolved;
            }
            self.update_tracking_acquisition(None);
            self.semantic_snap_decision = None;
            let tracking = find_object_snap_tracking_point(
                point,
                &self.acquired_tracking_for_enabled_sources(),
                self.observed_pixels_per_point.0,
                self.semantic_snap_settings.sensitivity_window_px(),
                &[OrthogonalAxis::Horizontal, OrthogonalAxis::Vertical],
            );
            let resolved = tracking.as_ref().map_or(point, |tracking| tracking.point);
            self.object_snap_tracking_result = tracking;
            return resolved;
        }
        if !matches!(
            self.tool,
            AnnotationTool::Rectangle
                | AnnotationTool::Ellipse
                | AnnotationTool::Line
                | AnnotationTool::Arrow
                | AnnotationTool::Length
                | AnnotationTool::Dimension
                | AnnotationTool::Image
                | AnnotationTool::Polyline
                | AnnotationTool::Polygon
                | AnnotationTool::Polylength
                | AnnotationTool::Area
                | AnnotationTool::Arc
        ) {
            self.semantic_snap_decision = None;
            self.object_snap_tracking_result = None;
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
            _ => self
                .vertex_path_draft
                .as_ref()
                .filter(|draft| (draft.document_id, draft.page_index) == (document_id, page_index))
                .and_then(|draft| {
                    draft
                        .points
                        .last()
                        .copied()
                        .map(|anchor| (vec![draft.id.clone()], Some(anchor)))
                })
                .or_else(|| {
                    self.measurement_path_draft
                        .as_ref()
                        .filter(|draft| {
                            (draft.document_id, draft.page_index) == (document_id, page_index)
                        })
                        .and_then(|draft| {
                            draft
                                .points
                                .last()
                                .copied()
                                .map(|anchor| (vec![draft.id.clone()], Some(anchor)))
                        })
                })
                .or_else(|| {
                    self.arc_draft
                        .as_ref()
                        .filter(|draft| {
                            (draft.document_id, draft.page_index) == (document_id, page_index)
                        })
                        .map(|draft| {
                            (
                                vec![draft.id.clone()],
                                draft.end.is_none().then_some(draft.start),
                            )
                        })
                })
                .unwrap_or((Vec::new(), None)),
        };
        let annotation_decision = self
            .semantic_snap_index(
                document_id,
                page_index,
                &excluded_ids,
                &AnnotationSelectionSupplement::new(),
            )
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
        if let Some(decision) = decision {
            let resolved = decision.point;
            self.update_tracking_acquisition(Some(&decision));
            self.object_snap_tracking_result = None;
            self.semantic_snap_decision = Some(decision);
            return resolved;
        }
        self.update_tracking_acquisition(None);
        self.semantic_snap_decision = None;
        let constrained = if constrain_orthogonal {
            creation_anchor.map_or(point, |anchor| {
                let dx = point.x - anchor.x;
                let dy = point.y - anchor.y;
                if dx.abs() >= dy.abs() {
                    PdfPoint {
                        x: point.x,
                        y: anchor.y,
                    }
                } else {
                    PdfPoint {
                        x: anchor.x,
                        y: point.y,
                    }
                }
            })
        } else {
            point
        };
        let mut resolved = constrained;
        let mut increment_applied = false;
        if self.tool == AnnotationTool::Dimension
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
                increment_applied = true;
            }
        }
        if !increment_applied {
            let allowed_axes = if constrain_orthogonal {
                creation_anchor.map_or_else(
                    || vec![OrthogonalAxis::Horizontal, OrthogonalAxis::Vertical],
                    |anchor| {
                        if (point.x - anchor.x).abs() >= (point.y - anchor.y).abs() {
                            vec![OrthogonalAxis::Vertical]
                        } else {
                            vec![OrthogonalAxis::Horizontal]
                        }
                    },
                )
            } else {
                vec![OrthogonalAxis::Horizontal, OrthogonalAxis::Vertical]
            };
            let tracking = find_object_snap_tracking_point(
                constrained,
                &self.acquired_tracking_for_enabled_sources(),
                self.observed_pixels_per_point.0,
                self.semantic_snap_settings.sensitivity_window_px(),
                &allowed_axes,
            );
            resolved = tracking
                .as_ref()
                .map_or(resolved, |tracking| tracking.point);
            self.object_snap_tracking_result = tracking;
        } else {
            self.object_snap_tracking_result = None;
        }
        resolved
    }

    fn resolve_equal_size_resize_point(&mut self, point: PdfPoint) -> PdfPoint {
        let target = match self.active.as_ref() {
            Some(ActivePointer::Domain {
                document_id,
                page_index,
                rectangle_resize_handle: Some(handle),
                ..
            }) => self.documents.get(document_id).and_then(|document| {
                let id = document.selected_id()?.clone();
                let annotation = document
                    .rectangles()
                    .iter()
                    .find(|annotation| annotation.id == id)?;
                (annotation.rotation_degrees.rem_euclid(360.).abs() <= f64::EPSILON).then_some({
                    EqualSizeResizeTarget {
                        document_id: *document_id,
                        page_index: *page_index,
                        id,
                        handle: *handle,
                        original_rect: annotation.rect,
                        geometry: EqualSizeResizeGeometry::Rectangle,
                    }
                })
            }),
            Some(ActivePointer::EllipseResize {
                document_id,
                page_index,
                id,
                handle,
                original_rect,
                original_rotation_degrees,
                grab_offset,
                ..
            }) if original_rotation_degrees.rem_euclid(360.).abs() <= f64::EPSILON => {
                Some(EqualSizeResizeTarget {
                    document_id: *document_id,
                    page_index: *page_index,
                    id: id.clone(),
                    handle: *handle,
                    original_rect: *original_rect,
                    geometry: EqualSizeResizeGeometry::Ellipse {
                        grab_offset: *grab_offset,
                    },
                })
            }
            Some(ActivePointer::RedactResize {
                document_id,
                page_index,
                id,
                handle,
                original_rect,
                ..
            }) => Some(EqualSizeResizeTarget {
                document_id: *document_id,
                page_index: *page_index,
                id: id.clone(),
                handle: *handle,
                original_rect: *original_rect,
                geometry: EqualSizeResizeGeometry::Redact,
            }),
            Some(ActivePointer::ImageResize {
                document_id,
                page_index,
                id,
                handle,
                start,
                original_rect,
                original_rotation_degrees,
                aspect_locked,
                ..
            }) if original_rotation_degrees.rem_euclid(360.).abs() <= f64::EPSILON => {
                let rectangle_handle = match handle {
                    ImageResizeHandle::SouthWest => RectangleResizeHandle::SouthWest,
                    ImageResizeHandle::South => RectangleResizeHandle::South,
                    ImageResizeHandle::SouthEast => RectangleResizeHandle::SouthEast,
                    ImageResizeHandle::East => RectangleResizeHandle::East,
                    ImageResizeHandle::NorthEast => RectangleResizeHandle::NorthEast,
                    ImageResizeHandle::North => RectangleResizeHandle::North,
                    ImageResizeHandle::NorthWest => RectangleResizeHandle::NorthWest,
                    ImageResizeHandle::West => RectangleResizeHandle::West,
                };
                Some(EqualSizeResizeTarget {
                    document_id: *document_id,
                    page_index: *page_index,
                    id: id.clone(),
                    handle: rectangle_handle,
                    original_rect: *original_rect,
                    geometry: EqualSizeResizeGeometry::Image {
                        handle: *handle,
                        start: *start,
                        aspect_locked: *aspect_locked,
                    },
                })
            }
            Some(ActivePointer::TextBoxResize {
                document_id,
                page_index,
                id,
                handle,
                original_rect,
                original_rotation_degrees,
                ..
            })
            | Some(ActivePointer::SnapshotResize {
                document_id,
                page_index,
                id,
                handle,
                original_rect,
                original_rotation_degrees,
                ..
            }) if original_rotation_degrees.rem_euclid(360.).abs() <= f64::EPSILON => {
                Some(EqualSizeResizeTarget {
                    document_id: *document_id,
                    page_index: *page_index,
                    id: id.clone(),
                    handle: *handle,
                    original_rect: *original_rect,
                    geometry: EqualSizeResizeGeometry::Rectangle,
                })
            }
            Some(ActivePointer::CalloutEdit {
                document_id,
                page_index,
                id,
                kind: CalloutPointerEditKind::TextBoxResize(handle),
                original,
                ..
            }) => Some(EqualSizeResizeTarget {
                document_id: *document_id,
                page_index: *page_index,
                id: id.clone(),
                handle: *handle,
                original_rect: original.text_box,
                geometry: EqualSizeResizeGeometry::Rectangle,
            }),
            Some(ActivePointer::CloudPlusEdit {
                document_id,
                page_index,
                id,
                kind: CloudPlusPointerEditKind::TextBoxResize(handle),
                original,
                ..
            }) => Some(EqualSizeResizeTarget {
                document_id: *document_id,
                page_index: *page_index,
                id: id.clone(),
                handle: *handle,
                original_rect: original.text_box,
                geometry: EqualSizeResizeGeometry::Rectangle,
            }),
            _ => None,
        };
        let Some(target) = target else {
            return point;
        };
        let Some(moving_bounds) = target.rect_at(point) else {
            return point;
        };
        let references = self.committed_guide_rects(
            target.document_id,
            target.page_index,
            std::slice::from_ref(&target.id),
        );
        let Some(size) = find_equal_size_snap(
            moving_bounds,
            &references,
            self.observed_pixels_per_point.0,
            self.semantic_snap_settings.sensitivity_window_px(),
        ) else {
            return point;
        };
        let west = matches!(
            target.handle,
            RectangleResizeHandle::NorthWest
                | RectangleResizeHandle::West
                | RectangleResizeHandle::SouthWest
        );
        let east = matches!(
            target.handle,
            RectangleResizeHandle::NorthEast
                | RectangleResizeHandle::East
                | RectangleResizeHandle::SouthEast
        );
        let north = matches!(
            target.handle,
            RectangleResizeHandle::NorthWest
                | RectangleResizeHandle::North
                | RectangleResizeHandle::NorthEast
        );
        let south = matches!(
            target.handle,
            RectangleResizeHandle::SouthWest
                | RectangleResizeHandle::South
                | RectangleResizeHandle::SouthEast
        );
        let width = (west || east).then_some(size.width).flatten();
        let height = (north || south).then_some(size.height).flatten();
        if width.is_none() && height.is_none() {
            return point;
        }
        let resolved = PdfPoint {
            x: point.x
                + width.map_or(0., |width| {
                    (width - moving_bounds.width) * if west { -1. } else { 1. }
                }),
            y: point.y
                + height.map_or(0., |height| {
                    (height - moving_bounds.height) * if south { -1. } else { 1. }
                }),
        };
        let Some(resolved_bounds) = target.rect_at(resolved) else {
            return point;
        };
        self.relationship_snap_guides = size
            .guides
            .into_iter()
            .filter_map(|guide| match guide {
                RelationshipSnapGuide::EqualSize {
                    axis, reference, ..
                } if (axis == OrthogonalAxis::Horizontal && width.is_some())
                    || (axis == OrthogonalAxis::Vertical && height.is_some()) =>
                {
                    Some(RelationshipSnapGuide::EqualSize {
                        axis,
                        moving: resolved_bounds,
                        reference,
                    })
                }
                _ => None,
            })
            .collect();
        self.semantic_snap_decision = None;
        self.object_snap_tracking_result = None;
        resolved
    }

    fn resolve_equal_size_placement_point(
        &mut self,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> PdfPoint {
        let placement = match self.active.as_ref() {
            Some(ActivePointer::Domain {
                document_id,
                page_index,
                rectangle_create_start: Some(start),
                ink: false,
                ..
            }) => Some((*document_id, *page_index, *start, false)),
            Some(ActivePointer::EllipseCreate {
                document_id,
                page_index,
                start,
                ..
            }) => Some((*document_id, *page_index, *start, true)),
            Some(ActivePointer::RedactCreate {
                document_id,
                page_index,
                start,
                ..
            }) => Some((*document_id, *page_index, *start, false)),
            _ => self
                .snapshot_draft
                .as_ref()
                .map(|draft| (draft.document_id, draft.page_index, draft.start, false)),
        };
        let Some((document_id, page_index, start, is_ellipse)) = placement else {
            return point;
        };
        let candidate = if is_ellipse && constrain_orthogonal {
            EllipseAnnotation::constrained_end(start, point)
        } else {
            point
        };
        let moving_bounds = PdfRect::from_corners(start, candidate);
        let references = self.committed_guide_rects(document_id, page_index, &[]);
        let Some(size) = find_equal_size_snap(
            moving_bounds,
            &references,
            self.observed_pixels_per_point.0,
            self.semantic_snap_settings.sensitivity_window_px(),
        ) else {
            return point;
        };
        let (mut width, mut height) = (size.width, size.height);
        if is_ellipse && constrain_orthogonal && (width.is_some() || height.is_some()) {
            let diameter = match (width, height) {
                (Some(width), Some(height)) => {
                    if (width - moving_bounds.width).abs() <= (height - moving_bounds.height).abs()
                    {
                        width
                    } else {
                        height
                    }
                }
                (Some(width), None) => width,
                (None, Some(height)) => height,
                (None, None) => unreachable!(),
            };
            width = Some(diameter);
            height = Some(diameter);
        }
        let resolved = PdfPoint {
            x: width.map_or(candidate.x, |width| {
                start.x + width * if candidate.x < start.x { -1. } else { 1. }
            }),
            y: height.map_or(candidate.y, |height| {
                start.y + height * if candidate.y < start.y { -1. } else { 1. }
            }),
        };
        let resolved_bounds = PdfRect::from_corners(start, resolved);
        self.relationship_snap_guides = size
            .guides
            .into_iter()
            .filter_map(|guide| match guide {
                RelationshipSnapGuide::EqualSize {
                    axis, reference, ..
                } if (axis == OrthogonalAxis::Horizontal && width.is_some())
                    || (axis == OrthogonalAxis::Vertical && height.is_some()) =>
                {
                    Some(RelationshipSnapGuide::EqualSize {
                        axis,
                        moving: resolved_bounds,
                        reference,
                    })
                }
                _ => None,
            })
            .collect();
        self.semantic_snap_decision = None;
        self.object_snap_tracking_result = None;
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

    pub fn clear_pending_image_asset(&mut self) {
        self.image_asset = None;
        self.image_placement_page = None;
    }

    pub fn set_image_asset(&mut self, asset: DecodedRgbaAsset) {
        self.set_image_asset_with_aspect_lock(asset, false);
    }

    /// The image the Image tool places next, its proportions locked or not.
    pub fn set_image_asset_with_aspect_lock(
        &mut self,
        asset: DecodedRgbaAsset,
        aspect_locked: bool,
    ) {
        self.image_asset = Some(PendingImageAsset {
            asset,
            aspect_locked,
            select_after_placement: false,
        });
    }

    pub fn set_signature_asset(&mut self, asset: DecodedRgbaAsset) {
        self.image_asset = Some(PendingImageAsset {
            asset,
            aspect_locked: true,
            select_after_placement: true,
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

    /// Read-only page-local Image ghost matching the geometry a click at the
    /// same point will commit. Pointer ownership remains in the workspace so
    /// leaving the page can remove the transient preview without discarding
    /// the prepared asset.
    pub fn pending_image_preview_at(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
    ) -> Result<Option<PendingImagePreview>, AnnotationError> {
        if self.tool != AnnotationTool::Image {
            return Ok(None);
        }
        let Some(pending) = self.image_asset.as_ref() else {
            return Ok(None);
        };
        // While its box is drawn, the image previews filling it.
        if let Some(ActivePointer::ImageCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            start,
            current,
            ..
        }) = &self.active
        {
            if (*active_document_id, *active_page_index) != (document_id, page_index)
                || point_distance_css_px(*start, *current, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
            {
                return Ok(None);
            }
            return Ok(Some(PendingImagePreview {
                document_id,
                page_index,
                rect: image_drawn_rect(pending, *start, *current)?,
                asset_id: pending.asset.id().as_str().to_owned(),
                opacity: IMAGE_PLACEMENT_PREVIEW_OPACITY,
            }));
        }
        let Some(placement_page) = self.image_placement_page else {
            return Ok(None);
        };
        Ok(Some(PendingImagePreview {
            document_id,
            page_index,
            rect: image_placement_rect(pending, placement_page, point)?,
            asset_id: pending.asset.id().as_str().to_owned(),
            opacity: IMAGE_PLACEMENT_PREVIEW_OPACITY,
        }))
    }

    pub(crate) fn image_select_after_placement(&self) -> bool {
        self.image_asset
            .as_ref()
            .is_some_and(|pending| pending.select_after_placement)
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

    pub fn remove_document_page_scale(
        &mut self,
        document_id: u64,
        target: crate::annotation_model::PageScaleApplyTarget,
        page_count: u32,
    ) -> Result<bool, AnnotationError> {
        self.cancel(PointerCancelReason::ToolChanged)?;
        self.documents
            .entry(document_id)
            .or_default()
            .remove_page_scale(target, page_count)
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

    /// The page's calibration, or Revu's default for an uncalibrated page:
    /// 1 in on paper = 1 in, to two decimal places. Measuring never waits for
    /// a scale to be set; setting one changes later measurements.
    pub fn measurement_calibration(&self, document_id: u64, page_index: u32) -> LengthCalibration {
        self.document_page_length_calibration(document_id, page_index)
            .cloned()
            .unwrap_or_else(|| {
                LengthCalibration::from_scale(72., 1., "in", 2, true)
                    .expect("the default 1 in = 1 in scale is valid")
            })
    }

    pub fn document_page_scale(&self, document_id: u64, page_index: u32) -> Option<&PageScale> {
        self.documents
            .get(&document_id)
            .and_then(|document| document.page_scale(page_index))
    }

    pub fn begin_length_placement(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        self.cancel(PointerCancelReason::AdapterError)?;
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
        } else {
            draft.mid = point;
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
        // A polygon with a cloud outline is a Cloud, as Revu stores it.
        if draft.kind == VertexPathKind::Polygon && properties.cloud_style {
            let id = self.next_id(AnnotationTool::Cloud)?;
            self.documents
                .entry(document_id)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Cloud(
                    CloudAnnotation::new(
                        id.clone(),
                        draft.page_index,
                        draft.points,
                        properties.cloud_intensity,
                        appearance,
                    )?,
                )))?;
            return Ok(PointerPhaseOutcome::AnnotationCreated(id));
        }
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
        self.cloud_drag_pointer = None;
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
                    rectangle_tool_appearance(&properties, true)?,
                )?,
            )))?;
        self.tool = AnnotationTool::Select;
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

    pub fn set_selected_cloud_plus_cloud_point(
        &mut self,
        document_id: u64,
        vertex_index: usize,
        point: PdfPoint,
    ) -> Result<(), AnnotationError> {
        self.set_selected_cloud_plus_cloud_point_with_routing_supplement(
            document_id,
            vertex_index,
            point,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn set_selected_cloud_plus_cloud_point_with_routing_supplement(
        &mut self,
        document_id: u64,
        vertex_index: usize,
        point: PdfPoint,
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Result<(), AnnotationError> {
        let selected_id = self
            .documents
            .get(&document_id)
            .and_then(AnnotationDocument::selected_id)
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let page_index = self
            .documents
            .get(&document_id)
            .and_then(|document| {
                document
                    .cloud_pluses()
                    .iter()
                    .find(|annotation| annotation.id == selected_id)
                    .map(|annotation| annotation.page_index)
            })
            .ok_or(AnnotationError::NoSelection)?;
        let routing_context = self.cloud_plus_routing_context(
            document_id,
            page_index,
            Some(&selected_id),
            caption_supplement,
        );
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
            &routing_context,
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
        self.translate_selected_cloud_plus_text_box_with_routing_supplement(
            document_id,
            delta_x,
            delta_y,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn translate_selected_cloud_plus_text_box_with_routing_supplement(
        &mut self,
        document_id: u64,
        delta_x: f64,
        delta_y: f64,
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Result<(), AnnotationError> {
        let selected_id = self
            .documents
            .get(&document_id)
            .and_then(AnnotationDocument::selected_id)
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let page_index = self
            .documents
            .get(&document_id)
            .and_then(|document| {
                document
                    .cloud_pluses()
                    .iter()
                    .find(|annotation| annotation.id == selected_id)
                    .map(|annotation| annotation.page_index)
            })
            .ok_or(AnnotationError::NoSelection)?;
        let routing_context = self.cloud_plus_routing_context(
            document_id,
            page_index,
            Some(&selected_id),
            caption_supplement,
        );
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
            &routing_context,
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
        // Commit exactly the path displayed by the hover preview, including
        // its final point when it is at least half a PDF point from the last click.
        let mut points = draft.points.clone();
        let last = *points
            .last()
            .expect("a measurement draft retains its first point");
        if (draft.hover.x - last.x).hypot(draft.hover.y - last.y) >= 0.5 {
            points.push(draft.hover);
        }
        if points.len() < draft.kind.minimum_points() {
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
                    points,
                    draft.kind,
                    draft.calibration,
                    rectangle_tool_appearance(&properties, false)?,
                    caption_tool_style(&properties)?,
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
            "",
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
        let calibration = self.measurement_calibration(document_id, page_index);
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
        if let Some(draft) = &self.cloud_draft {
            return Some((draft.document_id, draft.page_index));
        }
        if let Some(draft) = &self.arc_draft {
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
            | ActivePointer::ImageCreate {
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
            | ActivePointer::LengthMove {
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
            | ActivePointer::CloudPlusEdit {
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
            | ActivePointer::TextBoxRotate {
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
            | ActivePointer::ImageRotate {
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
            | ActivePointer::InkResize {
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

    pub fn selection_marquee_candidates(&self, document_id: u64, page_index: u32) -> Vec<MarkupId> {
        self.selection_marquee_candidates_with_supplement(
            document_id,
            page_index,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn selection_marquee_candidates_with_supplement(
        &self,
        document_id: u64,
        page_index: u32,
        supplement: &AnnotationSelectionSupplement,
    ) -> Vec<MarkupId> {
        let Some(ActivePointer::Marquee {
            document_id: active_document,
            page_index: active_page,
            marquee,
            pdf_points,
            ..
        }) = self.active.as_ref()
        else {
            return Vec::new();
        };
        if (*active_document, *active_page) != (document_id, page_index) || !marquee.active {
            return Vec::new();
        }
        self.documents
            .get(&document_id)
            .map(|document| {
                document.marquee_candidates_with_supplement(
                    page_index,
                    &marquee_in_pdf(marquee, pdf_points),
                    selection_point_from_pdf,
                    supplement,
                )
            })
            .unwrap_or_default()
    }

    /// Which of `rects` (page content as shown, in page space) the active
    /// selection marquee on the page selects, by the rule it selects
    /// markups with: box or lasso, window or crossing. `None` without an
    /// active marquee there.
    pub fn marquee_selects_rects(
        &self,
        document_id: u64,
        page_index: u32,
        rects: &[[f64; 4]],
    ) -> Option<Vec<bool>> {
        let Some(ActivePointer::Marquee {
            document_id: active_document,
            page_index: active_page,
            marquee,
            pdf_points,
            ..
        }) = self.active.as_ref()
        else {
            return None;
        };
        if (*active_document, *active_page) != (document_id, page_index) || !marquee.active {
            return None;
        }
        let marquee = marquee_in_pdf(marquee, pdf_points);
        Some(
            rects
                .iter()
                .map(|rect| {
                    let corners = [
                        (rect[0], rect[1]),
                        (rect[2], rect[1]),
                        (rect[2], rect[3]),
                        (rect[0], rect[3]),
                    ]
                    .map(|(x, y)| SelectionPoint::new(x, y))
                    .to_vec();
                    crate::selection_geometry::geometry_selected(
                        &[crate::selection_geometry::SelectionPath::new(corners, true)],
                        &marquee,
                    )
                })
                .collect(),
        )
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
            }) | Some(ActivePointer::CloudPlusCreate {
                click_placement_pending: true,
                ..
            }) | Some(ActivePointer::ImageCreate {
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

    /// Whether a hovered identity can advertise a move, independent of selection.
    pub fn annotation_is_movable(&self, document_id: u64, id: &MarkupId) -> bool {
        self.documents
            .get(&document_id)
            .and_then(|document| document.annotation_locked(id))
            == Some(false)
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

    /// Hover candidate for pointer-move feedback. Mirrors the Select
    /// pointer-down hit order (direct hit, then non-rectangle fallback) without
    /// mutating selection. Callers clear the candidate on pointer exit, press,
    /// and tool change.
    pub fn hover_markup_id(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        self.hover_markup_id_with_selection_paths(
            document_id,
            page_index,
            point,
            tolerance_pt,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn hover_markup_id_with_selection_paths(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
        supplement: &AnnotationSelectionSupplement,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        let selected_control = document
            .hit_test(page_index, point, tolerance_pt)?
            .filter(|hit| !matches!(hit, HitTarget::Body(_)))
            .map(|hit| hit.markup_id().clone());
        Ok(selected_control.or_else(|| {
            hit_annotation_body_in_document_order(
                document,
                page_index,
                point,
                tolerance_pt,
                supplement,
                self.observed_pixels_per_point.0,
            )
        }))
    }

    /// Read-only Select hover hit for the primary selected Rectangle. The
    /// result preserves body, resize-handle, and rotation-handle semantics so
    /// the presentation layer can choose feedback without repeating hit
    /// testing. Locked rectangles remain body-hoverable, but their inert
    /// controls do not advertise resize or rotation.
    pub fn select_hover_hit(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<HitTarget>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        // The knob sits a fixed screen distance out, as for a press.
        let Some(hit) = document.hit_test_with_rotation_handle_offset(
            page_index,
            point,
            tolerance_pt,
            ROTATION_HANDLE_OFFSET_CSS_PX / self.observed_pixels_per_point.0,
        )?
        else {
            return Ok(None);
        };
        let Some(annotation) = document
            .rectangles()
            .iter()
            .find(|annotation| &annotation.id == hit.markup_id())
        else {
            return Ok(None);
        };
        if annotation.locked
            && matches!(
                hit,
                HitTarget::ResizeHandle { .. } | HitTarget::RotationHandle(_)
            )
        {
            return Ok(None);
        }
        Ok(Some(hit))
    }

    /// Rectangle resize handle under the pointer in stable clockwise order.
    /// The selected unlocked Rectangle wins before the topmost unlocked
    /// hovered Rectangle. Rotation remains selected-only, matching Electron.
    pub fn hover_rectangle_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        _tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let to_hit = |annotation: &RectangleAnnotation| {
            let handle = annotation.resize_handle_at(
                point,
                crate::annotation_model::RECTANGLE_EDGE_GRAB_CSS_PX
                    / self.observed_pixels_per_point.0.max(f64::EPSILON),
            )?;
            RectangleResizeHandle::ALL
                .iter()
                .position(|candidate| *candidate == handle)
                .map(|index| (annotation.id.clone(), index))
        };
        let selected = document.selected_id();
        let selected_rectangle = selected.and_then(|selected| {
            document.rectangles().iter().find(|annotation| {
                annotation.page_index == page_index
                    && &annotation.id == selected
                    && !annotation.locked
            })
        });
        let hit = selected_rectangle.and_then(to_hit).or_else(|| {
            document
                .rectangles()
                .iter()
                .rev()
                .filter(|annotation| {
                    annotation.page_index == page_index
                        && !annotation.locked
                        && selected != Some(&annotation.id)
                })
                .find_map(to_hit)
        });
        Ok(hit)
    }

    /// Redact resize handle under the pointer in stable clockwise order. The
    /// selected unlocked Redact wins before the topmost unlocked hovered one.
    pub fn hover_redact_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        _tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let reach = edge_resize_reach(self.observed_pixels_per_point.0);
        let to_hit = |annotation: &RedactAnnotation| {
            box_outline_resize_handle(annotation.rect, 0., point, reach)
                .map(|handle| (annotation.id.clone(), resize_handle_index(handle)))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.redacts().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .redacts()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Ellipse resize handle under the pointer in stable clockwise order. The
    /// selected unlocked Ellipse wins before the topmost unlocked hovered one;
    /// rotation remains selected-only.
    pub fn hover_ellipse_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        _tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let reach = edge_resize_reach(self.observed_pixels_per_point.0);
        let to_hit = |annotation: &EllipseAnnotation| {
            ellipse_outline_resize_handle(
                annotation.rect,
                annotation.rotation_degrees,
                point,
                reach,
            )
            .map(|handle| (annotation.id.clone(), resize_handle_index(handle)))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.ellipses().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .ellipses()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Line/Arrow endpoint under the pointer in stable start/end feedback
    /// order. The selected unlocked line wins before the topmost unlocked
    /// hovered line.
    pub fn hover_straight_line_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let handle_tolerance = tolerance_pt.max(
            crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                / self.observed_pixels_per_point.0.max(f64::EPSILON),
        );
        let to_hit = |annotation: &StraightLineAnnotation| {
            [annotation.start, annotation.end]
                .into_iter()
                .enumerate()
                .rev()
                .find(|(_, handle)| distance(*handle, point) <= handle_tolerance)
                .map(|(index, _)| (annotation.id.clone(), index))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.straight_lines().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .straight_lines()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Length endpoint under the pointer in stable start/end feedback order.
    /// The selected unlocked Length wins before the topmost unlocked hovered
    /// Length.
    pub fn hover_length_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let handle_tolerance = tolerance_pt.max(
            crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                / self.observed_pixels_per_point.0.max(f64::EPSILON),
        );
        let to_hit = |annotation: &LengthAnnotation| {
            length_endpoint_at(annotation, point, handle_tolerance)
                .map(|endpoint| (annotation.id.clone(), endpoint as usize))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.lengths().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .lengths()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Callout handle under the pointer, using selected-first then topmost
    /// unselected overlap priority. The returned index is the stable feedback
    /// order: eight text-box handles followed by leader points.
    pub fn hover_callout_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let selected = document.selected_id();
        let selected_callout = selected.and_then(|selected| {
            document.callouts().iter().find(|annotation| {
                annotation.page_index == page_index
                    && &annotation.id == selected
                    && !annotation.locked
            })
        });
        let hit = selected_callout
            .and_then(|annotation| {
                hit_callout_handle(
                    annotation,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                )
                .map(|index| (annotation.id.clone(), index))
            })
            .or_else(|| {
                document
                    .callouts()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(|annotation| {
                        hit_callout_handle(
                            annotation,
                            point,
                            tolerance_pt,
                            self.observed_pixels_per_point.0,
                        )
                        .map(|index| (annotation.id.clone(), index))
                    })
            });
        Ok(hit)
    }

    /// Text Box handle under the pointer in stable rectangle feedback order,
    /// with rotation at index 8. The selected unlocked Text Box wins before
    /// the topmost unlocked hovered Text Box so the first press on a visible
    /// hover control can begin the transform.
    pub fn hover_text_box_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let to_hit = |annotation: &TextBoxAnnotation, allow_rotation: bool| {
            let handle_tolerance = tolerance_pt.max(
                crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                    / self.observed_pixels_per_point.0.max(f64::EPSILON),
            );
            if allow_rotation
                && text_box_rotation_handle_point(annotation, self.observed_pixels_per_point.0)
                    .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
            {
                return Some((annotation.id.clone(), 8));
            }
            hit_text_box_resize_handle(
                annotation,
                point,
                tolerance_pt,
                self.observed_pixels_per_point.0,
            )
            .map(|handle| {
                let index = RectangleResizeHandle::ALL
                    .iter()
                    .position(|candidate| candidate == &handle)
                    .expect("Text Box handle belongs to the stable feedback order");
                (annotation.id.clone(), index)
            })
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.text_boxes().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(|annotation| to_hit(annotation, true))
            .or_else(|| {
                document
                    .text_boxes()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(|annotation| to_hit(annotation, false))
            });
        Ok(hit)
    }

    /// Image handle under the pointer in stable clockwise feedback order,
    /// with rotation at index 8. The selected unlocked Image wins before the
    /// topmost unlocked hovered Image. Aspect-locked images advertise only
    /// working resize corners plus rotation.
    pub fn hover_image_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let to_hit = |annotation: &ImageAnnotation, allow_rotation: bool| {
            let handle_tolerance = tolerance_pt.max(
                crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                    / self.observed_pixels_per_point.0.max(f64::EPSILON),
            );
            if allow_rotation
                && image_rotation_handle_point(annotation, self.observed_pixels_per_point.0)
                    .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
            {
                return Some((annotation.id.clone(), 8));
            }
            hit_image_resize_handle(annotation, point, self.observed_pixels_per_point.0).map(
                |handle| {
                    let index = ImageResizeHandle::ALL
                        .iter()
                        .position(|candidate| candidate == &handle)
                        .expect("Image handle belongs to the stable feedback order");
                    (annotation.id.clone(), index)
                },
            )
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.images().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(|annotation| to_hit(annotation, true))
            .or_else(|| {
                document
                    .images()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(|annotation| to_hit(annotation, false))
            });
        Ok(hit)
    }

    /// Dimension handle under the pointer in stable feedback order: start,
    /// end, then offset/caption. The selected unlocked Dimension wins before
    /// the topmost unlocked hovered Dimension so Electron's first press on a
    /// visible hover handle can begin the transform without a selection-only
    /// precursor click.
    pub fn hover_dimension_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.dimensions().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(|annotation| {
                hit_dimension_handle(
                    annotation,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                )
                .map(|index| (annotation.id.clone(), index))
            })
            .or_else(|| {
                document
                    .dimensions()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(|annotation| {
                        hit_dimension_handle(
                            annotation,
                            point,
                            tolerance_pt,
                            self.observed_pixels_per_point.0,
                        )
                        .map(|index| (annotation.id.clone(), index))
                    })
            });
        Ok(hit)
    }

    /// Arc control under the pointer in stable start/mid/end feedback order.
    /// The selected unlocked Arc wins before the topmost unlocked hovered Arc.
    pub fn hover_arc_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let to_hit = |annotation: &ArcAnnotation| {
            hit_arc_handle_index(
                annotation,
                point,
                tolerance_pt,
                self.observed_pixels_per_point.0,
            )
            .map(|index| (annotation.id.clone(), index))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.arcs().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .arcs()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Polyline/Polygon vertex under the pointer in stable path order. The
    /// selected unlocked path wins before the topmost unlocked hovered path.
    pub fn hover_vertex_path_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let to_hit = |annotation: &VertexPathAnnotation| {
            hit_vertex_path_handle_index(
                annotation,
                point,
                tolerance_pt,
                self.observed_pixels_per_point.0,
            )
            .map(|index| (annotation.id.clone(), index))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.vertex_paths().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .vertex_paths()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Polylength/Area vertex under the pointer in stable path order. The
    /// selected unlocked measurement wins before the topmost unlocked hovered
    /// measurement.
    pub fn hover_measurement_path_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let to_hit = |annotation: &MeasurementPathAnnotation| {
            hit_measurement_path_handle_index(
                annotation,
                point,
                tolerance_pt,
                self.observed_pixels_per_point.0,
            )
            .map(|index| (annotation.id.clone(), index))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.measurement_paths().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .measurement_paths()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Snapshot resize control under the pointer in rectangle feedback order.
    /// The selected unlocked Snapshot may also expose rotation at index eight;
    /// hovered-unselected Snapshots expose only their eight resize controls.
    /// A Pen or Highlight stroke's resize handle under `point`: the selected
    /// stroke's first, else the topmost unlocked stroke's.
    pub fn hover_ink_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let selected = document.selected_id();
        let hit = |pen: &crate::annotation_model::PenAnnotation| {
            InkResizeBox::of(pen.paths(), pen.appearance.width_pt())?
                .hit(point, tolerance_pt, self.observed_pixels_per_point.0)
                .map(|index| (pen.id.clone(), index))
        };
        let pens = || {
            document
                .pens()
                .iter()
                .rev()
                .filter(|pen| pen.page_index == page_index && !pen.locked)
        };
        Ok(pens()
            .filter(|pen| selected == Some(&pen.id))
            .find_map(hit)
            .or_else(|| pens().find_map(hit)))
    }

    pub fn hover_snapshot_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.snapshots().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(|annotation| {
                hit_snapshot_handle_index(
                    annotation,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                    true,
                )
                .map(|index| (annotation.id.clone(), index))
            })
            .or_else(|| {
                document
                    .snapshots()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(|annotation| {
                        hit_snapshot_handle_index(
                            annotation,
                            point,
                            tolerance_pt,
                            self.observed_pixels_per_point.0,
                            false,
                        )
                        .map(|index| (annotation.id.clone(), index))
                    })
            });
        Ok(hit)
    }

    /// Cloud+ handle under the pointer in stable feedback order: cloud
    /// vertices, eight text-box resize handles, then leader points. Selected
    /// geometry wins before topmost hovered-unselected geometry.
    pub fn hover_cloud_plus_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let selected = document.selected_id();
        let selected_cloud_plus = selected.and_then(|selected| {
            document.cloud_pluses().iter().find(|annotation| {
                annotation.page_index == page_index
                    && &annotation.id == selected
                    && !annotation.locked
            })
        });
        let hit = selected_cloud_plus
            .and_then(|annotation| {
                hit_cloud_plus_handle(
                    annotation,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                )
                .map(|index| (annotation.id.clone(), index))
            })
            .or_else(|| {
                document
                    .cloud_pluses()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(|annotation| {
                        hit_cloud_plus_handle(
                            annotation,
                            point,
                            tolerance_pt,
                            self.observed_pixels_per_point.0,
                        )
                        .map(|index| (annotation.id.clone(), index))
                    })
            });
        Ok(hit)
    }

    /// Cloud vertex under the pointer in stable control-path order. The
    /// selected unlocked Cloud wins before the topmost unlocked hovered Cloud
    /// so the first press on visible hover chrome begins the vertex transform.
    pub fn hover_cloud_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let handle_tolerance = tolerance_pt.max(
            crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                / self.observed_pixels_per_point.0.max(f64::EPSILON),
        );
        let to_hit = |annotation: &CloudAnnotation| {
            annotation
                .points()
                .iter()
                .enumerate()
                .rev()
                .find(|(_, vertex)| distance(**vertex, point) <= handle_tolerance)
                .map(|(index, _)| (annotation.id.clone(), index))
        };
        let selected = document.selected_id();
        let hit = selected
            .and_then(|selected| {
                document.clouds().iter().find(|annotation| {
                    annotation.page_index == page_index
                        && &annotation.id == selected
                        && !annotation.locked
                })
            })
            .and_then(to_hit)
            .or_else(|| {
                document
                    .clouds()
                    .iter()
                    .rev()
                    .filter(|annotation| {
                        annotation.page_index == page_index
                            && !annotation.locked
                            && selected != Some(&annotation.id)
                    })
                    .find_map(to_hit)
            });
        Ok(hit)
    }

    /// Resolves transform controls across families with the reference's global
    /// priority: the selected annotation first, otherwise the topmost hovered
    /// annotation in document order. Family-local indices remain unchanged.
    pub fn hover_transform_handle(
        &self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<(MarkupId, usize)>, AnnotationError> {
        let document = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        if self.tool != AnnotationTool::Select {
            return Ok(None);
        }
        let candidates = [
            self.hover_rectangle_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_redact_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_ellipse_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_straight_line_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_length_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_text_box_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_image_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_dimension_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_arc_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_vertex_path_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_measurement_path_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_snapshot_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_ink_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_callout_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_cloud_plus_handle(document_id, page_index, point, tolerance_pt)?,
            self.hover_cloud_handle(document_id, page_index, point, tolerance_pt)?,
        ];
        if let Some(selected) = document.selected_id()
            && let Some(candidate) = candidates.iter().flatten().find(|(id, _)| id == selected)
        {
            return Ok(Some(candidate.clone()));
        }
        Ok(document.annotation_order().iter().rev().find_map(|id| {
            candidates
                .iter()
                .flatten()
                .find(|(candidate, _)| candidate == id)
                .cloned()
        }))
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

    /// Returns a Callout only when the current selection contains exactly one.
    pub fn exact_selected_callout(&self, document_id: u64) -> Option<&CalloutAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::Callout(selected)] = selected.as_slice() else {
            return None;
        };
        document
            .callouts()
            .iter()
            .find(|callout| callout.id == selected.id)
    }

    /// Returns a Cloud+ only when the current selection contains exactly one.
    pub fn exact_selected_cloud_plus(&self, document_id: u64) -> Option<&CloudPlusAnnotation> {
        let document = self.documents.get(&document_id)?;
        let selected = document.selected_annotations_in_document_order();
        let [Annotation::CloudPlus(selected)] = selected.as_slice() else {
            return None;
        };
        document
            .cloud_pluses()
            .iter()
            .find(|cloud_plus| cloud_plus.id == selected.id)
    }

    pub fn cloud_plus(&self, document_id: u64, id: &MarkupId) -> Option<&CloudPlusAnnotation> {
        self.documents
            .get(&document_id)?
            .cloud_pluses()
            .iter()
            .find(|cloud_plus| &cloud_plus.id == id)
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
        self.pointer_down_with_viewport_input_and_selection_paths(
            document_id,
            page_index,
            pointer_id,
            button,
            point,
            viewport_point,
            tolerance_pt,
            modifiers,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn pointer_down_with_viewport_input_and_selection_paths(
        &mut self,
        document_id: u64,
        page_index: u32,
        pointer_id: u64,
        button: u8,
        point: PdfPoint,
        viewport_point: SelectionPoint,
        tolerance_pt: f64,
        modifiers: PointerInputModifiers,
        supplement: &AnnotationSelectionSupplement,
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
            let pdf_marquee = marquee_in_pdf(&marquee, &pdf_points);
            let document = self
                .documents
                .get_mut(&document_id)
                .ok_or(AnnotationError::NoActiveGesture)?;
            document.apply_marquee_selection_with_supplement(
                page_index,
                &pdf_marquee,
                selection_point_from_pdf,
                supplement,
            );
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
                Some(ActivePointer::CloudPlusCreate {
                    document_id,
                    page_index,
                    pointer_id,
                    ..
                })
                | Some(ActivePointer::ImageCreate {
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
            if let Some(ActivePointer::CloudPlusCreate {
                id, start, current, ..
            }) = &self.active
            {
                // A second click too close to the first leaves the rectangle pending.
                if point_distance_css_px(*start, *current, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                let (id, start, end) = (id.clone(), *start, *current);
                self.active = None;
                return self.commit_cloud_plus_rectangle(
                    document_id,
                    page_index,
                    id,
                    start,
                    end,
                    supplement,
                );
            }
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
                let calibration = self.measurement_calibration(document_id, page_index);
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
                    self.cloud_drag_pointer = None;
                    return Err(AnnotationError::NoActiveGesture);
                }
                // The second click places the rectangle's opposite corner.
                draft.hover = point;
                if let Some(corners) = cloud_rectangle_corners(
                    draft.points[0],
                    point,
                    self.observed_pixels_per_point.0,
                ) {
                    draft.points = corners;
                    draft.hover = draft.points[0];
                    return self.finish_cloud(document_id);
                }
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
                self.cloud_drag_pointer = Some(pointer_id);
            }
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
        // Ordinary Image remains active after its one-shot asset is consumed.
        // A following press clears placement selection without editing or duplicating it.
        if self.tool == AnnotationTool::Image && self.image_asset.is_none() {
            self.documents
                .entry(document_id)
                .or_default()
                .clear_selection();
            return Ok(PointerPhaseOutcome::SelectionChanged(None));
        }
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
        // An unselected box's edge resizes only where the cursor shows it.
        let transform_owner = if tool == AnnotationTool::Select {
            self.hover_transform_handle(document_id, page_index, point, tolerance_pt)
                .ok()
                .flatten()
                .map(|(id, _)| id)
        } else {
            None
        };
        let document = self.documents.entry(document_id).or_default();
        match tool {
            AnnotationTool::Select => {
                let selectable_hit_id = document
                    .hit_test(page_index, point, tolerance_pt)?
                    .filter(|hit| !matches!(hit, HitTarget::Body(_)))
                    .map(|hit| hit.markup_id().clone())
                    .or_else(|| {
                        hit_annotation_body_in_document_order(
                            document,
                            page_index,
                            point,
                            tolerance_pt,
                            supplement,
                            self.observed_pixels_per_point.0,
                        )
                    });
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
                        id: id.clone(),
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
                    transform_owner.as_ref(),
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
                        EllipseHandleKind::Resize(handle) => {
                            let grabbed = ellipse_resize_handle_point(&annotation, handle);
                            ActivePointer::EllipseResize {
                                document_id,
                                page_index,
                                pointer_id,
                                id: id.clone(),
                                handle,
                                start: point,
                                current: point,
                                original_rect: annotation.rect,
                                original_rotation_degrees: annotation.rotation_degrees,
                                grab_offset: (grabbed.x - point.x, grabbed.y - point.y),
                            }
                        }
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
                    transform_owner.as_ref(),
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
                        id: id.clone(),
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
                    let (snap_anchor_points, excluded_ids) =
                        moving_snap_context(document, page_index, supplement);
                    self.active = Some(ActivePointer::GroupMove {
                        document_id,
                        page_index,
                        pointer_id,
                        start: point,
                        current: point,
                        snap_anchor_points,
                        excluded_ids,
                        snap_caption_supplement: supplement.clone(),
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
                    let scene = document.thumbnail_scene(page_index);
                    let snap_anchor_points =
                        moving_annotation_snap_anchor_points_with_selection_supplement(
                            &scene,
                            std::slice::from_ref(&id),
                            128,
                            supplement,
                        );
                    // An end grabbed along its extension line follows the
                    // pointer from the measured point without jumping.
                    let grabbed = match kind {
                        DimensionPointerEditKind::Start => original.start,
                        DimensionPointerEditKind::End => original.end,
                        DimensionPointerEditKind::Offset | DimensionPointerEditKind::Body => point,
                    };
                    self.active = Some(ActivePointer::DimensionEdit {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        expected_revision,
                        kind,
                        start: grabbed,
                        current: grabbed,
                        original,
                        snap_anchor_points,
                        snap_caption_supplement: supplement.clone(),
                        grab_offset: (grabbed.x - point.x, grabbed.y - point.y),
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
                        id: id.clone(),
                        expected_revision,
                        kind,
                        start: point,
                        current: point,
                        original,
                        snap_anchor_points: moving_annotation_snap_anchor_points(
                            &document.document_scene(page_index),
                            std::slice::from_ref(&id),
                            128,
                        ),
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, kind)) = hit_selected_cloud_plus_control(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let expected_revision = document.snapshot().revision;
                    let original = document
                        .cloud_pluses()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a Cloud+ control hit must retain its annotation")
                        .clone();
                    if original.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    let snap_anchor_points = if matches!(
                        kind,
                        CloudPlusPointerEditKind::TextBox | CloudPlusPointerEditKind::Body
                    ) {
                        moving_annotation_snap_anchor_points(
                            &document.document_scene(page_index),
                            std::slice::from_ref(&id),
                            128,
                        )
                    } else {
                        Vec::new()
                    };
                    self.active = Some(ActivePointer::CloudPlusEdit {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        expected_revision,
                        kind,
                        start: point,
                        current: point,
                        original,
                        snap_anchor_points,
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
                    let snap_anchor_points = match kind {
                        CloudPointerEditKind::Body => original.points().to_vec(),
                        CloudPointerEditKind::Vertex(_) => Vec::new(),
                    };
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
                        snap_anchor_points,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, endpoint)) = hit_straight_line_endpoint(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
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
                if let Some((id, vertex_index)) = hit_selected_vertex_path_point(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
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
                if let Some((id, vertex_index)) = hit_selected_measurement_path_point(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
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
                if let Some((id, endpoint)) = hit_length_endpoint(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .lengths()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("an endpoint hit must retain its Length");
                    if annotation.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    // The press may be anywhere along the extension line;
                    // the measured point follows it without jumping.
                    let grabbed = match endpoint {
                        LengthEndpoint::Start => annotation.start,
                        LengthEndpoint::End => annotation.end,
                    };
                    self.active = Some(ActivePointer::LengthEndpoint {
                        document_id,
                        page_index,
                        pointer_id,
                        id: id.clone(),
                        endpoint,
                        current: grabbed,
                        grab_offset: (grabbed.x - point.x, grabbed.y - point.y),
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some(id) = hit_selected_text_box_rotation_handle(
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
                        .expect("a rotation-handle hit must retain its Text Box");
                    self.active = Some(ActivePointer::TextBoxRotate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start: point,
                        current: point,
                        original_rect: annotation.layout_rect,
                        original_rotation_degrees: annotation.rotation_degrees(),
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, handle)) = hit_selected_text_box_resize_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                    transform_owner.as_ref(),
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
                        original_rotation_degrees: annotation.rotation_degrees(),
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some(id) = hit_selected_image_rotation_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                ) {
                    document.select(&id);
                    let annotation = document
                        .images()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a rotation-handle hit must retain its Image");
                    self.active = Some(ActivePointer::ImageRotate {
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
                if let Some((id, handle)) = hit_selected_image_resize_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                    transform_owner.as_ref(),
                ) {
                    document.select(&id);
                    let annotation = document
                        .images()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .expect("a resize-handle hit must retain its image");
                    if annotation.locked {
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
                        original_rotation_degrees: annotation.rotation_degrees(),
                        aspect_locked: annotation.aspect_locked,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, index)) = hit_selected_ink_resize_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                    transform_owner.as_ref(),
                ) {
                    document.select(&id);
                    let pen = document
                        .pens()
                        .iter()
                        .find(|pen| pen.id == id)
                        .expect("a resize-handle hit must retain its stroke");
                    if pen.locked {
                        return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                    }
                    let original_box = InkResizeBox::of(pen.paths(), pen.appearance.width_pt())
                        .expect("a stroke with a handle has points");
                    self.active = Some(ActivePointer::InkResize {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        handle: RectangleResizeHandle::ALL[index],
                        start: point,
                        current: point,
                        original_paths: pen.paths().map(<[PdfPoint]>::to_vec).collect(),
                        original_box,
                    });
                    return Ok(PointerPhaseOutcome::GestureStarted);
                }
                if let Some((id, handle)) = hit_selected_snapshot_resize_handle(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    self.observed_pixels_per_point.0,
                    transform_owner.as_ref(),
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
                if let Some(id) = hit_annotation_body_in_document_order(
                    document,
                    page_index,
                    point,
                    tolerance_pt,
                    supplement,
                    self.observed_pixels_per_point.0,
                ) {
                    if let Some(annotation) = document
                        .rectangles()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .cloned()
                    {
                        if annotation.locked {
                            document.select(&id);
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
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
                        if let CommandOutcome::GestureStarted { kind, .. } = outcome {
                            self.active = Some(ActivePointer::Domain {
                                document_id,
                                page_index,
                                pointer_id,
                                ink: false,
                                ink_start: None,
                                rectangle_translation_start: matches!(kind, GestureKind::Move)
                                    .then_some(point),
                                rectangle_resize_handle: match kind {
                                    GestureKind::Resize(handle) => Some(handle),
                                    _ => None,
                                },
                                rectangle_create_start: None,
                                click_placement_pending: false,
                            });
                            return Ok(PointerPhaseOutcome::GestureStarted);
                        }
                        return Ok(PointerPhaseOutcome::SelectionChanged(
                            document.selected_id().cloned(),
                        ));
                    }
                    document.select(&id);
                    if let Some(annotation) = document
                        .straight_lines()
                        .iter()
                        .find(|annotation| annotation.id == id)
                    {
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
                            snap_anchor_points: vec![
                                annotation.start,
                                PdfPoint {
                                    x: (annotation.start.x + annotation.end.x) * 0.5,
                                    y: (annotation.start.y + annotation.end.y) * 0.5,
                                },
                                annotation.end,
                            ],
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
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
                            id: id.clone(),
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
                        let (snap_anchor_points, excluded_ids) =
                            moving_snap_context(document, page_index, supplement);
                        self.active = Some(ActivePointer::GroupMove {
                            document_id,
                            page_index,
                            pointer_id,
                            start: point,
                            current: point,
                            snap_anchor_points,
                            excluded_ids,
                            snap_caption_supplement: supplement.clone(),
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if document
                        .measurement_paths()
                        .iter()
                        .any(|annotation| annotation.id == id && !annotation.locked)
                    {
                        let (snap_anchor_points, excluded_ids) =
                            moving_snap_context(document, page_index, supplement);
                        self.active = Some(ActivePointer::GroupMove {
                            document_id,
                            page_index,
                            pointer_id,
                            start: point,
                            current: point,
                            snap_anchor_points,
                            excluded_ids,
                            snap_caption_supplement: supplement.clone(),
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .lengths()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .cloned()
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        let scene = document.document_scene(page_index);
                        let snap_anchor_points =
                            moving_annotation_snap_anchor_points_with_selection_supplement(
                                &scene,
                                std::slice::from_ref(&id),
                                128,
                                supplement,
                            );
                        self.active = Some(ActivePointer::LengthMove {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            start: point,
                            current: point,
                            original_start: annotation.start,
                            original_end: annotation.end,
                            snap_anchor_points,
                            snap_caption_supplement: supplement.clone(),
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
                        let scene = document.thumbnail_scene(page_index);
                        let snap_anchor_points =
                            moving_annotation_snap_anchor_points_with_selection_supplement(
                                &scene,
                                std::slice::from_ref(&id),
                                128,
                                supplement,
                            );
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
                            snap_anchor_points,
                            snap_caption_supplement: supplement.clone(),
                            grab_offset: (0., 0.),
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
                            id: id.clone(),
                            expected_revision: document.snapshot().revision,
                            kind,
                            start: point,
                            current: point,
                            original: annotation,
                            snap_anchor_points: moving_annotation_snap_anchor_points(
                                &document.document_scene(page_index),
                                std::slice::from_ref(&id),
                                128,
                            ),
                        });
                        return Ok(PointerPhaseOutcome::GestureStarted);
                    }
                    if let Some(annotation) = document
                        .cloud_pluses()
                        .iter()
                        .find(|annotation| annotation.id == id)
                        .cloned()
                    {
                        if annotation.locked {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        }
                        let kind = if rect_contains(annotation.text_box, point, tolerance_pt) {
                            CloudPlusPointerEditKind::TextBox
                        } else if cloud_plus_cloud_hit(&annotation, point, tolerance_pt) {
                            CloudPlusPointerEditKind::Body
                        } else {
                            return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                        };
                        let snap_anchor_points = moving_annotation_snap_anchor_points(
                            &document.document_scene(page_index),
                            std::slice::from_ref(&id),
                            128,
                        );
                        self.active = Some(ActivePointer::CloudPlusEdit {
                            document_id,
                            page_index,
                            pointer_id,
                            id,
                            expected_revision: document.snapshot().revision,
                            kind,
                            start: point,
                            current: point,
                            original: annotation,
                            snap_anchor_points,
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
                            snap_anchor_points: annotation.points().to_vec(),
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
                    let rectangle_resize_handle = match outcome {
                        CommandOutcome::GestureStarted {
                            kind: GestureKind::Resize(handle),
                            ..
                        } => Some(handle),
                        _ => None,
                    };
                    self.active = Some(ActivePointer::Domain {
                        document_id,
                        page_index,
                        pointer_id,
                        ink: false,
                        ink_start: None,
                        rectangle_translation_start,
                        rectangle_resize_handle,
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
                    rectangle_resize_handle: None,
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
                    click_placement_pending: false,
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
                    rectangle_resize_handle: None,
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
                    rectangle_resize_handle: None,
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
                // The image fills the box drawn for it, by a drag or by
                // clicking one corner and then the other.
                let id = id.expect("drawing tools allocate an annotation ID");
                document.clear_selection();
                self.active = Some(ActivePointer::ImageCreate {
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
        if let Some(id) = hit_selected_text_box_rotation_handle(
            document,
            page_index,
            point,
            tolerance_pt,
            self.observed_pixels_per_point.0,
        ) {
            let rotation = document
                .text_boxes()
                .iter()
                .find(|annotation| annotation.id == id)
                .expect("a Text Box rotation-handle hit must retain its annotation")
                .rotation_degrees();
            if rotation == 0. {
                return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
            }
            document.apply_command(AnnotationCommand::EditAnnotation {
                id: id.clone(),
                edit: AnnotationEdit::SetTextBoxRotation(0.),
            })?;
            return Ok(PointerPhaseOutcome::AnnotationEdited(id));
        }
        if let Some(id) = hit_selected_image_rotation_handle(
            document,
            page_index,
            point,
            tolerance_pt,
            self.observed_pixels_per_point.0,
        ) {
            let rotation = document
                .images()
                .iter()
                .find(|annotation| annotation.id == id)
                .expect("an Image rotation-handle hit must retain its annotation")
                .rotation_degrees();
            if rotation == 0. {
                return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
            }
            document.apply_command(AnnotationCommand::EditAnnotation {
                id: id.clone(),
                edit: AnnotationEdit::SetImageRotation(0.),
            })?;
            return Ok(PointerPhaseOutcome::AnnotationEdited(id));
        }
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
            && document.callouts().iter().any(|annotation| {
                let text_box = annotation.text_box;
                annotation.id == selected
                    && annotation.page_index == page_index
                    && !annotation.locked
                    && point.x >= text_box.x - tolerance_pt
                    && point.x <= text_box.x + text_box.width + tolerance_pt
                    && point.y >= text_box.y - tolerance_pt
                    && point.y <= text_box.y + text_box.height + tolerance_pt
            })
        {
            return Ok(PointerPhaseOutcome::SelectionChanged(Some(selected)));
        }
        if let Some(selected) = document.selected_id().cloned()
            && document.cloud_pluses().iter().any(|annotation| {
                annotation.id == selected
                    && annotation.page_index == page_index
                    && !annotation.locked
                    && cloud_plus_hit(annotation, point, tolerance_pt)
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
            None,
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

    /// The pointer moved by the active gesture's grab offset, so a control
    /// grabbed off its exact point follows the pointer without jumping.
    fn offset_by_grab(&self, point: PdfPoint) -> PdfPoint {
        let (dx, dy) = match &self.active {
            Some(ActivePointer::LengthEndpoint { grab_offset, .. })
            | Some(ActivePointer::DimensionEdit { grab_offset, .. }) => *grab_offset,
            _ => return point,
        };
        PdfPoint {
            x: point.x + dx,
            y: point.y + dy,
        }
    }

    /// Click-built paths retain their draft between presses, rather than an
    /// `ActivePointer`. Both pressed and unpressed moves must update that draft.
    fn update_pending_path_preview(
        &mut self,
        point: PdfPoint,
        constrain_orthogonal: bool,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        if let Some(draft) = self.vertex_path_draft.as_mut() {
            draft.hover = point;
        } else if let Some(draft) = self.cloud_draft.as_mut() {
            draft.hover = point;
        } else if let Some(draft) = self.measurement_path_draft.as_mut() {
            draft.hover = point;
        } else if let Some(draft) = self.arc_draft.as_ref() {
            let (document_id, page_index) = (draft.document_id, draft.page_index);
            self.update_arc_hover(document_id, page_index, point, constrain_orthogonal)?;
        } else {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        Ok(PointerPhaseOutcome::PlacementPending)
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
        let point = self.offset_by_grab(point);
        let raw_point = point;
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
        let point = self.resolve_equal_size_resize_point(point);
        let point = self.resolve_equal_size_placement_point(point, constrain_orthogonal);
        if let Some(draft) = self.snapshot_draft.as_mut() {
            require_pointer(draft.pointer_id, pointer_id)?;
            draft.current = point;
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        let Some(active) = self.active.as_mut() else {
            if let Some(active_pointer) = self.cloud_drag_pointer {
                require_pointer(active_pointer, pointer_id)?;
            }
            return self.update_pending_path_preview(point, constrain_orthogonal);
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
            | ActivePointer::LengthMove {
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
            | ActivePointer::CloudPlusEdit {
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
            }
            | ActivePointer::ImageCreate {
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
            ActivePointer::TextBoxRotate {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = raw_point;
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
            | ActivePointer::InkResize {
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
            ActivePointer::ImageRotate {
                pointer_id: active_pointer,
                current,
                ..
            } => {
                require_pointer(*active_pointer, pointer_id)?;
                *current = raw_point;
                Ok(PointerPhaseOutcome::GestureStarted)
            }
        }
    }

    pub fn pointer_up(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        if self.snapshot_draft.is_some() {
            let point = self.resolve_equal_size_placement_point(point, false);
            let draft = self
                .snapshot_draft
                .as_mut()
                .expect("the Snapshot draft was checked before relationship snapping");
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
        self.pointer_up_with_viewport_input_and_selection_paths(
            pointer_id,
            point,
            viewport_point,
            modifiers,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn pointer_up_with_viewport_input_and_selection_paths(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
        viewport_point: SelectionPoint,
        modifiers: PointerInputModifiers,
        supplement: &AnnotationSelectionSupplement,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let constrain_orthogonal = modifiers.shift;
        let point = self.offset_by_grab(point);
        let raw_point = point;
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
        let point = self.resolve_equal_size_resize_point(point);
        let point = self.resolve_equal_size_placement_point(point, constrain_orthogonal);
        if let Some(draft) = self.snapshot_draft.as_mut() {
            require_pointer(draft.pointer_id, pointer_id)?;
            draft.current = point;
            return Ok(PointerPhaseOutcome::PlacementPending);
        }
        if let Some(active_pointer) = self.cloud_drag_pointer.take() {
            require_pointer(active_pointer, pointer_id)?;
        }
        // Dragging draws a rectangular cloud; a click leaves its first corner
        // for a second click to finish.
        if self.tool == AnnotationTool::Cloud
            && let Some(draft) = self.cloud_draft.as_mut()
            && draft.points.len() == 1
            && let Some(corners) =
                cloud_rectangle_corners(draft.points[0], point, self.observed_pixels_per_point.0)
        {
            draft.points = corners;
            draft.hover = draft.points[0];
            let document_id = draft.document_id;
            return self.finish_cloud(document_id);
        }
        if self.active.is_none() {
            return self.update_pending_path_preview(point, constrain_orthogonal);
        }
        let active = self.active.take().ok_or(AnnotationError::NoActiveGesture)?;
        let outcome = match active {
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
                // A click on empty page, without a drag, arms a selection
                // box from where it was: the next click finishes it.
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
                let pdf_marquee = marquee_in_pdf(&marquee, &pdf_points);
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                document.apply_marquee_selection_with_supplement(
                    page_index,
                    &pdf_marquee,
                    selection_point_from_pdf,
                    supplement,
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
                rectangle_resize_handle,
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
                        rectangle_resize_handle,
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
                        rectangle_resize_handle,
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
                grab_offset,
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
                            offset_point(point, grab_offset),
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
            ActivePointer::ImageCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                current,
                click_placement_pending,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if click_placement_pending
                    || point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                        < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    self.active = Some(ActivePointer::ImageCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start,
                        current: if click_placement_pending {
                            current
                        } else {
                            point
                        },
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                self.commit_image(document_id, page_index, id, start, point)
            }
            ActivePointer::CloudPlusCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                current,
                click_placement_pending,
            } => {
                require_pointer(active_pointer, pointer_id)?;
                // A click, rather than a drag, leaves the rectangle pending
                // until a second click places its opposite corner.
                if click_placement_pending
                    || point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                        < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    self.active = Some(ActivePointer::CloudPlusCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start,
                        current: if click_placement_pending {
                            current
                        } else {
                            point
                        },
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                self.commit_cloud_plus_rectangle(
                    document_id,
                    page_index,
                    id,
                    start,
                    point,
                    supplement,
                )
            }
            ActivePointer::StraightLineMove {
                document_id,
                pointer_id: active_pointer,
                id,
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
            ActivePointer::LengthMove {
                document_id,
                pointer_id: active_pointer,
                id,
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
                        edit: AnnotationEdit::TranslateLength {
                            delta_x: point.x - start.x,
                            delta_y: point.y - start.y,
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
                    CalloutPointerEditKind::TextBoxResize(handle) => {
                        let text_box = original
                            .text_box
                            .rotated_resize_from_handle(0., handle, point);
                        AnnotationEdit::SetCalloutTextBox(text_box)
                    }
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
            ActivePointer::CloudPlusEdit {
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
                let routing_context =
                    self.cloud_plus_routing_context(document_id, page_index, Some(&id), supplement);
                let preview = resolve_cloud_plus_pointer_edit(
                    &original,
                    kind,
                    start,
                    point,
                    &routing_context,
                )?;
                let document = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?;
                validate_cloud_plus_pointer_target(
                    document,
                    page_index,
                    &id,
                    expected_revision,
                    &original,
                )?;
                let edit = match kind {
                    CloudPlusPointerEditKind::CloudVertex(vertex_index) => {
                        AnnotationEdit::SetCloudPlusCloudPoint {
                            vertex_index,
                            point: preview.cloud_points()[vertex_index],
                            leader_points: preview.leader_points().to_vec(),
                        }
                    }
                    CloudPlusPointerEditKind::TextBoxResize(_)
                    | CloudPlusPointerEditKind::TextBox => AnnotationEdit::SetCloudPlusTextBox {
                        text_box: preview.text_box,
                        leader_points: preview.leader_points().to_vec(),
                    },
                    CloudPlusPointerEditKind::LeaderPoint(_) => {
                        AnnotationEdit::SetCloudPlusLeaderPoints(preview.leader_points().to_vec())
                    }
                    CloudPlusPointerEditKind::Body => AnnotationEdit::TranslateCloudPlusGroup {
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
            ActivePointer::InkResize {
                document_id,
                pointer_id: active_pointer,
                id,
                handle,
                start,
                original_paths,
                original_box,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let paths = original_box.resized_paths(
                    original_paths.iter().map(Vec::as_slice),
                    handle,
                    point,
                );
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
                        edit: AnnotationEdit::SetTextBoxLayoutRect(rect),
                    })?;
                Ok(PointerPhaseOutcome::AnnotationEdited(id))
            }
            ActivePointer::TextBoxRotate {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                original_rotation_degrees,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, raw_point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rotation = ellipse_rotation_from_drag(
                    original_rect,
                    original_rotation_degrees,
                    start,
                    raw_point,
                );
                let outcome = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetTextBoxRotation(rotation),
                    })?;
                Ok(match outcome {
                    CommandOutcome::AnnotationEdited { changed: true, .. } => {
                        PointerPhaseOutcome::AnnotationEdited(id)
                    }
                    _ => PointerPhaseOutcome::SelectionChanged(Some(id)),
                })
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
                original_rotation_degrees,
                aspect_locked,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rect = resized_image_rect(
                    original_rect,
                    handle,
                    start,
                    point,
                    original_rotation_degrees,
                    aspect_locked,
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
            ActivePointer::ImageRotate {
                document_id,
                pointer_id: active_pointer,
                id,
                start,
                original_rect,
                original_rotation_degrees,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                if point_distance_css_px(start, raw_point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    return Ok(PointerPhaseOutcome::SelectionChanged(Some(id)));
                }
                let rotation = ellipse_rotation_from_drag(
                    original_rect,
                    original_rotation_degrees,
                    start,
                    raw_point,
                );
                let outcome = self
                    .documents
                    .get_mut(&document_id)
                    .ok_or(AnnotationError::NoActiveGesture)?
                    .apply_command(AnnotationCommand::EditAnnotation {
                        id: id.clone(),
                        edit: AnnotationEdit::SetImageRotation(rotation),
                    })?;
                Ok(match outcome {
                    CommandOutcome::AnnotationEdited { changed: true, .. } => {
                        PointerPhaseOutcome::AnnotationEdited(id)
                    }
                    _ => PointerPhaseOutcome::SelectionChanged(Some(id)),
                })
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
        };
        self.acquired_tracking_points.clear();
        self.tracking_hover_key = None;
        self.object_snap_tracking_result = None;
        self.relationship_snap_guides.clear();
        outcome
    }

    pub fn cancel(&mut self, reason: PointerCancelReason) -> Result<(), AnnotationError> {
        if reason != PointerCancelReason::AdapterError {
            self.semantic_snap_decision = None;
            self.acquired_tracking_points.clear();
            self.tracking_hover_key = None;
            self.object_snap_tracking_result = None;
            self.relationship_snap_guides.clear();
        }
        self.vertex_path_draft = None;
        self.cloud_draft = None;
        self.cloud_drag_pointer = None;
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
            ActivePointer::ImageCreate {
                document_id,
                page_index,
                pointer_id: active_pointer,
                id,
                start,
                click_placement_pending: true,
                ..
            } => {
                require_pointer(active_pointer, pointer_id)?;
                // A second click on the first corner leaves the box pending.
                if point_distance_css_px(start, point, self.observed_pixels_per_point.0)
                    < POINTER_DRAG_THRESHOLD_CSS_PX
                {
                    self.active = Some(ActivePointer::ImageCreate {
                        document_id,
                        page_index,
                        pointer_id,
                        id,
                        start,
                        current: point,
                        click_placement_pending: true,
                    });
                    return Ok(PointerPhaseOutcome::PlacementPending);
                }
                self.commit_image(document_id, page_index, id, start, point)
            }
            active => {
                self.active = Some(active);
                Err(AnnotationError::NoActiveGesture)
            }
        }
    }

    /// Places the pending image at its natural size, centred on `point`, as
    /// for an image file dropped on the page. Drawing tools place it in the
    /// box drawn for it instead.
    pub fn place_image_at(
        &mut self,
        document_id: u64,
        page_index: u32,
        point: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let pending = self.image_asset.clone().ok_or_else(|| {
            AnnotationError::InvalidFixture(
                "image tool requires a decoded bounded PNG or JPEG asset".into(),
            )
        })?;
        let placement_page = self.image_placement_page.ok_or_else(|| {
            AnnotationError::InvalidGeometry(
                "image tool requires the current page dimensions".into(),
            )
        })?;
        self.cancel(PointerCancelReason::AdapterError)?;
        let point = self.resolve_semantic_creation_point(document_id, page_index, point, false);
        let id = self.next_id(AnnotationTool::Image)?;
        let annotation = ImageAnnotation::new_with_opacity(
            id.clone(),
            page_index,
            image_placement_rect(&pending, placement_page, point)?,
            pending.asset,
            pending.aspect_locked,
            self.tool_properties(AnnotationTool::Image).opacity,
        )?;
        self.documents
            .entry(document_id)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Image(
                annotation,
            )))?;
        self.image_asset = None;
        self.image_placement_page = None;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    fn commit_image(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let pending = self.image_asset.clone().ok_or_else(|| {
            AnnotationError::InvalidFixture(
                "image tool requires a decoded bounded PNG or JPEG asset".into(),
            )
        })?;
        let annotation = ImageAnnotation::new_with_opacity(
            id.clone(),
            page_index,
            image_drawn_rect(&pending, start, end)?,
            pending.asset,
            pending.aspect_locked,
            self.tool_properties(AnnotationTool::Image).opacity,
        )?;
        self.documents
            .entry(document_id)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Image(
                annotation,
            )))?;
        self.image_asset = None;
        self.image_placement_page = None;
        Ok(PointerPhaseOutcome::AnnotationCreated(id))
    }

    fn commit_cloud_plus_rectangle(
        &mut self,
        document_id: u64,
        page_index: u32,
        id: MarkupId,
        start: PdfPoint,
        end: PdfPoint,
        supplement: &AnnotationSelectionSupplement,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        if (end.x - start.x).hypot(end.y - start.y) <= LENGTH_MINIMUM_PDF_DISTANCE {
            return Ok(PointerPhaseOutcome::Ignored);
        }
        let rect = PdfRect::from_corners(start, end);
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
            supplement,
        )
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
        let properties = self.tool_properties(AnnotationTool::Callout);
        let appearance = callout_tool_appearance(&properties)?;
        let (text_box, leader_points) = callout_initial_layout(start, end, appearance.text())?;
        let annotation = match CalloutAnnotation::new(
            id.clone(),
            page_index,
            leader_points,
            text_box,
            CALLOUT_DEFAULT_TEXT,
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
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Result<PointerPhaseOutcome, AnnotationError> {
        let properties = self.tool_properties(AnnotationTool::CloudPlus);
        let routing_context =
            self.cloud_plus_routing_context(document_id, page_index, None, caption_supplement);
        let visible_path = cloud_visible_path(&cloud_points, properties.cloud_intensity)?;
        let appearance = cloud_plus_tool_appearance(&properties)?;
        let placement = place_initial_cloud_plus_text_box(
            &cloud_points,
            &visible_path,
            caption_box_width("Cloud+", appearance.text()),
            CLOUD_PLUS_TEXT_HEIGHT_PT,
            CLOUD_PLUS_TEXT_GAP_PT,
            &routing_context,
        )?;
        let annotation = CloudPlusAnnotation::new(
            id.clone(),
            page_index,
            cloud_points,
            properties.cloud_intensity,
            placement.leader.points,
            placement.text_box,
            "Cloud+",
            appearance,
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
        let content = content.into();
        let text_box = callout_fitted_text_box(
            document
                .callouts()
                .iter()
                .find(|annotation| &annotation.id == id)
                .ok_or(AnnotationError::NoSelection)?,
            &content,
        );
        document.replace_callout_content_in_create_transaction(id, content, text_box)?;
        Ok(())
    }

    pub fn callout(&self, document_id: u64, id: &MarkupId) -> Option<&CalloutAnnotation> {
        self.documents
            .get(&document_id)?
            .callouts()
            .iter()
            .find(|callout| &callout.id == id)
    }

    /// Replaces an existing Callout's text, refitting its box, as one
    /// undoable change.
    pub fn replace_callout_text(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        let content = content.into();
        let text_box = callout_fitted_text_box(
            self.callout(document_id, id)
                .ok_or(AnnotationError::NoSelection)?,
            &content,
        );
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id.clone(),
                edit: AnnotationEdit::SetCalloutContentAndTextBox { content, text_box },
            })?;
        Ok(())
    }

    pub fn replace_cloud_plus_text_in_create_transaction(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        self.replace_cloud_plus_text_in_create_transaction_with_routing_supplement(
            document_id,
            id,
            content,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn replace_cloud_plus_text_in_create_transaction_with_routing_supplement(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Result<(), AnnotationError> {
        let annotation = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .cloud_pluses()
            .iter()
            .find(|annotation| &annotation.id == id)
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let content = content.into();
        let routing_context = self.cloud_plus_routing_context(
            document_id,
            annotation.page_index,
            Some(id),
            caption_supplement,
        );
        let (text_box, leader_points) =
            cloud_plus_text_layout(&annotation, &content, &routing_context)?;
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        document.replace_cloud_plus_content_and_layout_in_create_transaction(
            id,
            content,
            text_box,
            leader_points,
        )?;
        Ok(())
    }

    pub fn replace_cloud_plus_text(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<(), AnnotationError> {
        self.replace_cloud_plus_text_with_routing_supplement(
            document_id,
            id,
            content,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn replace_cloud_plus_text_with_routing_supplement(
        &mut self,
        document_id: u64,
        id: &MarkupId,
        content: impl Into<String>,
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> Result<(), AnnotationError> {
        let annotation = self
            .documents
            .get(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .cloud_pluses()
            .iter()
            .find(|annotation| &annotation.id == id)
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let content = content.into();
        let routing_context = self.cloud_plus_routing_context(
            document_id,
            annotation.page_index,
            Some(id),
            caption_supplement,
        );
        let (text_box, leader_points) =
            cloud_plus_text_layout(&annotation, &content, &routing_context)?;
        let document = self
            .documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?;
        document.apply_command(AnnotationCommand::EditAnnotation {
            id: id.clone(),
            edit: AnnotationEdit::SetCloudPlusContentAndLayout {
                content,
                text_box,
                leader_points,
            },
        })?;
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

    /// Focused annotation for keyboard-focus feedback. Tracks the most
    /// recently selected id; empty when nothing is selected.
    pub fn focused_id(&self, document_id: u64) -> Option<MarkupId> {
        self.documents.get(&document_id)?.focused_id().cloned()
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

    /// Turns a box shape back to 0° (a double-click on its rotation knob).
    pub fn reset_rotation(
        &mut self,
        document_id: u64,
        id: &MarkupId,
    ) -> Result<Option<bool>, AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .reset_rotation(id)
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
        self.document_scene_with_routing_supplement(
            document_id,
            page_index,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn document_scene_with_routing_supplement(
        &self,
        document_id: u64,
        page_index: u32,
        caption_supplement: &AnnotationSelectionSupplement,
    ) -> AnnotationScene {
        self.document_scene_with_table_preview(
            document_id,
            page_index,
            caption_supplement,
            ContentPreview::default(),
        )
    }

    /// [`Self::document_scene_with_routing_supplement`], with the markups
    /// on page content being dragged moved with it.
    pub fn document_scene_with_table_preview(
        &self,
        document_id: u64,
        page_index: u32,
        caption_supplement: &AnnotationSelectionSupplement,
        preview: ContentPreview<'_>,
    ) -> AnnotationScene {
        let mut scene = self
            .documents
            .get(&document_id)
            .map(|document| {
                preview
                    .dragged
                    .and_then(|(groups, markups, delta)| {
                        document.with_moved_markups(page_index, groups, markups, delta)
                    })
                    .map_or_else(
                        || document.document_scene(page_index),
                        |preview| preview.document_scene(page_index),
                    )
            })
            .unwrap_or_else(|| empty_scene(page_index));
        let chrome_visible = self.manipulation_chrome_visible(document_id, page_index);
        for annotation in &mut scene.rectangles {
            annotation.feedback = match annotation.feedback {
                SceneInteractionFeedback::Move { .. } => {
                    SceneInteractionFeedback::Move { chrome_visible }
                }
                SceneInteractionFeedback::Transform { active_handle, .. } => {
                    SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle,
                    }
                }
                feedback => feedback,
            };
        }
        if let Some(draft) = &self.arc_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
            && draft.end.is_none()
            && draft.start != draft.mid
        {
            scene.straight_lines.push(SceneStraightLine {
                id: draft.id.clone(),
                start: draft.start,
                end: draft.mid,
                kind: LineKind::Line,
                appearance: StraightLineAppearance::new(
                    draft.appearance.stroke_color(),
                    draft.appearance.stroke_width_pt(),
                    draft.appearance.opacity(),
                    draft.appearance.stroke_style(),
                )
                .expect("stored Arc tool properties are validated"),
                selected: true,
                locked: false,
                draft: true,
                feedback: SceneInteractionFeedback::Creation,
            });
        }
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
                rect: annotation.rect(),
                angle1_degrees: annotation.angle1_degrees(),
                angle2_degrees: annotation.angle2_degrees(),
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
                feedback: SceneInteractionFeedback::Creation,
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
                feedback: SceneInteractionFeedback::Creation,
            });
        }
        if let Some(ActivePointer::EllipseCreate {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            appearance,
            start,
            current,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && start != current
            && let Ok(annotation) = EllipseAnnotation::new(
                id.clone(),
                page_index,
                PdfRect::from_corners(*start, *current),
                appearance.clone(),
            )
        {
            scene.ellipses.push(SceneRectangle {
                id: annotation.id,
                rect: annotation.rect,
                rotation_degrees: annotation.rotation_degrees,
                appearance: annotation.appearance,
                selected: true,
                locked: false,
                preview: true,
                feedback: SceneInteractionFeedback::Creation,
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
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                .clouds
                .iter_mut()
                .filter(|annotation| annotation.selected && !annotation.locked)
            {
                for point in &mut annotation.points {
                    *point = PdfPoint::new(point.x + delta_x, point.y + delta_y)
                        .expect("validated pointer points produce a finite Cloud preview");
                }
                for point in &mut annotation.scallop_path {
                    *point = PdfPoint::new(point.x + delta_x, point.y + delta_y)
                        .expect("validated pointer points produce a finite Cloud scallop preview");
                }
                annotation.draft = true;
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                annotation.draft = true;
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
                annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
            && let Ok(rect) = redact_resized_rect(*original_rect, *handle, *current)
        {
            annotation.rect = rect;
            annotation.draft = true;
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: RectangleResizeHandle::ALL
                    .iter()
                    .position(|candidate| candidate == handle)
                    .expect("a Redact resize uses a known handle"),
            };
        }
        if let Some(ActivePointer::EllipseResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            current,
            original_rect,
            original_rotation_degrees,
            grab_offset,
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
                offset_point(*current, *grab_offset),
            );
            annotation.preview = true;
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: RectangleResizeHandle::ALL
                    .iter()
                    .position(|candidate| candidate == handle)
                    .expect("an Ellipse resize uses a known handle"),
            };
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
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: RectangleResizeHandle::ALL.len(),
            };
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
            let preview = original
                .translated(delta_x, delta_y)
                .expect("translated Arc preview geometry remains valid");
            annotation.start = preview.start;
            annotation.end = preview.end;
            annotation.mid = preview.mid;
            annotation.rect = preview.rect();
            annotation.angle1_degrees = preview.angle1_degrees();
            annotation.angle2_degrees = preview.angle2_degrees();
            annotation.sampled_path = preview.sampled_path(64);
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
            if let Ok(preview) = original.with_control_point(*control, resolved) {
                annotation.start = preview.start;
                annotation.end = preview.end;
                annotation.mid = preview.mid;
                annotation.rect = preview.rect();
                annotation.angle1_degrees = preview.angle1_degrees();
                annotation.angle2_degrees = preview.angle2_degrees();
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
                feedback: SceneInteractionFeedback::Creation,
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
            let appearance = callout_tool_appearance(&properties)
                .expect("stored Callout tool properties are validated");
            let (text_box, leader_points) =
                callout_initial_layout(*start, *current, appearance.text())
                    .expect("a Callout drag between distinct finite points has a layout");
            scene.callouts.push(SceneCallout {
                id: id.clone(),
                leader_points,
                text_box,
                content: CALLOUT_DEFAULT_TEXT.into(),
                appearance,
                selected: true,
                locked: false,
                draft: true,
                feedback: SceneInteractionFeedback::Creation,
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
            let routing_context =
                self.cloud_plus_routing_context(document_id, page_index, None, caption_supplement);
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
                && let Ok(appearance) = cloud_plus_tool_appearance(&properties)
                && let Ok(placement) = place_initial_cloud_plus_text_box(
                    &cloud_points,
                    &visible_path,
                    caption_box_width("Cloud+", appearance.text()),
                    CLOUD_PLUS_TEXT_HEIGHT_PT,
                    CLOUD_PLUS_TEXT_GAP_PT,
                    &routing_context,
                )
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
                    feedback: SceneInteractionFeedback::Creation,
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
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
            let appearance =
                rectangle_tool_appearance(&properties, draft.kind == VertexPathKind::Polygon)
                    .expect("stored vertex-path tool properties are validated");
            if draft.kind == VertexPathKind::Polygon
                && properties.cloud_style
                && points.len() >= 3
                && let Ok(cloud) = CloudAnnotation::new(
                    draft.id.clone(),
                    draft.page_index,
                    points.clone(),
                    properties.cloud_intensity,
                    appearance.clone(),
                )
            {
                scene.clouds.push(SceneCloud {
                    id: draft.id.clone(),
                    scallop_path: cloud.scallop_path(),
                    points,
                    border_effect_intensity: properties.cloud_intensity,
                    appearance,
                    selected: true,
                    locked: false,
                    draft: true,
                    feedback: SceneInteractionFeedback::Creation,
                });
            } else {
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
        }
        if let Some(draft) = &self.cloud_draft
            && (draft.document_id, draft.page_index) == (document_id, page_index)
        {
            let properties = self.tool_properties(AnnotationTool::Cloud);
            let mut points = draft.points.clone();
            if points.len() == 1
                && let Some(corners) = cloud_rectangle_corners(
                    points[0],
                    draft.hover,
                    self.observed_pixels_per_point.0,
                )
            {
                points = corners;
            } else if point_distance_css_px(
                *points
                    .last()
                    .expect("a cloud draft retains its first point"),
                draft.hover,
                self.observed_pixels_per_point.0,
            ) >= 0.5 * self.observed_pixels_per_point.0
            {
                points.push(draft.hover);
            }
            let appearance = rectangle_tool_appearance(&properties, true)
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
                feedback: SceneInteractionFeedback::Creation,
            });
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
            let text_style = caption_tool_style(&self.tool_properties(tool))
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
            if let Some(retained) = self.documents.get(&document_id).and_then(|document| {
                document
                    .measurement_paths()
                    .iter()
                    .find(|retained| retained.id == *id)
            }) && let Ok(preview) = MeasurementPathAnnotation::new_with_text_style(
                retained.id.clone(),
                retained.page_index,
                annotation.points.clone(),
                retained.kind,
                retained.calibration().clone(),
                retained.appearance.clone(),
                retained.text_style().clone(),
            ) {
                annotation.caption = preview.caption();
            }
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
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: match endpoint {
                    LineEndpoint::Start => 0,
                    LineEndpoint::End => 1,
                },
            };
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
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
        }
        if let Some(ActivePointer::InkResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            current,
            original_paths,
            original_box,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .pens
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.paths = original_box.resized_paths(
                original_paths.iter().map(Vec::as_slice),
                *handle,
                *current,
            );
            annotation.points = annotation.paths.first().cloned().unwrap_or_default();
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: RectangleResizeHandle::ALL
                    .iter()
                    .position(|candidate| candidate == handle)
                    .expect("a stroke resize uses a known handle"),
            };
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
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
        }
        if let Some(ActivePointer::TextBoxResize {
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
                .text_boxes
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.layout_rect = original_rect.rotated_resize_from_handle(
                *original_rotation_degrees,
                *handle,
                *current,
            );
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: RectangleResizeHandle::ALL
                    .iter()
                    .position(|candidate| candidate == handle)
                    .expect("a Text Box resize uses a known handle"),
            };
        }
        if let Some(ActivePointer::TextBoxRotate {
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
                .text_boxes
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rotation_degrees = ellipse_rotation_from_drag(
                *original_rect,
                *original_rotation_degrees,
                *start,
                *current,
            );
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: 8,
            };
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
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
        }
        if let Some(ActivePointer::ImageResize {
            document_id: active_document_id,
            page_index: active_page_index,
            id,
            handle,
            start,
            current,
            original_rect,
            original_rotation_degrees,
            aspect_locked,
            ..
        }) = &self.active
            && (*active_document_id, *active_page_index) == (document_id, page_index)
            && let Some(annotation) = scene
                .images
                .iter_mut()
                .find(|annotation| annotation.id == *id)
            && let Ok(rect) = resized_image_rect(
                *original_rect,
                *handle,
                *start,
                *current,
                *original_rotation_degrees,
                *aspect_locked,
            )
        {
            annotation.rect = rect;
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: ImageResizeHandle::ALL
                    .iter()
                    .position(|candidate| candidate == handle)
                    .expect("an Image resize uses a known handle"),
            };
        }
        if let Some(ActivePointer::ImageRotate {
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
                .images
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            annotation.rotation_degrees = ellipse_rotation_from_drag(
                *original_rect,
                *original_rotation_degrees,
                *start,
                *current,
            );
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: 8,
            };
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
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: RectangleResizeHandle::ALL
                    .iter()
                    .position(|candidate| candidate == handle)
                    .expect("a Snapshot resize uses a known handle"),
            };
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
            annotation.feedback = SceneInteractionFeedback::Transform {
                chrome_visible,
                active_handle: RectangleResizeHandle::ALL.len(),
            };
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
                annotation.feedback = match kind {
                    DimensionPointerEditKind::Body => {
                        SceneInteractionFeedback::Move { chrome_visible }
                    }
                    DimensionPointerEditKind::Start => SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle: 0,
                    },
                    DimensionPointerEditKind::End => SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle: 1,
                    },
                    DimensionPointerEditKind::Offset => SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle: 2,
                    },
                };
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
                CalloutPointerEditKind::TextBoxResize(handle) => {
                    text_box = original
                        .text_box
                        .rotated_resize_from_handle(0., *handle, *current);
                    if let Ok(resized) = original.resized_text_box(text_box) {
                        leader_points = resized.leader_points().to_vec();
                    }
                }
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
                annotation.feedback = match kind {
                    CalloutPointerEditKind::TextBoxResize(handle) => {
                        SceneInteractionFeedback::Transform {
                            chrome_visible,
                            active_handle: RectangleResizeHandle::ALL
                                .iter()
                                .position(|candidate| candidate == handle)
                                .expect("a Callout resize handle must use the shared handle order"),
                        }
                    }
                    CalloutPointerEditKind::LeaderPoint(index) => {
                        SceneInteractionFeedback::Transform {
                            chrome_visible,
                            active_handle: RectangleResizeHandle::ALL.len() + *index,
                        }
                    }
                    CalloutPointerEditKind::TextBox | CalloutPointerEditKind::Body => {
                        SceneInteractionFeedback::Move { chrome_visible }
                    }
                };
            }
        }
        if let Some(ActivePointer::CloudPlusEdit {
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
                .cloud_pluses
                .iter_mut()
                .find(|annotation| annotation.id == *id)
            && let Ok(preview) = resolve_cloud_plus_pointer_edit(
                original,
                *kind,
                *start,
                *current,
                &self.cloud_plus_routing_context(
                    document_id,
                    page_index,
                    Some(id),
                    caption_supplement,
                ),
            )
        {
            annotation.cloud_points = preview.cloud_points().to_vec();
            annotation.scallop_path = preview.scallop_path();
            annotation.leader_points = preview.leader_points().to_vec();
            annotation.text_box = preview.text_box;
            annotation.draft = true;
            annotation.feedback = match kind {
                CloudPlusPointerEditKind::CloudVertex(index) => {
                    SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle: *index,
                    }
                }
                CloudPlusPointerEditKind::TextBoxResize(handle) => {
                    SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle: original.cloud_points().len()
                            + RectangleResizeHandle::ALL
                                .iter()
                                .position(|candidate| candidate == handle)
                                .expect("a Cloud+ resize handle must use the shared order"),
                    }
                }
                CloudPlusPointerEditKind::LeaderPoint(index) => {
                    SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle: original.cloud_points().len()
                            + RectangleResizeHandle::ALL.len()
                            + *index,
                    }
                }
                CloudPlusPointerEditKind::TextBox | CloudPlusPointerEditKind::Body => {
                    SceneInteractionFeedback::Move { chrome_visible }
                }
            };
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
                annotation.feedback = match kind {
                    CloudPointerEditKind::Vertex(index) => SceneInteractionFeedback::Transform {
                        chrome_visible,
                        active_handle: *index,
                    },
                    CloudPointerEditKind::Body => SceneInteractionFeedback::Move { chrome_visible },
                };
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
                "",
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
                    feedback: SceneInteractionFeedback::Creation,
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
            && let Ok(annotation) = LengthAnnotation::new_with_appearance(
                id.clone(),
                page_index,
                *start,
                *current,
                self.measurement_calibration(document_id, page_index),
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
                draft: true,
                feedback: SceneInteractionFeedback::Creation,
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
                annotation.draft = true;
                annotation.feedback = SceneInteractionFeedback::Transform {
                    chrome_visible,
                    active_handle: match endpoint {
                        LengthEndpoint::Start => 0,
                        LengthEndpoint::End => 1,
                    },
                };
            }
        }
        if let Some(ActivePointer::LengthMove {
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
                .lengths
                .iter_mut()
                .find(|annotation| annotation.id == *id)
        {
            let delta_x = current.x - start.x;
            let delta_y = current.y - start.y;
            annotation.start =
                PdfPoint::new(original_start.x + delta_x, original_start.y + delta_y)
                    .expect("validated pointer points produce a finite Length preview");
            annotation.end = PdfPoint::new(original_end.x + delta_x, original_end.y + delta_y)
                .expect("validated pointer points produce a finite Length preview");
            annotation.draft = true;
            annotation.feedback = SceneInteractionFeedback::Move { chrome_visible };
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
            .is_some_and(AnnotationDocument::is_dirty)
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

    /// [`Self::mark_saved`], keeping pages as the file had them before the
    /// save for undo to reach back to.
    pub fn mark_saved_keeping(
        &mut self,
        document_id: u64,
        kept_pages: Vec<crate::pdf_page_base::PageBase>,
    ) -> Result<(), AnnotationError> {
        self.documents
            .get_mut(&document_id)
            .ok_or(AnnotationError::NoSelection)?
            .mark_saved_keeping(kept_pages)
    }

    pub fn document_page_bases(&self, document_id: u64) -> &[crate::pdf_page_base::PageBase] {
        self.documents
            .get(&document_id)
            .map_or(&[], |document| document.page_bases())
    }

    fn next_id(&mut self, tool: AnnotationTool) -> Result<MarkupId, AnnotationError> {
        if let Some(id) = self.queued_id.take() {
            if let Some(sequence) = comparison_sequence(&id) {
                self.next_sequence = self.next_sequence.max(sequence);
            }
            return Ok(id);
        }
        self.next_sequence = self.next_sequence.checked_add(1).ok_or_else(|| {
            AnnotationError::InvalidRecoveryTimeline(
                "comparison markup sequence is exhausted".into(),
            )
        })?;
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

fn active_pointer_document_id(active: &ActivePointer) -> u64 {
    match active {
        ActivePointer::Marquee { document_id, .. }
        | ActivePointer::GroupMove { document_id, .. }
        | ActivePointer::Domain { document_id, .. }
        | ActivePointer::EllipseCreate { document_id, .. }
        | ActivePointer::EllipseMove { document_id, .. }
        | ActivePointer::EllipseResize { document_id, .. }
        | ActivePointer::EllipseRotate { document_id, .. }
        | ActivePointer::RedactCreate { document_id, .. }
        | ActivePointer::RedactMove { document_id, .. }
        | ActivePointer::RedactResize { document_id, .. }
        | ActivePointer::ArcMove { document_id, .. }
        | ActivePointer::ArcControlPoint { document_id, .. }
        | ActivePointer::StraightLineCreate { document_id, .. }
        | ActivePointer::CalloutCreate { document_id, .. }
        | ActivePointer::ImageCreate { document_id, .. }
        | ActivePointer::CloudPlusCreate { document_id, .. }
        | ActivePointer::StraightLineMove { document_id, .. }
        | ActivePointer::StraightLineEndpoint { document_id, .. }
        | ActivePointer::VertexPathPoint { document_id, .. }
        | ActivePointer::MeasurementPathPoint { document_id, .. }
        | ActivePointer::InkMove { document_id, .. }
        | ActivePointer::InkResize { document_id, .. }
        | ActivePointer::TextBoxMove { document_id, .. }
        | ActivePointer::TextBoxResize { document_id, .. }
        | ActivePointer::TextBoxRotate { document_id, .. }
        | ActivePointer::ImageMove { document_id, .. }
        | ActivePointer::ImageResize { document_id, .. }
        | ActivePointer::ImageRotate { document_id, .. }
        | ActivePointer::SnapshotMove { document_id, .. }
        | ActivePointer::SnapshotResize { document_id, .. }
        | ActivePointer::SnapshotRotate { document_id, .. }
        | ActivePointer::LengthCreate { document_id, .. }
        | ActivePointer::LengthMove { document_id, .. }
        | ActivePointer::DimensionCreate { document_id, .. }
        | ActivePointer::DimensionEdit { document_id, .. }
        | ActivePointer::CalloutEdit { document_id, .. }
        | ActivePointer::CloudPlusEdit { document_id, .. }
        | ActivePointer::CloudEdit { document_id, .. }
        | ActivePointer::LengthEndpoint { document_id, .. } => *document_id,
    }
}

fn comparison_sequence(id: &MarkupId) -> Option<u64> {
    let remainder = id.as_str().strip_prefix("comparison:")?;
    let (family, sequence) = remainder.rsplit_once(':')?;
    (!family.is_empty())
        .then(|| sequence.parse::<u64>().ok())
        .flatten()
}

fn image_placement_rect(
    pending: &PendingImageAsset,
    placement_page: ImagePlacementPage,
    point: PdfPoint,
) -> Result<PdfRect, AnnotationError> {
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
    let x = (point.x - width / 2.0).clamp(0.0, (placement_page.width_pt - width).max(0.0));
    let y = (point.y - height / 2.0).clamp(0.0, (placement_page.height_pt - height).max(0.0));
    PdfRect::new(x, y, width, height)
}

/// The image's box when drawn from `start` to `end`: the largest box with
/// the image's proportions inside the drawn one, from `start`. As when it is
/// placed at its natural size, a new image is never distorted.
fn image_drawn_rect(
    pending: &PendingImageAsset,
    start: PdfPoint,
    end: PdfPoint,
) -> Result<PdfRect, AnnotationError> {
    let aspect_ratio =
        (f64::from(pending.asset.width_px()) / f64::from(pending.asset.height_px())).max(0.01);
    let width = (end.x - start.x)
        .abs()
        .min((end.y - start.y).abs() * aspect_ratio)
        .max(1.);
    let height = width / aspect_ratio;
    let x = if end.x < start.x {
        start.x - width
    } else {
        start.x
    };
    let y = if end.y < start.y {
        start.y - height
    } else {
        start.y
    };
    PdfRect::new(x, y, width, height)
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

fn moving_snap_context(
    document: &AnnotationDocument,
    page_index: u32,
    supplement: &AnnotationSelectionSupplement,
) -> (Vec<PdfPoint>, Vec<MarkupId>) {
    let scene = document.document_scene(page_index);
    let mut excluded_ids = Vec::new();
    excluded_ids.extend(
        scene
            .straight_lines
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .rectangles
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .ellipses
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .redacts
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .arcs
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked && !annotation.draft)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .vertex_paths
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked && !annotation.draft)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .measurement_paths
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked && !annotation.draft)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .dimensions
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .lengths
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .clouds
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .callouts
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .text_boxes
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .images
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked)
            .map(|annotation| annotation.id.clone()),
    );
    excluded_ids.extend(
        scene
            .snapshots
            .iter()
            .filter(|annotation| annotation.selected && !annotation.locked && !annotation.draft)
            .map(|annotation| annotation.id.clone()),
    );
    let anchors = moving_annotation_snap_anchor_points_with_selection_supplement(
        &scene,
        &excluded_ids,
        128,
        supplement,
    );
    (anchors, excluded_ids)
}

/// The smallest drag, on each axis, that draws a rectangular cloud.
const CLOUD_DRAG_MINIMUM_CSS_PX: f64 = 8.0;

/// The corners of the rectangular cloud from `start` to `end`, once it is wide
/// and tall enough to draw.
fn cloud_rectangle_corners(
    start: PdfPoint,
    end: PdfPoint,
    pixels_per_point: f64,
) -> Option<Vec<PdfPoint>> {
    let wide = (end.x - start.x).abs() * pixels_per_point >= CLOUD_DRAG_MINIMUM_CSS_PX;
    let tall = (end.y - start.y).abs() * pixels_per_point >= CLOUD_DRAG_MINIMUM_CSS_PX;
    (wide && tall).then(|| {
        vec![
            start,
            PdfPoint {
                x: end.x,
                y: start.y,
            },
            end,
            PdfPoint {
                x: start.x,
                y: end.y,
            },
        ]
    })
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

/// Text in boxes, callouts and Cloud+ uses Revu's 3 pt margin.
const REVU_TEXT_MARGIN_PT: f64 = 3.;

pub(crate) fn text_box_tool_style(
    properties: &ToolProperties,
) -> Result<TextBoxStyle, AnnotationError> {
    let style = TextBoxStyle::new(
        properties.font_family.clone(),
        properties.font_size_pt,
        properties.colour.clone(),
        properties.opacity,
    )?;
    let line_height = style.line_height_pt();
    style.with_layout_metrics(line_height, REVU_TEXT_MARGIN_PT)
}

/// Measurement and dimension captions are centred without a margin, as in Revu.
fn caption_tool_style(properties: &ToolProperties) -> Result<TextBoxStyle, AnnotationError> {
    TextBoxStyle::new(
        properties.font_family.clone(),
        properties.font_size_pt,
        properties.colour.clone(),
        properties.opacity,
    )?
    .with_weight_and_alignment(400, TextAlignment::Center)
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
    let cloud_properties = ToolProperties {
        width_pt: 1.0,
        ..properties.clone()
    };
    CloudPlusAppearance::new(
        rectangle_tool_appearance(&cloud_properties, true)?,
        straight_line_tool_appearance(&line_properties)?,
        text_box_tool_style(properties)?,
    )
}

fn dimension_tool_appearance(
    properties: &ToolProperties,
) -> Result<DimensionAppearance, AnnotationError> {
    DimensionAppearance::new(
        straight_line_tool_appearance(properties)?,
        caption_tool_style(properties)?,
    )
}

#[cfg(test)]
fn default_cloud_plus_appearance() -> Result<CloudPlusAppearance, AnnotationError> {
    let properties = ToolProperties::for_tool(AnnotationTool::CloudPlus);
    cloud_plus_tool_appearance(&properties)
}

#[cfg(test)]
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
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, LineEndpoint)> {
    let selected = document.selected_id()?;
    let annotation = document
        .straight_lines()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    if distance(annotation.end, point) <= handle_tolerance {
        Some((annotation.id.clone(), LineEndpoint::End))
    } else if distance(annotation.start, point) <= handle_tolerance {
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
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, usize)> {
    let selected = document.selected_id()?;
    let annotation = document
        .vertex_paths()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    hit_vertex_path_handle_index(annotation, point, tolerance, observed_pixels_per_point)
        .map(|index| (annotation.id.clone(), index))
}

fn hit_vertex_path_handle_index(
    annotation: &VertexPathAnnotation,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<usize> {
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    annotation
        .points()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, vertex)| distance(**vertex, point) <= handle_tolerance)
        .map(|(index, _)| index)
}

fn hit_selected_measurement_path_point(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, usize)> {
    let selected = document.selected_id()?;
    let annotation = document
        .measurement_paths()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    hit_measurement_path_handle_index(annotation, point, tolerance, observed_pixels_per_point)
        .map(|index| (annotation.id.clone(), index))
}

fn hit_measurement_path_handle_index(
    annotation: &MeasurementPathAnnotation,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<usize> {
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    annotation
        .points()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, vertex)| distance(**vertex, point) <= handle_tolerance)
        .map(|(index, _)| index)
}

fn hit_length_endpoint(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, LengthEndpoint)> {
    let selected = document.selected_id()?;
    let annotation = document
        .lengths()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    length_endpoint_at(annotation, point, handle_tolerance).map(|endpoint| {
        let endpoint = match endpoint {
            0 => LengthEndpoint::Start,
            _ => LengthEndpoint::End,
        };
        (annotation.id.clone(), endpoint)
    })
}

fn length_endpoint_at(annotation: &LengthAnnotation, point: PdfPoint, reach: f64) -> Option<u8> {
    measurement_endpoint_at(
        annotation.start,
        annotation.end,
        crate::annotation_model::LENGTH_LEADER_LENGTH_PT,
        annotation.appearance.line().stroke_width_pt(),
        point,
        reach,
    )
}

/// The measured point (0 start, 1 end) a press within `reach` grabs. Its
/// extension line, from the point out past the dimension line and arrowhead,
/// is the drawn end, so anywhere along it grabs; the end wins a tie.
fn measurement_endpoint_at(
    start: PdfPoint,
    end: PdfPoint,
    offset_pt: f64,
    stroke_width_pt: f64,
    point: PdfPoint,
    reach: f64,
) -> Option<u8> {
    let reaches = |index: usize, measured: PdfPoint| {
        crate::annotation_model::measurement_line_layout(start, end, offset_pt, stroke_width_pt, 0.)
            .map_or_else(
                || distance(measured, point),
                |layout| {
                    let (from, to) = layout.extension_lines[index];
                    point_segment_distance(point, from, to)
                },
            )
            <= reach
    };
    if reaches(1, end) {
        Some(1)
    } else if reaches(0, start) {
        Some(0)
    } else {
        None
    }
}

fn hit_selected_image_resize_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
    transform_owner: Option<&MarkupId>,
) -> Option<(MarkupId, ImageResizeHandle)> {
    if let Some(annotation) = document.selected_id().and_then(|selected| {
        document
            .images()
            .iter()
            .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)
    }) {
        let handle_tolerance = tolerance.max(
            crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                / observed_pixels_per_point.max(f64::EPSILON),
        );
        if image_rotation_handle_point(annotation, observed_pixels_per_point)
            .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
        {
            return None;
        }
        if let Some(handle) = hit_image_resize_handle(annotation, point, observed_pixels_per_point)
        {
            return Some((annotation.id.clone(), handle));
        }
    }
    document
        .images()
        .iter()
        .rev()
        .filter(|annotation| {
            annotation.page_index == page_index
                && !annotation.locked
                && transform_owner == Some(&annotation.id)
        })
        .find_map(|annotation| {
            hit_image_resize_handle(annotation, point, observed_pixels_per_point)
                .map(|handle| (annotation.id.clone(), handle))
        })
}

fn hit_selected_image_rotation_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<MarkupId> {
    let selected = document.selected_id()?;
    let annotation = document.images().iter().find(|annotation| {
        annotation.page_index == page_index && &annotation.id == selected && !annotation.locked
    })?;
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    image_rotation_handle_point(annotation, observed_pixels_per_point)
        .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
        .then(|| annotation.id.clone())
}

/// Images resize from anywhere along their edges; aspect-locked Images only
/// from their corners.
fn hit_image_resize_handle(
    annotation: &ImageAnnotation,
    point: PdfPoint,
    observed_pixels_per_point: f64,
) -> Option<ImageResizeHandle> {
    let handle = match box_outline_resize_handle(
        annotation.rect,
        annotation.rotation_degrees(),
        point,
        edge_resize_reach(observed_pixels_per_point),
    )? {
        RectangleResizeHandle::NorthWest => ImageResizeHandle::NorthWest,
        RectangleResizeHandle::North => ImageResizeHandle::North,
        RectangleResizeHandle::NorthEast => ImageResizeHandle::NorthEast,
        RectangleResizeHandle::East => ImageResizeHandle::East,
        RectangleResizeHandle::SouthEast => ImageResizeHandle::SouthEast,
        RectangleResizeHandle::South => ImageResizeHandle::South,
        RectangleResizeHandle::SouthWest => ImageResizeHandle::SouthWest,
        RectangleResizeHandle::West => ImageResizeHandle::West,
    };
    (!annotation.aspect_locked
        || matches!(
            handle,
            ImageResizeHandle::SouthWest
                | ImageResizeHandle::SouthEast
                | ImageResizeHandle::NorthEast
                | ImageResizeHandle::NorthWest
        ))
    .then_some(handle)
}

fn image_resize_handle_local_point(rect: PdfRect, handle: ImageResizeHandle) -> PdfPoint {
    let left = rect.x;
    let center_x = rect.x + rect.width / 2.0;
    let right = rect.x + rect.width;
    let bottom = rect.y;
    let center_y = rect.y + rect.height / 2.0;
    let top = rect.y + rect.height;
    match handle {
        ImageResizeHandle::SouthWest => PdfPoint { x: left, y: bottom },
        ImageResizeHandle::South => PdfPoint {
            x: center_x,
            y: bottom,
        },
        ImageResizeHandle::SouthEast => PdfPoint {
            x: right,
            y: bottom,
        },
        ImageResizeHandle::East => PdfPoint {
            x: right,
            y: center_y,
        },
        ImageResizeHandle::NorthEast => PdfPoint { x: right, y: top },
        ImageResizeHandle::North => PdfPoint {
            x: center_x,
            y: top,
        },
        ImageResizeHandle::NorthWest => PdfPoint { x: left, y: top },
        ImageResizeHandle::West => PdfPoint {
            x: left,
            y: center_y,
        },
    }
}

fn hit_selected_snapshot_resize_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
    transform_owner: Option<&MarkupId>,
) -> Option<(MarkupId, RectangleResizeHandle)> {
    let snapshot_handle = |annotation: &SnapshotAnnotation| {
        hit_snapshot_handle_index(
            annotation,
            point,
            tolerance,
            observed_pixels_per_point,
            false,
        )
        .and_then(|index| RectangleResizeHandle::ALL.get(index).copied())
    };
    if let Some((annotation, handle)) = document
        .selected_id()
        .and_then(|selected| {
            document.snapshots().iter().find(|annotation| {
                annotation.page_index == page_index && &annotation.id == selected
            })
        })
        .and_then(|annotation| snapshot_handle(annotation).map(|handle| (annotation, handle)))
    {
        return Some((annotation.id.clone(), handle));
    }
    document
        .snapshots()
        .iter()
        .rev()
        .filter(|annotation| {
            annotation.page_index == page_index
                && !annotation.locked
                && transform_owner == Some(&annotation.id)
        })
        .find_map(|annotation| {
            snapshot_handle(annotation).map(|handle| (annotation.id.clone(), handle))
        })
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
    (hit_snapshot_handle_index(
        annotation,
        point,
        tolerance,
        observed_pixels_per_point,
        true,
    ) == Some(RectangleResizeHandle::ALL.len()))
    .then(|| annotation.id.clone())
}

/// The box a Pen or Highlight stroke resizes by: the bounds of its points,
/// with its handles half the line width further out, at the visible
/// stroke's edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InkResizeBox {
    points: PdfRect,
    padding: f64,
}

impl InkResizeBox {
    /// Below this extent a stroke is flat along that axis and has no
    /// handles across it, which would do nothing.
    const FLAT_PT: f64 = 0.5;
    /// A resized stroke keeps at least this extent along a non-flat axis.
    const MIN_EXTENT_PT: f64 = 1.;

    pub(crate) fn of<'a>(
        paths: impl IntoIterator<Item = &'a [PdfPoint]>,
        width_pt: f64,
    ) -> Option<Self> {
        let mut points = paths.into_iter().flatten();
        let first = points.next()?;
        let (mut left, mut bottom, mut right, mut top) = (first.x, first.y, first.x, first.y);
        for point in points {
            left = left.min(point.x);
            bottom = bottom.min(point.y);
            right = right.max(point.x);
            top = top.max(point.y);
        }
        Some(Self {
            points: PdfRect {
                x: left,
                y: bottom,
                width: right - left,
                height: top - bottom,
            },
            padding: (width_pt * 0.5).max(1.),
        })
    }

    pub(crate) fn outer(self) -> PdfRect {
        PdfRect {
            x: self.points.x - self.padding,
            y: self.points.y - self.padding,
            width: self.points.width + self.padding * 2.,
            height: self.points.height + self.padding * 2.,
        }
    }

    /// The handles that resize the stroke, as indices into
    /// [`RectangleResizeHandle::ALL`] with their points.
    pub(crate) fn handles(self) -> Vec<(usize, PdfPoint)> {
        let flat_x = self.points.width < Self::FLAT_PT;
        let flat_y = self.points.height < Self::FLAT_PT;
        RectangleResizeHandle::ALL
            .into_iter()
            .enumerate()
            .filter(|(_, handle)| {
                let horizontal = handle.affects_east() || handle.affects_west();
                let vertical = handle.affects_north() || handle.affects_south();
                !(horizontal && flat_x) && !(vertical && flat_y)
            })
            .map(|(index, handle)| (index, handle.point(self.outer())))
            .collect()
    }

    /// `paths` scaled into the box that dragging `handle` to `point` makes.
    pub(crate) fn resized_paths<'a>(
        self,
        paths: impl IntoIterator<Item = &'a [PdfPoint]>,
        handle: RectangleResizeHandle,
        point: PdfPoint,
    ) -> Vec<Vec<PdfPoint>> {
        let outer = self.outer().rotated_resize_from_handle(0., handle, point);
        let axis = |start: f64, extent: f64, new_start: f64, new_extent: f64| {
            let new_extent = if extent < Self::FLAT_PT {
                0.
            } else {
                (new_extent - self.padding * 2.).max(Self::MIN_EXTENT_PT)
            };
            let new_start = new_start + self.padding;
            move |value: f64| {
                if extent < Self::FLAT_PT {
                    value - start + new_start + (new_extent - extent) * 0.5
                } else {
                    new_start + (value - start) * new_extent / extent
                }
            }
        };
        let map_x = axis(self.points.x, self.points.width, outer.x, outer.width);
        let map_y = axis(self.points.y, self.points.height, outer.y, outer.height);
        paths
            .into_iter()
            .map(|path| {
                path.iter()
                    .map(|sample| PdfPoint {
                        x: map_x(sample.x),
                        y: map_y(sample.y),
                    })
                    .collect()
            })
            .collect()
    }

    fn hit(self, point: PdfPoint, tolerance: f64, observed_pixels_per_point: f64) -> Option<usize> {
        let reach = tolerance.max(
            crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                / observed_pixels_per_point.max(f64::EPSILON),
        );
        self.handles()
            .into_iter()
            .filter(|(_, handle)| distance(*handle, point) <= reach)
            .min_by(|(_, left), (_, right)| {
                distance(*left, point).total_cmp(&distance(*right, point))
            })
            .map(|(index, _)| index)
    }
}

/// The selected stroke's resize handle under `point`, else that of the
/// hovered stroke that owns the pointer's transform.
fn hit_selected_ink_resize_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
    transform_owner: Option<&MarkupId>,
) -> Option<(MarkupId, usize)> {
    let selected = document.selected_id();
    document
        .pens()
        .iter()
        .rev()
        .filter(|pen| {
            pen.page_index == page_index
                && (selected == Some(&pen.id) || transform_owner == Some(&pen.id))
        })
        .find_map(|pen| {
            InkResizeBox::of(pen.paths(), pen.appearance.width_pt())?
                .hit(point, tolerance, observed_pixels_per_point)
                .map(|index| (pen.id.clone(), index))
        })
}

fn hit_snapshot_handle_index(
    annotation: &SnapshotAnnotation,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
    include_rotation: bool,
) -> Option<usize> {
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    if include_rotation
        && snapshot_rotation_handle_point(annotation, observed_pixels_per_point)
            .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
    {
        return Some(RectangleResizeHandle::ALL.len());
    }
    box_outline_resize_handle(
        annotation.rect,
        annotation.rotation_degrees(),
        point,
        edge_resize_reach(observed_pixels_per_point),
    )
    .map(resize_handle_index)
}

fn resized_image_rect(
    original: PdfRect,
    handle: ImageResizeHandle,
    start: PdfPoint,
    current: PdfPoint,
    rotation_degrees: f64,
    aspect_locked: bool,
) -> Result<PdfRect, AnnotationError> {
    if rotation_degrees.rem_euclid(360.).abs() > f64::EPSILON {
        let local_start = rotate_point_around_rect_center(start, original, rotation_degrees);
        let local_current = rotate_point_around_rect_center(current, original, rotation_degrees);
        let mut resized = resized_image_rect(
            original,
            handle,
            local_start,
            local_current,
            0.,
            aspect_locked,
        )?;
        let opposite = match handle {
            ImageResizeHandle::SouthWest => ImageResizeHandle::NorthEast,
            ImageResizeHandle::South => ImageResizeHandle::North,
            ImageResizeHandle::SouthEast => ImageResizeHandle::NorthWest,
            ImageResizeHandle::East => ImageResizeHandle::West,
            ImageResizeHandle::NorthEast => ImageResizeHandle::SouthWest,
            ImageResizeHandle::North => ImageResizeHandle::South,
            ImageResizeHandle::NorthWest => ImageResizeHandle::SouthEast,
            ImageResizeHandle::West => ImageResizeHandle::East,
        };
        let original_anchor = rotate_point_around_rect_center(
            image_resize_handle_local_point(original, opposite),
            original,
            -rotation_degrees,
        );
        let resized_anchor = rotate_point_around_rect_center(
            image_resize_handle_local_point(resized, opposite),
            resized,
            -rotation_degrees,
        );
        resized.x += original_anchor.x - resized_anchor.x;
        resized.y += original_anchor.y - resized_anchor.y;
        return Ok(resized);
    }
    const MIN_IMAGE_SIZE_PT: f64 = 24.0;
    if aspect_locked {
        let aspect_ratio = original.width / original.height;
        let right = original.x + original.width;
        let top = original.y + original.height;
        let anchor = match handle {
            ImageResizeHandle::SouthWest => PdfPoint { x: right, y: top },
            ImageResizeHandle::SouthEast => PdfPoint {
                x: original.x,
                y: top,
            },
            ImageResizeHandle::NorthEast => PdfPoint {
                x: original.x,
                y: original.y,
            },
            ImageResizeHandle::NorthWest => PdfPoint {
                x: right,
                y: original.y,
            },
            _ => {
                return Err(AnnotationError::InvalidGeometry(
                    "aspect-locked images resize only from corner handles".into(),
                ));
            }
        };
        let requested_width = if matches!(
            handle,
            ImageResizeHandle::SouthWest | ImageResizeHandle::NorthWest
        ) {
            anchor.x - current.x
        } else {
            current.x - anchor.x
        };
        let requested_height = if matches!(
            handle,
            ImageResizeHandle::SouthWest | ImageResizeHandle::SouthEast
        ) {
            anchor.y - current.y
        } else {
            current.y - anchor.y
        };
        let minimum_width = MIN_IMAGE_SIZE_PT.max(MIN_IMAGE_SIZE_PT * aspect_ratio);
        let minimum_height = minimum_width / aspect_ratio;
        let scale = (requested_width / original.width)
            .max(requested_height / original.height)
            .max(minimum_width / original.width)
            .max(minimum_height / original.height);
        let width = original.width * scale;
        let height = original.height * scale;
        let x = if matches!(
            handle,
            ImageResizeHandle::SouthWest | ImageResizeHandle::NorthWest
        ) {
            anchor.x - width
        } else {
            anchor.x
        };
        let y = if matches!(
            handle,
            ImageResizeHandle::SouthWest | ImageResizeHandle::SouthEast
        ) {
            anchor.y - height
        } else {
            anchor.y
        };
        return PdfRect::new(x, y, width, height);
    }
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

fn hit_annotation_body_in_document_order(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    supplement: &AnnotationSelectionSupplement,
    observed_pixels_per_point: f64,
) -> Option<MarkupId> {
    document
        .annotation_order()
        .iter()
        .rev()
        .find(|id| {
            annotation_body_contains(
                document,
                id,
                page_index,
                point,
                tolerance,
                observed_pixels_per_point,
            ) || annotation_caption_contains(document, id, page_index, point, supplement)
        })
        .cloned()
        .or_else(|| {
            // Just inside a selected item's outline, or on the band around
            // its outset selection outline, the press still belongs to it.
            let points_per_css_px = 1. / observed_pixels_per_point.max(f64::EPSILON);
            document.selected_outline_zone_hit(
                page_index,
                point,
                tolerance + crate::annotation_model::SELECTION_OUTSET_CSS_PX * points_per_css_px,
                crate::annotation_model::SELECTION_INNER_BAND_CSS_PX * points_per_css_px,
                supplement,
            )
        })
}

fn annotation_caption_contains(
    document: &AnnotationDocument,
    id: &MarkupId,
    page_index: u32,
    point: PdfPoint,
    supplement: &AnnotationSelectionSupplement,
) -> bool {
    let belongs_to_page = document
        .lengths()
        .iter()
        .any(|annotation| &annotation.id == id && annotation.page_index == page_index)
        || document
            .measurement_paths()
            .iter()
            .any(|annotation| &annotation.id == id && annotation.page_index == page_index)
        || document
            .dimensions()
            .iter()
            .any(|annotation| &annotation.id == id && annotation.page_index == page_index);
    belongs_to_page
        && supplement
            .get(id)
            .is_some_and(|polygon| point_in_polygon(point, polygon))
}

fn annotation_body_contains(
    document: &AnnotationDocument,
    id: &MarkupId,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> bool {
    if let Some(annotation) = document
        .rectangles()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        let local_point =
            rotate_point_around_rect_center(point, annotation.rect, annotation.rotation_degrees);
        // An unfilled rectangle is its outline; its inside is empty.
        let band = tolerance.max(annotation.appearance.stroke_width_pt() / 2.);
        let rect = annotation.rect;
        let deep = local_point.x > rect.x + band
            && local_point.x < rect.x + rect.width - band
            && local_point.y > rect.y + band
            && local_point.y < rect.y + rect.height - band;
        return annotation.page_index == page_index
            && (annotation.move_zone_contains(
                point,
                crate::annotation_model::RECTANGLE_EDGE_GRAB_CSS_PX
                    / observed_pixels_per_point.max(f64::EPSILON),
            ) || (rect_contains(rect, local_point, band)
                && (!deep || annotation.appearance.fill_color().is_some())));
    }
    if let Some(annotation) = document
        .straight_lines()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index
            && point_segment_distance(point, annotation.start, annotation.end)
                <= tolerance.max(annotation.appearance.stroke_width_pt() / 2.);
    }
    if let Some(annotation) = document
        .redacts()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index
            && point_in_rect(point, annotation.rect, tolerance);
    }
    if let Some(annotation) = document
        .arcs()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index && arc_hit(annotation, point, tolerance);
    }
    if let Some(annotation) = document
        .ellipses()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index
            && (ellipse_hit(annotation, point, tolerance)
                || ellipse_move_zone_contains(
                    annotation.rect,
                    annotation.rotation_degrees,
                    point,
                    edge_resize_reach(observed_pixels_per_point),
                ));
    }
    if let Some(annotation) = document
        .vertex_paths()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index
            && vertex_path_hit(annotation, point, tolerance);
    }
    if let Some(annotation) = document
        .measurement_paths()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index
            && measurement_path_hit(annotation, point, tolerance);
    }
    if let Some(annotation) = document
        .clouds()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index && cloud_hit(annotation, point, tolerance);
    }
    if let Some(annotation) = document
        .cloud_pluses()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index && cloud_plus_hit(annotation, point, tolerance);
    }
    if let Some(annotation) = document
        .images()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        let local_point =
            rotate_point_around_rect_center(point, annotation.rect, annotation.rotation_degrees());
        return annotation.page_index == page_index
            && rect_contains(annotation.rect, local_point, tolerance);
    }
    if let Some(annotation) = document
        .snapshots()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        let local_point =
            rotate_point_around_rect_center(point, annotation.rect, annotation.rotation_degrees());
        return annotation.page_index == page_index
            && rect_contains(annotation.rect, local_point, tolerance);
    }
    if let Some(annotation) = document
        .text_boxes()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        let local_point = rotate_point_around_rect_center(
            point,
            annotation.layout_rect,
            annotation.rotation_degrees(),
        );
        return annotation.page_index == page_index
            && rect_contains(annotation.layout_rect, local_point, tolerance);
    }
    if let Some(annotation) = document
        .dimensions()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        let (start, end) = annotation.dimension_line_points();
        return annotation.page_index == page_index
            && point_segment_distance(point, start, end)
                <= tolerance.max(annotation.appearance.line().stroke_width_pt() / 2.);
    }
    if let Some(annotation) = document
        .callouts()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index
            && (rect_contains(annotation.text_box, point, tolerance)
                || annotation.leader_points().windows(2).any(|segment| {
                    point_segment_distance(point, segment[0], segment[1]) <= tolerance
                }));
    }
    if let Some(annotation) = document
        .lengths()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        let Some(layout) = crate::annotation_model::measurement_line_layout(
            annotation.start,
            annotation.end,
            crate::annotation_model::LENGTH_LEADER_LENGTH_PT,
            annotation.appearance.line().stroke_width_pt(),
            0.,
        ) else {
            return false;
        };
        return annotation.page_index == page_index
            && layout
                .extension_lines
                .iter()
                .chain(layout.dimension_segments.iter())
                .any(|(from, to)| point_segment_distance(point, *from, *to) <= tolerance);
    }
    if let Some(annotation) = document
        .pens()
        .iter()
        .find(|annotation| &annotation.id == id)
    {
        return annotation.page_index == page_index
            && annotation.paths().any(|path| {
                path.windows(2).any(|segment| {
                    point_segment_distance(point, segment[0], segment[1])
                        <= tolerance.max(annotation.appearance.width_pt() / 2.)
                })
            });
    }
    false
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
    if let Some(index) =
        hit_dimension_handle(annotation, point, tolerance, observed_pixels_per_point)
    {
        let kind = match index {
            0 => DimensionPointerEditKind::Start,
            1 => DimensionPointerEditKind::End,
            2 => DimensionPointerEditKind::Offset,
            _ => unreachable!("Dimension handles have three stable indices"),
        };
        return Some((annotation.id.clone(), kind));
    }
    let (offset_start, offset_end) = annotation.dimension_line_points();
    (point_segment_distance(point, offset_start, offset_end) <= tolerance
        || point_segment_distance(point, annotation.start, annotation.end) <= tolerance)
        .then(|| (annotation.id.clone(), DimensionPointerEditKind::Body))
}

fn hit_dimension_handle(
    annotation: &DimensionAnnotation,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<usize> {
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    measurement_endpoint_at(
        annotation.start,
        annotation.end,
        annotation.dimension_line_offset(),
        annotation.appearance.line().stroke_width_pt(),
        point,
        handle_tolerance,
    )
    .map(usize::from)
    .or_else(|| (distance(annotation.caption_center(), point) <= handle_tolerance).then_some(2))
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
    if let Some(index) = hit_callout_handle(annotation, point, tolerance, observed_pixels_per_point)
    {
        let kind = if index < RectangleResizeHandle::ALL.len() {
            CalloutPointerEditKind::TextBoxResize(RectangleResizeHandle::ALL[index])
        } else {
            CalloutPointerEditKind::LeaderPoint(index - RectangleResizeHandle::ALL.len())
        };
        return Some((annotation.id.clone(), kind));
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

fn hit_selected_cloud_plus_control(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<(MarkupId, CloudPlusPointerEditKind)> {
    let selected = document.selected_id()?;
    let annotation = document
        .cloud_pluses()
        .iter()
        .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)?;
    if let Some(index) =
        hit_cloud_plus_handle(annotation, point, tolerance, observed_pixels_per_point)
    {
        let cloud_count = annotation.cloud_points().len();
        let resize_end = cloud_count + RectangleResizeHandle::ALL.len();
        let kind = if index < cloud_count {
            CloudPlusPointerEditKind::CloudVertex(index)
        } else if index < resize_end {
            CloudPlusPointerEditKind::TextBoxResize(RectangleResizeHandle::ALL[index - cloud_count])
        } else {
            CloudPlusPointerEditKind::LeaderPoint(index - resize_end)
        };
        return Some((annotation.id.clone(), kind));
    }
    if rect_contains(annotation.text_box, point, tolerance) {
        return Some((annotation.id.clone(), CloudPlusPointerEditKind::TextBox));
    }
    cloud_plus_cloud_hit(annotation, point, tolerance)
        .then(|| (annotation.id.clone(), CloudPlusPointerEditKind::Body))
}

fn hit_cloud_plus_handle(
    annotation: &CloudPlusAnnotation,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<usize> {
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    let cloud_count = annotation.cloud_points().len();
    let resize_end = cloud_count + RectangleResizeHandle::ALL.len();
    if let Some((index, _)) = annotation
        .leader_points()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, candidate)| distance(**candidate, point) <= handle_tolerance)
    {
        return Some(resize_end + index);
    }
    if let Some(handle) = RectangleResizeHandle::ALL.into_iter().rev().find(|handle| {
        distance(
            axis_aligned_resize_handle_point(annotation.text_box, *handle),
            point,
        ) <= handle_tolerance
    }) {
        return RectangleResizeHandle::ALL
            .iter()
            .position(|candidate| *candidate == handle)
            .map(|index| cloud_count + index);
    }
    annotation
        .cloud_points()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, candidate)| distance(**candidate, point) <= handle_tolerance)
        .map(|(index, _)| index)
}

fn hit_callout_handle(
    annotation: &CalloutAnnotation,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<usize> {
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    if let Some((index, _)) = annotation
        .leader_points()
        .iter()
        .enumerate()
        .rev()
        .find(|(_, candidate)| distance(**candidate, point) <= handle_tolerance)
    {
        return Some(RectangleResizeHandle::ALL.len() + index);
    }
    if let Some(handle) = RectangleResizeHandle::ALL.into_iter().rev().find(|handle| {
        distance(
            axis_aligned_resize_handle_point(annotation.text_box, *handle),
            point,
        ) <= handle_tolerance
    }) {
        return RectangleResizeHandle::ALL
            .iter()
            .position(|candidate| *candidate == handle);
    }
    None
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
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
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
        original.constrained_midpoint_for_shape(point, minimum_bulge_pt, true)
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

fn validate_cloud_plus_pointer_target(
    document: &AnnotationDocument,
    page_index: u32,
    id: &MarkupId,
    expected_revision: u64,
    original: &CloudPlusAnnotation,
) -> Result<(), AnnotationError> {
    validate_pointer_identity(document, id, expected_revision)?;
    let retained = document
        .cloud_pluses()
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

fn routing_points_bounds(points: &[PdfPoint]) -> Option<PdfRect> {
    let first = *points.first()?;
    let (min_x, min_y, max_x, max_y) = points.iter().skip(1).fold(
        (first.x, first.y, first.x, first.y),
        |(min_x, min_y, max_x, max_y), point| {
            (
                min_x.min(point.x),
                min_y.min(point.y),
                max_x.max(point.x),
                max_y.max(point.y),
            )
        },
    );
    PdfRect::new(min_x, min_y, max_x - min_x, max_y - min_y).ok()
}

fn resolve_cloud_plus_pointer_edit(
    original: &CloudPlusAnnotation,
    kind: CloudPlusPointerEditKind,
    start: PdfPoint,
    current: PdfPoint,
    routing_context: &CloudPlusRoutingContext,
) -> Result<CloudPlusAnnotation, AnnotationError> {
    let mut cloud_points = original.cloud_points().to_vec();
    let mut text_box = original.text_box;
    let mut leader_points = original.leader_points().to_vec();
    match kind {
        CloudPlusPointerEditKind::CloudVertex(index) => {
            let point = cloud_points.get_mut(index).ok_or_else(|| {
                AnnotationError::InvalidGeometry("Cloud+ point index is out of range".into())
            })?;
            *point = current;
            let visible_path =
                cloud_visible_path(&cloud_points, original.border_effect_intensity())?;
            leader_points = route_cloud_plus_leader(
                &cloud_points,
                &visible_path,
                text_box,
                &leader_points,
                routing_context,
            )?
            .points;
        }
        CloudPlusPointerEditKind::TextBoxResize(handle) => {
            text_box = original
                .text_box
                .rotated_resize_from_handle(0., handle, current);
            leader_points = route_cloud_plus_leader(
                &cloud_points,
                &original.scallop_path(),
                text_box,
                &leader_points,
                routing_context,
            )?
            .points;
        }
        CloudPlusPointerEditKind::LeaderPoint(index) => {
            if index >= leader_points.len() {
                return Err(AnnotationError::InvalidGeometry(
                    "Cloud+ leader point index is out of range".into(),
                ));
            }
            if index == leader_points.len() - 1 {
                leader_points[index] = current;
                leader_points = route_cloud_plus_leader(
                    &cloud_points,
                    &original.scallop_path(),
                    text_box,
                    &leader_points,
                    routing_context,
                )?
                .points;
            } else if index == 0 {
                leader_points[index] =
                    snap_cloud_plus_leader_tip(&original.scallop_path(), current)?;
            } else {
                leader_points[index] = current;
            }
        }
        CloudPlusPointerEditKind::TextBox => {
            text_box.x += current.x - start.x;
            text_box.y += current.y - start.y;
            leader_points = route_cloud_plus_leader(
                &cloud_points,
                &original.scallop_path(),
                text_box,
                &leader_points,
                routing_context,
            )?
            .points;
        }
        CloudPlusPointerEditKind::Body => {
            let delta_x = current.x - start.x;
            let delta_y = current.y - start.y;
            for point in &mut cloud_points {
                point.x += delta_x;
                point.y += delta_y;
            }
            for point in &mut leader_points {
                point.x += delta_x;
                point.y += delta_y;
            }
            text_box.x += delta_x;
            text_box.y += delta_y;
        }
    }
    let cloud_appearance_path = match kind {
        CloudPlusPointerEditKind::CloudVertex(_) => None,
        CloudPlusPointerEditKind::Body => {
            original.translated_cloud_appearance_path(current.x - start.x, current.y - start.y)?
        }
        _ => original.cloud_appearance_path().map(|path| path.to_vec()),
    };
    let mut resolved = CloudPlusAnnotation::new(
        original.id.clone(),
        original.page_index,
        cloud_points,
        original.border_effect_intensity(),
        leader_points,
        text_box,
        original.content(),
        original.appearance.clone(),
    )?
    .with_cloud_appearance_path(cloud_appearance_path)?;
    resolved.locked = original.locked;
    Ok(resolved)
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
    _tolerance: f64,
    observed_pixels_per_point: f64,
    transform_owner: Option<&MarkupId>,
) -> Option<(MarkupId, RectangleResizeHandle)> {
    let reach = edge_resize_reach(observed_pixels_per_point);
    if let Some((annotation, handle)) = document
        .selected_id()
        .and_then(|selected| {
            document.redacts().iter().find(|annotation| {
                annotation.page_index == page_index && &annotation.id == selected
            })
        })
        .and_then(|annotation| {
            box_outline_resize_handle(annotation.rect, 0., point, reach)
                .map(|handle| (annotation, handle))
        })
    {
        return Some((annotation.id.clone(), handle));
    }
    document
        .redacts()
        .iter()
        .rev()
        .filter(|annotation| {
            annotation.page_index == page_index
                && !annotation.locked
                && transform_owner == Some(&annotation.id)
        })
        .find_map(|annotation| {
            box_outline_resize_handle(annotation.rect, 0., point, reach)
                .map(|handle| (annotation.id.clone(), handle))
        })
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
    hit_arc_handle_index(annotation, point, tolerance, observed_pixels_per_point).map(|index| {
        (
            annotation.id.clone(),
            [
                ArcControlPoint::Start,
                ArcControlPoint::Mid,
                ArcControlPoint::End,
            ][index],
        )
    })
}

fn hit_arc_handle_index(
    annotation: &ArcAnnotation,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<usize> {
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    [annotation.start, annotation.mid, annotation.end]
        .into_iter()
        .enumerate()
        .rev()
        .find(|(_, control_point)| distance(*control_point, point) <= handle_tolerance)
        .map(|(index, _)| index)
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
        || (annotation.appearance.fill_color().is_some() && point_in_polygon(point, &points))
}

fn cloud_plus_cloud_hit(annotation: &CloudPlusAnnotation, point: PdfPoint, tolerance: f64) -> bool {
    let edge_tolerance = tolerance.max(annotation.appearance.cloud().stroke_width_pt() / 2.0);
    let control_path = annotation.cloud_points();
    control_path
        .windows(2)
        .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= edge_tolerance)
        || control_path
            .first()
            .zip(control_path.last())
            .is_some_and(|(first, last)| {
                point_segment_distance(point, *last, *first) <= edge_tolerance
            })
        || annotation
            .scallop_path()
            .windows(2)
            .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= edge_tolerance)
        || (annotation.appearance.cloud().fill_color().is_some()
            && point_in_polygon(point, &annotation.scallop_path()))
}

fn cloud_plus_hit(annotation: &CloudPlusAnnotation, point: PdfPoint, tolerance: f64) -> bool {
    rect_contains(annotation.text_box, point, 0.)
        || annotation
            .leader_points()
            .windows(2)
            .any(|segment| point_segment_distance(point, segment[0], segment[1]) <= tolerance)
        || cloud_plus_cloud_hit(annotation, point, tolerance)
}

/// The width of a Callout or Cloud+ caption box that holds `content`, whose
/// lines never wrap, at `style`.
pub(crate) fn caption_box_width(content: &str, style: &TextBoxStyle) -> f64 {
    let text_width = crate::pdf_engine::appearance_text_block_width_pt(
        content,
        style.font_size_pt(),
        style.font_family(),
    )
    .unwrap_or_else(|| {
        content
            .split('\n')
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f64
            * style.font_size_pt()
            * 0.6
    });
    // A little slack keeps the last glyph clear of the border when the
    // canvas font measures slightly wider than the saved appearance.
    (text_width + style.inset_pt() * 2. + 2.).max(CAPTION_MIN_WIDTH_PT)
}

/// The height of a caption box that holds `content` without clipping it.
fn caption_box_height(content: &str, style: &TextBoxStyle) -> f64 {
    let line_count = content.split('\n').count().max(1) as f64;
    (line_count * style.font_size_pt() * 1.15 + 12.).max(CAPTION_HEIGHT_PT)
}

/// A Callout's text box and leader as drawn from `start`, its arrow tip, to
/// `end`. The box sits beyond `end` on the side away from the tip, centred
/// on it, so the leader never crosses its own text.
fn callout_initial_layout(
    start: PdfPoint,
    end: PdfPoint,
    style: &TextBoxStyle,
) -> Result<(PdfRect, Vec<PdfPoint>), AnnotationError> {
    let width = caption_box_width(CALLOUT_DEFAULT_TEXT, style);
    let height = caption_box_height(CALLOUT_DEFAULT_TEXT, style);
    let leftwards = end.x < start.x;
    let text_box = PdfRect::new(
        if leftwards { end.x - width } else { end.x },
        end.y - height * 0.5,
        width,
        height,
    )?;
    let connection = PdfPoint::new(end.x, end.y)?;
    let knee = PdfPoint::new((start.x + connection.x) * 0.5, connection.y)?;
    Ok((text_box, vec![start, knee, connection]))
}

/// A Callout's text box refitted to `content`. The side its leader meets
/// stays put and the box keeps its vertical centre.
pub(crate) fn callout_fitted_text_box(annotation: &CalloutAnnotation, content: &str) -> PdfRect {
    let style = annotation.appearance.text();
    let current = annotation.text_box;
    let width = caption_box_width(content, style);
    let height = caption_box_height(content, style);
    let right = current.x + current.width;
    let leader_meets_right = annotation
        .leader_points()
        .last()
        .is_some_and(|connection| (connection.x - right).abs() < (connection.x - current.x).abs());
    PdfRect {
        x: if leader_meets_right {
            right - width
        } else {
            current.x
        },
        y: current.y + (current.height - height) * 0.5,
        width,
        height,
    }
}

/// A Cloud+ caption box resized to fit `content`. The side its leader meets
/// stays put, so the text never drifts away from the leader.
pub(crate) fn cloud_plus_fitted_text_box(
    annotation: &CloudPlusAnnotation,
    content: &str,
) -> PdfRect {
    let current = annotation.text_box;
    let width = caption_box_width(content, annotation.appearance.text());
    let right = current.x + current.width;
    let leader_meets_right = annotation
        .leader_points()
        .last()
        .is_some_and(|connection| (connection.x - right).abs() < (connection.x - current.x).abs());
    PdfRect {
        x: if leader_meets_right {
            right - width
        } else {
            current.x
        },
        width,
        ..current
    }
}

fn cloud_plus_text_layout(
    annotation: &CloudPlusAnnotation,
    content: &str,
    routing_context: &CloudPlusRoutingContext,
) -> Result<(PdfRect, Vec<PdfPoint>), AnnotationError> {
    let normalized_content = content.replace("\r\n", "\n").replace('\r', "\n");
    let line_count = normalized_content.split('\n').count().max(1) as f64;
    let line_height = annotation.appearance.text().font_size_pt() * 1.15;
    let height = annotation
        .text_box
        .height
        .max(line_count * line_height + 12.);
    let fitted = cloud_plus_fitted_text_box(annotation, &normalized_content);
    let connection = annotation.leader_points().last().copied();
    let existing_center_y = annotation.text_box.y + annotation.text_box.height * 0.5;
    let connects_to_vertical_side = connection.is_some_and(|connection| {
        (connection.x - annotation.text_box.x)
            .abs()
            .min((connection.x - (annotation.text_box.x + annotation.text_box.width)).abs())
            <= (connection.y - annotation.text_box.y)
                .abs()
                .min((connection.y - (annotation.text_box.y + annotation.text_box.height)).abs())
    });
    let center_y = if connects_to_vertical_side {
        connection
            .expect("a vertical-side connection was checked above")
            .y
    } else {
        existing_center_y
    };
    let text_box = PdfRect::new(fitted.x, center_y - height * 0.5, fitted.width, height)?;
    let leader = route_cloud_plus_leader(
        annotation.cloud_points(),
        &annotation.scallop_path(),
        text_box,
        annotation.leader_points(),
        routing_context,
    )?;
    Ok((text_box, leader.points))
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
    transform_owner: Option<&MarkupId>,
) -> Option<(MarkupId, EllipseHandleKind)> {
    let reach = edge_resize_reach(observed_pixels_per_point);
    if let Some(annotation) = document.selected_id().and_then(|selected| {
        document
            .ellipses()
            .iter()
            .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)
    }) {
        let handle_tolerance = tolerance.max(
            crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                / observed_pixels_per_point.max(f64::EPSILON),
        );
        if ellipse_rotation_handle_point(annotation, observed_pixels_per_point)
            .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
        {
            return Some((annotation.id.clone(), EllipseHandleKind::Rotate));
        }
        if let Some(handle) = ellipse_outline_resize_handle(
            annotation.rect,
            annotation.rotation_degrees,
            point,
            reach,
        ) {
            return Some((annotation.id.clone(), EllipseHandleKind::Resize(handle)));
        }
    }
    document
        .ellipses()
        .iter()
        .rev()
        .filter(|annotation| {
            annotation.page_index == page_index
                && !annotation.locked
                && transform_owner == Some(&annotation.id)
        })
        .find_map(|annotation| {
            ellipse_outline_resize_handle(
                annotation.rect,
                annotation.rotation_degrees,
                point,
                reach,
            )
            .map(|handle| (annotation.id.clone(), EllipseHandleKind::Resize(handle)))
        })
}

fn hit_selected_text_box_resize_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
    transform_owner: Option<&MarkupId>,
) -> Option<(MarkupId, RectangleResizeHandle)> {
    if let Some(annotation) = document.selected_id().and_then(|selected| {
        document
            .text_boxes()
            .iter()
            .find(|annotation| annotation.page_index == page_index && &annotation.id == selected)
    }) {
        let handle_tolerance = tolerance.max(
            crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
                / observed_pixels_per_point.max(f64::EPSILON),
        );
        if text_box_rotation_handle_point(annotation, observed_pixels_per_point)
            .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
        {
            return None;
        }
        if let Some(handle) =
            hit_text_box_resize_handle(annotation, point, tolerance, observed_pixels_per_point)
        {
            return Some((annotation.id.clone(), handle));
        }
    }
    document
        .text_boxes()
        .iter()
        .rev()
        .filter(|annotation| {
            annotation.page_index == page_index
                && !annotation.locked
                && transform_owner == Some(&annotation.id)
        })
        .find_map(|annotation| {
            hit_text_box_resize_handle(annotation, point, tolerance, observed_pixels_per_point)
                .map(|handle| (annotation.id.clone(), handle))
        })
}

fn hit_selected_text_box_rotation_handle(
    document: &AnnotationDocument,
    page_index: u32,
    point: PdfPoint,
    tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<MarkupId> {
    let selected = document.selected_id()?;
    let annotation = document.text_boxes().iter().find(|annotation| {
        annotation.page_index == page_index && &annotation.id == selected && !annotation.locked
    })?;
    let handle_tolerance = tolerance.max(
        crate::annotation_model::RESIZE_HANDLE_GRAB_CSS_PX
            / observed_pixels_per_point.max(f64::EPSILON),
    );
    text_box_rotation_handle_point(annotation, observed_pixels_per_point)
        .is_ok_and(|handle| distance(handle, point) <= handle_tolerance)
        .then(|| annotation.id.clone())
}

/// Text Boxes resize from anywhere along their edges, like Rectangles.
fn hit_text_box_resize_handle(
    annotation: &TextBoxAnnotation,
    point: PdfPoint,
    _tolerance: f64,
    observed_pixels_per_point: f64,
) -> Option<RectangleResizeHandle> {
    box_outline_resize_handle(
        annotation.layout_rect,
        annotation.rotation_degrees(),
        point,
        edge_resize_reach(observed_pixels_per_point),
    )
}

/// The resize band's reach either side of a box's outline, in page points.
fn edge_resize_reach(observed_pixels_per_point: f64) -> f64 {
    crate::annotation_model::RECTANGLE_EDGE_GRAB_CSS_PX
        / observed_pixels_per_point.max(f64::EPSILON)
}

fn resize_handle_index(handle: RectangleResizeHandle) -> usize {
    RectangleResizeHandle::ALL
        .iter()
        .position(|candidate| *candidate == handle)
        .expect("every resize handle is in the stable feedback order")
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

fn offset_point(point: PdfPoint, (dx, dy): (f64, f64)) -> PdfPoint {
    PdfPoint {
        x: point.x + dx,
        y: point.y + dy,
    }
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

fn marquee_in_pdf(marquee: &SelectionMarquee, pdf_points: &[PdfPoint]) -> SelectionMarquee {
    let mut projected = marquee.clone();
    // Keep activation and latched direction from viewport coordinates; only the
    // geometry is projected to PDF space for the shared hit query.
    if let Some(first) = pdf_points.first().copied() {
        projected.start = selection_point_from_pdf(first);
    }
    if let Some(last) = pdf_points.last().copied() {
        projected.current = selection_point_from_pdf(last);
    }
    projected.points = pdf_points
        .iter()
        .copied()
        .map(selection_point_from_pdf)
        .collect();
    projected
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
        annotation_order: Vec::new(),
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
    use crate::semantic_snapping::SemanticSnapRole;
    use crate::semantic_snapping::SemanticSnapSource;

    fn point(x: f64, y: f64) -> PdfPoint {
        PdfPoint::new(x, y).unwrap()
    }

    fn assert_snap_evidence_references(
        adapter: &AnnotationAdapter,
        owner_id: &MarkupId,
        direct_role: SemanticSnapRole,
    ) {
        if let Some(decision) = adapter.semantic_snap_decision() {
            assert_eq!(decision.owner_id.as_ref(), Some(owner_id));
            assert_eq!(decision.role, direct_role);
            return;
        }
        assert!(
            adapter
                .relationship_snap_guides()
                .iter()
                .any(|guide| match guide {
                    RelationshipSnapGuide::EqualSize { reference, .. } => {
                        &reference.owner_id == owner_id
                    }
                    RelationshipSnapGuide::EqualSpacing { before, after, .. } => {
                        &before.owner_id == owner_id || &after.owner_id == owner_id
                    }
                })
        );
    }

    #[test]
    fn selection_chrome_stays_visible_with_installed_pdf_content_and_snapping() {
        let mut adapter = AnnotationAdapter::default();
        let settings = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        adapter.set_semantic_snap_settings(settings).unwrap();
        assert!(adapter.manipulation_chrome_visible(7, 0));
        adapter
            .set_semantic_snap_page_content(
                7,
                PageSnapGeometry {
                    page_index: 0,
                    primitives: vec![crate::pdf_content_geometry::PdfContentPrimitive::Line {
                        start: crate::pdf_content_geometry::PdfPoint { x: 0., y: 0. },
                        end: crate::pdf_content_geometry::PdfPoint { x: 10., y: 0. },
                    }],
                },
            )
            .unwrap();
        assert!(adapter.manipulation_chrome_visible(7, 0));
        adapter
            .set_semantic_snap_settings(settings.with_source(SemanticSnapSource::Content, false))
            .unwrap();
        assert!(adapter.manipulation_chrome_visible(7, 0));
        adapter.set_semantic_snap_settings(settings).unwrap();
        adapter.clear_semantic_snap_page_content_page(7, 0);
        assert!(adapter.manipulation_chrome_visible(7, 0));
    }

    #[test]
    fn content_moves_snap_to_markups_and_other_content_but_not_themselves() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_observed_pixels_per_point(1.).unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "target", 200.,
            )))
            .unwrap();
        // The moving content's own edge, and another line of content.
        adapter
            .set_semantic_snap_page_content(
                7,
                PageSnapGeometry {
                    page_index: 0,
                    primitives: vec![
                        crate::pdf_content_geometry::PdfContentPrimitive::Line {
                            start: crate::pdf_content_geometry::PdfPoint { x: 0., y: 0. },
                            end: crate::pdf_content_geometry::PdfPoint { x: 10., y: 0. },
                        },
                        crate::pdf_content_geometry::PdfContentPrimitive::Line {
                            start: crate::pdf_content_geometry::PdfPoint { x: 500., y: 400. },
                            end: crate::pdf_content_geometry::PdfPoint { x: 600., y: 400. },
                        },
                    ],
                },
            )
            .unwrap();
        let bounds = PdfRect::new(0., 0., 10., 10.).unwrap();
        let anchors = [PdfPoint { x: 0., y: 0. }, PdfPoint { x: 10., y: 10. }];
        let start = PdfPoint { x: 5., y: 5. };
        let resolve = |adapter: &mut AnnotationAdapter, point: PdfPoint| {
            adapter.resolve_content_move(
                7,
                0,
                start,
                point,
                &anchors,
                bounds,
                &[[-1., -1., 11., 11.]],
                &[],
                &[],
            )
        };
        // A small move does not stick to the content's own edge.
        let point = resolve(&mut adapter, PdfPoint { x: 7., y: 5. });
        assert_eq!((point.x, point.y), (7., 5.));
        // Near the markup's corner (200, 20), the content's corner snaps
        // to it, with its guide shown.
        let point = resolve(&mut adapter, PdfPoint { x: 203., y: 23. });
        assert_eq!((point.x, point.y), (205., 25.));
        assert!(adapter.semantic_snap_decision().is_some());
        // And to other page content.
        let point = resolve(&mut adapter, PdfPoint { x: 552., y: 403. });
        assert_eq!(point.y, 405.);
        adapter.clear_content_move_snap();
        assert!(adapter.semantic_snap_decision().is_none());
    }

    fn recovery_rectangle(id: &str, x: f64) -> Annotation {
        Annotation::Rectangle(RectangleAnnotation {
            id: MarkupId::new(id).unwrap(),
            page_index: 0,
            rect: PdfRect::new(x, 20., 40., 30.).unwrap(),
            rotation_degrees: 0.,
            appearance: RectangleAppearance::new("#ff0000", 1., Some("#ffffff"), 1.).unwrap(),
            locked: false,
        })
    }

    #[test]
    fn recovery_bridge_preserves_dirty_history_and_other_documents() {
        let mut adapter = AnnotationAdapter::default();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "recovery:first",
                20.,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::MarkSaved)
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "recovery:second",
                80.,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "recovery:future",
                140.,
            )))
            .unwrap();
        document.apply_command(AnnotationCommand::Undo).unwrap();
        adapter
            .documents
            .entry(9)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "other:survives",
                220.,
            )))
            .unwrap();

        let expected = adapter.snapshot(7).unwrap();
        let other = adapter.snapshot(9).unwrap();
        assert!(expected.dirty);
        assert!(expected.undo_depth > 0);
        assert_eq!(expected.redo_depth, 1);
        let bytes = adapter.encode_document_recovery_timeline(7).unwrap();

        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "recovery:discarded",
                300.,
            )))
            .unwrap();
        adapter
            .restore_document_recovery_timeline(7, &bytes)
            .unwrap();

        assert_eq!(adapter.snapshot(7).unwrap(), expected);
        assert_eq!(adapter.snapshot(9).unwrap(), other);
        adapter.undo(7).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().rectangles.len(), 1);
        adapter.redo(7).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), expected);
    }

    #[test]
    fn failed_recovery_decode_leaves_original_document_untouched() {
        let mut adapter = AnnotationAdapter::default();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "recovery:original",
                20.,
            )))
            .unwrap();
        let before = adapter.snapshot(7).unwrap();

        assert!(
            adapter
                .restore_document_recovery_timeline(7, br#"{"schema_version":999}"#)
                .is_err()
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
    }

    #[test]
    fn successful_recovery_clears_target_transient_interaction_state() {
        let mut adapter = AnnotationAdapter::default();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "recovery:stable",
                20.,
            )))
            .unwrap();
        let bytes = adapter.encode_document_recovery_timeline(7).unwrap();

        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        adapter.pointer_down(7, 0, 71, point(10., 10.), 4.).unwrap();
        adapter.vertex_path_draft = Some(VertexPathDraft {
            document_id: 7,
            page_index: 0,
            id: MarkupId::new("draft:vertex").unwrap(),
            kind: VertexPathKind::Polyline,
            points: vec![point(10., 10.)],
            hover: point(20., 20.),
        });
        adapter.snapshot_draft = Some(SnapshotDraft {
            document_id: 7,
            page_index: 0,
            pointer_id: 72,
            id: MarkupId::new("draft:snapshot").unwrap(),
            start: point(10., 10.),
            current: point(30., 30.),
        });
        let asset = DecodedRgbaAsset::new(1, 1, vec![1, 2, 3, 255]).unwrap();
        adapter.set_image_asset(asset.clone());
        adapter.set_snapshot_capture_asset(asset);
        adapter.set_image_placement_page(600., 800., 0.45).unwrap();
        adapter.queue_next_annotation_id(MarkupId::new("comparison:rectangle:8").unwrap());
        adapter.queue_next_rectangle_appearance(
            RectangleAppearance::new("#000000", 2., Some("#ffffff"), 0.5).unwrap(),
        );
        adapter.queue_next_text_content("discard me");
        adapter.semantic_snap_decision = Some(SemanticSnapDecision {
            point: point(12., 14.),
            owner_id: None,
            role: crate::semantic_snapping::SemanticSnapRole::Endpoint,
            source: SemanticSnapSource::Annotation,
            point_candidate: true,
            distance_window_px: 1.,
        });

        adapter
            .restore_document_recovery_timeline(7, &bytes)
            .unwrap();

        assert!(adapter.active.is_none());
        assert!(adapter.vertex_path_draft.is_none());
        assert!(adapter.snapshot_draft.is_none());
        assert!(adapter.snapshot_capture_asset.is_none());
        assert_eq!(
            adapter
                .pending_image_preview_at(9, 0, point(40., 40.))
                .unwrap(),
            None
        );
        assert!(adapter.queued_id.is_none());
        assert!(adapter.queued_rectangle_appearance.is_none());
        assert!(adapter.queued_text_content.is_none());
        assert!(adapter.semantic_snap_decision.is_none());
        assert_eq!(adapter.snapshot(7).unwrap().rectangles.len(), 1);
    }

    #[test]
    fn recovery_bridge_advances_generated_ids_past_history_only_ids() {
        let mut source = AnnotationAdapter::default();
        let document = source.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "comparison:rectangle:41",
                20.,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::DeleteSelected)
            .unwrap();
        assert!(source.snapshot(7).unwrap().rectangles.is_empty());
        let bytes = source.encode_document_recovery_timeline(7).unwrap();

        let mut restored = AnnotationAdapter::default();
        restored
            .documents
            .entry(9)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "comparison:rectangle:83",
                220.,
            )))
            .unwrap();
        restored.queue_next_annotation_id(MarkupId::new("comparison:text:97").unwrap());
        restored
            .restore_document_recovery_timeline(7, &bytes)
            .unwrap();

        assert_eq!(
            restored
                .next_id(AnnotationTool::Rectangle)
                .unwrap()
                .as_str(),
            "comparison:rectangle:98"
        );
    }

    #[test]
    fn imported_comparison_ids_raise_the_adapter_sequence() {
        let mut adapter = AnnotationAdapter::default();
        adapter
            .load_imported_annotations(7, vec![recovery_rectangle("comparison:rectangle:120", 20.)])
            .unwrap();

        assert_eq!(
            adapter.next_id(AnnotationTool::Rectangle).unwrap().as_str(),
            "comparison:rectangle:121"
        );
    }

    #[test]
    fn generated_id_allocation_reports_sequence_exhaustion_without_collision() {
        let mut adapter = AnnotationAdapter {
            next_sequence: u64::MAX - 1,
            ..AnnotationAdapter::default()
        };

        assert_eq!(
            adapter.next_id(AnnotationTool::Rectangle).unwrap().as_str(),
            format!("comparison:rectangle:{}", u64::MAX)
        );
        assert!(matches!(
            adapter.next_id(AnnotationTool::Rectangle),
            Err(AnnotationError::InvalidRecoveryTimeline(_))
        ));
    }

    #[test]
    fn recovery_rejects_an_exhausted_sequence_without_replacing_the_document() {
        let mut source = AnnotationAdapter::default();
        source
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                &format!("comparison:rectangle:{}", u64::MAX),
                20.,
            )))
            .unwrap();
        let bytes = source.encode_document_recovery_timeline(7).unwrap();

        let mut target = AnnotationAdapter::default();
        target
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(recovery_rectangle(
                "target:original",
                80.,
            )))
            .unwrap();
        let before = target.snapshot(7).unwrap();

        assert!(matches!(
            target.restore_document_recovery_timeline(7, &bytes),
            Err(AnnotationError::InvalidRecoveryTimeline(_))
        ));
        assert_eq!(target.snapshot(7).unwrap(), before);
    }

    #[test]
    fn measurement_hover_completion_uses_pdf_point_threshold_without_duplicates() {
        for tool in [AnnotationTool::Polylength, AnnotationTool::Area] {
            for offset in [0.0, 0.499, 0.5] {
                let mut adapter = AnnotationAdapter::default();
                adapter
                    .set_document_page_length_calibration(
                        7,
                        0,
                        LengthCalibration::new(1.0, "mm", "Scale", true).unwrap(),
                    )
                    .unwrap();
                adapter.set_tool(tool).unwrap();
                let mut expected = vec![point(20., 20.), point(100., 20.)];
                if tool == AnnotationTool::Area {
                    expected.push(point(100., 80.));
                }
                for (index, vertex) in expected.iter().copied().enumerate() {
                    adapter
                        .pointer_down(7, 0, index as u64 + 1, vertex, 4.)
                        .unwrap();
                }
                let last = *expected.last().unwrap();
                let hover = point(last.x + offset, last.y);
                adapter.update_measurement_path_hover(7, 0, hover).unwrap();
                if offset >= 0.5 {
                    expected.push(hover);
                }
                let undo_before = adapter.snapshot(7).unwrap().undo_depth;
                adapter.finish_measurement_path(7).unwrap();
                let snapshot = adapter.snapshot(7).unwrap();
                assert_eq!(snapshot.measurement_paths[0].points(), expected.as_slice());
                assert_eq!(snapshot.undo_depth, undo_before + 1);
                assert!(!adapter.measurement_path_pending(7));
            }
        }
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

    fn seed_cloud_plus(adapter: &mut AnnotationAdapter) -> MarkupId {
        let id = MarkupId::new("cloud-plus:pointer-edit").unwrap();
        let annotation = CloudPlusAnnotation::new(
            id.clone(),
            0,
            vec![
                point(10., 10.),
                point(50., 10.),
                point(50., 50.),
                point(10., 50.),
            ],
            2.,
            vec![point(50., 30.), point(75., 30.), point(100., 30.)],
            PdfRect::new(100., 20., 40., 20.).unwrap(),
            "Cloud+",
            default_cloud_plus_appearance().unwrap(),
        )
        .unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::CloudPlus(
                annotation,
            )))
            .unwrap();
        id
    }

    fn routing_obstacle_id(obstacle: &CloudPlusObstacle) -> Option<&str> {
        match obstacle {
            CloudPlusObstacle::Rect { id, .. }
            | CloudPlusObstacle::Polyline { id, .. }
            | CloudPlusObstacle::Polygon { id, .. } => id.as_deref(),
        }
    }

    #[test]
    fn cloud_plus_routing_context_is_page_local_deterministic_and_excludes_owner() {
        let mut adapter = AnnotationAdapter::default();
        let cloud_plus_id = seed_cloud_plus(&mut adapter);
        let callout_id = seed_callout(&mut adapter);
        let other_page_id = MarkupId::new("other-page").unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: other_page_id,
                    page_index: 1,
                    rect: PdfRect::new(20., 20., 40., 40.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        adapter.set_semantic_snap_page_size(7, 0, 612., 792.);

        let context = adapter.cloud_plus_routing_context(
            7,
            0,
            Some(&cloud_plus_id),
            &AnnotationSelectionSupplement::new(),
        );
        assert_eq!(
            context.page_bounds,
            Some(PdfRect::new(0., 0., 612., 792.).unwrap())
        );
        assert_eq!(
            context
                .obstacles
                .iter()
                .filter_map(routing_obstacle_id)
                .map(str::to_owned)
                .collect::<Vec<_>>(),
            vec![
                format!("{}:text", callout_id.as_str()),
                format!("{}:leader", callout_id.as_str()),
            ]
        );
    }

    #[test]
    fn cloud_plus_routing_includes_only_same_page_retained_annotation_bounds() {
        let mut adapter = AnnotationAdapter::default();
        let page_zero = PdfRect::new(200., 20., 80., 60.).unwrap();
        adapter.set_retained_annotation_obstacles(
            7,
            vec![
                RetainedAnnotationObstacle {
                    id: "opaque:1:0:other-page".into(),
                    page_index: 1,
                    rect: PdfRect::new(10., 10., 30., 30.).unwrap(),
                },
                RetainedAnnotationObstacle {
                    id: "opaque:0:0:note".into(),
                    page_index: 0,
                    rect: page_zero,
                },
            ],
        );

        let context =
            adapter.cloud_plus_routing_context(7, 0, None, &AnnotationSelectionSupplement::new());

        assert_eq!(
            context.obstacles,
            vec![CloudPlusObstacle::Rect {
                id: Some("opaque:0:0:note".into()),
                rect: page_zero,
            }]
        );
    }

    #[test]
    fn cloud_plus_routing_uses_the_electron_arc_rect_not_visible_sweep_bounds() {
        let mut adapter = AnnotationAdapter::default();
        let arc_id = MarkupId::new("arc:routing-obstacle").unwrap();
        let arc = ArcAnnotation::new(
            arc_id.clone(),
            0,
            point(100., 100.),
            point(200., 100.),
            point(150., 150.),
            RectangleAppearance::default(),
        )
        .unwrap();
        let expected_rect = arc.rect();
        let visible_bounds = routing_points_bounds(&arc.sampled_path(64)).unwrap();
        assert_ne!(visible_bounds, expected_rect);
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Arc(arc)))
            .unwrap();

        let context =
            adapter.cloud_plus_routing_context(7, 0, None, &AnnotationSelectionSupplement::new());

        assert_eq!(
            context.obstacles,
            vec![CloudPlusObstacle::Rect {
                id: Some(arc_id.as_str().to_owned()),
                rect: expected_rect,
            }]
        );
    }

    #[test]
    fn cloud_plus_routing_uses_ui_measured_dimension_caption_and_reference_line() {
        let mut adapter = AnnotationAdapter::default();
        let dimension_id = seed_dimension(&mut adapter);
        let caption = PdfRect::new(48., 57., 44., 14.).unwrap();
        let supplement = HashMap::from([(
            dimension_id.clone(),
            vec![
                point(caption.x, caption.y),
                point(caption.x + caption.width, caption.y),
                point(caption.x + caption.width, caption.y + caption.height),
                point(caption.x, caption.y + caption.height),
            ],
        )]);

        let context = adapter.cloud_plus_routing_context(7, 0, None, &supplement);

        assert_eq!(
            context.obstacles,
            vec![
                CloudPlusObstacle::Rect {
                    id: Some(format!("{}:caption", dimension_id.as_str())),
                    rect: caption,
                },
                CloudPlusObstacle::Polyline {
                    id: Some(format!("{}:line", dimension_id.as_str())),
                    points: vec![point(20., 40.), point(120., 40.)],
                },
            ]
        );
    }

    #[test]
    fn cloud_plus_creation_preview_and_commit_share_page_and_obstacle_routing() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_semantic_snap_page_size(7, 0, 612., 792.);
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: MarkupId::new("right-side-obstacle").unwrap(),
                    page_index: 0,
                    rect: PdfRect::new(295., 0., 220., 180.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        adapter.set_tool(AnnotationTool::CloudPlus).unwrap();
        adapter.pointer_down(7, 0, 1, point(200., 10.), 4.).unwrap();
        adapter.pointer_move(1, point(280., 60.)).unwrap();

        let draft = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert!(draft.draft);
        assert!(draft.text_box.x + draft.text_box.width <= 200.);
        adapter.pointer_up(1, point(280., 60.)).unwrap();
        let committed = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert!(!committed.draft);
        assert_eq!(committed.text_box, draft.text_box);
        assert_eq!(committed.leader_points, draft.leader_points);
    }

    #[test]
    fn cloud_plus_creation_preview_and_commit_share_measured_caption_routing() {
        let mut adapter = AnnotationAdapter::default();
        let dimension_id = seed_dimension(&mut adapter);
        let blocking_caption = PdfRect::new(295., 0., 220., 180.).unwrap();
        let supplement = HashMap::from([(
            dimension_id,
            vec![
                point(blocking_caption.x, blocking_caption.y),
                point(
                    blocking_caption.x + blocking_caption.width,
                    blocking_caption.y,
                ),
                point(
                    blocking_caption.x + blocking_caption.width,
                    blocking_caption.y + blocking_caption.height,
                ),
                point(
                    blocking_caption.x,
                    blocking_caption.y + blocking_caption.height,
                ),
            ],
        )]);
        adapter.set_semantic_snap_page_size(7, 0, 612., 792.);
        adapter.set_tool(AnnotationTool::CloudPlus).unwrap();
        adapter.pointer_down(7, 0, 1, point(200., 10.), 4.).unwrap();
        adapter.pointer_move(1, point(280., 60.)).unwrap();

        let draft = adapter
            .document_scene_with_routing_supplement(7, 0, &supplement)
            .cloud_pluses
            .remove(0);
        assert!(draft.draft);
        assert!(draft.text_box.x + draft.text_box.width <= 200.);
        adapter
            .pointer_up_with_viewport_input_and_selection_paths(
                1,
                point(280., 60.),
                SelectionPoint::new(280., 60.),
                PointerInputModifiers::default(),
                &supplement,
            )
            .unwrap();
        let committed = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert!(!committed.draft);
        assert_eq!(committed.text_box, draft.text_box);
        assert_eq!(committed.leader_points, draft.leader_points);
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
        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        let before = adapter.snapshot(7).unwrap();
        let original = before.dimensions[0].clone();
        assert_eq!(
            adapter.document_scene(7, 0).dimensions[0].feedback,
            SceneInteractionFeedback::Normal
        );

        assert_eq!(
            adapter.pointer_down(7, 0, 1, original.start, 4.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(1, point(30., 50.)).unwrap();
        let preview = adapter.document_scene(7, 0).dimensions.remove(0);
        assert_eq!(preview.start, point(30., 50.));
        assert!(preview.draft);
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 0,
            }
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        adapter.cancel(PointerCancelReason::FocusLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        adapter
            .set_semantic_snap_settings(
                snapping_off.with_source(SemanticSnapSource::Annotation, true),
            )
            .unwrap();
        adapter.pointer_down(7, 0, 2, original.end, 4.).unwrap();
        adapter.pointer_move(2, point(140., 50.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).dimensions[0].feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 1,
            }
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
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
        let restored = adapter.snapshot(7).unwrap();

        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        adapter.pointer_down(7, 0, 5, point(70., 40.), 4.).unwrap();
        adapter.pointer_move(5, point(80., 50.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).dimensions[0].feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        assert_eq!(adapter.snapshot(7).unwrap(), restored);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), restored);

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

        let mut creation = AnnotationAdapter::default();
        creation
            .begin_dimension_placement(
                9,
                0,
                MarkupId::new("dimension:feedback-creation").unwrap(),
                point(20., 20.),
            )
            .unwrap();
        creation
            .update_dimension_placement(point(120., 20.), false)
            .unwrap();
        let draft = creation.document_scene(9, 0).dimensions.remove(0);
        assert!(draft.draft);
        assert_eq!(draft.feedback, SceneInteractionFeedback::Creation);
        assert!(!draft.feedback.chrome_visible());
        assert_eq!(creation.history_depths(9), (0, 0));
    }

    #[test]
    fn dimension_hover_handles_preserve_stable_order_without_mutation() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_dimension(&mut adapter);
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let original = adapter.snapshot(7).unwrap().dimensions[0].clone();
        let before = adapter.snapshot(7).unwrap();
        let history_before = adapter.history_depths(7);

        for (point, index) in [
            (original.start, 0),
            (original.end, 1),
            (original.caption_center(), 2),
        ] {
            assert_eq!(
                adapter.hover_dimension_handle(7, 0, point, 1.).unwrap(),
                Some((id.clone(), index)),
            );
        }
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(adapter.history_depths(7), history_before);

        adapter.clear_selection(7);
        assert_eq!(
            adapter
                .hover_dimension_handle(7, 0, original.caption_center(), 1.)
                .unwrap(),
            Some((id.clone(), 2)),
            "an unlocked unselected Dimension exposes its visible hover controls",
        );
        assert_eq!(adapter.snapshot(7).unwrap().revision, before.revision);

        adapter.select_id(7, &id);
        adapter.set_selected_locked(7, true).unwrap();
        adapter.clear_selection(7);
        assert_eq!(
            adapter
                .hover_dimension_handle(7, 0, original.start, 1.)
                .unwrap(),
            None,
            "locked Dimension controls remain inert",
        );

        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        assert_eq!(
            adapter
                .hover_dimension_handle(7, 0, original.end, 1.)
                .unwrap(),
            None,
            "Dimension hover controls belong only to Select",
        );
    }

    #[test]
    fn callout_pointer_edits_keep_leader_order_and_distinguish_text_box_from_group() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_callout(&mut adapter);
        let original = adapter.snapshot(7).unwrap().callouts[0].clone();
        let base_revision = adapter.snapshot(7).unwrap().revision;
        assert_eq!(
            adapter.document_scene(7, 0).callouts[0].feedback,
            SceneInteractionFeedback::Normal
        );

        adapter
            .pointer_down(7, 0, 1, original.leader_points()[1], 4.)
            .unwrap();
        adapter.pointer_move(1, point(64., 52.)).unwrap();
        let leader_preview = adapter.document_scene(7, 0).callouts.remove(0);
        assert_eq!(leader_preview.leader_points[1], point(64., 52.));
        assert_eq!(
            leader_preview.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 9,
            }
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
            .pointer_move(2, point(text_center.x + 10., text_center.y + 5.))
            .unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).callouts[0].feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
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
        adapter.pointer_move(3, point(50., 40.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).callouts[0].feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
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

    fn seed_pen(adapter: &mut AnnotationAdapter, points: Vec<PdfPoint>) -> MarkupId {
        let id = MarkupId::new("pen:resize").unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Pen(
                PenAnnotation::new(
                    id.clone(),
                    0,
                    points,
                    PenAppearance::new("#ff0000", 2., 1.).unwrap(),
                )
                .unwrap(),
            )))
            .unwrap();
        id
    }

    #[test]
    fn dragging_a_selected_strokes_handle_scales_it_into_the_new_box_in_one_step() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_pen(
            &mut adapter,
            vec![point(100., 100.), point(150., 140.), point(200., 100.)],
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let history_before = adapter.history_depths(7);
        let resize_box = InkResizeBox::of(
            [&[point(100., 100.), point(150., 140.), point(200., 100.)][..]],
            2.,
        )
        .unwrap();
        assert_eq!(
            resize_box.outer(),
            PdfRect {
                x: 99.,
                y: 99.,
                width: 102.,
                height: 42.
            }
        );
        assert_eq!(resize_box.handles().len(), 8);
        // East handle, at the right edge of the visible stroke.
        let east = point(201., 120.);
        assert_eq!(
            adapter.hover_transform_handle(7, 0, east, 2.).unwrap(),
            Some((id.clone(), 3))
        );
        assert_eq!(
            adapter.pointer_down(7, 0, 1, east, 2.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(1, point(301., 120.)).unwrap();
        let preview = adapter.document_scene(7, 0).pens.remove(0);
        assert_eq!(
            preview.paths[0],
            vec![point(100., 100.), point(200., 140.), point(300., 100.)]
        );
        assert_eq!(
            adapter.history_depths(7),
            history_before,
            "a preview adds no history"
        );
        assert_eq!(
            adapter.pointer_up(1, point(301., 120.)).unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id.clone())
        );
        let resized = adapter.snapshot(7).unwrap().pens[0].clone();
        assert_eq!(
            resized.paths().next().unwrap(),
            &[point(100., 100.), point(200., 140.), point(300., 100.)][..],
            "the west edge stays put and heights are untouched"
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().pens[0].paths().next().unwrap(),
            &[point(100., 100.), point(150., 140.), point(200., 100.)][..]
        );
    }

    #[test]
    fn a_flat_stroke_has_no_handles_across_its_flat_side_and_locked_strokes_do_not_resize() {
        let flat = InkResizeBox::of([&[point(100., 100.), point(200., 100.)][..]], 12.).unwrap();
        let indices = flat
            .handles()
            .into_iter()
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(
            indices,
            vec![3, 7],
            "a level highlight only has its east and west handles"
        );

        let mut adapter = AnnotationAdapter::default();
        let id = seed_pen(
            &mut adapter,
            vec![point(100., 100.), point(150., 140.), point(200., 100.)],
        );
        let document = adapter.documents.get_mut(&7).unwrap();
        assert!(document.select(&id));
        document
            .apply_command(AnnotationCommand::SetLocked {
                id: id.clone(),
                locked: true,
            })
            .unwrap();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let before = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .hover_transform_handle(7, 0, point(201., 120.), 2.)
                .unwrap(),
            None
        );
        adapter
            .pointer_down(7, 0, 1, point(201., 120.), 2.)
            .unwrap();
        adapter.pointer_move(1, point(301., 120.)).unwrap();
        adapter.pointer_up(1, point(301., 120.)).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().pens, before.pens);
    }

    #[test]
    fn a_callout_box_sits_beyond_its_drag_end_away_from_the_tip_and_fits_its_text() {
        for (end, leftwards) in [(point(260., 300.), false), (point(140., 300.), true)] {
            let mut adapter = AnnotationAdapter::default();
            adapter.set_tool(AnnotationTool::Callout).unwrap();
            let start = point(200., 200.);
            adapter.pointer_down(7, 0, 1, start, 4.).unwrap();
            adapter.pointer_move(1, end).unwrap();
            let preview = adapter.document_scene(7, 0).callouts.remove(0);
            adapter.pointer_up(1, end).unwrap();
            let callout = adapter.snapshot(7).unwrap().callouts[0].clone();
            assert_eq!(callout.text_box, preview.text_box);
            assert_eq!(callout.leader_points(), preview.leader_points);
            let text_box = callout.text_box;
            assert_eq!(*callout.leader_points().last().unwrap(), end);
            if leftwards {
                assert_eq!(
                    text_box.x + text_box.width,
                    end.x,
                    "a leftward box ends at the drag end"
                );
            } else {
                assert_eq!(text_box.x, end.x, "a rightward box starts at the drag end");
            }
            assert!(
                !(text_box.x..text_box.x + text_box.width).contains(&start.x),
                "the box must not sit over its own leader"
            );
            assert_eq!(
                text_box.width,
                caption_box_width(CALLOUT_DEFAULT_TEXT, callout.appearance.text())
            );
        }
    }

    #[test]
    fn editing_callout_text_refits_its_box_against_the_leader_in_one_step() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_callout(&mut adapter);
        let original = adapter.snapshot(7).unwrap().callouts[0].clone();
        let history_before = adapter.history_depths(7);

        adapter
            .replace_callout_text(7, &id, "A much longer note\non two lines")
            .unwrap();
        let edited = adapter.snapshot(7).unwrap().callouts[0].clone();
        assert_eq!(edited.content(), "A much longer note\non two lines");
        assert_eq!(
            edited.text_box.x, original.text_box.x,
            "the leader meets the left edge"
        );
        assert!(edited.text_box.width > original.text_box.width);
        assert!(edited.text_box.height >= 2. * 12. * 1.15 + 12.);
        assert_eq!(
            edited.text_box.y + edited.text_box.height * 0.5,
            original.text_box.y + original.text_box.height * 0.5
        );
        assert_eq!(*edited.leader_points().last().unwrap(), point(100., 40.));
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().callouts[0], original);
    }

    #[test]
    fn double_clicking_a_selected_callouts_text_opens_it_for_editing() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_callout(&mut adapter);
        assert!(adapter.documents.get_mut(&7).unwrap().select(&id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let history_before = adapter.history_depths(7);
        assert_eq!(
            adapter
                .pointer_double_click(7, 0, point(140., 40.), 2.)
                .unwrap(),
            PointerPhaseOutcome::SelectionChanged(Some(id))
        );
        assert_eq!(adapter.history_depths(7), history_before);
    }

    #[test]
    fn callout_text_box_resize_previews_cancels_and_commits_once() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_callout(&mut adapter);
        let before = adapter.snapshot(7).unwrap();
        let original = before.callouts[0].clone();
        let north_handle = RectangleResizeHandle::North.point(original.text_box);
        let resized_north = point(north_handle.x, north_handle.y + 22.);

        adapter.pointer_down(7, 0, 1, north_handle, 4.).unwrap();
        adapter.pointer_move(1, resized_north).unwrap();
        let resize_preview = adapter.document_scene(7, 0).callouts.remove(0);
        assert_eq!(
            resize_preview.text_box,
            PdfRect::new(100., 20., 80., 62.).unwrap()
        );
        assert_eq!(
            resize_preview.leader_points,
            vec![point(20., 20.), point(60., 40.), point(100., 51.),]
        );
        assert_eq!(
            resize_preview.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 1,
            }
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        adapter.pointer_down(7, 0, 2, north_handle, 4.).unwrap();
        assert_eq!(
            adapter.pointer_up(2, resized_north).unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id)
        );
        let resized = adapter.snapshot(7).unwrap();
        assert_eq!(resized.revision, before.revision + 1);
        assert_eq!(resized.undo_depth, before.undo_depth + 1);
        assert_eq!(
            resized.callouts[0].text_box,
            PdfRect::new(100., 20., 80., 62.).unwrap()
        );
        assert_eq!(resized.callouts[0].leader_points()[2], point(100., 51.));
        assert_eq!(resized.callouts[0].id, original.id);
        assert_eq!(resized.callouts[0].content(), original.content());
        assert_eq!(resized.callouts[0].appearance, original.appearance);
    }

    #[test]
    fn callout_composite_routes_snap_to_external_geometry_without_preview_history() {
        use crate::semantic_snapping::SemanticSnapRole;

        let mut adapter = AnnotationAdapter::default();
        let id = seed_callout(&mut adapter);
        let target_id = MarkupId::new("callout:snap-target").unwrap();
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        target_id.clone(),
                        0,
                        point(210., 20.),
                        point(250., 20.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        assert!(adapter.documents.get_mut(&7).unwrap().select(&id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();
        let original = adapter.snapshot(7).unwrap().callouts[0].clone();
        let history_before = adapter.history_depths(7);

        adapter
            .pointer_down(7, 0, 81, original.leader_points()[0], 4.)
            .unwrap();
        adapter.pointer_move(81, point(209., 21.)).unwrap();
        let preview = adapter.document_scene(7, 0).callouts.remove(0);
        assert_eq!(preview.leader_points[0], point(210., 20.));
        assert_eq!(adapter.history_depths(7), history_before);
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().callouts[0], original);

        let text_center = point(
            original.text_box.x + original.text_box.width * 0.5,
            original.text_box.y + original.text_box.height * 0.5,
        );
        adapter.pointer_down(7, 0, 82, text_center, 4.).unwrap();
        adapter
            .pointer_move(82, point(text_center.x + 29., text_center.y + 1.))
            .unwrap();
        let preview = adapter.document_scene(7, 0).callouts.remove(0);
        assert_eq!(preview.text_box.x, original.text_box.x + 30.);
        assert_eq!(preview.text_box.y, original.text_box.y);
        assert_eq!(preview.leader_points[0], original.leader_points()[0]);
        assert_eq!(preview.leader_points[1], original.leader_points()[1]);
        assert_eq!(preview.leader_points[2], point(130., 40.));
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        assert_eq!(adapter.history_depths(7), history_before);
        assert_eq!(
            adapter
                .pointer_up(82, point(text_center.x + 29., text_center.y + 1.))
                .unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id)
        );
        let committed = adapter.snapshot(7).unwrap();
        assert_eq!(committed.callouts[0].text_box, preview.text_box);
        assert_eq!(committed.callouts[0].leader_points(), preview.leader_points);
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
    }

    #[test]
    fn callout_hover_handle_uses_feedback_order_and_leader_overlap_priority() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_callout(&mut adapter);
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter.clear_selection(7);
        let before_hover = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .hover_callout_handle(7, 0, point(20., 20.), 4.)
                .unwrap(),
            Some((id.clone(), 8))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before_hover);
        assert_eq!(
            adapter
                .hover_callout_handle(7, 0, point(100., 40.), 4.)
                .unwrap(),
            Some((id.clone(), 10))
        );
        assert_eq!(
            adapter
                .hover_callout_handle(7, 0, point(140., 40.), 4.)
                .unwrap()
                .map(|(_, index)| index),
            None
        );
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        assert_eq!(
            adapter
                .hover_callout_handle(7, 0, point(20., 20.), 4.)
                .unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::SetLocked { id, locked: true })
            .unwrap();
        assert_eq!(
            adapter
                .hover_callout_handle(7, 0, point(20., 20.), 4.)
                .unwrap(),
            None
        );
    }

    #[test]
    fn cloud_plus_composite_snaps_handles_and_text_move_without_self_targets_or_preview_history() {
        use crate::semantic_snapping::{SemanticSnapRole, SemanticSnapSource};

        let mut adapter = AnnotationAdapter::default();
        let id = seed_cloud_plus(&mut adapter);
        let target_id = MarkupId::new("cloud-plus:snap-target").unwrap();
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        target_id.clone(),
                        0,
                        point(200., 20.),
                        point(240., 20.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        assert!(adapter.documents.get_mut(&7).unwrap().select(&id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();
        let original = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        let history_before = adapter.history_depths(7);

        let index = SemanticSnapIndex::from_annotation_scene(
            &adapter.document_scene(7, 0),
            std::slice::from_ref(&target_id),
        );
        let indexed_cloud_point = index
            .resolve_point(
                point(50.5, 10.5),
                &adapter.semantic_snap_settings,
                adapter.observed_pixels_per_point.0,
            )
            .expect("Cloud+ control geometry must be an annotation snap target");
        assert_eq!(indexed_cloud_point.owner_id.as_ref(), Some(&id));
        assert_eq!(indexed_cloud_point.point, point(50., 10.));

        adapter
            .pointer_down(7, 0, 90, original.cloud_points()[1], 4.)
            .unwrap();
        adapter.pointer_move(90, point(199., 21.)).unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_eq!(preview.cloud_points[1], point(200., 20.));
        assert_eq!(adapter.history_depths(7), history_before);
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert!(adapter.snapshot(7).unwrap().cloud_pluses[0].same_persisted_state_as(&original));

        let text_center = point(
            original.text_box.x + original.text_box.width * 0.5,
            original.text_box.y + original.text_box.height * 0.5,
        );
        adapter.pointer_down(7, 0, 91, text_center, 4.).unwrap();
        adapter
            .pointer_move(91, point(text_center.x + 149., text_center.y + 10.))
            .unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_eq!(preview.cloud_points, original.cloud_points());
        assert_eq!(preview.text_box, PdfRect::new(250., 30., 40., 20.).unwrap());
        assert_eq!(adapter.history_depths(7), history_before);
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        assert_eq!(
            adapter
                .pointer_up(91, point(text_center.x + 149., text_center.y + 10.))
                .unwrap(),
            PointerPhaseOutcome::AnnotationEdited(id)
        );
        let committed = adapter.snapshot(7).unwrap();
        assert_eq!(committed.cloud_pluses[0].text_box, preview.text_box);
        assert_eq!(
            committed.cloud_pluses[0].leader_points(),
            preview.leader_points
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
    }

    #[test]
    fn cloud_pointer_vertex_and_body_edits_preserve_scallop_authority_and_cancel_exactly() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_cloud(&mut adapter);
        let before = adapter.snapshot(7).unwrap();
        let original = before.clouds[0].clone();
        assert_eq!(
            adapter.document_scene(7, 0).clouds[0].feedback,
            SceneInteractionFeedback::Normal
        );

        adapter
            .pointer_down(7, 0, 1, original.points()[0], 4.)
            .unwrap();
        adapter.pointer_move(1, point(25., 30.)).unwrap();
        let preview = adapter.document_scene(7, 0).clouds.remove(0);
        assert_eq!(preview.points[0], point(25., 30.));
        assert_ne!(preview.scallop_path, original.scallop_path());
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 0,
            }
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        let body = original
            .scallop_path()
            .iter()
            .copied()
            .max_by(|left, right| {
                let clearance = |candidate: PdfPoint| {
                    original
                        .points()
                        .iter()
                        .map(|point| distance(*point, candidate))
                        .fold(f64::INFINITY, f64::min)
                };
                clearance(*left).total_cmp(&clearance(*right))
            })
            .expect("a valid Cloud has a visible scallop body");
        adapter.pointer_down(7, 0, 2, body, 4.).unwrap();
        adapter
            .pointer_move(2, point(body.x + 10., body.y + 8.))
            .unwrap();
        let preview = adapter.document_scene(7, 0).clouds.remove(0);
        assert_eq!(preview.points[0], point(30., 28.));
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
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

    #[test]
    fn cloud_body_move_snaps_control_path_anchors_and_excludes_its_own_geometry() {
        let mut adapter = AnnotationAdapter::default();
        let cloud_id = seed_cloud(&mut adapter);
        let target_id = MarkupId::new("cloud:snap-target").unwrap();
        let target = StraightLineAnnotation::new(
            target_id.clone(),
            0,
            point(140., 20.),
            point(180., 20.),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let document = adapter.documents.get_mut(&7).unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(target),
            ))
            .unwrap();
        assert!(document.select(&cloud_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::Annotation, true)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();

        let cloud = adapter.snapshot(7).unwrap().clouds[0].clone();
        let body = cloud
            .scallop_path()
            .iter()
            .copied()
            .max_by(|left, right| {
                let clearance = |candidate: PdfPoint| {
                    cloud
                        .points()
                        .iter()
                        .map(|point| distance(*point, candidate))
                        .fold(f64::INFINITY, f64::min)
                };
                clearance(*left).total_cmp(&clearance(*right))
            })
            .expect("a valid Cloud has a visible scallop body");
        adapter.pointer_down(7, 0, 8, body, 4.).unwrap();
        assert!(
            matches!(
                adapter.active.as_ref(),
                Some(ActivePointer::CloudEdit {
                    kind: CloudPointerEditKind::Body,
                    ..
                })
            ),
            "unexpected Cloud body pointer state: {:?}",
            adapter.active
        );
        adapter
            .pointer_move(8, point(body.x + 39.5, body.y))
            .unwrap();
        let preview = adapter.document_scene(7, 0).clouds.remove(0);
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        assert_eq!(preview.points[1], point(140., 20.));
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().clouds[0].points()[1],
            point(100., 20.)
        );
    }

    #[test]
    fn a_line_snaps_beside_another_strokes_edges_as_well_as_onto_it() {
        // Snapping along edges (Nearest) is off by default; corners and
        // midpoints snap either way.
        let annotations_only = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false)
            .with_target(crate::semantic_snapping::SemanticSnapTarget::Nearest, true);
        let mut adapter = AnnotationAdapter::default();
        adapter
            .set_semantic_snap_settings(annotations_only)
            .unwrap();
        // Zoomed in, so positions a point apart are visibly apart; exact
        // targets win while beside-the-stroke ones are within a few pixels.
        adapter.set_observed_pixels_per_point(4.).unwrap();
        // A rectangle with a 4 pt stroke, its right edge's centre line at
        // x = 200.
        let mut thick = ToolProperties::for_tool(AnnotationTool::Rectangle);
        thick.width_pt = 4.;
        adapter
            .set_tool_properties(AnnotationTool::Rectangle, thick)
            .unwrap();
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        adapter
            .pointer_down(7, 0, 1, point(100., 100.), 4.)
            .unwrap();
        adapter.pointer_move(1, point(200., 200.)).unwrap();
        adapter.pointer_up(1, point(200., 200.)).unwrap();
        // Drawing a 2 pt rectangle next to it.
        let mut thin = ToolProperties::for_tool(AnnotationTool::Rectangle);
        thin.width_pt = 2.;
        adapter
            .set_tool_properties(AnnotationTool::Rectangle, thin)
            .unwrap();
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        let snapped_x = |adapter: &mut AnnotationAdapter, x: f64| {
            adapter
                // Clear of the edge's midpoint, which points outrank.
                .resolve_semantic_creation_point(7, 0, point(x, 125.), false)
                .x
        };
        // The strokes' outer edges touch: 3 pt off the centre line, either
        // side.
        assert!((snapped_x(&mut adapter, 203.6) - 203.).abs() < 1e-9);
        assert!((snapped_x(&mut adapter, 196.6) - 197.).abs() < 1e-9);
        // The thinner line flush inside the thicker: 1 pt off it.
        assert!((snapped_x(&mut adapter, 198.9) - 199.).abs() < 1e-9);
        // Exactly over it, as before.
        assert!((snapped_x(&mut adapter, 200.3) - 200.).abs() < 1e-9);
        // With the default targets, a corner and an edge's midpoint where
        // both strokes touch.
        adapter
            .set_semantic_snap_settings(
                annotations_only
                    .with_target(crate::semantic_snapping::SemanticSnapTarget::Nearest, false),
            )
            .unwrap();
        let corner = adapter.resolve_semantic_creation_point(7, 0, point(203.4, 203.4), false);
        assert!((corner.x - 203.).abs() < 1e-9 && (corner.y - 203.).abs() < 1e-9);
        let midpoint = adapter.resolve_semantic_creation_point(7, 0, point(203.4, 150.5), false);
        assert!((midpoint.x - 203.).abs() < 1e-9 && (midpoint.y - 150.).abs() < 1e-9);
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
    fn shape_feedback_distinguishes_creation_move_and_active_transform_without_history() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Ellipse).unwrap();
        adapter.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        adapter.pointer_move(1, point(120., 80.)).unwrap();
        let creation = adapter.document_scene(7, 0).ellipses.remove(0);
        assert!(creation.preview);
        assert_eq!(creation.feedback, SceneInteractionFeedback::Creation);
        adapter.pointer_up(1, point(120., 80.)).unwrap();
        let snapshot = adapter.snapshot(7).unwrap();
        assert_eq!((snapshot.undo_depth, snapshot.redo_depth), (1, 0));

        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        // The move band just inside the curve; the curve itself resizes.
        adapter.pointer_down(7, 0, 2, point(30., 50.), 4.).unwrap();
        adapter.pointer_move(2, point(40., 60.)).unwrap();
        let moving = adapter.document_scene(7, 0).ellipses.remove(0);
        assert_eq!(
            moving.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        let snapshot = adapter.snapshot(7).unwrap();
        assert_eq!((snapshot.undo_depth, snapshot.redo_depth), (1, 0));
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        let ellipse = adapter.snapshot(7).unwrap().ellipses[0].clone();
        let east = ellipse_resize_handle_point(&ellipse, RectangleResizeHandle::East);
        adapter.pointer_down(7, 0, 3, east, 4.).unwrap();
        adapter
            .pointer_move(3, point(east.x + 20., east.y))
            .unwrap();
        let resizing = adapter.document_scene(7, 0).ellipses.remove(0);
        assert_eq!(
            resizing.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 3,
            }
        );
        let snapshot = adapter.snapshot(7).unwrap();
        assert_eq!((snapshot.undo_depth, snapshot.redo_depth), (1, 0));
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        let snapping = snapping_off.with_source(SemanticSnapSource::Annotation, true);
        adapter.set_semantic_snap_settings(snapping).unwrap();
        adapter.pointer_down(7, 0, 4, point(30., 50.), 4.).unwrap();
        adapter.pointer_move(4, point(40., 60.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).ellipses[0].feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        let mut redact = AnnotationAdapter::default();
        redact.set_tool(AnnotationTool::Redact).unwrap();
        redact.pointer_down(9, 0, 10, point(30., 30.), 4.).unwrap();
        redact.pointer_move(10, point(130., 90.)).unwrap();
        assert_eq!(
            redact.document_scene(9, 0).redacts[0].feedback,
            SceneInteractionFeedback::Creation
        );
    }

    #[test]
    fn box_shapes_resize_from_anywhere_along_their_edges_selected_or_not() {
        let mut adapter = AnnotationAdapter::default();
        let text_id = MarkupId::new("text-box:edge").unwrap();
        let image_id = MarkupId::new("image:edge").unwrap();
        let redact_id = MarkupId::new("redact:edge").unwrap();
        let ellipse_id = MarkupId::new("ellipse:edge").unwrap();
        let document = adapter.documents.entry(7).or_default();
        for annotation in [
            Annotation::TextBox(
                TextBoxAnnotation::new(
                    text_id.clone(),
                    0,
                    PdfRect::new(20., 20., 100., 50.).unwrap(),
                    "Text",
                    TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                )
                .unwrap(),
            ),
            Annotation::Image(
                ImageAnnotation::new(
                    image_id.clone(),
                    0,
                    PdfRect::new(200., 20., 100., 50.).unwrap(),
                    DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
                    false,
                )
                .unwrap(),
            ),
            Annotation::Redact(
                RedactAnnotation::new(
                    redact_id.clone(),
                    0,
                    PdfRect::new(200., 120., 100., 50.).unwrap(),
                    "#000000",
                    None::<String>,
                    RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
                        .unwrap()
                        .with_fill_opacity(0.35)
                        .unwrap(),
                )
                .unwrap(),
            ),
            Annotation::Ellipse(
                EllipseAnnotation::new(
                    ellipse_id.clone(),
                    0,
                    PdfRect::new(20., 120., 100., 60.).unwrap(),
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
        ] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(annotation))
                .unwrap();
        }
        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        adapter.set_tool(AnnotationTool::Select).unwrap();

        // An Ellipse resizes from its curve, by the nearest eighth of a turn,
        // and the grabbed point follows the pointer without jumping to the
        // handle.
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        let original = PdfRect::new(20., 120., 100., 60.).unwrap();
        let turn = 30_f64.to_radians();
        let grab = point(70. + 50. * turn.cos(), 150. + 30. * turn.sin());
        let north_east = RectangleResizeHandle::ALL
            .iter()
            .position(|handle| *handle == RectangleResizeHandle::NorthEast)
            .unwrap();
        assert_eq!(
            adapter.hover_transform_handle(7, 0, grab, 1.).unwrap(),
            Some((ellipse_id.clone(), north_east))
        );
        adapter.pointer_down(7, 0, 4, grab, 1.).unwrap();
        adapter
            .pointer_up(4, point(grab.x + 12., grab.y + 6.))
            .unwrap();
        let handle =
            ellipse_resize_handle_point_for_rect(original, 0., RectangleResizeHandle::NorthEast);
        assert_eq!(
            adapter.snapshot(7).unwrap().ellipses[0].rect,
            ellipse_resized_rect(
                original,
                0.,
                RectangleResizeHandle::NorthEast,
                point(handle.x + 12., handle.y + 6.),
            )
        );
        // Well away from the east edge's midpoint, with nothing selected:
        // the cursor offers East and the press resizes.
        let east = RectangleResizeHandle::ALL
            .iter()
            .position(|handle| *handle == RectangleResizeHandle::East)
            .unwrap();
        for (pointer, id, grab) in [
            (1, &text_id, point(120., 28.)),
            (2, &image_id, point(300., 28.)),
            (3, &redact_id, point(300., 128.)),
        ] {
            adapter.documents.get_mut(&7).unwrap().clear_selection();
            assert_eq!(
                adapter.hover_transform_handle(7, 0, grab, 1.).unwrap(),
                Some((id.clone(), east)),
                "{id:?}"
            );
            adapter.pointer_down(7, 0, pointer, grab, 1.).unwrap();
            adapter
                .pointer_up(pointer, point(grab.x + 40., grab.y))
                .unwrap();
            let snapshot = adapter.snapshot(7).unwrap();
            let width = snapshot
                .text_boxes
                .iter()
                .map(|annotation| (&annotation.id, annotation.layout_rect.width))
                .chain(
                    snapshot
                        .images
                        .iter()
                        .map(|annotation| (&annotation.id, annotation.rect.width)),
                )
                .chain(
                    snapshot
                        .redacts
                        .iter()
                        .map(|annotation| (&annotation.id, annotation.rect.width)),
                )
                .find(|(candidate, _)| *candidate == id)
                .map(|(_, width)| width);
            assert_eq!(width, Some(140.), "{id:?}");
        }

        // Just inside the curve is the move band, as for a Rectangle.
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_transform_handle(7, 0, point(80., 150.), 1.)
                .unwrap(),
            None
        );
    }

    #[test]
    fn text_box_and_image_feedback_distinguish_move_transform_and_snap_visibility() {
        let mut adapter = AnnotationAdapter::default();
        let text_id = MarkupId::new("text-box:feedback").unwrap();
        let image_id = MarkupId::new("image:feedback").unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::TextBox(
                TextBoxAnnotation::new(
                    text_id.clone(),
                    0,
                    PdfRect::new(20., 20., 100., 50.).unwrap(),
                    "Text",
                    TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                )
                .unwrap(),
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Image(
                ImageAnnotation::new(
                    image_id.clone(),
                    0,
                    PdfRect::new(200., 20., 100., 50.).unwrap(),
                    DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
                    false,
                )
                .unwrap(),
            )))
            .unwrap();
        let baseline = adapter.snapshot(7).unwrap();
        assert_eq!((baseline.undo_depth, baseline.redo_depth), (2, 0));

        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        adapter.documents.get_mut(&7).unwrap().select(&text_id);
        adapter.pointer_down(7, 0, 1, point(70., 45.), 4.).unwrap();
        adapter.pointer_move(1, point(80., 55.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).text_boxes[0].feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        adapter.pointer_down(7, 0, 2, point(120., 45.), 4.).unwrap();
        adapter.pointer_move(2, point(140., 45.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).text_boxes[0].feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 3,
            }
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        adapter.documents.get_mut(&7).unwrap().select(&image_id);
        adapter.pointer_down(7, 0, 3, point(250., 45.), 4.).unwrap();
        adapter.pointer_move(3, point(260., 55.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).images[0].feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        adapter.pointer_down(7, 0, 4, point(300., 45.), 4.).unwrap();
        adapter.pointer_move(4, point(320., 45.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).images[0].feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 3,
            }
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), baseline);

        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::Annotation, true)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();
        adapter.pointer_down(7, 0, 5, point(300., 45.), 4.).unwrap();
        adapter.pointer_move(5, point(320., 45.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).images[0].feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 3,
            }
        );
        adapter.pointer_up(5, point(320., 45.)).unwrap();
        assert_eq!(adapter.history_depths(7), (3, 0));
    }

    #[test]
    fn selected_text_box_and_image_hover_handles_preserve_stable_feedback_identity() {
        let mut adapter = AnnotationAdapter::default();
        let text_id = MarkupId::new("text-box:hover-handle").unwrap();
        let lower_text_id = MarkupId::new("text-box:hover-handle-lower").unwrap();
        let upper_text_id = MarkupId::new("text-box:hover-handle-upper").unwrap();
        let image_id = MarkupId::new("image:hover-handle").unwrap();
        let aspect_image_id = MarkupId::new("image:hover-handle-aspect").unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::TextBox(
                TextBoxAnnotation::new(
                    text_id.clone(),
                    0,
                    PdfRect::new(20., 20., 100., 50.).unwrap(),
                    "Text",
                    TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                )
                .unwrap(),
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Image(
                ImageAnnotation::new(
                    aspect_image_id.clone(),
                    0,
                    PdfRect::new(350., 20., 100., 50.).unwrap(),
                    DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
                    true,
                )
                .unwrap(),
            )))
            .unwrap();
        for id in [&lower_text_id, &upper_text_id] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::TextBox(
                    TextBoxAnnotation::new(
                        id.clone(),
                        0,
                        PdfRect::new(500., 20., 100., 50.).unwrap(),
                        "Overlap",
                        TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
                    )
                    .unwrap(),
                )))
                .unwrap();
        }
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Image(
                ImageAnnotation::new(
                    image_id.clone(),
                    0,
                    PdfRect::new(200., 20., 100., 50.).unwrap(),
                    DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
                    false,
                )
                .unwrap(),
            )))
            .unwrap();

        adapter.documents.get_mut(&7).unwrap().select(&text_id);
        let before_text_hover = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, point(120., 45.), 4.)
                .unwrap(),
            Some((text_id.clone(), 3))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before_text_hover);

        adapter.clear_selection(7);
        let before_unselected_hover = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, point(120., 45.), 4.)
                .unwrap(),
            Some((text_id.clone(), 3))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before_unselected_hover);
        let text_rotation = {
            let text = adapter
                .documents
                .get(&7)
                .unwrap()
                .text_boxes()
                .iter()
                .find(|annotation| annotation.id == text_id)
                .unwrap();
            text_box_rotation_handle_point(text, adapter.observed_pixels_per_point.0).unwrap()
        };
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, text_rotation, 4.)
                .unwrap(),
            None,
            "Electron withholds Text Box rotation controls until selection"
        );
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, point(600., 45.), 4.)
                .unwrap(),
            Some((upper_text_id.clone(), 3)),
            "the topmost unselected Text Box must own an overlapping hover control"
        );
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .select(&lower_text_id);
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, point(600., 45.), 4.)
                .unwrap(),
            Some((lower_text_id, 3)),
            "the selected Text Box must win before an overlapping unselected control"
        );

        adapter.clear_selection(7);
        let before_unselected_image_hover = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .hover_image_handle(7, 0, point(300., 45.), 4.)
                .unwrap(),
            Some((image_id.clone(), 3))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before_unselected_image_hover);
        let image_rotation = {
            let image = adapter
                .documents
                .get(&7)
                .unwrap()
                .images()
                .iter()
                .find(|annotation| annotation.id == image_id)
                .unwrap();
            image_rotation_handle_point(image, adapter.observed_pixels_per_point.0).unwrap()
        };
        assert_eq!(
            adapter
                .hover_image_handle(7, 0, image_rotation, 4.)
                .unwrap(),
            None,
            "Electron withholds Image rotation controls until selection"
        );

        adapter.documents.get_mut(&7).unwrap().select(&image_id);
        let before_image_hover = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .hover_image_handle(7, 0, point(300., 45.), 4.)
                .unwrap(),
            Some((image_id.clone(), 3))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before_image_hover);
        assert_eq!(
            adapter
                .hover_image_handle(7, 0, image_rotation, 4.)
                .unwrap(),
            Some((image_id.clone(), 8))
        );

        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .select(&aspect_image_id);
        assert_eq!(
            adapter
                .hover_image_handle(7, 0, point(450., 45.), 4.)
                .unwrap(),
            None
        );
        assert_eq!(
            adapter
                .hover_image_handle(7, 0, point(450., 70.), 4.)
                .unwrap(),
            Some((aspect_image_id.clone(), 4))
        );
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::SetLocked {
                id: aspect_image_id,
                locked: true,
            })
            .unwrap();
        assert_eq!(
            adapter
                .hover_image_handle(7, 0, point(450., 70.), 4.)
                .unwrap(),
            None
        );

        adapter.documents.get_mut(&7).unwrap().select(&text_id);
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::SetLocked {
                id: text_id.clone(),
                locked: true,
            })
            .unwrap();
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, point(120., 45.), 4.)
                .unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, point(120., 45.), 4.)
                .unwrap(),
            None
        );
    }

    #[test]
    fn text_box_and_image_rotation_handles_use_raw_geometry_and_single_commit_history() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let text_id = MarkupId::new("text-box:rotate").unwrap();
        let image_id = MarkupId::new("image:rotate").unwrap();
        let overlapping_id = MarkupId::new("image:overlap").unwrap();
        let text = TextBoxAnnotation::new(
            text_id.clone(),
            0,
            PdfRect::new(20., 20., 100., 50.).unwrap(),
            "Text",
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap()
        .with_rotation_degrees(30.)
        .unwrap();
        let image = ImageAnnotation::new(
            image_id.clone(),
            0,
            PdfRect::new(200., 20., 100., 50.).unwrap(),
            DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
            true,
        )
        .unwrap()
        .with_rotation_degrees(330.)
        .unwrap();
        let overlapping = ImageAnnotation::new(
            overlapping_id,
            0,
            PdfRect::new(40., 70., 40., 30.).unwrap(),
            DecodedRgbaAsset::new(1, 1, vec![255; 4]).unwrap(),
            false,
        )
        .unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .load_imported_annotations(
                vec![
                    Annotation::TextBox(text),
                    Annotation::Image(image),
                    Annotation::Image(overlapping),
                ],
                Vec::new(),
            )
            .unwrap();

        adapter.documents.get_mut(&7).unwrap().select(&text_id);
        let text = adapter.documents[&7]
            .text_boxes()
            .iter()
            .find(|item| item.id == text_id)
            .unwrap()
            .clone();
        let text_handle = text_box_rotation_handle_point(&text, 1.).unwrap();
        assert_eq!(
            adapter
                .hover_text_box_handle(7, 0, text_handle, 1.)
                .unwrap(),
            Some((text_id.clone(), 8)),
            "the selected rotation handle wins even when another body overlaps it",
        );
        let text_center = point(
            text.layout_rect.x + text.layout_rect.width * 0.5,
            text.layout_rect.y + text.layout_rect.height * 0.5,
        );
        let start_angle = (text_handle.y - text_center.y).atan2(text_handle.x - text_center.x);
        let radius = distance(text_center, text_handle);
        let target_angle = start_angle - 60_f64.to_radians();
        let target = point(
            text_center.x + radius * target_angle.cos(),
            text_center.y + radius * target_angle.sin(),
        );
        assert_eq!(
            adapter.pointer_down(7, 0, 10, text_handle, 1.).unwrap(),
            PointerPhaseOutcome::GestureStarted,
        );
        adapter.pointer_move(10, target).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).text_boxes[0].rotation_degrees,
            90.
        );
        let history_before = adapter.history_depths(7);
        assert_eq!(
            adapter.pointer_up(10, target).unwrap(),
            PointerPhaseOutcome::AnnotationEdited(text_id.clone()),
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        assert_eq!(
            adapter.snapshot(7).unwrap().text_boxes[0].rotation_degrees(),
            90.
        );
        adapter.undo(7).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().text_boxes[0].rotation_degrees(),
            30.
        );
        let reset_handle =
            text_box_rotation_handle_point(&adapter.snapshot(7).unwrap().text_boxes[0], 1.)
                .unwrap();
        let history_before_reset = adapter.history_depths(7);
        assert_eq!(
            adapter
                .pointer_double_click(7, 0, reset_handle, 1.)
                .unwrap(),
            PointerPhaseOutcome::AnnotationEdited(text_id.clone()),
        );
        assert_eq!(
            adapter.snapshot(7).unwrap().text_boxes[0].rotation_degrees(),
            0.
        );
        assert_eq!(adapter.history_depths(7), (history_before_reset.0 + 1, 0));
        let zero_handle =
            text_box_rotation_handle_point(&adapter.snapshot(7).unwrap().text_boxes[0], 1.)
                .unwrap();
        let history_before_no_op_reset = adapter.history_depths(7);
        assert_eq!(
            adapter.pointer_double_click(7, 0, zero_handle, 1.).unwrap(),
            PointerPhaseOutcome::SelectionChanged(Some(text_id.clone())),
        );
        assert_eq!(adapter.history_depths(7), history_before_no_op_reset);

        adapter.documents.get_mut(&7).unwrap().select(&image_id);
        let image = adapter.documents[&7]
            .images()
            .iter()
            .find(|item| item.id == image_id)
            .unwrap();
        let image_handle = image_rotation_handle_point(image, 1.).unwrap();
        assert_eq!(
            adapter.hover_image_handle(7, 0, image_handle, 1.).unwrap(),
            Some((image_id.clone(), 8)),
        );
        let image_center = point(
            image.rect.x + image.rect.width * 0.5,
            image.rect.y + image.rect.height * 0.5,
        );
        let no_op_target = point(
            image_handle.x + (image_handle.x - image_center.x) * 0.25,
            image_handle.y + (image_handle.y - image_center.y) * 0.25,
        );
        let history_before = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 11, image_handle, 1.).unwrap();
        adapter.pointer_move(11, no_op_target).unwrap();
        assert_eq!(
            adapter.pointer_up(11, no_op_target).unwrap(),
            PointerPhaseOutcome::SelectionChanged(Some(image_id.clone())),
        );
        assert_eq!(adapter.history_depths(7), history_before);
        assert_eq!(
            adapter.snapshot(7).unwrap().images[0].rotation_degrees(),
            330.
        );

        adapter
            .set_primary_selected_locked(7, &image_id, true)
            .unwrap();
        assert_eq!(
            adapter.hover_image_handle(7, 0, image_handle, 1.).unwrap(),
            None
        );
    }

    #[test]
    fn aspect_locked_image_corner_resize_preserves_ratio_and_opposite_anchor() {
        let original = PdfRect::new(20., 30., 100., 50.).unwrap();
        let resized = resized_image_rect(
            original,
            ImageResizeHandle::NorthEast,
            point(120., 80.),
            point(160., 90.),
            0.,
            true,
        )
        .unwrap();
        assert_eq!(resized, PdfRect::new(20., 30., 140., 70.).unwrap());
        assert_eq!(
            resized.width / resized.height,
            original.width / original.height
        );
        assert!(
            resized_image_rect(
                original,
                ImageResizeHandle::East,
                point(120., 55.),
                point(160., 55.),
                0.,
                true,
            )
            .is_err()
        );
    }

    #[test]
    fn pending_image_preview_matches_committed_clamped_geometry_without_history() {
        let mut adapter = AnnotationAdapter::default();
        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        let asset = DecodedRgbaAsset::new(512, 384, vec![255; 512 * 384 * 4]).unwrap();
        let asset_id = asset.id().as_str().to_owned();
        adapter.set_tool(AnnotationTool::Image).unwrap();
        adapter.set_image_asset(asset);
        adapter
            .set_image_placement_page(612., 792., NATURAL_IMAGE_MAX_PAGE_FRACTION)
            .unwrap();

        let history_before = adapter.history_depths(7);
        let preview = adapter
            .pending_image_preview_at(7, 0, point(306., 396.))
            .unwrap()
            .expect("prepared Image hover produces a preview");
        assert_eq!(preview.document_id, 7);
        assert_eq!(preview.page_index, 0);
        assert_eq!(preview.asset_id, asset_id);
        assert_eq!(preview.opacity, IMAGE_PLACEMENT_PREVIEW_OPACITY);
        assert_eq!(adapter.history_depths(7), history_before);

        // Clicking one corner and then the other draws the image's box: the
        // preview fills it, keeping the image's 4:3 proportions.
        adapter.queue_next_annotation_id(MarkupId::new("image:preview-commit").unwrap());
        adapter
            .pointer_down(7, 0, 1, point(306., 396.), 4.)
            .unwrap();
        adapter.pointer_up(1, point(306., 396.)).unwrap();
        adapter.pointer_move(1, point(406., 496.)).unwrap();
        let drawn_preview = adapter
            .pending_image_preview_at(7, 0, point(306., 396.))
            .unwrap()
            .expect("an Image being drawn previews in its box");
        assert_eq!(
            drawn_preview.rect,
            PdfRect::new(306., 396., 100., 75.).unwrap()
        );
        assert_eq!(adapter.history_depths(7), history_before);
        assert!(matches!(
            adapter
                .pointer_down(7, 0, 1, point(406., 496.), 4.)
                .unwrap(),
            PointerPhaseOutcome::AnnotationCreated(_)
        ));
        assert_eq!(
            adapter.snapshot(7).unwrap().images[0].rect,
            drawn_preview.rect
        );
        assert!(
            adapter
                .pending_image_preview_at(7, 0, point(306., 396.))
                .unwrap()
                .is_none()
        );

        let mut edge = AnnotationAdapter::default();
        edge.set_tool(AnnotationTool::Image).unwrap();
        edge.set_image_asset(DecodedRgbaAsset::new(512, 384, vec![255; 512 * 384 * 4]).unwrap());
        edge.set_image_placement_page(612., 792., 0.45).unwrap();
        let edge_preview = edge
            .pending_image_preview_at(7, 0, point(0., 0.))
            .unwrap()
            .unwrap();
        assert_eq!(edge_preview.rect.x, 0.);
        assert_eq!(edge_preview.rect.y, 0.);

        let mut snapped = AnnotationAdapter::default();
        snapped.set_semantic_snap_settings(snapping_off).unwrap();
        snapped.set_tool(AnnotationTool::Rectangle).unwrap();
        snapped
            .pointer_down(7, 0, 1, point(200., 200.), 4.)
            .unwrap();
        snapped.pointer_move(1, point(300., 300.)).unwrap();
        snapped.pointer_up(1, point(300., 300.)).unwrap();
        snapped
            .set_semantic_snap_settings(
                snapping_off.with_source(SemanticSnapSource::Annotation, true),
            )
            .unwrap();
        snapped.set_tool(AnnotationTool::Image).unwrap();
        snapped.set_image_asset(DecodedRgbaAsset::new(512, 384, vec![255; 512 * 384 * 4]).unwrap());
        snapped
            .set_image_placement_page(612., 792., NATURAL_IMAGE_MAX_PAGE_FRACTION)
            .unwrap();
        let raw_point = point(202., 202.);
        snapped
            .pending_image_preview_at(7, 0, raw_point)
            .unwrap()
            .expect("Image hover stays at the raw pointer before click-time snapping");
        snapped.queue_next_annotation_id(MarkupId::new("image:snapped-commit").unwrap());
        snapped.pointer_down(7, 0, 2, raw_point, 4.).unwrap();
        snapped.pointer_up(2, raw_point).unwrap();
        assert!(matches!(
            snapped
                .pointer_down(7, 0, 2, point(140., 120.), 4.)
                .unwrap(),
            PointerPhaseOutcome::AnnotationCreated(_)
        ));
        // The first corner snaps to the rectangle's corner it was clicked by.
        let snapped_rect = snapped.snapshot(7).unwrap().images[0].rect;
        assert_eq!(
            point(
                snapped_rect.x + snapped_rect.width,
                snapped_rect.y + snapped_rect.height,
            ),
            point(200., 200.)
        );
    }

    #[test]
    fn ink_feedback_distinguishes_creation_single_move_and_group_move_without_preview_history() {
        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        let mut adapter = AnnotationAdapter::default();
        adapter.set_semantic_snap_settings(snapping_off).unwrap();

        adapter.set_tool(AnnotationTool::Pen).unwrap();
        adapter.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        adapter.pointer_move(1, point(70., 50.)).unwrap();
        adapter.pointer_move(1, point(120., 80.)).unwrap();
        let creation = adapter.document_scene(7, 0).pens.remove(0);
        assert!(creation.draft);
        assert_eq!(creation.feedback, SceneInteractionFeedback::Creation);
        assert!(!creation.feedback.chrome_visible());
        assert_eq!(adapter.history_depths(7), (0, 0));
        adapter.pointer_up(1, point(120., 80.)).unwrap();

        let first = adapter.snapshot(7).unwrap().pens[0].clone();
        let history_after_creation = adapter.history_depths(7);
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter.pointer_down(7, 0, 2, point(70., 50.), 5.).unwrap();
        adapter.pointer_move(2, point(80., 60.)).unwrap();
        let moving = adapter.document_scene(7, 0).pens.remove(0);
        assert_eq!(
            moving.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        assert_eq!(adapter.history_depths(7), history_after_creation);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().pens[0], first);
        assert_eq!(adapter.history_depths(7), history_after_creation);

        adapter
            .set_semantic_snap_settings(
                snapping_off.with_source(SemanticSnapSource::Annotation, true),
            )
            .unwrap();
        adapter.pointer_down(7, 0, 3, point(70., 50.), 5.).unwrap();
        adapter.pointer_move(3, point(80., 60.)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).pens[0].feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        assert_eq!(adapter.history_depths(7), history_after_creation);
        adapter.pointer_up(3, point(80., 60.)).unwrap();
        assert_eq!(adapter.history_depths(7), (history_after_creation.0 + 1, 0));

        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        adapter.set_tool(AnnotationTool::Pen).unwrap();
        adapter
            .pointer_down(7, 0, 4, point(200., 200.), 4.)
            .unwrap();
        adapter.pointer_move(4, point(230., 220.)).unwrap();
        adapter.pointer_move(4, point(260., 240.)).unwrap();
        adapter.pointer_up(4, point(260., 240.)).unwrap();
        let snapshot = adapter.snapshot(7).unwrap();
        let first_id = snapshot.pens[0].id.clone();
        let second_id = snapshot.pens[1].id.clone();
        let document = adapter.documents.get_mut(&7).unwrap();
        assert!(document.select(&first_id));
        document.toggle_selection(&second_id);
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let history_before_group = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 5, point(80., 60.), 5.).unwrap();
        adapter.pointer_move(5, point(90., 70.)).unwrap();
        let group = adapter.document_scene(7, 0);
        assert_eq!(group.pens.len(), 2);
        assert!(group.pens.iter().all(|pen| {
            pen.feedback
                == SceneInteractionFeedback::Move {
                    chrome_visible: true,
                }
        }));
        assert_eq!(adapter.history_depths(7), history_before_group);
        adapter.cancel(PointerCancelReason::ToolChanged).unwrap();
        assert_eq!(adapter.history_depths(7), history_before_group);
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
        cloud.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        cloud.pointer_up(1, point(20., 20.)).unwrap();
        cloud.pointer_down(7, 0, 1, point(100., 80.), 4.).unwrap();
        let cloud = &cloud.snapshot(7).unwrap().clouds[0];
        assert_rectangle_properties(&cloud.appearance, Some("#abcdef"));
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
            .pointer_down(7, 0, 1, point(100., 80.), 4.)
            .unwrap();
        let cloud_plus = &cloud_plus.snapshot(7).unwrap().cloud_pluses[0];
        assert_eq!(cloud_plus.appearance.cloud().stroke_color(), "#336699");
        assert_eq!(cloud_plus.appearance.cloud().stroke_width_pt(), 1.0);
        assert_eq!(cloud_plus.appearance.cloud().fill_color(), Some("#abcdef"));
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
        cloud.pointer_down(7, 0, 1, point(20., 20.), 4.).unwrap();
        cloud.pointer_up(1, point(20., 20.)).unwrap();
        cloud.pointer_move(1, point(100., 80.)).unwrap();
        let cloud_preview = cloud.document_scene(7, 0).clouds.remove(0);
        assert!(cloud_preview.draft);
        assert_eq!(cloud_preview.border_effect_intensity, 3.0);
        assert_eq!(cloud.history_depths(7), cloud_history_before);
        cloud.pointer_down(7, 0, 1, point(100., 80.), 4.).unwrap();
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
        cloud_plus.pointer_move(1, point(100., 80.)).unwrap();
        let cloud_plus_preview = cloud_plus.document_scene(7, 0).cloud_pluses.remove(0);
        assert!(cloud_plus_preview.draft);
        assert_eq!(cloud_plus_preview.border_effect_intensity, 3.0);
        assert_eq!(cloud_plus.history_depths(7), cloud_plus_history_before);
        cloud_plus
            .pointer_down(7, 0, 1, point(100., 80.), 4.)
            .unwrap();
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

    #[test]
    fn installed_page_grid_drives_current_page_creation_and_ignores_target_toggles() {
        use crate::semantic_snapping::{
            PageGridDefinition, PageGridKind, PageGridSource, SemanticSnapRole, SemanticSnapSource,
            SemanticSnapTarget,
        };

        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Line).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Annotation, false)
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, true)
                    .with_source(SemanticSnapSource::ConstructionGrid, false)
                    .with_target(SemanticSnapTarget::Intersection, false)
                    .with_target(SemanticSnapTarget::Nearest, false),
            )
            .unwrap();
        adapter.set_semantic_snap_page_grid(
            7,
            0,
            Some(
                PageGridDefinition::new(
                    PageGridKind::Rectangular,
                    point(0., 0.),
                    10.,
                    100.,
                    100.,
                    0.,
                    PageGridSource::Generated,
                )
                .unwrap(),
            ),
        );

        adapter.pointer_down(7, 0, 1, point(9.6, 10.3), 2.).unwrap();
        adapter.pointer_move(1, point(20.3, 19.7)).unwrap();
        let line = adapter.document_scene(7, 0).straight_lines.remove(0);
        assert_eq!(line.start, point(10., 10.));
        assert_eq!(line.end, point(20., 20.));
        assert_eq!(
            adapter
                .semantic_snap_decision()
                .map(|decision| decision.role),
            Some(SemanticSnapRole::GridPoint)
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        adapter.pointer_down(7, 1, 2, point(9.6, 10.3), 2.).unwrap();
        adapter.pointer_move(2, point(20.3, 19.7)).unwrap();
        let wrong_page = adapter.document_scene(7, 1).straight_lines.remove(0);
        assert_eq!(wrong_page.start, point(9.6, 10.3));
        assert_eq!(wrong_page.end, point(20.3, 19.7));
        assert!(adapter.semantic_snap_decision().is_none());
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        adapter
            .set_semantic_snap_settings(
                adapter
                    .semantic_snap_settings()
                    .with_source(SemanticSnapSource::PageGrid, false),
            )
            .unwrap();
        adapter.pointer_down(7, 0, 3, point(9.6, 10.3), 2.).unwrap();
        adapter.pointer_move(3, point(20.3, 19.7)).unwrap();
        let disabled = adapter.document_scene(7, 0).straight_lines.remove(0);
        assert_eq!(disabled.start, point(9.6, 10.3));
        assert_eq!(disabled.end, point(20.3, 19.7));
    }

    #[test]
    fn installed_pdf_content_drives_real_line_creation_and_respects_source_toggle() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Line).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Annotation, false)
                    .with_source(SemanticSnapSource::Content, true)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();
        adapter
            .set_semantic_snap_page_content(
                7,
                PageSnapGeometry {
                    page_index: 0,
                    primitives: vec![crate::pdf_content_geometry::PdfContentPrimitive::Line {
                        start: crate::pdf_content_geometry::PdfPoint { x: 10., y: 10. },
                        end: crate::pdf_content_geometry::PdfPoint { x: 30., y: 10. },
                    }],
                },
            )
            .unwrap();

        adapter.pointer_down(7, 0, 1, point(9.7, 10.2), 2.).unwrap();
        adapter.pointer_move(1, point(20.2, 10.1)).unwrap();
        let snapped = adapter.document_scene(7, 0).straight_lines.remove(0);
        assert_eq!(snapped.start, point(10., 10.));
        assert_eq!(snapped.end, point(20., 10.));
        assert_eq!(
            adapter
                .semantic_snap_decision()
                .map(|decision| decision.role),
            Some(crate::semantic_snapping::SemanticSnapRole::Midpoint)
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        adapter
            .set_semantic_snap_settings(
                adapter
                    .semantic_snap_settings()
                    .with_source(SemanticSnapSource::Content, false),
            )
            .unwrap();
        adapter.pointer_down(7, 0, 2, point(9.7, 10.2), 2.).unwrap();
        adapter.pointer_move(2, point(20.2, 10.1)).unwrap();
        let raw = adapter.document_scene(7, 0).straight_lines.remove(0);
        assert_eq!(raw.start, point(9.7, 10.2));
        assert_eq!(raw.end, point(20.2, 10.1));
    }

    #[test]
    fn real_line_creation_acquires_revisits_and_uses_content_tracking_paths() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Line).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Annotation, false)
                    .with_source(SemanticSnapSource::Content, true)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();
        adapter
            .set_semantic_snap_page_content(
                7,
                PageSnapGeometry {
                    page_index: 0,
                    primitives: vec![crate::pdf_content_geometry::PdfContentPrimitive::Line {
                        start: crate::pdf_content_geometry::PdfPoint { x: 10., y: 10. },
                        end: crate::pdf_content_geometry::PdfPoint { x: 30., y: 10. },
                    }],
                },
            )
            .unwrap();

        adapter.pointer_down(7, 0, 1, point(9.7, 10.2), 2.).unwrap();
        adapter.pointer_move(1, point(80., 12.)).unwrap();
        let tracked = adapter.document_scene(7, 0).straight_lines.remove(0);
        assert_eq!(tracked.start, point(10., 10.));
        assert_eq!(tracked.end, point(80., 10.));
        let tracking = adapter.object_snap_tracking_result().unwrap();
        assert_eq!(tracking.point, point(80., 10.));
        assert_eq!(tracking.guides.len(), 1);
        assert_eq!(tracking.guides[0].axis, OrthogonalAxis::Horizontal);
        assert_eq!(tracking.guides[0].source, SemanticSnapSource::Content);

        // Leaving and revisiting the acquired point removes it, matching the
        // stable Electron temporary-tracking contract.
        adapter.pointer_move(1, point(9.8, 10.1)).unwrap();
        adapter.pointer_move(1, point(80., 12.)).unwrap();
        let untracked = adapter.document_scene(7, 0).straight_lines.remove(0);
        assert_eq!(untracked.end, point(80., 12.));
        assert!(adapter.object_snap_tracking_result().is_none());
    }

    #[test]
    fn real_rectangle_move_snaps_equal_spacing_without_source_or_guide_dependency() {
        use crate::semantic_snapping::{
            EqualSpacingPlacement, RelationshipSnapGuide, SemanticSnapGuideType,
        };

        let mut adapter = AnnotationAdapter::default();
        let before_id = MarkupId::new("rectangle:spacing-before").unwrap();
        let moving_id = MarkupId::new("rectangle:spacing-moving").unwrap();
        let after_id = MarkupId::new("rectangle:spacing-after").unwrap();
        let document = adapter.documents.entry(7).or_default();
        for (id, x) in [
            (before_id.clone(), 10.),
            (moving_id.clone(), 40.),
            (after_id.clone(), 100.),
        ] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                    RectangleAnnotation {
                        id,
                        page_index: 0,
                        rect: PdfRect::new(x, 20., 20., 20.).unwrap(),
                        rotation_degrees: 0.,
                        appearance: RectangleAppearance::default(),
                        locked: false,
                    },
                )))
                .unwrap();
        }
        assert!(document.select(&moving_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Annotation, false)
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false)
                    .with_guides_enabled(false)
                    .with_guide(SemanticSnapGuideType::EqualSpacing, false),
            )
            .unwrap();

        // At this zoom the small rectangle has a move band outside its resize reach.
        adapter.set_observed_pixels_per_point(4.).unwrap();
        let history_before = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 8, point(50., 24.5), 1.).unwrap();
        adapter.pointer_move(8, point(66., 24.5)).unwrap();
        let preview = adapter.document_scene(7, 0);
        assert_eq!(
            preview
                .rectangles
                .iter()
                .find(|rectangle| rectangle.id == moving_id)
                .unwrap()
                .rect,
            PdfRect::new(55., 20., 20., 20.).unwrap()
        );
        assert_eq!(adapter.history_depths(7), history_before);
        assert!(adapter.semantic_snap_decision().is_none());
        assert!(adapter.object_snap_tracking_result().is_none());
        assert!(
            adapter
                .relationship_snap_guides()
                .iter()
                .any(|guide| matches!(
                    guide,
                    RelationshipSnapGuide::EqualSpacing {
                        placement: EqualSpacingPlacement::Between,
                        before,
                        after,
                        ..
                    } if before.owner_id == before_id && after.owner_id == after_id
                ))
        );

        adapter.pointer_up(8, point(66., 24.5)).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().rectangles[1].rect,
            PdfRect::new(55., 20., 20., 20.).unwrap()
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        assert!(adapter.relationship_snap_guides().is_empty());
        adapter.undo(7).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().rectangles[1].rect,
            PdfRect::new(40., 20., 20., 20.).unwrap()
        );
    }

    #[test]
    fn ink_move_uses_exact_path_bounds_for_equal_spacing_and_undo() {
        use crate::semantic_snapping::{EqualSpacingPlacement, RelationshipSnapGuide};

        let mut adapter = AnnotationAdapter::default();
        let before_id = MarkupId::new("ink-spacing:before").unwrap();
        let ink_id = MarkupId::new("ink-spacing:moving").unwrap();
        let after_id = MarkupId::new("ink-spacing:after").unwrap();
        let document = adapter.documents.entry(7).or_default();
        for (id, x) in [(before_id.clone(), 10.), (after_id.clone(), 100.)] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                    RectangleAnnotation {
                        id,
                        page_index: 0,
                        rect: PdfRect::new(x, 10., 20., 20.).unwrap(),
                        rotation_degrees: 0.,
                        appearance: RectangleAppearance::default(),
                        locked: false,
                    },
                )))
                .unwrap();
        }
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Pen(
                PenAnnotation::new(
                    ink_id.clone(),
                    0,
                    vec![point(40., 20.), point(60., 20.)],
                    PenAppearance::new("#ff0000", 2., 1.).unwrap(),
                )
                .unwrap(),
            )))
            .unwrap();
        assert!(document.select(&ink_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Annotation, false)
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();

        let before = adapter.snapshot(7).unwrap();
        let history_before = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 3, point(50., 20.), 2.).unwrap();
        adapter.pointer_move(3, point(66., 20.)).unwrap();
        let preview = adapter.document_scene(7, 0).pens.pop().unwrap();
        assert_eq!(preview.paths[0], vec![point(55., 20.), point(75., 20.)]);
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(adapter.history_depths(7), history_before);
        assert!(
            adapter
                .relationship_snap_guides()
                .iter()
                .any(|guide| matches!(
                    guide,
                    RelationshipSnapGuide::EqualSpacing {
                        placement: EqualSpacingPlacement::Between,
                        before,
                        after,
                        ..
                    } if before.owner_id == before_id && after.owner_id == after_id
                ))
        );

        adapter.pointer_up(3, point(66., 20.)).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().pens[0].paths().next().unwrap(),
            preview.paths[0]
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().pens, before.pens);
    }

    #[test]
    fn rectangle_and_constrained_ellipse_placement_snap_equal_size_in_both_directions() {
        use crate::semantic_snapping::{RelationshipSnapGuide, SemanticSnapGuideType};

        let mut adapter = AnnotationAdapter::default();
        let reference_id = MarkupId::new("placement-size:reference").unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: reference_id.clone(),
                    page_index: 0,
                    rect: PdfRect::new(10., 10., 20., 30.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Annotation, false)
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false)
                    .with_guides_enabled(false)
                    .with_guide(SemanticSnapGuideType::EqualSize, false),
            )
            .unwrap();

        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        adapter.queue_next_annotation_id(MarkupId::new("placement-size:rectangle").unwrap());
        let history_before = adapter.history_depths(7);
        adapter
            .pointer_down(7, 0, 1, point(100., 100.), 1.)
            .unwrap();
        adapter.pointer_move(1, point(119., 129.)).unwrap();
        let rectangle_preview = adapter.document_scene(7, 0).rectangles.pop().unwrap();
        assert_eq!(
            rectangle_preview.rect,
            PdfRect::new(100., 100., 20., 30.).unwrap()
        );
        assert_eq!(adapter.history_depths(7), history_before);
        assert_eq!(adapter.relationship_snap_guides().len(), 2);
        assert!(
            adapter
                .relationship_snap_guides()
                .iter()
                .all(|guide| matches!(
                    guide,
                    RelationshipSnapGuide::EqualSize { reference, .. }
                        if reference.owner_id == reference_id
                ))
        );
        adapter.pointer_up(1, point(119., 129.)).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().rectangles[1].rect,
            rectangle_preview.rect
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));

        adapter.set_tool(AnnotationTool::Ellipse).unwrap();
        adapter.queue_next_annotation_id(MarkupId::new("placement-size:ellipse").unwrap());
        let history_before_ellipse = adapter.history_depths(7);
        adapter
            .pointer_down(7, 0, 2, point(200., 200.), 1.)
            .unwrap();
        adapter
            .pointer_move_with_constraint(2, point(171., 169.), true)
            .unwrap();
        let ellipse_preview = adapter.document_scene(7, 0).ellipses.pop().unwrap();
        assert_eq!(
            ellipse_preview.rect,
            PdfRect::new(170., 170., 30., 30.).unwrap()
        );
        assert_eq!(adapter.history_depths(7), history_before_ellipse);
        assert_eq!(adapter.relationship_snap_guides().len(), 1);
        adapter
            .pointer_up_with_constraint(2, point(171., 169.), true)
            .unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().ellipses[0].rect,
            ellipse_preview.rect
        );
        assert_eq!(adapter.history_depths(7), (history_before_ellipse.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert!(adapter.snapshot(7).unwrap().ellipses.is_empty());
    }

    #[test]
    fn axis_aligned_ellipse_resize_snaps_only_the_active_handle_axes() {
        use crate::semantic_snapping::{RelationshipSnapGuide, SemanticSnapGuideType};

        let mut adapter = AnnotationAdapter::default();
        let reference_id = MarkupId::new("resize-size:reference").unwrap();
        let ellipse_id = MarkupId::new("resize-size:ellipse").unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: reference_id.clone(),
                    page_index: 0,
                    rect: PdfRect::new(100., 100., 50., 70.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Ellipse(
                EllipseAnnotation::new(
                    ellipse_id.clone(),
                    0,
                    PdfRect::new(40., 100., 40., 40.).unwrap(),
                    RectangleAppearance::default(),
                )
                .unwrap(),
            )))
            .unwrap();
        assert!(document.select(&ellipse_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Annotation, false)
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false)
                    .with_guides_enabled(false)
                    .with_guide(SemanticSnapGuideType::EqualSize, false),
            )
            .unwrap();

        let history_before = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 1, point(80., 120.), 1.).unwrap();
        adapter.pointer_move(1, point(89., 120.)).unwrap();
        let preview = adapter.document_scene(7, 0).ellipses.pop().unwrap();
        assert_eq!(preview.rect, PdfRect::new(40., 100., 50., 40.).unwrap());
        assert_eq!(adapter.history_depths(7), history_before);
        assert_eq!(adapter.relationship_snap_guides().len(), 1);
        assert!(matches!(
            &adapter.relationship_snap_guides()[0],
            RelationshipSnapGuide::EqualSize {
                axis: OrthogonalAxis::Horizontal,
                moving,
                reference,
            } if *moving == preview.rect && reference.owner_id == reference_id
        ));

        adapter.pointer_up(1, point(89., 120.)).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().ellipses[0].rect, preview.rect);
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        assert!(adapter.relationship_snap_guides().is_empty());
        adapter.undo(7).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().ellipses[0].rect,
            PdfRect::new(40., 100., 40., 40.).unwrap()
        );

        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::EditAnnotation {
                id: ellipse_id.clone(),
                edit: AnnotationEdit::SetEllipseRotation(45.),
            })
            .unwrap();
        let rotated = adapter.snapshot(7).unwrap().ellipses[0].clone();
        let rotated_handle = ellipse_resize_handle_point(&rotated, RectangleResizeHandle::East);
        adapter.pointer_down(7, 0, 2, rotated_handle, 1.).unwrap();
        adapter
            .pointer_move(2, point(rotated_handle.x + 9., rotated_handle.y))
            .unwrap();
        assert!(adapter.relationship_snap_guides().is_empty());
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
    }

    #[test]
    fn straight_line_endpoint_preview_separates_creation_chrome_and_real_snapping() {
        use crate::semantic_snapping::SemanticSnapRole;

        let mut adapter = AnnotationAdapter::default();
        let primary_id = MarkupId::new("line:manipulation-primary").unwrap();
        let target_id = MarkupId::new("line:manipulation-target").unwrap();
        let primary = StraightLineAnnotation::new(
            primary_id.clone(),
            0,
            PdfPoint::new(10., 10.).unwrap(),
            PdfPoint::new(100., 10.).unwrap(),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let target = StraightLineAnnotation::new(
            target_id.clone(),
            0,
            PdfPoint::new(130., 30.).unwrap(),
            PdfPoint::new(180., 30.).unwrap(),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(primary),
            ))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(target),
            ))
            .unwrap();
        assert!(document.select(&primary_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();

        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        adapter.set_semantic_snap_settings(snapping_off).unwrap();
        adapter
            .pointer_down(7, 0, 41, PdfPoint::new(100., 10.).unwrap(), 4.)
            .unwrap();
        adapter
            .pointer_move(41, PdfPoint::new(115., 20.).unwrap())
            .unwrap();
        let preview = adapter.document_scene(7, 0);
        let primary = preview
            .straight_lines
            .iter()
            .find(|line| line.id == primary_id)
            .unwrap();
        assert_eq!(
            primary.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 1,
            }
        );
        assert_eq!(primary.end, PdfPoint::new(115., 20.).unwrap());
        assert!(adapter.semantic_snap_decision().is_none());
        adapter.cancel(PointerCancelReason::ToolChanged).unwrap();
        let stable = adapter.document_scene(7, 0);
        let primary = stable
            .straight_lines
            .iter()
            .find(|line| line.id == primary_id)
            .unwrap();
        assert_eq!(primary.feedback, SceneInteractionFeedback::Normal);
        assert_eq!(primary.end, PdfPoint::new(100., 10.).unwrap());

        adapter
            .set_semantic_snap_settings(
                snapping_off.with_source(SemanticSnapSource::Annotation, true),
            )
            .unwrap();
        let history_before = adapter.history_depths(7);
        adapter
            .pointer_down(7, 0, 42, PdfPoint::new(100., 10.).unwrap(), 4.)
            .unwrap();
        adapter
            .pointer_move(42, PdfPoint::new(129., 31.).unwrap())
            .unwrap();
        let snapped = adapter.document_scene(7, 0);
        let primary = snapped
            .straight_lines
            .iter()
            .find(|line| line.id == primary_id)
            .unwrap();
        assert_eq!(
            primary.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 1,
            }
        );
        assert_eq!(primary.end, PdfPoint::new(130., 30.).unwrap());
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        assert_eq!(adapter.history_depths(7), history_before);
        adapter
            .pointer_up(42, PdfPoint::new(129., 31.).unwrap())
            .unwrap();
        let committed = adapter.snapshot(7).unwrap();
        assert_eq!(
            committed
                .straight_lines
                .iter()
                .find(|line| line.id == primary_id)
                .unwrap()
                .end,
            PdfPoint::new(130., 30.).unwrap()
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));

        let mut length_adapter = AnnotationAdapter::default();
        let length_id = MarkupId::new("length:manipulation-primary").unwrap();
        let length_target_id = MarkupId::new("length:manipulation-target").unwrap();
        let length = LengthAnnotation::new(
            length_id.clone(),
            0,
            PdfPoint::new(20., 60.).unwrap(),
            PdfPoint::new(100., 60.).unwrap(),
            LengthCalibration::from_scale(72., 1., "m", 2, false).unwrap(),
        )
        .unwrap();
        let target = StraightLineAnnotation::new(
            length_target_id.clone(),
            0,
            PdfPoint::new(140., 80.).unwrap(),
            PdfPoint::new(180., 80.).unwrap(),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let document = length_adapter.documents.entry(9).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
                length,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(target),
            ))
            .unwrap();
        assert!(document.select(&length_id));
        length_adapter.set_tool(AnnotationTool::Select).unwrap();
        length_adapter
            .set_semantic_snap_settings(
                snapping_off.with_source(SemanticSnapSource::Annotation, true),
            )
            .unwrap();
        length_adapter
            .pointer_down(9, 0, 43, PdfPoint::new(100., 60.).unwrap(), 4.)
            .unwrap();
        length_adapter
            .pointer_move(43, PdfPoint::new(139., 79.).unwrap())
            .unwrap();
        let scene = length_adapter.document_scene(9, 0);
        let length = scene
            .lengths
            .iter()
            .find(|item| item.id == length_id)
            .unwrap();
        assert_eq!(
            length.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 1,
            }
        );
        assert_eq!(length.end, PdfPoint::new(140., 80.).unwrap());
        let decision = length_adapter.semantic_snap_decision().unwrap();
        assert_eq!(decision.owner_id.as_ref(), Some(&length_target_id));
        assert_eq!(decision.role, SemanticSnapRole::Endpoint);
    }

    #[test]
    fn length_caption_anchor_snaps_body_move_without_self_target_or_preview_history() {
        use crate::semantic_snapping::{SemanticSnapRole, SemanticSnapSource};

        let mut adapter = AnnotationAdapter::default();
        let length_id = MarkupId::new("length:caption-snap-primary").unwrap();
        let target_id = MarkupId::new("length:caption-snap-target").unwrap();
        let length = LengthAnnotation::new(
            length_id.clone(),
            0,
            point(10., 50.),
            point(100., 50.),
            LengthCalibration::from_scale(72., 1., "m", 2, false).unwrap(),
        )
        .unwrap();
        let target = StraightLineAnnotation::new(
            target_id.clone(),
            0,
            point(250., 80.),
            point(280., 80.),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
                length,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(target),
            ))
            .unwrap();
        assert!(document.select(&length_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();
        let supplement = [(
            length_id.clone(),
            vec![
                point(200., 40.),
                point(220., 40.),
                point(220., 50.),
                point(200., 50.),
            ],
        )]
        .into_iter()
        .collect::<AnnotationSelectionSupplement>();
        let before = adapter.snapshot(7).unwrap();
        let history_before = adapter.history_depths(7);
        let caption_center = point(210., 45.);

        assert_eq!(
            adapter
                .pointer_down_with_viewport_input_and_selection_paths(
                    7,
                    0,
                    81,
                    0,
                    caption_center,
                    SelectionPoint::new(caption_center.x, caption_center.y),
                    1.,
                    PointerInputModifiers::default(),
                    &supplement,
                )
                .unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(81, point(239., 74.)).unwrap();
        let preview = adapter.document_scene(7, 0);
        let preview_length = preview
            .lengths
            .iter()
            .find(|annotation| annotation.id == length_id)
            .unwrap();
        assert_eq!(preview_length.start, point(40., 80.));
        assert_eq!(preview_length.end, point(130., 80.));
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(adapter.history_depths(7), history_before);
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);

        assert_eq!(
            adapter.pointer_up(81, point(239., 74.)).unwrap(),
            PointerPhaseOutcome::AnnotationEdited(length_id.clone())
        );
        let committed = adapter.snapshot(7).unwrap();
        assert_eq!(committed.lengths[0].start, point(40., 80.));
        assert_eq!(committed.lengths[0].end, point(130., 80.));
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().lengths, before.lengths);

        let group_line_id = MarkupId::new("length:caption-snap-group-line").unwrap();
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        group_line_id.clone(),
                        0,
                        point(10., 10.),
                        point(100., 10.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        let document = adapter.documents.get_mut(&7).unwrap();
        assert!(document.select(&group_line_id));
        document.toggle_selection(&length_id);
        let group_before = adapter.snapshot(7).unwrap();
        let group_history_before = adapter.history_depths(7);
        assert_eq!(
            adapter
                .pointer_down_with_viewport_input_and_selection_paths(
                    7,
                    0,
                    82,
                    0,
                    point(50., 10.),
                    SelectionPoint::new(50., 10.),
                    1.,
                    PointerInputModifiers::default(),
                    &supplement,
                )
                .unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(82, point(79., 39.)).unwrap();
        let group_preview = adapter.document_scene(7, 0);
        assert_eq!(
            group_preview
                .straight_lines
                .iter()
                .find(|annotation| annotation.id == group_line_id)
                .unwrap()
                .start,
            point(40., 40.)
        );
        assert_eq!(
            group_preview
                .lengths
                .iter()
                .find(|annotation| annotation.id == length_id)
                .unwrap()
                .start,
            point(40., 80.)
        );
        assert_eq!(adapter.snapshot(7).unwrap(), group_before);
        assert_eq!(adapter.history_depths(7), group_history_before);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), group_before);
    }

    #[test]
    fn rectangular_shape_body_moves_snap_non_pointer_anchors_without_preview_history() {
        use crate::semantic_snapping::{SemanticSnapRole, SemanticSnapSource};

        fn snapping_settings() -> SemanticSnapSettings {
            SemanticSnapSettings::default()
                .with_source(SemanticSnapSource::Content, false)
                .with_source(SemanticSnapSource::PageGrid, false)
                .with_source(SemanticSnapSource::ConstructionGrid, false)
        }

        fn target(id: MarkupId, start: PdfPoint) -> Annotation {
            Annotation::StraightLine(
                StraightLineAnnotation::new(
                    id,
                    0,
                    start,
                    point(start.x + 30., start.y),
                    LineKind::Line,
                    StraightLineAppearance::default_for(LineKind::Line),
                )
                .unwrap(),
            )
        }

        let rectangle_id = MarkupId::new("rectangle:body-snap").unwrap();
        let rectangle_target_id = MarkupId::new("rectangle:body-snap-target").unwrap();
        let rectangle = RectangleAnnotation {
            id: rectangle_id.clone(),
            page_index: 0,
            rect: PdfRect::new(10., 10., 40., 40.).unwrap(),
            rotation_degrees: 0.,
            appearance: RectangleAppearance::default(),
            locked: false,
        };
        let mut rectangle_adapter = AnnotationAdapter::default();
        let document = rectangle_adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                rectangle,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(target(
                rectangle_target_id.clone(),
                point(60., 60.),
            )))
            .unwrap();
        assert!(document.select(&rectangle_id));
        rectangle_adapter.set_tool(AnnotationTool::Select).unwrap();
        rectangle_adapter
            .set_semantic_snap_settings(snapping_settings())
            .unwrap();
        let rectangle_before = rectangle_adapter.snapshot(7).unwrap();
        let rectangle_history = rectangle_adapter.history_depths(7);
        rectangle_adapter
            .pointer_down(7, 0, 91, point(20., 16.), 2.)
            .unwrap();
        rectangle_adapter.pointer_move(91, point(29., 25.)).unwrap();
        assert_eq!(
            rectangle_adapter.document_scene(7, 0).rectangles[0].rect,
            PdfRect::new(20., 20., 40., 40.).unwrap()
        );
        assert_eq!(rectangle_adapter.snapshot(7).unwrap(), rectangle_before);
        assert_eq!(rectangle_adapter.history_depths(7), rectangle_history);
        assert_snap_evidence_references(
            &rectangle_adapter,
            &rectangle_target_id,
            SemanticSnapRole::Endpoint,
        );
        rectangle_adapter
            .cancel(PointerCancelReason::CaptureLost)
            .unwrap();
        assert_eq!(rectangle_adapter.snapshot(7).unwrap(), rectangle_before);
        rectangle_adapter
            .pointer_down(7, 0, 92, point(20., 16.), 2.)
            .unwrap();
        rectangle_adapter.pointer_move(92, point(29., 25.)).unwrap();
        rectangle_adapter.pointer_up(92, point(29., 25.)).unwrap();
        assert_eq!(
            rectangle_adapter.history_depths(7),
            (rectangle_history.0 + 1, 0)
        );
        rectangle_adapter.undo(7).unwrap();
        assert_eq!(
            rectangle_adapter.snapshot(7).unwrap().rectangles,
            rectangle_before.rectangles
        );

        let ellipse_id = MarkupId::new("ellipse:body-snap").unwrap();
        let ellipse_target_id = MarkupId::new("ellipse:body-snap-target").unwrap();
        let ellipse = EllipseAnnotation::new(
            ellipse_id.clone(),
            0,
            PdfRect::new(10., 10., 100., 60.).unwrap(),
            RectangleAppearance::default(),
        )
        .unwrap();
        let mut ellipse_adapter = AnnotationAdapter::default();
        let document = ellipse_adapter.documents.entry(8).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Ellipse(
                ellipse,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(target(
                ellipse_target_id.clone(),
                point(120., 80.),
            )))
            .unwrap();
        assert!(document.select(&ellipse_id));
        ellipse_adapter.set_tool(AnnotationTool::Select).unwrap();
        ellipse_adapter
            .set_semantic_snap_settings(snapping_settings())
            .unwrap();
        let ellipse_before = ellipse_adapter.snapshot(8).unwrap();
        let ellipse_history = ellipse_adapter.history_depths(8);
        // The move band just inside the curve; the curve itself resizes.
        ellipse_adapter
            .pointer_down(8, 0, 93, point(20., 40.), 4.)
            .unwrap();
        ellipse_adapter.pointer_move(93, point(29., 49.)).unwrap();
        assert_eq!(
            ellipse_adapter.document_scene(8, 0).ellipses[0].rect,
            PdfRect::new(20., 20., 100., 60.).unwrap()
        );
        assert_eq!(ellipse_adapter.snapshot(8).unwrap(), ellipse_before);
        assert_eq!(ellipse_adapter.history_depths(8), ellipse_history);
        assert_snap_evidence_references(
            &ellipse_adapter,
            &ellipse_target_id,
            SemanticSnapRole::Endpoint,
        );
        ellipse_adapter.pointer_up(93, point(29., 49.)).unwrap();
        assert_eq!(
            ellipse_adapter.history_depths(8),
            (ellipse_history.0 + 1, 0)
        );
        ellipse_adapter.undo(8).unwrap();
        assert_eq!(
            ellipse_adapter.snapshot(8).unwrap().ellipses,
            ellipse_before.ellipses
        );

        let redact_id = MarkupId::new("redact:body-snap").unwrap();
        let redact_target_id = MarkupId::new("redact:body-snap-target").unwrap();
        let redact_appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
            .unwrap()
            .with_fill_opacity(0.35)
            .unwrap();
        let redact = RedactAnnotation::new(
            redact_id.clone(),
            0,
            PdfRect::new(10., 10., 40., 40.).unwrap(),
            "#000000",
            None::<String>,
            redact_appearance,
        )
        .unwrap();
        let mut redact_adapter = AnnotationAdapter::default();
        let document = redact_adapter.documents.entry(9).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Redact(
                redact,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(target(
                redact_target_id.clone(),
                point(60., 60.),
            )))
            .unwrap();
        assert!(document.select(&redact_id));
        redact_adapter.set_tool(AnnotationTool::Select).unwrap();
        redact_adapter
            .set_semantic_snap_settings(snapping_settings())
            .unwrap();
        let redact_before = redact_adapter.snapshot(9).unwrap();
        let redact_history = redact_adapter.history_depths(9);
        redact_adapter
            .pointer_down(9, 0, 94, point(30., 30.), 2.)
            .unwrap();
        redact_adapter.pointer_move(94, point(39., 39.)).unwrap();
        assert_eq!(
            redact_adapter.document_scene(9, 0).redacts[0].rect,
            PdfRect::new(20., 20., 40., 40.).unwrap()
        );
        assert_eq!(redact_adapter.snapshot(9).unwrap(), redact_before);
        assert_eq!(redact_adapter.history_depths(9), redact_history);
        assert_snap_evidence_references(
            &redact_adapter,
            &redact_target_id,
            SemanticSnapRole::Endpoint,
        );
        redact_adapter.pointer_up(94, point(39., 39.)).unwrap();
        assert_eq!(redact_adapter.history_depths(9), (redact_history.0 + 1, 0));
        redact_adapter.undo(9).unwrap();
        assert_eq!(
            redact_adapter.snapshot(9).unwrap().redacts,
            redact_before.redacts
        );
    }

    #[test]
    fn rotated_snapshot_body_and_resize_snap_without_preview_history() {
        use crate::semantic_snapping::{SemanticSnapRole, SemanticSnapSource};

        let snapshot_id = MarkupId::new("snapshot:rotated-body-snap").unwrap();
        let target_id = MarkupId::new("snapshot:rotated-body-snap-target").unwrap();
        let snapshot = SnapshotAnnotation::new(
            snapshot_id.clone(),
            0,
            PdfRect::new(0., 0., 100., 50.).unwrap(),
            DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap(),
            1.,
        )
        .unwrap()
        .with_rotation_degrees(90.)
        .unwrap();
        let target = StraightLineAnnotation::new(
            target_id.clone(),
            0,
            point(100., 100.),
            point(130., 100.),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let mut adapter = AnnotationAdapter::default();
        let document = adapter.documents.entry(7).or_default();
        document
            .load_imported_annotations(
                vec![
                    Annotation::Snapshot(snapshot),
                    Annotation::StraightLine(target),
                ],
                Vec::new(),
            )
            .unwrap();
        assert!(document.select(&snapshot_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();

        let before = adapter.snapshot(7).unwrap();
        let history_before = adapter.history_depths(7);
        assert_eq!(
            adapter.pointer_down(7, 0, 95, point(50., 25.), 2.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(95, point(75.4, 150.4)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).snapshots[0].rect,
            PdfRect::new(25., 125., 100., 50.).unwrap()
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(adapter.history_depths(7), history_before);
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        adapter.pointer_down(7, 0, 96, point(50., 25.), 2.).unwrap();
        adapter.pointer_move(96, point(75.4, 150.4)).unwrap();
        adapter.pointer_up(96, point(75.4, 150.4)).unwrap();
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().snapshots, before.snapshots);

        let snapshot = adapter.snapshot(7).unwrap().snapshots[0].clone();
        let resize = snapshot_resize_handle_point(&snapshot, RectangleResizeHandle::NorthWest);
        let before_resize = adapter.snapshot(7).unwrap();
        let history_before_resize = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 97, resize, 2.).unwrap();
        adapter.pointer_move(97, point(100.4, 100.4)).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before_resize);
        assert_eq!(adapter.history_depths(7), history_before_resize);
        let decision = adapter.semantic_snap_decision().unwrap();
        assert_eq!(decision.point, point(100., 100.));
        assert_eq!(decision.owner_id.as_ref(), Some(&target_id));
        adapter.pointer_up(97, point(100.4, 100.4)).unwrap();
        assert_eq!(adapter.history_depths(7), (history_before_resize.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().snapshots,
            before_resize.snapshots
        );
    }

    #[test]
    fn rectangular_shape_group_snap_context_includes_only_selected_unlocked_page_geometry() {
        let rectangle_id = MarkupId::new("rectangle:group-snap").unwrap();
        let ellipse_id = MarkupId::new("ellipse:group-snap").unwrap();
        let redact_id = MarkupId::new("redact:group-snap").unwrap();
        let locked_id = MarkupId::new("ellipse:group-snap-locked").unwrap();
        let other_page_id = MarkupId::new("redact:group-snap-other-page").unwrap();
        let redact_appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
            .unwrap()
            .with_fill_opacity(0.35)
            .unwrap();
        let mut locked = EllipseAnnotation::new(
            locked_id.clone(),
            0,
            PdfRect::new(100., 10., 20., 20.).unwrap(),
            RectangleAppearance::default(),
        )
        .unwrap();
        locked.locked = true;
        let mut document = AnnotationDocument::default();
        for annotation in [
            Annotation::Rectangle(RectangleAnnotation {
                id: rectangle_id.clone(),
                page_index: 0,
                rect: PdfRect::new(10., 10., 20., 20.).unwrap(),
                rotation_degrees: 0.,
                appearance: RectangleAppearance::default(),
                locked: false,
            }),
            Annotation::Ellipse(
                EllipseAnnotation::new(
                    ellipse_id.clone(),
                    0,
                    PdfRect::new(40., 10., 20., 20.).unwrap(),
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
            Annotation::Redact(
                RedactAnnotation::new(
                    redact_id.clone(),
                    0,
                    PdfRect::new(70., 10., 20., 20.).unwrap(),
                    "#000000",
                    None::<String>,
                    redact_appearance.clone(),
                )
                .unwrap(),
            ),
            Annotation::Ellipse(locked),
            Annotation::Redact(
                RedactAnnotation::new(
                    other_page_id.clone(),
                    1,
                    PdfRect::new(130., 10., 20., 20.).unwrap(),
                    "#000000",
                    None::<String>,
                    redact_appearance,
                )
                .unwrap(),
            ),
        ] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(annotation))
                .unwrap();
        }
        assert!(document.select(&rectangle_id));
        for id in [&ellipse_id, &redact_id, &locked_id, &other_page_id] {
            document.toggle_selection(id);
        }

        let (anchors, excluded_ids) =
            moving_snap_context(&document, 0, &AnnotationSelectionSupplement::new());
        assert_eq!(excluded_ids, vec![rectangle_id, ellipse_id, redact_id]);
        assert_eq!(anchors.len(), 27);
        assert!(anchors.contains(&point(20., 20.)));
        assert!(anchors.contains(&point(50., 20.)));
        assert!(anchors.contains(&point(80., 20.)));
        assert!(!anchors.contains(&point(110., 20.)));
        assert!(!anchors.contains(&point(140., 20.)));
        assert!(anchors.len() <= 128);
    }

    #[test]
    fn rotated_composite_group_snap_context_excludes_only_moving_page_owners() {
        let text_id = MarkupId::new("text-box:group-snap").unwrap();
        let image_id = MarkupId::new("image:group-snap").unwrap();
        let snapshot_id = MarkupId::new("snapshot:group-snap").unwrap();
        let locked_id = MarkupId::new("image:group-snap-locked").unwrap();
        let other_page_id = MarkupId::new("snapshot:group-snap-other-page").unwrap();
        let asset = DecodedRgbaAsset::new(2, 1, vec![255; 8]).unwrap();
        let text = TextBoxAnnotation::new(
            text_id.clone(),
            0,
            PdfRect::new(10., 20., 20., 10.).unwrap(),
            "Text",
            TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
        )
        .unwrap()
        .with_rotation_degrees(90.)
        .unwrap();
        let image = ImageAnnotation::new(
            image_id.clone(),
            0,
            PdfRect::new(50., 60., 20., 10.).unwrap(),
            asset.clone(),
            false,
        )
        .unwrap()
        .with_rotation_degrees(90.)
        .unwrap();
        let mut locked = ImageAnnotation::new(
            locked_id.clone(),
            0,
            PdfRect::new(130., 140., 20., 10.).unwrap(),
            asset.clone(),
            false,
        )
        .unwrap();
        locked.locked = true;
        let snapshot = SnapshotAnnotation::new(
            snapshot_id.clone(),
            0,
            PdfRect::new(90., 100., 20., 10.).unwrap(),
            asset.clone(),
            1.,
        )
        .unwrap()
        .with_rotation_degrees(90.)
        .unwrap();
        let other_page = SnapshotAnnotation::new(
            other_page_id.clone(),
            1,
            PdfRect::new(170., 180., 20., 10.).unwrap(),
            asset,
            1.,
        )
        .unwrap();
        let mut document = AnnotationDocument::default();
        for annotation in [
            Annotation::TextBox(text),
            Annotation::Image(image),
            Annotation::Snapshot(snapshot),
            Annotation::Image(locked),
            Annotation::Snapshot(other_page),
        ] {
            document
                .apply_command(AnnotationCommand::CreateAnnotation(annotation))
                .unwrap();
        }
        assert!(document.select(&text_id));
        for id in [&image_id, &snapshot_id, &locked_id, &other_page_id] {
            document.toggle_selection(id);
        }

        let (anchors, excluded_ids) =
            moving_snap_context(&document, 0, &AnnotationSelectionSupplement::new());
        assert_eq!(excluded_ids, vec![text_id, image_id, snapshot_id]);
        assert_eq!(anchors.len(), 27);
        assert!(anchors.contains(&point(15., 15.)));
        assert!(anchors.contains(&point(55., 55.)));
        assert!(anchors.contains(&point(95., 95.)));
        assert!(!anchors.contains(&point(140., 145.)));
        assert!(!anchors.contains(&point(180., 185.)));
    }

    #[test]
    fn vertex_path_body_move_snaps_path_anchor_without_preview_history() {
        use crate::semantic_snapping::{SemanticSnapRole, SemanticSnapSource};

        let path_id = MarkupId::new("polyline:body-snap").unwrap();
        let target_id = MarkupId::new("polyline:body-snap-target").unwrap();
        let path = VertexPathAnnotation::new(
            path_id.clone(),
            0,
            vec![point(10., 10.), point(30., 10.), point(30., 30.)],
            VertexPathKind::Polyline,
            RectangleAppearance::default(),
        )
        .unwrap();
        let target = StraightLineAnnotation::new(
            target_id.clone(),
            0,
            point(60., 60.),
            point(90., 60.),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let mut adapter = AnnotationAdapter::default();
        let document = adapter.documents.entry(7).or_default();
        document
            .load_imported_annotations(
                vec![
                    Annotation::VertexPath(path),
                    Annotation::StraightLine(target),
                ],
                Vec::new(),
            )
            .unwrap();
        assert!(document.select(&path_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();

        let before = adapter.snapshot(7).unwrap();
        let history_before = adapter.history_depths(7);
        assert_eq!(
            adapter.pointer_down(7, 0, 98, point(20., 10.), 2.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(98, point(49.4, 39.4)).unwrap();
        let preview_points = adapter.document_scene(7, 0).vertex_paths[0].points.clone();
        for (actual, expected) in
            preview_points
                .iter()
                .zip([point(40., 40.), point(60., 40.), point(60., 60.)])
        {
            assert!((actual.x - expected.x).abs() < 0.000_001);
            assert!((actual.y - expected.y).abs() < 0.000_001);
        }
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(adapter.history_depths(7), history_before);
        let decision = adapter.semantic_snap_decision().unwrap();
        assert_eq!(decision.point, point(60., 60.));
        assert_eq!(decision.owner_id.as_ref(), Some(&target_id));
        assert_eq!(decision.role, SemanticSnapRole::Endpoint);
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        adapter.pointer_down(7, 0, 99, point(20., 10.), 2.).unwrap();
        adapter.pointer_move(99, point(49.4, 39.4)).unwrap();
        adapter.pointer_up(99, point(49.4, 39.4)).unwrap();
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
        adapter.undo(7).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().vertex_paths,
            before.vertex_paths
        );
    }

    #[test]
    fn vertex_path_point_edit_excludes_its_owner_and_snaps_to_external_geometry() {
        use crate::semantic_snapping::{SemanticSnapRole, SemanticSnapSource};

        let path_id = MarkupId::new("polyline:point-snap").unwrap();
        let target_id = MarkupId::new("line:point-snap-target").unwrap();
        let path = VertexPathAnnotation::new(
            path_id.clone(),
            0,
            vec![point(10., 10.), point(59., 60.), point(30., 30.)],
            VertexPathKind::Polyline,
            RectangleAppearance::default(),
        )
        .unwrap();
        let target = StraightLineAnnotation::new(
            target_id.clone(),
            0,
            point(60., 60.),
            point(90., 80.),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        let mut adapter = AnnotationAdapter::default();
        let document = adapter.documents.entry(7).or_default();
        document
            .load_imported_annotations(
                vec![
                    Annotation::VertexPath(path),
                    Annotation::StraightLine(target),
                ],
                Vec::new(),
            )
            .unwrap();
        assert!(document.select(&path_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();

        let before = adapter.snapshot(7).unwrap();
        let history_before = adapter.history_depths(7);
        adapter
            .pointer_down(7, 0, 101, point(10., 10.), 2.)
            .unwrap();
        adapter.pointer_move(101, point(59.4, 60.1)).unwrap();
        assert_eq!(
            adapter.document_scene(7, 0).vertex_paths[0].points[0],
            point(60., 60.)
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(adapter.history_depths(7), history_before);
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        adapter.pointer_up(101, point(59.4, 60.1)).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().vertex_paths[0].points()[0],
            point(60., 60.)
        );
        adapter.undo(7).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().vertex_paths,
            before.vertex_paths
        );
    }

    #[test]
    fn multi_click_path_creation_snaps_and_uses_last_vertex_for_shift_constraint() {
        use crate::semantic_snapping::SemanticSnapSource;

        fn target_line(id: MarkupId) -> Annotation {
            Annotation::StraightLine(
                StraightLineAnnotation::new(
                    id,
                    0,
                    point(50., 10.),
                    point(80., 30.),
                    LineKind::Line,
                    StraightLineAppearance::default_for(LineKind::Line),
                )
                .unwrap(),
            )
        }

        let snapping = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        let mut vertex = AnnotationAdapter::default();
        vertex
            .documents
            .entry(7)
            .or_default()
            .load_imported_annotations(
                vec![target_line(
                    MarkupId::new("line:vertex-create-target").unwrap(),
                )],
                Vec::new(),
            )
            .unwrap();
        vertex.set_semantic_snap_settings(snapping).unwrap();
        vertex.set_tool(AnnotationTool::Polyline).unwrap();
        vertex.pointer_down(7, 0, 102, point(10., 10.), 2.).unwrap();
        vertex
            .pointer_down_with_viewport_input(
                7,
                0,
                103,
                0,
                point(49.4, 20.),
                SelectionPoint::new(49.4, 20.),
                2.,
                PointerInputModifiers {
                    shift: true,
                    alt: false,
                },
            )
            .unwrap();
        assert_eq!(
            vertex.vertex_path_draft.as_ref().unwrap().points,
            vec![point(10., 10.), point(50., 10.)]
        );

        let mut measurement = AnnotationAdapter::default();
        measurement
            .documents
            .entry(8)
            .or_default()
            .load_imported_annotations(
                vec![target_line(
                    MarkupId::new("line:measurement-create-target").unwrap(),
                )],
                Vec::new(),
            )
            .unwrap();
        measurement
            .set_document_page_length_calibration(
                8,
                0,
                LengthCalibration::new(1., "mm", "Scale", true).unwrap(),
            )
            .unwrap();
        measurement.set_semantic_snap_settings(snapping).unwrap();
        measurement.set_tool(AnnotationTool::Polylength).unwrap();
        measurement
            .pointer_down(8, 0, 104, point(10., 10.), 2.)
            .unwrap();
        measurement
            .pointer_down_with_viewport_input(
                8,
                0,
                105,
                0,
                point(49.4, 20.),
                SelectionPoint::new(49.4, 20.),
                2.,
                PointerInputModifiers {
                    shift: true,
                    alt: false,
                },
            )
            .unwrap();
        assert_eq!(
            measurement.measurement_path_draft.as_ref().unwrap().points,
            vec![point(10., 10.), point(50., 10.)]
        );
    }

    #[test]
    fn arc_body_control_and_creation_use_sampled_semantic_geometry() {
        use crate::semantic_snapping::SemanticSnapSource;

        let arc_id = MarkupId::new("arc:semantic-routes").unwrap();
        let arc = ArcAnnotation::new(
            arc_id.clone(),
            0,
            point(10., 10.),
            point(50., 10.),
            point(30., 30.),
            RectangleAppearance::default(),
        )
        .unwrap();
        let body_pointer = arc.sampled_path(64)[16];
        let body_target_id = MarkupId::new("line:arc-body-target").unwrap();
        let control_target_id = MarkupId::new("line:arc-control-target").unwrap();
        let mut adapter = AnnotationAdapter::default();
        let document = adapter.documents.entry(7).or_default();
        document
            .load_imported_annotations(
                vec![
                    Annotation::Arc(arc.clone()),
                    Annotation::StraightLine(
                        StraightLineAnnotation::new(
                            body_target_id.clone(),
                            0,
                            point(100., 100.),
                            point(130., 100.),
                            LineKind::Line,
                            StraightLineAppearance::default_for(LineKind::Line),
                        )
                        .unwrap(),
                    ),
                    Annotation::StraightLine(
                        StraightLineAnnotation::new(
                            control_target_id.clone(),
                            0,
                            point(31., 30.),
                            point(31., 60.),
                            LineKind::Line,
                            StraightLineAppearance::default_for(LineKind::Line),
                        )
                        .unwrap(),
                    ),
                ],
                Vec::new(),
            )
            .unwrap();
        assert!(document.select(&arc_id));
        let expected_revision = document.snapshot().revision;
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .set_semantic_snap_settings(
                SemanticSnapSettings::default()
                    .with_source(SemanticSnapSource::Content, false)
                    .with_source(SemanticSnapSource::PageGrid, false)
                    .with_source(SemanticSnapSource::ConstructionGrid, false),
            )
            .unwrap();

        adapter.active = Some(ActivePointer::ArcMove {
            document_id: 7,
            page_index: 0,
            pointer_id: 106,
            id: arc_id.clone(),
            expected_revision,
            start: body_pointer,
            current: body_pointer,
            original: arc.clone(),
        });
        adapter
            .pointer_move(106, point(body_pointer.x + 89.4, body_pointer.y + 89.4))
            .unwrap();
        let moved_start = adapter.document_scene(7, 0).arcs[0].start;
        assert!((moved_start.x - 100.).abs() < 1.);
        assert!((moved_start.y - 100.).abs() < 1.);
        assert_ne!(moved_start, point(99.4, 99.4));
        assert_eq!(
            adapter.semantic_snap_decision().unwrap().owner_id.as_ref(),
            Some(&body_target_id)
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        adapter.active = Some(ActivePointer::ArcControlPoint {
            document_id: 7,
            page_index: 0,
            pointer_id: 107,
            id: arc_id.clone(),
            expected_revision,
            control: ArcControlPoint::Start,
            start: arc.start,
            current: arc.start,
            original: arc,
            snap_quarter_turn: false,
        });
        adapter.pointer_move(107, point(30.4, 30.)).unwrap();
        assert_eq!(adapter.document_scene(7, 0).arcs[0].start, point(31., 30.));
        assert_eq!(
            adapter.semantic_snap_decision().unwrap().owner_id.as_ref(),
            Some(&control_target_id)
        );
        adapter.cancel(PointerCancelReason::CaptureLost).unwrap();

        let mut creation = AnnotationAdapter::default();
        creation
            .documents
            .entry(8)
            .or_default()
            .load_imported_annotations(
                vec![Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        MarkupId::new("line:arc-create-target").unwrap(),
                        0,
                        point(10., 10.),
                        point(50., 10.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                )],
                Vec::new(),
            )
            .unwrap();
        creation
            .set_semantic_snap_settings(adapter.semantic_snap_settings())
            .unwrap();
        creation.set_tool(AnnotationTool::Arc).unwrap();
        creation
            .pointer_down(8, 0, 108, point(10.3, 10.3), 2.)
            .unwrap();
        creation
            .pointer_down_with_viewport_input(
                8,
                0,
                109,
                0,
                point(49.4, 20.),
                SelectionPoint::new(49.4, 20.),
                2.,
                PointerInputModifiers {
                    shift: true,
                    alt: false,
                },
            )
            .unwrap();
        let draft = creation.arc_draft.as_ref().unwrap();
        assert_eq!(draft.start, point(10., 10.));
        assert_eq!(draft.end, Some(point(50., 10.)));
    }

    #[test]
    fn linear_body_and_group_moves_snap_geometry_anchors_without_preview_history() {
        use crate::semantic_snapping::SemanticSnapRole;

        let snapping_off = SemanticSnapSettings::default()
            .with_source(SemanticSnapSource::Content, false)
            .with_source(SemanticSnapSource::Annotation, false)
            .with_source(SemanticSnapSource::PageGrid, false)
            .with_source(SemanticSnapSource::ConstructionGrid, false);
        let mut adapter = AnnotationAdapter::default();
        let line_id = MarkupId::new("line:move-primary").unwrap();
        let length_id = MarkupId::new("length:move-primary").unwrap();
        let target_id = MarkupId::new("line:move-target").unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        line_id.clone(),
                        0,
                        point(10., 10.),
                        point(100., 10.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
                LengthAnnotation::new(
                    length_id.clone(),
                    0,
                    point(10., 50.),
                    point(100., 50.),
                    LengthCalibration::from_scale(72., 1., "m", 2, false).unwrap(),
                )
                .unwrap(),
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        target_id.clone(),
                        0,
                        point(130., 30.),
                        point(180., 30.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        assert!(document.select(&line_id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter.set_semantic_snap_settings(snapping_off).unwrap();

        adapter.pointer_down(7, 0, 51, point(50., 10.), 3.).unwrap();
        adapter.pointer_move(51, point(79., 29.)).unwrap();
        let preview = adapter.document_scene(7, 0);
        let line = preview
            .straight_lines
            .iter()
            .find(|line| line.id == line_id)
            .unwrap();
        assert_eq!(line.start, point(39., 29.));
        assert_eq!(
            line.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        adapter.cancel(PointerCancelReason::ToolChanged).unwrap();
        assert_eq!(
            adapter
                .snapshot(7)
                .unwrap()
                .straight_lines
                .iter()
                .find(|line| line.id == line_id)
                .unwrap()
                .start,
            point(10., 10.)
        );

        adapter
            .set_semantic_snap_settings(
                snapping_off.with_source(SemanticSnapSource::Annotation, true),
            )
            .unwrap();
        let history_before = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 52, point(50., 10.), 3.).unwrap();
        adapter.pointer_move(52, point(79., 29.)).unwrap();
        let preview = adapter.document_scene(7, 0);
        let line = preview
            .straight_lines
            .iter()
            .find(|line| line.id == line_id)
            .unwrap();
        assert_eq!((line.start, line.end), (point(40., 30.), point(130., 30.)));
        assert_eq!(
            line.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        assert_snap_evidence_references(&adapter, &target_id, SemanticSnapRole::Endpoint);
        assert_eq!(adapter.history_depths(7), history_before);
        adapter.pointer_up(52, point(79., 29.)).unwrap();
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));

        let document = adapter.documents.get_mut(&7).unwrap();
        assert!(document.select(&length_id));
        // A Length is drawn, and pressed, on its dimension line 10 pt above.
        adapter.pointer_down(7, 0, 54, point(50., 60.), 3.).unwrap();
        adapter.pointer_move(54, point(79., 41.)).unwrap();
        let preview = adapter.document_scene(7, 0);
        let length = preview
            .lengths
            .iter()
            .find(|length| length.id == length_id)
            .unwrap();
        assert_eq!(
            (length.start, length.end),
            (point(40., 30.), point(130., 30.))
        );
        assert_eq!(
            length.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        adapter.cancel(PointerCancelReason::ToolChanged).unwrap();

        let document = adapter.documents.get_mut(&7).unwrap();
        assert!(document.select(&line_id));
        document.toggle_selection(&length_id);
        let history_before = adapter.history_depths(7);
        adapter.pointer_down(7, 0, 53, point(70., 30.), 3.).unwrap();
        adapter.pointer_move(53, point(99., 49.)).unwrap();
        let preview = adapter.document_scene(7, 0);
        let line = preview
            .straight_lines
            .iter()
            .find(|line| line.id == line_id)
            .unwrap();
        let length = preview
            .lengths
            .iter()
            .find(|length| length.id == length_id)
            .unwrap();
        assert_eq!(
            line.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true
            }
        );
        assert_eq!(
            length.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true
            }
        );
        assert_eq!(adapter.history_depths(7), history_before);
        adapter.cancel(PointerCancelReason::ToolChanged).unwrap();
        assert_eq!(adapter.history_depths(7), history_before);

        adapter.pointer_down(7, 0, 55, point(70., 30.), 3.).unwrap();
        adapter.pointer_move(55, point(99., 49.)).unwrap();
        let preview = adapter.document_scene(7, 0);
        let preview_line = preview
            .straight_lines
            .iter()
            .find(|line| line.id == line_id)
            .unwrap()
            .start;
        let preview_length = preview
            .lengths
            .iter()
            .find(|length| length.id == length_id)
            .unwrap()
            .start;
        adapter.pointer_up(55, point(99., 49.)).unwrap();
        let committed = adapter.snapshot(7).unwrap();
        assert_eq!(
            committed
                .straight_lines
                .iter()
                .find(|line| line.id == line_id)
                .unwrap()
                .start,
            preview_line
        );
        assert_eq!(
            committed
                .lengths
                .iter()
                .find(|length| length.id == length_id)
                .unwrap()
                .start,
            preview_length
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));
    }

    #[test]
    fn a_cloud_plus_caption_fits_its_text_and_keeps_the_side_its_leader_meets() {
        let mut adapter = AnnotationAdapter::default();
        let id = MarkupId::new("cloud-plus:left-caption").unwrap();
        let annotation = CloudPlusAnnotation::new(
            id.clone(),
            0,
            vec![
                point(200., 10.),
                point(240., 10.),
                point(240., 50.),
                point(200., 50.),
            ],
            2.,
            vec![point(200., 30.), point(188., 30.), point(176., 30.)],
            PdfRect::new(26., 20., 150., 20.).unwrap(),
            "Cloud+",
            default_cloud_plus_appearance().unwrap(),
        )
        .unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::CloudPlus(
                annotation,
            )))
            .unwrap();

        adapter.replace_cloud_plus_text(7, &id, "Gap").unwrap();
        let short = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        let width = caption_box_width("Gap", short.appearance.text());
        assert!(
            width < 60.,
            "a short caption must not keep a wide box: {width}"
        );
        assert_eq!(short.text_box.width, width);
        assert_eq!(
            short.text_box.x + short.text_box.width,
            176.,
            "a caption left of its cloud must stay against the leader"
        );

        adapter
            .replace_cloud_plus_text(7, &id, "A much longer caption")
            .unwrap();
        let long = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        assert!(long.text_box.width > short.text_box.width);
        assert_eq!(long.text_box.x + long.text_box.width, 176.);
    }

    #[test]
    fn selected_cloud_plus_double_click_and_existing_text_edit_use_ordinary_history() {
        let mut adapter = AnnotationAdapter::default();
        let id = seed_cloud_plus(&mut adapter);
        assert!(adapter.documents.get_mut(&7).unwrap().select(&id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let original = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        let history_before = adapter.history_depths(7);

        for point in [point(120., 30.), point(75., 30.), point(30., 10.)] {
            assert_eq!(
                adapter.pointer_double_click(7, 0, point, 2.).unwrap(),
                PointerPhaseOutcome::SelectionChanged(Some(id.clone()))
            );
            assert_eq!(adapter.history_depths(7), history_before);
        }

        adapter
            .replace_cloud_plus_text(7, &id, "one\ntwo\nthree")
            .unwrap();
        let edited = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        assert_eq!(edited.content(), "one\ntwo\nthree");
        assert!(edited.text_box.height > original.text_box.height);
        assert_eq!(edited.text_box.x, original.text_box.x);
        assert_eq!(
            edited.text_box.width,
            caption_box_width("one\ntwo\nthree", edited.appearance.text())
        );
        assert_eq!(
            edited.text_box.y + edited.text_box.height * 0.5,
            original.leader_points().last().unwrap().y,
            "a vertical-side leader connection must keep its centre while the caption grows"
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));

        adapter.undo(7).unwrap();
        assert!(adapter.snapshot(7).unwrap().cloud_pluses[0].same_persisted_state_as(&original));
        adapter.redo(7).unwrap();
        assert!(adapter.snapshot(7).unwrap().cloud_pluses[0].same_persisted_state_as(&edited));
        let history_after_redo = adapter.history_depths(7);
        adapter
            .replace_cloud_plus_text(7, &id, edited.content())
            .unwrap();
        assert_eq!(adapter.history_depths(7), history_after_redo);

        adapter.set_primary_selected_locked(7, &id, true).unwrap();
        assert_eq!(
            adapter
                .pointer_double_click(7, 0, point(120., 30.), 2.)
                .unwrap(),
            PointerPhaseOutcome::Ignored
        );
    }

    #[test]
    fn hover_markup_id_mirrors_select_hit_order() {
        let mut adapter = AnnotationAdapter::default();
        let id = MarkupId::new("rectangle:hover").unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: id.clone(),
                    page_index: 0,
                    rect: PdfRect::new(10., 20., 100., 50.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();

        assert_eq!(
            adapter
                .hover_markup_id(7, 0, point(50., 20.5), 1.0)
                .unwrap(),
            Some(id)
        );
        assert_eq!(
            adapter
                .hover_markup_id(7, 0, point(500., 500.), 1.0)
                .unwrap(),
            None
        );
        assert!(
            adapter
                .hover_markup_id(99, 0, point(50., 40.), 1.0)
                .is_err()
        );
    }

    #[test]
    fn measurement_caption_bodies_hover_move_commit_and_undo() {
        let calibration = LengthCalibration::from_scale(72., 1., "m", 2, false).unwrap();
        let length_id = MarkupId::new("caption-body:length").unwrap();
        let polylength_id = MarkupId::new("caption-body:polylength").unwrap();
        let area_id = MarkupId::new("caption-body:area").unwrap();
        let dimension_id = MarkupId::new("caption-body:dimension").unwrap();
        let annotations = vec![
            Annotation::Length(
                LengthAnnotation::new(
                    length_id.clone(),
                    0,
                    point(10., 10.),
                    point(80., 10.),
                    calibration.clone(),
                )
                .unwrap(),
            ),
            Annotation::MeasurementPath(
                MeasurementPathAnnotation::new(
                    polylength_id.clone(),
                    0,
                    vec![point(10., 30.), point(80., 30.), point(80., 50.)],
                    MeasurementPathKind::Polylength,
                    calibration.clone(),
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
            Annotation::MeasurementPath(
                MeasurementPathAnnotation::new(
                    area_id.clone(),
                    0,
                    vec![point(10., 60.), point(80., 60.), point(80., 90.)],
                    MeasurementPathKind::Area,
                    calibration,
                    RectangleAppearance::default(),
                )
                .unwrap(),
            ),
            Annotation::Dimension(
                DimensionAnnotation::new(
                    dimension_id.clone(),
                    0,
                    point(10., 110.),
                    point(80., 110.),
                    20.,
                    "70 mm",
                    default_dimension_appearance().unwrap(),
                )
                .unwrap(),
            ),
        ];
        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .load_imported_annotations(annotations, Vec::new())
            .unwrap();
        let caption_hits = [
            (length_id, point(210., 20.)),
            (polylength_id, point(210., 50.)),
            (area_id, point(210., 80.)),
            (dimension_id, point(210., 110.)),
        ];
        let supplement = caption_hits
            .iter()
            .map(|(id, centre)| {
                (
                    id.clone(),
                    vec![
                        point(centre.x - 8., centre.y - 5.),
                        point(centre.x + 8., centre.y - 5.),
                        point(centre.x + 8., centre.y + 5.),
                        point(centre.x - 8., centre.y + 5.),
                    ],
                )
            })
            .collect::<AnnotationSelectionSupplement>();

        assert_eq!(
            adapter
                .hover_markup_id_with_selection_paths(
                    7,
                    0,
                    caption_hits[0].1,
                    1.,
                    &AnnotationSelectionSupplement::new(),
                )
                .unwrap(),
            None,
            "caption bodies must come from the paint-owned measured supplement"
        );
        assert_eq!(
            adapter
                .hover_markup_id_with_selection_paths(7, 1, caption_hits[0].1, 1., &supplement)
                .unwrap(),
            None,
            "a caption supplement must not leak across pages"
        );

        for (index, (id, hit)) in caption_hits.iter().enumerate() {
            adapter.documents.get_mut(&7).unwrap().clear_selection();
            assert_eq!(
                adapter
                    .hover_markup_id_with_selection_paths(7, 0, *hit, 1., &supplement)
                    .unwrap(),
                Some(id.clone())
            );
            let before = adapter.snapshot(7).unwrap();
            let pointer_id = index as u64 + 100;
            assert_eq!(
                adapter
                    .pointer_down_with_viewport_input_and_selection_paths(
                        7,
                        0,
                        pointer_id,
                        0,
                        *hit,
                        SelectionPoint::new(hit.x, hit.y),
                        1.,
                        PointerInputModifiers::default(),
                        &supplement,
                    )
                    .unwrap(),
                PointerPhaseOutcome::GestureStarted
            );
            let moved = point(hit.x + 12., hit.y + 7.);
            adapter.pointer_move(pointer_id, moved).unwrap();
            let during = adapter.snapshot(7).unwrap();
            assert_eq!(during.revision, before.revision);
            assert_eq!(during.undo_depth, before.undo_depth);
            assert_eq!(during.lengths, before.lengths);
            assert_eq!(during.measurement_paths, before.measurement_paths);
            assert_eq!(during.dimensions, before.dimensions);
            assert!(matches!(
                adapter
                    .pointer_up_with_viewport_input_and_selection_paths(
                        pointer_id,
                        moved,
                        SelectionPoint::new(moved.x, moved.y),
                        PointerInputModifiers::default(),
                        &supplement,
                    )
                    .unwrap(),
                PointerPhaseOutcome::AnnotationEdited(_)
            ));
            let committed = adapter.snapshot(7).unwrap();
            assert!(committed.revision > before.revision);
            assert_eq!(committed.undo_depth, before.undo_depth + 1);
            adapter.undo(7).unwrap();
            let undone = adapter.snapshot(7).unwrap();
            assert_eq!(undone.lengths, before.lengths);
            assert_eq!(undone.measurement_paths, before.measurement_paths);
            assert_eq!(undone.dimensions, before.dimensions);
        }
    }

    #[test]
    fn cross_family_body_hit_uses_document_order_for_hover_press_commit_and_lock() {
        let rect = PdfRect::new(10., 20., 100., 50.).unwrap();
        let redact_appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
            .unwrap()
            .with_fill_opacity(0.35)
            .unwrap();
        let redact_id = MarkupId::new("cross-family:redact").unwrap();
        let redact = RedactAnnotation::new(
            redact_id.clone(),
            0,
            rect,
            "#000000",
            None::<String>,
            redact_appearance.clone(),
        )
        .unwrap();
        let image_id = MarkupId::new("cross-family:image").unwrap();
        let image = ImageAnnotation::new(
            image_id.clone(),
            0,
            rect,
            DecodedRgbaAsset::new(1, 1, vec![255; 4]).unwrap(),
            false,
        )
        .unwrap();
        let pen_id = MarkupId::new("cross-family:pen").unwrap();
        let pen = PenAnnotation::new(
            pen_id.clone(),
            0,
            vec![point(20., 45.), point(100., 45.)],
            PenAppearance::new("#ff0000", 2., 1.).unwrap(),
        )
        .unwrap();

        let mut adapter = AnnotationAdapter::default();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .load_imported_annotations(
                vec![
                    Annotation::Redact(redact.clone()),
                    Annotation::Image(image.clone()),
                ],
                Vec::new(),
            )
            .unwrap();
        let hit = point(60., 45.);
        assert_eq!(
            adapter.hover_markup_id(7, 0, hit, 2.).unwrap(),
            Some(image_id.clone()),
            "the visually top Image must win over a lower Redact"
        );
        let before = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter.pointer_down(7, 0, 1, hit, 2.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(1, point(72., 53.)).unwrap();
        let preview = adapter.document_scene(7, 0);
        let preview_image = preview
            .images
            .iter()
            .find(|annotation| annotation.id == image_id)
            .unwrap();
        assert!(matches!(
            preview_image.feedback,
            SceneInteractionFeedback::Move { .. }
        ));
        assert_ne!(preview_image.rect, image.rect);
        let during = adapter.snapshot(7).unwrap();
        assert_eq!(during.images, before.images);
        assert_eq!(
            (during.revision, during.undo_depth),
            (before.revision, before.undo_depth)
        );
        assert_eq!(during.selected_id.as_ref(), Some(&image_id));
        adapter.pointer_up(1, point(72., 53.)).unwrap();
        let committed = adapter.snapshot(7).unwrap();
        assert_eq!(committed.revision, before.revision + 1);
        assert_eq!(committed.undo_depth, before.undo_depth + 1);
        assert_ne!(committed.images[0].rect, image.rect);
        adapter.undo(7).unwrap();
        assert_eq!(adapter.snapshot(7).unwrap().images, before.images);

        let mut reversed = AnnotationAdapter::default();
        reversed.set_tool(AnnotationTool::Select).unwrap();
        reversed
            .documents
            .entry(8)
            .or_default()
            .load_imported_annotations(
                vec![
                    Annotation::Image(image.clone()),
                    Annotation::Redact(redact.clone()),
                ],
                Vec::new(),
            )
            .unwrap();
        assert_eq!(
            reversed.hover_markup_id(8, 0, hit, 2.).unwrap(),
            Some(redact_id.clone()),
            "reversing document order must reverse the cross-family winner"
        );

        reversed
            .documents
            .get_mut(&8)
            .unwrap()
            .load_imported_annotations(
                vec![Annotation::Redact(redact), Annotation::Pen(pen)],
                Vec::new(),
            )
            .unwrap();
        assert_eq!(
            reversed.hover_markup_id(8, 0, hit, 2.).unwrap(),
            Some(pen_id),
            "a top Pen stroke must win over a lower filled family"
        );

        let rectangle_id = MarkupId::new("cross-family:rectangle").unwrap();
        let rectangle = RectangleAnnotation {
            id: rectangle_id.clone(),
            page_index: 0,
            rect,
            rotation_degrees: 0.,
            // Filled, so its inside covers the image below.
            appearance: RectangleAppearance::new("#123456", 2., Some("#abcdef"), 0.5).unwrap(),
            locked: false,
        };
        let mut rectangle_top = AnnotationAdapter::default();
        rectangle_top.set_tool(AnnotationTool::Select).unwrap();
        rectangle_top
            .documents
            .entry(9)
            .or_default()
            .load_imported_annotations(
                vec![
                    Annotation::Image(image.clone()),
                    Annotation::Rectangle(rectangle.clone()),
                ],
                Vec::new(),
            )
            .unwrap();
        assert_eq!(
            rectangle_top.hover_markup_id(9, 0, hit, 2.).unwrap(),
            Some(rectangle_id.clone())
        );
        assert_eq!(
            rectangle_top.pointer_down(9, 0, 3, hit, 2.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        rectangle_top.pointer_move(3, point(70., 55.)).unwrap();
        assert_ne!(
            rectangle_top.document_scene(9, 0).rectangles[0].rect,
            rectangle.rect,
            "the unified body route must retain the Rectangle model gesture"
        );
        rectangle_top
            .cancel(PointerCancelReason::CaptureLost)
            .unwrap();

        let line_id = MarkupId::new("cross-family:line").unwrap();
        let line = StraightLineAnnotation::new(
            line_id.clone(),
            0,
            point(20., 45.),
            point(100., 45.),
            LineKind::Line,
            StraightLineAppearance::default_for(LineKind::Line),
        )
        .unwrap();
        reversed
            .documents
            .get_mut(&8)
            .unwrap()
            .load_imported_annotations(
                vec![
                    Annotation::Redact(
                        RedactAnnotation::new(
                            MarkupId::new("cross-family:line-redact").unwrap(),
                            0,
                            rect,
                            "#000000",
                            None::<String>,
                            redact_appearance.clone(),
                        )
                        .unwrap(),
                    ),
                    Annotation::StraightLine(line),
                ],
                Vec::new(),
            )
            .unwrap();
        assert_eq!(
            reversed.hover_markup_id(8, 0, hit, 2.).unwrap(),
            Some(line_id),
            "a top straight line must share the same cross-family ordering"
        );

        let mut locked_image = image;
        locked_image.locked = true;
        let locked_id = locked_image.id.clone();
        let locked_document = reversed.documents.get_mut(&8).unwrap();
        locked_document
            .load_imported_annotations(
                vec![
                    Annotation::Redact(
                        RedactAnnotation::new(
                            redact_id,
                            0,
                            rect,
                            "#000000",
                            None::<String>,
                            redact_appearance,
                        )
                        .unwrap(),
                    ),
                    Annotation::Image(locked_image),
                ],
                Vec::new(),
            )
            .unwrap();
        let locked_before = reversed.snapshot(8).unwrap();
        assert_eq!(
            reversed.pointer_down(8, 0, 2, hit, 2.).unwrap(),
            PointerPhaseOutcome::SelectionChanged(Some(locked_id.clone()))
        );
        let locked_after = reversed.snapshot(8).unwrap();
        assert_eq!(locked_after.images, locked_before.images);
        assert_eq!(
            (locked_after.revision, locked_after.undo_depth),
            (locked_before.revision, locked_before.undo_depth)
        );
        assert_eq!(locked_after.selected_id.as_ref(), Some(&locked_id));
    }

    #[test]
    fn select_hover_hit_preserves_selected_rectangle_hit_semantics_without_mutation() {
        let mut adapter = AnnotationAdapter::default();
        let id = MarkupId::new("rectangle:select-hover").unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: id.clone(),
                    page_index: 0,
                    rect: PdfRect::new(10., 20., 100., 50.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        assert!(adapter.documents.get_mut(&7).unwrap().select(&id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter
                .select_hover_hit(7, 0, point(110., 45.), 1.)
                .unwrap(),
            Some(HitTarget::ResizeHandle {
                id: id.clone(),
                handle: RectangleResizeHandle::East,
            })
        );
        assert_eq!(
            adapter.select_hover_hit(7, 0, point(60., 90.), 1.).unwrap(),
            Some(HitTarget::RotationHandle(id.clone()))
        );
        assert_eq!(
            adapter.select_hover_hit(7, 0, point(50., 28.), 1.).unwrap(),
            Some(HitTarget::Body(id))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
    }

    #[test]
    fn select_hover_hit_requires_select_and_suppresses_locked_rectangle_controls() {
        let mut adapter = AnnotationAdapter::default();
        let id = MarkupId::new("rectangle:locked-select-hover").unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: id.clone(),
                    page_index: 0,
                    rect: PdfRect::new(10., 20., 100., 50.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        assert!(adapter.documents.get_mut(&7).unwrap().select(&id));
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        assert_eq!(
            adapter
                .select_hover_hit(7, 0, point(110., 45.), 1.)
                .unwrap(),
            None
        );

        adapter.set_tool(AnnotationTool::Select).unwrap();
        adapter.set_primary_selected_locked(7, &id, true).unwrap();
        let before = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .select_hover_hit(7, 0, point(110., 45.), 1.)
                .unwrap(),
            Some(HitTarget::Body(id.clone()))
        );
        assert_eq!(
            adapter.select_hover_hit(7, 0, point(60., 90.), 1.).unwrap(),
            None
        );
        assert_eq!(
            adapter.select_hover_hit(7, 0, point(50., 25.), 1.).unwrap(),
            Some(HitTarget::Body(id))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
    }

    #[test]
    fn rectangle_hover_handle_is_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_id = MarkupId::new("rectangle:hover-bottom").unwrap();
        let top_id = MarkupId::new("rectangle:hover-top").unwrap();
        let rect = PdfRect::new(10., 20., 100., 50.).unwrap();
        for id in [&bottom_id, &top_id] {
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                    RectangleAnnotation {
                        id: id.clone(),
                        page_index: 0,
                        rect,
                        rotation_degrees: 30.,
                        appearance: RectangleAppearance::default(),
                        locked: false,
                    },
                )))
                .unwrap();
        }
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let east = RectangleResizeHandle::East.world_point(rect, 30.);
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter.hover_rectangle_handle(7, 0, east, 1.).unwrap(),
            Some((top_id.clone(), 3)),
            "the topmost unselected Rectangle must win an overlapping rotated handle"
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter.hover_rectangle_handle(7, 0, east, 1.).unwrap(),
            Some((bottom_id.clone(), 3)),
            "the selected Rectangle must win before a topmost unselected overlap"
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter.hover_rectangle_handle(7, 0, east, 1.).unwrap(),
            Some((top_id.clone(), 3)),
            "locked Rectangle controls must be skipped"
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter.hover_rectangle_handle(7, 0, east, 1.).unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        assert_eq!(
            adapter.hover_rectangle_handle(7, 0, east, 1.).unwrap(),
            None
        );
    }

    #[test]
    fn ellipse_hover_handle_is_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_id = MarkupId::new("ellipse:hover-bottom").unwrap();
        let top_id = MarkupId::new("ellipse:hover-top").unwrap();
        let rect = PdfRect::new(10., 20., 100., 50.).unwrap();
        for id in [&bottom_id, &top_id] {
            let mut ellipse =
                EllipseAnnotation::new(id.clone(), 0, rect, RectangleAppearance::default())
                    .unwrap();
            ellipse.rotation_degrees = 30.;
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Ellipse(
                    ellipse,
                )))
                .unwrap();
        }
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let east = ellipse_resize_handle_point_for_rect(rect, 30., RectangleResizeHandle::East);
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter.hover_ellipse_handle(7, 0, east, 1.).unwrap(),
            Some((top_id.clone(), 3)),
            "the topmost unselected Ellipse must win an overlapping rotated handle"
        );
        assert_eq!(
            adapter.hover_transform_handle(7, 0, east, 1.).unwrap(),
            Some((top_id.clone(), 3))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter.hover_ellipse_handle(7, 0, east, 1.).unwrap(),
            Some((bottom_id.clone(), 3)),
            "the selected Ellipse must win before a topmost unselected overlap"
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter.hover_ellipse_handle(7, 0, east, 1.).unwrap(),
            Some((top_id.clone(), 3)),
            "locked Ellipse controls must be skipped"
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(adapter.hover_ellipse_handle(7, 0, east, 1.).unwrap(), None);
        adapter.set_tool(AnnotationTool::Ellipse).unwrap();
        assert_eq!(adapter.hover_ellipse_handle(7, 0, east, 1.).unwrap(), None);
    }

    #[test]
    fn line_arrow_and_length_hover_handles_are_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_line_id = MarkupId::new("line:hover-bottom").unwrap();
        let length_id = MarkupId::new("length:hover-middle").unwrap();
        let top_arrow_id = MarkupId::new("arrow:hover-top").unwrap();
        let start = point(10., 20.);
        let end = point(110., 20.);
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        bottom_line_id.clone(),
                        0,
                        start,
                        end,
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
                LengthAnnotation::new(
                    length_id.clone(),
                    0,
                    start,
                    end,
                    LengthCalibration::from_scale(72., 1., "m", 2, false).unwrap(),
                )
                .unwrap(),
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        top_arrow_id.clone(),
                        0,
                        start,
                        end,
                        LineKind::Arrow,
                        StraightLineAppearance::default_for(LineKind::Arrow),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        document.clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter.hover_straight_line_handle(7, 0, end, 1.).unwrap(),
            Some((top_arrow_id.clone(), 1)),
            "the topmost unselected Line/Arrow must win within its family"
        );
        assert_eq!(
            adapter.hover_length_handle(7, 0, start, 1.).unwrap(),
            Some((length_id.clone(), 0))
        );
        assert_eq!(
            adapter.hover_transform_handle(7, 0, end, 1.).unwrap(),
            Some((top_arrow_id.clone(), 1)),
            "global hover priority must follow document order rather than family order"
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        assert!(
            adapter
                .documents
                .get_mut(&7)
                .unwrap()
                .select(&bottom_line_id)
        );
        assert_eq!(
            adapter.hover_transform_handle(7, 0, end, 1.).unwrap(),
            Some((bottom_line_id.clone(), 1)),
            "the selected line must beat later unselected families"
        );
        adapter
            .set_primary_selected_locked(7, &bottom_line_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert!(adapter.documents.get_mut(&7).unwrap().select(&length_id));
        assert_eq!(
            adapter.hover_transform_handle(7, 0, end, 1.).unwrap(),
            Some((length_id.clone(), 1)),
            "the selected Length must beat the topmost unselected Arrow"
        );
        adapter
            .set_primary_selected_locked(7, &length_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter.hover_transform_handle(7, 0, end, 1.).unwrap(),
            Some((top_arrow_id.clone(), 1)),
            "locked Length controls must be skipped"
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_arrow_id));
        adapter
            .set_primary_selected_locked(7, &top_arrow_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter.hover_transform_handle(7, 0, end, 1.).unwrap(),
            None,
            "only locked overlapping endpoints remain"
        );
        adapter.set_tool(AnnotationTool::Line).unwrap();
        assert_eq!(
            adapter.hover_straight_line_handle(7, 0, end, 1.).unwrap(),
            None
        );
        assert_eq!(adapter.hover_length_handle(7, 0, end, 1.).unwrap(), None);
    }

    #[test]
    fn a_diagonal_length_is_outlined_along_itself_and_its_ends_grab_from_the_arrows() {
        let mut adapter = AnnotationAdapter::default();
        let id = MarkupId::new("length:diagonal").unwrap();
        let start = point(100., 300.);
        let end = point(400., 200.);
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Length(
                LengthAnnotation::new(
                    id.clone(),
                    0,
                    start,
                    end,
                    LengthCalibration::from_scale(72., 1., "in", 2, false).unwrap(),
                )
                .unwrap(),
            )))
            .unwrap();
        assert!(document.select(&id));
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let length = adapter.snapshot(7).unwrap().lengths[0].clone();
        let layout = crate::annotation_model::measurement_line_layout(
            start,
            end,
            crate::annotation_model::LENGTH_LEADER_LENGTH_PT,
            length.appearance.line().stroke_width_pt(),
            0.,
        )
        .unwrap();

        // The upright box around the measured points takes in this empty
        // corner; the outline runs along the length, so it is not part of it.
        let document = adapter.documents.get(&7).unwrap();
        let no_captions = AnnotationSelectionSupplement::new();
        assert_eq!(
            document.selected_outline_zone_hit(0, point(390., 290.), 6., 20., &no_captions),
            None
        );
        let arrow_tip = layout.arrowheads[1][0];
        assert_eq!(
            document.selected_outline_zone_hit(0, arrow_tip, 6., 20., &no_captions),
            Some(id.clone()),
            "the drawn dimension line is inside the outline"
        );

        // The end's extension line, out to the arrow, grabs the end: the
        // hover shows it and the press drags it, without the end jumping.
        let grab = layout.extension_lines[1].1;
        assert!(
            distance(grab, end) > 9.,
            "the grab is off the measured point"
        );
        assert_eq!(
            adapter.hover_length_handle(7, 0, grab, 1.).unwrap(),
            Some((id.clone(), 1))
        );
        assert_eq!(
            adapter.pointer_down(7, 0, 1, grab, 1.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter
            .pointer_move(1, point(grab.x + 15., grab.y - 5.))
            .unwrap();
        adapter
            .pointer_up(1, point(grab.x + 15., grab.y - 5.))
            .unwrap();
        let moved = adapter.snapshot(7).unwrap().lengths[0].clone();
        assert_eq!(moved.start, start);
        assert!(distance(moved.end, point(end.x + 15., end.y - 5.)) < 1e-6);
    }

    #[test]
    fn redact_hover_handle_is_topmost_select_only_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_id = MarkupId::new("redact:hover-bottom").unwrap();
        let top_id = MarkupId::new("redact:hover-top").unwrap();
        let rect = PdfRect::new(10., 20., 100., 50.).unwrap();
        let appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
            .unwrap()
            .with_fill_opacity(0.35)
            .unwrap();
        for id in [&bottom_id, &top_id] {
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Redact(
                    RedactAnnotation::new(
                        id.clone(),
                        0,
                        rect,
                        "#000000",
                        None::<String>,
                        appearance.clone(),
                    )
                    .unwrap(),
                )))
                .unwrap();
        }
        let rectangle_id = MarkupId::new("rectangle:hover-over-redact").unwrap();
        adapter
            .documents
            .entry(7)
            .or_default()
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: rectangle_id.clone(),
                    page_index: 0,
                    rect,
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let east = axis_aligned_resize_handle_point(rect, RectangleResizeHandle::East);
        let before = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter.hover_redact_handle(7, 0, east, 1.).unwrap(),
            Some((top_id.clone(), 3))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert_eq!(
            adapter.hover_transform_handle(7, 0, east, 1.).unwrap(),
            Some((rectangle_id, 3)),
            "without selection the globally topmost family must win"
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter.hover_redact_handle(7, 0, east, 1.).unwrap(),
            Some((bottom_id.clone(), 3))
        );
        assert_eq!(
            adapter.hover_transform_handle(7, 0, east, 1.).unwrap(),
            Some((bottom_id.clone(), 3)),
            "a selected Redact must beat an overlapping unselected Rectangle"
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter.hover_redact_handle(7, 0, east, 1.).unwrap(),
            Some((top_id.clone(), 3))
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(adapter.hover_redact_handle(7, 0, east, 1.).unwrap(), None);
        adapter.set_tool(AnnotationTool::Redact).unwrap();
        assert_eq!(adapter.hover_redact_handle(7, 0, east, 1.).unwrap(), None);
    }

    #[test]
    fn cloud_hover_handles_are_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_id = MarkupId::new("cloud:hover-bottom").unwrap();
        let top_id = MarkupId::new("cloud:hover-top").unwrap();
        let overlapping_points = vec![
            point(20., 20.),
            point(100., 20.),
            point(20., 20.),
            point(100., 80.),
        ];
        for id in [&bottom_id, &top_id] {
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Cloud(
                    CloudAnnotation::new(
                        id.clone(),
                        0,
                        overlapping_points.clone(),
                        3.,
                        RectangleAppearance::default(),
                    )
                    .unwrap(),
                )))
                .unwrap();
        }
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter
                .hover_cloud_handle(7, 0, overlapping_points[0], 1.)
                .unwrap(),
            Some((top_id.clone(), 2)),
            "the topmost Cloud and last overlapping vertex must win"
        );
        assert_eq!(
            adapter
                .hover_transform_handle(7, 0, overlapping_points[0], 1.)
                .unwrap(),
            Some((top_id.clone(), 2))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter
                .hover_transform_handle(7, 0, overlapping_points[0], 1.)
                .unwrap(),
            Some((bottom_id.clone(), 2)),
            "the selected Cloud must win before a topmost unselected overlap"
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_cloud_handle(7, 0, overlapping_points[0], 1.)
                .unwrap(),
            Some((top_id.clone(), 2))
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_cloud_handle(7, 0, overlapping_points[0], 1.)
                .unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Cloud).unwrap();
        assert_eq!(
            adapter
                .hover_cloud_handle(7, 0, overlapping_points[0], 1.)
                .unwrap(),
            None
        );
    }

    #[test]
    fn arc_hover_handles_are_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_id = MarkupId::new("arc:hover-bottom").unwrap();
        let top_id = MarkupId::new("arc:hover-top").unwrap();
        for id in [&bottom_id, &top_id] {
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Arc(
                    ArcAnnotation::new(
                        id.clone(),
                        0,
                        point(20., 20.),
                        point(28., 20.),
                        point(24., 24.),
                        RectangleAppearance::default(),
                    )
                    .unwrap(),
                )))
                .unwrap();
        }
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let overlap = point(24., 20.);
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter.hover_arc_handle(7, 0, overlap, 1.).unwrap(),
            Some((top_id.clone(), 2)),
            "the topmost Arc and last overlapping control must win"
        );
        assert_eq!(
            adapter.hover_transform_handle(7, 0, overlap, 1.).unwrap(),
            Some((top_id.clone(), 2))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);

        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter.hover_transform_handle(7, 0, overlap, 1.).unwrap(),
            Some((bottom_id.clone(), 2)),
            "the selected Arc must win before a topmost unselected overlap"
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter.hover_arc_handle(7, 0, overlap, 1.).unwrap(),
            Some((top_id.clone(), 2))
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(adapter.hover_arc_handle(7, 0, overlap, 1.).unwrap(), None);
        adapter.set_tool(AnnotationTool::Arc).unwrap();
        assert_eq!(adapter.hover_arc_handle(7, 0, overlap, 1.).unwrap(), None);
    }

    #[test]
    fn vertex_path_hover_handles_are_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_id = MarkupId::new("vertex-path:hover-bottom").unwrap();
        let top_id = MarkupId::new("vertex-path:hover-top").unwrap();
        let points = vec![
            point(20., 20.),
            point(100., 20.),
            point(20., 20.),
            point(100., 80.),
        ];
        for id in [&bottom_id, &top_id] {
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::VertexPath(
                    VertexPathAnnotation::new(
                        id.clone(),
                        0,
                        points.clone(),
                        VertexPathKind::Polyline,
                        RectangleAppearance::default(),
                    )
                    .unwrap(),
                )))
                .unwrap();
        }
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter
                .hover_vertex_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            Some((top_id.clone(), 2)),
            "the topmost path and last overlapping vertex must win"
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter.hover_transform_handle(7, 0, points[0], 1.).unwrap(),
            Some((bottom_id.clone(), 2))
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_vertex_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            Some((top_id.clone(), 2))
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_vertex_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Polyline).unwrap();
        assert_eq!(
            adapter
                .hover_vertex_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            None
        );
    }

    #[test]
    fn measurement_path_hover_handles_are_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        let bottom_id = MarkupId::new("measurement-path:hover-bottom").unwrap();
        let top_id = MarkupId::new("measurement-path:hover-top").unwrap();
        let points = vec![
            point(20., 20.),
            point(100., 20.),
            point(20., 20.),
            point(100., 80.),
        ];
        let calibration = LengthCalibration::from_scale(72., 1., "m", 2, false).unwrap();
        for (id, kind) in [
            (&bottom_id, MeasurementPathKind::Polylength),
            (&top_id, MeasurementPathKind::Area),
        ] {
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(
                    Annotation::MeasurementPath(
                        MeasurementPathAnnotation::new(
                            id.clone(),
                            0,
                            points.clone(),
                            kind,
                            calibration.clone(),
                            RectangleAppearance::default(),
                        )
                        .unwrap(),
                    ),
                ))
                .unwrap();
        }
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let before = adapter.snapshot(7).unwrap();

        assert_eq!(
            adapter
                .hover_measurement_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            Some((top_id.clone(), 2)),
            "the topmost measurement and last overlapping vertex must win"
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter.hover_transform_handle(7, 0, points[0], 1.).unwrap(),
            Some((bottom_id.clone(), 2))
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_measurement_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            Some((top_id.clone(), 2))
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_measurement_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Area).unwrap();
        assert_eq!(
            adapter
                .hover_measurement_path_handle(7, 0, points[0], 1.)
                .unwrap(),
            None
        );
    }

    #[test]
    fn snapshot_hover_handles_are_selected_first_topmost_and_read_only() {
        let mut adapter = AnnotationAdapter::default();
        adapter.set_observed_pixels_per_point(1.).unwrap();
        let bottom_id = MarkupId::new("snapshot:hover-bottom").unwrap();
        let top_id = MarkupId::new("snapshot:hover-top").unwrap();
        let rect = PdfRect::new(20., 20., 100., 80.).unwrap();
        let asset = DecodedRgbaAsset::new(2, 2, vec![255; 16]).unwrap();
        let bottom =
            SnapshotAnnotation::new(bottom_id.clone(), 0, rect, asset.clone(), 1.).unwrap();
        let top = SnapshotAnnotation::new(top_id.clone(), 0, rect, asset, 1.).unwrap();
        for snapshot in [bottom.clone(), top.clone()] {
            adapter
                .documents
                .entry(7)
                .or_default()
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Snapshot(
                    snapshot,
                )))
                .unwrap();
        }
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();
        let before = adapter.snapshot(7).unwrap();
        let handle = RectangleResizeHandle::SouthEast;
        let handle_index = RectangleResizeHandle::ALL
            .iter()
            .position(|candidate| *candidate == handle)
            .unwrap();
        let resize_point = snapshot_resize_handle_point(&top, handle);

        assert_eq!(
            adapter
                .hover_snapshot_handle(7, 0, resize_point, 1.)
                .unwrap(),
            Some((top_id.clone(), handle_index))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before);
        let rotation_point = snapshot_rotation_handle_point(&top, 1.).unwrap();
        assert_eq!(
            adapter
                .hover_snapshot_handle(7, 0, rotation_point, 1.)
                .unwrap(),
            None,
            "an unselected Snapshot must withhold its rotation control"
        );

        assert!(adapter.documents.get_mut(&7).unwrap().select(&bottom_id));
        assert_eq!(
            adapter
                .hover_transform_handle(7, 0, resize_point, 1.)
                .unwrap(),
            Some((bottom_id.clone(), handle_index))
        );
        let bottom_rotation = snapshot_rotation_handle_point(&bottom, 1.).unwrap();
        assert_eq!(
            adapter
                .hover_snapshot_handle(7, 0, bottom_rotation, 1.)
                .unwrap(),
            Some((bottom_id.clone(), RectangleResizeHandle::ALL.len()))
        );
        adapter
            .set_primary_selected_locked(7, &bottom_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_snapshot_handle(7, 0, resize_point, 1.)
                .unwrap(),
            Some((top_id.clone(), handle_index))
        );
        assert!(adapter.documents.get_mut(&7).unwrap().select(&top_id));
        adapter
            .set_primary_selected_locked(7, &top_id, true)
            .unwrap();
        adapter.documents.get_mut(&7).unwrap().clear_selection();
        assert_eq!(
            adapter
                .hover_snapshot_handle(7, 0, resize_point, 1.)
                .unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Snapshot).unwrap();
        assert_eq!(
            adapter
                .hover_snapshot_handle(7, 0, resize_point, 1.)
                .unwrap(),
            None
        );
    }

    #[test]
    fn cloud_plus_direct_pointer_routes_composite_bodies_handles_preview_and_history() {
        let mut adapter = AnnotationAdapter::default();
        let id = MarkupId::new("cloud-plus:pointer").unwrap();
        let original = CloudPlusAnnotation::new(
            id.clone(),
            0,
            vec![
                point(10., 10.),
                point(50., 10.),
                point(50., 50.),
                point(10., 50.),
            ],
            2.,
            vec![point(50., 30.), point(75., 30.), point(100., 30.)],
            PdfRect::new(100., 20., 40., 20.).unwrap(),
            "Cloud+ pointer",
            default_cloud_plus_appearance().unwrap(),
        )
        .unwrap();
        let document = adapter.documents.entry(7).or_default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::CloudPlus(
                original.clone(),
            )))
            .unwrap();
        document.clear_selection();
        adapter.set_tool(AnnotationTool::Select).unwrap();

        let before_hover = adapter.snapshot(7).unwrap();
        assert_eq!(
            adapter
                .hover_cloud_plus_handle(7, 0, point(50., 10.), 1.)
                .unwrap(),
            Some((id.clone(), 1))
        );
        assert_eq!(adapter.snapshot(7).unwrap(), before_hover);
        adapter.set_tool(AnnotationTool::Rectangle).unwrap();
        assert_eq!(
            adapter
                .hover_cloud_plus_handle(7, 0, point(50., 10.), 1.)
                .unwrap(),
            None
        );
        adapter.set_tool(AnnotationTool::Select).unwrap();

        assert_eq!(
            adapter.pointer_down(7, 0, 1, point(62., 30.), 1.).unwrap(),
            PointerPhaseOutcome::SelectionChanged(Some(id.clone())),
            "the leader body selects the composite but remains adjust-only"
        );
        assert!(adapter.active.is_none());

        let history_before = adapter.history_depths(7);
        assert_eq!(
            adapter.pointer_down(7, 0, 2, point(50., 10.), 1.).unwrap(),
            PointerPhaseOutcome::GestureStarted
        );
        adapter.pointer_move(2, point(64., 6.)).unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert!(preview.draft);
        assert_eq!(preview.cloud_points[1], point(64., 6.));
        assert_ne!(preview.leader_points, original.leader_points());
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 1,
            }
        );
        assert_eq!(adapter.history_depths(7), history_before);
        adapter.cancel(PointerCancelReason::ToolChanged).unwrap();
        assert!(adapter.snapshot(7).unwrap().cloud_pluses[0].same_persisted_state_as(&original));
        assert_eq!(adapter.history_depths(7), history_before);

        adapter.pointer_down(7, 0, 3, point(120., 30.), 1.).unwrap();
        adapter.pointer_move(3, point(140., 42.)).unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_eq!(preview.text_box, PdfRect::new(120., 32., 40., 20.).unwrap());
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Move {
                chrome_visible: true,
            }
        );
        assert_eq!(adapter.history_depths(7), history_before);
        adapter.pointer_up(3, point(140., 42.)).unwrap();
        let moved_snapshot = adapter.snapshot(7).unwrap();
        let moved = &moved_snapshot.cloud_pluses[0];
        assert_eq!(moved.text_box, preview.text_box);
        assert_eq!(moved.leader_points(), preview.leader_points.as_slice());
        assert_eq!(moved.content(), original.content());
        assert_eq!(moved.appearance, original.appearance);
        assert_eq!(adapter.history_depths(7), (history_before.0 + 1, 0));

        let knee = moved.leader_points()[1];
        let next_knee = point(knee.x + 12., knee.y + 8.);
        adapter.pointer_down(7, 0, 4, knee, 1.).unwrap();
        adapter.pointer_move(4, next_knee).unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_eq!(preview.leader_points[1], next_knee);
        adapter.pointer_up(4, next_knee).unwrap();
        assert_eq!(
            adapter.snapshot(7).unwrap().cloud_pluses[0].leader_points()[1],
            next_knee
        );
        assert_eq!(adapter.history_depths(7), (history_before.0 + 2, 0));

        let before_group = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        adapter.pointer_down(7, 0, 5, point(30., 10.), 1.).unwrap();
        adapter.pointer_move(5, point(40., 16.)).unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_eq!(preview.cloud_points[0], point(20., 16.));
        assert_eq!(
            preview.text_box,
            PdfRect::new(
                before_group.text_box.x + 10.,
                before_group.text_box.y + 6.,
                before_group.text_box.width,
                before_group.text_box.height,
            )
            .unwrap()
        );
        assert_eq!(
            preview.leader_points[1],
            point(next_knee.x + 10., next_knee.y + 6.)
        );
        adapter.pointer_up(5, point(40., 16.)).unwrap();
        assert!(
            adapter.snapshot(7).unwrap().cloud_pluses[0].same_persisted_state_as(
                &CloudPlusAnnotation::new(
                    id.clone(),
                    0,
                    preview.cloud_points.clone(),
                    preview.border_effect_intensity,
                    preview.leader_points.clone(),
                    preview.text_box,
                    preview.content.clone(),
                    preview.appearance.clone(),
                )
                .unwrap()
            )
        );

        let before_resize = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        let north =
            axis_aligned_resize_handle_point(before_resize.text_box, RectangleResizeHandle::North);
        adapter.pointer_down(7, 0, 6, north, 1.).unwrap();
        adapter
            .pointer_move(6, point(north.x, north.y + 10.))
            .unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_eq!(preview.text_box.height, before_resize.text_box.height + 10.);
        assert_ne!(preview.leader_points, before_resize.leader_points());
        adapter
            .pointer_up(6, point(north.x, north.y + 10.))
            .unwrap();

        let before_connection = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        let connection = *before_connection.leader_points().last().unwrap();
        assert_eq!(
            adapter
                .hover_cloud_plus_handle(7, 0, connection, 1.)
                .unwrap(),
            Some((id.clone(), 14)),
            "leader connection must have reverse-order priority over its overlapping resize handle"
        );
        let hinted_connection = point(connection.x + 35., connection.y + 18.);
        adapter.pointer_down(7, 0, 7, connection, 1.).unwrap();
        adapter.pointer_move(7, hinted_connection).unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_eq!(preview.text_box, before_connection.text_box);
        assert_eq!(
            preview.feedback,
            SceneInteractionFeedback::Transform {
                chrome_visible: true,
                active_handle: 14,
            }
        );
        assert_ne!(*preview.leader_points.last().unwrap(), hinted_connection);
        assert!(
            preview.leader_points.last().unwrap().x == preview.text_box.x
                || preview.leader_points.last().unwrap().x
                    == preview.text_box.x + preview.text_box.width,
            "connection drag must reroute to a text-box edge and win over its overlapping resize handle"
        );
        adapter.pointer_up(7, hinted_connection).unwrap();

        let before_tip = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        let tip = before_tip.leader_points()[0];
        let raw_tip = point(tip.x - 30., tip.y + 15.);
        adapter.pointer_down(7, 0, 8, tip, 1.).unwrap();
        adapter.pointer_move(8, raw_tip).unwrap();
        let preview = adapter.document_scene(7, 0).cloud_pluses.remove(0);
        assert_ne!(preview.leader_points[0], raw_tip);
        assert_eq!(preview.text_box, before_tip.text_box);
        adapter.pointer_up(8, raw_tip).unwrap();

        let history_after_edits = adapter.history_depths(7);
        adapter
            .documents
            .get_mut(&7)
            .unwrap()
            .apply_command(AnnotationCommand::SetLocked {
                id: id.clone(),
                locked: true,
            })
            .unwrap();
        let locked = adapter.snapshot(7).unwrap().cloud_pluses[0].clone();
        assert_eq!(
            adapter
                .pointer_down(7, 0, 9, locked.cloud_points()[0], 1.)
                .unwrap(),
            PointerPhaseOutcome::SelectionChanged(Some(id))
        );
        assert!(adapter.active.is_none());
        assert_eq!(adapter.history_depths(7).0, history_after_edits.0 + 1);
    }
}
