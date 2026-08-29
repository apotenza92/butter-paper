use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use butter_paper_gpui_component_compat::document_workspace::{
    DocumentOpenBatchRequest, DocumentOpenOrigin,
};
use butter_paper_gpui_component_compat::native_document_view_state::RestartView;
use butter_paper_gpui_component_compat::native_launch::{
    NativeLaunchAction, NativeLaunchConfig, NativeLaunchError, NativeLaunchSessionSource,
    NativeLaunchWarning,
};
use butter_paper_gpui_component_compat::session_manifest::{
    SessionManifestCorruptionError, SessionManifestError, SessionManifestOperationError,
    SessionManifestStore, SessionManifestValidationError, SessionSnapshot,
};

static SCRATCH_SEQUENCE: AtomicU64 = AtomicU64::new(1);

struct ScratchRoot(PathBuf);

impl ScratchRoot {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "butter-paper-session-manifest-{label}-{}-{}",
            std::process::id(),
            SCRATCH_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for ScratchRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn manifest_pdf(name: &str) -> PathBuf {
    #[cfg(unix)]
    return PathBuf::from("/plans").join(name);
    #[cfg(windows)]
    return PathBuf::from(r"C:\plans").join(name);
}

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

#[test]
fn native_launch_resolver_skips_unneeded_loads_and_types_restore_failures() {
    let explicit = NativeLaunchConfig::parse_in(
        [OsString::from("first.pdf"), OsString::from("second.pdf")],
        PathBuf::from("/documents"),
    )
    .unwrap();
    let performance = NativeLaunchSessionSource::new(true, &explicit);
    assert!(!performance.requires_store());
    assert!(!performance.requires_manifest_load());
    let performance = performance.resolve(Err("must be ignored".into()));
    assert_eq!(performance.action, NativeLaunchAction::None);
    assert!(!performance.checkpoint_enabled);
    assert_eq!(performance.warning, None);

    let explicit_source = NativeLaunchSessionSource::new(false, &explicit);
    assert!(explicit_source.requires_store());
    assert!(!explicit_source.requires_manifest_load());
    let explicit_resolution = explicit_source.resolve(Ok(None));
    assert_eq!(
        explicit_resolution.action,
        NativeLaunchAction::OpenExplicit(DocumentOpenBatchRequest::new(
            DocumentOpenOrigin::System,
            [
                PathBuf::from("/documents/first.pdf"),
                PathBuf::from("/documents/second.pdf"),
            ],
        ))
    );
    assert!(explicit_resolution.checkpoint_enabled);

    let no_explicit = NativeLaunchConfig::default();
    let manifest_source = NativeLaunchSessionSource::new(false, &no_explicit);
    assert!(manifest_source.requires_manifest_load());
    let root = ScratchRoot::new("resolver-plan");
    let store = SessionManifestStore::open(root.path().to_path_buf()).unwrap();
    store
        .replace(&SessionSnapshot::new(
            vec![manifest_pdf("one.pdf"), manifest_pdf("two.pdf")],
            Some(1),
        ))
        .unwrap();
    let restored = manifest_source.resolve(Ok(Some(store.load().unwrap())));
    assert!(matches!(restored.action, NativeLaunchAction::Restore(_)));
    assert!(restored.checkpoint_enabled);
    assert_eq!(restored.warning, None);

    let empty = NativeLaunchSessionSource::new(false, &no_explicit).resolve(Ok(Some(
        SessionManifestStore::open(ScratchRoot::new("resolver-empty").path().to_path_buf())
            .unwrap()
            .load()
            .unwrap(),
    )));
    assert_eq!(empty.action, NativeLaunchAction::None);
    assert!(empty.checkpoint_enabled);

    let failed = NativeLaunchSessionSource::new(false, &no_explicit)
        .resolve(Err("manifest is corrupt".into()));
    assert_eq!(failed.action, NativeLaunchAction::None);
    assert!(!failed.checkpoint_enabled);
    assert_eq!(
        failed.warning,
        Some(NativeLaunchWarning::SessionStateUnavailable(
            "manifest is corrupt".into()
        ))
    );
}

#[test]
fn session_manifest_missing_roundtrips_order_and_replaces_atomically() {
    let root = ScratchRoot::new("roundtrip");
    let store = SessionManifestStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(store.load().unwrap().into_parts(), (Vec::new(), None));

    let first = SessionSnapshot::new(
        vec![manifest_pdf("First.pdf"), manifest_pdf("second.PDF")],
        Some(1),
    );
    store.replace(&first).unwrap();
    assert_eq!(
        store.load().unwrap().into_parts(),
        (
            vec![manifest_pdf("First.pdf"), manifest_pdf("second.PDF"),],
            Some(1),
        )
    );

    #[cfg(unix)]
    assert_eq!(
        std::fs::read(root.path().join("session-manifest.json")).unwrap(),
        b"{\"version\":2,\"documents\":[{\"encoding\":\"unix-bytes\",\"path\":\"2f706c616e732f46697273742e706466\",\"view\":{\"currentPage\":0,\"mode\":\"continuous\",\"zoom\":\"fitWidth\",\"manualPercent\":null,\"scrollX\":0,\"scrollY\":0}},{\"encoding\":\"unix-bytes\",\"path\":\"2f706c616e732f7365636f6e642e504446\",\"view\":{\"currentPage\":0,\"mode\":\"continuous\",\"zoom\":\"fitWidth\",\"manualPercent\":null,\"scrollX\":0,\"scrollY\":0}}],\"activeDocument\":1}\n"
    );

    #[cfg(unix)]
    {
        std::fs::write(
            root.path().join("session-manifest.json"),
            b"{\"version\":1,\"documents\":[{\"encoding\":\"unix-bytes\",\"path\":\"2f706c616e732f46697273742e706466\"},{\"encoding\":\"unix-bytes\",\"path\":\"2f706c616e732f7365636f6e642e504446\"}],\"activeDocument\":1}\n",
        )
        .unwrap();
        let (legacy_documents, legacy_active) = store.load().unwrap().into_documents();
        assert_eq!(
            legacy_documents
                .iter()
                .map(|document| (document.path().to_owned(), document.view()))
                .collect::<Vec<_>>(),
            vec![
                (manifest_pdf("First.pdf"), RestartView::default()),
                (manifest_pdf("second.PDF"), RestartView::default()),
            ],
            "v1 paths must load with the exact v2 default reader view",
        );
        assert_eq!(legacy_active, Some(1));
    }

    store
        .replace(&SessionSnapshot::new(
            vec![manifest_pdf("replacement.pdf")],
            Some(0),
        ))
        .unwrap();
    assert_eq!(
        store.load().unwrap().into_parts(),
        (vec![manifest_pdf("replacement.pdf")], Some(0))
    );
    assert_eq!(
        std::fs::read_dir(root.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>(),
        [OsString::from("session-manifest.json")]
    );
}

#[cfg(windows)]
#[test]
fn session_manifest_preserves_windows_utf16le_and_rejects_foreign_unix_encoding() {
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};

    let root = ScratchRoot::new("windows-utf16");
    let store = SessionManifestStore::open(root.path().to_path_buf()).unwrap();
    let units = [
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        0xd800,
        b'.' as u16,
        b'p' as u16,
        b'd' as u16,
        b'f' as u16,
    ];
    let path = PathBuf::from(OsString::from_wide(&units));
    store
        .replace(&SessionSnapshot::new(vec![path], Some(0)))
        .unwrap();
    let (loaded, active) = store.load().unwrap().into_parts();
    assert_eq!(
        loaded[0].as_os_str().encode_wide().collect::<Vec<_>>(),
        units
    );
    assert_eq!(active, Some(0));

    std::fs::write(
        root.path().join("session-manifest.json"),
        b"{\"version\":1,\"documents\":[{\"encoding\":\"unix-bytes\",\"path\":\"2f782e706466\"}],\"activeDocument\":0}\n",
    )
    .unwrap();
    assert_eq!(
        store.load().unwrap_err(),
        SessionManifestError::Corruption(SessionManifestCorruptionError::ForeignPathEncoding)
    );
}

#[cfg(unix)]
#[test]
fn session_manifest_preserves_unix_bytes_and_rejects_foreign_windows_encoding() {
    use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};

    let root = ScratchRoot::new("unix-bytes");
    let store = SessionManifestStore::open(root.path().to_path_buf()).unwrap();
    let mut bytes = b"/plans/non-utf8-".to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(b".pdf");
    let path = PathBuf::from(OsString::from_vec(bytes.clone()));
    store
        .replace(&SessionSnapshot::new(vec![path.clone()], Some(0)))
        .unwrap();
    let (loaded, active) = store.load().unwrap().into_parts();
    assert_eq!(loaded[0].as_os_str().as_bytes(), bytes);
    assert_eq!(active, Some(0));

    std::fs::write(
        root.path().join("session-manifest.json"),
        b"{\"version\":1,\"documents\":[{\"encoding\":\"windows-utf16le\",\"path\":\"43003a005c0078002e00700064006600\"}],\"activeDocument\":0}\n",
    )
    .unwrap();
    assert_eq!(
        store.load().unwrap_err(),
        SessionManifestError::Corruption(SessionManifestCorruptionError::ForeignPathEncoding)
    );
}

#[test]
fn session_manifest_rejects_invalid_snapshots_before_publication() {
    let root = ScratchRoot::new("validation");
    let store = SessionManifestStore::open(root.path().to_path_buf()).unwrap();
    store
        .replace(&SessionSnapshot::new(
            vec![manifest_pdf("kept.pdf")],
            Some(0),
        ))
        .unwrap();
    let manifest = root.path().join("session-manifest.json");
    let prior = std::fs::read(&manifest).unwrap();

    let mut nul_path = b"/plans/nul".to_vec();
    nul_path.push(0);
    nul_path.extend_from_slice(b".pdf");
    #[cfg(unix)]
    let nul_path = {
        use std::os::unix::ffi::OsStringExt as _;
        PathBuf::from(OsString::from_vec(nul_path))
    };
    #[cfg(windows)]
    let nul_path = PathBuf::from("C:\\plans\\nul\0.pdf");

    #[cfg(unix)]
    let mut oversized = OsString::from("/");
    #[cfg(windows)]
    let mut oversized = OsString::from(r"C:\");
    oversized.push("a".repeat(32_768));
    oversized.push(".pdf");
    let invalid = [
        (
            SessionSnapshot::new(vec![PathBuf::from("relative.pdf")], None),
            SessionManifestValidationError::PathNotAbsolute,
        ),
        (
            SessionSnapshot::new(vec![manifest_pdf("readme.txt")], None),
            SessionManifestValidationError::PathIsNotPdf,
        ),
        (
            SessionSnapshot::new(vec![nul_path], None),
            SessionManifestValidationError::PathContainsNul,
        ),
        (
            SessionSnapshot::new(vec![PathBuf::from(oversized)], None),
            SessionManifestValidationError::PathTooLong,
        ),
        (
            SessionSnapshot::new(
                {
                    #[cfg(unix)]
                    let paths = vec![
                        PathBuf::from("/plans/a/../same.pdf"),
                        PathBuf::from("/plans/same.pdf"),
                    ];
                    #[cfg(windows)]
                    let paths = vec![
                        PathBuf::from(r"C:\plans\a\..\same.pdf"),
                        PathBuf::from(r"C:\plans\same.pdf"),
                    ];
                    paths
                },
                None,
            ),
            SessionManifestValidationError::DuplicatePath,
        ),
        (
            SessionSnapshot::new(
                (0..65)
                    .map(|index| manifest_pdf(&format!("{index}.pdf")))
                    .collect(),
                None,
            ),
            SessionManifestValidationError::TooManyDocuments,
        ),
        (
            SessionSnapshot::new(vec![manifest_pdf("one.pdf")], Some(1)),
            SessionManifestValidationError::ActiveDocumentOutOfRange,
        ),
        (
            SessionSnapshot::new(Vec::new(), Some(0)),
            SessionManifestValidationError::EmptySnapshotHasActiveDocument,
        ),
    ];
    for (snapshot, expected) in invalid {
        assert_eq!(
            store.replace(&snapshot).unwrap_err(),
            SessionManifestError::Validation(expected)
        );
        assert_eq!(std::fs::read(&manifest).unwrap(), prior);
    }
}

#[test]
fn session_manifest_reports_exact_corruption_without_lossy_fallbacks() {
    let root = ScratchRoot::new("corruption");
    let store = SessionManifestStore::open(root.path().to_path_buf()).unwrap();
    let manifest = root.path().join("session-manifest.json");
    #[cfg(unix)]
    let bad_hex = b"{\"version\":1,\"documents\":[{\"encoding\":\"unix-bytes\",\"path\":\"0G\"}],\"activeDocument\":0}\n".as_slice();
    #[cfg(windows)]
    let bad_hex = b"{\"version\":1,\"documents\":[{\"encoding\":\"windows-utf16le\",\"path\":\"0G\"}],\"activeDocument\":0}\n".as_slice();
    let cases: &[(&[u8], SessionManifestCorruptionError)] = &[
        (b"{", SessionManifestCorruptionError::MalformedJson),
        (
            b"{\"version\":3,\"documents\":[],\"activeDocument\":null}\n",
            SessionManifestCorruptionError::UnsupportedVersion,
        ),
        (
            b"{\"version\":2,\"documents\":[{\"encoding\":\"unix-bytes\",\"path\":\"2f706c616e732f6f6e652e706466\"}],\"activeDocument\":0}\n",
            SessionManifestCorruptionError::UnknownField,
        ),
        (
            b"{\"version\":1,\"documents\":[],\"activeDocument\":null,\"extra\":true}\n",
            SessionManifestCorruptionError::UnknownField,
        ),
        (bad_hex, SessionManifestCorruptionError::MalformedPathHex),
    ];
    for (bytes, expected) in cases {
        std::fs::write(&manifest, bytes).unwrap();
        assert_eq!(
            store.load().unwrap_err(),
            SessionManifestError::Corruption(expected.clone())
        );
    }
    std::fs::write(&manifest, vec![b' '; 1_048_577]).unwrap();
    assert_eq!(
        store.load().unwrap_err(),
        SessionManifestError::Corruption(SessionManifestCorruptionError::ManifestTooLarge)
    );
}

#[cfg(unix)]
#[test]
fn session_manifest_rejects_symlink_roots_and_manifest_files_without_touching_targets() {
    use std::os::unix::fs::symlink;

    let target_root = ScratchRoot::new("symlink-target-root");
    let link_parent = ScratchRoot::new("symlink-parent");
    let root_link = link_parent.path().join("root-link");
    symlink(target_root.path(), &root_link).unwrap();
    assert_eq!(
        SessionManifestStore::open(root_link).unwrap_err(),
        SessionManifestError::Operation(SessionManifestOperationError::RootIsSymlink)
    );

    let root = ScratchRoot::new("symlink-manifest");
    let target = root.path().join("target.json");
    let target_bytes = b"do not replace this target";
    std::fs::write(&target, target_bytes).unwrap();
    symlink(&target, root.path().join("session-manifest.json")).unwrap();
    let store = SessionManifestStore::open(root.path().to_path_buf()).unwrap();
    assert_eq!(
        store.load().unwrap_err(),
        SessionManifestError::Operation(SessionManifestOperationError::ManifestIsSymlink)
    );
    assert_eq!(
        store
            .replace(&SessionSnapshot::new(vec![manifest_pdf("a.pdf")], Some(0),))
            .unwrap_err(),
        SessionManifestError::Operation(SessionManifestOperationError::ManifestIsSymlink)
    );
    assert_eq!(std::fs::read(target).unwrap(), target_bytes);
}
