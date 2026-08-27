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
        if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
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

        #[cfg(not(unix))]
        {
            let _ = (parent_path, leaf);
            return Err(SaveTargetError::new(
                SaveTargetErrorKind::UnsupportedPlatform,
                target_path,
                "native directory authority is not implemented on this platform",
            ));
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

    #[cfg(unix)]
    fn revalidate_parent(&self) -> Result<(), SaveTargetError> {
        validate_parent_path(&self.parent_path, self.parent_identity, &self.target_path)
    }
}

#[cfg(unix)]
pub(crate) struct AuthorizedPdfStage {
    target_path: PathBuf,
    parent_path: PathBuf,
    target_leaf: OsString,
    parent_identity: (u64, u64),
    parent_fd: OwnedFd,
    stage_leaf: OsString,
    stage_identity: (u64, u64),
    stage_path: PathBuf,
    file: Option<File>,
    published: bool,
}

#[cfg(unix)]
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
