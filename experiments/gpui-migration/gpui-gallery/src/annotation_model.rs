//! Deterministic annotation domain slice for migration qualification.
//!
//! Geometry is stored in PDF points. Gesture updates produce a preview and do
//! not mutate the committed document until `commit_gesture` succeeds. This
//! module deliberately has no GPUI, renderer, or PDF persistence dependency.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    error::Error,
    fmt,
    sync::Arc,
};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::selection_geometry::{
    SelectionMarquee, SelectionPath, SelectionPoint, geometry_selected, selection_after,
};
use crate::page_geometry::{PageCoordinateSpace, Rotation as CoordinateRotation};

pub const DEFAULT_HISTORY_LIMIT: usize = 100;
pub const MIN_RECT_SIZE_PT: f64 = 1.0;
const MIN_SNAPSHOT_SIZE_PT: f64 = 2.0;
pub const MIN_RECT_CREATE_SIZE_PT: f64 = 2.0;
pub const MIN_STRAIGHT_LINE_LENGTH_PT: f64 = 2.0;
pub const ROTATION_HANDLE_OFFSET_PT: f64 = 12.0;
pub const MAX_STREAMED_PATH_POINTS: usize = 100_000;
pub const MAX_COALESCED_PEN_SAMPLES: usize = 4_096;
pub const MAX_TEXT_BOX_BYTES: usize = 64 * 1024;
pub const MAX_FONT_FAMILY_BYTES: usize = 128;
pub const MAX_MEASUREMENT_UNIT_BYTES: usize = 32;
pub const MAX_MEASUREMENT_LABEL_BYTES: usize = 256;
pub const MAX_DECODED_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION_PX: u32 = 8_192;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum PageRotation {
    Degrees0,
    Degrees90,
    Degrees180,
    Degrees270,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageRotationDirection {
    Left,
    Right,
}

impl PageRotation {
    pub fn from_degrees(degrees: i64) -> Result<Self, AnnotationError> {
        match degrees.rem_euclid(360) {
            0 => Ok(Self::Degrees0),
            90 => Ok(Self::Degrees90),
            180 => Ok(Self::Degrees180),
            270 => Ok(Self::Degrees270),
            value => Err(AnnotationError::InvalidGeometry(format!(
                "page rotation must be a quarter turn, received {value} degrees"
            ))),
        }
    }

    pub fn degrees(self) -> i64 {
        match self {
            Self::Degrees0 => 0,
            Self::Degrees90 => 90,
            Self::Degrees180 => 180,
            Self::Degrees270 => 270,
        }
    }

    pub fn rotate(self, direction: PageRotationDirection) -> Self {
        let delta = match direction {
            PageRotationDirection::Left => -90,
            PageRotationDirection::Right => 90,
        };
        Self::from_degrees(self.degrees() + delta)
            .expect("a quarter-turn delta must remain canonical")
    }

    pub fn delta_from(self, source: Self) -> Self {
        Self::from_degrees(self.degrees() - source.degrees())
            .expect("canonical rotations must produce a canonical delta")
    }

    pub fn swaps_axes(self) -> bool {
        matches!(self, Self::Degrees90 | Self::Degrees270)
    }

    pub fn quarter_turns(self) -> u8 {
        (self.degrees() / 90) as u8
    }
}

/// Converts between the GPUI page surface's top-left pixel space and the PDF
/// page's bottom-left point space. Window origin and scroll offset stay in the
/// GPUI adapter so this transform remains deterministic and testable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageTransform {
    page_width_pt: f64,
    page_height_pt: f64,
    pixels_per_point: f64,
    rotation: PageRotation,
    view_box_x: f64,
    view_box_y: f64,
    user_unit: f64,
}

impl PageTransform {
    pub fn new(page_height_pt: f64, pixels_per_point: f64) -> Result<Self, AnnotationError> {
        require_finite("page_height_pt", page_height_pt)?;
        require_finite("pixels_per_point", pixels_per_point)?;
        if page_height_pt <= 0.0 || pixels_per_point <= 0.0 {
            return Err(AnnotationError::InvalidGeometry(
                "page height and scale must be positive".into(),
            ));
        }
        Ok(Self {
            page_width_pt: 0.0,
            page_height_pt,
            pixels_per_point,
            rotation: PageRotation::Degrees0,
            view_box_x: 0.0,
            view_box_y: 0.0,
            user_unit: 1.0,
        })
    }

    pub fn new_rotated(
        page_width_pt: f64,
        page_height_pt: f64,
        pixels_per_point: f64,
        rotation: PageRotation,
    ) -> Result<Self, AnnotationError> {
        require_finite("page_width_pt", page_width_pt)?;
        let mut transform = Self::new(page_height_pt, pixels_per_point)?;
        if page_width_pt <= 0.0 {
            return Err(AnnotationError::InvalidGeometry(
                "page width must be positive".into(),
            ));
        }
        transform.page_width_pt = page_width_pt;
        transform.rotation = rotation;
        Ok(transform)
    }

    pub fn from_page_coordinate_space(
        space: PageCoordinateSpace,
        pixels_per_point: f64,
    ) -> Result<Self, AnnotationError> {
        let view_box = space.view_box();
        let rotation = match space.rotation() {
            CoordinateRotation::Degrees0 => PageRotation::Degrees0,
            CoordinateRotation::Degrees90 => PageRotation::Degrees90,
            CoordinateRotation::Degrees180 => PageRotation::Degrees180,
            CoordinateRotation::Degrees270 => PageRotation::Degrees270,
        };
        let mut transform = Self::new_rotated(
            view_box.width,
            view_box.height,
            pixels_per_point,
            rotation,
        )?;
        transform.view_box_x = view_box.x;
        transform.view_box_y = view_box.y;
        transform.user_unit = space.user_unit();
        Ok(transform)
    }

    pub fn point_from_local_pixels(
        self,
        local_x: f64,
        local_y: f64,
    ) -> Result<PdfPoint, AnnotationError> {
        let pixels_per_raw_point = self.pixels_per_point * self.user_unit;
        let x = local_x / pixels_per_raw_point;
        let y = local_y / pixels_per_raw_point;
        let right = self.view_box_x + self.page_width_pt;
        let top = self.view_box_y + self.page_height_pt;
        match self.rotation {
            PageRotation::Degrees0 => PdfPoint::new(self.view_box_x + x, top - y),
            PageRotation::Degrees90 => PdfPoint::new(self.view_box_x + y, self.view_box_y + x),
            PageRotation::Degrees180 => PdfPoint::new(right - x, self.view_box_y + y),
            PageRotation::Degrees270 => PdfPoint::new(right - y, top - x),
        }
    }

    pub fn rect_to_local_pixels(self, rect: PdfRect) -> PdfRect {
        let corners = [
            PdfPoint {
                x: rect.x,
                y: rect.y,
            },
            PdfPoint {
                x: rect.x + rect.width,
                y: rect.y,
            },
            PdfPoint {
                x: rect.x,
                y: rect.y + rect.height,
            },
            PdfPoint {
                x: rect.x + rect.width,
                y: rect.y + rect.height,
            },
        ]
        .map(|point| self.point_to_local_pixels(point));
        let left = corners
            .iter()
            .map(|point| point.x)
            .fold(f64::INFINITY, f64::min);
        let right = corners
            .iter()
            .map(|point| point.x)
            .fold(f64::NEG_INFINITY, f64::max);
        let top = corners
            .iter()
            .map(|point| point.y)
            .fold(f64::INFINITY, f64::min);
        let bottom = corners
            .iter()
            .map(|point| point.y)
            .fold(f64::NEG_INFINITY, f64::max);
        PdfRect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }

    pub fn point_to_local_pixels(self, point: PdfPoint) -> PdfPoint {
        let right = self.view_box_x + self.page_width_pt;
        let top = self.view_box_y + self.page_height_pt;
        let (x, y) = match self.rotation {
            PageRotation::Degrees0 => (point.x - self.view_box_x, top - point.y),
            PageRotation::Degrees90 => (point.y - self.view_box_y, point.x - self.view_box_x),
            PageRotation::Degrees180 => (right - point.x, point.y - self.view_box_y),
            PageRotation::Degrees270 => (top - point.y, right - point.x),
        };
        PdfPoint {
            x: x * self.pixels_per_point * self.user_unit,
            y: y * self.pixels_per_point * self.user_unit,
        }
    }

    pub fn tolerance_points(self, tolerance_pixels: f64) -> Result<f64, AnnotationError> {
        validate_tolerance(tolerance_pixels)?;
        Ok(tolerance_pixels / (self.pixels_per_point * self.user_unit))
    }

    pub fn pixels_per_point(self) -> f64 {
        self.pixels_per_point
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfPoint {
    pub x: f64,
    pub y: f64,
}

impl PdfPoint {
    pub fn new(x: f64, y: f64) -> Result<Self, AnnotationError> {
        require_finite("point.x", x)?;
        require_finite("point.y", y)?;
        Ok(Self { x, y })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl PdfRect {
    /// The experiment PDF writer persists each rectangle edge as an `f32`
    /// real. Compare that representable edge tuple instead of the retained
    /// width/height subtraction, which can differ after an exact save/reopen.
    pub fn same_pdf_geometry_as(self, other: Self) -> bool {
        let persisted_edges = |rect: Self| {
            [
                rect.x as f32,
                rect.y as f32,
                (rect.x + rect.width) as f32,
                (rect.y + rect.height) as f32,
            ]
        };
        persisted_edges(self)
            .into_iter()
            .zip(persisted_edges(other))
            .all(|(expected, actual)| expected == actual)
    }

    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, AnnotationError> {
        for (name, value) in [
            ("rect.x", x),
            ("rect.y", y),
            ("rect.width", width),
            ("rect.height", height),
        ] {
            require_finite(name, value)?;
        }
        if width < 0.0 || height < 0.0 {
            return Err(AnnotationError::InvalidGeometry(
                "rectangle dimensions must be nonnegative".into(),
            ));
        }
        Ok(Self {
            x: canonical_float(x),
            y: canonical_float(y),
            width: canonical_float(width),
            height: canonical_float(height),
        })
    }

    pub(crate) fn from_corners(start: PdfPoint, end: PdfPoint) -> Self {
        Self {
            x: canonical_float(start.x.min(end.x)),
            y: canonical_float(start.y.min(end.y)),
            width: canonical_float((end.x - start.x).abs()),
            height: canonical_float((end.y - start.y).abs()),
        }
    }

    fn translated(self, delta_x: f64, delta_y: f64) -> Self {
        Self {
            x: canonical_float(self.x + delta_x),
            y: canonical_float(self.y + delta_y),
            ..self
        }
    }

    fn center(self) -> PdfPoint {
        PdfPoint {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        }
    }

    fn resized_from_handle(self, handle: RectangleResizeHandle, point: PdfPoint) -> Self {
        let mut left = self.x;
        let mut bottom = self.y;
        let mut right = self.x + self.width;
        let mut top = self.y + self.height;
        if handle.affects_west() {
            left = point.x.min(right - MIN_RECT_CREATE_SIZE_PT);
        }
        if handle.affects_east() {
            right = point.x.max(left + MIN_RECT_CREATE_SIZE_PT);
        }
        if handle.affects_north() {
            top = point.y.max(bottom + MIN_RECT_CREATE_SIZE_PT);
        }
        if handle.affects_south() {
            bottom = point.y.min(top - MIN_RECT_CREATE_SIZE_PT);
        }
        Self {
            x: canonical_float(left),
            y: canonical_float(bottom),
            width: canonical_float(right - left),
            height: canonical_float(top - bottom),
        }
    }

    pub(crate) fn rotated_resize_from_handle(
        self,
        rotation_degrees: f64,
        handle: RectangleResizeHandle,
        point: PdfPoint,
    ) -> Self {
        if rotation_degrees == 0.0 {
            return self.resized_from_handle(handle, point);
        }
        let anchor_before_world =
            rotate_point_around_rect_center(handle.opposite_anchor(self), self, -rotation_degrees);
        let local_point = rotate_point_around_rect_center(point, self, rotation_degrees);
        let resized = self.resized_from_handle(handle, local_point);
        let anchor_after_world = rotate_point_around_rect_center(
            handle.opposite_anchor(resized),
            resized,
            -rotation_degrees,
        );
        resized.translated(
            anchor_before_world.x - anchor_after_world.x,
            anchor_before_world.y - anchor_after_world.y,
        )
    }

    fn contains(self, point: PdfPoint, tolerance: f64) -> bool {
        point.x >= self.x - tolerance
            && point.x <= self.x + self.width + tolerance
            && point.y >= self.y - tolerance
            && point.y <= self.y + self.height + tolerance
    }

    fn near_perimeter(self, point: PdfPoint, tolerance: f64) -> bool {
        if !self.contains(point, tolerance) {
            return false;
        }
        (point.x - self.x).abs() <= tolerance
            || (point.x - (self.x + self.width)).abs() <= tolerance
            || (point.y - self.y).abs() <= tolerance
            || (point.y - (self.y + self.height)).abs() <= tolerance
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MarkupId(String);

impl MarkupId {
    pub fn new(value: impl Into<String>) -> Result<Self, AnnotationError> {
        let value = value.into();
        if value.is_empty() || value.trim() != value || value.chars().any(char::is_control) {
            return Err(AnnotationError::InvalidMarkupId);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MarkupId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RectangleAppearance {
    stroke_color: String,
    stroke_width_pt: f64,
    fill_color: Option<String>,
    opacity: f64,
    fill_opacity: f64,
    stroke_style: StrokeStyle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StrokeStyle {
    Solid,
    Dashed,
    Dotted,
}

impl RectangleAppearance {
    pub fn new(
        stroke_color: impl Into<String>,
        stroke_width_pt: f64,
        fill_color: Option<impl Into<String>>,
        opacity: f64,
    ) -> Result<Self, AnnotationError> {
        require_finite("stroke_width_pt", stroke_width_pt)?;
        require_finite("opacity", opacity)?;
        if stroke_width_pt < 0.0 {
            return Err(AnnotationError::InvalidAppearance(
                "stroke width must be nonnegative".into(),
            ));
        }
        if !(0.0..=1.0).contains(&opacity) {
            return Err(AnnotationError::InvalidAppearance(
                "opacity must be between 0 and 1".into(),
            ));
        }
        Ok(Self {
            stroke_color: normalize_color(stroke_color.into())?,
            stroke_width_pt: canonical_float(stroke_width_pt),
            fill_color: fill_color
                .map(Into::into)
                .map(normalize_color)
                .transpose()?,
            opacity: canonical_float(opacity),
            fill_opacity: 1.0,
            stroke_style: StrokeStyle::Solid,
        })
    }

    pub fn stroke_color(&self) -> &str {
        &self.stroke_color
    }

    pub fn stroke_width_pt(&self) -> f64 {
        self.stroke_width_pt
    }

    pub fn fill_color(&self) -> Option<&str> {
        self.fill_color.as_deref()
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    pub fn fill_opacity(&self) -> f64 {
        self.fill_opacity
    }

    pub fn with_fill_opacity(mut self, fill_opacity: f64) -> Result<Self, AnnotationError> {
        require_finite("fill_opacity", fill_opacity)?;
        if !(0.0..=1.0).contains(&fill_opacity) {
            return Err(AnnotationError::InvalidAppearance(
                "fill opacity must be between 0 and 1".into(),
            ));
        }
        self.fill_opacity = canonical_float(fill_opacity);
        Ok(self)
    }

    pub fn stroke_style(&self) -> StrokeStyle {
        self.stroke_style
    }

    pub fn with_stroke_style(mut self, stroke_style: StrokeStyle) -> Self {
        self.stroke_style = stroke_style;
        self
    }
}

impl Default for RectangleAppearance {
    fn default() -> Self {
        Self {
            stroke_color: "#ff0000".into(),
            stroke_width_pt: 1.0,
            fill_color: None,
            opacity: 1.0,
            fill_opacity: 1.0,
            stroke_style: StrokeStyle::Solid,
        }
    }
}

pub const PENDING_REDACTION_STATUS: &str = "Pending redaction mark — saving keeps the underlying PDF content; this mark does not securely remove text or graphics.";

#[derive(Clone, Debug, PartialEq)]
pub struct RedactAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub rect: PdfRect,
    redaction_color: String,
    overlay_text: Option<String>,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

impl RedactAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        rect: PdfRect,
        redaction_color: impl Into<String>,
        overlay_text: Option<impl Into<String>>,
        appearance: RectangleAppearance,
    ) -> Result<Self, AnnotationError> {
        if rect.width <= MIN_RECT_CREATE_SIZE_PT || rect.height <= MIN_RECT_CREATE_SIZE_PT {
            return Err(AnnotationError::InvalidGeometry(
                "redaction dimensions must be strictly greater than two points".into(),
            ));
        }
        if appearance.stroke_color != "#ff0000"
            || appearance.stroke_width_pt != 1.0
            || appearance.fill_color.as_deref() != Some("#000000")
            || appearance.opacity != 0.35
            || appearance.fill_opacity != 0.35
            || appearance.stroke_style != StrokeStyle::Solid
        {
            return Err(AnnotationError::InvalidAppearance(
                "pending redactions use the fixed red border and translucent black fill".into(),
            ));
        }
        Ok(Self {
            id,
            page_index,
            rect,
            redaction_color: normalize_color(redaction_color.into())?,
            overlay_text: overlay_text.map(Into::into),
            appearance,
            locked: false,
        })
    }

    pub fn redaction_color(&self) -> &str {
        &self.redaction_color
    }

    pub fn overlay_text(&self) -> Option<&str> {
        self.overlay_text.as_deref()
    }

    pub fn pending_status_text(&self) -> &'static str {
        PENDING_REDACTION_STATUS
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        self.id == other.id
            && self.page_index == other.page_index
            && self.rect.same_pdf_geometry_as(other.rect)
            && self.redaction_color == other.redaction_color
            && self.overlay_text == other.overlay_text
            && self.appearance == other.appearance
            && self.locked == other.locked
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RectangleAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub rect: PdfRect,
    pub rotation_degrees: f64,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

impl RectangleAnnotation {
    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        self.id == other.id
            && self.page_index == other.page_index
            && self.rect.same_pdf_geometry_as(other.rect)
            && self.rotation_degrees == other.rotation_degrees
            && self.appearance == other.appearance
            && self.locked == other.locked
    }

    fn world_to_local(&self, point: PdfPoint) -> PdfPoint {
        rotate_point_around_rect_center(point, self.rect, self.rotation_degrees)
    }

    fn rotation_handle_world_point(&self, offset_pt: f64) -> PdfPoint {
        let point = PdfPoint {
            x: self.rect.x + self.rect.width / 2.0,
            y: self.rect.y + self.rect.height + offset_pt,
        };
        rotate_point_around_rect_center(point, self.rect, -self.rotation_degrees)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EllipseAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub rect: PdfRect,
    pub rotation_degrees: f64,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

impl EllipseAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        rect: PdfRect,
        appearance: RectangleAppearance,
    ) -> Result<Self, AnnotationError> {
        if rect.width < MIN_RECT_CREATE_SIZE_PT || rect.height < MIN_RECT_CREATE_SIZE_PT {
            return Err(AnnotationError::InvalidGeometry(
                "ellipse dimensions must exceed the placement threshold".into(),
            ));
        }
        Ok(Self {
            id,
            page_index,
            rect,
            rotation_degrees: 0.,
            appearance,
            locked: false,
        })
    }

    pub fn constrained_end(start: PdfPoint, point: PdfPoint) -> PdfPoint {
        let delta_x = point.x - start.x;
        let delta_y = point.y - start.y;
        let diameter = delta_x.abs().max(delta_y.abs());
        PdfPoint {
            x: start.x + diameter * if delta_x == 0. { 1. } else { delta_x.signum() },
            y: start.y + diameter * if delta_y == 0. { 1. } else { delta_y.signum() },
        }
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        self.id == other.id
            && self.page_index == other.page_index
            && self.rect.same_pdf_geometry_as(other.rect)
            && (self.rotation_degrees as f32) == (other.rotation_degrees as f32)
            && self.appearance == other.appearance
            && self.locked == other.locked
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArcControlPoint {
    Start,
    Mid,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArcAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub mid: PdfPoint,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

impl ArcAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        end: PdfPoint,
        mid: PdfPoint,
        appearance: RectangleAppearance,
    ) -> Result<Self, AnnotationError> {
        for (name, value) in [
            ("arc.start.x", start.x),
            ("arc.start.y", start.y),
            ("arc.end.x", end.x),
            ("arc.end.y", end.y),
            ("arc.mid.x", mid.x),
            ("arc.mid.y", mid.y),
        ] {
            require_finite(name, value)?;
        }
        if point_distance(start, end) <= MIN_STRAIGHT_LINE_LENGTH_PT {
            return Err(AnnotationError::InvalidGeometry(
                "arc endpoints must be more than two points apart".into(),
            ));
        }
        let annotation = Self {
            id,
            page_index,
            start,
            end,
            mid,
            appearance,
            locked: false,
        };
        annotation.circle_geometry()?;
        Ok(annotation)
    }

    pub fn constrained_midpoint(
        start: PdfPoint,
        end: PdfPoint,
        pointer: PdfPoint,
        minimum_bulge_pt: f64,
        snap_quarter_turn: bool,
    ) -> Result<PdfPoint, AnnotationError> {
        require_finite("arc.minimum_bulge", minimum_bulge_pt)?;
        if minimum_bulge_pt <= 0. {
            return Err(AnnotationError::InvalidGeometry(
                "arc minimum bulge must be positive".into(),
            ));
        }
        let delta_x = end.x - start.x;
        let delta_y = end.y - start.y;
        let chord = delta_x.hypot(delta_y);
        if chord <= MIN_STRAIGHT_LINE_LENGTH_PT {
            return Err(AnnotationError::InvalidGeometry(
                "arc endpoints must be more than two points apart".into(),
            ));
        }
        let center = PdfPoint {
            x: (start.x + end.x) * 0.5,
            y: (start.y + end.y) * 0.5,
        };
        let normal = PdfPoint {
            x: -delta_y / chord,
            y: delta_x / chord,
        };
        let offset = (pointer.x - center.x) * normal.x + (pointer.y - center.y) * normal.y;
        let sign = if offset < 0. { -1. } else { 1. };
        let mut magnitude = offset.abs().max(minimum_bulge_pt);
        if snap_quarter_turn {
            let candidates = [
                chord * 0.5 * (std::f64::consts::FRAC_PI_8).tan(),
                chord * 0.5,
                chord * 0.5 * (3. * std::f64::consts::FRAC_PI_8).tan(),
            ];
            magnitude = candidates
                .into_iter()
                .min_by(|left, right| {
                    (left - magnitude)
                        .abs()
                        .total_cmp(&(right - magnitude).abs())
                })
                .expect("the Arc snap set is nonempty")
                .max(minimum_bulge_pt);
        }
        PdfPoint::new(
            canonical_float(center.x + normal.x * magnitude * sign),
            canonical_float(center.y + normal.y * magnitude * sign),
        )
    }

    pub fn rect(&self) -> PdfRect {
        let (center, radius, _, _) = self
            .circle_geometry()
            .expect("a retained Arc always has valid circle geometry");
        PdfRect::new(
            center.x - radius,
            center.y - radius,
            radius * 2.,
            radius * 2.,
        )
        .expect("a retained Arc circle has finite positive bounds")
    }

    pub fn angle1_degrees(&self) -> f64 {
        self.circle_geometry()
            .expect("a retained Arc always has valid circle geometry")
            .2
    }

    pub fn angle2_degrees(&self) -> f64 {
        let (_, _, start_angle, sweep) = self
            .circle_geometry()
            .expect("a retained Arc always has valid circle geometry");
        canonical_float(start_angle + sweep)
    }

    pub fn sweep_degrees(&self) -> f64 {
        self.circle_geometry()
            .expect("a retained Arc always has valid circle geometry")
            .3
    }

    pub fn sampled_path(&self, segments: usize) -> Vec<PdfPoint> {
        let (center, radius, start_angle, sweep) = self
            .circle_geometry()
            .expect("a retained Arc always has valid circle geometry");
        let segments = segments.max(1);
        let mut points = (0..=segments)
            .map(|index| {
                let angle = (start_angle + sweep * index as f64 / segments as f64).to_radians();
                PdfPoint {
                    x: canonical_float(center.x + radius * angle.cos()),
                    y: canonical_float(center.y + radius * angle.sin()),
                }
            })
            .collect::<Vec<_>>();
        points[0] = self.start;
        points[segments] = self.end;
        points
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && [
                (self.start, other.start),
                (self.end, other.end),
                (self.mid, other.mid),
            ]
            .into_iter()
            .all(|(left, right)| {
                (left.x - right.x).abs() <= PDF_NUMBER_TOLERANCE
                    && (left.y - right.y).abs() <= PDF_NUMBER_TOLERANCE
            })
            && self.appearance == other.appearance
            && self.locked == other.locked
    }

    fn circle_geometry(&self) -> Result<(PdfPoint, f64, f64, f64), AnnotationError> {
        let determinant = 2.
            * (self.start.x * (self.end.y - self.mid.y)
                + self.end.x * (self.mid.y - self.start.y)
                + self.mid.x * (self.start.y - self.end.y));
        if determinant.abs() <= f64::EPSILON {
            return Err(AnnotationError::InvalidGeometry(
                "arc control points must not be collinear".into(),
            ));
        }
        let start_squared = self.start.x * self.start.x + self.start.y * self.start.y;
        let end_squared = self.end.x * self.end.x + self.end.y * self.end.y;
        let mid_squared = self.mid.x * self.mid.x + self.mid.y * self.mid.y;
        let center = PdfPoint::new(
            (start_squared * (self.end.y - self.mid.y)
                + end_squared * (self.mid.y - self.start.y)
                + mid_squared * (self.start.y - self.end.y))
                / determinant,
            (start_squared * (self.mid.x - self.end.x)
                + end_squared * (self.start.x - self.mid.x)
                + mid_squared * (self.end.x - self.start.x))
                / determinant,
        )?;
        let radius = point_distance(center, self.start);
        if !radius.is_finite() || radius <= 0. {
            return Err(AnnotationError::InvalidGeometry(
                "arc radius must be positive and finite".into(),
            ));
        }
        let start_angle = normalize_degrees(
            (self.start.y - center.y)
                .atan2(self.start.x - center.x)
                .to_degrees(),
        );
        let end_angle = normalize_degrees(
            (self.end.y - center.y)
                .atan2(self.end.x - center.x)
                .to_degrees(),
        );
        let mid_angle = normalize_degrees(
            (self.mid.y - center.y)
                .atan2(self.mid.x - center.x)
                .to_degrees(),
        );
        let ccw_sweep = normalize_degrees(end_angle - start_angle);
        let mid_from_start = normalize_degrees(mid_angle - start_angle);
        let sweep = if mid_from_start <= ccw_sweep + 0.000_001 {
            ccw_sweep
        } else {
            ccw_sweep - 360.
        };
        Ok((center, canonical_float(radius), start_angle, sweep))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnnotationKind {
    Rectangle,
    Redact,
    Ellipse,
    Arc,
    Line,
    Arrow,
    Polyline,
    Polygon,
    Polylength,
    Area,
    Cloud,
    CloudPlus,
    Callout,
    Pen,
    TextBox,
    Dimension,
    Length,
    Image,
    Snapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VertexPathKind {
    Polyline,
    Polygon,
}

impl VertexPathKind {
    pub fn minimum_points(self) -> usize {
        match self {
            Self::Polyline => 2,
            Self::Polygon => 3,
        }
    }

    pub fn is_closed(self) -> bool {
        self == Self::Polygon
    }
}

impl From<VertexPathKind> for AnnotationKind {
    fn from(kind: VertexPathKind) -> Self {
        match kind {
            VertexPathKind::Polyline => Self::Polyline,
            VertexPathKind::Polygon => Self::Polygon,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VertexPathAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    points: Vec<PdfPoint>,
    pub kind: VertexPathKind,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

pub const DEFAULT_CLOUD_SCALLOP_RADIUS_PT: f64 = 14.28;

#[derive(Clone, Debug, PartialEq)]
pub struct CloudAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    points: Vec<PdfPoint>,
    border_effect_intensity: f64,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

impl CloudAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        points: Vec<PdfPoint>,
        border_effect_intensity: f64,
        appearance: RectangleAppearance,
    ) -> Result<Self, AnnotationError> {
        validate_vertex_path(&points, VertexPathKind::Polygon)?;
        require_finite("cloud.border_effect_intensity", border_effect_intensity)?;
        if !(0.0..=4.0).contains(&border_effect_intensity) {
            return Err(AnnotationError::InvalidAppearance(
                "cloud intensity must be between 0 and 4".into(),
            ));
        }
        if appearance.fill_color().is_some() {
            return Err(AnnotationError::InvalidAppearance(
                "cloud annotations do not support a fill".into(),
            ));
        }
        Ok(Self {
            id,
            page_index,
            points,
            border_effect_intensity: canonical_float(border_effect_intensity),
            appearance,
            locked: false,
        })
    }

    pub fn points(&self) -> &[PdfPoint] {
        &self.points
    }

    pub fn border_effect_intensity(&self) -> f64 {
        self.border_effect_intensity
    }

    /// A deterministic sampled scallop outline used for hit testing and the
    /// experiment-owned PDF appearance stream. The control vertices remain the
    /// persisted edit geometry.
    pub fn scallop_path(&self) -> Vec<PdfPoint> {
        sampled_cloud_scallop_path(
            &self.points,
            DEFAULT_CLOUD_SCALLOP_RADIUS_PT * (self.border_effect_intensity / 2.0).max(0.25),
        )
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && self.border_effect_intensity == other.border_effect_intensity
            && self.appearance == other.appearance
            && self.locked == other.locked
            && self.points.len() == other.points.len()
            && self.points.iter().zip(&other.points).all(|(left, right)| {
                (left.x - right.x).abs() <= PDF_NUMBER_TOLERANCE
                    && (left.y - right.y).abs() <= PDF_NUMBER_TOLERANCE
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MeasurementPathKind {
    Polylength,
    Area,
}

impl MeasurementPathKind {
    pub fn minimum_points(self) -> usize {
        match self {
            Self::Polylength => 2,
            Self::Area => 3,
        }
    }

    pub fn is_closed(self) -> bool {
        self == Self::Area
    }
}

impl From<MeasurementPathKind> for AnnotationKind {
    fn from(kind: MeasurementPathKind) -> Self {
        match kind {
            MeasurementPathKind::Polylength => Self::Polylength,
            MeasurementPathKind::Area => Self::Area,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementPathAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    points: Vec<PdfPoint>,
    pub kind: MeasurementPathKind,
    calibration: LengthCalibration,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

impl MeasurementPathAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        points: Vec<PdfPoint>,
        kind: MeasurementPathKind,
        calibration: LengthCalibration,
        appearance: RectangleAppearance,
    ) -> Result<Self, AnnotationError> {
        validate_measurement_path(&points, kind)?;
        Ok(Self {
            id,
            page_index,
            points,
            kind,
            calibration,
            appearance,
            locked: false,
        })
    }

    pub fn points(&self) -> &[PdfPoint] {
        &self.points
    }

    pub fn calibration(&self) -> &LengthCalibration {
        &self.calibration
    }

    pub fn measured_value(&self) -> f64 {
        match self.kind {
            MeasurementPathKind::Polylength => canonical_float(
                self.points
                    .windows(2)
                    .map(|segment| {
                        ((segment[1].x - segment[0].x) * self.calibration.scale_x)
                            .hypot((segment[1].y - segment[0].y) * self.calibration.scale_y)
                    })
                    .sum(),
            ),
            MeasurementPathKind::Area => {
                let doubled_area = self
                    .points
                    .iter()
                    .zip(self.points.iter().cycle().skip(1))
                    .take(self.points.len())
                    .map(|(left, right)| {
                        let left_x = left.x * self.calibration.scale_x;
                        let left_y = left.y * self.calibration.scale_y;
                        let right_x = right.x * self.calibration.scale_x;
                        let right_y = right.y * self.calibration.scale_y;
                        left_x * right_y - right_x * left_y
                    })
                    .sum::<f64>();
                canonical_float(doubled_area.abs() / 2.0)
            }
        }
    }

    pub fn caption(&self) -> String {
        let value = self
            .calibration
            .scale_precision
            .format(self.measured_value());
        match self.kind {
            MeasurementPathKind::Polylength => {
                format!("{value} {}", self.calibration.unit)
            }
            MeasurementPathKind::Area => {
                format!("{value} {}^2", self.calibration.unit)
            }
        }
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && self.kind == other.kind
            && self.calibration == other.calibration
            && self.appearance == other.appearance
            && self.locked == other.locked
            && self.points.len() == other.points.len()
            && self.points.iter().zip(&other.points).all(|(left, right)| {
                (left.x - right.x).abs() <= PDF_NUMBER_TOLERANCE
                    && (left.y - right.y).abs() <= PDF_NUMBER_TOLERANCE
            })
    }
}

impl VertexPathAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        points: Vec<PdfPoint>,
        kind: VertexPathKind,
        appearance: RectangleAppearance,
    ) -> Result<Self, AnnotationError> {
        validate_vertex_path(&points, kind)?;
        Ok(Self {
            id,
            page_index,
            points,
            kind,
            appearance,
            locked: false,
        })
    }

    pub fn points(&self) -> &[PdfPoint] {
        &self.points
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && self.kind == other.kind
            && self.appearance == other.appearance
            && self.locked == other.locked
            && self.points.len() == other.points.len()
            && self.points.iter().zip(&other.points).all(|(left, right)| {
                (left.x - right.x).abs() <= PDF_NUMBER_TOLERANCE
                    && (left.y - right.y).abs() <= PDF_NUMBER_TOLERANCE
            })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineKind {
    Line,
    Arrow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEndpoint {
    Start,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StraightLineAppearance {
    stroke_color: String,
    stroke_width_pt: f64,
    opacity: f64,
    stroke_style: StrokeStyle,
}

impl StraightLineAppearance {
    pub fn new(
        stroke_color: impl Into<String>,
        stroke_width_pt: f64,
        opacity: f64,
        stroke_style: StrokeStyle,
    ) -> Result<Self, AnnotationError> {
        require_finite("straight_line.stroke_width_pt", stroke_width_pt)?;
        require_finite("straight_line.opacity", opacity)?;
        if !(0.25..=24.0).contains(&stroke_width_pt) {
            return Err(AnnotationError::InvalidAppearance(
                "straight-line width must be between 0.25 and 24 points".into(),
            ));
        }
        if !(0.0..=1.0).contains(&opacity) {
            return Err(AnnotationError::InvalidAppearance(
                "straight-line opacity must be between 0 and 1".into(),
            ));
        }
        Ok(Self {
            stroke_color: normalize_color(stroke_color.into())?,
            stroke_width_pt: canonical_float(stroke_width_pt),
            opacity: canonical_float(opacity),
            stroke_style,
        })
    }

    pub fn default_for(kind: LineKind) -> Self {
        Self {
            stroke_color: "#ff0000".into(),
            stroke_width_pt: match kind {
                LineKind::Line => 1.0,
                LineKind::Arrow => 0.5,
            },
            opacity: 1.0,
            stroke_style: StrokeStyle::Solid,
        }
    }

    pub fn stroke_color(&self) -> &str {
        &self.stroke_color
    }

    pub fn stroke_width_pt(&self) -> f64 {
        self.stroke_width_pt
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    pub fn stroke_style(&self) -> StrokeStyle {
        self.stroke_style
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StraightLineAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub kind: LineKind,
    pub appearance: StraightLineAppearance,
    pub locked: bool,
}

impl StraightLineAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        end: PdfPoint,
        kind: LineKind,
        appearance: StraightLineAppearance,
    ) -> Result<Self, AnnotationError> {
        for (name, value) in [
            ("straight_line.start.x", start.x),
            ("straight_line.start.y", start.y),
            ("straight_line.end.x", end.x),
            ("straight_line.end.y", end.y),
        ] {
            require_finite(name, value)?;
        }
        if point_distance(start, end) <= MIN_STRAIGHT_LINE_LENGTH_PT {
            return Err(AnnotationError::InvalidGeometry(
                "straight-line endpoints must be more than two points apart".into(),
            ));
        }
        Ok(Self {
            id,
            page_index,
            start,
            end,
            kind,
            appearance,
            locked: false,
        })
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && (self.start.x - other.start.x).abs() <= PDF_NUMBER_TOLERANCE
            && (self.start.y - other.start.y).abs() <= PDF_NUMBER_TOLERANCE
            && (self.end.x - other.end.x).abs() <= PDF_NUMBER_TOLERANCE
            && (self.end.y - other.end.y).abs() <= PDF_NUMBER_TOLERANCE
            && self.kind == other.kind
            && self.appearance == other.appearance
            && self.locked == other.locked
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PenAppearance {
    color: String,
    width_pt: f64,
    opacity: f64,
}

impl PenAppearance {
    pub fn new(
        color: impl Into<String>,
        width_pt: f64,
        opacity: f64,
    ) -> Result<Self, AnnotationError> {
        require_finite("pen.width_pt", width_pt)?;
        require_finite("pen.opacity", opacity)?;
        if width_pt <= 0.0 {
            return Err(AnnotationError::InvalidAppearance(
                "pen width must be positive".into(),
            ));
        }
        if !(0.0..=1.0).contains(&opacity) {
            return Err(AnnotationError::InvalidAppearance(
                "pen opacity must be between 0 and 1".into(),
            ));
        }
        Ok(Self {
            color: normalize_color(color.into())?,
            width_pt: canonical_float(width_pt),
            opacity: canonical_float(opacity),
        })
    }

    pub fn color(&self) -> &str {
        &self.color
    }

    pub fn width_pt(&self) -> f64 {
        self.width_pt
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PenAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    points: Vec<PdfPoint>,
    additional_paths: Vec<Vec<PdfPoint>>,
    pub appearance: PenAppearance,
    pub smooth_curves: bool,
    tool: InkTool,
    blend_mode: BlendMode,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InkTool {
    Pen,
    Highlight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlendMode {
    Normal,
    Multiply,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBoxStyle {
    font_family: String,
    font_size_pt: f64,
    color: String,
    opacity: f64,
    weight: u16,
    alignment: TextAlignment,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

impl TextBoxStyle {
    pub fn new(
        font_family: impl Into<String>,
        font_size_pt: f64,
        color: impl Into<String>,
        opacity: f64,
    ) -> Result<Self, AnnotationError> {
        require_finite("text.font_size_pt", font_size_pt)?;
        require_finite("text.opacity", opacity)?;
        if font_size_pt <= 0.0 {
            return Err(AnnotationError::InvalidAppearance(
                "text font size must be positive".into(),
            ));
        }
        if !(0.0..=1.0).contains(&opacity) {
            return Err(AnnotationError::InvalidAppearance(
                "text opacity must be between 0 and 1".into(),
            ));
        }
        let font_family = font_family.into();
        validate_text(&font_family, "font family", MAX_FONT_FAMILY_BYTES)?;
        Ok(Self {
            font_family,
            font_size_pt: canonical_float(font_size_pt),
            color: normalize_color(color.into())?,
            opacity: canonical_float(opacity),
            weight: 400,
            alignment: TextAlignment::Left,
        })
    }

    pub fn with_weight_and_alignment(
        mut self,
        weight: u16,
        alignment: TextAlignment,
    ) -> Result<Self, AnnotationError> {
        if !(1..=1_000).contains(&weight) {
            return Err(AnnotationError::InvalidAppearance(
                "text weight must be between 1 and 1000".into(),
            ));
        }
        self.weight = weight;
        self.alignment = alignment;
        Ok(self)
    }

    pub fn font_family(&self) -> &str {
        &self.font_family
    }

    pub fn font_size_pt(&self) -> f64 {
        self.font_size_pt
    }

    pub fn color(&self) -> &str {
        &self.color
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    pub fn weight(&self) -> u16 {
        self.weight
    }

    pub fn alignment(&self) -> TextAlignment {
        self.alignment
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextBoxAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub layout_rect: PdfRect,
    content: String,
    style: TextBoxStyle,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalloutAppearance {
    line: StraightLineAppearance,
    text: TextBoxStyle,
}

impl CalloutAppearance {
    pub fn new(line: StraightLineAppearance, text: TextBoxStyle) -> Result<Self, AnnotationError> {
        if (line.opacity() - text.opacity()).abs() > f64::EPSILON {
            return Err(AnnotationError::InvalidAppearance(
                "callout line and text must share one opacity".into(),
            ));
        }
        Ok(Self { line, text })
    }

    pub fn line(&self) -> &StraightLineAppearance {
        &self.line
    }

    pub fn text(&self) -> &TextBoxStyle {
        &self.text
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CloudPlusAppearance {
    cloud: RectangleAppearance,
    leader: StraightLineAppearance,
    text: TextBoxStyle,
}

impl CloudPlusAppearance {
    pub fn new(
        cloud: RectangleAppearance,
        leader: StraightLineAppearance,
        text: TextBoxStyle,
    ) -> Result<Self, AnnotationError> {
        if cloud.fill_color().is_some() {
            return Err(AnnotationError::InvalidAppearance(
                "Cloud+ does not support a cloud fill".into(),
            ));
        }
        if cloud.stroke_color() != leader.stroke_color()
            || cloud.stroke_width_pt() != leader.stroke_width_pt()
            || cloud.stroke_style() != leader.stroke_style()
        {
            return Err(AnnotationError::InvalidAppearance(
                "Cloud+ cloud and leader must share one stroke".into(),
            ));
        }
        if cloud.opacity() != leader.opacity() || cloud.opacity() != text.opacity() {
            return Err(AnnotationError::InvalidAppearance(
                "Cloud+ cloud, leader, and text must share one opacity".into(),
            ));
        }
        Ok(Self {
            cloud,
            leader,
            text,
        })
    }

    pub fn cloud(&self) -> &RectangleAppearance {
        &self.cloud
    }

    pub fn leader(&self) -> &StraightLineAppearance {
        &self.leader
    }

    pub fn text(&self) -> &TextBoxStyle {
        &self.text
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CloudPlusAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    cloud_points: Vec<PdfPoint>,
    border_effect_intensity: f64,
    leader_points: Vec<PdfPoint>,
    pub text_box: PdfRect,
    content: String,
    pub appearance: CloudPlusAppearance,
    pub locked: bool,
}

impl CloudPlusAnnotation {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: MarkupId,
        page_index: u32,
        cloud_points: Vec<PdfPoint>,
        border_effect_intensity: f64,
        leader_points: Vec<PdfPoint>,
        text_box: PdfRect,
        content: impl Into<String>,
        appearance: CloudPlusAppearance,
    ) -> Result<Self, AnnotationError> {
        validate_vertex_path(&cloud_points, VertexPathKind::Polygon)?;
        require_finite(
            "cloud_plus.border_effect_intensity",
            border_effect_intensity,
        )?;
        if !(0.0..=4.0).contains(&border_effect_intensity) {
            return Err(AnnotationError::InvalidAppearance(
                "Cloud+ intensity must be between 0 and 4".into(),
            ));
        }
        validate_cloud_plus_leader_points(&leader_points)?;
        validate_layout_rect(text_box, "Cloud+ text box")?;
        let content = content.into();
        validate_text(&content, "Cloud+ content", MAX_TEXT_BOX_BYTES)?;
        Ok(Self {
            id,
            page_index,
            cloud_points,
            border_effect_intensity: canonical_float(border_effect_intensity),
            leader_points,
            text_box,
            content,
            appearance,
            locked: false,
        })
    }

    pub fn cloud_points(&self) -> &[PdfPoint] {
        &self.cloud_points
    }

    pub fn border_effect_intensity(&self) -> f64 {
        self.border_effect_intensity
    }

    pub fn leader_points(&self) -> &[PdfPoint] {
        &self.leader_points
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn scallop_path(&self) -> Vec<PdfPoint> {
        sampled_cloud_scallop_path(
            &self.cloud_points,
            DEFAULT_CLOUD_SCALLOP_RADIUS_PT * (self.border_effect_intensity / 2.0).max(0.25),
        )
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        let points_match = |left: &[PdfPoint], right: &[PdfPoint]| {
            left.len() == right.len()
                && left.iter().zip(right).all(|(left, right)| {
                    (left.x - right.x).abs() <= PDF_NUMBER_TOLERANCE
                        && (left.y - right.y).abs() <= PDF_NUMBER_TOLERANCE
                })
        };
        let rect_matches = |left: PdfRect, right: PdfRect| {
            (left.x - right.x).abs() <= PDF_NUMBER_TOLERANCE
                && (left.y - right.y).abs() <= PDF_NUMBER_TOLERANCE
                && (left.width - right.width).abs() <= PDF_NUMBER_TOLERANCE
                && (left.height - right.height).abs() <= PDF_NUMBER_TOLERANCE
        };
        self.id == other.id
            && self.page_index == other.page_index
            && self.border_effect_intensity == other.border_effect_intensity
            && rect_matches(self.text_box, other.text_box)
            && self.content == other.content
            && self.appearance == other.appearance
            && self.locked == other.locked
            && points_match(&self.cloud_points, &other.cloud_points)
            && points_match(&self.leader_points, &other.leader_points)
    }
}

fn validate_cloud_plus_leader_points(points: &[PdfPoint]) -> Result<(), AnnotationError> {
    if !matches!(points.len(), 0 | 3) {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ leader requires either no points or exactly tip, knee, and connection".into(),
        ));
    }
    for (index, point) in points.iter().enumerate() {
        require_finite(&format!("cloud_plus.leader[{index}].x"), point.x)?;
        require_finite(&format!("cloud_plus.leader[{index}].y"), point.y)?;
    }
    if points.len() == 3 && point_distance(points[0], points[2]) <= MIN_STRAIGHT_LINE_LENGTH_PT {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ leader tip and connection must be more than two points apart".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct CalloutAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    leader_points: Vec<PdfPoint>,
    pub text_box: PdfRect,
    content: String,
    pub appearance: CalloutAppearance,
    pub locked: bool,
}

impl CalloutAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        leader_points: Vec<PdfPoint>,
        text_box: PdfRect,
        content: impl Into<String>,
        appearance: CalloutAppearance,
    ) -> Result<Self, AnnotationError> {
        if leader_points.len() < 2 {
            return Err(AnnotationError::InvalidGeometry(
                "callout leader requires at least a tip and connection".into(),
            ));
        }
        for (index, point) in leader_points.iter().enumerate() {
            require_finite(&format!("callout.leader[{index}].x"), point.x)?;
            require_finite(&format!("callout.leader[{index}].y"), point.y)?;
        }
        if point_distance(leader_points[0], *leader_points.last().unwrap())
            <= MIN_STRAIGHT_LINE_LENGTH_PT
        {
            return Err(AnnotationError::InvalidGeometry(
                "callout tip and connection must be more than two points apart".into(),
            ));
        }
        validate_layout_rect(text_box, "callout text box")?;
        let content = content.into();
        validate_text(&content, "callout content", MAX_TEXT_BOX_BYTES)?;
        Ok(Self {
            id,
            page_index,
            leader_points,
            text_box,
            content,
            appearance,
            locked: false,
        })
    }

    pub fn leader_points(&self) -> &[PdfPoint] {
        &self.leader_points
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && self.text_box.same_pdf_geometry_as(other.text_box)
            && self.content == other.content
            && self.appearance == other.appearance
            && self.locked == other.locked
            && self.leader_points.len() == other.leader_points.len()
            && self
                .leader_points
                .iter()
                .zip(&other.leader_points)
                .all(|(left, right)| {
                    (left.x - right.x).abs() <= PDF_NUMBER_TOLERANCE
                        && (left.y - right.y).abs() <= PDF_NUMBER_TOLERANCE
                })
    }
}

impl TextBoxAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        layout_rect: PdfRect,
        content: impl Into<String>,
        style: TextBoxStyle,
    ) -> Result<Self, AnnotationError> {
        validate_layout_rect(layout_rect, "text box")?;
        let content = content.into();
        validate_text(&content, "text box content", MAX_TEXT_BOX_BYTES)?;
        Ok(Self {
            id,
            page_index,
            layout_rect,
            content,
            style,
            locked: false,
        })
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn style(&self) -> &TextBoxStyle {
        &self.style
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleUnit {
    In,
    Ft,
    Mm,
    Cm,
    M,
}

impl ScaleUnit {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::In => "in",
            Self::Ft => "ft",
            Self::Mm => "mm",
            Self::Cm => "cm",
            Self::M => "m",
        }
    }

    pub fn parse(value: &str) -> Result<Self, PageScaleError> {
        match value {
            "in" => Ok(Self::In),
            "ft" => Ok(Self::Ft),
            "mm" => Ok(Self::Mm),
            "cm" => Ok(Self::Cm),
            "m" => Ok(Self::M),
            _ => Err(PageScaleError("Scale unit is not supported.".into())),
        }
    }

    fn points(self) -> f64 {
        match self {
            Self::In => 72.,
            Self::Ft => 864.,
            Self::Mm => 72. / 25.4,
            Self::Cm => 72. / 2.54,
            Self::M => 72. / 0.0254,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScaleSource {
    Preset,
    Custom,
    Calibrated,
}

impl ScaleSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preset => "preset",
            Self::Custom => "custom",
            Self::Calibrated => "calibrated",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScalePrecisionMode {
    Decimal,
    Fraction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScalePrecision {
    pub mode: ScalePrecisionMode,
    pub value: f64,
}

impl ScalePrecision {
    pub fn decimal(value: f64) -> Result<Self, PageScaleError> {
        if !value.is_finite() || value <= 0. {
            return Err(PageScaleError("Decimal precision must be positive.".into()));
        }
        Ok(Self {
            mode: ScalePrecisionMode::Decimal,
            value: canonical_float(value),
        })
    }

    pub fn fraction(denominator: u16) -> Result<Self, PageScaleError> {
        if denominator == 0 {
            return Err(PageScaleError(
                "Fraction precision must be positive.".into(),
            ));
        }
        Ok(Self {
            mode: ScalePrecisionMode::Fraction,
            value: f64::from(denominator),
        })
    }

    fn decimal_digits(self) -> u8 {
        if self.mode != ScalePrecisionMode::Decimal {
            return 0;
        }
        (-self.value.log10()).round().clamp(0., 12.) as u8
    }

    fn format(self, value: f64) -> String {
        match self.mode {
            ScalePrecisionMode::Decimal => {
                let rounded = (value / self.value).round() * self.value;
                format!("{:.*}", usize::from(self.decimal_digits()), rounded)
            }
            ScalePrecisionMode::Fraction => {
                let denominator = self.value.round().max(1.) as i64;
                let whole = value.trunc() as i64;
                let numerator = ((value - whole as f64).abs() * denominator as f64).round() as i64;
                if numerator == 0 {
                    whole.to_string()
                } else if numerator == denominator {
                    (whole + if value.is_sign_negative() { -1 } else { 1 }).to_string()
                } else if whole == 0 {
                    format!("{numerator}/{denominator}")
                } else {
                    format!("{whole} {numerator}/{denominator}")
                }
            }
        }
    }
}

impl Default for ScalePrecision {
    fn default() -> Self {
        Self::decimal(0.001).expect("the default scale precision is positive")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageScale {
    pub page_index: u32,
    pub source: ScaleSource,
    pub name: String,
    pub pdf_units: ScaleUnit,
    pub real_units: ScaleUnit,
    pub scale_x: f64,
    pub scale_y: f64,
    pub precision: ScalePrecision,
}

impl PageScale {
    #[allow(clippy::too_many_arguments)]
    pub fn custom(
        page_index: u32,
        name: impl Into<String>,
        pdf_units: ScaleUnit,
        real_units: ScaleUnit,
        pdf_length: f64,
        real_length: f64,
        y_lengths: Option<(f64, f64)>,
        precision: ScalePrecision,
    ) -> Result<Self, PageScaleError> {
        let scale_x = scale_ratio(pdf_length, pdf_units, real_length)?;
        let scale_y = if let Some((y_pdf_length, y_real_length)) = y_lengths {
            scale_ratio(y_pdf_length, pdf_units, y_real_length)?
        } else {
            scale_x
        };
        Self::from_factors(
            page_index,
            ScaleSource::Custom,
            name,
            pdf_units,
            real_units,
            scale_x,
            scale_y,
            precision,
        )
    }

    pub fn calibrated(
        page_index: u32,
        start: PdfPoint,
        end: PdfPoint,
        real_length: f64,
        real_units: ScaleUnit,
        precision: ScalePrecision,
    ) -> Result<Self, PageScaleError> {
        let paper_points = (end.x - start.x).hypot(end.y - start.y);
        if !paper_points.is_finite()
            || paper_points <= 0.
            || !real_length.is_finite()
            || real_length <= 0.
        {
            return Err(PageScaleError(
                "Calibration requires positive PDF and real-world distances.".into(),
            ));
        }
        let scale = real_length / paper_points;
        Self::from_factors(
            page_index,
            ScaleSource::Calibrated,
            format!(
                "Calibrated {} {}",
                format_scale_number(real_length),
                real_units.as_str()
            ),
            ScaleUnit::In,
            real_units,
            scale,
            scale,
            precision,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_factors(
        page_index: u32,
        source: ScaleSource,
        name: impl Into<String>,
        pdf_units: ScaleUnit,
        real_units: ScaleUnit,
        scale_x: f64,
        scale_y: f64,
        precision: ScalePrecision,
    ) -> Result<Self, PageScaleError> {
        let name = name.into();
        if name.is_empty() || name.len() > MAX_MEASUREMENT_LABEL_BYTES {
            return Err(PageScaleError("Scale name is invalid.".into()));
        }
        if !scale_x.is_finite() || scale_x <= 0. || !scale_y.is_finite() || scale_y <= 0. {
            return Err(PageScaleError("Scale lengths must be positive.".into()));
        }
        Ok(Self {
            page_index,
            source,
            name,
            pdf_units,
            real_units,
            scale_x: canonical_float(scale_x),
            scale_y: canonical_float(scale_y),
            precision,
        })
    }

    pub fn with_page_index(&self, page_index: u32) -> Self {
        Self {
            page_index,
            ..self.clone()
        }
    }

    fn length_calibration(&self) -> Result<LengthCalibration, AnnotationError> {
        LengthCalibration::from_page_scale(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScalePreset {
    pub id: String,
    pub name: String,
    pub pdf_units: ScaleUnit,
    pub real_units: ScaleUnit,
    pub scale_x: f64,
    pub scale_y: f64,
    pub source: ScaleSource,
    pub built_in: bool,
}

pub fn built_in_scale_presets() -> Vec<ScalePreset> {
    [1_u16, 2, 5, 10, 20, 50, 100, 200, 500, 1000]
        .into_iter()
        .map(|ratio| ScalePreset {
            id: format!("one-to-{ratio}"),
            name: format!("1:{ratio}"),
            pdf_units: ScaleUnit::Cm,
            real_units: ScaleUnit::M,
            scale_x: canonical_float((f64::from(ratio) / 100.) / ScaleUnit::Cm.points()),
            scale_y: canonical_float((f64::from(ratio) / 100.) / ScaleUnit::Cm.points()),
            source: ScaleSource::Preset,
            built_in: true,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageScaleRange {
    pub start_page_index: u32,
    pub end_page_index: u32,
}

impl PageScaleRange {
    pub const fn new(start_page_index: u32, end_page_index: u32) -> Self {
        Self {
            start_page_index,
            end_page_index,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageScaleApplyTarget {
    Current(u32),
    All,
    Ranges(Vec<PageScaleRange>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageScaleError(String);

impl fmt::Display for PageScaleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for PageScaleError {}

pub fn parse_page_scale_ranges(
    input: &str,
    page_count: u32,
) -> Result<Vec<PageScaleRange>, PageScaleError> {
    let mut ranges = Vec::new();
    for part in input
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        let pieces = part.split('-').map(str::trim).collect::<Vec<_>>();
        if pieces.is_empty() || pieces.len() > 2 || pieces.iter().any(|piece| piece.is_empty()) {
            return Err(PageScaleError("Enter page ranges like 1-3, 5, 9.".into()));
        }
        let Ok(start) = pieces[0].parse::<u32>() else {
            return Err(PageScaleError("Enter page ranges like 1-3, 5, 9.".into()));
        };
        let end = if pieces.len() == 2 {
            pieces[1]
                .parse::<u32>()
                .map_err(|_| PageScaleError("Enter page ranges like 1-3, 5, 9.".into()))?
        } else {
            start
        };
        if start < 1 || end < 1 || start > page_count || end > page_count {
            return Err(PageScaleError(format!(
                "Page range must be between 1 and {page_count}."
            )));
        }
        ranges.push(PageScaleRange::new(start.min(end) - 1, start.max(end) - 1));
    }
    if ranges.is_empty() {
        return Err(PageScaleError("Enter at least one page range.".into()));
    }
    Ok(ranges)
}

fn scale_ratio(
    pdf_length: f64,
    pdf_units: ScaleUnit,
    real_length: f64,
) -> Result<f64, PageScaleError> {
    if !pdf_length.is_finite() || pdf_length <= 0. || !real_length.is_finite() || real_length <= 0.
    {
        return Err(PageScaleError("Scale lengths must be positive.".into()));
    }
    Ok(canonical_float(
        real_length / (pdf_length * pdf_units.points()),
    ))
}

fn format_scale_number(value: f64) -> String {
    if value.fract() == 0. {
        format!("{value:.0}")
    } else {
        format!("{value:.3}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LengthCalibration {
    units_per_point: f64,
    scale_x: f64,
    scale_y: f64,
    paper_points: f64,
    real_world_value: f64,
    unit: String,
    label: String,
    precision: u8,
    scale_precision: ScalePrecision,
    show_caption: bool,
}

impl LengthCalibration {
    pub fn new(
        units_per_point: f64,
        unit: impl Into<String>,
        label: impl Into<String>,
        show_caption: bool,
    ) -> Result<Self, AnnotationError> {
        require_finite("length.units_per_point", units_per_point)?;
        if units_per_point <= 0.0 {
            return Err(AnnotationError::InvalidGeometry(
                "length scale must be positive".into(),
            ));
        }
        let unit = unit.into();
        let label = label.into();
        validate_text(&unit, "measurement unit", MAX_MEASUREMENT_UNIT_BYTES)?;
        validate_text(&label, "measurement label", MAX_MEASUREMENT_LABEL_BYTES)?;
        Ok(Self {
            units_per_point: canonical_float(units_per_point),
            scale_x: canonical_float(units_per_point),
            scale_y: canonical_float(units_per_point),
            paper_points: 1.0,
            real_world_value: canonical_float(units_per_point),
            unit,
            label,
            precision: 0,
            scale_precision: ScalePrecision::decimal(1.).expect("whole-number precision is valid"),
            show_caption,
        })
    }

    pub fn from_scale(
        paper_points: f64,
        real_world_value: f64,
        unit: impl Into<String>,
        precision: u8,
        show_caption: bool,
    ) -> Result<Self, AnnotationError> {
        require_finite("length.paper_points", paper_points)?;
        require_finite("length.real_world_value", real_world_value)?;
        if paper_points <= 0.0 || real_world_value <= 0.0 {
            return Err(AnnotationError::InvalidGeometry(
                "length scale values must be positive".into(),
            ));
        }
        if precision > 12 {
            return Err(AnnotationError::InvalidGeometry(
                "length precision must be between 0 and 12".into(),
            ));
        }
        let unit = unit.into();
        validate_text(&unit, "measurement unit", MAX_MEASUREMENT_UNIT_BYTES)?;
        Ok(Self {
            units_per_point: canonical_float(real_world_value / paper_points),
            scale_x: canonical_float(real_world_value / paper_points),
            scale_y: canonical_float(real_world_value / paper_points),
            paper_points: canonical_float(paper_points),
            real_world_value: canonical_float(real_world_value),
            unit,
            label: String::new(),
            precision,
            scale_precision: ScalePrecision::decimal(10_f64.powi(-i32::from(precision)))
                .expect("validated decimal precision is positive"),
            show_caption,
        })
    }

    pub fn from_page_scale(scale: &PageScale) -> Result<Self, AnnotationError> {
        let mut calibration = Self::from_scale(
            1.,
            scale.scale_x,
            scale.real_units.as_str(),
            scale.precision.decimal_digits(),
            true,
        )?;
        calibration.scale_x = scale.scale_x;
        calibration.scale_y = scale.scale_y;
        calibration.scale_precision = scale.precision;
        Ok(calibration)
    }

    pub fn units_per_point(&self) -> f64 {
        self.units_per_point
    }

    pub fn unit(&self) -> &str {
        &self.unit
    }

    pub fn paper_points(&self) -> f64 {
        self.paper_points
    }

    pub fn real_world_value(&self) -> f64 {
        self.real_world_value
    }

    pub fn precision(&self) -> u8 {
        self.precision
    }

    pub fn scale_precision(&self) -> ScalePrecision {
        self.scale_precision
    }

    pub fn scale_x(&self) -> f64 {
        self.scale_x
    }

    pub fn scale_y(&self) -> f64 {
        self.scale_y
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn show_caption(&self) -> bool {
        self.show_caption
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Result<Self, AnnotationError> {
        let label = label.into();
        if !label.is_empty() {
            validate_text(&label, "measurement label", MAX_MEASUREMENT_LABEL_BYTES)?;
        }
        self.label = label;
        Ok(self)
    }

    pub fn with_scale_from(&self, scale: &LengthCalibration) -> Result<Self, AnnotationError> {
        let mut replacement = Self::from_scale(
            scale.paper_points,
            scale.real_world_value,
            scale.unit.clone(),
            scale.precision,
            self.show_caption,
        )?
        .with_label(self.label.clone())?;
        replacement.scale_x = scale.scale_x;
        replacement.scale_y = scale.scale_y;
        replacement.scale_precision = scale.scale_precision;
        Ok(replacement)
    }

    pub fn same_scale_as(&self, other: &LengthCalibration) -> bool {
        self.units_per_point == other.units_per_point
            && self.scale_x == other.scale_x
            && self.scale_y == other.scale_y
            && self.unit == other.unit
            && self.scale_precision == other.scale_precision
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DimensionAppearance {
    line: StraightLineAppearance,
    text: TextBoxStyle,
}

impl DimensionAppearance {
    pub fn new(line: StraightLineAppearance, text: TextBoxStyle) -> Result<Self, AnnotationError> {
        if (line.opacity() - text.opacity()).abs() > f64::EPSILON {
            return Err(AnnotationError::InvalidAppearance(
                "dimension line and text must share one opacity".into(),
            ));
        }
        Ok(Self { line, text })
    }

    pub fn line(&self) -> &StraightLineAppearance {
        &self.line
    }

    pub fn text(&self) -> &TextBoxStyle {
        &self.text
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DimensionAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
    dimension_line_offset: f64,
    content: String,
    pub appearance: DimensionAppearance,
    pub locked: bool,
}

impl DimensionAnnotation {
    pub const DEFAULT_OFFSET_PT: f64 = 24.;

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        end: PdfPoint,
        dimension_line_offset: f64,
        content: impl Into<String>,
        appearance: DimensionAppearance,
    ) -> Result<Self, AnnotationError> {
        for (name, value) in [
            ("dimension.start.x", start.x),
            ("dimension.start.y", start.y),
            ("dimension.end.x", end.x),
            ("dimension.end.y", end.y),
            ("dimension.line_offset", dimension_line_offset),
        ] {
            require_finite(name, value)?;
        }
        if point_distance(start, end) <= MIN_STRAIGHT_LINE_LENGTH_PT {
            return Err(AnnotationError::InvalidGeometry(
                "dimension endpoints must be more than two points apart".into(),
            ));
        }
        let content = content.into();
        validate_text(&content, "dimension content", MAX_TEXT_BOX_BYTES)?;
        Ok(Self {
            id,
            page_index,
            start,
            end,
            dimension_line_offset: canonical_float(dimension_line_offset),
            content,
            appearance,
            locked: false,
        })
    }

    pub fn default_offset(start: PdfPoint, end: PdfPoint) -> f64 {
        if end.x >= start.x {
            Self::DEFAULT_OFFSET_PT
        } else {
            -Self::DEFAULT_OFFSET_PT
        }
    }

    pub fn dimension_line_offset(&self) -> f64 {
        self.dimension_line_offset
    }

    pub fn dimension_line_points(&self) -> (PdfPoint, PdfPoint) {
        let delta_x = self.end.x - self.start.x;
        let delta_y = self.end.y - self.start.y;
        let length = delta_x.hypot(delta_y);
        let normal_x = -delta_y / length;
        let normal_y = delta_x / length;
        let offset_x = normal_x * self.dimension_line_offset;
        let offset_y = normal_y * self.dimension_line_offset;
        (
            PdfPoint {
                x: self.start.x + offset_x,
                y: self.start.y + offset_y,
            },
            PdfPoint {
                x: self.end.x + offset_x,
                y: self.end.y + offset_y,
            },
        )
    }

    pub fn caption_center(&self) -> PdfPoint {
        let (start, end) = self.dimension_line_points();
        PdfPoint {
            x: (start.x + end.x) * 0.5,
            y: (start.y + end.y) * 0.5,
        }
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && (self.start.x - other.start.x).abs() <= PDF_NUMBER_TOLERANCE
            && (self.start.y - other.start.y).abs() <= PDF_NUMBER_TOLERANCE
            && (self.end.x - other.end.x).abs() <= PDF_NUMBER_TOLERANCE
            && (self.end.y - other.end.y).abs() <= PDF_NUMBER_TOLERANCE
            && (self.dimension_line_offset - other.dimension_line_offset).abs()
                <= PDF_NUMBER_TOLERANCE
            && self.content == other.content
            && self.appearance == other.appearance
            && self.locked == other.locked
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LengthAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
    calibration: LengthCalibration,
    pub locked: bool,
}

impl LengthAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        end: PdfPoint,
        calibration: LengthCalibration,
    ) -> Result<Self, AnnotationError> {
        for (name, value) in [
            ("length.start.x", start.x),
            ("length.start.y", start.y),
            ("length.end.x", end.x),
            ("length.end.y", end.y),
        ] {
            require_finite(name, value)?;
        }
        if start == end {
            return Err(AnnotationError::InvalidGeometry(
                "length endpoints must be distinct".into(),
            ));
        }
        Ok(Self {
            id,
            page_index,
            start,
            end,
            calibration,
            locked: false,
        })
    }

    pub fn calibration(&self) -> &LengthCalibration {
        &self.calibration
    }

    pub fn measured_value(&self) -> f64 {
        canonical_float(
            ((self.end.x - self.start.x) * self.calibration.scale_x)
                .hypot((self.end.y - self.start.y) * self.calibration.scale_y),
        )
    }

    pub fn caption(&self) -> String {
        let value = self
            .calibration
            .scale_precision
            .format(self.measured_value());
        if self.calibration.label.is_empty() {
            format!("{value} {}", self.calibration.unit)
        } else {
            format!(
                "{}: {value} {}",
                self.calibration.label, self.calibration.unit
            )
        }
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && (self.start.x - other.start.x).abs() <= PDF_NUMBER_TOLERANCE
            && (self.start.y - other.start.y).abs() <= PDF_NUMBER_TOLERANCE
            && (self.end.x - other.end.x).abs() <= PDF_NUMBER_TOLERANCE
            && (self.end.y - other.end.y).abs() <= PDF_NUMBER_TOLERANCE
            && self.calibration.same_scale_as(&other.calibration)
            && self.calibration.label == other.calibration.label
            && self.calibration.precision == other.calibration.precision
            && self.calibration.show_caption == other.calibration.show_caption
            && self.locked == other.locked
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LengthEndpoint {
    Start,
    End,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImageAssetId(String);

impl ImageAssetId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedRgbaAsset {
    id: ImageAssetId,
    width_px: u32,
    height_px: u32,
    rgba: Arc<[u8]>,
}

impl DecodedRgbaAsset {
    pub fn new(width_px: u32, height_px: u32, rgba: Vec<u8>) -> Result<Self, AnnotationError> {
        if width_px == 0
            || height_px == 0
            || width_px > MAX_IMAGE_DIMENSION_PX
            || height_px > MAX_IMAGE_DIMENSION_PX
        {
            return Err(AnnotationError::InvalidGeometry(format!(
                "decoded image dimensions must be between 1 and {MAX_IMAGE_DIMENSION_PX} pixels"
            )));
        }
        let expected_bytes = usize::try_from(width_px)
            .ok()
            .and_then(|width| {
                usize::try_from(height_px)
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes <= MAX_DECODED_IMAGE_BYTES)
            .ok_or_else(|| {
                AnnotationError::InvalidGeometry(format!(
                    "decoded image exceeds the {MAX_DECODED_IMAGE_BYTES}-byte limit"
                ))
            })?;
        if rgba.len() != expected_bytes {
            return Err(AnnotationError::InvalidGeometry(format!(
                "decoded RGBA byte length is {}, expected {expected_bytes}",
                rgba.len(),
            )));
        }
        let mut digest = Sha256::new();
        digest.update(b"bp-decoded-rgba-v1\0");
        digest.update(width_px.to_be_bytes());
        digest.update(height_px.to_be_bytes());
        digest.update(&rgba);
        Ok(Self {
            id: ImageAssetId(format!("{:x}", digest.finalize())),
            width_px,
            height_px,
            rgba: rgba.into(),
        })
    }

    pub fn id(&self) -> &ImageAssetId {
        &self.id
    }

    pub fn width_px(&self) -> u32 {
        self.width_px
    }

    pub fn height_px(&self) -> u32 {
        self.height_px
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub rect: PdfRect,
    asset: DecodedRgbaAsset,
    pub aspect_locked: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub rect: PdfRect,
    asset: DecodedRgbaAsset,
    opacity: f64,
    rotation_degrees: f64,
    pub locked: bool,
}

impl SnapshotAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        rect: PdfRect,
        asset: DecodedRgbaAsset,
        opacity: f64,
    ) -> Result<Self, AnnotationError> {
        validate_snapshot_rect(rect)?;
        validate_snapshot_opacity(opacity)?;
        Ok(Self {
            id,
            page_index,
            rect,
            asset,
            opacity: canonical_float(opacity),
            rotation_degrees: 0.,
            locked: false,
        })
    }

    pub fn asset(&self) -> &DecodedRgbaAsset {
        &self.asset
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    pub fn rotation_degrees(&self) -> f64 {
        self.rotation_degrees
    }

    pub fn with_rotation_degrees(mut self, rotation_degrees: f64) -> Result<Self, AnnotationError> {
        require_finite("snapshot.rotation", rotation_degrees)?;
        self.rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
        Ok(self)
    }

    pub fn with_locked(mut self, locked: bool) -> Self {
        self.locked = locked;
        self
    }
}

impl ImageAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        rect: PdfRect,
        asset: DecodedRgbaAsset,
        aspect_locked: bool,
    ) -> Result<Self, AnnotationError> {
        validate_layout_rect(rect, "image")?;
        validate_image_aspect(rect, &asset, aspect_locked)?;
        Ok(Self {
            id,
            page_index,
            rect,
            asset,
            aspect_locked,
            locked: false,
        })
    }

    pub fn asset(&self) -> &DecodedRgbaAsset {
        &self.asset
    }
}

impl PenAnnotation {
    pub fn new(
        id: MarkupId,
        page_index: u32,
        points: Vec<PdfPoint>,
        appearance: PenAppearance,
    ) -> Result<Self, AnnotationError> {
        validate_pen_path(&points)?;
        Ok(Self {
            id,
            page_index,
            points,
            additional_paths: Vec::new(),
            appearance,
            smooth_curves: true,
            tool: InkTool::Pen,
            blend_mode: BlendMode::Normal,
            locked: false,
        })
    }

    pub fn new_highlight(
        id: MarkupId,
        page_index: u32,
        points: Vec<PdfPoint>,
        appearance: PenAppearance,
    ) -> Result<Self, AnnotationError> {
        validate_pen_path(&points)?;
        Ok(Self {
            id,
            page_index,
            points,
            additional_paths: Vec::new(),
            appearance,
            smooth_curves: false,
            tool: InkTool::Highlight,
            blend_mode: BlendMode::Multiply,
            locked: false,
        })
    }

    pub fn points(&self) -> &[PdfPoint] {
        &self.points
    }

    pub fn paths(&self) -> impl Iterator<Item = &[PdfPoint]> {
        std::iter::once(self.points.as_slice())
            .chain(self.additional_paths.iter().map(Vec::as_slice))
    }

    pub fn new_highlight_paths(
        id: MarkupId,
        page_index: u32,
        mut paths: Vec<Vec<PdfPoint>>,
        appearance: PenAppearance,
    ) -> Result<Self, AnnotationError> {
        if paths.is_empty() {
            return Err(AnnotationError::InvalidGeometry(
                "highlight must contain at least one path".into(),
            ));
        }
        for path in &paths {
            validate_pen_path(path)?;
        }
        let points = paths.remove(0);
        Ok(Self {
            id,
            page_index,
            points,
            additional_paths: paths,
            appearance,
            smooth_curves: false,
            tool: InkTool::Highlight,
            blend_mode: BlendMode::Multiply,
            locked: false,
        })
    }

    pub fn new_paths(
        id: MarkupId,
        page_index: u32,
        mut paths: Vec<Vec<PdfPoint>>,
        appearance: PenAppearance,
        smooth_curves: bool,
    ) -> Result<Self, AnnotationError> {
        if paths.is_empty() {
            return Err(AnnotationError::InvalidGeometry(
                "pen must contain at least one path".into(),
            ));
        }
        for path in &paths {
            validate_pen_path(path)?;
        }
        let points = paths.remove(0);
        Ok(Self {
            id,
            page_index,
            points,
            additional_paths: paths,
            appearance,
            smooth_curves,
            tool: InkTool::Pen,
            blend_mode: BlendMode::Normal,
            locked: false,
        })
    }

    pub fn tool(&self) -> InkTool {
        self.tool
    }

    pub fn blend_mode(&self) -> BlendMode {
        self.blend_mode
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Annotation {
    Rectangle(RectangleAnnotation),
    Redact(RedactAnnotation),
    Ellipse(EllipseAnnotation),
    Arc(ArcAnnotation),
    StraightLine(StraightLineAnnotation),
    VertexPath(VertexPathAnnotation),
    Cloud(CloudAnnotation),
    CloudPlus(CloudPlusAnnotation),
    Callout(CalloutAnnotation),
    MeasurementPath(MeasurementPathAnnotation),
    Pen(PenAnnotation),
    TextBox(TextBoxAnnotation),
    Dimension(DimensionAnnotation),
    Length(LengthAnnotation),
    Image(ImageAnnotation),
    Snapshot(SnapshotAnnotation),
}

impl Annotation {
    pub fn id(&self) -> &MarkupId {
        match self {
            Self::Rectangle(annotation) => &annotation.id,
            Self::Redact(annotation) => &annotation.id,
            Self::Ellipse(annotation) => &annotation.id,
            Self::Arc(annotation) => &annotation.id,
            Self::StraightLine(annotation) => &annotation.id,
            Self::VertexPath(annotation) => &annotation.id,
            Self::Cloud(annotation) => &annotation.id,
            Self::CloudPlus(annotation) => &annotation.id,
            Self::Callout(annotation) => &annotation.id,
            Self::MeasurementPath(annotation) => &annotation.id,
            Self::Pen(annotation) => &annotation.id,
            Self::TextBox(annotation) => &annotation.id,
            Self::Dimension(annotation) => &annotation.id,
            Self::Length(annotation) => &annotation.id,
            Self::Image(annotation) => &annotation.id,
            Self::Snapshot(annotation) => &annotation.id,
        }
    }

    pub fn kind(&self) -> AnnotationKind {
        match self {
            Self::Rectangle(_) => AnnotationKind::Rectangle,
            Self::Redact(_) => AnnotationKind::Redact,
            Self::Ellipse(_) => AnnotationKind::Ellipse,
            Self::Arc(_) => AnnotationKind::Arc,
            Self::StraightLine(annotation) => match annotation.kind {
                LineKind::Line => AnnotationKind::Line,
                LineKind::Arrow => AnnotationKind::Arrow,
            },
            Self::VertexPath(annotation) => match annotation.kind {
                VertexPathKind::Polyline => AnnotationKind::Polyline,
                VertexPathKind::Polygon => AnnotationKind::Polygon,
            },
            Self::Cloud(_) => AnnotationKind::Cloud,
            Self::CloudPlus(_) => AnnotationKind::CloudPlus,
            Self::Callout(_) => AnnotationKind::Callout,
            Self::MeasurementPath(annotation) => annotation.kind.into(),
            Self::Pen(_) => AnnotationKind::Pen,
            Self::TextBox(_) => AnnotationKind::TextBox,
            Self::Dimension(_) => AnnotationKind::Dimension,
            Self::Length(_) => AnnotationKind::Length,
            Self::Image(_) => AnnotationKind::Image,
            Self::Snapshot(_) => AnnotationKind::Snapshot,
        }
    }

    pub fn page_index(&self) -> u32 {
        match self {
            Self::Rectangle(annotation) => annotation.page_index,
            Self::Redact(annotation) => annotation.page_index,
            Self::Ellipse(annotation) => annotation.page_index,
            Self::Arc(annotation) => annotation.page_index,
            Self::StraightLine(annotation) => annotation.page_index,
            Self::VertexPath(annotation) => annotation.page_index,
            Self::Cloud(annotation) => annotation.page_index,
            Self::CloudPlus(annotation) => annotation.page_index,
            Self::Callout(annotation) => annotation.page_index,
            Self::MeasurementPath(annotation) => annotation.page_index,
            Self::Pen(annotation) => annotation.page_index,
            Self::TextBox(annotation) => annotation.page_index,
            Self::Dimension(annotation) => annotation.page_index,
            Self::Length(annotation) => annotation.page_index,
            Self::Image(annotation) => annotation.page_index,
            Self::Snapshot(annotation) => annotation.page_index,
        }
    }

    pub fn translated_copy(
        &self,
        id: MarkupId,
        page_index: u32,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, AnnotationError> {
        require_finite("copy.delta_x", delta_x)?;
        require_finite("copy.delta_y", delta_y)?;
        Ok(match self {
            Self::Rectangle(source) => Self::Rectangle(RectangleAnnotation {
                id,
                page_index,
                rect: PdfRect::new(
                    source.rect.x + delta_x,
                    source.rect.y + delta_y,
                    source.rect.width,
                    source.rect.height,
                )?,
                rotation_degrees: source.rotation_degrees,
                appearance: source.appearance.clone(),
                locked: source.locked,
            }),
            Self::Redact(source) => {
                let mut copy = RedactAnnotation::new(
                    id,
                    page_index,
                    PdfRect::new(
                        source.rect.x + delta_x,
                        source.rect.y + delta_y,
                        source.rect.width,
                        source.rect.height,
                    )?,
                    source.redaction_color.clone(),
                    source.overlay_text.clone(),
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::Redact(copy)
            }
            Self::Ellipse(source) => Self::Ellipse(EllipseAnnotation {
                id,
                page_index,
                rect: PdfRect::new(
                    source.rect.x + delta_x,
                    source.rect.y + delta_y,
                    source.rect.width,
                    source.rect.height,
                )?,
                rotation_degrees: source.rotation_degrees,
                appearance: source.appearance.clone(),
                locked: source.locked,
            }),
            Self::Arc(source) => {
                let mut copy = ArcAnnotation::new(
                    id,
                    page_index,
                    PdfPoint::new(source.start.x + delta_x, source.start.y + delta_y)?,
                    PdfPoint::new(source.end.x + delta_x, source.end.y + delta_y)?,
                    PdfPoint::new(source.mid.x + delta_x, source.mid.y + delta_y)?,
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::Arc(copy)
            }
            Self::StraightLine(source) => {
                let mut copy = StraightLineAnnotation::new(
                    id,
                    page_index,
                    PdfPoint::new(source.start.x + delta_x, source.start.y + delta_y)?,
                    PdfPoint::new(source.end.x + delta_x, source.end.y + delta_y)?,
                    source.kind,
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::StraightLine(copy)
            }
            Self::VertexPath(source) => {
                let points = source
                    .points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut copy = VertexPathAnnotation::new(
                    id,
                    page_index,
                    points,
                    source.kind,
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::VertexPath(copy)
            }
            Self::Cloud(source) => {
                let points = source
                    .points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut copy = CloudAnnotation::new(
                    id,
                    page_index,
                    points,
                    source.border_effect_intensity,
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::Cloud(copy)
            }
            Self::CloudPlus(source) => {
                let cloud_points = source
                    .cloud_points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let leader_points = source
                    .leader_points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut copy = CloudPlusAnnotation::new(
                    id,
                    page_index,
                    cloud_points,
                    source.border_effect_intensity,
                    leader_points,
                    source.text_box.translated(delta_x, delta_y),
                    source.content.clone(),
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::CloudPlus(copy)
            }
            Self::Callout(source) => {
                let leader_points = source
                    .leader_points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut copy = CalloutAnnotation::new(
                    id,
                    page_index,
                    leader_points,
                    PdfRect::new(
                        source.text_box.x + delta_x,
                        source.text_box.y + delta_y,
                        source.text_box.width,
                        source.text_box.height,
                    )?,
                    source.content.clone(),
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::Callout(copy)
            }
            Self::MeasurementPath(source) => {
                let points = source
                    .points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut copy = MeasurementPathAnnotation::new(
                    id,
                    page_index,
                    points,
                    source.kind,
                    source.calibration.clone(),
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::MeasurementPath(copy)
            }
            Self::Pen(source) => {
                let mut copy = source.clone();
                copy.id = id;
                copy.page_index = page_index;
                for point in &mut copy.points {
                    *point = PdfPoint::new(point.x + delta_x, point.y + delta_y)?;
                }
                for path in &mut copy.additional_paths {
                    for point in path {
                        *point = PdfPoint::new(point.x + delta_x, point.y + delta_y)?;
                    }
                }
                Self::Pen(copy)
            }
            Self::TextBox(source) => {
                let mut copy = TextBoxAnnotation::new(
                    id,
                    page_index,
                    PdfRect::new(
                        source.layout_rect.x + delta_x,
                        source.layout_rect.y + delta_y,
                        source.layout_rect.width,
                        source.layout_rect.height,
                    )?,
                    source.content.clone(),
                    source.style.clone(),
                )?;
                copy.locked = source.locked;
                Self::TextBox(copy)
            }
            Self::Dimension(source) => {
                let mut copy = DimensionAnnotation::new(
                    id,
                    page_index,
                    PdfPoint::new(source.start.x + delta_x, source.start.y + delta_y)?,
                    PdfPoint::new(source.end.x + delta_x, source.end.y + delta_y)?,
                    source.dimension_line_offset,
                    source.content.clone(),
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::Dimension(copy)
            }
            Self::Length(source) => {
                let mut copy = LengthAnnotation::new(
                    id,
                    page_index,
                    PdfPoint::new(source.start.x + delta_x, source.start.y + delta_y)?,
                    PdfPoint::new(source.end.x + delta_x, source.end.y + delta_y)?,
                    source.calibration.clone(),
                )?;
                copy.locked = source.locked;
                Self::Length(copy)
            }
            Self::Image(source) => {
                let mut copy = ImageAnnotation::new(
                    id,
                    page_index,
                    PdfRect::new(
                        source.rect.x + delta_x,
                        source.rect.y + delta_y,
                        source.rect.width,
                        source.rect.height,
                    )?,
                    source.asset.clone(),
                    source.aspect_locked,
                )?;
                copy.locked = source.locked;
                Self::Image(copy)
            }
            Self::Snapshot(source) => Self::Snapshot(
                SnapshotAnnotation::new(
                    id,
                    page_index,
                    PdfRect::new(
                        source.rect.x + delta_x,
                        source.rect.y + delta_y,
                        source.rect.width,
                        source.rect.height,
                    )?,
                    source.asset.clone(),
                    source.opacity,
                )?
                .with_rotation_degrees(source.rotation_degrees)?
                .with_locked(source.locked),
            ),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationEdit {
    SetRectangleRect(PdfRect),
    SetRectangleRotation(f64),
    SetRedactRect(PdfRect),
    TranslateRedact {
        delta_x: f64,
        delta_y: f64,
    },
    SetEllipseRect(PdfRect),
    TranslateEllipse {
        delta_x: f64,
        delta_y: f64,
    },
    SetEllipseRotation(f64),
    SetArcControlPoint {
        control: ArcControlPoint,
        point: PdfPoint,
        snap_quarter_turn: bool,
    },
    TranslateArc {
        delta_x: f64,
        delta_y: f64,
    },
    ReplacePenPath(Vec<PdfPoint>),
    ReplacePenPaths(Vec<Vec<PdfPoint>>),
    SetInkAppearance(PenAppearance),
    SetTextBoxContent(String),
    SetTextBoxLayoutRect(PdfRect),
    SetDimensionEndpoint {
        endpoint: LineEndpoint,
        point: PdfPoint,
    },
    SetDimensionOffset(f64),
    SetDimensionContent(String),
    SetDimensionAppearance(DimensionAppearance),
    TranslateDimension {
        delta_x: f64,
        delta_y: f64,
    },
    SetLengthCalibration(LengthCalibration),
    SetLengthEndpoint {
        endpoint: LengthEndpoint,
        point: PdfPoint,
    },
    SetStraightLineEndpoint {
        endpoint: LineEndpoint,
        point: PdfPoint,
    },
    TranslateStraightLine {
        delta_x: f64,
        delta_y: f64,
    },
    SetVertexPathPoint {
        vertex_index: usize,
        point: PdfPoint,
    },
    TranslateVertexPath {
        delta_x: f64,
        delta_y: f64,
    },
    SetCloudPoint {
        vertex_index: usize,
        point: PdfPoint,
    },
    TranslateCloud {
        delta_x: f64,
        delta_y: f64,
    },
    SetCloudPlusCloudPoint {
        vertex_index: usize,
        point: PdfPoint,
        leader_points: Vec<PdfPoint>,
    },
    SetCloudPlusLeaderPoints(Vec<PdfPoint>),
    SetCloudPlusTextBox {
        text_box: PdfRect,
        leader_points: Vec<PdfPoint>,
    },
    SetCloudPlusContentAndLayout {
        content: String,
        text_box: PdfRect,
        leader_points: Vec<PdfPoint>,
    },
    SetCloudPlusContent(String),
    SetCloudPlusAppearance(CloudPlusAppearance),
    TranslateCloudPlusGroup {
        delta_x: f64,
        delta_y: f64,
    },
    SetCalloutContent(String),
    SetCalloutLeaderPoint {
        point_index: usize,
        point: PdfPoint,
    },
    TranslateCalloutTextBox {
        delta_x: f64,
        delta_y: f64,
    },
    TranslateCalloutGroup {
        delta_x: f64,
        delta_y: f64,
    },
    SetMeasurementPathPoint {
        vertex_index: usize,
        point: PdfPoint,
    },
    TranslateMeasurementPath {
        delta_x: f64,
        delta_y: f64,
    },
    SetStraightLineAppearance(StraightLineAppearance),
    TranslateLength {
        delta_x: f64,
        delta_y: f64,
    },
    SetImageRect(PdfRect),
    SetSnapshotRect(PdfRect),
    SetSnapshotRotation(f64),
    SetSnapshotOpacity(f64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HitTarget {
    Body(MarkupId),
    LineEndpoint {
        id: MarkupId,
        endpoint: LineEndpoint,
    },
    RotationHandle(MarkupId),
    ResizeHandle {
        id: MarkupId,
        handle: RectangleResizeHandle,
    },
}

impl HitTarget {
    pub fn markup_id(&self) -> &MarkupId {
        match self {
            Self::Body(id)
            | Self::RotationHandle(id)
            | Self::LineEndpoint { id, .. }
            | Self::ResizeHandle { id, .. } => id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RectangleResizeHandle {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

impl RectangleResizeHandle {
    pub const ALL: [Self; 8] = [
        Self::NorthWest,
        Self::North,
        Self::NorthEast,
        Self::East,
        Self::SouthEast,
        Self::South,
        Self::SouthWest,
        Self::West,
    ];

    pub fn point(self, rect: PdfRect) -> PdfPoint {
        let center_x = rect.x + rect.width / 2.0;
        let center_y = rect.y + rect.height / 2.0;
        let east = rect.x + rect.width;
        let north = rect.y + rect.height;
        match self {
            Self::NorthWest => PdfPoint {
                x: rect.x,
                y: north,
            },
            Self::North => PdfPoint {
                x: center_x,
                y: north,
            },
            Self::NorthEast => PdfPoint { x: east, y: north },
            Self::East => PdfPoint {
                x: east,
                y: center_y,
            },
            Self::SouthEast => PdfPoint { x: east, y: rect.y },
            Self::South => PdfPoint {
                x: center_x,
                y: rect.y,
            },
            Self::SouthWest => PdfPoint {
                x: rect.x,
                y: rect.y,
            },
            Self::West => PdfPoint {
                x: rect.x,
                y: center_y,
            },
        }
    }

    pub fn world_point(self, rect: PdfRect, rotation_degrees: f64) -> PdfPoint {
        rotate_point_around_rect_center(self.point(rect), rect, -rotation_degrees)
    }

    fn affects_west(self) -> bool {
        matches!(self, Self::NorthWest | Self::SouthWest | Self::West)
    }

    fn affects_east(self) -> bool {
        matches!(self, Self::NorthEast | Self::East | Self::SouthEast)
    }

    fn affects_north(self) -> bool {
        matches!(self, Self::NorthWest | Self::North | Self::NorthEast)
    }

    fn affects_south(self) -> bool {
        matches!(self, Self::SouthEast | Self::South | Self::SouthWest)
    }

    fn opposite_anchor(self, rect: PdfRect) -> PdfPoint {
        let center = rect.center();
        let east = rect.x + rect.width;
        let north = rect.y + rect.height;
        PdfPoint {
            x: if self.affects_west() {
                east
            } else if self.affects_east() {
                rect.x
            } else {
                center.x
            },
            y: if self.affects_south() {
                north
            } else if self.affects_north() {
                rect.y
            } else {
                center.y
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GestureKind {
    Create,
    Move,
    Resize(RectangleResizeHandle),
    Rotate,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PointerTool {
    Select {
        rotation_handle_offset_pt: f64,
    },
    Rectangle {
        id: MarkupId,
        appearance: RectangleAppearance,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PointerCancelReason {
    CaptureLost,
    AdapterError,
    FocusLost,
    PageChanged,
    ToolChanged,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnnotationCommand {
    CreateAnnotation(Annotation),
    EditAnnotation {
        id: MarkupId,
        edit: AnnotationEdit,
    },
    BeginPen {
        pointer_id: u64,
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        appearance: PenAppearance,
        smooth_curves: bool,
    },
    BeginHighlight {
        pointer_id: u64,
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        appearance: PenAppearance,
        smooth_curves: bool,
    },
    BeginInk {
        pointer_id: u64,
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        appearance: PenAppearance,
        smooth_curves: bool,
        tool: InkTool,
    },
    AppendPenSamples {
        pointer_id: u64,
        samples: Vec<PdfPoint>,
        min_distance_pt: f64,
    },
    CommitPen {
        pointer_id: u64,
    },
    PointerDown {
        pointer_id: u64,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
        tool: PointerTool,
    },
    PointerMove {
        pointer_id: u64,
        point: PdfPoint,
    },
    PointerUp {
        pointer_id: u64,
        point: PdfPoint,
    },
    PointerCancel {
        pointer_id: u64,
        reason: PointerCancelReason,
    },
    SetSelectedAppearance(RectangleAppearance),
    Undo,
    Redo,
    MarkSaved,
    SetLocked {
        id: MarkupId,
        locked: bool,
    },
    DeleteSelected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryDirection {
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandOutcome {
    AnnotationCreated {
        id: MarkupId,
        kind: AnnotationKind,
        revision: u64,
    },
    AnnotationEdited {
        id: MarkupId,
        kind: AnnotationKind,
        changed: bool,
        revision: u64,
    },
    PenStarted {
        id: MarkupId,
    },
    PenSamplesAppended {
        id: MarkupId,
        accepted: usize,
        total: usize,
    },
    GestureStarted {
        kind: GestureKind,
        id: MarkupId,
    },
    PreviewUpdated(GesturePreview),
    GestureCancelled {
        reason: PointerCancelReason,
    },
    GestureCommitted(CommitOutcome),
    SelectionChanged(Option<MarkupId>),
    AppearanceChanged {
        changed: bool,
        revision: u64,
    },
    HistoryChanged {
        direction: HistoryDirection,
        changed: bool,
        revision: u64,
    },
    Saved {
        revision: u64,
    },
    LockChanged {
        id: MarkupId,
        locked: bool,
        changed: bool,
        revision: u64,
    },
    Deleted {
        id: MarkupId,
        revision: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationSnapshot {
    pub revision: u64,
    pub saved_revision: u64,
    pub dirty: bool,
    pub selected_id: Option<MarkupId>,
    pub annotation_order: Vec<MarkupId>,
    pub rectangles: Vec<RectangleAnnotation>,
    pub redacts: Vec<RedactAnnotation>,
    pub ellipses: Vec<EllipseAnnotation>,
    pub arcs: Vec<ArcAnnotation>,
    pub straight_lines: Vec<StraightLineAnnotation>,
    pub vertex_paths: Vec<VertexPathAnnotation>,
    pub clouds: Vec<CloudAnnotation>,
    pub cloud_pluses: Vec<CloudPlusAnnotation>,
    pub callouts: Vec<CalloutAnnotation>,
    pub measurement_paths: Vec<MeasurementPathAnnotation>,
    pub pens: Vec<PenAnnotation>,
    pub text_boxes: Vec<TextBoxAnnotation>,
    pub dimensions: Vec<DimensionAnnotation>,
    pub lengths: Vec<LengthAnnotation>,
    pub images: Vec<ImageAnnotation>,
    pub snapshots: Vec<SnapshotAnnotation>,
    pub page_scales: Vec<PageScale>,
    pub scale_presets: Vec<ScalePreset>,
    pub page_length_calibrations: Vec<(u32, LengthCalibration)>,
    pub page_rotations: Vec<(u32, PageRotation)>,
    pub undo_depth: usize,
    pub redo_depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneRectangle {
    pub id: MarkupId,
    pub rect: PdfRect,
    pub rotation_degrees: f64,
    pub appearance: RectangleAppearance,
    pub selected: bool,
    pub locked: bool,
    pub preview: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneRedact {
    pub id: MarkupId,
    pub body_id: &'static str,
    pub rect: PdfRect,
    pub appearance: RectangleAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationScene {
    pub page_index: u32,
    pub revision: u64,
    pub rectangles: Vec<SceneRectangle>,
    pub redacts: Vec<SceneRedact>,
    pub ellipses: Vec<SceneRectangle>,
    pub arcs: Vec<SceneArc>,
    pub straight_lines: Vec<SceneStraightLine>,
    pub vertex_paths: Vec<SceneVertexPath>,
    pub clouds: Vec<SceneCloud>,
    pub cloud_pluses: Vec<SceneCloudPlus>,
    pub callouts: Vec<SceneCallout>,
    pub measurement_paths: Vec<SceneMeasurementPath>,
    pub pens: Vec<ScenePen>,
    pub text_boxes: Vec<SceneTextBox>,
    pub dimensions: Vec<SceneDimension>,
    pub lengths: Vec<SceneLength>,
    pub images: Vec<SceneImage>,
    pub snapshots: Vec<SceneSnapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneArc {
    pub id: MarkupId,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub mid: PdfPoint,
    pub sampled_path: Vec<PdfPoint>,
    pub appearance: RectangleAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

impl SceneArc {
    pub fn sweep_degrees(&self) -> f64 {
        ArcAnnotation::new(
            self.id.clone(),
            0,
            self.start,
            self.end,
            self.mid,
            self.appearance.clone(),
        )
        .expect("a rendered Arc always has valid circle geometry")
        .sweep_degrees()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneStraightLine {
    pub id: MarkupId,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub kind: LineKind,
    pub appearance: StraightLineAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneVertexPath {
    pub id: MarkupId,
    pub points: Vec<PdfPoint>,
    pub kind: VertexPathKind,
    pub appearance: RectangleAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneCloud {
    pub id: MarkupId,
    pub points: Vec<PdfPoint>,
    pub scallop_path: Vec<PdfPoint>,
    pub border_effect_intensity: f64,
    pub appearance: RectangleAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneCloudPlus {
    pub id: MarkupId,
    pub cloud_points: Vec<PdfPoint>,
    pub scallop_path: Vec<PdfPoint>,
    pub border_effect_intensity: f64,
    pub leader_points: Vec<PdfPoint>,
    pub text_box: PdfRect,
    pub content: String,
    pub appearance: CloudPlusAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneCallout {
    pub id: MarkupId,
    pub leader_points: Vec<PdfPoint>,
    pub text_box: PdfRect,
    pub content: String,
    pub appearance: CalloutAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneMeasurementPath {
    pub id: MarkupId,
    pub points: Vec<PdfPoint>,
    pub kind: MeasurementPathKind,
    pub appearance: RectangleAppearance,
    pub caption: String,
    pub show_caption: bool,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScenePen {
    pub id: MarkupId,
    pub points: Vec<PdfPoint>,
    pub paths: Vec<Vec<PdfPoint>>,
    pub appearance: PenAppearance,
    pub tool: InkTool,
    pub blend_mode: BlendMode,
    pub smooth_curves: bool,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneTextBox {
    pub id: MarkupId,
    pub layout_rect: PdfRect,
    pub content: String,
    pub style: TextBoxStyle,
    pub selected: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneDimension {
    pub id: MarkupId,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub dimension_line_offset: f64,
    pub content: String,
    pub appearance: DimensionAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneLength {
    pub id: MarkupId,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub caption: String,
    pub show_caption: bool,
    pub selected: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneImage {
    pub id: MarkupId,
    pub rect: PdfRect,
    pub asset_id: ImageAssetId,
    pub width_px: u32,
    pub height_px: u32,
    pub aspect_locked: bool,
    pub selected: bool,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneSnapshot {
    pub id: MarkupId,
    pub body_id: &'static str,
    pub rect: PdfRect,
    pub asset_id: ImageAssetId,
    pub width_px: u32,
    pub height_px: u32,
    pub opacity: f64,
    pub rotation_degrees: f64,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureReplayOutcome {
    pub fixture_id: String,
    pub canonical_sha256: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GesturePreview {
    pub kind: GestureKind,
    pub annotation: RectangleAnnotation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    Created(MarkupId),
    Updated(MarkupId),
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnnotationError {
    ActiveGesture,
    DuplicateMarkupId(MarkupId),
    InvalidAppearance(String),
    InvalidGeometry(String),
    InvalidHistoryLimit,
    InvalidMarkupId,
    InvalidTolerance,
    InvalidFixture(String),
    CanonicalFixtureMismatch(String),
    NoActiveGesture,
    NoSelection,
    LockedMarkup(MarkupId),
    PointerMismatch { expected: u64, received: u64 },
}

impl fmt::Display for AnnotationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ActiveGesture => write!(formatter, "an annotation gesture is already active"),
            Self::DuplicateMarkupId(id) => write!(formatter, "markup id {id:?} already exists"),
            Self::InvalidAppearance(message) => write!(formatter, "invalid appearance: {message}"),
            Self::InvalidGeometry(message) => write!(formatter, "invalid geometry: {message}"),
            Self::InvalidHistoryLimit => write!(formatter, "history limit must be positive"),
            Self::InvalidMarkupId => write!(formatter, "markup id must be nonempty and canonical"),
            Self::InvalidTolerance => write!(
                formatter,
                "hit-test tolerance must be finite and nonnegative"
            ),
            Self::InvalidFixture(message) => write!(formatter, "invalid fixture: {message}"),
            Self::CanonicalFixtureMismatch(message) => {
                write!(formatter, "fixture canonical mismatch: {message}")
            }
            Self::NoActiveGesture => write!(formatter, "no annotation gesture is active"),
            Self::NoSelection => write!(formatter, "no annotation is selected"),
            Self::LockedMarkup(id) => write!(formatter, "markup {id} is locked"),
            Self::PointerMismatch { expected, received } => write!(
                formatter,
                "gesture belongs to pointer {expected}, not pointer {received}"
            ),
        }
    }
}

impl Error for AnnotationError {}

#[derive(Clone)]
struct DocumentState {
    annotation_order: Vec<MarkupId>,
    rectangles: Vec<RectangleAnnotation>,
    redacts: Vec<RedactAnnotation>,
    ellipses: Vec<EllipseAnnotation>,
    arcs: Vec<ArcAnnotation>,
    rectangle_index: RectangleSpatialIndex,
    straight_lines: Vec<StraightLineAnnotation>,
    vertex_paths: Vec<VertexPathAnnotation>,
    clouds: Vec<CloudAnnotation>,
    cloud_pluses: Vec<CloudPlusAnnotation>,
    callouts: Vec<CalloutAnnotation>,
    measurement_paths: Vec<MeasurementPathAnnotation>,
    pens: Vec<PenAnnotation>,
    text_boxes: Vec<TextBoxAnnotation>,
    dimensions: Vec<DimensionAnnotation>,
    lengths: Vec<LengthAnnotation>,
    images: Vec<ImageAnnotation>,
    snapshots: Vec<SnapshotAnnotation>,
    page_scales: BTreeMap<u32, PageScale>,
    scale_presets: Vec<ScalePreset>,
    page_length_calibrations: BTreeMap<u32, LengthCalibration>,
    page_rotations: BTreeMap<u32, PageRotation>,
    revision: u64,
}

const SPATIAL_CELL_PT: f64 = 64.0;

#[derive(Clone, Default)]
struct RectangleSpatialIndex {
    cells: BTreeMap<(u32, i32, i32), Vec<usize>>,
}

impl RectangleSpatialIndex {
    fn rebuild(rectangles: &[RectangleAnnotation]) -> Self {
        let mut index = Self::default();
        for (position, annotation) in rectangles.iter().enumerate() {
            let bounds = rectangle_world_bounds(annotation.rect, annotation.rotation_degrees);
            for x in spatial_cell(bounds.x)..=spatial_cell(bounds.x + bounds.width) {
                for y in spatial_cell(bounds.y)..=spatial_cell(bounds.y + bounds.height) {
                    index
                        .cells
                        .entry((annotation.page_index, x, y))
                        .or_default()
                        .push(position);
                }
            }
        }
        index
    }

    fn candidates(&self, page_index: u32, point: PdfPoint, tolerance_pt: f64) -> Vec<usize> {
        let mut candidates = BTreeSet::new();
        for x in spatial_cell(point.x - tolerance_pt)..=spatial_cell(point.x + tolerance_pt) {
            for y in spatial_cell(point.y - tolerance_pt)..=spatial_cell(point.y + tolerance_pt) {
                if let Some(entries) = self.cells.get(&(page_index, x, y)) {
                    candidates.extend(entries.iter().copied());
                }
            }
        }
        candidates.into_iter().collect()
    }
}

fn spatial_cell(value: f64) -> i32 {
    (value / SPATIAL_CELL_PT).floor() as i32
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpatialQueryWork {
    pub candidate_count: usize,
    pub total_rectangle_count: usize,
}

#[derive(Clone)]
enum ActiveGesture {
    Pen {
        pointer_id: u64,
        annotation: PenAnnotation,
    },
    Create {
        pointer_id: u64,
        annotation: RectangleAnnotation,
        start: PdfPoint,
    },
    Move {
        pointer_id: u64,
        annotation: RectangleAnnotation,
        original: PdfRect,
        start: PdfPoint,
    },
    Resize {
        pointer_id: u64,
        annotation: RectangleAnnotation,
        original: PdfRect,
        handle: RectangleResizeHandle,
    },
    Rotate {
        pointer_id: u64,
        annotation: RectangleAnnotation,
        original_rotation_degrees: f64,
        start_angle_radians: f64,
    },
}

impl ActiveGesture {
    fn pointer_id(&self) -> u64 {
        match self {
            Self::Pen { pointer_id, .. }
            | Self::Create { pointer_id, .. }
            | Self::Move { pointer_id, .. }
            | Self::Resize { pointer_id, .. }
            | Self::Rotate { pointer_id, .. } => *pointer_id,
        }
    }

    fn rectangle_preview(&self) -> Option<GesturePreview> {
        match self {
            Self::Pen { .. } => None,
            Self::Create { annotation, .. } => Some(GesturePreview {
                kind: GestureKind::Create,
                annotation: annotation.clone(),
            }),
            Self::Move { annotation, .. } => Some(GesturePreview {
                kind: GestureKind::Move,
                annotation: annotation.clone(),
            }),
            Self::Resize {
                annotation, handle, ..
            } => Some(GesturePreview {
                kind: GestureKind::Resize(*handle),
                annotation: annotation.clone(),
            }),
            Self::Rotate { annotation, .. } => Some(GesturePreview {
                kind: GestureKind::Rotate,
                annotation: annotation.clone(),
            }),
        }
    }
}

pub struct AnnotationDocument {
    state: DocumentState,
    selected_ids: Vec<MarkupId>,
    active_gesture: Option<ActiveGesture>,
    past: VecDeque<DocumentState>,
    future: VecDeque<DocumentState>,
    history_limit: usize,
    saved_revision: u64,
    next_revision: u64,
}

impl Default for AnnotationDocument {
    fn default() -> Self {
        Self::with_history_limit(DEFAULT_HISTORY_LIMIT)
            .expect("the default history limit must be valid")
    }
}

impl AnnotationDocument {
    pub fn with_history_limit(history_limit: usize) -> Result<Self, AnnotationError> {
        if history_limit == 0 {
            return Err(AnnotationError::InvalidHistoryLimit);
        }
        Ok(Self {
            state: DocumentState {
                annotation_order: Vec::new(),
                rectangles: Vec::new(),
                redacts: Vec::new(),
                ellipses: Vec::new(),
                arcs: Vec::new(),
                rectangle_index: RectangleSpatialIndex::default(),
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
                page_scales: BTreeMap::new(),
                scale_presets: Vec::new(),
                page_length_calibrations: BTreeMap::new(),
                page_rotations: BTreeMap::new(),
                revision: 0,
            },
            selected_ids: Vec::new(),
            active_gesture: None,
            past: VecDeque::new(),
            future: VecDeque::new(),
            history_limit,
            saved_revision: 0,
            next_revision: 1,
        })
    }

    pub fn rectangles(&self) -> &[RectangleAnnotation] {
        &self.state.rectangles
    }

    pub fn redacts(&self) -> &[RedactAnnotation] {
        &self.state.redacts
    }

    pub fn ellipses(&self) -> &[EllipseAnnotation] {
        &self.state.ellipses
    }

    pub fn arcs(&self) -> &[ArcAnnotation] {
        &self.state.arcs
    }

    pub fn straight_lines(&self) -> &[StraightLineAnnotation] {
        &self.state.straight_lines
    }

    pub fn vertex_paths(&self) -> &[VertexPathAnnotation] {
        &self.state.vertex_paths
    }

    pub fn clouds(&self) -> &[CloudAnnotation] {
        &self.state.clouds
    }

    pub fn cloud_pluses(&self) -> &[CloudPlusAnnotation] {
        &self.state.cloud_pluses
    }

    pub fn callouts(&self) -> &[CalloutAnnotation] {
        &self.state.callouts
    }

    pub fn measurement_paths(&self) -> &[MeasurementPathAnnotation] {
        &self.state.measurement_paths
    }

    pub fn pens(&self) -> &[PenAnnotation] {
        &self.state.pens
    }

    pub fn text_boxes(&self) -> &[TextBoxAnnotation] {
        &self.state.text_boxes
    }

    pub fn dimensions(&self) -> &[DimensionAnnotation] {
        &self.state.dimensions
    }

    pub fn lengths(&self) -> &[LengthAnnotation] {
        &self.state.lengths
    }

    pub fn images(&self) -> &[ImageAnnotation] {
        &self.state.images
    }

    pub fn snapshots(&self) -> &[SnapshotAnnotation] {
        &self.state.snapshots
    }

    pub(crate) fn load_imported_annotations(
        &mut self,
        annotations: Vec<Annotation>,
        page_length_calibrations: Vec<(u32, LengthCalibration)>,
    ) -> Result<(), AnnotationError> {
        self.load_imported_document_state(annotations, page_length_calibrations, Vec::new())
    }

    pub(crate) fn load_imported_document_state(
        &mut self,
        annotations: Vec<Annotation>,
        page_length_calibrations: Vec<(u32, LengthCalibration)>,
        page_rotations: Vec<(u32, PageRotation)>,
    ) -> Result<(), AnnotationError> {
        let legacy_calibrations = page_length_calibrations
            .iter()
            .cloned()
            .collect::<BTreeMap<_, _>>();
        let page_scales = page_length_calibrations
            .iter()
            .map(|(page_index, calibration)| {
                Ok((
                    *page_index,
                    PageScale::from_factors(
                        *page_index,
                        ScaleSource::Calibrated,
                        if calibration.label().is_empty() {
                            format!(
                                "Calibrated {} {}",
                                format_scale_number(calibration.real_world_value()),
                                calibration.unit()
                            )
                        } else {
                            calibration.label().to_owned()
                        },
                        ScaleUnit::In,
                        ScaleUnit::parse(calibration.unit())
                            .map_err(|error| AnnotationError::InvalidGeometry(error.to_string()))?,
                        calibration.scale_x(),
                        calibration.scale_y(),
                        calibration.scale_precision(),
                    )
                    .map_err(|error| AnnotationError::InvalidGeometry(error.to_string()))?,
                ))
            })
            .collect::<Result<Vec<_>, AnnotationError>>()?;
        self.load_imported_page_scale_state(annotations, page_scales, Vec::new(), page_rotations)?;
        self.state.page_length_calibrations = legacy_calibrations;
        for length in &mut self.state.lengths {
            if let Some(calibration) = self.state.page_length_calibrations.get(&length.page_index) {
                length.calibration = length.calibration.with_scale_from(calibration)?;
            }
        }
        for measurement in &mut self.state.measurement_paths {
            if let Some(calibration) = self
                .state
                .page_length_calibrations
                .get(&measurement.page_index)
            {
                measurement.calibration = measurement.calibration.with_scale_from(calibration)?;
            }
        }
        Ok(())
    }

    pub(crate) fn load_imported_page_scale_state(
        &mut self,
        annotations: Vec<Annotation>,
        page_scales: Vec<(u32, PageScale)>,
        scale_presets: Vec<ScalePreset>,
        page_rotations: Vec<(u32, PageRotation)>,
    ) -> Result<(), AnnotationError> {
        self.require_no_gesture()?;
        let mut ids = BTreeSet::new();
        let mut annotation_order = Vec::new();
        let mut rectangles = Vec::new();
        let mut redacts = Vec::new();
        let mut ellipses = Vec::new();
        let mut arcs = Vec::new();
        let mut straight_lines = Vec::new();
        let mut vertex_paths = Vec::new();
        let mut clouds = Vec::new();
        let mut cloud_pluses = Vec::new();
        let mut callouts = Vec::new();
        let mut measurement_paths = Vec::new();
        let mut pens = Vec::new();
        let mut text_boxes = Vec::new();
        let mut dimensions = Vec::new();
        let mut lengths = Vec::new();
        let mut images = Vec::new();
        let mut snapshots = Vec::new();
        for annotation in annotations {
            let id = annotation.id().clone();
            if !ids.insert(id.as_str().to_owned()) {
                return Err(AnnotationError::DuplicateMarkupId(id));
            }
            annotation_order.push(id);
            match annotation {
                Annotation::Rectangle(annotation) => rectangles.push(annotation),
                Annotation::Redact(annotation) => redacts.push(annotation),
                Annotation::Ellipse(annotation) => ellipses.push(annotation),
                Annotation::Arc(annotation) => arcs.push(annotation),
                Annotation::StraightLine(annotation) => straight_lines.push(annotation),
                Annotation::VertexPath(annotation) => vertex_paths.push(annotation),
                Annotation::Cloud(annotation) => clouds.push(annotation),
                Annotation::CloudPlus(annotation) => cloud_pluses.push(annotation),
                Annotation::Callout(annotation) => callouts.push(annotation),
                Annotation::MeasurementPath(annotation) => measurement_paths.push(annotation),
                Annotation::Pen(annotation) => pens.push(annotation),
                Annotation::TextBox(annotation) => text_boxes.push(annotation),
                Annotation::Dimension(annotation) => dimensions.push(annotation),
                Annotation::Length(annotation) => lengths.push(annotation),
                Annotation::Image(annotation) => images.push(annotation),
                Annotation::Snapshot(annotation) => snapshots.push(annotation),
            }
        }
        let rectangle_index = RectangleSpatialIndex::rebuild(&rectangles);
        let page_scales = page_scales.into_iter().collect::<BTreeMap<_, _>>();
        let page_length_calibrations = page_scales
            .iter()
            .map(|(page_index, scale)| Ok((*page_index, scale.length_calibration()?)))
            .collect::<Result<BTreeMap<_, _>, AnnotationError>>()?;
        let page_rotations = page_rotations.into_iter().collect::<BTreeMap<_, _>>();
        for length in &mut lengths {
            if let Some(scale) = page_length_calibrations.get(&length.page_index) {
                length.calibration = length.calibration.with_scale_from(scale)?;
            }
        }
        for measurement in &mut measurement_paths {
            if let Some(scale) = page_length_calibrations.get(&measurement.page_index) {
                measurement.calibration = measurement.calibration.with_scale_from(scale)?;
            }
        }
        self.state = DocumentState {
            annotation_order,
            rectangles,
            redacts,
            ellipses,
            arcs,
            rectangle_index,
            straight_lines,
            vertex_paths,
            clouds,
            cloud_pluses,
            callouts,
            measurement_paths,
            pens,
            text_boxes,
            dimensions,
            lengths,
            images,
            snapshots,
            page_scales,
            scale_presets,
            page_length_calibrations,
            page_rotations,
            revision: 0,
        };
        self.selected_ids.clear();
        self.past.clear();
        self.future.clear();
        self.saved_revision = 0;
        self.next_revision = 1;
        Ok(())
    }

    pub fn selected_id(&self) -> Option<&MarkupId> {
        self.selected_ids.first()
    }

    pub fn selected_ids(&self) -> &[MarkupId] {
        &self.selected_ids
    }

    pub fn selected_is_locked(&self) -> bool {
        self.selected_id()
            .and_then(|id| self.annotation_locked(id))
            .unwrap_or(false)
    }

    pub fn history_depths(&self) -> (usize, usize) {
        (self.past.len(), self.future.len())
    }

    pub fn active_preview(&self) -> Option<GesturePreview> {
        self.active_gesture
            .as_ref()
            .and_then(ActiveGesture::rectangle_preview)
    }

    pub fn snapshot(&self) -> AnnotationSnapshot {
        AnnotationSnapshot {
            revision: self.state.revision,
            saved_revision: self.saved_revision,
            dirty: self.state.revision != self.saved_revision,
            selected_id: self.selected_id().cloned(),
            annotation_order: self.state.annotation_order.clone(),
            rectangles: self.state.rectangles.clone(),
            redacts: self.state.redacts.clone(),
            ellipses: self.state.ellipses.clone(),
            arcs: self.state.arcs.clone(),
            straight_lines: self.state.straight_lines.clone(),
            vertex_paths: self.state.vertex_paths.clone(),
            clouds: self.state.clouds.clone(),
            cloud_pluses: self.state.cloud_pluses.clone(),
            callouts: self.state.callouts.clone(),
            measurement_paths: self.state.measurement_paths.clone(),
            pens: self.state.pens.clone(),
            text_boxes: self.state.text_boxes.clone(),
            dimensions: self.state.dimensions.clone(),
            lengths: self.state.lengths.clone(),
            images: self.state.images.clone(),
            snapshots: self.state.snapshots.clone(),
            page_scales: self.state.page_scales.values().cloned().collect(),
            scale_presets: self.state.scale_presets.clone(),
            page_length_calibrations: self
                .state
                .page_length_calibrations
                .iter()
                .map(|(page_index, calibration)| (*page_index, calibration.clone()))
                .collect(),
            page_rotations: self
                .state
                .page_rotations
                .iter()
                .map(|(page_index, rotation)| (*page_index, *rotation))
                .collect(),
            undo_depth: self.past.len(),
            redo_depth: self.future.len(),
        }
    }

    pub fn page_length_calibration(&self, page_index: u32) -> Option<&LengthCalibration> {
        self.state.page_length_calibrations.get(&page_index)
    }

    pub fn page_scale(&self, page_index: u32) -> Option<&PageScale> {
        self.state.page_scales.get(&page_index)
    }

    pub fn scale_presets(&self) -> &[ScalePreset] {
        &self.state.scale_presets
    }

    pub fn page_rotation(&self, page_index: u32) -> Option<PageRotation> {
        self.state.page_rotations.get(&page_index).copied()
    }

    pub fn rotate_page(
        &mut self,
        page_index: u32,
        direction: PageRotationDirection,
    ) -> Result<PageRotation, AnnotationError> {
        self.require_no_gesture()?;
        let current = self.page_rotation(page_index).ok_or_else(|| {
            AnnotationError::InvalidGeometry(format!(
                "page rotation target {page_index} does not exist"
            ))
        })?;
        let next = current.rotate(direction);
        self.commit_state_change(|state| {
            state.page_rotations.insert(page_index, next);
        });
        Ok(next)
    }

    /// Applies one page scale as a single revisioned document mutation. Length
    /// annotations retain a denormalized calibration for native PDF /Measure
    /// output, so they must move atomically with the authoritative page scale.
    pub fn set_page_length_calibration(
        &mut self,
        page_index: u32,
        calibration: LengthCalibration,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        let scale_changed = self
            .state
            .page_length_calibrations
            .get(&page_index)
            .is_none_or(|current| !current.same_scale_as(&calibration));
        let length_changed = self.state.lengths.iter().any(|length| {
            length.page_index == page_index && !length.calibration.same_scale_as(&calibration)
        });
        if !scale_changed && !length_changed {
            return Ok(false);
        }
        let page_scale = PageScale::from_factors(
            page_index,
            ScaleSource::Calibrated,
            if calibration.label().is_empty() {
                format!(
                    "Calibrated {} {}",
                    format_scale_number(calibration.real_world_value()),
                    calibration.unit()
                )
            } else {
                calibration.label().to_owned()
            },
            ScaleUnit::In,
            ScaleUnit::parse(calibration.unit())
                .map_err(|error| AnnotationError::InvalidGeometry(error.to_string()))?,
            calibration.scale_x(),
            calibration.scale_y(),
            calibration.scale_precision(),
        )
        .map_err(|error| AnnotationError::InvalidGeometry(error.to_string()))?;
        self.commit_state_change(move |state| {
            state.page_scales.insert(page_index, page_scale);
            state
                .page_length_calibrations
                .insert(page_index, calibration.clone());
            for length in state
                .lengths
                .iter_mut()
                .filter(|length| length.page_index == page_index)
            {
                length.calibration = length
                    .calibration
                    .with_scale_from(&calibration)
                    .expect("validated page and length calibration values must compose");
            }
        });
        Ok(true)
    }

    pub fn apply_page_scale(
        &mut self,
        scale: PageScale,
        target: PageScaleApplyTarget,
        page_count: u32,
    ) -> Result<bool, AnnotationError> {
        self.apply_page_scale_with_preset(scale, target, page_count, None)
    }

    pub fn apply_page_scale_with_preset(
        &mut self,
        scale: PageScale,
        target: PageScaleApplyTarget,
        page_count: u32,
        saved_preset: Option<ScalePreset>,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        if page_count == 0 {
            return Err(AnnotationError::InvalidGeometry(
                "page scale requires at least one page".into(),
            ));
        }
        let page_indices = match target {
            PageScaleApplyTarget::Current(page_index) => vec![page_index],
            PageScaleApplyTarget::All => (0..page_count).collect(),
            PageScaleApplyTarget::Ranges(ranges) => {
                let mut indices = BTreeSet::new();
                for range in ranges {
                    let start = range.start_page_index.min(range.end_page_index);
                    let end = range.start_page_index.max(range.end_page_index);
                    if end >= page_count {
                        return Err(AnnotationError::InvalidGeometry(format!(
                            "page scale target must be between 0 and {}",
                            page_count - 1
                        )));
                    }
                    indices.extend(start..=end);
                }
                if indices.is_empty() {
                    return Err(AnnotationError::InvalidGeometry(
                        "page scale target cannot be empty".into(),
                    ));
                }
                indices.into_iter().collect()
            }
        };
        if page_indices
            .iter()
            .any(|page_index| *page_index >= page_count)
        {
            return Err(AnnotationError::InvalidGeometry(format!(
                "page scale target must be between 0 and {}",
                page_count - 1
            )));
        }
        let replacements = page_indices
            .into_iter()
            .map(|page_index| (page_index, scale.with_page_index(page_index)))
            .collect::<BTreeMap<_, _>>();
        let scales_changed = replacements.iter().any(|(page_index, replacement)| {
            self.state.page_scales.get(page_index) != Some(replacement)
        });
        let lengths_changed = self.state.lengths.iter().any(|length| {
            replacements
                .get(&length.page_index)
                .and_then(|scale| scale.length_calibration().ok())
                .is_some_and(|calibration| !length.calibration.same_scale_as(&calibration))
        });
        let measurement_paths_changed = self.state.measurement_paths.iter().any(|measurement| {
            replacements
                .get(&measurement.page_index)
                .and_then(|scale| scale.length_calibration().ok())
                .is_some_and(|calibration| !measurement.calibration.same_scale_as(&calibration))
        });
        if let Some(preset) = &saved_preset {
            if preset.built_in || preset.id.is_empty() || preset.name.is_empty() {
                return Err(AnnotationError::InvalidGeometry(
                    "Saved scale preset is invalid.".into(),
                ));
            }
        }
        let preset_changed = saved_preset.as_ref().is_some_and(|preset| {
            self.state.scale_presets.first() != Some(preset)
                || self
                    .state
                    .scale_presets
                    .iter()
                    .skip(1)
                    .any(|candidate| candidate.id == preset.id)
        });
        if !scales_changed && !lengths_changed && !measurement_paths_changed && !preset_changed {
            return Ok(false);
        }
        self.commit_state_change(move |state| {
            if let Some(preset) = saved_preset {
                state
                    .scale_presets
                    .retain(|candidate| candidate.id != preset.id);
                state.scale_presets.insert(0, preset);
            }
            for (page_index, replacement) in replacements {
                let calibration = replacement
                    .length_calibration()
                    .expect("a validated page scale must produce a valid length calibration");
                state.page_scales.insert(page_index, replacement);
                state
                    .page_length_calibrations
                    .insert(page_index, calibration.clone());
                for length in state
                    .lengths
                    .iter_mut()
                    .filter(|length| length.page_index == page_index)
                {
                    length.calibration = length
                        .calibration
                        .with_scale_from(&calibration)
                        .expect("validated page and length calibration values must compose");
                }
                for measurement in state
                    .measurement_paths
                    .iter_mut()
                    .filter(|measurement| measurement.page_index == page_index)
                {
                    measurement.calibration = measurement
                        .calibration
                        .with_scale_from(&calibration)
                        .expect("validated page and measurement calibration values must compose");
                }
            }
        });
        Ok(true)
    }

    pub fn delete_scale_preset(&mut self, preset_id: &str) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        if built_in_scale_presets()
            .iter()
            .any(|preset| preset.id == preset_id)
            || self
                .state
                .scale_presets
                .iter()
                .any(|preset| preset.id == preset_id && preset.built_in)
        {
            return Err(AnnotationError::InvalidGeometry(
                "Built-in scale presets cannot be deleted.".into(),
            ));
        }
        if !self
            .state
            .scale_presets
            .iter()
            .any(|preset| preset.id == preset_id)
        {
            return Ok(false);
        }
        let preset_id = preset_id.to_owned();
        self.commit_state_change(move |state| {
            state.scale_presets.retain(|preset| preset.id != preset_id);
        });
        Ok(true)
    }

    pub fn document_scene(&self, page_index: u32) -> AnnotationScene {
        let preview = self.active_preview();
        let mut rectangles = self
            .state
            .rectangles
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneRectangle {
                id: annotation.id.clone(),
                rect: annotation.rect,
                rotation_degrees: annotation.rotation_degrees,
                appearance: annotation.appearance.clone(),
                selected: self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                preview: false,
            })
            .collect::<Vec<_>>();
        if let Some(preview) = preview.filter(|preview| preview.annotation.page_index == page_index)
        {
            let projected = SceneRectangle {
                id: preview.annotation.id.clone(),
                rect: preview.annotation.rect,
                rotation_degrees: preview.annotation.rotation_degrees,
                appearance: preview.annotation.appearance,
                selected: true,
                locked: preview.annotation.locked,
                preview: true,
            };
            if let Some(existing) = rectangles
                .iter_mut()
                .find(|rectangle| rectangle.id == projected.id)
            {
                *existing = projected;
            } else {
                rectangles.push(projected);
            }
        }
        AnnotationScene {
            page_index,
            revision: self.state.revision,
            rectangles,
            redacts: self.scene_redacts(page_index, true),
            ellipses: self.scene_ellipses(page_index, true),
            arcs: self.scene_arcs(page_index, true),
            straight_lines: self.scene_straight_lines(page_index, true),
            vertex_paths: self.scene_vertex_paths(page_index, true),
            clouds: self.scene_clouds(page_index, true),
            cloud_pluses: self.scene_cloud_pluses(page_index, true),
            callouts: self.scene_callouts(page_index, true),
            measurement_paths: self.scene_measurement_paths(page_index, true),
            pens: self.scene_pens(page_index, true),
            text_boxes: self.scene_text_boxes(page_index, true),
            dimensions: self.scene_dimensions(page_index, true),
            lengths: self.scene_lengths(page_index, true),
            images: self.scene_images(page_index, true),
            snapshots: self.scene_snapshots(page_index, true),
        }
    }

    pub fn thumbnail_scene(&self, page_index: u32) -> AnnotationScene {
        AnnotationScene {
            page_index,
            revision: self.state.revision,
            rectangles: self
                .state
                .rectangles
                .iter()
                .filter(|annotation| annotation.page_index == page_index)
                .map(|annotation| SceneRectangle {
                    id: annotation.id.clone(),
                    rect: annotation.rect,
                    rotation_degrees: annotation.rotation_degrees,
                    appearance: annotation.appearance.clone(),
                    selected: false,
                    locked: annotation.locked,
                    preview: false,
                })
                .collect(),
            redacts: self.scene_redacts(page_index, false),
            ellipses: self.scene_ellipses(page_index, false),
            arcs: self.scene_arcs(page_index, false),
            straight_lines: self.scene_straight_lines(page_index, false),
            vertex_paths: self.scene_vertex_paths(page_index, false),
            clouds: self.scene_clouds(page_index, false),
            cloud_pluses: self.scene_cloud_pluses(page_index, false),
            callouts: self.scene_callouts(page_index, false),
            measurement_paths: self.scene_measurement_paths(page_index, false),
            pens: self.scene_pens(page_index, false),
            text_boxes: self.scene_text_boxes(page_index, false),
            dimensions: self.scene_dimensions(page_index, false),
            lengths: self.scene_lengths(page_index, false),
            images: self.scene_images(page_index, false),
            snapshots: self.scene_snapshots(page_index, false),
        }
    }

    pub fn replay_rectangle_manifest(
        &mut self,
        manifest_json: &str,
    ) -> Result<FixtureReplayOutcome, AnnotationError> {
        self.require_no_gesture()?;
        if !self.state.rectangles.is_empty()
            || !self.state.redacts.is_empty()
            || !self.state.ellipses.is_empty()
            || !self.state.arcs.is_empty()
            || !self.state.straight_lines.is_empty()
            || !self.state.vertex_paths.is_empty()
            || !self.state.measurement_paths.is_empty()
            || !self.state.pens.is_empty()
            || !self.state.text_boxes.is_empty()
            || !self.state.dimensions.is_empty()
            || !self.state.lengths.is_empty()
            || !self.state.images.is_empty()
            || !self.state.snapshots.is_empty()
            || !self.past.is_empty()
            || !self.future.is_empty()
        {
            return Err(AnnotationError::InvalidFixture(
                "rectangle replay requires a new annotation document".into(),
            ));
        }
        let manifest: Value = serde_json::from_str(manifest_json)
            .map_err(|error| AnnotationError::InvalidFixture(error.to_string()))?;
        let fixture_id = fixture_string(&manifest, "fixture_id")?;
        if fixture_id != "bp-rectangle-v1" {
            return Err(AnnotationError::InvalidFixture(format!(
                "unsupported fixture {fixture_id}"
            )));
        }
        if fixture_string(&manifest, "coordinate_space")? != "pdf-points-bottom-left" {
            return Err(AnnotationError::InvalidFixture(
                "rectangle fixture must use PDF bottom-left points".into(),
            ));
        }
        let commands = manifest
            .get("commands")
            .and_then(Value::as_array)
            .ok_or_else(|| AnnotationError::InvalidFixture("commands must be an array".into()))?;
        let command_stream = canonicalize_json(json!({
            "schema_version": "bp-pdf-command-stream-v1",
            "fixture_id": fixture_id,
            "coordinate_space": "pdf-points-bottom-left",
            "commands": commands,
        }));
        let command_stream_hash = canonical_sha256(&command_stream);
        let expected_command_stream_hash = manifest
            .pointer("/artifact_sha256/commands")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AnnotationError::InvalidFixture("command stream hash is missing".into())
            })?;
        if command_stream_hash != expected_command_stream_hash {
            return Err(AnnotationError::InvalidFixture(format!(
                "command stream hash is {command_stream_hash}, expected {expected_command_stream_hash}"
            )));
        }
        let operations = commands
            .iter()
            .map(|command| fixture_string(command, "operation"))
            .collect::<Result<Vec<_>, _>>()?;
        let required_operations = [
            "create-rectangle",
            "select-annotation",
            "translate-annotation",
            "resize-annotation",
            "set-annotation-style",
            "undo",
            "redo",
            "assert-canonical-state",
        ];
        if operations != required_operations {
            return Err(AnnotationError::InvalidFixture(
                "rectangle command order does not match bp-rectangle-v1".into(),
            ));
        }

        let create = &commands[0];
        let annotation_id = MarkupId::new(fixture_string(create, "annotation_id")?)?;
        let path = create
            .get("pointer_path_pdf")
            .and_then(Value::as_array)
            .filter(|path| path.len() == 2)
            .ok_or_else(|| {
                AnnotationError::InvalidFixture(
                    "rectangle create path must have exactly two points".into(),
                )
            })?;
        let start = fixture_point(&path[0])?;
        let end = fixture_point(&path[1])?;
        let create_appearance =
            fixture_appearance(create.get("style").ok_or_else(|| {
                AnnotationError::InvalidFixture("create style is missing".into())
            })?)?;
        self.apply_command(AnnotationCommand::PointerDown {
            pointer_id: 1,
            page_index: 0,
            point: start,
            tolerance_pt: 4.0,
            tool: PointerTool::Rectangle {
                id: annotation_id.clone(),
                appearance: create_appearance,
            },
        })?;
        self.apply_command(AnnotationCommand::PointerMove {
            pointer_id: 1,
            point: end,
        })?;
        self.apply_command(AnnotationCommand::PointerUp {
            pointer_id: 1,
            point: end,
        })?;

        let select_point =
            fixture_point(commands[1].get("point_pdf").ok_or_else(|| {
                AnnotationError::InvalidFixture("select point is missing".into())
            })?)?;
        self.apply_command(AnnotationCommand::PointerDown {
            pointer_id: 2,
            page_index: 0,
            point: select_point,
            tolerance_pt: 4.0,
            tool: PointerTool::Select {
                rotation_handle_offset_pt: ROTATION_HANDLE_OFFSET_PT,
            },
        })?;
        self.apply_command(AnnotationCommand::PointerUp {
            pointer_id: 2,
            point: select_point,
        })?;

        let delta = fixture_point(
            commands[2]
                .get("delta_pdf")
                .ok_or_else(|| AnnotationError::InvalidFixture("move delta is missing".into()))?,
        )?;
        let rect = self
            .annotation(&annotation_id)
            .ok_or_else(|| AnnotationError::InvalidFixture("created rectangle is missing".into()))?
            .rect;
        let move_start = PdfPoint::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)?;
        let move_end = PdfPoint::new(move_start.x + delta.x, move_start.y + delta.y)?;
        self.apply_command(AnnotationCommand::PointerDown {
            pointer_id: 3,
            page_index: 0,
            point: move_start,
            tolerance_pt: 4.0,
            tool: PointerTool::Select {
                rotation_handle_offset_pt: ROTATION_HANDLE_OFFSET_PT,
            },
        })?;
        self.apply_command(AnnotationCommand::PointerUp {
            pointer_id: 3,
            point: move_end,
        })?;

        let resize_delta =
            fixture_point(commands[3].get("delta_pdf").ok_or_else(|| {
                AnnotationError::InvalidFixture("resize delta is missing".into())
            })?)?;
        let rect = self
            .annotation(&annotation_id)
            .ok_or_else(|| AnnotationError::InvalidFixture("moved rectangle is missing".into()))?
            .rect;
        let resize_start = PdfPoint::new(rect.x + rect.width, rect.y + rect.height / 2.0)?;
        let resize_end = PdfPoint::new(resize_start.x + resize_delta.x, resize_start.y)?;
        self.apply_command(AnnotationCommand::PointerDown {
            pointer_id: 4,
            page_index: 0,
            point: resize_start,
            tolerance_pt: 4.0,
            tool: PointerTool::Select {
                rotation_handle_offset_pt: ROTATION_HANDLE_OFFSET_PT,
            },
        })?;
        self.apply_command(AnnotationCommand::PointerUp {
            pointer_id: 4,
            point: resize_end,
        })?;

        let style =
            fixture_appearance(commands[4].get("style").ok_or_else(|| {
                AnnotationError::InvalidFixture("updated style is missing".into())
            })?)?;
        self.apply_command(AnnotationCommand::SetSelectedAppearance(style))?;
        self.apply_command(AnnotationCommand::Undo)?;
        self.apply_command(AnnotationCommand::Redo)?;

        let page_id = manifest
            .pointer("/document/pages/0/page_id")
            .and_then(Value::as_str)
            .ok_or_else(|| AnnotationError::InvalidFixture("page id is missing".into()))?;
        let annotation = self
            .annotation(&annotation_id)
            .ok_or_else(|| AnnotationError::InvalidFixture("final rectangle is missing".into()))?;
        let (undo_depth, redo_depth) = self.history_depths();
        let canonical_state = canonicalize_json(json!({
            "schema_version": "bp-canonical-annotation-state-v1",
            "fixture_id": fixture_id,
            "document": { "page_count": 1, "page_ids": [page_id] },
            "annotations": [{
                "annotation_id": annotation.id.as_str(),
                "type": "rectangle",
                "page_id": page_id,
                "bounds": {
                    "x": fixture_number_value(annotation.rect.x),
                    "y": fixture_number_value(annotation.rect.y),
                    "width": fixture_number_value(annotation.rect.width),
                    "height": fixture_number_value(annotation.rect.height),
                },
                "style": fixture_canonical_style(&annotation.appearance),
            }],
            "selected_annotation_ids": [annotation.id.as_str()],
            "history": { "undo_depth": undo_depth, "redo_depth": redo_depth },
            "dirty": self.snapshot().dirty,
        }));
        let expected_state = manifest
            .pointer("/canonical_expected/state")
            .cloned()
            .ok_or_else(|| {
                AnnotationError::InvalidFixture("canonical expected state is missing".into())
            })?;
        if canonical_state != canonicalize_json(expected_state) {
            return Err(AnnotationError::CanonicalFixtureMismatch(
                "final state differs from canonical_expected.state".into(),
            ));
        }
        let canonical_sha256 = canonical_sha256(&canonical_state);
        let expected_sha256 = manifest
            .pointer("/canonical_expected/sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                AnnotationError::InvalidFixture("canonical expected hash is missing".into())
            })?;
        if canonical_sha256 != expected_sha256 {
            return Err(AnnotationError::CanonicalFixtureMismatch(format!(
                "computed {canonical_sha256}, expected {expected_sha256}"
            )));
        }
        Ok(FixtureReplayOutcome {
            fixture_id: fixture_id.to_string(),
            canonical_sha256,
        })
    }

    pub fn apply_command(
        &mut self,
        command: AnnotationCommand,
    ) -> Result<CommandOutcome, AnnotationError> {
        match command {
            AnnotationCommand::CreateAnnotation(annotation) => {
                let id = annotation.id().clone();
                let kind = annotation.kind();
                self.create_annotation(annotation)?;
                Ok(CommandOutcome::AnnotationCreated {
                    id,
                    kind,
                    revision: self.state.revision,
                })
            }
            AnnotationCommand::EditAnnotation { id, edit } => {
                let (kind, changed) = self.edit_annotation(&id, edit)?;
                Ok(CommandOutcome::AnnotationEdited {
                    id,
                    kind,
                    changed,
                    revision: self.state.revision,
                })
            }
            AnnotationCommand::BeginPen {
                pointer_id,
                id,
                page_index,
                start,
                appearance,
                smooth_curves,
            } => {
                self.begin_ink(
                    pointer_id,
                    id.clone(),
                    page_index,
                    start,
                    appearance,
                    (InkTool::Pen, smooth_curves),
                )?;
                Ok(CommandOutcome::PenStarted { id })
            }
            AnnotationCommand::BeginHighlight {
                pointer_id,
                id,
                page_index,
                start,
                appearance,
                smooth_curves,
            } => {
                self.begin_highlight(
                    pointer_id,
                    id.clone(),
                    page_index,
                    start,
                    appearance,
                    smooth_curves,
                )?;
                Ok(CommandOutcome::PenStarted { id })
            }
            AnnotationCommand::BeginInk {
                pointer_id,
                id,
                page_index,
                start,
                appearance,
                smooth_curves,
                tool,
            } => {
                self.begin_ink(
                    pointer_id,
                    id.clone(),
                    page_index,
                    start,
                    appearance,
                    (tool, smooth_curves),
                )?;
                Ok(CommandOutcome::PenStarted { id })
            }
            AnnotationCommand::AppendPenSamples {
                pointer_id,
                samples,
                min_distance_pt,
            } => {
                let (id, accepted, total) =
                    self.append_pen_samples(pointer_id, samples, min_distance_pt)?;
                Ok(CommandOutcome::PenSamplesAppended {
                    id,
                    accepted,
                    total,
                })
            }
            AnnotationCommand::CommitPen { pointer_id } => {
                match self.commit_gesture(pointer_id)? {
                    CommitOutcome::Created(id) => Ok(CommandOutcome::AnnotationCreated {
                        id,
                        kind: AnnotationKind::Pen,
                        revision: self.state.revision,
                    }),
                    outcome => Ok(CommandOutcome::GestureCommitted(outcome)),
                }
            }
            AnnotationCommand::PointerDown {
                pointer_id,
                page_index,
                point,
                tolerance_pt,
                tool,
            } => match tool {
                PointerTool::Rectangle { id, appearance } => {
                    self.begin_create(pointer_id, id.clone(), page_index, point, appearance)?;
                    Ok(CommandOutcome::GestureStarted {
                        kind: GestureKind::Create,
                        id,
                    })
                }
                PointerTool::Select {
                    rotation_handle_offset_pt,
                } => match self.hit_test_with_rotation_handle_offset(
                    page_index,
                    point,
                    tolerance_pt,
                    rotation_handle_offset_pt,
                )? {
                    Some(HitTarget::RotationHandle(id)) => {
                        self.begin_rotation_with_rotation_handle_offset(
                            pointer_id,
                            page_index,
                            point,
                            tolerance_pt,
                            rotation_handle_offset_pt,
                        )?;
                        Ok(CommandOutcome::GestureStarted {
                            kind: GestureKind::Rotate,
                            id,
                        })
                    }
                    Some(HitTarget::ResizeHandle { id, handle }) => {
                        self.begin_resize(pointer_id, page_index, point, tolerance_pt)?;
                        Ok(CommandOutcome::GestureStarted {
                            kind: GestureKind::Resize(handle),
                            id,
                        })
                    }
                    Some(HitTarget::LineEndpoint { id, .. }) => {
                        self.select(&id);
                        Ok(CommandOutcome::SelectionChanged(Some(id)))
                    }
                    Some(HitTarget::Body(id)) => {
                        if self.straight_line(&id).is_some() {
                            self.select(&id);
                            return Ok(CommandOutcome::SelectionChanged(Some(id)));
                        }
                        self.begin_move(pointer_id, page_index, point, tolerance_pt)?;
                        Ok(CommandOutcome::GestureStarted {
                            kind: GestureKind::Move,
                            id,
                        })
                    }
                    None => {
                        self.clear_selection();
                        Ok(CommandOutcome::SelectionChanged(None))
                    }
                },
            },
            AnnotationCommand::PointerMove { pointer_id, point } => self
                .update_gesture(pointer_id, point)
                .map(CommandOutcome::PreviewUpdated),
            AnnotationCommand::PointerUp { pointer_id, point } => {
                self.update_gesture(pointer_id, point)?;
                self.commit_gesture(pointer_id)
                    .map(CommandOutcome::GestureCommitted)
            }
            AnnotationCommand::PointerCancel { pointer_id, reason } => {
                self.cancel_gesture(pointer_id)?;
                Ok(CommandOutcome::GestureCancelled { reason })
            }
            AnnotationCommand::SetSelectedAppearance(appearance) => {
                let changed = self.set_selected_appearance(appearance)?;
                Ok(CommandOutcome::AppearanceChanged {
                    changed,
                    revision: self.state.revision,
                })
            }
            AnnotationCommand::Undo => {
                let changed = self.undo()?;
                Ok(CommandOutcome::HistoryChanged {
                    direction: HistoryDirection::Undo,
                    changed,
                    revision: self.state.revision,
                })
            }
            AnnotationCommand::Redo => {
                let changed = self.redo()?;
                Ok(CommandOutcome::HistoryChanged {
                    direction: HistoryDirection::Redo,
                    changed,
                    revision: self.state.revision,
                })
            }
            AnnotationCommand::MarkSaved => {
                self.require_no_gesture()?;
                self.saved_revision = self.state.revision;
                Ok(CommandOutcome::Saved {
                    revision: self.state.revision,
                })
            }
            AnnotationCommand::SetLocked { id, locked } => {
                let changed = self.set_locked(&id, locked)?;
                Ok(CommandOutcome::LockChanged {
                    id,
                    locked,
                    changed,
                    revision: self.state.revision,
                })
            }
            AnnotationCommand::DeleteSelected => {
                let id = self.delete_selected()?;
                Ok(CommandOutcome::Deleted {
                    id,
                    revision: self.state.revision,
                })
            }
        }
    }

    /// Replaces text while retaining the undo boundary immediately before the
    /// selected text box was created. This is deliberately narrower than a
    /// general history merge: the prior state must not contain the target ID.
    pub fn replace_text_box_content_in_create_transaction(
        &mut self,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        if self.text_box(id).is_none() {
            return Err(AnnotationError::NoSelection);
        }
        let Some(undo_before_create) = self.past.back() else {
            return Err(AnnotationError::InvalidFixture(
                "text create transaction requires a prior undo boundary".into(),
            ));
        };
        if undo_before_create
            .text_boxes
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            return Err(AnnotationError::InvalidFixture(
                "text create transaction cannot merge edits for a pre-existing annotation".into(),
            ));
        }

        let undo_before_create = self
            .past
            .pop_back()
            .expect("the checked create undo boundary remains available");
        let edit = self.edit_annotation(id, AnnotationEdit::SetTextBoxContent(content.into()));
        match edit {
            Ok((AnnotationKind::TextBox, changed)) => {
                if changed {
                    self.past
                        .pop_back()
                        .expect("a changed text edit records its immediate prior state");
                }
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Ok(changed)
            }
            Ok((_, _)) => unreachable!("a text content edit has text-box kind"),
            Err(error) => {
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Err(error)
            }
        }
    }

    /// Replaces the initial Callout text while retaining the undo boundary
    /// immediately before that Callout was created.
    pub fn replace_callout_content_in_create_transaction(
        &mut self,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        if self.callout(id).is_none() {
            return Err(AnnotationError::NoSelection);
        }
        let Some(undo_before_create) = self.past.back() else {
            return Err(AnnotationError::InvalidFixture(
                "callout create transaction requires a prior undo boundary".into(),
            ));
        };
        if undo_before_create
            .callouts
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            return Err(AnnotationError::InvalidFixture(
                "callout create transaction cannot merge edits for a pre-existing annotation"
                    .into(),
            ));
        }

        let undo_before_create = self
            .past
            .pop_back()
            .expect("the checked Callout create undo boundary remains available");
        let edit = self.edit_annotation(id, AnnotationEdit::SetCalloutContent(content.into()));
        match edit {
            Ok((AnnotationKind::Callout, changed)) => {
                if changed {
                    self.past
                        .pop_back()
                        .expect("a changed Callout text edit records its immediate prior state");
                }
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Ok(changed)
            }
            Ok((_, _)) => unreachable!("a Callout text edit has Callout kind"),
            Err(error) => {
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Err(error)
            }
        }
    }

    /// Replaces the initial Dimension caption while retaining the undo
    /// boundary immediately before that Dimension was created.
    pub fn replace_dimension_content_in_create_transaction(
        &mut self,
        id: &MarkupId,
        content: impl Into<String>,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        if self.dimension(id).is_none() {
            return Err(AnnotationError::NoSelection);
        }
        let Some(undo_before_create) = self.past.back() else {
            return Err(AnnotationError::InvalidFixture(
                "dimension create transaction requires a prior undo boundary".into(),
            ));
        };
        if undo_before_create
            .dimensions
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            return Err(AnnotationError::InvalidFixture(
                "dimension create transaction cannot merge edits for a pre-existing annotation"
                    .into(),
            ));
        }

        let undo_before_create = self
            .past
            .pop_back()
            .expect("the checked Dimension create undo boundary remains available");
        let edit = self.edit_annotation(id, AnnotationEdit::SetDimensionContent(content.into()));
        match edit {
            Ok((AnnotationKind::Dimension, changed)) => {
                if changed {
                    self.past
                        .pop_back()
                        .expect("a changed Dimension text edit records its immediate prior state");
                }
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Ok(changed)
            }
            Ok((_, _)) => unreachable!("a Dimension text edit has Dimension kind"),
            Err(error) => {
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Err(error)
            }
        }
    }

    /// Replaces the initial Cloud+ text while retaining the undo boundary
    /// immediately before that logical composite was created.
    pub fn replace_cloud_plus_content_and_layout_in_create_transaction(
        &mut self,
        id: &MarkupId,
        content: impl Into<String>,
        text_box: PdfRect,
        leader_points: Vec<PdfPoint>,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        if self.cloud_plus(id).is_none() {
            return Err(AnnotationError::NoSelection);
        }
        let Some(undo_before_create) = self.past.back() else {
            return Err(AnnotationError::InvalidFixture(
                "Cloud+ create transaction requires a prior undo boundary".into(),
            ));
        };
        if undo_before_create
            .cloud_pluses
            .iter()
            .any(|annotation| &annotation.id == id)
        {
            return Err(AnnotationError::InvalidFixture(
                "Cloud+ create transaction cannot merge edits for a pre-existing annotation".into(),
            ));
        }

        let undo_before_create = self
            .past
            .pop_back()
            .expect("the checked Cloud+ create undo boundary remains available");
        let edit = self.edit_annotation(
            id,
            AnnotationEdit::SetCloudPlusContentAndLayout {
                content: content.into(),
                text_box,
                leader_points,
            },
        );
        match edit {
            Ok((AnnotationKind::CloudPlus, changed)) => {
                if changed {
                    self.past
                        .pop_back()
                        .expect("a changed Cloud+ text edit records its immediate prior state");
                }
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Ok(changed)
            }
            Ok((_, _)) => unreachable!("a Cloud+ text edit has Cloud+ kind"),
            Err(error) => {
                push_bounded(&mut self.past, undo_before_create, self.history_limit);
                Err(error)
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_ids.clear();
    }

    pub fn select(&mut self, id: &MarkupId) -> bool {
        if !self.contains_annotation(id) {
            return false;
        }
        if !self.selected_ids.contains(id) {
            self.selected_ids.clear();
            self.selected_ids.push(id.clone());
        }
        true
    }

    pub fn toggle_selection(&mut self, id: &MarkupId) -> bool {
        if !self.contains_annotation(id) {
            return false;
        }
        if let Some(index) = self.selected_ids.iter().position(|selected| selected == id) {
            self.selected_ids.remove(index);
        } else {
            self.selected_ids.push(id.clone());
        }
        true
    }

    pub fn translate_selection_on_page(
        &mut self,
        page_index: u32,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        require_finite("selection.delta_x", delta_x)?;
        require_finite("selection.delta_y", delta_y)?;
        if self.selected_ids.is_empty() {
            return Err(AnnotationError::NoSelection);
        }
        if delta_x == 0. && delta_y == 0. {
            return Ok(false);
        }

        let updates = self
            .selected_ids
            .iter()
            .filter(|id| {
                self.annotation_page(id) == Some(page_index)
                    && self.annotation_locked(id) == Some(false)
            })
            .map(|id| {
                self.annotation_owned(id)
                    .expect("a selected annotation must retain its document value")
                    .translated_copy(id.clone(), page_index, delta_x, delta_y)
            })
            .collect::<Result<Vec<_>, AnnotationError>>()?;
        if updates.is_empty() {
            return Ok(false);
        }

        self.commit_state_change(move |state| {
            for update in updates {
                match update {
                    Annotation::Rectangle(update) => {
                        let id = update.id.clone();
                        *state
                            .rectangles
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its Rectangle") = update;
                    }
                    Annotation::Redact(update) => {
                        let id = update.id.clone();
                        *state
                            .redacts
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its pending Redact") = update;
                    }
                    Annotation::Ellipse(update) => {
                        let id = update.id.clone();
                        *state
                            .ellipses
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its Ellipse") = update;
                    }
                    Annotation::Arc(update) => {
                        let id = update.id.clone();
                        *state
                            .arcs
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its Arc") = update;
                    }
                    Annotation::StraightLine(update) => {
                        let id = update.id.clone();
                        *state
                            .straight_lines
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its straight line") = update;
                    }
                    Annotation::VertexPath(update) => {
                        let id = update.id.clone();
                        *state
                            .vertex_paths
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its vertex path") = update;
                    }
                    Annotation::Cloud(update) => {
                        let id = update.id.clone();
                        *state
                            .clouds
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its cloud") = update;
                    }
                    Annotation::CloudPlus(update) => {
                        let id = update.id.clone();
                        *state
                            .cloud_pluses
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its Cloud+") = update;
                    }
                    Annotation::Callout(update) => {
                        let id = update.id.clone();
                        *state
                            .callouts
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its callout") = update;
                    }
                    Annotation::Dimension(update) => {
                        let id = update.id.clone();
                        *state
                            .dimensions
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its dimension") = update;
                    }
                    Annotation::MeasurementPath(update) => {
                        let id = update.id.clone();
                        *state
                            .measurement_paths
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its measurement path") = update;
                    }
                    Annotation::Pen(update) => {
                        let id = update.id.clone();
                        *state
                            .pens
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its ink annotation") = update;
                    }
                    Annotation::TextBox(update) => {
                        let id = update.id.clone();
                        *state
                            .text_boxes
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its text box") = update;
                    }
                    Annotation::Length(update) => {
                        let id = update.id.clone();
                        *state
                            .lengths
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its Length") = update;
                    }
                    Annotation::Image(update) => {
                        let id = update.id.clone();
                        *state
                            .images
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its Image") = update;
                    }
                    Annotation::Snapshot(update) => {
                        let id = update.id.clone();
                        *state
                            .snapshots
                            .iter_mut()
                            .find(|annotation| annotation.id == id)
                            .expect("a group move must retain its Snapshot") = update;
                    }
                }
            }
        });
        Ok(true)
    }

    pub fn selected_annotations_in_document_order(&self) -> Vec<Annotation> {
        self.state
            .annotation_order
            .iter()
            .filter(|id| self.selected_ids.contains(id))
            .filter_map(|id| self.annotation_owned(id))
            .collect()
    }

    pub fn selected_has_unlocked(&self) -> bool {
        self.selected_ids
            .iter()
            .any(|id| self.annotation_locked(id) == Some(false))
    }

    pub fn select_all_on_page(&mut self, page_index: u32) -> &[MarkupId] {
        self.selected_ids = self
            .state
            .annotation_order
            .iter()
            .filter(|id| self.annotation_page(id) == Some(page_index))
            .cloned()
            .collect();
        &self.selected_ids
    }

    pub fn apply_marquee_selection(
        &mut self,
        page_index: u32,
        marquee: &SelectionMarquee,
        to_viewport: impl Fn(PdfPoint) -> SelectionPoint,
    ) -> &[MarkupId] {
        if !marquee.active {
            return &self.selected_ids;
        }
        let hits = self
            .state
            .annotation_order
            .iter()
            .filter_map(|id| self.annotation_owned(id))
            .filter(|annotation| self.annotation_page(annotation.id()) == Some(page_index))
            .filter(|annotation| {
                geometry_selected(
                    &annotation_selection_paths(annotation, &to_viewport),
                    marquee,
                )
            })
            .map(|annotation| annotation.id().clone())
            .collect::<Vec<_>>();
        self.selected_ids = selection_after(&self.selected_ids, &hits, marquee.operation);
        &self.selected_ids
    }

    pub fn insert_annotations(
        &mut self,
        annotations: Vec<Annotation>,
    ) -> Result<Vec<MarkupId>, AnnotationError> {
        self.require_no_gesture()?;
        if annotations.is_empty() {
            return Ok(Vec::new());
        }
        let mut incoming_ids = BTreeSet::new();
        for annotation in &annotations {
            let id = annotation.id();
            if self.contains_annotation(id) || !incoming_ids.insert(id.as_str().to_owned()) {
                return Err(AnnotationError::DuplicateMarkupId(id.clone()));
            }
        }
        let ids = annotations
            .iter()
            .map(|annotation| annotation.id().clone())
            .collect::<Vec<_>>();
        let order_ids = ids.clone();
        self.commit_state_change(move |state| {
            state.annotation_order.extend(order_ids);
            for annotation in annotations {
                match annotation {
                    Annotation::Rectangle(annotation) => state.rectangles.push(annotation),
                    Annotation::Redact(annotation) => state.redacts.push(annotation),
                    Annotation::Ellipse(annotation) => state.ellipses.push(annotation),
                    Annotation::Arc(annotation) => state.arcs.push(annotation),
                    Annotation::StraightLine(annotation) => state.straight_lines.push(annotation),
                    Annotation::VertexPath(annotation) => state.vertex_paths.push(annotation),
                    Annotation::Cloud(annotation) => state.clouds.push(annotation),
                    Annotation::CloudPlus(annotation) => state.cloud_pluses.push(annotation),
                    Annotation::Callout(annotation) => state.callouts.push(annotation),
                    Annotation::MeasurementPath(annotation) => {
                        state.measurement_paths.push(annotation)
                    }
                    Annotation::Pen(annotation) => state.pens.push(annotation),
                    Annotation::TextBox(annotation) => state.text_boxes.push(annotation),
                    Annotation::Dimension(annotation) => state.dimensions.push(annotation),
                    Annotation::Length(annotation) => state.lengths.push(annotation),
                    Annotation::Image(annotation) => state.images.push(annotation),
                    Annotation::Snapshot(annotation) => state.snapshots.push(annotation),
                }
            }
        });
        self.selected_ids = ids.clone();
        Ok(ids)
    }

    pub fn delete_selected_unlocked(&mut self) -> Result<Vec<MarkupId>, AnnotationError> {
        self.require_no_gesture()?;
        if self.selected_ids.is_empty() {
            return Err(AnnotationError::NoSelection);
        }
        let deleted = self
            .selected_ids
            .iter()
            .filter(|id| self.annotation_locked(id) == Some(false))
            .cloned()
            .collect::<Vec<_>>();
        if deleted.is_empty() {
            return Ok(Vec::new());
        }
        let deleted_for_state = deleted.clone();
        self.commit_state_change(move |state| {
            state
                .annotation_order
                .retain(|id| !deleted_for_state.contains(id));
            state
                .rectangles
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .redacts
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .ellipses
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .arcs
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .straight_lines
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .vertex_paths
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .clouds
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .cloud_pluses
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .callouts
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .measurement_paths
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .pens
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .text_boxes
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .lengths
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .images
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
            state
                .snapshots
                .retain(|annotation| !deleted_for_state.contains(&annotation.id));
        });
        self.selected_ids.retain(|id| !deleted.contains(id));
        Ok(deleted)
    }

    pub fn hit_test(
        &self,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<HitTarget>, AnnotationError> {
        self.hit_test_with_rotation_handle_offset(
            page_index,
            point,
            tolerance_pt,
            ROTATION_HANDLE_OFFSET_PT,
        )
    }

    fn hit_test_with_rotation_handle_offset(
        &self,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
        rotation_handle_offset_pt: f64,
    ) -> Result<Option<HitTarget>, AnnotationError> {
        validate_tolerance(tolerance_pt)?;
        validate_tolerance(rotation_handle_offset_pt)?;
        if let Some(selected) = self
            .selected_id()
            .and_then(|id| self.straight_line(id))
            .filter(|annotation| annotation.page_index == page_index)
        {
            if point_distance(selected.end, point) <= tolerance_pt {
                return Ok(Some(HitTarget::LineEndpoint {
                    id: selected.id.clone(),
                    endpoint: LineEndpoint::End,
                }));
            }
            if point_distance(selected.start, point) <= tolerance_pt {
                return Ok(Some(HitTarget::LineEndpoint {
                    id: selected.id.clone(),
                    endpoint: LineEndpoint::Start,
                }));
            }
        }
        if let Some(selected) = self.selected_id().and_then(|id| self.annotation(id))
            && selected.page_index == page_index
        {
            if point_distance(
                selected.rotation_handle_world_point(rotation_handle_offset_pt),
                point,
            ) <= tolerance_pt
            {
                return Ok(Some(HitTarget::RotationHandle(selected.id.clone())));
            }
            if let Some(handle) = RectangleResizeHandle::ALL.into_iter().find(|handle| {
                point_distance(
                    handle.world_point(selected.rect, selected.rotation_degrees),
                    point,
                ) <= tolerance_pt
            }) {
                return Ok(Some(HitTarget::ResizeHandle {
                    id: selected.id.clone(),
                    handle,
                }));
            }
        }
        if let Some(hit) = self
            .state
            .rectangle_index
            .candidates(page_index, point, tolerance_pt)
            .into_iter()
            .rev()
            .map(|index| &self.state.rectangles[index])
            .find(|annotation| {
                let local_point = annotation.world_to_local(point);
                annotation.page_index == page_index
                    && (annotation.rect.near_perimeter(local_point, tolerance_pt)
                        || annotation.rect.contains(local_point, tolerance_pt))
            })
            .map(|annotation| HitTarget::Body(annotation.id.clone()))
        {
            return Ok(Some(hit));
        }
        Ok(self
            .state
            .straight_lines
            .iter()
            .rev()
            .find(|annotation| {
                annotation.page_index == page_index
                    && point_segment_distance(point, annotation.start, annotation.end)
                        <= tolerance_pt.max(annotation.appearance.stroke_width_pt / 2.0)
            })
            .map(|annotation| HitTarget::Body(annotation.id.clone())))
    }

    pub fn spatial_query_work(
        &self,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<SpatialQueryWork, AnnotationError> {
        validate_tolerance(tolerance_pt)?;
        Ok(SpatialQueryWork {
            candidate_count: self
                .state
                .rectangle_index
                .candidates(page_index, point, tolerance_pt)
                .len(),
            total_rectangle_count: self.state.rectangles.len(),
        })
    }

    pub fn select_at(
        &mut self,
        page_index: u32,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<HitTarget>, AnnotationError> {
        let hit = self.hit_test(page_index, point, tolerance_pt)?;
        self.selected_ids = hit
            .as_ref()
            .map(|target| vec![target.markup_id().clone()])
            .unwrap_or_default();
        Ok(hit)
    }

    pub fn begin_create(
        &mut self,
        pointer_id: u64,
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        appearance: RectangleAppearance,
    ) -> Result<(), AnnotationError> {
        self.require_no_gesture()?;
        if self.contains_annotation(&id) {
            return Err(AnnotationError::DuplicateMarkupId(id));
        }
        self.active_gesture = Some(ActiveGesture::Create {
            pointer_id,
            annotation: RectangleAnnotation {
                id,
                page_index,
                rect: PdfRect::from_corners(start, start),
                rotation_degrees: 0.0,
                appearance,
                locked: false,
            },
            start,
        });
        Ok(())
    }

    pub fn begin_pen(
        &mut self,
        pointer_id: u64,
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        appearance: PenAppearance,
        smooth_curves: bool,
    ) -> Result<(), AnnotationError> {
        self.begin_ink(
            pointer_id,
            id,
            page_index,
            start,
            appearance,
            (InkTool::Pen, smooth_curves),
        )
    }

    pub fn begin_highlight(
        &mut self,
        pointer_id: u64,
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        appearance: PenAppearance,
        smooth_curves: bool,
    ) -> Result<(), AnnotationError> {
        self.begin_ink(
            pointer_id,
            id,
            page_index,
            start,
            appearance,
            (InkTool::Highlight, smooth_curves),
        )
    }

    fn begin_ink(
        &mut self,
        pointer_id: u64,
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        appearance: PenAppearance,
        behavior: (InkTool, bool),
    ) -> Result<(), AnnotationError> {
        let (tool, requested_smooth_curves) = behavior;
        let smooth_curves = tool == InkTool::Pen && requested_smooth_curves;
        self.require_no_gesture()?;
        if self.contains_annotation(&id) {
            return Err(AnnotationError::DuplicateMarkupId(id));
        }
        require_finite("pen.start.x", start.x)?;
        require_finite("pen.start.y", start.y)?;
        self.active_gesture = Some(ActiveGesture::Pen {
            pointer_id,
            annotation: PenAnnotation {
                id,
                page_index,
                points: vec![start],
                additional_paths: Vec::new(),
                appearance,
                smooth_curves,
                tool,
                blend_mode: match tool {
                    InkTool::Pen => BlendMode::Normal,
                    InkTool::Highlight => BlendMode::Multiply,
                },
                locked: false,
            },
        });
        Ok(())
    }

    pub fn append_pen_samples(
        &mut self,
        pointer_id: u64,
        samples: Vec<PdfPoint>,
        min_distance_pt: f64,
    ) -> Result<(MarkupId, usize, usize), AnnotationError> {
        validate_tolerance(min_distance_pt)?;
        if samples.is_empty() || samples.len() > MAX_COALESCED_PEN_SAMPLES {
            return Err(AnnotationError::InvalidGeometry(format!(
                "coalesced pen batch must contain 1 to {MAX_COALESCED_PEN_SAMPLES} samples"
            )));
        }
        for sample in &samples {
            require_finite("pen.sample.x", sample.x)?;
            require_finite("pen.sample.y", sample.y)?;
        }
        let gesture = self
            .active_gesture
            .as_mut()
            .ok_or(AnnotationError::NoActiveGesture)?;
        require_pointer(gesture, pointer_id)?;
        let ActiveGesture::Pen { annotation, .. } = gesture else {
            return Err(AnnotationError::InvalidGeometry(
                "active gesture is not a pen stream".into(),
            ));
        };
        let mut accepted = Vec::new();
        let mut last = *annotation
            .points
            .last()
            .expect("a pen draft always contains its start sample");
        for sample in samples {
            if point_distance(last, sample) >= min_distance_pt {
                accepted.push(sample);
                last = sample;
            }
        }
        if annotation.points.len().saturating_add(accepted.len()) > MAX_STREAMED_PATH_POINTS {
            return Err(AnnotationError::InvalidGeometry(format!(
                "pen path exceeds the {MAX_STREAMED_PATH_POINTS}-point limit"
            )));
        }
        let accepted_count = accepted.len();
        annotation.points.extend(accepted);
        Ok((
            annotation.id.clone(),
            accepted_count,
            annotation.points.len(),
        ))
    }

    pub fn begin_move(
        &mut self,
        pointer_id: u64,
        page_index: u32,
        start: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        self.require_no_gesture()?;
        let Some(HitTarget::Body(id)) = self.select_at(page_index, start, tolerance_pt)? else {
            return Ok(None);
        };
        let annotation = self
            .annotation(&id)
            .expect("a hit-tested annotation must exist")
            .clone();
        if annotation.locked {
            return Err(AnnotationError::LockedMarkup(id));
        }
        self.active_gesture = Some(ActiveGesture::Move {
            pointer_id,
            original: annotation.rect,
            annotation,
            start,
        });
        Ok(Some(id))
    }

    pub fn begin_resize(
        &mut self,
        pointer_id: u64,
        page_index: u32,
        start: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        self.require_no_gesture()?;
        let Some(HitTarget::ResizeHandle { id, handle }) =
            self.hit_test(page_index, start, tolerance_pt)?
        else {
            return Ok(None);
        };
        let annotation = self
            .annotation(&id)
            .expect("a hit-tested annotation must exist")
            .clone();
        if annotation.locked {
            return Err(AnnotationError::LockedMarkup(id));
        }
        self.active_gesture = Some(ActiveGesture::Resize {
            pointer_id,
            original: annotation.rect,
            annotation,
            handle,
        });
        Ok(Some(id))
    }

    pub fn begin_rotation(
        &mut self,
        pointer_id: u64,
        page_index: u32,
        start: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        self.begin_rotation_with_rotation_handle_offset(
            pointer_id,
            page_index,
            start,
            tolerance_pt,
            ROTATION_HANDLE_OFFSET_PT,
        )
    }

    fn begin_rotation_with_rotation_handle_offset(
        &mut self,
        pointer_id: u64,
        page_index: u32,
        start: PdfPoint,
        tolerance_pt: f64,
        rotation_handle_offset_pt: f64,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        self.require_no_gesture()?;
        let Some(HitTarget::RotationHandle(id)) = self.hit_test_with_rotation_handle_offset(
            page_index,
            start,
            tolerance_pt,
            rotation_handle_offset_pt,
        )?
        else {
            return Ok(None);
        };
        let annotation = self
            .annotation(&id)
            .expect("a hit-tested annotation must exist")
            .clone();
        if annotation.locked {
            return Err(AnnotationError::LockedMarkup(id));
        }
        let center = annotation.rect.center();
        let start_angle_radians = (start.y - center.y).atan2(start.x - center.x);
        self.active_gesture = Some(ActiveGesture::Rotate {
            pointer_id,
            original_rotation_degrees: annotation.rotation_degrees,
            start_angle_radians,
            annotation,
        });
        Ok(Some(id))
    }

    pub fn begin_east_resize(
        &mut self,
        pointer_id: u64,
        page_index: u32,
        start: PdfPoint,
        tolerance_pt: f64,
    ) -> Result<Option<MarkupId>, AnnotationError> {
        self.require_no_gesture()?;
        if !matches!(
            self.hit_test(page_index, start, tolerance_pt)?,
            Some(HitTarget::ResizeHandle {
                handle: RectangleResizeHandle::East,
                ..
            })
        ) {
            return Ok(None);
        }
        self.begin_resize(pointer_id, page_index, start, tolerance_pt)
    }

    pub fn update_gesture(
        &mut self,
        pointer_id: u64,
        point: PdfPoint,
    ) -> Result<GesturePreview, AnnotationError> {
        let gesture = self
            .active_gesture
            .as_mut()
            .ok_or(AnnotationError::NoActiveGesture)?;
        require_pointer(gesture, pointer_id)?;
        match gesture {
            ActiveGesture::Pen { .. } => {
                return Err(AnnotationError::InvalidGeometry(
                    "pen samples must use AppendPenSamples".into(),
                ));
            }
            ActiveGesture::Create {
                annotation, start, ..
            } => annotation.rect = PdfRect::from_corners(*start, point),
            ActiveGesture::Move {
                annotation,
                original,
                start,
                ..
            } => annotation.rect = original.translated(point.x - start.x, point.y - start.y),
            ActiveGesture::Resize {
                annotation,
                original,
                handle,
                ..
            } => {
                annotation.rect =
                    original.rotated_resize_from_handle(annotation.rotation_degrees, *handle, point)
            }
            ActiveGesture::Rotate {
                annotation,
                original_rotation_degrees,
                start_angle_radians,
                ..
            } => {
                let center = annotation.rect.center();
                let current_angle_radians = (point.y - center.y).atan2(point.x - center.x);
                annotation.rotation_degrees = normalize_degrees(
                    *original_rotation_degrees
                        + (*start_angle_radians - current_angle_radians).to_degrees(),
                );
            }
        }
        Ok(gesture
            .rectangle_preview()
            .expect("rectangle update gestures always have a rectangle preview"))
    }

    pub fn cancel_gesture(&mut self, pointer_id: u64) -> Result<(), AnnotationError> {
        let gesture = self
            .active_gesture
            .as_ref()
            .ok_or(AnnotationError::NoActiveGesture)?;
        require_pointer(gesture, pointer_id)?;
        self.active_gesture = None;
        Ok(())
    }

    pub fn commit_gesture(&mut self, pointer_id: u64) -> Result<CommitOutcome, AnnotationError> {
        let active = self
            .active_gesture
            .as_ref()
            .ok_or(AnnotationError::NoActiveGesture)?;
        require_pointer(active, pointer_id)?;
        let gesture = self
            .active_gesture
            .take()
            .expect("the checked active gesture must remain present");
        match gesture {
            ActiveGesture::Pen { annotation, .. } => {
                if validate_pen_path(&annotation.points).is_err() {
                    return Ok(CommitOutcome::Cancelled);
                }
                let id = annotation.id.clone();
                let order_id = id.clone();
                self.commit_state_change(|state| {
                    state.annotation_order.push(order_id);
                    state.pens.push(annotation);
                });
                self.selected_ids = vec![id.clone()];
                Ok(CommitOutcome::Created(id))
            }
            ActiveGesture::Create { annotation, .. } => {
                if annotation.rect.width <= MIN_RECT_CREATE_SIZE_PT
                    || annotation.rect.height <= MIN_RECT_CREATE_SIZE_PT
                {
                    return Ok(CommitOutcome::Cancelled);
                }
                let id = annotation.id.clone();
                let order_id = id.clone();
                self.commit_state_change(|state| {
                    state.annotation_order.push(order_id);
                    state.rectangles.push(annotation);
                });
                self.selected_ids = vec![id.clone()];
                Ok(CommitOutcome::Created(id))
            }
            ActiveGesture::Move {
                annotation,
                original,
                ..
            }
            | ActiveGesture::Resize {
                annotation,
                original,
                ..
            } => {
                if annotation.rect == original {
                    return Ok(CommitOutcome::Cancelled);
                }
                let id = annotation.id.clone();
                self.replace_annotation(annotation);
                Ok(CommitOutcome::Updated(id))
            }
            ActiveGesture::Rotate {
                annotation,
                original_rotation_degrees,
                ..
            } => {
                if annotation.rotation_degrees == original_rotation_degrees {
                    return Ok(CommitOutcome::Cancelled);
                }
                let id = annotation.id.clone();
                self.replace_annotation(annotation);
                Ok(CommitOutcome::Updated(id))
            }
        }
    }

    pub fn set_selected_appearance(
        &mut self,
        appearance: RectangleAppearance,
    ) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        let id = self
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        if let Some(annotation) = self.annotation(&id) {
            if annotation.locked {
                return Err(AnnotationError::LockedMarkup(id));
            }
            if annotation.appearance == appearance {
                return Ok(false);
            }
            let mut replacement = annotation.clone();
            replacement.appearance = appearance;
            self.replace_annotation(replacement);
            return Ok(true);
        }
        if let Some(annotation) = self.vertex_path(&id) {
            if annotation.locked {
                return Err(AnnotationError::LockedMarkup(id));
            }
            if annotation.appearance == appearance {
                return Ok(false);
            }
            let id = id.clone();
            self.commit_state_change(move |state| {
                state
                    .vertex_paths
                    .iter_mut()
                    .find(|annotation| annotation.id == id)
                    .expect("a selected vertex path must retain its target")
                    .appearance = appearance;
            });
            return Ok(true);
        }
        if let Some(annotation) = self.measurement_path(&id) {
            if annotation.locked {
                return Err(AnnotationError::LockedMarkup(id));
            }
            if annotation.appearance == appearance {
                return Ok(false);
            }
            let id = id.clone();
            self.commit_state_change(move |state| {
                state
                    .measurement_paths
                    .iter_mut()
                    .find(|annotation| annotation.id == id)
                    .expect("a selected measurement path must retain its target")
                    .appearance = appearance;
            });
            return Ok(true);
        }
        if let Some(annotation) = self.arc(&id) {
            if annotation.locked {
                return Err(AnnotationError::LockedMarkup(id));
            }
            if annotation.appearance == appearance {
                return Ok(false);
            }
            let id = id.clone();
            self.commit_state_change(move |state| {
                state
                    .arcs
                    .iter_mut()
                    .find(|annotation| annotation.id == id)
                    .expect("a selected Arc must retain its target")
                    .appearance = appearance;
            });
            return Ok(true);
        }
        let annotation = self.ellipse(&id).ok_or(AnnotationError::NoSelection)?;
        if annotation.locked {
            return Err(AnnotationError::LockedMarkup(id));
        }
        if annotation.appearance == appearance {
            return Ok(false);
        }
        let mut replacement = annotation.clone();
        replacement.appearance = appearance;
        self.replace_ellipse_annotation(replacement);
        Ok(true)
    }

    pub fn set_locked(&mut self, id: &MarkupId, locked: bool) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        let current = self
            .annotation(id)
            .map(|annotation| annotation.locked)
            .or_else(|| self.redact(id).map(|annotation| annotation.locked))
            .or_else(|| self.ellipse(id).map(|annotation| annotation.locked))
            .or_else(|| self.arc(id).map(|annotation| annotation.locked))
            .or_else(|| self.straight_line(id).map(|annotation| annotation.locked))
            .or_else(|| self.vertex_path(id).map(|annotation| annotation.locked))
            .or_else(|| self.cloud(id).map(|annotation| annotation.locked))
            .or_else(|| self.cloud_plus(id).map(|annotation| annotation.locked))
            .or_else(|| self.callout(id).map(|annotation| annotation.locked))
            .or_else(|| self.dimension(id).map(|annotation| annotation.locked))
            .or_else(|| {
                self.measurement_path(id)
                    .map(|annotation| annotation.locked)
            })
            .or_else(|| self.pen(id).map(|annotation| annotation.locked))
            .or_else(|| self.text_box(id).map(|annotation| annotation.locked))
            .or_else(|| self.length(id).map(|annotation| annotation.locked))
            .or_else(|| self.image(id).map(|annotation| annotation.locked))
            .or_else(|| {
                self.snapshot_annotation(id)
                    .map(|annotation| annotation.locked)
            })
            .ok_or(AnnotationError::NoSelection)?;
        if current == locked {
            return Ok(false);
        }
        let id = id.clone();
        self.commit_state_change(|state| {
            if let Some(annotation) = state
                .rectangles
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .redacts
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .ellipses
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) =
                state.arcs.iter_mut().find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .straight_lines
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .vertex_paths
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .clouds
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .cloud_pluses
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .callouts
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .dimensions
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .measurement_paths
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) =
                state.pens.iter_mut().find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .text_boxes
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .lengths
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .images
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            } else if let Some(annotation) = state
                .snapshots
                .iter_mut()
                .find(|annotation| annotation.id == id)
            {
                annotation.locked = locked;
            }
        });
        Ok(true)
    }

    pub fn delete_selected(&mut self) -> Result<MarkupId, AnnotationError> {
        self.require_no_gesture()?;
        let id = self
            .selected_id()
            .cloned()
            .ok_or(AnnotationError::NoSelection)?;
        let locked = self
            .annotation(&id)
            .map(|annotation| annotation.locked)
            .or_else(|| self.redact(&id).map(|annotation| annotation.locked))
            .or_else(|| self.ellipse(&id).map(|annotation| annotation.locked))
            .or_else(|| self.arc(&id).map(|annotation| annotation.locked))
            .or_else(|| self.straight_line(&id).map(|annotation| annotation.locked))
            .or_else(|| self.vertex_path(&id).map(|annotation| annotation.locked))
            .or_else(|| self.cloud(&id).map(|annotation| annotation.locked))
            .or_else(|| self.cloud_plus(&id).map(|annotation| annotation.locked))
            .or_else(|| self.callout(&id).map(|annotation| annotation.locked))
            .or_else(|| self.dimension(&id).map(|annotation| annotation.locked))
            .or_else(|| {
                self.measurement_path(&id)
                    .map(|annotation| annotation.locked)
            })
            .or_else(|| self.pen(&id).map(|annotation| annotation.locked))
            .or_else(|| self.text_box(&id).map(|annotation| annotation.locked))
            .or_else(|| self.length(&id).map(|annotation| annotation.locked))
            .or_else(|| self.image(&id).map(|annotation| annotation.locked))
            .or_else(|| {
                self.snapshot_annotation(&id)
                    .map(|annotation| annotation.locked)
            })
            .ok_or(AnnotationError::NoSelection)?;
        if locked {
            return Err(AnnotationError::LockedMarkup(id));
        }
        self.commit_state_change(|state| {
            state
                .annotation_order
                .retain(|annotation_id| *annotation_id != id);
            state.rectangles.retain(|annotation| annotation.id != id);
            state.redacts.retain(|annotation| annotation.id != id);
            state.ellipses.retain(|annotation| annotation.id != id);
            state.arcs.retain(|annotation| annotation.id != id);
            state
                .straight_lines
                .retain(|annotation| annotation.id != id);
            state.vertex_paths.retain(|annotation| annotation.id != id);
            state.clouds.retain(|annotation| annotation.id != id);
            state.cloud_pluses.retain(|annotation| annotation.id != id);
            state.callouts.retain(|annotation| annotation.id != id);
            state.dimensions.retain(|annotation| annotation.id != id);
            state
                .measurement_paths
                .retain(|annotation| annotation.id != id);
            state.pens.retain(|annotation| annotation.id != id);
            state.text_boxes.retain(|annotation| annotation.id != id);
            state.lengths.retain(|annotation| annotation.id != id);
            state.images.retain(|annotation| annotation.id != id);
            state.snapshots.retain(|annotation| annotation.id != id);
        });
        self.selected_ids.retain(|selected| selected != &id);
        Ok(id)
    }

    pub fn undo(&mut self) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        let Some(previous) = self.past.pop_back() else {
            return Ok(false);
        };
        push_bounded(&mut self.future, self.state.clone(), self.history_limit);
        self.state = previous;
        self.reconcile_selection();
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, AnnotationError> {
        self.require_no_gesture()?;
        let Some(next) = self.future.pop_back() else {
            return Ok(false);
        };
        push_bounded(&mut self.past, self.state.clone(), self.history_limit);
        self.state = next;
        self.reconcile_selection();
        Ok(true)
    }

    pub fn canonical_json_snapshot(&self) -> Value {
        let mut markups = self
            .state
            .rectangles
            .iter()
            .map(canonical_rectangle)
            .collect::<Vec<_>>();
        markups.extend(self.state.ellipses.iter().map(canonical_ellipse));
        markups.extend(self.state.redacts.iter().map(canonical_redact));
        markups.extend(self.state.arcs.iter().map(canonical_arc));
        markups.extend(self.state.pens.iter().map(canonical_pen));
        markups.extend(
            self.state
                .straight_lines
                .iter()
                .map(canonical_straight_line),
        );
        markups.extend(self.state.vertex_paths.iter().map(canonical_vertex_path));
        markups.extend(self.state.clouds.iter().map(canonical_cloud));
        markups.extend(self.state.cloud_pluses.iter().map(canonical_cloud_plus));
        markups.extend(self.state.callouts.iter().map(canonical_callout));
        markups.extend(self.state.dimensions.iter().map(canonical_dimension));
        markups.extend(
            self.state
                .measurement_paths
                .iter()
                .map(canonical_measurement_path),
        );
        markups.extend(self.state.text_boxes.iter().map(canonical_text_box));
        markups.extend(self.state.lengths.iter().map(canonical_length));
        markups.extend(self.state.images.iter().map(canonical_image));
        markups.extend(self.state.snapshots.iter().map(canonical_snapshot));
        let mut snapshot = json!({
            "schema_version": 1,
            "selection": self.selected_id().map(MarkupId::as_str),
            "markups": markups,
        });
        if !self.state.page_length_calibrations.is_empty() {
            snapshot["page_scales"] = Value::Array(
                self.state
                    .page_length_calibrations
                    .iter()
                    .map(|(page_index, calibration)| {
                        json!({
                            "page_index": page_index,
                            "paper_points": calibration.paper_points(),
                            "precision": calibration.precision(),
                            "real_world_value": calibration.real_world_value(),
                            "unit": calibration.unit(),
                            "units_per_point": calibration.units_per_point(),
                        })
                    })
                    .collect(),
            );
        }
        if !self.state.page_rotations.is_empty() {
            snapshot["page_rotations"] = Value::Array(
                self.state
                    .page_rotations
                    .iter()
                    .map(|(page_index, rotation)| {
                        json!({"page_index": page_index, "degrees": rotation.degrees()})
                    })
                    .collect(),
            );
        }
        canonicalize_json(snapshot)
    }

    pub fn canonical_json_string(&self) -> String {
        serde_json::to_string(&self.canonical_json_snapshot())
            .expect("validated annotation values must serialize")
    }

    fn annotation(&self, id: &MarkupId) -> Option<&RectangleAnnotation> {
        self.state
            .rectangles
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn redact(&self, id: &MarkupId) -> Option<&RedactAnnotation> {
        self.state
            .redacts
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn ellipse(&self, id: &MarkupId) -> Option<&EllipseAnnotation> {
        self.state
            .ellipses
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn arc(&self, id: &MarkupId) -> Option<&ArcAnnotation> {
        self.state
            .arcs
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn pen(&self, id: &MarkupId) -> Option<&PenAnnotation> {
        self.state
            .pens
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn straight_line(&self, id: &MarkupId) -> Option<&StraightLineAnnotation> {
        self.state
            .straight_lines
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn vertex_path(&self, id: &MarkupId) -> Option<&VertexPathAnnotation> {
        self.state
            .vertex_paths
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn cloud(&self, id: &MarkupId) -> Option<&CloudAnnotation> {
        self.state
            .clouds
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn cloud_plus(&self, id: &MarkupId) -> Option<&CloudPlusAnnotation> {
        self.state
            .cloud_pluses
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn callout(&self, id: &MarkupId) -> Option<&CalloutAnnotation> {
        self.state
            .callouts
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn dimension(&self, id: &MarkupId) -> Option<&DimensionAnnotation> {
        self.state
            .dimensions
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn measurement_path(&self, id: &MarkupId) -> Option<&MeasurementPathAnnotation> {
        self.state
            .measurement_paths
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn text_box(&self, id: &MarkupId) -> Option<&TextBoxAnnotation> {
        self.state
            .text_boxes
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn length(&self, id: &MarkupId) -> Option<&LengthAnnotation> {
        self.state
            .lengths
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn image(&self, id: &MarkupId) -> Option<&ImageAnnotation> {
        self.state
            .images
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn snapshot_annotation(&self, id: &MarkupId) -> Option<&SnapshotAnnotation> {
        self.state
            .snapshots
            .iter()
            .find(|annotation| annotation.id == *id)
    }

    fn annotation_owned(&self, id: &MarkupId) -> Option<Annotation> {
        self.annotation(id)
            .cloned()
            .map(Annotation::Rectangle)
            .or_else(|| self.redact(id).cloned().map(Annotation::Redact))
            .or_else(|| self.ellipse(id).cloned().map(Annotation::Ellipse))
            .or_else(|| self.arc(id).cloned().map(Annotation::Arc))
            .or_else(|| {
                self.straight_line(id)
                    .cloned()
                    .map(Annotation::StraightLine)
            })
            .or_else(|| self.vertex_path(id).cloned().map(Annotation::VertexPath))
            .or_else(|| self.cloud(id).cloned().map(Annotation::Cloud))
            .or_else(|| self.cloud_plus(id).cloned().map(Annotation::CloudPlus))
            .or_else(|| self.callout(id).cloned().map(Annotation::Callout))
            .or_else(|| self.dimension(id).cloned().map(Annotation::Dimension))
            .or_else(|| {
                self.measurement_path(id)
                    .cloned()
                    .map(Annotation::MeasurementPath)
            })
            .or_else(|| self.pen(id).cloned().map(Annotation::Pen))
            .or_else(|| self.text_box(id).cloned().map(Annotation::TextBox))
            .or_else(|| self.length(id).cloned().map(Annotation::Length))
            .or_else(|| self.image(id).cloned().map(Annotation::Image))
            .or_else(|| {
                self.snapshot_annotation(id)
                    .cloned()
                    .map(Annotation::Snapshot)
            })
    }

    fn annotation_locked(&self, id: &MarkupId) -> Option<bool> {
        self.annotation(id)
            .map(|annotation| annotation.locked)
            .or_else(|| self.redact(id).map(|annotation| annotation.locked))
            .or_else(|| self.ellipse(id).map(|annotation| annotation.locked))
            .or_else(|| self.arc(id).map(|annotation| annotation.locked))
            .or_else(|| self.straight_line(id).map(|annotation| annotation.locked))
            .or_else(|| self.vertex_path(id).map(|annotation| annotation.locked))
            .or_else(|| self.cloud(id).map(|annotation| annotation.locked))
            .or_else(|| self.cloud_plus(id).map(|annotation| annotation.locked))
            .or_else(|| self.callout(id).map(|annotation| annotation.locked))
            .or_else(|| self.dimension(id).map(|annotation| annotation.locked))
            .or_else(|| {
                self.measurement_path(id)
                    .map(|annotation| annotation.locked)
            })
            .or_else(|| self.pen(id).map(|annotation| annotation.locked))
            .or_else(|| self.text_box(id).map(|annotation| annotation.locked))
            .or_else(|| self.length(id).map(|annotation| annotation.locked))
            .or_else(|| self.image(id).map(|annotation| annotation.locked))
            .or_else(|| {
                self.snapshot_annotation(id)
                    .map(|annotation| annotation.locked)
            })
    }

    fn annotation_page(&self, id: &MarkupId) -> Option<u32> {
        self.annotation(id)
            .map(|annotation| annotation.page_index)
            .or_else(|| self.redact(id).map(|annotation| annotation.page_index))
            .or_else(|| self.ellipse(id).map(|annotation| annotation.page_index))
            .or_else(|| self.arc(id).map(|annotation| annotation.page_index))
            .or_else(|| {
                self.straight_line(id)
                    .map(|annotation| annotation.page_index)
            })
            .or_else(|| self.vertex_path(id).map(|annotation| annotation.page_index))
            .or_else(|| self.cloud(id).map(|annotation| annotation.page_index))
            .or_else(|| self.cloud_plus(id).map(|annotation| annotation.page_index))
            .or_else(|| self.callout(id).map(|annotation| annotation.page_index))
            .or_else(|| self.dimension(id).map(|annotation| annotation.page_index))
            .or_else(|| {
                self.measurement_path(id)
                    .map(|annotation| annotation.page_index)
            })
            .or_else(|| self.pen(id).map(|annotation| annotation.page_index))
            .or_else(|| self.text_box(id).map(|annotation| annotation.page_index))
            .or_else(|| self.length(id).map(|annotation| annotation.page_index))
            .or_else(|| self.image(id).map(|annotation| annotation.page_index))
            .or_else(|| {
                self.snapshot_annotation(id)
                    .map(|annotation| annotation.page_index)
            })
    }

    fn contains_annotation(&self, id: &MarkupId) -> bool {
        self.annotation(id).is_some()
            || self.redact(id).is_some()
            || self.ellipse(id).is_some()
            || self.arc(id).is_some()
            || self.straight_line(id).is_some()
            || self.vertex_path(id).is_some()
            || self.cloud(id).is_some()
            || self.cloud_plus(id).is_some()
            || self.callout(id).is_some()
            || self.dimension(id).is_some()
            || self.measurement_path(id).is_some()
            || self.pen(id).is_some()
            || self.text_box(id).is_some()
            || self.length(id).is_some()
            || self.image(id).is_some()
            || self.snapshot_annotation(id).is_some()
    }

    fn create_annotation(&mut self, annotation: Annotation) -> Result<(), AnnotationError> {
        self.require_no_gesture()?;
        let id = annotation.id().clone();
        if self.contains_annotation(&id) {
            return Err(AnnotationError::DuplicateMarkupId(id));
        }
        let order_id = id.clone();
        let page_index = annotation.page_index();
        let insertion_index = self
            .state
            .annotation_order
            .iter()
            .position(|candidate| {
                self.annotation_page(candidate)
                    .is_some_and(|candidate_page| candidate_page > page_index)
            })
            .unwrap_or(self.state.annotation_order.len());
        self.commit_state_change(move |state| {
            state.annotation_order.insert(insertion_index, order_id);
            match annotation {
                Annotation::Rectangle(annotation) => state.rectangles.push(annotation),
                Annotation::Redact(annotation) => state.redacts.push(annotation),
                Annotation::Ellipse(annotation) => state.ellipses.push(annotation),
                Annotation::Arc(annotation) => state.arcs.push(annotation),
                Annotation::StraightLine(annotation) => state.straight_lines.push(annotation),
                Annotation::VertexPath(annotation) => state.vertex_paths.push(annotation),
                Annotation::Cloud(annotation) => state.clouds.push(annotation),
                Annotation::CloudPlus(annotation) => state.cloud_pluses.push(annotation),
                Annotation::Callout(annotation) => state.callouts.push(annotation),
                Annotation::Dimension(annotation) => state.dimensions.push(annotation),
                Annotation::MeasurementPath(annotation) => state.measurement_paths.push(annotation),
                Annotation::Pen(annotation) => state.pens.push(annotation),
                Annotation::TextBox(annotation) => state.text_boxes.push(annotation),
                Annotation::Length(annotation) => state.lengths.push(annotation),
                Annotation::Image(annotation) => state.images.push(annotation),
                Annotation::Snapshot(annotation) => state.snapshots.push(annotation),
            }
        });
        self.selected_ids = vec![id];
        Ok(())
    }

    fn edit_annotation(
        &mut self,
        id: &MarkupId,
        edit: AnnotationEdit,
    ) -> Result<(AnnotationKind, bool), AnnotationError> {
        self.require_no_gesture()?;
        match edit {
            AnnotationEdit::SetRectangleRect(rect) => {
                let annotation = self.annotation(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.rect == rect {
                    return Ok((AnnotationKind::Rectangle, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .rectangles
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Rectangle edit must retain its target")
                        .rect = rect;
                });
                Ok((AnnotationKind::Rectangle, true))
            }
            AnnotationEdit::SetRectangleRotation(rotation_degrees) => {
                require_finite("rectangle.rotation", rotation_degrees)?;
                let rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
                let annotation = self.annotation(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.rotation_degrees == rotation_degrees {
                    return Ok((AnnotationKind::Rectangle, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .rectangles
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Rectangle rotation must retain its target")
                        .rotation_degrees = rotation_degrees;
                });
                Ok((AnnotationKind::Rectangle, true))
            }
            AnnotationEdit::SetRedactRect(rect) => {
                let annotation = self.redact(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if rect.width <= MIN_RECT_CREATE_SIZE_PT || rect.height <= MIN_RECT_CREATE_SIZE_PT {
                    return Err(AnnotationError::InvalidGeometry(
                        "redaction dimensions must be strictly greater than two points".into(),
                    ));
                }
                if annotation.rect == rect {
                    return Ok((AnnotationKind::Redact, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .redacts
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated pending Redact edit must retain its target")
                        .rect = rect;
                });
                Ok((AnnotationKind::Redact, true))
            }
            AnnotationEdit::TranslateRedact { delta_x, delta_y } => {
                require_finite("redact.delta_x", delta_x)?;
                require_finite("redact.delta_y", delta_y)?;
                let annotation = self.redact(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Redact, false));
                }
                let rect = PdfRect::new(
                    annotation.rect.x + delta_x,
                    annotation.rect.y + delta_y,
                    annotation.rect.width,
                    annotation.rect.height,
                )?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .redacts
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated pending Redact move must retain its target")
                        .rect = rect;
                });
                Ok((AnnotationKind::Redact, true))
            }
            AnnotationEdit::SetEllipseRect(rect) => {
                let annotation = self.ellipse(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                // Placement keeps its minimum gesture threshold, but the
                // selected-property contract permits a zero width or height.
                // `PdfRect` has already rejected negative or non-finite input.
                if annotation.rect == rect {
                    return Ok((AnnotationKind::Ellipse, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .ellipses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Ellipse edit must retain its target")
                        .rect = rect;
                });
                Ok((AnnotationKind::Ellipse, true))
            }
            AnnotationEdit::TranslateEllipse { delta_x, delta_y } => {
                require_finite("ellipse.delta_x", delta_x)?;
                require_finite("ellipse.delta_y", delta_y)?;
                let annotation = self.ellipse(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Ellipse, false));
                }
                let rect = PdfRect::new(
                    annotation.rect.x + delta_x,
                    annotation.rect.y + delta_y,
                    annotation.rect.width,
                    annotation.rect.height,
                )?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .ellipses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Ellipse move must retain its target")
                        .rect = rect;
                });
                Ok((AnnotationKind::Ellipse, true))
            }
            AnnotationEdit::SetEllipseRotation(rotation_degrees) => {
                require_finite("ellipse.rotation", rotation_degrees)?;
                let rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
                let annotation = self.ellipse(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.rotation_degrees == rotation_degrees {
                    return Ok((AnnotationKind::Ellipse, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .ellipses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Ellipse rotation must retain its target")
                        .rotation_degrees = rotation_degrees;
                });
                Ok((AnnotationKind::Ellipse, true))
            }
            AnnotationEdit::ReplacePenPath(points) => {
                validate_pen_path(&points)?;
                let annotation = self.pen(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.points == points {
                    return Ok((AnnotationKind::Pen, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .pens
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated pen edit must retain its target")
                        .points = points;
                });
                Ok((AnnotationKind::Pen, true))
            }
            AnnotationEdit::ReplacePenPaths(mut paths) => {
                if paths.is_empty() {
                    return Err(AnnotationError::InvalidGeometry(
                        "ink must contain at least one path".into(),
                    ));
                }
                for path in &paths {
                    validate_pen_path(path)?;
                }
                let annotation = self.pen(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.paths().eq(paths.iter().map(Vec::as_slice)) {
                    return Ok((AnnotationKind::Pen, false));
                }
                let points = paths.remove(0);
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .pens
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated ink edit must retain its target");
                    annotation.points = points;
                    annotation.additional_paths = paths;
                });
                Ok((AnnotationKind::Pen, true))
            }
            AnnotationEdit::SetInkAppearance(appearance) => {
                let annotation = self.pen(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance {
                    return Ok((AnnotationKind::Pen, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .pens
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated ink edit must retain its target")
                        .appearance = appearance;
                });
                Ok((AnnotationKind::Pen, true))
            }
            AnnotationEdit::SetTextBoxContent(content) => {
                validate_text(&content, "text box content", MAX_TEXT_BOX_BYTES)?;
                let annotation = self.text_box(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.content == content {
                    return Ok((AnnotationKind::TextBox, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .text_boxes
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated text box edit must retain its target")
                        .content = content;
                });
                Ok((AnnotationKind::TextBox, true))
            }
            AnnotationEdit::SetTextBoxLayoutRect(layout_rect) => {
                validate_layout_rect(layout_rect, "text box")?;
                let annotation = self.text_box(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.layout_rect == layout_rect {
                    return Ok((AnnotationKind::TextBox, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .text_boxes
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated text layout edit must retain its target")
                        .layout_rect = layout_rect;
                });
                Ok((AnnotationKind::TextBox, true))
            }
            AnnotationEdit::SetArcControlPoint {
                control,
                point,
                snap_quarter_turn: _,
            } => {
                require_finite("arc.control.x", point.x)?;
                require_finite("arc.control.y", point.y)?;
                let annotation = self.arc(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                let (start, end, mid) = match control {
                    ArcControlPoint::Start => (point, annotation.end, annotation.mid),
                    ArcControlPoint::Mid => (annotation.start, annotation.end, point),
                    ArcControlPoint::End => (annotation.start, point, annotation.mid),
                };
                if (start, end, mid) == (annotation.start, annotation.end, annotation.mid) {
                    return Ok((AnnotationKind::Arc, false));
                }
                let mut replacement = ArcAnnotation::new(
                    annotation.id.clone(),
                    annotation.page_index,
                    start,
                    end,
                    mid,
                    annotation.appearance.clone(),
                )?;
                replacement.locked = annotation.locked;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .arcs
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Arc control-point edit must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::Arc, true))
            }
            AnnotationEdit::TranslateArc { delta_x, delta_y } => {
                require_finite("arc.delta_x", delta_x)?;
                require_finite("arc.delta_y", delta_y)?;
                let annotation = self.arc(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Arc, false));
                }
                let replacement = Annotation::Arc(annotation.clone()).translated_copy(
                    id.clone(),
                    annotation.page_index,
                    delta_x,
                    delta_y,
                )?;
                let Annotation::Arc(replacement) = replacement else {
                    unreachable!("Arc translation returns Arc")
                };
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .arcs
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Arc move must retain its target") = replacement;
                });
                Ok((AnnotationKind::Arc, true))
            }
            AnnotationEdit::SetDimensionEndpoint { endpoint, point } => {
                require_finite("dimension.endpoint.x", point.x)?;
                require_finite("dimension.endpoint.y", point.y)?;
                let annotation = self.dimension(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                let (start, end) = match endpoint {
                    LineEndpoint::Start => (point, annotation.end),
                    LineEndpoint::End => (annotation.start, point),
                };
                if point_distance(start, end) <= MIN_STRAIGHT_LINE_LENGTH_PT {
                    return Err(AnnotationError::InvalidGeometry(
                        "dimension endpoints must be more than two points apart".into(),
                    ));
                }
                if annotation.start == start && annotation.end == end {
                    return Ok((AnnotationKind::Dimension, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .dimensions
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated dimension endpoint edit must retain its target");
                    annotation.start = start;
                    annotation.end = end;
                });
                Ok((AnnotationKind::Dimension, true))
            }
            AnnotationEdit::SetDimensionOffset(offset) => {
                require_finite("dimension.line_offset", offset)?;
                let annotation = self.dimension(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                let offset = canonical_float(offset);
                if annotation.dimension_line_offset == offset {
                    return Ok((AnnotationKind::Dimension, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .dimensions
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated dimension offset edit must retain its target")
                        .dimension_line_offset = offset;
                });
                Ok((AnnotationKind::Dimension, true))
            }
            AnnotationEdit::SetDimensionContent(content) => {
                validate_text(&content, "dimension content", MAX_TEXT_BOX_BYTES)?;
                let annotation = self.dimension(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.content == content {
                    return Ok((AnnotationKind::Dimension, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .dimensions
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated dimension content edit must retain its target")
                        .content = content;
                });
                Ok((AnnotationKind::Dimension, true))
            }
            AnnotationEdit::SetDimensionAppearance(appearance) => {
                let annotation = self.dimension(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance {
                    return Ok((AnnotationKind::Dimension, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .dimensions
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated dimension appearance edit must retain its target")
                        .appearance = appearance;
                });
                Ok((AnnotationKind::Dimension, true))
            }
            AnnotationEdit::TranslateDimension { delta_x, delta_y } => {
                require_finite("dimension.delta_x", delta_x)?;
                require_finite("dimension.delta_y", delta_y)?;
                let annotation = self.dimension(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Dimension, false));
                }
                let start =
                    PdfPoint::new(annotation.start.x + delta_x, annotation.start.y + delta_y)?;
                let end = PdfPoint::new(annotation.end.x + delta_x, annotation.end.y + delta_y)?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .dimensions
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated dimension move must retain its target");
                    annotation.start = start;
                    annotation.end = end;
                });
                Ok((AnnotationKind::Dimension, true))
            }
            AnnotationEdit::SetLengthCalibration(calibration) => {
                let annotation = self.length(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.calibration == calibration {
                    return Ok((AnnotationKind::Length, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .lengths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated length edit must retain its target")
                        .calibration = calibration;
                });
                Ok((AnnotationKind::Length, true))
            }
            AnnotationEdit::SetLengthEndpoint { endpoint, point } => {
                require_finite("length.endpoint.x", point.x)?;
                require_finite("length.endpoint.y", point.y)?;
                let annotation = self.length(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                let (start, end) = match endpoint {
                    LengthEndpoint::Start => (point, annotation.end),
                    LengthEndpoint::End => (annotation.start, point),
                };
                if start == end {
                    return Err(AnnotationError::InvalidGeometry(
                        "length endpoints must be distinct".into(),
                    ));
                }
                if annotation.start == start && annotation.end == end {
                    return Ok((AnnotationKind::Length, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .lengths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated endpoint edit must retain its target");
                    annotation.start = start;
                    annotation.end = end;
                });
                Ok((AnnotationKind::Length, true))
            }
            AnnotationEdit::TranslateLength { delta_x, delta_y } => {
                require_finite("length.delta_x", delta_x)?;
                require_finite("length.delta_y", delta_y)?;
                let annotation = self.length(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Length, false));
                }
                let start =
                    PdfPoint::new(annotation.start.x + delta_x, annotation.start.y + delta_y)?;
                let end = PdfPoint::new(annotation.end.x + delta_x, annotation.end.y + delta_y)?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .lengths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated length translation must retain its target");
                    annotation.start = start;
                    annotation.end = end;
                });
                Ok((AnnotationKind::Length, true))
            }
            AnnotationEdit::SetStraightLineEndpoint { endpoint, point } => {
                require_finite("straight_line.endpoint.x", point.x)?;
                require_finite("straight_line.endpoint.y", point.y)?;
                let annotation = self.straight_line(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                let (start, end) = match endpoint {
                    LineEndpoint::Start => (point, annotation.end),
                    LineEndpoint::End => (annotation.start, point),
                };
                if point_distance(start, end) <= MIN_STRAIGHT_LINE_LENGTH_PT {
                    return Err(AnnotationError::InvalidGeometry(
                        "straight-line endpoints must be more than two points apart".into(),
                    ));
                }
                if annotation.start == start && annotation.end == end {
                    return Ok((line_annotation_kind(annotation.kind), false));
                }
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .straight_lines
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated straight-line edit must retain its target");
                    annotation.start = start;
                    annotation.end = end;
                });
                Ok((line_annotation_kind(kind), true))
            }
            AnnotationEdit::TranslateStraightLine { delta_x, delta_y } => {
                require_finite("straight_line.delta_x", delta_x)?;
                require_finite("straight_line.delta_y", delta_y)?;
                let annotation = self.straight_line(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0.0 && delta_y == 0.0 {
                    return Ok((line_annotation_kind(annotation.kind), false));
                }
                let start =
                    PdfPoint::new(annotation.start.x + delta_x, annotation.start.y + delta_y)?;
                let end = PdfPoint::new(annotation.end.x + delta_x, annotation.end.y + delta_y)?;
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .straight_lines
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated straight-line translation must retain its target");
                    annotation.start = start;
                    annotation.end = end;
                });
                Ok((line_annotation_kind(kind), true))
            }
            AnnotationEdit::SetStraightLineAppearance(appearance) => {
                let annotation = self.straight_line(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance {
                    return Ok((line_annotation_kind(annotation.kind), false));
                }
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .straight_lines
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated straight-line appearance edit must retain its target")
                        .appearance = appearance;
                });
                Ok((line_annotation_kind(kind), true))
            }
            AnnotationEdit::SetVertexPathPoint {
                vertex_index,
                point,
            } => {
                require_finite("vertex_path.point.x", point.x)?;
                require_finite("vertex_path.point.y", point.y)?;
                let annotation = self.vertex_path(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if vertex_index >= annotation.points.len() {
                    return Err(AnnotationError::InvalidGeometry(
                        "vertex-path point index is out of range".into(),
                    ));
                }
                if annotation.points[vertex_index] == point {
                    return Ok((annotation.kind.into(), false));
                }
                let mut points = annotation.points.clone();
                points[vertex_index] = point;
                validate_vertex_path(&points, annotation.kind)?;
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .vertex_paths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated vertex-path edit must retain its target")
                        .points = points;
                });
                Ok((kind.into(), true))
            }
            AnnotationEdit::TranslateVertexPath { delta_x, delta_y } => {
                require_finite("vertex_path.delta_x", delta_x)?;
                require_finite("vertex_path.delta_y", delta_y)?;
                let annotation = self.vertex_path(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((annotation.kind.into(), false));
                }
                let points = annotation
                    .points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .vertex_paths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated vertex-path move must retain its target")
                        .points = points;
                });
                Ok((kind.into(), true))
            }
            AnnotationEdit::SetCloudPoint {
                vertex_index,
                point,
            } => {
                require_finite("cloud.point.x", point.x)?;
                require_finite("cloud.point.y", point.y)?;
                let annotation = self.cloud(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if vertex_index >= annotation.points.len() {
                    return Err(AnnotationError::InvalidGeometry(
                        "cloud point index is out of range".into(),
                    ));
                }
                if annotation.points[vertex_index] == point {
                    return Ok((AnnotationKind::Cloud, false));
                }
                let mut points = annotation.points.clone();
                points[vertex_index] = point;
                validate_vertex_path(&points, VertexPathKind::Polygon)?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .clouds
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated cloud edit must retain its target")
                        .points = points;
                });
                Ok((AnnotationKind::Cloud, true))
            }
            AnnotationEdit::TranslateCloud { delta_x, delta_y } => {
                require_finite("cloud.delta_x", delta_x)?;
                require_finite("cloud.delta_y", delta_y)?;
                let annotation = self.cloud(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Cloud, false));
                }
                let points = annotation
                    .points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .clouds
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated cloud move must retain its target")
                        .points = points;
                });
                Ok((AnnotationKind::Cloud, true))
            }
            AnnotationEdit::SetCloudPlusCloudPoint {
                vertex_index,
                point,
                leader_points,
            } => {
                require_finite("cloud_plus.point.x", point.x)?;
                require_finite("cloud_plus.point.y", point.y)?;
                let annotation = self.cloud_plus(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if vertex_index >= annotation.cloud_points.len() {
                    return Err(AnnotationError::InvalidGeometry(
                        "Cloud+ point index is out of range".into(),
                    ));
                }
                if annotation.cloud_points[vertex_index] == point
                    && annotation.leader_points == leader_points
                {
                    return Ok((AnnotationKind::CloudPlus, false));
                }
                let mut cloud_points = annotation.cloud_points.clone();
                cloud_points[vertex_index] = point;
                let mut replacement = CloudPlusAnnotation::new(
                    annotation.id.clone(),
                    annotation.page_index,
                    cloud_points,
                    annotation.border_effect_intensity,
                    leader_points,
                    annotation.text_box,
                    annotation.content.clone(),
                    annotation.appearance.clone(),
                )?;
                replacement.locked = annotation.locked;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .cloud_pluses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud+ cloud edit must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::CloudPlus, true))
            }
            AnnotationEdit::SetCloudPlusLeaderPoints(leader_points) => {
                validate_cloud_plus_leader_points(&leader_points)?;
                let annotation = self.cloud_plus(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.leader_points == leader_points {
                    return Ok((AnnotationKind::CloudPlus, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .cloud_pluses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud+ leader edit must retain its target")
                        .leader_points = leader_points;
                });
                Ok((AnnotationKind::CloudPlus, true))
            }
            AnnotationEdit::SetCloudPlusTextBox {
                text_box,
                leader_points,
            } => {
                validate_layout_rect(text_box, "Cloud+ text box")?;
                validate_cloud_plus_leader_points(&leader_points)?;
                let annotation = self.cloud_plus(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.text_box == text_box && annotation.leader_points == leader_points {
                    return Ok((AnnotationKind::CloudPlus, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .cloud_pluses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud+ text-box edit must retain its target");
                    annotation.text_box = text_box;
                    annotation.leader_points = leader_points;
                });
                Ok((AnnotationKind::CloudPlus, true))
            }
            AnnotationEdit::SetCloudPlusContentAndLayout {
                content,
                text_box,
                leader_points,
            } => {
                validate_text(&content, "Cloud+ content", MAX_TEXT_BOX_BYTES)?;
                validate_layout_rect(text_box, "Cloud+ text box")?;
                validate_cloud_plus_leader_points(&leader_points)?;
                let annotation = self.cloud_plus(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.content == content
                    && annotation.text_box == text_box
                    && annotation.leader_points == leader_points
                {
                    return Ok((AnnotationKind::CloudPlus, false));
                }
                let mut replacement = CloudPlusAnnotation::new(
                    annotation.id.clone(),
                    annotation.page_index,
                    annotation.cloud_points.clone(),
                    annotation.border_effect_intensity,
                    leader_points,
                    text_box,
                    content,
                    annotation.appearance.clone(),
                )?;
                replacement.locked = annotation.locked;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .cloud_pluses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud+ content/layout edit must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::CloudPlus, true))
            }
            AnnotationEdit::SetCloudPlusContent(content) => {
                validate_text(&content, "Cloud+ content", MAX_TEXT_BOX_BYTES)?;
                let annotation = self.cloud_plus(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.content == content {
                    return Ok((AnnotationKind::CloudPlus, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .cloud_pluses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud+ text edit must retain its target")
                        .content = content;
                });
                Ok((AnnotationKind::CloudPlus, true))
            }
            AnnotationEdit::SetCloudPlusAppearance(appearance) => {
                let annotation = self.cloud_plus(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance {
                    return Ok((AnnotationKind::CloudPlus, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .cloud_pluses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud+ appearance edit must retain its target")
                        .appearance = appearance;
                });
                Ok((AnnotationKind::CloudPlus, true))
            }
            AnnotationEdit::TranslateCloudPlusGroup { delta_x, delta_y } => {
                require_finite("cloud_plus.delta_x", delta_x)?;
                require_finite("cloud_plus.delta_y", delta_y)?;
                let annotation = self.cloud_plus(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::CloudPlus, false));
                }
                let replacement = Annotation::CloudPlus(annotation.clone()).translated_copy(
                    id.clone(),
                    annotation.page_index,
                    delta_x,
                    delta_y,
                )?;
                let Annotation::CloudPlus(replacement) = replacement else {
                    unreachable!("Cloud+ translation returns Cloud+")
                };
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .cloud_pluses
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud+ group move must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::CloudPlus, true))
            }
            AnnotationEdit::SetCalloutContent(content) => {
                validate_text(&content, "callout content", MAX_TEXT_BOX_BYTES)?;
                let annotation = self.callout(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.content == content {
                    return Ok((AnnotationKind::Callout, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .callouts
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Callout text edit must retain its target")
                        .content = content;
                });
                Ok((AnnotationKind::Callout, true))
            }
            AnnotationEdit::SetCalloutLeaderPoint { point_index, point } => {
                require_finite("callout.point.x", point.x)?;
                require_finite("callout.point.y", point.y)?;
                let annotation = self.callout(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if point_index >= annotation.leader_points.len() {
                    return Err(AnnotationError::InvalidGeometry(
                        "callout leader point index is out of range".into(),
                    ));
                }
                if annotation.leader_points[point_index] == point {
                    return Ok((AnnotationKind::Callout, false));
                }
                let mut replacement = annotation.clone();
                replacement.leader_points[point_index] = point;
                CalloutAnnotation::new(
                    replacement.id.clone(),
                    replacement.page_index,
                    replacement.leader_points.clone(),
                    replacement.text_box,
                    replacement.content.clone(),
                    replacement.appearance.clone(),
                )?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .callouts
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated callout leader edit must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::Callout, true))
            }
            AnnotationEdit::TranslateCalloutTextBox { delta_x, delta_y } => {
                require_finite("callout.text_box.delta_x", delta_x)?;
                require_finite("callout.text_box.delta_y", delta_y)?;
                let annotation = self.callout(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Callout, false));
                }
                let mut replacement = annotation.clone();
                replacement.text_box = PdfRect::new(
                    replacement.text_box.x + delta_x,
                    replacement.text_box.y + delta_y,
                    replacement.text_box.width,
                    replacement.text_box.height,
                )?;
                let connection = replacement
                    .leader_points
                    .last_mut()
                    .expect("validated callout has a connection");
                *connection = PdfPoint::new(connection.x + delta_x, connection.y + delta_y)?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .callouts
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated callout text move must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::Callout, true))
            }
            AnnotationEdit::TranslateCalloutGroup { delta_x, delta_y } => {
                require_finite("callout.group.delta_x", delta_x)?;
                require_finite("callout.group.delta_y", delta_y)?;
                let annotation = self.callout(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((AnnotationKind::Callout, false));
                }
                let mut replacement = annotation.clone();
                replacement.text_box = PdfRect::new(
                    replacement.text_box.x + delta_x,
                    replacement.text_box.y + delta_y,
                    replacement.text_box.width,
                    replacement.text_box.height,
                )?;
                replacement.leader_points = replacement
                    .leader_points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .callouts
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated callout group move must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::Callout, true))
            }
            AnnotationEdit::SetMeasurementPathPoint {
                vertex_index,
                point,
            } => {
                require_finite("measurement_path.point.x", point.x)?;
                require_finite("measurement_path.point.y", point.y)?;
                let annotation = self
                    .measurement_path(id)
                    .ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if vertex_index >= annotation.points.len() {
                    return Err(AnnotationError::InvalidGeometry(
                        "measurement-path point index is out of range".into(),
                    ));
                }
                if annotation.points[vertex_index] == point {
                    return Ok((annotation.kind.into(), false));
                }
                let mut points = annotation.points.clone();
                points[vertex_index] = point;
                validate_measurement_path(&points, annotation.kind)?;
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .measurement_paths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated measurement-path edit must retain its target")
                        .points = points;
                });
                Ok((kind.into(), true))
            }
            AnnotationEdit::TranslateMeasurementPath { delta_x, delta_y } => {
                require_finite("measurement_path.delta_x", delta_x)?;
                require_finite("measurement_path.delta_y", delta_y)?;
                let annotation = self
                    .measurement_path(id)
                    .ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if delta_x == 0. && delta_y == 0. {
                    return Ok((annotation.kind.into(), false));
                }
                let points = annotation
                    .points
                    .iter()
                    .map(|point| PdfPoint::new(point.x + delta_x, point.y + delta_y))
                    .collect::<Result<Vec<_>, _>>()?;
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .measurement_paths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated measurement-path move must retain its target")
                        .points = points;
                });
                Ok((kind.into(), true))
            }
            AnnotationEdit::SetImageRect(rect) => {
                validate_layout_rect(rect, "image")?;
                let annotation = self.image(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                validate_image_aspect(rect, &annotation.asset, annotation.aspect_locked)?;
                if annotation.rect == rect {
                    return Ok((AnnotationKind::Image, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .images
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated image edit must retain its target")
                        .rect = rect;
                });
                Ok((AnnotationKind::Image, true))
            }
            AnnotationEdit::SetSnapshotRect(rect) => {
                validate_snapshot_rect(rect)?;
                let annotation = self
                    .snapshot_annotation(id)
                    .ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.rect == rect {
                    return Ok((AnnotationKind::Snapshot, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .snapshots
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Snapshot rect edit must retain its target")
                        .rect = rect;
                });
                Ok((AnnotationKind::Snapshot, true))
            }
            AnnotationEdit::SetSnapshotRotation(rotation_degrees) => {
                require_finite("snapshot.rotation", rotation_degrees)?;
                let rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
                let annotation = self
                    .snapshot_annotation(id)
                    .ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.rotation_degrees == rotation_degrees {
                    return Ok((AnnotationKind::Snapshot, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .snapshots
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Snapshot rotation edit must retain its target")
                        .rotation_degrees = rotation_degrees;
                });
                Ok((AnnotationKind::Snapshot, true))
            }
            AnnotationEdit::SetSnapshotOpacity(opacity) => {
                validate_snapshot_opacity(opacity)?;
                let opacity = canonical_float(opacity);
                let annotation = self
                    .snapshot_annotation(id)
                    .ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.opacity == opacity {
                    return Ok((AnnotationKind::Snapshot, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .snapshots
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Snapshot opacity edit must retain its target")
                        .opacity = opacity;
                });
                Ok((AnnotationKind::Snapshot, true))
            }
        }
    }

    fn scene_pens(&self, page_index: u32, editor_state: bool) -> Vec<ScenePen> {
        let mut pens = self
            .state
            .pens
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| ScenePen {
                id: annotation.id.clone(),
                points: annotation.points.clone(),
                paths: annotation.paths().map(|path| path.to_vec()).collect(),
                appearance: annotation.appearance.clone(),
                tool: annotation.tool,
                blend_mode: annotation.blend_mode,
                smooth_curves: annotation.smooth_curves,
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect::<Vec<_>>();
        if editor_state
            && let Some(ActiveGesture::Pen { annotation, .. }) = &self.active_gesture
            && annotation.page_index == page_index
        {
            pens.push(ScenePen {
                id: annotation.id.clone(),
                points: annotation.points.clone(),
                paths: annotation.paths().map(|path| path.to_vec()).collect(),
                appearance: annotation.appearance.clone(),
                tool: annotation.tool,
                blend_mode: annotation.blend_mode,
                smooth_curves: annotation.smooth_curves,
                selected: true,
                locked: false,
                draft: true,
            });
        }
        pens
    }

    fn scene_straight_lines(&self, page_index: u32, editor_state: bool) -> Vec<SceneStraightLine> {
        self.state
            .straight_lines
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneStraightLine {
                id: annotation.id.clone(),
                start: annotation.start,
                end: annotation.end,
                kind: annotation.kind,
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_vertex_paths(&self, page_index: u32, editor_state: bool) -> Vec<SceneVertexPath> {
        self.state
            .vertex_paths
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneVertexPath {
                id: annotation.id.clone(),
                points: annotation.points.clone(),
                kind: annotation.kind,
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_clouds(&self, page_index: u32, editor_state: bool) -> Vec<SceneCloud> {
        self.state
            .clouds
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneCloud {
                id: annotation.id.clone(),
                points: annotation.points.clone(),
                scallop_path: annotation.scallop_path(),
                border_effect_intensity: annotation.border_effect_intensity,
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_cloud_pluses(&self, page_index: u32, editor_state: bool) -> Vec<SceneCloudPlus> {
        self.state
            .cloud_pluses
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneCloudPlus {
                id: annotation.id.clone(),
                cloud_points: annotation.cloud_points.clone(),
                scallop_path: annotation.scallop_path(),
                border_effect_intensity: annotation.border_effect_intensity,
                leader_points: annotation.leader_points.clone(),
                text_box: annotation.text_box,
                content: annotation.content.clone(),
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_callouts(&self, page_index: u32, editor_state: bool) -> Vec<SceneCallout> {
        self.state
            .callouts
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneCallout {
                id: annotation.id.clone(),
                leader_points: annotation.leader_points.clone(),
                text_box: annotation.text_box,
                content: annotation.content.clone(),
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_dimensions(&self, page_index: u32, editor_state: bool) -> Vec<SceneDimension> {
        self.state
            .dimensions
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneDimension {
                id: annotation.id.clone(),
                start: annotation.start,
                end: annotation.end,
                dimension_line_offset: annotation.dimension_line_offset,
                content: annotation.content.clone(),
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_arcs(&self, page_index: u32, editor_state: bool) -> Vec<SceneArc> {
        self.state
            .arcs
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneArc {
                id: annotation.id.clone(),
                start: annotation.start,
                end: annotation.end,
                mid: annotation.mid,
                sampled_path: annotation.sampled_path(64),
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_measurement_paths(
        &self,
        page_index: u32,
        editor_state: bool,
    ) -> Vec<SceneMeasurementPath> {
        self.state
            .measurement_paths
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneMeasurementPath {
                id: annotation.id.clone(),
                points: annotation.points.clone(),
                kind: annotation.kind,
                appearance: annotation.appearance.clone(),
                caption: annotation.caption(),
                show_caption: annotation.calibration.show_caption(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_text_boxes(&self, page_index: u32, editor_state: bool) -> Vec<SceneTextBox> {
        self.state
            .text_boxes
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneTextBox {
                id: annotation.id.clone(),
                layout_rect: annotation.layout_rect,
                content: annotation.content.clone(),
                style: annotation.style.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
            })
            .collect()
    }

    fn scene_lengths(&self, page_index: u32, editor_state: bool) -> Vec<SceneLength> {
        self.state
            .lengths
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneLength {
                id: annotation.id.clone(),
                start: annotation.start,
                end: annotation.end,
                caption: annotation.caption(),
                show_caption: annotation.calibration.show_caption,
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
            })
            .collect()
    }

    fn scene_images(&self, page_index: u32, editor_state: bool) -> Vec<SceneImage> {
        self.state
            .images
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneImage {
                id: annotation.id.clone(),
                rect: annotation.rect,
                asset_id: annotation.asset.id.clone(),
                width_px: annotation.asset.width_px,
                height_px: annotation.asset.height_px,
                aspect_locked: annotation.aspect_locked,
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
            })
            .collect()
    }

    fn scene_snapshots(&self, page_index: u32, editor_state: bool) -> Vec<SceneSnapshot> {
        self.state
            .snapshots
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneSnapshot {
                id: annotation.id.clone(),
                body_id: "snapshot.body",
                rect: annotation.rect,
                asset_id: annotation.asset.id.clone(),
                width_px: annotation.asset.width_px,
                height_px: annotation.asset.height_px,
                opacity: annotation.opacity,
                rotation_degrees: annotation.rotation_degrees,
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_redacts(&self, page_index: u32, editor_state: bool) -> Vec<SceneRedact> {
        self.state
            .redacts
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneRedact {
                id: annotation.id.clone(),
                body_id: "redact.body",
                rect: annotation.rect,
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
            })
            .collect()
    }

    fn scene_ellipses(&self, page_index: u32, editor_state: bool) -> Vec<SceneRectangle> {
        self.state
            .ellipses
            .iter()
            .filter(|annotation| annotation.page_index == page_index)
            .map(|annotation| SceneRectangle {
                id: annotation.id.clone(),
                rect: annotation.rect,
                rotation_degrees: annotation.rotation_degrees,
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                preview: false,
            })
            .collect()
    }

    fn require_no_gesture(&self) -> Result<(), AnnotationError> {
        if self.active_gesture.is_some() {
            Err(AnnotationError::ActiveGesture)
        } else {
            Ok(())
        }
    }

    fn replace_annotation(&mut self, replacement: RectangleAnnotation) {
        let id = replacement.id.clone();
        self.commit_document_change(|rectangles| {
            let annotation = rectangles
                .iter_mut()
                .find(|annotation| annotation.id == id)
                .expect("a gesture replacement must target a committed annotation");
            *annotation = replacement;
        });
    }

    fn replace_ellipse_annotation(&mut self, replacement: EllipseAnnotation) {
        let id = replacement.id.clone();
        self.commit_state_change(|state| {
            let annotation = state
                .ellipses
                .iter_mut()
                .find(|annotation| annotation.id == id)
                .expect("an Ellipse replacement must target a committed annotation");
            *annotation = replacement;
        });
    }

    fn commit_document_change(&mut self, mutate: impl FnOnce(&mut Vec<RectangleAnnotation>)) {
        self.commit_state_change(|state| mutate(&mut state.rectangles));
    }

    fn commit_state_change(&mut self, mutate: impl FnOnce(&mut DocumentState)) {
        push_bounded(&mut self.past, self.state.clone(), self.history_limit);
        self.future.clear();
        mutate(&mut self.state);
        self.state.rectangle_index = RectangleSpatialIndex::rebuild(&self.state.rectangles);
        self.state.revision = self.next_revision;
        self.next_revision = self.next_revision.saturating_add(1);
    }

    fn reconcile_selection(&mut self) {
        let existing = self
            .selected_ids
            .iter()
            .filter(|id| self.contains_annotation(id))
            .cloned()
            .collect();
        self.selected_ids = existing;
    }
}

fn annotation_selection_paths(
    annotation: &Annotation,
    to_viewport: &impl Fn(PdfPoint) -> SelectionPoint,
) -> Vec<SelectionPath> {
    let path = |points: Vec<PdfPoint>, closed| {
        SelectionPath::new(points.into_iter().map(to_viewport).collect(), closed)
    };
    let rect_path = |rect: PdfRect, rotation_degrees: f64| {
        let corners = [
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
        ]
        .into_iter()
        .map(|point| rotate_point_around_rect_center(point, rect, -rotation_degrees))
        .collect();
        path(corners, true)
    };
    match annotation {
        Annotation::Rectangle(annotation) => {
            vec![rect_path(annotation.rect, annotation.rotation_degrees)]
        }
        Annotation::Redact(annotation) => vec![rect_path(annotation.rect, 0.)],
        Annotation::Ellipse(annotation) => {
            vec![rect_path(annotation.rect, annotation.rotation_degrees)]
        }
        Annotation::Arc(annotation) => vec![path(annotation.sampled_path(64), false)],
        Annotation::StraightLine(annotation) => {
            vec![path(vec![annotation.start, annotation.end], false)]
        }
        Annotation::VertexPath(annotation) => {
            vec![path(annotation.points.clone(), annotation.kind.is_closed())]
        }
        Annotation::Cloud(annotation) => vec![path(annotation.scallop_path(), true)],
        Annotation::CloudPlus(annotation) => {
            let mut paths = vec![path(annotation.scallop_path(), true)];
            if !annotation.leader_points.is_empty() {
                paths.push(path(annotation.leader_points.clone(), false));
            }
            paths.push(rect_path(annotation.text_box, 0.));
            paths
        }
        Annotation::Callout(annotation) => vec![
            path(annotation.leader_points.clone(), false),
            rect_path(annotation.text_box, 0.),
        ],
        Annotation::Dimension(annotation) => {
            let (dimension_start, dimension_end) = annotation.dimension_line_points();
            vec![
                path(vec![annotation.start, annotation.end], false),
                path(vec![dimension_start, dimension_end], false),
            ]
        }
        Annotation::MeasurementPath(annotation) => {
            vec![path(annotation.points.clone(), annotation.kind.is_closed())]
        }
        Annotation::Pen(annotation) => annotation
            .paths()
            .map(|points| path(points.to_vec(), false))
            .collect(),
        Annotation::TextBox(annotation) => vec![rect_path(annotation.layout_rect, 0.)],
        Annotation::Length(annotation) => {
            vec![path(vec![annotation.start, annotation.end], false)]
        }
        Annotation::Image(annotation) => vec![rect_path(annotation.rect, 0.)],
        Annotation::Snapshot(annotation) => {
            vec![rect_path(annotation.rect, annotation.rotation_degrees)]
        }
    }
}

fn fixture_string<'a>(value: &'a Value, key: &str) -> Result<&'a str, AnnotationError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| AnnotationError::InvalidFixture(format!("{key} must be a string")))
}

fn fixture_number(value: &Value, key: &str) -> Result<f64, AnnotationError> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| AnnotationError::InvalidFixture(format!("{key} must be a number")))
}

fn fixture_point(value: &Value) -> Result<PdfPoint, AnnotationError> {
    PdfPoint::new(fixture_number(value, "x")?, fixture_number(value, "y")?)
}

fn fixture_appearance(value: &Value) -> Result<RectangleAppearance, AnnotationError> {
    let stroke = fixture_rgba(value, "stroke_rgba")?;
    let fill = fixture_rgba(value, "fill_rgba")?;
    if canonical_float(stroke[3]) != 1.0 {
        return Err(AnnotationError::InvalidFixture(
            "rectangle stroke alpha must be one".into(),
        ));
    }
    let stroke_style = match fixture_string(value, "stroke_style")? {
        "solid" => StrokeStyle::Solid,
        "dashed" => StrokeStyle::Dashed,
        "dotted" => StrokeStyle::Dotted,
        other => {
            return Err(AnnotationError::InvalidFixture(format!(
                "unsupported stroke style {other}"
            )));
        }
    };
    Ok(RectangleAppearance::new(
        rgba_hex(stroke),
        fixture_number(value, "stroke_width_pt")?,
        (fill[3] > 0.0).then(|| rgba_hex(fill)),
        1.0,
    )?
    .with_fill_opacity(fill[3])?
    .with_stroke_style(stroke_style))
}

fn fixture_rgba(value: &Value, key: &str) -> Result<[f64; 4], AnnotationError> {
    let values = value
        .get(key)
        .and_then(Value::as_array)
        .filter(|values| values.len() == 4)
        .ok_or_else(|| AnnotationError::InvalidFixture(format!("{key} must be RGBA")))?;
    let mut rgba = [0.0; 4];
    for (index, value) in values.iter().enumerate() {
        rgba[index] = value
            .as_f64()
            .filter(|component| (0.0..=1.0).contains(component))
            .ok_or_else(|| {
                AnnotationError::InvalidFixture(format!(
                    "{key}[{index}] must be between zero and one"
                ))
            })?;
    }
    Ok(rgba)
}

fn rgba_hex(rgba: [f64; 4]) -> String {
    let component = |value: f64| (value * 255.0).round().clamp(0.0, 255.0) as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        component(rgba[0]),
        component(rgba[1]),
        component(rgba[2])
    )
}

fn color_rgba(color: &str, alpha: f64) -> Value {
    let parse = |range: std::ops::Range<usize>| {
        f64::from(u8::from_str_radix(&color[range], 16).expect("validated color")) / 255.0
    };
    Value::Array(vec![
        fixture_number_value(parse(1..3)),
        fixture_number_value(parse(3..5)),
        fixture_number_value(parse(5..7)),
        fixture_number_value(alpha),
    ])
}

fn fixture_number_value(value: f64) -> Value {
    let value = canonical_float(value);
    if value.fract() == 0.0 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

fn fixture_canonical_style(appearance: &RectangleAppearance) -> Value {
    json!({
        "stroke_rgba": color_rgba(appearance.stroke_color(), 1.0),
        "fill_rgba": appearance
            .fill_color()
            .map(|color| color_rgba(color, appearance.fill_opacity()))
            .unwrap_or_else(|| json!([0.0, 0.0, 0.0, 0.0])),
        "stroke_width_pt": fixture_number_value(appearance.stroke_width_pt()),
        "stroke_style": match appearance.stroke_style() {
            StrokeStyle::Solid => "solid",
            StrokeStyle::Dashed => "dashed",
            StrokeStyle::Dotted => "dotted",
        },
    })
}

fn canonical_sha256(value: &Value) -> String {
    let bytes = format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("canonical JSON must serialize")
    );
    format!("{:x}", Sha256::digest(bytes.as_bytes()))
}

fn push_bounded(history: &mut VecDeque<DocumentState>, state: DocumentState, limit: usize) {
    if history.len() == limit {
        history.pop_front();
    }
    history.push_back(state);
}

fn require_pointer(gesture: &ActiveGesture, pointer_id: u64) -> Result<(), AnnotationError> {
    let expected = gesture.pointer_id();
    if expected == pointer_id {
        Ok(())
    } else {
        Err(AnnotationError::PointerMismatch {
            expected,
            received: pointer_id,
        })
    }
}

fn validate_tolerance(tolerance: f64) -> Result<(), AnnotationError> {
    if tolerance.is_finite() && tolerance >= 0.0 {
        Ok(())
    } else {
        Err(AnnotationError::InvalidTolerance)
    }
}

fn validate_pen_path(points: &[PdfPoint]) -> Result<(), AnnotationError> {
    if !(2..=MAX_STREAMED_PATH_POINTS).contains(&points.len()) {
        return Err(AnnotationError::InvalidGeometry(format!(
            "pen path must contain between 2 and {MAX_STREAMED_PATH_POINTS} points"
        )));
    }
    for point in points {
        require_finite("pen.point.x", point.x)?;
        require_finite("pen.point.y", point.y)?;
    }
    if points.windows(2).all(|pair| pair[0] == pair[1]) {
        return Err(AnnotationError::InvalidGeometry(
            "pen path must span at least two distinct points".into(),
        ));
    }
    Ok(())
}

fn validate_vertex_path(points: &[PdfPoint], kind: VertexPathKind) -> Result<(), AnnotationError> {
    if !(kind.minimum_points()..=MAX_STREAMED_PATH_POINTS).contains(&points.len()) {
        return Err(AnnotationError::InvalidGeometry(format!(
            "{} must contain between {} and {MAX_STREAMED_PATH_POINTS} points",
            match kind {
                VertexPathKind::Polyline => "polyline",
                VertexPathKind::Polygon => "polygon",
            },
            kind.minimum_points(),
        )));
    }
    for point in points {
        require_finite("vertex_path.point.x", point.x)?;
        require_finite("vertex_path.point.y", point.y)?;
    }
    if points
        .windows(2)
        .any(|pair| point_distance(pair[0], pair[1]) < 0.5)
        || kind.is_closed()
            && point_distance(
                *points.last().expect("a Polygon has at least three points"),
                points[0],
            ) < 0.5
    {
        return Err(AnnotationError::InvalidGeometry(
            "adjacent vertex-path points must be at least 0.5 PDF points apart".into(),
        ));
    }
    Ok(())
}

fn sampled_cloud_scallop_path(control_path: &[PdfPoint], radius: f64) -> Vec<PdfPoint> {
    if control_path.len() < 3 {
        return control_path.to_vec();
    }
    let signed_area = control_path
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = control_path[(index + 1) % control_path.len()];
            point.x * next.y - next.x * point.y
        })
        .sum::<f64>();
    let orientation = if signed_area >= 0. { 1. } else { -1. };
    let spacing = radius.max(3.);
    let mut outline = Vec::new();
    for (index, start) in control_path.iter().copied().enumerate() {
        let end = control_path[(index + 1) % control_path.len()];
        let dx = end.x - start.x;
        let dy = end.y - start.y;
        let length = (dx * dx + dy * dy).sqrt();
        if length <= f64::EPSILON {
            continue;
        }
        let lobes = (length / spacing).round().max(1.) as usize;
        let outward_x = orientation * dy / length;
        let outward_y = orientation * -dx / length;
        for lobe in 0..lobes {
            let t0 = lobe as f64 / lobes as f64;
            let t1 = (lobe + 1) as f64 / lobes as f64;
            let from = PdfPoint {
                x: start.x + dx * t0,
                y: start.y + dy * t0,
            };
            let to = PdfPoint {
                x: start.x + dx * t1,
                y: start.y + dy * t1,
            };
            let control = PdfPoint {
                x: (from.x + to.x) / 2. + outward_x * radius * 0.65,
                y: (from.y + to.y) / 2. + outward_y * radius * 0.65,
            };
            if outline.is_empty() {
                outline.push(from);
            }
            for sample in 1..=8 {
                let t = sample as f64 / 8.;
                let one_minus_t = 1. - t;
                outline.push(PdfPoint {
                    x: canonical_float(
                        one_minus_t * one_minus_t * from.x
                            + 2. * one_minus_t * t * control.x
                            + t * t * to.x,
                    ),
                    y: canonical_float(
                        one_minus_t * one_minus_t * from.y
                            + 2. * one_minus_t * t * control.y
                            + t * t * to.y,
                    ),
                });
            }
        }
    }
    if let Some(first) = outline.first().copied() {
        if outline.last() != Some(&first) {
            outline.push(first);
        }
    }
    outline
}

fn validate_measurement_path(
    points: &[PdfPoint],
    kind: MeasurementPathKind,
) -> Result<(), AnnotationError> {
    if !(kind.minimum_points()..=MAX_STREAMED_PATH_POINTS).contains(&points.len()) {
        return Err(AnnotationError::InvalidGeometry(format!(
            "{} must contain between {} and {MAX_STREAMED_PATH_POINTS} points",
            match kind {
                MeasurementPathKind::Polylength => "polylength",
                MeasurementPathKind::Area => "area",
            },
            kind.minimum_points(),
        )));
    }
    for point in points {
        require_finite("measurement_path.point.x", point.x)?;
        require_finite("measurement_path.point.y", point.y)?;
    }
    if points
        .windows(2)
        .any(|pair| point_distance(pair[0], pair[1]) < 0.5)
    {
        return Err(AnnotationError::InvalidGeometry(
            "adjacent measurement-path points must be at least 0.5 PDF points apart".into(),
        ));
    }
    Ok(())
}

fn validate_layout_rect(rect: PdfRect, kind: &str) -> Result<(), AnnotationError> {
    for (name, value) in [
        ("layout.x", rect.x),
        ("layout.y", rect.y),
        ("layout.width", rect.width),
        ("layout.height", rect.height),
    ] {
        require_finite(name, value)?;
    }
    if rect.width < MIN_RECT_SIZE_PT || rect.height < MIN_RECT_SIZE_PT {
        return Err(AnnotationError::InvalidGeometry(format!(
            "{kind} layout dimensions must be at least {MIN_RECT_SIZE_PT} point"
        )));
    }
    Ok(())
}

fn validate_image_aspect(
    rect: PdfRect,
    asset: &DecodedRgbaAsset,
    aspect_locked: bool,
) -> Result<(), AnnotationError> {
    if aspect_locked {
        let rectangle_ratio = rect.width / rect.height;
        let asset_ratio = f64::from(asset.width_px) / f64::from(asset.height_px);
        if (rectangle_ratio - asset_ratio).abs() > 0.000_001 {
            return Err(AnnotationError::InvalidGeometry(
                "aspect-locked image rectangle must match the decoded asset ratio".into(),
            ));
        }
    }
    Ok(())
}

fn validate_snapshot_rect(rect: PdfRect) -> Result<(), AnnotationError> {
    validate_layout_rect(rect, "snapshot")?;
    if rect.width < MIN_SNAPSHOT_SIZE_PT || rect.height < MIN_SNAPSHOT_SIZE_PT {
        return Err(AnnotationError::InvalidGeometry(format!(
            "snapshot dimensions must be at least {MIN_SNAPSHOT_SIZE_PT} PDF points"
        )));
    }
    Ok(())
}

fn validate_snapshot_opacity(opacity: f64) -> Result<(), AnnotationError> {
    require_finite("snapshot.opacity", opacity)?;
    if !(0.0..=1.0).contains(&opacity) {
        return Err(AnnotationError::InvalidAppearance(
            "snapshot opacity must be between zero and one".into(),
        ));
    }
    Ok(())
}

fn validate_text(value: &str, field: &str, max_bytes: usize) -> Result<(), AnnotationError> {
    if value.is_empty() || value.len() > max_bytes || value.contains('\0') {
        return Err(AnnotationError::InvalidGeometry(format!(
            "{field} must contain 1 to {max_bytes} UTF-8 bytes without NUL"
        )));
    }
    Ok(())
}

fn require_finite(name: &str, value: f64) -> Result<(), AnnotationError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(AnnotationError::InvalidGeometry(format!(
            "{name} must be finite"
        )))
    }
}

fn normalize_color(color: String) -> Result<String, AnnotationError> {
    let bytes = color.as_bytes();
    if bytes.len() != 7
        || bytes.first() != Some(&b'#')
        || !bytes[1..].iter().all(u8::is_ascii_hexdigit)
    {
        return Err(AnnotationError::InvalidAppearance(format!(
            "{color:?} is not a six-digit hex color"
        )));
    }
    Ok(color.to_ascii_lowercase())
}

fn point_distance(left: PdfPoint, right: PdfPoint) -> f64 {
    (left.x - right.x).hypot(left.y - right.y)
}

fn point_segment_distance(point: PdfPoint, start: PdfPoint, end: PdfPoint) -> f64 {
    let delta_x = end.x - start.x;
    let delta_y = end.y - start.y;
    let length_squared = delta_x * delta_x + delta_y * delta_y;
    if length_squared == 0.0 {
        return point_distance(point, start);
    }
    let projection = (((point.x - start.x) * delta_x + (point.y - start.y) * delta_y)
        / length_squared)
        .clamp(0.0, 1.0);
    point_distance(
        point,
        PdfPoint {
            x: start.x + projection * delta_x,
            y: start.y + projection * delta_y,
        },
    )
}

fn line_annotation_kind(kind: LineKind) -> AnnotationKind {
    match kind {
        LineKind::Line => AnnotationKind::Line,
        LineKind::Arrow => AnnotationKind::Arrow,
    }
}

fn rotate_point_around_rect_center(
    point: PdfPoint,
    rect: PdfRect,
    rotation_degrees: f64,
) -> PdfPoint {
    let center = rect.center();
    let radians = rotation_degrees.to_radians();
    let delta_x = point.x - center.x;
    let delta_y = point.y - center.y;
    PdfPoint {
        x: canonical_float(center.x + delta_x * radians.cos() - delta_y * radians.sin()),
        y: canonical_float(center.y + delta_x * radians.sin() + delta_y * radians.cos()),
    }
}

fn normalize_degrees(value: f64) -> f64 {
    canonical_float(value.rem_euclid(360.0))
}

pub fn rectangle_world_corners(rect: PdfRect, rotation_degrees: f64) -> [PdfPoint; 4] {
    [
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
    ]
    .map(|point| rotate_point_around_rect_center(point, rect, -rotation_degrees))
}

pub fn ellipse_cubic_bezier_points(
    rect: PdfRect,
    rotation_degrees: f64,
) -> (PdfPoint, [(PdfPoint, PdfPoint, PdfPoint); 4]) {
    const KAPPA: f64 = 0.552_284_749_830_793_6;
    let center = rect.center();
    let radius_x = rect.width / 2.;
    let radius_y = rect.height / 2.;
    let rotate = |point| rotate_point_around_rect_center(point, rect, -rotation_degrees);
    let start = rotate(PdfPoint {
        x: center.x + radius_x,
        y: center.y,
    });
    let segments = [
        (
            PdfPoint {
                x: center.x + radius_x,
                y: center.y + KAPPA * radius_y,
            },
            PdfPoint {
                x: center.x + KAPPA * radius_x,
                y: center.y + radius_y,
            },
            PdfPoint {
                x: center.x,
                y: center.y + radius_y,
            },
        ),
        (
            PdfPoint {
                x: center.x - KAPPA * radius_x,
                y: center.y + radius_y,
            },
            PdfPoint {
                x: center.x - radius_x,
                y: center.y + KAPPA * radius_y,
            },
            PdfPoint {
                x: center.x - radius_x,
                y: center.y,
            },
        ),
        (
            PdfPoint {
                x: center.x - radius_x,
                y: center.y - KAPPA * radius_y,
            },
            PdfPoint {
                x: center.x - KAPPA * radius_x,
                y: center.y - radius_y,
            },
            PdfPoint {
                x: center.x,
                y: center.y - radius_y,
            },
        ),
        (
            PdfPoint {
                x: center.x + KAPPA * radius_x,
                y: center.y - radius_y,
            },
            PdfPoint {
                x: center.x + radius_x,
                y: center.y - KAPPA * radius_y,
            },
            PdfPoint {
                x: center.x + radius_x,
                y: center.y,
            },
        ),
    ]
    .map(|(control_a, control_b, to)| (rotate(control_a), rotate(control_b), rotate(to)));
    (start, segments)
}

fn rectangle_world_bounds(rect: PdfRect, rotation_degrees: f64) -> PdfRect {
    let corners = rectangle_world_corners(rect, rotation_degrees);
    let left = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let right = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let top = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect {
        x: canonical_float(left),
        y: canonical_float(bottom),
        width: canonical_float(right - left),
        height: canonical_float(top - bottom),
    }
}

pub fn rectangle_rotation_handle_world_point(
    rect: PdfRect,
    rotation_degrees: f64,
    offset_pt: f64,
) -> PdfPoint {
    rotate_point_around_rect_center(
        PdfPoint {
            x: rect.x + rect.width / 2.0,
            y: rect.y + rect.height + offset_pt,
        },
        rect,
        -rotation_degrees,
    )
}

fn canonical_float(value: f64) -> f64 {
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

fn canonical_rectangle(annotation: &RectangleAnnotation) -> Value {
    let mut value = json!({
        "appearance": {
            "fill": { "color": annotation.appearance.fill_color },
            "opacity": annotation.appearance.opacity,
            "fillOpacity": annotation.appearance.fill_opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "id": annotation.id.as_str(),
        "kind": "rectangle",
        "pageIndex": annotation.page_index,
        "rect": {
            "height": annotation.rect.height,
            "width": annotation.rect.width,
            "x": annotation.rect.x,
            "y": annotation.rect.y,
        },
    });
    if annotation.rotation_degrees != 0.0 {
        value["rotation"] = json!(annotation.rotation_degrees);
    }
    value
}

fn canonical_redact(annotation: &RedactAnnotation) -> Value {
    let mut value = json!({
        "appearance": {
            "fill": { "color": annotation.appearance.fill_color },
            "opacity": annotation.appearance.opacity,
            "fillOpacity": annotation.appearance.fill_opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "id": annotation.id.as_str(),
        "kind": "redact",
        "pageIndex": annotation.page_index,
        "pending": true,
        "redactionColor": annotation.redaction_color,
        "rect": {
            "height": annotation.rect.height,
            "width": annotation.rect.width,
            "x": annotation.rect.x,
            "y": annotation.rect.y,
        },
    });
    if let Some(overlay_text) = &annotation.overlay_text {
        value["overlayText"] = json!(overlay_text);
    }
    if annotation.locked {
        value["locked"] = json!(true);
    }
    value
}

fn canonical_ellipse(annotation: &EllipseAnnotation) -> Value {
    let mut value = json!({
        "appearance": {
            "fill": { "color": annotation.appearance.fill_color },
            "opacity": annotation.appearance.opacity,
            "fillOpacity": annotation.appearance.fill_opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "style": match annotation.appearance.stroke_style {
                    StrokeStyle::Solid => "solid",
                    StrokeStyle::Dashed => "dashed",
                    StrokeStyle::Dotted => "dotted",
                },
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "id": annotation.id.as_str(),
        "kind": "ellipse",
        "pageIndex": annotation.page_index,
        "rect": {
            "height": annotation.rect.height,
            "width": annotation.rect.width,
            "x": annotation.rect.x,
            "y": annotation.rect.y,
        },
    });
    if annotation.rotation_degrees != 0.0 {
        value["rotation"] = json!(annotation.rotation_degrees);
    }
    value
}

fn canonical_pen(annotation: &PenAnnotation) -> Value {
    let mut value = json!({
        "appearance": {
            "color": annotation.appearance.color,
            "opacity": annotation.appearance.opacity,
            "widthPt": annotation.appearance.width_pt,
        },
        "id": annotation.id.as_str(),
        "kind": "pen",
        "pageIndex": annotation.page_index,
        "smoothCurves": annotation.smooth_curves,
        "tool": match annotation.tool {
            InkTool::Pen => "pen",
            InkTool::Highlight => "highlight",
        },
        "blend": match annotation.blend_mode {
            BlendMode::Normal => "normal",
            BlendMode::Multiply => "multiply",
        },
        "points": annotation.points.iter().map(|point| json!({
            "x": point.x,
            "y": point.y,
        })).collect::<Vec<_>>(),
    });
    if !annotation.additional_paths.is_empty() {
        value["additionalPaths"] = json!(
            annotation
                .additional_paths
                .iter()
                .map(|path| {
                    path.iter()
                        .map(|point| json!({ "x": point.x, "y": point.y }))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        );
    }
    value
}

fn canonical_straight_line(annotation: &StraightLineAnnotation) -> Value {
    json!({
        "appearance": {
            "opacity": annotation.appearance.opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "style": match annotation.appearance.stroke_style {
                    StrokeStyle::Solid => "solid",
                    StrokeStyle::Dashed => "dashed",
                    StrokeStyle::Dotted => "dotted",
                },
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "end": { "x": annotation.end.x, "y": annotation.end.y },
        "id": annotation.id.as_str(),
        "kind": match annotation.kind {
            LineKind::Line => "line",
            LineKind::Arrow => "arrow",
        },
        "pageIndex": annotation.page_index,
        "start": { "x": annotation.start.x, "y": annotation.start.y },
    })
}

fn canonical_vertex_path(annotation: &VertexPathAnnotation) -> Value {
    json!({
        "appearance": {
            "fill": { "color": annotation.appearance.fill_color },
            "opacity": annotation.appearance.opacity,
            "fillOpacity": annotation.appearance.fill_opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "style": match annotation.appearance.stroke_style {
                    StrokeStyle::Solid => "solid",
                    StrokeStyle::Dashed => "dashed",
                    StrokeStyle::Dotted => "dotted",
                },
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "id": annotation.id.as_str(),
        "kind": match annotation.kind {
            VertexPathKind::Polyline => "polyline",
            VertexPathKind::Polygon => "polygon",
        },
        "pageIndex": annotation.page_index,
        "points": annotation.points.iter().map(|point| json!({
            "x": point.x,
            "y": point.y,
        })).collect::<Vec<_>>(),
    })
}

fn canonical_cloud(annotation: &CloudAnnotation) -> Value {
    json!({
        "appearance": {
            "opacity": annotation.appearance.opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "borderEffectIntensity": annotation.border_effect_intensity,
        "id": annotation.id.as_str(),
        "kind": "cloud",
        "pageIndex": annotation.page_index,
        "points": annotation.points.iter().map(|point| json!({
            "x": point.x,
            "y": point.y,
        })).collect::<Vec<_>>(),
    })
}

fn canonical_cloud_plus(annotation: &CloudPlusAnnotation) -> Value {
    json!({
        "appearance": {
            "opacity": annotation.appearance.cloud.opacity(),
            "stroke": {
                "color": annotation.appearance.cloud.stroke_color(),
                "style": match annotation.appearance.cloud.stroke_style() {
                    StrokeStyle::Solid => "solid",
                    StrokeStyle::Dashed => "dashed",
                    StrokeStyle::Dotted => "dotted",
                },
                "widthPt": annotation.appearance.cloud.stroke_width_pt(),
            },
            "text": {
                "color": annotation.appearance.text.color(),
                "fontFamily": annotation.appearance.text.font_family(),
                "fontSizePt": annotation.appearance.text.font_size_pt(),
            },
        },
        "borderEffectIntensity": annotation.border_effect_intensity,
        "cloudPoints": annotation.cloud_points.iter().map(|point| json!({
            "x": point.x,
            "y": point.y,
        })).collect::<Vec<_>>(),
        "content": annotation.content,
        "id": annotation.id.as_str(),
        "kind": "cloud-plus",
        "leaderPoints": annotation.leader_points.iter().map(|point| json!({
            "x": point.x,
            "y": point.y,
        })).collect::<Vec<_>>(),
        "pageIndex": annotation.page_index,
        "textBox": {
            "x": annotation.text_box.x,
            "y": annotation.text_box.y,
            "width": annotation.text_box.width,
            "height": annotation.text_box.height,
        },
    })
}

fn canonical_callout(annotation: &CalloutAnnotation) -> Value {
    json!({
        "appearance": {
            "opacity": annotation.appearance.line.opacity(),
            "stroke": {
                "color": annotation.appearance.line.stroke_color(),
                "widthPt": annotation.appearance.line.stroke_width_pt(),
            },
            "text": {
                "color": annotation.appearance.text.color(),
                "fontFamily": annotation.appearance.text.font_family(),
                "fontSizePt": annotation.appearance.text.font_size_pt(),
            },
        },
        "content": annotation.content,
        "id": annotation.id.as_str(),
        "kind": "callout",
        "leaderPoints": annotation.leader_points.iter().map(|point| json!({
            "x": point.x,
            "y": point.y,
        })).collect::<Vec<_>>(),
        "pageIndex": annotation.page_index,
        "textBox": {
            "x": annotation.text_box.x,
            "y": annotation.text_box.y,
            "width": annotation.text_box.width,
            "height": annotation.text_box.height,
        },
    })
}

fn canonical_dimension(annotation: &DimensionAnnotation) -> Value {
    json!({
        "appearance": {
            "opacity": annotation.appearance.line.opacity(),
            "stroke": {
                "color": annotation.appearance.line.stroke_color(),
                "style": match annotation.appearance.line.stroke_style() {
                    StrokeStyle::Solid => "solid",
                    StrokeStyle::Dashed => "dashed",
                    StrokeStyle::Dotted => "dotted",
                },
                "widthPt": annotation.appearance.line.stroke_width_pt(),
            },
            "text": {
                "color": annotation.appearance.text.color(),
                "fontFamily": annotation.appearance.text.font_family(),
                "fontSizePt": annotation.appearance.text.font_size_pt(),
            },
        },
        "content": annotation.content,
        "dimensionLineOffset": annotation.dimension_line_offset,
        "end": { "x": annotation.end.x, "y": annotation.end.y },
        "id": annotation.id.as_str(),
        "kind": "dimension",
        "pageIndex": annotation.page_index,
        "start": { "x": annotation.start.x, "y": annotation.start.y },
    })
}

fn canonical_arc(annotation: &ArcAnnotation) -> Value {
    json!({
        "appearance": {
            "opacity": annotation.appearance.opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "style": match annotation.appearance.stroke_style {
                    StrokeStyle::Solid => "solid",
                    StrokeStyle::Dashed => "dashed",
                    StrokeStyle::Dotted => "dotted",
                },
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "end": { "x": annotation.end.x, "y": annotation.end.y },
        "id": annotation.id.as_str(),
        "kind": "arc",
        "mid": { "x": annotation.mid.x, "y": annotation.mid.y },
        "pageIndex": annotation.page_index,
        "start": { "x": annotation.start.x, "y": annotation.start.y },
    })
}

fn canonical_measurement_path(annotation: &MeasurementPathAnnotation) -> Value {
    json!({
        "appearance": {
            "fill": { "color": annotation.appearance.fill_color },
            "opacity": annotation.appearance.opacity,
            "fillOpacity": annotation.appearance.fill_opacity,
            "stroke": {
                "color": annotation.appearance.stroke_color,
                "style": match annotation.appearance.stroke_style {
                    StrokeStyle::Solid => "solid",
                    StrokeStyle::Dashed => "dashed",
                    StrokeStyle::Dotted => "dotted",
                },
                "widthPt": annotation.appearance.stroke_width_pt,
            },
        },
        "caption": annotation.caption(),
        "id": annotation.id.as_str(),
        "kind": match annotation.kind {
            MeasurementPathKind::Polylength => "polylength",
            MeasurementPathKind::Area => "area",
        },
        "pageIndex": annotation.page_index,
        "points": annotation.points.iter().map(|point| json!({
            "x": point.x,
            "y": point.y,
        })).collect::<Vec<_>>(),
        "scale": {
            "paper_points": annotation.calibration.paper_points(),
            "precision": annotation.calibration.precision(),
            "real_world_value": annotation.calibration.real_world_value(),
            "unit": annotation.calibration.unit(),
            "units_per_point": annotation.calibration.units_per_point(),
        },
    })
}

fn canonical_text_box(annotation: &TextBoxAnnotation) -> Value {
    json!({
        "content": annotation.content,
        "id": annotation.id.as_str(),
        "kind": "textBox",
        "layoutRect": {
            "height": annotation.layout_rect.height,
            "width": annotation.layout_rect.width,
            "x": annotation.layout_rect.x,
            "y": annotation.layout_rect.y,
        },
        "pageIndex": annotation.page_index,
        "style": {
            "alignment": match annotation.style.alignment {
                TextAlignment::Left => "left",
                TextAlignment::Center => "center",
                TextAlignment::Right => "right",
            },
            "color": annotation.style.color,
            "fontFamily": annotation.style.font_family,
            "fontSizePt": annotation.style.font_size_pt,
            "opacity": annotation.style.opacity,
            "weight": annotation.style.weight,
        },
    })
}

fn canonical_length(annotation: &LengthAnnotation) -> Value {
    json!({
        "calibration": {
            "label": annotation.calibration.label,
            "paperPoints": annotation.calibration.paper_points,
            "precision": annotation.calibration.precision,
            "realWorldValue": annotation.calibration.real_world_value,
            "showCaption": annotation.calibration.show_caption,
            "unit": annotation.calibration.unit,
            "unitsPerPoint": annotation.calibration.units_per_point,
        },
        "caption": annotation.caption(),
        "end": { "x": annotation.end.x, "y": annotation.end.y },
        "id": annotation.id.as_str(),
        "kind": "length",
        "pageIndex": annotation.page_index,
        "start": { "x": annotation.start.x, "y": annotation.start.y },
    })
}

fn canonical_image(annotation: &ImageAnnotation) -> Value {
    json!({
        "aspectLocked": annotation.aspect_locked,
        "asset": {
            "heightPx": annotation.asset.height_px,
            "id": annotation.asset.id.as_str(),
            "widthPx": annotation.asset.width_px,
        },
        "id": annotation.id.as_str(),
        "kind": "image",
        "pageIndex": annotation.page_index,
        "rect": {
            "height": annotation.rect.height,
            "width": annotation.rect.width,
            "x": annotation.rect.x,
            "y": annotation.rect.y,
        },
    })
}

fn canonical_snapshot(annotation: &SnapshotAnnotation) -> Value {
    json!({
        "asset": {
            "heightPx": annotation.asset.height_px,
            "id": annotation.asset.id.as_str(),
            "widthPx": annotation.asset.width_px,
        },
        "id": annotation.id.as_str(),
        "kind": "snapshot",
        "locked": annotation.locked,
        "opacity": annotation.opacity,
        "pageIndex": annotation.page_index,
        "rect": {
            "height": annotation.rect.height,
            "width": annotation.rect.width,
            "x": annotation.rect.x,
            "y": annotation.rect.y,
        },
        "rotationDegrees": annotation.rotation_degrees,
    })
}

fn canonicalize_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_json).collect()),
        Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize_json(value)))
                    .collect::<Map<_, _>>(),
            )
        }
        scalar => scalar,
    }
}

/// A small, deterministic semantic fingerprint. This is not a security hash;
/// the performance protocol names the FNV-1a algorithm explicitly.
pub fn fnv1a64_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> PdfPoint {
        PdfPoint::new(x, y).unwrap()
    }

    fn id(value: &str) -> MarkupId {
        MarkupId::new(value).unwrap()
    }

    fn create_rectangle(document: &mut AnnotationDocument, markup_id: &str) {
        document
            .begin_create(
                7,
                id(markup_id),
                0,
                point(10.0, 20.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(7, point(110.0, 70.0)).unwrap();
        assert_eq!(
            document.commit_gesture(7).unwrap(),
            CommitOutcome::Created(id(markup_id))
        );
    }

    #[test]
    fn rendered_pointer_rectangle_matches_pdf_edge_reconstruction_only_within_pdf_tolerance() {
        let expected = RectangleAnnotation {
            id: id("workspace:rectangle:1"),
            page_index: 0,
            rect: PdfRect::new(89.999_995, 84.000_022, 144.000_052, 95.999_934).unwrap(),
            rotation_degrees: 0.,
            appearance: RectangleAppearance::default(),
            locked: false,
        };
        let reopened = RectangleAnnotation {
            rect: PdfRect::new(89.999_992, 84.000_023, 144.000_053, 95.999_931).unwrap(),
            ..expected.clone()
        };
        assert_ne!(expected, reopened);
        assert!(expected.same_persisted_state_as(&reopened));

        let materially_moved = RectangleAnnotation {
            rect: PdfRect::new(90.000_02, 84.000_023, 144.000_053, 95.999_931).unwrap(),
            ..reopened
        };
        assert!(!expected.same_persisted_state_as(&materially_moved));
    }

    #[test]
    fn rotated_page_transform_round_trips_points_and_rotates_rect_bounds() {
        let source = PdfPoint::new(72., 144.).unwrap();
        let rect = PdfRect::new(72., 144., 36., 54.).unwrap();
        for rotation in [
            PageRotation::Degrees0,
            PageRotation::Degrees90,
            PageRotation::Degrees180,
            PageRotation::Degrees270,
        ] {
            let transform = PageTransform::new_rotated(612., 792., 2., rotation).unwrap();
            let local = transform.point_to_local_pixels(source);
            let round_trip = transform.point_from_local_pixels(local.x, local.y).unwrap();
            assert!((round_trip.x - source.x).abs() < 0.000_1);
            assert!((round_trip.y - source.y).abs() < 0.000_1);
            let local_rect = transform.rect_to_local_pixels(rect);
            let expected = if rotation.swaps_axes() {
                (rect.height * 2., rect.width * 2.)
            } else {
                (rect.width * 2., rect.height * 2.)
            };
            assert!((local_rect.width - expected.0).abs() < 0.000_1);
            assert!((local_rect.height - expected.1).abs() < 0.000_1);
        }
    }

    #[test]
    fn create_uses_pdf_space_and_keeps_preview_uncommitted() {
        let mut document = AnnotationDocument::default();
        document
            .begin_create(
                11,
                id("rect-primary"),
                3,
                point(120.0, 200.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        let preview = document.update_gesture(11, point(40.0, 140.0)).unwrap();

        assert_eq!(document.rectangles(), []);
        assert_eq!(document.history_depths(), (0, 0));
        assert_eq!(preview.kind, GestureKind::Create);
        assert_eq!(
            preview.annotation.rect,
            PdfRect::new(40.0, 140.0, 80.0, 60.0).unwrap()
        );

        assert_eq!(
            document.commit_gesture(11).unwrap(),
            CommitOutcome::Created(id("rect-primary"))
        );
        assert_eq!(document.rectangles(), [preview.annotation]);
        assert_eq!(document.selected_id(), Some(&id("rect-primary")));
        assert_eq!(document.history_depths(), (1, 0));
    }

    #[test]
    fn short_create_and_cancel_do_not_mutate_history() {
        let mut document = AnnotationDocument::default();
        document
            .begin_create(
                1,
                id("short"),
                0,
                point(10.0, 10.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(1, point(10.5, 20.0)).unwrap();
        assert_eq!(
            document.commit_gesture(1).unwrap(),
            CommitOutcome::Cancelled
        );

        document
            .begin_create(
                2,
                id("cancelled"),
                0,
                point(0.0, 0.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(2, point(20.0, 20.0)).unwrap();
        document.cancel_gesture(2).unwrap();

        assert!(document.rectangles().is_empty());
        assert_eq!(document.history_depths(), (0, 0));
    }

    #[test]
    fn stable_ids_are_rejected_when_invalid_or_duplicated() {
        assert_eq!(MarkupId::new(""), Err(AnnotationError::InvalidMarkupId));
        assert_eq!(
            MarkupId::new(" rect"),
            Err(AnnotationError::InvalidMarkupId)
        );

        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "stable-rectangle");
        assert_eq!(
            document.begin_create(
                9,
                id("stable-rectangle"),
                0,
                point(0.0, 0.0),
                RectangleAppearance::default(),
            ),
            Err(AnnotationError::DuplicateMarkupId(id("stable-rectangle")))
        );
    }

    #[test]
    fn hit_testing_prefers_all_selected_resize_handles_and_topmost_body() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "back");
        document.clear_selection();
        document
            .begin_create(
                8,
                id("front"),
                0,
                point(10.0, 20.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(8, point(110.0, 70.0)).unwrap();
        document.commit_gesture(8).unwrap();

        assert_eq!(
            document.hit_test(0, point(10.0, 40.0), 2.0).unwrap(),
            Some(HitTarget::Body(id("front")))
        );
        for (handle, handle_point) in [
            (RectangleResizeHandle::NorthWest, point(10.0, 70.0)),
            (RectangleResizeHandle::North, point(60.0, 70.0)),
            (RectangleResizeHandle::NorthEast, point(110.0, 70.0)),
            (RectangleResizeHandle::East, point(110.0, 45.0)),
            (RectangleResizeHandle::SouthEast, point(110.0, 20.0)),
            (RectangleResizeHandle::South, point(60.0, 20.0)),
            (RectangleResizeHandle::SouthWest, point(10.0, 20.0)),
            (RectangleResizeHandle::West, point(10.0, 45.0)),
        ] {
            assert_eq!(
                document.hit_test(0, handle_point, 3.0).unwrap(),
                Some(HitTarget::ResizeHandle {
                    id: id("front"),
                    handle,
                })
            );
        }
        assert_eq!(document.hit_test(1, point(10.0, 40.0), 2.0).unwrap(), None);
    }

    #[test]
    fn rectangle_interior_is_a_body_hit_with_or_without_fill() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "rect");
        document.clear_selection();
        assert_eq!(
            document.hit_test(0, point(50.0, 40.0), 2.0).unwrap(),
            Some(HitTarget::Body(id("rect")))
        );

        document.select(&id("rect"));
        document
            .set_selected_appearance(
                RectangleAppearance::new("#123456", 2.0, Some("#abcdef"), 0.5).unwrap(),
            )
            .unwrap();
        document.clear_selection();
        assert_eq!(
            document.hit_test(0, point(50.0, 40.0), 2.0).unwrap(),
            Some(HitTarget::Body(id("rect")))
        );
    }

    #[test]
    fn spatial_index_matches_linear_topmost_hit_semantics_and_bounds_dense_work() {
        let mut document = AnnotationDocument::with_history_limit(128).unwrap();
        for index in 0..100 {
            let column = (index % 10) as f64;
            let row = (index / 10) as f64;
            document
                .begin_create(
                    index as u64 + 1,
                    id(&format!("dense-{index:03}")),
                    0,
                    point(12.0 + column * 58.0, 18.0 + row * 68.0),
                    RectangleAppearance::default(),
                )
                .unwrap();
            document
                .update_gesture(
                    index as u64 + 1,
                    point(46.0 + column * 58.0, 46.0 + row * 68.0),
                )
                .unwrap();
            document.commit_gesture(index as u64 + 1).unwrap();
        }
        document.clear_selection();

        for query in [
            point(20.0, 20.0),
            point(162.0, 192.0),
            point(530.0, 620.0),
            point(600.0, 760.0),
        ] {
            let linear = document
                .rectangles()
                .iter()
                .rev()
                .find(|annotation| {
                    annotation.page_index == 0
                        && (annotation.rect.near_perimeter(query, 4.0)
                            || annotation.rect.contains(query, 4.0))
                })
                .map(|annotation| HitTarget::Body(annotation.id.clone()));
            assert_eq!(document.hit_test(0, query, 4.0).unwrap(), linear);
        }
        let work = document
            .spatial_query_work(0, point(162.0, 192.0), 4.0)
            .unwrap();
        assert_eq!(work.total_rectangle_count, 100);
        assert!(work.candidate_count < 12, "query examined {work:?}");
    }

    #[test]
    fn move_stream_previews_many_updates_but_commits_one_history_entry() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "moving");
        document.clear_selection();
        let original = document.rectangles()[0].clone();

        assert_eq!(
            document.begin_move(31, 0, point(10.0, 40.0), 2.0).unwrap(),
            Some(id("moving"))
        );
        for step in 1..=360 {
            document
                .update_gesture(31, point(10.0 + f64::from(step) / 10.0, 40.0 + 24.0))
                .unwrap();
        }
        assert_eq!(document.rectangles()[0], original);
        assert_eq!(document.history_depths(), (1, 0));

        assert_eq!(
            document.commit_gesture(31).unwrap(),
            CommitOutcome::Updated(id("moving"))
        );
        assert_eq!(
            document.rectangles()[0].rect,
            PdfRect::new(46.0, 44.0, 100.0, 50.0).unwrap()
        );
        assert_eq!(document.history_depths(), (2, 0));
    }

    #[test]
    fn resize_handles_keep_the_opposite_edges_fixed_and_enforce_minimum_size() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "resizing");

        assert_eq!(
            document
                .begin_resize(41, 0, point(10.0, 70.0), 3.0)
                .unwrap(),
            Some(id("resizing"))
        );
        let preview = document.update_gesture(41, point(0.0, 80.0)).unwrap();
        assert_eq!(
            preview.annotation.rect,
            PdfRect::new(0.0, 20.0, 110.0, 60.0).unwrap()
        );
        assert_eq!(
            document.commit_gesture(41).unwrap(),
            CommitOutcome::Updated(id("resizing"))
        );
        assert_eq!(document.history_depths(), (2, 0));

        assert_eq!(
            document.begin_resize(42, 0, point(0.0, 50.0), 3.0).unwrap(),
            Some(id("resizing"))
        );
        let preview = document.update_gesture(42, point(120.0, 50.0)).unwrap();
        assert_eq!(
            preview.annotation.rect,
            PdfRect::new(108.0, 20.0, 2.0, 60.0).unwrap()
        );
    }

    #[test]
    fn rotation_and_rotated_resize_match_the_electron_rectangle_contract() {
        let mut document = AnnotationDocument::default();
        document
            .begin_create(
                42,
                id("rotating"),
                0,
                point(10.0, 10.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(42, point(110.0, 60.0)).unwrap();
        document.commit_gesture(42).unwrap();

        assert_eq!(
            document.hit_test(0, point(60.0, 72.0), 3.0).unwrap(),
            Some(HitTarget::RotationHandle(id("rotating")))
        );
        assert_eq!(
            document
                .begin_rotation(43, 0, point(60.0, 72.0), 3.0)
                .unwrap(),
            Some(id("rotating"))
        );
        let preview = document.update_gesture(43, point(97.0, 35.0)).unwrap();
        assert_eq!(
            preview.annotation.rect,
            PdfRect::new(10.0, 10.0, 100.0, 50.0).unwrap()
        );
        assert_eq!(preview.annotation.rotation_degrees, 90.0);
        assert_eq!(
            document.commit_gesture(43).unwrap(),
            CommitOutcome::Updated(id("rotating"))
        );
        assert_eq!(
            document.canonical_json_snapshot()["markups"][0]["rotation"],
            json!(90.0)
        );

        assert_eq!(
            document.hit_test(0, point(60.0, -15.0), 3.0).unwrap(),
            Some(HitTarget::ResizeHandle {
                id: id("rotating"),
                handle: RectangleResizeHandle::East,
            })
        );
        document
            .begin_resize(44, 0, point(60.0, -15.0), 3.0)
            .unwrap();
        let preview = document.update_gesture(44, point(60.0, -35.0)).unwrap();
        assert_eq!(
            preview.annotation.rect,
            PdfRect::new(0.0, 0.0, 120.0, 50.0).unwrap()
        );
        assert_eq!(preview.annotation.rotation_degrees, 90.0);
    }

    #[test]
    fn rotated_rectangle_body_is_selectable_outside_its_unrotated_bounds() {
        let mut document = AnnotationDocument::default();
        document
            .begin_create(
                45,
                id("rotated-hit-target"),
                0,
                point(10.0, 10.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(45, point(110.0, 60.0)).unwrap();
        document.commit_gesture(45).unwrap();
        document
            .begin_rotation(46, 0, point(60.0, 72.0), 3.0)
            .unwrap();
        document.update_gesture(46, point(97.0, 35.0)).unwrap();
        document.commit_gesture(46).unwrap();
        document.clear_selection();

        assert_eq!(
            document.hit_test(0, point(60.0, -10.0), 2.0).unwrap(),
            Some(HitTarget::Body(id("rotated-hit-target")))
        );
    }

    #[test]
    fn appearance_is_normalized_and_is_one_undoable_mutation() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "styled");
        let styled = RectangleAppearance::new("#123ABC", 3.25, Some("#ABCDEF"), 0.35).unwrap();
        assert!(document.set_selected_appearance(styled.clone()).unwrap());
        assert!(!document.set_selected_appearance(styled.clone()).unwrap());

        assert_eq!(document.history_depths(), (2, 0));
        assert_eq!(
            document.rectangles()[0].appearance.stroke_color(),
            "#123abc"
        );
        assert_eq!(
            document.rectangles()[0].appearance.fill_color(),
            Some("#abcdef")
        );
        assert_eq!(document.rectangles()[0].appearance.stroke_width_pt(), 3.25);
        assert_eq!(document.rectangles()[0].appearance.opacity(), 0.35);
        assert_eq!(document.rectangles()[0].appearance.fill_opacity(), 1.0);

        let separate_alpha = styled.clone().with_fill_opacity(0.12).unwrap();
        assert_eq!(separate_alpha.opacity(), 0.35);
        assert_eq!(separate_alpha.fill_opacity(), 0.12);

        assert!(document.undo().unwrap());
        assert_eq!(
            document.rectangles()[0].appearance,
            RectangleAppearance::default()
        );
        assert!(document.redo().unwrap());
        assert_eq!(document.rectangles()[0].appearance, styled);
    }

    #[test]
    fn undo_redo_reconcile_selection_and_new_edits_clear_future() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "history");
        assert!(document.undo().unwrap());
        assert!(document.rectangles().is_empty());
        assert_eq!(document.selected_id(), None);
        assert_eq!(document.history_depths(), (0, 1));

        assert!(document.redo().unwrap());
        assert_eq!(document.rectangles().len(), 1);
        assert_eq!(document.selected_id(), None);
        document.select(&id("history"));
        document
            .set_selected_appearance(
                RectangleAppearance::new("#000000", 2.0, None::<String>, 1.0).unwrap(),
            )
            .unwrap();
        assert_eq!(document.history_depths(), (2, 0));
        assert!(!document.redo().unwrap());
    }

    #[test]
    fn bounded_history_drops_oldest_states() {
        let mut document = AnnotationDocument::with_history_limit(2).unwrap();
        create_rectangle(&mut document, "bounded");
        for color in ["#111111", "#222222", "#333333"] {
            document
                .set_selected_appearance(
                    RectangleAppearance::new(color, 1.0, None::<String>, 1.0).unwrap(),
                )
                .unwrap();
        }
        assert_eq!(document.history_depths(), (2, 0));
        assert!(document.undo().unwrap());
        assert!(document.undo().unwrap());
        assert!(!document.undo().unwrap());
        assert_eq!(
            document.rectangles()[0].appearance.stroke_color(),
            "#111111"
        );
    }

    #[test]
    fn pointer_mismatch_and_active_gesture_fail_without_mutation() {
        let mut document = AnnotationDocument::default();
        document
            .begin_create(
                71,
                id("pointer"),
                0,
                point(0.0, 0.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        assert_eq!(
            document.update_gesture(72, point(10.0, 10.0)),
            Err(AnnotationError::PointerMismatch {
                expected: 71,
                received: 72,
            })
        );
        assert_eq!(
            document.commit_gesture(72),
            Err(AnnotationError::PointerMismatch {
                expected: 71,
                received: 72,
            })
        );
        assert!(document.active_preview().is_some());
        assert_eq!(document.undo(), Err(AnnotationError::ActiveGesture));
        document.cancel_gesture(71).unwrap();
        assert!(document.rectangles().is_empty());
    }

    #[test]
    fn rectangle_property_edits_commit_typed_geometry_rotation_and_lock_history() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "properties");
        let target_rect = PdfRect::new(12.0, 24.0, 80.0, 40.0).unwrap();

        assert_eq!(
            document
                .apply_command(AnnotationCommand::EditAnnotation {
                    id: id("properties"),
                    edit: AnnotationEdit::SetRectangleRect(target_rect),
                })
                .unwrap(),
            CommandOutcome::AnnotationEdited {
                id: id("properties"),
                kind: AnnotationKind::Rectangle,
                changed: true,
                revision: 2,
            }
        );
        assert_eq!(document.rectangles()[0].rect, target_rect);

        assert_eq!(
            document
                .apply_command(AnnotationCommand::EditAnnotation {
                    id: id("properties"),
                    edit: AnnotationEdit::SetRectangleRotation(375.0),
                })
                .unwrap(),
            CommandOutcome::AnnotationEdited {
                id: id("properties"),
                kind: AnnotationKind::Rectangle,
                changed: true,
                revision: 3,
            }
        );
        assert_eq!(document.rectangles()[0].rotation_degrees, 15.0);

        assert_eq!(
            document
                .apply_command(AnnotationCommand::EditAnnotation {
                    id: id("properties"),
                    edit: AnnotationEdit::SetRectangleRotation(15.0),
                })
                .unwrap(),
            CommandOutcome::AnnotationEdited {
                id: id("properties"),
                kind: AnnotationKind::Rectangle,
                changed: false,
                revision: 3,
            }
        );
        assert_eq!(document.history_depths(), (3, 0));

        let zero_width_rect = PdfRect::new(12.0, 24.0, 0.0, 40.0).unwrap();
        assert_eq!(
            document
                .apply_command(AnnotationCommand::EditAnnotation {
                    id: id("properties"),
                    edit: AnnotationEdit::SetRectangleRect(zero_width_rect),
                })
                .unwrap(),
            CommandOutcome::AnnotationEdited {
                id: id("properties"),
                kind: AnnotationKind::Rectangle,
                changed: true,
                revision: 4,
            }
        );
        assert_eq!(document.rectangles()[0].rect, zero_width_rect);
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("properties"),
                edit: AnnotationEdit::SetRectangleRect(target_rect),
            })
            .unwrap();

        document
            .apply_command(AnnotationCommand::SetLocked {
                id: id("properties"),
                locked: true,
            })
            .unwrap();
        assert_eq!(
            document.apply_command(AnnotationCommand::EditAnnotation {
                id: id("properties"),
                edit: AnnotationEdit::SetRectangleRotation(45.0),
            }),
            Err(AnnotationError::LockedMarkup(id("properties")))
        );
        assert_eq!(document.rectangles()[0].rotation_degrees, 15.0);

        document.apply_command(AnnotationCommand::Undo).unwrap();
        assert!(!document.rectangles()[0].locked);
        assert_eq!(document.rectangles()[0].rect, target_rect);
        assert_eq!(document.rectangles()[0].rotation_degrees, 15.0);
        document.apply_command(AnnotationCommand::Undo).unwrap();
        assert_eq!(document.rectangles()[0].rect, zero_width_rect);
        document.apply_command(AnnotationCommand::Redo).unwrap();
        assert_eq!(document.rectangles()[0].rect, target_rect);
    }

    #[test]
    fn canonical_json_is_stable_and_excludes_active_preview() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "canonical");
        document
            .set_selected_appearance(
                RectangleAppearance::new("#123456", 3.25, Some("#abcdef"), 0.35).unwrap(),
            )
            .unwrap();
        let committed = document.canonical_json_string();
        document
            .begin_east_resize(81, 0, point(110.0, 45.0), 3.0)
            .unwrap();
        document.update_gesture(81, point(182.0, 45.0)).unwrap();

        assert_eq!(document.canonical_json_string(), committed);
        assert_eq!(
            committed,
            r##"{"markups":[{"appearance":{"fill":{"color":"#abcdef"},"fillOpacity":1.0,"opacity":0.35,"stroke":{"color":"#123456","widthPt":3.25}},"id":"canonical","kind":"rectangle","pageIndex":0,"rect":{"height":50.0,"width":100.0,"x":10.0,"y":20.0}}],"schema_version":1,"selection":"canonical"}"##
        );
    }

    #[test]
    fn invalid_inputs_fail_closed() {
        assert!(PdfPoint::new(f64::NAN, 0.0).is_err());
        assert!(PdfRect::new(0.0, 0.0, -1.0, 1.0).is_err());
        assert!(RectangleAppearance::new("red", 1.0, None::<String>, 1.0).is_err());
        assert!(RectangleAppearance::new("#ff0000", -1.0, None::<String>, 1.0).is_err());
        assert!(RectangleAppearance::new("#ff0000", 1.0, None::<String>, 1.1).is_err());
        assert!(AnnotationDocument::with_history_limit(0).is_err());

        let document = AnnotationDocument::default();
        assert_eq!(
            document.hit_test(0, point(0.0, 0.0), f64::INFINITY),
            Err(AnnotationError::InvalidTolerance)
        );
    }

    #[test]
    fn capture_loss_cancels_preview_without_committing_or_dirtying() {
        let mut document = AnnotationDocument::default();
        assert_eq!(
            document
                .apply_command(AnnotationCommand::PointerDown {
                    pointer_id: 91,
                    page_index: 0,
                    point: point(10.0, 20.0),
                    tolerance_pt: 2.0,
                    tool: PointerTool::Rectangle {
                        id: id("capture-loss"),
                        appearance: RectangleAppearance::default(),
                    },
                })
                .unwrap(),
            CommandOutcome::GestureStarted {
                kind: GestureKind::Create,
                id: id("capture-loss"),
            }
        );
        document
            .apply_command(AnnotationCommand::PointerMove {
                pointer_id: 91,
                point: point(110.0, 70.0),
            })
            .unwrap();

        assert_eq!(
            document
                .apply_command(AnnotationCommand::PointerCancel {
                    pointer_id: 91,
                    reason: PointerCancelReason::CaptureLost,
                })
                .unwrap(),
            CommandOutcome::GestureCancelled {
                reason: PointerCancelReason::CaptureLost,
            }
        );
        assert!(document.rectangles().is_empty());
        assert!(!document.snapshot().dirty);
    }

    #[test]
    fn save_undo_and_redo_report_revisions_and_dirty_state() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "revisioned");
        assert_eq!(document.snapshot().revision, 1);
        assert!(document.snapshot().dirty);

        assert_eq!(
            document
                .apply_command(AnnotationCommand::MarkSaved)
                .unwrap(),
            CommandOutcome::Saved { revision: 1 }
        );
        assert!(!document.snapshot().dirty);
        assert_eq!(document.snapshot().saved_revision, 1);

        document
            .apply_command(AnnotationCommand::SetSelectedAppearance(
                RectangleAppearance::new("#123456", 2.0, Some("#abcdef"), 0.4).unwrap(),
            ))
            .unwrap();
        assert_eq!(document.snapshot().revision, 2);
        assert!(document.snapshot().dirty);
        assert_eq!(
            document.apply_command(AnnotationCommand::Undo).unwrap(),
            CommandOutcome::HistoryChanged {
                direction: HistoryDirection::Undo,
                changed: true,
                revision: 1,
            }
        );
        assert!(!document.snapshot().dirty);
        assert_eq!(
            document.apply_command(AnnotationCommand::Redo).unwrap(),
            CommandOutcome::HistoryChanged {
                direction: HistoryDirection::Redo,
                changed: true,
                revision: 2,
            }
        );
        assert!(document.snapshot().dirty);
    }

    #[test]
    fn locked_rectangle_rejects_edit_and_delete_until_unlocked() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "locked");
        assert_eq!(
            document
                .apply_command(AnnotationCommand::SetLocked {
                    id: id("locked"),
                    locked: true,
                })
                .unwrap(),
            CommandOutcome::LockChanged {
                id: id("locked"),
                locked: true,
                changed: true,
                revision: 2,
            }
        );
        assert_eq!(
            document.apply_command(AnnotationCommand::DeleteSelected),
            Err(AnnotationError::LockedMarkup(id("locked")))
        );
        assert_eq!(
            document.apply_command(AnnotationCommand::PointerDown {
                pointer_id: 73,
                page_index: 0,
                point: point(10.0, 40.0),
                tolerance_pt: 2.0,
                tool: PointerTool::Select {
                    rotation_handle_offset_pt: ROTATION_HANDLE_OFFSET_PT,
                },
            }),
            Err(AnnotationError::LockedMarkup(id("locked")))
        );
        assert_eq!(
            document.apply_command(AnnotationCommand::SetSelectedAppearance(
                RectangleAppearance::new("#000000", 2.0, None::<String>, 1.0).unwrap(),
            )),
            Err(AnnotationError::LockedMarkup(id("locked")))
        );

        document
            .apply_command(AnnotationCommand::SetLocked {
                id: id("locked"),
                locked: false,
            })
            .unwrap();
        assert_eq!(
            document
                .apply_command(AnnotationCommand::DeleteSelected)
                .unwrap(),
            CommandOutcome::Deleted {
                id: id("locked"),
                revision: 4,
            }
        );
        assert!(document.rectangles().is_empty());
    }

    #[test]
    fn empty_history_commands_report_unchanged_outcomes() {
        let mut document = AnnotationDocument::default();
        assert_eq!(
            document.apply_command(AnnotationCommand::Undo).unwrap(),
            CommandOutcome::HistoryChanged {
                direction: HistoryDirection::Undo,
                changed: false,
                revision: 0,
            }
        );
        assert_eq!(
            document.apply_command(AnnotationCommand::Redo).unwrap(),
            CommandOutcome::HistoryChanged {
                direction: HistoryDirection::Redo,
                changed: false,
                revision: 0,
            }
        );
    }

    #[test]
    fn thumbnail_scene_projects_committed_page_geometry_without_editor_chrome() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "thumbnail");
        document.begin_move(55, 0, point(10.0, 40.0), 2.0).unwrap();
        document.update_gesture(55, point(30.0, 60.0)).unwrap();

        assert_eq!(
            document.thumbnail_scene(0),
            AnnotationScene {
                page_index: 0,
                revision: 1,
                rectangles: vec![SceneRectangle {
                    id: id("thumbnail"),
                    rect: PdfRect::new(10.0, 20.0, 100.0, 50.0).unwrap(),
                    rotation_degrees: 0.0,
                    appearance: RectangleAppearance::default(),
                    selected: false,
                    locked: false,
                    preview: false,
                }],
                redacts: vec![],
                ellipses: vec![],
                arcs: vec![],
                straight_lines: vec![],
                vertex_paths: vec![],
                clouds: vec![],
                cloud_pluses: vec![],
                callouts: vec![],
                measurement_paths: vec![],
                pens: vec![],
                text_boxes: vec![],
                dimensions: vec![],
                lengths: vec![],
                images: vec![],
                snapshots: vec![],
            }
        );
    }

    #[test]
    fn tracked_rectangle_manifest_replays_to_its_exact_canonical_oracle() {
        let mut document = AnnotationDocument::default();
        let replay = document
            .replay_rectangle_manifest(include_str!(
                "../../performance/fixtures/bp-rectangle-v1.fixture.json"
            ))
            .unwrap();

        assert_eq!(replay.fixture_id, "bp-rectangle-v1");
        assert_eq!(
            replay.canonical_sha256,
            "935fce671f16c98104012ac386e3089dab509c6e577d6e3166aa28a27142eba9"
        );
        assert_eq!(document.history_depths(), (4, 0));
        assert_eq!(document.snapshot().revision, 4);
        assert!(document.snapshot().dirty);
        assert_eq!(document.rectangles().len(), 1);
        assert_eq!(
            document.rectangles()[0].rect,
            PdfRect::new(90.0, 132.0, 210.0, 96.0).unwrap()
        );
        assert_eq!(
            document.rectangles()[0].appearance.stroke_color(),
            "#dc2626"
        );
        assert_eq!(
            document.rectangles()[0].appearance.stroke_style(),
            StrokeStyle::Dashed
        );
    }

    #[test]
    fn rectangle_manifest_replay_rejects_command_stream_drift() {
        let manifest = include_str!("../../performance/fixtures/bp-rectangle-v1.fixture.json")
            .replacen("rectangle:create:001", "rectangle:create:drift", 1);
        let mut document = AnnotationDocument::default();

        assert!(matches!(
            document.replay_rectangle_manifest(&manifest),
            Err(AnnotationError::InvalidFixture(message))
                if message.contains("command stream hash")
        ));
        assert!(document.rectangles().is_empty());
    }

    #[test]
    fn page_transform_round_trips_pdf_geometry_at_zoom() {
        let transform = PageTransform::new(792.0, 1.5).unwrap();
        assert_eq!(
            transform.point_from_local_pixels(108.0, 216.0).unwrap(),
            point(72.0, 648.0)
        );
        assert_eq!(
            transform.rect_to_local_pixels(PdfRect::new(72.0, 576.0, 144.0, 72.0).unwrap()),
            PdfRect::new(108.0, 216.0, 216.0, 108.0).unwrap()
        );
        assert_eq!(transform.tolerance_points(9.0).unwrap(), 6.0);
    }

    #[test]
    fn marquee_selection_uses_document_order_and_never_mutates_history() {
        let mut document = AnnotationDocument::default();
        let rectangle_id = id("marquee:rectangle");
        let line_id = id("marquee:line");
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Rectangle(
                RectangleAnnotation {
                    id: rectangle_id.clone(),
                    page_index: 0,
                    rect: PdfRect::new(10., 10., 20., 20.).unwrap(),
                    rotation_degrees: 0.,
                    appearance: RectangleAppearance::default(),
                    locked: false,
                },
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(
                Annotation::StraightLine(
                    StraightLineAnnotation::new(
                        line_id.clone(),
                        0,
                        point(10., 50.),
                        point(100., 50.),
                        LineKind::Line,
                        StraightLineAppearance::default_for(LineKind::Line),
                    )
                    .unwrap(),
                ),
            ))
            .unwrap();
        let before = document.snapshot();
        let identity = |point: PdfPoint| SelectionPoint::new(point.x, point.y);

        let mut window = SelectionMarquee::armed_box(
            SelectionPoint::new(0., 0.),
            crate::selection_geometry::SelectionOperation::Replace,
        );
        window.update(SelectionPoint::new(35., 35.));
        assert_eq!(
            document.apply_marquee_selection(0, &window, identity),
            &[rectangle_id.clone()]
        );

        let mut crossing = SelectionMarquee::armed_box(
            SelectionPoint::new(50., 60.),
            crate::selection_geometry::SelectionOperation::Add,
        );
        crossing.update(SelectionPoint::new(0., 40.));
        assert_eq!(
            document.apply_marquee_selection(0, &crossing, identity),
            &[rectangle_id.clone(), line_id.clone()]
        );

        let mut remove = SelectionMarquee::armed_box(
            SelectionPoint::new(0., 0.),
            crate::selection_geometry::SelectionOperation::Remove,
        );
        remove.update(SelectionPoint::new(35., 35.));
        assert_eq!(
            document.apply_marquee_selection(0, &remove, identity),
            &[line_id]
        );
        let after = document.snapshot();
        assert_eq!(
            (after.revision, after.undo_depth, after.redo_depth),
            (before.revision, before.undo_depth, before.redo_depth)
        );
    }

    #[test]
    fn snapshot_preserves_stable_order_across_import_insert_delete_and_undo() {
        let line_id = id("order:line");
        let rectangle_id = id("order:rectangle");
        let text_id = id("order:text");
        let line = Annotation::StraightLine(
            StraightLineAnnotation::new(
                line_id.clone(),
                0,
                point(10., 10.),
                point(30., 30.),
                LineKind::Line,
                StraightLineAppearance::default_for(LineKind::Line),
            )
            .unwrap(),
        );
        let rectangle = Annotation::Rectangle(RectangleAnnotation {
            id: rectangle_id.clone(),
            page_index: 0,
            rect: PdfRect::new(40., 40., 20., 20.).unwrap(),
            rotation_degrees: 0.,
            appearance: RectangleAppearance::default(),
            locked: false,
        });
        let text = Annotation::TextBox(
            TextBoxAnnotation::new(
                text_id.clone(),
                0,
                PdfRect::new(70., 70., 30., 20.).unwrap(),
                "Order",
                TextBoxStyle::new("Helvetica", 12., "#111827", 1.).unwrap(),
            )
            .unwrap(),
        );
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(vec![line, rectangle], Vec::new())
            .unwrap();
        assert_eq!(
            document.snapshot().annotation_order,
            vec![line_id.clone(), rectangle_id.clone()]
        );
        document.insert_annotations(vec![text]).unwrap();
        assert_eq!(
            document.snapshot().annotation_order,
            vec![line_id.clone(), rectangle_id.clone(), text_id.clone()]
        );
        assert!(document.select(&rectangle_id));
        document
            .apply_command(AnnotationCommand::DeleteSelected)
            .unwrap();
        assert_eq!(
            document.snapshot().annotation_order,
            vec![line_id.clone(), text_id.clone()]
        );
        document.undo().unwrap();
        assert_eq!(
            document.snapshot().annotation_order,
            vec![line_id, rectangle_id, text_id]
        );
    }

    #[test]
    fn page_scale_contract_preserves_presets_axes_precision_targets_and_atomic_history() {
        assert_eq!(
            built_in_scale_presets()
                .iter()
                .map(|preset| preset.name.as_str())
                .collect::<Vec<_>>(),
            [
                "1:1", "1:2", "1:5", "1:10", "1:20", "1:50", "1:100", "1:200", "1:500", "1:1000",
            ]
        );
        assert!(built_in_scale_presets().iter().all(|preset| {
            preset.pdf_units == ScaleUnit::Cm
                && preset.real_units == ScaleUnit::M
                && preset.built_in
        }));

        let custom = PageScale::custom(
            0,
            "1 in = 2 ft",
            ScaleUnit::In,
            ScaleUnit::Ft,
            1.,
            2.,
            Some((2., 9.)),
            ScalePrecision::fraction(16).unwrap(),
        )
        .unwrap();
        assert!((custom.scale_x - (2. / 72.)).abs() < 0.000_001);
        assert!((custom.scale_y - (9. / 144.)).abs() < 0.000_001);
        assert_eq!(custom.precision, ScalePrecision::fraction(16).unwrap());

        let calibrated = PageScale::calibrated(
            2,
            point(0., 0.),
            point(25., 0.),
            100.,
            ScaleUnit::Ft,
            ScalePrecision::decimal(0.01).unwrap(),
        )
        .unwrap();
        assert_eq!(calibrated.source, ScaleSource::Calibrated);
        assert_eq!(calibrated.name, "Calibrated 100 ft");
        assert_eq!(calibrated.scale_x, 4.);
        assert_eq!(calibrated.scale_y, 4.);

        assert_eq!(
            parse_page_scale_ranges("1-3, 5, 9", 10).unwrap(),
            vec![
                PageScaleRange::new(0, 2),
                PageScaleRange::new(4, 4),
                PageScaleRange::new(8, 8),
            ]
        );
        assert_eq!(
            parse_page_scale_ranges("1-a", 10).unwrap_err().to_string(),
            "Enter page ranges like 1-3, 5, 9."
        );

        let original = LengthCalibration::from_scale(72., 1., "m", 2, false)
            .unwrap()
            .with_label("Span")
            .unwrap();
        let length = LengthAnnotation::new(
            id("page-scale:length"),
            1,
            point(0., 0.),
            point(0., 72.),
            original,
        )
        .unwrap();
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(vec![Annotation::Length(length)], Vec::new())
            .unwrap();
        let applied = custom.with_page_index(1);
        document
            .apply_page_scale(
                applied.clone(),
                PageScaleApplyTarget::Ranges(vec![PageScaleRange::new(1, 2)]),
                3,
            )
            .unwrap();
        let snapshot = document.snapshot();
        assert_eq!((snapshot.revision, snapshot.undo_depth), (1, 1));
        assert_eq!(
            snapshot.page_scales,
            vec![applied.with_page_index(1), applied.with_page_index(2)]
        );
        assert_eq!(snapshot.lengths[0].calibration().label(), "Span");
        assert!(!snapshot.lengths[0].calibration().show_caption());
        assert_eq!(snapshot.lengths[0].caption(), "Span: 4 8/16 ft");
        document.undo().unwrap();
        assert!(document.snapshot().page_scales.is_empty());
        document.redo().unwrap();
        assert_eq!(document.snapshot().page_scales.len(), 2);

        let saved = ScalePreset {
            id: "scale-test".into(),
            name: custom.name.clone(),
            pdf_units: custom.pdf_units,
            real_units: custom.real_units,
            scale_x: custom.scale_x,
            scale_y: custom.scale_y,
            source: custom.source,
            built_in: false,
        };
        document
            .apply_page_scale_with_preset(
                custom.clone(),
                PageScaleApplyTarget::Current(0),
                3,
                Some(saved.clone()),
            )
            .unwrap();
        let with_preset = document.snapshot();
        assert_eq!(with_preset.scale_presets, vec![saved.clone()]);
        assert_eq!(with_preset.revision, 2);
        assert_eq!(with_preset.undo_depth, 2);
        document.undo().unwrap();
        assert!(document.snapshot().scale_presets.is_empty());
        document.redo().unwrap();
        assert_eq!(document.snapshot().scale_presets, vec![saved.clone()]);
        assert!(document.delete_scale_preset("scale-test").unwrap());
        assert!(document.snapshot().scale_presets.is_empty());
        assert_eq!(document.snapshot().revision, 3);
        assert!(!document.delete_scale_preset("scale-test").unwrap());
        assert_eq!(
            document
                .delete_scale_preset("one-to-1")
                .unwrap_err()
                .to_string(),
            "invalid geometry: Built-in scale presets cannot be deleted."
        );
    }

    #[test]
    fn measurement_path_contract_keeps_scaled_polylength_and_area_distinct() {
        let scale = PageScale::from_factors(
            0,
            ScaleSource::Custom,
            "anisotropic test scale",
            ScaleUnit::In,
            ScaleUnit::Ft,
            0.5,
            0.25,
            ScalePrecision::decimal(0.01).unwrap(),
        )
        .unwrap();
        let calibration = LengthCalibration::from_page_scale(&scale).unwrap();
        let appearance = RectangleAppearance::new("#ff0000", 1.0, None::<String>, 1.0).unwrap();

        let polylength = MeasurementPathAnnotation::new(
            id("measurement:polylength"),
            0,
            vec![point(0., 0.), point(4., 0.), point(4., 8.)],
            MeasurementPathKind::Polylength,
            calibration.clone(),
            appearance.clone(),
        )
        .unwrap();
        let area = MeasurementPathAnnotation::new(
            id("measurement:area"),
            0,
            vec![point(0., 0.), point(4., 0.), point(4., 8.)],
            MeasurementPathKind::Area,
            calibration,
            appearance,
        )
        .unwrap();

        assert_eq!(polylength.kind, MeasurementPathKind::Polylength);
        assert_eq!(area.kind, MeasurementPathKind::Area);
        assert!(!polylength.kind.is_closed());
        assert!(area.kind.is_closed());
        assert_eq!(polylength.measured_value(), 4.0);
        assert_eq!(area.measured_value(), 2.0);
        assert_eq!(polylength.caption(), "4.00 ft");
        assert_eq!(area.caption(), "2.00 ft^2");
        assert_eq!(polylength.points().len(), 3);
        assert_eq!(area.points().len(), 3);

        assert!(
            MeasurementPathAnnotation::new(
                id("measurement:invalid-area"),
                0,
                vec![point(0., 0.), point(4., 0.)],
                MeasurementPathKind::Area,
                LengthCalibration::from_page_scale(&scale).unwrap(),
                RectangleAppearance::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn cloud_annotation_contract_preserves_intensity_identity_and_vertex_edits() {
        let cloud = CloudAnnotation::new(
            id("cloud:contract"),
            0,
            vec![
                point(10., 10.),
                point(90., 10.),
                point(90., 70.),
                point(10., 70.),
            ],
            2.,
            RectangleAppearance::default(),
        )
        .unwrap();
        assert_eq!(cloud.id.as_str(), "cloud:contract");
        assert_eq!(cloud.border_effect_intensity(), 2.);
        assert_eq!(cloud.points().len(), 4);
        assert!(cloud.scallop_path().len() > cloud.points().len());
        assert_eq!(cloud.scallop_path().first(), cloud.scallop_path().last());
        assert!(
            CloudAnnotation::new(
                id("cloud:too-few"),
                0,
                vec![point(0., 0.), point(10., 0.)],
                2.,
                RectangleAppearance::default(),
            )
            .is_err()
        );
        assert!(
            CloudAnnotation::new(
                id("cloud:bad-intensity"),
                0,
                vec![point(0., 0.), point(10., 0.), point(10., 10.)],
                4.25,
                RectangleAppearance::default(),
            )
            .is_err()
        );

        let cloud_id = cloud.id.clone();
        let mut document = AnnotationDocument::default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Cloud(
                cloud,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: cloud_id.clone(),
                edit: AnnotationEdit::SetCloudPoint {
                    vertex_index: 1,
                    point: point(100., 20.),
                },
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: cloud_id.clone(),
                edit: AnnotationEdit::TranslateCloud {
                    delta_x: 5.,
                    delta_y: -5.,
                },
            })
            .unwrap();

        let snapshot = document.snapshot();
        assert_eq!(snapshot.clouds.len(), 1);
        assert_eq!(snapshot.clouds[0].id, cloud_id);
        assert_eq!(snapshot.clouds[0].points()[1], point(105., 15.));
        assert_eq!(snapshot.annotation_order, vec![id("cloud:contract")]);
        assert_eq!(snapshot.revision, 3);
        assert_eq!(snapshot.undo_depth, 3);
    }

    #[test]
    fn callout_annotation_contract_preserves_composite_identity_and_independent_edits() {
        let mut document = AnnotationDocument::default();
        let callout_id = id("callout:contract");
        let appearance = CalloutAppearance::new(
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap();
        let callout = CalloutAnnotation::new(
            callout_id.clone(),
            0,
            vec![point(20., 20.), point(60., 80.), point(100., 80.)],
            PdfRect::new(100., 58., 150., 44.).unwrap(),
            "Callout",
            appearance,
        )
        .unwrap();

        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Callout(
                callout,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: callout_id.clone(),
                edit: AnnotationEdit::SetCalloutLeaderPoint {
                    point_index: 0,
                    point: point(30., 25.),
                },
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: callout_id.clone(),
                edit: AnnotationEdit::TranslateCalloutTextBox {
                    delta_x: 10.,
                    delta_y: -5.,
                },
            })
            .unwrap();

        let snapshot = document.snapshot();
        assert_eq!(snapshot.callouts.len(), 1);
        let retained = &snapshot.callouts[0];
        assert_eq!(retained.id, callout_id);
        assert_eq!(retained.leader_points()[0], point(30., 25.));
        assert_eq!(retained.leader_points()[1], point(60., 80.));
        assert_eq!(retained.leader_points()[2], point(110., 75.));
        assert_eq!(
            retained.text_box,
            PdfRect::new(110., 53., 150., 44.).unwrap()
        );
        assert_eq!(retained.content(), "Callout");
        assert_eq!(snapshot.annotation_order, vec![id("callout:contract")]);
        assert_eq!(snapshot.revision, 3);
        assert_eq!(snapshot.undo_depth, 3);
    }

    #[test]
    fn dimension_annotation_keeps_one_identity_across_caption_geometry_and_history() {
        let mut document = AnnotationDocument::default();
        let dimension_id = id("dimension:contract");
        let appearance = DimensionAppearance::new(
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap();
        let dimension = DimensionAnnotation::new(
            dimension_id.clone(),
            0,
            point(20., 50.),
            point(120., 50.),
            24.,
            "Dimension",
            appearance,
        )
        .unwrap();

        assert_eq!(
            DimensionAnnotation::default_offset(point(20., 50.), point(120., 50.)),
            24.
        );
        assert_eq!(
            DimensionAnnotation::default_offset(point(120., 50.), point(20., 50.)),
            -24.
        );

        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Dimension(
                dimension,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: dimension_id.clone(),
                edit: AnnotationEdit::SetDimensionEndpoint {
                    endpoint: LineEndpoint::End,
                    point: point(130., 55.),
                },
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: dimension_id.clone(),
                edit: AnnotationEdit::SetDimensionOffset(40.),
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: dimension_id.clone(),
                edit: AnnotationEdit::SetDimensionContent("Door opening".into()),
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: dimension_id.clone(),
                edit: AnnotationEdit::TranslateDimension {
                    delta_x: 5.,
                    delta_y: -2.,
                },
            })
            .unwrap();

        let snapshot = document.snapshot();
        assert_eq!(snapshot.dimensions.len(), 1);
        let retained = &snapshot.dimensions[0];
        assert_eq!(retained.id, dimension_id);
        assert_eq!(retained.start, point(25., 48.));
        assert_eq!(retained.end, point(135., 53.));
        assert_eq!(retained.dimension_line_offset(), 40.);
        assert_eq!(retained.content(), "Door opening");
        assert_eq!(snapshot.annotation_order, vec![id("dimension:contract")]);
        assert_eq!(snapshot.revision, 5);
        assert_eq!(snapshot.undo_depth, 5);
    }

    #[test]
    fn cloud_plus_aggregate_keeps_one_identity_for_cloud_leader_text_and_history() {
        let mut document = AnnotationDocument::default();
        let cloud_plus_id = id("cloud-plus:contract");
        let appearance = CloudPlusAppearance::new(
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap();
        let cloud_plus = CloudPlusAnnotation::new(
            cloud_plus_id.clone(),
            0,
            vec![
                point(10., 10.),
                point(90., 10.),
                point(90., 70.),
                point(10., 70.),
            ],
            2.,
            vec![point(90., 40.), point(110., 70.), point(130., 70.)],
            PdfRect::new(130., 48., 150., 44.).unwrap(),
            "Cloud+",
            appearance.clone(),
        )
        .unwrap();

        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::CloudPlus(
                cloud_plus,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: cloud_plus_id.clone(),
                edit: AnnotationEdit::SetCloudPlusCloudPoint {
                    vertex_index: 1,
                    point: point(100., 15.),
                    leader_points: vec![point(100., 42.), point(115., 70.), point(130., 70.)],
                },
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: cloud_plus_id.clone(),
                edit: AnnotationEdit::SetCloudPlusTextBox {
                    text_box: PdfRect::new(150., 58., 150., 44.).unwrap(),
                    leader_points: vec![point(100., 42.), point(125., 80.), point(150., 80.)],
                },
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: cloud_plus_id.clone(),
                edit: AnnotationEdit::SetCloudPlusContent("Composite note".into()),
            })
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: cloud_plus_id.clone(),
                edit: AnnotationEdit::TranslateCloudPlusGroup {
                    delta_x: 5.,
                    delta_y: -5.,
                },
            })
            .unwrap();

        let snapshot = document.snapshot();
        assert_eq!(snapshot.cloud_pluses.len(), 1);
        assert!(snapshot.clouds.is_empty());
        assert!(snapshot.callouts.is_empty());
        assert_eq!(snapshot.annotation_order, vec![cloud_plus_id.clone()]);
        let retained = &snapshot.cloud_pluses[0];
        assert_eq!(retained.id, cloud_plus_id);
        assert_eq!(retained.cloud_points()[1], point(105., 10.));
        assert_eq!(retained.leader_points()[0], point(105., 37.));
        assert_eq!(
            retained.text_box,
            PdfRect::new(155., 53., 150., 44.).unwrap()
        );
        assert_eq!(retained.content(), "Composite note");
        assert_eq!(snapshot.revision, 5);
        assert_eq!(snapshot.undo_depth, 5);

        let inline = CloudPlusAnnotation::new(
            id("cloud-plus:inline"),
            0,
            vec![
                point(10., 10.),
                point(200., 10.),
                point(200., 100.),
                point(10., 100.),
            ],
            2.,
            Vec::new(),
            PdfRect::new(40., 30., 100., 44.).unwrap(),
            "Inline",
            appearance,
        )
        .unwrap();
        assert!(inline.leader_points().is_empty());
    }

    #[test]
    fn many_preview_updates_commit_one_stable_semantic_result() {
        const SAMPLES: u32 = 360;
        let mut document = AnnotationDocument::default();
        let rectangle_id = id("perf-rectangle-1");

        document
            .begin_create(
                1,
                rectangle_id.clone(),
                0,
                point(72.0, 576.0),
                RectangleAppearance::default(),
            )
            .unwrap();
        for sample in 1..=SAMPLES {
            let progress = f64::from(sample) / f64::from(SAMPLES);
            document
                .update_gesture(1, point(72.0 + 144.0 * progress, 576.0 + 72.0 * progress))
                .unwrap();
        }
        document.commit_gesture(1).unwrap();

        document.begin_move(1, 0, point(72.0, 600.0), 4.0).unwrap();
        for sample in 1..=SAMPLES {
            let progress = f64::from(sample) / f64::from(SAMPLES);
            document
                .update_gesture(1, point(72.0 + 36.0 * progress, 600.0 - 24.0 * progress))
                .unwrap();
        }
        document.commit_gesture(1).unwrap();

        document
            .begin_east_resize(1, 0, point(252.0, 588.0), 4.0)
            .unwrap();
        for sample in 1..=SAMPLES {
            let progress = f64::from(sample) / f64::from(SAMPLES);
            document
                .update_gesture(1, point(252.0 + 72.0 * progress, 588.0))
                .unwrap();
        }
        document.commit_gesture(1).unwrap();
        document
            .set_selected_appearance(
                RectangleAppearance::new("#123456", 3.25, Some("#abcdef"), 0.35).unwrap(),
            )
            .unwrap();

        let snapshot = document.canonical_json_string();
        assert_eq!(document.history_depths(), (4, 0));
        assert_eq!(
            snapshot,
            r##"{"markups":[{"appearance":{"fill":{"color":"#abcdef"},"fillOpacity":1.0,"opacity":0.35,"stroke":{"color":"#123456","widthPt":3.25}},"id":"perf-rectangle-1","kind":"rectangle","pageIndex":0,"rect":{"height":72.0,"width":216.0,"x":108.0,"y":552.0}}],"schema_version":1,"selection":"perf-rectangle-1"}"##
        );
        assert_eq!(fnv1a64_hex(snapshot.as_bytes()), "c43b4a338e830124");
    }
}
