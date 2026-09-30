//! Lossless, strictly validated persistence for the ordered native session manifest.

use std::{
    collections::HashSet,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use serde_json::Value;

use crate::{
    native_document_view_state::{RestartView, RestartZoom},
    page_view_control::PageViewMode,
};

const MANIFEST_NAME: &str = "session-manifest.json";
const RECOVERY_MARKER_NAME: &str = "session-recovery.json";
const MANIFEST_VERSION: u64 = 2;
const RECOVERY_MARKER_VERSION: u64 = 1;
const MAX_MANIFEST_BYTES: u64 = 1_048_576;
const MAX_DOCUMENTS: usize = 64;
const MAX_PATH_UNITS: usize = 32_768;
const MAX_TITLE_BYTES: usize = 1_024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// A validated session checkpoint ready for atomic manifest publication.
///
/// Every durable PDF path has exactly one restart view. `active_document` is
/// either absent or an index into the ordered document list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    documents: Vec<PathBuf>,
    restart_views: Vec<RestartView>,
    active_document: Option<usize>,
}

impl SessionSnapshot {
    pub fn new(documents: Vec<PathBuf>, active_document: Option<usize>) -> Self {
        Self {
            restart_views: vec![RestartView::default(); documents.len()],
            documents,
            active_document,
        }
    }

    pub fn with_restart_views(mut self, restart_views: Vec<RestartView>) -> Self {
        self.restart_views = restart_views;
        self
    }

    pub fn documents(&self) -> &[PathBuf] {
        &self.documents
    }

    /// Appends another window's documents after this window's, skipping paths
    /// already present and anything past the manifest limit. This window's
    /// active document stays active; otherwise the other's does.
    pub fn append(&mut self, other: SessionSnapshot) {
        let mut seen = self
            .documents
            .iter()
            .map(|path| normalized_path_key(path))
            .collect::<HashSet<_>>();
        let mut other_active = None;
        for (index, (path, view)) in other.documents.into_iter().zip(other.restart_views).enumerate() {
            if self.documents.len() >= MAX_DOCUMENTS || !seen.insert(normalized_path_key(&path)) {
                continue;
            }
            if other.active_document == Some(index) {
                other_active = Some(self.documents.len());
            }
            self.documents.push(path);
            self.restart_views.push(view);
        }
        if self.active_document.is_none() {
            self.active_document = other_active;
        }
    }
}

/// One document whose committed in-memory changes had not reached a durable PDF.
///
/// This marker deliberately does not claim to contain recoverable annotation
/// content. It exists so an application-process crash cannot silently present a
/// clean restart after losing edits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRecoveryDocument {
    path: PathBuf,
    title: String,
    dirty_revision: u64,
    requires_save_as: bool,
}

impl SessionRecoveryDocument {
    pub fn new(
        path: PathBuf,
        title: impl Into<String>,
        dirty_revision: u64,
        requires_save_as: bool,
    ) -> Self {
        Self {
            path,
            title: title.into(),
            dirty_revision,
            requires_save_as,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub const fn dirty_revision(&self) -> u64 {
        self.dirty_revision
    }

    pub const fn requires_save_as(&self) -> bool {
        self.requires_save_as
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionRecoverySnapshot {
    documents: Vec<SessionRecoveryDocument>,
}

impl SessionRecoverySnapshot {
    pub fn new(documents: Vec<SessionRecoveryDocument>) -> Self {
        Self { documents }
    }

    pub fn documents(&self) -> &[SessionRecoveryDocument] {
        &self.documents
    }

    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// One marker for every window: the union of their unsaved documents,
    /// skipping duplicate paths and anything past the manifest limit.
    pub fn merged(snapshots: impl IntoIterator<Item = SessionRecoverySnapshot>) -> Self {
        let mut seen = HashSet::new();
        let mut documents = Vec::new();
        for document in snapshots.into_iter().flat_map(|snapshot| snapshot.documents) {
            if documents.len() < MAX_DOCUMENTS && seen.insert(normalized_path_key(&document.path)) {
                documents.push(document);
            }
        }
        Self { documents }
    }
}

/// A validated manifest load whose document order and active index are stable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionRestorePlan {
    documents: Vec<PathBuf>,
    restart_views: Vec<RestartView>,
    active_document: Option<usize>,
}

/// One durable PDF and the reader view that belongs to that exact path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRestoreDocument {
    path: PathBuf,
    view: RestartView,
}

impl SessionRestoreDocument {
    /// Returns the durable PDF path without normalizing its visible spelling.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the validated restart view for this document.
    pub const fn view(&self) -> RestartView {
        self.view
    }

    /// Consumes this entry and returns its durable PDF path.
    pub fn into_path(self) -> PathBuf {
        self.path
    }
}

impl SessionRestorePlan {
    /// Compatibility shim for launch and checkpoint callers that only need paths.
    ///
    /// New restore code should use [`Self::into_documents`] so a view cannot be
    /// detached from its path by positional tuple handling.
    pub fn into_parts(self) -> (Vec<PathBuf>, Option<usize>) {
        (self.documents, self.active_document)
    }

    /// Consumes the plan into path-bound document entries and the active index.
    pub fn into_documents(self) -> (Vec<SessionRestoreDocument>, Option<usize>) {
        let documents = self
            .documents
            .into_iter()
            .zip(self.restart_views)
            .map(|(path, view)| SessionRestoreDocument { path, view })
            .collect();
        (documents, self.active_document)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionManifestOperation {
    InspectRoot,
    InspectManifest,
    ReadManifest,
    CreateTemporary,
    WriteTemporary,
    SyncTemporary,
    Publish,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionManifestOperationError {
    RootIsSymlink,
    RootNotDirectory,
    ManifestIsSymlink,
    ManifestNotRegular,
    Io {
        operation: SessionManifestOperation,
        kind: io::ErrorKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionManifestValidationError {
    TooManyDocuments,
    PathNotAbsolute,
    PathIsNotPdf,
    PathContainsNul,
    PathTooLong,
    DuplicatePath,
    ActiveDocumentOutOfRange,
    EmptySnapshotHasActiveDocument,
    RestartViewCountMismatch,
    NonFiniteRestartView,
    RecoveryTitleEmpty,
    RecoveryTitleTooLong,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionManifestCorruptionError {
    ManifestTooLarge,
    MalformedJson,
    InvalidShape,
    UnsupportedVersion,
    UnknownField,
    ForeignPathEncoding,
    MalformedPathHex,
    InvalidSnapshot(SessionManifestValidationError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionManifestError {
    Operation(SessionManifestOperationError),
    Validation(SessionManifestValidationError),
    Corruption(SessionManifestCorruptionError),
    PublishedButDirectorySyncFailed { kind: io::ErrorKind },
}

#[derive(Debug)]
pub struct SessionManifestStore {
    root: PathBuf,
    manifest: PathBuf,
    recovery_marker: PathBuf,
    recovery_publication: Mutex<RecoveryPublicationState>,
}

#[derive(Debug, Default)]
struct RecoveryPublicationState {
    stopped: bool,
}

impl SessionManifestStore {
    pub fn open(root: PathBuf) -> Result<Self, SessionManifestError> {
        validate_root(&root)?;
        Ok(Self {
            manifest: root.join(MANIFEST_NAME),
            recovery_marker: root.join(RECOVERY_MARKER_NAME),
            root,
            recovery_publication: Mutex::new(RecoveryPublicationState::default()),
        })
    }

    pub fn load(&self) -> Result<SessionRestorePlan, SessionManifestError> {
        validate_root(&self.root)?;
        let metadata = match fs::symlink_metadata(&self.manifest) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(SessionRestorePlan::default());
            }
            Err(error) => {
                return Err(operation_io(
                    SessionManifestOperation::InspectManifest,
                    error,
                ));
            }
        };
        validate_manifest_metadata(&metadata)?;
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(SessionManifestError::Corruption(
                SessionManifestCorruptionError::ManifestTooLarge,
            ));
        }

        let file = open_manifest_for_read(&self.manifest)?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| operation_io(SessionManifestOperation::ReadManifest, error))?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(SessionManifestError::Corruption(
                SessionManifestCorruptionError::ManifestTooLarge,
            ));
        }
        decode_manifest(&bytes)
    }

    pub fn replace(&self, snapshot: &SessionSnapshot) -> Result<(), SessionManifestError> {
        validate_snapshot(
            &snapshot.documents,
            &snapshot.restart_views,
            snapshot.active_document,
        )
        .map_err(SessionManifestError::Validation)?;
        let encoded = encode_manifest(snapshot);

        validate_root(&self.root)?;
        match fs::symlink_metadata(&self.manifest) {
            Ok(metadata) => validate_manifest_metadata(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(operation_io(
                    SessionManifestOperation::InspectManifest,
                    error,
                ));
            }
        }

        let (temporary_path, mut temporary) = self.create_temporary("session-manifest")?;
        let mut guard = TemporaryGuard::new(temporary_path.clone());
        temporary
            .write_all(encoded.as_bytes())
            .map_err(|error| operation_io(SessionManifestOperation::WriteTemporary, error))?;
        temporary
            .sync_all()
            .map_err(|error| operation_io(SessionManifestOperation::SyncTemporary, error))?;
        drop(temporary);
        publish(&temporary_path, &self.manifest)
            .map_err(|error| operation_io(SessionManifestOperation::Publish, error))?;
        guard.disarm();

        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(
                |error| SessionManifestError::PublishedButDirectorySyncFailed {
                    kind: error.kind(),
                },
            )?;
        Ok(())
    }

    /// Loads the last live dirty-session marker. A missing marker means the
    /// previous process had no committed edits awaiting a PDF save.
    pub fn load_recovery_marker(
        &self,
    ) -> Result<Option<SessionRecoverySnapshot>, SessionManifestError> {
        validate_root(&self.root)?;
        let metadata = match fs::symlink_metadata(&self.recovery_marker) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(operation_io(
                    SessionManifestOperation::InspectManifest,
                    error,
                ));
            }
        };
        validate_manifest_metadata(&metadata)?;
        if metadata.len() > MAX_MANIFEST_BYTES {
            return Err(SessionManifestError::Corruption(
                SessionManifestCorruptionError::ManifestTooLarge,
            ));
        }
        let file = open_manifest_for_read(&self.recovery_marker)?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.take(MAX_MANIFEST_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| operation_io(SessionManifestOperation::ReadManifest, error))?;
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(SessionManifestError::Corruption(
                SessionManifestCorruptionError::ManifestTooLarge,
            ));
        }
        decode_recovery_marker(&bytes).map(Some)
    }

    /// Atomically replaces the live dirty-session marker, or durably removes it
    /// once every committed change has reached a PDF or been explicitly discarded.
    pub fn replace_recovery_marker(
        &self,
        snapshot: &SessionRecoverySnapshot,
    ) -> Result<(), SessionManifestError> {
        let publication = self
            .recovery_publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if publication.stopped {
            return Ok(());
        }
        self.replace_recovery_marker_locked(snapshot)
    }

    /// Makes a clean application-close checkpoint authoritative over every
    /// queued live marker write from this process.
    pub fn clear_recovery_marker_and_stop_live_writes(&self) -> Result<(), SessionManifestError> {
        let mut publication = self
            .recovery_publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = self.replace_recovery_marker_locked(&SessionRecoverySnapshot::default());
        // Clean-close owns the final checkpoint even when marker removal or its
        // directory durability sync reports a warning. Never let a delayed
        // debounced write race in afterwards and replace the close result.
        publication.stopped = true;
        result
    }

    /// Accepts live marker writes again after a clean close, for a window
    /// opened later in the same process (macOS keeps running without windows).
    pub fn resume_live_writes(&self) {
        self.recovery_publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stopped = false;
    }

    fn replace_recovery_marker_locked(
        &self,
        snapshot: &SessionRecoverySnapshot,
    ) -> Result<(), SessionManifestError> {
        validate_recovery_snapshot(snapshot).map_err(SessionManifestError::Validation)?;
        validate_root(&self.root)?;
        match fs::symlink_metadata(&self.recovery_marker) {
            Ok(metadata) => validate_manifest_metadata(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(operation_io(
                    SessionManifestOperation::InspectManifest,
                    error,
                ));
            }
        }

        if snapshot.is_empty() {
            match fs::remove_file(&self.recovery_marker) {
                Ok(()) => self.sync_root(),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(operation_io(SessionManifestOperation::Remove, error)),
            }
        } else {
            let encoded = encode_recovery_marker(snapshot);
            let (temporary_path, mut temporary) = self.create_temporary("session-recovery")?;
            let mut guard = TemporaryGuard::new(temporary_path.clone());
            temporary
                .write_all(encoded.as_bytes())
                .map_err(|error| operation_io(SessionManifestOperation::WriteTemporary, error))?;
            temporary
                .sync_all()
                .map_err(|error| operation_io(SessionManifestOperation::SyncTemporary, error))?;
            drop(temporary);
            publish(&temporary_path, &self.recovery_marker)
                .map_err(|error| operation_io(SessionManifestOperation::Publish, error))?;
            guard.disarm();
            self.sync_root()
        }
    }

    fn sync_root(&self) -> Result<(), SessionManifestError> {
        #[cfg(unix)]
        File::open(&self.root)
            .and_then(|directory| directory.sync_all())
            .map_err(
                |error| SessionManifestError::PublishedButDirectorySyncFailed {
                    kind: error.kind(),
                },
            )?;
        Ok(())
    }

    fn create_temporary(&self, label: &str) -> Result<(PathBuf, File), SessionManifestError> {
        for _ in 0..128 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = self
                .root
                .join(format!(".{label}.{}.{}.tmp", std::process::id(), sequence));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            match options.open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(operation_io(
                        SessionManifestOperation::CreateTemporary,
                        error,
                    ));
                }
            }
        }
        Err(SessionManifestError::Operation(
            SessionManifestOperationError::Io {
                operation: SessionManifestOperation::CreateTemporary,
                kind: io::ErrorKind::AlreadyExists,
            },
        ))
    }
}

fn validate_root(root: &Path) -> Result<(), SessionManifestError> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| operation_io(SessionManifestOperation::InspectRoot, error))?;
    if metadata.file_type().is_symlink() {
        return Err(SessionManifestError::Operation(
            SessionManifestOperationError::RootIsSymlink,
        ));
    }
    if !metadata.is_dir() {
        return Err(SessionManifestError::Operation(
            SessionManifestOperationError::RootNotDirectory,
        ));
    }
    Ok(())
}

fn validate_manifest_metadata(metadata: &fs::Metadata) -> Result<(), SessionManifestError> {
    if metadata.file_type().is_symlink() {
        return Err(SessionManifestError::Operation(
            SessionManifestOperationError::ManifestIsSymlink,
        ));
    }
    if !metadata.is_file() {
        return Err(SessionManifestError::Operation(
            SessionManifestOperationError::ManifestNotRegular,
        ));
    }
    Ok(())
}

fn operation_io(operation: SessionManifestOperation, error: io::Error) -> SessionManifestError {
    SessionManifestError::Operation(SessionManifestOperationError::Io {
        operation,
        kind: error.kind(),
    })
}

fn open_manifest_for_read(path: &Path) -> Result<File, SessionManifestError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    options
        .open(path)
        .map_err(|error| operation_io(SessionManifestOperation::ReadManifest, error))
}

fn validate_snapshot(
    documents: &[PathBuf],
    restart_views: &[RestartView],
    active_document: Option<usize>,
) -> Result<(), SessionManifestValidationError> {
    if documents.len() > MAX_DOCUMENTS {
        return Err(SessionManifestValidationError::TooManyDocuments);
    }
    if documents.is_empty() && active_document.is_some() {
        return Err(SessionManifestValidationError::EmptySnapshotHasActiveDocument);
    }
    if active_document.is_some_and(|index| index >= documents.len()) {
        return Err(SessionManifestValidationError::ActiveDocumentOutOfRange);
    }
    if restart_views.len() != documents.len() {
        return Err(SessionManifestValidationError::RestartViewCountMismatch);
    }
    if restart_views.iter().any(|view| !view.is_finite()) {
        return Err(SessionManifestValidationError::NonFiniteRestartView);
    }
    let mut unique = HashSet::with_capacity(documents.len());
    for path in documents {
        validate_path(path)?;
        if !unique.insert(normalized_path_key(path)) {
            return Err(SessionManifestValidationError::DuplicatePath);
        }
    }
    Ok(())
}

fn validate_recovery_snapshot(
    snapshot: &SessionRecoverySnapshot,
) -> Result<(), SessionManifestValidationError> {
    if snapshot.documents.len() > MAX_DOCUMENTS {
        return Err(SessionManifestValidationError::TooManyDocuments);
    }
    let mut unique = HashSet::with_capacity(snapshot.documents.len());
    for document in &snapshot.documents {
        validate_path(&document.path)?;
        if !unique.insert(normalized_path_key(&document.path)) {
            return Err(SessionManifestValidationError::DuplicatePath);
        }
        if document.title.is_empty() {
            return Err(SessionManifestValidationError::RecoveryTitleEmpty);
        }
        if document.title.len() > MAX_TITLE_BYTES {
            return Err(SessionManifestValidationError::RecoveryTitleTooLong);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn validate_path(path: &Path) -> Result<(), SessionManifestValidationError> {
    use std::os::unix::ffi::OsStrExt as _;

    let bytes = path.as_os_str().as_bytes();
    if !path.is_absolute() {
        return Err(SessionManifestValidationError::PathNotAbsolute);
    }
    if bytes.contains(&0) {
        return Err(SessionManifestValidationError::PathContainsNul);
    }
    if bytes.len() > MAX_PATH_UNITS {
        return Err(SessionManifestValidationError::PathTooLong);
    }
    if bytes.is_empty() || !ascii_suffix_is_pdf(bytes) {
        return Err(SessionManifestValidationError::PathIsNotPdf);
    }
    Ok(())
}

#[cfg(windows)]
fn validate_path(path: &Path) -> Result<(), SessionManifestValidationError> {
    use std::os::windows::ffi::OsStrExt as _;

    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if !path.is_absolute() {
        return Err(SessionManifestValidationError::PathNotAbsolute);
    }
    if units.contains(&0) {
        return Err(SessionManifestValidationError::PathContainsNul);
    }
    if units.len() > MAX_PATH_UNITS {
        return Err(SessionManifestValidationError::PathTooLong);
    }
    if units.is_empty() || !wide_suffix_is_pdf(&units) {
        return Err(SessionManifestValidationError::PathIsNotPdf);
    }
    Ok(())
}

#[cfg(unix)]
pub(crate) fn normalized_path_key(path: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt as _;

    let mut normalized = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
            Component::Prefix(_) => unreachable!("Unix paths do not have prefixes"),
        }
    }
    normalized.as_os_str().as_bytes().to_vec()
}

#[cfg(windows)]
pub(crate) fn normalized_path_key(path: &Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt as _;

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push("\\"),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
        .as_os_str()
        .encode_wide()
        .map(|unit| match unit {
            0x41..=0x5a => unit + 0x20,
            _ => unit,
        })
        .collect()
}

#[cfg(unix)]
fn encode_path(path: &Path) -> (&'static str, String) {
    use std::os::unix::ffi::OsStrExt as _;

    ("unix-bytes", hex_encode(path.as_os_str().as_bytes()))
}

#[cfg(windows)]
fn encode_path(path: &Path) -> (&'static str, String) {
    use std::os::windows::ffi::OsStrExt as _;

    let bytes = path
        .as_os_str()
        .encode_wide()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    ("windows-utf16le", hex_encode(&bytes))
}

fn encode_manifest(snapshot: &SessionSnapshot) -> String {
    let mut encoded = format!("{{\"version\":{MANIFEST_VERSION},\"documents\":[");
    for (index, path) in snapshot.documents.iter().enumerate() {
        if index != 0 {
            encoded.push(',');
        }
        let (encoding, path) = encode_path(path);
        encoded.push_str("{\"encoding\":\"");
        encoded.push_str(encoding);
        encoded.push_str("\",\"path\":\"");
        encoded.push_str(&path);
        encoded.push_str("\",\"view\":");
        encode_restart_view(&mut encoded, snapshot.restart_views[index]);
        encoded.push('}');
    }
    encoded.push_str("],\"activeDocument\":");
    match snapshot.active_document {
        Some(index) => encoded.push_str(&index.to_string()),
        None => encoded.push_str("null"),
    }
    encoded.push_str("}\n");
    encoded
}

fn encode_recovery_marker(snapshot: &SessionRecoverySnapshot) -> String {
    let mut documents = Vec::with_capacity(snapshot.documents.len());
    for document in &snapshot.documents {
        let (encoding, path) = encode_path(&document.path);
        documents.push(serde_json::json!({
            "encoding": encoding,
            "path": path,
            "title": document.title,
            "dirtyRevision": document.dirty_revision,
            "requiresSaveAs": document.requires_save_as,
        }));
    }
    let mut encoded = serde_json::json!({
        "version": RECOVERY_MARKER_VERSION,
        "documents": documents,
    })
    .to_string();
    encoded.push('\n');
    encoded
}

fn encode_restart_view(encoded: &mut String, view: RestartView) {
    let mode = match view.mode() {
        PageViewMode::Continuous => "continuous",
        PageViewMode::SinglePage => "singlePage",
    };
    let (zoom, percent) = match view.zoom() {
        RestartZoom::FitWidth => ("fitWidth", None),
        RestartZoom::FitPage => ("fitPage", None),
        RestartZoom::Manual(percent) => ("manual", Some(percent)),
    };
    let (scroll_x, scroll_y) = view.scroll();
    encoded.push_str("{\"currentPage\":");
    encoded.push_str(&view.current_page().to_string());
    encoded.push_str(",\"mode\":\"");
    encoded.push_str(mode);
    encoded.push_str("\",\"zoom\":\"");
    encoded.push_str(zoom);
    encoded.push_str("\",\"manualPercent\":");
    match percent {
        Some(percent) => encoded.push_str(&percent.to_string()),
        None => encoded.push_str("null"),
    }
    encoded.push_str(",\"scrollX\":");
    encoded.push_str(&scroll_x.to_string());
    encoded.push_str(",\"scrollY\":");
    encoded.push_str(&scroll_y.to_string());
    encoded.push('}');
}

fn decode_manifest(bytes: &[u8]) -> Result<SessionRestorePlan, SessionManifestError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| {
        SessionManifestError::Corruption(SessionManifestCorruptionError::MalformedJson)
    })?;
    let object = value.as_object().ok_or_else(invalid_shape)?;
    require_exact_fields(
        object.keys().map(String::as_str),
        &["version", "documents", "activeDocument"],
    )?;
    let version = object
        .get("version")
        .and_then(Value::as_u64)
        .ok_or_else(invalid_shape)?;
    if !matches!(version, 1 | MANIFEST_VERSION) {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::UnsupportedVersion,
        ));
    }
    let encoded_documents = object
        .get("documents")
        .and_then(Value::as_array)
        .ok_or_else(invalid_shape)?;
    let active_document = match object.get("activeDocument") {
        Some(Value::Null) => None,
        Some(value) => Some(
            usize::try_from(value.as_u64().ok_or_else(invalid_shape)?)
                .map_err(|_| invalid_shape())?,
        ),
        None => return Err(invalid_shape()),
    };
    let mut documents = Vec::with_capacity(encoded_documents.len());
    let mut restart_views = Vec::with_capacity(encoded_documents.len());
    for encoded in encoded_documents {
        let object = encoded.as_object().ok_or_else(invalid_shape)?;
        let expected_fields = if version == 1 {
            &["encoding", "path"][..]
        } else {
            &["encoding", "path", "view"][..]
        };
        require_exact_fields(object.keys().map(String::as_str), expected_fields)?;
        let encoding = object
            .get("encoding")
            .and_then(Value::as_str)
            .ok_or_else(invalid_shape)?;
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(invalid_shape)?;
        documents.push(decode_path(encoding, path)?);
        restart_views.push(if version == 1 {
            RestartView::default()
        } else {
            decode_restart_view(object.get("view").ok_or_else(invalid_shape)?)?
        });
    }
    validate_snapshot(&documents, &restart_views, active_document).map_err(|error| {
        SessionManifestError::Corruption(SessionManifestCorruptionError::InvalidSnapshot(error))
    })?;
    Ok(SessionRestorePlan {
        documents,
        restart_views,
        active_document,
    })
}

fn decode_recovery_marker(bytes: &[u8]) -> Result<SessionRecoverySnapshot, SessionManifestError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| {
        SessionManifestError::Corruption(SessionManifestCorruptionError::MalformedJson)
    })?;
    let object = value.as_object().ok_or_else(invalid_shape)?;
    require_exact_fields(object.keys().map(String::as_str), &["version", "documents"])?;
    if object.get("version").and_then(Value::as_u64) != Some(RECOVERY_MARKER_VERSION) {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::UnsupportedVersion,
        ));
    }
    let encoded_documents = object
        .get("documents")
        .and_then(Value::as_array)
        .ok_or_else(invalid_shape)?;
    let mut documents = Vec::with_capacity(encoded_documents.len());
    for encoded in encoded_documents {
        let object = encoded.as_object().ok_or_else(invalid_shape)?;
        require_exact_fields(
            object.keys().map(String::as_str),
            &[
                "encoding",
                "path",
                "title",
                "dirtyRevision",
                "requiresSaveAs",
            ],
        )?;
        let encoding = object
            .get("encoding")
            .and_then(Value::as_str)
            .ok_or_else(invalid_shape)?;
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(invalid_shape)?;
        let title = object
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(invalid_shape)?;
        let dirty_revision = object
            .get("dirtyRevision")
            .and_then(Value::as_u64)
            .ok_or_else(invalid_shape)?;
        let requires_save_as = object
            .get("requiresSaveAs")
            .and_then(Value::as_bool)
            .ok_or_else(invalid_shape)?;
        documents.push(SessionRecoveryDocument::new(
            decode_path(encoding, path)?,
            title,
            dirty_revision,
            requires_save_as,
        ));
    }
    let snapshot = SessionRecoverySnapshot::new(documents);
    validate_recovery_snapshot(&snapshot).map_err(|error| {
        SessionManifestError::Corruption(SessionManifestCorruptionError::InvalidSnapshot(error))
    })?;
    Ok(snapshot)
}

fn decode_restart_view(value: &Value) -> Result<RestartView, SessionManifestError> {
    let object = value.as_object().ok_or_else(invalid_shape)?;
    require_exact_fields(
        object.keys().map(String::as_str),
        &[
            "currentPage",
            "mode",
            "zoom",
            "manualPercent",
            "scrollX",
            "scrollY",
        ],
    )?;
    let current_page = u32::try_from(
        object
            .get("currentPage")
            .and_then(Value::as_u64)
            .ok_or_else(invalid_shape)?,
    )
    .map_err(|_| invalid_shape())?;
    let mode = match object.get("mode").and_then(Value::as_str) {
        Some("continuous") => PageViewMode::Continuous,
        Some("singlePage") => PageViewMode::SinglePage,
        _ => return Err(invalid_shape()),
    };
    let manual_percent = match object.get("manualPercent") {
        Some(Value::Null) => None,
        Some(value) => Some(value.as_f64().ok_or_else(invalid_shape)? as f32),
        None => return Err(invalid_shape()),
    };
    let zoom = match (object.get("zoom").and_then(Value::as_str), manual_percent) {
        (Some("fitWidth"), None) => RestartZoom::FitWidth,
        (Some("fitPage"), None) => RestartZoom::FitPage,
        (Some("manual"), Some(percent)) => RestartZoom::Manual(percent),
        _ => return Err(invalid_shape()),
    };
    let scroll_x = object
        .get("scrollX")
        .and_then(Value::as_f64)
        .ok_or_else(invalid_shape)? as f32;
    let scroll_y = object
        .get("scrollY")
        .and_then(Value::as_f64)
        .ok_or_else(invalid_shape)? as f32;
    let view = RestartView::new(current_page, mode, zoom, scroll_x, scroll_y);
    if !view.is_finite() {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::InvalidSnapshot(
                SessionManifestValidationError::NonFiniteRestartView,
            ),
        ));
    }
    Ok(view)
}

fn require_exact_fields<'a>(
    actual: impl Iterator<Item = &'a str>,
    expected: &[&str],
) -> Result<(), SessionManifestError> {
    let actual = actual.collect::<HashSet<_>>();
    if actual.len() != expected.len() || expected.iter().any(|field| !actual.contains(field)) {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::UnknownField,
        ));
    }
    Ok(())
}

fn invalid_shape() -> SessionManifestError {
    SessionManifestError::Corruption(SessionManifestCorruptionError::InvalidShape)
}

#[cfg(unix)]
fn decode_path(encoding: &str, encoded: &str) -> Result<PathBuf, SessionManifestError> {
    use std::os::unix::ffi::OsStringExt as _;

    if encoding != "unix-bytes" {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::ForeignPathEncoding,
        ));
    }
    Ok(PathBuf::from(OsString::from_vec(hex_decode(encoded)?)))
}

#[cfg(windows)]
fn decode_path(encoding: &str, encoded: &str) -> Result<PathBuf, SessionManifestError> {
    use std::os::windows::ffi::OsStringExt as _;

    if encoding != "windows-utf16le" {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::ForeignPathEncoding,
        ));
    }
    let bytes = hex_decode(encoded)?;
    if bytes.len() % 2 != 0 {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::MalformedPathHex,
        ));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    Ok(PathBuf::from(OsString::from_wide(&units)))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_decode(encoded: &str) -> Result<Vec<u8>, SessionManifestError> {
    if encoded.len() % 2 != 0
        || !encoded
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(SessionManifestError::Corruption(
            SessionManifestCorruptionError::MalformedPathHex,
        ));
    }
    encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_nibble(pair[0]);
            let low = hex_nibble(pair[1]);
            Ok((high << 4) | low)
        })
        .collect()
}

fn hex_nibble(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        _ => unreachable!("hex was validated before decoding"),
    }
}

fn ascii_suffix_is_pdf(bytes: &[u8]) -> bool {
    bytes.len() >= 4
        && bytes[bytes.len() - 4..]
            .iter()
            .zip(b".pdf")
            .all(|(actual, expected)| actual.to_ascii_lowercase() == *expected)
}

#[cfg(windows)]
fn wide_suffix_is_pdf(units: &[u16]) -> bool {
    units.len() >= 4
        && units[units.len() - 4..]
            .iter()
            .zip([b'.' as u16, b'p' as u16, b'd' as u16, b'f' as u16])
            .all(|(actual, expected)| match *actual {
                0x41..=0x5a => *actual + 0x20 == expected,
                _ => *actual == expected,
            })
}

#[cfg(unix)]
fn publish(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn publish(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

struct TemporaryGuard {
    path: Option<PathBuf>,
}

impl TemporaryGuard {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for TemporaryGuard {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}
