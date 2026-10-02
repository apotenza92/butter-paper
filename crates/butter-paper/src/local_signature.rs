//! Bounded, application-owned input and rasterization boundaries for signatures.

use std::{error::Error, fmt};

use crate::annotation_model::DecodedRgbaAsset;
use cosmic_text::{Align, Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, SwashCache};

pub const MAX_SIGNATURE_STROKES: usize = 64;
pub const MAX_SIGNATURE_POINTS: usize = 4_096;
pub const SIGNATURE_RASTER_WIDTH: u32 = 2048;
pub const SIGNATURE_RASTER_HEIGHT: u32 = 768;
pub const MAX_TYPED_SIGNATURE_NAME_CHARS: usize = 80;

const NORMALIZED_MAX: u64 = u16::MAX as u64;
const BRUSH_RADIUS: f32 = 4. * (2048. / 768.);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedSignaturePoint {
    pub x: u16,
    pub y: u16,
}

impl NormalizedSignaturePoint {
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DrawnSignatureError {
    TooManyStrokes,
    TooManyPoints,
    NoActiveStroke,
    Empty,
    InvalidRaster(String),
}

impl fmt::Display for DrawnSignatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyStrokes => write!(
                formatter,
                "a drawn signature can contain at most {MAX_SIGNATURE_STROKES} strokes"
            ),
            Self::TooManyPoints => write!(
                formatter,
                "a drawn signature can contain at most {MAX_SIGNATURE_POINTS} points"
            ),
            Self::NoActiveStroke => write!(formatter, "the drawn signature has no active stroke"),
            Self::Empty => write!(formatter, "Draw a signature before adding it."),
            Self::InvalidRaster(error) => error.fmt(formatter),
        }
    }
}

impl Error for DrawnSignatureError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedSignatureError {
    EmptyName,
    NameTooLong { actual: usize, max: usize },
    InvalidRaster(String),
}

impl fmt::Display for TypedSignatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(formatter, "Enter a name before adding a typed signature."),
            Self::NameTooLong { actual, max } => write!(
                formatter,
                "a typed signature name can contain at most {max} characters (received {actual})"
            ),
            Self::InvalidRaster(error) => error.fmt(formatter),
        }
    }
}

impl Error for TypedSignatureError {}

/// Validated, application-owned input for a typed signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedSignature {
    name: String,
}

impl TypedSignature {
    pub fn new(name: impl AsRef<str>) -> Result<Self, TypedSignatureError> {
        let name = name.as_ref().trim();
        if name.is_empty() {
            return Err(TypedSignatureError::EmptyName);
        }
        let character_count = name.chars().count();
        if character_count > MAX_TYPED_SIGNATURE_NAME_CHARS {
            return Err(TypedSignatureError::NameTooLong {
                actual: character_count,
                max: MAX_TYPED_SIGNATURE_NAME_CHARS,
            });
        }
        Ok(Self {
            name: name.to_owned(),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn rasterize(&self) -> Result<DecodedRgbaAsset, TypedSignatureError> {
        let mut font_database = cosmic_text::fontdb::Database::new();
        font_database.load_font_data(include_bytes!("../assets/fonts/Allura-Regular.ttf").to_vec());
        let mut font_system = FontSystem::new_with_locale_and_db("en-US".to_owned(), font_database);
        let mut buffer = Buffer::new(
            &mut font_system,
            Metrics::new(128. * (2048. / 768.), 176. * (2048. / 768.)),
        );
        let mut buffer = buffer.borrow_with(&mut font_system);
        buffer.set_size(
            Some(SIGNATURE_RASTER_WIDTH as f32),
            Some(SIGNATURE_RASTER_HEIGHT as f32),
        );
        buffer.set_text(
            &self.name,
            &Attrs::new().family(Family::Name("Allura")),
            Shaping::Advanced,
            Some(Align::Center),
        );

        let width = SIGNATURE_RASTER_WIDTH as usize;
        let height = SIGNATURE_RASTER_HEIGHT as usize;
        let mut alpha = vec![0_u8; width * height];
        let mut cache = SwashCache::new();
        buffer.draw(&mut cache, Color::rgb(17, 24, 39), |x, y, w, h, color| {
            for offset_y in 0..h as i32 {
                let target_y = y + offset_y;
                if !(0..height as i32).contains(&target_y) {
                    continue;
                }
                for offset_x in 0..w as i32 {
                    let target_x = x + offset_x;
                    if !(0..width as i32).contains(&target_x) {
                        continue;
                    }
                    let index = target_y as usize * width + target_x as usize;
                    alpha[index] = alpha[index].max(color.a());
                }
            }
        });
        alpha_to_asset(&alpha).map_err(TypedSignatureError::InvalidRaster)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DrawnSignature {
    strokes: Vec<Vec<NormalizedSignaturePoint>>,
    point_count: usize,
    active_stroke: Option<usize>,
}

impl DrawnSignature {
    pub fn begin_stroke(
        &mut self,
        point: NormalizedSignaturePoint,
    ) -> Result<(), DrawnSignatureError> {
        if self.strokes.len() >= MAX_SIGNATURE_STROKES {
            return Err(DrawnSignatureError::TooManyStrokes);
        }
        if self.point_count >= MAX_SIGNATURE_POINTS {
            return Err(DrawnSignatureError::TooManyPoints);
        }
        self.strokes.push(vec![point]);
        self.point_count += 1;
        self.active_stroke = Some(self.strokes.len() - 1);
        Ok(())
    }

    pub fn append_point(
        &mut self,
        point: NormalizedSignaturePoint,
    ) -> Result<(), DrawnSignatureError> {
        let stroke_ix = self
            .active_stroke
            .ok_or(DrawnSignatureError::NoActiveStroke)?;
        if self.point_count >= MAX_SIGNATURE_POINTS {
            return Err(DrawnSignatureError::TooManyPoints);
        }
        if self.strokes[stroke_ix].last() == Some(&point) {
            return Ok(());
        }
        self.strokes[stroke_ix].push(point);
        self.point_count += 1;
        Ok(())
    }

    pub fn end_stroke(&mut self) {
        self.active_stroke = None;
    }

    pub fn clear(&mut self) {
        self.strokes.clear();
        self.point_count = 0;
        self.active_stroke = None;
    }

    pub fn is_empty(&self) -> bool {
        self.point_count == 0
    }

    pub fn has_active_stroke(&self) -> bool {
        self.active_stroke.is_some()
    }

    pub fn stroke_count(&self) -> usize {
        self.strokes.len()
    }

    pub fn point_count(&self) -> usize {
        self.point_count
    }

    pub fn strokes(&self) -> &[Vec<NormalizedSignaturePoint>] {
        &self.strokes
    }

    pub fn rasterize(&self) -> Result<DecodedRgbaAsset, DrawnSignatureError> {
        if self.is_empty() {
            return Err(DrawnSignatureError::Empty);
        }
        let width = SIGNATURE_RASTER_WIDTH as usize;
        let height = SIGNATURE_RASTER_HEIGHT as usize;
        let mut alpha = vec![0_u8; width * height];
        for stroke in &self.strokes {
            let first = raster_point(stroke[0]);
            stamp_disk(&mut alpha, first.0, first.1);
            for segment in stroke.windows(2) {
                let from = raster_point(segment[0]);
                let to = raster_point(segment[1]);
                rasterize_segment(&mut alpha, from, to);
            }
        }

        alpha_to_asset(&alpha).map_err(DrawnSignatureError::InvalidRaster)
    }
}

fn alpha_to_asset(alpha: &[u8]) -> Result<DecodedRgbaAsset, String> {
    let mut left = SIGNATURE_RASTER_WIDTH;
    let mut top = SIGNATURE_RASTER_HEIGHT;
    let mut right = 0;
    let mut bottom = 0;
    for y in 0..SIGNATURE_RASTER_HEIGHT {
        for x in 0..SIGNATURE_RASTER_WIDTH {
            if alpha[(y * SIGNATURE_RASTER_WIDTH + x) as usize] != 0 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    if left == SIGNATURE_RASTER_WIDTH {
        return Err("The signature did not produce any visible ink.".into());
    }
    let ink_extent = (right - left + 1).max(bottom - top + 1);
    let padding = 4_u32.max((ink_extent * 4 + 99) / 100);
    left = left.saturating_sub(padding);
    top = top.saturating_sub(padding);
    right = right
        .saturating_add(padding)
        .min(SIGNATURE_RASTER_WIDTH - 1);
    bottom = bottom
        .saturating_add(padding)
        .min(SIGNATURE_RASTER_HEIGHT - 1);

    let output_width = right - left + 1;
    let output_height = bottom - top + 1;
    let mut rgba = Vec::with_capacity((output_width * output_height * 4) as usize);
    for y in top..=bottom {
        for x in left..=right {
            rgba.extend_from_slice(&[17, 24, 39, alpha[(y * SIGNATURE_RASTER_WIDTH + x) as usize]]);
        }
    }
    DecodedRgbaAsset::new(output_width, output_height, rgba).map_err(|error| error.to_string())
}

fn raster_point(point: NormalizedSignaturePoint) -> (i32, i32) {
    let x = (u64::from(point.x) * u64::from(SIGNATURE_RASTER_WIDTH - 1) + NORMALIZED_MAX / 2)
        / NORMALIZED_MAX;
    let y = (u64::from(point.y) * u64::from(SIGNATURE_RASTER_HEIGHT - 1) + NORMALIZED_MAX / 2)
        / NORMALIZED_MAX;
    (x as i32, y as i32)
}

fn rasterize_segment(alpha: &mut [u8], from: (i32, i32), to: (i32, i32)) {
    let (mut x, mut y) = from;
    let dx = (to.0 - x).abs();
    let sx = if x < to.0 { 1 } else { -1 };
    let dy = -(to.1 - y).abs();
    let sy = if y < to.1 { 1 } else { -1 };
    let mut error = dx + dy;
    loop {
        stamp_disk(alpha, x, y);
        if x == to.0 && y == to.1 {
            break;
        }
        let doubled = error * 2;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
    }
}

fn stamp_disk(alpha: &mut [u8], center_x: i32, center_y: i32) {
    let extent = (BRUSH_RADIUS + 1.).ceil() as i32;
    for offset_y in -extent..=extent {
        for offset_x in -extent..=extent {
            let distance = ((offset_x * offset_x + offset_y * offset_y) as f32).sqrt();
            let coverage = ((BRUSH_RADIUS + 0.5 - distance).clamp(0., 1.) * 255.).round() as u8;
            if coverage == 0 {
                continue;
            }
            let x = center_x + offset_x;
            let y = center_y + offset_y;
            if x >= 0
                && y >= 0
                && x < SIGNATURE_RASTER_WIDTH as i32
                && y < SIGNATURE_RASTER_HEIGHT as i32
            {
                let ix = y as usize * SIGNATURE_RASTER_WIDTH as usize + x as usize;
                alpha[ix] = alpha[ix].max(coverage);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawn_signature_has_high_resolution_antialiased_edges() {
        let mut drawing = DrawnSignature::default();
        drawing
            .begin_stroke(NormalizedSignaturePoint::new(8000, 55000))
            .unwrap();
        drawing
            .append_point(NormalizedSignaturePoint::new(57000, 8000))
            .unwrap();
        drawing.end_stroke();
        let asset = drawing.rasterize().unwrap();
        assert!(asset.width_px() > 1500);
        assert!(
            asset
                .rgba()
                .chunks_exact(4)
                .any(|pixel| pixel[3] > 0 && pixel[3] < 255)
        );
        assert_eq!(asset, drawing.rasterize().unwrap());
    }

    #[test]
    fn typed_signature_trims_and_preserves_a_valid_name() {
        let signature = TypedSignature::new("  Ada Lovelace\n").unwrap();

        assert_eq!(signature.name(), "Ada Lovelace");
    }

    #[test]
    fn typed_signature_rejects_empty_trimmed_names() {
        assert_eq!(
            TypedSignature::new(" \t\n "),
            Err(TypedSignatureError::EmptyName)
        );
    }

    #[test]
    fn typed_signature_enforces_the_character_limit() {
        let at_limit = "é".repeat(MAX_TYPED_SIGNATURE_NAME_CHARS);
        assert_eq!(TypedSignature::new(&at_limit).unwrap().name(), at_limit);

        let over_limit = format!("{at_limit}x");
        assert_eq!(
            TypedSignature::new(over_limit),
            Err(TypedSignatureError::NameTooLong {
                actual: MAX_TYPED_SIGNATURE_NAME_CHARS + 1,
                max: MAX_TYPED_SIGNATURE_NAME_CHARS,
            })
        );
    }

    #[test]
    fn typed_signature_rasterization_is_visible_bounded_and_deterministic() {
        let signature = TypedSignature::new("Ada Lovelace").unwrap();
        let first = signature.rasterize().unwrap();
        let second = signature.rasterize().unwrap();

        assert_eq!(first, second);
        assert!(first.width_px() > first.height_px());
        assert!(first.width_px() <= SIGNATURE_RASTER_WIDTH);
        assert!(first.height_px() <= SIGNATURE_RASTER_HEIGHT);
        assert!(first.rgba().chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}
