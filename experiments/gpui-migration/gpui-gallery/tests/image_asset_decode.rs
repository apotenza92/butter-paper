use std::{io::Cursor, path::PathBuf};

use butter_paper_gpui_gallery::image_asset_decode::{
    DecodedImageFormat, ImageDecodeError, SignatureImageError, decode_image_bytes,
    decode_image_path, decode_image_path_with_limit, sanitize_signature_bytes,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

fn checker_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../performance/results/public-fixtures-v1/bp-image-checker-v1.png")
}

fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> Rgba<u8>) -> Vec<u8> {
    let image = RgbaImage::from_fn(width, height, pixel);
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    encoded.into_inner()
}

#[test]
fn signature_sanitizer_rejects_blank_dense_and_unsafe_inputs() {
    let blank = png(40, 20, |_, _| Rgba([255, 255, 255, 255]));
    assert_eq!(
        sanitize_signature_bytes(&blank).unwrap_err().to_string(),
        "No signature was found. Use dark ink on white paper."
    );

    let dense = png(40, 20, |_, _| Rgba([0, 0, 0, 255]));
    assert_eq!(
        sanitize_signature_bytes(&dense).unwrap_err().to_string(),
        "Too much background was detected. Fill the view with white paper."
    );

    let too_wide = png(4097, 1, |_, _| Rgba([255, 255, 255, 255]));
    assert!(matches!(
        sanitize_signature_bytes(&too_wide),
        Err(SignatureImageError::UnsafeDimensions)
    ));
    let extreme = png(26, 1, |_, _| Rgba([255, 255, 255, 255]));
    assert!(matches!(
        sanitize_signature_bytes(&extreme),
        Err(SignatureImageError::ExtremeAspectRatio)
    ));
    assert!(matches!(
        sanitize_signature_bytes(&vec![0_u8; 10 * 1024 * 1024 + 1]),
        Err(SignatureImageError::SourceTooLarge)
    ));

    let transparent_black = png(40, 20, |_, _| Rgba([0, 0, 0, 0]));
    assert!(matches!(
        sanitize_signature_bytes(&transparent_black),
        Err(SignatureImageError::NoInk)
    ));
}

#[test]
fn signature_sanitizer_normalizes_crops_and_downscales_to_canonical_rgba() {
    let source = png(3000, 120, |x, y| {
        if (1200..1800).contains(&x) && (45..75).contains(&y) {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    let sanitized = sanitize_signature_bytes(&source).unwrap();
    let asset = sanitized.asset();
    assert!(asset.width_px() < 600);
    assert!(asset.height_px() < 120);
    assert!(asset.width_px() > 400);
    assert!(
        asset
            .rgba()
            .chunks_exact(4)
            .all(|pixel| { pixel[0] == 17 && pixel[1] == 24 && pixel[2] == 39 })
    );
    assert!(asset.rgba().chunks_exact(4).any(|pixel| pixel[3] == 255));
    assert!(asset.rgba().chunks_exact(4).any(|pixel| pixel[3] == 0));

    let repeated = sanitize_signature_bytes(&source).unwrap();
    assert_eq!(repeated.asset(), asset);
    assert_eq!(repeated.asset().id(), asset.id());
}

#[test]
fn signature_sanitizer_applies_jpeg_exif_orientation_before_cropping() {
    let image = RgbaImage::from_fn(120, 40, |x, y| {
        if (20..100).contains(&x) && (16..24).contains(&y) {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([255, 255, 255, 255])
        }
    });
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, ImageFormat::Jpeg)
        .unwrap();
    let encoded = encoded.into_inner();
    let exif_orientation_six = [
        0xff, 0xe1, 0x00, 0x22, b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 0x2a, 0, 8, 0, 0, 0, 1,
        0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
    ];
    let mut oriented = Vec::with_capacity(encoded.len() + exif_orientation_six.len());
    oriented.extend_from_slice(&encoded[..2]);
    oriented.extend_from_slice(&exif_orientation_six);
    oriented.extend_from_slice(&encoded[2..]);

    let sanitized = sanitize_signature_bytes(&oriented).unwrap();
    assert!(sanitized.asset().height_px() > sanitized.asset().width_px());
}

#[test]
fn locked_checker_decode_is_exact_and_bounded() {
    let decoded = decode_image_path(checker_path()).unwrap();
    assert_eq!(decoded.format(), DecodedImageFormat::Png);
    assert_eq!(
        decoded.encoded_sha256(),
        "fcc714d1ac60ed4b88abf7297830479c7557cb9d219033e7a5a5ad4d6ec18dda"
    );
    assert_eq!(
        (decoded.asset().width_px(), decoded.asset().height_px()),
        (512, 384)
    );
    assert_eq!(decoded.asset().rgba().len(), 512 * 384 * 4);
    assert!(decoded.encoded_bytes() > 0);

    assert!(matches!(
        decode_image_bytes(b"not an image"),
        Err(ImageDecodeError::UnsupportedFormat)
    ));
    assert!(matches!(
        decode_image_path_with_limit(&checker_path(), 8),
        Err(ImageDecodeError::EncodedFileTooLarge {
            maximum_bytes: 8,
            ..
        })
    ));
}
