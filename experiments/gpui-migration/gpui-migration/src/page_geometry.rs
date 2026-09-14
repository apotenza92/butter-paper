//! GPUI-free page coordinate space shared by the native viewer and annotations.
//!
//! PDF points stay in the document's raw, unrotated user space. Viewport
//! coordinates are top-left-origin and scale by `zoom * /UserUnit`. The crop
//! box, rather than the media box, defines the visible origin and extent.

use std::{error::Error, fmt};

use lopdf::{Document, Object, ObjectId};

/// Public alias for the parser-owned metadata document. Consumers need not
/// add a second direct `lopdf` dependency to use the worker's parsed snapshot.
pub type PdfMetadataDocument = Document;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfPoint {
    pub x: f64,
    pub y: f64,
}

impl PdfPoint {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
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
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, GeometryError> {
        for (name, value) in [("x", x), ("y", y), ("width", width), ("height", height)] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if width < 0.0 || height < 0.0 {
            return Err(GeometryError::NegativeExtent);
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    pub fn right(self) -> f64 {
        self.x + self.width
    }

    pub fn top(self) -> f64 {
        self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Rotation {
    Degrees0,
    Degrees90,
    Degrees180,
    Degrees270,
}

impl Rotation {
    pub fn from_degrees(degrees: i64) -> Result<Self, GeometryError> {
        match degrees.rem_euclid(360) {
            0 => Ok(Self::Degrees0),
            90 => Ok(Self::Degrees90),
            180 => Ok(Self::Degrees180),
            270 => Ok(Self::Degrees270),
            value => Err(GeometryError::InvalidRotation(value)),
        }
    }

    pub const fn degrees(self) -> u16 {
        match self {
            Self::Degrees0 => 0,
            Self::Degrees90 => 90,
            Self::Degrees180 => 180,
            Self::Degrees270 => 270,
        }
    }

    pub const fn swaps_axes(self) -> bool {
        matches!(self, Self::Degrees90 | Self::Degrees270)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GeometryError {
    NonFinite(&'static str),
    NegativeExtent,
    NonPositivePageBox(&'static str),
    InvalidUserUnit,
    InvalidRotation(i64),
    MissingPageBox(&'static str),
    Pdf(String),
    InheritanceDepth,
}

impl fmt::Display for GeometryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite(name) => write!(formatter, "{name} must be finite"),
            Self::NegativeExtent => write!(formatter, "rectangle extents must be nonnegative"),
            Self::NonPositivePageBox(name) => {
                write!(formatter, "{name} must have positive extents")
            }
            Self::InvalidUserUnit => write!(formatter, "/UserUnit must be finite and positive"),
            Self::InvalidRotation(degrees) => {
                write!(
                    formatter,
                    "page rotation must be a quarter turn, received {degrees} degrees"
                )
            }
            Self::MissingPageBox(name) => write!(formatter, "page is missing {name}"),
            Self::Pdf(detail) => write!(formatter, "invalid PDF page dictionary: {detail}"),
            Self::InheritanceDepth => write!(formatter, "PDF page parent chain exceeded the limit"),
        }
    }
}

impl Error for GeometryError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageCoordinateSpace {
    media_box: PdfRect,
    view_box: PdfRect,
    rotation: Rotation,
    user_unit: f64,
}

impl PageCoordinateSpace {
    pub fn new(
        media_box: PdfRect,
        view_box: PdfRect,
        rotation: Rotation,
        user_unit: f64,
    ) -> Result<Self, GeometryError> {
        for (name, value) in [
            ("media_box.x", media_box.x),
            ("media_box.y", media_box.y),
            ("media_box.width", media_box.width),
            ("media_box.height", media_box.height),
            ("view_box.x", view_box.x),
            ("view_box.y", view_box.y),
            ("view_box.width", view_box.width),
            ("view_box.height", view_box.height),
        ] {
            if !value.is_finite() {
                return Err(GeometryError::NonFinite(name));
            }
        }
        if media_box.width <= 0.0 || media_box.height <= 0.0 {
            return Err(GeometryError::NonPositivePageBox("media box"));
        }
        if view_box.width <= 0.0 || view_box.height <= 0.0 {
            return Err(GeometryError::NonPositivePageBox("crop box"));
        }
        if !user_unit.is_finite() || user_unit <= 0.0 {
            return Err(GeometryError::InvalidUserUnit);
        }
        Ok(Self {
            media_box,
            view_box,
            rotation,
            user_unit,
        })
    }

    /// Reads page boxes and inherited page attributes without relying on
    /// PDFium's incomplete metadata surface. The page dictionary wins over
    /// each ancestor, as required by the PDF page-tree contract.
    pub fn from_lopdf_page(document: &Document, page_id: ObjectId) -> Result<Self, GeometryError> {
        let media = inherited_value(document, page_id, b"MediaBox")?
            .ok_or(GeometryError::MissingPageBox("/MediaBox"))
            .and_then(|object| parse_box(object, "/MediaBox"))?;
        let view_box = match inherited_value(document, page_id, b"CropBox")? {
            Some(object) => parse_box(object, "/CropBox")?,
            None => media,
        };
        let rotation = inherited_value(document, page_id, b"Rotate")?
            .map(|object| {
                object
                    .as_i64()
                    .map_err(|error| GeometryError::Pdf(error.to_string()))
            })
            .transpose()?
            .unwrap_or(0);
        let rotation = Rotation::from_degrees(rotation)?;
        let user_unit = inherited_value(document, page_id, b"UserUnit")?
            .map(|object| {
                object
                    .as_float()
                    .map(f64::from)
                    .map_err(|error| GeometryError::Pdf(error.to_string()))
            })
            .transpose()?
            .unwrap_or(1.0);
        Self::new(media, view_box, rotation, user_unit)
    }

    pub const fn media_box(self) -> PdfRect {
        self.media_box
    }

    pub const fn view_box(self) -> PdfRect {
        self.view_box
    }

    pub const fn rotation(self) -> Rotation {
        self.rotation
    }

    pub const fn user_unit(self) -> f64 {
        self.user_unit
    }

    /// Returns the same raw page boxes and `/UserUnit` with a different
    /// effective quarter-turn. Annotation edits can change the displayed
    /// rotation without changing the PDF page's source boxes.
    pub const fn with_rotation(self, rotation: Rotation) -> Self {
        Self { rotation, ..self }
    }

    pub fn display_size_points(self) -> (f64, f64) {
        let width = self.view_box.width * self.user_unit;
        let height = self.view_box.height * self.user_unit;
        if self.rotation.swaps_axes() {
            (height, width)
        } else {
            (width, height)
        }
    }

    pub fn pdf_to_viewport(self, point: PdfPoint) -> PdfPoint {
        let unscaled = self.rotate_pdf_to_unscaled_viewport(point);
        PdfPoint::new(unscaled.x * self.user_unit, unscaled.y * self.user_unit)
    }

    pub fn viewport_to_pdf(self, point: PdfPoint) -> PdfPoint {
        let unscaled = PdfPoint::new(point.x / self.user_unit, point.y / self.user_unit);
        self.rotate_unscaled_viewport_to_pdf(unscaled)
    }

    pub fn pdf_rect_to_viewport(self, rect: PdfRect) -> PdfRect {
        let corners = [
            PdfPoint::new(rect.x, rect.y),
            PdfPoint::new(rect.right(), rect.y),
            PdfPoint::new(rect.x, rect.top()),
            PdfPoint::new(rect.right(), rect.top()),
        ]
        .map(|point| self.pdf_to_viewport(point));
        rect_from_points(corners)
    }

    pub fn viewport_rect_to_pdf(self, rect: PdfRect) -> PdfRect {
        let corners = [
            PdfPoint::new(rect.x, rect.y),
            PdfPoint::new(rect.right(), rect.y),
            PdfPoint::new(rect.x, rect.top()),
            PdfPoint::new(rect.right(), rect.top()),
        ]
        .map(|point| self.viewport_to_pdf(point));
        rect_from_points(corners)
    }

    fn rotate_pdf_to_unscaled_viewport(self, point: PdfPoint) -> PdfPoint {
        let right = self.view_box.right();
        let top = self.view_box.top();
        match self.rotation {
            Rotation::Degrees0 => PdfPoint::new(point.x - self.view_box.x, top - point.y),
            Rotation::Degrees90 => {
                PdfPoint::new(point.y - self.view_box.y, point.x - self.view_box.x)
            }
            Rotation::Degrees180 => PdfPoint::new(right - point.x, point.y - self.view_box.y),
            Rotation::Degrees270 => PdfPoint::new(top - point.y, right - point.x),
        }
    }

    fn rotate_unscaled_viewport_to_pdf(self, point: PdfPoint) -> PdfPoint {
        let right = self.view_box.right();
        let top = self.view_box.top();
        match self.rotation {
            Rotation::Degrees0 => PdfPoint::new(self.view_box.x + point.x, top - point.y),
            Rotation::Degrees90 => {
                PdfPoint::new(self.view_box.x + point.y, self.view_box.y + point.x)
            }
            Rotation::Degrees180 => PdfPoint::new(right - point.x, self.view_box.y + point.y),
            Rotation::Degrees270 => PdfPoint::new(right - point.y, top - point.x),
        }
    }
}

fn rect_from_points(points: [PdfPoint; 4]) -> PdfRect {
    let left = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let right = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let top = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect {
        x: left,
        y: bottom,
        width: right - left,
        height: top - bottom,
    }
}

fn inherited_value<'a>(
    document: &'a Document,
    page_id: ObjectId,
    key: &[u8],
) -> Result<Option<&'a Object>, GeometryError> {
    let mut object_id = page_id;
    for _ in 0..64 {
        let dictionary = document
            .get_object(object_id)
            .map_err(|error| GeometryError::Pdf(error.to_string()))?
            .as_dict()
            .map_err(|error| GeometryError::Pdf(error.to_string()))?;
        if let Ok(value) = dictionary.get(key) {
            return resolve_object(document, value).map(Some);
        }
        object_id = match dictionary.get(b"Parent") {
            Ok(Object::Reference(parent)) => *parent,
            _ => return Ok(None),
        };
    }
    Err(GeometryError::InheritanceDepth)
}

fn resolve_object<'a>(
    document: &'a Document,
    object: &'a Object,
) -> Result<&'a Object, GeometryError> {
    let mut current = object;
    for _ in 0..64 {
        current = match current {
            Object::Reference(object_id) => document
                .get_object(*object_id)
                .map_err(|error| GeometryError::Pdf(error.to_string()))?,
            _ => return Ok(current),
        };
    }
    Err(GeometryError::InheritanceDepth)
}

fn parse_box(object: &Object, name: &'static str) -> Result<PdfRect, GeometryError> {
    let values = object
        .as_array()
        .map_err(|error| GeometryError::Pdf(error.to_string()))?;
    if values.len() != 4 {
        return Err(GeometryError::Pdf(format!(
            "{name} must contain four numbers"
        )));
    }
    let mut numbers = [0.0; 4];
    for (index, value) in values.iter().enumerate() {
        numbers[index] = f64::from(
            value
                .as_float()
                .map_err(|error| GeometryError::Pdf(error.to_string()))?,
        );
    }
    let [left, bottom, right, top] = numbers;
    PdfRect::new(left, bottom, right - left, top - bottom)
}
