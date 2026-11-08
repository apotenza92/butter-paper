//! GPUI-free, bounded PNG/JPEG decoding for annotation assets.

use std::{
    error::Error,
    fmt, fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use image::{
    DynamicImage, ImageDecoder as _, ImageFormat, ImageReader,
    imageops::{FilterType, resize},
};
use sha2::{Digest as _, Sha256};

use crate::annotation_model::{
    AnnotationError, DecodedRgbaAsset, MAX_DECODED_IMAGE_BYTES, MAX_IMAGE_DIMENSION_PX,
};

pub const MAX_ENCODED_IMAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SIGNATURE_SOURCE_BYTES: usize = 10 * 1024 * 1024;
const MAX_SIGNATURE_DIMENSION: u32 = 4_096;
const MAX_SIGNATURE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_SIGNATURE_ASPECT_RATIO: f64 = 25.;
const MAX_SIGNATURE_OUTPUT_DIMENSION: u32 = 2_048;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DecodedImageFormat {
    Png,
    Jpeg,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedImageFile {
    asset: DecodedRgbaAsset,
    format: DecodedImageFormat,
    encoded_bytes: usize,
    encoded_sha256: String,
}

impl DecodedImageFile {
    pub fn asset(&self) -> &DecodedRgbaAsset {
        &self.asset
    }
    pub fn into_asset(self) -> DecodedRgbaAsset {
        self.asset
    }
    pub const fn format(&self) -> DecodedImageFormat {
        self.format
    }
    pub const fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }
    pub fn encoded_sha256(&self) -> &str {
        &self.encoded_sha256
    }
}

#[derive(Debug)]
pub enum ImageDecodeError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    EncodedFileTooLarge {
        actual_bytes: usize,
        maximum_bytes: usize,
    },
    UnsupportedFormat,
    Decode(image::ImageError),
    InvalidGeometry(AnnotationError),
}

impl fmt::Display for ImageDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(
                formatter,
                "could not read image {}: {source}",
                path.display()
            ),
            Self::EncodedFileTooLarge {
                actual_bytes,
                maximum_bytes,
            } => write!(
                formatter,
                "encoded image is {actual_bytes} bytes; the limit is {maximum_bytes} bytes"
            ),
            Self::UnsupportedFormat => write!(formatter, "image must be a PNG or JPEG file"),
            Self::Decode(source) => write!(formatter, "could not decode image: {source}"),
            Self::InvalidGeometry(source) => source.fmt(formatter),
        }
    }
}

impl Error for ImageDecodeError {}

/// A canonical signature raster. The source path and encoded bytes are not retained.
///
/// Decoding currently runs in the application's background executor. This is a
/// development-only boundary, not hostile-input process isolation equivalent to
/// the Electron sanitizer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SanitizedSignatureFile {
    asset: DecodedRgbaAsset,
}

impl SanitizedSignatureFile {
    pub fn asset(&self) -> &DecodedRgbaAsset {
        &self.asset
    }

    pub fn into_asset(self) -> DecodedRgbaAsset {
        self.asset
    }
}

#[derive(Debug)]
pub enum SignatureImageError {
    Read { path: PathBuf, source: io::Error },
    SourceTooLarge,
    UnsupportedFormat,
    UnsafeDimensions,
    ExtremeAspectRatio,
    Decode(image::ImageError),
    InvalidGeometry(AnnotationError),
    NoInk,
    ExcessiveCoverage,
}

impl fmt::Display for SignatureImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "Unable to read image {}: {source}",
                    path.display()
                )
            }
            Self::SourceTooLarge => write!(formatter, "Choose an image smaller than 10 MB."),
            Self::UnsupportedFormat => write!(formatter, "Choose a PNG or JPEG image."),
            Self::UnsafeDimensions => {
                write!(
                    formatter,
                    "Choose an image no larger than 4096 × 4096 pixels."
                )
            }
            Self::ExtremeAspectRatio => {
                write!(
                    formatter,
                    "Choose an image with a less extreme aspect ratio."
                )
            }
            Self::Decode(_) => write!(formatter, "Unable to process this image."),
            Self::InvalidGeometry(source) => source.fmt(formatter),
            Self::NoInk => write!(
                formatter,
                "No signature was found. Use dark ink on white paper."
            ),
            Self::ExcessiveCoverage => write!(
                formatter,
                "Too much background was detected. Fill the view with white paper."
            ),
        }
    }
}

impl Error for SignatureImageError {}

pub fn sanitize_signature_path(
    path: impl AsRef<Path>,
) -> Result<SanitizedSignatureFile, SignatureImageError> {
    let path = path.as_ref();
    let mut file = fs::File::open(path).map_err(|source| SignatureImageError::Read {
        path: path.to_owned(),
        source,
    })?;
    let mut encoded = Vec::with_capacity(MAX_SIGNATURE_SOURCE_BYTES.min(1024 * 1024));
    file.by_ref()
        .take(MAX_SIGNATURE_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut encoded)
        .map_err(|source| SignatureImageError::Read {
            path: path.to_owned(),
            source,
        })?;
    sanitize_signature_bytes(&encoded)
}

pub fn sanitize_signature_bytes(
    encoded: &[u8],
) -> Result<SanitizedSignatureFile, SignatureImageError> {
    if encoded.len() > MAX_SIGNATURE_SOURCE_BYTES {
        return Err(SignatureImageError::SourceTooLarge);
    }
    let image_format = match image::guess_format(encoded).ok() {
        Some(ImageFormat::Png) => ImageFormat::Png,
        Some(ImageFormat::Jpeg) => ImageFormat::Jpeg,
        _ => return Err(SignatureImageError::UnsupportedFormat),
    };
    let mut decoder = ImageReader::with_format(std::io::Cursor::new(encoded), image_format)
        .into_decoder()
        .map_err(SignatureImageError::Decode)?;
    validate_signature_geometry(decoder.dimensions())?;
    let orientation = decoder.orientation().map_err(SignatureImageError::Decode)?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(SignatureImageError::Decode)?;
    decoded.apply_orientation(orientation);
    let mut rgba = decoded.into_rgba8();
    validate_signature_geometry(rgba.dimensions())?;

    let (width, height) = rgba.dimensions();
    let longest = width.max(height);
    if longest > MAX_SIGNATURE_OUTPUT_DIMENSION {
        let scale = f64::from(MAX_SIGNATURE_OUTPUT_DIMENSION) / f64::from(longest);
        let output_width = (f64::from(width) * scale).round().max(1.) as u32;
        let output_height = (f64::from(height) * scale).round().max(1.) as u32;
        rgba = resize(&rgba, output_width, output_height, FilterType::Lanczos3);
    }
    sanitize_signature_rgba(rgba)
}

/// A signature in a picture of part of a page (RGBA), such as the box the
/// user drew round one with From the page.
pub fn sanitize_signature_rgba_pixels(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<SanitizedSignatureFile, SignatureImageError> {
    validate_signature_geometry((width, height))?;
    let rgba = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or(SignatureImageError::UnsafeDimensions)?;
    sanitize_signature_rgba(rgba)
}

/// A signature drawn on a PDF page (a Bluebeam stamp, or a page signed in
/// another app): the page's BGRA pixels as the worker drew them on white.
pub fn sanitize_signature_raster(
    width: u32,
    height: u32,
    pixels_bgra: &[u8],
) -> Result<SanitizedSignatureFile, SignatureImageError> {
    validate_signature_geometry((width, height))?;
    let mut rgba = Vec::with_capacity(pixels_bgra.len());
    for pixel in pixels_bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    let rgba = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or(SignatureImageError::UnsafeDimensions)?;
    sanitize_signature_rgba(rgba)
}

fn validate_signature_geometry((width, height): (u32, u32)) -> Result<(), SignatureImageError> {
    let pixels = u64::from(width).checked_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_SIGNATURE_DIMENSION
        || height > MAX_SIGNATURE_DIMENSION
        || pixels.is_none_or(|pixels| pixels > MAX_SIGNATURE_PIXELS)
    {
        return Err(SignatureImageError::UnsafeDimensions);
    }
    let aspect = (f64::from(width) / f64::from(height)).max(f64::from(height) / f64::from(width));
    if aspect > MAX_SIGNATURE_ASPECT_RATIO {
        return Err(SignatureImageError::ExtremeAspectRatio);
    }
    Ok(())
}

fn sanitize_signature_rgba(
    rgba: image::RgbaImage,
) -> Result<SanitizedSignatureFile, SignatureImageError> {
    let (width, height) = rgba.dimensions();
    let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
        .map_err(|_| SignatureImageError::UnsafeDimensions)?;
    // Transparent drawings already carry edge coverage; photo cleanup would
    // threshold and blur those anti-aliased pixels.
    let transparent = rgba.pixels().any(|pixel| pixel[3] < 255);
    let mut strength = Vec::with_capacity(pixel_count);
    for pixel in rgba.pixels() {
        let luminance = f32::from(pixel[0]) * 0.2126
            + f32::from(pixel[1]) * 0.7152
            + f32::from(pixel[2]) * 0.0722;
        strength.push(((255. - luminance) / 255.) * (f32::from(pixel[3]) / 255.));
    }

    let mut alpha = vec![0_u8; pixel_count];
    let mut left = width;
    let mut top = height;
    let mut right = 0;
    let mut bottom = 0;
    let mut ink_count = 0_u64;
    for y in 0..height {
        for x in 0..width {
            let ix = (y * width + x) as usize;
            let mut weighted = strength[ix] * 4.;
            let mut weight = 4.;
            if x > 0 {
                weighted += strength[ix - 1];
                weight += 1.;
            }
            if x + 1 < width {
                weighted += strength[ix + 1];
                weight += 1.;
            }
            if y > 0 {
                weighted += strength[ix - width as usize];
                weight += 1.;
            }
            if y + 1 < height {
                weighted += strength[ix + width as usize];
                weight += 1.;
            }
            let scaled = ((weighted / weight - 0.1) / 0.3).clamp(0., 1.);
            let value = if transparent {
                (255. * strength[ix]).round() as u8
            } else {
                (255. * scaled * scaled * (3. - 2. * scaled)).round() as u8
            };
            alpha[ix] = value;
            if value >= 16 {
                ink_count += 1;
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    if ink_count == 0 {
        return Err(SignatureImageError::NoInk);
    }
    if ink_count as f64 / pixel_count as f64 > 0.35 {
        return Err(SignatureImageError::ExcessiveCoverage);
    }

    let ink_extent = (right - left + 1).max(bottom - top + 1);
    let padding = 4_u32.max((f64::from(ink_extent) * 0.04).ceil() as u32);
    left = left.saturating_sub(padding);
    top = top.saturating_sub(padding);
    right = right.saturating_add(padding).min(width - 1);
    bottom = bottom.saturating_add(padding).min(height - 1);
    let output_width = right - left + 1;
    let output_height = bottom - top + 1;
    // The ink keeps its colour, as Acrobat and Revu keep an imported
    // signature's: the average of its solid strokes, so edges blended with
    // the paper do not wash it out.
    let ink = {
        let mut sums = [0_u64; 3];
        let mut solid = 0_u64;
        for (ix, pixel) in rgba.pixels().enumerate() {
            if alpha[ix] >= 200 && strength[ix] >= 0.5 {
                for (sum, channel) in sums.iter_mut().zip(pixel.0) {
                    *sum += u64::from(channel);
                }
                solid += 1;
            }
        }
        if solid == 0 {
            [17, 24, 39]
        } else {
            sums.map(|sum| (sum / solid) as u8)
        }
    };
    let mut output = Vec::with_capacity((output_width * output_height * 4) as usize);
    for y in top..=bottom {
        for x in left..=right {
            output.extend_from_slice(&[ink[0], ink[1], ink[2], alpha[(y * width + x) as usize]]);
        }
    }
    let asset = DecodedRgbaAsset::new(output_width, output_height, output)
        .map_err(SignatureImageError::InvalidGeometry)?;
    Ok(SanitizedSignatureFile { asset })
}

pub fn decode_image_path(path: impl AsRef<Path>) -> Result<DecodedImageFile, ImageDecodeError> {
    decode_image_path_with_limit(path.as_ref(), MAX_ENCODED_IMAGE_BYTES)
}

pub fn decode_image_path_with_limit(
    path: &Path,
    encoded_limit: usize,
) -> Result<DecodedImageFile, ImageDecodeError> {
    let mut file = fs::File::open(path).map_err(|source| ImageDecodeError::Read {
        path: path.to_owned(),
        source,
    })?;
    let mut encoded = Vec::with_capacity(encoded_limit.min(1024 * 1024));
    file.by_ref()
        .take(
            u64::try_from(encoded_limit)
                .unwrap_or(u64::MAX - 1)
                .saturating_add(1),
        )
        .read_to_end(&mut encoded)
        .map_err(|source| ImageDecodeError::Read {
            path: path.to_owned(),
            source,
        })?;
    if encoded.len() > encoded_limit {
        return Err(ImageDecodeError::EncodedFileTooLarge {
            actual_bytes: encoded.len(),
            maximum_bytes: encoded_limit,
        });
    }
    decode_image_bytes(&encoded)
}

/// A picture another app put on the clipboard, as the PNG or JPEG Butter
/// Paper places: a TIFF (as macOS apps copy pictures), a bitmap (as Windows
/// copies them), GIF or WebP is converted to PNG.
pub fn clipboard_picture_png_or_jpeg(
    encoded: &[u8],
) -> Result<std::borrow::Cow<'_, [u8]>, ImageDecodeError> {
    let format = match image::guess_format(encoded).ok() {
        Some(ImageFormat::Png | ImageFormat::Jpeg) => {
            return Ok(std::borrow::Cow::Borrowed(encoded));
        }
        Some(
            format @ (ImageFormat::Tiff | ImageFormat::Bmp | ImageFormat::Gif | ImageFormat::WebP),
        ) => format,
        _ => return Err(ImageDecodeError::UnsupportedFormat),
    };
    let mut decoder = ImageReader::with_format(std::io::Cursor::new(encoded), format)
        .into_decoder()
        .map_err(ImageDecodeError::Decode)?;
    validate_geometry(decoder.dimensions())?;
    let orientation = decoder.orientation().map_err(ImageDecodeError::Decode)?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(ImageDecodeError::Decode)?;
    decoded.apply_orientation(orientation);
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(decoded.into_rgba8())
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .map_err(ImageDecodeError::Decode)?;
    Ok(std::borrow::Cow::Owned(png))
}

pub fn decode_image_bytes(encoded: &[u8]) -> Result<DecodedImageFile, ImageDecodeError> {
    let format = match image::guess_format(encoded).ok() {
        Some(ImageFormat::Png) => DecodedImageFormat::Png,
        Some(ImageFormat::Jpeg) => DecodedImageFormat::Jpeg,
        _ => return Err(ImageDecodeError::UnsupportedFormat),
    };
    let image_format = match format {
        DecodedImageFormat::Png => ImageFormat::Png,
        DecodedImageFormat::Jpeg => ImageFormat::Jpeg,
    };
    let mut decoder = ImageReader::with_format(std::io::Cursor::new(encoded), image_format)
        .into_decoder()
        .map_err(ImageDecodeError::Decode)?;
    validate_geometry(decoder.dimensions())?;
    let orientation = decoder.orientation().map_err(ImageDecodeError::Decode)?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(ImageDecodeError::Decode)?;
    decoded.apply_orientation(orientation);
    let rgba = decoded.into_rgba8();
    validate_geometry(rgba.dimensions())?;
    let asset = DecodedRgbaAsset::new(rgba.width(), rgba.height(), rgba.into_raw())
        .map_err(ImageDecodeError::InvalidGeometry)?;
    Ok(DecodedImageFile {
        asset,
        format,
        encoded_bytes: encoded.len(),
        encoded_sha256: format!("{:x}", Sha256::digest(encoded)),
    })
}

fn validate_geometry((width, height): (u32, u32)) -> Result<(), ImageDecodeError> {
    let bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4));
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION_PX
        || height > MAX_IMAGE_DIMENSION_PX
        || bytes.is_none_or(|bytes| bytes > MAX_DECODED_IMAGE_BYTES as u64)
    {
        return Err(ImageDecodeError::InvalidGeometry(
            AnnotationError::InvalidGeometry(
                "decoded image dimensions exceed the annotation limits".into(),
            ),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod signature_quality_tests {
    use super::*;

    #[test]
    fn transparent_signature_preserves_edge_coverage_without_spreading_ink() {
        let mut image = image::RgbaImage::new(32, 16);
        for (x, alpha) in [(10, 64), (11, 128), (12, 255)] {
            image.put_pixel(x, 8, image::Rgba([0, 0, 0, alpha]));
        }
        let signature = sanitize_signature_rgba(image).unwrap();
        let coverage: Vec<_> = signature
            .asset()
            .rgba()
            .chunks_exact(4)
            .filter_map(|pixel| (pixel[3] > 0).then_some(pixel[3]))
            .collect();
        assert_eq!(coverage, vec![64, 128, 255]);
    }

    #[test]
    fn imported_ink_keeps_its_colour() {
        // A blue stroke on white, as a scan or screenshot holds it.
        let mut image = image::RgbaImage::from_pixel(120, 40, image::Rgba([255, 255, 255, 255]));
        for x in 20..100 {
            for y in 18..23 {
                image.put_pixel(x, y, image::Rgba([20, 40, 160, 255]));
            }
        }
        let signature = sanitize_signature_rgba(image).unwrap();
        let solid: Vec<_> = signature
            .asset()
            .rgba()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] == 255)
            .collect();
        assert!(!solid.is_empty());
        assert!(solid.iter().all(|pixel| pixel[..3] == [20, 40, 160]));
    }

    #[test]
    fn a_page_drawn_on_white_keeps_only_its_ink() {
        // A 200 x 100 white page (BGRA) with a dark blue stroke.
        let (width, height) = (200_u32, 100_u32);
        let mut page = vec![255_u8; (width * height * 4) as usize];
        for x in 60..140 {
            for y in 48..52 {
                let ix = ((y * width + x) * 4) as usize;
                page[ix..ix + 4].copy_from_slice(&[120, 20, 10, 255]);
            }
        }
        let signature = sanitize_signature_raster(width, height, &page).unwrap();
        let asset = signature.asset();
        assert!(
            asset.width_px() < 100 && asset.height_px() < 20,
            "cropped to the ink"
        );
        let pixels: Vec<_> = asset.rgba().chunks_exact(4).collect();
        assert_eq!(pixels.first().unwrap()[3], 0, "the paper is transparent");
        assert!(pixels.iter().any(|pixel| pixel[3] == 255));
        assert!(matches!(
            sanitize_signature_raster(width, height, &vec![255; page.len()]),
            Err(SignatureImageError::NoInk)
        ));
    }

    #[test]
    fn a_copied_tiff_bitmap_gif_or_webp_picture_converts_to_png() {
        let picture = image::RgbaImage::from_fn(6, 4, |x, y| {
            image::Rgba([(x * 40) as u8, (y * 60) as u8, 200, 255])
        });
        for format in [
            ImageFormat::Tiff,
            ImageFormat::Bmp,
            ImageFormat::Gif,
            ImageFormat::WebP,
        ] {
            let mut encoded = Vec::new();
            DynamicImage::ImageRgba8(picture.clone())
                .write_to(&mut std::io::Cursor::new(&mut encoded), format)
                .unwrap();
            let png = clipboard_picture_png_or_jpeg(&encoded).unwrap();
            assert_eq!(image::guess_format(&png).unwrap(), ImageFormat::Png);
            let decoded = decode_image_bytes(&png).unwrap();
            assert_eq!(
                (decoded.asset().width_px(), decoded.asset().height_px()),
                (6, 4),
                "{format:?}"
            );
        }
        let mut png = Vec::new();
        DynamicImage::ImageRgba8(picture)
            .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
            .unwrap();
        assert!(matches!(
            clipboard_picture_png_or_jpeg(&png),
            Ok(std::borrow::Cow::Borrowed(_))
        ));
        assert!(matches!(
            clipboard_picture_png_or_jpeg(b"<svg/>"),
            Err(ImageDecodeError::UnsupportedFormat)
        ));
    }
}
