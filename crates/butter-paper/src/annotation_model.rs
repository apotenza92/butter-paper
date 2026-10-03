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

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::page_geometry::{PageCoordinateSpace, Rotation as CoordinateRotation};
use crate::selection_geometry::{
    SelectionMarquee, SelectionPath, SelectionPoint, geometry_selected, selection_after,
};

/// Additional measured selection polygons, keyed by annotation identity in PDF space.
pub type AnnotationSelectionSupplement = std::collections::HashMap<MarkupId, Vec<PdfPoint>>;

/// Immutable page-local bounds for a PDF annotation retained in the opaque
/// page-rendering channel rather than admitted into the editable model.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedAnnotationObstacle {
    pub id: String,
    pub page_index: u32,
    pub rect: PdfRect,
}

pub const DEFAULT_HISTORY_LIMIT: usize = 100;
pub const MIN_RECT_SIZE_PT: f64 = 1.0;
const MIN_SNAPSHOT_SIZE_PT: f64 = 2.0;
pub const MIN_RECT_CREATE_SIZE_PT: f64 = 2.0;
pub const MIN_STRAIGHT_LINE_LENGTH_PT: f64 = 2.0;
pub const ROTATION_HANDLE_OFFSET_PT: f64 = 12.0;
/// Selection bounds are drawn this many screen pixels outside the item.
pub const SELECTION_OUTSET_CSS_PX: f64 = 6.0;
pub const MAX_STREAMED_PATH_POINTS: usize = 100_000;
pub const MAX_COALESCED_PEN_SAMPLES: usize = 4_096;
pub const MAX_TEXT_BOX_BYTES: usize = 64 * 1024;
pub const MAX_FONT_FAMILY_BYTES: usize = 128;
pub const MAX_MEASUREMENT_UNIT_BYTES: usize = 32;
pub const MAX_MEASUREMENT_LABEL_BYTES: usize = 256;
pub const MAX_DECODED_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION_PX: u32 = 8_192;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
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
        let mut transform =
            Self::new_rotated(view_box.width, view_box.height, pixels_per_point, rotation)?;
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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
        // PDF numbers are f32 and some geometry is stored offset by a border
        // padding, so allow the rounding of one or two f32 operations.
        persisted_edges(self)
            .into_iter()
            .zip(persisted_edges(other))
            .all(|(expected, actual)| {
                (expected - actual).abs() <= 1e-5_f32.max(expected.abs() * 2. * f32::EPSILON)
            })
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

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MarkupId(String);

/// How far a dimension's extension lines run past the dimension line: Revu's
/// default `/LLE`.
pub const DIMENSION_LEADER_EXTENSION_PT: f64 = 2.;

/// Revu's Length measurement dimension-line offset (`/LL`).
pub const LENGTH_LEADER_LENGTH_PT: f64 = 10.;

/// Gap left in the dimension line on each side of its caption.
const MEASUREMENT_CAPTION_GAP_PT: f64 = 4.;

/// The lines and arrowheads Revu draws for a Dimension or Length
/// measurement, in PDF points.
#[derive(Clone, Debug, PartialEq)]
pub struct MeasurementLineLayout {
    /// From each measured point to just past the dimension line.
    pub extension_lines: [(PdfPoint, PdfPoint); 2],
    pub dimension_segments: Vec<(PdfPoint, PdfPoint)>,
    /// Closed arrowheads: tip, then the two base corners.
    pub arrowheads: [[PdfPoint; 3]; 2],
    /// The middle of the dimension line, where the caption is centred.
    pub caption_center: PdfPoint,
}

/// Lays out a measurement line as Revu does. Arrowheads are `7.8w` long and
/// `9w` wide, with tips `w` inside the extension lines. When the caption and
/// both arrowheads do not fit between the extension lines, the arrowheads
/// move outside them on `15.6w` tails and the caption sits on the line.
pub fn measurement_line_layout(
    start: PdfPoint,
    end: PdfPoint,
    offset_pt: f64,
    stroke_width_pt: f64,
    caption_width_pt: f64,
) -> Option<MeasurementLineLayout> {
    let delta_x = end.x - start.x;
    let delta_y = end.y - start.y;
    let length = delta_x.hypot(delta_y);
    if !length.is_finite() || length <= f64::EPSILON || !stroke_width_pt.is_finite() {
        return None;
    }
    let unit = (delta_x / length, delta_y / length);
    let normal = (-unit.1, unit.0);
    let at = |origin: PdfPoint, along: f64, across: f64| PdfPoint {
        x: origin.x + unit.0 * along + normal.0 * across,
        y: origin.y + unit.1 * along + normal.1 * across,
    };
    let extension = offset_pt + DIMENSION_LEADER_EXTENSION_PT.copysign(offset_pt);
    let width = stroke_width_pt.max(0.);
    let arrow_length = 7.8 * width;
    let arrow_half_width = 4.5 * width;
    let arrowhead = |tip: PdfPoint, direction: f64| {
        let base = at(tip, -direction * arrow_length, 0.);
        [
            tip,
            at(base, 0., arrow_half_width),
            at(base, 0., -arrow_half_width),
        ]
    };
    let caption_center = at(start, length * 0.5, offset_pt);
    let caption_gap = if caption_width_pt > 0. {
        caption_width_pt + MEASUREMENT_CAPTION_GAP_PT * 2.
    } else {
        0.
    };
    let inside = length - 2. * width >= 2. * arrow_length + caption_gap;
    let (dimension_segments, arrowheads) = if inside {
        let first = at(start, width, offset_pt);
        let last = at(start, length - width, offset_pt);
        let segments = if caption_gap > 0. {
            vec![
                (first, at(start, (length - caption_gap) * 0.5, offset_pt)),
                (at(start, (length + caption_gap) * 0.5, offset_pt), last),
            ]
        } else {
            vec![(first, last)]
        };
        (segments, [arrowhead(first, -1.), arrowhead(last, 1.)])
    } else {
        let first = at(start, -width, offset_pt);
        let last = at(start, length + width, offset_pt);
        (
            vec![
                (first, at(first, -2. * arrow_length, 0.)),
                (last, at(last, 2. * arrow_length, 0.)),
            ],
            [arrowhead(first, 1.), arrowhead(last, -1.)],
        )
    };
    Some(MeasurementLineLayout {
        extension_lines: [
            (start, at(start, 0., extension)),
            (end, at(start, length, extension)),
        ],
        dimension_segments,
        arrowheads,
        caption_center,
    })
}

/// Groups the integer digits of a formatted measurement with commas, as Revu
/// captions do (`2,697.37`). Fractional parts such as `1/2` are untouched.
pub fn group_measurement_thousands(value: &str) -> String {
    let (sign, unsigned) = value
        .strip_prefix('-')
        .map_or(("", value), |rest| ("-", rest));
    let digits_end = unsigned
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(unsigned.len());
    let (integer, rest) = unsigned.split_at(digits_end);
    if integer.len() <= 3 || rest.starts_with('/') {
        return value.to_owned();
    }
    let mut grouped = String::with_capacity(value.len() + integer.len() / 3);
    for (index, digit) in integer.chars().enumerate() {
        if index > 0 && (integer.len() - index) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    format!("{sign}{grouped}{rest}")
}

/// Sixteen random uppercase ASCII letters, as Revu writes for `/NM`.
pub fn generate_markup_name() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the operating system random source is available");
    bytes
        .iter()
        // 256 is not a multiple of 26; the slight bias is irrelevant for names.
        .map(|byte| char::from(b'A' + byte % 26))
        .collect()
}

impl MarkupId {
    /// A fresh markup name in Bluebeam Revu's form: sixteen random uppercase
    /// letters, used as the PDF `/NM`.
    pub fn generate() -> Self {
        Self(generate_markup_name())
    }

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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RectangleAppearance {
    stroke_color: String,
    stroke_width_pt: f64,
    fill_color: Option<String>,
    opacity: f64,
    fill_opacity: f64,
    stroke_style: StrokeStyle,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

    /// The same stroke without a fill, for open paths that cannot be filled.
    pub fn without_fill(mut self) -> Self {
        self.fill_color = None;
        self
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

    /// The side whose edge, away from its corners, lies within `tolerance_pt`
    /// of `point`. With no painted handles, a selected Rectangle resizes from
    /// anywhere along an edge.
    pub fn edge_resize_handle(
        &self,
        point: PdfPoint,
        tolerance_pt: f64,
    ) -> Option<RectangleResizeHandle> {
        let local = self.world_to_local(point);
        let rect = self.rect;
        let (left, right) = (rect.x, rect.x + rect.width);
        let (bottom, top) = (rect.y, rect.y + rect.height);
        let along_x = local.x > left + tolerance_pt && local.x < right - tolerance_pt;
        let along_y = local.y > bottom + tolerance_pt && local.y < top - tolerance_pt;
        if along_y && (local.x - right).abs() <= tolerance_pt {
            Some(RectangleResizeHandle::East)
        } else if along_y && (local.x - left).abs() <= tolerance_pt {
            Some(RectangleResizeHandle::West)
        } else if along_x && (local.y - top).abs() <= tolerance_pt {
            Some(RectangleResizeHandle::North)
        } else if along_x && (local.y - bottom).abs() <= tolerance_pt {
            Some(RectangleResizeHandle::South)
        } else {
            None
        }
    }

    fn rotation_handle_world_point(&self, offset_pt: f64) -> PdfPoint {
        let point = PdfPoint {
            x: self.rect.x + self.rect.width / 2.0,
            y: self.rect.y + self.rect.height + offset_pt,
        };
        rotate_point_around_rect_center(point, self.rect, -self.rotation_degrees)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArcAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub mid: PdfPoint,
    pub appearance: RectangleAppearance,
    pub locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ellipse_geometry: Option<ArcEllipseGeometry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct ArcEllipseGeometry {
    rect: PdfRect,
    angle1_degrees: f64,
    angle2_degrees: f64,
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
            ellipse_geometry: None,
        };
        annotation.circle_geometry()?;
        Ok(annotation)
    }

    pub fn from_rect_angles(
        id: MarkupId,
        page_index: u32,
        rect: PdfRect,
        angle1_degrees: f64,
        angle2_degrees: f64,
        appearance: RectangleAppearance,
    ) -> Result<Self, AnnotationError> {
        require_finite("arc.angle1", angle1_degrees)?;
        require_finite("arc.angle2", angle2_degrees)?;
        let sweep = normalize_arc_sweep(angle1_degrees, angle2_degrees);
        if sweep.abs() <= f64::EPSILON {
            return Err(AnnotationError::InvalidGeometry(
                "arc sweep must be nonzero".into(),
            ));
        }
        let point_at = |angle: f64| ellipse_point(rect, angle);
        Ok(Self {
            id,
            page_index,
            start: point_at(angle1_degrees),
            end: point_at(angle1_degrees + sweep),
            mid: point_at(angle1_degrees + sweep * 0.5),
            appearance,
            locked: false,
            ellipse_geometry: Some(ArcEllipseGeometry {
                rect,
                angle1_degrees: canonical_float(angle1_degrees),
                angle2_degrees: canonical_float(angle1_degrees + sweep),
            }),
        })
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
        if let Some(geometry) = &self.ellipse_geometry {
            return geometry.rect;
        }
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
        if let Some(geometry) = &self.ellipse_geometry {
            return geometry.angle1_degrees;
        }
        self.circle_geometry()
            .expect("a retained Arc always has valid circle geometry")
            .2
    }

    pub fn angle2_degrees(&self) -> f64 {
        if let Some(geometry) = &self.ellipse_geometry {
            return geometry.angle2_degrees;
        }
        let (_, _, start_angle, sweep) = self
            .circle_geometry()
            .expect("a retained Arc always has valid circle geometry");
        canonical_float(start_angle + sweep)
    }

    pub fn sweep_degrees(&self) -> f64 {
        if let Some(geometry) = &self.ellipse_geometry {
            return normalize_arc_sweep(geometry.angle1_degrees, geometry.angle2_degrees);
        }
        self.circle_geometry()
            .expect("a retained Arc always has valid circle geometry")
            .3
    }

    pub fn sampled_path(&self, segments: usize) -> Vec<PdfPoint> {
        if self.ellipse_geometry.is_some() {
            let rect = self.rect();
            let start_angle = self.angle1_degrees();
            let sweep = self.sweep_degrees();
            let segments = segments.max(1);
            let mut points = (0..=segments)
                .map(|index| {
                    ellipse_point(rect, start_angle + sweep * index as f64 / segments as f64)
                })
                .collect::<Vec<_>>();
            points[0] = self.start;
            points[segments] = self.end;
            return points;
        }
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
            && self.rect().same_pdf_geometry_as(other.rect())
            && (self.angle1_degrees() - other.angle1_degrees()).abs() <= PDF_NUMBER_TOLERANCE
            && (self.angle2_degrees() - other.angle2_degrees()).abs() <= PDF_NUMBER_TOLERANCE
            && self.appearance == other.appearance
            && self.locked == other.locked
    }

    pub fn with_control_point(
        &self,
        control: ArcControlPoint,
        point: PdfPoint,
    ) -> Result<Self, AnnotationError> {
        let (start, end, mid) = match control {
            ArcControlPoint::Start => (point, self.end, self.mid),
            ArcControlPoint::Mid => (self.start, self.end, point),
            ArcControlPoint::End => (self.start, point, self.mid),
        };
        let Some(geometry) = &self.ellipse_geometry else {
            let mut replacement = Self::new(
                self.id.clone(),
                self.page_index,
                start,
                end,
                mid,
                self.appearance.clone(),
            )?;
            replacement.locked = self.locked;
            return Ok(replacement);
        };
        let ratio = geometry.rect.height / geometry.rect.width;
        let to_circle = |point: PdfPoint| PdfPoint {
            x: point.x,
            y: point.y / ratio,
        };
        let circle = Self::new(
            self.id.clone(),
            self.page_index,
            to_circle(start),
            to_circle(end),
            to_circle(mid),
            self.appearance.clone(),
        )?;
        let circle_rect = circle.rect();
        let rect = PdfRect::new(
            circle_rect.x,
            circle_rect.y * ratio,
            circle_rect.width,
            circle_rect.height * ratio,
        )?;
        let mut replacement = Self::from_rect_angles(
            self.id.clone(),
            self.page_index,
            rect,
            circle.angle1_degrees(),
            circle.angle2_degrees(),
            self.appearance.clone(),
        )?;
        replacement.locked = self.locked;
        Ok(replacement)
    }

    pub fn constrained_midpoint_for_shape(
        &self,
        pointer: PdfPoint,
        minimum_bulge_pt: f64,
        snap_quarter_turn: bool,
    ) -> Result<PdfPoint, AnnotationError> {
        let Some(geometry) = &self.ellipse_geometry else {
            return Self::constrained_midpoint(
                self.start,
                self.end,
                pointer,
                minimum_bulge_pt,
                snap_quarter_turn,
            );
        };
        let ratio = geometry.rect.height / geometry.rect.width;
        let to_circle = |point: PdfPoint| PdfPoint {
            x: point.x,
            y: point.y / ratio,
        };
        let resolved = Self::constrained_midpoint(
            to_circle(self.start),
            to_circle(self.end),
            to_circle(pointer),
            minimum_bulge_pt / ratio.max(f64::EPSILON),
            snap_quarter_turn,
        )?;
        PdfPoint::new(resolved.x, resolved.y * ratio)
    }

    pub fn translated(&self, delta_x: f64, delta_y: f64) -> Result<Self, AnnotationError> {
        if let Some(geometry) = &self.ellipse_geometry {
            let mut translated = Self::from_rect_angles(
                self.id.clone(),
                self.page_index,
                PdfRect::new(
                    geometry.rect.x + delta_x,
                    geometry.rect.y + delta_y,
                    geometry.rect.width,
                    geometry.rect.height,
                )?,
                geometry.angle1_degrees,
                geometry.angle2_degrees,
                self.appearance.clone(),
            )?;
            translated.locked = self.locked;
            Ok(translated)
        } else {
            let mut translated = Self::new(
                self.id.clone(),
                self.page_index,
                PdfPoint::new(self.start.x + delta_x, self.start.y + delta_y)?,
                PdfPoint::new(self.end.x + delta_x, self.end.y + delta_y)?,
                PdfPoint::new(self.mid.x + delta_x, self.mid.y + delta_y)?,
                self.appearance.clone(),
            )?;
            translated.locked = self.locked;
            Ok(translated)
        }
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VertexPathAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    points: Vec<PdfPoint>,
    pub kind: VertexPathKind,
    pub appearance: RectangleAppearance,
    pub locked: bool,
}

/// Revu's nominal cloud curl spacing at intensity 2; it scales with intensity.
pub const DEFAULT_CLOUD_SCALLOP_RADIUS_PT: f64 = 14.093;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
            cloud_nominal_spacing(self.border_effect_intensity),
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MeasurementPathAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    points: Vec<PdfPoint>,
    pub kind: MeasurementPathKind,
    calibration: LengthCalibration,
    pub appearance: RectangleAppearance,
    text_style: TextBoxStyle,
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
        Self::new_with_text_style(
            id,
            page_index,
            points,
            kind,
            calibration,
            appearance,
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.)?,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_text_style(
        id: MarkupId,
        page_index: u32,
        points: Vec<PdfPoint>,
        kind: MeasurementPathKind,
        calibration: LengthCalibration,
        appearance: RectangleAppearance,
        text_style: TextBoxStyle,
    ) -> Result<Self, AnnotationError> {
        validate_measurement_path(&points, kind)?;
        Ok(Self {
            id,
            page_index,
            points,
            kind,
            calibration,
            appearance,
            text_style,
            locked: false,
        })
    }

    pub fn points(&self) -> &[PdfPoint] {
        &self.points
    }

    pub fn calibration(&self) -> &LengthCalibration {
        &self.calibration
    }

    pub fn text_style(&self) -> &TextBoxStyle {
        &self.text_style
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
        let value = group_measurement_thousands(
            &self
                .calibration
                .scale_precision
                .format(self.measured_value()),
        );
        match self.kind {
            MeasurementPathKind::Polylength => {
                format!("{value} {}", self.calibration.unit)
            }
            MeasurementPathKind::Area => {
                format!("{value} sq {}", self.calibration.unit)
            }
        }
    }

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && self.kind == other.kind
            && self.calibration.same_persisted_state_as(&other.calibration)
            && self.appearance == other.appearance
            && self.text_style == other.text_style
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum LineKind {
    Line,
    Arrow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEndpoint {
    Start,
    End,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

pub fn straight_line_arrowhead_points(
    start: PdfPoint,
    end: PdfPoint,
    stroke_width_pt: f64,
) -> Option<[PdfPoint; 3]> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let distance = dx.hypot(dy);
    if !distance.is_finite() || distance <= f64::EPSILON || !stroke_width_pt.is_finite() {
        return None;
    }
    let direction_x = dx / distance;
    let direction_y = dy / distance;
    let length = (stroke_width_pt * 8.).max(7.);
    let width = (stroke_width_pt * 5.).max(4.);
    let base_x = end.x - direction_x * length;
    let base_y = end.y - direction_y * length;
    let half_width = width / 2.;
    Some([
        end,
        PdfPoint::new(
            base_x - direction_y * half_width,
            base_y + direction_x * half_width,
        )
        .ok()?,
        PdfPoint::new(
            base_x + direction_y * half_width,
            base_y - direction_x * half_width,
        )
        .ok()?,
    ])
}

pub fn straight_line_painted_bounds(
    annotation: &StraightLineAnnotation,
    antialias_allowance_pt: f64,
) -> Option<PdfRect> {
    if !antialias_allowance_pt.is_finite() || antialias_allowance_pt < 0. {
        return None;
    }
    let mut points = vec![annotation.start, annotation.end];
    if annotation.kind == LineKind::Arrow {
        points.extend(straight_line_arrowhead_points(
            annotation.start,
            annotation.end,
            annotation.appearance.stroke_width_pt(),
        )?);
    }
    let min_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let padding = annotation.appearance.stroke_width_pt() * 0.5 + antialias_allowance_pt;
    PdfRect::new(
        min_x - padding,
        min_y - padding,
        max_x - min_x + padding * 2.,
        max_y - min_y + padding * 2.,
    )
    .ok()
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum InkTool {
    Pen,
    Highlight,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BlendMode {
    Normal,
    Multiply,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextBoxStyle {
    font_family: String,
    font_size_pt: f64,
    color: String,
    opacity: f64,
    weight: u16,
    alignment: TextAlignment,
    line_height_pt: Option<f64>,
    inset_pt: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
            line_height_pt: None,
            inset_pt: 0.,
        })
    }

    pub fn with_layout_metrics(
        mut self,
        line_height_pt: f64,
        inset_pt: f64,
    ) -> Result<Self, AnnotationError> {
        require_finite("text.line_height_pt", line_height_pt)?;
        require_finite("text.inset_pt", inset_pt)?;
        let line_height_pt = canonical_float(line_height_pt);
        let inset_pt = canonical_float(inset_pt);
        require_finite("text.line_height_pt", line_height_pt)?;
        require_finite("text.inset_pt", inset_pt)?;
        if line_height_pt <= 0. || inset_pt < 0. {
            return Err(AnnotationError::InvalidAppearance(
                "text line height must be positive and inset non-negative".into(),
            ));
        }
        self.line_height_pt =
            (line_height_pt != canonical_float(self.font_size_pt * 1.15)).then_some(line_height_pt);
        self.inset_pt = inset_pt;
        Ok(self)
    }

    pub fn line_height_pt(&self) -> f64 {
        self.line_height_pt.unwrap_or(self.font_size_pt * 1.15)
    }
    pub fn inset_pt(&self) -> f64 {
        self.inset_pt
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextBoxRichTextRun {
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    font_family: Option<String>,
    #[serde(default)]
    bold: bool,
    #[serde(default)]
    italic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    font_size_pt: Option<f64>,
}

impl TextBoxRichTextRun {
    /// The run's text and resolved style: an unset family, size or colour
    /// inherits the box's, so an explicit repeat is the same run.
    fn effective_style<'a>(&'a self, base: &'a TextBoxStyle) -> (&'a str, &'a str, u64, &'a str, bool, bool) {
        (
            self.text.as_str(),
            self.font_family.as_deref().unwrap_or(base.font_family()),
            self.font_size_pt.unwrap_or(base.font_size_pt()).to_bits(),
            self.color.as_deref().unwrap_or(base.color()),
            self.bold,
            self.italic,
        )
    }

    pub fn new(text: impl Into<String>) -> Result<Self, AnnotationError> {
        let text = text.into();
        validate_text(&text, "rich text run", MAX_TEXT_BOX_BYTES)?;
        Ok(Self {
            text,
            font_family: None,
            bold: false,
            italic: false,
            color: None,
            font_size_pt: None,
        })
    }

    pub fn with_font_family(
        mut self,
        font_family: impl Into<String>,
    ) -> Result<Self, AnnotationError> {
        let font_family = font_family.into();
        validate_text(&font_family, "rich text font family", MAX_FONT_FAMILY_BYTES)?;
        self.font_family = Some(font_family);
        Ok(self)
    }

    pub fn with_emphasis(mut self, bold: bool, italic: bool) -> Self {
        self.bold = bold;
        self.italic = italic;
        self
    }

    pub fn with_color(mut self, color: impl Into<String>) -> Result<Self, AnnotationError> {
        self.color = Some(normalize_color(color.into())?);
        Ok(self)
    }

    pub fn with_font_size_pt(mut self, font_size_pt: f64) -> Result<Self, AnnotationError> {
        require_finite("rich_text.font_size_pt", font_size_pt)?;
        if font_size_pt <= 0.0 {
            return Err(AnnotationError::InvalidAppearance(
                "rich text font size must be positive".into(),
            ));
        }
        self.font_size_pt = Some(canonical_float(font_size_pt));
        Ok(self)
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    pub fn bold(&self) -> bool {
        self.bold
    }

    pub fn italic(&self) -> bool {
        self.italic
    }

    pub fn color(&self) -> Option<&str> {
        self.color.as_deref()
    }

    pub fn font_size_pt(&self) -> Option<f64> {
        self.font_size_pt
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextBoxAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub layout_rect: PdfRect,
    content: String,
    style: TextBoxStyle,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    rich_text_runs: Vec<TextBoxRichTextRun>,
    rotation_degrees: f64,
    pub locked: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CloudPlusAppearance {
    cloud: RectangleAppearance,
    leader: StraightLineAppearance,
    text: TextBoxStyle,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub enum CloudAppearancePathCommand {
    MoveTo(PdfPoint),
    LineTo(PdfPoint),
    CubicTo {
        control_1: PdfPoint,
        control_2: PdfPoint,
        end: PdfPoint,
    },
    Close,
}

impl CloudPlusAppearance {
    pub fn new(
        cloud: RectangleAppearance,
        leader: StraightLineAppearance,
        text: TextBoxStyle,
    ) -> Result<Self, AnnotationError> {
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CloudPlusAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    cloud_points: Vec<PdfPoint>,
    border_effect_intensity: f64,
    cloud_appearance_path: Option<Vec<CloudAppearancePathCommand>>,
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
        validate_optional_text(&content, "Cloud+ content", MAX_TEXT_BOX_BYTES)?;
        Ok(Self {
            id,
            page_index,
            cloud_points,
            border_effect_intensity: canonical_float(border_effect_intensity),
            cloud_appearance_path: None,
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

    pub fn cloud_appearance_path(&self) -> Option<&[CloudAppearancePathCommand]> {
        self.cloud_appearance_path.as_deref()
    }

    pub fn with_cloud_appearance_path(
        mut self,
        path: Option<Vec<CloudAppearancePathCommand>>,
    ) -> Result<Self, AnnotationError> {
        if let Some(path) = path.as_deref() {
            validate_cloud_appearance_path(path)?;
        }
        self.cloud_appearance_path = path;
        Ok(self)
    }

    pub fn translated_cloud_appearance_path(
        &self,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Option<Vec<CloudAppearancePathCommand>>, AnnotationError> {
        self.cloud_appearance_path
            .as_deref()
            .map(|path| translate_cloud_appearance_path(path, delta_x, delta_y))
            .transpose()
    }

    pub fn leader_points(&self) -> &[PdfPoint] {
        &self.leader_points
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn scallop_path(&self) -> Vec<PdfPoint> {
        if let Some(path) = &self.cloud_appearance_path {
            return sample_cloud_appearance_path(path);
        }
        sampled_cloud_scallop_path(
            &self.cloud_points,
            cloud_nominal_spacing(self.border_effect_intensity),
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
            && cloud_appearance_paths_match(
                self.cloud_appearance_path.as_deref(),
                other.cloud_appearance_path.as_deref(),
                PDF_NUMBER_TOLERANCE,
            )
    }
}

fn validate_cloud_appearance_path(
    path: &[CloudAppearancePathCommand],
) -> Result<(), AnnotationError> {
    if path.len() < 3 || !matches!(path.first(), Some(CloudAppearancePathCommand::MoveTo(_))) {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ appearance path must start with MoveTo and retain a drawable closed path"
                .into(),
        ));
    }
    if !matches!(path.last(), Some(CloudAppearancePathCommand::Close)) {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ appearance path must end with Close".into(),
        ));
    }
    let mut drawable = 0usize;
    for (index, command) in path.iter().enumerate() {
        if matches!(
            command,
            CloudAppearancePathCommand::LineTo(_) | CloudAppearancePathCommand::CubicTo { .. }
        ) {
            drawable += 1;
        }
        let points = match command {
            CloudAppearancePathCommand::MoveTo(point)
            | CloudAppearancePathCommand::LineTo(point) => vec![point],
            CloudAppearancePathCommand::CubicTo {
                control_1,
                control_2,
                end,
            } => vec![control_1, control_2, end],
            CloudAppearancePathCommand::Close => Vec::new(),
        };
        for point in points {
            require_finite(&format!("cloud_plus.appearance_path[{index}].x"), point.x)?;
            require_finite(&format!("cloud_plus.appearance_path[{index}].y"), point.y)?;
        }
    }
    if drawable < 2 {
        return Err(AnnotationError::InvalidGeometry(
            "Cloud+ appearance path must retain at least two drawable segments".into(),
        ));
    }
    Ok(())
}

pub(crate) fn sample_cloud_appearance_path(path: &[CloudAppearancePathCommand]) -> Vec<PdfPoint> {
    const CUBIC_STEPS: usize = 12;
    let mut sampled = Vec::new();
    let mut current = None;
    let mut subpath_start = None;
    for command in path {
        match *command {
            CloudAppearancePathCommand::MoveTo(point) => {
                sampled.push(point);
                current = Some(point);
                subpath_start = Some(point);
            }
            CloudAppearancePathCommand::LineTo(point) => {
                sampled.push(point);
                current = Some(point);
            }
            CloudAppearancePathCommand::CubicTo {
                control_1,
                control_2,
                end,
            } => {
                let start = current.expect("validated Cloud+ path starts with MoveTo");
                for step in 1..=CUBIC_STEPS {
                    let t = step as f64 / CUBIC_STEPS as f64;
                    let one_minus_t = 1. - t;
                    sampled.push(PdfPoint {
                        x: one_minus_t.powi(3) * start.x
                            + 3. * one_minus_t.powi(2) * t * control_1.x
                            + 3. * one_minus_t * t.powi(2) * control_2.x
                            + t.powi(3) * end.x,
                        y: one_minus_t.powi(3) * start.y
                            + 3. * one_minus_t.powi(2) * t * control_1.y
                            + 3. * one_minus_t * t.powi(2) * control_2.y
                            + t.powi(3) * end.y,
                    });
                }
                current = Some(end);
            }
            CloudAppearancePathCommand::Close => {
                if let Some(start) = subpath_start
                    && sampled.last() != Some(&start)
                {
                    sampled.push(start);
                }
            }
        }
    }
    sampled
}

fn translate_cloud_appearance_path(
    path: &[CloudAppearancePathCommand],
    delta_x: f64,
    delta_y: f64,
) -> Result<Vec<CloudAppearancePathCommand>, AnnotationError> {
    let translated = |point: PdfPoint| PdfPoint::new(point.x + delta_x, point.y + delta_y);
    path.iter()
        .map(|command| match *command {
            CloudAppearancePathCommand::MoveTo(point) => {
                Ok(CloudAppearancePathCommand::MoveTo(translated(point)?))
            }
            CloudAppearancePathCommand::LineTo(point) => {
                Ok(CloudAppearancePathCommand::LineTo(translated(point)?))
            }
            CloudAppearancePathCommand::CubicTo {
                control_1,
                control_2,
                end,
            } => Ok(CloudAppearancePathCommand::CubicTo {
                control_1: translated(control_1)?,
                control_2: translated(control_2)?,
                end: translated(end)?,
            }),
            CloudAppearancePathCommand::Close => Ok(CloudAppearancePathCommand::Close),
        })
        .collect()
}

fn cloud_appearance_paths_match(
    left: Option<&[CloudAppearancePathCommand]>,
    right: Option<&[CloudAppearancePathCommand]>,
    tolerance: f64,
) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return left.is_none() && right.is_none();
    };
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| match (left, right) {
                (CloudAppearancePathCommand::Close, CloudAppearancePathCommand::Close) => true,
                (
                    CloudAppearancePathCommand::MoveTo(left),
                    CloudAppearancePathCommand::MoveTo(right),
                )
                | (
                    CloudAppearancePathCommand::LineTo(left),
                    CloudAppearancePathCommand::LineTo(right),
                ) => (left.x - right.x).abs() <= tolerance && (left.y - right.y).abs() <= tolerance,
                (
                    CloudAppearancePathCommand::CubicTo {
                        control_1: l1,
                        control_2: l2,
                        end: le,
                    },
                    CloudAppearancePathCommand::CubicTo {
                        control_1: r1,
                        control_2: r2,
                        end: re,
                    },
                ) => [(*l1, *r1), (*l2, *r2), (*le, *re)]
                    .into_iter()
                    .all(|(left, right)| {
                        (left.x - right.x).abs() <= tolerance
                            && (left.y - right.y).abs() <= tolerance
                    }),
                _ => false,
            })
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CalloutAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    leader_points: Vec<PdfPoint>,
    pub text_box: PdfRect,
    content: String,
    pub appearance: CalloutAppearance,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CalloutDiskGeometry {
    pub outer_rect: PdfRect,
    pub rect_differences: [f32; 4],
    pub text_box: PdfRect,
}

impl CalloutDiskGeometry {
    pub fn reconstruct_text_box(
        outer_rect: PdfRect,
        rect_differences: [f32; 4],
    ) -> Result<PdfRect, AnnotationError> {
        let [left, bottom, right, top] = rect_differences.map(f64::from);
        PdfRect::new(
            outer_rect.x + left,
            outer_rect.y + bottom,
            outer_rect.width - left - right,
            outer_rect.height - bottom - top,
        )
    }
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
        validate_optional_text(&content, "callout content", MAX_TEXT_BOX_BYTES)?;
        Self {
            id,
            page_index,
            leader_points,
            text_box,
            content,
            appearance,
            locked: false,
        }
        .canonicalized_for_disk()
    }

    pub fn leader_points(&self) -> &[PdfPoint] {
        &self.leader_points
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn resized_text_box(&self, text_box: PdfRect) -> Result<Self, AnnotationError> {
        let connection = *self
            .leader_points
            .last()
            .expect("validated Callout has a connection point");
        let attached_to_left = (connection.x - self.text_box.x).abs()
            <= (connection.x - (self.text_box.x + self.text_box.width)).abs();
        let mut leader_points = self.leader_points.clone();
        *leader_points
            .last_mut()
            .expect("validated Callout has a connection point") = PdfPoint {
            x: if attached_to_left {
                text_box.x
            } else {
                text_box.x + text_box.width
            },
            y: text_box.y + text_box.height * 0.5,
        };
        let mut replacement = Self::new(
            self.id.clone(),
            self.page_index,
            leader_points,
            text_box,
            self.content.clone(),
            self.appearance.clone(),
        )?;
        replacement.locked = self.locked;
        Ok(replacement)
    }

    pub fn disk_geometry(&self) -> Result<CalloutDiskGeometry, AnnotationError> {
        const PADDING_PT: f64 = 5.5;
        let connection = *self
            .leader_points
            .last()
            .expect("validated Callout has a connection point");
        let tip = self.leader_points[0];
        let leader = if self.leader_points.len() <= 2 {
            vec![tip, connection]
        } else {
            let knee_index = (self.leader_points.len() - 1)
                .saturating_div(2)
                .min(self.leader_points.len() - 2)
                .max(1);
            vec![tip, self.leader_points[knee_index], connection]
        };
        let mut min_x = self.text_box.x;
        let mut min_y = self.text_box.y;
        let mut max_x = self.text_box.x + self.text_box.width;
        let mut max_y = self.text_box.y + self.text_box.height;
        for point in leader {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
        let raw_outer = PdfRect::new(
            min_x - PADDING_PT,
            min_y - PADDING_PT,
            max_x - min_x + PADDING_PT * 2.,
            max_y - min_y + PADDING_PT * 2.,
        )?;
        let left = raw_outer.x as f32;
        let bottom = raw_outer.y as f32;
        let right = (raw_outer.x + raw_outer.width) as f32;
        let top = (raw_outer.y + raw_outer.height) as f32;
        let outer_rect = PdfRect::new(
            f64::from(left),
            f64::from(bottom),
            f64::from(right) - f64::from(left),
            f64::from(top) - f64::from(bottom),
        )?;
        let rect_differences = [
            (self.text_box.x - outer_rect.x) as f32,
            (self.text_box.y - outer_rect.y) as f32,
            (outer_rect.x + outer_rect.width - self.text_box.x - self.text_box.width) as f32,
            (outer_rect.y + outer_rect.height - self.text_box.y - self.text_box.height) as f32,
        ];
        let text_box = CalloutDiskGeometry::reconstruct_text_box(outer_rect, rect_differences)?;
        Ok(CalloutDiskGeometry {
            outer_rect,
            rect_differences,
            text_box,
        })
    }

    pub fn canonicalized_for_disk(&self) -> Result<Self, AnnotationError> {
        const MAX_PASSES: usize = 8;
        let mut canonical = self.clone();
        for _ in 0..MAX_PASSES {
            let text_box = canonical.disk_geometry()?.text_box;
            if canonical.text_box == text_box {
                return Ok(canonical);
            }
            canonical.text_box = text_box;
        }
        Err(AnnotationError::InvalidGeometry(
            "Callout PDF /Rect and /RD geometry did not converge".into(),
        ))
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
    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        self.id == other.id
            && self.page_index == other.page_index
            && self.layout_rect.same_pdf_geometry_as(other.layout_rect)
            && self.content == other.content
            && self.style == other.style
            && self.rich_text_runs.len() == other.rich_text_runs.len()
            && self
                .rich_text_runs
                .iter()
                .zip(&other.rich_text_runs)
                .all(|(left, right)| {
                    left.effective_style(&self.style) == right.effective_style(&other.style)
                })
            && (self.rotation_degrees - other.rotation_degrees).abs() <= 0.000_1
            && self.locked == other.locked
    }

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
            rich_text_runs: Vec::new(),
            rotation_degrees: 0.,
            locked: false,
        })
    }

    pub fn with_rich_text_runs(
        mut self,
        rich_text_runs: Vec<TextBoxRichTextRun>,
    ) -> Result<Self, AnnotationError> {
        validate_text_box_rich_text_runs(&self.content, &rich_text_runs)?;
        self.rich_text_runs = rich_text_runs;
        Ok(self)
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn style(&self) -> &TextBoxStyle {
        &self.style
    }

    pub fn rich_text_runs(&self) -> &[TextBoxRichTextRun] {
        &self.rich_text_runs
    }

    pub fn rotation_degrees(&self) -> f64 {
        self.rotation_degrees
    }

    pub fn with_rotation_degrees(mut self, rotation_degrees: f64) -> Result<Self, AnnotationError> {
        require_finite("text_box.rotation", rotation_degrees)?;
        self.rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

    pub fn points(self) -> f64 {
        match self {
            Self::In => 72.,
            Self::Ft => 864.,
            Self::Mm => 72. / 25.4,
            Self::Cm => 72. / 2.54,
            Self::M => 72. / 0.0254,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ScalePrecisionMode {
    Decimal,
    Fraction,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
    /// Paper-to-real ratio, using the same unit conversion as the Electron sidebar.
    pub fn ratio_label(&self) -> String {
        let ratio = self.scale_x * self.real_units.points();
        if !ratio.is_finite() || ratio <= 0. {
            return self.name.clone();
        }
        format!("1:{}", format_scale_number(ratio))
    }

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
            // Unit conversion factors can be much smaller than one. Rounding
            // them like drawing coordinates changes the measurement scale.
            scale_x,
            scale_y,
            precision,
        })
    }

    /// Whether a reopened page scale is this one as a PDF viewport stores
    /// it: units, f32 factors and precision. The scale's name and source are
    /// editing metadata Revu does not record.
    pub fn same_persisted_scale_as(&self, other: &Self) -> bool {
        let same_factor = |left: f64, right: f64| {
            (left - right).abs() <= 1e-6 * left.abs().max(right.abs()).max(f64::MIN_POSITIVE)
        };
        self.page_index == other.page_index
            && self.pdf_units == other.pdf_units
            && self.real_units == other.real_units
            && same_factor(self.scale_x, other.scale_x)
            && same_factor(self.scale_y, other.scale_y)
            && self.precision.mode == other.precision.mode
            && (self.precision.value - other.precision.value).abs() <= 1e-9
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
            scale_x: (f64::from(ratio) / 100.) / ScaleUnit::Cm.points(),
            scale_y: (f64::from(ratio) / 100.) / ScaleUnit::Cm.points(),
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
    Ok(real_length / (pdf_length * pdf_units.points()))
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
        // Per-point factors are small; rounding them like coordinates would
        // skew every measurement (0.0352778 m/pt would read 0.035278).
        calibration.units_per_point = scale.scale_x;
        calibration.real_world_value = scale.scale_x;
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

    pub fn with_show_caption(mut self, show_caption: bool) -> Self {
        self.show_caption = show_caption;
        self
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
        replacement.units_per_point = scale.units_per_point;
        replacement.paper_points = scale.paper_points;
        replacement.real_world_value = scale.real_world_value;
        replacement.scale_x = scale.scale_x;
        replacement.scale_y = scale.scale_y;
        replacement.scale_precision = scale.scale_precision;
        Ok(replacement)
    }

    /// Whether two calibrations agree as a PDF `/Measure` stores them: the
    /// f32 scale ratio, unit and precision.
    pub fn same_persisted_scale_as(&self, other: &LengthCalibration) -> bool {
        let same_factor = |left: f64, right: f64| {
            (left - right).abs() <= 1e-6 * left.abs().max(right.abs()).max(f64::MIN_POSITIVE)
        };
        same_factor(self.units_per_point, other.units_per_point)
            && same_factor(self.scale_y, other.scale_y)
            && self.unit == other.unit
            && self.scale_precision.mode == other.scale_precision.mode
            && (self.scale_precision.value - other.scale_precision.value).abs() <= 1e-9
    }

    pub fn same_scale_as(&self, other: &LengthCalibration) -> bool {
        self.units_per_point == other.units_per_point
            && self.scale_x == other.scale_x
            && self.scale_y == other.scale_y
            && self.unit == other.unit
            && self.scale_precision == other.scale_precision
    }

    /// A PDF `/Measure` stores the scale as a ratio in f32, so the separate
    /// paper and real-world values and the caption toggle are not persisted.
    fn same_persisted_state_as(&self, other: &LengthCalibration) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_001;
        let same_ratio = |left: f64, right: f64| {
            (left - right).abs() <= PDF_NUMBER_TOLERANCE * left.abs().max(right.abs()).max(1.)
        };
        same_ratio(self.units_per_point, other.units_per_point)
            && same_ratio(self.scale_x, other.scale_x)
            && same_ratio(self.scale_y, other.scale_y)
            && self.unit == other.unit
            && self.label == other.label
            && self.precision == other.precision
            && self.scale_precision.mode == other.scale_precision.mode
            && (self.scale_precision.value - other.scale_precision.value).abs()
                <= PDF_NUMBER_TOLERANCE
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
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
        validate_optional_text(&content, "dimension content", MAX_TEXT_BOX_BYTES)?;
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LengthAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub start: PdfPoint,
    pub end: PdfPoint,
    calibration: LengthCalibration,
    pub appearance: DimensionAppearance,
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
        Self::new_with_appearance(
            id,
            page_index,
            start,
            end,
            calibration,
            DimensionAppearance::new(
                StraightLineAppearance::default_for(LineKind::Line),
                TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.)?,
            )?,
        )
    }

    pub fn new_with_appearance(
        id: MarkupId,
        page_index: u32,
        start: PdfPoint,
        end: PdfPoint,
        calibration: LengthCalibration,
        appearance: DimensionAppearance,
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
            appearance,
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
        let value = group_measurement_thousands(
            &self
                .calibration
                .scale_precision
                .format(self.measured_value()),
        );
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
            && self.calibration.same_persisted_state_as(&other.calibration)
            && self.appearance == other.appearance
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

const RECOVERY_ASSET_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DecodedRgbaAssetWire {
    asset_schema_version: u32,
    id: String,
    width_px: u32,
    height_px: u32,
    rgba_base64: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageAnnotation {
    pub id: MarkupId,
    pub page_index: u32,
    pub rect: PdfRect,
    asset: DecodedRgbaAsset,
    opacity: f64,
    rotation_degrees: f64,
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

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        self.same_placement_as(other) && self.asset == other.asset
    }

    /// Everything but the picture: id, page, box, rotation, opacity, lock.
    pub fn same_placement_as(&self, other: &Self) -> bool {
        const PDF_NUMBER_TOLERANCE: f64 = 0.000_1;
        self.id == other.id
            && self.page_index == other.page_index
            && (self.rect.x - other.rect.x).abs() <= PDF_NUMBER_TOLERANCE
            && (self.rect.y - other.rect.y).abs() <= PDF_NUMBER_TOLERANCE
            && (self.rect.width - other.rect.width).abs() <= PDF_NUMBER_TOLERANCE
            && (self.rect.height - other.rect.height).abs() <= PDF_NUMBER_TOLERANCE
            && (self.opacity - other.opacity).abs() <= PDF_NUMBER_TOLERANCE
            && (self.rotation_degrees - other.rotation_degrees).abs() <= PDF_NUMBER_TOLERANCE
            && self.locked == other.locked
    }

    /// Replaces the picture, as when the PDF worker rasterises a Revu vector
    /// Snapshot's Form for the canvas.
    pub fn with_asset(mut self, asset: DecodedRgbaAsset) -> Self {
        self.asset = asset;
        self
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
        Self::new_with_opacity(id, page_index, rect, asset, aspect_locked, 1.)
    }

    pub fn new_with_opacity(
        id: MarkupId,
        page_index: u32,
        rect: PdfRect,
        asset: DecodedRgbaAsset,
        aspect_locked: bool,
        opacity: f64,
    ) -> Result<Self, AnnotationError> {
        validate_layout_rect(rect, "image")?;
        validate_image_aspect(rect, &asset, aspect_locked)?;
        validate_snapshot_opacity(opacity)?;
        Ok(Self {
            id,
            page_index,
            rect,
            asset,
            opacity: canonical_float(opacity),
            rotation_degrees: 0.,
            aspect_locked,
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

    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        self.id == other.id
            && self.page_index == other.page_index
            && self.rect.same_pdf_geometry_as(other.rect)
            && self.asset == other.asset
            && (self.opacity - other.opacity).abs() <= 0.000_1
            && (self.rotation_degrees - other.rotation_degrees).abs() <= 0.000_1
            // Aspect lock is an editing preference with no PDF field.
            && self.locked == other.locked
    }

    pub fn with_rotation_degrees(mut self, rotation_degrees: f64) -> Result<Self, AnnotationError> {
        require_finite("image.rotation", rotation_degrees)?;
        self.rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
        Ok(self)
    }
}

impl PenAnnotation {
    /// Whether a reopened Ink is this one as a PDF stores it: f32 `/InkList`
    /// points, stroke, tool and lock. Smoothing is a display preference with
    /// no PDF field.
    pub fn same_persisted_state_as(&self, other: &Self) -> bool {
        let same_number = |left: f64, right: f64| {
            (left as f32 - right as f32).abs()
                <= 1e-5_f32.max(left.abs() as f32 * 2. * f32::EPSILON)
        };
        self.id == other.id
            && self.page_index == other.page_index
            && self.tool == other.tool
            && self.blend_mode == other.blend_mode
            && self.locked == other.locked
            && self.appearance.color == other.appearance.color
            && same_number(self.appearance.width_pt, other.appearance.width_pt)
            && same_number(self.appearance.opacity, other.appearance.opacity)
            && self.paths().count() == other.paths().count()
            && self.paths().zip(other.paths()).all(|(left, right)| {
                left.len() == right.len()
                    && left.iter().zip(right).all(|(left, right)| {
                        same_number(left.x, right.x) && same_number(left.y, right.y)
                    })
            })
    }

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
                let mut copy = if let Some(geometry) = &source.ellipse_geometry {
                    ArcAnnotation::from_rect_angles(
                        id,
                        page_index,
                        PdfRect::new(
                            geometry.rect.x + delta_x,
                            geometry.rect.y + delta_y,
                            geometry.rect.width,
                            geometry.rect.height,
                        )?,
                        geometry.angle1_degrees,
                        geometry.angle2_degrees,
                        source.appearance.clone(),
                    )?
                } else {
                    ArcAnnotation::new(
                        id,
                        page_index,
                        PdfPoint::new(source.start.x + delta_x, source.start.y + delta_y)?,
                        PdfPoint::new(source.end.x + delta_x, source.end.y + delta_y)?,
                        PdfPoint::new(source.mid.x + delta_x, source.mid.y + delta_y)?,
                        source.appearance.clone(),
                    )?
                };
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
                )?
                .with_cloud_appearance_path(
                    source.translated_cloud_appearance_path(delta_x, delta_y)?,
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
                let mut copy = MeasurementPathAnnotation::new_with_text_style(
                    id,
                    page_index,
                    points,
                    source.kind,
                    source.calibration.clone(),
                    source.appearance.clone(),
                    source.text_style.clone(),
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
                )?
                .with_rich_text_runs(source.rich_text_runs.clone())?
                .with_rotation_degrees(source.rotation_degrees)?;
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
                let mut copy = LengthAnnotation::new_with_appearance(
                    id,
                    page_index,
                    PdfPoint::new(source.start.x + delta_x, source.start.y + delta_y)?,
                    PdfPoint::new(source.end.x + delta_x, source.end.y + delta_y)?,
                    source.calibration.clone(),
                    source.appearance.clone(),
                )?;
                copy.locked = source.locked;
                Self::Length(copy)
            }
            Self::Image(source) => {
                let mut copy = ImageAnnotation::new_with_opacity(
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
                    source.opacity,
                )?
                .with_rotation_degrees(source.rotation_degrees)?;
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
    SetTextBoxRotation(f64),
    SetTextBoxStyle(TextBoxStyle),
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
    SetCloudAppearance(RectangleAppearance),
    SetCloudIntensity(f64),
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
    SetCalloutTextBox(PdfRect),
    TranslateCalloutTextBox {
        delta_x: f64,
        delta_y: f64,
    },
    TranslateCalloutGroup {
        delta_x: f64,
        delta_y: f64,
    },
    SetMeasurementPathCalibration(LengthCalibration),
    SetMeasurementPathAppearance {
        appearance: RectangleAppearance,
        text_style: TextBoxStyle,
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
    SetLengthAppearance(DimensionAppearance),
    SetVertexPathAppearance(RectangleAppearance),
    TranslateLength {
        delta_x: f64,
        delta_y: f64,
    },
    SetImageRect(PdfRect),
    SetImageRotation(f64),
    SetImageOpacity(f64),
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
    pub feedback: SceneInteractionFeedback,
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
    pub feedback: SceneInteractionFeedback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationScene {
    /// Committed page identities in document stacking order; new draft identities paint last.
    pub annotation_order: Vec<MarkupId>,
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

/// An owned scene item; moving into paint order never clones annotation payloads.
#[derive(Clone, Debug, PartialEq)]
pub enum SceneAnnotation {
    Rectangle(SceneRectangle),
    Redact(SceneRedact),
    Ellipse(SceneRectangle),
    Arc(SceneArc),
    StraightLine(SceneStraightLine),
    VertexPath(SceneVertexPath),
    Cloud(SceneCloud),
    CloudPlus(SceneCloudPlus),
    Callout(SceneCallout),
    MeasurementPath(SceneMeasurementPath),
    Pen(ScenePen),
    TextBox(SceneTextBox),
    Dimension(SceneDimension),
    Length(SceneLength),
    Image(SceneImage),
    Snapshot(SceneSnapshot),
}

impl SceneAnnotation {
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
}

impl AnnotationScene {
    fn with_document_order(mut self, order: &[MarkupId]) -> Self {
        let mut ids = std::collections::HashSet::new();
        ids.extend(self.rectangles.iter().map(|annotation| &annotation.id));
        ids.extend(self.redacts.iter().map(|annotation| &annotation.id));
        ids.extend(self.ellipses.iter().map(|annotation| &annotation.id));
        ids.extend(self.arcs.iter().map(|annotation| &annotation.id));
        ids.extend(self.straight_lines.iter().map(|annotation| &annotation.id));
        ids.extend(self.vertex_paths.iter().map(|annotation| &annotation.id));
        ids.extend(self.clouds.iter().map(|annotation| &annotation.id));
        ids.extend(self.cloud_pluses.iter().map(|annotation| &annotation.id));
        ids.extend(self.callouts.iter().map(|annotation| &annotation.id));
        ids.extend(
            self.measurement_paths
                .iter()
                .map(|annotation| &annotation.id),
        );
        ids.extend(self.pens.iter().map(|annotation| &annotation.id));
        ids.extend(self.text_boxes.iter().map(|annotation| &annotation.id));
        ids.extend(self.dimensions.iter().map(|annotation| &annotation.id));
        ids.extend(self.lengths.iter().map(|annotation| &annotation.id));
        ids.extend(self.images.iter().map(|annotation| &annotation.id));
        ids.extend(self.snapshots.iter().map(|annotation| &annotation.id));
        self.annotation_order = order
            .iter()
            .filter(|id| ids.contains(id))
            .cloned()
            .collect();
        self
    }

    /// Existing-identity previews replace their committed slot. New drafts follow
    /// all committed items, preserving their deterministic scene encounter order.
    pub fn into_ordered_annotations(self) -> std::vec::IntoIter<SceneAnnotation> {
        let slot_count = self.annotation_order.len();
        let ranks: std::collections::HashMap<_, _> = self
            .annotation_order
            .into_iter()
            .enumerate()
            .map(|(rank, id)| (id, rank))
            .collect();
        let mut ordered: Vec<Option<SceneAnnotation>> =
            std::iter::repeat_with(|| None).take(slot_count).collect();
        let mut drafts = Vec::new();
        let mut append = |annotation: SceneAnnotation| {
            if let Some(&rank) = ranks.get(annotation.id()) {
                ordered[rank] = Some(annotation);
            } else {
                drafts.push(annotation);
            }
        };
        for annotation in self.rectangles {
            append(SceneAnnotation::Rectangle(annotation));
        }
        for annotation in self.redacts {
            append(SceneAnnotation::Redact(annotation));
        }
        for annotation in self.ellipses {
            append(SceneAnnotation::Ellipse(annotation));
        }
        for annotation in self.arcs {
            append(SceneAnnotation::Arc(annotation));
        }
        for annotation in self.straight_lines {
            append(SceneAnnotation::StraightLine(annotation));
        }
        for annotation in self.vertex_paths {
            append(SceneAnnotation::VertexPath(annotation));
        }
        for annotation in self.clouds {
            append(SceneAnnotation::Cloud(annotation));
        }
        for annotation in self.cloud_pluses {
            append(SceneAnnotation::CloudPlus(annotation));
        }
        for annotation in self.callouts {
            append(SceneAnnotation::Callout(annotation));
        }
        for annotation in self.measurement_paths {
            append(SceneAnnotation::MeasurementPath(annotation));
        }
        for annotation in self.pens {
            append(SceneAnnotation::Pen(annotation));
        }
        for annotation in self.text_boxes {
            append(SceneAnnotation::TextBox(annotation));
        }
        for annotation in self.dimensions {
            append(SceneAnnotation::Dimension(annotation));
        }
        for annotation in self.lengths {
            append(SceneAnnotation::Length(annotation));
        }
        for annotation in self.images {
            append(SceneAnnotation::Image(annotation));
        }
        for annotation in self.snapshots {
            append(SceneAnnotation::Snapshot(annotation));
        }
        ordered
            .into_iter()
            .flatten()
            .chain(drafts)
            .collect::<Vec<_>>()
            .into_iter()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneArc {
    pub id: MarkupId,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub mid: PdfPoint,
    /// Canonical full-circle bounds retained for Electron-compatible routing.
    pub rect: PdfRect,
    pub angle1_degrees: f64,
    pub angle2_degrees: f64,
    pub sampled_path: Vec<PdfPoint>,
    pub appearance: RectangleAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
}

impl SceneArc {
    pub fn sweep_degrees(&self) -> f64 {
        normalize_arc_sweep(self.angle1_degrees, self.angle2_degrees)
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
    pub feedback: SceneInteractionFeedback,
}

/// Paint-only interaction state. Creation drafts suppress selection chrome,
/// while manipulation previews can retain the reference outline/active-handle
/// feedback without changing canonical annotation or history state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneInteractionFeedback {
    Normal,
    Creation,
    Move {
        chrome_visible: bool,
    },
    Transform {
        chrome_visible: bool,
        active_handle: usize,
    },
}

impl SceneInteractionFeedback {
    pub const fn chrome_visible(self) -> bool {
        match self {
            Self::Normal => true,
            Self::Creation => false,
            Self::Move { chrome_visible } | Self::Transform { chrome_visible, .. } => {
                chrome_visible
            }
        }
    }

    pub const fn handle_visible(self, handle: usize) -> bool {
        match self {
            Self::Normal => true,
            Self::Transform {
                chrome_visible: true,
                active_handle,
            } => active_handle == handle,
            Self::Creation
            | Self::Move { .. }
            | Self::Transform {
                chrome_visible: false,
                ..
            } => false,
        }
    }
}

#[cfg(test)]
mod scene_interaction_feedback_tests {
    use super::SceneInteractionFeedback;

    #[test]
    fn creation_move_and_transform_filter_chrome_and_handles() {
        assert!(SceneInteractionFeedback::Normal.chrome_visible());
        assert!(SceneInteractionFeedback::Normal.handle_visible(0));
        assert!(!SceneInteractionFeedback::Creation.chrome_visible());
        assert!(!SceneInteractionFeedback::Creation.handle_visible(0));

        let moving = SceneInteractionFeedback::Move {
            chrome_visible: true,
        };
        assert!(moving.chrome_visible());
        assert!(!moving.handle_visible(0));

        let transforming = SceneInteractionFeedback::Transform {
            chrome_visible: true,
            active_handle: 1,
        };
        assert!(transforming.chrome_visible());
        assert!(!transforming.handle_visible(0));
        assert!(transforming.handle_visible(1));

        let snapped = SceneInteractionFeedback::Transform {
            chrome_visible: false,
            active_handle: 1,
        };
        assert!(!snapped.chrome_visible());
        assert!(!snapped.handle_visible(1));
    }
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
    pub feedback: SceneInteractionFeedback,
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
    pub feedback: SceneInteractionFeedback,
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
    pub feedback: SceneInteractionFeedback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneMeasurementPath {
    pub id: MarkupId,
    pub points: Vec<PdfPoint>,
    pub kind: MeasurementPathKind,
    pub appearance: RectangleAppearance,
    pub text_style: TextBoxStyle,
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
    pub feedback: SceneInteractionFeedback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneTextBox {
    pub id: MarkupId,
    pub layout_rect: PdfRect,
    pub content: String,
    pub style: TextBoxStyle,
    pub rich_text_runs: Vec<TextBoxRichTextRun>,
    pub rotation_degrees: f64,
    pub selected: bool,
    pub locked: bool,
    pub feedback: SceneInteractionFeedback,
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
    pub feedback: SceneInteractionFeedback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneLength {
    pub id: MarkupId,
    pub start: PdfPoint,
    pub end: PdfPoint,
    pub caption: String,
    pub show_caption: bool,
    pub appearance: DimensionAppearance,
    pub selected: bool,
    pub locked: bool,
    pub draft: bool,
    pub feedback: SceneInteractionFeedback,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneImage {
    pub id: MarkupId,
    pub rect: PdfRect,
    pub asset_id: ImageAssetId,
    pub width_px: u32,
    pub height_px: u32,
    pub aspect_locked: bool,
    pub opacity: f64,
    pub rotation_degrees: f64,
    pub selected: bool,
    pub locked: bool,
    pub feedback: SceneInteractionFeedback,
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
    pub feedback: SceneInteractionFeedback,
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
    InvalidRecoveryTimeline(String),
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
            Self::InvalidRecoveryTimeline(message) => {
                write!(formatter, "invalid recovery timeline: {message}")
            }
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

const RECOVERY_TIMELINE_SCHEMA_VERSION: u32 = 2;
const MAX_RECOVERY_TIMELINE_BYTES: usize = 256 * 1024 * 1024;
const MAX_RECOVERY_HISTORY_STATES: usize = 10_000;
const MAX_RECOVERY_ANNOTATIONS_PER_STATE: usize = 1_000_000;
const MAX_RECOVERY_ASSETS: usize = 65_536;
const MAX_RECOVERY_ASSET_REFERENCES: usize = 1_000_000;
// Base64 expands by 4/3. Keep decoded bytes below the envelope limit with
// headroom for state metadata; the final serialized-length check remains authoritative.
const MAX_RECOVERY_DECODED_ASSET_BYTES: usize = 191 * 1024 * 1024;
const MAX_RECOVERY_SPATIAL_CELLS_PER_RECTANGLE: usize = 65_536;
const MAX_RECOVERY_SPATIAL_CELLS_PER_STATE: usize = 1_000_000;
const MAX_RECOVERY_ABS_COORDINATE_PT: f64 = 1_000_000.;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryTimelineWire {
    schema_version: u32,
    assets: Vec<DecodedRgbaAssetWire>,
    current: RecoveryDocumentState,
    past: Vec<RecoveryDocumentState>,
    future: Vec<RecoveryDocumentState>,
    history_limit: usize,
    saved_revision: u64,
    next_revision: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryImageAnnotation {
    id: MarkupId,
    page_index: u32,
    rect: PdfRect,
    asset_ref: String,
    opacity: f64,
    rotation_degrees: f64,
    aspect_locked: bool,
    locked: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoverySnapshotAnnotation {
    id: MarkupId,
    page_index: u32,
    rect: PdfRect,
    asset_ref: String,
    opacity: f64,
    rotation_degrees: f64,
    locked: bool,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryDocumentState {
    annotation_order: Vec<MarkupId>,
    rectangles: Vec<RectangleAnnotation>,
    redacts: Vec<RedactAnnotation>,
    ellipses: Vec<EllipseAnnotation>,
    arcs: Vec<ArcAnnotation>,
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
    images: Vec<RecoveryImageAnnotation>,
    snapshots: Vec<RecoverySnapshotAnnotation>,
    page_scales: BTreeMap<u32, PageScale>,
    scale_presets: Vec<ScalePreset>,
    page_length_calibrations: BTreeMap<u32, LengthCalibration>,
    page_rotations: BTreeMap<u32, PageRotation>,
    revision: u64,
}

fn recovery_error(message: impl Into<String>) -> AnnotationError {
    AnnotationError::InvalidRecoveryTimeline(message.into())
}

fn serialize_recovery_wire(
    wire: &RecoveryTimelineWire,
    max_bytes: usize,
) -> Result<Vec<u8>, AnnotationError> {
    let encoded = serde_json::to_vec(wire)
        .map_err(|error| recovery_error(format!("cannot encode timeline: {error}")))?;
    if encoded.len() > max_bytes {
        return Err(recovery_error("encoded timeline exceeds the byte limit"));
    }
    Ok(encoded)
}

fn register_recovery_asset(
    asset: &DecodedRgbaAsset,
    assets: &mut BTreeMap<String, DecodedRgbaAssetWire>,
) -> Result<String, AnnotationError> {
    let id = asset.id().as_str().to_owned();
    assets
        .entry(id.clone())
        .or_insert_with(|| DecodedRgbaAssetWire {
            asset_schema_version: RECOVERY_ASSET_SCHEMA_VERSION,
            id: id.clone(),
            width_px: asset.width_px(),
            height_px: asset.height_px(),
            rgba_base64: BASE64.encode(asset.rgba()),
        });
    Ok(id)
}

impl RecoveryDocumentState {
    fn encode(
        state: &DocumentState,
        assets: &mut BTreeMap<String, DecodedRgbaAssetWire>,
    ) -> Result<Self, AnnotationError> {
        let images = state
            .images
            .iter()
            .map(|annotation| {
                Ok(RecoveryImageAnnotation {
                    id: annotation.id.clone(),
                    page_index: annotation.page_index,
                    rect: annotation.rect,
                    asset_ref: register_recovery_asset(&annotation.asset, assets)?,
                    opacity: annotation.opacity,
                    rotation_degrees: annotation.rotation_degrees,
                    aspect_locked: annotation.aspect_locked,
                    locked: annotation.locked,
                })
            })
            .collect::<Result<_, AnnotationError>>()?;
        let snapshots = state
            .snapshots
            .iter()
            .map(|annotation| {
                Ok(RecoverySnapshotAnnotation {
                    id: annotation.id.clone(),
                    page_index: annotation.page_index,
                    rect: annotation.rect,
                    asset_ref: register_recovery_asset(&annotation.asset, assets)?,
                    opacity: annotation.opacity,
                    rotation_degrees: annotation.rotation_degrees,
                    locked: annotation.locked,
                })
            })
            .collect::<Result<_, AnnotationError>>()?;
        Ok(Self {
            annotation_order: state.annotation_order.clone(),
            rectangles: state.rectangles.clone(),
            redacts: state.redacts.clone(),
            ellipses: state.ellipses.clone(),
            arcs: state.arcs.clone(),
            straight_lines: state.straight_lines.clone(),
            vertex_paths: state.vertex_paths.clone(),
            clouds: state.clouds.clone(),
            cloud_pluses: state.cloud_pluses.clone(),
            callouts: state.callouts.clone(),
            measurement_paths: state.measurement_paths.clone(),
            pens: state.pens.clone(),
            text_boxes: state.text_boxes.clone(),
            dimensions: state.dimensions.clone(),
            lengths: state.lengths.clone(),
            images,
            snapshots,
            page_scales: state.page_scales.clone(),
            scale_presets: state.scale_presets.clone(),
            page_length_calibrations: state.page_length_calibrations.clone(),
            page_rotations: state.page_rotations.clone(),
            revision: state.revision,
        })
    }

    fn decode(
        self,
        assets: &BTreeMap<String, DecodedRgbaAsset>,
        referenced: &mut BTreeSet<String>,
        reference_count: &mut usize,
    ) -> Result<DocumentState, AnnotationError> {
        let resolve = |id: &str,
                       referenced: &mut BTreeSet<String>,
                       reference_count: &mut usize|
         -> Result<DecodedRgbaAsset, AnnotationError> {
            *reference_count = reference_count
                .checked_add(1)
                .ok_or_else(|| recovery_error("asset reference count overflow"))?;
            if *reference_count > MAX_RECOVERY_ASSET_REFERENCES {
                return Err(recovery_error(
                    "timeline contains too many asset references",
                ));
            }
            let asset = assets
                .get(id)
                .ok_or_else(|| recovery_error(format!("missing asset {id}")))?;
            referenced.insert(id.to_owned());
            Ok(asset.clone())
        };
        let images = self
            .images
            .into_iter()
            .map(|wire| {
                let asset = resolve(&wire.asset_ref, referenced, reference_count)?;
                let mut annotation = ImageAnnotation::new_with_opacity(
                    wire.id,
                    wire.page_index,
                    wire.rect,
                    asset,
                    wire.aspect_locked,
                    wire.opacity,
                )?
                .with_rotation_degrees(wire.rotation_degrees)?;
                annotation.locked = wire.locked;
                Ok(annotation)
            })
            .collect::<Result<_, AnnotationError>>()?;
        let snapshots = self
            .snapshots
            .into_iter()
            .map(|wire| {
                let asset = resolve(&wire.asset_ref, referenced, reference_count)?;
                let mut annotation = SnapshotAnnotation::new(
                    wire.id,
                    wire.page_index,
                    wire.rect,
                    asset,
                    wire.opacity,
                )?
                .with_rotation_degrees(wire.rotation_degrees)?;
                annotation.locked = wire.locked;
                Ok(annotation)
            })
            .collect::<Result<_, AnnotationError>>()?;
        let mut state = DocumentState {
            annotation_order: self.annotation_order,
            rectangles: self.rectangles,
            redacts: self.redacts,
            ellipses: self.ellipses,
            arcs: self.arcs,
            rectangle_index: RectangleSpatialIndex::default(),
            straight_lines: self.straight_lines,
            vertex_paths: self.vertex_paths,
            clouds: self.clouds,
            cloud_pluses: self.cloud_pluses,
            callouts: self.callouts,
            measurement_paths: self.measurement_paths,
            pens: self.pens,
            text_boxes: self.text_boxes,
            dimensions: self.dimensions,
            lengths: self.lengths,
            images,
            snapshots,
            page_scales: self.page_scales,
            scale_presets: self.scale_presets,
            page_length_calibrations: self.page_length_calibrations,
            page_rotations: self.page_rotations,
            revision: self.revision,
        };
        validate_recovery_state(&state)?;
        state.rectangle_index = RectangleSpatialIndex::rebuild(&state.rectangles);
        Ok(state)
    }
}

fn validate_recovery_rectangle_appearance(
    appearance: &RectangleAppearance,
) -> Result<(), AnnotationError> {
    let rebuilt = RectangleAppearance::new(
        appearance.stroke_color.clone(),
        appearance.stroke_width_pt,
        appearance.fill_color.clone(),
        appearance.opacity,
    )?
    .with_fill_opacity(appearance.fill_opacity)?
    .with_stroke_style(appearance.stroke_style);
    if &rebuilt != appearance {
        return Err(recovery_error("rectangle appearance is not canonical"));
    }
    Ok(())
}

fn validate_recovery_line_appearance(
    appearance: &StraightLineAppearance,
) -> Result<(), AnnotationError> {
    let rebuilt = StraightLineAppearance::new(
        appearance.stroke_color.clone(),
        appearance.stroke_width_pt,
        appearance.opacity,
        appearance.stroke_style,
    )?;
    if &rebuilt != appearance {
        return Err(recovery_error("line appearance is not canonical"));
    }
    Ok(())
}

fn validate_recovery_pen_appearance(appearance: &PenAppearance) -> Result<(), AnnotationError> {
    let rebuilt = PenAppearance::new(
        appearance.color.clone(),
        appearance.width_pt,
        appearance.opacity,
    )?;
    if &rebuilt != appearance {
        return Err(recovery_error("pen appearance is not canonical"));
    }
    Ok(())
}

fn validate_recovery_text_style(style: &TextBoxStyle) -> Result<(), AnnotationError> {
    let rebuilt = TextBoxStyle::new(
        style.font_family.clone(),
        style.font_size_pt,
        style.color.clone(),
        style.opacity,
    )?
    .with_weight_and_alignment(style.weight, style.alignment)?
    .with_layout_metrics(style.line_height_pt(), style.inset_pt)?;
    if &rebuilt != style {
        return Err(recovery_error("text style is not canonical"));
    }
    Ok(())
}

fn validate_recovery_calibration(calibration: &LengthCalibration) -> Result<(), AnnotationError> {
    for (name, value) in [
        ("units_per_point", calibration.units_per_point),
        ("scale_x", calibration.scale_x),
        ("scale_y", calibration.scale_y),
        ("paper_points", calibration.paper_points),
        ("real_world_value", calibration.real_world_value),
    ] {
        require_finite(name, value)?;
        if value <= 0. {
            return Err(recovery_error(format!("{name} must be positive")));
        }
    }
    if calibration.precision > 12 {
        return Err(recovery_error("calibration precision is out of range"));
    }
    validate_text(
        &calibration.unit,
        "measurement unit",
        MAX_MEASUREMENT_UNIT_BYTES,
    )?;
    if !calibration.label.is_empty() {
        validate_text(
            &calibration.label,
            "measurement label",
            MAX_MEASUREMENT_LABEL_BYTES,
        )?;
    }
    let precision = validate_recovery_scale_precision(calibration.scale_precision)?;
    let reconstructed = LengthCalibration::from_scale(
        calibration.paper_points,
        calibration.real_world_value,
        calibration.unit.clone(),
        calibration.precision,
        calibration.show_caption,
    )?
    .with_label(calibration.label.clone())?;
    if precision != calibration.scale_precision
        || (calibration.units_per_point - reconstructed.units_per_point).abs() > 0.000_001_1
    {
        return Err(recovery_error("calibration scale metadata is inconsistent"));
    }
    Ok(())
}

fn validate_recovery_scale_precision(
    precision: ScalePrecision,
) -> Result<ScalePrecision, AnnotationError> {
    match precision.mode {
        ScalePrecisionMode::Decimal => ScalePrecision::decimal(precision.value),
        ScalePrecisionMode::Fraction => {
            let value = precision.value;
            if !value.is_finite() || value.fract() != 0. || value > f64::from(u16::MAX) {
                return Err(recovery_error("fraction precision is invalid"));
            }
            ScalePrecision::fraction(value as u16)
        }
    }
    .map_err(|error| recovery_error(error.to_string()))
}

fn validate_recovery_rotation(field: &str, rotation: f64) -> Result<(), AnnotationError> {
    require_finite(field, rotation)?;
    if !(0. ..360.).contains(&rotation) || canonical_float(rotation) != rotation {
        return Err(recovery_error(format!("{field} is not canonical")));
    }
    Ok(())
}

fn recovery_rectangle_spatial_cell_count(
    annotation: &RectangleAnnotation,
) -> Result<usize, AnnotationError> {
    let bounds = rectangle_world_bounds(annotation.rect, annotation.rotation_degrees);
    let edges = [
        bounds.x,
        bounds.y,
        bounds.x + bounds.width,
        bounds.y + bounds.height,
    ];
    if edges
        .iter()
        .any(|edge| !edge.is_finite() || edge.abs() > MAX_RECOVERY_ABS_COORDINATE_PT)
    {
        return Err(recovery_error(
            "rectangle bounds exceed the recovery geometry budget",
        ));
    }
    let min_x = (edges[0] / SPATIAL_CELL_PT).floor();
    let min_y = (edges[1] / SPATIAL_CELL_PT).floor();
    let max_x = (edges[2] / SPATIAL_CELL_PT).floor();
    let max_y = (edges[3] / SPATIAL_CELL_PT).floor();
    let columns = (max_x - min_x + 1.) as usize;
    let rows = (max_y - min_y + 1.) as usize;
    let cells = columns
        .checked_mul(rows)
        .ok_or_else(|| recovery_error("rectangle spatial cell count overflow"))?;
    if cells > MAX_RECOVERY_SPATIAL_CELLS_PER_RECTANGLE {
        return Err(recovery_error("rectangle exceeds the spatial cell budget"));
    }
    Ok(cells)
}

fn validate_recovery_state(state: &DocumentState) -> Result<(), AnnotationError> {
    let family_ids = state
        .rectangles
        .iter()
        .map(|a| &a.id)
        .chain(state.redacts.iter().map(|a| &a.id))
        .chain(state.ellipses.iter().map(|a| &a.id))
        .chain(state.arcs.iter().map(|a| &a.id))
        .chain(state.straight_lines.iter().map(|a| &a.id))
        .chain(state.vertex_paths.iter().map(|a| &a.id))
        .chain(state.clouds.iter().map(|a| &a.id))
        .chain(state.cloud_pluses.iter().map(|a| &a.id))
        .chain(state.callouts.iter().map(|a| &a.id))
        .chain(state.measurement_paths.iter().map(|a| &a.id))
        .chain(state.pens.iter().map(|a| &a.id))
        .chain(state.text_boxes.iter().map(|a| &a.id))
        .chain(state.dimensions.iter().map(|a| &a.id))
        .chain(state.lengths.iter().map(|a| &a.id))
        .chain(state.images.iter().map(|a| &a.id))
        .chain(state.snapshots.iter().map(|a| &a.id))
        .collect::<Vec<_>>();
    let annotation_count = family_ids.len();
    if annotation_count > MAX_RECOVERY_ANNOTATIONS_PER_STATE {
        return Err(recovery_error("state contains too many annotations"));
    }
    let mut ids = BTreeSet::new();
    for id in family_ids {
        MarkupId::new(id.as_str()).map_err(|_| recovery_error("invalid markup id"))?;
        if !ids.insert(id.clone()) {
            return Err(recovery_error(format!("duplicate markup id {id}")));
        }
    }
    if state.annotation_order.len() != annotation_count {
        return Err(recovery_error(
            "annotation order length does not match state",
        ));
    }
    let mut ordered_ids = BTreeSet::new();
    for id in &state.annotation_order {
        if !ids.contains(id) || !ordered_ids.insert(id.clone()) {
            return Err(recovery_error(
                "annotation order is missing, duplicate, or unknown",
            ));
        }
    }
    let mut spatial_cell_count = 0usize;
    for annotation in &state.rectangles {
        validate_layout_rect(annotation.rect, "rectangle")?;
        validate_recovery_rotation("rectangle.rotation", annotation.rotation_degrees)?;
        validate_recovery_rectangle_appearance(&annotation.appearance)?;
        spatial_cell_count = spatial_cell_count
            .checked_add(recovery_rectangle_spatial_cell_count(annotation)?)
            .ok_or_else(|| recovery_error("state spatial cell count overflow"))?;
        if spatial_cell_count > MAX_RECOVERY_SPATIAL_CELLS_PER_STATE {
            return Err(recovery_error("state exceeds the spatial cell budget"));
        }
    }
    for annotation in &state.redacts {
        validate_recovery_rectangle_appearance(&annotation.appearance)?;
        RedactAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.rect,
            annotation.redaction_color.clone(),
            annotation.overlay_text.clone(),
            annotation.appearance.clone(),
        )?;
    }
    for annotation in &state.ellipses {
        validate_recovery_rectangle_appearance(&annotation.appearance)?;
        EllipseAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.rect,
            annotation.appearance.clone(),
        )?;
        validate_recovery_rotation("ellipse.rotation", annotation.rotation_degrees)?;
    }
    for annotation in &state.arcs {
        validate_recovery_rectangle_appearance(&annotation.appearance)?;
        if let Some(geometry) = &annotation.ellipse_geometry {
            validate_layout_rect(geometry.rect, "arc")?;
            let expected = ArcAnnotation::from_rect_angles(
                annotation.id.clone(),
                annotation.page_index,
                geometry.rect,
                geometry.angle1_degrees,
                geometry.angle2_degrees,
                annotation.appearance.clone(),
            )?;
            let controls_match = [
                (annotation.start, expected.start),
                (annotation.mid, expected.mid),
                (annotation.end, expected.end),
            ]
            .into_iter()
            .all(|(actual, expected)| {
                (actual.x - expected.x).abs() <= 0.000_1 && (actual.y - expected.y).abs() <= 0.000_1
            });
            if !controls_match {
                return Err(recovery_error(
                    "elliptical Arc controls do not match its rectangle and angles",
                ));
            }
        } else {
            ArcAnnotation::new(
                annotation.id.clone(),
                annotation.page_index,
                annotation.start,
                annotation.end,
                annotation.mid,
                annotation.appearance.clone(),
            )?;
        }
    }
    for annotation in &state.straight_lines {
        validate_recovery_line_appearance(&annotation.appearance)?;
        StraightLineAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.start,
            annotation.end,
            annotation.kind,
            annotation.appearance.clone(),
        )?;
    }
    for annotation in &state.vertex_paths {
        validate_recovery_rectangle_appearance(&annotation.appearance)?;
        VertexPathAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.points.clone(),
            annotation.kind,
            annotation.appearance.clone(),
        )?;
    }
    for annotation in &state.clouds {
        validate_recovery_rectangle_appearance(&annotation.appearance)?;
        CloudAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.points.clone(),
            annotation.border_effect_intensity,
            annotation.appearance.clone(),
        )?;
    }
    for annotation in &state.cloud_pluses {
        validate_recovery_rectangle_appearance(&annotation.appearance.cloud)?;
        validate_recovery_line_appearance(&annotation.appearance.leader)?;
        validate_recovery_text_style(&annotation.appearance.text)?;
        CloudPlusAppearance::new(
            annotation.appearance.cloud.clone(),
            annotation.appearance.leader.clone(),
            annotation.appearance.text.clone(),
        )?;
        CloudPlusAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.cloud_points.clone(),
            annotation.border_effect_intensity,
            annotation.leader_points.clone(),
            annotation.text_box,
            annotation.content.clone(),
            annotation.appearance.clone(),
        )?;
        if let Some(path) = &annotation.cloud_appearance_path {
            validate_cloud_appearance_path(path)?;
        }
    }
    for annotation in &state.callouts {
        validate_recovery_line_appearance(&annotation.appearance.line)?;
        validate_recovery_text_style(&annotation.appearance.text)?;
        CalloutAppearance::new(
            annotation.appearance.line.clone(),
            annotation.appearance.text.clone(),
        )?;
        CalloutAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.leader_points.clone(),
            annotation.text_box,
            annotation.content.clone(),
            annotation.appearance.clone(),
        )?;
    }
    for annotation in &state.measurement_paths {
        validate_recovery_calibration(&annotation.calibration)?;
        validate_recovery_rectangle_appearance(&annotation.appearance)?;
        validate_recovery_text_style(&annotation.text_style)?;
        MeasurementPathAnnotation::new_with_text_style(
            annotation.id.clone(),
            annotation.page_index,
            annotation.points.clone(),
            annotation.kind,
            annotation.calibration.clone(),
            annotation.appearance.clone(),
            annotation.text_style.clone(),
        )?;
    }
    for annotation in &state.pens {
        validate_recovery_pen_appearance(&annotation.appearance)?;
        let paths = annotation.paths().map(<[PdfPoint]>::to_vec).collect();
        match (annotation.tool, annotation.blend_mode) {
            (InkTool::Pen, BlendMode::Normal) => {
                PenAnnotation::new_paths(
                    annotation.id.clone(),
                    annotation.page_index,
                    paths,
                    annotation.appearance.clone(),
                    annotation.smooth_curves,
                )?;
            }
            (InkTool::Highlight, BlendMode::Multiply) if !annotation.smooth_curves => {
                PenAnnotation::new_highlight_paths(
                    annotation.id.clone(),
                    annotation.page_index,
                    paths,
                    annotation.appearance.clone(),
                )?;
            }
            _ => {
                return Err(recovery_error(
                    "ink tool and blend metadata are inconsistent",
                ));
            }
        }
    }
    for annotation in &state.text_boxes {
        validate_recovery_text_style(&annotation.style)?;
        TextBoxAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.layout_rect,
            annotation.content.clone(),
            annotation.style.clone(),
        )?
        .with_rich_text_runs(annotation.rich_text_runs.clone())?;
        validate_recovery_rotation("text.rotation", annotation.rotation_degrees)?;
    }
    for annotation in &state.dimensions {
        validate_recovery_line_appearance(&annotation.appearance.line)?;
        validate_recovery_text_style(&annotation.appearance.text)?;
        DimensionAppearance::new(
            annotation.appearance.line.clone(),
            annotation.appearance.text.clone(),
        )?;
        DimensionAnnotation::new(
            annotation.id.clone(),
            annotation.page_index,
            annotation.start,
            annotation.end,
            annotation.dimension_line_offset,
            annotation.content.clone(),
            annotation.appearance.clone(),
        )?;
    }
    for annotation in &state.lengths {
        validate_recovery_calibration(&annotation.calibration)?;
        validate_recovery_line_appearance(&annotation.appearance.line)?;
        validate_recovery_text_style(&annotation.appearance.text)?;
        DimensionAppearance::new(
            annotation.appearance.line.clone(),
            annotation.appearance.text.clone(),
        )?;
        LengthAnnotation::new_with_appearance(
            annotation.id.clone(),
            annotation.page_index,
            annotation.start,
            annotation.end,
            annotation.calibration.clone(),
            annotation.appearance.clone(),
        )?;
    }
    for (page_index, scale) in &state.page_scales {
        if page_index != &scale.page_index {
            return Err(recovery_error(
                "page scale key does not match its page index",
            ));
        }
        PageScale::from_factors(
            scale.page_index,
            scale.source,
            scale.name.clone(),
            scale.pdf_units,
            scale.real_units,
            scale.scale_x,
            scale.scale_y,
            scale.precision,
        )
        .map_err(|error| recovery_error(error.to_string()))?;
        validate_recovery_scale_precision(scale.precision)?;
    }
    if state
        .page_scales
        .keys()
        .ne(state.page_length_calibrations.keys())
    {
        return Err(recovery_error(
            "page scale and calibration keys are inconsistent",
        ));
    }
    for (page_index, calibration) in &state.page_length_calibrations {
        validate_recovery_calibration(calibration)?;
        let expected = state
            .page_scales
            .get(page_index)
            .expect("matching page scale key was checked")
            .length_calibration()?;
        if !calibration.same_scale_as(&expected) {
            return Err(recovery_error("page scale and calibration values disagree"));
        }
    }
    let mut preset_ids = BTreeSet::new();
    for preset in &state.scale_presets {
        validate_text(&preset.id, "scale preset id", MAX_MEASUREMENT_LABEL_BYTES)?;
        validate_text(
            &preset.name,
            "scale preset name",
            MAX_MEASUREMENT_LABEL_BYTES,
        )?;
        for value in [preset.scale_x, preset.scale_y] {
            if !value.is_finite() || value <= 0. {
                return Err(recovery_error("scale preset factors must be positive"));
            }
        }
        if !preset_ids.insert(&preset.id) {
            return Err(recovery_error("scale preset ids must be unique"));
        }
    }
    Ok(())
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
    focused_id: Option<MarkupId>,
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
            focused_id: None,
            active_gesture: None,
            past: VecDeque::new(),
            future: VecDeque::new(),
            history_limit,
            saved_revision: 0,
            next_revision: 1,
        })
    }

    /// Encodes the committed model and its exact undo/redo timeline. Ephemeral
    /// selection, gesture previews, drafts, spatial indexes, and renderer caches
    /// are deliberately not part of this model-owned recovery contract.
    pub fn encode_recovery_timeline(&self) -> Result<Vec<u8>, AnnotationError> {
        if self.history_limit > MAX_RECOVERY_HISTORY_STATES
            || self.next_revision == u64::MAX
            || self.saved_revision >= self.next_revision
        {
            return Err(recovery_error(
                "document revision or history bounds are not recoverable",
            ));
        }
        let mut assets = BTreeMap::new();
        let current = RecoveryDocumentState::encode(&self.state, &mut assets)?;
        let past = self
            .past
            .iter()
            .map(|state| RecoveryDocumentState::encode(state, &mut assets))
            .collect::<Result<Vec<_>, _>>()?;
        let future = self
            .future
            .iter()
            .map(|state| RecoveryDocumentState::encode(state, &mut assets))
            .collect::<Result<Vec<_>, _>>()?;
        let reference_count = self
            .past
            .iter()
            .chain(std::iter::once(&self.state))
            .chain(self.future.iter())
            .try_fold(0usize, |total, state| {
                total
                    .checked_add(state.images.len())
                    .and_then(|total| total.checked_add(state.snapshots.len()))
                    .ok_or_else(|| recovery_error("asset reference count overflow"))
            })?;
        let aggregate_asset_bytes = assets.values().try_fold(0usize, |total, asset| {
            let bytes = usize::try_from(asset.width_px)
                .ok()
                .and_then(|width| {
                    usize::try_from(asset.height_px)
                        .ok()
                        .and_then(|height| width.checked_mul(height))
                })
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| recovery_error("decoded asset byte count overflow"))?;
            total
                .checked_add(bytes)
                .ok_or_else(|| recovery_error("decoded asset byte count overflow"))
        })?;
        if assets.len() > MAX_RECOVERY_ASSETS
            || reference_count > MAX_RECOVERY_ASSET_REFERENCES
            || aggregate_asset_bytes > MAX_RECOVERY_DECODED_ASSET_BYTES
        {
            return Err(recovery_error("document assets exceed recovery bounds"));
        }
        serialize_recovery_wire(
            &RecoveryTimelineWire {
                schema_version: RECOVERY_TIMELINE_SCHEMA_VERSION,
                assets: assets.into_values().collect(),
                current,
                past,
                future,
                history_limit: self.history_limit,
                saved_revision: self.saved_revision,
                next_revision: self.next_revision,
            },
            MAX_RECOVERY_TIMELINE_BYTES,
        )
    }

    pub fn hydrate_recovery_timeline(bytes: &[u8]) -> Result<Self, AnnotationError> {
        if bytes.len() > MAX_RECOVERY_TIMELINE_BYTES {
            return Err(recovery_error("timeline exceeds the decoder byte limit"));
        }
        let wire: RecoveryTimelineWire = serde_json::from_slice(bytes)
            .map_err(|error| recovery_error(format!("invalid JSON: {error}")))?;
        if !matches!(wire.schema_version, 1 | RECOVERY_TIMELINE_SCHEMA_VERSION) {
            return Err(recovery_error(format!(
                "unsupported schema version {}",
                wire.schema_version
            )));
        }
        if wire.history_limit == 0 || wire.history_limit > MAX_RECOVERY_HISTORY_STATES {
            return Err(recovery_error(
                "history limit is outside the supported range",
            ));
        }
        if wire.next_revision == u64::MAX || wire.saved_revision >= wire.next_revision {
            return Err(recovery_error(
                "saved or next revision is outside the supported range",
            ));
        }
        if wire.past.len() > wire.history_limit
            || wire.future.len() > wire.history_limit
            || wire.past.len() > MAX_RECOVERY_HISTORY_STATES
            || wire.future.len() > MAX_RECOVERY_HISTORY_STATES
        {
            return Err(recovery_error(
                "history contains more states than its limit",
            ));
        }
        if wire.assets.len() > MAX_RECOVERY_ASSETS {
            return Err(recovery_error("timeline contains too many assets"));
        }
        let mut assets = BTreeMap::new();
        let mut aggregate_asset_bytes = 0usize;
        for wire_asset in wire.assets {
            if assets.contains_key(&wire_asset.id) {
                return Err(recovery_error(format!(
                    "duplicate asset id {}",
                    wire_asset.id
                )));
            }
            if wire_asset.asset_schema_version != RECOVERY_ASSET_SCHEMA_VERSION {
                return Err(recovery_error("unsupported decoded RGBA asset version"));
            }
            let rgba = BASE64
                .decode(&wire_asset.rgba_base64)
                .map_err(|_| recovery_error("invalid decoded RGBA base64"))?;
            aggregate_asset_bytes = aggregate_asset_bytes
                .checked_add(rgba.len())
                .ok_or_else(|| recovery_error("decoded asset byte count overflow"))?;
            if aggregate_asset_bytes > MAX_RECOVERY_DECODED_ASSET_BYTES {
                return Err(recovery_error(
                    "decoded assets exceed the aggregate byte limit",
                ));
            }
            let asset = DecodedRgbaAsset::new(wire_asset.width_px, wire_asset.height_px, rgba)
                .map_err(|error| recovery_error(error.to_string()))?;
            if asset.id().as_str() != wire_asset.id {
                return Err(recovery_error("decoded RGBA asset hash mismatch"));
            }
            assets.insert(wire_asset.id, asset);
        }
        let mut referenced = BTreeSet::new();
        let mut reference_count = 0usize;
        let state = wire
            .current
            .decode(&assets, &mut referenced, &mut reference_count)
            .map_err(|error| recovery_error(error.to_string()))?;
        let past = wire
            .past
            .into_iter()
            .map(|state| {
                state
                    .decode(&assets, &mut referenced, &mut reference_count)
                    .map_err(|error| recovery_error(error.to_string()))
            })
            .collect::<Result<VecDeque<_>, _>>()?;
        let future = wire
            .future
            .into_iter()
            .map(|state| {
                state
                    .decode(&assets, &mut referenced, &mut reference_count)
                    .map_err(|error| recovery_error(error.to_string()))
            })
            .collect::<Result<VecDeque<_>, _>>()?;
        if referenced.len() != assets.len() {
            return Err(recovery_error("asset table contains unreachable entries"));
        }
        let mut revisions = BTreeSet::new();
        for revision in past
            .iter()
            .map(|state| state.revision)
            .chain(std::iter::once(state.revision))
            .chain(future.iter().map(|state| state.revision))
        {
            if !revisions.insert(revision) {
                return Err(recovery_error("timeline contains duplicate revisions"));
            }
        }
        if !past
            .iter()
            .map(|state| state.revision)
            .chain(std::iter::once(state.revision))
            .is_sorted()
        {
            return Err(recovery_error("past revisions are out of order"));
        }
        if !std::iter::once(state.revision)
            .chain(future.iter().rev().map(|state| state.revision))
            .is_sorted()
        {
            return Err(recovery_error("future revisions are out of order"));
        }
        let greatest_revision = revisions.last().copied().unwrap_or(0);
        if wire.next_revision < greatest_revision
            || (greatest_revision != u64::MAX && wire.next_revision == greatest_revision)
        {
            return Err(recovery_error("next revision does not follow the timeline"));
        }
        Ok(Self {
            state,
            selected_ids: Vec::new(),
            focused_id: None,
            active_gesture: None,
            past,
            future,
            history_limit: wire.history_limit,
            saved_revision: wire.saved_revision,
            next_revision: wire.next_revision,
        })
    }

    /// Returns every annotation identity retained by current, undo, or redo
    /// state so external recovery storage can reason about historical reachability.
    pub fn recovery_markup_ids(&self) -> BTreeSet<MarkupId> {
        self.past
            .iter()
            .chain(std::iter::once(&self.state))
            .chain(self.future.iter())
            .flat_map(|state| state.annotation_order.iter().cloned())
            .collect()
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
        self.focused_id = None;
        Ok(())
    }

    pub fn selected_id(&self) -> Option<&MarkupId> {
        self.selected_ids.first()
    }

    /// Most recently selected annotation. An empty selection has no focus;
    /// removing the focused annotation falls back to the last remaining one.
    pub fn focused_id(&self) -> Option<&MarkupId> {
        self.focused_id.as_ref()
    }

    fn refresh_focused_id(&mut self, explicit: Option<&MarkupId>) {
        if let Some(id) = explicit {
            if self.selected_ids.contains(id) {
                self.focused_id = Some(id.clone());
                return;
            }
        }
        let still_selected = self
            .focused_id
            .as_ref()
            .is_some_and(|id| self.selected_ids.contains(id));
        if !still_selected {
            self.focused_id = self.selected_ids.last().cloned();
        }
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

    /// Revision of the committed annotation state; changes with every edit.
    pub fn revision(&self) -> u64 {
        self.state.revision
    }

    /// Whether the document has unsaved changes; equal to `snapshot().dirty`
    /// without cloning every annotation.
    pub fn is_dirty(&self) -> bool {
        self.state.revision != self.saved_revision
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: match preview.kind {
                    GestureKind::Create => SceneInteractionFeedback::Creation,
                    GestureKind::Move => SceneInteractionFeedback::Move {
                        chrome_visible: true,
                    },
                    GestureKind::Resize(handle) => SceneInteractionFeedback::Transform {
                        chrome_visible: true,
                        active_handle: RectangleResizeHandle::ALL
                            .iter()
                            .position(|candidate| *candidate == handle)
                            .expect("a rectangle resize gesture uses a known handle"),
                    },
                    GestureKind::Rotate => SceneInteractionFeedback::Transform {
                        chrome_visible: true,
                        active_handle: RectangleResizeHandle::ALL.len(),
                    },
                },
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
            annotation_order: Vec::new(),
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
        .with_document_order(&self.state.annotation_order)
    }

    pub fn thumbnail_scene(&self, page_index: u32) -> AnnotationScene {
        AnnotationScene {
            annotation_order: Vec::new(),
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
                    feedback: SceneInteractionFeedback::Normal,
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
        .with_document_order(&self.state.annotation_order)
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
        self.focused_id = None;
    }

    pub fn select(&mut self, id: &MarkupId) -> bool {
        if !self.contains_annotation(id) {
            return false;
        }
        if !self.selected_ids.contains(id) {
            self.selected_ids.clear();
            self.selected_ids.push(id.clone());
        }
        self.refresh_focused_id(Some(id));
        true
    }

    pub fn toggle_selection(&mut self, id: &MarkupId) -> bool {
        if !self.contains_annotation(id) {
            return false;
        }
        if let Some(index) = self.selected_ids.iter().position(|selected| selected == id) {
            self.selected_ids.remove(index);
            self.refresh_focused_id(None);
        } else {
            self.selected_ids.push(id.clone());
            self.refresh_focused_id(Some(id));
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

    pub(crate) fn annotation_order(&self) -> &[MarkupId] {
        &self.state.annotation_order
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
        self.refresh_focused_id(None);
        &self.selected_ids
    }

    /// Geometric hits in document order, independent of the selection operation.
    /// Preview and release use this same read-only query.
    pub fn marquee_candidates(
        &self,
        page_index: u32,
        marquee: &SelectionMarquee,
        to_viewport: impl Fn(PdfPoint) -> SelectionPoint,
    ) -> Vec<MarkupId> {
        self.marquee_candidates_with_supplement(
            page_index,
            marquee,
            to_viewport,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn marquee_candidates_with_supplement(
        &self,
        page_index: u32,
        marquee: &SelectionMarquee,
        to_viewport: impl Fn(PdfPoint) -> SelectionPoint,
        supplement: &AnnotationSelectionSupplement,
    ) -> Vec<MarkupId> {
        if !marquee.active {
            return Vec::new();
        }
        // Walk page-filtered families once; looking up every ordered id in each
        // family would make a pointer-move preview quadratic in document size.
        let annotations = std::iter::empty::<Annotation>()
            .chain(
                self.state
                    .rectangles
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Rectangle),
            )
            .chain(
                self.state
                    .redacts
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Redact),
            )
            .chain(
                self.state
                    .ellipses
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Ellipse),
            )
            .chain(
                self.state
                    .arcs
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Arc),
            )
            .chain(
                self.state
                    .straight_lines
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::StraightLine),
            )
            .chain(
                self.state
                    .vertex_paths
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::VertexPath),
            )
            .chain(
                self.state
                    .clouds
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Cloud),
            )
            .chain(
                self.state
                    .cloud_pluses
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::CloudPlus),
            )
            .chain(
                self.state
                    .callouts
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Callout),
            )
            .chain(
                self.state
                    .dimensions
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Dimension),
            )
            .chain(
                self.state
                    .measurement_paths
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::MeasurementPath),
            )
            .chain(
                self.state
                    .pens
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Pen),
            )
            .chain(
                self.state
                    .text_boxes
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::TextBox),
            )
            .chain(
                self.state
                    .lengths
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Length),
            )
            .chain(
                self.state
                    .images
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Image),
            )
            .chain(
                self.state
                    .snapshots
                    .iter()
                    .filter(|a| a.page_index == page_index)
                    .cloned()
                    .map(Annotation::Snapshot),
            );
        let hits = annotations
            .filter(|annotation| {
                let mut paths = annotation_selection_paths(annotation, &to_viewport);
                if let Some(points) = supplement.get(annotation.id()) {
                    paths.push(SelectionPath::new(
                        points.iter().copied().map(&to_viewport).collect(),
                        true,
                    ));
                }
                geometry_selected(&paths, marquee)
            })
            .map(|annotation| annotation.id().clone())
            .collect::<std::collections::HashSet<_>>();
        self.state
            .annotation_order
            .iter()
            .filter(|id| hits.contains(*id))
            .cloned()
            .collect()
    }

    pub fn apply_marquee_selection(
        &mut self,
        page_index: u32,
        marquee: &SelectionMarquee,
        to_viewport: impl Fn(PdfPoint) -> SelectionPoint,
    ) -> &[MarkupId] {
        self.apply_marquee_selection_with_supplement(
            page_index,
            marquee,
            to_viewport,
            &AnnotationSelectionSupplement::new(),
        )
    }

    pub fn apply_marquee_selection_with_supplement(
        &mut self,
        page_index: u32,
        marquee: &SelectionMarquee,
        to_viewport: impl Fn(PdfPoint) -> SelectionPoint,
        supplement: &AnnotationSelectionSupplement,
    ) -> &[MarkupId] {
        if !marquee.active {
            return &self.selected_ids;
        }
        let hits =
            self.marquee_candidates_with_supplement(page_index, marquee, to_viewport, supplement);
        self.selected_ids = selection_after(&self.selected_ids, &hits, marquee.operation);
        self.refresh_focused_id(None);
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
        self.refresh_focused_id(None);
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
        self.refresh_focused_id(None);
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
            if !selected.locked
                && let Some(handle) = selected.edge_resize_handle(point, tolerance_pt)
            {
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
        if let Some(hit) = self
            .state
            .straight_lines
            .iter()
            .rev()
            .find(|annotation| {
                annotation.page_index == page_index
                    && point_segment_distance(point, annotation.start, annotation.end)
                        <= tolerance_pt.max(annotation.appearance.stroke_width_pt / 2.0)
            })
            .map(|annotation| HitTarget::Body(annotation.id.clone()))
        {
            return Ok(Some(hit));
        }
        // Both offsets are fixed screen distances expressed in page points.
        let outset_pt =
            rotation_handle_offset_pt * SELECTION_OUTSET_CSS_PX / ROTATION_HANDLE_OFFSET_PT;
        Ok(self
            .selected_outline_zone_hit(page_index, point, tolerance_pt + outset_pt)
            .map(HitTarget::Body))
    }

    /// The selected item whose outline zone contains `point`: its own bounds
    /// grown by `margin_pt`, which covers the gaps inside an unfilled shape
    /// and the band around its outset selection outline. Dragging there moves
    /// the item.
    pub fn selected_outline_zone_hit(
        &self,
        page_index: u32,
        point: PdfPoint,
        margin_pt: f64,
    ) -> Option<MarkupId> {
        self.selected_ids.iter().rev().find_map(|id| {
            let annotation = self.annotation_owned(id)?;
            if annotation.page_index() != page_index {
                return None;
            }
            if let Annotation::Rectangle(rectangle) = &annotation {
                let local = rectangle.world_to_local(point);
                return rectangle.rect.contains(local, margin_pt).then(|| id.clone());
            }
            let paths = annotation_selection_paths(&annotation, &|sample: PdfPoint| {
                SelectionPoint { x: sample.x, y: sample.y }
            });
            let mut samples = paths.iter().flat_map(|path| path.points.iter());
            let first = samples.next()?;
            let (mut min_x, mut min_y, mut max_x, mut max_y) = (first.x, first.y, first.x, first.y);
            for sample in samples {
                min_x = min_x.min(sample.x);
                min_y = min_y.min(sample.y);
                max_x = max_x.max(sample.x);
                max_y = max_y.max(sample.y);
            }
            (point.x >= min_x - margin_pt
                && point.x <= max_x + margin_pt
                && point.y >= min_y - margin_pt
                && point.y <= max_y + margin_pt)
                .then(|| id.clone())
        })
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
        self.refresh_focused_id(hit.as_ref().map(|target| target.markup_id()));
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
                self.focused_id = self.selected_ids.last().cloned();
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
                self.focused_id = self.selected_ids.last().cloned();
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
        self.focused_id = self.selected_ids.last().cloned();
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
                if annotation.content == content && annotation.rich_text_runs.is_empty() {
                    return Ok((AnnotationKind::TextBox, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .text_boxes
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated text box edit must retain its target");
                    annotation.content = content;
                    annotation.rich_text_runs.clear();
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
            AnnotationEdit::SetTextBoxRotation(rotation_degrees) => {
                require_finite("text_box.rotation", rotation_degrees)?;
                let rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
                let annotation = self.text_box(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.rotation_degrees == rotation_degrees {
                    return Ok((AnnotationKind::TextBox, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .text_boxes
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated text box rotation edit must retain its target")
                        .rotation_degrees = rotation_degrees;
                });
                Ok((AnnotationKind::TextBox, true))
            }
            AnnotationEdit::SetTextBoxStyle(style) => {
                let annotation = self.text_box(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.style == style {
                    return Ok((AnnotationKind::TextBox, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .text_boxes
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated text style edit must retain its target")
                        .style = style;
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
                let current = match control {
                    ArcControlPoint::Start => annotation.start,
                    ArcControlPoint::Mid => annotation.mid,
                    ArcControlPoint::End => annotation.end,
                };
                if point == current {
                    return Ok((AnnotationKind::Arc, false));
                }
                let replacement = annotation.with_control_point(control, point)?;
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
                validate_optional_text(&content, "dimension content", MAX_TEXT_BOX_BYTES)?;
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
            AnnotationEdit::SetLengthAppearance(appearance) => {
                let annotation = self.length(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance {
                    return Ok((AnnotationKind::Length, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .lengths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated length edit must retain its target")
                        .appearance = appearance;
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
            AnnotationEdit::SetVertexPathAppearance(appearance) => {
                let annotation = self.vertex_path(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance {
                    return Ok((annotation.kind.into(), false));
                }
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .vertex_paths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated vertex-path appearance edit must retain its target")
                        .appearance = appearance;
                });
                Ok((kind.into(), true))
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
            AnnotationEdit::SetCloudAppearance(appearance) => {
                let annotation = self.cloud(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance {
                    return Ok((AnnotationKind::Cloud, false));
                }
                let mut replacement = CloudAnnotation::new(
                    annotation.id.clone(),
                    annotation.page_index,
                    annotation.points.clone(),
                    annotation.border_effect_intensity,
                    appearance,
                )?;
                replacement.locked = annotation.locked;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .clouds
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud appearance edit must retain its target") =
                        replacement;
                });
                Ok((AnnotationKind::Cloud, true))
            }
            AnnotationEdit::SetCloudIntensity(border_effect_intensity) => {
                let annotation = self.cloud(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.border_effect_intensity == border_effect_intensity {
                    return Ok((AnnotationKind::Cloud, false));
                }
                let mut replacement = CloudAnnotation::new(
                    annotation.id.clone(),
                    annotation.page_index,
                    annotation.points.clone(),
                    border_effect_intensity,
                    annotation.appearance.clone(),
                )?;
                replacement.locked = annotation.locked;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .clouds
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Cloud intensity edit must retain its target") =
                        replacement;
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
                validate_optional_text(&content, "Cloud+ content", MAX_TEXT_BOX_BYTES)?;
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
                )?
                .with_cloud_appearance_path(annotation.cloud_appearance_path.clone())?;
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
                validate_optional_text(&content, "Cloud+ content", MAX_TEXT_BOX_BYTES)?;
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
                validate_optional_text(&content, "callout content", MAX_TEXT_BOX_BYTES)?;
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
                let locked = replacement.locked;
                replacement = CalloutAnnotation::new(
                    replacement.id.clone(),
                    replacement.page_index,
                    replacement.leader_points.clone(),
                    replacement.text_box,
                    replacement.content.clone(),
                    replacement.appearance.clone(),
                )?;
                replacement.locked = locked;
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
            AnnotationEdit::SetCalloutTextBox(text_box) => {
                let annotation = self.callout(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.text_box == text_box {
                    return Ok((AnnotationKind::Callout, false));
                }
                let replacement = annotation.resized_text_box(text_box)?;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    *state
                        .callouts
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated Callout resize must retain its target") = replacement;
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
                replacement = replacement.canonicalized_for_disk()?;
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
                replacement = replacement.canonicalized_for_disk()?;
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
            AnnotationEdit::SetMeasurementPathCalibration(calibration) => {
                let annotation = self
                    .measurement_path(id)
                    .ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.calibration == calibration {
                    return Ok((annotation.kind.into(), false));
                }
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .measurement_paths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated measurement-path edit must retain its target")
                        .calibration = calibration;
                });
                Ok((kind.into(), true))
            }
            AnnotationEdit::SetMeasurementPathAppearance {
                appearance,
                text_style,
            } => {
                let annotation = self
                    .measurement_path(id)
                    .ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.appearance == appearance && annotation.text_style == text_style {
                    return Ok((annotation.kind.into(), false));
                }
                let kind = annotation.kind;
                let id = id.clone();
                self.commit_state_change(move |state| {
                    let annotation = state
                        .measurement_paths
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated measurement-path edit must retain its target");
                    annotation.appearance = appearance;
                    annotation.text_style = text_style;
                });
                Ok((kind.into(), true))
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
            AnnotationEdit::SetImageRotation(rotation_degrees) => {
                require_finite("image.rotation", rotation_degrees)?;
                let rotation_degrees = canonical_float(rotation_degrees.rem_euclid(360.));
                let annotation = self.image(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                if annotation.rotation_degrees == rotation_degrees {
                    return Ok((AnnotationKind::Image, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .images
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated image rotation edit must retain its target")
                        .rotation_degrees = rotation_degrees;
                });
                Ok((AnnotationKind::Image, true))
            }
            AnnotationEdit::SetImageOpacity(opacity) => {
                validate_snapshot_opacity(opacity)?;
                let annotation = self.image(id).ok_or(AnnotationError::NoSelection)?;
                if annotation.locked {
                    return Err(AnnotationError::LockedMarkup(id.clone()));
                }
                let opacity = canonical_float(opacity);
                if annotation.opacity == opacity {
                    return Ok((AnnotationKind::Image, false));
                }
                let id = id.clone();
                self.commit_state_change(move |state| {
                    state
                        .images
                        .iter_mut()
                        .find(|annotation| annotation.id == id)
                        .expect("a validated image edit must retain its target")
                        .opacity = opacity;
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Creation,
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Normal,
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
                rect: annotation.rect(),
                angle1_degrees: annotation.angle1_degrees(),
                angle2_degrees: annotation.angle2_degrees(),
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
                text_style: annotation.text_style.clone(),
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
                rich_text_runs: annotation.rich_text_runs.clone(),
                rotation_degrees: annotation.rotation_degrees,
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                feedback: SceneInteractionFeedback::Normal,
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
                appearance: annotation.appearance.clone(),
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                draft: false,
                feedback: SceneInteractionFeedback::Normal,
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
                opacity: annotation.opacity,
                rotation_degrees: annotation.rotation_degrees,
                selected: editor_state && self.selected_ids.contains(&annotation.id),
                locked: annotation.locked,
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Normal,
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
                feedback: SceneInteractionFeedback::Normal,
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
        self.refresh_focused_id(None);
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
        Annotation::Cloud(annotation) => vec![path(annotation.points.clone(), true)],
        Annotation::CloudPlus(annotation) => {
            let mut paths = vec![path(annotation.cloud_points.clone(), true)];
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
            let delta_x = annotation.end.x - annotation.start.x;
            let delta_y = annotation.end.y - annotation.start.y;
            let length = delta_x.hypot(delta_y);
            let sign = if annotation.dimension_line_offset >= 0. {
                1.
            } else {
                -1.
            };
            // The dimension line plus the extension-line overhang past it,
            // rather than the baseline or the full painted extension lines.
            let overhang_x = -delta_y / length * sign * DIMENSION_LEADER_EXTENSION_PT;
            let overhang_y = delta_x / length * sign * DIMENSION_LEADER_EXTENSION_PT;
            vec![path(
                vec![
                    PdfPoint {
                        x: dimension_start.x + overhang_x,
                        y: dimension_start.y + overhang_y,
                    },
                    dimension_start,
                    dimension_end,
                    PdfPoint {
                        x: dimension_end.x + overhang_x,
                        y: dimension_end.y + overhang_y,
                    },
                ],
                false,
            )]
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
            // Revu draws a Length as a dimension line `LL` above the points.
            match measurement_line_layout(
                annotation.start,
                annotation.end,
                LENGTH_LEADER_LENGTH_PT,
                annotation.appearance.line().stroke_width_pt(),
                0.,
            ) {
                Some(layout) => vec![path(
                    vec![
                        layout.extension_lines[0].0,
                        layout.extension_lines[0].1,
                        layout.extension_lines[1].1,
                        layout.extension_lines[1].0,
                    ],
                    false,
                )],
                None => vec![path(vec![annotation.start, annotation.end], false)],
            }
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

/// Whole curls round the perimeter, their spacing and radius.
fn cloud_curl_layout(perimeter: f64, nominal_spacing: f64) -> (usize, f64, f64) {
    let count = ((perimeter / nominal_spacing.max(1.)).round() as usize).max(3);
    let spacing = perimeter / count as f64;
    (count, spacing, spacing * 0.6)
}

/// The curl radius Revu uses for a cloud outline.
pub fn cloud_curl_radius(control_path: &[PdfPoint], border_effect_intensity: f64) -> f64 {
    let perimeter = (0..control_path.len())
        .map(|index| {
            point_distance(control_path[index], control_path[(index + 1) % control_path.len()])
        })
        .sum::<f64>();
    cloud_curl_layout(perimeter, cloud_nominal_spacing(border_effect_intensity)).2
}

fn cloud_nominal_spacing(border_effect_intensity: f64) -> f64 {
    DEFAULT_CLOUD_SCALLOP_RADIUS_PT * (border_effect_intensity / 2.0).max(0.25)
}

/// Revu's (Adobe's) cloudy border. Curls are circles of one radius centred
/// on the outline at equal spacing along its perimeter, starting at the first
/// vertex: the nominal spacing is rounded to a whole number of curls and the
/// radius is 0.6 of the actual spacing. Each curl is an outward arc from where
/// it meets the previous curl to where it meets the next, continuing 21.2°
/// past that point and hooking back to it. Measured from Revu 21 appearance
/// streams (intensity 2: 14.09 pt spacing, 8.46 pt radius).
fn sampled_cloud_scallop_path(control_path: &[PdfPoint], nominal_spacing: f64) -> Vec<PdfPoint> {
    const OVERSHOOT_RADIANS: f64 = 21.2 * std::f64::consts::PI / 180.;
    const HOOK_HANDLE_RATIO: f64 = 0.1034;
    const ARC_STEP_RADIANS: f64 = 10. * std::f64::consts::PI / 180.;
    if control_path.len() < 3 {
        return control_path.to_vec();
    }
    let vertices = control_path.len();
    let edges = (0..vertices)
        .map(|index| (control_path[index], control_path[(index + 1) % vertices]))
        .filter(|(from, to)| point_distance(*from, *to) > f64::EPSILON)
        .collect::<Vec<_>>();
    let perimeter = edges
        .iter()
        .map(|(from, to)| point_distance(*from, *to))
        .sum::<f64>();
    if edges.len() < 2 || !perimeter.is_finite() || perimeter <= f64::EPSILON {
        return control_path.to_vec();
    }
    let signed_area = control_path
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = control_path[(index + 1) % vertices];
            point.x * next.y - next.x * point.y
        })
        .sum::<f64>();
    // Curls turn the same way as the outline: on a clockwise outline (as
    // Revu draws them) angles decrease and curls bulge left of travel.
    let turn = if signed_area < 0. { -1. } else { 1. };
    let (count, spacing, radius) = cloud_curl_layout(perimeter, nominal_spacing);
    let mut centres = Vec::with_capacity(count);
    let mut edge_index = 0;
    let mut edge_start = 0.;
    for curl in 0..count {
        let distance = curl as f64 * spacing;
        while edge_index + 1 < edges.len()
            && edge_start + point_distance(edges[edge_index].0, edges[edge_index].1) < distance
        {
            edge_start += point_distance(edges[edge_index].0, edges[edge_index].1);
            edge_index += 1;
        }
        let (from, to) = edges[edge_index];
        let length = point_distance(from, to);
        let t = ((distance - edge_start) / length).clamp(0., 1.);
        centres.push(PdfPoint {
            x: from.x + (to.x - from.x) * t,
            y: from.y + (to.y - from.y) * t,
        });
    }
    let meeting = |first: PdfPoint, second: PdfPoint| {
        let dx = second.x - first.x;
        let dy = second.y - first.y;
        let distance = dx.hypot(dy);
        let mid = PdfPoint {
            x: (first.x + second.x) / 2.,
            y: (first.y + second.y) / 2.,
        };
        if distance <= f64::EPSILON {
            return mid;
        }
        let half_chord = (radius * radius - distance * distance / 4.).max(0.).sqrt();
        PdfPoint {
            x: mid.x + turn * dy / distance * half_chord,
            y: mid.y - turn * dx / distance * half_chord,
        }
    };
    let on_circle = |centre: PdfPoint, angle: f64| PdfPoint {
        x: canonical_float(centre.x + radius * angle.cos()),
        y: canonical_float(centre.y + radius * angle.sin()),
    };
    let mut outline = Vec::new();
    for curl in 0..count {
        let centre = centres[curl];
        let start = meeting(centres[(curl + count - 1) % count], centre);
        let end = meeting(centre, centres[(curl + 1) % count]);
        let start_angle = (start.y - centre.y).atan2(start.x - centre.x);
        let end_angle = (end.y - centre.y).atan2(end.x - centre.x);
        let mut sweep = (turn * (end_angle - start_angle)).rem_euclid(std::f64::consts::TAU);
        if sweep <= f64::EPSILON {
            sweep = std::f64::consts::TAU;
        }
        let total = sweep + OVERSHOOT_RADIANS;
        let steps = (total / ARC_STEP_RADIANS).ceil().max(1.) as usize;
        if outline.is_empty() {
            outline.push(on_circle(centre, start_angle));
        }
        for step in 1..=steps {
            let angle = start_angle + turn * total * step as f64 / steps as f64;
            outline.push(on_circle(centre, angle));
        }
        // The hook: back from the overshoot to the meeting point, leaving and
        // arriving along this curl's tangent.
        let overshoot_angle = start_angle + turn * total;
        let tangent = |angle: f64| (-turn * angle.sin(), turn * angle.cos());
        let handle = radius * HOOK_HANDLE_RATIO;
        let hook_start = on_circle(centre, overshoot_angle);
        let (leave_x, leave_y) = tangent(overshoot_angle);
        let (arrive_x, arrive_y) = tangent(end_angle);
        let control_1 = PdfPoint {
            x: hook_start.x - leave_x * handle,
            y: hook_start.y - leave_y * handle,
        };
        let control_2 = PdfPoint {
            x: end.x + arrive_x * handle,
            y: end.y + arrive_y * handle,
        };
        for sample in 1..=4 {
            let t = sample as f64 / 4.;
            let u = 1. - t;
            let blend = |a: f64, b: f64, c: f64, d: f64| {
                u * u * u * a + 3. * u * u * t * b + 3. * u * t * t * c + t * t * t * d
            };
            outline.push(PdfPoint {
                x: canonical_float(blend(hook_start.x, control_1.x, control_2.x, end.x)),
                y: canonical_float(blend(hook_start.y, control_1.y, control_2.y, end.y)),
            });
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

/// Callout, Cloud+ and Dimension text may be empty, as in Revu.
fn validate_optional_text(value: &str, field: &str, max_bytes: usize) -> Result<(), AnnotationError> {
    if value.len() > max_bytes || value.contains('\0') {
        return Err(AnnotationError::InvalidGeometry(format!(
            "{field} must contain at most {max_bytes} UTF-8 bytes without NUL"
        )));
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

fn validate_text_box_rich_text_runs(
    content: &str,
    rich_text_runs: &[TextBoxRichTextRun],
) -> Result<(), AnnotationError> {
    if rich_text_runs.is_empty() {
        return Ok(());
    }
    let content_bytes = content.as_bytes();
    let mut offset = 0usize;
    for run in rich_text_runs {
        validate_text(&run.text, "rich text run", MAX_TEXT_BOX_BYTES)?;
        offset = offset.checked_add(run.text.len()).ok_or_else(|| {
            AnnotationError::InvalidGeometry("rich text run length overflow".into())
        })?;
        if offset > MAX_TEXT_BOX_BYTES
            || content_bytes.get(offset - run.text.len()..offset) != Some(run.text.as_bytes())
        {
            return Err(AnnotationError::InvalidGeometry(
                "rich text runs must concatenate to the text box content".into(),
            ));
        }
        if let Some(font_family) = &run.font_family {
            validate_text(font_family, "rich text font family", MAX_FONT_FAMILY_BYTES)?;
        }
        if let Some(color) = &run.color
            && normalize_color(color.clone())? != *color
        {
            return Err(AnnotationError::InvalidAppearance(
                "rich text color must be canonical".into(),
            ));
        }
        if let Some(font_size_pt) = run.font_size_pt {
            require_finite("rich_text.font_size_pt", font_size_pt)?;
            if font_size_pt <= 0.0 || canonical_float(font_size_pt) != font_size_pt {
                return Err(AnnotationError::InvalidAppearance(
                    "rich text font size must be positive and canonical".into(),
                ));
            }
        }
    }
    if offset != content_bytes.len() {
        return Err(AnnotationError::InvalidGeometry(
            "rich text runs must concatenate to the text box content".into(),
        ));
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

fn normalize_arc_sweep(angle1_degrees: f64, angle2_degrees: f64) -> f64 {
    let mut sweep = angle2_degrees - angle1_degrees;
    while sweep <= -360. {
        sweep += 360.;
    }
    while sweep > 360. {
        sweep -= 360.;
    }
    canonical_float(sweep)
}

fn ellipse_point(rect: PdfRect, angle_degrees: f64) -> PdfPoint {
    let angle = angle_degrees.to_radians();
    PdfPoint {
        x: canonical_float(rect.x + rect.width * 0.5 + rect.width * 0.5 * angle.cos()),
        y: canonical_float(rect.y + rect.height * 0.5 + rect.height * 0.5 * angle.sin()),
    }
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
        "cloudAppearancePath": annotation.cloud_appearance_path.as_deref().map(|path| path.iter().map(|command| match command {
            CloudAppearancePathCommand::MoveTo(point) => json!({"command": "move", "point": {"x": point.x, "y": point.y}}),
            CloudAppearancePathCommand::LineTo(point) => json!({"command": "line", "point": {"x": point.x, "y": point.y}}),
            CloudAppearancePathCommand::CubicTo { control_1, control_2, end } => json!({
                "command": "cubic",
                "control1": {"x": control_1.x, "y": control_1.y},
                "control2": {"x": control_2.x, "y": control_2.y},
                "end": {"x": end.x, "y": end.y},
            }),
            CloudAppearancePathCommand::Close => json!({"command": "close"}),
        }).collect::<Vec<_>>()),
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
    let mut value = json!({
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
    });
    if annotation.ellipse_geometry.is_some() {
        let object = value
            .as_object_mut()
            .expect("canonical Arc JSON is an object");
        object.insert("rect".into(), json!(annotation.rect()));
        object.insert("angle1Degrees".into(), json!(annotation.angle1_degrees()));
        object.insert("angle2Degrees".into(), json!(annotation.angle2_degrees()));
    }
    value
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
    let mut value = json!({
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
    });
    if annotation.rotation_degrees != 0.0 {
        value["rotation"] = json!(annotation.rotation_degrees);
    }
    if !annotation.rich_text_runs.is_empty() {
        value["richTextRuns"] = Value::Array(
            annotation
                .rich_text_runs
                .iter()
                .map(|run| {
                    let mut value = Map::from_iter([
                        ("bold".into(), Value::Bool(run.bold)),
                        ("italic".into(), Value::Bool(run.italic)),
                        ("text".into(), Value::String(run.text.clone())),
                    ]);
                    if let Some(font_family) = &run.font_family {
                        value.insert("fontFamily".into(), Value::String(font_family.clone()));
                    }
                    if let Some(color) = &run.color {
                        value.insert("color".into(), Value::String(color.clone()));
                    }
                    if let Some(font_size_pt) = run.font_size_pt {
                        value.insert("fontSizePt".into(), json!(font_size_pt));
                    }
                    Value::Object(value)
                })
                .collect(),
        );
    }
    value
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
    let mut value = json!({
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
    });
    if annotation.rotation_degrees != 0.0 {
        value["rotation"] = json!(annotation.rotation_degrees);
    }
    value
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

    #[test]
    fn elliptical_arc_geometry_translates_reshapes_and_recovers_without_circularising() {
        let rect = PdfRect::new(10., 20., 220., 110.).unwrap();
        let arc = ArcAnnotation::from_rect_angles(
            id("ellipse-arc"),
            3,
            rect,
            0.,
            180.,
            RectangleAppearance::default(),
        )
        .unwrap();
        assert_eq!(arc.start, point(230., 75.));
        assert_eq!(arc.mid, point(120., 130.));
        assert_eq!(arc.end, point(10., 75.));
        assert_eq!(arc.sampled_path(2), vec![arc.start, arc.mid, arc.end]);

        let translated = arc.translated(15., -5.).unwrap();
        assert_eq!(
            translated.rect(),
            PdfRect::new(25., 15., 220., 110.).unwrap()
        );
        assert_eq!(translated.angle1_degrees(), 0.);
        assert_eq!(translated.angle2_degrees(), 180.);

        let reshaped = arc
            .with_control_point(ArcControlPoint::Mid, point(120., 145.))
            .unwrap();
        assert!((reshaped.rect().width / reshaped.rect().height - 2.).abs() < 1e-9);
        assert_ne!(reshaped.rect(), arc.rect());

        let mut document = AnnotationDocument::with_history_limit(4).unwrap();
        document
            .insert_annotations(vec![Annotation::Arc(arc.clone())])
            .unwrap();
        let encoded = document.encode_recovery_timeline().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&encoded).unwrap()["schema_version"],
            RECOVERY_TIMELINE_SCHEMA_VERSION
        );
        let recovered = AnnotationDocument::hydrate_recovery_timeline(&encoded).unwrap();
        assert!(recovered.arcs()[0].same_persisted_state_as(&arc));

        let mut malformed = serde_json::from_slice::<Value>(&encoded).unwrap();
        malformed["current"]["arcs"][0]["ellipse_geometry"]["rect"]["width"] = json!(0.0);
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(&serde_json::to_vec(&malformed).unwrap())
                .is_err()
        );

        let legacy_circle = ArcAnnotation::new(
            id("legacy-circle-arc"),
            0,
            point(0., 0.),
            point(20., 0.),
            point(10., 8.),
            RectangleAppearance::default(),
        )
        .unwrap();
        let mut legacy_document = AnnotationDocument::with_history_limit(4).unwrap();
        legacy_document
            .insert_annotations(vec![Annotation::Arc(legacy_circle.clone())])
            .unwrap();
        let mut legacy =
            serde_json::from_slice::<Value>(&legacy_document.encode_recovery_timeline().unwrap())
                .unwrap();
        legacy["schema_version"] = json!(1);
        let legacy_recovered =
            AnnotationDocument::hydrate_recovery_timeline(&serde_json::to_vec(&legacy).unwrap())
                .unwrap();
        assert!(legacy_recovered.arcs()[0].same_persisted_state_as(&legacy_circle));

        assert!(
            ArcAnnotation::from_rect_angles(
                id("invalid-ellipse-arc"),
                0,
                rect,
                f64::NAN,
                180.,
                RectangleAppearance::default(),
            )
            .is_err()
        );
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

    fn recovery_document_with_future_only_assets() -> AnnotationDocument {
        let asset = DecodedRgbaAsset::new(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let mut image = ImageAnnotation::new_with_opacity(
            id("recovery:image:雪"),
            2,
            PdfRect::new(10., 20., 30., 15.).unwrap(),
            asset.clone(),
            true,
            0.35,
        )
        .unwrap()
        .with_rotation_degrees(450.)
        .unwrap();
        image.locked = true;
        let snapshot = SnapshotAnnotation::new(
            id("recovery:snapshot:é"),
            3,
            PdfRect::new(50., 60., 70., 80.).unwrap(),
            asset,
            0.625,
        )
        .unwrap()
        .with_rotation_degrees(-30.)
        .unwrap()
        .with_locked(true);
        let mut document = AnnotationDocument::with_history_limit(7).unwrap();
        document
            .insert_annotations(vec![
                Annotation::Image(image),
                Annotation::Snapshot(snapshot),
            ])
            .unwrap();
        assert!(matches!(
            document
                .apply_command(AnnotationCommand::MarkSaved)
                .unwrap(),
            CommandOutcome::Saved { revision: 1 }
        ));
        document.commit_state_change(|state| {
            state.annotation_order.clear();
            state.images.clear();
            state.snapshots.clear();
        });
        assert_eq!(document.state.revision, 2);
        assert!(document.undo().unwrap());
        assert!(document.undo().unwrap());
        assert!(document.images().is_empty());
        assert!(document.snapshots().is_empty());
        assert_eq!(document.history_depths(), (0, 2));
        document
    }

    #[test]
    fn recovery_timeline_round_trips_exact_history_assets_and_revision_semantics() {
        let document = recovery_document_with_future_only_assets();
        let encoded = document.encode_recovery_timeline().unwrap();
        assert!(encoded.len() <= MAX_RECOVERY_TIMELINE_BYTES);
        let wire: RecoveryTimelineWire = serde_json::from_slice(&encoded).unwrap();
        assert!(serialize_recovery_wire(&wire, encoded.len() - 1).is_err());
        let boundary_encoded = serialize_recovery_wire(&wire, encoded.len()).unwrap();
        assert_eq!(boundary_encoded, encoded);
        AnnotationDocument::hydrate_recovery_timeline(&boundary_encoded).unwrap();
        let encoded_json: Value = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(
            encoded_json["schema_version"],
            RECOVERY_TIMELINE_SCHEMA_VERSION
        );
        assert_eq!(encoded_json["history_limit"], 7);
        assert_eq!(encoded_json["assets"].as_array().unwrap().len(), 1);
        assert!(
            !encoded
                .windows(8)
                .any(|window| window == [1, 2, 3, 4, 5, 6, 7, 8])
        );

        let mut hydrated = AnnotationDocument::hydrate_recovery_timeline(&encoded).unwrap();
        assert_eq!(hydrated.history_depths(), (0, 2));
        assert_eq!(hydrated.saved_revision, 1);
        assert_eq!(hydrated.next_revision, 3);
        assert!(hydrated.snapshot().dirty);
        assert!(hydrated.selected_ids.is_empty());
        assert!(hydrated.focused_id.is_none());
        assert!(hydrated.active_gesture.is_none());
        assert_eq!(
            hydrated.recovery_markup_ids(),
            BTreeSet::from([id("recovery:image:雪"), id("recovery:snapshot:é")])
        );

        assert!(hydrated.redo().unwrap());
        assert_eq!(hydrated.snapshot().revision, 1);
        assert!(!hydrated.snapshot().dirty);
        assert_eq!(hydrated.images()[0].rotation_degrees(), 90.);
        assert_eq!(hydrated.images()[0].opacity(), 0.35);
        assert!(hydrated.images()[0].aspect_locked);
        assert!(hydrated.images()[0].locked);
        assert_eq!(
            hydrated.images()[0].asset().rgba(),
            &[1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(hydrated.snapshots()[0].rotation_degrees(), 330.);
        assert_eq!(hydrated.snapshots()[0].opacity(), 0.625);
        assert!(hydrated.snapshots()[0].locked);
        assert_eq!(
            hydrated.snapshots()[0].asset().rgba(),
            &[1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert!(Arc::ptr_eq(
            &hydrated.images()[0].asset.rgba,
            &hydrated.snapshots()[0].asset.rgba
        ));
        assert!(hydrated.redo().unwrap());
        assert_eq!(hydrated.snapshot().revision, 2);
        assert!(hydrated.snapshot().dirty);
        assert!(hydrated.images().is_empty());
        assert!(hydrated.undo().unwrap());
        assert_eq!(hydrated.images().len(), 1);
    }

    #[test]
    fn recovery_timeline_rebuilds_spatial_index_and_excludes_selection_and_preview() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "recovery:rectangle");
        assert!(document.select(&id("recovery:rectangle")));
        document
            .begin_create(
                99,
                id("recovery:preview"),
                0,
                point(300., 300.),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(99, point(360., 360.)).unwrap();

        let encoded = document.encode_recovery_timeline().unwrap();
        let hydrated = AnnotationDocument::hydrate_recovery_timeline(&encoded).unwrap();
        assert_eq!(hydrated.rectangles().len(), 1);
        assert_eq!(hydrated.rectangles()[0].id, id("recovery:rectangle"));
        assert!(hydrated.selected_ids.is_empty());
        assert!(hydrated.active_gesture.is_none());
        assert_eq!(
            hydrated
                .spatial_query_work(0, point(50., 40.), 1.)
                .unwrap()
                .candidate_count,
            1
        );
        let mut huge_geometry: Value = serde_json::from_slice(&encoded).unwrap();
        huge_geometry["current"]["rectangles"][0]["rect"]["width"] = json!(1.0e100);
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(
                &serde_json::to_vec(&huge_geometry).unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn recovery_timeline_accepts_constructor_canonical_calibration_ratio() {
        let calibration = LengthCalibration::from_scale(
            65.395_838_068_188_72,
            590.682_577_254_596_5,
            "m",
            6,
            true,
        )
        .unwrap();
        assert_ne!(
            calibration.units_per_point,
            canonical_float(calibration.real_world_value / calibration.paper_points)
        );
        let length = LengthAnnotation::new(
            id("recovery:nontrivial-calibration"),
            0,
            point(0., 0.),
            point(100., 0.),
            calibration.clone(),
        )
        .unwrap();
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(vec![Annotation::Length(length)], Vec::new())
            .unwrap();
        let encoded = document.encode_recovery_timeline().unwrap();
        let hydrated = AnnotationDocument::hydrate_recovery_timeline(&encoded).unwrap();
        assert_eq!(hydrated.lengths()[0].calibration, calibration);

        let mut corrupt: Value = serde_json::from_slice(&encoded).unwrap();
        corrupt["current"]["lengths"][0]["calibration"]["units_per_point"] = json!(123.0);
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(&serde_json::to_vec(&corrupt).unwrap())
                .is_err()
        );
    }

    #[test]
    fn recovery_timeline_round_trips_every_annotation_family_and_private_state() {
        let rectangle_appearance = RectangleAppearance::new("#123456", 2.5, Some("#abcdef"), 0.75)
            .unwrap()
            .with_fill_opacity(0.25)
            .unwrap()
            .with_stroke_style(StrokeStyle::Dashed);
        let line_appearance =
            StraightLineAppearance::new("#123456", 2.5, 0.75, StrokeStyle::Dashed).unwrap();
        let text_style = TextBoxStyle::new("Arimo", 13., "#654321", 0.75)
            .unwrap()
            .with_weight_and_alignment(650, TextAlignment::Center)
            .unwrap()
            .with_layout_metrics(17., 2.)
            .unwrap();
        let calibration = LengthCalibration::from_scale(72., 3., "m", 3, false)
            .unwrap()
            .with_label("Échelle 雪")
            .unwrap();
        let page_calibration = calibration.clone();
        let asset = DecodedRgbaAsset::new(1, 1, vec![9, 8, 7, 6]).unwrap();
        let redact_appearance = RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)
            .unwrap()
            .with_fill_opacity(0.35)
            .unwrap();
        let cloud_appearance = RectangleAppearance::new("#123456", 2.5, None::<String>, 0.75)
            .unwrap()
            .with_stroke_style(StrokeStyle::Dashed);
        let cloud_plus_appearance = CloudPlusAppearance::new(
            cloud_appearance.clone(),
            line_appearance.clone(),
            text_style.clone(),
        )
        .unwrap();
        let callout_appearance =
            CalloutAppearance::new(line_appearance.clone(), text_style.clone()).unwrap();
        let dimension_appearance =
            DimensionAppearance::new(line_appearance.clone(), text_style.clone()).unwrap();
        let annotations = vec![
            Annotation::Rectangle(RectangleAnnotation {
                id: id("family:rectangle"),
                page_index: 0,
                rect: PdfRect::new(1., 2., 30., 20.).unwrap(),
                rotation_degrees: 15.,
                appearance: rectangle_appearance.clone(),
                locked: true,
            }),
            Annotation::Redact(
                RedactAnnotation::new(
                    id("family:redact"),
                    0,
                    PdfRect::new(2., 3., 30., 20.).unwrap(),
                    "#010203",
                    Some("秘密"),
                    redact_appearance,
                )
                .unwrap(),
            ),
            Annotation::Ellipse(
                EllipseAnnotation::new(
                    id("family:ellipse"),
                    0,
                    PdfRect::new(3., 4., 30., 20.).unwrap(),
                    rectangle_appearance.clone(),
                )
                .unwrap(),
            ),
            Annotation::Arc(
                ArcAnnotation::new(
                    id("family:arc"),
                    0,
                    point(0., 0.),
                    point(20., 0.),
                    point(10., 8.),
                    rectangle_appearance.clone(),
                )
                .unwrap(),
            ),
            Annotation::StraightLine(
                StraightLineAnnotation::new(
                    id("family:line"),
                    0,
                    point(0., 0.),
                    point(20., 5.),
                    LineKind::Arrow,
                    line_appearance.clone(),
                )
                .unwrap(),
            ),
            Annotation::VertexPath(
                VertexPathAnnotation::new(
                    id("family:polygon"),
                    0,
                    vec![point(0., 0.), point(20., 0.), point(10., 10.)],
                    VertexPathKind::Polygon,
                    rectangle_appearance.clone(),
                )
                .unwrap(),
            ),
            Annotation::Cloud(
                CloudAnnotation::new(
                    id("family:cloud"),
                    0,
                    vec![point(0., 0.), point(20., 0.), point(10., 10.)],
                    2.5,
                    cloud_appearance.clone(),
                )
                .unwrap(),
            ),
            Annotation::CloudPlus(
                CloudPlusAnnotation::new(
                    id("family:cloud-plus"),
                    0,
                    vec![point(0., 0.), point(20., 0.), point(10., 10.)],
                    2.,
                    vec![point(20., 5.), point(28., 8.), point(35., 5.)],
                    PdfRect::new(35., 0., 40., 20.).unwrap(),
                    "Nuage 雪",
                    cloud_plus_appearance,
                )
                .unwrap()
                .with_cloud_appearance_path(Some(vec![
                    CloudAppearancePathCommand::MoveTo(point(0., 0.)),
                    CloudAppearancePathCommand::LineTo(point(20., 0.)),
                    CloudAppearancePathCommand::LineTo(point(10., 10.)),
                    CloudAppearancePathCommand::Close,
                ]))
                .unwrap(),
            ),
            Annotation::Callout(
                CalloutAnnotation::new(
                    id("family:callout"),
                    0,
                    vec![point(0., 0.), point(20., 10.)],
                    PdfRect::new(20., 5., 40., 20.).unwrap(),
                    "Appel é",
                    callout_appearance,
                )
                .unwrap(),
            ),
            Annotation::MeasurementPath(
                MeasurementPathAnnotation::new_with_text_style(
                    id("family:area"),
                    0,
                    vec![point(0., 0.), point(20., 0.), point(10., 10.)],
                    MeasurementPathKind::Area,
                    calibration.clone(),
                    rectangle_appearance.clone(),
                    text_style.clone(),
                )
                .unwrap(),
            ),
            Annotation::Pen(
                PenAnnotation::new_paths(
                    id("family:pen"),
                    0,
                    vec![
                        vec![point(0., 0.), point(1., 1.)],
                        vec![point(2., 2.), point(3., 4.)],
                    ],
                    PenAppearance::new("#123456", 3., 0.5).unwrap(),
                    false,
                )
                .unwrap(),
            ),
            Annotation::TextBox(
                TextBoxAnnotation::new(
                    id("family:text"),
                    0,
                    PdfRect::new(1., 2., 50., 25.).unwrap(),
                    "Unicode 雪 é",
                    text_style.clone(),
                )
                .unwrap()
                .with_rotation_degrees(270.)
                .unwrap(),
            ),
            Annotation::Dimension(
                DimensionAnnotation::new(
                    id("family:dimension"),
                    0,
                    point(0., 0.),
                    point(30., 0.),
                    -12.,
                    "30 m",
                    dimension_appearance.clone(),
                )
                .unwrap(),
            ),
            Annotation::Length(
                LengthAnnotation::new_with_appearance(
                    id("family:length"),
                    0,
                    point(0., 0.),
                    point(30., 5.),
                    calibration,
                    dimension_appearance,
                )
                .unwrap(),
            ),
            Annotation::Image(
                ImageAnnotation::new_with_opacity(
                    id("family:image"),
                    0,
                    PdfRect::new(1., 1., 10., 10.).unwrap(),
                    asset.clone(),
                    true,
                    0.4,
                )
                .unwrap()
                .with_rotation_degrees(90.)
                .unwrap(),
            ),
            Annotation::Snapshot(
                SnapshotAnnotation::new(
                    id("family:snapshot"),
                    0,
                    PdfRect::new(2., 2., 12., 12.).unwrap(),
                    asset,
                    0.6,
                )
                .unwrap()
                .with_rotation_degrees(180.)
                .unwrap(),
            ),
        ];
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(annotations, Vec::new())
            .unwrap();
        let page_scale = PageScale::from_factors(
            0,
            ScaleSource::Custom,
            "Échelle 1:24",
            ScaleUnit::In,
            ScaleUnit::M,
            page_calibration.scale_x,
            page_calibration.scale_y,
            page_calibration.scale_precision,
        )
        .unwrap();
        document.state.page_scales.insert(0, page_scale);
        document
            .state
            .page_length_calibrations
            .insert(0, page_calibration);
        document.state.scale_presets.push(ScalePreset {
            id: "custom-雪".into(),
            name: "Custom é".into(),
            pdf_units: ScaleUnit::In,
            real_units: ScaleUnit::M,
            scale_x: 0.25,
            scale_y: 0.5,
            source: ScaleSource::Custom,
            built_in: false,
        });
        document
            .state
            .page_rotations
            .insert(0, PageRotation::Degrees270);
        let encoded = document.encode_recovery_timeline().unwrap();
        let hydrated = AnnotationDocument::hydrate_recovery_timeline(&encoded).unwrap();
        assert_eq!(
            hydrated.state.annotation_order,
            document.state.annotation_order
        );
        assert_eq!(hydrated.state.rectangles, document.state.rectangles);
        assert_eq!(hydrated.state.redacts, document.state.redacts);
        assert_eq!(hydrated.state.ellipses, document.state.ellipses);
        assert_eq!(hydrated.state.arcs, document.state.arcs);
        assert_eq!(hydrated.state.straight_lines, document.state.straight_lines);
        assert_eq!(hydrated.state.vertex_paths, document.state.vertex_paths);
        assert_eq!(hydrated.state.clouds, document.state.clouds);
        assert_eq!(hydrated.state.cloud_pluses, document.state.cloud_pluses);
        assert_eq!(hydrated.state.callouts, document.state.callouts);
        assert_eq!(
            hydrated.state.measurement_paths,
            document.state.measurement_paths
        );
        assert_eq!(hydrated.state.pens, document.state.pens);
        assert_eq!(hydrated.state.text_boxes, document.state.text_boxes);
        assert_eq!(hydrated.state.dimensions, document.state.dimensions);
        assert_eq!(hydrated.state.lengths, document.state.lengths);
        assert_eq!(hydrated.state.images, document.state.images);
        assert_eq!(hydrated.state.snapshots, document.state.snapshots);
        assert_eq!(hydrated.state.page_scales, document.state.page_scales);
        assert_eq!(hydrated.state.scale_presets, document.state.scale_presets);
        assert_eq!(
            hydrated.state.page_length_calibrations,
            document.state.page_length_calibrations
        );
        assert_eq!(hydrated.state.page_rotations, document.state.page_rotations);

        let mut malformed_path: Value = serde_json::from_slice(&encoded).unwrap();
        malformed_path["current"]["cloud_pluses"][0]["cloud_appearance_path"] = json!([]);
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(
                &serde_json::to_vec(&malformed_path).unwrap()
            )
            .is_err()
        );
        let mut malformed_style: Value = serde_json::from_slice(&encoded).unwrap();
        malformed_style["current"]["text_boxes"][0]["style"]["opacity"] = json!(2.0);
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(
                &serde_json::to_vec(&malformed_style).unwrap()
            )
            .is_err()
        );
        let mut malformed_preset: Value = serde_json::from_slice(&encoded).unwrap();
        malformed_preset["current"]["scale_presets"][0]["scale_x"] = json!(-1.0);
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(
                &serde_json::to_vec(&malformed_preset).unwrap()
            )
            .is_err()
        );
        let mut mismatched_calibration: Value = serde_json::from_slice(&encoded).unwrap();
        let calibration = mismatched_calibration["current"]["page_length_calibrations"]["0"].take();
        mismatched_calibration["current"]["page_length_calibrations"]["1"] = calibration;
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(
                &serde_json::to_vec(&mismatched_calibration).unwrap()
            )
            .is_err()
        );
        let mut malformed_rotation: Value = serde_json::from_slice(&encoded).unwrap();
        malformed_rotation["current"]["page_rotations"]["0"] = json!("Degrees45");
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(
                &serde_json::to_vec(&malformed_rotation).unwrap()
            )
            .is_err()
        );
    }

    #[test]
    fn recovery_timeline_rejects_unknown_versions_fields_hashes_dimensions_and_order() {
        let encoded = recovery_document_with_future_only_assets()
            .encode_recovery_timeline()
            .unwrap();
        let original: Value = serde_json::from_slice(&encoded).unwrap();
        let rejects = |mutate: fn(&mut Value)| {
            let mut value = original.clone();
            mutate(&mut value);
            match AnnotationDocument::hydrate_recovery_timeline(
                &serde_json::to_vec(&value).unwrap(),
            ) {
                Ok(_) => panic!("malformed recovery timeline was accepted"),
                Err(error) => error,
            }
        };

        assert!(matches!(
            rejects(|value| value["schema_version"] = json!(99)),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| value["history_limit"] = json!(MAX_RECOVERY_HISTORY_STATES + 1)),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| {
                let next = value["next_revision"].clone();
                value["saved_revision"] = next;
            }),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| value["next_revision"] = json!(u64::MAX)),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| value["unexpected"] = json!(true)),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| value["future"][1]["unexpected"] = json!(true)),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| value["assets"][0]["id"] = json!("00")),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| value["assets"][0]["width_px"] = json!(0)),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| {
                value["future"][1]["annotation_order"] =
                    json!(["recovery:image:雪", "recovery:image:雪"])
            }),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
        assert!(matches!(
            rejects(|value| {
                let duplicate = value["assets"][0].clone();
                value["assets"].as_array_mut().unwrap().push(duplicate);
            }),
            AnnotationError::InvalidRecoveryTimeline(_)
        ));
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
    fn snapshot_persisted_state_accepts_pdf_rounding_but_rejects_material_rotation_changes() {
        let asset = DecodedRgbaAsset::new(1, 1, vec![10, 20, 30, 255]).unwrap();
        let expected = SnapshotAnnotation::new(
            id("workspace:snapshot:1"),
            0,
            PdfRect::new(84., 84., 480., 660.).unwrap(),
            asset,
            0.45,
        )
        .unwrap()
        .with_rotation_degrees(30.)
        .unwrap()
        .with_locked(true);
        let rounded = SnapshotAnnotation {
            rotation_degrees: 30.000_099,
            ..expected.clone()
        };
        assert_ne!(expected, rounded);
        assert!(expected.same_persisted_state_as(&rounded));

        let materially_rotated = SnapshotAnnotation {
            rotation_degrees: 30.000_101,
            ..rounded
        };
        assert!(!expected.same_persisted_state_as(&materially_rotated));
    }

    #[test]
    fn text_box_and_image_rotation_is_normalized_persisted_and_history_aware() {
        let text_id = id("rotation:text-box");
        let image_id = id("rotation:image");
        let style = TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap();
        let text = TextBoxAnnotation::new(
            text_id.clone(),
            0,
            PdfRect::new(10., 20., 100., 40.).unwrap(),
            "Rotated",
            style,
        )
        .unwrap()
        .with_rotation_degrees(-30.)
        .unwrap();
        let image = ImageAnnotation::new(
            image_id.clone(),
            0,
            PdfRect::new(50., 60., 40., 40.).unwrap(),
            DecodedRgbaAsset::new(1, 1, vec![10, 20, 30, 255]).unwrap(),
            true,
        )
        .unwrap()
        .with_rotation_degrees(390.)
        .unwrap();

        assert_eq!(text.rotation_degrees(), 330.);
        assert_eq!(image.rotation_degrees(), 30.);
        assert!(text.clone().with_rotation_degrees(f64::NAN).is_err());
        assert!(image.clone().with_rotation_degrees(f64::INFINITY).is_err());
        assert!(!text.same_persisted_state_as(&text.clone().with_rotation_degrees(331.).unwrap()));
        assert!(!image.same_persisted_state_as(&image.clone().with_rotation_degrees(31.).unwrap()));

        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(
                vec![Annotation::TextBox(text), Annotation::Image(image)],
                Vec::new(),
            )
            .unwrap();
        assert_eq!(document.history_depths(), (0, 0));
        assert_eq!(
            document
                .edit_annotation(&text_id, AnnotationEdit::SetTextBoxRotation(690.))
                .unwrap(),
            (AnnotationKind::TextBox, false)
        );
        assert_eq!(document.history_depths(), (0, 0));
        assert_eq!(
            document
                .edit_annotation(&image_id, AnnotationEdit::SetImageRotation(-45.))
                .unwrap(),
            (AnnotationKind::Image, true)
        );
        assert_eq!(document.images()[0].rotation_degrees(), 315.);
        assert_eq!(document.history_depths(), (1, 0));

        document.state.images[0].locked = true;
        assert_eq!(
            document.edit_annotation(&image_id, AnnotationEdit::SetImageRotation(0.)),
            Err(AnnotationError::LockedMarkup(image_id))
        );
        assert_eq!(document.history_depths(), (1, 0));
    }

    #[test]
    fn remaining_parity_appearance_edits_are_atomic_and_undoable() {
        let calibration = LengthCalibration::from_scale(72., 1., "ft", 2, true).unwrap();
        let length_id = id("parity:length");
        let path_id = id("parity:path");
        let image_id = id("parity:image");
        let asset = DecodedRgbaAsset::new(1, 1, vec![10, 20, 30, 255]).unwrap();
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(
                vec![
                    Annotation::Length(
                        LengthAnnotation::new(
                            length_id.clone(),
                            0,
                            point(10., 10.),
                            point(100., 10.),
                            calibration.clone(),
                        )
                        .unwrap(),
                    ),
                    Annotation::MeasurementPath(
                        MeasurementPathAnnotation::new(
                            path_id.clone(),
                            0,
                            vec![point(10., 20.), point(100., 20.)],
                            MeasurementPathKind::Polylength,
                            calibration,
                            RectangleAppearance::default(),
                        )
                        .unwrap(),
                    ),
                    Annotation::Image(
                        ImageAnnotation::new(
                            image_id.clone(),
                            0,
                            PdfRect::new(10., 30., 24., 24.).unwrap(),
                            asset,
                            true,
                        )
                        .unwrap(),
                    ),
                ],
                Vec::new(),
            )
            .unwrap();

        let line = StraightLineAppearance::new("#123456", 2.5, 0.4, StrokeStyle::Dashed).unwrap();
        let text = TextBoxStyle::new("Arimo", 18., "#654321", 0.4).unwrap();
        assert!(document.select(&length_id));
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: length_id,
                edit: AnnotationEdit::SetLengthAppearance(
                    DimensionAppearance::new(line.clone(), text.clone()).unwrap(),
                ),
            })
            .unwrap();
        assert!(document.select(&path_id));
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: path_id,
                edit: AnnotationEdit::SetMeasurementPathAppearance {
                    appearance: RectangleAppearance::new("#123456", 2.5, None::<String>, 0.4)
                        .unwrap()
                        .with_stroke_style(StrokeStyle::Dashed),
                    text_style: text,
                },
            })
            .unwrap();
        assert!(document.select(&image_id));
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: image_id,
                edit: AnnotationEdit::SetImageOpacity(0.35),
            })
            .unwrap();

        let changed = document.snapshot();
        assert_eq!(changed.lengths[0].appearance.line(), &line);
        assert_eq!(
            changed.measurement_paths[0].text_style().font_size_pt(),
            18.
        );
        assert_eq!(changed.images[0].opacity(), 0.35);
        document.undo().unwrap();
        assert_eq!(document.snapshot().images[0].opacity(), 1.);
    }

    #[test]
    fn length_calibration_persisted_state_accepts_only_pdf_rounding_error() {
        let expected = LengthCalibration::from_scale(36., 1., "m", 3, true).unwrap();
        let rounded = LengthCalibration {
            scale_x: expected.scale_x + 0.000_000_9,
            ..expected.clone()
        };
        assert_ne!(expected, rounded);
        assert!(expected.same_persisted_state_as(&rounded));

        let materially_changed = LengthCalibration {
            scale_x: expected.scale_x + 0.000_001_1,
            ..rounded
        };
        assert!(!expected.same_persisted_state_as(&materially_changed));
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
            document.hit_test(0, point(40.0, 40.0), 2.0).unwrap(),
            Some(HitTarget::Body(id("front")))
        );
        // With no painted handles, the selected Rectangle resizes from
        // anywhere along an edge.
        assert_eq!(
            document.hit_test(0, point(10.0, 40.0), 2.0).unwrap(),
            Some(HitTarget::ResizeHandle {
                id: id("front"),
                handle: RectangleResizeHandle::West,
            })
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
    fn selected_item_moves_from_the_band_around_its_outset_outline() {
        let mut document = AnnotationDocument::default();
        document
            .begin_create(8, id("rect"), 0, point(10.0, 20.0), RectangleAppearance::default())
            .unwrap();
        document.update_gesture(8, point(110.0, 70.0)).unwrap();
        document.commit_gesture(8).unwrap();
        // 4 pt outside the left edge, between its corner and midpoint
        // controls: inside the 6 pt outline band plus a 1 pt tolerance.
        let band = point(6.0, 30.0);
        assert_eq!(
            document.hit_test(0, band, 1.0).unwrap(),
            Some(HitTarget::Body(id("rect"))),
            "a selected item moves from the band along its dashed outline"
        );
        assert_eq!(
            document.hit_test(0, point(2.0, 30.0), 1.0).unwrap(),
            None,
            "beyond the band the press misses"
        );
        document.clear_selection();
        assert_eq!(
            document.hit_test(0, band, 1.0).unwrap(),
            None,
            "an unselected item has no outline band"
        );
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
    fn ordered_scene_preserves_cross_family_page_and_thumbnail_order() {
        let mut document = AnnotationDocument::default();
        let pen = |name: &str, page| {
            Annotation::Pen(
                PenAnnotation::new(
                    id(name),
                    page,
                    vec![point(150., 150.), point(180., 180.)],
                    PenAppearance::new("#000000", 1., 1.).unwrap(),
                )
                .unwrap(),
            )
        };
        let rectangle = Annotation::Rectangle(RectangleAnnotation {
            id: id("middle"),
            page_index: 0,
            rect: PdfRect::new(10., 20., 100., 50.).unwrap(),
            rotation_degrees: 0.,
            appearance: RectangleAppearance::default(),
            locked: false,
        });
        document
            .load_imported_annotations(
                vec![
                    pen("bottom", 0),
                    pen("other-page", 1),
                    rectangle,
                    pen("top", 0),
                ],
                vec![],
            )
            .unwrap();
        let before = document.snapshot();
        for scene in [document.document_scene(0), document.thumbnail_scene(0)] {
            assert_eq!(
                scene.annotation_order,
                vec![id("bottom"), id("middle"), id("top")]
            );
            let ordered: Vec<_> = scene.into_ordered_annotations().collect();
            assert_eq!(
                ordered
                    .iter()
                    .map(|item| item.id().as_str())
                    .collect::<Vec<_>>(),
                vec!["bottom", "middle", "top"]
            );
            assert!(matches!(ordered[0], SceneAnnotation::Pen(_)));
            assert!(matches!(ordered[1], SceneAnnotation::Rectangle(_)));
            assert!(matches!(ordered[2], SceneAnnotation::Pen(_)));
        }
        assert_eq!(
            document
                .document_scene(1)
                .into_ordered_annotations()
                .map(|item| item.id().clone())
                .collect::<Vec<_>>(),
            vec![id("other-page")]
        );
        let mut duplicate_order = document.document_scene(0);
        duplicate_order.annotation_order.push(id("bottom"));
        assert_eq!(duplicate_order.into_ordered_annotations().count(), 3);
        assert_eq!(document.snapshot(), before);
    }

    #[test]
    fn select_all_is_page_scoped_ordered_and_includes_locked_annotations() {
        let rectangle = |name: &str, page_index, locked| {
            Annotation::Rectangle(RectangleAnnotation {
                id: id(name),
                page_index,
                rect: PdfRect::new(10., 20., 100., 50.).unwrap(),
                rotation_degrees: 0.,
                appearance: RectangleAppearance::default(),
                locked,
            })
        };
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(
                vec![
                    rectangle("page-zero-first", 0, false),
                    rectangle("page-one", 1, false),
                    rectangle("page-zero-locked", 0, true),
                ],
                Vec::new(),
            )
            .unwrap();

        assert_eq!(
            document.select_all_on_page(0),
            &[id("page-zero-first"), id("page-zero-locked")],
        );
        assert_eq!(document.focused_id(), Some(&id("page-zero-locked")));

        assert_eq!(document.select_all_on_page(1), &[id("page-one")]);
        assert_eq!(document.focused_id(), Some(&id("page-one")));
    }

    #[test]
    fn ordered_scene_replaces_preview_in_place_and_appends_new_draft() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "existing");
        document
            .insert_annotations(vec![Annotation::Pen(
                PenAnnotation::new(
                    id("top-pen"),
                    0,
                    vec![point(150., 150.), point(180., 180.)],
                    PenAppearance::new("#000000", 1., 1.).unwrap(),
                )
                .unwrap(),
            )])
            .unwrap();
        let mut before = document.snapshot();
        // Pointer-down deliberately selects the existing rectangle.
        before.selected_id = Some(id("existing"));
        document.begin_move(55, 0, point(10., 40.), 2.).unwrap();
        document.update_gesture(55, point(30., 60.)).unwrap();
        let ordered: Vec<_> = document
            .document_scene(0)
            .into_ordered_annotations()
            .collect();
        assert!(matches!(&ordered[0], SceneAnnotation::Rectangle(value)
            if value.id == id("existing") && value.preview && value.rect.x == 30.));
        assert_eq!(ordered[1].id(), &id("top-pen"));
        assert_eq!(
            document
                .thumbnail_scene(0)
                .into_ordered_annotations()
                .next()
                .unwrap()
                .id(),
            &id("existing")
        );
        document.cancel_gesture(55).unwrap();
        document
            .begin_create(
                56,
                id("new-draft"),
                0,
                point(200., 200.),
                RectangleAppearance::default(),
            )
            .unwrap();
        document.update_gesture(56, point(220., 240.)).unwrap();
        assert_eq!(
            document
                .document_scene(0)
                .into_ordered_annotations()
                .map(|item| item.id().clone())
                .collect::<Vec<_>>(),
            vec![id("existing"), id("top-pen"), id("new-draft")]
        );
        assert_eq!(
            document
                .thumbnail_scene(0)
                .into_ordered_annotations()
                .count(),
            2
        );
        assert_eq!(document.snapshot(), before);
    }

    #[test]
    fn thumbnail_scene_projects_committed_page_geometry_without_editor_chrome() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "thumbnail");
        document.begin_move(55, 0, point(40.0, 40.0), 2.0).unwrap();
        document.update_gesture(55, point(60.0, 60.0)).unwrap();

        assert_eq!(
            document.thumbnail_scene(0),
            AnnotationScene {
                annotation_order: vec![id("thumbnail")],
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
                    feedback: SceneInteractionFeedback::Normal,
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
                "../tests/fixtures/bp-rectangle-v1.fixture.json"
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
        let manifest = include_str!("../tests/fixtures/bp-rectangle-v1.fixture.json")
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

        for operation in [
            crate::selection_geometry::SelectionOperation::Replace,
            crate::selection_geometry::SelectionOperation::Add,
            crate::selection_geometry::SelectionOperation::Remove,
        ] {
            let mut preview = SelectionMarquee::armed_box(SelectionPoint::new(0., 0.), operation);
            assert!(
                document
                    .marquee_candidates(0, &preview, identity)
                    .is_empty()
            );
            preview.update(SelectionPoint::new(110., 70.));
            let unchanged = document.snapshot();
            assert_eq!(
                document.marquee_candidates(0, &preview, identity),
                vec![rectangle_id.clone(), line_id.clone()]
            );
            assert!(
                document
                    .marquee_candidates(1, &preview, identity)
                    .is_empty()
            );
            assert_eq!(document.snapshot(), unchanged);
        }

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
        assert_eq!(area.caption(), "2.00 sq ft");
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
    fn measurement_caption_visibility_is_one_atomic_edit_with_exact_undo_redo() {
        let calibration = LengthCalibration::from_scale(72., 1., "m", 2, true).unwrap();
        let length = LengthAnnotation::new(
            id("measurement-caption:length"),
            0,
            point(0., 0.),
            point(72., 0.),
            calibration.clone(),
        )
        .unwrap();
        let path = MeasurementPathAnnotation::new(
            id("measurement-caption:path"),
            0,
            vec![point(0., 0.), point(72., 0.)],
            MeasurementPathKind::Polylength,
            calibration,
            RectangleAppearance::default(),
        )
        .unwrap();
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(
                vec![
                    Annotation::Length(length),
                    Annotation::MeasurementPath(path),
                ],
                Vec::new(),
            )
            .unwrap();

        let hidden = document.snapshot().lengths[0]
            .calibration()
            .clone()
            .with_show_caption(false);
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("measurement-caption:length"),
                edit: AnnotationEdit::SetLengthCalibration(hidden),
            })
            .unwrap();
        let after_length = document.snapshot();
        assert_eq!((after_length.revision, after_length.undo_depth), (1, 1));
        assert!(!after_length.lengths[0].calibration().show_caption());
        assert!(
            after_length.measurement_paths[0]
                .calibration()
                .show_caption()
        );

        let hidden = after_length.measurement_paths[0]
            .calibration()
            .clone()
            .with_show_caption(false);
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id("measurement-caption:path"),
                edit: AnnotationEdit::SetMeasurementPathCalibration(hidden),
            })
            .unwrap();
        let after_path = document.snapshot();
        assert_eq!((after_path.revision, after_path.undo_depth), (2, 2));
        assert!(!after_path.lengths[0].calibration().show_caption());
        assert!(!after_path.measurement_paths[0].calibration().show_caption());

        document.undo().unwrap();
        assert!(
            document.snapshot().measurement_paths[0]
                .calibration()
                .show_caption()
        );
        document.undo().unwrap();
        assert!(document.snapshot().lengths[0].calibration().show_caption());
        document.redo().unwrap();
        document.redo().unwrap();
        let redone = document.snapshot();
        assert!(!redone.lengths[0].calibration().show_caption());
        assert!(!redone.measurement_paths[0].calibration().show_caption());
    }

    #[test]
    fn cloud_scallops_reproduce_revu_curl_geometry() {
        // Revu 21's own Cloud (tests/fixtures/bluebeam/revu-shapes.pdf,
        // intensity 2): its appearance starts where the first and last curls
        // meet, and each curl runs past the next meeting point and hooks back.
        let vertices = [
            PdfPoint { x: 288.3672, y: 670.2866 },
            PdfPoint { x: 364.8282, y: 670.2866 },
            PdfPoint { x: 364.8282, y: 612.8628 },
            PdfPoint { x: 288.3672, y: 612.8628 },
        ];
        let path = sampled_cloud_scallop_path(&vertices, DEFAULT_CLOUD_SCALLOP_RADIUS_PT);
        let near = |x: f64, y: f64| {
            path.iter()
                .any(|point| (point.x - x).abs() < 0.01 && (point.y - y).abs() < 0.01)
        };
        assert!((path[0].x - 283.693).abs() < 0.01 && (path[0].y - 663.2401).abs() < 0.01);
        assert!(near(296.6271, 672.0964), "overshoot past the first meeting point");
        assert!(near(295.4137, 674.9608), "first meeting point");
        // The curl centred 8.1 pt below the top-right corner bulges right.
        assert!(path.iter().any(|point| point.x > 373.2 && (point.y - 662.189).abs() < 2.));
        assert_eq!(path.first(), path.last());
        // Every curl bulges outwards: nothing falls inside the outline by
        // more than the hooks reach.
        assert!(path.iter().all(|point| point.y > 612.8628 - 9. && point.y < 670.2866 + 9.));
        assert!(path.iter().any(|point| point.y > 678.));

        // The same outline drawn anticlockwise bulges outwards too.
        let mut reversed = vertices;
        reversed.reverse();
        let reversed_path = sampled_cloud_scallop_path(&reversed, DEFAULT_CLOUD_SCALLOP_RADIUS_PT);
        assert!(reversed_path.iter().any(|point| point.y > 678.));
        assert!(reversed_path.iter().any(|point| point.y < 605.));
    }

    #[test]
    fn marquee_cloud_uses_control_path_instead_of_visible_scallops() {
        use crate::selection_geometry::SelectionOperation;
        let identity = |point: PdfPoint| SelectionPoint::new(point.x, point.y);
        for intensity in [0.5, 2., 4.] {
            let cloud = CloudAnnotation::new(
                id("marquee:cloud"),
                0,
                vec![
                    point(10., 10.),
                    point(90., 10.),
                    point(90., 70.),
                    point(10., 70.),
                ],
                intensity,
                RectangleAppearance::default(),
            )
            .unwrap();
            assert!(
                cloud.scallop_path().iter().any(|point| point.y > 70.01),
                "fixture must have a visible lobe outside its control path"
            );
            let mut document = AnnotationDocument::default();
            document
                .apply_command(AnnotationCommand::CreateAnnotation(Annotation::Cloud(
                    cloud,
                )))
                .unwrap();
            let before = document.snapshot();
            for (start, end, expected) in [
                ((10., 10.), (90., 70.), vec![id("marquee:cloud")]),
                ((100., 80.), (0., 70.01), vec![]),
            ] {
                let mut marquee = SelectionMarquee::armed_box(
                    SelectionPoint::new(start.0, start.1),
                    SelectionOperation::Replace,
                );
                marquee.update(SelectionPoint::new(end.0, end.1));
                let before_query = document.snapshot();
                assert_eq!(
                    document.marquee_candidates(0, &marquee, identity),
                    expected,
                    "cloud control-path query at intensity {intensity}"
                );
                assert_eq!(document.snapshot(), before_query);
                assert_eq!(
                    document.apply_marquee_selection(0, &marquee, identity),
                    expected
                );
                let mut after = document.snapshot();
                after.selected_id = before.selected_id.clone();
                assert_eq!(after, before, "selection must not mutate model history");
            }
        }
    }

    #[test]
    fn marquee_cloud_plus_keeps_control_path_leader_and_caption_as_one_group() {
        use crate::selection_geometry::SelectionOperation;
        let identity = |point: PdfPoint| SelectionPoint::new(point.x, point.y);
        let cloud = CloudPlusAnnotation::new(
            id("marquee:cloud-plus"),
            0,
            vec![
                point(10., 10.),
                point(90., 10.),
                point(90., 70.),
                point(10., 70.),
            ],
            2.,
            vec![point(90., 40.), point(110., 50.), point(130., 50.)],
            PdfRect::new(130., 40., 50., 20.).unwrap(),
            "Cloud+",
            CloudPlusAppearance::new(
                RectangleAppearance::default(),
                StraightLineAppearance::default_for(LineKind::Line),
                TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(cloud.scallop_path().iter().any(|point| point.y > 70.01));
        let mut document = AnnotationDocument::default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::CloudPlus(
                cloud,
            )))
            .unwrap();
        let before = document.snapshot();
        for (label, start, end, hit) in [
            (
                "whole group inside tight control bounds",
                (10., 10.),
                (180., 70.),
                true,
            ),
            (
                "cloud alone is insufficient for window",
                (10., 10.),
                (90., 70.),
                false,
            ),
            (
                "caption interior alone does not cross its path",
                (170., 55.),
                (150., 45.),
                false,
            ),
            ("caption-only crossing", (185., 55.), (175., 45.), true),
            ("leader-only crossing", (122., 55.), (112., 45.), true),
            (
                "visible scallop alone is insufficient",
                (100., 80.),
                (0., 70.01),
                false,
            ),
        ] {
            let mut marquee = SelectionMarquee::armed_box(
                SelectionPoint::new(start.0, start.1),
                SelectionOperation::Replace,
            );
            marquee.update(SelectionPoint::new(end.0, end.1));
            let expected = if hit {
                vec![id("marquee:cloud-plus")]
            } else {
                vec![]
            };
            let before_query = document.snapshot();
            assert_eq!(
                document.marquee_candidates(0, &marquee, identity),
                expected,
                "{label}"
            );
            assert_eq!(document.snapshot(), before_query);
            assert_eq!(
                document.apply_marquee_selection(0, &marquee, identity),
                expected,
                "{label}"
            );
            let mut after = document.snapshot();
            after.selected_id = before.selected_id.clone();
            assert_eq!(after, before, "{label} must not mutate model history");
        }
    }

    #[test]
    fn marquee_dimension_uses_offset_and_overhang_path_without_baseline() {
        use crate::selection_geometry::SelectionOperation;
        // Ten CSS pixels per PDF point makes each small crossing box active.
        let project = |point: PdfPoint| SelectionPoint::new(point.x * 10., point.y * 10.);
        for (end, offset, normal) in [
            (point(110., 10.), 20., (0., 1.)),
            (point(110., 10.), -20., (0., 1.)),
            (point(70., 90.), 20., (-0.8, 0.6)),
            (point(70., 90.), -20., (-0.8, 0.6)),
        ] {
            let start = point(10., 10.);
            let dimension = DimensionAnnotation::new(
                id("marquee:dimension"),
                0,
                start,
                end,
                offset,
                "Dimension",
                DimensionAppearance::new(
                    StraightLineAppearance::default_for(LineKind::Line),
                    TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
                )
                .unwrap(),
            )
            .unwrap();
            let (dimension_start, dimension_end) = dimension.dimension_line_points();
            let sign = if offset >= 0. { 1. } else { -1. };
            let baseline_midpoint = point((start.x + end.x) / 2., (start.y + end.y) / 2.);
            let extension_midpoint = point(
                dimension_start.x + normal.0 * sign * 2.,
                dimension_start.y + normal.1 * sign * 2.,
            );
            let offset_quarter = point(
                dimension_start.x * 0.75 + dimension_end.x * 0.25,
                dimension_start.y * 0.75 + dimension_end.y * 0.25,
            );
            let annotation = Annotation::Dimension(dimension);
            let mut document = AnnotationDocument::default();
            document
                .apply_command(AnnotationCommand::CreateAnnotation(annotation.clone()))
                .unwrap();
            let before = document.snapshot();
            for (label, center, hit) in [
                ("baseline alone", baseline_midpoint, false),
                ("extension overhang", extension_midpoint, true),
                ("offset line", offset_quarter, true),
            ] {
                let mut marquee = SelectionMarquee::armed_box(
                    project(point(center.x + 0.5, center.y + 0.5)),
                    SelectionOperation::Replace,
                );
                marquee.update(project(point(center.x - 0.5, center.y - 0.5)));
                assert!(marquee.active);
                let expected = if hit {
                    vec![id("marquee:dimension")]
                } else {
                    vec![]
                };
                let before_query = document.snapshot();
                assert_eq!(
                    document.marquee_candidates(0, &marquee, project),
                    expected,
                    "{label}: end {end:?}, offset {offset}"
                );
                assert_eq!(document.snapshot(), before_query);
                assert_eq!(
                    document.apply_marquee_selection(0, &marquee, project),
                    expected
                );
                let mut after = document.snapshot();
                after.selected_id = before.selected_id.clone();
                assert_eq!(after, before);
            }
            let paths = annotation_selection_paths(&annotation, &project);
            let body_path = paths.first().expect("dimension body path");
            let expected = [
                point(
                    dimension_start.x + normal.0 * sign * DIMENSION_LEADER_EXTENSION_PT,
                    dimension_start.y + normal.1 * sign * DIMENSION_LEADER_EXTENSION_PT,
                ),
                dimension_start,
                dimension_end,
                point(
                    dimension_end.x + normal.0 * sign * DIMENSION_LEADER_EXTENSION_PT,
                    dimension_end.y + normal.1 * sign * DIMENSION_LEADER_EXTENSION_PT,
                ),
            ];
            assert!(!body_path.closed);
            assert_eq!(body_path.points.len(), expected.len());
            for (actual, expected) in body_path.points.iter().zip(expected.map(project)) {
                assert!((actual.x - expected.x).abs() < 0.000_001);
                assert!((actual.y - expected.y).abs() < 0.000_001);
            }
        }
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
    fn callout_text_box_resize_preserves_leader_order_and_attachment_side() {
        let appearance = CalloutAppearance::new(
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap();
        let make_callout = |connection| {
            CalloutAnnotation::new(
                id("callout:resize-side"),
                0,
                vec![point(20., 20.), point(60., 80.), connection],
                PdfRect::new(100., 60., 100., 40.).unwrap(),
                "Callout",
                appearance.clone(),
            )
            .unwrap()
        };
        let next_box = PdfRect::new(120., 70., 140., 60.).unwrap();

        let right = make_callout(point(200., 80.))
            .resized_text_box(next_box)
            .unwrap();
        assert_eq!(
            right.leader_points(),
            &[point(20., 20.), point(60., 80.), point(260., 100.),]
        );

        let tie = make_callout(point(150., 80.))
            .resized_text_box(next_box)
            .unwrap();
        assert_eq!(
            tie.leader_points(),
            &[point(20., 20.), point(60., 80.), point(120., 100.),]
        );
        assert_eq!(tie.text_box, next_box);
        assert_eq!(tie.content(), "Callout");
        assert_eq!(tie.appearance, appearance);
    }

    #[test]
    fn callout_disk_geometry_canonicalization_is_an_exact_idempotent_fixed_point() {
        let appearance = CalloutAppearance::new(
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap();
        let original_leader = vec![
            point(107.99997729745478, 312.0000086485887),
            point(174.0000095494833, 426.0000308105971),
            point(251.999947, 407.999988),
        ];
        let raw = CalloutAnnotation {
            id: id("callout:fractional-disk-fixed-point"),
            page_index: 0,
            leader_points: original_leader.clone(),
            text_box: PdfRect::new(251.999947, 385.999988, 150., 44.).unwrap(),
            content: "field\nnote".into(),
            appearance,
            locked: true,
        };

        let canonical = raw.canonicalized_for_disk().unwrap();
        let second = canonical.canonicalized_for_disk().unwrap();
        let imported_geometry = canonical.disk_geometry().unwrap();

        assert_eq!(canonical, second);
        assert_eq!(canonical.text_box, imported_geometry.text_box);
        assert!(
            canonical
                .text_box
                .same_pdf_geometry_as(imported_geometry.text_box)
        );
        assert_eq!(canonical.leader_points(), original_leader);
        assert_eq!(canonical.content(), "field\nnote");
        assert!(canonical.locked);
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
    fn cloud_plus_custom_appearance_path_survives_non_shape_edits_and_translates_atomically() {
        let id = id("cloud-plus:custom-path");
        let appearance = CloudPlusAppearance::new(
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap();
        let custom_path = vec![
            CloudAppearancePathCommand::MoveTo(point(8., 12.)),
            CloudAppearancePathCommand::CubicTo {
                control_1: point(25., 0.),
                control_2: point(65., 0.),
                end: point(92., 12.),
            },
            CloudAppearancePathCommand::LineTo(point(92., 72.)),
            CloudAppearancePathCommand::LineTo(point(8., 72.)),
            CloudAppearancePathCommand::Close,
        ];
        let annotation = CloudPlusAnnotation::new(
            id.clone(),
            0,
            vec![
                point(10., 10.),
                point(90., 10.),
                point(90., 70.),
                point(10., 70.),
            ],
            2.,
            vec![point(92., 40.), point(110., 60.), point(130., 60.)],
            PdfRect::new(130., 38., 150., 44.).unwrap(),
            "External",
            appearance,
        )
        .unwrap()
        .with_cloud_appearance_path(Some(custom_path.clone()))
        .unwrap();
        assert!(annotation.scallop_path().len() > custom_path.len());
        let mut document = AnnotationDocument::default();
        document
            .apply_command(AnnotationCommand::CreateAnnotation(Annotation::CloudPlus(
                annotation,
            )))
            .unwrap();
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id.clone(),
                edit: AnnotationEdit::SetCloudPlusContent("Edited caption".into()),
            })
            .unwrap();
        assert_eq!(
            document.cloud_plus(&id).unwrap().cloud_appearance_path(),
            Some(custom_path.as_slice())
        );
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id.clone(),
                edit: AnnotationEdit::TranslateCloudPlusGroup {
                    delta_x: 7.,
                    delta_y: -3.,
                },
            })
            .unwrap();
        let translated = document
            .cloud_plus(&id)
            .unwrap()
            .cloud_appearance_path()
            .unwrap();
        assert_eq!(
            translated[0],
            CloudAppearancePathCommand::MoveTo(point(15., 9.))
        );
        assert_eq!(
            translated[1],
            CloudAppearancePathCommand::CubicTo {
                control_1: point(32., -3.),
                control_2: point(72., -3.),
                end: point(99., 9.),
            }
        );
        document.undo().unwrap();
        assert_eq!(
            document.cloud_plus(&id).unwrap().cloud_appearance_path(),
            Some(custom_path.as_slice())
        );
        document
            .apply_command(AnnotationCommand::EditAnnotation {
                id: id.clone(),
                edit: AnnotationEdit::SetCloudPlusCloudPoint {
                    vertex_index: 1,
                    point: point(100., 15.),
                    leader_points: vec![point(100., 42.), point(115., 60.), point(130., 60.)],
                },
            })
            .unwrap();
        assert!(
            document
                .cloud_plus(&id)
                .unwrap()
                .cloud_appearance_path()
                .is_none()
        );
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

        // Pressed inside: the selected Rectangle's edges resize.
        document.begin_move(1, 0, point(100.0, 600.0), 4.0).unwrap();
        for sample in 1..=SAMPLES {
            let progress = f64::from(sample) / f64::from(SAMPLES);
            document
                .update_gesture(1, point(100.0 + 36.0 * progress, 600.0 - 24.0 * progress))
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

    #[test]
    fn focused_id_tracks_most_recent_selection() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "a");
        create_rectangle(&mut document, "b");
        assert_eq!(document.focused_id(), Some(&id("b")));

        assert!(document.select(&id("a")));
        assert_eq!(document.focused_id(), Some(&id("a")));

        assert!(document.toggle_selection(&id("b")));
        assert_eq!(document.focused_id(), Some(&id("b")));

        assert!(document.toggle_selection(&id("b")));
        assert_eq!(document.focused_id(), Some(&id("a")));

        document.clear_selection();
        assert_eq!(document.focused_id(), None);
        assert!(document.selected_ids().is_empty());
    }

    #[test]
    fn focused_id_clears_when_selection_deleted() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "a");
        assert_eq!(document.focused_id(), Some(&id("a")));

        document.delete_selected_unlocked().unwrap();
        assert_eq!(document.focused_id(), None);
        assert!(document.selected_ids().is_empty());
    }

    #[test]
    fn text_box_rich_runs_are_bounded_canonical_and_backwards_compatible() {
        let runs = vec![
            TextBoxRichTextRun::new("Normal ").unwrap(),
            TextBoxRichTextRun::new("bold")
                .unwrap()
                .with_font_family("Arimo")
                .unwrap()
                .with_emphasis(true, false)
                .with_color("#0080FF")
                .unwrap()
                .with_font_size_pt(14.)
                .unwrap(),
        ];
        let annotation = TextBoxAnnotation::new(
            id("rich:model"),
            0,
            PdfRect::new(10., 20., 120., 40.).unwrap(),
            "Normal bold",
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap()
        .with_rich_text_runs(runs)
        .unwrap();

        assert_eq!(annotation.rich_text_runs().len(), 2);
        let styled = &annotation.rich_text_runs()[1];
        assert_eq!(styled.text(), "bold");
        assert_eq!(styled.font_family(), Some("Arimo"));
        assert!(styled.bold());
        assert!(!styled.italic());
        assert_eq!(styled.color(), Some("#0080ff"));
        assert_eq!(styled.font_size_pt(), Some(14.));
        assert!(
            TextBoxAnnotation::new(
                id("rich:mismatch"),
                0,
                PdfRect::new(0., 0., 20., 20.).unwrap(),
                "different",
                TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
            )
            .unwrap()
            .with_rich_text_runs(annotation.rich_text_runs().to_vec())
            .is_err()
        );
        assert!(TextBoxRichTextRun::new("").is_err());
        assert!(
            TextBoxRichTextRun::new("x")
                .unwrap()
                .with_font_family("x".repeat(MAX_FONT_FAMILY_BYTES + 1))
                .is_err()
        );
        assert!(
            TextBoxRichTextRun::new("x")
                .unwrap()
                .with_color("red")
                .is_err()
        );
        assert!(
            TextBoxRichTextRun::new("x")
                .unwrap()
                .with_font_size_pt(f64::NAN)
                .is_err()
        );

        let mut legacy = serde_json::to_value(&annotation).unwrap();
        legacy.as_object_mut().unwrap().remove("rich_text_runs");
        let legacy: TextBoxAnnotation = serde_json::from_value(legacy).unwrap();
        assert!(legacy.rich_text_runs().is_empty());
    }

    #[test]
    fn text_box_rich_runs_survive_copy_and_non_content_edits_but_content_clears_them() {
        let text_id = id("rich:edit");
        let text = TextBoxAnnotation::new(
            text_id.clone(),
            0,
            PdfRect::new(10., 20., 120., 40.).unwrap(),
            "bold italic",
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap()
        .with_rich_text_runs(vec![
            TextBoxRichTextRun::new("bold ")
                .unwrap()
                .with_emphasis(true, false),
            TextBoxRichTextRun::new("italic")
                .unwrap()
                .with_emphasis(false, true),
        ])
        .unwrap();
        let translated = Annotation::TextBox(text.clone())
            .translated_copy(id("rich:copy"), 1, 3., 4.)
            .unwrap();
        let Annotation::TextBox(translated) = translated else {
            panic!("expected translated text box");
        };
        assert_eq!(translated.rich_text_runs(), text.rich_text_runs());

        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(vec![Annotation::TextBox(text)], Vec::new())
            .unwrap();
        assert_eq!(
            document.document_scene(0).text_boxes[0]
                .rich_text_runs
                .len(),
            2
        );
        assert_eq!(
            document.thumbnail_scene(0).text_boxes[0]
                .rich_text_runs
                .len(),
            2
        );
        document
            .edit_annotation(
                &text_id,
                AnnotationEdit::SetTextBoxLayoutRect(PdfRect::new(20., 30., 120., 40.).unwrap()),
            )
            .unwrap();
        assert_eq!(document.text_boxes()[0].rich_text_runs().len(), 2);
        document
            .edit_annotation(
                &text_id,
                AnnotationEdit::SetTextBoxStyle(
                    TextBoxStyle::new("Arimo", 16., "#123456", 0.75).unwrap(),
                ),
            )
            .unwrap();
        assert_eq!(document.text_boxes()[0].rich_text_runs().len(), 2);
        assert_eq!(
            document
                .edit_annotation(
                    &text_id,
                    AnnotationEdit::SetTextBoxContent("bold italic".into()),
                )
                .unwrap(),
            (AnnotationKind::TextBox, true)
        );
        assert!(document.text_boxes()[0].rich_text_runs().is_empty());
        assert!(document.undo().unwrap());
        assert_eq!(document.text_boxes()[0].rich_text_runs().len(), 2);
    }

    #[test]
    fn recovery_round_trips_rich_runs_and_rejects_invalid_run_state() {
        let text = TextBoxAnnotation::new(
            id("rich:recovery"),
            0,
            PdfRect::new(10., 20., 120., 40.).unwrap(),
            "styled",
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
        )
        .unwrap()
        .with_rich_text_runs(vec![
            TextBoxRichTextRun::new("styled")
                .unwrap()
                .with_font_family("Tinos")
                .unwrap()
                .with_emphasis(true, true)
                .with_color("#123456")
                .unwrap()
                .with_font_size_pt(18.)
                .unwrap(),
        ])
        .unwrap();
        let mut document = AnnotationDocument::default();
        document
            .load_imported_annotations(vec![Annotation::TextBox(text.clone())], Vec::new())
            .unwrap();
        let encoded = document.encode_recovery_timeline().unwrap();
        let hydrated = AnnotationDocument::hydrate_recovery_timeline(&encoded).unwrap();
        assert_eq!(hydrated.text_boxes()[0], text);

        let mut malformed: Value = serde_json::from_slice(&encoded).unwrap();
        malformed["current"]["text_boxes"][0]["rich_text_runs"][0]["text"] = json!("mismatch");
        assert!(
            AnnotationDocument::hydrate_recovery_timeline(&serde_json::to_vec(&malformed).unwrap())
                .is_err()
        );

        let mut legacy: Value = serde_json::from_slice(&encoded).unwrap();
        legacy["current"]["text_boxes"][0]
            .as_object_mut()
            .unwrap()
            .remove("rich_text_runs");
        let hydrated =
            AnnotationDocument::hydrate_recovery_timeline(&serde_json::to_vec(&legacy).unwrap())
                .unwrap();
        assert!(hydrated.text_boxes()[0].rich_text_runs().is_empty());
    }

    #[test]
    fn select_at_moves_focus_to_hit() {
        let mut document = AnnotationDocument::default();
        create_rectangle(&mut document, "a");
        document.clear_selection();
        assert_eq!(document.focused_id(), None);

        let miss = document.select_at(0, point(500.0, 500.0), 1.0).unwrap();
        assert!(miss.is_none());
        assert_eq!(document.focused_id(), None);

        let hit = document.select_at(0, point(50.0, 40.0), 1.0).unwrap();
        assert!(hit.is_some());
        assert_eq!(document.focused_id(), Some(&id("a")));
    }
}
