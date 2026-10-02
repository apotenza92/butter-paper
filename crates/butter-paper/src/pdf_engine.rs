//! Transport-neutral messages at the Butter Paper PDF engine seam.
//!
//! Version 1 starts with document opening only. `source_handle_id` identifies a
//! read-only file descriptor or Windows handle inherited out of band; it is not
//! a filesystem path or a PDFium handle. Page, text, render, cancellation, and
//! close messages require a later reviewed protocol version and compatibility
//! fixtures. Password transport is also deferred because this JSON slice cannot
//! guarantee bounded or zeroized secret storage. Renderer implementation types
//! must not cross this seam.

#[cfg(unix)]
use rustix::{
    fs::{self as rfs, AtFlags, FlockOperation, Mode, OFlags},
    io::Errno,
};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(unix)]
use std::{
    ffi::{OsStr, OsString},
    os::{fd::OwnedFd, unix::ffi::OsStrExt as _},
};

use lopdf::{
    Dictionary, Document, Encoding, Object, ObjectId, Stream, StringFormat, content::Content,
    dictionary,
};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

use crate::annotation_model::{
    built_in_scale_presets, cloud_curl_radius, measurement_line_layout, MeasurementLineLayout, LENGTH_LEADER_LENGTH_PT, DIMENSION_LEADER_EXTENSION_PT,
    Annotation, AnnotationError, ArcAnnotation, BlendMode, CalloutAnnotation, CalloutAppearance,
    CalloutDiskGeometry, CloudAnnotation, CloudAppearancePathCommand, CloudPlusAnnotation,
    CloudPlusAppearance, DecodedRgbaAsset, DimensionAnnotation, DimensionAppearance,
    EllipseAnnotation, ImageAnnotation, InkTool, LengthAnnotation, LengthCalibration, LineKind,
    MarkupId, MeasurementPathAnnotation, MeasurementPathKind, PageRotation, PageScale, PdfPoint,
    PdfRect, PenAnnotation, PenAppearance, RectangleAnnotation, RectangleAppearance,
    RedactAnnotation, RetainedAnnotationObstacle, ScalePrecision, ScalePrecisionMode, ScaleSource,
    ScaleUnit, SnapshotAnnotation, StraightLineAnnotation, StraightLineAppearance, StrokeStyle,
    TextAlignment, TextBoxAnnotation, TextBoxRichTextRun, TextBoxStyle, VertexPathAnnotation,
    VertexPathKind, ellipse_cubic_bezier_points, rectangle_world_corners,
    sample_cloud_appearance_path,
};
use crate::image_asset_decode::MAX_ENCODED_IMAGE_BYTES;
#[cfg(any(unix, windows))]
use crate::pdf_file_authority::AuthorizedPdfStage;
use crate::pdf_file_authority::{SaveAsTargetAuthority, SaveTargetError};
use crate::semantic_snapping::{PageGridDefinition, PageGridKind, PageGridSource};

mod straight_line_pdf;

pub const PDF_ENGINE_PROTOCOL_NAME: &str = "butter-paper-pdf-engine";
pub const PDF_ENGINE_PROTOCOL_VERSION: u16 = 1;
static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(1);

/// The immutable publication capability of the compiled persistence backend.
///
/// This is separate from document provenance. A generated document can require
/// a new target even when the platform supports replacement, while Windows
/// requires a new target for every save until verified in-place replacement is
/// implemented there.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InPlacePublicationCapability {
    VerifiedAtomicReplacement,
    NewTargetRequired,
}

/// A bounded document-writer slice behind the application-owned PDF seam.
///
/// The renderer decision remains PDFium-in-a-worker. This session proves only
/// that an existing PDF object graph can import and persist native rectangle
/// annotations without rebuilding untouched annotation dictionaries.
pub struct PdfPersistenceSession {
    source_path: PathBuf,
    source_guard: Option<SourceGuard>,
    document: Document,
    rectangles: Vec<RectangleAnnotation>,
    redacts: Vec<RedactAnnotation>,
    redact_native_identities: HashMap<MarkupId, RedactNativeIdentity>,
    ellipses: Vec<EllipseAnnotation>,
    ellipse_native_identities: HashMap<MarkupId, EllipseNativeIdentity>,
    arcs: Vec<ArcAnnotation>,
    arc_native_identities: HashMap<MarkupId, ArcNativeIdentity>,
    pens: Vec<PenAnnotation>,
    pen_native_identities: HashMap<MarkupId, PenNativeIdentity>,
    text_boxes: Vec<TextBoxAnnotation>,
    lengths: Vec<LengthAnnotation>,
    length_native_identities: HashMap<MarkupId, LengthNativeIdentity>,
    dimensions: Vec<DimensionAnnotation>,
    dimension_native_names: HashMap<MarkupId, String>,
    straight_lines: Vec<StraightLineAnnotation>,
    straight_line_native_identities: HashMap<MarkupId, StraightLineNativeIdentity>,
    vertex_paths: Vec<VertexPathAnnotation>,
    vertex_path_native_identities: HashMap<MarkupId, VertexPathNativeIdentity>,
    clouds: Vec<CloudAnnotation>,
    cloud_native_identities: HashMap<MarkupId, CloudNativeIdentity>,
    cloud_pluses: Vec<CloudPlusAnnotation>,
    cloud_plus_native_identities: HashMap<MarkupId, CloudPlusNativeIdentity>,
    callouts: Vec<CalloutAnnotation>,
    callout_native_identities: HashMap<MarkupId, CalloutNativeIdentity>,
    measurement_paths: Vec<MeasurementPathAnnotation>,
    measurement_path_native_identities: HashMap<MarkupId, MeasurementPathNativeIdentity>,
    images: Vec<ImageAnnotation>,
    image_native_names: HashMap<MarkupId, String>,
    snapshots: Vec<SnapshotAnnotation>,
    snapshot_native_names: HashMap<MarkupId, String>,
    /// Revu vector Snapshots, in document order, with their original Form.
    vector_snapshot_sources: Vec<(MarkupId, VectorSnapshotSource)>,
    annotation_order: Vec<MarkupId>,
    page_scales: Vec<PageScale>,
    original_page_scales: Vec<PageScale>,
    page_length_calibrations: BTreeMap<u32, LengthCalibration>,
    page_rotations: BTreeMap<u32, PageRotation>,
    original_page_rotations: BTreeMap<u32, PageRotation>,
    changed_page_rotations: std::collections::BTreeSet<u32>,
    untouched_annotations: Vec<UntouchedAnnotation>,
    retained_annotation_obstacles: Vec<RetainedAnnotationObstacle>,
}

pub struct PreparedPdfSave {
    temporary: PathBuf,
    target: PathBuf,
    replacement_guard: Option<SourceGuard>,
    #[cfg(any(unix, windows))]
    authorized_stage: Option<AuthorizedPdfStage>,
    #[cfg(any(unix, windows))]
    cleanup_owned_by_authority: bool,
    #[cfg(unix)]
    replacement_stage: Option<OwnedInPlaceStage>,
    published: bool,
}

#[cfg(unix)]
struct OwnedInPlaceStage {
    parent_fd: OwnedFd,
    stage_leaf: OsString,
    stage_identity: (u64, u64),
    file: Option<File>,
    receipt_leaf: OsString,
    receipt_identity: (u64, u64),
    receipt_lease: Option<File>,
    published: bool,
}

#[cfg(unix)]
const IN_PLACE_RECEIPT_VERSION: u64 = 1;
#[cfg(unix)]
const IN_PLACE_RECEIPT_PREFIX: &str = ".butter-paper-in-place-v1-";
#[cfg(unix)]
const IN_PLACE_RECEIPT_SUFFIX: &str = ".receipt";
#[cfg(unix)]
const MAX_IN_PLACE_RECEIPT_BYTES: u64 = 4096;

#[cfg(unix)]
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct InPlaceStageReceipt {
    version: u64,
    token: String,
    owner_pid: u32,
    parent_device: u64,
    parent_inode: u64,
    target_leaf: Vec<u8>,
    stage_leaf: String,
    stage_device: u64,
    stage_inode: u64,
}

#[cfg(unix)]
fn secure_stage_token() -> Result<String, PdfPersistenceError> {
    let mut bytes = [0_u8; 16];
    getrandom::fill(&mut bytes).map_err(|error| {
        PdfPersistenceError::InvalidDocument(format!(
            "secure stage ownership token is unavailable: {error}"
        ))
    })?;
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(HEX[(byte >> 4) as usize] as char);
        token.push(HEX[(byte & 0x0f) as usize] as char);
    }
    Ok(token)
}

#[cfg(unix)]
fn receipt_leaf_for_token(token: &str) -> OsString {
    OsString::from(format!(
        "{IN_PLACE_RECEIPT_PREFIX}{token}{IN_PLACE_RECEIPT_SUFFIX}"
    ))
}

#[cfg(unix)]
fn receipt_token_from_leaf(leaf: &OsStr) -> Option<&str> {
    let leaf = leaf.to_str()?;
    let token = leaf
        .strip_prefix(IN_PLACE_RECEIPT_PREFIX)?
        .strip_suffix(IN_PLACE_RECEIPT_SUFFIX)?;
    (token.len() == 32 && token.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(token)
}

#[cfg(unix)]
fn stat_identity(stat: &rfs::Stat) -> (u64, u64) {
    (stat.st_dev as u64, stat.st_ino as u64)
}

#[cfg(unix)]
fn stat_is_regular(stat: &rfs::Stat) -> bool {
    stat.st_mode & libc::S_IFMT == libc::S_IFREG
}

#[cfg(unix)]
fn stat_owned_by_current_user(stat: &rfs::Stat) -> bool {
    stat.st_uid == unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn unlink_exact_single_link(
    parent_fd: &OwnedFd,
    leaf: &OsStr,
    expected_identity: (u64, u64),
) -> Result<bool, std::io::Error> {
    let stat = match rfs::statat(parent_fd, leaf, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(Errno::NOENT) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if stat_identity(&stat) != expected_identity
        || !stat_is_regular(&stat)
        || !stat_owned_by_current_user(&stat)
        || stat.st_nlink != 1
    {
        return Ok(false);
    }
    rfs::unlinkat(parent_fd, leaf, AtFlags::empty()).map_err(std::io::Error::from)?;
    Ok(true)
}

#[cfg(unix)]
fn recover_abandoned_in_place_stage(
    parent_fd: &OwnedFd,
    parent_identity: (u64, u64),
    target_leaf: &OsStr,
) -> Result<(), PdfPersistenceError> {
    let mut directory = rfs::Dir::read_from(parent_fd).map_err(std::io::Error::from)?;
    while let Some(entry) = directory.next() {
        let entry = entry.map_err(std::io::Error::from)?;
        let receipt_leaf = OsStr::from_bytes(entry.file_name().to_bytes());
        let Some(name_token) = receipt_token_from_leaf(receipt_leaf) else {
            continue;
        };
        let receipt_fd = match rfs::openat(
            parent_fd,
            receipt_leaf,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(_) => continue,
        };
        let receipt_file: File = receipt_fd.into();
        let before = match rfs::fstat(&receipt_file) {
            Ok(stat) => stat,
            Err(_) => continue,
        };
        if !stat_is_regular(&before)
            || !stat_owned_by_current_user(&before)
            || before.st_nlink != 1
            || before.st_size < 0
            || before.st_size as u64 > MAX_IN_PLACE_RECEIPT_BYTES
        {
            continue;
        }
        match rfs::flock(&receipt_file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(error) if error == Errno::WOULDBLOCK || error == Errno::AGAIN => continue,
            Err(_) => continue,
        }
        let named = match rfs::statat(parent_fd, receipt_leaf, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => stat,
            Err(_) => continue,
        };
        let locked = match rfs::fstat(&receipt_file) {
            Ok(stat) => stat,
            Err(_) => continue,
        };
        let receipt_identity = stat_identity(&locked);
        if receipt_identity != stat_identity(&before)
            || receipt_identity != stat_identity(&named)
            || !stat_is_regular(&named)
            || !stat_owned_by_current_user(&named)
            || !stat_owned_by_current_user(&locked)
            || named.st_nlink != 1
            || locked.st_nlink != 1
        {
            continue;
        }
        let mut serialized = Vec::with_capacity(locked.st_size.max(0) as usize);
        let mut reader = (&receipt_file).take(MAX_IN_PLACE_RECEIPT_BYTES + 1);
        if reader.read_to_end(&mut serialized).is_err()
            || serialized.len() as u64 > MAX_IN_PLACE_RECEIPT_BYTES
        {
            continue;
        }
        let Ok(receipt) = serde_json::from_slice::<InPlaceStageReceipt>(&serialized) else {
            continue;
        };
        if receipt.version != IN_PLACE_RECEIPT_VERSION
            || receipt.token != name_token
            || (receipt.parent_device, receipt.parent_inode) != parent_identity
            || receipt.target_leaf.as_slice() != target_leaf.as_bytes()
            || receipt.stage_leaf.as_bytes().contains(&b'/')
            || receipt.stage_leaf.is_empty()
        {
            continue;
        }
        let stage_leaf = OsStr::new(&receipt.stage_leaf);
        let expected_stage_identity = (receipt.stage_device, receipt.stage_inode);
        let target_stat = rfs::statat(parent_fd, target_leaf, AtFlags::SYMLINK_NOFOLLOW).ok();
        let stage_stat = rfs::statat(parent_fd, stage_leaf, AtFlags::SYMLINK_NOFOLLOW).ok();

        if target_stat
            .as_ref()
            .is_some_and(|stat| stat_identity(stat) == expected_stage_identity)
        {
            // Rename already crossed the publication boundary. Only the exact,
            // single-linked receipt may be removed; the target is never cleanup.
            if stage_stat.is_none()
                && target_stat.as_ref().is_some_and(|stat| {
                    stat_is_regular(stat) && stat_owned_by_current_user(stat) && stat.st_nlink == 1
                })
            {
                if unlink_exact_single_link(parent_fd, receipt_leaf, receipt_identity)? {
                    rfs::fsync(parent_fd).map_err(std::io::Error::from)?;
                }
            }
            continue;
        }

        let Some(stage_stat) = stage_stat else {
            continue;
        };
        if stat_identity(&stage_stat) != expected_stage_identity
            || !stat_is_regular(&stage_stat)
            || !stat_owned_by_current_user(&stage_stat)
            || stage_stat.st_nlink != 1
        {
            continue;
        }
        let stage_fd = match rfs::openat(
            parent_fd,
            stage_leaf,
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(_) => continue,
        };
        let Ok(open_stage) = rfs::fstat(&stage_fd) else {
            continue;
        };
        let Ok(named_stage) = rfs::statat(parent_fd, stage_leaf, AtFlags::SYMLINK_NOFOLLOW) else {
            continue;
        };
        if stat_identity(&open_stage) != expected_stage_identity
            || stat_identity(&named_stage) != expected_stage_identity
            || !stat_is_regular(&open_stage)
            || !stat_owned_by_current_user(&open_stage)
            || !stat_owned_by_current_user(&named_stage)
            || open_stage.st_nlink != 1
            || named_stage.st_nlink != 1
        {
            continue;
        }
        if unlink_exact_single_link(parent_fd, stage_leaf, expected_stage_identity)? {
            let _ = unlink_exact_single_link(parent_fd, receipt_leaf, receipt_identity);
            rfs::fsync(parent_fd).map_err(std::io::Error::from)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
impl OwnedInPlaceStage {
    fn create(
        parent: &Path,
        stage_path: PathBuf,
        target_leaf: &OsStr,
        expected_parent_identity: (u64, u64),
    ) -> Result<Self, PdfPersistenceError> {
        let stage_leaf = stage_path
            .file_name()
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "in-place PDF stage must have a file name".into(),
                )
            })?
            .to_owned();
        let parent_fd = rfs::open(
            parent,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(std::io::Error::from)?;
        let parent_stat = rfs::fstat(&parent_fd).map_err(std::io::Error::from)?;
        if (parent_stat.st_dev as u64, parent_stat.st_ino as u64) != expected_parent_identity {
            return Err(PdfPersistenceError::InvalidDocument(
                "source PDF directory changed before stage creation".into(),
            ));
        }
        recover_abandoned_in_place_stage(&parent_fd, expected_parent_identity, target_leaf)?;
        let token = secure_stage_token()?;
        Self::create_with_token(
            parent_fd,
            stage_leaf,
            target_leaf,
            expected_parent_identity,
            token,
        )
    }

    fn create_with_token(
        parent_fd: OwnedFd,
        stage_leaf: OsString,
        target_leaf: &OsStr,
        parent_identity: (u64, u64),
        token: String,
    ) -> Result<Self, PdfPersistenceError> {
        let stage_leaf_string = stage_leaf
            .to_str()
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "in-place PDF stage name must be valid UTF-8".into(),
                )
            })?
            .to_owned();
        let receipt_leaf = receipt_leaf_for_token(&token);
        let pending_receipt_leaf =
            OsString::from(format!("{}{}.pending", IN_PLACE_RECEIPT_PREFIX, token));
        let receipt_fd = rfs::openat(
            &parent_fd,
            &pending_receipt_leaf,
            OFlags::RDWR | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::from_raw_mode(0o600),
        )
        .map_err(std::io::Error::from)?;
        let mut receipt_file: File = receipt_fd.into();
        rfs::flock(&receipt_file, FlockOperation::NonBlockingLockExclusive)
            .map_err(std::io::Error::from)?;
        let receipt_stat = rfs::fstat(&receipt_file).map_err(std::io::Error::from)?;
        let receipt_identity = stat_identity(&receipt_stat);
        let stage_fd = rfs::openat(
            &parent_fd,
            &stage_leaf,
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::from_raw_mode(0o600),
        )
        .map_err(|error| {
            let _ = unlink_exact_single_link(&parent_fd, &pending_receipt_leaf, receipt_identity);
            std::io::Error::from(error)
        })?;
        let stage_stat = rfs::fstat(&stage_fd).map_err(std::io::Error::from)?;
        let stage_identity = stat_identity(&stage_stat);
        let receipt = InPlaceStageReceipt {
            version: IN_PLACE_RECEIPT_VERSION,
            token,
            owner_pid: process::id(),
            parent_device: parent_identity.0,
            parent_inode: parent_identity.1,
            target_leaf: target_leaf.as_bytes().to_vec(),
            stage_leaf: stage_leaf_string,
            stage_device: stage_identity.0,
            stage_inode: stage_identity.1,
        };
        let create_result = (|| {
            let serialized = serde_json::to_vec(&receipt).map_err(|error| {
                PdfPersistenceError::InvalidDocument(format!(
                    "in-place PDF ownership receipt could not be encoded: {error}"
                ))
            })?;
            receipt_file.write_all(&serialized)?;
            receipt_file.flush()?;
            receipt_file.sync_all()?;
            rfs::linkat(
                &parent_fd,
                &pending_receipt_leaf,
                &parent_fd,
                &receipt_leaf,
                AtFlags::empty(),
            )
            .map_err(std::io::Error::from)?;
            rfs::unlinkat(&parent_fd, &pending_receipt_leaf, AtFlags::empty())
                .map_err(std::io::Error::from)?;
            rfs::fsync(&parent_fd).map_err(std::io::Error::from)?;
            Ok::<(), PdfPersistenceError>(())
        })();
        if let Err(error) = create_result {
            let _ = unlink_exact_single_link(&parent_fd, &stage_leaf, stage_identity);
            let _ = unlink_exact_single_link(&parent_fd, &receipt_leaf, receipt_identity);
            let _ = unlink_exact_single_link(&parent_fd, &pending_receipt_leaf, receipt_identity);
            return Err(error);
        }
        Ok(Self {
            parent_fd,
            stage_leaf,
            stage_identity,
            file: Some(stage_fd.into()),
            receipt_leaf,
            receipt_identity,
            receipt_lease: Some(receipt_file),
            published: false,
        })
    }

    fn file_mut(&mut self) -> &mut File {
        self.file
            .as_mut()
            .expect("an unpublished stage owns its file")
    }

    fn revalidate(&self) -> Result<(), PdfPersistenceError> {
        let named = rfs::statat(&self.parent_fd, &self.stage_leaf, AtFlags::SYMLINK_NOFOLLOW)
            .map_err(std::io::Error::from)?;
        let open = self
            .file
            .as_ref()
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument("in-place PDF stage is no longer open".into())
            })?
            .metadata()?;
        let named_identity = (named.st_dev as u64, named.st_ino as u64);
        let open_identity = (open.dev(), open.ino());
        if named_identity != self.stage_identity || open_identity != self.stage_identity {
            return Err(PdfPersistenceError::InvalidDocument(
                "in-place PDF stage changed before publication".into(),
            ));
        }
        Ok(())
    }

    fn publish_replacing(
        self,
        target_leaf: &std::ffi::OsStr,
        mode: u32,
    ) -> Result<Vec<String>, PdfPersistenceError> {
        self.publish_replacing_with(target_leaf, mode, File::sync_all, |parent_fd| {
            rfs::fsync(parent_fd).map_err(std::io::Error::from)
        })
    }

    fn publish_replacing_with(
        mut self,
        target_leaf: &std::ffi::OsStr,
        mode: u32,
        sync_stage: impl FnOnce(&File) -> Result<(), std::io::Error>,
        sync_parent: impl FnOnce(&OwnedFd) -> Result<(), std::io::Error>,
    ) -> Result<Vec<String>, PdfPersistenceError> {
        self.revalidate()?;
        let stage_file = self.file.as_ref().expect("a validated stage remains open");
        stage_file.set_permissions(fs::Permissions::from_mode(mode & 0o777))?;
        // Permission metadata is part of the staged replacement. Sync it after
        // chmod and before rename so a Durable receipt covers both bytes and
        // the source mode copied onto the replacement inode.
        sync_stage(stage_file)?;
        rfs::renameat(
            &self.parent_fd,
            &self.stage_leaf,
            &self.parent_fd,
            target_leaf,
        )
        .map_err(std::io::Error::from)?;
        self.published = true;
        self.file.take();
        let mut warnings = Vec::new();
        match unlink_exact_single_link(&self.parent_fd, &self.receipt_leaf, self.receipt_identity) {
            Ok(true) => {}
            Ok(false) => warnings.push(
                "saved PDF was published, but its ownership receipt changed and was preserved"
                    .to_owned(),
            ),
            Err(error) => warnings.push(format!(
                "saved PDF was published, but its ownership receipt could not be removed: {error}"
            )),
        }
        self.receipt_lease.take();
        match sync_parent(&self.parent_fd) {
            Ok(()) => {}
            Err(error) => warnings.push(format!(
                "saved PDF was published, but its directory durability sync failed: {error}"
            )),
        }
        Ok(warnings)
    }
}

#[cfg(unix)]
impl Drop for OwnedInPlaceStage {
    fn drop(&mut self) {
        if self.published {
            self.receipt_lease.take();
            return;
        }
        self.file.take();
        let _ = unlink_exact_single_link(&self.parent_fd, &self.stage_leaf, self.stage_identity);
        let _ =
            unlink_exact_single_link(&self.parent_fd, &self.receipt_leaf, self.receipt_identity);
        self.receipt_lease.take();
    }
}

/// The observable result after a staged PDF crosses the publication boundary.
///
/// `PublishedWithWarning` means the destination already names the complete,
/// synced file, but a post-publication cleanup or parent-directory durability
/// operation failed. Callers must not report that case as an unpublished save.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PdfPublicationOutcome {
    Durable,
    PublishedWithWarning { warning: String },
}

impl PdfPublicationOutcome {
    pub fn warning(&self) -> Option<&str> {
        match self {
            Self::Durable => None,
            Self::PublishedWithWarning { warning } => Some(warning),
        }
    }
}

fn publication_outcome(warnings: Vec<String>) -> PdfPublicationOutcome {
    if warnings.is_empty() {
        PdfPublicationOutcome::Durable
    } else {
        PdfPublicationOutcome::PublishedWithWarning {
            warning: warnings.join("; "),
        }
    }
}

impl PreparedPdfSave {
    pub fn path(&self) -> &Path {
        &self.temporary
    }

    pub fn publish(mut self) -> Result<PdfPublicationOutcome, PdfPersistenceError> {
        #[cfg(any(unix, windows))]
        if let Some(stage) = self.authorized_stage.take() {
            let warnings = stage.publish()?;
            self.published = true;
            return Ok(publication_outcome(warnings));
        }
        if self.target.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("refusing to replace existing PDF {}", self.target.display()),
            )
            .into());
        }
        let parent = self.target.parent().ok_or_else(|| {
            PdfPersistenceError::InvalidDocument("save target must have a parent directory".into())
        })?;
        fs::hard_link(&self.temporary, &self.target)?;
        self.published = true;
        let mut warnings = Vec::new();
        if let Err(error) = fs::remove_file(&self.temporary) {
            warnings.push(format!(
                "saved PDF was published, but its staging name could not be removed: {error}"
            ));
        }
        if let Err(error) = sync_parent_directory(parent) {
            warnings.push(format!(
                "saved PDF was published, but its directory durability sync failed: {error}"
            ));
        }
        Ok(publication_outcome(warnings))
    }

    /// Atomically replaces the verified regular source on Unix-like targets.
    ///
    /// The staged file has already been synced and independently reopened by
    /// the caller. This final boundary rechecks the source immediately before
    /// publication, copies its Unix permission bits, renames the staged inode,
    /// and syncs the parent directory. The temporary file remains owned by this
    /// value and is removed on every pre-publication failure.
    pub fn publish_replacing(mut self) -> Result<PdfPublicationOutcome, PdfPersistenceError> {
        #[cfg(not(unix))]
        {
            return Err(PdfPersistenceError::InvalidDocument(
                "atomic in-place PDF publication is not yet implemented on this platform".into(),
            ));
        }
        #[cfg(unix)]
        {
            let parent = self.target.parent().ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "save target must have a parent directory".into(),
                )
            })?;
            let guard = self.replacement_guard.as_ref().ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "in-place publication requires a verified source guard".into(),
                )
            })?;
            let verified = read_regular_file_snapshot(&self.target)?;
            if verified.sha256 != guard.sha256 || verified.identity != guard.identity {
                return Err(PdfPersistenceError::InvalidDocument(
                    "source PDF changed before in-place publication".into(),
                ));
            }
            let canonical_parent = fs::canonicalize(parent)?;
            let parent_metadata = fs::metadata(&canonical_parent)?;
            if canonical_parent != guard.canonical_parent
                || (parent_metadata.dev(), parent_metadata.ino()) != guard.parent_identity
            {
                return Err(PdfPersistenceError::InvalidDocument(
                    "source PDF directory changed before in-place publication".into(),
                ));
            }
            let target_leaf = self.target.file_name().ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "in-place save target must have a file name".into(),
                )
            })?;
            let warnings = self
                .replacement_stage
                .take()
                .ok_or_else(|| {
                    PdfPersistenceError::InvalidDocument(
                        "in-place publication lost its owned staging authority".into(),
                    )
                })?
                .publish_replacing(target_leaf, guard.mode)?;
            self.published = true;
            Ok(publication_outcome(warnings))
        }
    }
}

impl Drop for PreparedPdfSave {
    fn drop(&mut self) {
        if !self.published && {
            #[cfg(any(unix, windows))]
            {
                !self.cleanup_owned_by_authority
            }
            #[cfg(not(any(unix, windows)))]
            {
                true
            }
        } {
            fs::remove_file(&self.temporary).ok();
        }
    }
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StableRegularFileIdentity {
    device: u64,
    inode: u64,
    size: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
    changed_seconds: i64,
    changed_nanoseconds: i64,
}

#[cfg(unix)]
impl StableRegularFileIdentity {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.size(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
            changed_seconds: metadata.ctime(),
            changed_nanoseconds: metadata.ctime_nsec(),
        }
    }
}

struct RegularFileSnapshot {
    bytes: Vec<u8>,
    sha256: [u8; 32],
    #[cfg(unix)]
    identity: StableRegularFileIdentity,
    #[cfg(unix)]
    mode: u32,
}

#[derive(Clone)]
struct SourceGuard {
    sha256: [u8; 32],
    #[cfg(unix)]
    identity: StableRegularFileIdentity,
    #[cfg(unix)]
    mode: u32,
    #[cfg(unix)]
    canonical_parent: PathBuf,
    #[cfg(unix)]
    parent_identity: (u64, u64),
}

impl SourceGuard {
    fn capture(path: &Path, snapshot: &RegularFileSnapshot) -> Result<Self, PdfPersistenceError> {
        #[cfg(unix)]
        {
            let parent = path.parent().ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "source PDF must have a parent directory".into(),
                )
            })?;
            let canonical_parent = fs::canonicalize(parent)?;
            let metadata = fs::metadata(&canonical_parent)?;
            if !metadata.is_dir() {
                return Err(PdfPersistenceError::InvalidDocument(
                    "source PDF parent must be a directory".into(),
                ));
            }
            return Ok(Self {
                sha256: snapshot.sha256,
                identity: snapshot.identity,
                mode: snapshot.mode,
                canonical_parent,
                parent_identity: (metadata.dev(), metadata.ino()),
            });
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(Self {
                sha256: snapshot.sha256,
            })
        }
    }
}

fn read_regular_file_snapshot(path: &Path) -> Result<RegularFileSnapshot, PdfPersistenceError> {
    let path_before = fs::symlink_metadata(path)?;
    if path_before.file_type().is_symlink() || !path_before.is_file() {
        return Err(PdfPersistenceError::InvalidDocument(
            "PDF saving requires a regular, non-symlink source file".into(),
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options.open(path)?;
    let opened_before = file.metadata()?;
    if !opened_before.is_file() {
        return Err(PdfPersistenceError::InvalidDocument(
            "PDF saving requires a regular source file".into(),
        ));
    }
    #[cfg(unix)]
    if StableRegularFileIdentity::from_metadata(&path_before)
        != StableRegularFileIdentity::from_metadata(&opened_before)
    {
        return Err(PdfPersistenceError::InvalidDocument(
            "source PDF identity changed while it was opened".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(opened_before.len() as usize);
    file.read_to_end(&mut bytes)?;
    let opened_after = file.metadata()?;
    let path_after = fs::symlink_metadata(path)?;
    if path_after.file_type().is_symlink() || !path_after.is_file() {
        return Err(PdfPersistenceError::InvalidDocument(
            "source PDF identity changed while it was read".into(),
        ));
    }
    #[cfg(unix)]
    {
        let expected = StableRegularFileIdentity::from_metadata(&opened_before);
        if StableRegularFileIdentity::from_metadata(&opened_after) != expected
            || StableRegularFileIdentity::from_metadata(&path_after) != expected
        {
            return Err(PdfPersistenceError::InvalidDocument(
                "source PDF identity changed while it was read".into(),
            ));
        }
    }
    #[cfg(not(unix))]
    if opened_before.len() != opened_after.len()
        || opened_after.len() != path_after.len()
        || opened_before.modified().ok() != opened_after.modified().ok()
        || opened_after.modified().ok() != path_after.modified().ok()
    {
        return Err(PdfPersistenceError::InvalidDocument(
            "source PDF identity changed while it was read".into(),
        ));
    }
    Ok(RegularFileSnapshot {
        sha256: Sha256::digest(&bytes).into(),
        bytes,
        #[cfg(unix)]
        identity: StableRegularFileIdentity::from_metadata(&opened_after),
        #[cfg(unix)]
        mode: opened_after.mode(),
    })
}

pub fn regular_file_sha256(path: impl AsRef<Path>) -> Result<[u8; 32], PdfPersistenceError> {
    Ok(read_regular_file_snapshot(path.as_ref())?.sha256)
}

#[derive(Clone, Debug)]
struct PenNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct EllipseNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct RedactNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct ArcNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct StraightLineNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct LengthNativeIdentity {
    raw_name: Option<String>,
    object_id: ObjectId,
}

/// Opaque identity authority retained across staged Length validation.
///
/// Callers can carry this value from reconciliation to independent reopen
/// validation without learning lopdf object identities or ownership rules.
#[derive(Clone, Debug)]
pub struct LengthSaveExpectation {
    id: MarkupId,
    raw_name: Option<String>,
    canonical_managed: bool,
}

#[derive(Clone, Debug)]
struct VertexPathNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct CloudNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct CloudPlusNativeIdentity {
    cloud_raw_name: String,
    cloud_object_id: ObjectId,
    text_raw_name: String,
    text_object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct CalloutNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug)]
struct MeasurementPathNativeIdentity {
    raw_name: String,
    object_id: ObjectId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UntouchedAnnotation {
    pub name: String,
    pub subtype: String,
}

#[derive(Debug)]
pub enum PdfPersistenceError {
    Annotation(AnnotationError),
    InvalidDocument(String),
    SaveTarget(SaveTargetError),
    Io(std::io::Error),
    Pdf(lopdf::Error),
}

impl fmt::Display for PdfPersistenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Annotation(error) => error.fmt(formatter),
            Self::InvalidDocument(message) => write!(formatter, "invalid PDF document: {message}"),
            Self::SaveTarget(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
            Self::Pdf(error) => error.fmt(formatter),
        }
    }
}

impl Error for PdfPersistenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Annotation(error) => Some(error),
            Self::SaveTarget(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Pdf(error) => Some(error),
            Self::InvalidDocument(_) => None,
        }
    }
}

impl From<lopdf::Error> for PdfPersistenceError {
    fn from(error: lopdf::Error) -> Self {
        Self::Pdf(error)
    }
}

impl From<std::io::Error> for PdfPersistenceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<AnnotationError> for PdfPersistenceError {
    fn from(error: AnnotationError) -> Self {
        Self::Annotation(error)
    }
}

impl From<SaveTargetError> for PdfPersistenceError {
    fn from(error: SaveTargetError) -> Self {
        Self::SaveTarget(error)
    }
}

impl PdfPersistenceError {
    pub fn save_target_error(&self) -> Option<&SaveTargetError> {
        match self {
            Self::SaveTarget(error) => Some(error),
            _ => None,
        }
    }
}

impl PdfPersistenceSession {
    pub const fn in_place_publication_capability() -> InPlacePublicationCapability {
        #[cfg(unix)]
        {
            InPlacePublicationCapability::VerifiedAtomicReplacement
        }
        #[cfg(not(unix))]
        {
            InPlacePublicationCapability::NewTargetRequired
        }
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, PdfPersistenceError> {
        let source_path = path.as_ref().to_path_buf();
        let document = Document::load(&source_path)?;
        Self::from_document(source_path, document, None)
    }


    pub fn open_for_update(
        path: impl AsRef<Path>,
        expected_sha256: [u8; 32],
    ) -> Result<Self, PdfPersistenceError> {
        let requested_path = path.as_ref();
        if !requested_path.is_absolute() {
            return Err(PdfPersistenceError::InvalidDocument(
                "source PDF path must be absolute for update".into(),
            ));
        }
        let canonical_path = fs::canonicalize(requested_path)?;
        if canonical_path != requested_path {
            return Err(PdfPersistenceError::InvalidDocument(
                "source PDF path must be canonical for update".into(),
            ));
        }
        let snapshot = read_regular_file_snapshot(&canonical_path)?;
        if snapshot.sha256 != expected_sha256 {
            return Err(PdfPersistenceError::InvalidDocument(
                "source PDF changed after it was opened".into(),
            ));
        }
        let source_guard = SourceGuard::capture(&canonical_path, &snapshot)?;
        let document = Document::load_mem(&snapshot.bytes)?;
        Self::from_document(canonical_path, document, Some(source_guard))
    }

    /// Opens the exact base bytes already protected by a durable recovery
    /// authority for publication to a new target. This deliberately carries
    /// no replacement guard: callers may only use it with retained Save As
    /// target authority, never to replace the externally changed source.
    pub(crate) fn open_recovery_base_for_save_as(
        source_path: impl AsRef<Path>,
        base_pdf: &[u8],
        expected_sha256: [u8; 32],
    ) -> Result<Self, PdfPersistenceError> {
        let actual_sha256: [u8; 32] = Sha256::digest(base_pdf).into();
        if actual_sha256 != expected_sha256 {
            return Err(PdfPersistenceError::InvalidDocument(
                "recovery base PDF does not match its source digest".into(),
            ));
        }
        let document = Document::load_mem(base_pdf)?;
        Self::from_document(source_path.as_ref().to_path_buf(), document, None)
    }

    fn from_document(
        source_path: PathBuf,
        document: Document,
        source_guard: Option<SourceGuard>,
    ) -> Result<Self, PdfPersistenceError> {
        let page_scales = import_page_scales(&document);
        let page_length_calibrations = page_scales
            .iter()
            .filter_map(|scale| {
                LengthCalibration::from_page_scale(scale)
                    .ok()
                    .map(|calibration| (scale.page_index, calibration))
            })
            .collect();
        let imported = import_annotations(&document, &page_length_calibrations)?;
        let page_rotations = import_page_rotations(&document)?;
        let original_page_rotations = page_rotations.clone();
        Ok(Self {
            source_path,
            source_guard,
            document,
            rectangles: imported.rectangles,
            redacts: imported.redacts,
            redact_native_identities: imported.redact_native_identities,
            ellipses: imported.ellipses,
            ellipse_native_identities: imported.ellipse_native_identities,
            arcs: imported.arcs,
            arc_native_identities: imported.arc_native_identities,
            pens: imported.pens,
            pen_native_identities: imported.pen_native_identities,
            text_boxes: imported.text_boxes,
            lengths: imported.lengths,
            length_native_identities: imported.length_native_identities,
            dimensions: imported.dimensions,
            dimension_native_names: imported.dimension_native_names,
            straight_lines: imported.straight_lines,
            straight_line_native_identities: imported.straight_line_native_identities,
            vertex_paths: imported.vertex_paths,
            vertex_path_native_identities: imported.vertex_path_native_identities,
            clouds: imported.clouds,
            cloud_native_identities: imported.cloud_native_identities,
            cloud_pluses: imported.cloud_pluses,
            cloud_plus_native_identities: imported.cloud_plus_native_identities,
            callouts: imported.callouts,
            callout_native_identities: imported.callout_native_identities,
            measurement_paths: imported.measurement_paths,
            measurement_path_native_identities: imported.measurement_path_native_identities,
            images: imported.images,
            image_native_names: imported.image_native_names,
            snapshots: imported.snapshots,
            snapshot_native_names: imported.snapshot_native_names,
            vector_snapshot_sources: imported.vector_snapshot_sources,
            annotation_order: imported.annotation_order,
            original_page_scales: page_scales.clone(),
            page_scales,
            page_length_calibrations,
            page_rotations,
            original_page_rotations,
            changed_page_rotations: std::collections::BTreeSet::new(),
            untouched_annotations: imported.untouched,
            retained_annotation_obstacles: imported.retained_annotation_obstacles,
        })
    }

    pub fn page_count(&self) -> usize {
        self.document.get_pages().len()
    }

    pub fn page_grid_definition(&self) -> Option<PageGridDefinition> {
        import_page_grid_definition(&self.document)
    }

    pub fn rectangles(&self) -> &[RectangleAnnotation] {
        &self.rectangles
    }

    pub fn redacts(&self) -> &[RedactAnnotation] {
        &self.redacts
    }

    pub fn ellipses(&self) -> &[EllipseAnnotation] {
        &self.ellipses
    }

    pub fn arcs(&self) -> &[ArcAnnotation] {
        &self.arcs
    }

    pub fn pens(&self) -> &[PenAnnotation] {
        &self.pens
    }

    pub fn text_boxes(&self) -> &[TextBoxAnnotation] {
        &self.text_boxes
    }

    pub fn lengths(&self) -> &[LengthAnnotation] {
        &self.lengths
    }

    pub fn dimensions(&self) -> &[DimensionAnnotation] {
        &self.dimensions
    }

    pub fn straight_lines(&self) -> &[StraightLineAnnotation] {
        &self.straight_lines
    }

    pub fn vertex_paths(&self) -> &[VertexPathAnnotation] {
        &self.vertex_paths
    }

    pub fn clouds(&self) -> &[CloudAnnotation] {
        &self.clouds
    }

    pub fn cloud_pluses(&self) -> &[CloudPlusAnnotation] {
        &self.cloud_pluses
    }

    pub fn callouts(&self) -> &[CalloutAnnotation] {
        &self.callouts
    }

    pub fn measurement_paths(&self) -> &[MeasurementPathAnnotation] {
        &self.measurement_paths
    }

    pub fn images(&self) -> &[ImageAnnotation] {
        &self.images
    }

    pub fn snapshots(&self) -> &[SnapshotAnnotation] {
        &self.snapshots
    }

    pub fn annotation_order(&self) -> &[MarkupId] {
        &self.annotation_order
    }

    pub fn annotations_in_document_order(&self) -> Vec<Annotation> {
        self.annotation_order
            .iter()
            .filter_map(|id| self.annotation_by_id(id))
            .collect()
    }

    pub fn reorder_managed_annotations(
        &mut self,
        requested: &[MarkupId],
    ) -> Result<(), PdfPersistenceError> {
        let current = self
            .rectangles
            .iter()
            .map(|value| &value.id)
            .chain(self.redacts.iter().map(|value| &value.id))
            .chain(self.ellipses.iter().map(|value| &value.id))
            .chain(self.arcs.iter().map(|value| &value.id))
            .chain(self.pens.iter().map(|value| &value.id))
            .chain(self.text_boxes.iter().map(|value| &value.id))
            .chain(self.lengths.iter().map(|value| &value.id))
            .chain(self.dimensions.iter().map(|value| &value.id))
            .chain(self.straight_lines.iter().map(|value| &value.id))
            .chain(self.vertex_paths.iter().map(|value| &value.id))
            .chain(self.clouds.iter().map(|value| &value.id))
            .chain(self.cloud_pluses.iter().map(|value| &value.id))
            .chain(self.callouts.iter().map(|value| &value.id))
            .chain(self.measurement_paths.iter().map(|value| &value.id))
            .chain(self.images.iter().map(|value| &value.id))
            .chain(self.snapshots.iter().map(|value| &value.id))
            .cloned()
            .collect::<HashSet<_>>();
        let requested_set = requested.iter().cloned().collect::<HashSet<_>>();
        if requested.len() != requested_set.len() || requested_set != current {
            return Err(PdfPersistenceError::InvalidDocument(
                "managed annotation order must contain every current stable id exactly once".into(),
            ));
        }

        let mut requested_by_page = BTreeMap::<u32, Vec<Vec<ObjectId>>>::new();
        let mut previous_page = None;
        for id in requested {
            let (page_index, object_ids) = self.managed_annotation_objects(id)?;
            if previous_page.is_some_and(|previous| page_index < previous) {
                return Err(PdfPersistenceError::InvalidDocument(
                    "managed annotation order must be page-major".into(),
                ));
            }
            previous_page = Some(page_index);
            requested_by_page
                .entry(page_index)
                .or_default()
                .push(object_ids);
        }

        for (page_index, requested_objects) in requested_by_page {
            reorder_page_managed_annotation_references(
                &mut self.document,
                page_index,
                &requested_objects,
            )?;
        }
        self.annotation_order = requested.to_vec();
        Ok(())
    }

    fn annotation_by_id(&self, id: &MarkupId) -> Option<Annotation> {
        self.rectangles
            .iter()
            .find(|value| &value.id == id)
            .cloned()
            .map(Annotation::Rectangle)
            .or_else(|| {
                self.redacts
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Redact)
            })
            .or_else(|| {
                self.ellipses
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Ellipse)
            })
            .or_else(|| {
                self.arcs
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Arc)
            })
            .or_else(|| {
                self.straight_lines
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::StraightLine)
            })
            .or_else(|| {
                self.vertex_paths
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::VertexPath)
            })
            .or_else(|| {
                self.clouds
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Cloud)
            })
            .or_else(|| {
                self.cloud_pluses
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::CloudPlus)
            })
            .or_else(|| {
                self.callouts
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Callout)
            })
            .or_else(|| {
                self.measurement_paths
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::MeasurementPath)
            })
            .or_else(|| {
                self.pens
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Pen)
            })
            .or_else(|| {
                self.text_boxes
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::TextBox)
            })
            .or_else(|| {
                self.lengths
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Length)
            })
            .or_else(|| {
                self.dimensions
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Dimension)
            })
            .or_else(|| {
                self.images
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Image)
            })
            .or_else(|| {
                self.snapshots
                    .iter()
                    .find(|value| &value.id == id)
                    .cloned()
                    .map(Annotation::Snapshot)
            })
    }

    fn managed_annotation_objects(
        &self,
        id: &MarkupId,
    ) -> Result<(u32, Vec<ObjectId>), PdfPersistenceError> {
        if let Some(value) = self.rectangles.iter().find(|value| &value.id == id) {
            return Ok((
                value.page_index,
                vec![annotation_object_id(
                    &self.document,
                    value.page_index,
                    id.as_str(),
                )?],
            ));
        }
        if let Some(value) = self.redacts.iter().find(|value| &value.id == id) {
            let identity = self.redact_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "pending Redact {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.ellipses.iter().find(|value| &value.id == id) {
            let identity = self.ellipse_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "ellipse {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.arcs.iter().find(|value| &value.id == id) {
            let identity = self.arc_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "arc {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.straight_lines.iter().find(|value| &value.id == id) {
            let identity = self
                .straight_line_native_identities
                .get(id)
                .ok_or_else(|| {
                    PdfPersistenceError::InvalidDocument(format!(
                        "straight line {id} has no unambiguous native object identity"
                    ))
                })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.vertex_paths.iter().find(|value| &value.id == id) {
            let identity = self.vertex_path_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "vertex path {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.clouds.iter().find(|value| &value.id == id) {
            let identity = self.cloud_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "cloud {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.cloud_pluses.iter().find(|value| &value.id == id) {
            let identity = self.cloud_plus_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "Cloud+ {id} has no unambiguous paired native identity"
                ))
            })?;
            return Ok((
                value.page_index,
                vec![identity.cloud_object_id, identity.text_object_id],
            ));
        }
        if let Some(value) = self.callouts.iter().find(|value| &value.id == id) {
            let identity = self.callout_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "callout {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.measurement_paths.iter().find(|value| &value.id == id) {
            let identity = self
                .measurement_path_native_identities
                .get(id)
                .ok_or_else(|| {
                    PdfPersistenceError::InvalidDocument(format!(
                        "measurement path {id} has no unambiguous native object identity"
                    ))
                })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.pens.iter().find(|value| &value.id == id) {
            let identity = self.pen_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "ink {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.text_boxes.iter().find(|value| &value.id == id) {
            return Ok((
                value.page_index,
                vec![annotation_object_id(
                    &self.document,
                    value.page_index,
                    id.as_str(),
                )?],
            ));
        }
        if let Some(value) = self.lengths.iter().find(|value| &value.id == id) {
            let identity = self.length_native_identities.get(id).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "length {id} has no unambiguous native object identity"
                ))
            })?;
            return Ok((value.page_index, vec![identity.object_id]));
        }
        if let Some(value) = self.dimensions.iter().find(|value| &value.id == id) {
            let native_name = self
                .dimension_native_names
                .get(id)
                .map(String::as_str)
                .unwrap_or(id.as_str());
            return Ok((
                value.page_index,
                vec![annotation_object_id(
                    &self.document,
                    value.page_index,
                    native_name,
                )?],
            ));
        }
        if let Some(value) = self.images.iter().find(|value| &value.id == id) {
            let native_name = self
                .image_native_names
                .get(id)
                .map(String::as_str)
                .unwrap_or(id.as_str());
            return Ok((
                value.page_index,
                vec![annotation_object_id(
                    &self.document,
                    value.page_index,
                    native_name,
                )?],
            ));
        }
        if let Some(value) = self.snapshots.iter().find(|value| &value.id == id) {
            let native_name = self
                .snapshot_native_names
                .get(id)
                .map(String::as_str)
                .unwrap_or(id.as_str());
            return Ok((
                value.page_index,
                vec![annotation_object_id(
                    &self.document,
                    value.page_index,
                    native_name,
                )?],
            ));
        }
        Err(PdfPersistenceError::InvalidDocument(format!(
            "managed annotation {id} is missing"
        )))
    }

    pub fn page_length_calibrations(&self) -> &BTreeMap<u32, LengthCalibration> {
        &self.page_length_calibrations
    }

    pub fn page_scales(&self) -> &[PageScale] {
        &self.page_scales
    }

    pub fn page_rotations(&self) -> &BTreeMap<u32, PageRotation> {
        &self.page_rotations
    }

    pub fn page_rotation(&self, page_index: u32) -> Option<PageRotation> {
        self.page_rotations.get(&page_index).copied()
    }

    pub fn set_page_rotation(
        &mut self,
        page_index: u32,
        rotation: PageRotation,
    ) -> Result<(), PdfPersistenceError> {
        let page_number = page_index.checked_add(1).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "page rotation index exceeds the PDF page limit".into(),
            )
        })?;
        if !self.document.get_pages().contains_key(&page_number) {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "page rotation target {page_index} does not exist"
            )));
        }
        self.page_rotations.insert(page_index, rotation);
        if self.original_page_rotations.get(&page_index) == Some(&rotation) {
            self.changed_page_rotations.remove(&page_index);
        } else {
            self.changed_page_rotations.insert(page_index);
        }
        Ok(())
    }

    pub fn direct_page_rotation(&self, page_index: u32) -> Option<i64> {
        let page_number = page_index.checked_add(1)?;
        let page_id = *self.document.get_pages().get(&page_number)?;
        self.document
            .get_object(page_id)
            .ok()?
            .as_dict()
            .ok()?
            .get(b"Rotate")
            .ok()?
            .as_i64()
            .ok()
    }

    pub fn set_page_length_calibration(
        &mut self,
        page_index: u32,
        calibration: LengthCalibration,
    ) -> Result<(), PdfPersistenceError> {
        let page_number = page_index.checked_add(1).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "page scale index exceeds the PDF page limit".into(),
            )
        })?;
        if !self.document.get_pages().contains_key(&page_number) {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "page scale target {page_index} does not exist"
            )));
        }
        let real_units = ScaleUnit::parse(calibration.unit())
            .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
        let scale = PageScale::from_factors(
            page_index,
            ScaleSource::Calibrated,
            if calibration.label().is_empty() {
                format!(
                    "Calibrated {} {}",
                    calibration.real_world_value(),
                    calibration.unit()
                )
            } else {
                calibration.label().to_owned()
            },
            ScaleUnit::In,
            real_units,
            calibration.scale_x(),
            calibration.scale_y(),
            calibration.scale_precision(),
        )
        .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
        self.set_page_scale(scale)?;
        Ok(())
    }

    pub fn set_page_scale(&mut self, scale: PageScale) -> Result<(), PdfPersistenceError> {
        let page_number = scale.page_index.checked_add(1).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "page scale index exceeds the PDF page limit".into(),
            )
        })?;
        if !self.document.get_pages().contains_key(&page_number) {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "page scale target {} does not exist",
                scale.page_index
            )));
        }
        let calibration = LengthCalibration::from_page_scale(&scale)?;
        self.page_length_calibrations
            .insert(scale.page_index, calibration);
        if let Some(existing) = self
            .page_scales
            .iter_mut()
            .find(|candidate| candidate.page_index == scale.page_index)
        {
            *existing = scale;
        } else {
            self.page_scales.push(scale);
            self.page_scales.sort_by_key(|scale| scale.page_index);
        }
        Ok(())
    }

    /// Replaces the complete persisted page-scale set atomically.
    ///
    /// Save callers use this instead of repeated upserts so undoing the final
    /// scale removes the page's `/VP` viewport from the PDF.
    pub fn replace_page_scales(&mut self, scales: &[PageScale]) -> Result<(), PdfPersistenceError> {
        let pages = self.document.get_pages();
        let mut next_scales = scales.to_vec();
        next_scales.sort_by_key(|scale| scale.page_index);
        if next_scales
            .windows(2)
            .any(|pair| pair[0].page_index == pair[1].page_index)
        {
            return Err(PdfPersistenceError::InvalidDocument(
                "page scales contain duplicate page indexes".into(),
            ));
        }

        let mut next_calibrations = BTreeMap::new();
        for scale in &next_scales {
            let page_number = scale.page_index.checked_add(1).ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "page scale index exceeds the PDF page limit".into(),
                )
            })?;
            if !pages.contains_key(&page_number) {
                return Err(PdfPersistenceError::InvalidDocument(format!(
                    "page scale target {} does not exist",
                    scale.page_index
                )));
            }
            next_calibrations.insert(scale.page_index, LengthCalibration::from_page_scale(scale)?);
        }

        self.page_scales = next_scales;
        self.page_length_calibrations = next_calibrations;
        Ok(())
    }

    /// Revu vector Snapshots in the order of `vector_snapshot_layer` pages.
    pub fn vector_snapshot_ids(&self) -> Vec<MarkupId> {
        self.vector_snapshot_sources.iter().map(|(id, _)| id.clone()).collect()
    }

    pub fn untouched_annotations(&self) -> &[UntouchedAnnotation] {
        &self.untouched_annotations
    }

    pub fn retained_annotation_obstacles(&self) -> &[RetainedAnnotationObstacle] {
        &self.retained_annotation_obstacles
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn replace_rectangle(
        &mut self,
        replacement: RectangleAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let rectangle_index = self
            .rectangles
            .iter()
            .position(|rectangle| rectangle.id == replacement.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "rectangle {} is not imported from this document",
                    replacement.id,
                ))
            })?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.rectangles[rectangle_index].same_persisted_state_as(&replacement) {
            return Ok(());
        }
        let object_id = annotation_object_id(
            &self.document,
            replacement.page_index,
            replacement.id.as_str(),
        )?;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let appearance_id = add_rectangle_appearance(&mut self.document, &replacement);
        let replacement_dictionary = rectangle_dictionary(&replacement, appearance_id, &original)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(replacement_dictionary));
        self.rectangles[rectangle_index] = replacement;
        Ok(())
    }

    /// Removes one imported native Rectangle by stable annotation identity.
    ///
    /// The page `/Annots` entry is authoritative. Unrelated annotation
    /// objects and dictionaries are left untouched.
    pub fn remove_rectangle(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let rectangle_index =
            find_annotation_index(&self.rectangles, id, |rectangle| &rectangle.id, "rectangle")?;
        let page_index = self.rectangles[rectangle_index].page_index;
        let object_id = annotation_object_id(&self.document, page_index, id.as_str())?;
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        self.rectangles.remove(rectangle_index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    /// Adds a Butter Paper-managed pending ISO PDF `/Redact` annotation.
    ///
    /// This records only a pending mark. It never removes or rewrites page
    /// content, and the canonical dictionary deliberately has no `/AP`.
    pub fn add_redact(&mut self, annotation: RedactAnnotation) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let dictionary = redact_dictionary(&annotation, &Dictionary::new(), &native_name)?;
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.redact_native_identities.insert(
            annotation.id.clone(),
            RedactNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.redacts.push(annotation);
        Ok(())
    }

    pub fn replace_redact(
        &mut self,
        annotation: RedactAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(
            &self.redacts,
            &annotation.id,
            |value| &value.id,
            "pending Redact",
        )?;
        if self.redacts[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        if self.redacts[index].page_index != annotation.page_index {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "pending Redact {} cannot move between PDF pages",
                annotation.id
            )));
        }
        let identity = self
            .redact_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "pending Redact {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let native_name = canonical_native_annotation_name(&annotation.id);
        let dictionary = redact_dictionary(&annotation, &original, &native_name)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
        self.redact_native_identities.insert(
            annotation.id.clone(),
            RedactNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        self.redacts[index] = annotation;
        Ok(())
    }

    pub fn remove_redact(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.redacts, id, |value| &value.id, "pending Redact")?;
        let page_index = self.redacts[index].page_index;
        let identity = self.redact_native_identities.get(id).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "pending Redact {id} has no unambiguous native object identity"
            ))
        })?;
        remove_annotation_reference(&mut self.document, page_index, identity.object_id)?;
        self.document.objects.remove(&identity.object_id);
        self.redact_native_identities.remove(id);
        self.redacts.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    /// Adds a Butter Paper-managed native PDF `/Circle` annotation with a
    /// standard border style and normal appearance stream. Private metadata is
    /// retained only as a compatibility aid for older Butter Paper versions.
    pub fn add_ellipse(
        &mut self,
        annotation: EllipseAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_ellipse_appearance(&mut self.document, &annotation);
        let dictionary =
            ellipse_dictionary(&annotation, appearance_id, &Dictionary::new(), &native_name)?;
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.ellipse_native_identities.insert(
            annotation.id.clone(),
            EllipseNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.ellipses.push(annotation);
        Ok(())
    }

    pub fn replace_ellipse(
        &mut self,
        annotation: EllipseAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index =
            find_annotation_index(&self.ellipses, &annotation.id, |value| &value.id, "ellipse")?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.ellipses[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let identity = self
            .ellipse_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "ellipse {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_id = normal_appearance_object_id(&original);
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_ellipse_appearance(&mut self.document, &annotation);
        let dictionary = ellipse_dictionary(&annotation, appearance_id, &original, &native_name)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
        self.ellipse_native_identities.insert(
            annotation.id.clone(),
            EllipseNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        if let Some(old_appearance_id) = old_appearance_id {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        self.ellipses[index] = annotation;
        Ok(())
    }

    pub fn remove_ellipse(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.ellipses, id, |value| &value.id, "ellipse")?;
        let page_index = self.ellipses[index].page_index;
        let identity = self.ellipse_native_identities.get(id).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "ellipse {id} has no unambiguous native object identity"
            ))
        })?;
        let object_id = identity.object_id;
        let appearance_id = self
            .document
            .get_object(object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(normal_appearance_object_id);
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        if let Some(appearance_id) = appearance_id {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.ellipse_native_identities.remove(id);
        self.ellipses.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn add_arc(&mut self, annotation: ArcAnnotation) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_arc_appearance(&mut self.document, &annotation);
        let dictionary =
            arc_dictionary(&annotation, appearance_id, &Dictionary::new(), &native_name)?;
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.arc_native_identities.insert(
            annotation.id.clone(),
            ArcNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.arcs.push(annotation);
        Ok(())
    }

    pub fn replace_arc(&mut self, annotation: ArcAnnotation) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.arcs, &annotation.id, |value| &value.id, "arc")?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.arcs[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let identity = self
            .arc_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "arc {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_arc_appearance(&mut self.document, &annotation);
        let dictionary = arc_dictionary(&annotation, appearance_id, &original, &native_name)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
        self.arc_native_identities.insert(
            annotation.id.clone(),
            ArcNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        self.arcs[index] = annotation;
        Ok(())
    }

    pub fn remove_arc(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.arcs, id, |value| &value.id, "arc")?;
        let page_index = self.arcs[index].page_index;
        let identity = self.arc_native_identities.get(id).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "arc {id} has no unambiguous native object identity"
            ))
        })?;
        remove_annotation_reference(&mut self.document, page_index, identity.object_id)?;
        self.document.objects.remove(&identity.object_id);
        self.arc_native_identities.remove(id);
        self.arcs.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn has_raw_annotation_name(&self, id: &MarkupId) -> bool {
        self.document.objects.values().any(|object| {
            object.as_dict().ok().is_some_and(|dictionary| {
                dictionary_string(dictionary, b"NM").as_deref() == Some(id.as_str())
            })
        })
    }

    pub fn has_canonical_raw_annotation_name(&self, id: &MarkupId) -> bool {
        let canonical = canonical_native_annotation_name(id);
        self.document.objects.values().any(|object| {
            object.as_dict().ok().is_some_and(|dictionary| {
                dictionary_string(dictionary, b"NM").as_deref() == Some(canonical.as_str())
            })
        })
    }

    pub fn has_cloud_plus_native_fragment_names(&self, id: &MarkupId) -> bool {
        self.has_raw_annotation_name(id)
    }

    pub fn pen_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.pen_native_identities.get(id) else {
            return false;
        };
        identity.raw_name == canonical_native_annotation_name(id)
            && self
                .document
                .get_object(identity.object_id)
                .ok()
                .and_then(|object| object.as_dict().ok())
                .and_then(|dictionary| dictionary_string(dictionary, b"NM"))
                .as_deref()
                == Some(identity.raw_name.as_str())
    }

    pub fn ellipse_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.ellipse_native_identities.get(id) else {
            return false;
        };
        identity.raw_name == canonical_native_annotation_name(id)
            && self
                .document
                .get_object(identity.object_id)
                .ok()
                .and_then(|object| object.as_dict().ok())
                .is_some_and(|dictionary| {
                    dictionary_string(dictionary, b"NM").as_deref()
                        == Some(identity.raw_name.as_str())
                        && dictionary_name(dictionary, b"Subtype").as_deref() == Some("Circle")
                        && dictionary_name(dictionary, b"IT").is_none()
                        && dictionary.get(b"AP").is_ok()
                })
    }

    pub fn redact_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.redact_native_identities.get(id) else {
            return false;
        };
        identity.raw_name == canonical_native_annotation_name(id)
            && self
                .document
                .get_object(identity.object_id)
                .ok()
                .and_then(|object| object.as_dict().ok())
                .is_some_and(|dictionary| {
                    dictionary_name(dictionary, b"Subtype").as_deref() == Some("Redact")
                        && dictionary_string(dictionary, b"NM").as_deref()
                            == Some(identity.raw_name.as_str())
                        && dictionary.get(b"AP").is_err()
                })
    }

    pub fn arc_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.arc_native_identities.get(id) else {
            return false;
        };
        identity.raw_name == canonical_native_annotation_name(id)
            && self
                .document
                .get_object(identity.object_id)
                .ok()
                .and_then(|object| object.as_dict().ok())
                .is_some_and(|dictionary| {
                    dictionary_string(dictionary, b"NM").as_deref()
                        == Some(identity.raw_name.as_str())
                        && dictionary_name(dictionary, b"Subtype").as_deref() == Some("Circle")
                        && dictionary_name(dictionary, b"IT").as_deref() == Some("CircleArc")
                        && dictionary.get(b"AP").is_ok()
                })
    }

    pub fn image_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(raw_name) = self.image_native_names.get(id) else {
            return false;
        };
        if raw_name != &canonical_native_annotation_name(id) {
            return false;
        }
        self.images
            .iter()
            .find(|image| &image.id == id)
            .and_then(|image| annotation_object_id(&self.document, image.page_index, raw_name).ok())
            .and_then(|object_id| self.document.get_object(object_id).ok())
            .and_then(|object| object.as_dict().ok())
            .and_then(|dictionary| dictionary_string(dictionary, b"NM"))
            .as_deref()
            == Some(raw_name.as_str())
    }

    pub fn snapshot_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(raw_name) = self.snapshot_native_names.get(id) else {
            return false;
        };
        if raw_name != &canonical_native_annotation_name(id) {
            return false;
        }
        self.snapshots
            .iter()
            .find(|snapshot| &snapshot.id == id)
            .and_then(|snapshot| {
                annotation_object_id(&self.document, snapshot.page_index, raw_name).ok()
            })
            .and_then(|object_id| self.document.get_object(object_id).ok())
            .and_then(|object| object.as_dict().ok())
            .is_some_and(|dictionary| {
                dictionary_name(dictionary, b"Subtype").as_deref() == Some("Stamp")
                    && dictionary_name(dictionary, b"IT").as_deref() == Some("StampSnapshot")
                    && normal_appearance_object_id(dictionary).is_some()
            })
    }

    pub fn straight_line_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.straight_line_native_identities.get(id) else {
            return false;
        };
        identity.raw_name == canonical_native_annotation_name(id)
            && self
                .document
                .get_object(identity.object_id)
                .ok()
                .and_then(|object| object.as_dict().ok())
                .and_then(|dictionary| dictionary_string(dictionary, b"NM"))
                .as_deref()
                == Some(identity.raw_name.as_str())
    }

    /// Appends a newly created native rectangle annotation to its page.
    ///
    /// Stable annotation names are unique across the document. Existing
    /// annotations must use `replace_rectangle` so an accidental create cannot
    /// overwrite imported PDF data.
    pub fn add_rectangle(
        &mut self,
        annotation: RectangleAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_rectangle_appearance(&mut self.document, &annotation);
        let annotation_dictionary =
            rectangle_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        append_markup_annotation(
            &mut self.document,
            annotation.page_index,
            annotation_dictionary,
        )?;
        self.annotation_order.push(annotation.id.clone());
        self.rectangles.push(annotation);
        Ok(())
    }

    pub fn add_pen(&mut self, annotation: PenAnnotation) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_pen_appearance(&mut self.document, &annotation);
        let dictionary =
            pen_dictionary(&annotation, appearance_id, &Dictionary::new(), &native_name);
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.pen_native_identities.insert(
            annotation.id.clone(),
            PenNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.pens.push(annotation);
        Ok(())
    }

    pub fn replace_pen(&mut self, annotation: PenAnnotation) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.pens, &annotation.id, |value| &value.id, "ink")?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.pens[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let identity = self
            .pen_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "ink {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_id = normal_appearance_object_id(&original);
        let appearance_id = add_pen_appearance(&mut self.document, &annotation);
        let canonical_name = canonical_native_annotation_name(&annotation.id);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(pen_dictionary(
                &annotation,
                appearance_id,
                &original,
                &canonical_name,
            )),
        );
        self.pen_native_identities.insert(
            annotation.id.clone(),
            PenNativeIdentity {
                raw_name: canonical_name,
                object_id,
            },
        );
        if let Some(old_appearance_id) = old_appearance_id {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        self.pens[index] = annotation;
        Ok(())
    }

    /// Removes one imported native Ink annotation by stable annotation identity.
    pub fn remove_pen(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.pens, id, |value| &value.id, "ink")?;
        let page_index = self.pens[index].page_index;
        let identity = self.pen_native_identities.get(id).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "ink {id} has no unambiguous native object identity"
            ))
        })?;
        let object_id = identity.object_id;
        let appearance_id = self
            .document
            .get_object(object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(normal_appearance_object_id);
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        if let Some(appearance_id) = appearance_id {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.pen_native_identities.remove(id);
        self.pens.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn add_text_box(
        &mut self,
        annotation: TextBoxAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        require_text_box_appearance_encodable(&annotation)?;
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_text_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        let dictionary = text_box_dictionary(
            &annotation,
            appearance_id,
            font_resources,
            &Dictionary::new(),
        );
        append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.annotation_order.push(annotation.id.clone());
        self.text_boxes.push(annotation);
        Ok(())
    }

    pub fn replace_text_box(
        &mut self,
        annotation: TextBoxAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        require_text_box_appearance_encodable(&annotation)?;
        let index = find_annotation_index(
            &self.text_boxes,
            &annotation.id,
            |value| &value.id,
            "text box",
        )?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.text_boxes[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let object_id = annotation_object_id(
            &self.document,
            annotation.page_index,
            annotation.id.as_str(),
        )?;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let appearance_id = add_text_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(text_box_dictionary(
                &annotation,
                appearance_id,
                font_resources,
                &original,
            )),
        );
        self.text_boxes[index] = annotation;
        Ok(())
    }

    /// Removes one imported native FreeText annotation by stable identity.
    pub fn remove_text_box(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.text_boxes, id, |value| &value.id, "text box")?;
        let page_index = self.text_boxes[index].page_index;
        let object_id = annotation_object_id(&self.document, page_index, id.as_str())?;
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        self.text_boxes.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn add_length(
        &mut self,
        annotation: LengthAnnotation,
    ) -> Result<LengthSaveExpectation, PdfPersistenceError> {
        if annotation.calibration().show_caption() {
            require_appearance_text_encodable(
                &annotation.caption(),
                annotation.appearance.text().font_family(),
                "length caption",
            )?;
        }
        self.require_unique_name(&annotation.id)?;
        validate_native_annotation_append_target(&self.document, annotation.page_index)?;
        let appearance_id = add_length_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        let dictionary = length_dictionary(
            &annotation,
            appearance_id,
            font_resources,
            &Dictionary::new(),
        );
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        let raw_name = canonical_native_annotation_name(&annotation.id);
        self.length_native_identities.insert(
            annotation.id.clone(),
            LengthNativeIdentity {
                raw_name: Some(raw_name.clone()),
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        let expectation = LengthSaveExpectation {
            id: annotation.id.clone(),
            raw_name: Some(raw_name),
            canonical_managed: true,
        };
        self.lengths.push(annotation);
        Ok(expectation)
    }

    pub fn replace_length(
        &mut self,
        annotation: LengthAnnotation,
    ) -> Result<LengthSaveExpectation, PdfPersistenceError> {
        let index =
            find_annotation_index(&self.lengths, &annotation.id, |value| &value.id, "length")?;
        let identity = self
            .length_native_identities
            .get(&annotation.id)
            .cloned()
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "length {} has no unambiguous native object identity",
                    annotation.id,
                ))
            })?;
        if self.lengths[index].same_persisted_state_as(&annotation) {
            let canonical_managed = self.length_has_canonical_native_identity(&annotation.id);
            return Ok(LengthSaveExpectation {
                id: annotation.id,
                canonical_managed,
                raw_name: identity.raw_name,
            });
        }
        if annotation.calibration().show_caption() {
            require_appearance_text_encodable(
                &annotation.caption(),
                annotation.appearance.text().font_family(),
                "length caption",
            )?;
        }
        if self.lengths[index].page_index != annotation.page_index {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "length {} cannot move between PDF pages",
                annotation.id,
            )));
        }
        let canonical_name = canonical_native_annotation_name(&annotation.id);
        require_available_native_name(&self.document, &canonical_name, identity.object_id)?;
        let original = self
            .document
            .get_object(identity.object_id)?
            .as_dict()?
            .clone();
        let old_appearance_graph = appearance_graph_object_ids(&self.document, &original);
        let appearance_id = add_length_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        self.document.objects.insert(
            identity.object_id,
            Object::Dictionary(length_dictionary(
                &annotation,
                appearance_id,
                font_resources,
                &original,
            )),
        );
        remove_unreferenced_object_graph(&mut self.document, &old_appearance_graph);
        self.length_native_identities.insert(
            annotation.id.clone(),
            LengthNativeIdentity {
                raw_name: Some(canonical_name.clone()),
                object_id: identity.object_id,
            },
        );
        let expectation = LengthSaveExpectation {
            id: annotation.id.clone(),
            raw_name: Some(canonical_name),
            canonical_managed: true,
        };
        self.lengths[index] = annotation;
        Ok(expectation)
    }

    /// Removes one imported native LineDimension annotation by stable identity.
    pub fn remove_length(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.lengths, id, |value| &value.id, "length")?;
        let page_index = self.lengths[index].page_index;
        let object_id = self
            .length_native_identities
            .get(id)
            .map(|identity| identity.object_id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "length {id} has no unambiguous native object identity"
                ))
            })?;
        let appearance_graph = self
            .document
            .get_object(object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .map(|dictionary| appearance_graph_object_ids(&self.document, dictionary))
            .unwrap_or_default();
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        remove_unreferenced_object_graph(&mut self.document, &appearance_graph);
        self.length_native_identities.remove(id);
        self.lengths.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn length_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(_) = self.lengths.iter().find(|value| &value.id == id) else {
            return false;
        };
        let Some(identity) = self.length_native_identities.get(id) else {
            return false;
        };
        let canonical_name = canonical_native_annotation_name(id);
        if identity.raw_name.as_deref() != Some(canonical_name.as_str()) {
            return false;
        }
        let Ok(dictionary) = self
            .document
            .get_object(identity.object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        dictionary_string(dictionary, b"NM").as_deref() == identity.raw_name.as_deref()
            && dictionary_name(dictionary, b"Subtype").as_deref() == Some("Line")
            && dictionary_name(dictionary, b"IT").as_deref() == Some("LineDimension")
            && dictionary_string(dictionary, b"Subj").as_deref() == Some("Length Measurement")
            && dictionary.get(b"Measure").is_ok()
            && dictionary.get(b"AP").is_ok()
    }

    pub fn length_matches_save_expectation(&self, expectation: &LengthSaveExpectation) -> bool {
        let Some(identity) = self.length_native_identities.get(&expectation.id) else {
            return false;
        };
        let physical_name = self
            .document
            .get_object(identity.object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(|dictionary| dictionary_string(dictionary, b"NM"));
        let native_name_matches = identity.raw_name.as_deref() == expectation.raw_name.as_deref()
            && physical_name.as_deref() == expectation.raw_name.as_deref();
        native_name_matches
            && self.length_has_canonical_native_identity(&expectation.id)
                == expectation.canonical_managed
    }

    pub fn add_dimension(
        &mut self,
        annotation: DimensionAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        require_appearance_text_encodable(
            annotation.content(),
            annotation.appearance.text().font_family(),
            "dimension caption",
        )?;
        self.require_unique_name(&annotation.id)?;
        validate_native_annotation_append_target(&self.document, annotation.page_index)?;
        let appearance_id = add_dimension_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        let dictionary = dimension_dictionary(
            &annotation,
            appearance_id,
            font_resources,
            &Dictionary::new(),
        )?;
        append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.dimension_native_names.insert(
            annotation.id.clone(),
            canonical_native_annotation_name(&annotation.id),
        );
        self.annotation_order.push(annotation.id.clone());
        self.dimensions.push(annotation);
        Ok(())
    }

    pub fn replace_dimension(
        &mut self,
        annotation: DimensionAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        require_appearance_text_encodable(
            annotation.content(),
            annotation.appearance.text().font_family(),
            "dimension caption",
        )?;
        let index = find_annotation_index(
            &self.dimensions,
            &annotation.id,
            |value| &value.id,
            "dimension",
        )?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.dimensions[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let native_name = self
            .dimension_native_names
            .get(&annotation.id)
            .map(String::as_str)
            .unwrap_or(annotation.id.as_str());
        let object_id = annotation_object_id(&self.document, annotation.page_index, native_name)?;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let appearance_id = add_dimension_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(dimension_dictionary(
                &annotation,
                appearance_id,
                font_resources,
                &original,
            )?),
        );
        self.dimension_native_names.insert(
            annotation.id.clone(),
            canonical_native_annotation_name(&annotation.id),
        );
        self.dimensions[index] = annotation;
        Ok(())
    }

    pub fn remove_dimension(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.dimensions, id, |value| &value.id, "dimension")?;
        let page_index = self.dimensions[index].page_index;
        let native_name = self
            .dimension_native_names
            .get(id)
            .map(String::as_str)
            .unwrap_or(id.as_str());
        let object_id = annotation_object_id(&self.document, page_index, native_name)?;
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        self.dimension_native_names.remove(id);
        self.dimensions.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn dimension_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(annotation) = self.dimensions.iter().find(|value| &value.id == id) else {
            return false;
        };
        let Some(native_name) = self.dimension_native_names.get(id) else {
            return false;
        };
        if native_name != &canonical_native_annotation_name(id) {
            return false;
        }
        let Ok(object_id) =
            annotation_object_id(&self.document, annotation.page_index, native_name.as_str())
        else {
            return false;
        };
        let Ok(dictionary) = self
            .document
            .get_object(object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        dictionary_name(dictionary, b"Subtype").as_deref() == Some("Line")
            && dictionary_name(dictionary, b"IT").as_deref() == Some("LineDimension")
            && dictionary_string(dictionary, b"Subj").as_deref() == Some("Dimension")
            && dictionary.get(b"Measure").is_err()
            && dictionary.get(b"AP").is_ok()
    }

    pub fn add_straight_line(
        &mut self,
        annotation: StraightLineAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        validate_native_annotation_append_target(&self.document, annotation.page_index)?;
        let max_id_before = self.document.max_id;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let dictionary = straight_line_pdf::rebuild_managed(
            &mut self.document,
            &annotation,
            &Dictionary::new(),
        )?;
        let appearance_id = normal_appearance_object_id(&dictionary);
        let object_id =
            match append_markup_annotation(&mut self.document, annotation.page_index, dictionary) {
                Ok(object_id) => object_id,
                Err(error) => {
                    if let Some(appearance_id) = appearance_id {
                        remove_object_if_unreferenced(&mut self.document, appearance_id);
                    }
                    self.document.max_id = max_id_before;
                    return Err(error);
                }
            };
        self.straight_line_native_identities.insert(
            annotation.id.clone(),
            StraightLineNativeIdentity {
                raw_name: native_name,
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.straight_lines.push(annotation);
        Ok(())
    }

    pub fn replace_straight_line(
        &mut self,
        annotation: StraightLineAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(
            &self.straight_lines,
            &annotation.id,
            |value| &value.id,
            "straight line",
        )?;
        let identity = self
            .straight_line_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "straight line {} has no unambiguous native object identity",
                    annotation.id,
                ))
            })?;
        let object_id = identity.object_id;
        if self.straight_lines[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        if self.straight_lines[index].page_index != annotation.page_index {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "straight line {} cannot move between PDF pages",
                annotation.id,
            )));
        }
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_id = normal_appearance_object_id(&original);
        let replacement =
            straight_line_pdf::rebuild_managed(&mut self.document, &annotation, &original)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(replacement));
        if let Some(old_appearance_id) = old_appearance_id {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        self.straight_line_native_identities.insert(
            annotation.id.clone(),
            StraightLineNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.straight_lines[index] = annotation;
        Ok(())
    }

    pub fn remove_straight_line(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index =
            find_annotation_index(&self.straight_lines, id, |value| &value.id, "straight line")?;
        let page_index = self.straight_lines[index].page_index;
        let identity = self
            .straight_line_native_identities
            .get(id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "straight line {id} has no unambiguous native object identity"
                ))
            })?;
        let appearance_id = self
            .document
            .get_object(identity.object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(normal_appearance_object_id);
        remove_annotation_reference(&mut self.document, page_index, identity.object_id)?;
        self.document.objects.remove(&identity.object_id);
        if let Some(appearance_id) = appearance_id {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.straight_line_native_identities.remove(id);
        self.straight_lines.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn add_vertex_path(
        &mut self,
        annotation: VertexPathAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_vertex_path_appearance(&mut self.document, &annotation)?;
        let dictionary = vertex_path_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.vertex_path_native_identities.insert(
            annotation.id.clone(),
            VertexPathNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.vertex_paths.push(annotation);
        Ok(())
    }

    pub fn replace_vertex_path(
        &mut self,
        annotation: VertexPathAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(
            &self.vertex_paths,
            &annotation.id,
            |value| &value.id,
            "vertex path",
        )?;
        if self.vertex_paths[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let identity = self
            .vertex_path_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "vertex path {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_id = normal_appearance_object_id(&original);
        let appearance_id = add_vertex_path_appearance(&mut self.document, &annotation)?;
        let dictionary = vertex_path_dictionary(&annotation, appearance_id, &original)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
        if let Some(old_appearance_id) = old_appearance_id {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        self.vertex_path_native_identities.insert(
            annotation.id.clone(),
            VertexPathNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.vertex_paths[index] = annotation;
        Ok(())
    }

    pub fn remove_vertex_path(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index =
            find_annotation_index(&self.vertex_paths, id, |value| &value.id, "vertex path")?;
        let page_index = self.vertex_paths[index].page_index;
        let identity = self.vertex_path_native_identities.get(id).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "vertex path {id} has no unambiguous native object identity"
            ))
        })?;
        let appearance_id = self
            .document
            .get_object(identity.object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(normal_appearance_object_id);
        remove_annotation_reference(&mut self.document, page_index, identity.object_id)?;
        self.document.objects.remove(&identity.object_id);
        if let Some(appearance_id) = appearance_id {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.vertex_path_native_identities.remove(id);
        self.vertex_paths.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn add_cloud(&mut self, annotation: CloudAnnotation) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_cloud_appearance(&mut self.document, &annotation)?;
        let dictionary = cloud_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.cloud_native_identities.insert(
            annotation.id.clone(),
            CloudNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.clouds.push(annotation);
        Ok(())
    }

    pub fn replace_cloud(
        &mut self,
        annotation: CloudAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index =
            find_annotation_index(&self.clouds, &annotation.id, |value| &value.id, "cloud")?;
        if self.clouds[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let identity = self
            .cloud_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "cloud {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_id = normal_appearance_object_id(&original);
        let appearance_id = add_cloud_appearance(&mut self.document, &annotation)?;
        let dictionary = cloud_dictionary(&annotation, appearance_id, &original)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
        if let Some(old_appearance_id) = old_appearance_id {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        self.clouds[index] = annotation;
        Ok(())
    }

    pub fn remove_cloud(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.clouds, id, |value| &value.id, "cloud")?;
        let page_index = self.clouds[index].page_index;
        let identity = self.cloud_native_identities.get(id).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "cloud {id} has no unambiguous native object identity"
            ))
        })?;
        let appearance_id = self
            .document
            .get_object(identity.object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(normal_appearance_object_id);
        remove_annotation_reference(&mut self.document, page_index, identity.object_id)?;
        self.document.objects.remove(&identity.object_id);
        if let Some(appearance_id) = appearance_id {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.cloud_native_identities.remove(id);
        self.clouds.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn cloud_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.cloud_native_identities.get(id) else {
            return false;
        };
        let Some(annotation) = self.clouds.iter().find(|annotation| &annotation.id == id) else {
            return false;
        };
        let canonical_name = canonical_native_annotation_name(id);
        if identity.raw_name != canonical_name {
            return false;
        }
        let Ok(dictionary) = self
            .document
            .get_object(identity.object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        dictionary_string(dictionary, b"NM").as_deref() == Some(canonical_name.as_str())
            && dictionary_name(dictionary, b"Subtype").as_deref() == Some("Polygon")
            && dictionary_name(dictionary, b"IT").as_deref() == Some("PolygonCloud")
            && dictionary
                .get(b"Vertices")
                .ok()
                .and_then(|value| value.as_array().ok())
                .is_some_and(|vertices| vertices.len() == annotation.points().len() * 2)
            && dictionary
                .get(b"BE")
                .ok()
                .and_then(|value| value.as_dict().ok())
                .is_some_and(|effect| {
                    dictionary_name(effect, b"S").as_deref() == Some("C")
                        && dictionary_float(effect, b"I")
                            == Some(annotation.border_effect_intensity())
                })
            && normal_appearance_object_id(dictionary).is_some()
    }

    pub fn add_cloud_plus(
        &mut self,
        annotation: CloudPlusAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        require_appearance_text_encodable(
            annotation.content(),
            annotation.appearance.text().font_family(),
            "Cloud+",
        )?;
        self.require_unique_name(&annotation.id)?;
        validate_native_annotation_append_target(&self.document, annotation.page_index)?;
        let (cloud_name, text_name) = new_cloud_plus_native_names(&annotation.id);
        let cloud_appearance_id = add_cloud_plus_cloud_appearance(&mut self.document, &annotation)?;
        let text_appearance_id = add_cloud_plus_text_appearance(&mut self.document, &annotation)?;
        let text_font_resources =
            text_appearance_font_resources(&self.document, text_appearance_id);
        let cloud_dictionary = cloud_plus_cloud_dictionary(
            &annotation,
            cloud_appearance_id,
            &cloud_name,
            &Dictionary::new(),
        )?;
        let text_dictionary = cloud_plus_text_dictionary(
            &annotation,
            text_appearance_id,
            &cloud_name,
            &text_name,
            text_font_resources,
            &Dictionary::new(),
        )?;
        let cloud_object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, cloud_dictionary)?;
        let text_object_id = match append_markup_annotation(
            &mut self.document,
            annotation.page_index,
            text_dictionary,
        ) {
            Ok(object_id) => object_id,
            Err(error) => {
                remove_annotation_reference(
                    &mut self.document,
                    annotation.page_index,
                    cloud_object_id,
                )?;
                self.document.objects.remove(&cloud_object_id);
                remove_object_if_unreferenced(&mut self.document, cloud_appearance_id);
                remove_object_if_unreferenced(&mut self.document, text_appearance_id);
                return Err(error);
            }
        };
        self.document
            .get_object_mut(cloud_object_id)?
            .as_dict_mut()?
            .set("IRT", Object::Reference(text_object_id));
        self.document
            .get_object_mut(cloud_object_id)?
            .as_dict_mut()?
            .set("RT", Object::Name(b"Group".to_vec()));
        self.cloud_plus_native_identities.insert(
            annotation.id.clone(),
            CloudPlusNativeIdentity {
                cloud_raw_name: cloud_name,
                cloud_object_id,
                text_raw_name: text_name,
                text_object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.cloud_pluses.push(annotation);
        Ok(())
    }

    pub fn replace_cloud_plus(
        &mut self,
        annotation: CloudPlusAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        require_appearance_text_encodable(
            annotation.content(),
            annotation.appearance.text().font_family(),
            "Cloud+",
        )?;
        let index = find_annotation_index(
            &self.cloud_pluses,
            &annotation.id,
            |value| &value.id,
            "Cloud+",
        )?;
        if self.cloud_pluses[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let identity = self
            .cloud_plus_native_identities
            .get(&annotation.id)
            .cloned()
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "Cloud+ {} has no unambiguous paired native identity",
                    annotation.id
                ))
            })?;
        let cloud_original = self
            .document
            .get_object(identity.cloud_object_id)?
            .as_dict()?
            .clone();
        let text_original = self
            .document
            .get_object(identity.text_object_id)?
            .as_dict()?
            .clone();
        let old_appearance_ids = [
            normal_appearance_object_id(&cloud_original),
            normal_appearance_object_id(&text_original),
        ];
        let (cloud_name, text_name) = (
            annotation.id.as_str().to_owned(),
            identity.text_raw_name.clone(),
        );
        let cloud_appearance_id = add_cloud_plus_cloud_appearance(&mut self.document, &annotation)?;
        let text_appearance_id = add_cloud_plus_text_appearance(&mut self.document, &annotation)?;
        let text_font_resources =
            text_appearance_font_resources(&self.document, text_appearance_id);
        let mut cloud_dictionary = cloud_plus_cloud_dictionary(
            &annotation,
            cloud_appearance_id,
            &cloud_name,
            &cloud_original,
        )?;
        cloud_dictionary.set("IRT", Object::Reference(identity.text_object_id));
        cloud_dictionary.set("RT", Object::Name(b"Group".to_vec()));
        let text_dictionary = cloud_plus_text_dictionary(
            &annotation,
            text_appearance_id,
            &cloud_name,
            &text_name,
            text_font_resources,
            &text_original,
        )?;
        self.document.objects.insert(
            identity.cloud_object_id,
            Object::Dictionary(cloud_dictionary),
        );
        self.document
            .objects
            .insert(identity.text_object_id, Object::Dictionary(text_dictionary));
        self.cloud_plus_native_identities.insert(
            annotation.id.clone(),
            CloudPlusNativeIdentity {
                cloud_raw_name: cloud_name,
                cloud_object_id: identity.cloud_object_id,
                text_raw_name: text_name,
                text_object_id: identity.text_object_id,
            },
        );
        self.cloud_pluses[index] = annotation;
        for old_appearance_id in old_appearance_ids.into_iter().flatten() {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        Ok(())
    }

    pub fn remove_cloud_plus(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.cloud_pluses, id, |value| &value.id, "Cloud+")?;
        let page_index = self.cloud_pluses[index].page_index;
        let identity = self
            .cloud_plus_native_identities
            .get(id)
            .cloned()
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "Cloud+ {id} has no unambiguous paired native identity"
                ))
            })?;
        let appearance_ids = [identity.cloud_object_id, identity.text_object_id].map(|object_id| {
            self.document
                .get_object(object_id)
                .ok()
                .and_then(|object| object.as_dict().ok())
                .and_then(normal_appearance_object_id)
        });
        for object_id in [identity.cloud_object_id, identity.text_object_id] {
            remove_annotation_reference(&mut self.document, page_index, object_id)?;
            self.document.objects.remove(&object_id);
        }
        for appearance_id in appearance_ids.into_iter().flatten() {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.cloud_plus_native_identities.remove(id);
        self.cloud_pluses.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn cloud_plus_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.cloud_plus_native_identities.get(id) else {
            return false;
        };
        let Some(annotation) = self
            .cloud_pluses
            .iter()
            .find(|annotation| &annotation.id == id)
        else {
            return false;
        };
        let (cloud_name, text_name) = (id.as_str().to_owned(), identity.text_raw_name.clone());
        if identity.cloud_raw_name != cloud_name {
            return false;
        }
        let Ok(cloud) = self
            .document
            .get_object(identity.cloud_object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        let Ok(text) = self
            .document
            .get_object(identity.text_object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        dictionary_string(cloud, b"NM").as_deref() == Some(cloud_name.as_str())
            && dictionary_name(cloud, b"Subtype").as_deref() == Some("Polygon")
            && dictionary_name(cloud, b"IT").as_deref() == Some("PolygonCloud")
            && dictionary_name(cloud, b"ITEx").as_deref() == Some("PolyText")
            && dictionary_string(cloud, b"Subj").as_deref() == Some("Cloud+")
            && cloud
                .get(b"IRT")
                .ok()
                .and_then(|value| value.as_reference().ok())
                == Some(identity.text_object_id)
            && dictionary_string(text, b"NM").as_deref() == Some(text_name.as_str())
            && dictionary_name(text, b"Subtype").as_deref() == Some("FreeText")
            && dictionary_name(text, b"IT").as_deref() == Some("FreeTextCallout")
            && dictionary_name(text, b"ITEx").as_deref() == Some("PolyText")
            && dictionary_string(text, b"Subj").as_deref() == Some("Cloud+")
            && text
                .get(b"CL")
                .ok()
                .and_then(|value| value.as_array().ok())
                .is_some_and(|values| values.len() == annotation.leader_points().len() * 2)
            && normal_appearance_object_id(cloud).is_some()
            && normal_appearance_object_id(text).is_some()
    }

    pub fn add_callout(
        &mut self,
        annotation: CalloutAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let annotation = annotation.canonicalized_for_disk()?;
        require_appearance_text_encodable(
            annotation.content(),
            annotation.appearance.text().font_family(),
            "callout",
        )?;
        self.require_unique_name(&annotation.id)?;
        validate_native_annotation_append_target(&self.document, annotation.page_index)?;
        let appearance_id = add_callout_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        let dictionary = callout_dictionary(
            &annotation,
            appearance_id,
            font_resources,
            &Dictionary::new(),
        )?;
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.callout_native_identities.insert(
            annotation.id.clone(),
            CalloutNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.callouts.push(annotation);
        Ok(())
    }

    pub fn replace_callout(
        &mut self,
        annotation: CalloutAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let annotation = annotation.canonicalized_for_disk()?;
        require_appearance_text_encodable(
            annotation.content(),
            annotation.appearance.text().font_family(),
            "callout",
        )?;
        let index =
            find_annotation_index(&self.callouts, &annotation.id, |value| &value.id, "callout")?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.callouts[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let identity = self
            .callout_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "callout {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_id = normal_appearance_object_id(&original);
        let appearance_id = add_callout_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        let dictionary = callout_dictionary(&annotation, appearance_id, font_resources, &original)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
        if let Some(old_appearance_id) = old_appearance_id {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        self.callout_native_identities.insert(
            annotation.id.clone(),
            CalloutNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.callouts[index] = annotation;
        Ok(())
    }

    pub fn remove_callout(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.callouts, id, |value| &value.id, "callout")?;
        let page_index = self.callouts[index].page_index;
        let identity = self.callout_native_identities.get(id).ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "callout {id} has no unambiguous native object identity"
            ))
        })?;
        let appearance_id = self
            .document
            .get_object(identity.object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(normal_appearance_object_id);
        remove_annotation_reference(&mut self.document, page_index, identity.object_id)?;
        self.document.objects.remove(&identity.object_id);
        if let Some(appearance_id) = appearance_id {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.callout_native_identities.remove(id);
        self.callouts.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn callout_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.callout_native_identities.get(id) else {
            return false;
        };
        let Some(annotation) = self.callouts.iter().find(|annotation| &annotation.id == id) else {
            return false;
        };
        let canonical_name = canonical_native_annotation_name(id);
        if identity.raw_name != canonical_name {
            return false;
        }
        let Ok(dictionary) = self
            .document
            .get_object(identity.object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        dictionary_string(dictionary, b"NM").as_deref() == Some(canonical_name.as_str())
            && dictionary_name(dictionary, b"Subtype").as_deref() == Some("FreeText")
            && dictionary_name(dictionary, b"IT").as_deref() == Some("FreeTextCallout")
            && dictionary_string(dictionary, b"Subj").as_deref() == Some("Callout")
            && dictionary
                .get(b"CL")
                .ok()
                .and_then(|value| value.as_array().ok())
                .is_some_and(|points| {
                    points.len() == normalize_callout_leader(annotation).len() * 2
                })
            && normal_appearance_object_id(dictionary).is_some()
    }

    pub fn add_measurement_path(
        &mut self,
        annotation: MeasurementPathAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        if annotation.calibration().show_caption() {
            require_appearance_text_encodable(
                &annotation.caption(),
                annotation.text_style().font_family(),
                "measurement caption",
            )?;
        }
        self.require_unique_name(&annotation.id)?;
        validate_native_annotation_append_target(&self.document, annotation.page_index)?;
        let appearance_id = add_measurement_path_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        let dictionary = measurement_path_dictionary(
            &annotation,
            appearance_id,
            font_resources,
            &Dictionary::new(),
        )?;
        let object_id =
            append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.measurement_path_native_identities.insert(
            annotation.id.clone(),
            MeasurementPathNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.annotation_order.push(annotation.id.clone());
        self.measurement_paths.push(annotation);
        Ok(())
    }

    pub fn replace_measurement_path(
        &mut self,
        annotation: MeasurementPathAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(
            &self.measurement_paths,
            &annotation.id,
            |value| &value.id,
            "measurement path",
        )?;
        if self.measurement_paths[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        if annotation.calibration().show_caption() {
            require_appearance_text_encodable(
                &annotation.caption(),
                annotation.text_style().font_family(),
                "measurement caption",
            )?;
        }
        let identity = self
            .measurement_path_native_identities
            .get(&annotation.id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "measurement path {} has no unambiguous native object identity",
                    annotation.id
                ))
            })?;
        let object_id = identity.object_id;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_id = normal_appearance_object_id(&original);
        let appearance_id = add_measurement_path_appearance(&mut self.document, &annotation)?;
        let font_resources = text_appearance_font_resources(&self.document, appearance_id);
        let dictionary =
            measurement_path_dictionary(&annotation, appearance_id, font_resources, &original)?;
        self.document
            .objects
            .insert(object_id, Object::Dictionary(dictionary));
        if let Some(old_appearance_id) = old_appearance_id {
            remove_object_if_unreferenced(&mut self.document, old_appearance_id);
        }
        self.measurement_path_native_identities.insert(
            annotation.id.clone(),
            MeasurementPathNativeIdentity {
                raw_name: canonical_native_annotation_name(&annotation.id),
                object_id,
            },
        );
        self.measurement_paths[index] = annotation;
        Ok(())
    }

    pub fn remove_measurement_path(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(
            &self.measurement_paths,
            id,
            |value| &value.id,
            "measurement path",
        )?;
        let page_index = self.measurement_paths[index].page_index;
        let identity = self
            .measurement_path_native_identities
            .get(id)
            .ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(format!(
                    "measurement path {id} has no unambiguous native object identity"
                ))
            })?;
        let appearance_id = self
            .document
            .get_object(identity.object_id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .and_then(normal_appearance_object_id);
        remove_annotation_reference(&mut self.document, page_index, identity.object_id)?;
        self.document.objects.remove(&identity.object_id);
        if let Some(appearance_id) = appearance_id {
            remove_object_if_unreferenced(&mut self.document, appearance_id);
        }
        self.measurement_path_native_identities.remove(id);
        self.measurement_paths.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn vertex_path_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.vertex_path_native_identities.get(id) else {
            return false;
        };
        let Some(annotation) = self
            .vertex_paths
            .iter()
            .find(|annotation| &annotation.id == id)
        else {
            return false;
        };
        let canonical_name = canonical_native_annotation_name(id);
        if identity.raw_name != canonical_name {
            return false;
        }
        let Ok(dictionary) = self
            .document
            .get_object(identity.object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        let expected_subtype = match annotation.kind {
            VertexPathKind::Polyline => "PolyLine",
            VertexPathKind::Polygon => "Polygon",
        };
        dictionary_string(dictionary, b"NM").as_deref() == Some(canonical_name.as_str())
            && dictionary_name(dictionary, b"Subtype").as_deref() == Some(expected_subtype)
            && dictionary
                .get(b"Vertices")
                .ok()
                .and_then(|value| value.as_array().ok())
                .is_some_and(|vertices| vertices.len() == annotation.points().len() * 2)
            && dictionary.get(b"Rect").is_ok()
            && normal_appearance_object_id(dictionary).is_some_and(|appearance_id| {
                self.document
                    .get_object(appearance_id)
                    .is_ok_and(|object| object.as_stream().is_ok())
            })
    }

    pub fn measurement_path_has_canonical_native_identity(&self, id: &MarkupId) -> bool {
        let Some(identity) = self.measurement_path_native_identities.get(id) else {
            return false;
        };
        let Some(annotation) = self
            .measurement_paths
            .iter()
            .find(|annotation| &annotation.id == id)
        else {
            return false;
        };
        let canonical_name = canonical_native_annotation_name(id);
        if identity.raw_name != canonical_name {
            return false;
        }
        let Ok(dictionary) = self
            .document
            .get_object(identity.object_id)
            .and_then(Object::as_dict)
        else {
            return false;
        };
        let (subtype, intent) = match annotation.kind {
            MeasurementPathKind::Polylength => ("PolyLine", "PolyLineDimension"),
            MeasurementPathKind::Area => ("Polygon", "PolygonDimension"),
        };
        dictionary_string(dictionary, b"NM").as_deref() == Some(canonical_name.as_str())
            && dictionary_name(dictionary, b"Subtype").as_deref() == Some(subtype)
            && dictionary_name(dictionary, b"IT").as_deref() == Some(intent)
            && dictionary.get(b"Measure").is_ok()
            && dictionary
                .get(b"Vertices")
                .ok()
                .and_then(|value| value.as_array().ok())
                .is_some_and(|vertices| vertices.len() == annotation.points().len() * 2)
            && normal_appearance_object_id(dictionary).is_some_and(|appearance_id| {
                self.document
                    .get_object(appearance_id)
                    .is_ok_and(|object| object.as_stream().is_ok())
            })
    }

    pub fn add_image(&mut self, annotation: ImageAnnotation) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let (appearance_id, image_id) = add_image_appearance(&mut self.document, &annotation);
        let dictionary =
            image_dictionary(&annotation, appearance_id, Some(image_id), &Dictionary::new());
        append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.image_native_names.insert(
            annotation.id.clone(),
            canonical_native_annotation_name(&annotation.id),
        );
        self.annotation_order.push(annotation.id.clone());
        self.images.push(annotation);
        Ok(())
    }

    pub fn replace_image(
        &mut self,
        annotation: ImageAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index =
            find_annotation_index(&self.images, &annotation.id, |value| &value.id, "image")?;
        // An unchanged markup keeps its original bytes, whoever wrote it.
        if self.images[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        let native_name = self
            .image_native_names
            .get(&annotation.id)
            .map(String::as_str)
            .unwrap_or(annotation.id.as_str());
        let object_id = annotation_object_id(&self.document, annotation.page_index, native_name)?;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_ids = image_appearance_object_ids(&self.document, &original);
        let (appearance_id, image_id) = add_image_appearance(&mut self.document, &annotation);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(image_dictionary(
                &annotation,
                appearance_id,
                Some(image_id),
                &original,
            )),
        );
        self.image_native_names.insert(
            annotation.id.clone(),
            canonical_native_annotation_name(&annotation.id),
        );
        self.images[index] = annotation;
        for object_id in old_appearance_ids {
            remove_object_if_unreferenced(&mut self.document, object_id);
        }
        Ok(())
    }

    pub fn remove_image(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.images, id, |value| &value.id, "image")?;
        let page_index = self.images[index].page_index;
        let native_name = self
            .image_native_names
            .get(id)
            .map(String::as_str)
            .unwrap_or(id.as_str());
        let object_id = annotation_object_id(&self.document, page_index, native_name)?;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_ids = image_appearance_object_ids(&self.document, &original);
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        for object_id in old_appearance_ids {
            remove_object_if_unreferenced(&mut self.document, object_id);
        }
        self.image_native_names.remove(id);
        self.images.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn add_snapshot(
        &mut self,
        annotation: SnapshotAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_snapshot_appearance(&mut self.document, &annotation);
        let dictionary = snapshot_dictionary(&annotation, appearance_id, &Dictionary::new());
        append_markup_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.snapshot_native_names.insert(
            annotation.id.clone(),
            canonical_native_annotation_name(&annotation.id),
        );
        self.annotation_order.push(annotation.id.clone());
        self.snapshots.push(annotation);
        Ok(())
    }

    pub fn replace_snapshot(
        &mut self,
        annotation: SnapshotAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(
            &self.snapshots,
            &annotation.id,
            |value| &value.id,
            "snapshot",
        )?;
        let vector_source = self
            .vector_snapshot_sources
            .iter()
            .find(|(id, _)| id == &annotation.id)
            .map(|(_, source)| *source);
        // An unchanged markup keeps its original bytes, whoever wrote it. A
        // vector Snapshot's raster is only its canvas picture of the Form.
        if self.snapshots[index].same_persisted_state_as(&annotation)
            || (vector_source.is_some() && self.snapshots[index].same_placement_as(&annotation))
        {
            return Ok(());
        }
        let original_page_index = self.snapshots[index].page_index;
        if annotation.page_index != original_page_index {
            return Err(PdfPersistenceError::InvalidDocument(
                "Snapshot replacement cannot move between PDF pages".into(),
            ));
        }
        let native_name = self
            .snapshot_native_names
            .get(&annotation.id)
            .map(String::as_str)
            .unwrap_or(annotation.id.as_str());
        let object_id = annotation_object_id(&self.document, original_page_index, native_name)?;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_ids = image_appearance_object_ids(&self.document, &original);
        let appearance_id = if let Some(source) = vector_source {
            add_vector_snapshot_appearance(&mut self.document, &annotation, &source)
        } else if self.snapshots[index].asset() == annotation.asset() {
            old_appearance_ids
                .get(1)
                .copied()
                .map(|image_id| {
                    add_snapshot_form_appearance(&mut self.document, &annotation, image_id)
                })
                .unwrap_or_else(|| add_snapshot_appearance(&mut self.document, &annotation))
        } else {
            add_snapshot_appearance(&mut self.document, &annotation)
        };
        self.document.objects.insert(
            object_id,
            Object::Dictionary(snapshot_dictionary(&annotation, appearance_id, &original)),
        );
        self.snapshot_native_names.insert(
            annotation.id.clone(),
            canonical_native_annotation_name(&annotation.id),
        );
        self.snapshots[index] = annotation;
        for object_id in old_appearance_ids {
            remove_object_if_unreferenced(&mut self.document, object_id);
        }
        Ok(())
    }

    pub fn remove_snapshot(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.snapshots, id, |value| &value.id, "snapshot")?;
        let page_index = self.snapshots[index].page_index;
        let native_name = self
            .snapshot_native_names
            .get(id)
            .map(String::as_str)
            .unwrap_or(id.as_str());
        let object_id = annotation_object_id(&self.document, page_index, native_name)?;
        // Revu writes some entries (such as `/BS`) as indirect objects.
        let original =
            resolved_annotation_view(&self.document, self.document.get_object(object_id)?.as_dict()?);
        let old_appearance_ids = image_appearance_object_ids(&self.document, &original);
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        for object_id in old_appearance_ids {
            remove_object_if_unreferenced(&mut self.document, object_id);
        }
        self.snapshot_native_names.remove(id);
        self.vector_snapshot_sources.retain(|(source_id, _)| source_id != id);
        self.snapshots.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    fn require_unique_name(&self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let canonical = canonical_native_annotation_name(id);
        let duplicate = self.rectangles.iter().any(|value| &value.id == id)
            || self.redacts.iter().any(|value| &value.id == id)
            || self.ellipses.iter().any(|value| &value.id == id)
            || self.arcs.iter().any(|value| &value.id == id)
            || self.pens.iter().any(|value| &value.id == id)
            || self.text_boxes.iter().any(|value| &value.id == id)
            || self.lengths.iter().any(|value| &value.id == id)
            || self.dimensions.iter().any(|value| &value.id == id)
            || self.straight_lines.iter().any(|value| &value.id == id)
            || self.vertex_paths.iter().any(|value| &value.id == id)
            || self.clouds.iter().any(|value| &value.id == id)
            || self.cloud_pluses.iter().any(|value| &value.id == id)
            || self.callouts.iter().any(|value| &value.id == id)
            || self.measurement_paths.iter().any(|value| &value.id == id)
            || self.images.iter().any(|value| &value.id == id)
            || self.snapshots.iter().any(|value| &value.id == id)
            || self
                .untouched_annotations
                .iter()
                .any(|value| value.name == id.as_str() || value.name == canonical);
        if duplicate {
            Err(AnnotationError::DuplicateMarkupId(id.clone()).into())
        } else {
            Ok(())
        }
    }


    fn prepare_save_inner(
        &self,
        target: &Path,
        replacement_guard: Option<SourceGuard>,
    ) -> Result<PreparedPdfSave, PdfPersistenceError> {
        if replacement_guard.is_none() && target.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                format!("refusing to replace existing PDF {}", target.display()),
            )
            .into());
        }
        let parent = target.parent().ok_or_else(|| {
            PdfPersistenceError::InvalidDocument("save target must have a parent directory".into())
        })?;
        let file_name = target.file_name().ok_or_else(|| {
            PdfPersistenceError::InvalidDocument("save target must have a file name".into())
        })?;
        let temp_id = NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let temporary = parent.join(format!(
            ".{}.butter-paper-{}-{temp_id}.tmp",
            file_name.to_string_lossy(),
            process::id(),
        ));
        #[cfg(unix)]
        let mut replacement_stage = replacement_guard
            .as_ref()
            .map(|guard| {
                OwnedInPlaceStage::create(
                    parent,
                    temporary.clone(),
                    file_name,
                    guard.parent_identity,
                )
            })
            .transpose()?;
        let mut ambient_output: Option<File> = None;
        let result = (|| {
            #[cfg(unix)]
            let output = if let Some(stage) = replacement_stage.as_mut() {
                stage.file_mut()
            } else {
                ambient_output.insert(
                    OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&temporary)?,
                )
            };
            #[cfg(not(unix))]
            let output = ambient_output.insert(
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)?,
            );
            let mut document = self.document.clone();
            write_page_scales(&mut document, &self.page_scales, &self.original_page_scales)?;
            write_page_rotations(
                &mut document,
                &self.page_rotations,
                &self.changed_page_rotations,
            )?;
            prune_unreachable_objects_for_save(&mut document);
            document.save_to(&mut *output)?;
            output.flush()?;
            output.sync_all()?;
            Ok(PreparedPdfSave {
                temporary: temporary.clone(),
                target: target.to_path_buf(),
                replacement_guard,
                #[cfg(any(unix, windows))]
                authorized_stage: None,
                #[cfg(any(unix, windows))]
                cleanup_owned_by_authority: {
                    #[cfg(unix)]
                    {
                        replacement_stage.is_some()
                    }
                    #[cfg(windows)]
                    {
                        false
                    }
                },
                #[cfg(unix)]
                replacement_stage: replacement_stage.take(),
                published: false,
            })
        })();
        if result.is_err() {
            #[cfg(unix)]
            if replacement_stage.is_none() {
                fs::remove_file(&temporary).ok();
            }
            #[cfg(not(unix))]
            fs::remove_file(&temporary).ok();
        }
        result
    }

    /// Prepares a new PDF through a one-shot target authority captured at the
    /// native picker boundary. The stage and final publication remain relative
    /// to the retained parent directory instead of reopening an ambient path.
    pub fn prepare_save_authorized(
        &self,
        authority: &SaveAsTargetAuthority,
    ) -> Result<PreparedPdfSave, PdfPersistenceError> {
        #[cfg(not(any(unix, windows)))]
        {
            let _ = authority;
            return Err(PdfPersistenceError::InvalidDocument(
                "authorized Save As publication is not implemented on this platform".into(),
            ));
        }
        #[cfg(any(unix, windows))]
        {
            let mut stage = authority.prepare_stage()?;
            let result = (|| {
                let output = stage.file_mut();
                let mut document = self.document.clone();
                write_page_scales(&mut document, &self.page_scales, &self.original_page_scales)?;
                write_page_rotations(
                    &mut document,
                    &self.page_rotations,
                    &self.changed_page_rotations,
                )?;
                prune_unreachable_objects_for_save(&mut document);
                document.save_to(&mut *output)?;
                output.flush()?;
                output.sync_all()?;
                Ok::<(), PdfPersistenceError>(())
            })();
            result?;
            Ok(PreparedPdfSave {
                temporary: stage.path().to_path_buf(),
                target: stage.target_path().to_path_buf(),
                replacement_guard: None,
                authorized_stage: Some(stage),
                cleanup_owned_by_authority: true,
                #[cfg(unix)]
                replacement_stage: None,
                published: false,
            })
        }
    }


    pub fn prepare_save_replacing(
        &self,
        target: impl AsRef<Path>,
    ) -> Result<PreparedPdfSave, PdfPersistenceError> {
        if Self::in_place_publication_capability()
            == InPlacePublicationCapability::NewTargetRequired
        {
            return Err(PdfPersistenceError::InvalidDocument(
                "in-place Save requires a new target on this platform".into(),
            ));
        }
        let target = target.as_ref();
        if target != self.source_path {
            return Err(PdfPersistenceError::InvalidDocument(
                "in-place Save target must be the opened source path".into(),
            ));
        }
        let guard = self.source_guard.clone().ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "in-place Save requires a source opened for update".into(),
            )
        })?;
        let current = read_regular_file_snapshot(target)?;
        #[cfg(unix)]
        if current.sha256 != guard.sha256 || current.identity != guard.identity {
            return Err(PdfPersistenceError::InvalidDocument(
                "source PDF changed before save preparation".into(),
            ));
        }
        #[cfg(not(unix))]
        if current.sha256 != guard.sha256 {
            return Err(PdfPersistenceError::InvalidDocument(
                "source PDF changed before save preparation".into(),
            ));
        }
        self.prepare_save_inner(target, Some(guard))
    }
}

fn normalize_max_id_for_save(document: &mut Document) {
    document.max_id = document
        .objects
        .keys()
        .map(|(object_number, _)| *object_number)
        .max()
        .unwrap_or(0);
}

fn prune_unreachable_objects_for_save(document: &mut Document) -> Vec<ObjectId> {
    let removed = document.prune_objects();
    normalize_max_id_for_save(document);
    removed
}

fn append_annotation_reference(
    document: &mut Document,
    page_id: ObjectId,
    annotation_id: ObjectId,
) -> Result<(), PdfPersistenceError> {
    let annotations = document
        .get_object(page_id)?
        .as_dict()?
        .get(b"Annots")
        .ok()
        .cloned();
    match annotations {
        Some(Object::Reference(array_id)) => document
            .get_object_mut(array_id)?
            .as_array_mut()?
            .push(annotation_id.into()),
        Some(Object::Array(_)) => document
            .get_object_mut(page_id)?
            .as_dict_mut()?
            .get_mut(b"Annots")?
            .as_array_mut()?
            .push(annotation_id.into()),
        Some(_) => {
            return Err(PdfPersistenceError::InvalidDocument(
                "page /Annots must be an array or an indirect array".into(),
            ));
        }
        None => document
            .get_object_mut(page_id)?
            .as_dict_mut()?
            .set("Annots", vec![Object::Reference(annotation_id)]),
    }
    Ok(())
}

fn remove_annotation_reference(
    document: &mut Document,
    page_index: u32,
    annotation_id: ObjectId,
) -> Result<(), PdfPersistenceError> {
    let page_number = page_index.checked_add(1).ok_or_else(|| {
        PdfPersistenceError::InvalidDocument("page index exceeds the PDF page limit".into())
    })?;
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!("page {page_index} does not exist"))
        })?;
    let annotations = document
        .get_object(page_id)?
        .as_dict()?
        .get(b"Annots")
        .map_err(|_| {
            PdfPersistenceError::InvalidDocument(format!(
                "page {page_index} has no annotation array"
            ))
        })?
        .clone();
    let annotations = match annotations {
        Object::Reference(array_id) => document.get_object_mut(array_id)?.as_array_mut()?,
        Object::Array(_) => document
            .get_object_mut(page_id)?
            .as_dict_mut()?
            .get_mut(b"Annots")?
            .as_array_mut()?,
        _ => {
            return Err(PdfPersistenceError::InvalidDocument(
                "page /Annots must be an array or an indirect array".into(),
            ));
        }
    };
    let matching = annotations
        .iter()
        .enumerate()
        .filter_map(|(index, annotation)| {
            matches!(annotation, Object::Reference(candidate) if *candidate == annotation_id)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    if matching.len() != 1 {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "annotation object {annotation_id:?} does not have exactly one page reference"
        )));
    }
    annotations.remove(matching[0]);
    Ok(())
}

fn reorder_page_managed_annotation_references(
    document: &mut Document,
    page_index: u32,
    requested_groups: &[Vec<ObjectId>],
) -> Result<(), PdfPersistenceError> {
    let requested = requested_groups
        .iter()
        .flatten()
        .copied()
        .collect::<Vec<_>>();
    let requested_set = requested.iter().copied().collect::<HashSet<_>>();
    if requested.len() != requested_set.len() {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "page {page_index} managed annotation order contains duplicate objects"
        )));
    }
    let page_number = page_index.checked_add(1).ok_or_else(|| {
        PdfPersistenceError::InvalidDocument("page index exceeds the PDF page limit".into())
    })?;
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!("page {page_index} does not exist"))
        })?;
    let annotation_array = document
        .get_object(page_id)?
        .as_dict()?
        .get(b"Annots")
        .map_err(|_| {
            PdfPersistenceError::InvalidDocument(format!(
                "page {page_index} has no annotation array"
            ))
        })?
        .clone();
    let annotations = match annotation_array {
        Object::Reference(array_id) => document.get_object_mut(array_id)?.as_array_mut()?,
        Object::Array(_) => document
            .get_object_mut(page_id)?
            .as_dict_mut()?
            .get_mut(b"Annots")?
            .as_array_mut()?,
        _ => {
            return Err(PdfPersistenceError::InvalidDocument(
                "page /Annots must be an array or an indirect array".into(),
            ));
        }
    };
    let managed_slots = annotations
        .iter()
        .filter(|value| {
            matches!(value, Object::Reference(object_id) if requested_set.contains(object_id))
        })
        .count();
    if managed_slots != requested.len() {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "page {page_index} does not contain every managed annotation exactly once"
        )));
    }
    // An ordinary save requests the already-imported logical order. Do not
    // regroup physical members (notably separated Cloud+ text/cloud objects)
    // across opaque annotations when that order has not changed.
    let group_for_object = requested_groups
        .iter()
        .enumerate()
        .flat_map(|(index, group)| group.iter().map(move |id| (*id, index)))
        .collect::<HashMap<_, _>>();
    let mut seen_groups = HashSet::new();
    let current_groups = annotations
        .iter()
        .filter_map(|annotation| {
            let Object::Reference(id) = annotation else {
                return None;
            };
            let group = *group_for_object.get(id)?;
            seen_groups.insert(group).then_some(group)
        })
        .collect::<Vec<_>>();
    if current_groups.iter().copied().eq(0..requested_groups.len()) {
        return Ok(());
    }
    let mut ordered_groups = requested_groups.iter();
    let mut reordered = Vec::with_capacity(annotations.len());
    for value in std::mem::take(annotations) {
        if matches!(&value, Object::Reference(object_id) if requested_set.contains(object_id)) {
            if let Some(group) = ordered_groups.next() {
                reordered.extend(group.iter().copied().map(Object::Reference));
            }
        } else {
            reordered.push(value);
        }
    }
    if ordered_groups.next().is_some() {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "page {page_index} did not expose enough managed slots for logical annotation groups"
        )));
    }
    *annotations = reordered;
    Ok(())
}

/// Appends a markup this session wrote, with Revu's `/P` page reference.
fn append_markup_annotation(
    document: &mut Document,
    page_index: u32,
    mut dictionary: Dictionary,
) -> Result<ObjectId, PdfPersistenceError> {
    let page_id = validate_native_annotation_append_target(document, page_index)?;
    dictionary.set("P", page_id);
    append_native_annotation(document, page_index, dictionary)
}

fn append_native_annotation(
    document: &mut Document,
    page_index: u32,
    dictionary: Dictionary,
) -> Result<ObjectId, PdfPersistenceError> {
    let page_id = validate_native_annotation_append_target(document, page_index)?;
    let max_id_before = document.max_id;
    let annotation_id = document.add_object(dictionary);
    if let Err(error) = append_annotation_reference(document, page_id, annotation_id) {
        document.objects.remove(&annotation_id);
        document.max_id = max_id_before;
        return Err(error);
    }
    Ok(annotation_id)
}

fn validate_native_annotation_append_target(
    document: &Document,
    page_index: u32,
) -> Result<ObjectId, PdfPersistenceError> {
    let page_number = page_index.checked_add(1).ok_or_else(|| {
        PdfPersistenceError::InvalidDocument("page index exceeds the PDF page limit".into())
    })?;
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!("page {page_index} does not exist"))
        })?;
    let annotations = document.get_object(page_id)?.as_dict()?.get(b"Annots").ok();
    match annotations {
        Some(Object::Reference(array_id)) => {
            document.get_object(*array_id)?.as_array()?;
        }
        Some(Object::Array(_)) | None => {}
        Some(_) => {
            return Err(PdfPersistenceError::InvalidDocument(
                "page /Annots must be an array or an indirect array".into(),
            ));
        }
    }
    Ok(page_id)
}

#[cfg(test)]
mod straight_line_object_graph_tests {
    use super::*;

    #[test]
    fn malformed_annotation_array_rejects_before_allocating_any_pdf_object() {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Annots" => Object::Array(Vec::new()),
            }),
        );
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut session = PdfPersistenceSession::from_document(PathBuf::new(), document, None)
            .expect("the valid page must open before its annotation array is corrupted");
        session
            .document
            .get_object_mut(page_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Annots", 7);
        let before_objects = session.document.objects.clone();
        let before_trailer = session.document.trailer.clone();
        let before_max_id = session.document.max_id;

        let error = session
            .add_straight_line(
                StraightLineAnnotation::new(
                    MarkupId::new("malformed-annots-add").unwrap(),
                    0,
                    PdfPoint::new(72., 72.).unwrap(),
                    PdfPoint::new(144., 144.).unwrap(),
                    LineKind::Line,
                    StraightLineAppearance::default_for(LineKind::Line),
                )
                .unwrap(),
            )
            .unwrap_err();

        assert!(error.to_string().contains("page /Annots must be an array"));
        assert_eq!(session.document.objects, before_objects);
        assert_eq!(session.document.trailer, before_trailer);
        assert_eq!(session.document.max_id, before_max_id);
        assert!(session.straight_lines.is_empty());
        assert!(session.straight_line_native_identities.is_empty());
        assert!(session.annotation_order.is_empty());
    }
}

fn find_annotation_index<T>(
    values: &[T],
    id: &MarkupId,
    get_id: impl Fn(&T) -> &MarkupId,
    kind: &str,
) -> Result<usize, PdfPersistenceError> {
    values
        .iter()
        .position(|value| get_id(value) == id)
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!(
                "{kind} {id} is not imported from this document"
            ))
        })
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<(), std::io::Error> {
    File::open(path)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<(), std::io::Error> {
    Ok(())
}

fn annotation_object_id(
    document: &Document,
    page_index: u32,
    annotation_name: &str,
) -> Result<ObjectId, PdfPersistenceError> {
    let page_number = page_index.checked_add(1).ok_or_else(|| {
        PdfPersistenceError::InvalidDocument("page index exceeds the PDF page limit".into())
    })?;
    let page_id = document
        .get_pages()
        .get(&page_number)
        .copied()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!("page {page_index} does not exist"))
        })?;
    let page = document.get_object(page_id)?.as_dict()?;
    let annotations = resolve_object(document, page.get(b"Annots")?)?.as_array()?;
    for annotation in annotations {
        let Object::Reference(object_id) = annotation else {
            continue;
        };
        let dictionary = document.get_object(*object_id)?.as_dict()?;
        if dictionary_string(dictionary, b"NM").as_deref() == Some(annotation_name) {
            return Ok(*object_id);
        }
    }
    Err(PdfPersistenceError::InvalidDocument(format!(
        "annotation {annotation_name:?} is not an indirect object on page {page_index}",
    )))
}




fn text_box_annotation_bounds(annotation: &TextBoxAnnotation) -> PdfRect {
    let rotation = annotation.rotation_degrees();
    if rotation == 0.0 {
        return annotation.layout_rect;
    }
    let corners = rectangle_world_corners(annotation.layout_rect, rotation);
    let min_x = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        .expect("validated text box rotation has finite bounds")
}

fn text_box_appearance_matrix(annotation: &TextBoxAnnotation) -> Object {
    let rect = annotation.layout_rect;
    let rotation = annotation.rotation_degrees();
    if rotation == 0.0 {
        return Object::Array(vec![
            1.into(),
            0.into(),
            0.into(),
            1.into(),
            Object::Real((-rect.x) as f32),
            Object::Real((-rect.y) as f32),
        ]);
    }
    let radians = rotation.to_radians();
    let cosine = radians.cos();
    let sine = radians.sin();
    let center_x = rect.x + rect.width * 0.5;
    let center_y = rect.y + rect.height * 0.5;
    let unshifted_e = center_x - cosine * center_x - sine * center_y;
    let unshifted_f = center_y + sine * center_x - cosine * center_y;
    let bounds = text_box_annotation_bounds(annotation);
    Object::Array(vec![
        Object::Real(cosine as f32),
        Object::Real((-sine) as f32),
        Object::Real(sine as f32),
        Object::Real(cosine as f32),
        Object::Real((unshifted_e - bounds.x) as f32),
        Object::Real((unshifted_f - bounds.y) as f32),
    ])
}

/// Revu's placement for a box appearance that may be rotated clockwise:
/// `/BBox` is the unrotated box in page space and `/Matrix` rotates it and
/// moves its rotated bounds to the origin. Returns the BBox, the Matrix and
/// the annotation `/Rect` (the rotated bounds).
fn rotated_box_appearance_placement(
    bbox: PdfRect,
    rotation_degrees: f64,
) -> (Object, Object, PdfRect) {
    let radians = rotation_degrees.to_radians();
    let (sine, cosine) = if rotation_degrees.rem_euclid(360.) == 0. {
        (0., 1.)
    } else {
        radians.sin_cos()
    };
    let corners = [
        (bbox.x, bbox.y),
        (bbox.x + bbox.width, bbox.y),
        (bbox.x + bbox.width, bbox.y + bbox.height),
        (bbox.x, bbox.y + bbox.height),
    ]
    .map(|(x, y)| (cosine * x + sine * y, -sine * x + cosine * y));
    let min_x = corners.iter().map(|corner| corner.0).fold(f64::INFINITY, f64::min);
    let min_y = corners.iter().map(|corner| corner.1).fold(f64::INFINITY, f64::min);
    let rect = if rotation_degrees.rem_euclid(360.) == 0. {
        bbox
    } else {
        rotated_rect_bounds(bbox, rotation_degrees)
    };
    (
        pdf_rect(bbox),
        Object::Array(
            [cosine, -sine, sine, cosine, -min_x, -min_y]
                .into_iter()
                .map(|value| Object::Real(value as f32))
                .collect(),
        ),
        rect,
    )
}

fn rotated_rect_bounds(rect: PdfRect, rotation_degrees: f64) -> PdfRect {
    let corners = rectangle_world_corners(rect, rotation_degrees);
    let min_x = corners.iter().map(|point| point.x).fold(f64::INFINITY, f64::min);
    let max_x = corners.iter().map(|point| point.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|point| point.y).fold(f64::INFINITY, f64::min);
    let max_y = corners.iter().map(|point| point.y).fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        .expect("a validated rotation has finite bounds")
}

/// Revu's appearance placement for page-space drawing: `/BBox` is the
/// markup's `/Rect` and `/Matrix` moves it to the origin.
fn page_space_matrix(bounds: PdfRect) -> Object {
    Object::Array(vec![
        1.into(),
        0.into(),
        0.into(),
        1.into(),
        Object::Real((-bounds.x) as f32),
        Object::Real((-bounds.y) as f32),
    ])
}

fn union_rect(left: PdfRect, right: PdfRect) -> PdfRect {
    let min_x = left.x.min(right.x);
    let min_y = left.y.min(right.y);
    let max_x = (left.x + left.width).max(right.x + right.width);
    let max_y = (left.y + left.height).max(right.y + right.height);
    PdfRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        .expect("the union of finite rectangles is finite")
}

fn inflate_rect(rect: PdfRect, amount: f64) -> PdfRect {
    PdfRect::new(
        rect.x - amount,
        rect.y - amount,
        rect.width + amount * 2.,
        rect.height + amount * 2.,
    )
    .expect("a validated rectangle inflated by a finite margin stays finite")
}

/// Revu draws Square and Circle strokes inside the drawn rectangle and pads
/// `/Rect` by `/RD` = half the stroke width on each side.
fn shape_rect_differences(stroke_width_pt: f64) -> f64 {
    stroke_width_pt / 2.
}

fn rect_differences_array(value: f64) -> Object {
    Object::Array(vec![Object::Real(value as f32); 4])
}

fn rectangle_annotation_pdf_rect(annotation: &RectangleAnnotation) -> PdfRect {
    let bbox = inflate_rect(
        annotation.rect,
        shape_rect_differences(annotation.appearance.stroke_width_pt()),
    );
    rotated_box_appearance_placement(bbox, annotation.rotation_degrees).2
}

fn shape_paint_operations(appearance: &RectangleAppearance) -> (String, &'static str) {
    let (stroke_red, stroke_green, stroke_blue) = color_components(appearance.stroke_color());
    let fill = appearance.fill_color().map(color_components);
    let fill_operation = fill.map_or_else(String::new, |(red, green, blue)| {
        format!("{red:.6} {green:.6} {blue:.6} rg\n")
    });
    let dash_operation =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
            .map_or_else(String::new, |(dash, gap)| {
                format!("[{dash:.6} {gap:.6}] 0 d\n")
            });
    let graphics_state = if shape_is_translucent(appearance) { "/GS0 gs\n" } else { "" };
    (
        format!(
            "{graphics_state}{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n",
            appearance.stroke_width_pt(),
        ),
        if fill.is_some() { "B" } else { "S" },
    )
}

fn shape_is_translucent(appearance: &RectangleAppearance) -> bool {
    appearance.opacity() < 1. || (appearance.fill_color().is_some() && appearance.fill_opacity() < 1.)
}

/// Appearance resources as Revu writes them: `/ProcSet`, plus a graphics
/// state only when the markup is translucent.
fn shape_graphics_state(appearance: &RectangleAppearance) -> Dictionary {
    let mut resources = dictionary! { "ProcSet" => vec![Object::Name(b"PDF".to_vec())] };
    if shape_is_translucent(appearance) {
        resources.set(
            "ExtGState",
            dictionary! {
                "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(appearance.opacity() as f32),
                    "ca" => Object::Real(appearance.fill_opacity() as f32),
                },
            },
        );
    }
    resources
}

fn add_rectangle_appearance(document: &mut Document, annotation: &RectangleAnnotation) -> ObjectId {
    let appearance = &annotation.appearance;
    let half_width = shape_rect_differences(appearance.stroke_width_pt());
    let bbox = inflate_rect(annotation.rect, half_width);
    let (bbox_object, matrix, _) =
        rotated_box_appearance_placement(bbox, annotation.rotation_degrees);
    let (paint, paint_operator) = shape_paint_operations(appearance);
    let content = format!(
        "q\n{paint}{:.6} {:.6} {:.6} {:.6} re {paint_operator}\nQ\n",
        annotation.rect.x + half_width,
        annotation.rect.y + half_width,
        (annotation.rect.width - appearance.stroke_width_pt()).max(0.0),
        (annotation.rect.height - appearance.stroke_width_pt()).max(0.0),
    );
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => bbox_object,
            "Matrix" => matrix,
            "Resources" => shape_graphics_state(appearance),
        },
        content.into_bytes(),
    ))
}

fn rectangle_dash_pattern(style: StrokeStyle, width: f64) -> Option<(f64, f64)> {
    if width <= f64::EPSILON {
        return None;
    }
    match style {
        StrokeStyle::Solid => None,
        StrokeStyle::Dashed => Some((width * 4.0, width * 2.0)),
        StrokeStyle::Dotted => Some((width, width * 2.0)),
    }
}

fn rectangle_dictionary(
    annotation: &RectangleAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => pdf_rect(rectangle_annotation_pdf_rect(annotation)),
        "RD" => rect_differences_array(shape_rect_differences(appearance.stroke_width_pt())),
        "NM" => pdf_literal(annotation.id.as_str()),
        "Subj" => pdf_literal("Rectangle"),
        "C" => color_array(appearance.stroke_color()),
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_shape_border(&mut replacement, appearance);
    set_shape_fill(&mut replacement, appearance);
    set_markup_opacity(&mut replacement, appearance.opacity());
    set_markup_rotation(&mut replacement, annotation.rotation_degrees);
    preserve_markup_comment(&mut replacement, original);
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

/// Square, Circle, Polygon and PolyLine omit `/BS` at Revu's 1 pt solid default.
fn set_shape_border(dictionary: &mut Dictionary, appearance: &RectangleAppearance) {
    if appearance.stroke_width_pt() == 1. && appearance.stroke_style() == StrokeStyle::Solid {
        dictionary.remove(b"BS");
    } else {
        dictionary.set(
            "BS",
            markup_border_style(appearance.stroke_width_pt(), appearance.stroke_style()),
        );
    }
}

fn set_shape_fill(dictionary: &mut Dictionary, appearance: &RectangleAppearance) {
    if let Some(fill_color) = appearance.fill_color() {
        dictionary.set("IC", color_array(fill_color));
        set_markup_fill_opacity(dictionary, appearance.fill_opacity());
    } else {
        dictionary.remove(b"IC");
        dictionary.remove(b"FillOpacity");
    }
}

fn set_markup_rotation(dictionary: &mut Dictionary, rotation_degrees: f64) {
    let rotation = rotation_degrees.rem_euclid(360.);
    if rotation == 0. {
        dictionary.remove(b"Rotation");
    } else {
        dictionary.set("Rotation", Object::Real(rotation as f32));
    }
}

/// A user's subject and comment belong to the markup, not to Butter Paper's
/// tool, so an edit keeps whatever Revu or another editor recorded.
fn preserve_markup_comment(replacement: &mut Dictionary, original: &Dictionary) {
    for key in [b"Subj".as_slice(), b"Contents".as_slice(), b"RC".as_slice()] {
        if let Ok(value) = original.get(key) {
            replacement.set(key, value.clone());
        }
    }
}

/// An ISO 32000 pending `/Redact` mark. The covered content is untouched and
/// the mark carries no appearance; viewers draw their own pending style.
fn redact_dictionary(
    annotation: &RedactAnnotation,
    original: &Dictionary,
    native_name: &str,
) -> Result<Dictionary, PdfPersistenceError> {
    let left = annotation.rect.x;
    let bottom = annotation.rect.y;
    let right = left + annotation.rect.width;
    let top = bottom + annotation.rect.height;
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Redact",
        "Rect" => pdf_rect(annotation.rect),
        "QuadPoints" => vec![
            Object::Real(left as f32), Object::Real(top as f32),
            Object::Real(right as f32), Object::Real(top as f32),
            Object::Real(left as f32), Object::Real(bottom as f32),
            Object::Real(right as f32), Object::Real(bottom as f32),
        ],
        "IC" => color_array(annotation.redaction_color()),
        "NM" => pdf_literal(native_name),
        "Subj" => pdf_literal("Redaction"),
    };
    if let Some(overlay_text) = annotation.overlay_text() {
        replacement.set("OverlayText", pdf_literal(overlay_text));
    }
    preserve_markup_comment(&mut replacement, original);
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    replacement.remove(b"AP");
    Ok(replacement)
}


/// Revu strokes a Circle inside its drawn rectangle; Butter Paper strokes on
/// the ellipse rectangle. The drawn rectangle is therefore the ellipse
/// rectangle grown by half the stroke, and `/RD` pads it by another half.
fn ellipse_drawn_rect(annotation: &EllipseAnnotation) -> PdfRect {
    inflate_rect(annotation.rect, annotation.appearance.stroke_width_pt() / 2.)
}

fn ellipse_appearance_bbox(annotation: &EllipseAnnotation) -> PdfRect {
    inflate_rect(
        ellipse_drawn_rect(annotation),
        shape_rect_differences(annotation.appearance.stroke_width_pt()),
    )
}

fn add_ellipse_appearance(document: &mut Document, annotation: &EllipseAnnotation) -> ObjectId {
    let appearance = &annotation.appearance;
    let (bbox, matrix, _) = rotated_box_appearance_placement(
        ellipse_appearance_bbox(annotation),
        annotation.rotation_degrees,
    );
    let (start, segments) = ellipse_cubic_bezier_points(annotation.rect, 0.);
    let (paint, paint_operator) = shape_paint_operations(appearance);
    let mut content = format!("q\n{paint}{:.6} {:.6} m\n", start.x, start.y);
    for (control_a, control_b, to) in segments {
        content.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c\n",
            control_a.x, control_a.y, control_b.x, control_b.y, to.x, to.y,
        ));
    }
    content.push_str(&format!("h {paint_operator}\nQ\n"));
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => bbox,
            "Matrix" => matrix,
            "Resources" => shape_graphics_state(appearance),
        },
        content.into_bytes(),
    ))
}

fn ellipse_dictionary(
    annotation: &EllipseAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
    native_name: &str,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let (_, _, rect) = rotated_box_appearance_placement(
        ellipse_appearance_bbox(annotation),
        annotation.rotation_degrees,
    );
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Circle",
        "Rect" => pdf_rect(rect),
        "RD" => rect_differences_array(shape_rect_differences(appearance.stroke_width_pt())),
        "NM" => pdf_literal(native_name),
        "Subj" => pdf_literal("Ellipse"),
        "C" => color_array(appearance.stroke_color()),
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_shape_border(&mut replacement, appearance);
    set_shape_fill(&mut replacement, appearance);
    set_markup_opacity(&mut replacement, appearance.opacity());
    set_markup_rotation(&mut replacement, annotation.rotation_degrees);
    preserve_markup_comment(&mut replacement, original);
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

/// Like an Ellipse, a Revu Arc strokes inside its drawn rectangle; the arc's
/// ellipse rectangle is its stroke centreline.
fn arc_appearance_bbox(annotation: &ArcAnnotation) -> PdfRect {
    let width = annotation.appearance.stroke_width_pt();
    inflate_rect(annotation.rect(), width / 2. + shape_rect_differences(width))
}

fn add_arc_appearance(document: &mut Document, annotation: &ArcAnnotation) -> ObjectId {
    let appearance = &annotation.appearance;
    let bbox = arc_appearance_bbox(annotation);
    let path = arc_pdf_path_commands(
        annotation.rect(),
        annotation.angle1_degrees(),
        annotation.angle2_degrees(),
    );
    let (paint, _) = shape_paint_operations(&appearance.clone().without_fill());
    let content = format!("q\n{paint}{path}S\nQ\n").into_bytes();
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => pdf_rect(bbox),
            "Matrix" => vec![
                1.into(), 0.into(), 0.into(), 1.into(),
                Object::Real((-bbox.x) as f32), Object::Real((-bbox.y) as f32),
            ],
            "Resources" => shape_graphics_state(&appearance.clone().without_fill()),
        },
        content,
    ))
}

fn arc_pdf_path_commands(rect: PdfRect, angle1: f64, angle2: f64) -> String {
    let delta = normalize_arc_delta(angle1, angle2);
    let segment_count = ((delta.abs() / 22.5).ceil() as usize).max(1);
    let segment_delta = delta / segment_count as f64;
    let radius_x = rect.width * 0.5;
    let radius_y = rect.height * 0.5;
    let center_x = rect.x + radius_x;
    let center_y = rect.y + radius_y;
    let mut commands = String::new();
    for index in 0..segment_count {
        let start_angle = (angle1 + segment_delta * index as f64).to_radians();
        let end_angle = (angle1 + segment_delta * (index + 1) as f64).to_radians();
        let alpha = (4. / 3.) * ((end_angle - start_angle) / 4.).tan();
        let start = PdfPoint {
            x: center_x + radius_x * start_angle.cos(),
            y: center_y + radius_y * start_angle.sin(),
        };
        let end = PdfPoint {
            x: center_x + radius_x * end_angle.cos(),
            y: center_y + radius_y * end_angle.sin(),
        };
        let control1 = PdfPoint {
            x: start.x - alpha * radius_x * start_angle.sin(),
            y: start.y + alpha * radius_y * start_angle.cos(),
        };
        let control2 = PdfPoint {
            x: end.x + alpha * radius_x * end_angle.sin(),
            y: end.y - alpha * radius_y * end_angle.cos(),
        };
        if index == 0 {
            commands.push_str(&format!("{:.6} {:.6} m\n", start.x, start.y));
        }
        commands.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c\n",
            control1.x, control1.y, control2.x, control2.y, end.x, end.y,
        ));
    }
    commands
}

fn normalize_arc_delta(angle1: f64, angle2: f64) -> f64 {
    let mut delta = angle2 - angle1;
    while delta <= -360. {
        delta += 360.;
    }
    while delta > 360. {
        delta -= 360.;
    }
    delta
}

fn arc_dictionary(
    annotation: &ArcAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
    native_name: &str,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Circle",
        "IT" => "CircleArc",
        "Rect" => pdf_rect(arc_appearance_bbox(annotation)),
        "RD" => rect_differences_array(shape_rect_differences(appearance.stroke_width_pt())),
        "Angle1" => Object::Real(annotation.angle1_degrees() as f32),
        "Angle2" => Object::Real(annotation.angle2_degrees() as f32),
        "NM" => pdf_literal(native_name),
        "Subj" => pdf_literal("Arc"),
        "C" => color_array(appearance.stroke_color()),
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_shape_border(&mut replacement, appearance);
    set_markup_opacity(&mut replacement, appearance.opacity());
    preserve_markup_comment(&mut replacement, original);
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn pen_bounds(annotation: &PenAnnotation) -> PdfRect {
    // Revu pads Ink `/Rect` by 6.5 pt plus half the stroke.
    let half_width = annotation.appearance.width_pt() / 2.0 + 6.5;
    let min_x = annotation
        .paths()
        .flatten()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = annotation
        .paths()
        .flatten()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = annotation
        .paths()
        .flatten()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = annotation
        .paths()
        .flatten()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(
        min_x - half_width,
        min_y - half_width,
        max_x - min_x + half_width * 2.0,
        max_y - min_y + half_width * 2.0,
    )
    .expect("validated ink geometry must have finite bounds")
}

fn add_pen_appearance(document: &mut Document, annotation: &PenAnnotation) -> ObjectId {
    let bounds = pen_bounds(annotation);
    let (red, green, blue) = color_components(annotation.appearance.color());
    let mut content = format!(
        "q\n/GS0 gs\n1 J 1 j\n{red:.6} {green:.6} {blue:.6} RG\n{:.6} w\n",
        annotation.appearance.width_pt(),
    );
    for path in annotation.paths() {
        if let Some(first) = path.first() {
            content.push_str(&format!(
                "{:.6} {:.6} m\n",
                first.x - bounds.x,
                first.y - bounds.y
            ));
            for point in &path[1..] {
                content.push_str(&format!(
                    "{:.6} {:.6} l\n",
                    point.x - bounds.x,
                    point.y - bounds.y
                ));
            }
            // Each captured Highlight path is a separate translucent paint
            // operation. This preserves one coverage for self-overlap within a
            // path while allowing distinct gestures to multiply where they cross,
            // matching the live compositor and Electron's saved appearance.
            content.push_str("S\n");
        }
    }
    content.push_str("Q\n");
    let blend = match annotation.blend_mode() {
        BlendMode::Normal => "Normal",
        BlendMode::Multiply => "Multiply",
    };
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ExtGState" => dictionary! {
                    "GS0" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(annotation.appearance.opacity() as f32),
                        "ca" => Object::Real(annotation.appearance.opacity() as f32),
                        "BM" => blend,
                    },
                },
            },
        },
        content.into_bytes(),
    ))
}

fn pen_dictionary(
    annotation: &PenAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
    native_name: &str,
) -> Dictionary {
    let subject = match annotation.tool() {
        InkTool::Pen => "Pen",
        InkTool::Highlight => "Highlight",
    };
    let paths = annotation
        .paths()
        .map(|path| {
            Object::Array(
                path.iter()
                    .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
                    .collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Ink",
        "Rect" => pdf_rect(pen_bounds(annotation)),
        "NM" => pdf_literal(native_name),
        "Subj" => pdf_literal(subject),
        "InkList" => paths,
        "C" => color_array(annotation.appearance.color()),
        "BS" => markup_border_style(annotation.appearance.width_pt(), StrokeStyle::Solid),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if annotation.blend_mode() == BlendMode::Multiply {
        dictionary.set("BM", "Multiply");
    }
    set_markup_opacity(&mut dictionary, annotation.appearance.opacity());
    preserve_markup_comment(&mut dictionary, original);
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn add_standard_font(document: &mut Document) -> ObjectId {
    add_standard_font_variant(document, StandardTextFont::Regular)
}

fn add_standard_font_variant(document: &mut Document, font: StandardTextFont) -> ObjectId {
    document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => font.base_font(),
        "Encoding" => "WinAnsiEncoding",
    })
}

fn normalize_callout_leader(annotation: &CalloutAnnotation) -> Vec<PdfPoint> {
    let points = annotation.leader_points();
    let connection = *points
        .last()
        .expect("validated callout has a connection point");
    let tip = points[0];
    if points.len() <= 2 {
        vec![tip, connection]
    } else {
        let knee_index = (points.len() - 1).saturating_div(2).min(points.len() - 2);
        vec![tip, points[knee_index.max(1)], connection]
    }
}

fn add_callout_appearance(
    document: &mut Document,
    annotation: &CalloutAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let text = annotation.appearance.text();
    let mut lines = unicode_text_lines(annotation.content(), text.font_family()).map_err(|()| {
        PdfPersistenceError::InvalidDocument(
            "callout appearance cannot encode one or more Unicode characters with the bundled PDF fonts"
                .into(),
        )
    })?;
    let font_resources = appearance_text_font_resources(document, &mut lines)?;
    let leader = normalize_callout_leader(annotation);
    let bounds = annotation.disk_geometry()?.outer_rect;
    let line = annotation.appearance.line();
    let (line_red, line_green, line_blue) = color_components(line.stroke_color());
    let (text_red, text_green, text_blue) = color_components(text.color());
    let content = format!(
        "q\n/GS0 gs\n{line_red:.6} {line_green:.6} {line_blue:.6} RG\n{:.6} w\n",
        line.stroke_width_pt()
    );
    let mut content = content.into_bytes();
    if let Some(first) = leader.first() {
        content.extend_from_slice(
            format!("{:.6} {:.6} m\n", first.x - bounds.x, first.y - bounds.y).as_bytes(),
        );
        for point in leader.iter().skip(1) {
            content.extend_from_slice(
                format!("{:.6} {:.6} l\n", point.x - bounds.x, point.y - bounds.y).as_bytes(),
            );
        }
        content.extend_from_slice(b"S\n");
    }
    append_callout_box_border(&mut content, annotation.text_box, bounds, line.stroke_width_pt());
    if leader.len() >= 2 {
        let tip = leader[0];
        let next = leader[1];
        let dx = tip.x - next.x;
        let dy = tip.y - next.y;
        let distance = dx.hypot(dy);
        if distance > f64::EPSILON {
            let ux = dx / distance;
            let uy = dy / distance;
            let base_x = tip.x - ux * 10.;
            let base_y = tip.y - uy * 10.;
            let px = -uy * 3.5;
            let py = ux * 3.5;
            content.extend_from_slice(
                format!(
                    "{:.6} {:.6} m\n{:.6} {:.6} l\n{:.6} {:.6} l\nS\n",
                    base_x + px - bounds.x,
                    base_y + py - bounds.y,
                    tip.x - bounds.x,
                    tip.y - bounds.y,
                    base_x - px - bounds.x,
                    base_y - py - bounds.y,
                )
                .as_bytes(),
            );
        }
    }
    let line_height = text.font_size_pt() * 1.15;
    let total_height = line_height * lines.len() as f64;
    let start_y = annotation.text_box.y
        + ((annotation.text_box.height - total_height) * 0.5).max(0.)
        + total_height
        - text.font_size_pt();
    if annotation.content().is_empty() {
        // Revu allows a callout without text; it draws no text object.
    } else if uses_helvetica_winansi_fast_path(annotation.content(), text.font_family()) {
        content.extend_from_slice(
            format!(
                "BT\n/Helv {:.6} Tf\n{text_red:.6} {text_green:.6} {text_blue:.6} rg\n",
                text.font_size_pt()
            )
            .as_bytes(),
        );
        for (index, line_text) in annotation.content().split('\n').enumerate() {
            let encoded = text_appearance_line_bytes(line_text);
            content.extend_from_slice(
                format!(
                    "1 0 0 1 {:.6} {:.6} Tm\n(",
                    annotation.text_box.x + text.inset_pt() - bounds.x,
                    start_y - index as f64 * line_height - bounds.y,
                )
                .as_bytes(),
            );
            content.extend_from_slice(&escape_pdf_literal_bytes(&encoded));
            content.extend_from_slice(b") Tj\n");
        }
    } else {
        content.extend_from_slice(
            format!("BT\n{text_red:.6} {text_green:.6} {text_blue:.6} rg\n").as_bytes(),
        );
        for (index, line_text) in lines.iter().enumerate() {
            content.extend_from_slice(
                format!(
                    "1 0 0 1 {:.6} {:.6} Tm\n",
                    annotation.text_box.x + text.inset_pt() - bounds.x,
                    start_y - index as f64 * line_height - bounds.y,
                )
                .as_bytes(),
            );
            append_appearance_text_line(
                &mut content,
                line_text,
                text.font_size_pt(),
                annotation.text_box.x + text.inset_pt() - bounds.x,
                start_y - index as f64 * line_height - bounds.y,
            );
        }
    }
    if !annotation.content().is_empty() {
        content.extend_from_slice(b"ET\n");
    }
    content.extend_from_slice(b"Q\n");
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => font_resources,
                "ExtGState" => dictionary! { "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(line.opacity() as f32),
                    "ca" => Object::Real(line.opacity() as f32),
                } },
            },
        },
        content,
    )))
}

/// Revu strokes the callout text box whenever the border width is not the
/// default 1 pt leader (`BS W 0`).
fn append_callout_box_border(content: &mut Vec<u8>, text_box: PdfRect, bounds: PdfRect, width_pt: f64) {
    if revu_callout_border_width(width_pt) <= 0. {
        return;
    }
    let inset = width_pt * 0.5;
    content.extend_from_slice(
        format!(
            "{:.6} {:.6} {:.6} {:.6} re\nS\n",
            text_box.x + inset - bounds.x,
            text_box.y + inset - bounds.y,
            (text_box.width - width_pt).max(0.),
            (text_box.height - width_pt).max(0.),
        )
        .as_bytes(),
    );
}

/// Revu's callout border width: `0` draws the default 1 pt leader with no
/// box border; any other width is the leader and box border width.
fn revu_callout_border_width(leader_width_pt: f64) -> f64 {
    if leader_width_pt == 1. { 0. } else { leader_width_pt }
}

fn import_callout_leader_width(annotation: &Dictionary) -> f64 {
    let width = import_border_width(annotation);
    if width <= 0. { 1. } else { width }
}

fn callout_dictionary(
    annotation: &CalloutAnnotation,
    appearance_id: ObjectId,
    font_resources: Dictionary,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let geometry = annotation.disk_geometry()?;
    let leader = normalize_callout_leader(annotation);
    let text = annotation.appearance.text();
    let line = annotation.appearance.line();
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "IT" => "FreeTextCallout",
        "Rect" => pdf_rect(geometry.outer_rect),
        "RD" => geometry.rect_differences.into_iter().map(Object::Real).collect::<Vec<_>>(),
        "NM" => pdf_literal(annotation.id.as_str()),
        "Subj" => pdf_literal("Callout"),
        "Contents" => pdf_text_box_contents(annotation.content()),
        "CL" => leader
            .iter()
            .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
            .collect::<Vec<_>>(),
        "LE" => "OpenArrow",
        "DA" => pdf_literal(&revu_default_appearance(line.stroke_color(), text)),
        "DS" => pdf_literal(&revu_default_style(text, Some(text.inset_pt()))),
        "RC" => pdf_text_box_contents(&revu_rich_text(text, Some(text.inset_pt()), annotation.content(), true)),
        "BS" => markup_border_style(revu_callout_border_width(line.stroke_width_pt()), StrokeStyle::Solid),
        "C" => Vec::<Object>::new(),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if text.font_family() != "Helvetica" {
        dictionary.set("DR", dictionary! { "Font" => font_resources });
    }
    set_markup_opacity(&mut dictionary, line.opacity());
    if let Ok(subject) = original.get(b"Subj") {
        dictionary.set("Subj", subject.clone());
    }
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    Ok(dictionary)
}

const TEXT_APPEARANCE_LINE_HEIGHT_FACTOR: f64 = 1.15;

// Character advances for Base-14 Helvetica with WinAnsiEncoding, in thousandths
// of an em. Undefined control codes are zero; the appearance encoder replaces
// unsupported input with `?` before this table is consulted.
const HELVETICA_WIN_ANSI_WIDTHS: [u16; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
    350, 556, 350, 222, 556, 333, 1000, 556, 556, 333, 1000, 667, 333, 1000, 350, 611, 350, 350,
    222, 222, 333, 333, 350, 556, 1000, 333, 1000, 500, 333, 944, 350, 500, 667, 278, 333, 556,
    556, 556, 556, 260, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 556,
    537, 278, 333, 333, 365, 556, 834, 834, 834, 611, 667, 667, 667, 667, 667, 667, 1000, 722, 667,
    667, 667, 667, 278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722,
    722, 667, 667, 611, 556, 556, 556, 556, 556, 556, 889, 500, 556, 556, 556, 556, 278, 278, 278,
    278, 556, 556, 556, 556, 556, 556, 556, 584, 611, 556, 556, 556, 556, 500, 556, 500,
];

const HELVETICA_BOLD_WIN_ANSI_WIDTHS: [u16; 256] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667,
    611, 778, 722, 278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556,
    278, 889, 611, 611, 611, 611, 389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
    0, 556, 0, 278, 556, 500, 1000, 556, 556, 333, 1000, 667, 333, 1000, 0, 611, 0, 0, 278, 278,
    500, 500, 350, 556, 1000, 333, 1000, 556, 333, 944, 0, 500, 556, 278, 333, 556, 556, 556, 556,
    280, 556, 333, 737, 370, 556, 584, 333, 737, 333, 400, 584, 333, 333, 333, 611, 556, 278, 333,
    333, 365, 556, 834, 834, 834, 611, 722, 722, 722, 722, 722, 722, 1000, 722, 667, 667, 667, 667,
    278, 278, 278, 278, 722, 722, 778, 778, 778, 778, 778, 584, 778, 722, 722, 722, 722, 667, 667,
    611, 556, 556, 556, 556, 556, 556, 889, 556, 556, 556, 556, 556, 278, 278, 278, 278, 611, 611,
    611, 611, 611, 611, 611, 584, 611, 611, 611, 611, 611, 556, 611, 556,
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum StandardTextFont {
    Regular,
    Bold,
    Oblique,
    BoldOblique,
}

impl Default for StandardTextFont {
    fn default() -> Self {
        Self::Regular
    }
}

impl StandardTextFont {
    fn for_emphasis(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (false, false) => Self::Regular,
            (true, false) => Self::Bold,
            (false, true) => Self::Oblique,
            (true, true) => Self::BoldOblique,
        }
    }

    fn base_font(self) -> &'static str {
        match self {
            Self::Regular => "Helvetica",
            Self::Bold => "Helvetica-Bold",
            Self::Oblique => "Helvetica-Oblique",
            Self::BoldOblique => "Helvetica-BoldOblique",
        }
    }

    fn resource_name(self) -> &'static str {
        match self {
            Self::Regular => "Helv",
            Self::Bold => "HelvBld",
            Self::Oblique => "HelvOblique",
            Self::BoldOblique => "HelvBoldOblique",
        }
    }

    fn widths(self) -> &'static [u16; 256] {
        match self {
            Self::Regular | Self::Oblique => &HELVETICA_WIN_ANSI_WIDTHS,
            Self::Bold | Self::BoldOblique => &HELVETICA_BOLD_WIN_ANSI_WIDTHS,
        }
    }
}

fn text_appearance_line_bytes(line: &str) -> Vec<u8> {
    let encoding = Encoding::SimpleEncoding(b"WinAnsiEncoding");
    line.chars()
        .map(|character| {
            let encoded = encoding.string_to_bytes(&character.to_string());
            if encoded.len() == 1 { encoded[0] } else { b'?' }
        })
        .collect()
}

fn require_text_box_appearance_encodable(
    annotation: &TextBoxAnnotation,
) -> Result<(), PdfPersistenceError> {
    if !annotation.rich_text_runs().is_empty() {
        rich_text_appearance_lines(annotation).map(|_| ())?;
        return Ok(());
    }
    require_appearance_text_encodable(
        annotation.content(),
        annotation.style().font_family(),
        "text box",
    )
}

#[derive(Clone, Debug)]
struct RichTextAppearanceSpan {
    shaped_index: usize,
    font_size_pt: f64,
    color: String,
}

#[derive(Clone, Debug, Default)]
struct RichTextAppearanceLine {
    spans: Vec<RichTextAppearanceSpan>,
}

fn rich_text_appearance_lines(
    annotation: &TextBoxAnnotation,
) -> Result<(Vec<RichTextAppearanceLine>, Vec<UnicodeTextLine>), PdfPersistenceError> {
    let mut lines = vec![RichTextAppearanceLine::default()];
    let mut shaped = Vec::new();
    for run in annotation.rich_text_runs() {
        let family = run
            .font_family()
            .unwrap_or(annotation.style().font_family());
        let size = run
            .font_size_pt()
            .unwrap_or(annotation.style().font_size_pt());
        let color = run.color().unwrap_or(annotation.style().color()).to_owned();
        let parts = run.text().split('\n').collect::<Vec<_>>();
        for (index, part) in parts.iter().enumerate() {
            if !part.is_empty() {
                let line = unicode_text_line_styled(part, family, run.bold(), run.italic())
                    .map_err(|()| {
                        PdfPersistenceError::InvalidDocument(
                            "text box rich-text appearance cannot encode one or more Unicode characters with the bundled PDF fonts"
                                .into(),
                        )
                    })?;
                let shaped_index = shaped.len();
                shaped.push(line);
                lines
                    .last_mut()
                    .expect("rich text layout always retains one line")
                    .spans
                    .push(RichTextAppearanceSpan {
                        shaped_index,
                        font_size_pt: size,
                        color: color.clone(),
                    });
            }
            if index + 1 < parts.len() {
                lines.push(RichTextAppearanceLine::default());
            }
        }
    }
    Ok((lines, shaped))
}

fn require_appearance_text_encodable(
    content: &str,
    font_family: &str,
    annotation_kind: &str,
) -> Result<(), PdfPersistenceError> {
    if unicode_text_lines(content, font_family).is_ok() {
        return Ok(());
    }
    Err(PdfPersistenceError::InvalidDocument(format!(
        "{annotation_kind} appearance cannot encode one or more Unicode characters with the bundled PDF fonts"
    )))
}

fn text_content_is_winansi(content: &str) -> bool {
    let encoding = Encoding::SimpleEncoding(b"WinAnsiEncoding");
    content.chars().all(|character| {
        character == '\n' || encoding.string_to_bytes(&character.to_string()).len() == 1
    })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum EmbeddedTextFont {
    ArimoRegular,
    ArimoBold,
    ArimoItalic,
    ArimoBoldItalic,
    RobotoMonoRegular,
    RobotoMonoBold,
    RobotoMonoItalic,
    RobotoMonoBoldItalic,
    TinosRegular,
    TinosBold,
    TinosItalic,
    TinosBoldItalic,
    Sans,
    Emoji,
}

impl EmbeddedTextFont {
    fn data(self) -> &'static [u8] {
        match self {
            Self::ArimoRegular => include_bytes!("../assets/fonts/Arimo-Regular.ttf"),
            Self::ArimoBold => include_bytes!("../assets/fonts/Arimo-Bold.ttf"),
            Self::ArimoItalic => include_bytes!("../assets/fonts/Arimo-Italic.ttf"),
            Self::ArimoBoldItalic => include_bytes!("../assets/fonts/Arimo-BoldItalic.ttf"),
            Self::RobotoMonoRegular => include_bytes!("../assets/fonts/RobotoMono-Regular.ttf"),
            Self::RobotoMonoBold => include_bytes!("../assets/fonts/RobotoMono-Bold.ttf"),
            Self::RobotoMonoItalic => include_bytes!("../assets/fonts/RobotoMono-Italic.ttf"),
            Self::RobotoMonoBoldItalic => {
                include_bytes!("../assets/fonts/RobotoMono-BoldItalic.ttf")
            }
            Self::TinosRegular => include_bytes!("../assets/fonts/Tinos-Regular.ttf"),
            Self::TinosBold => include_bytes!("../assets/fonts/Tinos-Bold.ttf"),
            Self::TinosItalic => include_bytes!("../assets/fonts/Tinos-Italic.ttf"),
            Self::TinosBoldItalic => include_bytes!("../assets/fonts/Tinos-BoldItalic.ttf"),
            Self::Sans => include_bytes!("../assets/fonts/NotoSansSC-Regular.ttf"),
            Self::Emoji => include_bytes!("../assets/fonts/NotoEmoji-Regular.ttf"),
        }
    }

    fn pdf_name(self) -> &'static str {
        match self {
            Self::ArimoRegular => "Arimo-Regular",
            Self::ArimoBold => "Arimo-Bold",
            Self::ArimoItalic => "Arimo-Italic",
            Self::ArimoBoldItalic => "Arimo-BoldItalic",
            Self::RobotoMonoRegular => "RobotoMono-Regular",
            Self::RobotoMonoBold => "RobotoMono-Bold",
            Self::RobotoMonoItalic => "RobotoMono-Italic",
            Self::RobotoMonoBoldItalic => "RobotoMono-BoldItalic",
            Self::TinosRegular => "Tinos-Regular",
            Self::TinosBold => "Tinos-Bold",
            Self::TinosItalic => "Tinos-Italic",
            Self::TinosBoldItalic => "Tinos-BoldItalic",
            Self::Sans => "NotoSansSC-Thin",
            Self::Emoji => "NotoEmoji-Regular",
        }
    }

    fn resource_name(self) -> &'static str {
        match self {
            Self::ArimoRegular => "Arimo",
            Self::ArimoBold => "ArimoBold",
            Self::ArimoItalic => "ArimoOblique",
            Self::ArimoBoldItalic => "ArimoBoldOblique",
            Self::RobotoMonoRegular => "RobotoMono",
            Self::RobotoMonoBold => "RobotoMonoBold",
            Self::RobotoMonoItalic => "RobotoMonoOblique",
            Self::RobotoMonoBoldItalic => "RobotoMonoBoldOblique",
            Self::TinosRegular => "Tinos",
            Self::TinosBold => "TinosBold",
            Self::TinosItalic => "TinosOblique",
            Self::TinosBoldItalic => "TinosBoldOblique",
            Self::Sans => "NotoSansSC",
            Self::Emoji => "NotoEmoji",
        }
    }
}

fn embedded_annotation_font(font_family: &str) -> Option<EmbeddedTextFont> {
    embedded_annotation_font_variant(font_family, false, false)
}

fn embedded_annotation_font_variant(
    font_family: &str,
    bold: bool,
    italic: bool,
) -> Option<EmbeddedTextFont> {
    match font_family {
        "Arimo" => Some(match (bold, italic) {
            (false, false) => EmbeddedTextFont::ArimoRegular,
            (true, false) => EmbeddedTextFont::ArimoBold,
            (false, true) => EmbeddedTextFont::ArimoItalic,
            (true, true) => EmbeddedTextFont::ArimoBoldItalic,
        }),
        "Roboto Mono" => Some(match (bold, italic) {
            (false, false) => EmbeddedTextFont::RobotoMonoRegular,
            (true, false) => EmbeddedTextFont::RobotoMonoBold,
            (false, true) => EmbeddedTextFont::RobotoMonoItalic,
            (true, true) => EmbeddedTextFont::RobotoMonoBoldItalic,
        }),
        "Tinos" => Some(match (bold, italic) {
            (false, false) => EmbeddedTextFont::TinosRegular,
            (true, false) => EmbeddedTextFont::TinosBold,
            (false, true) => EmbeddedTextFont::TinosItalic,
            (true, true) => EmbeddedTextFont::TinosBoldItalic,
        }),
        _ => None,
    }
}

fn appearance_font_resource_name(font_family: &str) -> &'static str {
    embedded_annotation_font(font_family)
        .map(EmbeddedTextFont::resource_name)
        .unwrap_or("Helv")
}

fn uses_helvetica_winansi_fast_path(content: &str, font_family: &str) -> bool {
    embedded_annotation_font(font_family).is_none() && text_content_is_winansi(content)
}

#[derive(Clone, Debug, PartialEq)]
struct UnicodeTextGlyph {
    font: Option<EmbeddedTextFont>,
    glyph_id: u16,
    cid: u16,
    source_start: usize,
    source_order: usize,
    unicode: Option<String>,
    x_em: f64,
    y_em: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct UnicodeTextLine {
    source: String,
    glyphs: Vec<UnicodeTextGlyph>,
    width_em: f64,
    standard_font: StandardTextFont,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct EmbeddedCidGlyph {
    glyph_id: u16,
    unicode: Option<String>,
}

fn rustybuzz_direction(rtl: bool) -> rustybuzz::Direction {
    if rtl {
        rustybuzz::Direction::RightToLeft
    } else {
        rustybuzz::Direction::LeftToRight
    }
}

fn shape_buffer(text: &str, font: EmbeddedTextFont, rtl: bool) -> rustybuzz::GlyphBuffer {
    let face = rustybuzz::Face::from_slice(font.data(), 0)
        .expect("prepared Noto font must remain valid for shaping");
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(rustybuzz_direction(rtl));
    buffer.set_cluster_level(rustybuzz::BufferClusterLevel::MonotoneGraphemes);
    buffer.guess_segment_properties();
    rustybuzz::shape(&face, &[], buffer)
}

fn shaping_cluster_ranges(text: &str, rtl: bool) -> Vec<std::ops::Range<usize>> {
    if text.is_empty() {
        return Vec::new();
    }
    let shaped = shape_buffer(text, EmbeddedTextFont::Sans, rtl);
    let mut starts = shaped
        .glyph_infos()
        .iter()
        .map(|info| info.cluster as usize)
        .filter(|start| *start < text.len())
        .collect::<BTreeSet<_>>();
    starts.insert(0);
    starts.insert(text.len());
    let starts = starts.into_iter().collect::<Vec<_>>();
    starts.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

fn emoji_presentation_cluster(text: &str) -> bool {
    text.contains('\u{fe0f}')
        || text.contains('\u{200d}')
        || text.contains('\u{20e3}')
        || text
            .chars()
            .any(|character| matches!(character as u32, 0x1f1e6..=0x1f1ff | 0x1f3fb..=0x1f3ff))
}

fn cluster_font(
    text: &str,
    rtl: bool,
    primary: Option<EmbeddedTextFont>,
) -> Result<Option<EmbeddedTextFont>, ()> {
    let mut order = Vec::with_capacity(3);
    if emoji_presentation_cluster(text) {
        order.push(EmbeddedTextFont::Emoji);
    }
    if let Some(primary) = primary {
        order.push(primary);
    }
    order.push(EmbeddedTextFont::Sans);
    order.push(EmbeddedTextFont::Emoji);
    order.dedup();
    if let Some(font) = order.into_iter().find(|font| {
        shape_buffer(text, *font, rtl)
            .glyph_infos()
            .iter()
            .all(|glyph| glyph.glyph_id != 0)
    }) {
        return Ok(Some(font));
    }
    text_content_is_winansi(text).then_some(None).ok_or(())
}

fn shape_winansi_span(
    text: &str,
    source_offset: usize,
    rtl: bool,
    standard_font: StandardTextFont,
) -> Result<(Vec<UnicodeTextGlyph>, f64), ()> {
    let encoding = Encoding::SimpleEncoding(b"WinAnsiEncoding");
    let mut characters = text.char_indices().collect::<Vec<_>>();
    if rtl {
        characters.reverse();
    }
    let mut x_em = 0.;
    let mut glyphs = Vec::with_capacity(characters.len());
    for (source_order, (source_start, character)) in characters.into_iter().enumerate() {
        let encoded = encoding.string_to_bytes(&character.to_string());
        if encoded.len() != 1 {
            return Err(());
        }
        let byte = encoded[0];
        glyphs.push(UnicodeTextGlyph {
            font: None,
            glyph_id: u16::from(byte),
            cid: u16::from(byte),
            source_start: source_offset + source_start,
            source_order,
            unicode: Some(character.to_string()),
            x_em,
            y_em: 0.,
        });
        x_em += f64::from(standard_font.widths()[usize::from(byte)]) / 1000.;
    }
    Ok((glyphs, x_em))
}

fn shape_embedded_span(
    text: &str,
    source_offset: usize,
    font: EmbeddedTextFont,
    rtl: bool,
) -> Result<(Vec<UnicodeTextGlyph>, f64), ()> {
    let face = rustybuzz::Face::from_slice(font.data(), 0).ok_or(())?;
    let units_per_em = f64::from(face.units_per_em());
    let shaped = shape_buffer(text, font, rtl);
    if shaped.glyph_infos().iter().any(|glyph| glyph.glyph_id == 0) {
        return Err(());
    }
    let mut cluster_starts = shaped
        .glyph_infos()
        .iter()
        .map(|glyph| glyph.cluster as usize)
        .collect::<BTreeSet<_>>();
    cluster_starts.insert(text.len());
    let cluster_starts = cluster_starts.into_iter().collect::<Vec<_>>();
    let cluster_end = |start: usize| {
        cluster_starts
            .iter()
            .copied()
            .find(|candidate| *candidate > start)
            .unwrap_or(text.len())
    };
    let mut pen_x = 0.;
    let mut pen_y = 0.;
    let mut min_pen_x = 0_f64;
    let mut max_pen_x = 0_f64;
    let mut seen_clusters = BTreeSet::new();
    let mut glyphs = Vec::with_capacity(shaped.len());
    for (source_order, (info, position)) in shaped
        .glyph_infos()
        .iter()
        .zip(shaped.glyph_positions())
        .enumerate()
    {
        let cluster_start = info.cluster as usize;
        let unicode = seen_clusters
            .insert(cluster_start)
            .then(|| text[cluster_start..cluster_end(cluster_start)].to_owned());
        glyphs.push(UnicodeTextGlyph {
            font: Some(font),
            glyph_id: info.glyph_id as u16,
            cid: 0,
            source_start: source_offset + cluster_start,
            source_order,
            unicode,
            x_em: (pen_x + f64::from(position.x_offset)) / units_per_em,
            y_em: (pen_y + f64::from(position.y_offset)) / units_per_em,
        });
        pen_x += f64::from(position.x_advance);
        pen_y += f64::from(position.y_advance);
        min_pen_x = min_pen_x.min(pen_x);
        max_pen_x = max_pen_x.max(pen_x);
    }
    let shift_x = -min_pen_x / units_per_em;
    for glyph in &mut glyphs {
        glyph.x_em += shift_x;
    }
    Ok((glyphs, (max_pen_x - min_pen_x) / units_per_em))
}

fn unicode_text_line(line: &str, font_family: &str) -> Result<UnicodeTextLine, ()> {
    unicode_text_line_styled(line, font_family, false, false)
}

fn unicode_text_line_styled(
    line: &str,
    font_family: &str,
    bold: bool,
    italic: bool,
) -> Result<UnicodeTextLine, ()> {
    let primary = embedded_annotation_font_variant(font_family, bold, italic);
    let standard_font = StandardTextFont::for_emphasis(bold, italic);
    if primary.is_none() && text_content_is_winansi(line) {
        let mut x_em = 0.;
        let glyphs = line
            .char_indices()
            .map(|(source_start, character)| {
                let byte = text_appearance_line_bytes(&character.to_string())[0];
                let glyph = UnicodeTextGlyph {
                    font: None,
                    glyph_id: u16::from(byte),
                    cid: u16::from(byte),
                    source_start,
                    source_order: 0,
                    unicode: Some(character.to_string()),
                    x_em,
                    y_em: 0.,
                };
                x_em += f64::from(standard_font.widths()[usize::from(byte)]) / 1000.;
                glyph
            })
            .collect();
        return Ok(UnicodeTextLine {
            source: line.to_owned(),
            glyphs,
            width_em: x_em,
            standard_font,
        });
    }

    let bidi = unicode_bidi::BidiInfo::new(line, None);
    let mut glyphs = Vec::new();
    let mut line_x = 0.;
    for paragraph in &bidi.paragraphs {
        let (levels, visual_runs) = bidi.visual_runs(paragraph, paragraph.range.clone());
        for visual_run in visual_runs {
            if visual_run.is_empty() {
                continue;
            }
            let rtl = levels[visual_run.start].is_rtl();
            let run_text = &line[visual_run.clone()];
            let mut font_runs = Vec::<(std::ops::Range<usize>, Option<EmbeddedTextFont>)>::new();
            for cluster in shaping_cluster_ranges(run_text, rtl) {
                let font = cluster_font(&run_text[cluster.clone()], rtl, primary)?;
                if let Some((range, current_font)) = font_runs.last_mut()
                    && *current_font == font
                    && range.end == cluster.start
                {
                    range.end = cluster.end;
                } else {
                    font_runs.push((cluster, font));
                }
            }
            if rtl {
                font_runs.reverse();
            }
            for (range, font) in font_runs {
                let span = &run_text[range.clone()];
                let source_offset = visual_run.start + range.start;
                let (mut shaped, width_em) = if let Some(font) = font {
                    shape_embedded_span(span, source_offset, font, rtl)?
                } else {
                    shape_winansi_span(span, source_offset, rtl, standard_font)?
                };
                for glyph in &mut shaped {
                    glyph.x_em += line_x;
                }
                glyphs.extend(shaped);
                line_x += width_em;
            }
        }
    }
    Ok(UnicodeTextLine {
        source: line.to_owned(),
        glyphs,
        width_em: line_x,
        standard_font,
    })
}

fn unicode_text_lines(content: &str, font_family: &str) -> Result<Vec<UnicodeTextLine>, ()> {
    content
        .split('\n')
        .map(|line| unicode_text_line(line, font_family))
        .collect()
}

fn unicode_text_width_pt(line: &UnicodeTextLine, font_size_pt: f64) -> f64 {
    line.width_em * font_size_pt
}

fn appearance_text_width_pt(content: &str, font_size_pt: f64, font_family: &str) -> f64 {
    if uses_helvetica_winansi_fast_path(content, font_family) {
        return helvetica_text_width_pt(&text_appearance_line_bytes(content), font_size_pt);
    }
    let lines = unicode_text_lines(content, font_family)
        .expect("appearance text must be validated before its width is measured");
    unicode_text_width_pt(
        lines.first().unwrap_or(&UnicodeTextLine::default()),
        font_size_pt,
    )
}

fn appearance_text_font_resources(
    document: &mut Document,
    lines: &mut [UnicodeTextLine],
) -> Result<Dictionary, PdfPersistenceError> {
    let mut fonts = Dictionary::new();
    let standard_fonts = lines
        .iter()
        .flat_map(|line| {
            line.glyphs
                .iter()
                .map(move |glyph| (line.standard_font, glyph))
        })
        .filter_map(|(font, glyph)| glyph.font.is_none().then_some(font))
        .collect::<BTreeSet<_>>();
    for font in standard_fonts {
        fonts.set(
            font.resource_name(),
            add_standard_font_variant(document, font),
        );
    }
    let mut requested = BTreeMap::<EmbeddedTextFont, BTreeSet<EmbeddedCidGlyph>>::new();
    for glyph in lines.iter().flat_map(|line| &line.glyphs) {
        if let Some(font) = glyph.font {
            requested.entry(font).or_default().insert(EmbeddedCidGlyph {
                glyph_id: glyph.glyph_id,
                unicode: glyph.unicode.clone(),
            });
        }
    }
    let mut assignments = BTreeMap::new();
    for (font, glyphs) in requested {
        let (font_id, assigned) = add_embedded_unicode_font(document, font, &glyphs)?;
        fonts.set(font.resource_name(), font_id);
        assignments.insert(font, assigned);
    }
    for glyph in lines.iter_mut().flat_map(|line| &mut line.glyphs) {
        if let Some(font) = glyph.font {
            glyph.cid = assignments[&font][&EmbeddedCidGlyph {
                glyph_id: glyph.glyph_id,
                unicode: glyph.unicode.clone(),
            }];
        }
    }
    Ok(fonts)
}

fn ensure_default_text_font_resource(
    document: &mut Document,
    fonts: &mut Dictionary,
    font_family: &str,
) -> Result<(), PdfPersistenceError> {
    let resource_name = appearance_font_resource_name(font_family);
    if fonts.has(resource_name.as_bytes()) {
        return Ok(());
    }
    let font_id = if let Some(font) = embedded_annotation_font(font_family) {
        add_embedded_unicode_font(document, font, &BTreeSet::new())?.0
    } else {
        add_standard_font(document)
    };
    fonts.set(resource_name, font_id);
    Ok(())
}

fn append_appearance_text_line(
    content: &mut Vec<u8>,
    line: &UnicodeTextLine,
    font_size_pt: f64,
    origin_x: f64,
    origin_y: f64,
) {
    content.extend_from_slice(
        format!(
            "/Span << /ActualText <FEFF{}> >> BDC\n",
            unicode_destination_hex(&line.source)
        )
        .as_bytes(),
    );
    let mut logical = line.glyphs.iter().collect::<Vec<_>>();
    logical.sort_by_key(|glyph| (glyph.source_start, glyph.source_order));
    for glyph in logical {
        let x = origin_x + glyph.x_em * font_size_pt;
        let y = origin_y + glyph.y_em * font_size_pt;
        if let Some(font) = glyph.font {
            content.extend_from_slice(
                format!(
                    "/{} {font_size_pt:.6} Tf\n1 0 0 1 {x:.6} {y:.6} Tm\n<{:04X}> Tj\n",
                    font.resource_name(),
                    glyph.cid,
                )
                .as_bytes(),
            );
        } else {
            content.extend_from_slice(
                format!(
                    "/{} {font_size_pt:.6} Tf\n1 0 0 1 {x:.6} {y:.6} Tm\n(",
                    line.standard_font.resource_name()
                )
                .as_bytes(),
            );
            content.extend_from_slice(&escape_pdf_literal_bytes(&[glyph.glyph_id as u8]));
            content.extend_from_slice(b") Tj\n");
        }
    }
    content.extend_from_slice(b"EMC\n");
}

fn pdf_font_units(value: i16, units_per_em: u16) -> i64 {
    (f64::from(value) * 1000. / f64::from(units_per_em)).round() as i64
}

fn unicode_destination_hex(value: &str) -> String {
    value
        .encode_utf16()
        .map(|unit| format!("{unit:04X}"))
        .collect()
}

fn add_embedded_unicode_font(
    document: &mut Document,
    font: EmbeddedTextFont,
    requested: &BTreeSet<EmbeddedCidGlyph>,
) -> Result<(ObjectId, BTreeMap<EmbeddedCidGlyph, u16>), PdfPersistenceError> {
    let data = font.data();
    let face = ttf_parser::Face::parse(data, 0).expect("prepared Noto font must remain valid");
    let units_per_em = face.units_per_em();
    let existing_id = document.objects.iter().find_map(|(object_id, object)| {
        let dictionary = object.as_dict().ok()?;
        is_bundled_embedded_font(document, dictionary, font).then_some(*object_id)
    });
    if let Some(existing_id) = existing_id {
        let existing = document
            .get_object(existing_id)
            .expect("located embedded font must remain present")
            .as_dict()
            .expect("located embedded font must remain a dictionary")
            .clone();
        let descendant_id = existing
            .get(b"DescendantFonts")
            .and_then(Object::as_array)
            .map(|descendants| {
                descendants
                    .first()
                    .expect("Butter Paper embedded font must retain one descendant")
            })
            .and_then(Object::as_reference)
            .expect("Butter Paper embedded font must retain its descendant");
        let to_unicode_id = existing
            .get(b"ToUnicode")
            .and_then(Object::as_reference)
            .expect("Butter Paper embedded font must retain its ToUnicode map");
        let mut merged = embedded_cid_mapping_from_standard_font(document, &existing);
        let mut reverse = merged
            .iter()
            .map(|(cid, glyph)| (glyph.clone(), *cid))
            .collect::<BTreeMap<_, _>>();
        let mut next_cid = merged
            .keys()
            .next_back()
            .copied()
            .map_or(Some(1), |cid| cid.checked_add(1));
        for glyph in requested {
            if !reverse.contains_key(glyph) {
                let cid = next_cid.ok_or_else(|| {
                    PdfPersistenceError::InvalidDocument(
                        "embedded PDF font exhausted its 16-bit CID space".into(),
                    )
                })?;
                merged.insert(cid, glyph.clone());
                reverse.insert(glyph.clone(), cid);
                next_cid = cid.checked_add(1);
            }
        }
        let cid_to_gid_id = existing_cid_to_gid_map(document, descendant_id, &merged);
        document
            .get_object_mut(descendant_id)
            .expect("Butter Paper descendant font must remain present")
            .as_dict_mut()
            .expect("Butter Paper descendant font must remain a dictionary")
            .set("W", Object::Array(unicode_font_widths(&face, &merged)));
        document
            .get_object_mut(descendant_id)
            .expect("Butter Paper descendant font must remain present")
            .as_dict_mut()
            .expect("Butter Paper descendant font must remain a dictionary")
            .set("CIDToGIDMap", cid_to_gid_id);
        document
            .get_object_mut(to_unicode_id)
            .expect("Butter Paper ToUnicode map must remain present")
            .as_stream_mut()
            .expect("Butter Paper ToUnicode map must remain a stream")
            .set_content(unicode_to_unicode_cmap(&merged).into_bytes());
        return Ok((existing_id, reverse));
    }
    let merged = requested
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, glyph)| {
            u16::try_from(index + 1)
                .map(|cid| (cid, glyph))
                .map_err(|_| {
                    PdfPersistenceError::InvalidDocument(
                        "embedded PDF font exhausted its 16-bit CID space".into(),
                    )
                })
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let reverse = merged
        .iter()
        .map(|(cid, glyph)| (glyph.clone(), *cid))
        .collect::<BTreeMap<_, _>>();
    let bbox = face.global_bounding_box();
    let font_file_id = document.add_object(Stream::new(
        dictionary! { "Length1" => data.len() as i64 },
        data.to_vec(),
    ));
    let descriptor_id = document.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(font.pdf_name().as_bytes().to_vec()),
        "Flags" => 32,
        "FontBBox" => vec![
            Object::Integer(pdf_font_units(bbox.x_min, units_per_em)),
            Object::Integer(pdf_font_units(bbox.y_min, units_per_em)),
            Object::Integer(pdf_font_units(bbox.x_max, units_per_em)),
            Object::Integer(pdf_font_units(bbox.y_max, units_per_em)),
        ],
        "ItalicAngle" => Object::Real(face.italic_angle()),
        "Ascent" => pdf_font_units(face.ascender(), units_per_em),
        "Descent" => pdf_font_units(face.descender(), units_per_em),
        "CapHeight" => pdf_font_units(face.capital_height().unwrap_or(face.ascender()), units_per_em),
        "StemV" => 80,
        "FontFile2" => font_file_id,
    });
    let widths = unicode_font_widths(&face, &merged);
    let cid_to_gid_id = document.add_object(Stream::new(
        Dictionary::new(),
        cid_to_gid_map_bytes(&merged),
    ));
    let descendant_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(font.pdf_name().as_bytes().to_vec()),
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::String(b"Adobe".to_vec(), StringFormat::Literal),
            "Ordering" => Object::String(b"Identity".to_vec(), StringFormat::Literal),
            "Supplement" => 0,
        },
        "FontDescriptor" => descriptor_id,
        "CIDToGIDMap" => cid_to_gid_id,
        "DW" => 1000,
        "W" => Object::Array(widths),
    });
    let to_unicode_id = document.add_object(Stream::new(
        Dictionary::new(),
        unicode_to_unicode_cmap(&merged).into_bytes(),
    ));
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(font.pdf_name().as_bytes().to_vec()),
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(descendant_id)],
        "ToUnicode" => to_unicode_id,
    });
    Ok((font_id, reverse))
}

fn existing_cid_to_gid_map(
    document: &mut Document,
    descendant_id: ObjectId,
    glyphs: &BTreeMap<u16, EmbeddedCidGlyph>,
) -> ObjectId {
    let existing = document
        .get_object(descendant_id)
        .expect("Butter Paper descendant font must remain present")
        .as_dict()
        .expect("Butter Paper descendant font must remain a dictionary")
        .get(b"CIDToGIDMap")
        .ok()
        .and_then(|value| value.as_reference().ok());
    if let Some(existing) = existing {
        document
            .get_object_mut(existing)
            .expect("Butter Paper CIDToGIDMap must remain present")
            .as_stream_mut()
            .expect("Butter Paper CIDToGIDMap must remain a stream")
            .set_content(cid_to_gid_map_bytes(glyphs));
        existing
    } else {
        document.add_object(Stream::new(Dictionary::new(), cid_to_gid_map_bytes(glyphs)))
    }
}

fn cid_to_gid_map_bytes(glyphs: &BTreeMap<u16, EmbeddedCidGlyph>) -> Vec<u8> {
    let max_cid = glyphs.keys().next_back().copied().unwrap_or(0);
    let mut bytes = vec![0; (usize::from(max_cid) + 1) * 2];
    for (cid, glyph) in glyphs {
        let offset = usize::from(*cid) * 2;
        bytes[offset..offset + 2].copy_from_slice(&glyph.glyph_id.to_be_bytes());
    }
    bytes
}

fn unicode_font_widths(
    face: &ttf_parser::Face<'_>,
    glyphs: &BTreeMap<u16, EmbeddedCidGlyph>,
) -> Vec<Object> {
    let units_per_em = face.units_per_em();
    glyphs
        .iter()
        .flat_map(|(cid, mapped)| {
            let glyph = ttf_parser::GlyphId(mapped.glyph_id);
            let width = face.glyph_hor_advance(glyph).unwrap_or_default();
            [
                Object::Integer(i64::from(*cid)),
                Object::Array(vec![Object::Integer(
                    (f64::from(width) * 1000. / f64::from(units_per_em)).round() as i64,
                )]),
            ]
        })
        .collect()
}

/// Whether a font dictionary is one of the bundled fonts embedded earlier in
/// this document: a Type0 font with the bundled name whose descendant embeds
/// the complete bundled font program.
fn is_bundled_embedded_font(document: &Document, font_dictionary: &Dictionary, font: EmbeddedTextFont) -> bool {
    let matches = || -> Option<bool> {
        if dictionary_name(font_dictionary, b"Subtype").as_deref() != Some("Type0")
            || font_dictionary.get(b"BaseFont").ok()?.as_name().ok()? != font.pdf_name().as_bytes()
        {
            return Some(false);
        }
        let descendant = resolve_object(
            document,
            font_dictionary.get(b"DescendantFonts").ok()?.as_array().ok()?.first()?,
        )
        .ok()?
        .as_dict()
        .ok()?;
        let descriptor = resolve_object(document, descendant.get(b"FontDescriptor").ok()?)
            .ok()?
            .as_dict()
            .ok()?;
        let program = resolve_object(document, descriptor.get(b"FontFile2").ok()?)
            .ok()?
            .as_stream()
            .ok()?;
        Some(program.dict.get(b"Length1").ok()?.as_i64().ok()? == font.data().len() as i64)
    };
    matches().unwrap_or(false)
}

/// Rebuilds a bundled font's CID assignments from its standard
/// `CIDToGIDMap` and `ToUnicode` streams.
fn embedded_cid_mapping_from_standard_font(
    document: &Document,
    font_dictionary: &Dictionary,
) -> BTreeMap<u16, EmbeddedCidGlyph> {
    let mut mapped = BTreeMap::new();
    let descendant = font_dictionary
        .get(b"DescendantFonts")
        .and_then(Object::as_array)
        .ok()
        .and_then(|descendants| descendants.first())
        .and_then(|descendant| resolve_object(document, descendant).ok())
        .and_then(|descendant| descendant.as_dict().ok());
    let cid_to_gid = descendant
        .and_then(|descendant| descendant.get(b"CIDToGIDMap").ok())
        .and_then(|value| resolve_object(document, value).ok())
        .and_then(|value| value.as_stream().ok())
        .and_then(|stream| stream.decompressed_content().ok())
        .unwrap_or_default();
    for (cid, pair) in cid_to_gid.chunks_exact(2).enumerate().skip(1) {
        let glyph_id = u16::from_be_bytes([pair[0], pair[1]]);
        let Ok(cid) = u16::try_from(cid) else {
            break;
        };
        if glyph_id != 0 {
            mapped.insert(cid, EmbeddedCidGlyph { glyph_id, unicode: None });
        }
    }
    let to_unicode = font_dictionary
        .get(b"ToUnicode")
        .ok()
        .and_then(|value| resolve_object(document, value).ok())
        .and_then(|value| value.as_stream().ok())
        .and_then(|stream| stream.decompressed_content().ok())
        .unwrap_or_default();
    for line in String::from_utf8_lossy(&to_unicode).lines() {
        let mut fields = line.split_whitespace();
        let (Some(source), Some(destination), None) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        let hex = |value: &str| -> Option<Vec<u8>> {
            let value = value.strip_prefix('<')?.strip_suffix('>')?;
            (value.len() % 2 == 0)
                .then(|| {
                    (0..value.len())
                        .step_by(2)
                        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
                        .collect::<Option<Vec<_>>>()
                })
                .flatten()
        };
        let (Some(source), Some(destination)) = (hex(source), hex(destination)) else {
            continue;
        };
        let [high, low] = source.as_slice() else {
            continue;
        };
        let cid = u16::from_be_bytes([*high, *low]);
        let units = destination
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>();
        if let (Some(glyph), Ok(text)) = (mapped.get_mut(&cid), String::from_utf16(&units)) {
            glyph.unicode = Some(text);
        }
    }
    mapped
}

fn unicode_to_unicode_cmap(glyphs: &BTreeMap<u16, EmbeddedCidGlyph>) -> String {
    let mut to_unicode = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /ButterPaperUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let mapped = glyphs
        .iter()
        .filter(|(_, glyph)| glyph.unicode.is_some())
        .collect::<Vec<_>>();
    for chunk in mapped.chunks(100) {
        to_unicode.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (cid, glyph) in chunk {
            to_unicode.push_str(&format!(
                "<{cid:04X}> <{}>\n",
                unicode_destination_hex(
                    glyph.unicode.as_deref().expect("filtered Unicode mapping")
                )
            ));
        }
        to_unicode.push_str("endbfchar\n");
    }
    to_unicode.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    to_unicode
}

fn helvetica_text_width_pt(encoded: &[u8], font_size_pt: f64) -> f64 {
    let width_units = encoded
        .iter()
        .map(|byte| u64::from(HELVETICA_WIN_ANSI_WIDTHS[*byte as usize]))
        .sum::<u64>();
    width_units as f64 * font_size_pt / 1000.
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MeasurementCaptionLayout {
    rect: PdfRect,
    text_origin: PdfPoint,
}

fn measurement_caption_layout(
    anchor: PdfPoint,
    caption: &str,
    text: &TextBoxStyle,
    centered: bool,
) -> MeasurementCaptionLayout {
    let measured_width = appearance_text_width_pt(caption, text.font_size_pt(), text.font_family());
    let (width, height) = if centered {
        (
            text.font_size_pt()
                .max(measured_width + text.inset_pt() * 2.),
            text.line_height_pt().max(text.font_size_pt()),
        )
    } else {
        (
            (text.font_size_pt() * (56. / 12.)).max(measured_width + text.inset_pt() * 2.),
            text.line_height_pt().max(text.font_size_pt() * 1.5),
        )
    };
    let x = if centered {
        anchor.x - width * 0.5
    } else {
        anchor.x + 6.
    };
    let y = if centered {
        anchor.y - height * 0.5
    } else {
        anchor.y + 6.
    };
    let rect = PdfRect::new(x, y, width, height)
        .expect("validated measurement caption geometry must be finite");
    MeasurementCaptionLayout {
        rect,
        text_origin: PdfPoint {
            x: rect.x + text.inset_pt(),
            y: rect.y + rect.height - text.font_size_pt() * (13. / 12.),
        },
    }
}

fn measurement_path_midpoint(points: &[PdfPoint]) -> PdfPoint {
    let total = points
        .windows(2)
        .map(|segment| (segment[1].x - segment[0].x).hypot(segment[1].y - segment[0].y))
        .sum::<f64>();
    let target = total * 0.5;
    let mut travelled = 0.;
    for segment in points.windows(2) {
        let length = (segment[1].x - segment[0].x).hypot(segment[1].y - segment[0].y);
        if travelled + length >= target && length > 0. {
            let progress = (target - travelled) / length;
            return PdfPoint {
                x: segment[0].x + (segment[1].x - segment[0].x) * progress,
                y: segment[0].y + (segment[1].y - segment[0].y) * progress,
            };
        }
        travelled += length;
    }
    points.last().copied().unwrap_or(PdfPoint { x: 0., y: 0. })
}

fn measurement_vertex_mean(points: &[PdfPoint]) -> PdfPoint {
    let count = points.len() as f64;
    PdfPoint {
        x: points.iter().map(|point| point.x).sum::<f64>() / count,
        y: points.iter().map(|point| point.y).sum::<f64>() / count,
    }
}

fn measurement_points_bounds(points: &[PdfPoint], padding: f64) -> PdfRect {
    let min_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(
        min_x - padding,
        min_y - padding,
        max_x - min_x + padding * 2.,
        max_y - min_y + padding * 2.,
    )
    .expect("validated measurement points must have finite bounds")
}

fn union_measurement_bounds(left: PdfRect, right: PdfRect) -> PdfRect {
    let min_x = left.x.min(right.x);
    let min_y = left.y.min(right.y);
    let max_x = (left.x + left.width).max(right.x + right.width);
    let max_y = (left.y + left.height).max(right.y + right.height);
    PdfRect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        .expect("validated measurement bounds must have a finite union")
}

fn text_appearance_line_x(
    box_width: f64,
    line_width: f64,
    alignment: TextAlignment,
    inset_pt: f64,
) -> f64 {
    let available = (box_width - inset_pt * 2.).max(0.);
    let remaining = (available - line_width).max(0.);
    inset_pt
        + match alignment {
            TextAlignment::Left => 0.,
            TextAlignment::Center => remaining * 0.5,
            TextAlignment::Right => remaining,
        }
}

fn escape_pdf_literal_bytes(value: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(value.len());
    for byte in value {
        if matches!(byte, b'\\' | b'(' | b')') {
            escaped.push(b'\\');
        }
        escaped.push(*byte);
    }
    escaped
}

fn add_text_appearance(
    document: &mut Document,
    annotation: &TextBoxAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let (red, green, blue) = color_components(annotation.style().color());
    let font_size = annotation.style().font_size_pt();
    let line_height = font_size * TEXT_APPEARANCE_LINE_HEIGHT_FACTOR;
    let inset = annotation.style().inset_pt();
    let start_y = (annotation.layout_rect.height - inset - font_size).max(0.);
    let mut fonts = Dictionary::new();
    let mut content = format!("q\n/GS0 gs\nBT\n{red:.6} {green:.6} {blue:.6} rg\n").into_bytes();
    if !annotation.rich_text_runs().is_empty() {
        let (lines, mut shaped) = rich_text_appearance_lines(annotation)?;
        fonts = appearance_text_font_resources(document, &mut shaped)?;
        for (line_index, line) in lines.iter().enumerate() {
            let width = line
                .spans
                .iter()
                .map(|span| unicode_text_width_pt(&shaped[span.shaped_index], span.font_size_pt))
                .sum::<f64>();
            let mut x = annotation.layout_rect.x
                + text_appearance_line_x(
                    annotation.layout_rect.width,
                    width,
                    annotation.style().alignment(),
                    inset,
                );
            let y = annotation.layout_rect.y + start_y - line_index as f64 * line_height;
            for span in &line.spans {
                let (red, green, blue) = color_components(&span.color);
                content.extend_from_slice(format!("{red:.6} {green:.6} {blue:.6} rg\n").as_bytes());
                let shaped_line = &shaped[span.shaped_index];
                append_appearance_text_line(&mut content, shaped_line, span.font_size_pt, x, y);
                x += unicode_text_width_pt(shaped_line, span.font_size_pt);
            }
        }
    } else if uses_helvetica_winansi_fast_path(
        annotation.content(),
        annotation.style().font_family(),
    ) {
        let font = StandardTextFont::for_emphasis(text_is_bold(annotation.style()), false);
        let font_id = add_standard_font_variant(document, font);
        fonts.set(font.resource_name(), font_id);
        content.extend_from_slice(
            format!("/{} {font_size:.6} Tf\n", font.resource_name()).as_bytes(),
        );
        for (index, line) in annotation.content().split('\n').enumerate() {
            let encoded = text_appearance_line_bytes(line);
            let width = encoded
                .iter()
                .map(|byte| f64::from(font.widths()[usize::from(*byte)]))
                .sum::<f64>()
                * font_size
                / 1000.;
            let x = annotation.layout_rect.x
                + text_appearance_line_x(
                    annotation.layout_rect.width,
                    width,
                    annotation.style().alignment(),
                    inset,
                );
            let y = annotation.layout_rect.y + start_y - index as f64 * line_height;
            content.extend_from_slice(format!("1 0 0 1 {x:.6} {y:.6} Tm\n(").as_bytes());
            content.extend_from_slice(&escape_pdf_literal_bytes(&encoded));
            content.extend_from_slice(b") Tj\n");
        }
    } else {
        let mut lines = unicode_text_lines(annotation.content(), annotation.style().font_family())
            .expect("Text Box appearance must be validated before it is built");
        fonts = appearance_text_font_resources(document, &mut lines)?;
        for (index, line) in lines.iter().enumerate() {
            let width = unicode_text_width_pt(line, font_size);
            let x = annotation.layout_rect.x
                + text_appearance_line_x(
                    annotation.layout_rect.width,
                    width,
                    annotation.style().alignment(),
                    inset,
                );
            let y = annotation.layout_rect.y + start_y - index as f64 * line_height;
            content.extend_from_slice(format!("1 0 0 1 {x:.6} {y:.6} Tm\n").as_bytes());
            append_appearance_text_line(&mut content, line, font_size, x, y);
        }
    }
    ensure_default_text_font_resource(document, &mut fonts, annotation.style().font_family())?;
    content.extend_from_slice(b"ET\nQ\n");
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => pdf_rect(annotation.layout_rect),
            "Matrix" => text_box_appearance_matrix(annotation),
            "Resources" => dictionary! {
                "Font" => fonts,
                "ExtGState" => dictionary! { "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(annotation.style().opacity() as f32),
                    "ca" => Object::Real(annotation.style().opacity() as f32),
                } },
            },
        },
        content,
    )))
}

fn text_appearance_font_resources(document: &Document, appearance_id: ObjectId) -> Dictionary {
    document
        .get_object(appearance_id)
        .expect("new Text Box appearance must remain present")
        .as_stream()
        .expect("new Text Box appearance must remain a stream")
        .dict
        .get(b"Resources")
        .and_then(Object::as_dict)
        .and_then(|resources| resources.get(b"Font"))
        .and_then(Object::as_dict)
        .expect("new Text Box appearance must carry font resources")
        .clone()
}

fn escape_rich_text_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\n' => escaped.push_str("<br/>"),
            _ => escaped.push(character),
        }
    }
    escaped
}

fn text_box_rich_contents(annotation: &TextBoxAnnotation) -> String {
    let wrapper = revu_rich_text(
        annotation.style(),
        Some(annotation.style().inset_pt()),
        "",
        false,
    );
    let mut rich = wrapper.trim_end_matches("</body>").to_owned();
    rich.push_str("<p>");
    for run in annotation.rich_text_runs() {
        // Revu spans state family, size and colour in full.
        let style = annotation.style();
        let mut declarations = vec![
            format!(
                "font-family:{}",
                escape_rich_text_xml(run.font_family().unwrap_or(style.font_family()))
            ),
            format!(
                "font-size:{}pt",
                revu_number(run.font_size_pt().unwrap_or(style.font_size_pt()))
            ),
            format!(
                "color:{}",
                run.color().unwrap_or(style.color()).to_ascii_uppercase()
            ),
        ];
        if run.bold() {
            declarations.push("font-weight:bold".into());
        }
        if run.italic() {
            declarations.push("font-style:italic".into());
        }
        rich.push_str("<span style=\"");
        rich.push_str(&declarations.join("; "));
        rich.push_str("\">");
        rich.push_str(&escape_rich_text_xml(run.text()));
        rich.push_str("</span>");
    }
    rich.push_str("</p></body>");
    rich
}

fn text_box_dictionary(
    annotation: &TextBoxAnnotation,
    appearance_id: ObjectId,
    font_resources: Dictionary,
    original: &Dictionary,
) -> Dictionary {
    let style = annotation.style();
    let rich_text = if annotation.rich_text_runs().is_empty() {
        revu_rich_text(style, Some(style.inset_pt()), annotation.content(), true)
    } else {
        text_box_rich_contents(annotation)
    };
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "Rect" => pdf_rect(text_box_annotation_bounds(annotation)),
        "NM" => pdf_literal(annotation.id.as_str()),
        "Subj" => pdf_literal("Text Box"),
        "Contents" => pdf_text_box_contents(annotation.content()),
        "DA" => pdf_literal(&revu_default_appearance(style.color(), style)),
        "DS" => pdf_literal(&revu_default_style(style, Some(style.inset_pt()))),
        "RC" => pdf_text_box_contents(&rich_text),
        "BS" => markup_border_style(0., StrokeStyle::Solid),
        "C" => Vec::<Object>::new(),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if style.font_family() != "Helvetica" {
        dictionary.set("DR", dictionary! { "Font" => font_resources });
    }
    set_markup_opacity(&mut dictionary, style.opacity());
    set_markup_rotation(&mut dictionary, annotation.rotation_degrees());
    if let Ok(subject) = original.get(b"Subj") {
        dictionary.set("Subj", subject.clone());
    }
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn length_bounds(annotation: &LengthAnnotation) -> PdfRect {
    let layout = length_line_layout(annotation);
    let caption = length_caption_text(annotation).map(|caption| {
        measurement_caption_layout(
            layout.caption_center,
            &caption,
            annotation.appearance.text(),
            true,
        )
        .rect
    })
    .map(|rect| {
        rotated_rect_bounds(
            rect,
            measurement_caption_angle(annotation.start, annotation.end).to_degrees(),
        )
    });
    measurement_line_bounds(&layout, annotation.appearance.line().stroke_width_pt(), caption)
}

fn vertex_path_bounds(annotation: &VertexPathAnnotation) -> PdfRect {
    // Revu pads PolyLine and Polygon `/Rect` by 5 pt plus half the stroke.
    let padding = annotation.appearance.stroke_width_pt() / 2.0 + 5.0;
    let min_x = annotation
        .points()
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = annotation
        .points()
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = annotation
        .points()
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = annotation
        .points()
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(
        min_x - padding,
        min_y - padding,
        (max_x - min_x).max(0.0) + padding * 2.0,
        (max_y - min_y).max(0.0) + padding * 2.0,
    )
    .expect("validated vertex-path points must have finite padded bounds")
}

fn add_vertex_path_appearance(
    document: &mut Document,
    annotation: &VertexPathAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let bounds = vertex_path_bounds(annotation);
    let appearance = &annotation.appearance;
    let (stroke_red, stroke_green, stroke_blue) = color_components(appearance.stroke_color());
    let fill = (annotation.kind == VertexPathKind::Polygon)
        .then(|| appearance.fill_color().map(color_components))
        .flatten();
    let fill_operation = fill.map_or_else(String::new, |(red, green, blue)| {
        format!("{red:.6} {green:.6} {blue:.6} rg\n")
    });
    let dash_operation =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
            .map_or_else(String::new, |(dash, gap)| {
                format!("[{dash:.6} {gap:.6}] 0 d\n")
            });
    let first = annotation.points()[0];
    let graphics_state = if shape_is_translucent(appearance) { "/GS0 gs\n" } else { "" };
    let mut content = format!(
        "q\n{graphics_state}1 J 1 j\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n{:.6} {:.6} m\n",
        appearance.stroke_width_pt(),
        first.x - bounds.x,
        first.y - bounds.y,
    );
    for point in annotation.points().iter().skip(1) {
        content.push_str(&format!(
            "{:.6} {:.6} l\n",
            point.x - bounds.x,
            point.y - bounds.y
        ));
    }
    content.push_str(match (annotation.kind, fill.is_some()) {
        (VertexPathKind::Polyline, _) => "S\nQ\n",
        (VertexPathKind::Polygon, true) => "h B\nQ\n",
        (VertexPathKind::Polygon, false) => "h S\nQ\n",
    });
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => shape_graphics_state(appearance),
        },
        content.into_bytes(),
    )))
}

fn vertex_path_dictionary(
    annotation: &VertexPathAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let vertices = annotation
        .points()
        .iter()
        .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
        .collect::<Vec<_>>();
    let (subtype, subject) = match annotation.kind {
        VertexPathKind::Polyline => ("PolyLine", "PolyLine"),
        VertexPathKind::Polygon => ("Polygon", "Polygon"),
    };
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => subtype,
        "Rect" => pdf_rect(vertex_path_bounds(annotation)),
        "Vertices" => vertices,
        "NM" => pdf_literal(annotation.id.as_str()),
        "Subj" => pdf_literal(subject),
        "C" => color_array(appearance.stroke_color()),
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_shape_border(&mut replacement, appearance);
    match annotation.kind {
        // Revu records a PolyLine's line-ending fill as its stroke colour.
        VertexPathKind::Polyline => {
            replacement.set("IC", color_array(appearance.stroke_color()));
        }
        VertexPathKind::Polygon => set_shape_fill(&mut replacement, appearance),
    }
    set_markup_opacity(&mut replacement, appearance.opacity());
    preserve_markup_comment(&mut replacement, original);
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn cloud_bounds(annotation: &CloudAnnotation) -> PdfRect {
    // Revu's `/Rect` is the outline's bounds grown by 1.789 curl radii; it
    // also always holds the drawn curls and their stroke.
    let path = annotation.scallop_path();
    let padding = annotation.appearance.stroke_width_pt() / 2.0 + 1.0;
    let revu_padding = 1.7889 * cloud_curl_radius(annotation.points(), annotation.border_effect_intensity());
    let points = annotation.points();
    let min_x = path
        .iter()
        .map(|point| point.x - padding)
        .chain(points.iter().map(|point| point.x - revu_padding))
        .fold(f64::INFINITY, f64::min);
    let max_x = path
        .iter()
        .map(|point| point.x + padding)
        .chain(points.iter().map(|point| point.x + revu_padding))
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = path
        .iter()
        .map(|point| point.y - padding)
        .chain(points.iter().map(|point| point.y - revu_padding))
        .fold(f64::INFINITY, f64::min);
    let max_y = path
        .iter()
        .map(|point| point.y + padding)
        .chain(points.iter().map(|point| point.y + revu_padding))
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(min_x, min_y, (max_x - min_x).max(0.0), (max_y - min_y).max(0.0))
        .expect("validated cloud points must have finite padded bounds")
}

fn add_cloud_appearance(
    document: &mut Document,
    annotation: &CloudAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let bounds = cloud_bounds(annotation);
    let appearance = &annotation.appearance;
    let (red, green, blue) = color_components(appearance.stroke_color());
    let path = annotation.scallop_path();
    let first = path[0];
    let dash_operation =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
            .map_or_else(String::new, |(dash, gap)| {
                format!("[{dash:.6} {gap:.6}] 0 d\n")
            });
    let graphics_state = if shape_is_translucent(appearance) { "/GS0 gs\n" } else { "" };
    let fill_operation = appearance.fill_color().map_or_else(String::new, |color| {
        let (red, green, blue) = color_components(color);
        format!("{red:.6} {green:.6} {blue:.6} rg\n")
    });
    let mut content = format!(
        "q\n{graphics_state}1 J 1 j\n{red:.6} {green:.6} {blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n{:.6} {:.6} m\n",
        appearance.stroke_width_pt(),
        first.x - bounds.x,
        first.y - bounds.y,
    );
    for point in path.iter().skip(1) {
        content.push_str(&format!(
            "{:.6} {:.6} l\n",
            point.x - bounds.x,
            point.y - bounds.y,
        ));
    }
    content.push_str(if appearance.fill_color().is_some() { "h B\nQ\n" } else { "h S\nQ\n" });
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => shape_graphics_state(appearance),
        },
        content.into_bytes(),
    )))
}

fn cloud_dictionary(
    annotation: &CloudAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let vertices = annotation
        .points()
        .iter()
        .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
        .collect::<Vec<_>>();
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Polygon",
        "IT" => "PolygonCloud",
        "Rect" => pdf_rect(cloud_bounds(annotation)),
        "Vertices" => vertices,
        "NM" => pdf_literal(annotation.id.as_str()),
        "Subj" => pdf_literal("Cloud"),
        "C" => color_array(appearance.stroke_color()),
        "BE" => dictionary! {
            "S" => "C",
            "I" => Object::Real(annotation.border_effect_intensity() as f32),
        },
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_shape_border(&mut replacement, appearance);
    set_shape_fill(&mut replacement, appearance);
    set_markup_opacity(&mut replacement, appearance.opacity());
    preserve_markup_comment(&mut replacement, original);
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn cloud_plus_cloud_annotation(
    annotation: &CloudPlusAnnotation,
) -> Result<CloudAnnotation, PdfPersistenceError> {
    let mut cloud = CloudAnnotation::new(
        annotation.id.clone(),
        annotation.page_index,
        annotation.cloud_points().to_vec(),
        annotation.border_effect_intensity(),
        annotation.appearance.cloud().clone(),
    )?;
    cloud.locked = annotation.locked;
    Ok(cloud)
}

fn add_cloud_plus_cloud_appearance(
    document: &mut Document,
    annotation: &CloudPlusAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let Some(path) = annotation.cloud_appearance_path() else {
        return add_cloud_appearance(document, &cloud_plus_cloud_annotation(annotation)?);
    };
    let bounds = cloud_plus_cloud_bounds(annotation)?;
    let appearance = annotation.appearance.cloud();
    let (red, green, blue) = color_components(appearance.stroke_color());
    let dash_operation =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
            .map_or_else(String::new, |(dash, gap)| {
                format!("[{dash:.6} {gap:.6}] 0 d\n")
            });
    let fill_operation = appearance.fill_color().map_or_else(String::new, |color| {
        let (red, green, blue) = color_components(color);
        format!("{red:.6} {green:.6} {blue:.6} rg\n")
    });
    let mut content = format!(
        "q\n/GS0 gs\n1 J 1 j\n{red:.6} {green:.6} {blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n",
        appearance.stroke_width_pt(),
    );
    for command in path {
        match command {
            CloudAppearancePathCommand::MoveTo(point) => content.push_str(&format!(
                "{:.6} {:.6} m\n",
                point.x - bounds.x,
                point.y - bounds.y,
            )),
            CloudAppearancePathCommand::LineTo(point) => content.push_str(&format!(
                "{:.6} {:.6} l\n",
                point.x - bounds.x,
                point.y - bounds.y,
            )),
            CloudAppearancePathCommand::CubicTo {
                control_1,
                control_2,
                end,
            } => {
                content.push_str(&format!(
                    "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c\n",
                    control_1.x - bounds.x,
                    control_1.y - bounds.y,
                    control_2.x - bounds.x,
                    control_2.y - bounds.y,
                    end.x - bounds.x,
                    end.y - bounds.y,
                ));
            }
            CloudAppearancePathCommand::Close => content.push_str("h\n"),
        }
    }
    content.push_str(if appearance.fill_color().is_some() { "B\nQ\n" } else { "S\nQ\n" });
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ExtGState" => dictionary! {
                    "GS0" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real(if appearance.fill_color().is_some() {
                            appearance.fill_opacity() as f32
                        } else {
                            appearance.opacity() as f32
                        }),
                    },
                },
            },
        },
        content.into_bytes(),
    )))
}

fn cloud_plus_cloud_bounds(annotation: &CloudPlusAnnotation) -> Result<PdfRect, AnnotationError> {
    let path = annotation.cloud_appearance_path().map_or_else(
        || annotation.scallop_path(),
        |path| {
            path.iter()
                .flat_map(|command| match command {
                    CloudAppearancePathCommand::MoveTo(point)
                    | CloudAppearancePathCommand::LineTo(point) => vec![*point],
                    CloudAppearancePathCommand::CubicTo {
                        control_1,
                        control_2,
                        end,
                    } => vec![*control_1, *control_2, *end],
                    CloudAppearancePathCommand::Close => Vec::new(),
                })
                .collect()
        },
    );
    let padding = annotation.appearance.cloud().stroke_width_pt() / 2.0 + 1.0;
    let min_x = path
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = path
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = path
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = path
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(
        min_x - padding,
        min_y - padding,
        max_x - min_x + padding * 2.,
        max_y - min_y + padding * 2.,
    )
}

fn cloud_plus_text_bounds(annotation: &CloudPlusAnnotation) -> PdfRect {
    const PADDING_PT: f64 = 5.5;
    let mut min_x = annotation.text_box.x;
    let mut min_y = annotation.text_box.y;
    let mut max_x = annotation.text_box.x + annotation.text_box.width;
    let mut max_y = annotation.text_box.y + annotation.text_box.height;
    for point in annotation.leader_points() {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    PdfRect::new(
        min_x - PADDING_PT,
        min_y - PADDING_PT,
        max_x - min_x + PADDING_PT * 2.,
        max_y - min_y + PADDING_PT * 2.,
    )
    .expect("validated Cloud+ geometry has finite bounds")
}

fn add_cloud_plus_text_appearance(
    document: &mut Document,
    annotation: &CloudPlusAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let text = annotation.appearance.text();
    let mut lines = unicode_text_lines(annotation.content(), text.font_family()).map_err(|()| {
        PdfPersistenceError::InvalidDocument(
            "Cloud+ appearance cannot encode one or more Unicode characters with the bundled PDF fonts"
                .into(),
        )
    })?;
    let font_resources = appearance_text_font_resources(document, &mut lines)?;
    let bounds = cloud_plus_text_bounds(annotation);
    let line = annotation.appearance.leader();
    let (line_red, line_green, line_blue) = color_components(line.stroke_color());
    let (text_red, text_green, text_blue) = color_components(text.color());
    let content = format!(
        "q\n/GS0 gs\n{line_red:.6} {line_green:.6} {line_blue:.6} RG\n{:.6} w\n",
        line.stroke_width_pt()
    );
    let mut content = content.into_bytes();
    if let Some(first) = annotation.leader_points().first() {
        content.extend_from_slice(
            format!("{:.6} {:.6} m\n", first.x - bounds.x, first.y - bounds.y).as_bytes(),
        );
        for point in annotation.leader_points().iter().skip(1) {
            content.extend_from_slice(
                format!("{:.6} {:.6} l\n", point.x - bounds.x, point.y - bounds.y).as_bytes(),
            );
        }
        content.extend_from_slice(b"S\n");
    }
    append_callout_box_border(&mut content, annotation.text_box, bounds, line.stroke_width_pt());
    let line_height = text.font_size_pt() * 1.15;
    let total_height = line_height * lines.len() as f64;
    let start_y = annotation.text_box.y
        + ((annotation.text_box.height - total_height) * 0.5).max(0.)
        + total_height
        - text.font_size_pt();
    if annotation.content().is_empty() {
        // Revu allows a callout without text; it draws no text object.
    } else if uses_helvetica_winansi_fast_path(annotation.content(), text.font_family()) {
        content.extend_from_slice(
            format!(
                "BT\n/Helv {:.6} Tf\n{text_red:.6} {text_green:.6} {text_blue:.6} rg\n",
                text.font_size_pt()
            )
            .as_bytes(),
        );
        for (index, line_text) in annotation.content().split('\n').enumerate() {
            let encoded = text_appearance_line_bytes(line_text);
            content.extend_from_slice(
                format!(
                    "1 0 0 1 {:.6} {:.6} Tm\n(",
                    annotation.text_box.x + text.inset_pt() - bounds.x,
                    start_y - index as f64 * line_height - bounds.y,
                )
                .as_bytes(),
            );
            content.extend_from_slice(&escape_pdf_literal_bytes(&encoded));
            content.extend_from_slice(b") Tj\n");
        }
    } else {
        content.extend_from_slice(
            format!("BT\n{text_red:.6} {text_green:.6} {text_blue:.6} rg\n").as_bytes(),
        );
        for (index, line_text) in lines.iter().enumerate() {
            content.extend_from_slice(
                format!(
                    "1 0 0 1 {:.6} {:.6} Tm\n",
                    annotation.text_box.x + text.inset_pt() - bounds.x,
                    start_y - index as f64 * line_height - bounds.y,
                )
                .as_bytes(),
            );
            append_appearance_text_line(
                &mut content,
                line_text,
                text.font_size_pt(),
                annotation.text_box.x + text.inset_pt() - bounds.x,
                start_y - index as f64 * line_height - bounds.y,
            );
        }
    }
    if !annotation.content().is_empty() {
        content.extend_from_slice(b"ET\n");
    }
    content.extend_from_slice(b"Q\n");
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => font_resources,
                "ExtGState" => dictionary! { "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(line.opacity() as f32),
                    "ca" => Object::Real(line.opacity() as f32),
                } },
            },
        },
        content,
    )))
}

fn cloud_plus_cloud_dictionary(
    annotation: &CloudPlusAnnotation,
    appearance_id: ObjectId,
    cloud_name: &str,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let cloud = cloud_plus_cloud_annotation(annotation)?;
    let mut replacement = cloud_dictionary(&cloud, appearance_id, original)?;
    if annotation.cloud_appearance_path().is_some() {
        replacement.set("Rect", pdf_rect(cloud_plus_cloud_bounds(annotation)?));
    }
    replacement.set("NM", pdf_literal(cloud_name));
    replacement.set("Subj", pdf_literal("Cloud+"));
    replacement.set("IT", Object::Name(b"PolygonCloud".to_vec()));
    replacement.set("ITEx", Object::Name(b"PolyText".to_vec()));
    Ok(replacement)
}

fn cloud_plus_text_dictionary(
    annotation: &CloudPlusAnnotation,
    appearance_id: ObjectId,
    cloud_name: &str,
    text_name: &str,
    font_resources: Dictionary,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let bounds = cloud_plus_text_bounds(annotation);
    let text = annotation.appearance.text();
    let line = annotation.appearance.leader();
    let rd = vec![
        Object::Real((annotation.text_box.x - bounds.x) as f32),
        Object::Real((annotation.text_box.y - bounds.y) as f32),
        Object::Real(
            (bounds.x + bounds.width - annotation.text_box.x - annotation.text_box.width) as f32,
        ),
        Object::Real(
            (bounds.y + bounds.height - annotation.text_box.y - annotation.text_box.height) as f32,
        ),
    ];
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "IT" => "FreeTextCallout",
        "ITEx" => "PolyText",
        "Rect" => pdf_rect(bounds),
        "RD" => rd,
        "NM" => pdf_literal(text_name),
        "Subj" => pdf_literal("Cloud+"),
        "Contents" => pdf_text_box_contents(annotation.content()),
        "CL" => annotation
            .leader_points()
            .iter()
            .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
            .collect::<Vec<_>>(),
        "DA" => pdf_literal(&revu_default_appearance(line.stroke_color(), text)),
        "DS" => pdf_literal(&revu_default_style(text, Some(text.inset_pt()))),
        "RC" => pdf_text_box_contents(&revu_rich_text(text, Some(text.inset_pt()), annotation.content(), true)),
        "BS" => markup_border_style(revu_callout_border_width(line.stroke_width_pt()), StrokeStyle::Solid),
        "C" => Vec::<Object>::new(),
        "GroupNesting" => vec![
            pdf_literal("Cloud+"),
            Object::Name(text_name.as_bytes().to_vec()),
            Object::Name(cloud_name.as_bytes().to_vec()),
        ],
        "AP" => dictionary! { "N" => appearance_id },
    };
    if text.font_family() != "Helvetica" {
        replacement.set("DR", dictionary! { "Font" => font_resources });
    }
    set_markup_opacity(&mut replacement, line.opacity());
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn measurement_path_bounds(annotation: &MeasurementPathAnnotation) -> PdfRect {
    let bounds = measurement_points_bounds(
        annotation.points(),
        8_f64.max(annotation.appearance.stroke_width_pt() * 0.5),
    );
    if !annotation.calibration().show_caption() {
        return bounds;
    }
    let anchor = match annotation.kind {
        MeasurementPathKind::Polylength => measurement_path_midpoint(annotation.points()),
        MeasurementPathKind::Area => measurement_vertex_mean(annotation.points()),
    };
    // Revu centres an Area caption inside the area.
    let caption = measurement_caption_layout(
        anchor,
        &annotation.caption(),
        annotation.text_style(),
        annotation.kind == MeasurementPathKind::Area,
    );
    union_measurement_bounds(bounds, caption.rect)
}

fn add_measurement_path_appearance(
    document: &mut Document,
    annotation: &MeasurementPathAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let caption_text = annotation
        .calibration()
        .show_caption()
        .then(|| annotation.caption());
    let mut caption_lines = caption_text
        .as_deref()
        .map(|caption| {
            unicode_text_lines(caption, annotation.text_style().font_family()).map_err(|()| {
                PdfPersistenceError::InvalidDocument(
                    "measurement caption appearance cannot encode one or more Unicode characters with the bundled PDF fonts"
                        .into(),
                )
            })
        })
        .transpose()?
        .unwrap_or_default();
    let font_resources = appearance_text_font_resources(document, &mut caption_lines)?;
    let bounds = measurement_path_bounds(annotation);
    let appearance = &annotation.appearance;
    let text = annotation.text_style();
    let (stroke_red, stroke_green, stroke_blue) = color_components(appearance.stroke_color());
    let fill = (annotation.kind == MeasurementPathKind::Area)
        .then(|| appearance.fill_color().map(color_components))
        .flatten();
    let fill_operation = fill.map_or_else(String::new, |(red, green, blue)| {
        format!("{red:.6} {green:.6} {blue:.6} rg\n")
    });
    let dash_operation =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
            .map_or_else(String::new, |(dash, gap)| {
                format!("[{dash:.6} {gap:.6}] 0 d\n")
            });
    let first = annotation.points()[0];
    let mut content = format!(
        "q\n/GSPath gs\n1 J 1 j\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n{:.6} {:.6} m\n",
        appearance.stroke_width_pt(),
        first.x - bounds.x,
        first.y - bounds.y,
    );
    for point in annotation.points().iter().skip(1) {
        content.push_str(&format!(
            "{:.6} {:.6} l\n",
            point.x - bounds.x,
            point.y - bounds.y
        ));
    }
    content.push_str(match (annotation.kind, fill.is_some()) {
        (MeasurementPathKind::Polylength, _) => "S\nQ\n",
        (MeasurementPathKind::Area, true) => "h B\nQ\n",
        (MeasurementPathKind::Area, false) => "h S\nQ\n",
    });
    let mut content = content.into_bytes();
    if annotation.calibration().show_caption() {
        let (text_red, text_green, text_blue) = color_components(text.color());
        let anchor = match annotation.kind {
            MeasurementPathKind::Polylength => measurement_path_midpoint(annotation.points()),
            MeasurementPathKind::Area => measurement_vertex_mean(annotation.points()),
        };
        let caption_text = caption_text
            .as_deref()
            .expect("visible measurement caption must have text");
        let caption = measurement_caption_layout(
            anchor,
            caption_text,
            text,
            annotation.kind == MeasurementPathKind::Area,
        );
        let caption_x = caption.text_origin.x - bounds.x;
        let caption_y = caption.text_origin.y - bounds.y;
        if uses_helvetica_winansi_fast_path(caption_text, text.font_family()) {
            let encoded = text_appearance_line_bytes(caption_text);
            content.extend_from_slice(format!(
                "q\n/GSText gs\nBT {text_red:.6} {text_green:.6} {text_blue:.6} rg /Helv {:.6} Tf 1 0 0 1 {caption_x:.6} {caption_y:.6} Tm (",
                text.font_size_pt(),
            ).as_bytes());
            content.extend_from_slice(&escape_pdf_literal_bytes(&encoded));
            content.extend_from_slice(b") Tj ");
        } else {
            content.extend_from_slice(format!(
                "q\n/GSText gs\nBT {text_red:.6} {text_green:.6} {text_blue:.6} rg 1 0 0 1 {caption_x:.6} {caption_y:.6} Tm\n",
            ).as_bytes());
            append_appearance_text_line(
                &mut content,
                caption_lines
                    .first()
                    .expect("visible measurement caption must have one line"),
                text.font_size_pt(),
                caption_x,
                caption_y,
            );
        }
        content.extend_from_slice(b"ET\nQ\n");
    }
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => font_resources,
                "ExtGState" => dictionary! {
                    "GSPath" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real(appearance.fill_opacity() as f32),
                    },
                    "GSText" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(text.opacity() as f32),
                        "ca" => Object::Real(text.opacity() as f32),
                    },
                },
            },
        },
        content,
    )))
}

// Merge only the fields owned by the native caption editor. Electron and
// third-party appearance extensions survive an edit instead of being dropped.
/// Paper metres per PDF point, which Revu records as `TargetUnitConversion`.
const REVU_TARGET_UNIT_CONVERSION: f64 = 0.0254 / 72.;

fn revu_number_format(unit: &[u8], conversion: f64, denominator: i64, fixed: bool) -> Object {
    let mut format = dictionary! {
        "Type" => "NumberFormat",
        "U" => Object::String(unit.to_vec(), StringFormat::Literal),
        "C" => Object::Real(conversion as f32),
        "D" => denominator,
        "SS" => pdf_literal(""),
    };
    if fixed {
        format.set("FD", Object::Boolean(true));
    }
    Object::Dictionary(format)
}

fn revu_precision_denominator(precision: ScalePrecision) -> (i64, bool) {
    match precision.mode {
        ScalePrecisionMode::Decimal => ((1. / precision.value).round().max(1.) as i64, false),
        ScalePrecisionMode::Fraction => (precision.value.round().max(1.) as i64, true),
    }
}

/// Paper length of one point in a unit, for Revu's measurement depth unit.
fn paper_point_in_unit(unit: &str) -> f64 {
    match unit {
        "mm" => 25.4 / 72.,
        "cm" => 2.54 / 72.,
        "m" => REVU_TARGET_UNIT_CONVERSION,
        "km" => REVU_TARGET_UNIT_CONVERSION / 1000.,
        "in" => 1. / 72.,
        "ft" => 1. / 864.,
        "yd" => 1. / 2592.,
        _ => 1.,
    }
}

/// Revu's scale ratio string, e.g. `1 cm = 1 m`.
fn revu_scale_ratio(paper_value: f64, paper_unit: &str, real_value: f64, real_unit: &str) -> String {
    format!(
        "{} {paper_unit} = {} {real_unit}",
        revu_number(paper_value),
        revu_number(real_value)
    )
}

/// Revu's ratio for a measurement: one paper centimetre (or inch) against
/// its real length in the base unit.
fn calibration_scale_ratio(calibration: &LengthCalibration) -> String {
    let (base_unit, display_per_base) = revu_base_unit(calibration.unit());
    let (paper_unit, unit_points) = if base_unit == "ft" { ("in", 72.) } else { ("cm", 72. / 2.54) };
    revu_scale_ratio(
        1.,
        paper_unit,
        calibration.units_per_point() * unit_points / display_per_base,
        base_unit,
    )
}

/// Revu measures in a base unit (metres or feet) and converts to the
/// displayed unit through `/D`. Returns the base unit and display units per
/// base unit.
fn revu_base_unit(unit: &str) -> (&str, f64) {
    match unit {
        "mm" => ("m", 1000.),
        "cm" => ("m", 100.),
        "m" => ("m", 1.),
        "km" => ("m", 0.001),
        "in" => ("ft", 12.),
        "ft" => ("ft", 1.),
        "yd" => ("ft", 1. / 3.),
        "mi" => ("ft", 1. / 5280.),
        _ => (unit, 1.),
    }
}

/// The `/Measure` rectilinear dictionary in the shape Revu writes.
fn revu_measure_dictionary(
    ratio: &str,
    unit: &str,
    units_per_point: f64,
    precision: ScalePrecision,
) -> Dictionary {
    let (base_unit, display_per_base) = revu_base_unit(unit);
    let (denominator, fractional) = revu_precision_denominator(precision);
    let length = |unit: &[u8], conversion: f64, fixed: bool| {
        let mut format = revu_number_format(unit, conversion, denominator, fixed);
        if fractional && let Object::Dictionary(format) = &mut format {
            format.set("F", "F");
        }
        vec![format]
    };
    dictionary! {
        "Type" => "Measure",
        "Subtype" => "RL",
        "R" => pdf_literal(ratio),
        "X" => length(base_unit.as_bytes(), units_per_point / display_per_base, false),
        "D" => length(unit.as_bytes(), display_per_base, false),
        "A" => length(format!("sq {unit}").as_bytes(), display_per_base.powi(2), true),
        "T" => {
            let mut angle = revu_number_format(&[0xB0], 1., denominator, true);
            if let Object::Dictionary(angle) = &mut angle {
                angle.set("PS", pdf_literal(""));
            }
            vec![angle]
        },
        "V" => length(format!("cu {unit}").as_bytes(), display_per_base.powi(3), true),
        "TargetUnitConversion" => Object::Real(REVU_TARGET_UNIT_CONVERSION as f32),
    }
}

fn calibration_measure_dictionary(calibration: &LengthCalibration) -> Dictionary {
    revu_measure_dictionary(
        &calibration_scale_ratio(calibration),
        calibration.unit(),
        calibration.units_per_point(),
        calibration.scale_precision(),
    )
}

fn calibration_depth_unit(calibration: &LengthCalibration) -> Object {
    let (base_unit, _) = revu_base_unit(calibration.unit());
    let (denominator, _) = revu_precision_denominator(calibration.scale_precision());
    Object::Array(vec![revu_number_format(
        base_unit.as_bytes(),
        paper_point_in_unit(base_unit),
        denominator,
        true,
    )])
}

/// Revu's measurement caption keys: `Contents`, a paragraph-free `RC` and a
/// `DS` without a margin.
fn set_measurement_caption(dictionary: &mut Dictionary, text: &TextBoxStyle, caption: &str) {
    dictionary.set("Contents", pdf_text_box_contents(caption));
    dictionary.set(
        "RC",
        pdf_text_box_contents(&revu_rich_text(text, None, caption, false)),
    );
    dictionary.set("DS", pdf_literal(&revu_default_style(text, None)));
}


/// A number as Revu prints it in text styles: up to four decimals, trimmed.
fn revu_number(value: f64) -> String {
    let formatted = format!("{value:.4}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed == "-0" { "0".into() } else { trimmed.into() }
}

fn revu_color_components(color: &str) -> String {
    let (red, green, blue) = color_components(color);
    [red, green, blue]
        .map(|component| revu_number(f64::from(component)))
        .join(" ")
}

fn text_is_bold(text: &TextBoxStyle) -> bool {
    text.weight() >= 600
}

fn text_alignment_name(alignment: TextAlignment) -> &'static str {
    match alignment {
        TextAlignment::Left => "left",
        TextAlignment::Center => "center",
        TextAlignment::Right => "right",
    }
}

/// Revu's `/DA`: colour then font, e.g. `(1 0 0 rg /Helv 12 Tf)`. For a
/// callout the colour is the leader/border colour; the text colour lives in
/// `/DS`.
fn revu_default_appearance(color: &str, text: &TextBoxStyle) -> String {
    let resource = if text.font_family() == "Helvetica" {
        if text_is_bold(text) { "HelvBld" } else { "Helv" }
    } else {
        appearance_font_resource_name(text.font_family())
    };
    format!(
        "{} rg /{resource} {} Tf",
        revu_color_components(color),
        revu_number(text.font_size_pt())
    )
}

/// The CSS shorthand Revu writes in `/DS` and the `RC` body style.
fn revu_font_shorthand(text: &TextBoxStyle) -> String {
    format!(
        "{}{} {}pt",
        if text_is_bold(text) { "bold " } else { "" },
        text.font_family(),
        revu_number(text.font_size_pt())
    )
}

/// Revu's `/DS`. Text boxes and callouts carry a margin; measurement and
/// dimension captions do not.
fn revu_default_style(text: &TextBoxStyle, margin_pt: Option<f64>) -> String {
    format!(
        "font: {}; text-align:{}; {}line-height:{}pt; color:{}",
        revu_font_shorthand(text),
        text_alignment_name(text.alignment()),
        margin_pt.map_or_else(String::new, |margin| format!("margin:{}pt; ", revu_number(margin))),
        revu_number(text.line_height_pt()),
        text.color().to_ascii_uppercase()
    )
}

/// Revu's rich-text body. `paragraphs` wraps each line in `<p>`; Revu omits
/// paragraphs for measurement captions.
fn revu_rich_text(
    text: &TextBoxStyle,
    margin_pt: Option<f64>,
    content: &str,
    paragraphs: bool,
) -> String {
    let body_style = format!(
        "font:{}; text-align:{}; {}line-height:{}pt; color:{}",
        revu_font_shorthand(text),
        text_alignment_name(text.alignment()),
        margin_pt.map_or_else(String::new, |margin| format!("margin:{}pt; ", revu_number(margin))),
        revu_number(text.line_height_pt()),
        text.color().to_ascii_uppercase()
    );
    let mut rich = format!(
        "<?xml version=\"1.0\"?><body xmlns:xfa=\"http://www.xfa.org/schema/xfa-data/1.0/\" xfa:contentType=\"text/html\" xfa:APIVersion=\"BluebeamPDFRevu:2018\" xfa:spec=\"2.2.0\" style=\"{body_style}\" xmlns=\"http://www.w3.org/1999/xhtml\">"
    );
    if paragraphs {
        let mut paragraph_style = Vec::new();
        if text_is_bold(text) {
            paragraph_style.push("font-weight:bold".to_owned());
        }
        if text.alignment() != TextAlignment::Left {
            paragraph_style.push(format!("text-align:{}", text_alignment_name(text.alignment())));
        }
        let open = if paragraph_style.is_empty() {
            "<p>".to_owned()
        } else {
            format!("<p style=\"{}\">", paragraph_style.join("; "))
        };
        for line in content.split('\n') {
            rich.push_str(&open);
            rich.push_str(&escape_rich_text_xml(line));
            rich.push_str("</p>");
        }
    } else {
        rich.push_str(&escape_rich_text_xml(content));
    }
    rich.push_str("</body>");
    rich
}

#[derive(Clone, Debug, Default, PartialEq)]
struct ParsedDefaultStyle {
    family: Option<String>,
    size_pt: Option<f64>,
    bold: bool,
    italic: bool,
    alignment: Option<TextAlignment>,
    margin_pt: Option<f64>,
    line_height_pt: Option<f64>,
    color: Option<String>,
}

fn parse_point_value(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix("pt")
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.)
}

fn parse_css_color(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit()))
    .then(|| value.to_ascii_lowercase())
}

/// Parses Revu's `/DS` (and the standard CSS subset ISO 32000 allows there).
fn parse_default_style(style: &str) -> ParsedDefaultStyle {
    let mut parsed = ParsedDefaultStyle::default();
    if style.len() > 4096 {
        return parsed;
    }
    for declaration in style.split(';') {
        let Some((property, value)) = declaration.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match property.trim().to_ascii_lowercase().as_str() {
            "font" => {
                let Some((prefix, size)) = value.rsplit_once(char::is_whitespace) else {
                    continue;
                };
                parsed.size_pt = parse_point_value(size).or(parsed.size_pt);
                let mut family = prefix.trim();
                loop {
                    let lower = family.to_ascii_lowercase();
                    if let Some(rest) = lower.strip_prefix("bold ") {
                        parsed.bold = true;
                        family = family[family.len() - rest.len()..].trim_start();
                    } else if let Some(rest) =
                        lower.strip_prefix("italic ").or_else(|| lower.strip_prefix("oblique "))
                    {
                        parsed.italic = true;
                        family = family[family.len() - rest.len()..].trim_start();
                    } else if let Some(rest) = lower.strip_prefix("normal ") {
                        family = family[family.len() - rest.len()..].trim_start();
                    } else {
                        break;
                    }
                }
                parsed.family = canonical_annotation_font_family(family).or(parsed.family);
            }
            "font-family" => {
                parsed.family = value
                    .split(',')
                    .next()
                    .and_then(canonical_annotation_font_family)
                    .or(parsed.family);
            }
            "font-size" => parsed.size_pt = parse_point_value(value).or(parsed.size_pt),
            "font-weight" => {
                parsed.bold = matches!(value.to_ascii_lowercase().as_str(), "bold" | "bolder")
                    || value.parse::<u16>().is_ok_and(|weight| weight >= 600);
            }
            "font-style" => {
                parsed.italic =
                    matches!(value.to_ascii_lowercase().as_str(), "italic" | "oblique");
            }
            "text-align" => {
                parsed.alignment = match value.to_ascii_lowercase().as_str() {
                    "center" => Some(TextAlignment::Center),
                    "right" => Some(TextAlignment::Right),
                    "left" | "justify" => Some(TextAlignment::Left),
                    _ => parsed.alignment,
                };
            }
            "margin" => parsed.margin_pt = parse_point_value(value).or(parsed.margin_pt),
            "line-height" => {
                parsed.line_height_pt = parse_point_value(value).or(parsed.line_height_pt);
            }
            "color" => parsed.color = parse_css_color(value).or(parsed.color),
            _ => {}
        }
    }
    parsed
}

/// The `rg` colour and `Tf` size from a `/DA` string.
fn parse_default_appearance(default_appearance: &str) -> (Option<String>, Option<f64>) {
    let tokens = default_appearance.split_whitespace().collect::<Vec<_>>();
    let size = tokens
        .windows(2)
        .find(|pair| pair[1] == "Tf")
        .and_then(|pair| pair[0].parse::<f64>().ok())
        .filter(|size| size.is_finite() && *size > 0.);
    let color = tokens
        .windows(4)
        .find(|values| values[3] == "rg")
        .and_then(|values| {
            Some(format!(
                "#{:02x}{:02x}{:02x}",
                color_byte(values[0].parse::<f32>().ok()?),
                color_byte(values[1].parse::<f32>().ok()?),
                color_byte(values[2].parse::<f32>().ok()?),
            ))
        });
    (color, size)
}

/// A text style from Revu's `/DS`, falling back to `/DA` and the font
/// resources for producers that only write the standard default appearance.
fn import_text_style(
    document: &Document,
    annotation: &Dictionary,
    text_color_from_default_appearance: bool,
) -> Result<TextBoxStyle, PdfPersistenceError> {
    let style = dictionary_string(annotation, b"DS")
        .map(|value| parse_default_style(&value))
        .unwrap_or_default();
    let (appearance_color, appearance_size) =
        parse_default_appearance(&dictionary_string(annotation, b"DA").unwrap_or_default());
    let family = style
        .family
        .clone()
        .or_else(|| standard_annotation_font_family(document, annotation))
        .unwrap_or_else(|| "Helvetica".into());
    let size = style.size_pt.or(appearance_size).unwrap_or(12.);
    let color = style
        .color
        .clone()
        .or_else(|| text_color_from_default_appearance.then(|| appearance_color.clone()).flatten())
        .unwrap_or_else(|| "#000000".into());
    let bold = style.bold
        || default_appearance_font_resource(annotation).is_some_and(|resource| {
            let resource = String::from_utf8_lossy(&resource).to_ascii_lowercase();
            resource.contains("bold") || resource.ends_with("bld")
        });
    let mut text = TextBoxStyle::new(family, size, color, import_opacity(annotation))?
        .with_weight_and_alignment(
            if bold { 700 } else { 400 },
            style.alignment.unwrap_or(TextAlignment::Left),
        )?;
    if style.line_height_pt.is_some() || style.margin_pt.is_some() {
        let line_height = style
            .line_height_pt
            .filter(|value| *value > 0.)
            .unwrap_or(text.line_height_pt());
        let inset = style.margin_pt.unwrap_or(text.inset_pt());
        text = text.with_layout_metrics(line_height, inset)?;
    }
    Ok(text)
}


fn measurement_path_dictionary(
    annotation: &MeasurementPathAnnotation,
    appearance_id: ObjectId,
    font_resources: Dictionary,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let text = annotation.text_style();
    let calibration = annotation.calibration();
    let vertices = annotation
        .points()
        .iter()
        .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
        .collect::<Vec<_>>();
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => match annotation.kind {
            MeasurementPathKind::Polylength => "PolyLine",
            MeasurementPathKind::Area => "Polygon",
        },
        "IT" => match annotation.kind {
            MeasurementPathKind::Polylength => "PolyLineDimension",
            MeasurementPathKind::Area => "PolygonDimension",
        },
        "Subj" => pdf_literal(match annotation.kind {
            MeasurementPathKind::Polylength => "Polylength Measurement",
            MeasurementPathKind::Area => "Area Measurement",
        }),
        "Rect" => pdf_rect(measurement_path_bounds(annotation)),
        "Vertices" => vertices,
        "NM" => pdf_literal(annotation.id.as_str()),
        "C" => color_array(appearance.stroke_color()),
        "Cap" => Object::Boolean(true),
        "AlignOnSegment" => Object::Boolean(true),
        "MeasurementTypes" => match annotation.kind {
            MeasurementPathKind::Polylength => 130,
            MeasurementPathKind::Area => 129,
        },
        "Measure" => calibration_measure_dictionary(calibration),
        "DepthUnit" => calibration_depth_unit(calibration),
        "Label" => pdf_literal(calibration.label()),
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_measurement_caption(&mut replacement, text, &annotation.caption());
    set_shape_border(&mut replacement, appearance);
    match annotation.kind {
        MeasurementPathKind::Polylength => {
            replacement.set("IC", color_array(appearance.stroke_color()));
            replacement.set("RiseDrop", 0);
        }
        MeasurementPathKind::Area => {
            if let Some(fill_color) = appearance.fill_color() {
                replacement.set("IC", color_array(fill_color));
            }
            // Revu always records an Area's fill opacity.
            replacement.set("FillOpacity", Object::Real(appearance.fill_opacity() as f32));
            replacement.set("PitchRun", 12);
            replacement.set("SlopeType", 1);
        }
    }
    if text.font_family() != "Helvetica" {
        replacement.set("DR", dictionary! { "Font" => font_resources });
    }
    set_markup_opacity(&mut replacement, appearance.opacity());
    if let Ok(subject) = original.get(b"Subj") {
        replacement.set("Subj", subject.clone());
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn add_length_appearance(
    document: &mut Document,
    annotation: &LengthAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let caption_text = length_caption_text(annotation);
    let mut caption_lines = caption_text
        .as_deref()
        .map(|caption| {
            unicode_text_lines(caption, annotation.appearance.text().font_family()).map_err(|()| {
                PdfPersistenceError::InvalidDocument(
                    "length caption appearance cannot encode one or more Unicode characters with the bundled PDF fonts"
                        .into(),
                )
            })
        })
        .transpose()?
        .unwrap_or_default();
    let font_resources = appearance_text_font_resources(document, &mut caption_lines)?;
    let bounds = length_bounds(annotation);
    let line = annotation.appearance.line();
    let text = annotation.appearance.text();
    let layout = length_line_layout(annotation);
    let local = |point: PdfPoint| (point.x - bounds.x, point.y - bounds.y);
    let (stroke_red, stroke_green, stroke_blue) = color_components(line.stroke_color());
    let (text_red, text_green, text_blue) = color_components(text.color());
    let dash_operation = rectangle_dash_pattern(line.stroke_style(), line.stroke_width_pt())
        .map_or_else(String::new, |(dash, gap)| {
            format!("[{dash:.6} {gap:.6}] 0 d\n")
        });
    let mut lines = format!(
        "q\n/GS0 gs\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} rg\n{dash_operation}{:.6} w\n",
        line.stroke_width_pt()
    );
    append_measurement_line(&mut lines, &layout, local);
    let mut content = lines.into_bytes();
    if annotation.calibration().show_caption() {
        let caption_text = caption_text
            .as_deref()
            .expect("visible length caption must have text");
        let caption = measurement_caption_layout(layout.caption_center, caption_text, text, true);
        content.extend_from_slice(
            caption_rotation_operator(
                caption.rect,
                bounds,
                measurement_caption_angle(annotation.start, annotation.end),
            )
            .as_bytes(),
        );
        if uses_helvetica_winansi_fast_path(caption_text, text.font_family()) {
            let encoded = text_appearance_line_bytes(caption_text);
            content.extend_from_slice(format!(
                "BT {text_red:.6} {text_green:.6} {text_blue:.6} rg /Helv {:.6} Tf 1 0 0 1 {:.6} {:.6} Tm (",
                text.font_size_pt(),
                caption.text_origin.x - bounds.x,
                caption.text_origin.y - bounds.y,
            ).as_bytes());
            content.extend_from_slice(&escape_pdf_literal_bytes(&encoded));
            content.extend_from_slice(b") Tj ");
        } else {
            content.extend_from_slice(
                format!(
                    "BT {text_red:.6} {text_green:.6} {text_blue:.6} rg 1 0 0 1 {:.6} {:.6} Tm\n",
                    caption.text_origin.x - bounds.x,
                    caption.text_origin.y - bounds.y,
                )
                .as_bytes(),
            );
            append_appearance_text_line(
                &mut content,
                caption_lines
                    .first()
                    .expect("visible length caption must have one line"),
                text.font_size_pt(),
                caption.text_origin.x - bounds.x,
                caption.text_origin.y - bounds.y,
            );
        }
        content.extend_from_slice(b"ET\nQ\n");
    }
    content.extend_from_slice(b"Q\n");
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "Font" => font_resources,
                "ExtGState" => dictionary! { "GS0" => dictionary! { "Type" => "ExtGState", "CA" => Object::Real(line.opacity() as f32), "ca" => Object::Real(text.opacity() as f32) } },
            },
        },
        content,
    )))
}

fn length_dictionary(
    annotation: &LengthAnnotation,
    appearance_id: ObjectId,
    font_resources: Dictionary,
    original: &Dictionary,
) -> Dictionary {
    let calibration = annotation.calibration();
    let line = annotation.appearance.line();
    let text = annotation.appearance.text();
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "IT" => "LineDimension",
        "Subj" => pdf_literal("Length Measurement"),
        "Rect" => pdf_rect(length_bounds(annotation)),
        "NM" => pdf_literal(annotation.id.as_str()),
        "L" => vec![
            Object::Real(annotation.start.x as f32),
            Object::Real(annotation.start.y as f32),
            Object::Real(annotation.end.x as f32),
            Object::Real(annotation.end.y as f32),
        ],
        "BS" => markup_border_style(line.stroke_width_pt(), line.stroke_style()),
        "C" => color_array(line.stroke_color()),
        "IC" => color_array(line.stroke_color()),
        "LE" => vec![Object::Name(b"ClosedArrow".to_vec()), Object::Name(b"ClosedArrow".to_vec())],
        "LL" => Object::Real(LENGTH_LEADER_LENGTH_PT as f32),
        "LLE" => Object::Real(DIMENSION_LEADER_EXTENSION_PT as f32),
        "Cap" => Object::Boolean(true),
        "MeasurementTypes" => 130,
        "Measure" => calibration_measure_dictionary(calibration),
        "DepthUnit" => calibration_depth_unit(calibration),
        "Label" => pdf_literal(calibration.label()),
        "PitchRun" => 12,
        "SlopeType" => 1,
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_measurement_caption(&mut dictionary, text, &annotation.caption());
    if text.font_family() != "Helvetica" {
        dictionary.set("DR", dictionary! { "Font" => font_resources });
    }
    set_markup_opacity(&mut dictionary, line.opacity());
    if let Ok(subject) = original.get(b"Subj") {
        dictionary.set("Subj", subject.clone());
    }
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn dimension_caption_text_width(annotation: &DimensionAnnotation) -> f64 {
    if annotation.content().is_empty() {
        return 0.;
    }
    let text = annotation.appearance.text();
    appearance_text_width_pt(annotation.content(), text.font_size_pt(), text.font_family())
}

fn dimension_line_layout(annotation: &DimensionAnnotation) -> MeasurementLineLayout {
    measurement_line_layout(
        annotation.start,
        annotation.end,
        annotation.dimension_line_offset(),
        annotation.appearance.line().stroke_width_pt(),
        dimension_caption_text_width(annotation),
    )
    .expect("validated Dimension endpoints are distinct")
}

fn length_caption_text(annotation: &LengthAnnotation) -> Option<String> {
    annotation
        .calibration()
        .show_caption()
        .then(|| annotation.caption())
}

fn length_line_layout(annotation: &LengthAnnotation) -> MeasurementLineLayout {
    let text = annotation.appearance.text();
    let caption_width = length_caption_text(annotation).map_or(0., |caption| {
        appearance_text_width_pt(&caption, text.font_size_pt(), text.font_family())
    });
    measurement_line_layout(
        annotation.start,
        annotation.end,
        LENGTH_LEADER_LENGTH_PT,
        annotation.appearance.line().stroke_width_pt(),
        caption_width,
    )
    .expect("validated Length endpoints are distinct")
}

fn measurement_line_bounds(
    layout: &MeasurementLineLayout,
    stroke_width_pt: f64,
    caption: Option<PdfRect>,
) -> PdfRect {
    let mut points = Vec::new();
    for (from, to) in layout
        .extension_lines
        .iter()
        .chain(layout.dimension_segments.iter())
    {
        points.extend([*from, *to]);
    }
    for arrowhead in &layout.arrowheads {
        points.extend(arrowhead.iter().copied());
    }
    let bounds = measurement_points_bounds(&points, stroke_width_pt.max(1.));
    caption.map_or(bounds, |caption| union_measurement_bounds(bounds, caption))
}

/// Revu turns a Length or Dimension caption to follow its line, kept
/// upright: the angle in radians, within (-90°, 90°].
fn measurement_caption_angle(start: PdfPoint, end: PdfPoint) -> f64 {
    let mut angle = (end.y - start.y).atan2(end.x - start.x);
    if angle > std::f64::consts::FRAC_PI_2 + 1e-9 {
        angle -= std::f64::consts::PI;
    } else if angle <= -std::f64::consts::FRAC_PI_2 + 1e-9 {
        angle += std::f64::consts::PI;
    }
    angle
}

/// `q … cm` turning the caption about its centre (appearance coordinates).
fn caption_rotation_operator(caption: PdfRect, bounds: PdfRect, angle: f64) -> String {
    let centre_x = caption.x + caption.width / 2. - bounds.x;
    let centre_y = caption.y + caption.height / 2. - bounds.y;
    let (sin, cos) = angle.sin_cos();
    format!(
        "q {cos:.6} {sin:.6} {:.6} {cos:.6} {:.6} {:.6} cm\n",
        -sin,
        centre_x - cos * centre_x + sin * centre_y,
        centre_y - sin * centre_x - cos * centre_y,
    )
}

fn dimension_bounds(annotation: &DimensionAnnotation) -> PdfRect {
    let layout = dimension_line_layout(annotation);
    let caption = (!annotation.content().is_empty()).then(|| {
        measurement_caption_layout(
            layout.caption_center,
            annotation.content(),
            annotation.appearance.text(),
            true,
        )
        .rect
    })
    .map(|rect| {
        rotated_rect_bounds(
            rect,
            measurement_caption_angle(annotation.start, annotation.end).to_degrees(),
        )
    });
    measurement_line_bounds(&layout, annotation.appearance.line().stroke_width_pt(), caption)
}

/// Strokes the extension lines and dimension line, then fills and strokes
/// the closed arrowheads, as Revu's appearance does.
fn append_measurement_line(
    content: &mut String,
    layout: &MeasurementLineLayout,
    local: impl Fn(PdfPoint) -> (f64, f64),
) {
    for (from, to) in layout
        .extension_lines
        .iter()
        .chain(layout.dimension_segments.iter())
    {
        let (from_x, from_y) = local(*from);
        let (to_x, to_y) = local(*to);
        content.push_str(&format!(
            "{from_x:.6} {from_y:.6} m {to_x:.6} {to_y:.6} l S\n"
        ));
    }
    content.push_str("[] 0 d\n");
    for arrowhead in &layout.arrowheads {
        let [tip, left, right] = arrowhead.map(&local);
        content.push_str(&format!(
            "{:.6} {:.6} m {:.6} {:.6} l {:.6} {:.6} l b\n",
            left.0, left.1, tip.0, tip.1, right.0, right.1,
        ));
    }
}

fn add_dimension_appearance(
    document: &mut Document,
    annotation: &DimensionAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let text = annotation.appearance.text();
    let mut caption_lines = unicode_text_lines(annotation.content(), text.font_family()).map_err(|()| {
        PdfPersistenceError::InvalidDocument(
            "dimension caption appearance cannot encode one or more Unicode characters with the bundled PDF fonts"
                .into(),
        )
    })?;
    let font_resources = appearance_text_font_resources(document, &mut caption_lines)?;
    let bounds = dimension_bounds(annotation);
    let line = annotation.appearance.line();
    let layout = dimension_line_layout(annotation);
    let local = |point: PdfPoint| (point.x - bounds.x, point.y - bounds.y);
    let (stroke_red, stroke_green, stroke_blue) = color_components(line.stroke_color());
    let (text_red, text_green, text_blue) = color_components(text.color());
    let dash_operation = rectangle_dash_pattern(line.stroke_style(), line.stroke_width_pt())
        .map_or_else(String::new, |(dash, gap)| {
            format!("[{dash:.6} {gap:.6}] 0 d\n")
        });
    let mut content = format!(
        "q\n/GSDimension gs\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} rg\n{dash_operation}{:.6} w\n",
        line.stroke_width_pt(),
    );
    append_measurement_line(&mut content, &layout, local);
    let caption = measurement_caption_layout(
        layout.caption_center,
        annotation.content(),
        text,
        true,
    );
    let (caption_x, caption_y) = local(caption.text_origin);
    let mut content = content.into_bytes();
    if !annotation.content().is_empty() {
        content.extend_from_slice(
            caption_rotation_operator(
                caption.rect,
                bounds,
                measurement_caption_angle(annotation.start, annotation.end),
            )
            .as_bytes(),
        );
    }
    if annotation.content().is_empty() {
        // An unlabelled Dimension, like Revu's, draws no text.
    } else if uses_helvetica_winansi_fast_path(annotation.content(), text.font_family()) {
        let encoded = text_appearance_line_bytes(annotation.content());
        content.extend_from_slice(format!(
            "BT {text_red:.6} {text_green:.6} {text_blue:.6} rg /Helv {:.6} Tf 1 0 0 1 {caption_x:.6} {caption_y:.6} Tm (",
            text.font_size_pt(),
        ).as_bytes());
        content.extend_from_slice(&escape_pdf_literal_bytes(&encoded));
        content.extend_from_slice(b") Tj ");
    } else {
        content.extend_from_slice(format!(
            "BT {text_red:.6} {text_green:.6} {text_blue:.6} rg 1 0 0 1 {caption_x:.6} {caption_y:.6} Tm\n",
        ).as_bytes());
        append_appearance_text_line(
            &mut content,
            caption_lines
                .first()
                .expect("dimension caption must have one line"),
            text.font_size_pt(),
            caption_x,
            caption_y,
        );
    }
    if !annotation.content().is_empty() {
        content.extend_from_slice(b"ET\nQ\n");
    }
    content.extend_from_slice(b"Q\n");
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => font_resources,
                "ExtGState" => dictionary! {
                    "GSDimension" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(line.opacity() as f32),
                        "ca" => Object::Real(text.opacity() as f32),
                    },
                },
            },
        },
        content,
    )))
}

fn dimension_dictionary(
    annotation: &DimensionAnnotation,
    appearance_id: ObjectId,
    font_resources: Dictionary,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let line = annotation.appearance.line();
    let text = annotation.appearance.text();
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "IT" => "LineDimension",
        "Subj" => pdf_literal("Dimension"),
        "Rect" => pdf_rect(dimension_bounds(annotation)),
        "NM" => pdf_literal(annotation.id.as_str()),
        "L" => vec![
            Object::Real(annotation.start.x as f32),
            Object::Real(annotation.start.y as f32),
            Object::Real(annotation.end.x as f32),
            Object::Real(annotation.end.y as f32),
        ],
        "BS" => markup_border_style(line.stroke_width_pt(), line.stroke_style()),
        "C" => color_array(line.stroke_color()),
        "IC" => color_array(line.stroke_color()),
        "LE" => vec![Object::Name(b"ClosedArrow".to_vec()), Object::Name(b"ClosedArrow".to_vec())],
        "LL" => Object::Real(annotation.dimension_line_offset() as f32),
        "LLE" => Object::Real(DIMENSION_LEADER_EXTENSION_PT as f32),
        "Cap" => Object::Boolean(true),
        "PitchRun" => 12,
        "SlopeType" => 0,
        "AP" => dictionary! { "N" => appearance_id },
    };
    if annotation.content().is_empty() {
        // Revu's Dimension carries only its text style, no label.
        replacement.set("DS", pdf_literal(&revu_default_style(text, None)));
    } else {
        set_measurement_caption(&mut replacement, text, annotation.content());
    }
    if text.font_family() != "Helvetica" {
        replacement.set("DR", dictionary! { "Font" => font_resources });
    }
    set_markup_opacity(&mut replacement, line.opacity());
    if let Ok(subject) = original.get(b"Subj") {
        replacement.set("Subj", subject.clone());
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}


fn add_rgba_image_xobject(document: &mut Document, asset: &DecodedRgbaAsset) -> ObjectId {
    let mut rgb = Vec::with_capacity(asset.rgba().len() / 4 * 3);
    let mut alpha = Vec::with_capacity(asset.rgba().len() / 4);
    for pixel in asset.rgba().chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
        alpha.push(pixel[3]);
    }
    let mut image = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Image",
        "Width" => i64::from(asset.width_px()),
        "Height" => i64::from(asset.height_px()),
        "ColorSpace" => "DeviceRGB",
        "BitsPerComponent" => 8,
    };
    if alpha.iter().any(|value| *value != u8::MAX) {
        let alpha_id = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(asset.width_px()),
                "Height" => i64::from(asset.height_px()),
                "ColorSpace" => "DeviceGray",
                "BitsPerComponent" => 8,
            },
            alpha,
        ));
        image.set("SMask", alpha_id);
    }
    let mut stream = Stream::new(image, rgb);
    let _ = stream.compress();
    document.add_object(stream)
}

/// A media appearance in Revu's layout: the unrotated box in page space with
/// a rotation `/Matrix`, painting the image over the box.
fn add_media_appearance(
    document: &mut Document,
    rect: PdfRect,
    rotation_degrees: f64,
    opacity: f64,
    image_id: ObjectId,
) -> ObjectId {
    let (bbox, matrix, _) = rotated_box_appearance_placement(rect, rotation_degrees);
    let translucent = opacity < 1.;
    let content = format!(
        "q\n{}{:.6} 0 0 {:.6} {:.6} {:.6} cm\n/Image Do\nQ\n",
        if translucent { "/GS0 gs\n" } else { "" },
        rect.width,
        rect.height,
        rect.x,
        rect.y,
    );
    let mut resources = dictionary! {
        "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"ImageC".to_vec())],
        "XObject" => dictionary! { "Image" => image_id },
    };
    if translucent {
        resources.set(
            "ExtGState",
            dictionary! { "GS0" => dictionary! {
                "Type" => "ExtGState",
                "CA" => Object::Real(opacity as f32),
                "ca" => Object::Real(opacity as f32),
            } },
        );
    }
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => bbox,
            "Matrix" => matrix,
            "Resources" => resources,
        },
        content.into_bytes(),
    ))
}

/// Returns the appearance and the image XObject it paints.
fn add_image_appearance(
    document: &mut Document,
    annotation: &ImageAnnotation,
) -> (ObjectId, ObjectId) {
    let image_id = add_rgba_image_xobject(document, annotation.asset());
    let appearance_id = add_media_appearance(
        document,
        annotation.rect,
        annotation.rotation_degrees(),
        annotation.opacity(),
        image_id,
    );
    (appearance_id, image_id)
}

fn image_dictionary(
    annotation: &ImageAnnotation,
    appearance_id: ObjectId,
    image_id: Option<ObjectId>,
    original: &Dictionary,
) -> Dictionary {
    let (_, _, rect) =
        rotated_box_appearance_placement(annotation.rect, annotation.rotation_degrees());
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "IT" => "SquareImage",
        "Subj" => pdf_literal("Image"),
        "Rect" => pdf_rect(rect),
        "RD" => rect_differences_array(0.),
        "NM" => pdf_literal(annotation.id.as_str()),
        "C" => color_array("#ff0000"),
        "BS" => markup_border_style(0., StrokeStyle::Solid),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if let Some(image_id) = image_id {
        dictionary.set("Image", image_id);
    }
    set_markup_opacity(&mut dictionary, annotation.opacity());
    set_markup_rotation(&mut dictionary, annotation.rotation_degrees());
    preserve_markup_comment(&mut dictionary, original);
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn add_snapshot_appearance(document: &mut Document, annotation: &SnapshotAnnotation) -> ObjectId {
    let image_id = add_rgba_image_xobject(document, annotation.asset());
    add_snapshot_form_appearance(document, annotation, image_id)
}

fn add_snapshot_form_appearance(
    document: &mut Document,
    annotation: &SnapshotAnnotation,
    image_id: ObjectId,
) -> ObjectId {
    add_media_appearance(
        document,
        annotation.rect,
        annotation.rotation_degrees(),
        annotation.opacity(),
        image_id,
    )
}

fn snapshot_dictionary(
    annotation: &SnapshotAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Dictionary {
    let (_, _, rect) =
        rotated_box_appearance_placement(annotation.rect, annotation.rotation_degrees());
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Stamp",
        "IT" => "StampSnapshot",
        "Subj" => pdf_literal("Snapshot"),
        "Rect" => pdf_rect(rect),
        "NM" => pdf_literal(annotation.id.as_str()),
        "C" => color_array("#ff0000"),
        // Revu records a Snapshot's rotation even when it is zero.
        "Rotation" => Object::Real(annotation.rotation_degrees().rem_euclid(360.) as f32),
        "AP" => dictionary! { "N" => appearance_id },
    };
    set_markup_opacity(&mut dictionary, annotation.opacity());
    preserve_markup_comment(&mut dictionary, original);
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

/// A markup's PDF `/NM` is its id, as in Revu.
fn canonical_native_annotation_name(id: &MarkupId) -> String {
    id.as_str().to_owned()
}

/// A new Cloud+ pair: the cloud carries the markup id as its `/NM` and the
/// text member gets its own Revu-style name.
fn new_cloud_plus_native_names(id: &MarkupId) -> (String, String) {
    (id.as_str().to_owned(), crate::annotation_model::generate_markup_name())
}



fn require_available_native_name(
    document: &Document,
    native_name: &str,
    owned_object_id: ObjectId,
) -> Result<(), PdfPersistenceError> {
    // PDF-lib retains superseded indirect annotation dictionaries as
    // unreachable objects after rewriting an annotation. They have no page
    // authority and must not prevent a later editor from canonicalising the
    // live annotation. Still reject another direct or referenced annotation
    // with the same name on any page.
    let collision = document.get_pages().into_values().any(|page_id| {
        let Ok(page) = document.get_object(page_id).and_then(Object::as_dict) else {
            return false;
        };
        let Ok(annotations) = page
            .get(b"Annots")
            .and_then(|value| resolve_object(document, value))
            .and_then(Object::as_array)
        else {
            return false;
        };
        annotations.iter().any(|annotation| {
            if matches!(annotation, Object::Reference(object_id) if *object_id == owned_object_id) {
                return false;
            }
            resolve_object(document, annotation)
                .and_then(Object::as_dict)
                .ok()
                .is_some_and(|dictionary| {
                    dictionary_string(dictionary, b"NM").as_deref() == Some(native_name)
                })
        })
    });
    if collision {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "canonical native annotation name {native_name} belongs to another object"
        )));
    }
    Ok(())
}











fn image_appearance_object_ids(document: &Document, annotation: &Dictionary) -> Vec<ObjectId> {
    let Some(form_id) = normal_appearance_object_id(annotation) else {
        return Vec::new();
    };
    let image_id = document
        .get_object(form_id)
        .ok()
        .and_then(|object| object.as_stream().ok())
        .and_then(|stream| stream.dict.get(b"Resources").ok())
        .and_then(|object| object.as_dict().ok())
        .and_then(|resources| resources.get(b"XObject").ok())
        .and_then(|object| object.as_dict().ok())
        .and_then(|xobjects| xobjects.iter().next().map(|(_, object)| object))
        .and_then(|object| object.as_reference().ok());
    let alpha_id = image_id
        .and_then(|image_id| document.get_object(image_id).ok())
        .and_then(|object| object.as_stream().ok())
        .and_then(|stream| stream.dict.get(b"SMask").ok())
        .and_then(|object| object.as_reference().ok());
    [Some(form_id), image_id, alpha_id]
        .into_iter()
        .flatten()
        .collect()
}

fn normal_appearance_object_id(dictionary: &Dictionary) -> Option<ObjectId> {
    dictionary
        .get(b"AP")
        .ok()?
        .as_dict()
        .ok()?
        .get(b"N")
        .ok()?
        .as_reference()
        .ok()
}

fn appearance_graph_object_ids(document: &Document, annotation: &Dictionary) -> HashSet<ObjectId> {
    let mut graph = HashSet::new();
    let mut pending = Vec::new();
    if let Ok(appearance) = annotation.get(b"AP") {
        collect_referenced_object_ids(appearance, &mut pending);
    }
    while let Some(object_id) = pending.pop() {
        if !graph.insert(object_id) {
            continue;
        }
        if let Ok(object) = document.get_object(object_id) {
            collect_referenced_object_ids(object, &mut pending);
        }
    }
    graph
}

fn collect_referenced_object_ids(object: &Object, output: &mut Vec<ObjectId>) {
    match object {
        Object::Reference(object_id) => output.push(*object_id),
        Object::Array(values) => {
            for value in values {
                collect_referenced_object_ids(value, output);
            }
        }
        Object::Dictionary(dictionary) => {
            for (_, value) in dictionary.iter() {
                collect_referenced_object_ids(value, output);
            }
        }
        Object::Stream(stream) => {
            for (_, value) in stream.dict.iter() {
                collect_referenced_object_ids(value, output);
            }
        }
        _ => {}
    }
}

fn remove_unreferenced_object_graph(document: &mut Document, graph: &HashSet<ObjectId>) {
    if graph.is_empty() {
        return;
    }
    let mut externally_reachable = HashSet::new();
    let mut pending = Vec::new();
    for (source_id, object) in &document.objects {
        if graph.contains(source_id) {
            continue;
        }
        let mut references = Vec::new();
        collect_referenced_object_ids(object, &mut references);
        pending.extend(
            references
                .into_iter()
                .filter(|target| graph.contains(target)),
        );
    }
    let mut trailer_references = Vec::new();
    collect_referenced_object_ids(
        &Object::Dictionary(document.trailer.clone()),
        &mut trailer_references,
    );
    pending.extend(
        trailer_references
            .into_iter()
            .filter(|target| graph.contains(target)),
    );
    while let Some(object_id) = pending.pop() {
        if !externally_reachable.insert(object_id) {
            continue;
        }
        if let Some(object) = document.objects.get(&object_id) {
            let mut references = Vec::new();
            collect_referenced_object_ids(object, &mut references);
            pending.extend(
                references
                    .into_iter()
                    .filter(|target| graph.contains(target)),
            );
        }
    }
    for object_id in graph.difference(&externally_reachable) {
        document.objects.remove(object_id);
    }
}

fn remove_object_if_unreferenced(document: &mut Document, object_id: ObjectId) {
    let references = document
        .objects
        .values()
        .map(|object| object_reference_count(object, object_id))
        .sum::<usize>()
        + object_reference_count(&Object::Dictionary(document.trailer.clone()), object_id);
    if references == 0 {
        document.objects.remove(&object_id);
    }
}

fn object_reference_count(object: &Object, target: ObjectId) -> usize {
    match object {
        Object::Reference(object_id) => usize::from(*object_id == target),
        Object::Array(values) => values
            .iter()
            .map(|value| object_reference_count(value, target))
            .sum(),
        Object::Dictionary(dictionary) => dictionary
            .iter()
            .map(|(_, value)| object_reference_count(value, target))
            .sum(),
        Object::Stream(stream) => stream
            .dict
            .iter()
            .map(|(_, value)| object_reference_count(value, target))
            .sum(),
        _ => 0,
    }
}

fn pdf_literal(value: &str) -> Object {
    lopdf::text_string(value)
}

fn pdf_text_box_contents(value: &str) -> Object {
    let mut bytes = Vec::with_capacity(2 + value.encode_utf16().count() * 2);
    lopdf::encode_utf16_be(value, &mut bytes);
    Object::String(bytes, StringFormat::Hexadecimal)
}

fn decode_pdf_text_string_compat(object: &Object) -> Result<String, PdfPersistenceError> {
    let bytes = object.as_str().map_err(|_| {
        PdfPersistenceError::InvalidDocument("PDF text value must be a PDF string".into())
    })?;
    if let Some(payload) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return decode_text_box_utf16(payload, u16::from_be_bytes, "UTF-16BE");
    }
    if let Some(payload) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return decode_text_box_utf16(payload, u16::from_le_bytes, "UTF-16LE");
    }
    if let Some(payload) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return String::from_utf8(payload.to_vec()).map_err(|_| {
            PdfPersistenceError::InvalidDocument(
                "PDF text value has an invalid UTF-8 byte order mark payload".into(),
            )
        });
    }
    if let Ok(value) = String::from_utf8(bytes.to_vec()) {
        return Ok(value);
    }
    lopdf::decode_text_string(object).map_err(|_| {
        PdfPersistenceError::InvalidDocument(
            "PDF text value is not valid UTF-8 or PDFDocEncoding".into(),
        )
    })
}

fn decode_text_box_utf16(
    payload: &[u8],
    decode_unit: fn([u8; 2]) -> u16,
    encoding: &str,
) -> Result<String, PdfPersistenceError> {
    if !payload.len().is_multiple_of(2) {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "PDF text value has an odd-length {encoding} payload"
        )));
    }
    let units = payload
        .chunks_exact(2)
        .map(|bytes| decode_unit([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).map_err(|_| {
        PdfPersistenceError::InvalidDocument(format!(
            "PDF text value has an invalid {encoding} surrogate sequence"
        ))
    })
}

fn pdf_rect(rect: PdfRect) -> Object {
    Object::Array(vec![
        Object::Real(rect.x as f32),
        Object::Real(rect.y as f32),
        Object::Real((rect.x + rect.width) as f32),
        Object::Real((rect.y + rect.height) as f32),
    ])
}

fn rect_bbox(rect: PdfRect) -> Object {
    Object::Array(vec![
        Object::Real(0.0),
        Object::Real(0.0),
        Object::Real(rect.width as f32),
        Object::Real(rect.height as f32),
    ])
}

fn preserve_annotation_metadata(replacement: &mut Dictionary, original: &Dictionary, locked: bool) {
    let mut flags = original
        .get(b"F")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .or_else(|| {
            replacement
                .get(b"F")
                .ok()
                .and_then(|value| value.as_i64().ok())
        })
        .unwrap_or(4);
    if locked {
        flags |= 128;
    } else {
        flags &= !128;
    }
    replacement.set("F", flags);
    for key in [
        b"T".as_slice(),
        b"CreationDate".as_slice(),
        b"P".as_slice(),
        b"StateModel".as_slice(),
        b"State".as_slice(),
    ] {
        if let Ok(value) = original.get(key) {
            replacement.set(key, value.clone());
        }
    }
    // Revu stamps every markup with its author and creation date, and
    // refreshes the modification date whenever the markup is rewritten.
    let now = pdf_date_now();
    if replacement.get(b"T").is_err() {
        replacement.set("T", pdf_literal(&markup_author()));
    }
    if replacement.get(b"CreationDate").is_err() {
        replacement.set("CreationDate", pdf_literal(&now));
    }
    replacement.set("M", pdf_literal(&now));
    preserve_dash_pattern(replacement, original);
    // Empty comments and full opacity are defaults Revu leaves implicit.
    if matches!(replacement.get(b"Contents"), Ok(Object::String(value, _)) if value.is_empty()) {
        replacement.remove(b"Contents");
    }
    if replacement
        .get(b"CA")
        .ok()
        .and_then(|value| value.as_float().ok())
        .is_some_and(|opacity| opacity >= 1.)
    {
        replacement.remove(b"CA");
    }
}

/// Keeps the original `/BS /D` dash array when an edit leaves the line's
/// style and width unchanged, so a Revu `dashed1..6` pattern is not replaced
/// by the nearest native pattern.
fn preserve_dash_pattern(replacement: &mut Dictionary, original: &Dictionary) {
    let border = |dictionary: &Dictionary| {
        dictionary
            .get(b"BS")
            .ok()
            .and_then(|value| value.as_dict().ok())
            .cloned()
    };
    let (Some(mut next), Some(previous)) = (border(replacement), border(original)) else {
        return;
    };
    let is_dashed = |border: &Dictionary| dictionary_name(border, b"S").as_deref() == Some("D");
    if !is_dashed(&next) || !is_dashed(&previous) || previous.get(b"D").is_err() {
        return;
    }
    let width = |border: &Dictionary| dictionary_float(border, b"W").unwrap_or(1.);
    if (width(&next) - width(&previous)).abs() > 1e-4 {
        return;
    }
    let style = |dictionary: &Dictionary, border: &Dictionary| {
        let mut probe = dictionary.clone();
        probe.set("BS", border.clone());
        import_stroke_style(&probe, width(border))
    };
    if style(replacement, &next) == style(original, &previous) {
        next.set("D", previous.get(b"D").expect("checked above").clone());
        replacement.set("BS", next);
    }
}

/// Revu writes `/CA` for opacity only when it is below one and never `/ca`.
fn set_markup_opacity(dictionary: &mut Dictionary, opacity: f64) {
    dictionary.remove(b"ca");
    if opacity < 1. {
        dictionary.set("CA", Object::Real(opacity as f32));
    } else {
        dictionary.remove(b"CA");
    }
}

/// Bluebeam's fill-opacity key, written only when the fill is translucent.
fn set_markup_fill_opacity(dictionary: &mut Dictionary, fill_opacity: f64) {
    if fill_opacity < 1. {
        dictionary.set("FillOpacity", Object::Real(fill_opacity as f32));
    } else {
        dictionary.remove(b"FillOpacity");
    }
}

/// Border style as Revu writes it: solid `/S /S` or dashed `/S /D /D [..]`.
fn markup_border_style(width_pt: f64, style: StrokeStyle) -> Dictionary {
    let mut border = dictionary! {
        "Type" => "Border",
        "W" => Object::Real(width_pt as f32),
        "S" => "S",
    };
    if let Some((dash, gap)) = rectangle_dash_pattern(style, width_pt) {
        border.set("S", "D");
        border.set(
            "D",
            vec![Object::Real(dash as f32), Object::Real(gap as f32)],
        );
    }
    border
}

/// The operating-system account name, which Revu records as the author.
fn markup_author() -> String {
    ["USER", "USERNAME", "LOGNAME"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| "Butter Paper".into())
}

/// A PDF date in local time with its UTC offset, e.g.
/// `D:20261002005432+10'00'`, matching Revu.
fn pdf_date_now() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs() as i64);
    let offset_seconds = local_utc_offset_seconds(seconds);
    format_pdf_date(seconds, offset_seconds)
}

#[cfg(unix)]
fn local_utc_offset_seconds(seconds: i64) -> i64 {
    let time = seconds as libc::time_t;
    let mut local = std::mem::MaybeUninit::<libc::tm>::zeroed();
    // SAFETY: `localtime_r` writes only into the provided `tm`.
    let converted = unsafe { libc::localtime_r(&time, local.as_mut_ptr()) };
    if converted.is_null() {
        return 0;
    }
    // SAFETY: `localtime_r` succeeded and initialised `local`.
    i64::from(unsafe { local.assume_init() }.tm_gmtoff)
}

#[cfg(not(unix))]
fn local_utc_offset_seconds(_seconds: i64) -> i64 {
    0
}

fn format_pdf_date(seconds: i64, offset_seconds: i64) -> String {
    let local = seconds + offset_seconds;
    let days = local.div_euclid(86_400);
    let time_of_day = local.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant), valid for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 { month_index + 3 } else { month_index - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let offset_minutes = offset_seconds.abs() / 60;
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    format!(
        "D:{year:04}{month:02}{day:02}{:02}{:02}{:02}{sign}{:02}'{:02}'",
        time_of_day / 3_600,
        time_of_day / 60 % 60,
        time_of_day % 60,
        offset_minutes / 60,
        offset_minutes % 60,
    )
}

fn color_array(color: &str) -> Object {
    let (red, green, blue) = color_components(color);
    Object::Array(vec![
        Object::Real(red),
        Object::Real(green),
        Object::Real(blue),
    ])
}

fn color_components(color: &str) -> (f32, f32, f32) {
    let component = |range: std::ops::Range<usize>| {
        f32::from(u8::from_str_radix(&color[range], 16).expect("validated colors are hexadecimal"))
            / 255.0
    };
    (component(1..3), component(3..5), component(5..7))
}


/// Render preparation leaves the source document and persistence data untouched.
#[derive(Debug)]
pub enum RetainedAnnotationRender {
    /// No annotations remain outside the native editable overlay.
    None,
    /// All annotations are retained; the original PDFium document can be reused.
    Original,
    /// Render-only PDF with editable slots removed and existing Widget APs adapted.
    Filtered(Vec<u8>),
}

const MAX_PDFIUM_DISPLAY_OPTIONAL_CONTENT_GROUPS: usize = 4_096;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PdfiumViewStateOnLocation {
    IndirectView(ObjectId),
    DirectViewInUsage(ObjectId),
    DirectViewInGroup(ObjectId),
}

fn resolved_object_and_id<'a>(
    document: &'a Document,
    object: &'a Object,
) -> Result<(Option<ObjectId>, &'a Object), lopdf::Error> {
    let mut object = object;
    let mut object_id = None;
    let mut visited = HashSet::new();
    for _ in 0..MAX_OPTIONAL_PDF_REFERENCE_HOPS {
        let Object::Reference(next_id) = object else {
            return Ok((object_id, object));
        };
        if !visited.insert(*next_id) {
            return Err(lopdf::Error::ReferenceCycle(*next_id));
        }
        object_id = Some(*next_id);
        object = document.get_object(*next_id)?;
    }
    if matches!(object, Object::Reference(_)) {
        Err(lopdf::Error::ReferenceLimit)
    } else {
        Ok((object_id, object))
    }
}

fn pdfium_view_state_on_location(
    document: &Document,
    group_id: ObjectId,
) -> Option<PdfiumViewStateOnLocation> {
    let group = document.get_object(group_id).ok()?.as_dict().ok()?;
    if dictionary_name(group, b"Type").as_deref() != Some("OCG") {
        return None;
    }
    let usage = group.get(b"Usage").ok()?;
    let (usage_id, usage) = resolved_object_and_id(document, usage).ok()?;
    let usage = usage.as_dict().ok()?;
    let view = usage.get(b"View").ok()?;
    let (view_id, view) = resolved_object_and_id(document, view).ok()?;
    let view = view.as_dict().ok()?;
    if !matches!(view.get(b"ViewState"), Ok(Object::Name(state)) if state == b"ON") {
        return None;
    }
    Some(if let Some(view_id) = view_id {
        PdfiumViewStateOnLocation::IndirectView(view_id)
    } else if let Some(usage_id) = usage_id {
        PdfiumViewStateOnLocation::DirectViewInUsage(usage_id)
    } else {
        PdfiumViewStateOnLocation::DirectViewInGroup(group_id)
    })
}

/// Produces bytes for PDFium's display-only document without changing the
/// source object graph or persistence bytes. PDF.js treats `/ViewState /ON` as
/// neutral and leaves the default configuration authoritative; the pinned
/// PDFium build instead enables a base-disabled group in page and Form content.
/// Only that valid name is removed. `/OFF` and malformed values are preserved.
pub fn pdfium_display_render_bytes(
    document: &Document,
    source_bytes: Vec<u8>,
) -> Result<Vec<u8>, PdfPersistenceError> {
    let Ok(root) = document
        .trailer
        .get(b"Root")
        .and_then(|value| resolve_optional_object(document, value))
        .and_then(Object::as_dict)
    else {
        return Ok(source_bytes);
    };
    let Ok(properties) = root
        .get(b"OCProperties")
        .and_then(|value| resolve_optional_object(document, value))
        .and_then(Object::as_dict)
    else {
        return Ok(source_bytes);
    };
    let Ok(groups) = properties
        .get(b"OCGs")
        .and_then(|value| resolve_optional_object(document, value))
        .and_then(Object::as_array)
    else {
        return Ok(source_bytes);
    };
    if groups.len() > MAX_PDFIUM_DISPLAY_OPTIONAL_CONTENT_GROUPS {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "optional-content group count exceeds render preparation limit of {MAX_PDFIUM_DISPLAY_OPTIONAL_CONTENT_GROUPS}"
        )));
    }
    let locations = groups
        .iter()
        .filter_map(|group| group.as_reference().ok())
        .filter_map(|group_id| pdfium_view_state_on_location(document, group_id))
        .collect::<HashSet<_>>();
    if locations.is_empty() {
        return Ok(source_bytes);
    }

    let mut display = document.clone();
    for location in locations {
        let view = match location {
            PdfiumViewStateOnLocation::IndirectView(view_id) => {
                display.get_object_mut(view_id)?.as_dict_mut()?
            }
            PdfiumViewStateOnLocation::DirectViewInUsage(usage_id) => display
                .get_object_mut(usage_id)?
                .as_dict_mut()?
                .get_mut(b"View")?
                .as_dict_mut()?,
            PdfiumViewStateOnLocation::DirectViewInGroup(group_id) => display
                .get_object_mut(group_id)?
                .as_dict_mut()?
                .get_mut(b"Usage")?
                .as_dict_mut()?
                .get_mut(b"View")?
                .as_dict_mut()?,
        };
        view.remove(b"ViewState");
    }
    let mut bytes = Vec::new();
    display.save_to(&mut bytes)?;
    Ok(bytes)
}

fn optional_content_group_default_visibility(document: &Document) -> HashMap<ObjectId, bool> {
    let Ok(root) = document
        .trailer
        .get(b"Root")
        .and_then(|value| resolve_object(document, value))
        .and_then(Object::as_dict)
    else {
        return HashMap::new();
    };
    let Ok(properties) = root
        .get(b"OCProperties")
        .and_then(|value| resolve_object(document, value))
        .and_then(Object::as_dict)
    else {
        return HashMap::new();
    };
    let Ok(groups) = properties
        .get(b"OCGs")
        .and_then(|value| resolve_object(document, value))
        .and_then(Object::as_array)
    else {
        return HashMap::new();
    };
    let group_ids = groups
        .iter()
        .filter_map(|group| group.as_reference().ok())
        .collect::<HashSet<_>>();
    let mut visibility = group_ids
        .iter()
        .copied()
        .map(|group| (group, false))
        .collect::<HashMap<_, _>>();
    let Ok(configuration) = properties
        .get(b"D")
        .and_then(|value| resolve_object(document, value))
        .and_then(Object::as_dict)
    else {
        return visibility;
    };

    let base_visible = match configuration.get(b"BaseState") {
        Err(_) => true,
        Ok(Object::Name(state)) if state == b"ON" || state == b"Unchanged" => true,
        Ok(Object::Name(state)) if state == b"OFF" => false,
        _ => return visibility,
    };
    visibility
        .values_mut()
        .for_each(|value| *value = base_visible);
    for (key, value) in [(b"ON".as_slice(), true), (b"OFF".as_slice(), false)] {
        let Ok(overrides) = configuration.get(key) else {
            continue;
        };
        let Ok(overrides) = resolve_object(document, overrides).and_then(Object::as_array) else {
            return group_ids.into_iter().map(|group| (group, false)).collect();
        };
        for group in overrides {
            let Ok(group) = group.as_reference() else {
                return group_ids.into_iter().map(|group| (group, false)).collect();
            };
            let Some(visible) = visibility.get_mut(&group) else {
                return group_ids.into_iter().map(|group| (group, false)).collect();
            };
            *visible = value;
        }
    }
    visibility
}

fn optional_content_group_visibility(
    document: &Document,
    group: &Object,
    default_visibility: &HashMap<ObjectId, bool>,
) -> Option<bool> {
    let group_id = group.as_reference().ok()?;
    let dictionary = document.get_object(group_id).ok()?.as_dict().ok()?;
    if dictionary_name(dictionary, b"Type").as_deref() != Some("OCG") {
        return None;
    }
    let configured_visible = default_visibility.get(&group_id).copied()?;
    let Ok(usage) = dictionary.get(b"Usage") else {
        return Some(configured_visible);
    };
    let usage = resolve_object(document, usage).ok()?.as_dict().ok()?;
    let Ok(view) = usage.get(b"View") else {
        return Some(configured_visible);
    };
    let view = resolve_object(document, view).ok()?.as_dict().ok()?;
    match view.get(b"ViewState") {
        Err(_) => Some(configured_visible),
        Ok(Object::Name(state)) if state == b"ON" => Some(configured_visible),
        Ok(Object::Name(state)) if state == b"OFF" => Some(false),
        _ => None,
    }
}

fn optional_content_visibility_expression(
    document: &Document,
    expression: &Object,
    default_visibility: &HashMap<ObjectId, bool>,
) -> Option<bool> {
    const MAX_DEPTH: usize = 10;
    const MAX_NODES: usize = 4_096;
    const MAX_OPERANDS: usize = 256;

    fn evaluate(
        document: &Document,
        expression: &Object,
        default_visibility: &HashMap<ObjectId, bool>,
        depth: usize,
        nodes: &mut usize,
        active_expression_ids: &mut HashSet<ObjectId>,
    ) -> Option<bool> {
        if depth > MAX_DEPTH || *nodes >= MAX_NODES {
            return None;
        }
        *nodes += 1;
        let expression_id = expression.as_reference().ok();
        if expression_id.is_some_and(|id| !active_expression_ids.insert(id)) {
            return None;
        }
        let result = (|| {
            let expression = resolve_object(document, expression).ok()?.as_array().ok()?;
            let (operator, operands) = expression.split_first()?;
            let operator = resolve_object(document, operator).ok()?.as_name().ok()?;
            match operator {
                b"And" | b"Or" if (2..=MAX_OPERANDS).contains(&operands.len()) => {}
                b"Not" if operands.len() == 1 => {}
                _ => return None,
            }
            let states = operands
                .iter()
                .map(|operand| {
                    if *nodes >= MAX_NODES {
                        return None;
                    }
                    *nodes += 1;
                    if resolve_object(document, operand).ok()?.as_array().is_ok() {
                        evaluate(
                            document,
                            operand,
                            default_visibility,
                            depth + 1,
                            nodes,
                            active_expression_ids,
                        )
                    } else {
                        optional_content_group_visibility(document, operand, default_visibility)
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            match operator {
                b"And" => Some(states.into_iter().all(|visible| visible)),
                b"Or" => Some(states.into_iter().any(|visible| visible)),
                b"Not" => states.into_iter().next().map(|visible| !visible),
                _ => None,
            }
        })();
        if let Some(expression_id) = expression_id {
            active_expression_ids.remove(&expression_id);
        }
        result
    }

    let mut nodes = 0;
    let mut active_expression_ids = HashSet::new();
    evaluate(
        document,
        expression,
        default_visibility,
        1,
        &mut nodes,
        &mut active_expression_ids,
    )
}

fn optional_content_membership_is_visible(
    document: &Document,
    membership: &Dictionary,
    default_visibility: &HashMap<ObjectId, bool>,
) -> bool {
    // ISO 32000 makes /VE authoritative over /P. Match PDF.js for valid
    // And/Or/Not expressions, but fail closed instead of exposing content when
    // the expression is malformed, unknown or exceeds the explicit bounds.
    if let Ok(expression) = membership.get(b"VE") {
        return optional_content_visibility_expression(document, expression, default_visibility)
            .unwrap_or(false);
    }
    let Ok(groups) = membership.get(b"OCGs") else {
        return false;
    };
    let groups = match resolve_object(document, groups) {
        Ok(Object::Array(groups)) => groups.as_slice(),
        Ok(Object::Dictionary(_)) if groups.as_reference().is_ok() => std::slice::from_ref(groups),
        _ => return false,
    };
    if groups.is_empty() {
        return false;
    }
    let Some(states) = groups
        .iter()
        .map(|group| optional_content_group_visibility(document, group, default_visibility))
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };
    match dictionary_name(membership, b"P").as_deref() {
        None | Some("AnyOn") => states.into_iter().any(|visible| visible),
        Some("AllOn") => states.into_iter().all(|visible| visible),
        Some("AnyOff") => states.into_iter().any(|visible| !visible),
        Some("AllOff") => states.into_iter().all(|visible| !visible),
        _ => false,
    }
}

fn annotation_is_hidden_by_default_optional_content(
    document: &Document,
    annotation: &Object,
    default_visibility: &HashMap<ObjectId, bool>,
) -> Result<bool, PdfPersistenceError> {
    let dictionary = resolve_object(document, annotation)?.as_dict()?;
    let Ok(optional_content) = dictionary.get(b"OC") else {
        return Ok(false);
    };
    let Ok(optional_content_dictionary) =
        resolve_object(document, optional_content).and_then(Object::as_dict)
    else {
        return Ok(true);
    };
    let visible = match dictionary_name(optional_content_dictionary, b"Type").as_deref() {
        Some("OCG") => {
            optional_content_group_visibility(document, optional_content, default_visibility)
                .unwrap_or(false)
        }
        Some("OCMD") => optional_content_membership_is_visible(
            document,
            optional_content_dictionary,
            default_visibility,
        ),
        _ => false,
    };
    Ok(!visible)
}

/// Prepares the retained PDF annotation channel using the actual import admission
/// result. Exact page/array slots, not names or subtype guesses, define ownership.
/// The bytes are for rendering only and must never replace the persistence source.
pub fn retained_annotation_render(
    document: &Document,
) -> Result<RetainedAnnotationRender, PdfPersistenceError> {
    let calibrations = import_page_scales(document)
        .iter()
        .filter_map(|scale| {
            LengthCalibration::from_page_scale(scale)
                .ok()
                .map(|value| (scale.page_index, value))
        })
        .collect();
    let imported = import_annotations(document, &calibrations)?;
    if imported.untouched.is_empty() {
        return Ok(RetainedAnnotationRender::None);
    }
    let mut widgets = BTreeMap::new();
    let mut hidden_slots: BTreeMap<ObjectId, BTreeSet<usize>> = BTreeMap::new();
    let default_optional_content_visibility = optional_content_group_default_visibility(document);
    for (_, page_id) in document.get_pages() {
        let page = document.get_object(page_id)?.as_dict()?;
        let Ok(annotations) = page.get(b"Annots") else {
            continue;
        };
        for (index, annotation) in resolve_object(document, annotations)?
            .as_array()?
            .iter()
            .enumerate()
        {
            if imported
                .managed_annotation_slots
                .get(&page_id)
                .is_some_and(|slots| slots.contains(&index))
            {
                continue;
            }
            if annotation_is_hidden_by_default_optional_content(
                document,
                annotation,
                &default_optional_content_visibility,
            )? {
                hidden_slots.entry(page_id).or_default().insert(index);
                continue;
            }
            let annotation = resolve_object(document, annotation)?.as_dict()?;
            if let Some(surrogate) = retained_widget_surrogate(document, annotation) {
                widgets.insert((page_id, index), surrogate);
            }
        }
    }
    if imported.managed_annotation_slots.is_empty() && widgets.is_empty() && hidden_slots.is_empty()
    {
        return Ok(RetainedAnnotationRender::Original);
    }
    let mut filtered = document.clone();
    for (_, page_id) in document.get_pages() {
        let slots = imported.managed_annotation_slots.get(&page_id);
        let hidden = hidden_slots.get(&page_id);
        if slots.is_none() && hidden.is_none() && !widgets.keys().any(|(page, _)| *page == page_id)
        {
            continue;
        }
        let page = document.get_object(page_id)?.as_dict()?;
        let annotations = resolve_object(document, page.get(b"Annots")?)?.as_array()?;
        let mut retained = Vec::new();
        for (index, annotation) in annotations.iter().enumerate() {
            if slots.is_some_and(|slots| slots.contains(&index)) {
                continue;
            }
            if hidden.is_some_and(|slots| slots.contains(&index)) {
                continue;
            }
            if let Some(mut surrogate) = widgets.remove(&(page_id, index)) {
                let normal = surrogate.get(b"AP")?.as_dict()?.get(b"N")?.clone();
                if matches!(normal, Object::Stream(_)) {
                    let id = filtered.add_object(normal);
                    surrogate.set("AP", dictionary! { "N" => id });
                }
                retained.push(Object::Reference(filtered.add_object(surrogate)));
            } else {
                retained.push(annotation.clone());
            }
        }
        // Replace the page slot array, never a possibly shared indirect array.
        filtered
            .get_object_mut(page_id)?
            .as_dict_mut()?
            .set("Annots", Object::Array(retained));
    }
    let mut bytes = Vec::new();
    filtered.save_to(&mut bytes)?;
    Ok(RetainedAnnotationRender::Filtered(bytes))
}

/// PDFium's annotation pass excludes Widgets. Reuse a resolved existing normal
/// appearance as a static render-only Stamp. When no appearance exists, mirror
/// Electron's noninteractive imported-annotation marker instead of inventing a
/// form value or mutating the AcroForm. Popup annotations remain omitted.
fn retained_widget_surrogate(document: &Document, annotation: &Dictionary) -> Option<Dictionary> {
    if dictionary_name(annotation, b"Subtype").as_deref() != Some("Widget") {
        return None;
    }
    let rect = annotation.get(b"Rect").ok()?.clone();
    let selected = if let Ok(ap) = annotation.get(b"AP") {
        let ap = resolve_object(document, ap).ok()?.as_dict().ok()?;
        let normal = ap.get(b"N").ok()?;
        let resolved = resolve_object(document, normal).ok()?;
        if resolved.as_stream().is_ok() {
            normal.clone()
        } else {
            let states = resolved.as_dict().ok()?;
            let bytes = |dictionary: &Dictionary, key: &[u8]| -> Option<Vec<u8>> {
                let object = resolve_object(document, dictionary.get(key).ok()?).ok()?;
                object
                    .as_name()
                    .or_else(|_| object.as_str())
                    .ok()
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_vec())
            };
            // Match PDFium: a nonempty AS is authoritative, even if unavailable.
            // Only missing/empty AS consults V, immediate Parent/V, then Off.
            let state = if let Some(state) = bytes(annotation, b"AS") {
                state
            } else {
                let value = bytes(annotation, b"V").or_else(|| {
                    let parent = resolve_object(document, annotation.get(b"Parent").ok()?)
                        .ok()?
                        .as_dict()
                        .ok()?;
                    bytes(parent, b"V")
                });
                value
                    .filter(|value| states.get(value).is_ok())
                    .unwrap_or_else(|| b"Off".to_vec())
            };
            states.get(&state).ok()?.clone()
        }
    } else {
        Object::Stream(retained_widget_fallback_appearance(
            best_effort_annotation_rect(document, annotation)?,
        )?)
    };
    resolve_object(document, &selected).ok()?.as_stream().ok()?;
    let mut surrogate = dictionary! { "Type" => "Annot", "Subtype" => "Stamp", "Rect" => rect,
    "AP" => dictionary! { "N" => selected } };
    for key in [b"F".as_slice(), b"CA".as_slice(), b"OC".as_slice()] {
        if let Ok(value) = annotation.get(key) {
            surrogate.set(key.to_vec(), value.clone());
        }
    }
    Some(surrogate)
}

fn retained_widget_fallback_appearance(rect: PdfRect) -> Option<Stream> {
    if !(rect.width.is_finite() && rect.height.is_finite() && rect.width > 0. && rect.height > 0.) {
        return None;
    }
    let inset = 0.625_f64.min(rect.width / 2.).min(rect.height / 2.);
    let inner_width = (rect.width - inset * 2.).max(0.);
    let inner_height = (rect.height - inset * 2.).max(0.);
    let font_size = 10_f64.min((rect.height - 2.).max(1.));
    let baseline = ((rect.height - font_size) / 2. + font_size * 0.2).max(1.);
    let content = format!(
        "q\n/FillAlpha gs\n0.937255 0.266667 0.266667 rg\n0 0 {width:.6} {height:.6} re f\nQ\nq\n0.937255 0.266667 0.266667 RG\n1.25 w\n[6 3] 0 d\n{inset:.6} {inset:.6} {inner_width:.6} {inner_height:.6} re S\nQ\nq\n0 0 {width:.6} {height:.6} re W n\nBT\n/Helv {font_size:.6} Tf\n0.600000 0.105882 0.105882 rg\n1 0 0 1 2 {baseline:.6} Tm\n(Widget) Tj\nET\nQ\n",
        width = rect.width,
        height = rect.height,
    );
    Some(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => Object::Array(vec![0.into(), 0.into(), Object::Real(rect.width as f32), Object::Real(rect.height as f32)]),
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "Helv" => dictionary! {
                        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
                    },
                },
                "ExtGState" => dictionary! {
                    "FillAlpha" => dictionary! { "Type" => "ExtGState", "ca" => Object::Real(0.04) },
                },
            },
        },
        content.into_bytes(),
    ))
}

struct ImportedAnnotations {
    managed_annotation_slots: BTreeMap<ObjectId, HashSet<usize>>,
    rectangles: Vec<RectangleAnnotation>,
    redacts: Vec<RedactAnnotation>,
    redact_native_identities: HashMap<MarkupId, RedactNativeIdentity>,
    ellipses: Vec<EllipseAnnotation>,
    ellipse_native_identities: HashMap<MarkupId, EllipseNativeIdentity>,
    arcs: Vec<ArcAnnotation>,
    arc_native_identities: HashMap<MarkupId, ArcNativeIdentity>,
    pens: Vec<PenAnnotation>,
    pen_native_identities: HashMap<MarkupId, PenNativeIdentity>,
    text_boxes: Vec<TextBoxAnnotation>,
    lengths: Vec<LengthAnnotation>,
    length_native_identities: HashMap<MarkupId, LengthNativeIdentity>,
    dimensions: Vec<DimensionAnnotation>,
    dimension_native_names: HashMap<MarkupId, String>,
    straight_lines: Vec<StraightLineAnnotation>,
    straight_line_native_identities: HashMap<MarkupId, StraightLineNativeIdentity>,
    vertex_paths: Vec<VertexPathAnnotation>,
    vertex_path_native_identities: HashMap<MarkupId, VertexPathNativeIdentity>,
    clouds: Vec<CloudAnnotation>,
    cloud_native_identities: HashMap<MarkupId, CloudNativeIdentity>,
    cloud_pluses: Vec<CloudPlusAnnotation>,
    cloud_plus_native_identities: HashMap<MarkupId, CloudPlusNativeIdentity>,
    callouts: Vec<CalloutAnnotation>,
    callout_native_identities: HashMap<MarkupId, CalloutNativeIdentity>,
    measurement_paths: Vec<MeasurementPathAnnotation>,
    measurement_path_native_identities: HashMap<MarkupId, MeasurementPathNativeIdentity>,
    images: Vec<ImageAnnotation>,
    image_native_names: HashMap<MarkupId, String>,
    snapshots: Vec<SnapshotAnnotation>,
    snapshot_native_names: HashMap<MarkupId, String>,
    vector_snapshot_sources: Vec<(MarkupId, VectorSnapshotSource)>,
    annotation_order: Vec<MarkupId>,
    untouched: Vec<UntouchedAnnotation>,
    retained_annotation_obstacles: Vec<RetainedAnnotationObstacle>,
}

fn scale_unit_from_revu(unit: &str) -> Option<ScaleUnit> {
    ScaleUnit::parse(unit.trim()).ok()
}

/// Parses Revu's scale ratio, `1 cm = 1 m`.
fn parse_revu_scale_ratio(ratio: &str) -> Option<(f64, ScaleUnit, f64, ScaleUnit)> {
    let (paper, real) = ratio.split_once('=')?;
    let mut paper = paper.split_whitespace();
    let mut real = real.split_whitespace();
    let paper_value = paper.next()?.parse::<f64>().ok()?;
    let paper_unit = scale_unit_from_revu(paper.next()?)?;
    let real_value = real.next()?.parse::<f64>().ok()?;
    let real_unit = scale_unit_from_revu(real.next()?)?;
    (paper_value > 0. && real_value > 0.).then_some((paper_value, paper_unit, real_value, real_unit))
}

fn first_number_format(measure: &Dictionary, key: &[u8]) -> Option<Dictionary> {
    measure
        .get(key)
        .ok()?
        .as_array()
        .ok()?
        .first()?
        .as_dict()
        .ok()
        .cloned()
}

fn number_format_precision(format: &Dictionary) -> Option<ScalePrecision> {
    let denominator = format.get(b"D").ok()?.as_i64().ok().filter(|value| *value > 0)?;
    if dictionary_name(format, b"F").as_deref() == Some("F") {
        ScalePrecision::fraction(u16::try_from(denominator).ok()?).ok()
    } else {
        ScalePrecision::decimal(1. / denominator as f64).ok()
    }
}

fn import_viewport_scale(document: &Document, page_index: u32, page: &Dictionary) -> Option<PageScale> {
    let viewports = resolve_optional_object(document, page.get(b"VP").ok()?)
        .ok()?
        .as_array()
        .ok()?;
    viewports.iter().find_map(|viewport| {
        let viewport = resolve_optional_object(document, viewport).ok()?.as_dict().ok()?;
        let measure = resolve_optional_object(document, viewport.get(b"Measure").ok()?)
            .ok()?
            .as_dict()
            .ok()?;
        if dictionary_name(measure, b"Subtype").as_deref() != Some("RL") {
            return None;
        }
        let x = first_number_format(measure, b"X")?;
        let real_units = scale_unit_from_revu(&dictionary_string(&x, b"U")?)?;
        let scale_x = dictionary_float(&x, b"C").filter(|value| *value > 0.)?;
        let scale_y = first_number_format(measure, b"Y")
            .and_then(|y| dictionary_float(&y, b"C"))
            .filter(|value| *value > 0.)
            .unwrap_or(scale_x);
        let ratio = dictionary_string(measure, b"R").unwrap_or_default();
        let pdf_units = parse_revu_scale_ratio(&ratio).map_or(
            if matches!(real_units, ScaleUnit::In | ScaleUnit::Ft) { ScaleUnit::In } else { ScaleUnit::Cm },
            |(_, paper_unit, _, _)| paper_unit,
        );
        let precision = first_number_format(measure, b"D")
            .and_then(|format| number_format_precision(&format))
            .or_else(|| number_format_precision(&x))
            .unwrap_or_else(|| ScalePrecision::decimal(0.01).expect("valid precision"));
        let preset = built_in_scale_presets().into_iter().find(|preset| {
            preset.pdf_units == pdf_units
                && preset.real_units == real_units
                && ((preset.scale_x - scale_x) / scale_x).abs() < 1e-6
        });
        let (source, name) = match preset {
            Some(preset) => (ScaleSource::Preset, preset.name),
            None => (ScaleSource::Custom, if ratio.is_empty() { "Custom".into() } else { ratio }),
        };
        PageScale::from_factors(
            page_index, source, name, pdf_units, real_units, scale_x, scale_y, precision,
        )
        .ok()
    })
}

fn import_page_scales(document: &Document) -> Vec<PageScale> {
    document
        .get_pages()
        .into_iter()
        .filter_map(|(page_number, page_id)| {
            let page = document.get_object(page_id).and_then(Object::as_dict).ok()?;
            import_viewport_scale(document, page_number.saturating_sub(1), page)
        })
        .collect()
}

/// Writes each page scale as Revu does: a page `/VP` viewport covering the
/// page with a rectilinear `/Measure` in the scale's real-world unit.
fn write_page_scales(
    document: &mut Document,
    scales: &[PageScale],
    original_scales: &[PageScale],
) -> Result<(), PdfPersistenceError> {
    let pages = document.get_pages();
    for (page_number, page_id) in pages {
        let page_index = page_number.saturating_sub(1);
        let media_box = document
            .get_object(page_id)?
            .as_dict()?
            .get(b"MediaBox")
            .ok()
            .cloned()
            .unwrap_or_else(|| vec![0.into(), 0.into(), 612.into(), 792.into()].into());
        let existing_name = document
            .get_object(page_id)?
            .as_dict()?
            .get(b"VP")
            .ok()
            .and_then(|value| resolve_optional_object(document, value).ok())
            .and_then(|value| value.as_array().ok())
            .and_then(|viewports| viewports.first())
            .and_then(|viewport| resolve_optional_object(document, viewport).ok())
            .and_then(|viewport| viewport.as_dict().ok())
            .and_then(|viewport| dictionary_string(viewport, b"NM"));
        let scale = scales.iter().find(|scale| scale.page_index == page_index);
        // An unchanged page keeps its original viewports, including any this
        // reader does not understand.
        if scale == original_scales.iter().find(|scale| scale.page_index == page_index) {
            continue;
        }
        let page = document.get_object_mut(page_id)?.as_dict_mut()?;
        let Some(scale) = scale else {
            page.remove(b"VP");
            continue;
        };
        let real_unit = scale.real_units.as_str();
        let ratio = revu_scale_ratio(
            1.,
            scale.pdf_units.as_str(),
            scale.scale_x * scale.pdf_units.points(),
            real_unit,
        );
        let mut measure = revu_measure_dictionary(&ratio, real_unit, scale.scale_x, scale.precision);
        if scale.scale_y != scale.scale_x {
            let (denominator, _) = revu_precision_denominator(scale.precision);
            measure.set(
                "Y",
                vec![revu_number_format(real_unit.as_bytes(), scale.scale_y, denominator, false)],
            );
        }
        page.set(
            "VP",
            vec![Object::Dictionary(dictionary! {
                "Type" => "Viewport",
                "BBox" => media_box,
                "Measure" => measure,
                "NM" => pdf_literal(&existing_name.unwrap_or_else(crate::annotation_model::generate_markup_name)),
            })],
        );
    }
    Ok(())
}

fn import_page_rotations(
    document: &Document,
) -> Result<BTreeMap<u32, PageRotation>, PdfPersistenceError> {
    document
        .get_pages()
        .into_iter()
        .map(|(page_number, page_id)| {
            let degrees = inherited_page_rotation(document, page_id)?;
            let rotation = PageRotation::from_degrees(degrees)?;
            Ok((page_number.saturating_sub(1), rotation))
        })
        .collect()
}

fn inherited_page_rotation(
    document: &Document,
    mut object_id: ObjectId,
) -> Result<i64, PdfPersistenceError> {
    for _ in 0..64 {
        let dictionary = document.get_object(object_id)?.as_dict()?;
        if let Ok(rotation) = dictionary.get(b"Rotate") {
            return Ok(rotation.as_i64()?);
        }
        object_id = match dictionary.get(b"Parent") {
            Ok(Object::Reference(parent)) => *parent,
            _ => return Ok(0),
        };
    }
    Err(PdfPersistenceError::InvalidDocument(
        "PDF page parent chain exceeded the rotation inheritance limit".into(),
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloudPlusRole {
    Cloud,
    Text,
}

#[derive(Clone, Debug)]
struct CloudPlusPairMember {
    annotation_index: usize,
    object_id: ObjectId,
    raw_name: String,
    stable_name: String,
    role: CloudPlusRole,
}

#[derive(Clone, Debug)]
struct CloudPlusPagePair {
    first_index: usize,
    cloud: CloudPlusPairMember,
    text: CloudPlusPairMember,
}

fn cloud_plus_role(annotation: &Dictionary) -> Option<CloudPlusRole> {
    if !is_cloud_plus_fragment(annotation) {
        return None;
    }
    match dictionary_name(annotation, b"Subtype").as_deref() {
        Some("Polygon") => Some(CloudPlusRole::Cloud),
        Some("FreeText") => Some(CloudPlusRole::Text),
        _ => None,
    }
}

fn cloud_plus_group_tokens(annotation: &Dictionary) -> Vec<String> {
    annotation
        .get(b"GroupNesting")
        .ok()
        .and_then(|value| value.as_array().ok())
        .into_iter()
        .flatten()
        .filter_map(|value| {
            value
                .as_str()
                .ok()
                .or_else(|| value.as_name().ok())
                .map(|bytes| {
                    String::from_utf8_lossy(bytes)
                        .trim_start_matches('/')
                        .to_owned()
                })
        })
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("Cloud+"))
        .collect()
}

fn cloud_plus_external_members(
    document: &Document,
    annotations: &[Object],
) -> Result<Vec<CloudPlusPairMember>, PdfPersistenceError> {
    let mut members = Vec::new();
    for (annotation_index, annotation_object) in annotations.iter().enumerate() {
        let Object::Reference(object_id) = annotation_object else {
            continue;
        };
        let annotation = document.get_object(*object_id)?.as_dict()?;
        let Some(role) = cloud_plus_role(annotation) else {
            continue;
        };
        let Some(raw_name) = dictionary_string(annotation, b"NM") else {
            continue;
        };
        members.push(CloudPlusPairMember {
            annotation_index,
            object_id: *object_id,
            stable_name: raw_name.clone(),
            raw_name,
            role,
        });
    }
    Ok(members)
}

fn exact_managed_cloud_plus_pairs(
    document: &Document,
    annotations: &[Object],
) -> Result<(HashMap<usize, CloudPlusPagePair>, HashSet<usize>), PdfPersistenceError> {
    let mut by_first_index = HashMap::new();
    let mut consumed_indices = HashSet::new();
    let external_members = cloud_plus_external_members(document, annotations)?;
    for text in external_members
        .iter()
        .filter(|member| member.role == CloudPlusRole::Text)
    {
        if consumed_indices.contains(&text.annotation_index) {
            continue;
        }
        let text_dictionary = document.get_object(text.object_id)?.as_dict()?;
        let group = cloud_plus_group_tokens(text_dictionary);
        if !group.iter().any(|name| name == &text.raw_name) {
            continue;
        }
        let matching_clouds = external_members
            .iter()
            .filter(|member| {
                member.role == CloudPlusRole::Cloud
                    && !consumed_indices.contains(&member.annotation_index)
                    && group.iter().any(|name| name == &member.raw_name)
            })
            .collect::<Vec<_>>();
        let matching_texts = external_members
            .iter()
            .filter(|member| {
                member.role == CloudPlusRole::Text
                    && !consumed_indices.contains(&member.annotation_index)
                    && group.iter().any(|name| name == &member.raw_name)
            })
            .collect::<Vec<_>>();
        let ([cloud], [matched_text]) = (matching_clouds.as_slice(), matching_texts.as_slice())
        else {
            continue;
        };
        if matched_text.annotation_index != text.annotation_index {
            continue;
        }
        let stable_name = cloud.raw_name.clone();
        let mut cloud = (*cloud).clone();
        let mut text = (*matched_text).clone();
        cloud.stable_name.clone_from(&stable_name);
        text.stable_name = stable_name;
        let first_index = cloud.annotation_index.min(text.annotation_index);
        consumed_indices.insert(cloud.annotation_index);
        consumed_indices.insert(text.annotation_index);
        by_first_index.insert(
            first_index,
            CloudPlusPagePair {
                first_index,
                cloud,
                text,
            },
        );
    }
    Ok((by_first_index, consumed_indices))
}

fn annotation_requires_opaque_import(annotation: &Dictionary) -> bool {
    annotation.get(b"OC").is_ok()
        || annotation.get(b"Popup").is_ok()
        || (dictionary_name(annotation, b"Subtype").as_deref() == Some("FreeText")
            && annotation_rich_text_requires_opaque_import(annotation))
}

fn annotation_rich_text_requires_opaque_import(annotation: &Dictionary) -> bool {
    let Ok(value) = annotation.get(b"RC") else {
        return false;
    };
    let Ok(rich_text) = decode_pdf_text_string_compat(value) else {
        return true;
    };
    if rich_text.len() > 256 * 1024 {
        return true;
    }
    let lower = rich_text.to_ascii_lowercase();
    if [
        "<b>", "<b ", "<strong", "<i>", "<i ", "<em", "<u>", "<u ", "<s>", "<s ", "<strike",
        "<font",
    ]
    .into_iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }
    if !lower.contains("<span") {
        return false;
    }
    let Some(spans) = parse_rich_text_spans(&rich_text) else {
        return true;
    };
    let Ok(Some(contents)) = dictionary_text_box_contents(annotation, b"Contents") else {
        return true;
    };
    spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>()
        != contents
}

#[derive(Clone, Debug, PartialEq)]
struct ParsedRichTextSpan {
    text: String,
    font_family: Option<String>,
    bold: bool,
    italic: bool,
    color: Option<String>,
    font_size_pt: Option<f64>,
}

fn decode_rich_text_entities(value: &str) -> Option<String> {
    let mut decoded = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(index) = rest.find('&') {
        decoded.push_str(&rest[..index]);
        rest = &rest[index + 1..];
        let end = rest.find(';')?;
        let entity = &rest[..end];
        let character = match entity {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '\"',
            "apos" => '\'',
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                char::from_u32(u32::from_str_radix(&entity[2..], 16).ok()?)?
            }
            _ if entity.starts_with('#') => char::from_u32(entity[1..].parse().ok()?)?,
            _ => return None,
        };
        if character == '\0' {
            return None;
        }
        decoded.push(character);
        rest = &rest[end + 1..];
    }
    decoded.push_str(rest);
    Some(decoded)
}

fn rich_text_style_attribute(open_tag: &str) -> Option<&str> {
    let lower = open_tag.to_ascii_lowercase();
    let start = lower.find("style")? + "style".len();
    let suffix = open_tag.get(start..)?.trim_start();
    let suffix = suffix.strip_prefix('=')?.trim_start();
    let quote = suffix.chars().next()?;
    if !matches!(quote, '\'' | '\"') {
        return None;
    }
    let suffix = &suffix[quote.len_utf8()..];
    let end = suffix.find(quote)?;
    Some(&suffix[..end])
}

fn parse_rich_text_span_inner(value: &str) -> Option<String> {
    let mut text = String::with_capacity(value.len());
    let mut rest = value;
    loop {
        let Some(index) = rest.find('<') else {
            text.push_str(rest);
            break;
        };
        text.push_str(&rest[..index]);
        rest = &rest[index..];
        let end = rest.find('>')?;
        let tag = rest[..=end]
            .chars()
            .filter(|character| !character.is_ascii_whitespace())
            .collect::<String>()
            .to_ascii_lowercase();
        if !matches!(tag.as_str(), "<br>" | "<br/>" | "<br></br>") {
            return None;
        }
        text.push('\n');
        rest = &rest[end + 1..];
    }
    decode_rich_text_entities(&text)
}

fn rich_text_body_attributes_are_supported(open_tag: &str) -> Option<bool> {
    let lower = open_tag.to_ascii_lowercase();
    if !lower.starts_with("<body") || !open_tag.ends_with('>') {
        return None;
    }
    let mut rest = open_tag.get("<body".len()..open_tag.len() - 1)?.trim();
    let mut seen = HashSet::new();
    let mut has_style = false;
    while !rest.is_empty() {
        let equals = rest.find('=')?;
        let name = rest[..equals].trim().to_ascii_lowercase();
        if name.is_empty() || name.chars().any(char::is_whitespace) || !seen.insert(name.clone()) {
            return None;
        }
        rest = rest[equals + 1..].trim_start();
        let quote = rest.chars().next()?;
        if !matches!(quote, '\'' | '"') {
            return None;
        }
        rest = &rest[quote.len_utf8()..];
        let end = rest.find(quote)?;
        let value = &rest[..end];
        rest = rest[end + quote.len_utf8()..].trim_start();
        match name.as_str() {
            "xmlns" if value == "http://www.w3.org/1999/xhtml" => {}
            "xmlns:xfa" if value == "http://www.xfa.org/schema/xfa-data/1.0/" => {}
            "xfa:contenttype" if value.eq_ignore_ascii_case("text/html") => {}
            "xfa:apiversion" if !value.is_empty() && value.len() <= 64 => {}
            "xfa:spec"
                if !value.is_empty()
                    && value.len() <= 32
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || byte == b'.') => {}
            "style" if rich_text_body_style_is_supported(value) => has_style = true,
            _ => return None,
        }
    }
    Some(has_style)
}

fn rich_text_body_style_is_supported(style: &str) -> bool {
    style
        .split(';')
        .filter(|declaration| !declaration.trim().is_empty())
        .all(|declaration| {
            let Some((property, value)) = declaration.split_once(':') else {
                return false;
            };
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim();
            match property.as_str() {
                "font" => {
                    let Some((family, size)) = value.rsplit_once(char::is_whitespace) else {
                        return false;
                    };
                    canonical_annotation_font_family(family).is_some()
                        && size
                            .strip_suffix("pt")
                            .and_then(|size| size.parse::<f64>().ok())
                            .is_some_and(|size| size.is_finite() && size > 0.)
                }
                "text-align" => matches!(
                    value.to_ascii_lowercase().as_str(),
                    "left" | "center" | "right"
                ),
                "margin" | "line-height" => value
                    .strip_suffix("pt")
                    .and_then(|value| value.parse::<f64>().ok())
                    .is_some_and(|value| value.is_finite() && value >= 0.),
                "color" => {
                    value.len() == 7
                        && value.starts_with('#')
                        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
                }
                _ => false,
            }
        })
}

fn rich_text_wrapper_markup_is_supported(value: &str) -> bool {
    let mut rest = value;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            return true;
        }
        if !trimmed.starts_with('<') {
            return false;
        }
        let Some(end) = trimmed.find('>') else {
            return false;
        };
        let tag = trimmed[..=end].trim().to_ascii_lowercase();
        let supported = matches!(tag.as_str(), "<body>" | "</body>" | "<p>" | "</p>")
            || tag.starts_with("<?xml ") && tag.ends_with("?>")
            || rich_text_body_attributes_are_supported(trimmed.get(..=end).unwrap()).is_some();
        if !supported {
            return false;
        }
        rest = &trimmed[end + 1..];
    }
}

fn parse_rich_text_spans(value: &str) -> Option<Vec<ParsedRichTextSpan>> {
    let lower = value.to_ascii_lowercase();
    let styled_body = if let Some(body_start) = lower.find("<body") {
        let body_end = value[body_start..].find('>')? + body_start;
        rich_text_body_attributes_are_supported(&value[body_start..=body_end])?
    } else {
        false
    };
    let mut cursor = 0usize;
    let mut spans = Vec::new();
    while let Some(relative_start) = lower[cursor..].find("<span") {
        let start = cursor + relative_start;
        if !rich_text_wrapper_markup_is_supported(&value[cursor..start]) {
            return None;
        }
        let open_end = value[start..].find('>')? + start;
        let close_start = lower[open_end + 1..].find("</span>")? + open_end + 1;
        let style = rich_text_style_attribute(&value[start..=open_end])?;
        let mut parsed = ParsedRichTextSpan {
            text: parse_rich_text_span_inner(&value[open_end + 1..close_start])?,
            font_family: None,
            bold: false,
            italic: false,
            color: None,
            font_size_pt: None,
        };
        if parsed.text.is_empty() {
            return None;
        }
        for declaration in style.split(';').filter(|value| !value.trim().is_empty()) {
            let (property, value) = declaration.split_once(':')?;
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim();
            match property.as_str() {
                "font-family" => {
                    parsed.font_family =
                        canonical_annotation_font_family(value.split(',').next()?.trim());
                    parsed.font_family.as_ref()?;
                }
                "font-size" => {
                    let size = value.strip_suffix("pt")?.trim().parse::<f64>().ok()?;
                    if !size.is_finite() || size <= 0. {
                        return None;
                    }
                    parsed.font_size_pt = Some(size);
                }
                "color" => {
                    if value.len() != 7
                        || !value.starts_with('#')
                        || !value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
                    {
                        return None;
                    }
                    parsed.color = Some(value.to_ascii_lowercase());
                }
                "font-weight" => match value.to_ascii_lowercase().as_str() {
                    "bold" | "700" => parsed.bold = true,
                    "normal" | "400" => parsed.bold = false,
                    _ => return None,
                },
                "font-style" => match value.to_ascii_lowercase().as_str() {
                    "italic" | "oblique" => parsed.italic = true,
                    "normal" => parsed.italic = false,
                    _ => return None,
                },
                _ => return None,
            }
        }
        spans.push(parsed);
        cursor = close_start + "</span>".len();
    }
    (!spans.is_empty()
        && rich_text_wrapper_markup_is_supported(&value[cursor..])
        && (!styled_body
            || spans.iter().all(|span| {
                span.font_family.is_some() && span.font_size_pt.is_some() && span.color.is_some()
            })))
    .then_some(spans)
}

fn write_page_rotations(
    document: &mut Document,
    rotations: &BTreeMap<u32, PageRotation>,
    changed_pages: &std::collections::BTreeSet<u32>,
) -> Result<(), PdfPersistenceError> {
    for (page_number, page_id) in document.get_pages() {
        let page_index = page_number.saturating_sub(1);
        if !changed_pages.contains(&page_index) {
            continue;
        }
        let page = document.get_object_mut(page_id)?.as_dict_mut()?;
        let rotation = rotations
            .get(&page_index)
            .copied()
            .unwrap_or(PageRotation::Degrees0);
        page.set("Rotate", Object::Integer(rotation.degrees()));
    }
    Ok(())
}

/// An annotation with its indirect values inlined, as Revu often writes
/// `/BS`, `/Measure` and similar entries as separate objects. Links to other
/// objects keep their references.
fn resolved_annotation_view(document: &Document, annotation: &Dictionary) -> Dictionary {
    fn inline(document: &Document, object: &Object, depth: usize) -> Object {
        if depth > 4 {
            return object.clone();
        }
        match object {
            Object::Reference(id) => match document.get_object(*id) {
                Ok(resolved @ (Object::Dictionary(_) | Object::Array(_))) => {
                    inline(document, resolved, depth + 1)
                }
                Ok(Object::Stream(_)) | Err(_) => object.clone(),
                Ok(resolved) => resolved.clone(),
            },
            Object::Dictionary(dictionary) => Object::Dictionary(
                dictionary
                    .iter()
                    .map(|(key, value)| (key.clone(), inline(document, value, depth + 1)))
                    .collect(),
            ),
            Object::Array(values) => Object::Array(
                values
                    .iter()
                    .map(|value| inline(document, value, depth + 1))
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    const LINKS: [&[u8]; 9] = [
        b"AP", b"P", b"IRT", b"Popup", b"Parent", b"OC", b"Image", b"RT", b"DR",
    ];
    annotation
        .iter()
        .map(|(key, value)| {
            let value = if LINKS.contains(&key.as_slice()) {
                value.clone()
            } else {
                inline(document, value, 0)
            };
            (key.clone(), value)
        })
        .collect()
}

fn import_annotations(
    document: &Document,
    page_length_calibrations: &BTreeMap<u32, LengthCalibration>,
) -> Result<ImportedAnnotations, PdfPersistenceError> {
    let mut imported = ImportedAnnotations {
        managed_annotation_slots: BTreeMap::new(),
        rectangles: Vec::new(),
        redacts: Vec::new(),
        redact_native_identities: HashMap::new(),
        ellipses: Vec::new(),
        ellipse_native_identities: HashMap::new(),
        arcs: Vec::new(),
        arc_native_identities: HashMap::new(),
        pens: Vec::new(),
        pen_native_identities: HashMap::new(),
        text_boxes: Vec::new(),
        lengths: Vec::new(),
        length_native_identities: HashMap::new(),
        dimensions: Vec::new(),
        dimension_native_names: HashMap::new(),
        straight_lines: Vec::new(),
        straight_line_native_identities: HashMap::new(),
        vertex_paths: Vec::new(),
        vertex_path_native_identities: HashMap::new(),
        clouds: Vec::new(),
        cloud_native_identities: HashMap::new(),
        cloud_pluses: Vec::new(),
        cloud_plus_native_identities: HashMap::new(),
        callouts: Vec::new(),
        callout_native_identities: HashMap::new(),
        measurement_paths: Vec::new(),
        measurement_path_native_identities: HashMap::new(),
        images: Vec::new(),
        image_native_names: HashMap::new(),
        snapshots: Vec::new(),
        snapshot_native_names: HashMap::new(),
        vector_snapshot_sources: Vec::new(),
        annotation_order: Vec::new(),
        untouched: Vec::new(),
        retained_annotation_obstacles: Vec::new(),
    };
    for (page_number, page_id) in document.get_pages() {
        let page = document.get_object(page_id)?.as_dict()?;
        let Ok(annotation_object) = page.get(b"Annots") else {
            continue;
        };
        let annotations = resolve_object(document, annotation_object)?.as_array()?;
        let (cloud_plus_pairs, consumed_cloud_plus_indices) =
            exact_managed_cloud_plus_pairs(document, annotations)?;
        for (annotation_index, annotation_object) in annotations.iter().enumerate() {
            if let Some(pair) = cloud_plus_pairs.get(&annotation_index) {
                debug_assert_eq!(pair.first_index, annotation_index);
                let cloud_view =
                    resolved_annotation_view(document, document.get_object(pair.cloud.object_id)?.as_dict()?);
                let text_view =
                    resolved_annotation_view(document, document.get_object(pair.text.object_id)?.as_dict()?);
                let (cloud_dictionary, text_dictionary) = (&cloud_view, &text_view);
                if annotation_requires_opaque_import(cloud_dictionary)
                    || annotation_requires_opaque_import(text_dictionary)
                {
                    imported.untouched.push(UntouchedAnnotation {
                        name: pair.cloud.raw_name.clone(),
                        subtype: "Polygon".into(),
                    });
                    imported.untouched.push(UntouchedAnnotation {
                        name: pair.text.raw_name.clone(),
                        subtype: "FreeText".into(),
                    });
                    continue;
                }
                match import_cloud_plus_pair(
                    document,
                    cloud_dictionary,
                    text_dictionary,
                    pair.cloud.stable_name.clone(),
                    page_number.saturating_sub(1),
                ) {
                    Ok(cloud_plus)
                        if !imported
                            .cloud_plus_native_identities
                            .contains_key(&cloud_plus.id) =>
                    {
                        imported.cloud_plus_native_identities.insert(
                            cloud_plus.id.clone(),
                            CloudPlusNativeIdentity {
                                cloud_raw_name: pair.cloud.raw_name.clone(),
                                cloud_object_id: pair.cloud.object_id,
                                text_raw_name: pair.text.raw_name.clone(),
                                text_object_id: pair.text.object_id,
                            },
                        );
                        imported.annotation_order.push(cloud_plus.id.clone());
                        imported.cloud_pluses.push(cloud_plus);
                        imported
                            .managed_annotation_slots
                            .entry(page_id)
                            .or_default()
                            .extend([pair.cloud.annotation_index, pair.text.annotation_index]);
                    }
                    Ok(_) | Err(_) => {
                        imported.untouched.push(UntouchedAnnotation {
                            name: pair.cloud.raw_name.clone(),
                            subtype: "Polygon".into(),
                        });
                        imported.untouched.push(UntouchedAnnotation {
                            name: pair.text.raw_name.clone(),
                            subtype: "FreeText".into(),
                        });
                    }
                }
                continue;
            }
            if consumed_cloud_plus_indices.contains(&annotation_index) {
                continue;
            }
            let annotation_view =
                resolved_annotation_view(document, resolve_object(document, annotation_object)?.as_dict()?);
            let annotation = &annotation_view;
            let subtype = dictionary_name(annotation, b"Subtype").unwrap_or_default();
            let physical_name = dictionary_string(annotation, b"NM");
            let name = physical_name.clone().unwrap_or_else(|| {
                format!("page-{}-annotation-{}", page_number - 1, annotation_index)
            });
            let page_index = page_number.saturating_sub(1);
            if annotation_requires_opaque_import(annotation) {
                imported
                    .untouched
                    .push(UntouchedAnnotation { name, subtype });
                continue;
            }
            let admitted_before = imported.annotation_order.len();
            match subtype.as_str() {
                "Square"
                    if dictionary_name(annotation, b"IT").as_deref() == Some("SquareImage") =>
                {
                    let stable_name = name.clone();
                    let image = match import_image(document, annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.image_native_names.insert(image.id.clone(), name);
                    imported.annotation_order.push(image.id.clone());
                    imported.images.push(image);
                }
                "Stamp"
                    if dictionary_name(annotation, b"IT").as_deref() == Some("StampSnapshot") =>
                {
                    let stable_name = name.clone();
                    let Object::Reference(_) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let imported_snapshot =
                        match import_snapshot(document, annotation, stable_name.clone(), page_index) {
                            Ok(snapshot) => Ok((snapshot, None)),
                            Err(_) => import_vector_snapshot(document, annotation, stable_name, page_index)
                                .map(|(snapshot, source)| (snapshot, Some(source))),
                        };
                    match imported_snapshot {
                        Ok((snapshot, source))
                            if !imported.snapshot_native_names.contains_key(&snapshot.id) =>
                        {
                            imported
                                .snapshot_native_names
                                .insert(snapshot.id.clone(), name);
                            if let Some(source) = source {
                                imported.vector_snapshot_sources.push((snapshot.id.clone(), source));
                            }
                            imported.annotation_order.push(snapshot.id.clone());
                            imported.snapshots.push(snapshot);
                        }
                        Ok(_) | Err(_) => imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype }),
                    }
                }
                "Square" => {
                    let rectangle = match import_rectangle(document, annotation, name.clone(), page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.annotation_order.push(rectangle.id.clone());
                    imported.rectangles.push(rectangle);
                }
                "Redact" => {
                    let stable_name = name.clone();
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.redact_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous pending Redact identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let redact = match import_redact(annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.redact_native_identities.insert(
                        redact.id.clone(),
                        RedactNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(redact.id.clone());
                    imported.redacts.push(redact);
                }
                "Circle" if dictionary_name(annotation, b"IT").as_deref() == Some("CircleArc") => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.arc_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous Arc identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let arc = match import_arc(document, annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.arc_native_identities.insert(
                        arc.id.clone(),
                        ArcNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(arc.id.clone());
                    imported.arcs.push(arc);
                }
                "Circle" if dictionary_name(annotation, b"IT").as_deref() != Some("CircleArc") => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.ellipse_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous Ellipse identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let ellipse = match import_ellipse(document, annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.ellipse_native_identities.insert(
                        ellipse.id.clone(),
                        EllipseNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(ellipse.id.clone());
                    imported.ellipses.push(ellipse);
                }
                "Ink" => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.pen_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous Ink identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let pen = match import_pen(annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.pen_native_identities.insert(
                        pen.id.clone(),
                        PenNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(pen.id.clone());
                    imported.pens.push(pen);
                }
                "FreeText" if is_cloud_plus_fragment(annotation) => {
                    imported
                        .untouched
                        .push(UntouchedAnnotation { name, subtype });
                }
                "FreeText" if is_callout_dictionary(annotation) => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.callout_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous callout identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let callout = match import_callout(document, annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.callout_native_identities.insert(
                        callout.id.clone(),
                        CalloutNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(callout.id.clone());
                    imported.callouts.push(callout);
                }
                "FreeText" => {
                    let text_box = match import_text_box(document, annotation, name.clone(), page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.annotation_order.push(text_box.id.clone());
                    imported.text_boxes.push(text_box);
                }
                "Line" if is_length_like_dictionary(annotation) => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let length = match import_length(
                        document,
                        annotation,
                        stable_name.clone(),
                        page_index,
                        page_length_calibrations.get(&page_index),
                    ) {
                        Ok(length) => length,
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    if imported.length_native_identities.contains_key(&length.id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous length identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    imported.length_native_identities.insert(
                        length.id.clone(),
                        LengthNativeIdentity {
                            raw_name: physical_name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(length.id.clone());
                    imported.lengths.push(length);
                }
                "Line"
                    if dictionary_name(annotation, b"IT").as_deref() == Some("LineDimension") =>
                {
                    let Object::Reference(_) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let dimension = match import_dimension(document, annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    if imported.dimension_native_names.contains_key(&dimension.id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous dimension identity {}",
                            dimension.id
                        )));
                    }
                    imported
                        .dimension_native_names
                        .insert(dimension.id.clone(), name);
                    imported.annotation_order.push(dimension.id.clone());
                    imported.dimensions.push(dimension);
                }
                "Line" => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported
                        .straight_line_native_identities
                        .contains_key(&stable_id)
                    {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous straight-line identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let straight_line = match import_straight_line(annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.straight_line_native_identities.insert(
                        straight_line.id.clone(),
                        StraightLineNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(straight_line.id.clone());
                    imported.straight_lines.push(straight_line);
                }
                "Polygon" if is_cloud_plus_fragment(annotation) => {
                    imported
                        .untouched
                        .push(UntouchedAnnotation { name, subtype });
                }
                "Polygon" if is_cloud_dictionary(annotation) => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.cloud_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous cloud identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let cloud = match import_cloud(annotation, stable_name, page_index) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.cloud_native_identities.insert(
                        cloud.id.clone(),
                        CloudNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(cloud.id.clone());
                    imported.clouds.push(cloud);
                }
                "PolyLine" | "Polygon"
                    if measurement_path_kind(annotation, subtype.as_str()).is_some() =>
                {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported
                        .measurement_path_native_identities
                        .contains_key(&stable_id)
                    {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous measurement-path identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let kind = measurement_path_kind(annotation, subtype.as_str())
                        .expect("a guarded measurement path keeps its classified kind");
                    let measurement = match import_measurement_path(
                        document,
                        annotation,
                        stable_name,
                        page_index,
                        kind,
                        page_length_calibrations.get(&page_index),
                    ) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.measurement_path_native_identities.insert(
                        measurement.id.clone(),
                        MeasurementPathNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(measurement.id.clone());
                    imported.measurement_paths.push(measurement);
                }
                "PolyLine" | "Polygon" if is_basic_vertex_path_dictionary(annotation) => {
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let stable_name = name.clone();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported
                        .vertex_path_native_identities
                        .contains_key(&stable_id)
                    {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous vertex-path identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let vertex_path = match import_vertex_path(
                        annotation,
                        stable_name,
                        page_index,
                        if subtype == "PolyLine" {
                            VertexPathKind::Polyline
                        } else {
                            VertexPathKind::Polygon
                        },
                    ) {
                        Ok(value) => value,
                        // A markup this model cannot represent is kept exactly.
                        Err(_) => {
                            imported
                                .untouched
                                .push(UntouchedAnnotation { name, subtype });
                            continue;
                        }
                    };
                    imported.vertex_path_native_identities.insert(
                        vertex_path.id.clone(),
                        VertexPathNativeIdentity {
                            raw_name: name,
                            object_id: *object_id,
                        },
                    );
                    imported.annotation_order.push(vertex_path.id.clone());
                    imported.vertex_paths.push(vertex_path);
                }
                _ => imported
                    .untouched
                    .push(UntouchedAnnotation { name, subtype }),
            }
            if imported.annotation_order.len() > admitted_before {
                imported
                    .managed_annotation_slots
                    .entry(page_id)
                    .or_default()
                    .insert(annotation_index);
            }
        }
    }
    for redact in &imported.redacts {
        if imported
            .annotation_order
            .iter()
            .filter(|candidate| *candidate == &redact.id)
            .count()
            != 1
        {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "pending Redact identity {} collides with another managed native annotation",
                redact.id
            )));
        }
    }
    for snapshot in &imported.snapshots {
        if imported
            .annotation_order
            .iter()
            .filter(|candidate| *candidate == &snapshot.id)
            .count()
            != 1
        {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "Snapshot identity {} collides with another managed native annotation",
                snapshot.id
            )));
        }
    }
    let mut reserved_ids = HashSet::new();
    for id in &imported.annotation_order {
        if !reserved_ids.insert(id.clone()) {
            return Err(PdfPersistenceError::InvalidDocument(format!(
                "ambiguous managed annotation identity {id}: multiple native annotations normalize to the same stable id"
            )));
        }
    }
    imported.retained_annotation_obstacles =
        retained_annotation_obstacles(document, &imported.managed_annotation_slots);
    Ok(imported)
}

fn measurement_path_kind(annotation: &Dictionary, subtype: &str) -> Option<MeasurementPathKind> {
    let subject = dictionary_string(annotation, b"Subj")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let intent = dictionary_name(annotation, b"IT")
        .unwrap_or_default()
        .to_ascii_lowercase();
    match subtype {
        "PolyLine"
            if subject == "polylength"
                || subject == "polylength measurement"
                || (intent == "polylinedimension" && annotation.get(b"Measure").is_ok()) =>
        {
            Some(MeasurementPathKind::Polylength)
        }
        "Polygon"
            if subject == "area"
                || subject == "area measurement"
                || (intent == "polygondimension" && annotation.get(b"Measure").is_ok()) =>
        {
            Some(MeasurementPathKind::Area)
        }
        _ => None,
    }
}

fn retained_annotation_obstacles(
    document: &Document,
    managed_slots: &BTreeMap<ObjectId, HashSet<usize>>,
) -> Vec<RetainedAnnotationObstacle> {
    let mut obstacles = Vec::new();
    for (page_number, page_id) in document.get_pages() {
        let Ok(page) = document.get_object(page_id).and_then(Object::as_dict) else {
            continue;
        };
        let Ok(annotation_object) = page.get(b"Annots") else {
            continue;
        };
        let Ok(annotations) =
            resolve_object(document, annotation_object).and_then(Object::as_array)
        else {
            continue;
        };
        for (annotation_index, annotation_object) in annotations.iter().enumerate() {
            if managed_slots
                .get(&page_id)
                .is_some_and(|slots| slots.contains(&annotation_index))
            {
                continue;
            }
            let Ok(annotation) =
                resolve_object(document, annotation_object).and_then(Object::as_dict)
            else {
                continue;
            };
            let subtype = dictionary_name(annotation, b"Subtype").unwrap_or_default();
            if matches!(subtype.as_str(), "Link" | "Popup") {
                continue;
            }
            let Some(rect) = best_effort_annotation_rect(document, annotation) else {
                continue;
            };
            let name = dictionary_string(annotation, b"NM").unwrap_or_else(|| {
                format!("page-{}-annotation-{annotation_index}", page_number - 1)
            });
            obstacles.push(RetainedAnnotationObstacle {
                id: format!("opaque:{}:{annotation_index}:{name}", page_number - 1),
                page_index: page_number.saturating_sub(1),
                rect,
            });
        }
    }
    obstacles.sort_by(|left, right| {
        (left.page_index, left.id.as_str()).cmp(&(right.page_index, right.id.as_str()))
    });
    obstacles
}

fn best_effort_annotation_rect(document: &Document, annotation: &Dictionary) -> Option<PdfRect> {
    let values = resolve_object(document, annotation.get(b"Rect").ok()?)
        .ok()?
        .as_array()
        .ok()?;
    let [left, bottom, right, top] = values.as_slice() else {
        return None;
    };
    let number = |value: &Object| {
        resolve_object(document, value)
            .ok()?
            .as_float()
            .ok()
            .map(f64::from)
            .filter(|value| value.is_finite())
    };
    let (left, bottom, right, top) = (number(left)?, number(bottom)?, number(right)?, number(top)?);
    PdfRect::new(
        left.min(right),
        bottom.min(top),
        (right - left).abs(),
        (top - bottom).abs(),
    )
    .ok()
}

fn is_cloud_dictionary(annotation: &Dictionary) -> bool {
    let intent = dictionary_name(annotation, b"IT")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let border_effect_is_cloud = annotation
        .get(b"BE")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .and_then(|effect| dictionary_name(effect, b"S"))
        .is_some_and(|style| style.eq_ignore_ascii_case("C"));
    intent == "polygoncloud" || border_effect_is_cloud
}

fn is_cloud_plus_fragment(annotation: &Dictionary) -> bool {
    dictionary_string(annotation, b"Subj")
        .is_some_and(|subject| subject.eq_ignore_ascii_case("Cloud+"))
        || dictionary_name(annotation, b"ITEx")
            .is_some_and(|intent| intent.eq_ignore_ascii_case("PolyText"))
        || annotation
            .get(b"GroupNesting")
            .ok()
            .and_then(|value| value.as_array().ok())
            .is_some_and(|group| {
                group.iter().any(|value| {
                    value.as_str().ok().is_some_and(|bytes| {
                        String::from_utf8_lossy(bytes).eq_ignore_ascii_case("Cloud+")
                    })
                })
            })
}

fn is_callout_dictionary(annotation: &Dictionary) -> bool {
    dictionary_name(annotation, b"IT")
        .is_some_and(|intent| intent.eq_ignore_ascii_case("FreeTextCallout"))
        || annotation.get(b"CL").is_ok()
}

fn is_basic_vertex_path_dictionary(annotation: &Dictionary) -> bool {
    [
        b"IT".as_slice(),
        b"Measure".as_slice(),
        b"BE".as_slice(),
        b"LE".as_slice(),
        b"Popup".as_slice(),
    ]
    .into_iter()
    .all(|key| annotation.get(key).is_err())
}

fn resolve_object<'a>(
    document: &'a Document,
    object: &'a Object,
) -> Result<&'a Object, lopdf::Error> {
    match object {
        Object::Reference(id) => document.get_object(*id),
        _ => Ok(object),
    }
}

fn import_rectangle(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<RectangleAnnotation, PdfPersistenceError> {
    let stroke_width = import_border_width(annotation);
    let (rect, rotation_degrees) = import_padded_rotated_box(document, annotation)?;
    Ok(RectangleAnnotation {
        id: MarkupId::new(name)?,
        page_index,
        rect,
        rotation_degrees,
        appearance: import_shape_appearance(annotation, stroke_width)?,
        locked: annotation_locked(annotation),
    })
}

/// Border width from `/BS` (or the older `/Border` array); the PDF default is 1.
fn import_border_width(annotation: &Dictionary) -> f64 {
    annotation
        .get(b"BS")
        .ok()
        .and_then(|object| object.as_dict().ok())
        .and_then(|border| border.get(b"W").ok())
        .and_then(|width| width.as_float().ok())
        .map(f64::from)
        .or_else(|| {
            annotation
                .get(b"Border")
                .ok()
                .and_then(|object| object.as_array().ok())
                .and_then(|values| values.get(2))
                .and_then(|width| width.as_float().ok())
                .map(f64::from)
        })
        .unwrap_or(1.)
}

fn import_stroke_style(annotation: &Dictionary, stroke_width: f64) -> StrokeStyle {
    let Some(border) = annotation
        .get(b"BS")
        .ok()
        .and_then(|object| object.as_dict().ok())
    else {
        return StrokeStyle::Solid;
    };
    if border.get(b"S").ok().and_then(|style| style.as_name().ok()) != Some(b"D".as_slice()) {
        return StrokeStyle::Solid;
    }
    let first_dash = border
        .get(b"D")
        .ok()
        .and_then(|object| object.as_array().ok())
        .and_then(|values| values.first())
        .and_then(|value| value.as_float().ok())
        .map(f64::from);
    if stroke_width > f64::EPSILON && first_dash.is_some_and(|dash| dash / stroke_width <= 1.5) {
        StrokeStyle::Dotted
    } else {
        StrokeStyle::Dashed
    }
}

fn import_opacity(annotation: &Dictionary) -> f64 {
    dictionary_float(annotation, b"CA").map_or(1., |value| value.clamp(0., 1.))
}

fn import_fill_opacity(annotation: &Dictionary) -> f64 {
    dictionary_float(annotation, b"FillOpacity").map_or(1., |value| value.clamp(0., 1.))
}

fn import_shape_appearance(
    annotation: &Dictionary,
    stroke_width: f64,
) -> Result<RectangleAppearance, PdfPersistenceError> {
    let stroke = dictionary_color(annotation, b"C").unwrap_or_else(|| "#ff0000".into());
    let fill = dictionary_color(annotation, b"IC");
    Ok(
        RectangleAppearance::new(stroke, stroke_width, fill, import_opacity(annotation))?
            .with_fill_opacity(import_fill_opacity(annotation))?
            .with_stroke_style(import_stroke_style(annotation, stroke_width)),
    )
}

fn import_rect_differences(annotation: &Dictionary) -> [f64; 4] {
    annotation
        .get(b"RD")
        .ok()
        .and_then(|object| object.as_array().ok())
        .and_then(|values| {
            let values = values
                .iter()
                .map(|value| value.as_float().ok().map(f64::from))
                .collect::<Option<Vec<_>>>()?;
            <[f64; 4]>::try_from(values).ok()
        })
        .filter(|values| values.iter().all(|value| value.is_finite() && *value >= 0.))
        .unwrap_or([0.; 4])
}

/// The drawn box of a Square, Circle, Image or Snapshot: `/Rect` less `/RD`,
/// or for a rotated markup the unrotated appearance `/BBox` (Revu's layout)
/// centred on `/Rect`.
fn import_padded_rotated_box(
    document: &Document,
    annotation: &Dictionary,
) -> Result<(PdfRect, f64), PdfPersistenceError> {
    let outer = import_pdf_rect(annotation, b"Rect")?;
    let [left, bottom, right, top] = import_rect_differences(annotation);
    let rotation_degrees = dictionary_float(annotation, b"Rotation")
        .unwrap_or(0.)
        .rem_euclid(360.);
    if rotation_degrees == 0. {
        return Ok((
            PdfRect::new(
                outer.x + left,
                outer.y + bottom,
                (outer.width - left - right).max(f64::EPSILON),
                (outer.height - bottom - top).max(f64::EPSILON),
            )?,
            0.,
        ));
    }
    let bbox = normal_appearance_stream(document, annotation)
        .ok()
        .and_then(|stream| import_pdf_rect(&stream.dict, b"BBox").ok());
    let (center_x, center_y) = (outer.x + outer.width / 2., outer.y + outer.height / 2.);
    // Revu places the unrotated box at its true page position, centred on
    // `/Rect`; use it directly when it is.
    if let Some(bbox) = bbox.filter(|bbox| {
        (bbox.x + bbox.width / 2. - center_x).abs() < 0.01
            && (bbox.y + bbox.height / 2. - center_y).abs() < 0.01
    }) {
        return Ok((
            PdfRect::new(
                bbox.x + left,
                bbox.y + bottom,
                (bbox.width - left - right).max(f64::EPSILON),
                (bbox.height - bottom - top).max(f64::EPSILON),
            )?,
            rotation_degrees,
        ));
    }
    let unrotated_size = bbox
        .map(|bbox| (bbox.width, bbox.height))
        .or_else(|| unrotated_box_size(outer, rotation_degrees))
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "rotated markup has no recoverable unrotated size".into(),
            )
        })?;
    let width = (unrotated_size.0 - left - right).max(f64::EPSILON);
    let height = (unrotated_size.1 - bottom - top).max(f64::EPSILON);
    Ok((
        PdfRect::new(center_x - width / 2., center_y - height / 2., width, height)?,
        rotation_degrees,
    ))
}

/// Inverts the rotated bounds of a box. Undefined at 45 degree multiples
/// where every aspect ratio has the same bounds.
fn unrotated_box_size(bounds: PdfRect, rotation_degrees: f64) -> Option<(f64, f64)> {
    let (sine, cosine) = rotation_degrees.to_radians().sin_cos();
    let (sine, cosine) = (sine.abs(), cosine.abs());
    let determinant = cosine * cosine - sine * sine;
    if determinant.abs() < 1e-6 {
        return None;
    }
    let width = (bounds.width * cosine - bounds.height * sine) / determinant;
    let height = (bounds.height * cosine - bounds.width * sine) / determinant;
    (width > 0. && height > 0.).then_some((width, height))
}

fn import_redact(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<RedactAnnotation, PdfPersistenceError> {
    let mut redact = RedactAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        import_pdf_rect(annotation, b"Rect")?,
        dictionary_color(annotation, b"IC").unwrap_or_else(|| "#000000".into()),
        dictionary_string(annotation, b"OverlayText"),
        RectangleAppearance::new("#ff0000", 1., Some("#000000"), 0.35)?.with_fill_opacity(0.35)?,
    )?;
    redact.locked = annotation_locked(annotation);
    Ok(redact)
}

fn import_ellipse(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<EllipseAnnotation, PdfPersistenceError> {
    let stroke_width = import_border_width(annotation);
    let (drawn, rotation_degrees) = import_padded_rotated_box(document, annotation)?;
    let half_width = stroke_width / 2.;
    let rect = PdfRect::new(
        drawn.x + half_width,
        drawn.y + half_width,
        (drawn.width - stroke_width).max(f64::EPSILON),
        (drawn.height - stroke_width).max(f64::EPSILON),
    )?;
    Ok(EllipseAnnotation {
        id: MarkupId::new(name)?,
        page_index,
        rect,
        rotation_degrees,
        appearance: import_shape_appearance(annotation, stroke_width)?,
        locked: annotation_locked(annotation),
    })
}

fn import_arc(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<ArcAnnotation, PdfPersistenceError> {
    let ellipse = import_ellipse(document, annotation, name, page_index)?;
    let rect = ellipse.rect;
    let angle1 = dictionary_float(annotation, b"Angle1").unwrap_or(90.);
    let angle2 = dictionary_float(annotation, b"Angle2").unwrap_or(180.);
    let mut arc = ArcAnnotation::from_rect_angles(
        ellipse.id,
        page_index,
        rect,
        angle1,
        angle2,
        ellipse.appearance,
    )?;
    arc.locked = ellipse.locked;
    Ok(arc)
}

fn import_pen(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<PenAnnotation, PdfPersistenceError> {
    let ink_lists = annotation.get(b"InkList")?.as_array()?;
    if ink_lists.is_empty() {
        return Err(PdfPersistenceError::InvalidDocument(
            "ink annotation has no path".into(),
        ));
    }
    let paths = ink_lists
        .iter()
        .map(|value| {
            let path = value.as_array()?;
            if path.len() < 4 || path.len() % 2 != 0 {
                return Err(PdfPersistenceError::InvalidDocument(
                    "ink path must contain coordinate pairs".into(),
                ));
            }
            path.chunks_exact(2)
                .map(|pair| {
                    Ok(PdfPoint::new(
                        f64::from(pair[0].as_float()?),
                        f64::from(pair[1].as_float()?),
                    )?)
                })
                .collect::<Result<Vec<_>, PdfPersistenceError>>()
        })
        .collect::<Result<Vec<_>, PdfPersistenceError>>()?;
    if paths.iter().any(|path| path.len() < 2) {
        return Err(PdfPersistenceError::InvalidDocument(
            "ink path must contain at least two canonical points".into(),
        ));
    }
    let color = dictionary_color(annotation, b"C").unwrap_or_else(|| "#000000".into());
    let width = annotation
        .get(b"BS")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .and_then(|border| border.get(b"W").ok())
        .and_then(|value| value.as_float().ok())
        .map_or(1.0, f64::from);
    let opacity = dictionary_float(annotation, b"CA").unwrap_or(1.0);
    let appearance = PenAppearance::new(color, width, opacity)?;
    // PDF has no dedicated highlighter subtype: Revu writes Ink with the
    // Multiply blend mode and a Highlight subject. Accept either signal.
    let is_highlight = dictionary_string(annotation, b"Subj")
        .is_some_and(|subject| subject.eq_ignore_ascii_case("highlight"))
        || dictionary_name(annotation, b"BM")
            .is_some_and(|blend| blend.eq_ignore_ascii_case("Multiply"));
    let mut imported = if is_highlight {
        PenAnnotation::new_highlight_paths(MarkupId::new(name)?, page_index, paths, appearance)?
    } else {
        // Smoothing is a Butter Paper display preference with no PDF field.
        let smooth_curves = true;
        PenAnnotation::new_paths(
            MarkupId::new(name)?,
            page_index,
            paths,
            appearance,
            smooth_curves,
        )?
    };
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn canonical_annotation_font_family(value: &str) -> Option<String> {
    let mut value = value.trim().trim_start_matches('/').trim().to_owned();
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let quoted = (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"');
        if quoted {
            value = value[1..value.len() - 1].trim().to_owned();
        }
    }
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return None;
    }
    if value.len() > 7
        && value.as_bytes()[..6]
            .iter()
            .all(|byte| byte.is_ascii_uppercase())
        && value.as_bytes()[6] == b'+'
    {
        value.drain(..7);
    }
    let mut normalized = value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();
    loop {
        let before = normalized.len();
        for suffix in [
            "bolditalic",
            "boldoblique",
            "italic",
            "oblique",
            "regular",
            "bold",
            "ps",
            "mt",
        ] {
            if normalized.ends_with(suffix) {
                normalized.truncate(normalized.len() - suffix.len());
                break;
            }
        }
        if normalized.len() == before {
            break;
        }
    }
    match normalized.as_str() {
        "helv" | "helvetica" | "helveticaneue" => Some("Helvetica".into()),
        "arimo" | "bparimo" | "arial" | "arialunicode" | "calibri" => Some("Arimo".into()),
        "robotomono" | "bprobotomono" | "couriernew" | "cousine" => Some("Roboto Mono".into()),
        "tinos" | "bptinos" | "timesnewroman" | "timesroman" | "georgia" | "cambria" => {
            Some("Tinos".into())
        }
        _ => None,
    }
}

fn css_annotation_font_family(annotation: &Dictionary) -> Option<String> {
    let style = dictionary_string(annotation, b"DS")?;
    if style.len() > 4096 {
        return None;
    }
    let declarations = style.split(';').filter_map(|declaration| {
        let (property, value) = declaration.split_once(':')?;
        Some((property.trim().to_ascii_lowercase(), value.trim()))
    });
    let declarations = declarations.collect::<Vec<_>>();
    if let Some((_, value)) = declarations
        .iter()
        .find(|(property, _)| property == "font-family")
    {
        return canonical_annotation_font_family(value.split(',').next()?.trim());
    }
    let (_, shorthand) = declarations
        .iter()
        .find(|(property, _)| property == "font")?;
    let (family, size) = shorthand.rsplit_once(char::is_whitespace)?;
    let size = size.strip_suffix("pt")?;
    let size = size.parse::<f64>().ok()?;
    size.is_finite()
        .then(|| canonical_annotation_font_family(family))
        .flatten()
}

const MAX_OPTIONAL_PDF_REFERENCE_HOPS: usize = 8;
const MAX_DEFAULT_APPEARANCE_BYTES: usize = 4096;

fn resolve_optional_object<'a>(
    document: &'a Document,
    object: &'a Object,
) -> Result<&'a Object, lopdf::Error> {
    let mut object = object;
    let mut visited = HashSet::new();
    for _ in 0..MAX_OPTIONAL_PDF_REFERENCE_HOPS {
        let Object::Reference(object_id) = object else {
            return Ok(object);
        };
        if !visited.insert(*object_id) {
            return Err(lopdf::Error::ReferenceCycle(*object_id));
        }
        object = document.get_object(*object_id)?;
    }
    if matches!(object, Object::Reference(_)) {
        Err(lopdf::Error::ReferenceLimit)
    } else {
        Ok(object)
    }
}

fn default_appearance_font_resource(annotation: &Dictionary) -> Option<Vec<u8>> {
    let Object::String(default_appearance, _) = annotation.get(b"DA").ok()? else {
        return None;
    };
    if default_appearance.len() > MAX_DEFAULT_APPEARANCE_BYTES {
        return None;
    }
    Content::decode(default_appearance)
        .ok()?
        .operations
        .into_iter()
        .find_map(|operation| {
            let [resource, size] = operation.operands.as_slice() else {
                return None;
            };
            if operation.operator != "Tf" {
                return None;
            }
            let resource = resource.as_name().ok()?;
            let size = f64::from(size.as_float().ok()?);
            (size.is_finite() && resource.len() <= 128).then(|| resource.to_vec())
        })
}

fn font_family_from_resources(
    document: &Document,
    resources: &Object,
    resource_name: &[u8],
) -> Option<String> {
    let resources = resolve_optional_object(document, resources)
        .ok()?
        .as_dict()
        .ok()?;
    let fonts = resolve_optional_object(document, resources.get(b"Font").ok()?)
        .ok()?
        .as_dict()
        .ok()?;
    let font = resolve_optional_object(document, fonts.get(resource_name).ok()?)
        .ok()?
        .as_dict()
        .ok()?;
    canonical_annotation_font_family(
        std::str::from_utf8(font.get(b"BaseFont").ok()?.as_name().ok()?).ok()?,
    )
}

fn standard_annotation_font_family(document: &Document, annotation: &Dictionary) -> Option<String> {
    if let Some(family) = css_annotation_font_family(annotation) {
        return Some(family);
    }
    let resource_name = default_appearance_font_resource(annotation)?;
    let family = annotation
        .get(b"DR")
        .ok()
        .and_then(|resources| font_family_from_resources(document, resources, &resource_name))
        .or_else(|| {
            normal_appearance_stream(document, annotation)
                .ok()
                .and_then(|appearance| appearance.dict.get(b"Resources").ok())
                .and_then(|resources| {
                    font_family_from_resources(document, resources, &resource_name)
                })
        });
    family.or_else(|| canonical_annotation_font_family(&String::from_utf8_lossy(&resource_name)))
}

fn import_text_box(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<TextBoxAnnotation, PdfPersistenceError> {
    let rotation_degrees = dictionary_float(annotation, b"Rotation").unwrap_or(0.);
    let style = import_text_style(document, annotation, true)?;
    let layout_rect = if rotation_degrees.abs() > f64::EPSILON {
        normal_appearance_stream(document, annotation)
            .ok()
            .and_then(|appearance| import_pdf_rect(&appearance.dict, b"BBox").ok())
            .unwrap_or(import_pdf_rect(annotation, b"Rect")?)
    } else {
        import_pdf_rect(annotation, b"Rect")?
    };
    let mut imported = TextBoxAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        layout_rect,
        dictionary_text_box_contents(annotation, b"Contents")?.unwrap_or_default(),
        style,
    )?
    .with_rotation_degrees(rotation_degrees)?;
    if let Ok(value) = annotation.get(b"RC")
        && let Ok(rich_text) = decode_pdf_text_string_compat(value)
        && let Some(spans) = parse_rich_text_spans(&rich_text)
    {
        // A span repeating the box's own style is not an override.
        let base = imported.style().clone();
        let runs = spans
            .into_iter()
            .map(|span| {
                let mut run = TextBoxRichTextRun::new(span.text)?;
                if let Some(family) = span.font_family.filter(|family| family != base.font_family()) {
                    run = run.with_font_family(family)?;
                }
                run = run.with_emphasis(span.bold, span.italic);
                if let Some(color) = span.color.filter(|color| color != base.color()) {
                    run = run.with_color(color)?;
                }
                if let Some(size) = span
                    .font_size_pt
                    .filter(|size| (size - base.font_size_pt()).abs() > 1e-6)
                {
                    run = run.with_font_size_pt(size)?;
                }
                Ok::<_, AnnotationError>(run)
            })
            .collect::<Result<Vec<_>, _>>()?;
        imported = imported.with_rich_text_runs(runs)?;
    }
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_callout(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<CalloutAnnotation, PdfPersistenceError> {
    // Read the style and text directly: a callout's text may be empty.
    let imported_style = import_text_style(document, annotation, true)?;
    let imported_content = dictionary_text_box_contents(annotation, b"Contents")?.unwrap_or_default();
    let outer = import_pdf_rect(annotation, b"Rect")?;
    let text_box = annotation
        .get(b"RD")
        .ok()
        .and_then(|value| value.as_array().ok())
        .and_then(|values| {
            let [left, bottom, right, top] = values.as_slice() else {
                return None;
            };
            Some([
                left.as_float().ok()?,
                bottom.as_float().ok()?,
                right.as_float().ok()?,
                top.as_float().ok()?,
            ])
        })
        .and_then(|rect_differences| {
            CalloutDiskGeometry::reconstruct_text_box(outer, rect_differences).ok()
        })
        .unwrap_or(outer);
    let leader = annotation
        .get(b"CL")?
        .as_array()?
        .chunks_exact(2)
        .map(|pair| {
            Ok(PdfPoint::new(
                f64::from(pair[0].as_float()?),
                f64::from(pair[1].as_float()?),
            )?)
        })
        .collect::<Result<Vec<_>, PdfPersistenceError>>()?;
    // Revu keeps the leader colour in `/DA` and the text colour in `/DS`.
    let stroke_color = parse_default_appearance(
        &dictionary_string(annotation, b"DA").unwrap_or_default(),
    )
    .0
    .unwrap_or_else(|| (&imported_style).color().to_owned());
    let width = import_callout_leader_width(annotation);
    let opacity = (&imported_style).opacity();
    let appearance = CalloutAppearance::new(
        StraightLineAppearance::new(stroke_color, width, opacity, StrokeStyle::Solid)?,
        (&imported_style).clone(),
    )?;
    let mut imported = CalloutAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        leader,
        text_box,
        imported_content.as_str(),
        appearance,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_cloud_plus_text_box(annotation: &Dictionary) -> Result<PdfRect, PdfPersistenceError> {
    let outer = import_pdf_rect(annotation, b"Rect")?;
    Ok(annotation
        .get(b"RD")
        .ok()
        .and_then(|value| value.as_array().ok())
        .and_then(|values| {
            let [left, bottom, right, top] = values.as_slice() else {
                return None;
            };
            Some((
                f64::from(left.as_float().ok()?),
                f64::from(bottom.as_float().ok()?),
                f64::from(right.as_float().ok()?),
                f64::from(top.as_float().ok()?),
            ))
        })
        .and_then(|(left, bottom, right, top)| {
            PdfRect::new(
                outer.x + left,
                outer.y + bottom,
                outer.width - left - right,
                outer.height - bottom - top,
            )
            .ok()
        })
        .unwrap_or(outer))
}

fn import_cloud_plus_appearance_path(
    document: &Document,
    annotation: &Dictionary,
) -> Result<Option<Vec<CloudAppearancePathCommand>>, PdfPersistenceError> {
    let Ok(appearance) = normal_appearance_stream(document, annotation) else {
        return Ok(None);
    };
    let bbox = import_pdf_rect(&appearance.dict, b"BBox")?;
    let annotation_rect = import_pdf_rect(annotation, b"Rect")?;
    let matrix = appearance
        .dict
        .get(b"Matrix")
        .ok()
        .map(|value| value.as_array())
        .transpose()?
        .map(|values| {
            let [a, b, c, d, e, f] = values.as_slice() else {
                return Err(PdfPersistenceError::InvalidDocument(
                    "Cloud+ appearance /Matrix must contain six numbers".into(),
                ));
            };
            Ok([
                f64::from(a.as_float()?),
                f64::from(b.as_float()?),
                f64::from(c.as_float()?),
                f64::from(d.as_float()?),
                f64::from(e.as_float()?),
                f64::from(f.as_float()?),
            ])
        })
        .transpose()?
        .unwrap_or([1., 0., 0., 1., 0., 0.]);
    let transform = |matrix: [f64; 6], point: PdfPoint| {
        PdfPoint::new(
            matrix[0] * point.x + matrix[2] * point.y + matrix[4],
            matrix[1] * point.x + matrix[3] * point.y + matrix[5],
        )
    };
    let bbox_corners = [
        PdfPoint::new(bbox.x, bbox.y)?,
        PdfPoint::new(bbox.x + bbox.width, bbox.y)?,
        PdfPoint::new(bbox.x + bbox.width, bbox.y + bbox.height)?,
        PdfPoint::new(bbox.x, bbox.y + bbox.height)?,
    ]
    .map(|point| transform(matrix, point))
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
    let min_x = bbox_corners
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = bbox_corners
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = bbox_corners
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_y = bbox_corners
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let transformed_width = max_x - min_x;
    let transformed_height = max_y - min_y;
    if transformed_width <= 0. || transformed_height <= 0. {
        return Err(PdfPersistenceError::InvalidDocument(
            "Cloud+ appearance has a degenerate transformed /BBox".into(),
        ));
    }
    let to_page = |point: PdfPoint| {
        let point = transform(matrix, point)?;
        let tolerance = 1e-6;
        if point.x < min_x - tolerance
            || point.x > max_x + tolerance
            || point.y < min_y - tolerance
            || point.y > max_y + tolerance
        {
            return Err(PdfPersistenceError::InvalidDocument(
                "Cloud+ appearance path escapes its transformed /BBox".into(),
            ));
        }
        Ok(PdfPoint::new(
            annotation_rect.x + (point.x - min_x) * annotation_rect.width / transformed_width,
            annotation_rect.y + (point.y - min_y) * annotation_rect.height / transformed_height,
        )?)
    };
    let content = Content::decode(&appearance.decompressed_content()?)?;
    let mut ctm = [1., 0., 0., 1., 0., 0.];
    let mut stack = Vec::new();
    let mut path = Vec::new();
    let mut painted = false;
    let operand_point = |operands: &[Object], offset: usize| {
        PdfPoint::new(
            f64::from(operands[offset].as_float()?),
            f64::from(operands[offset + 1].as_float()?),
        )
        .map_err(PdfPersistenceError::from)
    };
    let apply_ctm = |point: PdfPoint, ctm: [f64; 6]| transform(ctm, point);
    for operation in content.operations {
        match operation.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => {
                ctm = stack.pop().ok_or_else(|| {
                    PdfPersistenceError::InvalidDocument(
                        "Cloud+ appearance has an unmatched Q".into(),
                    )
                })?;
            }
            "cm" => {
                let [a, b, c, d, e, f] = operation.operands.as_slice() else {
                    return Err(PdfPersistenceError::InvalidDocument(
                        "Cloud+ appearance cm must contain six numbers".into(),
                    ));
                };
                let next = [
                    f64::from(a.as_float()?),
                    f64::from(b.as_float()?),
                    f64::from(c.as_float()?),
                    f64::from(d.as_float()?),
                    f64::from(e.as_float()?),
                    f64::from(f.as_float()?),
                ];
                ctm = [
                    next[0] * ctm[0] + next[2] * ctm[1],
                    next[1] * ctm[0] + next[3] * ctm[1],
                    next[0] * ctm[2] + next[2] * ctm[3],
                    next[1] * ctm[2] + next[3] * ctm[3],
                    next[0] * ctm[4] + next[2] * ctm[5] + next[4],
                    next[1] * ctm[4] + next[3] * ctm[5] + next[5],
                ];
            }
            "m" => {
                if painted || !path.is_empty() || operation.operands.len() != 2 {
                    return Err(PdfPersistenceError::InvalidDocument(
                        "Cloud+ appearance must contain one m/l/c/h path".into(),
                    ));
                }
                path.push(CloudAppearancePathCommand::MoveTo(to_page(apply_ctm(
                    operand_point(&operation.operands, 0)?,
                    ctm,
                )?)?));
            }
            "l" => {
                if painted || operation.operands.len() != 2 {
                    return Err(PdfPersistenceError::InvalidDocument(
                        "Cloud+ appearance l must contain two numbers".into(),
                    ));
                }
                path.push(CloudAppearancePathCommand::LineTo(to_page(apply_ctm(
                    operand_point(&operation.operands, 0)?,
                    ctm,
                )?)?));
            }
            "c" => {
                if painted || operation.operands.len() != 6 {
                    return Err(PdfPersistenceError::InvalidDocument(
                        "Cloud+ appearance c must contain six numbers".into(),
                    ));
                }
                path.push(CloudAppearancePathCommand::CubicTo {
                    control_1: to_page(apply_ctm(operand_point(&operation.operands, 0)?, ctm)?)?,
                    control_2: to_page(apply_ctm(operand_point(&operation.operands, 2)?, ctm)?)?,
                    end: to_page(apply_ctm(operand_point(&operation.operands, 4)?, ctm)?)?,
                });
            }
            "h" if !painted => path.push(CloudAppearancePathCommand::Close),
            // A filled Cloud+ (as Revu allows) fills and strokes its path.
            "S" | "B" | "B*" => {
                if painted || path.is_empty() {
                    return Err(PdfPersistenceError::InvalidDocument(
                        "Cloud+ appearance must contain exactly one stroked path".into(),
                    ));
                }
                // Revu strokes its scallops back to the start without `h`.
                if !matches!(path.last(), Some(CloudAppearancePathCommand::Close)) {
                    path.push(CloudAppearancePathCommand::Close);
                }
                painted = true;
            }
            "s" | "b" | "b*" => {
                if painted || path.is_empty() {
                    return Err(PdfPersistenceError::InvalidDocument(
                        "Cloud+ appearance must contain exactly one closed stroke".into(),
                    ));
                }
                if !matches!(path.last(), Some(CloudAppearancePathCommand::Close)) {
                    path.push(CloudAppearancePathCommand::Close);
                }
                painted = true;
            }
            "v" | "y" | "re" => {
                return Err(PdfPersistenceError::InvalidDocument(format!(
                    "Cloud+ appearance path operator {} is unsupported",
                    operation.operator
                )));
            }
            "w" | "J" | "j" | "M" | "d" | "RG" | "rg" | "G" | "g" | "K" | "k" | "CS" | "cs"
            | "SC" | "sc" | "SCN" | "scn" | "gs" => {}
            _ => {
                return Err(PdfPersistenceError::InvalidDocument(format!(
                    "Cloud+ appearance operator {} is unsupported",
                    operation.operator
                )));
            }
        }
    }
    if !stack.is_empty() {
        return Err(PdfPersistenceError::InvalidDocument(
            "Cloud+ appearance has an unmatched q".into(),
        ));
    }
    if path.is_empty() {
        Ok(None)
    } else if !painted {
        Err(PdfPersistenceError::InvalidDocument(
            "Cloud+ appearance path is not stroked".into(),
        ))
    } else {
        Ok(Some(path))
    }
}

fn import_cloud_plus_pair(
    document: &Document,
    cloud_dictionary: &Dictionary,
    text_dictionary: &Dictionary,
    stable_name: String,
    page_index: u32,
) -> Result<CloudPlusAnnotation, PdfPersistenceError> {
    let cloud = import_cloud(cloud_dictionary, stable_name.clone(), page_index)?;
    let imported_content =
        dictionary_text_box_contents(text_dictionary, b"Contents")?.unwrap_or_default();
    let leader_values = text_dictionary.get(b"CL")?.as_array()?;
    if !matches!(leader_values.len(), 0 | 6) {
        return Err(PdfPersistenceError::InvalidDocument(
            "Cloud+ /CL must contain either zero or six numbers".into(),
        ));
    }
    let leader_points = leader_values
        .chunks_exact(2)
        .map(|pair| {
            Ok(PdfPoint::new(
                f64::from(pair[0].as_float()?),
                f64::from(pair[1].as_float()?),
            )?)
        })
        .collect::<Result<Vec<_>, PdfPersistenceError>>()?;
    let imported_text_style =
        import_measurement_text_style(document, text_dictionary, cloud.appearance.opacity())?;
    let text_style = TextBoxStyle::new(
        imported_text_style.font_family(),
        imported_text_style.font_size_pt(),
        imported_text_style.color(),
        cloud.appearance.opacity(),
    )?
    .with_weight_and_alignment(
        imported_text_style.weight(),
        imported_text_style.alignment(),
    )?
    .with_layout_metrics(
        imported_text_style.line_height_pt(),
        imported_text_style.inset_pt(),
    )?;
    // Revu keeps the leader colour in the text member's `/DA`.
    let leader_color = parse_default_appearance(
        &dictionary_string(text_dictionary, b"DA").unwrap_or_default(),
    )
    .0
    .unwrap_or_else(|| cloud.appearance.stroke_color().to_owned());
    let leader_width = import_callout_leader_width(text_dictionary);
    let leader_style = StrokeStyle::Solid;
    let leader_appearance = StraightLineAppearance::new(
        leader_color,
        leader_width,
        cloud.appearance.opacity(),
        leader_style,
    )?;
    let appearance = CloudPlusAppearance::new(
        cloud.appearance.clone(),
        leader_appearance,
        text_style,
    )?;
    let cloud_appearance_path = import_cloud_plus_appearance_path(document, cloud_dictionary)?
        .filter(|path| {
            let imported = sample_cloud_appearance_path(path);
            let generated = cloud.scallop_path();
            imported.len() != generated.len()
                || imported.iter().zip(generated).any(|(left, right)| {
                    (left.x - right.x).abs() > 1e-4 || (left.y - right.y).abs() > 1e-4
                })
        });
    let mut imported = CloudPlusAnnotation::new(
        MarkupId::new(stable_name)?,
        page_index,
        cloud.points().to_vec(),
        cloud.border_effect_intensity(),
        leader_points,
        import_cloud_plus_text_box(text_dictionary)?,
        imported_content.as_str(),
        appearance,
    )?
    .with_cloud_appearance_path(cloud_appearance_path)?;
    imported.locked = cloud.locked || annotation_locked(text_dictionary);
    Ok(imported)
}

fn import_length(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
    page_calibration: Option<&LengthCalibration>,
) -> Result<LengthAnnotation, PdfPersistenceError> {
    let line = annotation.get(b"L")?.as_array()?;
    let [start_x, start_y, end_x, end_y] = line.as_slice() else {
        return Err(PdfPersistenceError::InvalidDocument(
            "length /L must contain four numbers".into(),
        ));
    };
    let calibration = import_standard_length_calibration(annotation, page_calibration)?;
    let opacity = dictionary_float(annotation, b"CA").unwrap_or(1.);
    let stroke_width = annotation
        .get(b"BS")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .and_then(|border| dictionary_float(border, b"W"))
        .unwrap_or(1.);
    let stroke_style = annotation
        .get(b"BS")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .filter(|border| dictionary_name(border, b"S").as_deref() == Some("D"))
        .map_or(StrokeStyle::Solid, |border| {
            let first_dash = border
                .get(b"D")
                .ok()
                .and_then(|value| value.as_array().ok())
                .and_then(|values| values.first())
                .and_then(|value| value.as_float().ok())
                .map(f64::from);
            if stroke_width > f64::EPSILON
                && first_dash.is_some_and(|dash| dash / stroke_width <= 1.5)
            {
                StrokeStyle::Dotted
            } else {
                StrokeStyle::Dashed
            }
        });
    let line = StraightLineAppearance::new(
        dictionary_color(annotation, b"C").unwrap_or_else(|| "#ff0000".into()),
        stroke_width,
        opacity,
        stroke_style,
    )?;
    let mut imported = LengthAnnotation::new_with_appearance(
        MarkupId::new(name)?,
        page_index,
        PdfPoint::new(
            f64::from(start_x.as_float()?),
            f64::from(start_y.as_float()?),
        )?,
        PdfPoint::new(f64::from(end_x.as_float()?), f64::from(end_y.as_float()?))?,
        calibration,
        DimensionAppearance::new(
            line,
            import_measurement_text_style(document, annotation, opacity)?,
        )?,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_dimension(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<DimensionAnnotation, PdfPersistenceError> {
    let line_points = annotation.get(b"L")?.as_array()?;
    let [start_x, start_y, end_x, end_y] = line_points.as_slice() else {
        return Err(PdfPersistenceError::InvalidDocument(
            "dimension /L must contain four numbers".into(),
        ));
    };
    let start = PdfPoint::new(
        f64::from(start_x.as_float()?),
        f64::from(start_y.as_float()?),
    )?;
    let end = PdfPoint::new(f64::from(end_x.as_float()?), f64::from(end_y.as_float()?))?;
    let stroke_color = dictionary_color(annotation, b"C").unwrap_or_else(|| "#ff0000".into());
    let stroke_width = import_border_width(annotation);
    let stroke_style = import_stroke_style(annotation, stroke_width);
    let opacity = import_opacity(annotation);
    let appearance = DimensionAppearance::new(
        StraightLineAppearance::new(stroke_color, stroke_width, opacity, stroke_style)?,
        import_measurement_text_style(document, annotation, opacity)?,
    )?;
    let content = dictionary_string(annotation, b"Contents").unwrap_or_default();
    let offset = dictionary_float(annotation, b"LL")
        .unwrap_or_else(|| DimensionAnnotation::default_offset(start, end));
    let mut imported = DimensionAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        start,
        end,
        offset,
        content,
        appearance,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_straight_line(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<StraightLineAnnotation, PdfPersistenceError> {
    let coordinates = annotation
        .get(b"L")
        .or_else(|_| annotation.get(b"Rect"))?
        .as_array()?;
    let [start_x, start_y, end_x, end_y] = coordinates.as_slice() else {
        return Err(PdfPersistenceError::InvalidDocument(
            "straight-line /L or /Rect must contain four numbers".into(),
        ));
    };
    let kind = if dictionary_name(annotation, b"IT")
        .is_some_and(|intent| intent.eq_ignore_ascii_case("LineArrow"))
    {
        LineKind::Arrow
    } else {
        LineKind::Line
    };
    let stroke_width = annotation
        .get(b"BS")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .and_then(|border| border.get(b"W").ok())
        .and_then(|value| value.as_float().ok())
        .map(f64::from)
        .or_else(|| {
            annotation
                .get(b"Border")
                .ok()
                .and_then(|value| value.as_array().ok())
                .and_then(|border| border.get(2))
                .and_then(|value| value.as_float().ok())
                .map(f64::from)
        })
        .unwrap_or_else(|| StraightLineAppearance::default_for(kind).stroke_width_pt());
    let stroke_style = annotation
        .get(b"BS")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .and_then(|border| border.get(b"S").ok())
        .and_then(|value| value.as_name().ok())
        .map_or(StrokeStyle::Solid, |value| {
            if value == b"D" {
                let first_dash = annotation
                    .get(b"BS")
                    .ok()
                    .and_then(|object| object.as_dict().ok())
                    .and_then(|border| border.get(b"D").ok())
                    .and_then(|object| object.as_array().ok())
                    .and_then(|values| values.first())
                    .and_then(|value| value.as_float().ok())
                    .map(f64::from);
                if stroke_width > f64::EPSILON
                    && first_dash.is_some_and(|dash| dash / stroke_width <= 1.5)
                {
                    StrokeStyle::Dotted
                } else {
                    StrokeStyle::Dashed
                }
            } else {
                StrokeStyle::Solid
            }
        });
    let appearance = StraightLineAppearance::new(
        dictionary_color(annotation, b"C").unwrap_or_else(|| "#ff0000".into()),
        stroke_width,
        dictionary_float(annotation, b"CA")
            .or_else(|| dictionary_float(annotation, b"ca"))
            .unwrap_or(1.0),
        stroke_style,
    )?;
    let mut imported = StraightLineAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        PdfPoint::new(
            f64::from(start_x.as_float()?),
            f64::from(start_y.as_float()?),
        )?,
        PdfPoint::new(f64::from(end_x.as_float()?), f64::from(end_y.as_float()?))?,
        kind,
        appearance,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_vertex_path(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
    kind: VertexPathKind,
) -> Result<VertexPathAnnotation, PdfPersistenceError> {
    let vertices = annotation.get(b"Vertices")?.as_array()?;
    if vertices.len() % 2 != 0 {
        return Err(PdfPersistenceError::InvalidDocument(
            "vertex-path /Vertices must contain x/y pairs".into(),
        ));
    }
    let points = vertices
        .chunks_exact(2)
        .map(|pair| {
            Ok(PdfPoint::new(
                f64::from(pair[0].as_float()?),
                f64::from(pair[1].as_float()?),
            )?)
        })
        .collect::<Result<Vec<_>, PdfPersistenceError>>()?;
    let stroke_width = import_border_width(annotation);
    let mut appearance = import_shape_appearance(annotation, stroke_width)?;
    if kind == VertexPathKind::Polyline {
        appearance = appearance.without_fill();
    }
    let mut imported = VertexPathAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        points,
        kind,
        appearance,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_cloud(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<CloudAnnotation, PdfPersistenceError> {
    let imported = import_vertex_path(annotation, name, page_index, VertexPathKind::Polygon)?;
    let intensity = annotation
        .get(b"BE")
        .ok()
        .and_then(|value| value.as_dict().ok())
        .and_then(|effect| dictionary_float(effect, b"I"))
        .unwrap_or(2.0);
    let appearance = imported.appearance.clone();
    let points = imported.points().to_vec();
    let mut cloud = CloudAnnotation::new(
        imported.id,
        imported.page_index,
        points,
        intensity,
        appearance,
    )?;
    cloud.locked = imported.locked;
    Ok(cloud)
}

fn import_measurement_path(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
    kind: MeasurementPathKind,
    page_calibration: Option<&LengthCalibration>,
) -> Result<MeasurementPathAnnotation, PdfPersistenceError> {
    let vertex = import_vertex_path(
        annotation,
        name.clone(),
        page_index,
        match kind {
            MeasurementPathKind::Polylength => VertexPathKind::Polyline,
            MeasurementPathKind::Area => VertexPathKind::Polygon,
        },
    )?;
    let calibration = import_measurement_path_calibration(annotation, page_calibration, kind)?;
    let text_opacity = vertex.appearance.opacity();
    let mut imported = MeasurementPathAnnotation::new_with_text_style(
        MarkupId::new(name)?,
        page_index,
        vertex.points().to_vec(),
        kind,
        calibration,
        vertex.appearance,
        import_measurement_text_style(document, annotation, text_opacity)?,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_measurement_text_style(
    document: &Document,
    annotation: &Dictionary,
    _opacity: f64,
) -> Result<TextBoxStyle, PdfPersistenceError> {
    import_text_style(document, annotation, true)
}

fn import_measurement_path_calibration(
    annotation: &Dictionary,
    page_calibration: Option<&LengthCalibration>,
    kind: MeasurementPathKind,
) -> Result<LengthCalibration, PdfPersistenceError> {
    if annotation.get(b"Measure").is_ok() {
        let length = import_standard_length_calibration(annotation, page_calibration)?;
        if kind == MeasurementPathKind::Area
            && let Some(area) = import_area_calibration(annotation, page_calibration, &length)
        {
            return Ok(area);
        }
        return Ok(length);
    }
    Ok(page_calibration
        .cloned()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "measurement path has no annotation or page calibration".into(),
            )
        })?
        .with_label(dictionary_string(annotation, b"Label").unwrap_or_default())?)
}

/// Revu reports an area in its `/A` format's unit (`sq m`), which may differ
/// from the length display unit in `/D`.
fn import_area_calibration(
    annotation: &Dictionary,
    page_calibration: Option<&LengthCalibration>,
    length: &LengthCalibration,
) -> Option<LengthCalibration> {
    let measure = annotation.get(b"Measure").ok()?.as_dict().ok()?;
    let x = first_number_format(measure, b"X")?;
    let area = first_number_format(measure, b"A")?;
    let x_unit = dictionary_string(&x, b"U")?;
    let area_unit = dictionary_string(&area, b"U")?;
    let unit = area_unit.strip_prefix("sq ")?.trim().to_owned();
    let x_factor = page_calibration
        .filter(|page| page.unit() == x_unit)
        .map(LengthCalibration::units_per_point)
        .or_else(|| dictionary_float(&x, b"C"))?;
    let area_factor = dictionary_float(&area, b"C").filter(|value| *value > 0.)?;
    let precision = area
        .get(b"D")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .filter(|value| *value > 0)
        .map_or(length.precision(), |value| {
            (value as f64).log10().round().clamp(0., 12.) as u8
        });
    LengthCalibration::from_scale(1., x_factor * area_factor.sqrt(), unit, precision, true)
        .ok()?
        .with_label(length.label())
        .ok()
}

fn is_length_like_dictionary(annotation: &Dictionary) -> bool {
    let subject = dictionary_string(annotation, b"Subj")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    subject == "length" || subject == "length measurement" || annotation.get(b"Measure").is_ok()
}

/// A measurement's scale from its `/Measure`. Revu rounds the annotation's
/// `X` factor, so when the page viewport uses the same base unit its precise
/// factor is used; `D` then converts to the displayed unit.
fn import_standard_length_calibration(
    annotation: &Dictionary,
    page_calibration: Option<&LengthCalibration>,
) -> Result<LengthCalibration, PdfPersistenceError> {
    let measure = annotation.get(b"Measure")?.as_dict()?;
    let x = first_number_format(measure, b"X").ok_or_else(|| {
        PdfPersistenceError::InvalidDocument(
            "length measurement scale has no horizontal number format".into(),
        )
    })?;
    let x_unit = dictionary_string(&x, b"U").unwrap_or_else(|| "pt".into());
    let x_factor = page_calibration
        .filter(|page| page.unit() == x_unit)
        .map(LengthCalibration::units_per_point)
        .or_else(|| dictionary_float(&x, b"C"))
        .filter(|value| *value > 0.)
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "length measurement conversion factor is missing".into(),
            )
        })?;
    let display = first_number_format(measure, b"D");
    let (unit, factor) = display
        .as_ref()
        .and_then(|display| {
            Some((
                dictionary_string(display, b"U")?,
                dictionary_float(display, b"C").filter(|value| *value > 0.)?,
            ))
        })
        .unwrap_or((x_unit, 1.));
    let precision = display
        .as_ref()
        .unwrap_or(&x)
        .get(b"D")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .filter(|value| *value > 0)
        .and_then(|value| {
            let mut places = 0_u8;
            let mut divisor = value;
            while divisor > 1 && divisor % 10 == 0 {
                divisor /= 10;
                places = places.saturating_add(1);
            }
            (divisor == 1).then_some(places)
        })
        .unwrap_or(2);
    LengthCalibration::from_scale(1., x_factor * factor, unit, precision, true)?
        .with_label(dictionary_string(annotation, b"Label").unwrap_or_default())
        .map_err(PdfPersistenceError::from)
}

fn import_image(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<ImageAnnotation, PdfPersistenceError> {
    let asset = import_media_appearance_asset(document, annotation)?;
    let (rect, rotation_degrees) = import_padded_rotated_box(document, annotation)?;
    // Aspect lock has no PDF field: an image that still has its pixels' shape
    // reopens locked, anything else reopens free.
    let ratio = f64::from(asset.width_px()) / f64::from(asset.height_px());
    let restored = restore_image_aspect_after_pdf_rounding(rect, ratio);
    let aspect_locked = (restored.width / restored.height - ratio).abs() <= 0.000_001;
    let mut imported = ImageAnnotation::new_with_opacity(
        MarkupId::new(name)?,
        page_index,
        if aspect_locked { restored } else { rect },
        asset,
        aspect_locked,
        import_opacity(annotation),
    )?
    .with_rotation_degrees(rotation_degrees)?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

// PDF rectangle edges are f32 values. Restore the exact asset ratio only when
// doing so leaves every persisted edge unchanged.
fn restore_image_aspect_after_pdf_rounding(rect: PdfRect, ratio: f64) -> PdfRect {
    // Each stored edge represents an interval between adjacent f32 midpoints.
    // Find dimensions and an origin inside all four intervals simultaneously.
    let interval = |value: f64| {
        let value = value as f32;
        (
            (f64::from(value.next_down()) + f64::from(value)) / 2.,
            (f64::from(value) + f64::from(value.next_up())) / 2.,
        )
    };
    let (left, bottom, right, top) = (
        interval(rect.x),
        interval(rect.y),
        interval(rect.x + rect.width),
        interval(rect.y + rect.height),
    );
    let min_height = (top.0 - bottom.1).max((right.0 - left.1) / ratio);
    let max_height = (top.1 - bottom.0).min((right.1 - left.0) / ratio);
    if !min_height.is_finite()
        || !max_height.is_finite()
        || max_height <= min_height
        || max_height <= 0.
    {
        return rect;
    }
    let height = (min_height.max(0.) + max_height) / 2.;
    let width = height * ratio;
    let candidate = PdfRect {
        x: (left.0.max(right.0 - width) + left.1.min(right.1 - width)) / 2.,
        y: (bottom.0.max(top.0 - height) + bottom.1.min(top.1 - height)) / 2.,
        width,
        height,
    };
    if candidate.same_pdf_geometry_as(rect) {
        candidate
    } else {
        rect
    }
}

fn import_snapshot(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<SnapshotAnnotation, PdfPersistenceError> {
    let asset = import_media_appearance_asset(document, annotation)?;
    let (rect, rotation_degrees) = import_padded_rotated_box(document, annotation)?;
    let mut imported = SnapshotAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        rect,
        asset,
        import_opacity(annotation),
    )?
    .with_rotation_degrees(rotation_degrees)?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

/// Revu's Snapshot appearance is a vector Form copy of page content rather
/// than an image. Edits redraw that Form; the canvas shows a raster of it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct VectorSnapshotSource {
    form_id: ObjectId,
    bbox: PdfRect,
    matrix: [f64; 6],
}

fn vector_snapshot_source(document: &Document, annotation: &Dictionary) -> Option<VectorSnapshotSource> {
    let form_id = normal_appearance_object_id(annotation)?;
    let stream = document.get_object(form_id).ok()?.as_stream().ok()?;
    if dictionary_name(&stream.dict, b"Subtype").as_deref() != Some("Form") {
        return None;
    }
    let bbox = import_pdf_rect(&stream.dict, b"BBox").ok()?;
    let matrix = match stream.dict.get(b"Matrix") {
        Ok(matrix) => {
            let values = matrix
                .as_array()
                .ok()?
                .iter()
                .map(|value| value.as_float().ok().map(f64::from))
                .collect::<Option<Vec<_>>>()?;
            <[f64; 6]>::try_from(values).ok()?
        }
        Err(_) => [1., 0., 0., 1., 0., 0.],
    };
    let determinant = matrix[0] * matrix[3] - matrix[1] * matrix[2];
    (determinant.abs() > f64::EPSILON && bbox.width > 0. && bbox.height > 0.)
        .then_some(VectorSnapshotSource { form_id, bbox, matrix })
}

/// Shown until the PDF worker has drawn the Form for the canvas.
fn vector_snapshot_placeholder() -> Result<DecodedRgbaAsset, PdfPersistenceError> {
    Ok(DecodedRgbaAsset::new(1, 1, vec![200, 200, 200, 96])?)
}

fn import_vector_snapshot(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<(SnapshotAnnotation, VectorSnapshotSource), PdfPersistenceError> {
    let source = vector_snapshot_source(document, annotation).ok_or_else(|| {
        PdfPersistenceError::InvalidDocument("Snapshot appearance is neither image nor Form".into())
    })?;
    let (rect, rotation_degrees) = import_padded_rotated_box(document, annotation)?;
    let mut imported = SnapshotAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        rect,
        vector_snapshot_placeholder()?,
        import_opacity(annotation),
    )?
    .with_rotation_degrees(rotation_degrees)?;
    imported.locked = annotation_locked(annotation);
    Ok((imported, source))
}

/// Row-vector PDF affine product: `first` then `second`.
fn multiply_affine(first: [f64; 6], second: [f64; 6]) -> [f64; 6] {
    [
        first[0] * second[0] + first[1] * second[2],
        first[0] * second[1] + first[1] * second[3],
        first[2] * second[0] + first[3] * second[2],
        first[2] * second[1] + first[3] * second[3],
        first[4] * second[0] + first[5] * second[2] + second[4],
        first[4] * second[1] + first[5] * second[3] + second[5],
    ]
}

fn invert_affine(matrix: [f64; 6]) -> [f64; 6] {
    let [a, b, c, d, e, f] = matrix;
    let determinant = a * d - b * c;
    [
        d / determinant,
        -b / determinant,
        -c / determinant,
        a / determinant,
        (c * f - d * e) / determinant,
        (b * e - a * f) / determinant,
    ]
}

/// The `cm` that, followed by the Form's own `/Matrix` in `Do`, draws the
/// Form's `/BBox` content onto `target` (unrotated, in the drawing space).
fn vector_snapshot_placement(source: &VectorSnapshotSource, target: PdfRect) -> [f64; 6] {
    let scale_x = target.width / source.bbox.width;
    let scale_y = target.height / source.bbox.height;
    let fit = [
        scale_x,
        0.,
        0.,
        scale_y,
        target.x - scale_x * source.bbox.x,
        target.y - scale_y * source.bbox.y,
    ];
    multiply_affine(invert_affine(source.matrix), fit)
}

fn add_vector_snapshot_appearance(
    document: &mut Document,
    annotation: &SnapshotAnnotation,
    source: &VectorSnapshotSource,
) -> ObjectId {
    let (bbox, matrix, _) =
        rotated_box_appearance_placement(annotation.rect, annotation.rotation_degrees());
    let translucent = annotation.opacity() < 1.;
    let [a, b, c, d, e, f] = vector_snapshot_placement(source, annotation.rect);
    let content = format!(
        "q\n{}{a:.6} {b:.6} {c:.6} {d:.6} {e:.6} {f:.6} cm\n/Snapshot Do\nQ\n",
        if translucent { "/GS0 gs\n" } else { "" },
    );
    let mut resources = dictionary! {
        "XObject" => dictionary! { "Snapshot" => source.form_id },
    };
    if translucent {
        resources.set(
            "ExtGState",
            dictionary! { "GS0" => dictionary! {
                "Type" => "ExtGState",
                "CA" => Object::Real(annotation.opacity() as f32),
                "ca" => Object::Real(annotation.opacity() as f32),
            } },
        );
    }
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => bbox,
            "Matrix" => matrix,
            "Resources" => resources,
        },
        content.into_bytes(),
    ))
}

/// A render-only PDF with one page per Revu vector Snapshot, in import
/// order, each the Snapshot's unrotated box with its Form drawn on it. The PDF
/// worker rasterises these pages for the canvas.
pub fn vector_snapshot_layer(document: &Document) -> Result<Option<Vec<u8>>, PdfPersistenceError> {
    let calibrations = import_page_scales(document)
        .iter()
        .filter_map(|scale| {
            LengthCalibration::from_page_scale(scale)
                .ok()
                .map(|value| (scale.page_index, value))
        })
        .collect();
    let imported = import_annotations(document, &calibrations)?;
    if imported.vector_snapshot_sources.is_empty() {
        return Ok(None);
    }
    let mut layer = document.clone();
    let pages_id = layer.new_object_id();
    let mut kids = Vec::new();
    for (id, source) in &imported.vector_snapshot_sources {
        let snapshot = imported
            .snapshots
            .iter()
            .find(|snapshot| &snapshot.id == id)
            .expect("every vector Snapshot source was imported as a Snapshot");
        let size = PdfRect::new(0., 0., snapshot.rect.width, snapshot.rect.height)?;
        let [a, b, c, d, e, f] = vector_snapshot_placement(source, size);
        let content_id = layer.add_object(Stream::new(
            Dictionary::new(),
            format!("q {a:.6} {b:.6} {c:.6} {d:.6} {e:.6} {f:.6} cm /Snapshot Do Q").into_bytes(),
        ));
        let page_id = layer.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![
                0.into(),
                0.into(),
                Object::Real(size.width as f32),
                Object::Real(size.height as f32),
            ],
            "Contents" => content_id,
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Snapshot" => source.form_id },
            },
        });
        kids.push(Object::Reference(page_id));
    }
    layer.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => i64::try_from(kids.len()).unwrap_or(i64::MAX),
            "Kids" => kids,
        }),
    );
    let catalog_id = layer.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    layer.trailer = dictionary! { "Root" => catalog_id };
    let mut bytes = Vec::new();
    layer.save_to(&mut bytes)?;
    Ok(Some(bytes))
}

/// The first image XObject reachable from a resource dictionary, searching
/// nested forms the way Revu nests snapshot appearances.
fn find_image_xobject<'a>(
    document: &'a Document,
    resources: &'a Object,
    depth: usize,
) -> Option<&'a Stream> {
    if depth > 4 {
        return None;
    }
    let resources = resolve_optional_object(document, resources).ok()?.as_dict().ok()?;
    let xobjects = resolve_optional_object(document, resources.get(b"XObject").ok()?)
        .ok()?
        .as_dict()
        .ok()?;
    let mut forms = Vec::new();
    for (_, object) in xobjects.iter() {
        let Ok(stream) = resolve_optional_object(document, object).and_then(Object::as_stream)
        else {
            continue;
        };
        match dictionary_name(&stream.dict, b"Subtype").as_deref() {
            Some("Image") => return Some(stream),
            Some("Form") => forms.push(stream),
            _ => {}
        }
    }
    forms.into_iter().find_map(|form| {
        find_image_xobject(document, form.dict.get(b"Resources").ok()?, depth + 1)
    })
}

/// Decodes 8-bit DeviceRGB/DeviceGray samples (Flate or uncompressed) or a
/// DCT (JPEG) image, plus an optional soft mask.
fn decode_image_xobject(
    document: &Document,
    image: &Stream,
) -> Result<DecodedRgbaAsset, PdfPersistenceError> {
    let invalid = |message: &str| PdfPersistenceError::InvalidDocument(message.into());
    let width = u32::try_from(image.dict.get(b"Width")?.as_i64()?)
        .map_err(|_| invalid("image width is outside the supported range"))?;
    let height = u32::try_from(image.dict.get(b"Height")?.as_i64()?)
        .map_err(|_| invalid("image height is outside the supported range"))?;
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| usize::try_from(height).ok()?.checked_mul(width))
        .ok_or_else(|| invalid("image dimensions overflow"))?;
    let filters = match image.dict.get(b"Filter") {
        Ok(Object::Name(name)) => vec![name.clone()],
        Ok(Object::Array(names)) => names
            .iter()
            .filter_map(|name| name.as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => Vec::new(),
    };
    let mut rgba = if filters.last().is_some_and(|filter| filter == b"DCTDecode") {
        if image.content.len() > MAX_ENCODED_IMAGE_BYTES {
            return Err(invalid("JPEG image is too large"));
        }
        let decoded = image::load_from_memory_with_format(&image.content, image::ImageFormat::Jpeg)
            .map_err(|error| invalid(&format!("JPEG image cannot be decoded: {error}")))?
            .to_rgba8();
        if decoded.width() != width || decoded.height() != height {
            return Err(invalid("JPEG dimensions do not match the image XObject"));
        }
        decoded.into_raw()
    } else {
        if image
            .dict
            .get(b"BitsPerComponent")
            .ok()
            .and_then(|value| value.as_i64().ok())
            != Some(8)
        {
            return Err(invalid("only 8-bit image samples are supported"));
        }
        let samples = image.decompressed_content()?;
        let components = match image
            .dict
            .get(b"ColorSpace")
            .ok()
            .and_then(|value| value.as_name().ok())
        {
            Some(b"DeviceGray") => 1,
            Some(b"DeviceRGB") => 3,
            _ => return Err(invalid("only DeviceRGB and DeviceGray images are supported")),
        };
        if samples.len() != pixel_count * components {
            return Err(invalid("image XObject byte lengths do not match its dimensions"));
        }
        samples
            .chunks_exact(components)
            .flat_map(|pixel| {
                if components == 1 {
                    [pixel[0], pixel[0], pixel[0], u8::MAX]
                } else {
                    [pixel[0], pixel[1], pixel[2], u8::MAX]
                }
            })
            .collect()
    };
    if let Ok(mask) = image
        .dict
        .get(b"SMask")
        .and_then(|object| resolve_object(document, object))
        .and_then(Object::as_stream)
    {
        let alpha = mask.decompressed_content()?;
        if alpha.len() == pixel_count {
            for (pixel, alpha) in rgba.chunks_exact_mut(4).zip(alpha) {
                pixel[3] = alpha;
            }
        }
    }
    Ok(DecodedRgbaAsset::new(width, height, rgba)?)
}

/// A media markup's pixels: Revu's `/Image` key on a SquareImage, otherwise
/// the first image in its normal appearance.
fn import_media_appearance_asset(
    document: &Document,
    annotation: &Dictionary,
) -> Result<DecodedRgbaAsset, PdfPersistenceError> {
    if let Ok(image) = annotation
        .get(b"Image")
        .and_then(|object| resolve_object(document, object))
        .and_then(Object::as_stream)
    {
        return decode_image_xobject(document, image);
    }
    let appearance = normal_appearance_stream(document, annotation)?;
    let image = find_image_xobject(document, appearance.dict.get(b"Resources")?, 0).ok_or_else(
        || PdfPersistenceError::InvalidDocument("media appearance has no image".into()),
    )?;
    decode_image_xobject(document, image)
}

fn normal_appearance_stream<'a>(
    document: &'a Document,
    annotation: &'a Dictionary,
) -> Result<&'a Stream, lopdf::Error> {
    let appearances = resolve_optional_object(document, annotation.get(b"AP")?)?.as_dict()?;
    let normal = resolve_optional_object(document, appearances.get(b"N")?)?;
    if let Ok(stream) = normal.as_stream() {
        return Ok(stream);
    }
    let states = normal.as_dict()?;
    if let Some(state) = dictionary_name(annotation, b"AS") {
        if let Ok(stream) = states
            .get(state.as_bytes())
            .and_then(|object| resolve_optional_object(document, object))
            .and_then(Object::as_stream)
        {
            return Ok(stream);
        }
    }
    for object in states.iter().map(|(_, object)| object) {
        if let Ok(stream) = resolve_optional_object(document, object).and_then(Object::as_stream) {
            return Ok(stream);
        }
    }
    Err(lopdf::Error::InvalidStream(
        "normal appearance state dictionary has no resolvable stream".into(),
    ))
}

fn import_pdf_rect(dictionary: &Dictionary, key: &[u8]) -> Result<PdfRect, PdfPersistenceError> {
    let values = dictionary.get(key)?.as_array()?;
    let [left, bottom, right, top] = values.as_slice() else {
        return Err(PdfPersistenceError::InvalidDocument(
            "PDF rectangle must contain four numbers".into(),
        ));
    };
    let left = f64::from(left.as_float()?);
    let bottom = f64::from(bottom.as_float()?);
    let right = f64::from(right.as_float()?);
    let top = f64::from(top.as_float()?);
    Ok(PdfRect::new(left, bottom, right - left, top - bottom)?)
}

fn dictionary_float(dictionary: &Dictionary, key: &[u8]) -> Option<f64> {
    dictionary.get(key).ok()?.as_float().ok().map(f64::from)
}

fn annotation_locked(dictionary: &Dictionary) -> bool {
    dictionary
        .get(b"F")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .is_some_and(|flags| flags & 128 != 0)
}

fn dictionary_name(dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    dictionary
        .get(key)
        .ok()?
        .as_name()
        .ok()
        .map(|value| String::from_utf8_lossy(value).into_owned())
}

fn dictionary_string(dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    dictionary
        .get(key)
        .ok()
        .and_then(|object| decode_pdf_text_string_compat(object).ok())
}

fn import_page_grid_definition(document: &Document) -> Option<PageGridDefinition> {
    let info = resolve_object(document, document.trailer.get(b"Info").ok()?)
        .ok()?
        .as_dict()
        .ok()?;
    let subject = dictionary_string(info, b"Subject")?;
    let encoded = subject.strip_prefix("butter-paper:page-grid:")?;
    let value: Value = serde_json::from_str(encoded).ok()?;
    let object = value.as_object()?;
    if object.get("version")?.as_u64()? != 1 {
        return None;
    }
    let kind = match object.get("type")?.as_str()? {
        "rectangular" => PageGridKind::Rectangular,
        "ruled" => PageGridKind::Ruled,
        "isometric" => PageGridKind::Isometric,
        "triangle" => PageGridKind::Triangle,
        _ => return None,
    };
    let origin = object.get("origin")?.as_object()?;
    let origin = PdfPoint::new(origin.get("x")?.as_f64()?, origin.get("y")?.as_f64()?).ok()?;
    let source = match object.get("source")?.as_str()? {
        "generated" => PageGridSource::Generated,
        "detected" => PageGridSource::Detected,
        "manual" => PageGridSource::Manual,
        _ => return None,
    };
    PageGridDefinition::new(
        kind,
        origin,
        object.get("spacing")?.as_f64()?,
        object.get("width")?.as_f64()?,
        object.get("height")?.as_f64()?,
        object.get("rotationDegrees")?.as_f64()?,
        source,
    )
    .ok()
}

fn dictionary_text_box_contents(
    dictionary: &Dictionary,
    key: &[u8],
) -> Result<Option<String>, PdfPersistenceError> {
    let Ok(object) = dictionary.get(key) else {
        return Ok(None);
    };
    decode_pdf_text_string_compat(object).map(Some)
}

fn dictionary_color(dictionary: &Dictionary, key: &[u8]) -> Option<String> {
    let components = dictionary
        .get(key)
        .ok()?
        .as_array()
        .ok()?
        .iter()
        .map(|value| value.as_float().ok())
        .collect::<Option<Vec<_>>>()?;
    let [red, green, blue] = components.as_slice() else {
        return None;
    };
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        color_byte(*red),
        color_byte(*green),
        color_byte(*blue),
    ))
}

fn color_byte(component: f32) -> u8 {
    (component.clamp(0.0, 1.0) * 255.0).round() as u8
}

macro_rules! protocol_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Self {
                Self(value)
            }

            pub const fn value(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

protocol_id!(RequestId);
protocol_id!(SessionId);
protocol_id!(SourceHandleId);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenRequest {
    /// Correlates one response with this request.
    pub request_id: RequestId,
    /// Parent-assigned document session identifier.
    pub session_id: SessionId,
    /// Correlates an out-of-band inherited read-only source handle.
    pub source_handle_id: SourceHandleId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineRequest {
    Open(OpenRequest),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentPermissions {
    pub print: bool,
    pub copy: bool,
    pub modify: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub accessibility: bool,
    pub assemble: bool,
    pub high_quality_print: bool,
}

impl DocumentPermissions {
    pub fn all() -> Self {
        Self {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
            fill_forms: true,
            accessibility: true,
            assemble: true,
            high_quality_print: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentSecurity {
    Unencrypted,
    Encrypted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentInfo {
    pub page_count: u32,
    pub metadata: DocumentMetadata,
    pub permissions: DocumentPermissions,
    pub security: DocumentSecurity,
    /// True when the engine recovered a damaged cross-reference structure.
    pub xref_reconstructed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineErrorCode {
    PasswordRequired,
    UnsupportedSecurity,
    MalformedDocument,
    RepairedDocument,
    LimitExceeded,
    WorkerCrashed,
}

impl EngineErrorCode {
    fn as_str(self) -> &'static str {
        match self {
            Self::PasswordRequired => "password_required",
            Self::UnsupportedSecurity => "unsupported_security",
            Self::MalformedDocument => "malformed_document",
            Self::RepairedDocument => "repaired_document",
            Self::LimitExceeded => "limit_exceeded",
            Self::WorkerCrashed => "worker_crashed",
        }
    }

    fn parse(value: &str) -> Result<Self, ProtocolError> {
        match value {
            "password_required" => Ok(Self::PasswordRequired),
            "unsupported_security" => Ok(Self::UnsupportedSecurity),
            "malformed_document" => Ok(Self::MalformedDocument),
            "repaired_document" => Ok(Self::RepairedDocument),
            "limit_exceeded" => Ok(Self::LimitExceeded),
            "worker_crashed" => Ok(Self::WorkerCrashed),
            other => Err(ProtocolError::UnsupportedErrorCode(other.into())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineResponse {
    Opened {
        request_id: RequestId,
        document: DocumentInfo,
    },
    Failed {
        request_id: RequestId,
        /// The parent maps this stable code to user-facing text.
        error: EngineErrorCode,
    },
}

impl EngineResponse {
    fn request_id(&self) -> RequestId {
        match self {
            Self::Opened { request_id, .. } | Self::Failed { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtocolError {
    InvalidJson(String),
    InvalidMessage(String),
    UnexpectedProtocol(String),
    UnsupportedVersion(u64),
    UnsupportedCommand(String),
    UnsupportedResult(String),
    UnsupportedErrorCode(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson(message) => write!(formatter, "invalid PDF engine JSON: {message}"),
            Self::InvalidMessage(message) => {
                write!(formatter, "invalid PDF engine message: {message}")
            }
            Self::UnexpectedProtocol(protocol) => write!(
                formatter,
                "unexpected PDF engine protocol {protocol:?}; expected {PDF_ENGINE_PROTOCOL_NAME:?}",
            ),
            Self::UnsupportedVersion(version) => write!(
                formatter,
                "unsupported PDF engine protocol version {version}; expected {PDF_ENGINE_PROTOCOL_VERSION}",
            ),
            Self::UnsupportedCommand(command) => {
                write!(formatter, "unsupported PDF engine command {command:?}")
            }
            Self::UnsupportedResult(result) => {
                write!(formatter, "unsupported PDF engine result {result:?}")
            }
            Self::UnsupportedErrorCode(code) => {
                write!(formatter, "unsupported PDF engine error code {code:?}")
            }
        }
    }
}

impl Error for ProtocolError {}

pub fn encode_request(request: &EngineRequest) -> Result<Vec<u8>, ProtocolError> {
    let (request_id, command) = match request {
        EngineRequest::Open(request) => (
            request.request_id,
            json!({
                "type": "open",
                "session_id": request.session_id.to_string(),
                "source_handle_id": request.source_handle_id.to_string(),
            }),
        ),
    };
    encode_envelope(json!({
        "protocol": PDF_ENGINE_PROTOCOL_NAME,
        "version": PDF_ENGINE_PROTOCOL_VERSION,
        "request_id": request_id.to_string(),
        "command": command,
    }))
}

pub fn decode_request(bytes: &[u8]) -> Result<EngineRequest, ProtocolError> {
    let root = decode_envelope(bytes)?;
    let request_id = required_id(&root, "request_id", RequestId::new)?;
    let command = required_object(&root, "command")?;
    match required_string(command, "type")? {
        "open" => {
            reject_unknown_fields(command, "open", &["type", "session_id", "source_handle_id"])?;
            Ok(EngineRequest::Open(OpenRequest {
                request_id,
                session_id: required_id(command, "session_id", SessionId::new)?,
                source_handle_id: required_id(command, "source_handle_id", SourceHandleId::new)?,
            }))
        }
        other => Err(ProtocolError::UnsupportedCommand(other.into())),
    }
}

pub fn encode_response(response: &EngineResponse) -> Result<Vec<u8>, ProtocolError> {
    let result = match response {
        EngineResponse::Opened { document, .. } => json!({
            "type": "opened",
            "document": document_to_value(document),
        }),
        EngineResponse::Failed { error, .. } => json!({
            "type": "failed",
            "code": error.as_str(),
        }),
    };
    encode_envelope(json!({
        "protocol": PDF_ENGINE_PROTOCOL_NAME,
        "version": PDF_ENGINE_PROTOCOL_VERSION,
        "request_id": response.request_id().to_string(),
        "result": result,
    }))
}

pub fn decode_response(bytes: &[u8]) -> Result<EngineResponse, ProtocolError> {
    let root = decode_envelope(bytes)?;
    let request_id = required_id(&root, "request_id", RequestId::new)?;
    let result = required_object(&root, "result")?;
    match required_string(result, "type")? {
        "opened" => Ok(EngineResponse::Opened {
            request_id,
            document: parse_document_info(required_object(result, "document")?)?,
        }),
        "failed" => Ok(EngineResponse::Failed {
            request_id,
            error: EngineErrorCode::parse(required_string(result, "code")?)?,
        }),
        other => Err(ProtocolError::UnsupportedResult(other.into())),
    }
}

fn encode_envelope(value: Value) -> Result<Vec<u8>, ProtocolError> {
    serde_json::to_vec(&value).map_err(|error| ProtocolError::InvalidJson(error.to_string()))
}

fn decode_envelope(bytes: &[u8]) -> Result<Map<String, Value>, ProtocolError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|error| ProtocolError::InvalidJson(error.to_string()))?;
    let root = value.as_object().ok_or_else(|| {
        ProtocolError::InvalidMessage("the protocol envelope must be an object".into())
    })?;
    let protocol = required_string(root, "protocol")?;
    if protocol != PDF_ENGINE_PROTOCOL_NAME {
        return Err(ProtocolError::UnexpectedProtocol(protocol.into()));
    }
    let version = required_u64(root, "version")?;
    if version != u64::from(PDF_ENGINE_PROTOCOL_VERSION) {
        return Err(ProtocolError::UnsupportedVersion(version));
    }
    Ok(root.clone())
}

fn document_to_value(document: &DocumentInfo) -> Value {
    let security = match document.security {
        DocumentSecurity::Unencrypted => "unencrypted",
        DocumentSecurity::Encrypted => "encrypted",
    };
    json!({
        "page_count": document.page_count,
        "metadata": {
            "title": document.metadata.title,
            "author": document.metadata.author,
            "subject": document.metadata.subject,
            "creator": document.metadata.creator,
            "producer": document.metadata.producer,
        },
        "permissions": {
            "print": document.permissions.print,
            "copy": document.permissions.copy,
            "modify": document.permissions.modify,
            "annotate": document.permissions.annotate,
            "fill_forms": document.permissions.fill_forms,
            "accessibility": document.permissions.accessibility,
            "assemble": document.permissions.assemble,
            "high_quality_print": document.permissions.high_quality_print,
        },
        "security": security,
        "xref_reconstructed": document.xref_reconstructed,
    })
}

fn parse_document_info(value: &Map<String, Value>) -> Result<DocumentInfo, ProtocolError> {
    let metadata = required_object(value, "metadata")?;
    let permissions = required_object(value, "permissions")?;
    let security = match required_string(value, "security")? {
        "unencrypted" => DocumentSecurity::Unencrypted,
        "encrypted" => DocumentSecurity::Encrypted,
        other => {
            return Err(ProtocolError::InvalidMessage(format!(
                "unknown document security kind {other:?}"
            )));
        }
    };
    Ok(DocumentInfo {
        page_count: u32::try_from(required_u64(value, "page_count")?).map_err(|_| {
            ProtocolError::InvalidMessage("page_count exceeds the version 1 limit".into())
        })?,
        metadata: DocumentMetadata {
            title: optional_string(metadata, "title")?.map(str::to_owned),
            author: optional_string(metadata, "author")?.map(str::to_owned),
            subject: optional_string(metadata, "subject")?.map(str::to_owned),
            creator: optional_string(metadata, "creator")?.map(str::to_owned),
            producer: optional_string(metadata, "producer")?.map(str::to_owned),
        },
        permissions: DocumentPermissions {
            print: required_bool(permissions, "print")?,
            copy: required_bool(permissions, "copy")?,
            modify: required_bool(permissions, "modify")?,
            annotate: required_bool(permissions, "annotate")?,
            fill_forms: required_bool(permissions, "fill_forms")?,
            accessibility: required_bool(permissions, "accessibility")?,
            assemble: required_bool(permissions, "assemble")?,
            high_quality_print: required_bool(permissions, "high_quality_print")?,
        },
        security,
        xref_reconstructed: required_bool(value, "xref_reconstructed")?,
    })
}

fn required_object<'a>(
    value: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a Map<String, Value>, ProtocolError> {
    value
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| ProtocolError::InvalidMessage(format!("{field} must be an object")))
}

fn required_string<'a>(
    value: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, ProtocolError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| ProtocolError::InvalidMessage(format!("{field} must be a string")))
}

fn optional_string<'a>(
    value: &'a Map<String, Value>,
    field: &str,
) -> Result<Option<&'a str>, ProtocolError> {
    match value.get(field) {
        Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value)),
        _ => Err(ProtocolError::InvalidMessage(format!(
            "{field} must be a string or null"
        ))),
    }
}

fn required_id<T>(
    value: &Map<String, Value>,
    field: &str,
    wrap: impl FnOnce(u64) -> T,
) -> Result<T, ProtocolError> {
    let encoded = required_string(value, field)?;
    let parsed = encoded.parse::<u64>().map_err(|_| {
        ProtocolError::InvalidMessage(format!(
            "{field} must be a canonical decimal string containing an unsigned 64-bit integer"
        ))
    })?;
    if parsed.to_string() != encoded {
        return Err(ProtocolError::InvalidMessage(format!(
            "{field} must be a canonical decimal string containing an unsigned 64-bit integer"
        )));
    }
    Ok(wrap(parsed))
}

fn reject_unknown_fields(
    value: &Map<String, Value>,
    context: &str,
    allowed: &[&str],
) -> Result<(), ProtocolError> {
    if let Some(field) = value
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return Err(ProtocolError::InvalidMessage(format!(
            "{context} contains unsupported field {field:?}"
        )));
    }
    Ok(())
}

fn required_u64(value: &Map<String, Value>, field: &str) -> Result<u64, ProtocolError> {
    value.get(field).and_then(Value::as_u64).ok_or_else(|| {
        ProtocolError::InvalidMessage(format!("{field} must be an unsigned integer"))
    })
}

fn required_bool(value: &Map<String, Value>, field: &str) -> Result<bool, ProtocolError> {
    value
        .get(field)
        .and_then(Value::as_bool)
        .ok_or_else(|| ProtocolError::InvalidMessage(format!("{field} must be a boolean")))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::annotation_model::{BlendMode, InkTool};
    use crate::generated_document::{GeneratedDocumentRequest, GeneratedPattern};
    use crate::semantic_snapping::{PageGridKind, PageGridSource};
    use lopdf::{Document, Object, ObjectId, StringFormat, dictionary};
    use serde_json::json;

    use super::{
        DocumentInfo, DocumentMetadata, DocumentPermissions, DocumentSecurity, EmbeddedCidGlyph,
        EmbeddedTextFont, EngineErrorCode, EngineRequest, EngineResponse,
        MAX_PDFIUM_DISPLAY_OPTIONAL_CONTENT_GROUPS, OpenRequest, PDF_ENGINE_PROTOCOL_NAME,
        PDF_ENGINE_PROTOCOL_VERSION, PdfPersistenceError, RequestId, SessionId, SourceHandleId,
        TextAlignment, add_embedded_unicode_font, decode_pdf_text_string_compat, decode_request,
        decode_response, embedded_cid_mapping_from_standard_font, encode_request, encode_response,
        helvetica_text_width_pt, pdf_text_box_contents, pdfium_display_render_bytes,
        resolve_optional_object, text_appearance_line_bytes, text_appearance_line_x,
        unicode_text_line,
    };

    // Mirrors current Electron's /BPAppearance + /DA text fields. The import
    // entrypoints below cover all caption families, not only the shared helper.

    #[test]
    fn generated_page_grid_metadata_imports_and_invalid_metadata_fails_closed() {
        let request = GeneratedDocumentRequest {
            title: "Grid import".into(),
            width_mm: 210.,
            height_mm: 297.,
            pattern: Some(GeneratedPattern::SquareGrid {
                spacing_mm: 10.,
                color: "#d1d5db".into(),
            }),
        };
        let document = lopdf::Document::load_mem(&request.to_pdf_bytes().unwrap()).unwrap();
        let imported = super::import_page_grid_definition(&document).unwrap();
        assert_eq!(imported.kind, PageGridKind::Rectangular);
        assert_eq!(imported.source, PageGridSource::Generated);
        assert_eq!(
            imported.origin,
            crate::annotation_model::PdfPoint { x: 0., y: 0. }
        );
        assert!((imported.spacing - 10. * 72. / 25.4).abs() < 0.000_001);
        assert!((imported.width - 210. * 72. / 25.4).abs() < 0.000_001);
        assert!((imported.height - 297. * 72. / 25.4).abs() < 0.000_001);

        let document_with_subject = |subject: &str| {
            let mut document = lopdf::Document::with_version("1.7");
            let info = document.add_object(lopdf::dictionary! {
                "Subject" => super::pdf_literal(subject),
            });
            document.trailer.set("Info", info);
            document
        };
        for subject in [
            r#"butter-paper:page-grid:{"version":2,"type":"rectangular","origin":{"x":0,"y":0},"spacing":10,"width":100,"height":100,"rotationDegrees":0,"source":"generated"}"#,
            r#"butter-paper:page-grid:{"version":1,"type":"unknown","origin":{"x":0,"y":0},"spacing":10,"width":100,"height":100,"rotationDegrees":0,"source":"generated"}"#,
            r#"butter-paper:page-grid:{"version":1,"type":"rectangular","origin":{"x":0,"y":0},"spacing":10,"width":100,"height":100,"rotationDegrees":0,"source":"unknown"}"#,
            r#"butter-paper:page-grid:{"version":1,"type":"rectangular","origin":{"x":0,"y":0},"spacing":0.001,"width":1000,"height":1000,"rotationDegrees":0,"source":"detected"}"#,
            "butter-paper:page-grid:not-json",
        ] {
            assert!(
                super::import_page_grid_definition(&document_with_subject(subject)).is_none(),
                "invalid page-grid metadata must fail closed: {subject}"
            );
        }
    }

    #[test]
    fn standard_multiply_ink_without_subject_imports_as_highlight() {
        let dictionary = lopdf::dictionary! {
            "InkList" => Object::Array(vec![Object::Array(vec![
                Object::Real(10.), Object::Real(20.),
                Object::Real(30.), Object::Real(40.),
            ])]),
            "C" => Object::Array(vec![Object::Real(1.), Object::Real(1.), Object::Real(0.)]),
            "CA" => Object::Real(0.35),
            "BS" => lopdf::dictionary! { "W" => Object::Real(16.) },
            "BM" => Object::Name(b"Multiply".to_vec()),
        };

        let imported = super::import_pen(&dictionary, "electron-highlight".into(), 0).unwrap();

        assert_eq!(imported.tool(), InkTool::Highlight);
        assert_eq!(imported.blend_mode(), BlendMode::Multiply);
        assert!(!imported.smooth_curves);
    }

    #[test]
    fn highlight_appearance_paints_each_path_separately_with_multiply_blending() {
        use super::*;
        use crate::annotation_model::{MarkupId, PdfPoint, PenAnnotation, PenAppearance};

        let highlight = PenAnnotation::new_highlight_paths(
            MarkupId::new("highlight-appearance").unwrap(),
            0,
            vec![
                vec![
                    PdfPoint::new(10., 20.).unwrap(),
                    PdfPoint::new(50., 20.).unwrap(),
                ],
                vec![
                    PdfPoint::new(30., 10.).unwrap(),
                    PdfPoint::new(30., 40.).unwrap(),
                ],
            ],
            PenAppearance::new("#ffff00", 12., 0.35).unwrap(),
        )
        .unwrap();
        let mut document = Document::with_version("1.7");
        let appearance_id = add_pen_appearance(&mut document, &highlight);
        let stream = document
            .get_object(appearance_id)
            .unwrap()
            .as_stream()
            .unwrap();
        let content = String::from_utf8(stream.content.clone()).unwrap();
        assert_eq!(
            content.lines().filter(|line| *line == "S").count(),
            2,
            "separate Highlight paths must remain separate transparency operations"
        );
        assert_eq!(content.matches(" m\n").count(), 2);

        let resources = stream.dict.get(b"Resources").unwrap().as_dict().unwrap();
        let graphics_state = resources
            .get(b"ExtGState")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"GS0")
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            dictionary_name(graphics_state, b"BM").as_deref(),
            Some("Multiply")
        );
        assert!(
            (dictionary_float(graphics_state, b"CA").unwrap() - 0.35).abs() < 1e-6,
            "appearance opacity must survive lopdf's f32 PDF number representation"
        );

        let dictionary = pen_dictionary(
            &highlight,
            appearance_id,
            &Dictionary::new(),
            "bp:highlight-appearance",
        );
        assert_eq!(
            dictionary_string(&dictionary, b"Subj").as_deref(),
            Some("Highlight")
        );
        assert_eq!(
            dictionary_name(&dictionary, b"BM").as_deref(),
            Some("Multiply")
        );
        assert!(dictionary.get(b"BPSmoothCurves").is_err());

        let thin = PenAnnotation::new_highlight_paths(
            MarkupId::new("thin-highlight").unwrap(),
            0,
            vec![vec![
                PdfPoint::new(10., 20.).unwrap(),
                PdfPoint::new(50., 20.).unwrap(),
            ]],
            PenAppearance::new("#ffff00", 1., 0.5).unwrap(),
        )
        .unwrap();
        // Revu pads Ink by 6.5 pt plus half the stroke.
        assert_eq!(pen_bounds(&thin), PdfRect::new(3., 13., 54., 14.).unwrap());
        let thin_appearance_id = add_pen_appearance(&mut document, &thin);
        let thin_dictionary = pen_dictionary(
            &thin,
            thin_appearance_id,
            &Dictionary::new(),
            "bp:thin-highlight",
        );
        assert_eq!(
            import_pdf_rect(&thin_dictionary, b"Rect").unwrap(),
            pen_bounds(&thin),
            "annotation /Rect must use Revu's Ink padding"
        );
    }

    #[cfg(unix)]
    #[test]
    fn in_place_stage_creation_preserves_a_preexisting_collision() {
        use std::os::unix::fs::MetadataExt as _;

        let root = std::env::temp_dir().join(format!(
            "bp-in-place-stage-collision-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let stage_path = root.join("occupied-stage.tmp");
        std::fs::write(&stage_path, b"competitor-owned stage bytes").unwrap();
        let parent = std::fs::metadata(&root).unwrap();

        let error = match super::OwnedInPlaceStage::create(
            &root,
            stage_path.clone(),
            std::ffi::OsStr::new("source.pdf"),
            (parent.dev(), parent.ino()),
        ) {
            Ok(_) => panic!("exclusive stage creation must reject an occupied name"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("exists"), "{error}");
        assert_eq!(
            std::fs::read(stage_path).unwrap(),
            b"competitor-owned stage bytes"
        );
    }

    #[cfg(unix)]
    fn fixed_receipted_stage(
        root: &std::path::Path,
        stage_leaf: &str,
        target_leaf: &str,
        token: &str,
    ) -> super::OwnedInPlaceStage {
        use std::os::unix::fs::MetadataExt as _;

        let parent = std::fs::metadata(root).unwrap();
        let parent_fd = super::rfs::open(
            root,
            super::OFlags::RDONLY | super::OFlags::DIRECTORY | super::OFlags::CLOEXEC,
            super::Mode::empty(),
        )
        .unwrap();
        super::OwnedInPlaceStage::create_with_token(
            parent_fd,
            stage_leaf.into(),
            std::ffi::OsStr::new(target_leaf),
            (parent.dev(), parent.ino()),
            token.to_owned(),
        )
        .unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn receipt_lock_child_process_helper() {
        use std::io::Write as _;

        let Some(root) = std::env::var_os("BP_TEST_RECEIPT_CHILD_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let mut stage = fixed_receipted_stage(
            &root,
            "child-owned-stage.tmp",
            "source.pdf",
            "44444444444444444444444444444444",
        );
        stage.file_mut().write_all(b"child-owned bytes").unwrap();
        println!("BP_RECEIPT_CHILD_READY");
        std::io::stdout().flush().unwrap();
        loop {
            std::thread::park();
        }
    }

    #[cfg(unix)]
    #[test]
    fn sigkill_releases_receipt_lease_before_exact_recovery() {
        use std::{
            io::{BufRead as _, BufReader},
            process::{Command, Stdio},
        };

        let root = std::env::temp_dir().join(format!(
            "bp-in-place-sigkill-receipt-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "pdf_engine::tests::receipt_lock_child_process_helper",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BP_TEST_RECEIPT_CHILD_ROOT", &root)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let ready = BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .any(|line| line.contains("BP_RECEIPT_CHILD_READY"));
        assert!(
            ready,
            "the child must acquire and announce its retained lease"
        );

        let parent_fd = super::rfs::open(
            &root,
            super::OFlags::RDONLY | super::OFlags::DIRECTORY | super::OFlags::CLOEXEC,
            super::Mode::empty(),
        )
        .unwrap();
        let parent_identity = super::stat_identity(&super::rfs::fstat(&parent_fd).unwrap());
        super::recover_abandoned_in_place_stage(
            &parent_fd,
            parent_identity,
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();
        let receipt_path = root.join(super::receipt_leaf_for_token(
            "44444444444444444444444444444444",
        ));
        assert!(root.join("child-owned-stage.tmp").exists());
        assert!(receipt_path.exists());

        child.kill().unwrap();
        assert!(!child.wait().unwrap().success());
        super::recover_abandoned_in_place_stage(
            &parent_fd,
            parent_identity,
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();
        assert!(!root.join("child-owned-stage.tmp").exists());
        assert!(!receipt_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn active_in_place_receipt_lock_preserves_the_exact_stage() {
        let root = std::env::temp_dir().join(format!(
            "bp-in-place-active-receipt-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut stage = fixed_receipted_stage(
            &root,
            "active-stage.tmp",
            "source.pdf",
            "11111111111111111111111111111111",
        );
        std::io::Write::write_all(stage.file_mut(), b"active bytes").unwrap();
        let receipt_path = root.join(&stage.receipt_leaf);

        super::recover_abandoned_in_place_stage(
            &stage.parent_fd,
            super::stat_identity(&super::rfs::fstat(&stage.parent_fd).unwrap()),
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();

        assert!(root.join("active-stage.tmp").exists());
        assert!(receipt_path.exists());
        drop(stage);
        assert!(!root.join("active-stage.tmp").exists());
        assert!(!receipt_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn crash_released_receipt_recovers_only_its_authenticated_stage() {
        let root = std::env::temp_dir().join(format!(
            "bp-in-place-released-receipt-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut stage = fixed_receipted_stage(
            &root,
            "abandoned-stage.tmp",
            "source.pdf",
            "22222222222222222222222222222222",
        );
        std::io::Write::write_all(stage.file_mut(), b"abandoned bytes").unwrap();
        let receipt_path = root.join(&stage.receipt_leaf);
        let parent_fd = super::rfs::open(
            &root,
            super::OFlags::RDONLY | super::OFlags::DIRECTORY | super::OFlags::CLOEXEC,
            super::Mode::empty(),
        )
        .unwrap();
        let parent_identity = super::stat_identity(&super::rfs::fstat(&parent_fd).unwrap());
        stage.file.take();
        stage.receipt_lease.take();
        std::mem::forget(stage);

        super::recover_abandoned_in_place_stage(
            &parent_fd,
            parent_identity,
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();

        assert!(!root.join("abandoned-stage.tmp").exists());
        assert!(!receipt_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn malformed_or_substituted_in_place_receipts_preserve_files() {
        use std::io::{Seek as _, SeekFrom, Write as _};

        let root = std::env::temp_dir().join(format!(
            "bp-in-place-invalid-receipt-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut stage = fixed_receipted_stage(
            &root,
            "preserved-stage.tmp",
            "source.pdf",
            "33333333333333333333333333333333",
        );
        let receipt = stage.receipt_lease.as_mut().unwrap();
        receipt.set_len(0).unwrap();
        receipt.seek(SeekFrom::Start(0)).unwrap();
        receipt.write_all(b"{\"version\":1}").unwrap();
        receipt.sync_all().unwrap();
        let receipt_path = root.join(&stage.receipt_leaf);
        let parent_fd = super::rfs::open(
            &root,
            super::OFlags::RDONLY | super::OFlags::DIRECTORY | super::OFlags::CLOEXEC,
            super::Mode::empty(),
        )
        .unwrap();
        let parent_identity = super::stat_identity(&super::rfs::fstat(&parent_fd).unwrap());
        stage.file.take();
        stage.receipt_lease.take();
        std::mem::forget(stage);

        std::fs::remove_file(root.join("preserved-stage.tmp")).unwrap();
        std::fs::write(root.join("preserved-stage.tmp"), b"competitor bytes").unwrap();
        super::recover_abandoned_in_place_stage(
            &parent_fd,
            parent_identity,
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();

        assert_eq!(
            std::fs::read(root.join("preserved-stage.tmp")).unwrap(),
            b"competitor bytes"
        );
        assert!(receipt_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn hardlinked_stage_is_ambiguous_and_preserved() {
        let root = std::env::temp_dir().join(format!(
            "bp-in-place-hardlink-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut stage = fixed_receipted_stage(
            &root,
            "hardlinked-stage.tmp",
            "source.pdf",
            "44444444444444444444444444444444",
        );
        let receipt_path = root.join(&stage.receipt_leaf);
        std::fs::hard_link(
            root.join("hardlinked-stage.tmp"),
            root.join("unexpected-stage-link"),
        )
        .unwrap();
        let parent_fd = super::rfs::open(
            &root,
            super::OFlags::RDONLY | super::OFlags::DIRECTORY | super::OFlags::CLOEXEC,
            super::Mode::empty(),
        )
        .unwrap();
        let parent_identity = super::stat_identity(&super::rfs::fstat(&parent_fd).unwrap());
        stage.file.take();
        stage.receipt_lease.take();
        std::mem::forget(stage);

        super::recover_abandoned_in_place_stage(
            &parent_fd,
            parent_identity,
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();

        assert!(root.join("hardlinked-stage.tmp").exists());
        assert!(root.join("unexpected-stage-link").exists());
        assert!(receipt_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn recovery_after_rename_removes_only_receipt_and_never_published_target() {
        use std::io::Write as _;

        let root = std::env::temp_dir().join(format!(
            "bp-in-place-published-recovery-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("source.pdf"), b"old bytes").unwrap();
        let mut stage = fixed_receipted_stage(
            &root,
            "renamed-stage.tmp",
            "source.pdf",
            "55555555555555555555555555555555",
        );
        stage.file_mut().write_all(b"published bytes").unwrap();
        stage.file_mut().sync_all().unwrap();
        let receipt_path = root.join(&stage.receipt_leaf);
        super::rfs::renameat(
            &stage.parent_fd,
            &stage.stage_leaf,
            &stage.parent_fd,
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();
        let parent_fd = super::rfs::open(
            &root,
            super::OFlags::RDONLY | super::OFlags::DIRECTORY | super::OFlags::CLOEXEC,
            super::Mode::empty(),
        )
        .unwrap();
        let parent_identity = super::stat_identity(&super::rfs::fstat(&parent_fd).unwrap());
        stage.file.take();
        stage.receipt_lease.take();
        std::mem::forget(stage);

        super::recover_abandoned_in_place_stage(
            &parent_fd,
            parent_identity,
            std::ffi::OsStr::new("source.pdf"),
        )
        .unwrap();

        assert_eq!(
            std::fs::read(root.join("source.pdf")).unwrap(),
            b"published bytes"
        );
        assert!(!receipt_path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn in_place_stage_sync_failure_preserves_source_and_removes_owned_stage() {
        use std::{io::Write as _, os::unix::fs::MetadataExt as _};

        let root = std::env::temp_dir().join(format!(
            "bp-in-place-stage-sync-failure-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let target = root.join("source.pdf");
        let stage_path = root.join("owned-stage.tmp");
        std::fs::write(&target, b"original source bytes").unwrap();
        let parent = std::fs::metadata(&root).unwrap();
        let mut stage = super::OwnedInPlaceStage::create(
            &root,
            stage_path.clone(),
            target.file_name().unwrap(),
            (parent.dev(), parent.ino()),
        )
        .unwrap();
        stage
            .file_mut()
            .write_all(b"complete replacement bytes")
            .unwrap();

        let error = stage
            .publish_replacing_with(
                target.file_name().unwrap(),
                0o640,
                |_| Err(std::io::Error::from_raw_os_error(libc::ENOSPC)),
                |_| panic!("directory sync must not run before the stage file is durable"),
            )
            .unwrap_err();

        assert_eq!(error.to_string(), "No space left on device (os error 28)");
        assert_eq!(std::fs::read(&target).unwrap(), b"original source bytes");
        assert!(
            !stage_path.exists(),
            "a pre-publication file-sync failure must remove the exact owned stage"
        );
    }

    #[cfg(unix)]
    #[test]
    fn in_place_retained_parent_sync_failure_reports_published_warning() {
        use std::{
            io::Write as _,
            os::unix::fs::{MetadataExt as _, PermissionsExt as _},
        };

        let root = std::env::temp_dir().join(format!(
            "bp-in-place-parent-sync-failure-{}-{}",
            std::process::id(),
            super::NEXT_TEMP_FILE_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(std::path::PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let target = root.join("source.pdf");
        let stage_path = root.join("owned-stage.tmp");
        std::fs::write(&target, b"original source bytes").unwrap();
        let parent = std::fs::metadata(&root).unwrap();
        let mut stage = super::OwnedInPlaceStage::create(
            &root,
            stage_path.clone(),
            target.file_name().unwrap(),
            (parent.dev(), parent.ino()),
        )
        .unwrap();
        stage
            .file_mut()
            .write_all(b"durable replacement bytes")
            .unwrap();

        let warnings = stage
            .publish_replacing_with(
                target.file_name().unwrap(),
                0o640,
                std::fs::File::sync_all,
                |_| Err(std::io::Error::from_raw_os_error(libc::EIO)),
            )
            .unwrap();

        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"durable replacement bytes"
        );
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(!stage_path.exists());
        assert_eq!(warnings.len(), 1);
        assert!(
            warnings[0].contains("directory durability sync failed")
                && warnings[0].contains("Input/output error"),
            "{warnings:?}"
        );
    }


    #[test]
    fn measurement_caption_geometry_uses_helvetica_metrics_and_path_distance_anchor() {
        use super::*;
        let text = TextBoxStyle::new("Helvetica", 12., "#172b4d", 1.)
            .unwrap()
            .with_layout_metrics(13.8, 2.)
            .unwrap();
        let points = [
            PdfPoint::new(0., 0.).unwrap(),
            PdfPoint::new(100., 0.).unwrap(),
            PdfPoint::new(100., 10.).unwrap(),
        ];

        let midpoint = measurement_path_midpoint(&points);
        assert!((midpoint.x - 55.).abs() < 0.000_001);
        assert_eq!(midpoint.y, 0.);
        let path_caption = measurement_caption_layout(midpoint, "iiii", &text, false);
        assert_eq!(path_caption.rect, PdfRect::new(61., 6., 56., 18.).unwrap());
        assert_eq!(path_caption.text_origin, PdfPoint::new(63., 11.).unwrap());

        let area_anchor = measurement_vertex_mean(&points);
        assert!((area_anchor.x - 200. / 3.).abs() < 0.000_001);
        assert!((area_anchor.y - 10. / 3.).abs() < 0.000_001);
        let centered =
            measurement_caption_layout(PdfPoint::new(50., 24.).unwrap(), "WWW", &text, true);
        assert!((centered.rect.x - 31.008).abs() < 0.000_001);
        assert!((centered.rect.y - 17.1).abs() < 0.000_001);
        assert!((centered.rect.width - 37.984).abs() < 0.000_001);
        assert!((centered.text_origin.x - 33.008).abs() < 0.000_001);
        assert!((centered.text_origin.y - 17.9).abs() < 0.000_001);
    }

    #[test]
    fn saved_length_polylength_and_area_bounds_include_their_caption_boxes() {
        use super::*;
        let calibration = LengthCalibration::from_scale(1., 1., "m", 0, true).unwrap();
        let text = TextBoxStyle::new("Helvetica", 12., "#172b4d", 1.)
            .unwrap()
            .with_layout_metrics(13.8, 2.)
            .unwrap();
        let line = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
        let length = LengthAnnotation::new_with_appearance(
            MarkupId::new("length-caption-bounds").unwrap(),
            0,
            PdfPoint::new(0., 0.).unwrap(),
            PdfPoint::new(100., 0.).unwrap(),
            calibration.clone(),
            DimensionAppearance::new(line, text.clone()).unwrap(),
        )
        .unwrap();
        // Revu draws a Length `LL` 10 pt above its points, caption centred on
        // that line; the bounds hold the extension lines, arrowheads and caption.
        let bounds = length_bounds(&length);
        let caption = measurement_caption_layout(
            PdfPoint::new(50., 10.).unwrap(),
            &length.caption(),
            &text,
            true,
        )
        .rect;
        assert!((bounds.y + 1.).abs() < 0.000_001);
        assert!(bounds.y + bounds.height >= 14.5);
        assert!(bounds.x <= caption.x && bounds.x + bounds.width >= caption.x + caption.width);
        assert!((caption.y + caption.height * 0.5 - 10.).abs() < 0.000_001);
        assert!(bounds.y + bounds.height + 0.000_001 >= caption.y + caption.height);

        let points = vec![
            PdfPoint::new(0., 0.).unwrap(),
            PdfPoint::new(100., 0.).unwrap(),
            PdfPoint::new(100., 10.).unwrap(),
        ];
        let path_appearance =
            || RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap();
        let polylength = MeasurementPathAnnotation::new_with_text_style(
            MarkupId::new("polylength-caption-bounds").unwrap(),
            0,
            points.clone(),
            MeasurementPathKind::Polylength,
            calibration.clone(),
            path_appearance(),
            text.clone(),
        )
        .unwrap();
        assert_eq!(
            measurement_path_bounds(&polylength),
            PdfRect::new(-8., -8., 125., 32.).unwrap()
        );

        let area = MeasurementPathAnnotation::new_with_text_style(
            MarkupId::new("area-caption-bounds").unwrap(),
            0,
            points,
            MeasurementPathKind::Area,
            calibration,
            path_appearance(),
            text,
        )
        .unwrap();
        // Revu centres the Area caption on the area (here the vertex mean).
        let area_bounds = measurement_path_bounds(&area);
        let area_caption = measurement_caption_layout(
            PdfPoint::new(200. / 3., 10. / 3.).unwrap(),
            &area.caption(),
            area.text_style(),
            true,
        )
        .rect;
        assert!(
            (area_caption.x + area_caption.width / 2. - 200. / 3.).abs() < 0.000_001
                && (area_caption.y + area_caption.height / 2. - 10. / 3.).abs() < 0.000_001
        );
        assert!((area_bounds.y + 8.).abs() < 0.000_001);
        assert!(area_bounds.x <= area_caption.x.min(-8.) + 0.000_001);
        assert!(
            area_bounds.x + area_bounds.width + 0.000_001
                >= (area_caption.x + area_caption.width).max(108.)
        );
        assert!(area_bounds.y + area_bounds.height + 0.000_001 >= 18.);
    }

    #[test]
    fn dimension_appearance_bbox_text_matrix_and_line_gap_share_caption_geometry() {
        use super::*;
        let line = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
        let text = TextBoxStyle::new("Helvetica", 12., "#172b4d", 1.)
            .unwrap()
            .with_layout_metrics(13.8, 2.)
            .unwrap();
        let annotation = DimensionAnnotation::new(
            MarkupId::new("dimension-caption-geometry").unwrap(),
            0,
            PdfPoint::new(0., 0.).unwrap(),
            PdfPoint::new(100., 0.).unwrap(),
            24.,
            "WWW",
            DimensionAppearance::new(line, text).unwrap(),
        )
        .unwrap();
        // Revu's layout: tips 1 pt inside the extension lines, 7.8 pt by 9 pt
        // closed arrowheads, a 4 pt gap either side of the caption.
        let bounds = dimension_bounds(&annotation);
        assert_eq!(bounds, PdfRect::new(-1., -1., 102., 31.9).unwrap());
        let layout = dimension_line_layout(&annotation);
        assert_eq!(layout.dimension_segments.len(), 2);
        assert!((layout.dimension_segments[0].0.x - 1.).abs() < 0.000_001);
        assert!((layout.dimension_segments[0].1.x - 29.008).abs() < 0.000_001);
        assert!((layout.dimension_segments[1].0.x - 70.992).abs() < 0.000_001);
        assert!((layout.dimension_segments[1].1.x - 99.).abs() < 0.000_001);

        let mut document = Document::with_version("1.7");
        let appearance_id = add_dimension_appearance(&mut document, &annotation).unwrap();
        let stream = document
            .get_object(appearance_id)
            .unwrap()
            .as_stream()
            .unwrap();
        assert_eq!(stream.dict.get(b"BBox").unwrap(), &rect_bbox(bounds));
        let content = String::from_utf8(stream.content.clone()).unwrap();
        assert!(content.contains("1.000000 1.000000 m 1.000000 27.000000 l S"));
        assert!(content.contains("2.000000 25.000000 m 30.008000 25.000000 l S"));
        assert!(content.contains("71.992000 25.000000 m 100.000000 25.000000 l S"));
        assert!(content.contains("9.800000 29.500000 m 2.000000 25.000000 l 9.800000 20.500000 l b"));
        assert!(content.contains("1 0 0 1 34.008000 18.900000 Tm (WWW) Tj"));
    }

    #[test]
    fn sloped_measurement_captions_follow_their_line_upright() {
        use super::*;
        let point = |x, y| PdfPoint::new(x, y).unwrap();
        let degrees = |start, end| measurement_caption_angle(start, end).to_degrees();
        assert!((degrees(point(0., 0.), point(100., 100.)) - 45.).abs() < 1e-9);
        // Drawn right to left or downwards, the text still reads upright.
        assert!(degrees(point(100., 0.), point(0., 0.)).abs() < 1e-9);
        assert!((degrees(point(100., 100.), point(0., 0.)) - 45.).abs() < 1e-9);
        assert!((degrees(point(0., 100.), point(0., 0.)) - 90.).abs() < 1e-9);

        let line = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
        let text = TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.)
            .unwrap()
            .with_layout_metrics(13.8, 3.)
            .unwrap();
        let sloped = DimensionAnnotation::new(
            MarkupId::new("SLOPEDDIMENSIONA").unwrap(),
            0,
            point(0., 0.),
            point(100., 100.),
            10.,
            "WWW",
            DimensionAppearance::new(line, text).unwrap(),
        )
        .unwrap();
        let mut document = Document::with_version("1.7");
        let appearance_id = add_dimension_appearance(&mut document, &sloped).unwrap();
        let content = String::from_utf8(
            document.get_object(appearance_id).unwrap().as_stream().unwrap().content.clone(),
        )
        .unwrap();
        assert!(content.contains("q 0.707107 0.707107 -0.707107 0.707107 "));
        assert!(content.contains("ET\nQ\n"));
    }

    #[test]
    fn measurement_line_layout_matches_revu_inside_and_outside_arrows() {
        use super::*;
        let point = |x, y| PdfPoint::new(x, y).unwrap();
        // Revu's unlabelled Dimension: arrows inside, one continuous line.
        let inside = measurement_line_layout(
            point(173.5196, 729.895),
            point(249.9806, 729.895),
            10.,
            1.,
            0.,
        )
        .unwrap();
        assert_eq!(inside.extension_lines[0].1.y, 741.895);
        assert_eq!(inside.dimension_segments.len(), 1);
        assert!((inside.dimension_segments[0].0.x - 174.5196).abs() < 0.000_01);
        assert!((inside.dimension_segments[0].1.x - 248.9806).abs() < 0.000_01);
        assert!((inside.arrowheads[0][1].x - 182.3196).abs() < 0.000_01);
        assert!((inside.arrowheads[0][1].y - 744.395).abs() < 0.000_01);
        // Revu's Length whose caption fills the gap: arrows outside.
        let outside = measurement_line_layout(
            point(58.6721, 589.4564),
            point(135.1331, 589.4564),
            10.,
            1.,
            70.032,
        )
        .unwrap();
        assert!((outside.dimension_segments[0].0.x - 57.6721).abs() < 0.000_01);
        assert!((outside.dimension_segments[0].1.x - 42.0721).abs() < 0.000_01);
        assert!((outside.dimension_segments[1].1.x - 151.7331).abs() < 0.000_01);
        assert!((outside.arrowheads[0][1].x - 49.8721).abs() < 0.000_01);
        assert!((outside.arrowheads[1][1].x - 143.9331).abs() < 0.000_01);
        assert!((outside.caption_center.y - 599.4564).abs() < 0.000_01);
        // Arrowheads scale with the line width (Revu at 0.5 pt: 3.9 by 4.5).
        let thin = measurement_line_layout(point(0., 0.), point(100., 0.), 10., 0.5, 0.).unwrap();
        assert!((thin.arrowheads[0][1].x - 4.4).abs() < 0.000_001);
        assert!((thin.arrowheads[0][1].y - 12.25).abs() < 0.000_001);
    }



    fn retained_render_test_document() -> lopdf::Document {
        lopdf::Document::load_mem(
            &crate::generated_document::GeneratedDocumentRequest::a3_landscape_blank()
                .to_pdf_bytes()
                .unwrap(),
        )
        .unwrap()
    }

    fn retained_render_test_square(name: &str) -> lopdf::Dictionary {
        dictionary! {
            "Type" => "Annot", "Subtype" => "Square", "NM" => super::pdf_literal(name),
            "Rect" => Object::Array(vec![20.into(), 20.into(), 120.into(), 80.into()]),
            "C" => Object::Array(vec![1.into(), 0.into(), 0.into()]),
        }
    }

    #[test]
    fn save_pruning_removes_only_unreachable_objects_and_preserves_unknown_reachable_content() {
        let mut document = retained_render_test_document();
        let page_id = document.get_pages()[&1];
        let vendor_stream = lopdf::Stream::new(
            dictionary! { "Type" => "VendorPayload", "VendorFlag" => true },
            b"reachable unknown vendor bytes".to_vec(),
        );
        let vendor_id = document.add_object(vendor_stream.clone());
        document
            .get_object_mut(page_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("VendorPrivate", vendor_id);
        let orphan_id = document.add_object(lopdf::Stream::new(
            dictionary! { "Type" => "OrphanPayload" },
            vec![0x5a; 1024 * 1024],
        ));

        let removed = super::prune_unreachable_objects_for_save(&mut document);

        assert!(removed.contains(&orphan_id));
        assert!(!removed.contains(&vendor_id));
        assert!(document.get_object(orphan_id).is_err());
        assert_eq!(
            document.get_object(vendor_id).unwrap(),
            &Object::Stream(vendor_stream)
        );
        assert_eq!(
            document
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"VendorPrivate")
                .unwrap()
                .as_reference()
                .unwrap(),
            vendor_id
        );
        assert_eq!(
            document.max_id,
            document
                .objects
                .keys()
                .map(|(object_number, _)| *object_number)
                .max()
                .unwrap()
        );
    }

    #[test]
    fn retained_routing_obstacles_normalize_opaque_rects_and_exclude_nonvisual_slots() {
        use std::collections::{BTreeMap, HashSet};

        let mut document = retained_render_test_document();
        let page_id = document.get_pages()[&1];
        super::append_native_annotation(
            &mut document,
            0,
            dictionary! {
                "Subtype" => "Text", "NM" => super::pdf_literal("opaque-note"),
                "Rect" => Object::Array(vec![120.into(), 80.into(), 20.into(), 10.into()]),
            },
        )
        .unwrap();
        super::append_native_annotation(
            &mut document,
            0,
            dictionary! {
                "Subtype" => "Link", "NM" => super::pdf_literal("link"),
                "Rect" => Object::Array(vec![1.into(), 2.into(), 30.into(), 40.into()]),
            },
        )
        .unwrap();
        super::append_native_annotation(
            &mut document,
            0,
            dictionary! {
                "Subtype" => "Popup", "NM" => super::pdf_literal("popup"),
                "Rect" => Object::Array(vec![1.into(), 2.into(), 30.into(), 40.into()]),
            },
        )
        .unwrap();
        super::append_native_annotation(
            &mut document,
            0,
            retained_render_test_square("admitted-square"),
        )
        .unwrap();
        super::append_native_annotation(
            &mut document,
            0,
            dictionary! { "Subtype" => "Text", "NM" => super::pdf_literal("missing-rect") },
        )
        .unwrap();
        let managed = BTreeMap::from([(page_id, HashSet::from([3_usize]))]);

        let obstacles = super::retained_annotation_obstacles(&document, &managed);

        assert_eq!(obstacles.len(), 1);
        assert_eq!(obstacles[0].page_index, 0);
        assert!(obstacles[0].id.ends_with(":opaque-note"));
        assert_eq!(
            obstacles[0].rect,
            crate::annotation_model::PdfRect::new(20., 10., 100., 70.).unwrap()
        );
    }

    #[test]
    fn retained_annotation_render_widget_normal_appearance_state_and_flags() {
        use super::*;
        let mut document = retained_render_test_document();
        let page_id = document.get_pages()[&1];
        let mut appearance = dictionary! {
            "Type" => "XObject", "Subtype" => "Form",
            "BBox" => Object::Array(vec![0.into(), 0.into(), 20.into(), 10.into()]),
            "Matrix" => Object::Array(vec![2.into(), 0.into(), 0.into(), 1.into(), 3.into(), 4.into()]),
            "Resources" => Dictionary::new(),
        };
        let on = document.add_object(Stream::new(
            appearance.clone(),
            b"0 1 0 rg 0 0 20 10 re f".to_vec(),
        ));
        appearance.set("VendorProbe", pdf_literal("off appearance"));
        let off = document.add_object(Stream::new(appearance, b"1 0 0 rg 0 0 20 10 re f".to_vec()));
        let parent = document.add_object(dictionary! { "FT" => "Btn", "V" => "On", "AA" => dictionary! { "K" => pdf_literal("do not copy") } });
        let base = dictionary! {
            "Subtype" => "Widget", "Rect" => Object::Array(vec![10.into(), 20.into(), 110.into(), 70.into()]),
            "Parent" => parent, "AP" => dictionary! { "N" => dictionary! { "On" => on, "Off" => off } },
            "F" => 2 | 32, "A" => dictionary! { "S" => "JavaScript", "JS" => pdf_literal("do not copy") },
        };
        let selected = |widget: &Dictionary| {
            retained_widget_surrogate(&document, widget).map(|surrogate| {
                surrogate
                    .get(b"AP")
                    .unwrap()
                    .as_dict()
                    .unwrap()
                    .get(b"N")
                    .unwrap()
                    .clone()
            })
        };
        assert_eq!(
            selected(&base),
            Some(Object::Reference(on)),
            "immediate parent V"
        );
        let mut widget = base.clone();
        widget.set("AS", "Off");
        widget.set("V", "On");
        assert_eq!(
            selected(&widget),
            Some(Object::Reference(off)),
            "AS overrides V"
        );
        widget.set("AS", "Missing");
        assert_eq!(
            selected(&widget),
            None,
            "nonempty missing AS must not fall back"
        );
        widget.set("AS", pdf_literal(""));
        widget.set("V", pdf_literal("Off"));
        assert_eq!(
            selected(&widget),
            Some(Object::Reference(off)),
            "string V with empty AS"
        );
        widget.set("V", "Missing");
        assert_eq!(
            selected(&widget),
            Some(Object::Reference(off)),
            "missing V state resolves Off, not parent V"
        );
        widget.remove(b"V");
        widget.remove(b"Parent");
        assert_eq!(
            selected(&widget),
            Some(Object::Reference(off)),
            "no value resolves Off"
        );
        widget.set("AP", dictionary! { "N" => on });
        widget.set("AS", "Missing");
        assert_eq!(
            selected(&widget),
            Some(Object::Reference(on)),
            "direct normal stream ignores AS"
        );
        let mut popup = widget.clone();
        popup.set("Subtype", "Popup");
        assert!(retained_widget_surrogate(&document, &popup).is_none());
        let mut absent = widget.clone();
        absent.remove(b"AP");
        let fallback = retained_widget_surrogate(&document, &absent).unwrap();
        let fallback_stream = fallback
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap()
            .as_stream()
            .unwrap();
        assert!(String::from_utf8_lossy(&fallback_stream.content).contains("(Widget) Tj"));
        let widget_id = append_native_annotation(&mut document, 0, base.clone()).unwrap();
        let absent_id = append_native_annotation(&mut document, 0, absent).unwrap();
        let popup_id = append_native_annotation(&mut document, 0, popup).unwrap();
        let before = document.objects.clone();
        let RetainedAnnotationRender::Filtered(bytes) =
            retained_annotation_render(&document).unwrap()
        else {
            panic!("Widget conversion requires filtered bytes even without managed annotations");
        };
        assert_eq!(document.objects, before);
        let filtered = Document::load_mem(&bytes).unwrap();
        let entries = filtered
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[2], Object::Reference(popup_id));
        assert_ne!(entries[0], Object::Reference(widget_id));
        assert_ne!(entries[1], Object::Reference(absent_id));
        for (entry, source) in [(&entries[0], &base), (&entries[1], &base)] {
            let surrogate = resolve_object(&filtered, entry).unwrap().as_dict().unwrap();
            assert_eq!(
                dictionary_name(surrogate, b"Subtype").as_deref(),
                Some("Stamp")
            );
            assert_eq!(
                surrogate.get(b"Rect").unwrap(),
                source.get(b"Rect").unwrap()
            );
            assert_eq!(surrogate.get(b"F").unwrap(), source.get(b"F").unwrap());
            for key in [
                b"Parent".as_slice(),
                b"A".as_slice(),
                b"AA".as_slice(),
                b"FT".as_slice(),
                b"V".as_slice(),
                b"DA".as_slice(),
                b"DR".as_slice(),
            ] {
                assert!(surrogate.get(key).is_err());
            }
        }
        for id in [on, off, parent, widget_id] {
            assert_eq!(
                filtered.get_object(id).unwrap(),
                document.get_object(id).unwrap()
            );
        }
    }

    #[test]
    fn retained_annotation_render_avoids_unnecessary_document_copies() {
        use super::*;
        let mut document = retained_render_test_document();
        assert!(matches!(
            retained_annotation_render(&document).unwrap(),
            RetainedAnnotationRender::None
        ));
        let square =
            append_native_annotation(&mut document, 0, retained_render_test_square("square"))
                .unwrap();
        assert!(matches!(
            retained_annotation_render(&document).unwrap(),
            RetainedAnnotationRender::None
        ));
        remove_annotation_reference(&mut document, 0, square).unwrap();
        append_native_annotation(
            &mut document,
            0,
            dictionary! { "Subtype" => "Text", "NM" => pdf_literal("opaque") },
        )
        .unwrap();
        assert!(matches!(
            retained_annotation_render(&document).unwrap(),
            RetainedAnnotationRender::Original
        ));
    }

    #[test]
    fn retained_annotation_render_filters_exact_admitted_slots_and_preserves_unknown_content() {
        use super::*;
        let mut document = retained_render_test_document();
        let page_id = document.get_pages()[&1];
        let appearance_id = document.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => Object::Array(vec![0.into(), 0.into(), 20.into(), 20.into()]),
            "Resources" => dictionary! { "VendorResource" => pdf_literal("keep resource") },
        }, b"q 1 0 0 rg 0 0 20 20 re f Q".to_vec()));
        let unknown = dictionary! {
            "Subtype" => "Text", "NM" => pdf_literal("same-name"),
            "AP" => dictionary! { "N" => appearance_id }, "VendorUnknown" => pdf_literal("keep me"),
        };
        let unknown_id = append_native_annotation(&mut document, 0, unknown.clone()).unwrap();
        let square_id =
            append_native_annotation(&mut document, 0, retained_render_test_square("same-name"))
                .unwrap();
        let inline = Object::Dictionary(
            dictionary! { "Subtype" => "Text", "AP" => dictionary! { "N" => appearance_id } },
        );
        let elliptical_id = append_native_annotation(
            &mut document,
            0,
            dictionary! {
                "Subtype" => "Circle", "IT" => "CircleArc", "NM" => pdf_literal("bp:elliptical"),
                "Rect" => Object::Array(vec![0.into(), 0.into(), 220.into(), 110.into()]),
                "AP" => dictionary! { "N" => appearance_id },
            },
        )
        .unwrap();
        let shared_array = document.add_object(Object::Array(vec![
            Object::Reference(unknown_id),
            Object::Reference(square_id),
            inline.clone(),
            Object::Reference(elliptical_id),
        ]));
        document
            .get_object_mut(page_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Annots", Object::Reference(shared_array));
        let before = document.objects.clone();
        let RetainedAnnotationRender::Filtered(bytes) =
            retained_annotation_render(&document).unwrap()
        else {
            panic!("mixed annotations need filtered render bytes");
        };
        assert_eq!(
            document.objects, before,
            "render preparation must not mutate source objects"
        );
        let filtered = Document::load_mem(&bytes).unwrap();
        let retained = filtered
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(retained, &vec![Object::Reference(unknown_id), inline]);
        assert_eq!(
            filtered.get_object(shared_array).unwrap(),
            document.get_object(shared_array).unwrap(),
            "shared indirect arrays are not rewritten"
        );
        for id in [unknown_id, square_id, elliptical_id, appearance_id] {
            assert_eq!(
                filtered.get_object(id).unwrap(),
                document.get_object(id).unwrap()
            );
        }
        assert_eq!(
            filtered.get_page_content(page_id),
            document.get_page_content(page_id)
        );
        assert_eq!(
            filtered
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Resources")
                .ok(),
            document
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Resources")
                .ok()
        );
    }

    fn persistence_order_test_cloud_plus() -> crate::annotation_model::CloudPlusAnnotation {
        use super::*;
        CloudPlusAnnotation::new(
            MarkupId::new("paired").unwrap(),
            0,
            vec![
                PdfPoint::new(10., 10.).unwrap(),
                PdfPoint::new(100., 10.).unwrap(),
                PdfPoint::new(100., 100.).unwrap(),
            ],
            1.,
            vec![
                PdfPoint::new(100., 100.).unwrap(),
                PdfPoint::new(125., 150.).unwrap(),
                PdfPoint::new(150., 150.).unwrap(),
            ],
            PdfRect::new(150., 150., 100., 40.).unwrap(),
            "pair",
            CloudPlusAppearance::new(
                RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
                StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap(),
                TextBoxStyle::new("Helvetica", 12., "#ff0000", 1.).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn cloud_plus_external_appearance_path_maps_bbox_matrix_and_content_cm_to_page_space() {
        use super::*;
        let mut document = retained_render_test_document();
        let appearance = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 20.into(), 10.into()],
                "Matrix" => vec![2.into(), 0.into(), 0.into(), 1.into(), 4.into(), 6.into()],
            },
            b"q 1 0 0 1 1 1 cm 0 0 m 5 0 15 0 18 0 c 18 8 l 0 8 l h S Q".to_vec(),
        ));
        let annotation = dictionary! {
            "Rect" => vec![100.into(), 200.into(), 180.into(), 240.into()],
            "AP" => dictionary! { "N" => appearance },
        };
        let path = import_cloud_plus_appearance_path(&document, &annotation)
            .unwrap()
            .unwrap();
        assert_eq!(
            path[0],
            CloudAppearancePathCommand::MoveTo(PdfPoint::new(104., 204.).unwrap())
        );
        assert_eq!(
            path[1],
            CloudAppearancePathCommand::CubicTo {
                control_1: PdfPoint::new(124., 204.).unwrap(),
                control_2: PdfPoint::new(164., 204.).unwrap(),
                end: PdfPoint::new(176., 204.).unwrap(),
            }
        );
        assert_eq!(path.last(), Some(&CloudAppearancePathCommand::Close));
    }

    #[test]
    fn cloud_plus_unsupported_appearance_keeps_both_physical_members_untouched() {
        use super::*;
        for content in [
            "0 0 m 20 20 40 20 60 0 v h S",
            // Fill without stroke (a filled Cloud+ fills and strokes).
            "0 0 m 60 0 l 60 60 l 0 60 l h f",
            "0 0 m 60 0 l 60 60 l 0 60 l h W n",
            "q 0 0 m 60 0 l 60 60 l 0 60 l h S",
            "0 0 m 60 0 l 60 60 l 0 60 l h S /Foreign Do",
        ] {
            let mut session = PdfPersistenceSession::from_document(
                PathBuf::new(),
                retained_render_test_document(),
                None,
            )
            .unwrap();
            let cloud_plus = persistence_order_test_cloud_plus();
            session.add_cloud_plus(cloud_plus.clone()).unwrap();
            let identity = session.cloud_plus_native_identities[&cloud_plus.id].clone();
            let appearance_id = normal_appearance_object_id(
                session
                    .document
                    .get_object(identity.cloud_object_id)
                    .unwrap()
                    .as_dict()
                    .unwrap(),
            )
            .unwrap();
            session
                .document
                .get_object_mut(appearance_id)
                .unwrap()
                .as_stream_mut()
                .unwrap()
                .set_plain_content(content.as_bytes().to_vec());

            let imported = import_annotations(&session.document, &BTreeMap::new()).unwrap();
            assert!(
                imported.cloud_pluses.is_empty(),
                "unexpectedly imported {content}"
            );
            assert_eq!(imported.untouched.len(), 2);
            assert!(
                imported
                    .untouched
                    .iter()
                    .any(|annotation| annotation.name == identity.cloud_raw_name)
            );
            assert!(
                imported
                    .untouched
                    .iter()
                    .any(|annotation| annotation.name == identity.text_raw_name)
            );
        }
    }

    #[test]
    fn cloud_plus_generated_path_reopens_as_derived_geometry() {
        use super::*;
        let mut session = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        let cloud_plus = persistence_order_test_cloud_plus();
        assert!(cloud_plus.cloud_appearance_path().is_none());
        session.add_cloud_plus(cloud_plus.clone()).unwrap();

        let imported = import_annotations(&session.document, &BTreeMap::new()).unwrap();
        assert_eq!(imported.cloud_pluses.len(), 1);
        assert!(imported.cloud_pluses[0].cloud_appearance_path().is_none());
        assert!(imported.cloud_pluses[0].same_persisted_state_as(&cloud_plus));
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn cloud_plus_external_cubic_path_survives_caption_edit_save_and_reopen() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "bp-cloud-plus-custom-path-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let source = root.join("source.pdf");
        let first = root.join("first.pdf");
        let second = root.join("second.pdf");
        let seeded = persistence_order_test_cloud_plus();
        let mut seed = PdfPersistenceSession::from_document(
            source.clone(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        seed.add_cloud_plus(seeded.clone()).unwrap();
        let identity = seed.cloud_plus_native_identities[&seeded.id].clone();
        let appearance_id = normal_appearance_object_id(
            seed.document
                .get_object(identity.cloud_object_id)
                .unwrap()
                .as_dict()
                .unwrap(),
        )
        .unwrap();
        let appearance = seed
            .document
            .get_object_mut(appearance_id)
            .unwrap()
            .as_stream_mut()
            .unwrap();
        let bbox = import_pdf_rect(&appearance.dict, b"BBox").unwrap();
        appearance.set_plain_content(
            format!(
                "q 1 0 0 1 0 0 cm 1 1 m {:.6} 0 {:.6} 0 {:.6} 1 c {:.6} {:.6} l 1 {:.6} l h S Q",
                bbox.width * 0.25,
                bbox.width * 0.75,
                bbox.width - 1.,
                bbox.width - 1.,
                bbox.height - 1.,
                bbox.height - 1.,
            )
            .into_bytes(),
        );
        seed.document.save(&source).unwrap();

        let mut opened = PdfPersistenceSession::open(&source).unwrap();
        let retained = opened.cloud_pluses()[0].clone();
        assert!(retained.cloud_appearance_path().is_some());
        assert_ne!(retained.scallop_path(), seeded.scallop_path());
        let edited = CloudPlusAnnotation::new(
            retained.id.clone(),
            retained.page_index,
            retained.cloud_points().to_vec(),
            retained.border_effect_intensity(),
            retained.leader_points().to_vec(),
            retained.text_box,
            "edited caption only",
            retained.appearance.clone(),
        )
        .unwrap()
        .with_cloud_appearance_path(retained.cloud_appearance_path().map(|path| path.to_vec()))
        .unwrap();
        opened.replace_cloud_plus(edited.clone()).unwrap();
        let authority = SaveAsTargetAuthority::bind(first.clone(), &source).unwrap();
        opened
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();
        let mut reopened = PdfPersistenceSession::open(&first).unwrap();
        assert!(reopened.cloud_pluses()[0].same_persisted_state_as(&edited));
        let moved = Annotation::CloudPlus(reopened.cloud_pluses()[0].clone())
            .translated_copy(edited.id.clone(), 0, 11., -7.)
            .unwrap();
        let Annotation::CloudPlus(moved) = moved else {
            unreachable!()
        };
        reopened.replace_cloud_plus(moved.clone()).unwrap();
        let authority = SaveAsTargetAuthority::bind(second.clone(), &first).unwrap();
        reopened
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();
        let final_reopen = PdfPersistenceSession::open(&second).unwrap();
        assert!(final_reopen.cloud_pluses()[0].same_persisted_state_as(&moved));
        assert!(final_reopen.cloud_plus_has_canonical_native_identity(&moved.id));
    }

    #[test]
    fn retained_annotation_render_filters_both_cloud_plus_members_only_after_admission() {
        use super::*;
        let mut session = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        let cloud = persistence_order_test_cloud_plus();
        session.add_cloud_plus(cloud).unwrap();
        let opaque_id = append_native_annotation(
            &mut session.document,
            0,
            dictionary! { "Subtype" => "Text", "NM" => pdf_literal("bp:paired:cloud") },
        )
        .unwrap();
        let page_id = session.document.get_pages()[&1];
        let RetainedAnnotationRender::Filtered(bytes) =
            retained_annotation_render(&session.document).unwrap()
        else {
            panic!("admitted pair must be filtered");
        };
        let filtered = Document::load_mem(&bytes).unwrap();
        assert_eq!(
            filtered
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![Object::Reference(opaque_id)]
        );
        let pair_id = session
            .cloud_plus_native_identities
            .values()
            .next()
            .unwrap()
            .text_object_id;
        session
            .document
            .get_object_mut(pair_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .remove(b"CL");
        assert!(
            matches!(
                retained_annotation_render(&session.document).unwrap(),
                RetainedAnnotationRender::Original
            ),
            "invalid pair must remain entirely opaque"
        );
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn supported_span_rich_text_is_editable_while_unsupported_semantics_stay_opaque() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "bp-rich-text-opaque-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let source = root.join("source.pdf");
        let output = root.join("output.pdf");
        let mut seed = PdfPersistenceSession::from_document(
            source.clone(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        let style = TextBoxStyle::new("Arimo", 12., "#172b4d", 1.).unwrap();
        for (id, y, text) in [
            ("rich-runs", 120., "Normal bold italic"),
            ("electron-rich", 90., "Normal bold italic"),
            ("plain-rc", 60., "Plain editable text"),
            ("semantic-rich", 10., "Unsupported strong text"),
        ] {
            seed.add_text_box(
                TextBoxAnnotation::new(
                    MarkupId::new(id).unwrap(),
                    0,
                    PdfRect::new(40., y, 240., 40.).unwrap(),
                    text,
                    style.clone(),
                )
                .unwrap(),
            )
            .unwrap();
        }
        let rich_id = annotation_object_id(&seed.document, 0, "rich-runs").unwrap();
        let electron_id = annotation_object_id(&seed.document, 0, "electron-rich").unwrap();
        let plain_id = annotation_object_id(&seed.document, 0, "plain-rc").unwrap();
        let semantic_id = annotation_object_id(&seed.document, 0, "semantic-rich").unwrap();
        seed.document
            .get_object_mut(rich_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "RC",
                pdf_literal(
                    r#"<body><p><span style="font-family:Arimo">Normal </span><span style="font-family:Arimo;font-weight:bold">bold </span><span style="font-family:Arimo;font-style:italic">italic</span></p></body>"#,
                ),
            );
        seed.document
            .get_object_mut(electron_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "RC",
                pdf_literal(
                    r##"<?xml version="1.0"?><body xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/" xfa:contentType="text/html" xfa:APIVersion="BluebeamPDFRevu:2018" xfa:spec="2.2.0" style="font: Arimo 12pt; text-align:left; margin:2pt; line-height:14.4pt; color:#172B4D" xmlns="http://www.w3.org/1999/xhtml"><p><span style="font-family:Arimo; font-size:12pt; color:#172B4D">Normal </span><span style="font-family:Arimo; font-size:12pt; color:#172B4D; font-weight:bold">bold </span><span style="font-family:Arimo; font-size:12pt; color:#172B4D; font-style:italic">italic</span></p></body>"##,
                ),
            );
        seed.document
            .get_object_mut(plain_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("RC", pdf_literal("<body><p>Plain editable text</p></body>"));
        seed.document
            .get_object_mut(semantic_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "RC",
                pdf_literal("<body><p>Unsupported <strong>strong</strong> text</p></body>"),
            );
        seed.document.save(&source).unwrap();

        let source_document = Document::load(&source).unwrap();
        let semantic_before = source_document.get_object(semantic_id).unwrap().clone();
        let semantic_appearance_id =
            normal_appearance_object_id(semantic_before.as_dict().unwrap()).unwrap();
        let semantic_appearance_before = source_document
            .get_object(semantic_appearance_id)
            .unwrap()
            .clone();
        let mut session = PdfPersistenceSession::open(&source).unwrap();
        assert_eq!(
            session.text_boxes().len(),
            3,
            "{:?} untouched {:?}",
            session.text_boxes().iter().map(|value| value.id.as_str()).collect::<Vec<_>>(),
            session.untouched_annotations()
        );
        let rich = session
            .text_boxes()
            .iter()
            .find(|annotation| annotation.id.as_str() == "rich-runs")
            .unwrap();
        assert_eq!(rich.rich_text_runs().len(), 3);
        assert!(rich.rich_text_runs()[1].bold());
        assert!(rich.rich_text_runs()[2].italic());
        let electron = session
            .text_boxes()
            .iter()
            .find(|annotation| annotation.id.as_str() == "electron-rich")
            .expect("Electron rich text must remain editable in GPUI");
        assert_eq!(electron.rich_text_runs().len(), 3);
        // Spans that repeat the box style resolve to it rather than overriding it.
        let first = &electron.rich_text_runs()[0];
        assert_eq!(first.font_family().unwrap_or(electron.style().font_family()), "Arimo");
        assert_eq!(first.font_size_pt().unwrap_or(electron.style().font_size_pt()), 12.);
        assert_eq!(first.color().unwrap_or(electron.style().color()), "#172b4d");
        assert!(electron.rich_text_runs()[1].bold());
        assert!(electron.rich_text_runs()[2].italic());
        assert_eq!(session.untouched_annotations().len(), 1);
        assert_eq!(session.untouched_annotations()[0].name, "semantic-rich");
        assert_eq!(session.untouched_annotations()[0].subtype, "FreeText");

        let mut rich = rich.clone();
        rich.layout_rect.x += 9.;
        session.replace_text_box(rich.clone()).unwrap();
        let authority = SaveAsTargetAuthority::bind(output.clone(), &source).unwrap();
        session
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();

        let reopened = PdfPersistenceSession::open(&output).unwrap();
        let reopened_rich = reopened
            .text_boxes()
            .iter()
            .find(|annotation| annotation.id == rich.id)
            .unwrap();
        assert!(reopened_rich.same_persisted_state_as(&rich));
        assert_eq!(
            reopened.document.get_object(semantic_id).unwrap(),
            &semantic_before
        );
        assert_eq!(
            reopened
                .document
                .get_object(semantic_appearance_id)
                .unwrap(),
            &semantic_appearance_before
        );
    }

    #[test]
    #[cfg(any(unix, windows))]
    #[ignore = "requires BP_EXTERNAL_FONT_FIXTURE pointing to the disposable Revu-saved compatibility corpus"]
    fn external_revu_font_fixture_exercises_indirect_and_appearance_state_resources() {
        use super::*;
        let fixture = std::env::var_os("BP_EXTERNAL_FONT_FIXTURE")
            .map(PathBuf::from)
            .expect("BP_EXTERNAL_FONT_FIXTURE must name the Revu-saved compatibility fixture");
        assert_eq!(
            format!("{:x}", Sha256::digest(std::fs::read(&fixture).unwrap())),
            "24234d0204da3f5992efda7006b66a5f10e488c19376ed9c4eb2712b4806b42c",
            "the external corpus must remain the reviewed Electron-produced, Revu-saved seed"
        );
        let root = std::env::temp_dir().join(format!(
            "bp-external-font-corpus-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let derived = root.join("revu-standard-font-resources.pdf");
        let output = root.join("native-edited.pdf");
        let mut document = Document::load(&fixture).unwrap();
        let arimo_id = annotation_object_id(&document, 0, "bp:font-1").unwrap();
        let tinos_id = annotation_object_id(&document, 0, "bp:font-4").unwrap();

        for object_id in [arimo_id, tinos_id] {
            let annotation = document
                .get_object_mut(object_id)
                .unwrap()
                .as_dict_mut()
                .unwrap();
            for key in [
                b"BPAppearance".as_slice(),
                b"BPFontFamily".as_slice(),
                b"BPFontWeight".as_slice(),
                b"DS".as_slice(),
                b"RC".as_slice(),
            ] {
                annotation.remove(key);
            }
        }

        let mut arimo_dr = document
            .get_object(arimo_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"DR")
            .unwrap()
            .as_dict()
            .unwrap()
            .clone();
        let arimo_fonts = arimo_dr.get(b"Font").unwrap().as_dict().unwrap().clone();
        let arimo_fonts_id = document.add_object(Object::Dictionary(arimo_fonts));
        arimo_dr.set("Font", Object::Reference(arimo_fonts_id));
        let arimo_dr_id = document.add_object(Object::Dictionary(arimo_dr));
        document
            .get_object_mut(arimo_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("DR", Object::Reference(arimo_dr_id));

        let tinos_normal = document
            .get_object(tinos_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"AP")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"N")
            .unwrap()
            .clone();
        let tinos = document
            .get_object_mut(tinos_id)
            .unwrap()
            .as_dict_mut()
            .unwrap();
        tinos.remove(b"DR");
        tinos.set("AS", Object::Name(b"On".to_vec()));
        tinos.set(
            "AP",
            dictionary! { "N" => dictionary! {
                "Off" => tinos_normal.clone(),
                "On" => tinos_normal,
            } },
        );
        document.save(&derived).unwrap();

        let mut session = PdfPersistenceSession::open(&derived).unwrap();
        let arimo = session
            .text_boxes()
            .iter()
            .find(|annotation| annotation.content() == "Arial compatible: Architecture 123")
            .unwrap()
            .clone();
        assert_eq!(arimo.style().font_family(), "Arimo");
        let tinos = session
            .text_boxes()
            .iter()
            .find(|annotation| annotation.content() == "Times New Roman compatible: Review")
            .unwrap();
        assert_eq!(tinos.style().font_family(), "Tinos");

        let mut moved = arimo;
        moved.layout_rect.x += 7.;
        moved.layout_rect.y -= 5.;
        session.replace_text_box(moved.clone()).unwrap();
        let authority = SaveAsTargetAuthority::bind(output.clone(), &derived).unwrap();
        session
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();

        let reopened = PdfPersistenceSession::open(&output).unwrap();
        assert!(
            reopened
                .text_boxes()
                .iter()
                .any(|annotation| annotation.same_persisted_state_as(&moved))
        );
        assert_eq!(
            reopened
                .text_boxes()
                .iter()
                .find(|annotation| annotation.content() == "Times New Roman compatible: Review")
                .unwrap()
                .style()
                .font_family(),
            "Tinos"
        );
    }

    #[test]
    fn pdfium_display_render_neutralises_only_valid_view_state_on() {
        let mut document = Document::with_version("1.7");
        let pages = document.new_object_id();
        let page = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        document.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let direct_on = document.add_object(dictionary! {
            "Type" => "OCG",
            "Usage" => dictionary! { "View" => dictionary! { "ViewState" => "ON" } },
        });
        let indirect_view = document.add_object(dictionary! { "ViewState" => "ON" });
        let view_on = document.add_object(dictionary! {
            "Type" => "OCG", "Usage" => dictionary! { "View" => indirect_view },
        });
        let indirect_usage = document.add_object(dictionary! {
            "View" => dictionary! { "ViewState" => "ON" },
        });
        let usage_on = document.add_object(dictionary! {
            "Type" => "OCG", "Usage" => indirect_usage,
        });
        let view_off = document.add_object(dictionary! {
            "Type" => "OCG",
            "Usage" => dictionary! { "View" => dictionary! { "ViewState" => "OFF" } },
        });
        let malformed = document.add_object(dictionary! {
            "Type" => "OCG",
            "Usage" => dictionary! { "View" => dictionary! { "ViewState" => 1 } },
        });
        let catalog = document.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages,
            "OCProperties" => dictionary! {
                "OCGs" => vec![direct_on.into(), view_on.into(), usage_on.into(), view_off.into(), malformed.into()],
                "D" => dictionary! { "BaseState" => "OFF" },
            },
        });
        document.trailer.set("Root", catalog);
        let mut source_bytes = Vec::new();
        document.save_to(&mut source_bytes).unwrap();

        let display_bytes = pdfium_display_render_bytes(&document, source_bytes.clone()).unwrap();
        let display = Document::load_mem(&display_bytes).unwrap();
        let view_state = |document: &Document, group_id: ObjectId| {
            let group = document.get_object(group_id).unwrap().as_dict().unwrap();
            let usage = resolve_optional_object(document, group.get(b"Usage").unwrap())
                .unwrap()
                .as_dict()
                .unwrap();
            resolve_optional_object(document, usage.get(b"View").unwrap())
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"ViewState")
                .ok()
                .cloned()
        };
        for group_id in [direct_on, view_on, usage_on] {
            assert_eq!(view_state(&display, group_id), None);
            assert_eq!(
                view_state(&document, group_id),
                Some(Object::Name(b"ON".to_vec())),
                "render preparation must not mutate the source object graph",
            );
        }
        assert_eq!(
            view_state(&display, view_off),
            Some(Object::Name(b"OFF".to_vec()))
        );
        assert_eq!(view_state(&display, malformed), Some(Object::Integer(1)));

        let unchanged = pdfium_display_render_bytes(&display, display_bytes.clone()).unwrap();
        assert_eq!(
            unchanged, display_bytes,
            "documents without valid ViewState ON must retain their exact supplied bytes",
        );
    }

    #[test]
    fn pdfium_display_render_rejects_unbounded_optional_content_groups() {
        let mut document = Document::with_version("1.7");
        let pages = document.add_object(dictionary! {
            "Type" => "Pages", "Kids" => Object::Array(Vec::new()), "Count" => 0,
        });
        let group = document.add_object(dictionary! { "Type" => "OCG" });
        let catalog = document.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages,
            "OCProperties" => dictionary! {
                "OCGs" => Object::Array(
                    std::iter::repeat_n(Object::Reference(group), MAX_PDFIUM_DISPLAY_OPTIONAL_CONTENT_GROUPS + 1)
                        .collect()
                ),
            },
        });
        document.trailer.set("Root", catalog);
        let error = pdfium_display_render_bytes(&document, Vec::new()).unwrap_err();
        assert!(matches!(error, PdfPersistenceError::InvalidDocument(_)));
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn retained_render_evaluates_ocmd_membership_policies_and_hides_unknown_semantics() {
        use super::*;
        let mut session = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        let on_group = session.document.add_object(dictionary! {
            "Type" => "OCG", "Name" => pdf_literal("Explicitly enabled layer"),
        });
        let off_group = session.document.add_object(dictionary! {
            "Type" => "OCG", "Name" => pdf_literal("Default disabled layer"),
        });
        let default_off_group = session.document.add_object(dictionary! {
            "Type" => "OCG", "Name" => pdf_literal("BaseState enabled layer"),
        });
        let unknown_group = session.document.add_object(dictionary! {
            "Type" => "OCG", "Name" => pdf_literal("Unregistered layer"),
        });
        let view_off_enabled_group = session.document.add_object(dictionary! {
            "Type" => "OCG",
            "Name" => pdf_literal("Enabled but hidden for display"),
            "Usage" => dictionary! {
                "View" => dictionary! { "ViewState" => "OFF" },
            },
        });
        let view_on_default_off_group = session.document.add_object(dictionary! {
            "Type" => "OCG",
            "Name" => pdf_literal("Display-on but disabled by configuration"),
            "Usage" => dictionary! {
                "View" => dictionary! { "ViewState" => "ON" },
            },
        });
        let view_on_enabled_group = session.document.add_object(dictionary! {
            "Type" => "OCG",
            "Name" => pdf_literal("Enabled display layer"),
            "Usage" => dictionary! {
                "View" => dictionary! { "ViewState" => "ON" },
            },
        });
        let malformed_view_group = session.document.add_object(dictionary! {
            "Type" => "OCG",
            "Name" => pdf_literal("Malformed display layer"),
            "Usage" => dictionary! {
                "View" => dictionary! { "ViewState" => "VendorState" },
            },
        });
        let catalog_id = session
            .document
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        session
            .document
            .get_object_mut(catalog_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "OCProperties",
                dictionary! {
                    "OCGs" => Object::Array(vec![
                        on_group.into(),
                        off_group.into(),
                        default_off_group.into(),
                        view_off_enabled_group.into(),
                        view_on_default_off_group.into(),
                        view_on_enabled_group.into(),
                        malformed_view_group.into(),
                    ]),
                    "D" => dictionary! {
                        "BaseState" => "OFF",
                        "ON" => Object::Array(vec![
                            on_group.into(),
                            view_off_enabled_group.into(),
                            view_on_enabled_group.into(),
                        ]),
                        "Order" => Object::Array(vec![
                            on_group.into(),
                            off_group.into(),
                            default_off_group.into(),
                            view_off_enabled_group.into(),
                            view_on_default_off_group.into(),
                            view_on_enabled_group.into(),
                            malformed_view_group.into(),
                        ]),
                        "AS" => Object::Array(vec![dictionary! {
                            "Event" => "View",
                            "OCGs" => Object::Array(vec![view_on_default_off_group.into()]),
                            "Category" => Object::Array(vec![Object::Name(b"View".to_vec())]),
                        }.into()]),
                    },
                },
            );

        let membership = |document: &mut Document, policy: Option<&str>| {
            let mut dictionary = dictionary! {
                "Type" => "OCMD",
                "OCGs" => Object::Array(vec![on_group.into(), off_group.into()]),
            };
            if let Some(policy) = policy {
                dictionary.set("P", Object::Name(policy.as_bytes().to_vec()));
            }
            document.add_object(dictionary)
        };
        let any_on = membership(&mut session.document, None);
        let all_on = membership(&mut session.document, Some("AllOn"));
        let any_off = membership(&mut session.document, Some("AnyOff"));
        let all_off = membership(&mut session.document, Some("AllOff"));
        let unknown_policy = membership(&mut session.document, Some("VendorPolicy"));
        let expression = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "OCGs" => Object::Array(vec![on_group.into(), off_group.into()]),
            "P" => "AnyOn",
            "VE" => Object::Array(vec![
                Object::Name(b"And".to_vec()),
                on_group.into(),
                off_group.into(),
            ]),
        });
        let visible_expression = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "OCGs" => Object::Array(vec![on_group.into(), off_group.into()]),
            "P" => "AllOn",
            "VE" => Object::Array(vec![
                Object::Name(b"And".to_vec()),
                Object::Array(vec![
                    Object::Name(b"Or".to_vec()),
                    off_group.into(),
                    on_group.into(),
                ]),
                Object::Array(vec![
                    Object::Name(b"Not".to_vec()),
                    off_group.into(),
                ]),
            ]),
        });
        let malformed_not = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "P" => "AnyOn",
            "VE" => Object::Array(vec![
                Object::Name(b"Not".to_vec()),
                off_group.into(),
                on_group.into(),
            ]),
        });
        let malformed_single_operand = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "VE" => Object::Array(vec![
                Object::Name(b"And".to_vec()),
                on_group.into(),
            ]),
        });
        let oversized_expression = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "VE" => Object::Array(
                std::iter::once(Object::Name(b"Or".to_vec()))
                    .chain(std::iter::repeat_n(on_group.into(), 257))
                    .collect(),
            ),
        });
        let mut too_deep_expression =
            Object::Array(vec![Object::Name(b"Not".to_vec()), off_group.into()]);
        for _ in 0..10 {
            too_deep_expression =
                Object::Array(vec![Object::Name(b"Not".to_vec()), too_deep_expression]);
        }
        let too_deep = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "P" => "AnyOn",
            "VE" => too_deep_expression,
        });
        let cyclic_expression = session.document.add_object(Object::Null);
        *session.document.get_object_mut(cyclic_expression).unwrap() = Object::Array(vec![
            Object::Name(b"And".to_vec()),
            on_group.into(),
            cyclic_expression.into(),
        ]);
        let cyclic = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "P" => "AnyOn",
            "VE" => cyclic_expression,
        });
        let unknown_expression_group = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "P" => "AnyOn",
            "VE" => Object::Array(vec![
                Object::Name(b"Or".to_vec()),
                unknown_group.into(),
            ]),
        });
        let unknown_expression_operator = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "P" => "AnyOn",
            "VE" => Object::Array(vec![
                Object::Name(b"VendorOperator".to_vec()),
                on_group.into(),
            ]),
        });
        let unknown_member = session.document.add_object(dictionary! {
            "Type" => "OCMD",
            "OCGs" => Object::Array(vec![on_group.into(), unknown_group.into()]),
            "P" => "AnyOn",
        });

        let mut add_optional_square = |name: &str, optional_content: ObjectId| {
            let mut square = retained_render_test_square(name);
            square.set("OC", optional_content);
            append_native_annotation(&mut session.document, 0, square).unwrap()
        };
        let direct_on = add_optional_square("bp:ocg-on", on_group);
        let _default_off = add_optional_square("bp:ocg-default-off", default_off_group);
        let _direct_off = add_optional_square("bp:ocg-off", off_group);
        let _view_off_enabled =
            add_optional_square("bp:ocg-view-off-enabled", view_off_enabled_group);
        let _view_on_default_off =
            add_optional_square("bp:ocg-view-on-default-off", view_on_default_off_group);
        let view_on_enabled = add_optional_square("bp:ocg-view-on-enabled", view_on_enabled_group);
        let _malformed_view = add_optional_square("bp:ocg-malformed-view", malformed_view_group);
        let any_on = add_optional_square("bp:ocmd-any-on", any_on);
        let _all_on = add_optional_square("bp:ocmd-all-on", all_on);
        let any_off = add_optional_square("bp:ocmd-any-off", any_off);
        let _all_off = add_optional_square("bp:ocmd-all-off", all_off);
        let _unknown_policy = add_optional_square("bp:ocmd-unknown-policy", unknown_policy);
        let _expression = add_optional_square("bp:ocmd-expression", expression);
        let visible_expression =
            add_optional_square("bp:ocmd-visible-expression", visible_expression);
        let _malformed_not = add_optional_square("bp:ocmd-malformed-not", malformed_not);
        let _malformed_single_operand =
            add_optional_square("bp:ocmd-single-operand", malformed_single_operand);
        let _oversized_expression = add_optional_square("bp:ocmd-oversized", oversized_expression);
        let _too_deep = add_optional_square("bp:ocmd-too-deep", too_deep);
        let _cyclic = add_optional_square("bp:ocmd-cyclic-expression", cyclic);
        let _unknown_expression_group =
            add_optional_square("bp:ocmd-unknown-expression-group", unknown_expression_group);
        let _unknown_expression_operator = add_optional_square(
            "bp:ocmd-unknown-expression-operator",
            unknown_expression_operator,
        );
        let _unknown_member = add_optional_square("bp:ocmd-unknown-member", unknown_member);

        let source_objects_before_render = session.document.objects.clone();
        let RetainedAnnotationRender::Filtered(bytes) =
            retained_annotation_render(&session.document).unwrap()
        else {
            panic!("optional-content evaluation must produce render-only filtered bytes");
        };
        let filtered = Document::load_mem(&bytes).unwrap();
        let page_id = filtered.get_pages()[&1];
        let retained = filtered
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(
            retained,
            &vec![
                Object::Reference(direct_on),
                Object::Reference(view_on_enabled),
                Object::Reference(any_on),
                Object::Reference(any_off),
                Object::Reference(visible_expression),
            ],
            "configured-visible OCGs and the Electron/PDF.js OCMD policy and valid /VE cases remain rendered; default-OFF, hidden or malformed cases are removed only from the render clone",
        );
        assert_eq!(
            session
                .document
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            22,
            "optional-content filtering must not alter persistence source annotations",
        );
        assert_eq!(
            session.document.objects, source_objects_before_render,
            "optional-content evaluation must leave the source PDF object graph byte-for-byte represented by the same objects",
        );
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn optional_content_annotations_and_cloud_plus_pairs_remain_opaque_after_save() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "bp-optional-content-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let source = root.join("source.pdf");
        let output = root.join("output.pdf");
        let mut seed = PdfPersistenceSession::from_document(
            source.clone(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        let layer_id = seed.document.add_object(dictionary! {
            "Type" => "OCG", "Name" => pdf_literal("Hidden review layer"),
        });
        let catalog_id = seed
            .document
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        seed.document
            .get_object_mut(catalog_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "OCProperties",
                dictionary! {
                    "OCGs" => Object::Array(vec![Object::Reference(layer_id)]),
                    "D" => dictionary! {
                        "OFF" => Object::Array(vec![Object::Reference(layer_id)]),
                        "Order" => Object::Array(vec![Object::Reference(layer_id)]),
                    },
                },
            );
        seed.add_cloud_plus(persistence_order_test_cloud_plus())
            .unwrap();
        let pair = seed
            .cloud_plus_native_identities
            .values()
            .next()
            .unwrap()
            .clone();
        seed.document
            .get_object_mut(pair.cloud_object_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("OC", layer_id);
        let hidden_dictionary = dictionary! {
            "Type" => "Annot", "Subtype" => "Square", "NM" => pdf_literal("bp:hidden"),
            "Rect" => Object::Array(vec![20.into(), 20.into(), 120.into(), 80.into()]),
            "C" => Object::Array(vec![1.into(), 0.into(), 0.into()]),
            "OC" => layer_id, "VendorProbe" => pdf_literal("retain-hidden"),
        };
        let hidden_id =
            append_native_annotation(&mut seed.document, 0, hidden_dictionary.clone()).unwrap();
        let visible_id = append_native_annotation(
            &mut seed.document,
            0,
            retained_render_test_square("bp:visible"),
        )
        .unwrap();
        seed.document.save(&source).unwrap();

        let mut session = PdfPersistenceSession::open(&source).unwrap();
        assert_eq!(session.rectangles().len(), 1);
        assert_eq!(session.rectangles()[0].id.as_str(), "bp:visible");
        assert!(session.cloud_pluses().is_empty());
        assert_eq!(
            session
                .untouched_annotations()
                .iter()
                .map(|annotation| (annotation.name.as_str(), annotation.subtype.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (pair.cloud_raw_name.as_str(), "Polygon"),
                (pair.text_raw_name.as_str(), "FreeText"),
                ("bp:hidden", "Square"),
            ]
        );
        let page_id = session.document.get_pages()[&1];
        let RetainedAnnotationRender::Filtered(bytes) =
            retained_annotation_render(&session.document).unwrap()
        else {
            panic!("the visible Rectangle must be filtered from opaque render bytes");
        };
        let filtered = Document::load_mem(&bytes).unwrap();
        let retained = filtered
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        // The cloud member and standalone square explicitly belong to the
        // default-off OCG. The companion FreeText has no /OC and remains visible.
        assert_eq!(retained, &vec![Object::Reference(pair.text_object_id)]);

        let mut visible = session.rectangles()[0].clone();
        visible.rect.x += 8.;
        session.replace_rectangle(visible.clone()).unwrap();
        let authority = SaveAsTargetAuthority::bind(output.clone(), &source).unwrap();
        session
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();
        let reopened = PdfPersistenceSession::open(&output).unwrap();
        assert!(reopened.rectangles()[0].same_persisted_state_as(&visible));
        assert!(reopened.cloud_pluses().is_empty());
        assert_eq!(
            reopened.document.get_object(hidden_id).unwrap(),
            &Object::Dictionary(hidden_dictionary)
        );
        let catalog = reopened
            .document
            .get_object(catalog_id)
            .unwrap()
            .as_dict()
            .unwrap();
        let optional_content = catalog.get(b"OCProperties").unwrap().as_dict().unwrap();
        assert_eq!(
            optional_content.get(b"OCGs").unwrap().as_array().unwrap(),
            &vec![Object::Reference(layer_id)]
        );
        assert_eq!(
            optional_content
                .get(b"D")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"OFF")
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![Object::Reference(layer_id)]
        );
        assert!(
            reopened
                .document
                .get_object(pair.cloud_object_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"OC")
                .is_ok()
        );
        assert_eq!(
            reopened
                .document
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![
                Object::Reference(pair.cloud_object_id),
                Object::Reference(pair.text_object_id),
                Object::Reference(hidden_id),
                Object::Reference(visible_id),
            ]
        );
    }

    #[test]
    fn optional_content_on_cloud_plus_text_member_keeps_both_members_opaque() {
        use super::*;
        let mut session = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        session
            .add_cloud_plus(persistence_order_test_cloud_plus())
            .unwrap();
        let pair = session
            .cloud_plus_native_identities
            .values()
            .next()
            .unwrap()
            .clone();
        let layer_id = session.document.add_object(dictionary! {
            "Type" => "OCG", "Name" => pdf_literal("Hidden review layer"),
        });
        session
            .document
            .get_object_mut(pair.text_object_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("OC", layer_id);

        let imported = import_annotations(&session.document, &BTreeMap::new()).unwrap();
        assert!(imported.cloud_pluses.is_empty());
        assert!(imported.cloud_plus_native_identities.is_empty());
        assert!(imported.annotation_order.is_empty());
        assert!(imported.managed_annotation_slots.is_empty());
        assert_eq!(
            imported
                .untouched
                .iter()
                .map(|annotation| (annotation.name.as_str(), annotation.subtype.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (pair.cloud_raw_name.as_str(), "Polygon"),
                (pair.text_raw_name.as_str(), "FreeText"),
            ]
        );
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn popup_linked_annotations_and_cloud_plus_pairs_remain_opaque_after_save() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "bp-popup-linked-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let source = root.join("source.pdf");
        let output = root.join("output.pdf");
        let mut seed = PdfPersistenceSession::from_document(
            source.clone(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        seed.add_cloud_plus(persistence_order_test_cloud_plus())
            .unwrap();
        let pair = seed
            .cloud_plus_native_identities
            .values()
            .next()
            .unwrap()
            .clone();
        let parent_id = append_native_annotation(
            &mut seed.document,
            0,
            dictionary! {
                "Type" => "Annot", "Subtype" => "Square", "NM" => pdf_literal("bp:popup-parent"),
                "Rect" => Object::Array(vec![20.into(), 20.into(), 120.into(), 80.into()]),
                "C" => Object::Array(vec![1.into(), 0.into(), 0.into()]),
                "VendorProbe" => pdf_literal("retain-parent"),
            },
        )
        .unwrap();
        let parent_popup_id = append_native_annotation(
            &mut seed.document,
            0,
            dictionary! {
                "Type" => "Annot", "Subtype" => "Popup", "Parent" => parent_id,
                "Rect" => Object::Array(vec![120.into(), 20.into(), 240.into(), 100.into()]),
                "VendorProbe" => pdf_literal("retain-parent-popup"),
            },
        )
        .unwrap();
        seed.document
            .get_object_mut(parent_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Popup", parent_popup_id);
        let pair_popup_id = append_native_annotation(
            &mut seed.document,
            0,
            dictionary! {
                "Type" => "Annot", "Subtype" => "Popup", "Parent" => pair.cloud_object_id,
                "Rect" => Object::Array(vec![120.into(), 100.into(), 300.into(), 210.into()]),
                "VendorProbe" => pdf_literal("retain-pair-popup"),
            },
        )
        .unwrap();
        seed.document
            .get_object_mut(pair.cloud_object_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Popup", pair_popup_id);
        let visible_id = append_native_annotation(
            &mut seed.document,
            0,
            retained_render_test_square("bp:visible-popup-control"),
        )
        .unwrap();
        let opaque_ids = [
            pair.cloud_object_id,
            pair.text_object_id,
            parent_id,
            parent_popup_id,
            pair_popup_id,
        ];
        let page_id = seed.document.get_pages()[&1];
        let source_content = seed.document.get_page_content(page_id);
        seed.document.save(&source).unwrap();
        let source_document = Document::load(&source).unwrap();
        let opaque_before = opaque_ids.map(|id| source_document.get_object(id).unwrap().clone());

        let mut session = PdfPersistenceSession::open(&source).unwrap();
        assert_eq!(session.rectangles().len(), 1);
        assert_eq!(
            session.rectangles()[0].id.as_str(),
            "bp:visible-popup-control"
        );
        assert!(session.cloud_pluses().is_empty());
        assert_eq!(session.untouched_annotations().len(), 5);
        let RetainedAnnotationRender::Filtered(bytes) =
            retained_annotation_render(&session.document).unwrap()
        else {
            panic!("the unrelated Rectangle must be filtered from opaque render bytes");
        };
        let filtered = Document::load_mem(&bytes).unwrap();
        assert_eq!(
            filtered
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap(),
            &opaque_ids.map(Object::Reference)
        );

        let mut visible = session.rectangles()[0].clone();
        visible.rect.x += 8.;
        session.replace_rectangle(visible.clone()).unwrap();
        let authority = SaveAsTargetAuthority::bind(output.clone(), &source).unwrap();
        session
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();
        let reopened = PdfPersistenceSession::open(&output).unwrap();
        assert!(reopened.rectangles()[0].same_persisted_state_as(&visible));
        assert!(reopened.cloud_pluses().is_empty());
        for (id, expected) in opaque_ids.into_iter().zip(opaque_before) {
            assert_eq!(reopened.document.get_object(id).unwrap(), &expected);
        }
        assert_eq!(reopened.document.get_page_content(page_id), source_content);
        assert_eq!(
            reopened
                .document
                .get_object(parent_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Popup")
                .unwrap()
                .as_reference()
                .unwrap(),
            parent_popup_id
        );
        assert_eq!(
            reopened
                .document
                .get_object(parent_popup_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Parent")
                .unwrap()
                .as_reference()
                .unwrap(),
            parent_id
        );
        assert_eq!(
            reopened
                .document
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![
                Object::Reference(pair.cloud_object_id),
                Object::Reference(pair.text_object_id),
                Object::Reference(parent_id),
                Object::Reference(parent_popup_id),
                Object::Reference(pair_popup_id),
                Object::Reference(visible_id),
            ]
        );
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn widget_form_tree_survives_unrelated_edit_while_rendering_selected_static_appearance() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "bp-widget-form-tree-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let source = root.join("source.pdf");
        let output = root.join("output.pdf");
        let mut seed = PdfPersistenceSession::from_document(
            source.clone(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        let font_id = seed.document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let field_id = seed.document.add_object(dictionary! {
            "FT" => "Btn", "T" => pdf_literal("site-reference"),
            "V" => "Yes", "DV" => "Off",
            "DA" => pdf_literal("/Helv 12 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font_id } },
            "AA" => dictionary! { "K" => pdf_literal("retain-field-action") },
            "VendorProbe" => pdf_literal("retain-field"),
        });
        let yes_appearance_id = seed.document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => Object::Array(vec![0.into(), 0.into(), 120.into(), 30.into()]),
                "Resources" => Dictionary::new(),
                "VendorProbe" => pdf_literal("retain-yes-appearance"),
            },
            b"0 1 0 rg 0 0 120 30 re f".to_vec(),
        ));
        let off_appearance_id = seed.document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => Object::Array(vec![0.into(), 0.into(), 120.into(), 30.into()]),
                "Resources" => Dictionary::new(),
                "VendorProbe" => pdf_literal("retain-off-appearance"),
            },
            b"1 1 0 rg 0 0 120 30 re f".to_vec(),
        ));
        let widget_id = append_native_annotation(
            &mut seed.document,
            0,
            dictionary! {
                "Type" => "Annot", "Subtype" => "Widget", "Parent" => field_id,
                "Rect" => Object::Array(vec![30.into(), 100.into(), 150.into(), 130.into()]),
                "F" => 4,
                "AP" => dictionary! { "N" => dictionary! {
                    "Yes" => yes_appearance_id, "Off" => off_appearance_id,
                } },
                "A" => dictionary! { "S" => "JavaScript", "JS" => pdf_literal("retain-widget-action") },
                "VendorProbe" => pdf_literal("retain-widget"),
            },
        )
        .unwrap();
        seed.document
            .get_object_mut(field_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Kids", Object::Array(vec![Object::Reference(widget_id)]));
        let acro_form_id = seed.document.add_object(dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field_id)]),
            "NeedAppearances" => true,
            "DA" => pdf_literal("/Helv 12 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font_id } },
            "VendorProbe" => pdf_literal("retain-acro-form"),
        });
        let catalog_id = seed
            .document
            .trailer
            .get(b"Root")
            .unwrap()
            .as_reference()
            .unwrap();
        seed.document
            .get_object_mut(catalog_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("AcroForm", acro_form_id);
        let visible_id = append_native_annotation(
            &mut seed.document,
            0,
            retained_render_test_square("bp:visible-form-control"),
        )
        .unwrap();
        seed.document.save(&source).unwrap();
        let source_document = Document::load(&source).unwrap();
        let preserved = [
            catalog_id,
            acro_form_id,
            field_id,
            widget_id,
            yes_appearance_id,
            off_appearance_id,
            font_id,
        ]
        .map(|id| source_document.get_object(id).unwrap().clone());

        let mut session = PdfPersistenceSession::open(&source).unwrap();
        assert_eq!(session.rectangles().len(), 1);
        assert_eq!(session.untouched_annotations().len(), 1);
        let RetainedAnnotationRender::Filtered(bytes) =
            retained_annotation_render(&session.document).unwrap()
        else {
            panic!("the Widget appearance must be isolated in filtered render bytes");
        };
        if let Some(output) =
            std::env::var_os("BP_WIDGET_FORM_RENDER_FIXTURE_OUTPUT").map(PathBuf::from)
        {
            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(output, &bytes).unwrap();
        }
        let filtered = Document::load_mem(&bytes).unwrap();
        let page_id = filtered.get_pages()[&1];
        let entries = filtered
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_ne!(entries[0], Object::Reference(widget_id));
        let surrogate = resolve_object(&filtered, &entries[0])
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            dictionary_name(surrogate, b"Subtype").as_deref(),
            Some("Stamp")
        );
        assert_eq!(
            surrogate
                .get(b"AP")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"N")
                .unwrap()
                .as_reference()
                .unwrap(),
            yes_appearance_id,
            "missing Widget /AS must select the immediate parent /V state"
        );
        for key in [
            b"Parent".as_slice(),
            b"A".as_slice(),
            b"AA".as_slice(),
            b"FT".as_slice(),
            b"V".as_slice(),
            b"DA".as_slice(),
            b"DR".as_slice(),
        ] {
            assert!(
                surrogate.get(key).is_err(),
                "render surrogate must strip {}",
                String::from_utf8_lossy(key)
            );
        }
        assert_eq!(
            filtered.get_object(acro_form_id).unwrap(),
            source_document.get_object(acro_form_id).unwrap()
        );

        let mut visible = session.rectangles()[0].clone();
        visible.rect.x += 8.;
        session.replace_rectangle(visible.clone()).unwrap();
        let authority = SaveAsTargetAuthority::bind(output.clone(), &source).unwrap();
        session
            .prepare_save_authorized(&authority)
            .unwrap()
            .publish()
            .unwrap();
        let reopened = PdfPersistenceSession::open(&output).unwrap();
        assert!(reopened.rectangles()[0].same_persisted_state_as(&visible));
        for ((id, expected), label) in [
            catalog_id,
            acro_form_id,
            field_id,
            widget_id,
            yes_appearance_id,
            off_appearance_id,
            font_id,
        ]
        .into_iter()
        .zip(preserved)
        .zip([
            "catalogue",
            "AcroForm",
            "field",
            "Widget",
            "Yes AP",
            "Off AP",
            "font resource",
        ]) {
            assert_eq!(
                reopened.document.get_object(id).unwrap(),
                &expected,
                "{label} dictionary must remain exact"
            );
        }
        assert_eq!(
            reopened
                .document
                .get_object(catalog_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"AcroForm")
                .unwrap()
                .as_reference()
                .unwrap(),
            acro_form_id
        );
        assert_eq!(
            reopened
                .document
                .get_object(field_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Kids")
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![Object::Reference(widget_id)]
        );
        assert_eq!(
            reopened
                .document
                .get_object(widget_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Parent")
                .unwrap()
                .as_reference()
                .unwrap(),
            field_id
        );
        assert_eq!(
            reopened
                .document
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![Object::Reference(widget_id), Object::Reference(visible_id)]
        );

        let reopened_objects = reopened.document.objects.clone();
        let RetainedAnnotationRender::Filtered(reopened_render_bytes) =
            retained_annotation_render(&reopened.document).unwrap()
        else {
            panic!("the reopened Widget appearance must remain isolated in render bytes");
        };
        assert_eq!(reopened.document.objects, reopened_objects);
        let reopened_render = Document::load_mem(&reopened_render_bytes).unwrap();
        let reopened_entries = reopened_render
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Annots")
            .unwrap()
            .as_array()
            .unwrap();
        assert_eq!(reopened_entries.len(), 1);
        assert_ne!(reopened_entries[0], Object::Reference(widget_id));
        let reopened_surrogate = resolve_object(&reopened_render, &reopened_entries[0])
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            dictionary_name(reopened_surrogate, b"Subtype").as_deref(),
            Some("Stamp")
        );
        let reopened_widget = reopened
            .document
            .get_object(widget_id)
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            reopened_surrogate.get(b"Rect").unwrap(),
            reopened_widget.get(b"Rect").unwrap()
        );
        assert_eq!(
            reopened_surrogate.get(b"F").unwrap(),
            reopened_widget.get(b"F").unwrap()
        );
        assert_eq!(
            reopened_surrogate
                .get(b"AP")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"N")
                .unwrap()
                .as_reference()
                .unwrap(),
            yes_appearance_id,
            "reopened render clone must retain the parent-selected Yes appearance"
        );
        for key in [
            b"Parent".as_slice(),
            b"A".as_slice(),
            b"AA".as_slice(),
            b"FT".as_slice(),
            b"V".as_slice(),
            b"DA".as_slice(),
            b"DR".as_slice(),
        ] {
            assert!(
                reopened_surrogate.get(key).is_err(),
                "reopened render surrogate must strip {}",
                String::from_utf8_lossy(key)
            );
        }
    }

    #[test]
    fn popup_link_on_cloud_plus_text_member_keeps_both_members_opaque() {
        use super::*;
        let mut session = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        session
            .add_cloud_plus(persistence_order_test_cloud_plus())
            .unwrap();
        let pair = session
            .cloud_plus_native_identities
            .values()
            .next()
            .unwrap()
            .clone();
        let popup_id = session.document.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Popup", "Parent" => pair.text_object_id,
        });
        session
            .document
            .get_object_mut(pair.text_object_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Popup", popup_id);

        let imported = import_annotations(&session.document, &BTreeMap::new()).unwrap();
        assert!(imported.cloud_pluses.is_empty());
        assert!(imported.cloud_plus_native_identities.is_empty());
        assert!(imported.annotation_order.is_empty());
        assert!(imported.managed_annotation_slots.is_empty());
        assert_eq!(imported.untouched.len(), 2);
    }

    #[test]
    fn retained_annotation_render_propagates_ambiguous_managed_identity() {
        use super::*;
        let mut document = retained_render_test_document();
        append_native_annotation(&mut document, 0, retained_render_test_square("duplicate"))
            .unwrap();
        append_native_annotation(&mut document, 0, retained_render_test_square("duplicate"))
            .unwrap();
        assert!(retained_annotation_render(&document).is_err());
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn persistence_order_unchanged_cloud_plus_preserves_separated_members_on_save() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "bp-physical-order-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let source = root.join("source.pdf");
        let mut seed = PdfPersistenceSession::from_document(
            source.clone(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        seed.add_cloud_plus(persistence_order_test_cloud_plus())
            .unwrap();
        let pair = seed
            .cloud_plus_native_identities
            .values()
            .next()
            .unwrap()
            .clone();
        let opaque_rect = PdfRect::new(24., 36., 96., 48.).unwrap();
        let opaque_id = append_native_annotation(
            &mut seed.document,
            0,
            dictionary! {
                "Subtype" => "Text", "NM" => pdf_literal("opaque"),
                "Rect" => Object::Array(vec![24.into(), 36.into(), 120.into(), 84.into()]),
                "VendorProbe" => pdf_literal("retain")
            },
        )
        .unwrap();
        let rectangle_id = append_native_annotation(
            &mut seed.document,
            0,
            retained_render_test_square("rectangle"),
        )
        .unwrap();
        let page_id = seed.document.get_pages()[&1];
        // Text before cloud is also valid: import admits the logical pair at its
        // first member, without authorising regrouping of physical members.
        let original_order = vec![
            Object::Reference(pair.text_object_id),
            Object::Reference(opaque_id),
            Object::Reference(pair.cloud_object_id),
            Object::Reference(rectangle_id),
        ];
        seed.document
            .get_object_mut(page_id)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Annots", Object::Array(original_order.clone()));
        seed.document.save(&source).unwrap();
        let mut session = PdfPersistenceSession::open(&source).unwrap();
        assert_eq!(
            session.retained_annotation_obstacles(),
            &[RetainedAnnotationObstacle {
                id: "opaque:0:1:opaque".into(),
                page_index: 0,
                rect: opaque_rect,
            }]
        );
        let logical = session.annotation_order().to_vec();
        assert_eq!(
            logical.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
            vec!["paired", "rectangle"]
        );
        let mut previous = source;
        for phase in 0..2 {
            if phase == 1 {
                let mut rectangle = session.rectangles()[0].clone();
                rectangle.rect.x += 12.;
                session.replace_rectangle(rectangle).unwrap();
            }
            session.reorder_managed_annotations(&logical).unwrap();
            let target = root.join(format!("saved-{phase}.pdf"));
            let authority = SaveAsTargetAuthority::bind(target.clone(), &previous).unwrap();
            assert_eq!(
                session
                    .prepare_save_authorized(&authority)
                    .unwrap()
                    .publish()
                    .unwrap(),
                PdfPublicationOutcome::Durable
            );
            session = PdfPersistenceSession::open(&target).unwrap();
            let page = session
                .document
                .get_object(page_id)
                .unwrap()
                .as_dict()
                .unwrap();
            assert_eq!(
                resolve_object(&session.document, page.get(b"Annots").unwrap())
                    .unwrap()
                    .as_array()
                    .unwrap(),
                &original_order
            );
            assert_eq!(session.annotation_order(), logical.as_slice());
            assert_eq!(session.retained_annotation_obstacles()[0].rect, opaque_rect);
            assert_eq!(
                session.document.get_object(opaque_id).unwrap(),
                seed.document.get_object(opaque_id).unwrap()
            );
            previous = target;
        }
        let reversed = logical.iter().rev().cloned().collect::<Vec<_>>();
        session.reorder_managed_annotations(&reversed).unwrap();
        let page = session
            .document
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            resolve_object(&session.document, page.get(b"Annots").unwrap())
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![
                Object::Reference(rectangle_id),
                Object::Reference(opaque_id),
                Object::Reference(pair.cloud_object_id),
                Object::Reference(pair.text_object_id)
            ],
            "explicit logical reorder retains established grouping semantics"
        );
    }

    #[test]
    fn canonical_name_collision_ignores_unreachable_rewrite_debris_but_rejects_live_annotations() {
        use super::*;

        let mut document = retained_render_test_document();
        let owned = append_native_annotation(
            &mut document,
            0,
            dictionary! {
                "Type" => "Annot", "Subtype" => "Line",
                "NM" => pdf_literal("bp:coordinate-length"),
                "Rect" => Object::Array(vec![10.into(), 10.into(), 100.into(), 30.into()]),
            },
        )
        .unwrap();
        let _unreachable = document.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Line",
            "NM" => pdf_literal("bp:coordinate-length"),
            "Rect" => Object::Array(vec![0.into(), 0.into(), 1.into(), 1.into()]),
        });
        require_available_native_name(&document, "bp:coordinate-length", owned).unwrap();

        append_native_annotation(
            &mut document,
            0,
            dictionary! {
                "Type" => "Annot", "Subtype" => "Line",
                "NM" => pdf_literal("bp:coordinate-length"),
                "Rect" => Object::Array(vec![120.into(), 10.into(), 200.into(), 30.into()]),
            },
        )
        .unwrap();
        assert!(
            require_available_native_name(&document, "bp:coordinate-length", owned)
                .unwrap_err()
                .to_string()
                .contains("belongs to another object")
        );
    }

    #[test]
    #[cfg(any(unix, windows))]
    fn arc_compat_elliptical_arcs_import_typed_and_survive_unrelated_edits_exactly() {
        use super::*;
        let root = std::env::temp_dir().join(format!(
            "bp-arc-compat-{}-{}",
            std::process::id(),
            NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let source = root.join("source.pdf");
        let bytes = crate::generated_document::GeneratedDocumentRequest::a3_landscape_blank()
            .to_pdf_bytes()
            .unwrap();
        let mut document = Document::load_mem(&bytes).unwrap();
        let page_id = document.get_pages()[&1];
        let base_content = document.get_page_content(page_id);
        let mut retained = Vec::new();
        for (name, width, angle1, angle2) in [
            ("elliptical-half", 220, 0, 180),
            ("elliptical-wide", 220, 20, 260),
            ("circular", 110, 20, 260),
        ] {
            let appearance = Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Form", "FormType" => 1,
                    "BBox" => Object::Array(vec![0.into(), 0.into(), width.into(), 110.into()]),
                    "Resources" => Dictionary::new(),
                },
                b"q 1 0 0 RG 2 w 220 55 m 220 85.3757 170.751 110 110 110 c S Q".to_vec(),
            );
            let appearance_id = document.add_object(appearance);
            let dictionary = dictionary! {
                "Type" => "Annot", "Subtype" => "Circle", "IT" => "CircleArc",
                "NM" => pdf_literal(name), "Rect" => Object::Array(vec![0.into(), 0.into(), width.into(), 110.into()]),
                "Angle1" => angle1, "Angle2" => angle2,
                "C" => Object::Array(vec![1.into(), 0.into(), 0.into()]), "F" => 4,
                "AP" => dictionary! { "N" => appearance_id },
                "VendorUnknown" => pdf_literal("retain elliptical source"),
            };
            let object_id = append_native_annotation(&mut document, 0, dictionary.clone()).unwrap();
            retained.push((object_id, dictionary, appearance_id));
        }
        let rectangle_dictionary = dictionary! {
            "Type" => "Annot", "Subtype" => "Square", "NM" => pdf_literal("rectangle"),
            "Rect" => Object::Array(vec![
                Object::Real(285.89746), Object::Real(157.36861),
                Object::Real(514.10254), Object::Real(352.6314),
            ]),
            "Rotation" => 30,
            "C" => Object::Array(vec![0.into(), 0.into(), 1.into()]),
            "BS" => dictionary! { "W" => 2, "S" => "D", "D" => Object::Array(vec![8.into(), 4.into()]) },
            "CA" => Object::Real(0.75), "VendorUnknown" => pdf_literal("retain rectangle source"),
        };
        append_native_annotation(&mut document, 0, rectangle_dictionary.clone()).unwrap();
        document.save(&source).unwrap();
        let source_document = Document::load(&source).unwrap();
        let mut session = PdfPersistenceSession::open(&source).unwrap();
        assert_eq!(session.arcs().len(), 3);
        assert_eq!(session.untouched_annotations().len(), 0);
        assert_eq!(session.arcs()[0].id.as_str(), "elliptical-half");
        // The 1 pt default border is painted inside `/Rect`.
        assert_eq!(
            session.arcs()[0].rect(),
            PdfRect::new(0.5, 0.5, 219., 109.).unwrap()
        );
        assert_eq!(session.arcs()[0].angle1_degrees(), 0.);
        assert_eq!(session.arcs()[0].angle2_degrees(), 180.);
        assert_eq!(session.arcs()[1].id.as_str(), "elliptical-wide");
        assert_eq!(
            session.arcs()[1].rect(),
            PdfRect::new(0.5, 0.5, 219., 109.).unwrap()
        );
        assert_eq!(session.arcs()[1].angle1_degrees(), 20.);
        assert_eq!(session.arcs()[1].angle2_degrees(), 260.);
        assert_eq!(session.arcs()[2].id.as_str(), "circular");
        let mut rectangle = session.rectangles()[0].clone();
        // Without an appearance, the unrotated size is recovered from the
        // rotated `/Rect` to f32 precision.
        let recovered = rectangle.rect;
        for (actual, expected) in [
            (recovered.x, 300.),
            (recovered.y, 200.),
            (recovered.width, 200.),
            (recovered.height, 110.),
        ] {
            assert!((actual - expected).abs() < 1e-3, "{recovered:?}");
        }
        assert_eq!(rectangle.rotation_degrees, 30.);
        assert_eq!(rectangle.appearance.stroke_style(), StrokeStyle::Dashed);
        assert_eq!(rectangle.appearance.opacity(), 0.75);
        let original_appearance = rectangle.appearance.clone();
        rectangle.rect.x += 10.;
        session.replace_rectangle(rectangle.clone()).unwrap();
        let target = root.join("saved.pdf");
        let authority = SaveAsTargetAuthority::bind(target.clone(), &source).unwrap();
        assert_eq!(
            session
                .prepare_save_authorized(&authority)
                .unwrap()
                .publish()
                .unwrap(),
            PdfPublicationOutcome::Durable
        );
        let reopened = PdfPersistenceSession::open(&target).unwrap();
        assert!(
            reopened.rectangles()[0].same_persisted_state_as(&rectangle),
            "{:?} != {rectangle:?}",
            reopened.rectangles()[0]
        );
        assert_eq!(reopened.rectangles()[0].appearance, original_appearance);
        assert_eq!(reopened.arcs(), session.arcs());
        assert_eq!(
            reopened.untouched_annotations(),
            session.untouched_annotations()
        );
        assert_eq!(reopened.document.get_page_content(page_id), base_content);
        let page = reopened
            .document
            .get_object(page_id)
            .unwrap()
            .as_dict()
            .unwrap();
        let annotations = resolve_object(&reopened.document, page.get(b"Annots").unwrap())
            .unwrap()
            .as_array()
            .unwrap();
        for (object_id, dictionary, appearance_id) in retained {
            assert!(annotations.contains(&Object::Reference(object_id)));
            assert_eq!(
                reopened
                    .document
                    .get_object(object_id)
                    .unwrap()
                    .as_dict()
                    .unwrap(),
                &dictionary
            );
            let before = source_document
                .get_object(appearance_id)
                .unwrap()
                .as_stream()
                .unwrap();
            let after = reopened
                .document
                .get_object(appearance_id)
                .unwrap()
                .as_stream()
                .unwrap();
            assert_eq!(after.dict, before.dict);
            assert_eq!(after.content, before.content);
        }
        let mut edited_session = PdfPersistenceSession::open(&target).unwrap();
        let edited_arc = edited_session
            .arcs()
            .iter()
            .find(|arc| arc.id.as_str() == "elliptical-wide")
            .unwrap()
            .translated(12., 8.)
            .unwrap();
        edited_session.replace_arc(edited_arc.clone()).unwrap();
        let edited_target = root.join("edited.pdf");
        let edited_authority = SaveAsTargetAuthority::bind(edited_target.clone(), &target).unwrap();
        edited_session
            .prepare_save_authorized(&edited_authority)
            .unwrap()
            .publish()
            .unwrap();
        let edited_reopened = PdfPersistenceSession::open(&edited_target).unwrap();
        let reopened_arc = edited_reopened
            .arcs()
            .iter()
            .find(|arc| arc.id == edited_arc.id)
            .unwrap();
        assert!(reopened_arc.same_persisted_state_as(&edited_arc));
        assert_eq!(
            reopened_arc.rect(),
            PdfRect::new(12.5, 8.5, 219., 109.).unwrap()
        );
        let edited_object_id =
            annotation_object_id(&edited_reopened.document, 0, "elliptical-wide").unwrap();
        let edited_dictionary = edited_reopened
            .document
            .get_object(edited_object_id)
            .unwrap()
            .as_dict()
            .unwrap();
        let edited_appearance =
            normal_appearance_stream(&edited_reopened.document, edited_dictionary).unwrap();
        assert_eq!(
            edited_appearance
                .dict
                .get(b"BBox")
                .unwrap()
                .as_array()
                .unwrap(),
            &vec![
                Object::Real(11.5),
                Object::Real(7.5),
                Object::Real(232.5),
                Object::Real(118.5)
            ]
        );
    }

    #[test]
    fn image_aspect_rounding_restores_thin_signatures_without_changing_pdf_edges() {
        use super::{PdfRect, restore_image_aspect_after_pdf_rounding};
        for ratio in [128., 1. / 128.] {
            let original = PdfRect::new(123.456, 456.789, 128., 128. / ratio).unwrap();
            let left = f64::from(original.x as f32);
            let bottom = f64::from(original.y as f32);
            let right = f64::from((original.x + original.width) as f32);
            let top = f64::from((original.y + original.height) as f32);
            let rounded = PdfRect::new(left, bottom, right - left, top - bottom).unwrap();
            let restored = restore_image_aspect_after_pdf_rounding(rounded, ratio);
            assert!(restored.same_pdf_geometry_as(rounded));
            assert!((restored.width / restored.height - ratio).abs() < 1e-10);
        }
        let distorted = PdfRect::new(100., 200., 128., 2.).unwrap();
        assert_eq!(
            restore_image_aspect_after_pdf_rounding(distorted, 128.),
            distorted
        );
    }

    #[test]
    fn rotated_image_appearance_roundtrips_nominal_geometry_without_aabb_growth() {
        use super::*;

        let expected = ImageAnnotation::new_with_opacity(
            MarkupId::new("rotation-image").unwrap(),
            0,
            PdfRect::new(100., 200., 120., 60.).unwrap(),
            DecodedRgbaAsset::new(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]).unwrap(),
            false,
            0.75,
        )
        .unwrap()
        .with_rotation_degrees(30.)
        .unwrap();

        let export = |annotation: &ImageAnnotation| {
            let mut document = Document::with_version("1.7");
            let (appearance_id, image_id) = add_image_appearance(&mut document, annotation);
            let dictionary =
                image_dictionary(annotation, appearance_id, Some(image_id), &Dictionary::new());
            (document, dictionary)
        };

        let (first_document, first_dictionary) = export(&expected);
        assert_eq!(dictionary_float(&first_dictionary, b"Rotation"), Some(30.));
        assert!(
            import_pdf_rect(&first_dictionary, b"Rect")
                .unwrap()
                .same_pdf_geometry_as(rotated_rect_bounds(expected.rect, expected.rotation_degrees()))
        );
        let appearance = normal_appearance_stream(&first_document, &first_dictionary).unwrap();
        // Revu's layout: the unrotated box in page space, rotated clockwise
        // by the form matrix.
        assert!(
            import_pdf_rect(&appearance.dict, b"BBox")
                .unwrap()
                .same_pdf_geometry_as(expected.rect)
        );
        let matrix = appearance.dict.get(b"Matrix").unwrap().as_array().unwrap();
        assert_eq!(matrix.len(), 6);
        assert!(matrix[1].as_float().unwrap() < 0.);
        assert!(matrix[2].as_float().unwrap() > 0.);

        let first_reopen = import_image(
            &first_document,
            &first_dictionary,
            expected.id.as_str().to_owned(),
            0,
        )
        .unwrap();
        assert!(
            first_reopen.same_persisted_state_as(&expected),
            "{first_reopen:?} != {expected:?}"
        );

        let appearance_ids = image_appearance_object_ids(&first_document, &first_dictionary);
        let (form_id, image_id) = (appearance_ids[0], appearance_ids[1]);
        let mut opaque_document = first_document.clone();
        let form = opaque_document
            .get_object_mut(form_id)
            .unwrap()
            .as_stream_mut()
            .unwrap();
        form.content
            .splice(0..0, b"1 0 0 1 0 0 cm\n".iter().copied());
        opaque_document
            .get_object_mut(image_id)
            .unwrap()
            .as_stream_mut()
            .unwrap()
            .dict
            .remove(b"SMask");
        let opaque_reopen = import_image(
            &opaque_document,
            &first_dictionary,
            expected.id.as_str().to_owned(),
            0,
        )
        .unwrap();
        assert!(opaque_reopen.same_persisted_state_as(&expected));

        // Aspect lock has no PDF field: an image with its pixels' shape
        // reopens locked, a stretched one reopens free.
        assert!(first_reopen.aspect_locked);
        let mut stretched = expected.clone();
        stretched.aspect_locked = false;
        stretched.rect.width *= 1.5;
        let (stretched_document, stretched_dictionary) = export(&stretched);
        assert!(
            !import_image(
                &stretched_document,
                &stretched_dictionary,
                "stretched".into(),
                0,
            )
            .unwrap()
            .aspect_locked
        );

        let (second_document, second_dictionary) = export(&first_reopen);
        let second_reopen = import_image(
            &second_document,
            &second_dictionary,
            expected.id.as_str().to_owned(),
            0,
        )
        .unwrap();
        assert!(second_reopen.same_persisted_state_as(&expected));
        assert!(
            import_pdf_rect(&second_dictionary, b"Rect")
                .unwrap()
                .same_pdf_geometry_as(import_pdf_rect(&first_dictionary, b"Rect").unwrap())
        );
    }









    #[test]
    fn rotated_text_box_appearance_roundtrips_nominal_geometry_without_aabb_growth() {
        use super::*;

        let expected = TextBoxAnnotation::new(
            MarkupId::new("rotation-text").unwrap(),
            0,
            PdfRect::new(100., 200., 120., 60.).unwrap(),
            "Rotated\ntext",
            TextBoxStyle::new("Helvetica", 12., "#ff0000", 0.75).unwrap(),
        )
        .unwrap()
        .with_rotation_degrees(30.)
        .unwrap();

        let export = |annotation: &TextBoxAnnotation| {
            let mut document = Document::with_version("1.7");
            let appearance_id = add_text_appearance(&mut document, annotation).unwrap();
            let font_resources = text_appearance_font_resources(&document, appearance_id);
            let dictionary = text_box_dictionary(
                annotation,
                appearance_id,
                font_resources,
                &Dictionary::new(),
            );
            (document, dictionary)
        };

        let (first_document, first_dictionary) = export(&expected);
        assert_eq!(dictionary_float(&first_dictionary, b"Rotation"), Some(30.));
        assert!(
            import_pdf_rect(&first_dictionary, b"Rect")
                .unwrap()
                .same_pdf_geometry_as(text_box_annotation_bounds(&expected))
        );
        let appearance = normal_appearance_stream(&first_document, &first_dictionary).unwrap();
        assert!(
            import_pdf_rect(&appearance.dict, b"BBox")
                .unwrap()
                .same_pdf_geometry_as(expected.layout_rect)
        );
        assert_eq!(
            appearance
                .dict
                .get(b"Matrix")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            6
        );

        let first_reopen = import_text_box(
            &first_document,
            &first_dictionary,
            expected.id.as_str().to_owned(),
            0,
        )
        .unwrap();
        assert!(
            first_reopen.same_persisted_state_as(&expected),
            "{first_reopen:?} != {expected:?}"
        );

        let (second_document, second_dictionary) = export(&first_reopen);
        let second_reopen = import_text_box(
            &second_document,
            &second_dictionary,
            expected.id.as_str().to_owned(),
            0,
        )
        .unwrap();
        assert!(second_reopen.same_persisted_state_as(&expected));
        assert!(
            import_pdf_rect(&second_dictionary, b"Rect")
                .unwrap()
                .same_pdf_geometry_as(import_pdf_rect(&first_dictionary, b"Rect").unwrap())
        );
    }

    #[test]
    fn text_box_contents_use_utf16_bom_and_decode_legacy_and_malformed_inputs_safely() {
        for value in ["ASCII", "ASCII\n世界 😀 é €"] {
            let encoded = pdf_text_box_contents(value);
            let Object::String(bytes, StringFormat::Hexadecimal) = &encoded else {
                panic!("Text Box /Contents must be a hexadecimal PDF text string");
            };
            assert_eq!(&bytes[..2], &[0xfe, 0xff]);
            assert_eq!(bytes.len(), 2 + value.encode_utf16().count() * 2);
            assert_eq!(decode_pdf_text_string_compat(&encoded).unwrap(), value);
        }

        let value = "legacy raw UTF-8: 世界 😀 é €";
        let legacy = Object::String(value.as_bytes().to_vec(), StringFormat::Literal);
        assert_eq!(decode_pdf_text_string_compat(&legacy).unwrap(), value);

        let mut utf16_le = vec![0xff, 0xfe];
        for unit in "little-endian 😀".encode_utf16() {
            utf16_le.extend_from_slice(&unit.to_le_bytes());
        }
        let utf16_le = Object::String(utf16_le, StringFormat::Hexadecimal);
        assert_eq!(
            decode_pdf_text_string_compat(&utf16_le).unwrap(),
            "little-endian 😀"
        );

        let pdf_doc = Object::String(vec![0x8b], StringFormat::Literal);
        assert_eq!(decode_pdf_text_string_compat(&pdf_doc).unwrap(), "‰");

        for bytes in [
            vec![0xfe, 0xff, 0x00],
            vec![0xff, 0xfe, 0x00],
            vec![0xfe, 0xff, 0xd8, 0x3d],
        ] {
            let malformed = Object::String(bytes, StringFormat::Hexadecimal);
            assert!(decode_pdf_text_string_compat(&malformed).is_err());
        }
    }

    #[test]
    fn unicode_shaping_uses_ligatures_kerning_variation_clusters_and_bidi_positions() {
        let ligature = unicode_text_line("ffi中", "Helvetica").unwrap();
        assert!((ligature.width_em - 1.812).abs() < 0.000_001);
        assert_eq!(ligature.glyphs.len(), 2);
        assert_eq!(ligature.glyphs[0].font, Some(EmbeddedTextFont::Sans));
        assert_eq!(ligature.glyphs[0].glyph_id, 4259);
        assert_eq!(ligature.glyphs[0].unicode.as_deref(), Some("ffi"));
        assert_eq!(ligature.glyphs[1].glyph_id, 564);
        assert_eq!(ligature.glyphs[1].unicode.as_deref(), Some("中"));

        let kerned = unicode_text_line("AV中", "Helvetica").unwrap();
        assert!((kerned.width_em - 2.095).abs() < 0.000_001);
        assert_eq!(kerned.glyphs[0].glyph_id, 34);
        assert_eq!(kerned.glyphs[1].glyph_id, 55);
        assert!((kerned.glyphs[1].x_em - 0.563).abs() < 0.000_001);

        let emoji = unicode_text_line("❤️中", "Helvetica").unwrap();
        assert_eq!(emoji.glyphs[0].font, Some(EmbeddedTextFont::Emoji));
        assert_eq!(emoji.glyphs[0].glyph_id, 170);
        assert_eq!(emoji.glyphs[0].unicode.as_deref(), Some("❤️"));
        assert_eq!(emoji.glyphs[1].glyph_id, 3);
        assert_eq!(emoji.glyphs[1].unicode, None);
        assert!((emoji.width_em - 2.269_531_25).abs() < 0.000_001);
        let text_selector = unicode_text_line("❤︎中", "Helvetica").unwrap();
        assert_eq!(text_selector.glyphs[0].unicode.as_deref(), Some("❤︎"));
        assert_eq!(text_selector.glyphs[0].glyph_id, 170);
        assert_eq!(text_selector.glyphs.len(), 2);

        let bidi = unicode_text_line("A\u{202e}123\u{202c}B", "Helvetica").unwrap();
        let visible = bidi
            .glyphs
            .iter()
            .filter(|glyph| {
                glyph
                    .unicode
                    .as_deref()
                    .is_some_and(|value| value.chars().all(char::is_alphanumeric))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            visible
                .iter()
                .map(|glyph| glyph.source_start)
                .collect::<Vec<_>>(),
            vec![0, 6, 5, 4, 10]
        );
        assert!(visible.windows(2).all(|pair| pair[0].x_em < pair[1].x_em));
        assert!((bidi.width_em - 2.769).abs() < 0.000_001);
        assert!(unicode_text_line("שלום", "Helvetica").is_err());
    }

    #[test]
    fn selected_annotation_fonts_drive_metrics_resources_and_standard_metadata() {
        use super::*;

        let content = "iiii WWWW é € ffi中 ❤️";
        let helvetica = unicode_text_line(content, "Helvetica").unwrap();
        let arimo = unicode_text_line(content, "Arimo").unwrap();
        let mono = unicode_text_line(content, "Roboto Mono").unwrap();
        assert_ne!(arimo.width_em, helvetica.width_em);
        assert_ne!(mono.width_em, arimo.width_em);
        assert!(
            arimo
                .glyphs
                .iter()
                .filter(|glyph| glyph
                    .unicode
                    .as_deref()
                    .is_some_and(|value| value.is_ascii()))
                .all(|glyph| glyph.font == Some(EmbeddedTextFont::ArimoRegular))
        );

        for (index, (family, resource)) in [
            ("Arimo", "Arimo"),
            ("Roboto Mono", "RobotoMono"),
            ("Tinos", "Tinos"),
        ]
        .into_iter()
        .enumerate()
        {
            let annotation = TextBoxAnnotation::new(
                MarkupId::new(format!("font-family-{index}")).unwrap(),
                0,
                PdfRect::new(40., 50. + index as f64 * 80., 260., 60.).unwrap(),
                content,
                TextBoxStyle::new(family, 12., "#172b4d", 1.).unwrap(),
            )
            .unwrap();
            let mut document = Document::with_version("1.7");
            let appearance_id = add_text_appearance(&mut document, &annotation).unwrap();
            let font_resources = text_appearance_font_resources(&document, appearance_id);
            assert!(font_resources.has(resource.as_bytes()));
            let dictionary = text_box_dictionary(
                &annotation,
                appearance_id,
                font_resources,
                &Dictionary::new(),
            );
            assert!(
                dictionary_string(&dictionary, b"DA")
                    .unwrap()
                    .ends_with(&format!("/{resource} 12 Tf"))
            );
            assert!(
                dictionary_string(&dictionary, b"DS")
                    .unwrap()
                    .starts_with(&format!("font: {family} 12pt"))
            );
            let appearance = normal_appearance_stream(&document, &dictionary).unwrap();
            assert!(
                String::from_utf8_lossy(&appearance.content)
                    .contains(&format!("/{resource} 12.000000 Tf"))
            );
            let reopened =
                import_text_box(&document, &dictionary, annotation.id.to_string(), 0).unwrap();
            assert_eq!(reopened.style().font_family(), family);
        }
    }

    #[test]
    fn standard_css_font_aliases_import_without_private_metadata() {
        use super::*;

        for (style, expected) in [
            ("font-family: 'ArialMT';", "Arimo"),
            ("font: TimesNewRomanPS-BoldMT 11pt;", "Tinos"),
            ("font-family: ABCDEF+CourierNewPSMT;", "Roboto Mono"),
            ("font: Helvetica Neue 0pt;", "Helvetica"),
        ] {
            let dictionary = dictionary! { "DS" => pdf_literal(style) };
            assert_eq!(
                css_annotation_font_family(&dictionary).as_deref(),
                Some(expected)
            );
        }
        let malformed = dictionary! { "DS" => pdf_literal("font: Unknown 12pt;") };
        assert_eq!(css_annotation_font_family(&malformed), None);
    }

    #[test]
    fn standard_da_fonts_resolve_indirect_dr_and_selected_appearance_state_resources() {
        use super::*;

        let mut document = Document::with_version("1.7");
        let arimo = document.add_object(dictionary! {
            "Type" => "Font", "BaseFont" => "ABCDEF+ArialMT",
        });
        let annotation_fonts = document.add_object(dictionary! {
            "Primary" => Object::Reference(arimo),
        });
        let annotation_resources = document.add_object(dictionary! {
            "Font" => Object::Reference(annotation_fonts),
        });
        let annotation = dictionary! {
            "DA" => pdf_literal("/Primary 11 Tf"),
            "DR" => Object::Reference(annotation_resources),
        };
        assert_eq!(
            standard_annotation_font_family(&document, &annotation).as_deref(),
            Some("Arimo")
        );

        let tinos = document.add_object(dictionary! {
            "Type" => "Font", "BaseFont" => "TimesNewRomanPS-BoldMT",
        });
        let selected = document.add_object(Stream::new(
            dictionary! {
                "Resources" => dictionary! {
                    "Font" => dictionary! { "Caption" => Object::Reference(tinos) },
                },
            },
            Vec::new(),
        ));
        let unused = document.add_object(Stream::new(Dictionary::new(), Vec::new()));
        let states = document.add_object(dictionary! {
            "Off" => Object::Reference(unused),
            "On" => Object::Reference(selected),
        });
        let appearance_annotation = dictionary! {
            "DA" => pdf_literal("/Caption 12 Tf"),
            "AS" => "On",
            "AP" => dictionary! { "N" => Object::Reference(states) },
        };
        assert_eq!(
            normal_appearance_stream(&document, &appearance_annotation)
                .unwrap()
                .dict
                .get(b"Resources")
                .is_ok(),
            true
        );
        assert_eq!(
            standard_annotation_font_family(&document, &appearance_annotation).as_deref(),
            Some("Tinos")
        );
    }

    #[test]
    fn standard_da_font_resolution_degrades_safely_for_cycles_and_malformed_content() {
        use super::*;

        let mut document = Document::with_version("1.7");
        let cycle_id = document.new_object_id();
        document
            .objects
            .insert(cycle_id, Object::Reference(cycle_id));
        for dictionary in [
            dictionary! {
                "DA" => pdf_literal("/Primary 11 Tf"),
                "DR" => Object::Reference(cycle_id),
            },
            dictionary! { "DA" => pdf_literal("/Primary nope Tf") },
            dictionary! { "DA" => pdf_literal("/Primary 11 Tf"), "DR" => 42 },
        ] {
            assert_eq!(
                standard_annotation_font_family(&document, &dictionary),
                None
            );
        }
    }

    #[test]
    fn standard_font_metadata_flows_through_text_box_and_shared_caption_imports() {
        use super::*;

        let mut document = Document::with_version("1.7");
        let font = document.add_object(dictionary! {
            "Type" => "Font", "BaseFont" => "ABCDEF+CourierNewPSMT",
        });
        let resources = document.add_object(dictionary! {
            "Font" => dictionary! { "Body" => Object::Reference(font) },
        });
        let mut annotation = dictionary! {
            "Rect" => Object::Array(vec![0.into(), 0.into(), 120.into(), 40.into()]),
            "Contents" => pdf_literal("standard font"),
            "DA" => pdf_literal("/Body 12 Tf"),
            "DR" => Object::Reference(resources),
        };
        assert_eq!(
            import_text_box(&document, &annotation, "standard-text".into(), 0)
                .unwrap()
                .style()
                .font_family(),
            "Roboto Mono"
        );
        assert_eq!(
            import_measurement_text_style(&document, &annotation, 1.)
                .unwrap()
                .font_family(),
            "Roboto Mono"
        );

        annotation.set("DS", pdf_literal("font-family: ArialMT;"));
        assert_eq!(
            import_text_box(&document, &annotation, "standard-ds".into(), 0)
                .unwrap()
                .style()
                .font_family(),
            "Arimo"
        );
        // Revu's `/DS` shorthand carries the family, weight and size.
        annotation.set(
            "DS",
            pdf_literal("font: bold Times New Roman 14pt; text-align:center; color:#00AA00"),
        );
        let style = import_text_box(&document, &annotation, "revu-font".into(), 0)
            .unwrap()
            .style()
            .clone();
        assert_eq!(style.font_family(), "Tinos");
        assert_eq!(style.font_size_pt(), 14.);
        assert_eq!(style.weight(), 700);
        assert_eq!(style.alignment(), TextAlignment::Center);
        assert_eq!(style.color(), "#00aa00");
        // A private family key is no longer read.
        annotation.set("BPFontFamily", pdf_literal("Arimo"));
        assert_eq!(
            import_text_box(&document, &annotation, "private-font".into(), 0)
                .unwrap()
                .style()
                .font_family(),
            "Tinos"
        );
    }

    #[test]
    fn embedded_font_cids_distinguish_sequences_and_preserve_continuation_glyphs() {
        let mut document = Document::with_version("1.7");
        let first = BTreeSet::from([
            EmbeddedCidGlyph {
                glyph_id: 170,
                unicode: Some("❤︎".into()),
            },
            EmbeddedCidGlyph {
                glyph_id: 170,
                unicode: Some("❤️".into()),
            },
            EmbeddedCidGlyph {
                glyph_id: 3,
                unicode: None,
            },
        ]);
        let (font_id, assigned) =
            add_embedded_unicode_font(&mut document, EmbeddedTextFont::Emoji, &first).unwrap();
        assert_eq!(assigned.len(), 3);
        assert_ne!(
            assigned[&EmbeddedCidGlyph {
                glyph_id: 170,
                unicode: Some("❤︎".into()),
            }],
            assigned[&EmbeddedCidGlyph {
                glyph_id: 170,
                unicode: Some("❤️".into()),
            }]
        );
        let font = document.get_object(font_id).unwrap().as_dict().unwrap();
        let persisted = embedded_cid_mapping_from_standard_font(&document, font);
        assert_eq!(persisted.len(), 3);
        assert!(persisted.values().any(|glyph| glyph.unicode.is_none()));
        let cmap_id = font.get(b"ToUnicode").unwrap().as_reference().unwrap();
        let cmap = String::from_utf8(
            document
                .get_object(cmap_id)
                .unwrap()
                .as_stream()
                .unwrap()
                .content
                .clone(),
        )
        .unwrap();
        assert!(cmap.contains("2764FE0E"));
        assert!(cmap.contains("2764FE0F"));

        let second = BTreeSet::from([EmbeddedCidGlyph {
            glyph_id: 578,
            unicode: Some("👍🏽".into()),
        }]);
        let (reused_id, _) =
            add_embedded_unicode_font(&mut document, EmbeddedTextFont::Emoji, &second).unwrap();
        assert_eq!(reused_id, font_id);
        let reused = document.get_object(font_id).unwrap().as_dict().unwrap();
        let persisted = embedded_cid_mapping_from_standard_font(&document, reused);
        assert_eq!(persisted.len(), 4);
        assert!(persisted.values().any(|glyph| glyph.unicode.is_none()));
    }

    #[test]
    fn rich_text_box_exports_imports_and_renders_every_selectable_font_variant() {
        use super::*;

        let families = ["Helvetica", "Arimo", "Roboto Mono", "Tinos"];
        let emphases = [(false, false), (true, false), (false, true), (true, true)];
        let mut content = String::new();
        let mut runs = Vec::new();
        for (family_index, family) in families.into_iter().enumerate() {
            for (emphasis_index, (bold, italic)) in emphases.into_iter().enumerate() {
                let separator =
                    if emphasis_index == emphases.len() - 1 && family_index < families.len() - 1 {
                        "\n"
                    } else {
                        "|"
                    };
                let text = format!("{family}-{bold}-{italic}{separator}");
                content.push_str(&text);
                runs.push(
                    TextBoxRichTextRun::new(text)
                        .unwrap()
                        .with_font_family(family)
                        .unwrap()
                        .with_emphasis(bold, italic)
                        .with_color(if bold { "#aa1122" } else { "#2255aa" })
                        .unwrap()
                        .with_font_size_pt(if italic { 13. } else { 11. })
                        .unwrap(),
                );
            }
        }
        let annotation = TextBoxAnnotation::new(
            MarkupId::new("rich-font-variants").unwrap(),
            0,
            PdfRect::new(24., 30., 540., 80.).unwrap(),
            content,
            TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
        )
        .unwrap()
        .with_rich_text_runs(runs)
        .unwrap();
        let mut session = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        session.add_text_box(annotation.clone()).unwrap();
        let object_id = annotation_object_id(&session.document, 0, annotation.id.as_str()).unwrap();
        let dictionary = session
            .document
            .get_object(object_id)
            .unwrap()
            .as_dict()
            .unwrap();
        // Revu writes fonts in the appearance, not an annotation `/DR`.
        let fonts = normal_appearance_stream(&session.document, dictionary)
            .unwrap()
            .dict
            .get(b"Resources")
            .and_then(Object::as_dict)
            .and_then(|resources| resources.get(b"Font"))
            .and_then(Object::as_dict)
            .unwrap()
            .clone();
        for name in [
            b"Helv".as_slice(),
            b"HelvBld",
            b"HelvOblique",
            b"HelvBoldOblique",
            b"Arimo",
            b"ArimoBold",
            b"ArimoOblique",
            b"ArimoBoldOblique",
            b"RobotoMono",
            b"RobotoMonoBold",
            b"RobotoMonoOblique",
            b"RobotoMonoBoldOblique",
            b"Tinos",
            b"TinosBold",
            b"TinosOblique",
            b"TinosBoldOblique",
        ] {
            assert!(
                fonts.has(name),
                "missing rich-text font resource {:?}",
                String::from_utf8_lossy(name)
            );
        }
        let rich = decode_pdf_text_string_compat(dictionary.get(b"RC").unwrap()).unwrap();
        assert!(rich.contains("font-weight:bold"));
        assert!(rich.contains("font-style:italic"));
        assert!(rich.contains("font-family:Roboto Mono"));
        let imported = import_text_box(
            &session.document,
            dictionary,
            annotation.id.as_str().into(),
            0,
        )
        .unwrap();
        assert!(
            imported.same_persisted_state_as(&annotation),
            "{imported:?} != {annotation:?}"
        );
        if let Some(path) = std::env::var_os("BP_RICH_TEXT_FIXTURE_OUTPUT") {
            session.document.save(PathBuf::from(path)).unwrap();
        }
    }

    #[test]
    fn unicode_text_box_creation_and_edit_embed_reusable_type0_fonts_without_loss() {
        use super::*;
        let unicode = "ffi中 ❤️\nA\u{202e}123\u{202c}B\n世界 😀 é €";
        let annotation = TextBoxAnnotation::new(
            MarkupId::new("unicode-text").unwrap(),
            0,
            PdfRect::new(40., 50., 160., 90.).unwrap(),
            unicode,
            TextBoxStyle::new("Helvetica", 12., "#172b4d", 1.).unwrap(),
        )
        .unwrap();
        let mut creation = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        creation.add_text_box(annotation.clone()).unwrap();
        let object_id =
            annotation_object_id(&creation.document, 0, annotation.id.as_str()).unwrap();
        let dictionary = creation
            .document
            .get_object(object_id)
            .unwrap()
            .as_dict()
            .unwrap();
        assert_eq!(
            decode_pdf_text_string_compat(dictionary.get(b"Contents").unwrap()).unwrap(),
            unicode
        );
        let appearance = normal_appearance_stream(&creation.document, dictionary).unwrap();
        assert_eq!(
            appearance
                .dict
                .get(b"Resources")
                .and_then(Object::as_dict)
                .and_then(|resources| resources.get(b"Font"))
                .and_then(Object::as_dict)
                .unwrap()
                .len(),
            3
        );
        let appearance_text = String::from_utf8_lossy(&appearance.content);
        assert!(appearance_text.contains("/NotoSansSC 12.000000 Tf"));
        assert!(appearance_text.contains("/NotoEmoji 12.000000 Tf"));
        assert!(appearance_text.contains(&format!(
            "/ActualText <FEFF{}>",
            unicode_destination_hex("A\u{202e}123\u{202c}B")
        )));
        assert!(!appearance_text.contains("??"));
        let fonts = appearance
            .dict
            .get(b"Resources")
            .and_then(Object::as_dict)
            .and_then(|resources| resources.get(b"Font"))
            .and_then(Object::as_dict)
            .unwrap();
        assert_eq!(fonts.len(), 3);
        for font_id in fonts
            .iter()
            .filter(|(name, _)| name.as_slice() != b"Helv")
            .map(|(_, font)| font.as_reference().unwrap())
        {
            let font = creation
                .document
                .get_object(font_id)
                .unwrap()
                .as_dict()
                .unwrap();
            assert_eq!(font.get(b"Subtype").unwrap().as_name().unwrap(), b"Type0");
            assert_eq!(
                font.get(b"Encoding").unwrap().as_name().unwrap(),
                b"Identity-H"
            );
            assert!(font.get(b"ToUnicode").unwrap().as_reference().is_ok());
            assert!(font.get(b"DescendantFonts").unwrap().as_array().is_ok());
            assert!(font.get(b"BPUnicodeMap").is_err() && font.get(b"BPGlyphMap").is_err());
        }
        let embedded_before = creation
            .document
            .objects
            .values()
            .filter(|object| {
                object
                    .as_dict()
                    .is_ok_and(|dictionary| dictionary_name(dictionary, b"Subtype").as_deref() == Some("Type0"))
            })
            .count();
        let mut moved = creation.text_boxes()[0].clone();
        moved.layout_rect.x += 8.;
        creation.replace_text_box(moved.clone()).unwrap();
        assert_eq!(
            creation
                .document
                .objects
                .values()
                .filter(|object| {
                    object
                        .as_dict()
                        .is_ok_and(|dictionary| dictionary_name(dictionary, b"Subtype").as_deref() == Some("Type0"))
                })
                .count(),
            embedded_before
        );
        let requested_output =
            std::env::var_os("BP_UNICODE_TEXT_FIXTURE_OUTPUT").map(PathBuf::from);
        let output = requested_output.clone().unwrap_or_else(|| {
            std::env::temp_dir().join(format!(
                "butter-paper-unicode-text-{}-{}.pdf",
                process::id(),
                NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed)
            ))
        });
        let mut serialized = creation.document.clone();
        serialized.save(&output).unwrap();
        let reopened_document = Document::load(&output).unwrap();
        let reopened =
            PdfPersistenceSession::from_document(output.clone(), reopened_document, None).unwrap();
        assert_eq!(reopened.text_boxes()[0].content(), unicode);
        assert!(reopened.text_boxes()[0].same_persisted_state_as(&moved));
        if requested_output.is_none() {
            fs::remove_file(&output).unwrap();
        }

        let unsupported = TextBoxAnnotation::new(
            MarkupId::new("unsupported-unicode").unwrap(),
            0,
            PdfRect::new(20., 20., 120., 40.).unwrap(),
            "unsupported \u{10ffff}",
            annotation.style().clone(),
        )
        .unwrap();
        let before_objects = creation.document.objects.clone();
        let before_order = creation.annotation_order.clone();
        assert!(matches!(
            creation.add_text_box(unsupported),
            Err(PdfPersistenceError::InvalidDocument(message))
                if message.contains("cannot encode one or more Unicode characters")
        ));
        assert_eq!(creation.document.objects, before_objects);
        assert_eq!(creation.annotation_order, before_order);
    }

    #[test]
    fn unicode_text_bearing_caption_families_share_embedded_fonts_and_reopen() {
        use super::*;
        let unicode = "ASCII 世界 😀 é €";
        let text = TextBoxStyle::new("Arimo", 12., "#172b4d", 1.).unwrap();
        let line = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
        let cloud = RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap();
        let calibration = LengthCalibration::from_scale(1., 1., "m", 0, true)
            .unwrap()
            .with_label(unicode)
            .unwrap();
        let points = vec![
            PdfPoint::new(20., 20.).unwrap(),
            PdfPoint::new(120., 20.).unwrap(),
            PdfPoint::new(120., 80.).unwrap(),
        ];
        let callout = CalloutAnnotation::new(
            MarkupId::new("unicode-callout").unwrap(),
            0,
            vec![
                PdfPoint::new(20., 20.).unwrap(),
                PdfPoint::new(60., 40.).unwrap(),
                PdfPoint::new(100., 40.).unwrap(),
            ],
            PdfRect::new(100., 20., 170., 50.).unwrap(),
            unicode,
            CalloutAppearance::new(line.clone(), text.clone()).unwrap(),
        )
        .unwrap();
        let cloud_plus = CloudPlusAnnotation::new(
            MarkupId::new("unicode-cloud-plus").unwrap(),
            0,
            points.clone(),
            1.,
            vec![
                PdfPoint::new(120., 80.).unwrap(),
                PdfPoint::new(145., 110.).unwrap(),
                PdfPoint::new(170., 110.).unwrap(),
            ],
            PdfRect::new(170., 90., 170., 50.).unwrap(),
            unicode,
            CloudPlusAppearance::new(cloud, line.clone(), text.clone()).unwrap(),
        )
        .unwrap();
        let length = LengthAnnotation::new_with_appearance(
            MarkupId::new("unicode-length").unwrap(),
            0,
            PdfPoint::new(20., 140.).unwrap(),
            PdfPoint::new(180., 140.).unwrap(),
            calibration.clone(),
            DimensionAppearance::new(line.clone(), text.clone()).unwrap(),
        )
        .unwrap();
        let polylength = MeasurementPathAnnotation::new_with_text_style(
            MarkupId::new("unicode-polylength").unwrap(),
            0,
            points.clone(),
            MeasurementPathKind::Polylength,
            calibration.clone(),
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            text.clone(),
        )
        .unwrap();
        let area = MeasurementPathAnnotation::new_with_text_style(
            MarkupId::new("unicode-area").unwrap(),
            0,
            points,
            MeasurementPathKind::Area,
            calibration,
            RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
            text.clone(),
        )
        .unwrap();
        let dimension = DimensionAnnotation::new(
            MarkupId::new("unicode-dimension").unwrap(),
            0,
            PdfPoint::new(20., 200.).unwrap(),
            PdfPoint::new(180., 200.).unwrap(),
            24.,
            unicode,
            DimensionAppearance::new(line, text).unwrap(),
        )
        .unwrap();

        let mut session = PdfPersistenceSession::from_document(
            PathBuf::new(),
            retained_render_test_document(),
            None,
        )
        .unwrap();
        session.add_callout(callout).unwrap();
        session.add_cloud_plus(cloud_plus).unwrap();
        session.add_length(length).unwrap();
        session.add_measurement_path(polylength).unwrap();
        session.add_measurement_path(area).unwrap();
        session.add_dimension(dimension).unwrap();

        let unicode_appearances = session
            .document
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .filter(|stream| {
                let fonts = stream
                    .dict
                    .get(b"Resources")
                    .and_then(Object::as_dict)
                    .and_then(|resources| resources.get(b"Font"))
                    .and_then(Object::as_dict);
                fonts.is_ok_and(|fonts| {
                    fonts.has(b"Arimo") && fonts.has(b"NotoSansSC") && fonts.has(b"NotoEmoji")
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(unicode_appearances.len(), 4);
        for appearance in unicode_appearances {
            let content = String::from_utf8_lossy(&appearance.content);
            assert!(content.contains("/Arimo 12.000000 Tf"));
            assert!(content.contains("/NotoSansSC 12.000000 Tf"));
            assert!(content.contains("/NotoEmoji 12.000000 Tf"));
            assert!(!content.contains("??"));
        }
        assert_eq!(
            session
                .document
                .objects
                .values()
                .filter(|object| object
                    .as_dict()
                    .is_ok_and(|dictionary| dictionary_name(dictionary, b"Subtype").as_deref() == Some("Type0")))
                .count(),
            3
        );

        let mut bytes = Vec::new();
        session.document.save_to(&mut bytes).unwrap();
        if let Some(output) = std::env::var_os("BP_UNICODE_CAPTION_FIXTURE_OUTPUT") {
            std::fs::write(PathBuf::from(output), &bytes).unwrap();
        }
        let reopened = PdfPersistenceSession::from_document(
            PathBuf::new(),
            Document::load_mem(&bytes).unwrap(),
            None,
        )
        .unwrap();
        assert_eq!(reopened.callouts()[0].content(), unicode);
        assert_eq!(reopened.cloud_pluses()[0].content(), unicode);
        assert_eq!(reopened.dimensions()[0].content(), unicode);
        assert!(reopened.lengths()[0].caption().contains(unicode));
        assert_eq!(reopened.measurement_paths()[0].caption(), "160 m");
        assert_eq!(reopened.measurement_paths()[1].caption(), "3,000 sq m");
    }

    #[test]
    fn unsupported_caption_unicode_rejects_before_document_mutation() {
        use super::*;
        let unsupported = "unsupported \u{10ffff}";
        let text = TextBoxStyle::new("Helvetica", 12., "#172b4d", 1.).unwrap();
        let line = StraightLineAppearance::new("#ff0000", 1., 1., StrokeStyle::Solid).unwrap();
        let assert_rejected =
            |mutate: &dyn Fn(&mut PdfPersistenceSession) -> Result<(), PdfPersistenceError>| {
                let mut session = PdfPersistenceSession::from_document(
                    PathBuf::new(),
                    retained_render_test_document(),
                    None,
                )
                .unwrap();
                let before_objects = session.document.objects.clone();
                let before_order = session.annotation_order.clone();
                assert!(matches!(
                    mutate(&mut session),
                    Err(PdfPersistenceError::InvalidDocument(message))
                        if message.contains("cannot encode one or more Unicode characters")
                ));
                assert_eq!(session.document.objects, before_objects);
                assert_eq!(session.annotation_order, before_order);
            };
        assert_rejected(&|session| {
            session.add_callout(
                CalloutAnnotation::new(
                    MarkupId::new("unsupported-callout").unwrap(),
                    0,
                    vec![
                        PdfPoint::new(20., 20.).unwrap(),
                        PdfPoint::new(60., 40.).unwrap(),
                        PdfPoint::new(100., 40.).unwrap(),
                    ],
                    PdfRect::new(100., 20., 170., 50.).unwrap(),
                    unsupported,
                    CalloutAppearance::new(line.clone(), text.clone()).unwrap(),
                )
                .unwrap(),
            )
        });
        assert_rejected(&|session| {
            session.add_cloud_plus(
                CloudPlusAnnotation::new(
                    MarkupId::new("unsupported-cloud-plus").unwrap(),
                    0,
                    vec![
                        PdfPoint::new(20., 20.).unwrap(),
                        PdfPoint::new(120., 20.).unwrap(),
                        PdfPoint::new(120., 80.).unwrap(),
                    ],
                    1.,
                    vec![
                        PdfPoint::new(120., 80.).unwrap(),
                        PdfPoint::new(145., 110.).unwrap(),
                        PdfPoint::new(170., 110.).unwrap(),
                    ],
                    PdfRect::new(170., 90., 170., 50.).unwrap(),
                    unsupported,
                    CloudPlusAppearance::new(
                        RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
                        line.clone(),
                        text.clone(),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
        });
        assert_rejected(&|session| {
            session
                .add_length(
                    LengthAnnotation::new_with_appearance(
                        MarkupId::new("unsupported-length").unwrap(),
                        0,
                        PdfPoint::new(20., 140.).unwrap(),
                        PdfPoint::new(180., 140.).unwrap(),
                        LengthCalibration::from_scale(1., 1., unsupported, 0, true).unwrap(),
                        DimensionAppearance::new(line.clone(), text.clone()).unwrap(),
                    )
                    .unwrap(),
                )
                .map(|_| ())
        });
        for kind in [MeasurementPathKind::Polylength, MeasurementPathKind::Area] {
            assert_rejected(&|session| {
                session.add_measurement_path(
                    MeasurementPathAnnotation::new_with_text_style(
                        MarkupId::new(format!("unsupported-{kind:?}").to_lowercase()).unwrap(),
                        0,
                        vec![
                            PdfPoint::new(20., 20.).unwrap(),
                            PdfPoint::new(120., 20.).unwrap(),
                            PdfPoint::new(120., 80.).unwrap(),
                        ],
                        kind,
                        LengthCalibration::from_scale(1., 1., unsupported, 0, true).unwrap(),
                        RectangleAppearance::new("#ff0000", 1., None::<String>, 1.).unwrap(),
                        text.clone(),
                    )
                    .unwrap(),
                )
            });
        }
        assert_rejected(&|session| {
            session.add_dimension(
                DimensionAnnotation::new(
                    MarkupId::new("unsupported-dimension").unwrap(),
                    0,
                    PdfPoint::new(20., 200.).unwrap(),
                    PdfPoint::new(180., 200.).unwrap(),
                    24.,
                    unsupported,
                    DimensionAppearance::new(line.clone(), text.clone()).unwrap(),
                )
                .unwrap(),
            )
        });
    }

    #[test]
    fn base14_helvetica_alignment_uses_the_same_win_ansi_bytes_for_width_and_output() {
        let narrow = text_appearance_line_bytes("iiii");
        let wide = text_appearance_line_bytes("WWWW");
        assert_eq!(narrow, b"iiii");
        assert_eq!(wide, b"WWWW");
        assert!((helvetica_text_width_pt(&narrow, 12.) - 10.656).abs() < 0.000_001);
        assert!((helvetica_text_width_pt(&wide, 12.) - 45.312).abs() < 0.000_001);

        assert_eq!(text_appearance_line_bytes("é €"), [0xe9, b' ', 0x80]);
        assert_eq!(text_appearance_line_bytes("世界"), b"??");
        assert!((helvetica_text_width_pt(&[0xe9, 0x80], 12.) - 13.344).abs() < 0.000_001);

        assert!((text_appearance_line_x(120., 10.656, TextAlignment::Left, 2.) - 2.).abs() < 0.000_001);
        assert!(
            (text_appearance_line_x(120., 10.656, TextAlignment::Center, 2.) - 54.672).abs()
                < 0.000_001
        );
        assert!(
            (text_appearance_line_x(120., 10.656, TextAlignment::Right, 2.) - 107.344).abs()
                < 0.000_001
        );
        assert!(
            (text_appearance_line_x(120., 45.312, TextAlignment::Center, 2.) - 37.344).abs()
                < 0.000_001
        );
        assert!(
            (text_appearance_line_x(120., 45.312, TextAlignment::Right, 2.) - 72.688).abs() < 0.000_001
        );
        assert_eq!(text_appearance_line_x(20., 200., TextAlignment::Right, 2.), 2.);
    }

    #[test]
    fn open_exchange_has_a_stable_versioned_json_contract() {
        let request_fixture = json!({
            "protocol": "butter-paper-pdf-engine",
            "version": 1,
            "request_id": "41",
            "command": {
                "type": "open",
                "session_id": "7",
                "source_handle_id": "3"
            }
        });
        let request = EngineRequest::Open(OpenRequest {
            request_id: RequestId::new(41),
            session_id: SessionId::new(7),
            source_handle_id: SourceHandleId::new(3),
        });

        assert_eq!(PDF_ENGINE_PROTOCOL_NAME, "butter-paper-pdf-engine");
        assert_eq!(PDF_ENGINE_PROTOCOL_VERSION, 1);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&encode_request(&request).unwrap())
                .unwrap(),
            request_fixture,
        );
        assert_eq!(
            decode_request(&serde_json::to_vec(&request_fixture).unwrap()).unwrap(),
            request,
        );

        let response_fixture = json!({
            "protocol": "butter-paper-pdf-engine",
            "version": 1,
            "request_id": "41",
            "result": {
                "type": "opened",
                "document": {
                    "page_count": 6,
                    "metadata": {
                        "title": "Fixture",
                        "author": null,
                        "subject": null,
                        "creator": "Butter Paper fixture generator",
                        "producer": null
                    },
                    "permissions": {
                        "print": true,
                        "copy": true,
                        "modify": true,
                        "annotate": true,
                        "fill_forms": true,
                        "accessibility": true,
                        "assemble": true,
                        "high_quality_print": true
                    },
                    "security": "unencrypted",
                    "xref_reconstructed": false
                }
            }
        });
        let response = EngineResponse::Opened {
            request_id: RequestId::new(41),
            document: DocumentInfo {
                page_count: 6,
                metadata: DocumentMetadata {
                    title: Some("Fixture".into()),
                    author: None,
                    subject: None,
                    creator: Some("Butter Paper fixture generator".into()),
                    producer: None,
                },
                permissions: DocumentPermissions::all(),
                security: DocumentSecurity::Unencrypted,
                xref_reconstructed: false,
            },
        };

        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&encode_response(&response).unwrap())
                .unwrap(),
            response_fixture,
        );
        assert_eq!(
            decode_response(&serde_json::to_vec(&response_fixture).unwrap()).unwrap(),
            response,
        );
    }

    #[test]
    fn identifiers_round_trip_across_the_typescript_json_boundary_without_precision_loss() {
        let request = EngineRequest::Open(OpenRequest {
            request_id: RequestId::new(u64::MAX),
            session_id: SessionId::new(9_007_199_254_740_993),
            source_handle_id: SourceHandleId::new(u64::MAX - 1),
        });
        let encoded = encode_request(&request).unwrap();
        let fixture: serde_json::Value = serde_json::from_slice(&encoded).unwrap();

        assert_eq!(fixture["request_id"], json!(u64::MAX.to_string()));
        assert_eq!(fixture["command"]["session_id"], json!("9007199254740993"));
        assert_eq!(
            fixture["command"]["source_handle_id"],
            json!((u64::MAX - 1).to_string())
        );
        assert_eq!(decode_request(&encoded).unwrap(), request);

        let numeric_id = json!({
            "protocol": "butter-paper-pdf-engine",
            "version": 1,
            "request_id": 41,
            "command": {
                "type": "open",
                "session_id": "7",
                "source_handle_id": "3"
            }
        });
        assert!(decode_request(&serde_json::to_vec(&numeric_id).unwrap()).is_err());
    }

    #[test]
    fn incompatible_versions_and_noncanonical_identifiers_fail_before_dispatch() {
        let wrong_version = json!({
            "protocol": "butter-paper-pdf-engine",
            "version": 2,
            "request_id": "41",
            "command": {
                "type": "open",
                "session_id": "7",
                "source_handle_id": "3"
            }
        });
        let error = decode_request(&serde_json::to_vec(&wrong_version).unwrap()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "unsupported PDF engine protocol version 2; expected 1"
        );

        let leading_zero = json!({
            "protocol": "butter-paper-pdf-engine",
            "version": 1,
            "request_id": "041",
            "command": {
                "type": "open",
                "session_id": "7",
                "source_handle_id": "3"
            }
        });
        assert!(decode_request(&serde_json::to_vec(&leading_zero).unwrap()).is_err());
    }

    #[test]
    fn password_transport_is_not_part_of_version_one() {
        let request = json!({
            "protocol": "butter-paper-pdf-engine",
            "version": 1,
            "request_id": "41",
            "command": {
                "type": "open",
                "session_id": "7",
                "source_handle_id": "3",
                "password": "must-not-enter-v1-json"
            }
        });

        let error = decode_request(&serde_json::to_vec(&request).unwrap()).unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid PDF engine message: open contains unsupported field \"password\""
        );
    }

    #[test]
    fn every_open_failure_uses_only_a_stable_code() {
        let cases = [
            (EngineErrorCode::PasswordRequired, "password_required"),
            (EngineErrorCode::UnsupportedSecurity, "unsupported_security"),
            (EngineErrorCode::MalformedDocument, "malformed_document"),
            (EngineErrorCode::RepairedDocument, "repaired_document"),
            (EngineErrorCode::LimitExceeded, "limit_exceeded"),
            (EngineErrorCode::WorkerCrashed, "worker_crashed"),
        ];

        for (code, wire_code) in cases {
            let response = EngineResponse::Failed {
                request_id: RequestId::new(5),
                error: code,
            };
            let encoded = encode_response(&response).unwrap();
            let fixture: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
            assert_eq!(
                fixture["result"],
                json!({ "type": "failed", "code": wire_code })
            );
            assert_eq!(decode_response(&encoded).unwrap(), response);
        }
    }

    #[test]
    fn encrypted_reconstructed_document_information_round_trips() {
        let response = EngineResponse::Opened {
            request_id: RequestId::new(u64::MAX),
            document: DocumentInfo {
                page_count: 935,
                metadata: DocumentMetadata::default(),
                permissions: DocumentPermissions {
                    print: true,
                    copy: false,
                    modify: false,
                    annotate: true,
                    fill_forms: false,
                    accessibility: true,
                    assemble: false,
                    high_quality_print: false,
                },
                security: DocumentSecurity::Encrypted,
                xref_reconstructed: true,
            },
        };
        let encoded = encode_response(&response).unwrap();
        let fixture: serde_json::Value = serde_json::from_slice(&encoded).unwrap();

        assert_eq!(fixture["request_id"], json!(u64::MAX.to_string()));
        assert_eq!(
            fixture["result"]["document"]["security"],
            json!("encrypted")
        );
        assert_eq!(
            fixture["result"]["document"]["xref_reconstructed"],
            json!(true)
        );
        assert_eq!(decode_response(&encoded).unwrap(), response);
    }
}
