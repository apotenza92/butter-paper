use std::{ffi::OsString, path::PathBuf};

use butter_paper_gpui_component_compat::document_workspace::{
    DocumentOpenBatchRequest, DocumentOpenOrigin,
};
use butter_paper_gpui_component_compat::native_launch::{NativeLaunchConfig, NativeLaunchError};

#[test]
fn normal_launch_accepts_explicit_and_system_pdf_paths_in_stable_order() {
    let config = NativeLaunchConfig::parse_in(
        [
            OsString::from("--open"),
            OsString::from("first.pdf"),
            OsString::from("second.PDF"),
            OsString::from("first.pdf"),
        ],
        PathBuf::from("/documents"),
    )
    .unwrap();

    assert_eq!(
        config.open_paths(),
        [
            PathBuf::from("/documents/first.pdf"),
            PathBuf::from("/documents/second.PDF"),
        ]
    );
    assert_eq!(
        config.document_open_request(),
        DocumentOpenBatchRequest::new(
            DocumentOpenOrigin::System,
            [
                PathBuf::from("/documents/first.pdf"),
                PathBuf::from("/documents/second.PDF"),
            ],
        )
    );
}

#[test]
fn normal_launch_rejects_a_missing_explicit_value_but_ignores_unrelated_arguments() {
    assert_eq!(
        NativeLaunchConfig::parse([OsString::from("--open")]).unwrap_err(),
        NativeLaunchError::MissingOpenPath
    );
    let config = NativeLaunchConfig::parse_in(
        [
            OsString::from("--unknown"),
            OsString::from("notes.txt"),
            OsString::from("drawing.pdf"),
        ],
        PathBuf::from("/documents"),
    )
    .unwrap();
    assert_eq!(
        config.open_paths(),
        [PathBuf::from("/documents/drawing.pdf")]
    );
}

#[test]
fn normal_launch_ignores_the_macos_persistence_pair() {
    let config = NativeLaunchConfig::parse_in(
        [
            OsString::from("-ApplePersistenceIgnoreState"),
            OsString::from("YES"),
            OsString::from("drawing.pdf"),
        ],
        PathBuf::from("/documents"),
    )
    .unwrap();
    assert_eq!(
        config.open_paths(),
        [PathBuf::from("/documents/drawing.pdf")]
    );
}

#[cfg(unix)]
#[test]
fn normal_launch_preserves_a_non_utf8_pdf_path() {
    use std::os::unix::ffi::OsStringExt as _;

    let mut bytes = b"drawing-".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b".PdF");
    let path = PathBuf::from(OsString::from_vec(bytes));
    let config =
        NativeLaunchConfig::parse_in([path.clone().into_os_string()], PathBuf::from("/documents"))
            .unwrap();
    assert_eq!(
        config.open_paths(),
        [PathBuf::from("/documents").join(path)]
    );
}
