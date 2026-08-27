//! GPUI-free, bounded PNG/JPEG decoding for annotation assets.

use std::{
    error::Error,
    fmt, fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
};

use image::{DynamicImage, ImageDecoder as _, ImageFormat, ImageReader};
use sha2::{Digest as _, Sha256};

use crate::annotation_model::{
    AnnotationError, DecodedRgbaAsset, MAX_DECODED_IMAGE_BYTES, MAX_IMAGE_DIMENSION_PX,
};

pub const MAX_ENCODED_IMAGE_BYTES: usize = 32 * 1024 * 1024;

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
