use std::{
    error::Error,
    ffi::{OsStr, OsString},
    fmt, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

#[cfg(unix)]
use rustix::{
    fs::{self as rfs, AtFlags, Mode, OFlags},
    io::{self as rio, Errno},
};
#[cfg(unix)]
use std::{fs::File, os::fd::OwnedFd};
#[cfg(windows)]
use std::{
    fs::File,
    os::windows::{
        ffi::OsStrExt as _,
        fs::MetadataExt as _,
        io::{AsRawHandle, FromRawHandle as _, IntoRawHandle as _, OwnedHandle, RawHandle},
    },
    sync::Arc,
};
#[cfg(windows)]
use windows_sys::Wdk::{
    Foundation::OBJECT_ATTRIBUTES,
    Storage::FileSystem::{
        FILE_CREATE, FILE_NON_DIRECTORY_FILE, FILE_OPEN_REPARSE_POINT, FILE_RENAME_INFORMATION,
        FILE_SYNCHRONOUS_IO_NONALERT, FileRenameInformation, NtCreateFile, NtOpenFile,
        NtSetInformationFile,
    },
};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{
        HANDLE, INVALID_HANDLE_VALUE, OBJ_CASE_INSENSITIVE, STATUS_OBJECT_NAME_COLLISION,
        STATUS_OBJECT_NAME_NOT_FOUND, STATUS_OBJECT_PATH_NOT_FOUND, UNICODE_STRING,
    },
    Storage::FileSystem::{
        CreateFileW, DELETE, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO, FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS,
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, FILE_WRITE_DATA,
        FileAttributeTagInfo, FileDispositionInfo, FileIdInfo, GetFileInformationByHandleEx,
        OPEN_EXISTING, SYNCHRONIZE, SetFileInformationByHandle,
    },
    System::IO::IO_STATUS_BLOCK,
};

static NEXT_STAGE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveTargetErrorKind {
    NotAbsolute,
    MissingParent,
    MissingFileName,
    NotPdf,
    SameAsSource,
    UnsafeParent,
    ParentChanged,
    StageChanged,
    TargetExists,
    AlreadyConsumed,
    UnsupportedPlatform,
    Io,
}

#[derive(Debug)]
pub struct SaveTargetError {
    kind: SaveTargetErrorKind,
    path: PathBuf,
    detail: String,
}

impl SaveTargetError {
    fn new(kind: SaveTargetErrorKind, path: PathBuf, detail: impl Into<String>) -> Self {
        Self {
            kind,
            path,
            detail: detail.into(),
        }
    }

    pub const fn kind(&self) -> SaveTargetErrorKind {
        self.kind
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl fmt::Display for SaveTargetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "PDF save target {} is unavailable: {}",
            self.path.display(),
            self.detail
        )
    }
}

impl Error for SaveTargetError {}

/// Move-only authority for one exact, new PDF destination.
///
/// A picker path is ambient authority. Binding immediately opens and retains
/// the selected parent directory, freezes its identity, preserves the native
/// file name, and proves the leaf is absent. The authority can prepare one
/// staged publication only and deliberately does not implement `Clone`.
pub struct SaveAsTargetAuthority {
    target_path: PathBuf,
    parent_path: PathBuf,
    leaf: OsString,
    consumed: AtomicBool,
    #[cfg(unix)]
    parent_fd: OwnedFd,
    #[cfg(unix)]
    parent_identity: (u64, u64),
    #[cfg(windows)]
    parent_handle: Arc<OwnedHandle>,
    #[cfg(windows)]
    parent_identity: WindowsFileIdentity,
    #[cfg(windows)]
    selected_parent_path: PathBuf,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowsFileIdentity {
    volume_serial_number: u64,
    file_id: [u8; 16],
}

#[cfg(windows)]
struct WindowsCreatedStage(Option<OwnedHandle>);

#[cfg(windows)]
impl WindowsCreatedStage {
    fn handle(&self) -> &OwnedHandle {
        self.0.as_ref().expect("a new stage retains its handle")
    }

    fn into_file(mut self) -> File {
        let handle = self.0.take().expect("a new stage retains its handle");
        unsafe { File::from_raw_handle(handle.into_raw_handle()) }
    }
}

#[cfg(windows)]
impl Drop for WindowsCreatedStage {
    fn drop(&mut self) {
        if let Some(handle) = self.0.as_ref()
            && let Err(error) = mark_windows_delete_on_close(handle)
        {
            windows_cleanup_diagnostic(&format!("new stage cleanup failed: {error}"));
        }
    }
}

impl fmt::Debug for SaveAsTargetAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SaveAsTargetAuthority")
            .field("target_path", &self.target_path)
            .field("parent_path", &self.parent_path)
            .field("leaf", &self.leaf)
            .field("consumed", &self.consumed.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

impl SaveAsTargetAuthority {
    pub fn bind(selected: PathBuf, source_path: &Path) -> Result<Self, SaveTargetError> {
        if !selected.is_absolute() {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::NotAbsolute,
                selected,
                "the selected path is not absolute",
            ));
        }
        let parent = selected.parent().ok_or_else(|| {
            SaveTargetError::new(
                SaveTargetErrorKind::MissingParent,
                selected.clone(),
                "the selected path has no parent directory",
            )
        })?;
        let leaf = selected.file_name().ok_or_else(|| {
            SaveTargetError::new(
                SaveTargetErrorKind::MissingFileName,
                selected.clone(),
                "the selected path has no file name",
            )
        })?;
        if selected.file_stem().is_none_or(|stem| stem.is_empty())
            || selected
                .extension()
                .is_none_or(|extension| !ascii_eq_ignore_case(extension, b"pdf"))
        {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::NotPdf,
                selected,
                "the selected file name must end in .pdf",
            ));
        }
        let parent_metadata = fs::symlink_metadata(parent).map_err(|error| {
            SaveTargetError::new(
                SaveTargetErrorKind::UnsafeParent,
                selected.clone(),
                format!("the selected parent cannot be inspected: {error}"),
            )
        })?;
        #[cfg(windows)]
        let parent_is_reparse =
            parent_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
        #[cfg(not(windows))]
        let parent_is_reparse = false;
        if !parent_metadata.is_dir()
            || parent_metadata.file_type().is_symlink()
            || parent_is_reparse
        {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::UnsafeParent,
                selected,
                "the selected parent is not a direct directory",
            ));
        }
        let parent_path = fs::canonicalize(parent).map_err(|error| {
            SaveTargetError::new(
                SaveTargetErrorKind::UnsafeParent,
                selected.clone(),
                format!("the selected parent cannot be resolved: {error}"),
            )
        })?;
        let target_path = parent_path.join(leaf);
        if fs::canonicalize(source_path)
            .ok()
            .is_some_and(|source| source == target_path)
        {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::SameAsSource,
                target_path,
                "Save As cannot replace the opened source",
            ));
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = (parent_path, leaf);
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::UnsupportedPlatform,
                target_path,
                "native directory authority is not implemented on this platform",
            ));
        }

        #[cfg(windows)]
        {
            let selected_parent_path = parent.to_path_buf();
            let parent_handle = open_windows_parent(&selected_parent_path, &target_path)?;
            let parent_identity =
                windows_file_identity(&parent_handle, &target_path, "selected parent")?;
            ensure_windows_not_reparse(
                &parent_handle,
                &target_path,
                SaveTargetErrorKind::UnsafeParent,
                "selected parent",
            )?;
            validate_windows_parent_path(
                &selected_parent_path,
                parent_identity,
                &target_path,
                SaveTargetErrorKind::UnsafeParent,
            )?;
            ensure_leaf_absent_windows(&parent_handle, leaf, &target_path)?;
            Ok(Self {
                target_path,
                parent_path,
                leaf: leaf.to_owned(),
                consumed: AtomicBool::new(false),
                parent_handle: Arc::new(parent_handle),
                parent_identity,
                selected_parent_path,
            })
        }

        #[cfg(unix)]
        {
            let parent_fd = rfs::open(
                &parent_path,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|error| target_io_error(&target_path, "open selected parent", error))?;
            let parent_stat = rfs::fstat(&parent_fd)
                .map_err(|error| target_io_error(&target_path, "inspect selected parent", error))?;
            ensure_leaf_absent(&parent_fd, leaf, &target_path)?;
            Ok(Self {
                target_path,
                parent_path,
                leaf: leaf.to_owned(),
                consumed: AtomicBool::new(false),
                parent_fd,
                parent_identity: (parent_stat.st_dev as u64, parent_stat.st_ino as u64),
            })
        }
    }

    pub fn path(&self) -> &Path {
        &self.target_path
    }

    #[cfg(unix)]
    pub(crate) fn prepare_stage(&self) -> Result<AuthorizedPdfStage, SaveTargetError> {
        if self
            .consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::AlreadyConsumed,
                self.target_path.clone(),
                "the one-shot target authority was already consumed",
            ));
        }
        self.revalidate_parent()?;
        ensure_leaf_absent(&self.parent_fd, &self.leaf, &self.target_path)?;
        let parent_fd = rio::dup(&self.parent_fd).map_err(|error| {
            target_io_error(&self.target_path, "duplicate parent authority", error)
        })?;
        let stage_leaf = OsString::from(format!(
            ".butter-paper-{}-{}.tmp",
            std::process::id(),
            NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let stage_fd = rfs::openat(
            &parent_fd,
            &stage_leaf,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|error| target_io_error(&self.target_path, "create exclusive PDF stage", error))?;
        let stage_stat = rfs::fstat(&stage_fd)
            .map_err(|error| target_io_error(&self.target_path, "inspect PDF stage", error))?;
        let stage_path = self.parent_path.join(&stage_leaf);
        Ok(AuthorizedPdfStage {
            target_path: self.target_path.clone(),
            parent_path: self.parent_path.clone(),
            target_leaf: self.leaf.clone(),
            parent_identity: self.parent_identity,
            parent_fd,
            stage_leaf,
            stage_identity: (stage_stat.st_dev as u64, stage_stat.st_ino as u64),
            stage_path,
            file: Some(stage_fd.into()),
            published: false,
        })
    }

    #[cfg(windows)]
    pub(crate) fn prepare_stage(&self) -> Result<AuthorizedPdfStage, SaveTargetError> {
        if self
            .consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::AlreadyConsumed,
                self.target_path.clone(),
                "the one-shot target authority was already consumed",
            ));
        }
        validate_windows_identity(
            self.parent_handle.as_ref(),
            self.parent_identity,
            &self.target_path,
            SaveTargetErrorKind::ParentChanged,
            "selected parent",
        )?;
        validate_windows_parent_path(
            &self.selected_parent_path,
            self.parent_identity,
            &self.target_path,
            SaveTargetErrorKind::ParentChanged,
        )?;
        ensure_windows_not_reparse(
            self.parent_handle.as_ref(),
            &self.target_path,
            SaveTargetErrorKind::ParentChanged,
            "selected parent",
        )?;
        ensure_leaf_absent_windows(self.parent_handle.as_ref(), &self.leaf, &self.target_path)?;
        let stage_leaf = OsString::from(format!(
            ".butter-paper-{}-{}.tmp",
            std::process::id(),
            NEXT_STAGE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let stage_handle = WindowsCreatedStage(Some(create_windows_stage(
            self.parent_handle.as_ref(),
            &stage_leaf,
            &self.target_path,
        )?));
        ensure_windows_not_reparse(
            stage_handle.handle(),
            &self.target_path,
            SaveTargetErrorKind::StageChanged,
            "PDF stage",
        )?;
        let stage_identity =
            windows_file_identity(stage_handle.handle(), &self.target_path, "PDF stage")?;
        let stage_path = self.parent_path.join(&stage_leaf);
        let file = stage_handle.into_file();
        Ok(AuthorizedPdfStage {
            target_path: self.target_path.clone(),
            target_leaf: self.leaf.clone(),
            parent_identity: self.parent_identity,
            parent_handle: Arc::clone(&self.parent_handle),
            selected_parent_path: self.selected_parent_path.clone(),
            stage_leaf,
            stage_identity,
            stage_path,
            file: Some(file),
            published: false,
        })
    }

    #[cfg(unix)]
    fn revalidate_parent(&self) -> Result<(), SaveTargetError> {
        validate_parent_path(&self.parent_path, self.parent_identity, &self.target_path)
    }
}

#[cfg(any(unix, windows))]
pub(crate) struct AuthorizedPdfStage {
    target_path: PathBuf,
    #[cfg(unix)]
    parent_path: PathBuf,
    target_leaf: OsString,
    #[cfg(unix)]
    parent_identity: (u64, u64),
    #[cfg(unix)]
    parent_fd: OwnedFd,
    #[cfg(windows)]
    parent_identity: WindowsFileIdentity,
    #[cfg(windows)]
    parent_handle: Arc<OwnedHandle>,
    #[cfg(windows)]
    selected_parent_path: PathBuf,
    stage_leaf: OsString,
    #[cfg(unix)]
    stage_identity: (u64, u64),
    #[cfg(windows)]
    stage_identity: WindowsFileIdentity,
    stage_path: PathBuf,
    file: Option<File>,
    published: bool,
}

#[cfg(any(unix, windows))]
impl AuthorizedPdfStage {
    pub(crate) fn path(&self) -> &Path {
        &self.stage_path
    }

    pub(crate) fn target_path(&self) -> &Path {
        &self.target_path
    }

    pub(crate) fn file_mut(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("an unpublished stage owns its file")
    }

    #[cfg(unix)]
    pub(crate) fn publish(mut self) -> Result<Vec<String>, SaveTargetError> {
        validate_parent_path(&self.parent_path, self.parent_identity, &self.target_path)?;
        self.revalidate_stage()?;
        ensure_leaf_absent(&self.parent_fd, &self.target_leaf, &self.target_path)?;
        self.file.take();
        rfs::linkat(
            &self.parent_fd,
            &self.stage_leaf,
            &self.parent_fd,
            &self.target_leaf,
            AtFlags::empty(),
        )
        .map_err(|error| {
            if error == Errno::EXIST {
                SaveTargetError::new(
                    SaveTargetErrorKind::TargetExists,
                    self.target_path.clone(),
                    "another file claimed the selected destination",
                )
            } else {
                target_io_error(&self.target_path, "publish PDF without replacement", error)
            }
        })?;
        if let Err(error) =
            validate_parent_path(&self.parent_path, self.parent_identity, &self.target_path)
        {
            let _ = rfs::unlinkat(&self.parent_fd, &self.target_leaf, AtFlags::empty());
            return Err(error);
        }
        self.published = true;
        let mut warnings = Vec::new();
        if let Err(error) = rfs::unlinkat(&self.parent_fd, &self.stage_leaf, AtFlags::empty()) {
            warnings.push(format!(
                "saved PDF was published, but its staging name could not be removed: {error}"
            ));
        }
        if let Err(error) = rfs::fsync(&self.parent_fd) {
            warnings.push(format!(
                "saved PDF was published, but its directory durability sync failed: {error}"
            ));
        }
        Ok(warnings)
    }

    #[cfg(windows)]
    pub(crate) fn publish(mut self) -> Result<Vec<String>, SaveTargetError> {
        validate_windows_parent_path(
            &self.selected_parent_path,
            self.parent_identity,
            &self.target_path,
            SaveTargetErrorKind::ParentChanged,
        )?;
        validate_windows_identity(
            self.parent_handle.as_ref(),
            self.parent_identity,
            &self.target_path,
            SaveTargetErrorKind::ParentChanged,
            "selected parent",
        )?;
        self.revalidate_stage_windows()?;
        ensure_leaf_absent_windows(
            self.parent_handle.as_ref(),
            &self.target_leaf,
            &self.target_path,
        )?;
        let file = self.file.as_ref().ok_or_else(|| {
            SaveTargetError::new(
                SaveTargetErrorKind::StageChanged,
                self.target_path.clone(),
                "the prepared staging file is no longer open",
            )
        })?;
        validate_windows_parent_path(
            &self.selected_parent_path,
            self.parent_identity,
            &self.target_path,
            SaveTargetErrorKind::ParentChanged,
        )?;
        rename_windows_handle_relative(
            file,
            self.parent_handle.as_ref(),
            &self.target_leaf,
            &self.target_path,
        )?;
        // The retained parent handle denies delete sharing for its entire
        // lifetime. A successful root-relative rename is therefore the
        // definitive commit: the selected parent path cannot be renamed or
        // deleted between the final validation above and this operation.
        self.published = true;
        self.file.take();
        Ok(Vec::new())
    }

    #[cfg(unix)]
    fn revalidate_stage(&self) -> Result<(), SaveTargetError> {
        let current = rfs::statat(&self.parent_fd, &self.stage_leaf, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(|error| target_io_error(&self.target_path, "inspect PDF stage name", error))?;
        let open = self
            .file
            .as_ref()
            .ok_or_else(|| {
                SaveTargetError::new(
                    SaveTargetErrorKind::StageChanged,
                    self.target_path.clone(),
                    "the prepared staging file is no longer open",
                )
            })
            .and_then(|file| {
                rfs::fstat(file)
                    .map_err(|error| target_io_error(&self.target_path, "inspect PDF stage", error))
            })?;
        let current_identity = (current.st_dev as u64, current.st_ino as u64);
        let open_identity = (open.st_dev as u64, open.st_ino as u64);
        if current_identity != self.stage_identity || open_identity != self.stage_identity {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::StageChanged,
                self.target_path.clone(),
                "the prepared staging file changed before publication",
            ));
        }
        Ok(())
    }

    #[cfg(windows)]
    fn revalidate_stage_windows(&self) -> Result<(), SaveTargetError> {
        let file = self.file.as_ref().ok_or_else(|| {
            SaveTargetError::new(
                SaveTargetErrorKind::StageChanged,
                self.target_path.clone(),
                "the prepared staging file is no longer open",
            )
        })?;
        validate_windows_identity(
            file,
            self.stage_identity,
            &self.target_path,
            SaveTargetErrorKind::StageChanged,
            "PDF stage handle",
        )?;
        ensure_windows_not_reparse(
            file,
            &self.target_path,
            SaveTargetErrorKind::StageChanged,
            "PDF stage handle",
        )?;
        let current = open_windows_relative(
            self.parent_handle.as_ref(),
            &self.stage_leaf,
            &self.target_path,
            "inspect PDF stage name",
        )?
        .ok_or_else(|| {
            SaveTargetError::new(
                SaveTargetErrorKind::StageChanged,
                self.target_path.clone(),
                "the prepared staging name is unavailable",
            )
        })?;
        validate_windows_identity(
            &current,
            self.stage_identity,
            &self.target_path,
            SaveTargetErrorKind::StageChanged,
            "PDF stage name",
        )?;
        ensure_windows_not_reparse(
            &current,
            &self.target_path,
            SaveTargetErrorKind::StageChanged,
            "PDF stage name",
        )
    }
}

#[cfg(unix)]
impl Drop for AuthorizedPdfStage {
    fn drop(&mut self) {
        if !self.published {
            self.file.take();
            let current_identity =
                rfs::statat(&self.parent_fd, &self.stage_leaf, AtFlags::SYMLINK_NOFOLLOW)
                    .ok()
                    .map(|stat| (stat.st_dev as u64, stat.st_ino as u64));
            if current_identity == Some(self.stage_identity) {
                let _ = rfs::unlinkat(&self.parent_fd, &self.stage_leaf, AtFlags::empty());
            }
        }
    }
}

#[cfg(windows)]
impl Drop for AuthorizedPdfStage {
    fn drop(&mut self) {
        if self.published {
            return;
        }
        let Some(file) = self.file.as_ref() else {
            return;
        };
        if validate_windows_identity(
            file,
            self.stage_identity,
            &self.target_path,
            SaveTargetErrorKind::StageChanged,
            "PDF stage handle",
        )
        .is_err()
            || ensure_windows_not_reparse(
                file,
                &self.target_path,
                SaveTargetErrorKind::StageChanged,
                "PDF stage cleanup handle",
            )
            .is_err()
        {
            windows_cleanup_diagnostic("stage handle validation failed; cleanup skipped");
            return;
        }
        let current = open_windows_relative(
            self.parent_handle.as_ref(),
            &self.stage_leaf,
            &self.target_path,
            "inspect PDF stage name for cleanup",
        )
        .ok()
        .flatten();
        let Some(current) = current else {
            windows_cleanup_diagnostic("stage name validation failed; cleanup skipped");
            return;
        };
        if windows_file_identity(&current, &self.target_path, "PDF stage cleanup name").ok()
            != Some(self.stage_identity)
            || ensure_windows_not_reparse(
                &current,
                &self.target_path,
                SaveTargetErrorKind::StageChanged,
                "PDF stage cleanup name",
            )
            .is_err()
        {
            windows_cleanup_diagnostic("stage name identity changed; cleanup skipped");
            return;
        }
        if let Err(error) = mark_windows_delete_on_close(file) {
            windows_cleanup_diagnostic(&format!("stage cleanup failed: {error}"));
        }
    }
}

#[cfg(windows)]
fn windows_handle(handle: &impl AsRawHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

#[cfg(windows)]
fn open_windows_parent(
    parent_path: &Path,
    target_path: &Path,
) -> Result<OwnedHandle, SaveTargetError> {
    let mut path = parent_path.as_os_str().encode_wide().collect::<Vec<_>>();
    path.push(0);
    let handle = unsafe {
        CreateFileW(
            path.as_ptr(),
            FILE_LIST_DIRECTORY
                | FILE_WRITE_DATA
                | FILE_READ_ATTRIBUTES
                | FILE_TRAVERSE
                | SYNCHRONIZE,
            // Omitting FILE_SHARE_DELETE prevents any other handle from
            // acquiring the DELETE access required to rename or delete this
            // exact directory until every retained authority handle closes.
            // It does not restrict root-relative opens of child files.
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(SaveTargetError::new(
            SaveTargetErrorKind::UnsafeParent,
            target_path.to_path_buf(),
            format!(
                "the selected parent cannot be opened without reparse traversal: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

#[cfg(windows)]
fn validate_windows_parent_path(
    selected_parent_path: &Path,
    expected: WindowsFileIdentity,
    target_path: &Path,
    reparse_kind: SaveTargetErrorKind,
) -> Result<(), SaveTargetError> {
    let reopened = open_windows_parent(selected_parent_path, target_path).map_err(|error| {
        SaveTargetError::new(
            SaveTargetErrorKind::ParentChanged,
            target_path.to_path_buf(),
            format!("the exact selected parent path cannot be reopened: {error}"),
        )
    })?;
    ensure_windows_not_reparse(&reopened, target_path, reparse_kind, "selected parent path")?;
    validate_windows_identity(
        &reopened,
        expected,
        target_path,
        SaveTargetErrorKind::ParentChanged,
        "selected parent path",
    )
}

#[cfg(windows)]
fn mark_windows_delete_on_close(handle: &impl AsRawHandle) -> std::io::Result<()> {
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    let success = unsafe {
        SetFileInformationByHandle(
            windows_handle(handle),
            FileDispositionInfo,
            std::ptr::from_ref(&disposition).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if success == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn windows_cleanup_diagnostic(message: &str) {
    let bounded = message.chars().take(240).collect::<String>();
    eprintln!("Butter Paper Windows Save As cleanup warning: {bounded}");
}

#[cfg(windows)]
fn with_windows_relative_name<T>(
    parent: &impl AsRawHandle,
    leaf: &OsStr,
    target_path: &Path,
    operation: impl FnOnce(*const OBJECT_ATTRIBUTES) -> T,
) -> Result<T, SaveTargetError> {
    let mut name = leaf.encode_wide().collect::<Vec<_>>();
    let byte_length = name
        .len()
        .checked_mul(2)
        .filter(|length| *length <= u16::MAX as usize)
        .ok_or_else(|| {
            SaveTargetError::new(
                SaveTargetErrorKind::Io,
                target_path.to_path_buf(),
                "the selected file name is too long for a native relative open",
            )
        })?;
    let unicode = UNICODE_STRING {
        Length: byte_length as u16,
        MaximumLength: byte_length as u16,
        Buffer: name.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: windows_handle(parent),
        ObjectName: std::ptr::from_ref(&unicode),
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null(),
        SecurityQualityOfService: std::ptr::null(),
    };
    Ok(operation(std::ptr::from_ref(&attributes)))
}

#[cfg(windows)]
fn open_windows_relative(
    parent: &impl AsRawHandle,
    leaf: &OsStr,
    target_path: &Path,
    operation: &str,
) -> Result<Option<OwnedHandle>, SaveTargetError> {
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = IO_STATUS_BLOCK::default();
    let status = with_windows_relative_name(parent, leaf, target_path, |attributes| unsafe {
        NtOpenFile(
            &mut handle,
            FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            attributes,
            &mut io_status,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
        )
    })?;
    if status == STATUS_OBJECT_NAME_NOT_FOUND || status == STATUS_OBJECT_PATH_NOT_FOUND {
        return Ok(None);
    }
    if status < 0 {
        return Err(windows_nt_error(target_path, operation, status));
    }
    Ok(Some(unsafe {
        OwnedHandle::from_raw_handle(handle as RawHandle)
    }))
}

#[cfg(windows)]
fn create_windows_stage(
    parent: &impl AsRawHandle,
    stage_leaf: &OsStr,
    target_path: &Path,
) -> Result<OwnedHandle, SaveTargetError> {
    let mut handle: HANDLE = std::ptr::null_mut();
    let mut io_status = IO_STATUS_BLOCK::default();
    let status =
        with_windows_relative_name(parent, stage_leaf, target_path, |attributes| unsafe {
            NtCreateFile(
                &mut handle,
                FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | DELETE | SYNCHRONIZE,
                attributes,
                &mut io_status,
                std::ptr::null(),
                FILE_ATTRIBUTE_NORMAL,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_CREATE,
                FILE_NON_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
                std::ptr::null(),
                0,
            )
        })?;
    if status < 0 {
        if status == STATUS_OBJECT_NAME_COLLISION {
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::Io,
                target_path.to_path_buf(),
                "the exclusive PDF staging name already exists",
            ));
        }
        return Err(windows_nt_error(
            target_path,
            "create exclusive PDF stage",
            status,
        ));
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

#[cfg(windows)]
fn ensure_leaf_absent_windows(
    parent: &impl AsRawHandle,
    leaf: &OsStr,
    target_path: &Path,
) -> Result<(), SaveTargetError> {
    if open_windows_relative(parent, leaf, target_path, "inspect target leaf")?.is_some() {
        Err(SaveTargetError::new(
            SaveTargetErrorKind::TargetExists,
            target_path.to_path_buf(),
            "the selected destination already exists",
        ))
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn windows_file_identity(
    handle: &impl AsRawHandle,
    target_path: &Path,
    description: &str,
) -> Result<WindowsFileIdentity, SaveTargetError> {
    let mut identity = FILE_ID_INFO::default();
    let success = unsafe {
        GetFileInformationByHandleEx(
            windows_handle(handle),
            FileIdInfo,
            std::ptr::from_mut(&mut identity).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if success == 0 {
        return Err(SaveTargetError::new(
            SaveTargetErrorKind::Io,
            target_path.to_path_buf(),
            format!(
                "could not inspect {description} identity: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    Ok(WindowsFileIdentity {
        volume_serial_number: identity.VolumeSerialNumber,
        file_id: identity.FileId.Identifier,
    })
}

#[cfg(windows)]
fn validate_windows_identity(
    handle: &impl AsRawHandle,
    expected: WindowsFileIdentity,
    target_path: &Path,
    kind: SaveTargetErrorKind,
    description: &str,
) -> Result<(), SaveTargetError> {
    let current = windows_file_identity(handle, target_path, description)?;
    if current == expected {
        Ok(())
    } else {
        Err(SaveTargetError::new(
            kind,
            target_path.to_path_buf(),
            format!("the retained {description} identity changed"),
        ))
    }
}

#[cfg(windows)]
fn ensure_windows_not_reparse(
    handle: &impl AsRawHandle,
    target_path: &Path,
    kind: SaveTargetErrorKind,
    description: &str,
) -> Result<(), SaveTargetError> {
    let mut attributes = FILE_ATTRIBUTE_TAG_INFO::default();
    let success = unsafe {
        GetFileInformationByHandleEx(
            windows_handle(handle),
            FileAttributeTagInfo,
            std::ptr::from_mut(&mut attributes).cast(),
            std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    };
    if success == 0 {
        return Err(SaveTargetError::new(
            SaveTargetErrorKind::Io,
            target_path.to_path_buf(),
            format!(
                "could not inspect {description} reparse state: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    if attributes.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(SaveTargetError::new(
            kind,
            target_path.to_path_buf(),
            format!("the retained {description} is a reparse point"),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn rename_windows_handle_relative(
    file: &File,
    parent: &impl AsRawHandle,
    target_leaf: &OsStr,
    target_path: &Path,
) -> Result<(), SaveTargetError> {
    let name = target_leaf.encode_wide().collect::<Vec<_>>();
    let name_bytes = name.len().checked_mul(2).ok_or_else(|| {
        SaveTargetError::new(
            SaveTargetErrorKind::Io,
            target_path.to_path_buf(),
            "the selected target name is too long",
        )
    })?;
    let header_bytes = std::mem::offset_of!(FILE_RENAME_INFORMATION, FileName);
    let total_bytes = std::mem::size_of::<FILE_RENAME_INFORMATION>()
        .checked_add(name_bytes)
        .ok_or_else(|| {
            SaveTargetError::new(
                SaveTargetErrorKind::Io,
                target_path.to_path_buf(),
                "the selected target name is too long",
            )
        })?;
    let total_bytes_u32 = u32::try_from(total_bytes).map_err(|_| {
        SaveTargetError::new(
            SaveTargetErrorKind::Io,
            target_path.to_path_buf(),
            "the selected target name exceeds the native rename buffer limit",
        )
    })?;
    let name_bytes_u32 = u32::try_from(name_bytes).map_err(|_| {
        SaveTargetError::new(
            SaveTargetErrorKind::Io,
            target_path.to_path_buf(),
            "the selected target name exceeds the native rename buffer limit",
        )
    })?;
    let words = total_bytes.div_ceil(std::mem::size_of::<usize>());
    let mut storage = vec![0usize; words];
    let rename = storage.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
    unsafe {
        (*rename).Anonymous.ReplaceIfExists = false;
        (*rename).RootDirectory = windows_handle(parent);
        (*rename).FileNameLength = name_bytes_u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            storage.as_mut_ptr().cast::<u8>().add(header_bytes),
            name_bytes,
        );
    }
    let mut io_status = IO_STATUS_BLOCK::default();
    let status = unsafe {
        NtSetInformationFile(
            windows_handle(file),
            &mut io_status,
            storage.as_ptr().cast(),
            total_bytes_u32,
            FileRenameInformation,
        )
    };
    if status >= 0 {
        return Ok(());
    }
    if status == STATUS_OBJECT_NAME_COLLISION {
        Err(SaveTargetError::new(
            SaveTargetErrorKind::TargetExists,
            target_path.to_path_buf(),
            "another file claimed the selected destination",
        ))
    } else {
        Err(windows_nt_error(
            target_path,
            "publish PDF without replacement",
            status,
        ))
    }
}

#[cfg(windows)]
fn windows_nt_error(path: &Path, operation: &str, status: i32) -> SaveTargetError {
    if status == STATUS_OBJECT_NAME_COLLISION {
        return SaveTargetError::new(
            SaveTargetErrorKind::TargetExists,
            path.to_path_buf(),
            "another file claimed the selected destination",
        );
    }
    SaveTargetError::new(
        SaveTargetErrorKind::Io,
        path.to_path_buf(),
        format!("could not {operation}: NTSTATUS 0x{:08x}", status as u32),
    )
}

fn ascii_eq_ignore_case(value: &OsStr, expected: &[u8]) -> bool {
    value.as_encoded_bytes().eq_ignore_ascii_case(expected)
}

#[cfg(unix)]
fn ensure_leaf_absent(
    parent_fd: &OwnedFd,
    leaf: &OsStr,
    target_path: &Path,
) -> Result<(), SaveTargetError> {
    match rfs::statat(parent_fd, leaf, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(_) => Err(SaveTargetError::new(
            SaveTargetErrorKind::TargetExists,
            target_path.to_path_buf(),
            "the selected destination already exists",
        )),
        Err(error) if error == Errno::NOENT => Ok(()),
        Err(error) => Err(target_io_error(target_path, "inspect target leaf", error)),
    }
}

#[cfg(unix)]
fn validate_parent_path(
    parent_path: &Path,
    expected_identity: (u64, u64),
    target_path: &Path,
) -> Result<(), SaveTargetError> {
    use std::os::unix::fs::MetadataExt as _;

    let metadata = fs::symlink_metadata(parent_path).map_err(|error| {
        SaveTargetError::new(
            SaveTargetErrorKind::ParentChanged,
            target_path.to_path_buf(),
            format!("the selected parent is unavailable: {error}"),
        )
    })?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || (metadata.dev(), metadata.ino()) != expected_identity
        || fs::canonicalize(parent_path).ok().as_deref() != Some(parent_path)
    {
        return Err(SaveTargetError::new(
            SaveTargetErrorKind::ParentChanged,
            target_path.to_path_buf(),
            "the selected parent directory changed",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn target_io_error(path: &Path, operation: &str, error: Errno) -> SaveTargetError {
    SaveTargetError::new(
        SaveTargetErrorKind::Io,
        path.to_path_buf(),
        format!("could not {operation}: {error}"),
    )
}
