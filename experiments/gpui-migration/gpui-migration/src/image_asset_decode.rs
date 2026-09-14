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
            let value = (255. * scaled * scaled * (3. - 2. * scaled)).round() as u8;
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
    let mut output = Vec::with_capacity((output_width * output_height * 4) as usize);
    for y in top..=bottom {
        for x in left..=right {
            output.extend_from_slice(&[17, 24, 39, alpha[(y * width + x) as usize]]);
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
