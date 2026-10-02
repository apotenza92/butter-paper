use crate::{
    annotation_model::{InkTool, PenAnnotation},
    page_geometry::{PageCoordinateSpace, PdfPoint, PdfRect, Rotation},
};

/// Maps an output raster back to one unrotated PDF page. `crop_x_px` and
/// `crop_y_px` are offsets in the fully scaled page raster, before the output
/// crop is applied.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HighlightRasterMapping {
    pub page_index: u32,
    coordinate_space: PageCoordinateSpace,
    pub scale_x: f64,
    pub scale_y: f64,
    pub crop_x_px: f64,
    pub crop_y_px: f64,
}

impl HighlightRasterMapping {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        page_index: u32,
        page_width_pt: f64,
        page_height_pt: f64,
        scale_x: f64,
        scale_y: f64,
        crop_x_px: f64,
        crop_y_px: f64,
    ) -> Result<Self, String> {
        let coordinate_space = PageCoordinateSpace::new(
            PdfRect::new(0., 0., page_width_pt, page_height_pt)
                .map_err(|error| error.to_string())?,
            PdfRect::new(0., 0., page_width_pt, page_height_pt)
                .map_err(|error| error.to_string())?,
            Rotation::Degrees0,
            1.,
        )
        .map_err(|error| error.to_string())?;
        Self::from_coordinate_space(
            page_index,
            coordinate_space,
            scale_x,
            scale_y,
            crop_x_px,
            crop_y_px,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_coordinate_space(
        page_index: u32,
        coordinate_space: PageCoordinateSpace,
        scale_x: f64,
        scale_y: f64,
        crop_x_px: f64,
        crop_y_px: f64,
    ) -> Result<Self, String> {
        let mapping = Self {
            page_index,
            coordinate_space,
            scale_x,
            scale_y,
            crop_x_px,
            crop_y_px,
        };
        mapping.validate()?;
        Ok(mapping)
    }

    fn validate(self) -> Result<(), String> {
        let (page_width, page_height) = self.coordinate_space.display_size_points();
        if !page_width.is_finite()
            || !page_height.is_finite()
            || page_width <= 0.
            || page_height <= 0.
        {
            return Err("highlight compositor page dimensions must be finite and positive".into());
        }
        if !self.scale_x.is_finite()
            || !self.scale_y.is_finite()
            || self.scale_x <= 0.
            || self.scale_y <= 0.
        {
            return Err("highlight compositor raster scales must be finite and positive".into());
        }
        if !self.crop_x_px.is_finite()
            || !self.crop_y_px.is_finite()
            || self.crop_x_px < 0.
            || self.crop_y_px < 0.
        {
            return Err("highlight compositor crop offsets must be finite and nonnegative".into());
        }
        Ok(())
    }
}

/// Precomposes application-owned Highlight paths into a PDF page raster with
/// the same source-over Multiply equation used by PDF `/BM /Multiply`.
///
/// The input uses RGBA byte order and an
/// unrotated PDF page coordinate system. Pen paths are intentionally ignored. The compositor is
/// a no-fork compatibility seam for renderers whose public path API cannot set
/// a blend mode; it does not change the retained annotation model.
pub fn precompose_highlights_multiply_rgba(
    pixels_rgba: &mut [u8],
    width: u32,
    height: u32,
    page_width_pt: f64,
    page_height_pt: f64,
    annotations: &[PenAnnotation],
) -> Result<usize, String> {
    let mapping = HighlightRasterMapping::new(
        annotations
            .first()
            .map_or(0, |annotation| annotation.page_index),
        page_width_pt,
        page_height_pt,
        f64::from(width) / page_width_pt,
        f64::from(height) / page_height_pt,
        0.,
        0.,
    )?;
    precompose_highlights_multiply_rgba_mapped(pixels_rgba, width, height, mapping, annotations)
}

/// Crop-aware compositor for callers with RGBA pixels.
pub fn precompose_highlights_multiply_rgba_mapped(
    pixels_rgba: &mut [u8],
    width: u32,
    height: u32,
    mapping: HighlightRasterMapping,
    annotations: &[PenAnnotation],
) -> Result<usize, String> {
    precompose_highlights_multiply_mapped(pixels_rgba, width, height, mapping, annotations, false)
}

/// Crop-aware compositor for native PDFium and GPUI BGRA surfaces.
pub fn precompose_highlights_multiply_bgra_mapped(
    pixels_bgra: &mut [u8],
    width: u32,
    height: u32,
    mapping: HighlightRasterMapping,
    annotations: &[PenAnnotation],
) -> Result<usize, String> {
    precompose_highlights_multiply_mapped(pixels_bgra, width, height, mapping, annotations, true)
}

fn precompose_highlights_multiply_mapped(
    pixels_rgba: &mut [u8],
    width: u32,
    height: u32,
    mapping: HighlightRasterMapping,
    annotations: &[PenAnnotation],
    bgra: bool,
) -> Result<usize, String> {
    if width == 0 || height == 0 {
        return Err("highlight compositor raster dimensions must be non-zero".into());
    }
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "highlight compositor raster dimensions overflow".to_owned())?;
    if pixels_rgba.len() as u64 != expected {
        return Err("highlight compositor pixel byte length does not match its dimensions".into());
    }
    mapping.validate()?;

    let scale_x = mapping.scale_x;
    let scale_y = mapping.scale_y;
    let pixel_count = width as usize * height as usize;
    let mut changed = vec![false; pixel_count];

    for annotation in annotations.iter().filter(|annotation| {
        annotation.tool() == InkTool::Highlight && annotation.page_index == mapping.page_index
    }) {
        let [red, green, blue] = parse_rgb(annotation.appearance.color())?;
        let source_rgb = if bgra {
            [blue, green, red]
        } else {
            [red, green, blue]
        };
        let radius = annotation.appearance.width_pt()
            * mapping.coordinate_space.user_unit()
            * (scale_x + scale_y)
            / 4.;
        let mut coverage = vec![0_f64; pixel_count];

        // Electron emits each retained path as a separate SVG primitive. A path
        // unions its own stroke coverage, then blends over earlier paths.
        for path in annotation.paths() {
            coverage.fill(0.);
            for segment in path.windows(2) {
                let start = mapping
                    .coordinate_space
                    .pdf_to_viewport(PdfPoint::new(segment[0].x, segment[0].y));
                let end = mapping
                    .coordinate_space
                    .pdf_to_viewport(PdfPoint::new(segment[1].x, segment[1].y));
                let start = (
                    start.x * scale_x - mapping.crop_x_px,
                    start.y * scale_y - mapping.crop_y_px,
                );
                let end = (
                    end.x * scale_x - mapping.crop_x_px,
                    end.y * scale_y - mapping.crop_y_px,
                );
                let feathered_radius = radius + 0.5;
                let min_x = ((start.0.min(end.0) - feathered_radius).floor() as i64)
                    .clamp(0, i64::from(width) - 1) as u32;
                let max_x = ((start.0.max(end.0) + feathered_radius).ceil() as i64)
                    .clamp(0, i64::from(width) - 1) as u32;
                let min_y = ((start.1.min(end.1) - feathered_radius).floor() as i64)
                    .clamp(0, i64::from(height) - 1) as u32;
                let max_y = ((start.1.max(end.1) + feathered_radius).ceil() as i64)
                    .clamp(0, i64::from(height) - 1) as u32;

                for y in min_y..=max_y {
                    for x in min_x..=max_x {
                        let center = (f64::from(x) + 0.5, f64::from(y) + 0.5);
                        let distance = distance_to_segment(center, start, end);
                        let sample_coverage = (radius + 0.5 - distance).clamp(0., 1.);
                        let index = (y * width + x) as usize;
                        coverage[index] = coverage[index].max(sample_coverage);
                    }
                }
            }

            for (index, coverage) in coverage.iter().copied().enumerate() {
                let alpha = coverage * annotation.appearance.opacity();
                if alpha <= 0. {
                    continue;
                }
                let start = index * 4;
                for channel in 0..3 {
                    pixels_rgba[start + channel] = multiply_source_over(
                        pixels_rgba[start + channel],
                        source_rgb[channel],
                        alpha,
                    );
                }
                changed[index] = true;
            }
        }
    }

    Ok(changed.into_iter().filter(|changed| *changed).count())
}

fn distance_to_segment(point: (f64, f64), start: (f64, f64), end: (f64, f64)) -> f64 {
    let delta = (end.0 - start.0, end.1 - start.1);
    let length_squared = delta.0 * delta.0 + delta.1 * delta.1;
    if length_squared == 0. {
        return ((point.0 - start.0).powi(2) + (point.1 - start.1).powi(2)).sqrt();
    }
    let projection = (((point.0 - start.0) * delta.0 + (point.1 - start.1) * delta.1)
        / length_squared)
        .clamp(0., 1.);
    let nearest = (
        start.0 + projection * delta.0,
        start.1 + projection * delta.1,
    );
    ((point.0 - nearest.0).powi(2) + (point.1 - nearest.1).powi(2)).sqrt()
}

fn multiply_source_over(backdrop: u8, source: u8, alpha: f64) -> u8 {
    let multiplied = f64::from(backdrop) * f64::from(source) / 255.;
    (f64::from(backdrop) * (1. - alpha) + multiplied * alpha)
        .round()
        .clamp(0., 255.) as u8
}

fn parse_rgb(color: &str) -> Result<[u8; 3], String> {
    if color.len() != 7 || !color.starts_with('#') {
        return Err("highlight compositor requires a canonical #rrggbb color".into());
    }
    let byte = |range| {
        u8::from_str_radix(&color[range], 16)
            .map_err(|_| "highlight compositor color is not hexadecimal".to_owned())
    };
    Ok([byte(1..3)?, byte(3..5)?, byte(5..7)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_model::{MarkupId, PdfPoint as ModelPoint, PenAppearance};

    #[test]
    fn highlight_paths_multiply_separately_but_self_overlap_uses_one_coverage() {
        let a = ModelPoint::new(2., 10.).unwrap();
        let b = ModelPoint::new(18., 10.).unwrap();
        let mapping = HighlightRasterMapping::new(0, 20., 20., 1., 1., 0., 0.).unwrap();
        for (paths, expected) in [
            (vec![vec![a, b, a, b]], [20, 40, 100, 128]),
            (vec![vec![a, b], vec![a, b]], [10, 20, 100, 128]),
        ] {
            let annotation = PenAnnotation::new_highlight_paths(
                MarkupId::new("paths").unwrap(),
                0,
                paths,
                PenAppearance::new("#ff0000", 8., 0.5).unwrap(),
            )
            .unwrap();
            let mut bgra = [40, 80, 100, 128].repeat(400);
            precompose_highlights_multiply_bgra_mapped(
                &mut bgra,
                20,
                20,
                mapping,
                std::slice::from_ref(&annotation),
            )
            .unwrap();
            let offset = (10 * 20 + 10) * 4;
            assert_eq!(&bgra[offset..offset + 4], &expected);
            let mut rgba = [100, 80, 40, 128].repeat(400);
            precompose_highlights_multiply_rgba_mapped(&mut rgba, 20, 20, mapping, &[annotation])
                .unwrap();
            assert_eq!(
                &rgba[offset..offset + 4],
                &[expected[2], expected[1], expected[0], expected[3]]
            );
        }
    }
}
