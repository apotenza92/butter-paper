//! Visual search: find more of a symbol on rendered pages.
//!
//! The method is local template matching. It has no learned model and no
//! new dependency. All images are single-channel
//! grey; colour is ignored. Internally ink is high and paper is 0.
//!
//! 1. [`VisualTemplate`] cuts the template from a page box or takes a pasted
//!    picture, trims it to its ink and refuses a blank or uniform template.
//! 2. [`VisualTemplate::prepare`] makes the variants for one page scale: four
//!    quarter turns, each also mirrored, and a geometric ladder of sizes.
//!    Orientations that give the same picture (a symmetric symbol) are made
//!    once. Each variant gets a
//!    3×3 box blur and a pyramid of 2×2 averages, as the page does.
//! 3. [`PreparedPage::new`] converts a page render to blurred ink and builds
//!    its pyramid.
//! 4. [`PreparedTemplate::search_page`] runs a coarse pass at the pyramid
//!    level where the variant is 6 to 12 px on its short side. An ink
//!    prefilter skips most positions, the rest get zero-mean normalised
//!    cross-correlation (ZNCC). The best candidate of each neighbourhood
//!    moves down the pyramid with a hill climb at each level. At full
//!    resolution the neighbouring half-step sizes are also tried.
//!    Non-maximum suppression across variants keeps the best box of each
//!    group of boxes that overlap (IoU) or that lie inside a better box.
//!
//! The functions are single-threaded. The caller prepares the template once,
//! shares it between threads and runs one page on each thread. The cancel
//! flag is read between the preparation stages of a page (about 20 ms for
//! an A1 page in total), on every row of the coarse pass and before each
//! candidate, so a search ends within milliseconds of a cancel.
//!
//! Memory for each page: the grey input (1 byte for each pixel, owned by the
//! caller), the prepared page (about 1.33 bytes for each pixel) and the coarse
//! level as floats (4 bytes for each coarse pixel, a sixteenth of the page at
//! level 2). An A1 sheet at 1 px/pt (4.0 Mpx) uses about 4 + 5.3 + 1 MB.

use std::{
    error::Error,
    fmt,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use crate::page_geometry::{PageCoordinateSpace, PdfRect, Rotation};

/// The lowest score a search keeps. The similarity slider filters from here.
pub const SCORE_FLOOR: f32 = 0.60;
/// The similarity the panel shows first.
pub const DEFAULT_SIMILARITY: f32 = 0.80;
/// The hits that one page can give.
pub const DEFAULT_MAX_HITS: usize = 10_000;
/// A template is reduced until its long side is at most this.
pub const MAX_TEMPLATE_SIDE_PX: usize = 512;
/// A template with a shorter trimmed side is refused ("Draw a larger box").
pub const MIN_TEMPLATE_SIDE_PX: usize = 12;
/// The largest page render that the matcher accepts.
pub const MAX_PAGE_PIXELS: usize = 16_000_000;
/// The template short side that [`working_scale`] aims for.
pub const TARGET_TEMPLATE_SIDE_PX: f32 = 32.0;

/// Ink above 12 % grey counts when the template is trimmed.
const INK_THRESHOLD: u8 = 31;
const TRIM_MARGIN_PX: usize = 2;
/// A template with less ink cover than this is refused.
const MIN_INK_COVER: f64 = 0.005;
/// A template with a smaller ink standard deviation is uniform.
const MIN_INK_STD_DEV: f64 = 2.0;
/// The largest ratio between two adjacent sizes. ZNCC on blurred line work
/// tolerates about ±8 %, so 1.18 leaves no gaps.
const SIZE_STEP: f32 = 1.18;
/// The variant short side at its coarse level is 6 to 12 px. A 10 mm
/// symbol (about 29 px after its trim at 1 px/pt) is searched at level 2.
const COARSE_SIDE_PX: usize = 6;
/// Coarse rows are padded to a multiple of this, so dot products have no
/// tail and vectorise.
const LANES: usize = 8;
const MAX_LEVEL: usize = 3;
/// A variant smaller than this at full resolution is not searched.
const MIN_VARIANT_SIDE_PX: usize = 6;
/// A variant larger than this is refused, as its cost is too high.
const MAX_VARIANT_SIDE_PX: usize = 2 * MAX_TEMPLATE_SIDE_PX;
/// A coarse candidate needs `floor - COARSE_MARGIN`, where the floor is the
/// lower of `min_score` and [`SCORE_FLOOR`]. The neighbouring half-step
/// sizes are tried from the same score.
const COARSE_MARGIN: f32 = 0.10;
/// At each finer pyramid level a candidate needs `floor - REFINE_MARGIN`.
/// Finer levels have less blur, but a true hit still scores at least its
/// full-resolution score there; this drops most false candidates early.
const REFINE_MARGIN: f32 = 0.05;
const MAX_CANDIDATES_PER_VARIANT: usize = 500;
/// The window's ink must be in this ratio of the template's ink.
const INK_RATIO_MIN: f64 = 0.4;
const INK_RATIO_MAX: f64 = 2.5;
/// The window's variance must be at least this ratio of the template's.
const VARIANCE_RATIO_MIN: f64 = 0.1;
/// Two hits that overlap by this IoU or more are one hit.
const NMS_IOU: f64 = 0.3;
/// A weaker hit with this part of its area inside a stronger hit is a part
/// of that hit (a small size matching one corner of a symbol).
const NMS_CONTAINED: f64 = 0.7;
/// A hit that overlaps the source box by this IoU is the source.
const SOURCE_IOU: f64 = 0.5;
/// Two orientations of a template with this ZNCC are the same variant.
const DUPLICATE_TURN_SCORE: f32 = 0.95;
const HILL_CLIMB_STEPS: usize = 4;

/// A grey image: one byte for each pixel, 0 is black and 255 is paper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GreyImage {
    width: usize,
    height: usize,
    pixels: Vec<u8>,
}

impl GreyImage {
    pub fn new(width: usize, height: usize, pixels: Vec<u8>) -> Result<Self, VisualSearchError> {
        if width == 0 || height == 0 || pixels.len() != width * height {
            return Err(VisualSearchError::InvalidImage);
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    /// A white image.
    pub fn blank(width: usize, height: usize) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            pixels: vec![255; width.max(1) * height.max(1)],
        }
    }

    /// Converts premultiplied BGRA (the render surface format) with integer
    /// Rec. 601 weights. Transparent pixels become paper.
    pub fn from_bgra(
        width: usize,
        height: usize,
        stride: usize,
        bytes: &[u8],
    ) -> Result<Self, VisualSearchError> {
        if width == 0
            || height == 0
            || stride < width * 4
            || bytes.len() < stride * (height - 1) + width * 4
        {
            return Err(VisualSearchError::InvalidImage);
        }
        let mut pixels = Vec::with_capacity(width * height);
        for row in bytes.chunks(stride).take(height) {
            for pixel in row[..width * 4].chunks_exact(4) {
                let (b, g, r, a) = (
                    u32::from(pixel[0]),
                    u32::from(pixel[1]),
                    u32::from(pixel[2]),
                    u32::from(pixel[3]),
                );
                let luma = (77 * r + 150 * g + 29 * b + 128) >> 8;
                pixels.push((luma + 255 - a).min(255) as u8);
            }
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        &mut self.pixels
    }
}

/// A rectangle in image pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PixelRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl PixelRect {
    fn area(self) -> f64 {
        (self.width * self.height) as f64
    }

    fn overlap(self, other: Self) -> f64 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        if right <= left || bottom <= top {
            return 0.0;
        }
        ((right - left) * (bottom - top)) as f64
    }

    /// The weaker hit `self` is the same place as the stronger `kept`.
    fn duplicates(self, kept: Self) -> bool {
        let overlap = self.overlap(kept);
        let union = self.area() + kept.area() - overlap;
        overlap / union >= NMS_IOU || overlap / self.area().min(kept.area()) >= NMS_CONTAINED
    }

    fn to_viewport(self, scale: f32) -> PdfRect {
        let scale = f64::from(scale);
        PdfRect {
            x: self.x as f64 / scale,
            y: self.y as f64 / scale,
            width: self.width as f64 / scale,
            height: self.height as f64 / scale,
        }
    }
}

fn viewport_iou(a: PdfRect, b: PdfRect) -> f64 {
    let left = a.x.max(b.x);
    let top = a.y.max(b.y);
    let right = a.right().min(b.right());
    let bottom = a.top().min(b.top());
    if right <= left || bottom <= top {
        return 0.0;
    }
    let overlap = (right - left) * (bottom - top);
    overlap / (a.width * a.height + b.width * b.height - overlap)
}

/// Sizes to try, relative to the template's size on the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeRange {
    pub min: f32,
    pub max: f32,
}

impl SizeRange {
    /// The range for a box drawn on a page: 85 % to 118 %.
    pub const PAGE_BOX: Self = Self {
        min: 0.85,
        max: 1.18,
    };
    /// The range for a pasted picture: 50 % to 200 %.
    pub const PASTED_PICTURE: Self = Self { min: 0.5, max: 2.0 };

    /// A geometric ladder from `min` to `max` whose steps are at most 1.18.
    /// The page box range gives 0.85, 1.0 and 1.18.
    pub fn steps(self) -> Vec<f32> {
        let min = self.min.min(self.max);
        let max = self.max.max(self.min);
        if !(min.is_finite() && max.is_finite()) || min <= 0.0 {
            return vec![1.0];
        }
        let ratio = max / min;
        if ratio <= 1.001 {
            return vec![min];
        }
        let count = (ratio.ln() / SIZE_STEP.ln()).ceil() as usize + 1;
        let step = ratio.powf(1.0 / (count - 1) as f32);
        (0..count)
            .map(|index| {
                let size = min * step.powi(index as i32);
                // Show 1.0 exactly when the ladder passes it.
                if (size - 1.0).abs() < 0.01 { 1.0 } else { size }
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VisualSearchOptions {
    /// Also find the template turned 90°, 180° and 270°.
    pub turns: bool,
    /// Also find the template mirrored (flipped left to right), at each turn
    /// that `turns` allows.
    pub mirrored: bool,
    /// Also find the template at other sizes. `None` finds the same size.
    pub sizes: Option<SizeRange>,
    /// Hits below this score are dropped (0..1).
    pub min_score: f32,
    /// The most hits one page gives, best first.
    pub max_hits: usize,
}

impl VisualSearchOptions {
    pub fn for_page_box() -> Self {
        Self {
            turns: true,
            mirrored: true,
            sizes: Some(SizeRange::PAGE_BOX),
            min_score: SCORE_FLOOR,
            max_hits: DEFAULT_MAX_HITS,
        }
    }

    pub fn for_pasted_picture() -> Self {
        Self {
            sizes: Some(SizeRange::PASTED_PICTURE),
            ..Self::for_page_box()
        }
    }
}

impl Default for VisualSearchOptions {
    fn default() -> Self {
        Self::for_page_box()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum VisualSearchError {
    /// The image size and its pixel count do not agree.
    InvalidImage,
    /// A scale is not a positive finite number.
    InvalidScale,
    /// The template is too small after its trim.
    TemplateTooSmall,
    /// The template has no ink or no contrast.
    TemplateEmpty,
    /// A variant of the template is too large for this page scale.
    TemplateTooLarge,
    /// The page has more pixels than [`MAX_PAGE_PIXELS`].
    PageTooLarge,
    /// The page was prepared at a scale other than the template's.
    ScaleMismatch,
    Cancelled,
}

impl fmt::Display for VisualSearchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidImage => "The picture has an invalid size.",
            Self::InvalidScale => "The scale is not a positive number.",
            Self::TemplateTooSmall => "Draw a larger box.",
            Self::TemplateEmpty => "That box has nothing in it to find.",
            Self::TemplateTooLarge => "Draw a smaller box.",
            Self::PageTooLarge => "The page picture is too large to search.",
            Self::ScaleMismatch => "The page and the template have different scales.",
            Self::Cancelled => "The search was stopped.",
        })
    }
}

impl Error for VisualSearchError {}

/// The working scale in pixels per point for a template box and the largest
/// page in scope (both in points). The template short side becomes about
/// 32 px. The result is one of 2, 1.41, 1, 0.71 and 0.5 (or smaller halves),
/// so cached renders are reusable. It is reduced until the largest page has
/// at most [`MAX_PAGE_PIXELS`].
pub fn working_scale(template_short_side_pt: f32, largest_page_pt: (f32, f32)) -> f32 {
    const LADDER: [f32; 5] = [2.0, 1.41, 1.0, 0.71, 0.5];
    let wanted = if template_short_side_pt.is_finite() && template_short_side_pt > 0.0 {
        (TARGET_TEMPLATE_SIDE_PX / template_short_side_pt).clamp(0.5, 2.0)
    } else {
        1.0
    };
    let mut index = LADDER
        .iter()
        .position(|&step| step <= wanted + 1e-4)
        .unwrap_or(LADDER.len() - 1);
    let page_pixels = |scale: f32| {
        f64::from(largest_page_pt.0 * scale).ceil() * f64::from(largest_page_pt.1 * scale).ceil()
    };
    let mut scale = LADDER[index];
    while page_pixels(scale) > MAX_PAGE_PIXELS as f64 {
        if index + 1 < LADDER.len() {
            index += 1;
            scale = LADDER[index];
        } else {
            scale *= std::f32::consts::FRAC_1_SQRT_2;
        }
    }
    scale
}

/// One byte for each pixel, ink high.
#[derive(Clone, Debug)]
struct Plane {
    width: usize,
    height: usize,
    data: Vec<u8>,
}

impl Plane {
    fn row(&self, y: usize) -> &[u8] {
        &self.data[y * self.width..(y + 1) * self.width]
    }
}

/// The picture to find, trimmed to its ink.
#[derive(Clone, Debug)]
pub struct VisualTemplate {
    ink: Plane,
    scale: f32,
    source_box: Option<PdfRect>,
}

impl VisualTemplate {
    /// Cuts `box_viewport` (top-left viewport points) from a page render at
    /// `scale` pixels per point. The trimmed box is the source hit.
    pub fn from_page_box(
        page: &GreyImage,
        scale: f32,
        box_viewport: PdfRect,
    ) -> Result<Self, VisualSearchError> {
        check_scale(scale)?;
        let scale64 = f64::from(scale);
        let left = (box_viewport.x * scale64).floor().max(0.0) as usize;
        let top = (box_viewport.y * scale64).floor().max(0.0) as usize;
        let right = ((box_viewport.right() * scale64).ceil().max(0.0) as usize).min(page.width);
        let bottom = ((box_viewport.top() * scale64).ceil().max(0.0) as usize).min(page.height);
        if right <= left || bottom <= top {
            return Err(VisualSearchError::TemplateTooSmall);
        }
        let (ink, trimmed) = trimmed_ink(page, left, top, right - left, bottom - top)?;
        let source = trimmed.to_viewport(scale);
        Self::from_ink(ink, scale, Some(source))
    }

    /// Takes a pasted picture whose pixels are at `scale` pixels per point
    /// (screen pixels at the zoom of the active tab).
    pub fn from_picture(picture: &GreyImage, scale: f32) -> Result<Self, VisualSearchError> {
        check_scale(scale)?;
        let (ink, _) = trimmed_ink(picture, 0, 0, picture.width, picture.height)?;
        Self::from_ink(ink, scale, None)
    }

    fn from_ink(
        mut ink: Plane,
        mut scale: f32,
        source_box: Option<PdfRect>,
    ) -> Result<Self, VisualSearchError> {
        let long = ink.width.max(ink.height);
        if long > MAX_TEMPLATE_SIDE_PX {
            let factor = MAX_TEMPLATE_SIDE_PX as f32 / long as f32;
            ink = resample(&ink, factor);
            scale *= factor;
        }
        if ink.width.min(ink.height) < MIN_TEMPLATE_SIDE_PX {
            return Err(VisualSearchError::TemplateTooSmall);
        }
        Ok(Self {
            ink,
            scale,
            source_box,
        })
    }

    pub fn width_px(&self) -> usize {
        self.ink.width
    }

    pub fn height_px(&self) -> usize {
        self.ink.height
    }

    /// Pixels per point of the template's own pixels.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The trimmed box in viewport points of the source page, for a template
    /// cut from a page.
    pub fn source_box(&self) -> Option<PdfRect> {
        self.source_box
    }

    /// Makes the variants for pages rendered at `page_scale` pixels per
    /// point. Prepare once and share the result between page threads.
    pub fn prepare(
        &self,
        page_scale: f32,
        options: &VisualSearchOptions,
    ) -> Result<PreparedTemplate, VisualSearchError> {
        check_scale(page_scale)?;
        let base = page_scale / self.scale;
        let (sizes, range) = match options.sizes {
            Some(range) => (range.steps(), Some(range)),
            None => (vec![1.0], None),
        };
        let step_ratio = if sizes.len() > 1 {
            sizes[1] / sizes[0]
        } else {
            1.0
        };
        let orientations = distinct_orientations(&self.ink, base, options.turns, options.mirrored);

        let mut variants = Vec::new();
        for &size in &sizes {
            let scaled = resample(&self.ink, base * size);
            if scaled.width.max(scaled.height) > MAX_VARIANT_SIDE_PX {
                return Err(VisualSearchError::TemplateTooLarge);
            }
            if scaled.width.min(scaled.height) < MIN_VARIANT_SIDE_PX {
                continue;
            }
            let fine_sizes: Vec<f32> = if step_ratio > 1.001 {
                let half = step_ratio.sqrt();
                let (min, max) = range.map_or((size, size), |range| (range.min, range.max));
                [size / half, size * half]
                    .into_iter()
                    .filter(|fine| *fine >= min * 0.999 && *fine <= max * 1.001)
                    .collect()
            } else {
                Vec::new()
            };
            let fine_scaled: Vec<(f32, Plane)> = fine_sizes
                .iter()
                .map(|&fine| (fine, resample(&self.ink, base * fine)))
                .filter(|(_, plane)| plane.width.min(plane.height) >= MIN_VARIANT_SIDE_PX)
                .collect();
            for &orientation in &orientations {
                let level0 = blur3(&orient(&scaled, orientation));
                if let Some(variant) = Variant::new(level0, orientation, size, &fine_scaled) {
                    variants.push(variant);
                }
            }
        }
        if variants.is_empty() {
            return Err(VisualSearchError::TemplateTooSmall);
        }
        Ok(PreparedTemplate {
            page_scale,
            variants,
            min_score: options.min_score.clamp(0.0, 1.0),
            max_hits: options.max_hits,
            source_box: self.source_box,
        })
    }
}

fn check_scale(scale: f32) -> Result<(), VisualSearchError> {
    if scale.is_finite() && scale > 0.0 {
        Ok(())
    } else {
        Err(VisualSearchError::InvalidScale)
    }
}

/// Ink of a region, trimmed to the ink bounding box plus a margin, and the
/// trimmed region in image pixels.
fn trimmed_ink(
    image: &GreyImage,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
) -> Result<(Plane, PixelRect), VisualSearchError> {
    let ink_at = |x: usize, y: usize| 255 - image.pixels[y * image.width + x];
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
    for y in top..top + height {
        for x in left..left + width {
            if ink_at(x, y) > INK_THRESHOLD {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x == usize::MAX {
        return Err(VisualSearchError::TemplateEmpty);
    }
    let x0 = min_x.saturating_sub(TRIM_MARGIN_PX).max(left);
    let y0 = min_y.saturating_sub(TRIM_MARGIN_PX).max(top);
    let x1 = (max_x + 1 + TRIM_MARGIN_PX).min(left + width);
    let y1 = (max_y + 1 + TRIM_MARGIN_PX).min(top + height);
    let rect = PixelRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    };
    let mut data = Vec::with_capacity(rect.width * rect.height);
    for y in y0..y1 {
        data.extend((x0..x1).map(|x| ink_at(x, y)));
    }
    let count = data.len() as f64;
    let inked = data.iter().filter(|&&value| value > INK_THRESHOLD).count() as f64;
    let mean = data.iter().map(|&value| f64::from(value)).sum::<f64>() / count;
    let variance = data
        .iter()
        .map(|&value| (f64::from(value) - mean).powi(2))
        .sum::<f64>()
        / count;
    if inked / count < MIN_INK_COVER || variance.sqrt() < MIN_INK_STD_DEV {
        return Err(VisualSearchError::TemplateEmpty);
    }
    if rect.width.min(rect.height) < MIN_TEMPLATE_SIDE_PX {
        return Err(VisualSearchError::TemplateTooSmall);
    }
    Ok((
        Plane {
            width: rect.width,
            height: rect.height,
            data,
        },
        rect,
    ))
}

/// How a variant lies on the page: the template mirrored left to right
/// first when `mirrored`, then turned clockwise by `turn` quarter turns.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Orientation {
    turn: u8,
    mirrored: bool,
}

/// The orientations that give different pictures. A symmetric template (a
/// circle with a cross) needs only one; a mirrored arrow is the arrow
/// turned, so it adds none.
fn distinct_orientations(ink: &Plane, base: f32, turns: bool, mirrored: bool) -> Vec<Orientation> {
    let scaled = resample(ink, base);
    let mut kept: Vec<(Orientation, Plane)> = Vec::new();
    for mirrored in [false, true].into_iter().take(if mirrored { 2 } else { 1 }) {
        for turn in 0..if turns { 4u8 } else { 1 } {
            let orientation = Orientation { turn, mirrored };
            let plane = blur3(&orient(&scaled, orientation));
            if !kept
                .iter()
                .any(|(_, other)| shifted_similarity(&plane, other) >= DUPLICATE_TURN_SCORE)
            {
                kept.push((orientation, plane));
            }
        }
    }
    kept.into_iter()
        .map(|(orientation, _)| orientation)
        .collect()
}

/// The best ZNCC of two near-equal pictures over shifts of up to 1 px.
fn shifted_similarity(a: &Plane, b: &Plane) -> f32 {
    if a.width.abs_diff(b.width) > 1 || a.height.abs_diff(b.height) > 1 {
        return 0.0;
    }
    let mut best = -1.0f32;
    for dy in -1isize..=1 {
        for dx in -1isize..=1 {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab, mut n) =
                (0f64, 0f64, 0f64, 0f64, 0f64, 0f64);
            for y in 0..a.height as isize {
                let by = y + dy;
                if by < 0 || by >= b.height as isize {
                    continue;
                }
                for x in 0..a.width as isize {
                    let bx = x + dx;
                    if bx < 0 || bx >= b.width as isize {
                        continue;
                    }
                    let va = f64::from(a.data[y as usize * a.width + x as usize]);
                    let vb = f64::from(b.data[by as usize * b.width + bx as usize]);
                    sa += va;
                    sb += vb;
                    saa += va * va;
                    sbb += vb * vb;
                    sab += va * vb;
                    n += 1.0;
                }
            }
            let var_a = saa - sa * sa / n;
            let var_b = sbb - sb * sb / n;
            if var_a > 0.0 && var_b > 0.0 {
                best = best.max(((sab - sa * sb / n) / (var_a * var_b).sqrt()) as f32);
            }
        }
    }
    best
}

/// A template level: zero-mean values and their statistics.
#[derive(Clone, Debug)]
struct LevelTemplate {
    width: usize,
    height: usize,
    /// `T - mean(T)`, row by row.
    values: Vec<f32>,
    /// `Σ (T - mean)²`.
    norm2: f64,
    /// `Σ T`.
    sum: f64,
    /// `values` with each row padded with zeros to `padded_width`.
    padded: Vec<f32>,
    padded_width: usize,
}

impl LevelTemplate {
    fn new(plane: &Plane) -> Option<Self> {
        let count = plane.data.len() as f64;
        let sum = plane
            .data
            .iter()
            .map(|&value| f64::from(value))
            .sum::<f64>();
        let mean = sum / count;
        let values: Vec<f32> = plane
            .data
            .iter()
            .map(|&value| (f64::from(value) - mean) as f32)
            .collect();
        let norm2 = values
            .iter()
            .map(|&value| f64::from(value).powi(2))
            .sum::<f64>();
        let padded_width = plane.width.div_ceil(LANES) * LANES;
        let mut padded = vec![0f32; padded_width * plane.height];
        for (target, source) in padded
            .chunks_exact_mut(padded_width)
            .zip(values.chunks_exact(plane.width))
        {
            target[..plane.width].copy_from_slice(source);
        }
        (norm2 > 1e-3 && sum > 0.0).then_some(Self {
            width: plane.width,
            height: plane.height,
            values,
            norm2,
            sum,
            padded,
            padded_width,
        })
    }

    fn bytes(&self) -> usize {
        (self.values.len() + self.padded.len()) * std::mem::size_of::<f32>()
    }
}

#[derive(Clone, Debug)]
struct FineTemplate {
    size: f32,
    template: LevelTemplate,
}

#[derive(Clone, Debug)]
struct Variant {
    orientation: Orientation,
    size: f32,
    coarse_level: usize,
    /// Levels 0 to `coarse_level`.
    levels: Vec<LevelTemplate>,
    /// Full-resolution templates at the neighbouring half-step sizes.
    fine: Vec<FineTemplate>,
}

impl Variant {
    fn new(
        level0: Plane,
        orientation: Orientation,
        size: f32,
        fine_scaled: &[(f32, Plane)],
    ) -> Option<Self> {
        let short = level0.width.min(level0.height);
        let coarse_level = ((short / COARSE_SIDE_PX).max(1).ilog2() as usize).min(MAX_LEVEL);
        let mut planes = vec![level0];
        for _ in 0..coarse_level {
            let next = halve(planes.last()?);
            planes.push(next);
        }
        let levels = planes
            .iter()
            .map(LevelTemplate::new)
            .collect::<Option<Vec<_>>>()?;
        let fine = fine_scaled
            .iter()
            .filter_map(|(fine_size, plane)| {
                LevelTemplate::new(&blur3(&orient(plane, orientation))).map(|template| {
                    FineTemplate {
                        size: *fine_size,
                        template,
                    }
                })
            })
            .collect();
        Some(Self {
            orientation,
            size,
            coarse_level,
            levels,
            fine,
        })
    }
}

/// What a variant is, for reports and the benchmark.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VariantInfo {
    pub rotation: Rotation,
    pub mirrored: bool,
    pub size: f32,
    pub width_px: usize,
    pub height_px: usize,
    pub coarse_level: usize,
}

/// The template's variants at one page scale. It is `Sync`: share one
/// between the threads that search pages.
#[derive(Clone, Debug)]
pub struct PreparedTemplate {
    page_scale: f32,
    variants: Vec<Variant>,
    min_score: f32,
    max_hits: usize,
    source_box: Option<PdfRect>,
}

/// A page made ready for matching: blurred ink and its pyramid.
#[derive(Clone, Debug)]
pub struct PreparedPage {
    scale: f32,
    levels: Vec<Plane>,
}

impl PreparedPage {
    /// Converts a page render at `scale` pixels per point.
    pub fn new(
        image: &GreyImage,
        scale: f32,
        cancel: &AtomicBool,
    ) -> Result<Self, VisualSearchError> {
        check_scale(scale)?;
        if image.width * image.height > MAX_PAGE_PIXELS {
            return Err(VisualSearchError::PageTooLarge);
        }
        let ink = Plane {
            width: image.width,
            height: image.height,
            data: image.pixels.iter().map(|&value| 255 - value).collect(),
        };
        check_cancel(cancel)?;
        let mut levels = vec![blur3(&ink)];
        drop(ink);
        for _ in 0..MAX_LEVEL {
            check_cancel(cancel)?;
            let last = levels.last().expect("level 0 exists");
            if last.width < 2 || last.height < 2 {
                break;
            }
            let next = halve(last);
            levels.push(next);
        }
        Ok(Self { scale, levels })
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// The bytes this page keeps while it is searched.
    pub fn memory_bytes(&self) -> usize {
        self.levels.iter().map(|level| level.data.len()).sum()
    }
}

/// One place where the template was found.
#[derive(Clone, Debug, PartialEq)]
pub struct VisualHit {
    /// The box in page render pixels.
    pub pixel_rect: PixelRect,
    /// The box in top-left viewport points (display orientation).
    pub viewport_rect: PdfRect,
    /// The box in PDF user space, when the caller gave the page space.
    pub pdf_rect: Option<PdfRect>,
    /// How the template is turned (clockwise) on the page, after its
    /// mirror when `mirrored`.
    pub rotation: Rotation,
    /// The template is mirrored (flipped left to right) on the page.
    pub mirrored: bool,
    /// The size relative to the template.
    pub size: f32,
    /// ZNCC in 0..1. Show it as a percentage.
    pub score: f32,
    /// This hit is the box that the template came from.
    pub source: bool,
}

/// Counts and times for one page, for reports and the benchmark.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageStats {
    /// Coarse positions over all variants.
    pub positions: u64,
    /// Positions that passed the ink prefilter and got ZNCC.
    pub correlated: u64,
    /// Local maxima that went to refinement.
    pub candidates: u64,
    /// Bytes of the prepared page.
    pub page_bytes: usize,
    /// Peak extra bytes the matching used (float coarse levels, candidates).
    pub working_bytes: usize,
    pub prepare_time: Duration,
    pub match_time: Duration,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageMatches {
    /// Best score first.
    pub hits: Vec<VisualHit>,
    pub stats: PageStats,
}

/// The page that [`PreparedTemplate::search_page`] searches.
#[derive(Clone, Copy, Debug)]
pub struct VisualPage<'a> {
    /// The page content render (no markups), in display orientation.
    pub image: &'a GreyImage,
    /// Pixels per viewport point. It must equal the template's page scale.
    pub scale: f32,
    /// Converts hits to PDF user space when it is given.
    pub space: Option<PageCoordinateSpace>,
    /// The template was cut from this page: mark or add the source hit.
    pub contains_source: bool,
}

#[derive(Clone, Copy, Debug)]
struct RawHit {
    rect: PixelRect,
    orientation: Orientation,
    size: f32,
    score: f32,
}

impl PreparedTemplate {
    pub fn page_scale(&self) -> f32 {
        self.page_scale
    }

    pub fn variant_count(&self) -> usize {
        self.variants.len()
    }

    pub fn variants(&self) -> Vec<VariantInfo> {
        self.variants
            .iter()
            .map(|variant| VariantInfo {
                rotation: quarter_rotation(variant.orientation.turn),
                mirrored: variant.orientation.mirrored,
                size: variant.size,
                width_px: variant.levels[0].width,
                height_px: variant.levels[0].height,
                coarse_level: variant.coarse_level,
            })
            .collect()
    }

    /// The bytes the variants keep.
    pub fn memory_bytes(&self) -> usize {
        self.variants
            .iter()
            .map(|variant| {
                variant
                    .levels
                    .iter()
                    .map(LevelTemplate::bytes)
                    .sum::<usize>()
                    + variant
                        .fine
                        .iter()
                        .map(|fine| fine.template.bytes())
                        .sum::<usize>()
            })
            .sum()
    }

    /// A copy that keeps only the variants `keep` selects (for benchmarks).
    pub fn only_variants(&self, keep: impl Fn(usize) -> bool) -> Self {
        Self {
            variants: self
                .variants
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, variant)| variant.clone())
                .collect(),
            ..self.clone()
        }
    }

    /// Prepares and searches one page.
    pub fn search_page(
        &self,
        page: VisualPage<'_>,
        cancel: &AtomicBool,
    ) -> Result<PageMatches, VisualSearchError> {
        let started = Instant::now();
        let prepared = PreparedPage::new(page.image, page.scale, cancel)?;
        let prepare_time = started.elapsed();
        let mut matches =
            self.search_prepared(&prepared, page.space, page.contains_source, cancel)?;
        matches.stats.prepare_time = prepare_time;
        Ok(matches)
    }

    /// Searches a page that is already prepared.
    pub fn search_prepared(
        &self,
        page: &PreparedPage,
        space: Option<PageCoordinateSpace>,
        contains_source: bool,
        cancel: &AtomicBool,
    ) -> Result<PageMatches, VisualSearchError> {
        let started = Instant::now();
        if (page.scale - self.page_scale).abs() > self.page_scale * 1e-4 {
            return Err(VisualSearchError::ScaleMismatch);
        }
        let mut stats = PageStats {
            page_bytes: page.memory_bytes(),
            ..PageStats::default()
        };
        let cutoff = self.floor() - COARSE_MARGIN;
        let mut float_levels: Vec<Option<FloatPlane>> = vec![None; page.levels.len()];
        let mut raw = Vec::new();
        for variant in &self.variants {
            check_cancel(cancel)?;
            let level = variant.coarse_level;
            let Some(plane) = page.levels.get(level) else {
                continue;
            };
            let floats = float_levels[level].get_or_insert_with(|| FloatPlane::new(plane));
            let template = &variant.levels[level];
            let candidates = coarse_pass(plane, floats, template, cutoff, cancel, &mut stats)?;
            let radius = (template.width.min(template.height) / 2).max(1);
            let kept = suppress_nearby(candidates, radius, plane.width, plane.height);
            stats.candidates += kept.len() as u64;
            let floats_bytes: usize = float_levels
                .iter()
                .flatten()
                .map(|level| level.data.len() * std::mem::size_of::<f32>())
                .sum();
            stats.working_bytes = stats
                .working_bytes
                .max(floats_bytes + kept.capacity() * std::mem::size_of::<(f32, usize, usize)>());
            for (_, x, y) in kept {
                check_cancel(cancel)?;
                if let Some(hit) = self.refine(page, variant, x, y) {
                    raw.push(hit);
                }
            }
        }

        raw.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then(a.rect.y.cmp(&b.rect.y))
                .then(a.rect.x.cmp(&b.rect.x))
        });
        let mut kept: Vec<RawHit> = Vec::new();
        for hit in raw {
            if kept.len() >= self.max_hits {
                break;
            }
            if !kept.iter().any(|other| hit.rect.duplicates(other.rect)) {
                kept.push(hit);
            }
        }

        let mut hits: Vec<VisualHit> = kept
            .into_iter()
            .map(|hit| {
                let viewport_rect = hit.rect.to_viewport(page.scale);
                VisualHit {
                    pixel_rect: hit.rect,
                    viewport_rect,
                    pdf_rect: space.map(|space| space.viewport_rect_to_pdf(viewport_rect)),
                    rotation: quarter_rotation(hit.orientation.turn),
                    mirrored: hit.orientation.mirrored,
                    size: hit.size,
                    score: hit.score.clamp(0.0, 1.0),
                    source: false,
                }
            })
            .collect();

        if contains_source && let Some(source_box) = self.source_box {
            let best = hits
                .iter_mut()
                .map(|hit| (viewport_iou(hit.viewport_rect, source_box), hit))
                .filter(|(iou, _)| *iou >= SOURCE_IOU)
                .max_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((_, hit)) = best {
                hit.source = true;
            } else {
                let scale = f64::from(page.scale);
                hits.insert(
                    0,
                    VisualHit {
                        pixel_rect: PixelRect {
                            x: (source_box.x * scale).round() as usize,
                            y: (source_box.y * scale).round() as usize,
                            width: (source_box.width * scale).round() as usize,
                            height: (source_box.height * scale).round() as usize,
                        },
                        viewport_rect: source_box,
                        pdf_rect: space.map(|space| space.viewport_rect_to_pdf(source_box)),
                        rotation: Rotation::Degrees0,
                        mirrored: false,
                        size: 1.0,
                        score: 1.0,
                        source: true,
                    },
                );
            }
        }
        stats.match_time = started.elapsed();
        Ok(PageMatches { hits, stats })
    }

    /// The lower of `min_score` and [`SCORE_FLOOR`]. The cutoffs do not
    /// rise with `min_score`: a coarse level can score a true hit lower than
    /// full resolution does, and a strict search must give the same hits as
    /// a filter of a loose one.
    fn floor(&self) -> f32 {
        self.min_score.min(SCORE_FLOOR)
    }

    /// Moves a coarse candidate down the pyramid and scores it at full
    /// resolution, also at the neighbouring half-step sizes.
    fn refine(&self, page: &PreparedPage, variant: &Variant, x: usize, y: usize) -> Option<RawHit> {
        let (mut x, mut y) = (x as isize, y as isize);
        let mut score = -1.0;
        for level in (0..variant.coarse_level).rev() {
            (score, x, y) = hill_climb(&page.levels[level], &variant.levels[level], 2 * x, 2 * y);
            if score < self.floor() - REFINE_MARGIN {
                return None;
            }
        }
        if variant.coarse_level == 0 {
            (score, x, y) = hill_climb(&page.levels[0], &variant.levels[0], x, y);
        }
        let main = &variant.levels[0];
        let mut best = (score, x, y, main.width, main.height, variant.size);
        if score >= self.floor() - COARSE_MARGIN {
            for fine in &variant.fine {
                let template = &fine.template;
                let fx = x + (main.width as isize - template.width as isize) / 2;
                let fy = y + (main.height as isize - template.height as isize) / 2;
                let (fine_score, fx, fy) = hill_climb(&page.levels[0], template, fx, fy);
                if fine_score > best.0 {
                    best = (
                        fine_score,
                        fx,
                        fy,
                        template.width,
                        template.height,
                        fine.size,
                    );
                }
            }
        }
        let (score, x, y, width, height, size) = best;
        (score >= self.min_score && x >= 0 && y >= 0).then_some(RawHit {
            rect: PixelRect {
                x: x as usize,
                y: y as usize,
                width,
                height,
            },
            orientation: variant.orientation,
            size,
            score,
        })
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), VisualSearchError> {
    if cancel.load(Ordering::Relaxed) {
        Err(VisualSearchError::Cancelled)
    } else {
        Ok(())
    }
}

fn quarter_rotation(turn: u8) -> Rotation {
    match turn % 4 {
        0 => Rotation::Degrees0,
        1 => Rotation::Degrees90,
        2 => Rotation::Degrees180,
        _ => Rotation::Degrees270,
    }
}

/// ZNCC at every position of a level that passes the ink prefilter. Window
/// sums slide along rows from column sums, so the prefilter is O(1) for each
/// position and needs no integral image.
fn coarse_pass(
    plane: &Plane,
    floats: &FloatPlane,
    template: &LevelTemplate,
    cutoff: f32,
    cancel: &AtomicBool,
    stats: &mut PageStats,
) -> Result<Vec<(f32, usize, usize)>, VisualSearchError> {
    let (width, height) = (template.width, template.height);
    if plane.width < width || plane.height < height {
        return Ok(Vec::new());
    }
    let columns = plane.width - width + 1;
    let rows = plane.height - height + 1;
    let count = (width * height) as f64;
    let ink_min = template.sum * INK_RATIO_MIN;
    let ink_max = template.sum * INK_RATIO_MAX;
    let variance_min = template.norm2 * VARIANCE_RATIO_MIN;
    let template_norm = template.norm2.sqrt();

    let mut column_sum = vec![0u32; plane.width];
    let mut column_squares = vec![0u32; plane.width];
    for y in 0..height {
        for (x, &value) in plane.row(y).iter().enumerate() {
            column_sum[x] += u32::from(value);
            column_squares[x] += u32::from(value) * u32::from(value);
        }
    }
    let mut candidates = Vec::new();
    stats.positions += (columns * rows) as u64;
    for y in 0..rows {
        check_cancel(cancel)?;
        if y > 0 {
            let (leaving, entering) = (plane.row(y - 1), plane.row(y + height - 1));
            for x in 0..plane.width {
                let (out, inn) = (u32::from(leaving[x]), u32::from(entering[x]));
                column_sum[x] = column_sum[x] + inn - out;
                column_squares[x] = column_squares[x] + inn * inn - out * out;
            }
        }
        let mut sum: u64 = column_sum[..width]
            .iter()
            .map(|&value| u64::from(value))
            .sum();
        let mut squares: u64 = column_squares[..width]
            .iter()
            .map(|&value| u64::from(value))
            .sum();
        for x in 0..columns {
            if x > 0 {
                sum = sum + u64::from(column_sum[x + width - 1]) - u64::from(column_sum[x - 1]);
                squares = squares + u64::from(column_squares[x + width - 1])
                    - u64::from(column_squares[x - 1]);
            }
            let ink = sum as f64;
            if ink < ink_min || ink > ink_max {
                continue;
            }
            let variance = squares as f64 - ink * ink / count;
            if variance < variance_min {
                continue;
            }
            stats.correlated += 1;
            let padded = template.padded_width;
            let mut lanes = [0f32; LANES];
            for (row, weights) in template.padded.chunks_exact(padded).enumerate() {
                let start = (y + row) * floats.stride + x;
                let pixels = &floats.data[start..start + padded];
                for (pixels, weights) in pixels.chunks_exact(LANES).zip(weights.chunks_exact(LANES))
                {
                    for lane in 0..LANES {
                        lanes[lane] += pixels[lane] * weights[lane];
                    }
                }
            }
            let correlation: f32 = lanes.iter().sum();
            let score = (f64::from(correlation) / (variance.sqrt() * template_norm)) as f32;
            if score >= cutoff {
                candidates.push((score, x, y));
            }
        }
    }
    Ok(candidates)
}

/// Keeps the best candidate of each neighbourhood (greedy, best first): a
/// candidate closer than `radius` on both axes to a kept one is dropped.
fn suppress_nearby(
    mut candidates: Vec<(f32, usize, usize)>,
    radius: usize,
    width: usize,
    height: usize,
) -> Vec<(f32, usize, usize)> {
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));
    let cells_x = width / radius + 1;
    let cells_y = height / radius + 1;
    let mut grid: Vec<Vec<(usize, usize)>> = vec![Vec::new(); cells_x * cells_y];
    let mut kept = Vec::new();
    for (score, x, y) in candidates {
        if kept.len() >= MAX_CANDIDATES_PER_VARIANT {
            break;
        }
        let (cx, cy) = (x / radius, y / radius);
        let near = (cy.saturating_sub(1)..=(cy + 1).min(cells_y - 1)).any(|gy| {
            (cx.saturating_sub(1)..=(cx + 1).min(cells_x - 1)).any(|gx| {
                grid[gy * cells_x + gx]
                    .iter()
                    .any(|&(kx, ky)| kx.abs_diff(x) < radius && ky.abs_diff(y) < radius)
            })
        });
        if !near {
            grid[cy * cells_x + cx].push((x, y));
            kept.push((score, x, y));
        }
    }
    kept
}

/// Moves to the best of the 8 neighbours until no neighbour is better.
fn hill_climb(plane: &Plane, template: &LevelTemplate, x: isize, y: isize) -> (f32, isize, isize) {
    let (mut x, mut y) = (x, y);
    let mut best = score_at(plane, template, x, y);
    for _ in 0..HILL_CLIMB_STEPS {
        let mut next = (best, x, y);
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let score = score_at(plane, template, x + dx, y + dy);
                if score > next.0 {
                    next = (score, x + dx, y + dy);
                }
            }
        }
        if next.0 > best {
            (best, x, y) = next;
        } else {
            break;
        }
    }
    (best, x, y)
}

/// ZNCC of the template at one position, with the window statistics read
/// directly. Positions outside the page score -1.
fn score_at(plane: &Plane, template: &LevelTemplate, x: isize, y: isize) -> f32 {
    if x < 0
        || y < 0
        || x as usize + template.width > plane.width
        || y as usize + template.height > plane.height
    {
        return -1.0;
    }
    let (x, y) = (x as usize, y as usize);
    let mut sum = 0u64;
    let mut squares = 0u64;
    let mut lanes = [0f32; LANES];
    for row in 0..template.height {
        let start = (y + row) * plane.width + x;
        let window = &plane.data[start..start + template.width];
        let values = &template.values[row * template.width..(row + 1) * template.width];
        // A row is at most 1024 px, so its sums fit in u32.
        let (mut row_sum, mut row_squares) = (0u32, 0u32);
        let pixels = window.chunks_exact(LANES);
        let weights = values.chunks_exact(LANES);
        for (&pixel, &value) in pixels.remainder().iter().zip(weights.remainder()) {
            row_sum += u32::from(pixel);
            row_squares += u32::from(pixel) * u32::from(pixel);
            lanes[0] += f32::from(pixel) * value;
        }
        for (pixels, weights) in pixels.zip(weights) {
            for lane in 0..LANES {
                let pixel = pixels[lane];
                row_sum += u32::from(pixel);
                row_squares += u32::from(pixel) * u32::from(pixel);
                lanes[lane] += f32::from(pixel) * weights[lane];
            }
        }
        sum += u64::from(row_sum);
        squares += u64::from(row_squares);
    }
    let correlation: f32 = lanes.iter().sum();
    let count = (template.width * template.height) as f64;
    let variance = squares as f64 - (sum as f64).powi(2) / count;
    if variance <= 1e-6 {
        return -1.0;
    }
    (f64::from(correlation) / (variance.sqrt() * template.norm2.sqrt())) as f32
}

/// A coarse level as floats, each row padded with `LANES` zeros, so a padded
/// template row never reads past its page row.
#[derive(Clone, Debug)]
struct FloatPlane {
    stride: usize,
    data: Vec<f32>,
}

impl FloatPlane {
    fn new(plane: &Plane) -> Self {
        let stride = plane.width + LANES;
        let mut data = vec![0f32; stride * plane.height];
        for (target, source) in data
            .chunks_exact_mut(stride)
            .zip(plane.data.chunks_exact(plane.width))
        {
            for (out, &value) in target.iter_mut().zip(source) {
                *out = f32::from(value);
            }
        }
        Self { stride, data }
    }
}

/// A 3×3 box blur with edge pixels repeated. It tolerates the half-pixel
/// shifts of anti-aliasing.
fn blur3(plane: &Plane) -> Plane {
    let (width, height) = (plane.width, plane.height);
    let horizontal = |y: usize, out: &mut [u16]| {
        let row = plane.row(y);
        for x in 0..width {
            let left = row[x.saturating_sub(1)];
            let right = row[(x + 1).min(width - 1)];
            out[x] = u16::from(left) + u16::from(row[x]) + u16::from(right);
        }
    };
    let mut above = vec![0u16; width];
    let mut middle = vec![0u16; width];
    let mut below = vec![0u16; width];
    horizontal(0, &mut middle);
    above.copy_from_slice(&middle);
    let mut data = vec![0u8; width * height];
    for y in 0..height {
        horizontal((y + 1).min(height - 1), &mut below);
        for (x, out) in data[y * width..(y + 1) * width].iter_mut().enumerate() {
            *out = ((above[x] + middle[x] + below[x] + 4) / 9) as u8;
        }
        std::mem::swap(&mut above, &mut middle);
        std::mem::swap(&mut middle, &mut below);
    }
    Plane {
        width,
        height,
        data,
    }
}

/// Averages of 2×2 blocks. An odd last row or column is dropped.
fn halve(plane: &Plane) -> Plane {
    let width = (plane.width / 2).max(1);
    let height = (plane.height / 2).max(1);
    let mut data = Vec::with_capacity(width * height);
    for y in 0..height {
        let top = plane.row((2 * y).min(plane.height - 1));
        let bottom = plane.row((2 * y + 1).min(plane.height - 1));
        for x in 0..width {
            let (left, right) = (2 * x, (2 * x + 1).min(plane.width - 1));
            let total = u16::from(top[left])
                + u16::from(top[right])
                + u16::from(bottom[left])
                + u16::from(bottom[right]);
            data.push(((total + 2) / 4) as u8);
        }
    }
    Plane {
        width,
        height,
        data,
    }
}

/// Turns a plane clockwise by `turn` quarter turns.
fn rotate(plane: &Plane, turn: u8) -> Plane {
    let (width, height) = (plane.width, plane.height);
    let at = |x: usize, y: usize| plane.data[y * width + x];
    match turn % 4 {
        0 => plane.clone(),
        1 => Plane {
            width: height,
            height: width,
            data: (0..width)
                .flat_map(|y| (0..height).map(move |x| (x, y)))
                .map(|(x, y)| at(y, height - 1 - x))
                .collect(),
        },
        2 => Plane {
            width,
            height,
            data: plane.data.iter().rev().copied().collect(),
        },
        _ => Plane {
            width: height,
            height: width,
            data: (0..width)
                .flat_map(|y| (0..height).map(move |x| (x, y)))
                .map(|(x, y)| at(width - 1 - y, x))
                .collect(),
        },
    }
}

/// Mirrors a plane left to right when the orientation asks, then turns it.
fn orient(plane: &Plane, orientation: Orientation) -> Plane {
    if !orientation.mirrored {
        return rotate(plane, orientation.turn);
    }
    let mirrored = Plane {
        width: plane.width,
        height: plane.height,
        data: plane
            .data
            .chunks_exact(plane.width)
            .flat_map(|row| row.iter().rev().copied())
            .collect(),
    };
    rotate(&mirrored, orientation.turn)
}

/// Resamples by `factor`: area average to reduce, bilinear to enlarge.
fn resample(plane: &Plane, factor: f32) -> Plane {
    let new_width = ((plane.width as f32 * factor).round() as usize).max(1);
    let new_height = ((plane.height as f32 * factor).round() as usize).max(1);
    if new_width == plane.width && new_height == plane.height {
        return plane.clone();
    }
    let mut rows = vec![0f32; new_width * plane.height];
    let mut line = vec![0f32; plane.width.max(plane.height)];
    for y in 0..plane.height {
        for (target, &value) in line.iter_mut().zip(plane.row(y)) {
            *target = f32::from(value);
        }
        resample_line(
            &line[..plane.width],
            &mut rows[y * new_width..(y + 1) * new_width],
        );
    }
    let mut data = vec![0u8; new_width * new_height];
    let mut column_out = vec![0f32; new_height];
    for x in 0..new_width {
        for y in 0..plane.height {
            line[y] = rows[y * new_width + x];
        }
        resample_line(&line[..plane.height], &mut column_out);
        for (y, &value) in column_out.iter().enumerate() {
            data[y * new_width + x] = value.round().clamp(0.0, 255.0) as u8;
        }
    }
    Plane {
        width: new_width,
        height: new_height,
        data,
    }
}

fn resample_line(source: &[f32], target: &mut [f32]) {
    let ratio = target.len() as f32 / source.len() as f32;
    let last = source.len() - 1;
    if ratio >= 1.0 {
        for (index, out) in target.iter_mut().enumerate() {
            let position = ((index as f32 + 0.5) / ratio - 0.5).clamp(0.0, last as f32);
            let low = position.floor() as usize;
            let high = (low + 1).min(last);
            let weight = position - low as f32;
            *out = source[low] * (1.0 - weight) + source[high] * weight;
        }
    } else {
        for (index, out) in target.iter_mut().enumerate() {
            let start = index as f32 / ratio;
            let end = ((index + 1) as f32 / ratio).min(source.len() as f32);
            let mut total = 0.0;
            let mut cell = start.floor() as usize;
            while (cell as f32) < end && cell <= last {
                let overlap = (end.min(cell as f32 + 1.0) - start.max(cell as f32)).max(0.0);
                total += source[cell] * overlap;
                cell += 1;
            }
            *out = total / (end - start).max(f32::EPSILON);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane(width: usize, height: usize, data: Vec<u8>) -> Plane {
        Plane {
            width,
            height,
            data,
        }
    }

    #[test]
    fn quarter_turns_are_clockwise_and_compose() {
        // 1 2 3
        // 4 5 6
        let source = plane(3, 2, vec![1, 2, 3, 4, 5, 6]);
        let turned = rotate(&source, 1);
        assert_eq!((turned.width, turned.height), (2, 3));
        assert_eq!(turned.data, vec![4, 1, 5, 2, 6, 3]);
        assert_eq!(rotate(&source, 3).data, vec![3, 6, 2, 5, 1, 4]);
        assert_eq!(rotate(&rotate(&source, 1), 1).data, rotate(&source, 2).data);
        assert_eq!(rotate(&rotate(&source, 2), 1).data, rotate(&source, 3).data);
    }

    #[test]
    fn a_mirror_flips_left_to_right_before_the_turn() {
        // 1 2 3
        // 4 5 6
        let source = plane(3, 2, vec![1, 2, 3, 4, 5, 6]);
        let mirrored = |turn| {
            orient(
                &source,
                Orientation {
                    turn,
                    mirrored: true,
                },
            )
        };
        assert_eq!(mirrored(0).data, vec![3, 2, 1, 6, 5, 4]);
        // Mirrored then turned 180° is the picture flipped top to bottom.
        assert_eq!(mirrored(2).data, vec![4, 5, 6, 1, 2, 3]);
        let turned = mirrored(1);
        assert_eq!((turned.width, turned.height), (2, 3));
        assert_eq!(turned.data, rotate(&mirrored(0), 1).data);
    }

    #[test]
    fn resampling_keeps_the_mean_and_the_size() {
        let source = plane(10, 4, (0..40).map(|value| (value * 6) as u8).collect());
        let mean = |plane: &Plane| {
            plane.data.iter().map(|&v| f64::from(v)).sum::<f64>() / plane.data.len() as f64
        };
        let smaller = resample(&source, 0.5);
        assert_eq!((smaller.width, smaller.height), (5, 2));
        assert!((mean(&smaller) - mean(&source)).abs() < 1.0);
        let larger = resample(&source, 1.5);
        assert_eq!((larger.width, larger.height), (15, 6));
        assert!((mean(&larger) - mean(&source)).abs() < 2.0);
    }

    #[test]
    fn blur_and_halve_keep_flat_areas_flat() {
        let flat = plane(7, 5, vec![90; 35]);
        assert!(blur3(&flat).data.iter().all(|&value| value == 90));
        let half = halve(&flat);
        assert_eq!((half.width, half.height), (3, 2));
        assert!(half.data.iter().all(|&value| value == 90));
    }

    #[test]
    fn size_ladders_follow_the_design() {
        assert_eq!(SizeRange::PAGE_BOX.steps().len(), 3);
        let steps = SizeRange::PAGE_BOX.steps();
        assert!(
            (steps[0] - 0.85).abs() < 1e-4 && steps[1] == 1.0 && (steps[2] - 1.18).abs() < 1e-3
        );
        let pasted = SizeRange::PASTED_PICTURE.steps();
        assert!((pasted[0] - 0.5).abs() < 1e-4 && (pasted.last().unwrap() - 2.0).abs() < 1e-3);
        assert!(
            pasted
                .windows(2)
                .all(|pair| pair[1] / pair[0] <= SIZE_STEP + 1e-3)
        );
    }

    #[test]
    fn working_scale_aims_for_a_32_pixel_template() {
        // A 10 mm symbol (28.35 pt) on A1 (2384 × 1684 pt).
        assert_eq!(working_scale(28.35, (2384.0, 1684.0)), 1.0);
        // A 4 mm symbol wants 2.82, clamps to 2 and drops for 16 Mpx on A0.
        assert_eq!(working_scale(11.3, (3370.0, 2384.0)), 1.41);
        assert_eq!(working_scale(200.0, (595.0, 842.0)), 0.5);
    }

    #[test]
    fn suppression_keeps_one_candidate_in_each_neighbourhood() {
        let kept = suppress_nearby(
            vec![(0.9, 10, 10), (0.8, 11, 10), (0.7, 30, 10), (0.95, 12, 11)],
            4,
            64,
            64,
        );
        assert_eq!(kept, vec![(0.95, 12, 11), (0.7, 30, 10)]);
    }
}
