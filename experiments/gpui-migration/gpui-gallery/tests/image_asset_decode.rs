use std::path::PathBuf;

use butter_paper_gpui_gallery::image_asset_decode::{
    DecodedImageFormat, ImageDecodeError, decode_image_bytes, decode_image_path,
    decode_image_path_with_limit,
};

fn checker_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../performance/results/public-fixtures-v1/bp-image-checker-v1.png")
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
