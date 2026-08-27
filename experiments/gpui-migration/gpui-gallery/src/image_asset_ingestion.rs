//! Bounded, synchronous image ingestion for image annotations.
//!
//! The annotation model preserves canonical RGBA bytes. GPUI's `RenderImage`
//! contract uses BGRA bytes, so ingestion creates both payloads from one
//! completed decode. The returned value has no deferred file read or image
//! decode left to enter a measured annotation journey.

use std::{
    error::Error,
    fmt, fs,
    io::{self, Read as _},
    path::{Path, PathBuf},
    sync::Arc,
};

use gpui::RenderImage;
use image::{DynamicImage, Frame, ImageBuffer, ImageDecoder as _, ImageFormat, ImageReader, Rgba};
use sha2::{Digest as _, Sha256};
use smallvec::smallvec;

use crate::annotation_model::{
    AnnotationError, DecodedRgbaAsset, MAX_DECODED_IMAGE_BYTES, MAX_IMAGE_DIMENSION_PX,
};

/// Maximum encoded file size accepted at the annotation product boundary.
pub const MAX_ENCODED_IMAGE_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IngestedImageFormat {
    Png,
    Jpeg,
}

impl IngestedImageFormat {
    fn from_image_format(format: ImageFormat) -> Option<Self> {
        match format {
            ImageFormat::Png => Some(Self::Png),
            ImageFormat::Jpeg => Some(Self::Jpeg),
            _ => None,
        }
    }
}

/// The two image payloads consumed by the annotation model and GPUI paint path.
#[derive(Debug)]
pub struct IngestedImageAsset {
    annotation_asset: DecodedRgbaAsset,
    render_image: Arc<RenderImage>,
    format: IngestedImageFormat,
    encoded_bytes: usize,
    encoded_sha256: String,
}

impl IngestedImageAsset {
    pub fn annotation_asset(&self) -> &DecodedRgbaAsset {
        &self.annotation_asset
    }

    pub fn into_annotation_asset(self) -> DecodedRgbaAsset {
        self.annotation_asset
    }

    pub fn render_image(&self) -> &Arc<RenderImage> {
        &self.render_image
    }

    pub fn format(&self) -> IngestedImageFormat {
        self.format
    }

    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    /// SHA-256 of the exact encoded file bytes read from disk.
    pub fn encoded_sha256(&self) -> &str {
        &self.encoded_sha256
    }

    pub fn into_parts(self) -> (DecodedRgbaAsset, Arc<RenderImage>) {
        (self.annotation_asset, self.render_image)
    }
}

#[derive(Debug)]
pub enum ImageAssetIngestionError {
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

impl fmt::Display for ImageAssetIngestionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    formatter,
                    "could not read image {}: {source}",
                    path.display()
                )
            }
            Self::EncodedFileTooLarge {
                actual_bytes,
                maximum_bytes,
            } => write!(
                formatter,
                "encoded image is {actual_bytes} bytes; the limit is {maximum_bytes} bytes"
            ),
            Self::UnsupportedFormat => {
                write!(formatter, "image must be a PNG or JPEG file")
            }
            Self::Decode(source) => write!(formatter, "could not decode image: {source}"),
            Self::InvalidGeometry(source) => source.fmt(formatter),
        }
    }
}

impl Error for ImageAssetIngestionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read { source, .. } => Some(source),
            Self::Decode(source) => Some(source),
            Self::InvalidGeometry(source) => Some(source),
            Self::EncodedFileTooLarge { .. } | Self::UnsupportedFormat => None,
        }
    }
}

/// Read and fully decode one bounded PNG or JPEG from disk.
pub fn ingest_image_asset_from_path(
    path: impl AsRef<Path>,
) -> Result<IngestedImageAsset, ImageAssetIngestionError> {
    ingest_image_asset_from_path_with_limit(path.as_ref(), MAX_ENCODED_IMAGE_BYTES)
}

fn ingest_image_asset_from_path_with_limit(
    path: &Path,
    encoded_limit: usize,
) -> Result<IngestedImageAsset, ImageAssetIngestionError> {
    let mut file = fs::File::open(path).map_err(|source| ImageAssetIngestionError::Read {
        path: path.to_owned(),
        source,
    })?;
    let read_limit = u64::try_from(encoded_limit)
        .unwrap_or(u64::MAX - 1)
        .saturating_add(1);
    let mut encoded = Vec::with_capacity(encoded_limit.min(1024 * 1024));
    file.by_ref()
        .take(read_limit)
        .read_to_end(&mut encoded)
        .map_err(|source| ImageAssetIngestionError::Read {
            path: path.to_owned(),
            source,
        })?;
    if encoded.len() > encoded_limit {
        return Err(ImageAssetIngestionError::EncodedFileTooLarge {
            actual_bytes: encoded.len(),
            maximum_bytes: encoded_limit,
        });
    }
    decode_image_asset(&encoded)
}

fn decode_image_asset(encoded: &[u8]) -> Result<IngestedImageAsset, ImageAssetIngestionError> {
    let image_format = image::guess_format(encoded)
        .ok()
        .and_then(IngestedImageFormat::from_image_format)
        .ok_or(ImageAssetIngestionError::UnsupportedFormat)?;
    let decoder_format = match image_format {
        IngestedImageFormat::Png => ImageFormat::Png,
        IngestedImageFormat::Jpeg => ImageFormat::Jpeg,
    };
    let cursor = std::io::Cursor::new(encoded);
    let mut decoder = ImageReader::with_format(cursor, decoder_format)
        .into_decoder()
        .map_err(ImageAssetIngestionError::Decode)?;
    let (encoded_width, encoded_height) = decoder.dimensions();
    validate_decoded_geometry(encoded_width, encoded_height)?;
    let orientation = decoder
        .orientation()
        .map_err(ImageAssetIngestionError::Decode)?;
    let mut decoded =
        DynamicImage::from_decoder(decoder).map_err(ImageAssetIngestionError::Decode)?;
    decoded.apply_orientation(orientation);
    let rgba = decoded.into_rgba8();
    let (width, height) = rgba.dimensions();
    validate_decoded_geometry(width, height)?;

    let annotation_asset = DecodedRgbaAsset::new(width, height, rgba.as_raw().clone())
        .map_err(ImageAssetIngestionError::InvalidGeometry)?;
    let mut bgra = rgba.into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    let render_pixels = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width, height, bgra)
        .ok_or_else(|| {
            ImageAssetIngestionError::InvalidGeometry(AnnotationError::InvalidGeometry(
                "could not construct the GPUI image payload".into(),
            ))
        })?;
    let render_image = Arc::new(RenderImage::new(smallvec![Frame::new(render_pixels)]));
    Ok(IngestedImageAsset {
        annotation_asset,
        render_image,
        format: image_format,
        encoded_bytes: encoded.len(),
        encoded_sha256: format!("{:x}", Sha256::digest(encoded)),
    })
}

fn validate_decoded_geometry(width: u32, height: u32) -> Result<(), ImageAssetIngestionError> {
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION_PX
        || height > MAX_IMAGE_DIMENSION_PX
    {
        return Err(ImageAssetIngestionError::InvalidGeometry(
            AnnotationError::InvalidGeometry(format!(
                "decoded image dimensions must be between 1 and {MAX_IMAGE_DIMENSION_PX} pixels"
            )),
        ));
    }
    let decoded_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            ImageAssetIngestionError::InvalidGeometry(AnnotationError::InvalidGeometry(
                "decoded image byte count overflowed".into(),
            ))
        })?;
    if decoded_bytes > MAX_DECODED_IMAGE_BYTES as u64 {
        return Err(ImageAssetIngestionError::InvalidGeometry(
            AnnotationError::InvalidGeometry(format!(
                "decoded image exceeds the {MAX_DECODED_IMAGE_BYTES}-byte limit"
            )),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use image::{ExtendedColorType, ImageEncoder as _, codecs::jpeg::JpegEncoder};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn png_ingestion_returns_canonical_rgba_and_gpui_bgra() {
        let directory = tempdir().expect("temporary image directory");
        let path = directory.path().join("pixel.png");
        let rgba = [29, 110, 216, 255, 220, 38, 38, 127];
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(&rgba, 2, 1, ExtendedColorType::Rgba8)
            .expect("encode PNG");
        fs::write(&path, &encoded).expect("write PNG fixture");

        let ingested = ingest_image_asset_from_path(&path).expect("ingest PNG");

        assert_eq!(ingested.format(), IngestedImageFormat::Png);
        assert_eq!(ingested.encoded_bytes(), encoded.len());
        assert_eq!(ingested.annotation_asset().rgba(), rgba);
        assert_eq!(
            ingested.render_image().as_bytes(0),
            Some([216, 110, 29, 255, 38, 38, 220, 127].as_slice())
        );
        assert_eq!(ingested.render_image().size(0).width.0, 2);
        assert_eq!(ingested.render_image().size(0).height.0, 1);
    }

    #[test]
    fn jpeg_ingestion_is_supported_and_fully_decoded() {
        let directory = tempdir().expect("temporary image directory");
        let path = directory.path().join("pixel.jpeg");
        let mut encoded = Vec::new();
        JpegEncoder::new_with_quality(&mut encoded, 100)
            .write_image(&[29, 110, 216], 1, 1, ExtendedColorType::Rgb8)
            .expect("encode JPEG");
        fs::write(&path, &encoded).expect("write JPEG fixture");

        let ingested = ingest_image_asset_from_path(&path).expect("ingest JPEG");

        assert_eq!(ingested.format(), IngestedImageFormat::Jpeg);
        assert_eq!(ingested.annotation_asset().width_px(), 1);
        assert_eq!(ingested.annotation_asset().height_px(), 1);
        assert_eq!(ingested.annotation_asset().rgba().len(), 4);
        assert_eq!(ingested.render_image().frame_count(), 1);
        assert_eq!(
            ingested.render_image().as_bytes(0).map(<[u8]>::len),
            Some(4)
        );
    }

    #[test]
    fn encoded_limit_is_enforced_while_reading() {
        let directory = tempdir().expect("temporary image directory");
        let path = directory.path().join("too-large.png");
        fs::write(&path, [0_u8; 9]).expect("write oversized fixture");

        let error = ingest_image_asset_from_path_with_limit(&path, 8)
            .expect_err("encoded limit must reject the file");

        assert!(matches!(
            error,
            ImageAssetIngestionError::EncodedFileTooLarge {
                actual_bytes: 9,
                maximum_bytes: 8
            }
        ));
    }

    #[test]
    fn unsupported_encoded_format_is_rejected() {
        let directory = tempdir().expect("temporary image directory");
        let path = directory.path().join("animation.gif");
        fs::write(&path, b"GIF89a").expect("write unsupported fixture");

        let error =
            ingest_image_asset_from_path(&path).expect_err("non-PNG/JPEG input must be rejected");

        assert!(matches!(error, ImageAssetIngestionError::UnsupportedFormat));
    }

    #[test]
    fn decoded_dimensions_are_rejected_before_pixel_decode() {
        let directory = tempdir().expect("temporary image directory");
        let path = directory.path().join("wide.png");
        let width = MAX_IMAGE_DIMENSION_PX + 1;
        let pixels = vec![0_u8; width as usize * 3];
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(&pixels, width, 1, ExtendedColorType::Rgb8)
            .expect("encode wide PNG");
        fs::write(&path, encoded).expect("write wide fixture");

        let error = ingest_image_asset_from_path(&path)
            .expect_err("oversized decoded dimensions must be rejected");

        assert!(matches!(
            error,
            ImageAssetIngestionError::InvalidGeometry(_)
        ));
    }

    #[test]
    fn exact_frozen_comparison_png_is_decoded_from_disk() {
        const FROZEN_PNG_BASE64: &str =
            include_str!("../tests/fixtures/bp-image-checker-v1.png.base64");
        const FROZEN_PNG_SHA256: &str =
            "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda";
        let encoded = decode_base64(FROZEN_PNG_BASE64.trim());
        assert_eq!(format!("{:x}", Sha256::digest(&encoded)), FROZEN_PNG_SHA256);
        let directory = tempdir().expect("temporary image directory");
        let path = directory.path().join("bp-image-checker-v1.png");
        fs::write(&path, &encoded).expect("write exact frozen PNG to disk");

        let ingested = ingest_image_asset_from_path(&path).expect("ingest exact frozen PNG");

        assert_eq!(ingested.format(), IngestedImageFormat::Png);
        assert_eq!(ingested.encoded_bytes(), 3_153);
        assert_eq!(ingested.encoded_sha256(), FROZEN_PNG_SHA256);
        assert_eq!(ingested.annotation_asset().width_px(), 512);
        assert_eq!(ingested.annotation_asset().height_px(), 384);
        assert_eq!(ingested.annotation_asset().rgba().len(), 786_432);
        assert_eq!(
            &ingested.annotation_asset().rgba()[0..4],
            &[29, 110, 216, 255]
        );
        assert_eq!(
            &ingested.annotation_asset().rgba()[((192 * 512 + 256) * 4)..][..4],
            &[220, 38, 38, 255]
        );
        assert_eq!(
            ingested.render_image().as_bytes(0).map(<[u8]>::len),
            Some(786_432)
        );
        assert_eq!(
            &ingested.render_image().as_bytes(0).expect("BGRA frame")[0..4],
            &[216, 110, 29, 255]
        );
    }

    fn decode_base64(encoded: &str) -> Vec<u8> {
        let mut output = Vec::with_capacity(encoded.len() / 4 * 3);
        let mut quartet = [0_u8; 4];
        let mut quartet_length = 0;
        for byte in encoded.bytes().filter(|byte| !byte.is_ascii_whitespace()) {
            quartet[quartet_length] = match byte {
                b'A'..=b'Z' => byte - b'A',
                b'a'..=b'z' => byte - b'a' + 26,
                b'0'..=b'9' => byte - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                b'=' => 64,
                _ => panic!("invalid base64 byte"),
            };
            quartet_length += 1;
            if quartet_length == 4 {
                output.push((quartet[0] << 2) | (quartet[1] >> 4));
                if quartet[2] != 64 {
                    output.push((quartet[1] << 4) | (quartet[2] >> 2));
                }
                if quartet[3] != 64 {
                    output.push((quartet[2] << 6) | quartet[3]);
                }
                quartet_length = 0;
            }
        }
        assert_eq!(quartet_length, 0, "base64 input must contain full quartets");
        output
    }
}
