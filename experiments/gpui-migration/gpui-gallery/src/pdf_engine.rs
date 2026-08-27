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
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process,
    sync::atomic::{AtomicU64, Ordering},
};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat, dictionary};
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};

use crate::annotation_model::{
    Annotation, AnnotationError, ArcAnnotation, BlendMode, CalloutAnnotation, CalloutAppearance,
    CloudAnnotation, CloudPlusAnnotation, CloudPlusAppearance, DecodedRgbaAsset,
    DimensionAnnotation, DimensionAppearance, EllipseAnnotation, ImageAnnotation, InkTool,
    LengthAnnotation, LengthCalibration, LineKind, MarkupId, MeasurementPathAnnotation,
    MeasurementPathKind, PageRotation, PageScale, PdfPoint, PdfRect, PenAnnotation, PenAppearance,
    RectangleAnnotation, RectangleAppearance, RedactAnnotation, ScalePrecision, ScalePrecisionMode,
    ScaleSource, ScaleUnit, SnapshotAnnotation, StraightLineAnnotation, StraightLineAppearance,
    StrokeStyle, TextAlignment, TextBoxAnnotation, TextBoxStyle, VertexPathAnnotation,
    VertexPathKind, ellipse_cubic_bezier_points, rectangle_world_corners,
};
#[cfg(unix)]
use crate::pdf_file_authority::AuthorizedPdfStage;
use crate::pdf_file_authority::{SaveAsTargetAuthority, SaveTargetError};

pub const PDF_ENGINE_PROTOCOL_NAME: &str = "butter-paper-pdf-engine";
pub const PDF_ENGINE_PROTOCOL_VERSION: u16 = 1;
static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(1);

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
    length_native_names: HashMap<MarkupId, String>,
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
    annotation_order: Vec<MarkupId>,
    page_scales: Vec<PageScale>,
    page_length_calibrations: BTreeMap<u32, LengthCalibration>,
    page_rotations: BTreeMap<u32, PageRotation>,
    original_page_rotations: BTreeMap<u32, PageRotation>,
    changed_page_rotations: std::collections::BTreeSet<u32>,
    untouched_annotations: Vec<UntouchedAnnotation>,
}

pub struct PreparedPdfSave {
    temporary: PathBuf,
    target: PathBuf,
    replacement_guard: Option<SourceGuard>,
    #[cfg(unix)]
    authorized_stage: Option<AuthorizedPdfStage>,
    #[cfg(unix)]
    cleanup_owned_by_authority: bool,
    published: bool,
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
        #[cfg(unix)]
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
            fs::set_permissions(
                &self.temporary,
                fs::Permissions::from_mode(guard.mode & 0o777),
            )?;
            fs::rename(&self.temporary, &self.target)?;
            self.published = true;
            let warnings = match sync_parent_directory(parent) {
                Ok(()) => Vec::new(),
                Err(error) => vec![format!(
                    "saved PDF was published, but its directory durability sync failed: {error}"
                )],
            };
            Ok(publication_outcome(warnings))
        }
    }
}

impl Drop for PreparedPdfSave {
    fn drop(&mut self) {
        if !self.published && {
            #[cfg(unix)]
            {
                !self.cleanup_owned_by_authority
            }
            #[cfg(not(unix))]
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
            length_native_names: imported.length_native_names,
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
            annotation_order: imported.annotation_order,
            page_scales,
            page_length_calibrations,
            page_rotations,
            original_page_rotations,
            changed_page_rotations: std::collections::BTreeSet::new(),
            untouched_annotations: imported.untouched,
        })
    }

    pub fn page_count(&self) -> usize {
        self.document.get_pages().len()
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
            let native_name = self
                .length_native_names
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
    /// scale removes stale `/BPPageScale` metadata from the PDF.
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

    pub fn untouched_annotations(&self) -> &[UntouchedAnnotation] {
        &self.untouched_annotations
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
        let object_id = annotation_object_id(
            &self.document,
            replacement.page_index,
            replacement.id.as_str(),
        )?;
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        require_canonical_redact_stable_id(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let dictionary = redact_dictionary(&annotation, &Dictionary::new(), &native_name)?;
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_redact_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        require_canonical_ellipse_stable_id(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_ellipse_appearance(&mut self.document, &annotation);
        let dictionary = ellipse_dictionary(
            &annotation,
            appearance_id,
            &Dictionary::new(),
            &native_name,
        )?;
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_ellipse_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        require_canonical_ellipse_stable_id(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_arc_appearance(&mut self.document, &annotation);
        let dictionary =
            arc_dictionary(&annotation, appearance_id, &Dictionary::new(), &native_name)?;
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_ellipse_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        let (cloud_name, text_name) = cloud_plus_native_names(id);
        self.document.objects.values().any(|object| {
            object.as_dict().ok().is_some_and(|dictionary| {
                dictionary_string(dictionary, b"NM")
                    .is_some_and(|name| name == cloud_name || name == text_name)
            })
        })
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
                    is_canonical_managed_redact(dictionary, &identity.raw_name)
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
                is_canonical_managed_snapshot(&self.document, dictionary, raw_name)
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
        append_native_annotation(
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
        require_canonical_pen_stable_id(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let appearance_id = add_pen_appearance(&mut self.document, &annotation);
        let dictionary =
            pen_dictionary(&annotation, appearance_id, &Dictionary::new(), &native_name);
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_pen_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_text_appearance(&mut self.document, &annotation);
        let dictionary = text_box_dictionary(&annotation, appearance_id, &Dictionary::new());
        append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.annotation_order.push(annotation.id.clone());
        self.text_boxes.push(annotation);
        Ok(())
    }

    pub fn replace_text_box(
        &mut self,
        annotation: TextBoxAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(
            &self.text_boxes,
            &annotation.id,
            |value| &value.id,
            "text box",
        )?;
        let object_id = annotation_object_id(
            &self.document,
            annotation.page_index,
            annotation.id.as_str(),
        )?;
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let appearance_id = add_text_appearance(&mut self.document, &annotation);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(text_box_dictionary(&annotation, appearance_id, &original)),
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

    pub fn add_length(&mut self, annotation: LengthAnnotation) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_length_appearance(&mut self.document, &annotation);
        let dictionary = length_dictionary(&annotation, appearance_id, &Dictionary::new());
        append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
        self.length_native_names
            .insert(annotation.id.clone(), format!("bp:{}", annotation.id));
        self.annotation_order.push(annotation.id.clone());
        self.lengths.push(annotation);
        Ok(())
    }

    pub fn replace_length(
        &mut self,
        annotation: LengthAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        let index =
            find_annotation_index(&self.lengths, &annotation.id, |value| &value.id, "length")?;
        let native_name = self
            .length_native_names
            .get(&annotation.id)
            .map(String::as_str)
            .unwrap_or(annotation.id.as_str());
        let object_id = annotation_object_id(&self.document, annotation.page_index, native_name)?;
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let appearance_id = add_length_appearance(&mut self.document, &annotation);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(length_dictionary(&annotation, appearance_id, &original)),
        );
        self.length_native_names
            .insert(annotation.id.clone(), format!("bp:{}", annotation.id));
        self.lengths[index] = annotation;
        Ok(())
    }

    /// Removes one imported native LineDimension annotation by stable identity.
    pub fn remove_length(&mut self, id: &MarkupId) -> Result<(), PdfPersistenceError> {
        let index = find_annotation_index(&self.lengths, id, |value| &value.id, "length")?;
        let page_index = self.lengths[index].page_index;
        let native_name = self
            .length_native_names
            .get(id)
            .map(String::as_str)
            .unwrap_or(id.as_str());
        let object_id = annotation_object_id(&self.document, page_index, native_name)?;
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        self.length_native_names.remove(id);
        self.lengths.remove(index);
        self.annotation_order.retain(|candidate| candidate != id);
        Ok(())
    }

    pub fn add_dimension(
        &mut self,
        annotation: DimensionAnnotation,
    ) -> Result<(), PdfPersistenceError> {
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_dimension_appearance(&mut self.document, &annotation)?;
        let dictionary = dimension_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        let index = find_annotation_index(
            &self.dimensions,
            &annotation.id,
            |value| &value.id,
            "dimension",
        )?;
        let native_name = self
            .dimension_native_names
            .get(&annotation.id)
            .map(String::as_str)
            .unwrap_or(annotation.id.as_str());
        let object_id = annotation_object_id(&self.document, annotation.page_index, native_name)?;
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let appearance_id = add_dimension_appearance(&mut self.document, &annotation)?;
        self.document.objects.insert(
            object_id,
            Object::Dictionary(dimension_dictionary(&annotation, appearance_id, &original)?),
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
        require_canonical_straight_line_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        let native_name = canonical_native_annotation_name(&annotation.id);
        let dictionary = straight_line_dictionary(&annotation, &Dictionary::new());
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_straight_line_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let old_appearance_id = normal_appearance_object_id(&original);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(straight_line_dictionary(&annotation, &original)),
        );
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
        require_canonical_vertex_path_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_vertex_path_appearance(&mut self.document, &annotation)?;
        let dictionary = vertex_path_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_vertex_path_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        require_canonical_cloud_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_cloud_appearance(&mut self.document, &annotation)?;
        let dictionary = cloud_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_cloud_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        require_canonical_cloud_plus_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        require_page(&self.document, annotation.page_index)?;
        let (cloud_name, text_name) = cloud_plus_native_names(&annotation.id);
        let cloud_appearance_id = add_cloud_plus_cloud_appearance(&mut self.document, &annotation)?;
        let text_appearance_id = add_cloud_plus_text_appearance(&mut self.document, &annotation)?;
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
            &Dictionary::new(),
        )?;
        let cloud_object_id =
            append_native_annotation(&mut self.document, annotation.page_index, cloud_dictionary)?;
        let text_object_id = match append_native_annotation(
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
        let index = find_annotation_index(
            &self.cloud_pluses,
            &annotation.id,
            |value| &value.id,
            "Cloud+",
        )?;
        if self.cloud_pluses[index].same_persisted_state_as(&annotation) {
            return Ok(());
        }
        require_canonical_cloud_plus_stable_id(&annotation.id)?;
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
        let (cloud_name, text_name) = cloud_plus_native_names(&annotation.id);
        let cloud_appearance_id = add_cloud_plus_cloud_appearance(&mut self.document, &annotation)?;
        let text_appearance_id = add_cloud_plus_text_appearance(&mut self.document, &annotation)?;
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
        let (cloud_name, text_name) = cloud_plus_native_names(id);
        if identity.cloud_raw_name != cloud_name || identity.text_raw_name != text_name {
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
        require_canonical_callout_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_callout_appearance(&mut self.document, &annotation)?;
        let dictionary = callout_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        let index =
            find_annotation_index(&self.callouts, &annotation.id, |value| &value.id, "callout")?;
        require_canonical_callout_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let old_appearance_id = normal_appearance_object_id(&original);
        let appearance_id = add_callout_appearance(&mut self.document, &annotation)?;
        let dictionary = callout_dictionary(&annotation, appearance_id, &original)?;
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
        require_canonical_measurement_path_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_measurement_path_appearance(&mut self.document, &annotation)?;
        let dictionary =
            measurement_path_dictionary(&annotation, appearance_id, &Dictionary::new())?;
        let object_id =
            append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        require_canonical_measurement_path_stable_id(&annotation.id)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let old_appearance_id = normal_appearance_object_id(&original);
        let appearance_id = add_measurement_path_appearance(&mut self.document, &annotation)?;
        let dictionary = measurement_path_dictionary(&annotation, appearance_id, &original)?;
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
        require_canonical_image_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_image_appearance(&mut self.document, &annotation);
        let dictionary = image_dictionary(&annotation, appearance_id, &Dictionary::new());
        append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        let native_name = self
            .image_native_names
            .get(&annotation.id)
            .map(String::as_str)
            .unwrap_or(annotation.id.as_str());
        let object_id = annotation_object_id(&self.document, annotation.page_index, native_name)?;
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let old_appearance_ids = image_appearance_object_ids(&self.document, &original);
        let appearance_id = add_image_appearance(&mut self.document, &annotation);
        self.document.objects.insert(
            object_id,
            Object::Dictionary(image_dictionary(&annotation, appearance_id, &original)),
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
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
        require_canonical_snapshot_stable_id(&annotation.id)?;
        self.require_unique_name(&annotation.id)?;
        let appearance_id = add_snapshot_appearance(&mut self.document, &annotation);
        let dictionary = snapshot_dictionary(&annotation, appearance_id, &Dictionary::new());
        append_native_annotation(&mut self.document, annotation.page_index, dictionary)?;
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let old_appearance_ids = image_appearance_object_ids(&self.document, &original);
        let appearance_id = if self.snapshots[index].asset() == annotation.asset() {
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
        let original = self.document.get_object(object_id)?.as_dict()?.clone();
        let old_appearance_ids = image_appearance_object_ids(&self.document, &original);
        remove_annotation_reference(&mut self.document, page_index, object_id)?;
        self.document.objects.remove(&object_id);
        for object_id in old_appearance_ids {
            remove_object_if_unreferenced(&mut self.document, object_id);
        }
        self.snapshot_native_names.remove(id);
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

    pub fn prepare_save(
        &self,
        target: impl AsRef<Path>,
    ) -> Result<PreparedPdfSave, PdfPersistenceError> {
        self.prepare_save_inner(target.as_ref(), None)
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
        let result = (|| {
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            let mut document = self.document.clone();
            write_page_scales(&mut document, &self.page_scales)?;
            write_page_rotations(
                &mut document,
                &self.page_rotations,
                &self.changed_page_rotations,
            )?;
            document.save_to(&mut output)?;
            output.flush()?;
            #[cfg(unix)]
            if let Some(guard) = replacement_guard.as_ref() {
                fs::set_permissions(&temporary, fs::Permissions::from_mode(guard.mode & 0o777))?;
            }
            output.sync_all()?;
            Ok(PreparedPdfSave {
                temporary: temporary.clone(),
                target: target.to_path_buf(),
                replacement_guard,
                #[cfg(unix)]
                authorized_stage: None,
                #[cfg(unix)]
                cleanup_owned_by_authority: false,
                published: false,
            })
        })();
        if result.is_err() {
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
        #[cfg(not(unix))]
        {
            let _ = authority;
            return Err(PdfPersistenceError::InvalidDocument(
                "authorized Save As publication is not implemented on this platform".into(),
            ));
        }
        #[cfg(unix)]
        {
            let mut stage = authority.prepare_stage()?;
            let result = (|| {
                let output = stage.file_mut();
                let mut document = self.document.clone();
                write_page_scales(&mut document, &self.page_scales)?;
                write_page_rotations(
                    &mut document,
                    &self.page_rotations,
                    &self.changed_page_rotations,
                )?;
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
                published: false,
            })
        }
    }

    /// Publishes a new file without overwriting an existing path.
    ///
    /// The complete PDF is written and synced to a same-directory temporary
    /// file. A hard link then creates the destination atomically only when it
    /// does not already exist. Removing the temporary name leaves the synced
    /// inode reachable through the destination name.
    pub fn save_as(&self, target: impl AsRef<Path>) -> Result<(), PdfPersistenceError> {
        self.prepare_save(target)?.publish().map(|_| ())
    }

    pub fn prepare_save_replacing(
        &self,
        target: impl AsRef<Path>,
    ) -> Result<PreparedPdfSave, PdfPersistenceError> {
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

fn append_native_annotation(
    document: &mut Document,
    page_index: u32,
    dictionary: Dictionary,
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
    let annotation_id = document.add_object(dictionary);
    append_annotation_reference(document, page_id, annotation_id)?;
    Ok(annotation_id)
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

fn rectangle_annotation_bounds(annotation: &RectangleAnnotation) -> PdfRect {
    if annotation.rotation_degrees == 0.0 {
        return annotation.rect;
    }
    let corners = rectangle_world_corners(annotation.rect, annotation.rotation_degrees);
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
        .expect("validated rectangle rotation has finite bounds")
}

fn add_rectangle_appearance(document: &mut Document, annotation: &RectangleAnnotation) -> ObjectId {
    let appearance = &annotation.appearance;
    let (stroke_red, stroke_green, stroke_blue) = color_components(appearance.stroke_color());
    let fill = appearance.fill_color().map(color_components);
    let paint_operator = if fill.is_some() { "B" } else { "S" };
    let fill_operation = fill.map_or_else(String::new, |(red, green, blue)| {
        format!("{red:.6} {green:.6} {blue:.6} rg\n")
    });
    let dash_operation =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
            .map_or_else(String::new, |(dash, gap)| {
                format!("[{dash:.6} {gap:.6}] 0 d\n")
            });
    let appearance_bounds = rectangle_annotation_bounds(annotation);
    let content = if annotation.rotation_degrees == 0.0 {
        let half_width = appearance.stroke_width_pt() / 2.0;
        let draw_width = (annotation.rect.width - appearance.stroke_width_pt()).max(0.0);
        let draw_height = (annotation.rect.height - appearance.stroke_width_pt()).max(0.0);
        format!(
            "q\n/GS0 gs\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n{half_width:.6} {half_width:.6} {draw_width:.6} {draw_height:.6} re {paint_operator}\nQ\n",
            appearance.stroke_width_pt(),
        )
    } else {
        let points = rectangle_world_corners(annotation.rect, annotation.rotation_degrees)
            .map(|point| PdfPoint {
                x: point.x - appearance_bounds.x,
                y: point.y - appearance_bounds.y,
            });
        format!(
            "q\n/GS0 gs\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n{:.6} {:.6} m {:.6} {:.6} l {:.6} {:.6} l {:.6} {:.6} l h {paint_operator}\nQ\n",
            appearance.stroke_width_pt(),
            points[0].x,
            points[0].y,
            points[1].x,
            points[1].y,
            points[2].x,
            points[2].y,
            points[3].x,
            points[3].y,
        )
    }
    .into_bytes();
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(appearance_bounds),
            "Resources" => dictionary! {
                "ExtGState" => dictionary! {
                    "GS0" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
                    },
                },
            },
        },
        content,
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
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => pdf_rect(rectangle_annotation_bounds(annotation)),
        "BPRect" => pdf_rect(annotation.rect),
        "BPRotation" => Object::Real(annotation.rotation_degrees as f32),
        "NM" => pdf_literal(annotation.id.as_str()),
        "C" => color_array(annotation.appearance.stroke_color()),
        "BS" => dictionary! {
            "Type" => "Border",
            "W" => Object::Real(annotation.appearance.stroke_width_pt() as f32),
            "S" => "S",
        },
        "CA" => Object::Real(annotation.appearance.opacity() as f32),
        "BPFillAlpha" => Object::Real(annotation.appearance.fill_opacity() as f32),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if let Some((dash, gap)) = rectangle_dash_pattern(
        annotation.appearance.stroke_style(),
        annotation.appearance.stroke_width_pt(),
    ) {
        replacement.set(
            "BS",
            dictionary! {
                "Type" => "Border",
                "W" => Object::Real(annotation.appearance.stroke_width_pt() as f32),
                "S" => "D",
                "D" => vec![Object::Real(dash as f32), Object::Real(gap as f32)],
            },
        );
    }
    if let Some(fill_color) = annotation.appearance.fill_color() {
        replacement.set("IC", color_array(fill_color));
    } else {
        replacement.remove(b"IC");
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    for key in [b"Subj".as_slice(), b"Contents".as_slice(), b"RC".as_slice()] {
        if let Ok(value) = original.get(key) {
            replacement.set(key, value.clone());
        }
    }
    Ok(replacement)
}

fn redact_dictionary(
    annotation: &RedactAnnotation,
    original: &Dictionary,
    native_name: &str,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let mut stored_appearance = json!({
        "stroke": {
            "color": appearance.stroke_color(),
            "widthPt": appearance.stroke_width_pt(),
        },
        "opacity": appearance.opacity(),
        "fillOpacity": appearance.fill_opacity(),
        "blendMode": "normal",
    });
    if appearance.stroke_style() != StrokeStyle::Solid {
        stored_appearance["stroke"]["style"] = Value::String(match appearance.stroke_style() {
            StrokeStyle::Solid => unreachable!(),
            StrokeStyle::Dashed => "dashed".into(),
            StrokeStyle::Dotted => "dotted".into(),
        });
    }
    if let Some(fill_color) = appearance.fill_color() {
        stored_appearance["fill"] = json!({ "color": fill_color });
    }
    let stored_appearance = serde_json::to_string(&stored_appearance)
        .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
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
        "Contents" => pdf_literal("Marked for redaction"),
        "F" => Object::Integer(4),
        "BPAppearance" => pdf_literal(&stored_appearance),
        "CA" => Object::Real(appearance.opacity() as f32),
        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
    };
    if let Some(overlay_text) = annotation.overlay_text() {
        replacement.set("OverlayText", pdf_literal(overlay_text));
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    let mut flags = original
        .get(b"F")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(4)
        | 4;
    if annotation.locked {
        flags |= 128;
    } else {
        flags &= !128;
    }
    replacement.set("F", flags);
    // A pending mark is not an applied redaction. The canonical Butter Paper
    // contract intentionally never carries an opaque `/AP` appearance.
    replacement.remove(b"AP");
    Ok(replacement)
}

fn is_canonical_managed_redact(annotation: &Dictionary, raw_name: &str) -> bool {
    let Some(stable_name) = raw_name.strip_prefix("bp:") else {
        return false;
    };
    !stable_name.is_empty()
        && dictionary_name(annotation, b"Type").as_deref() == Some("Annot")
        && dictionary_name(annotation, b"Subtype").as_deref() == Some("Redact")
        && dictionary_string(annotation, b"NM").as_deref() == Some(raw_name)
        && dictionary_string(annotation, b"Subj").as_deref() == Some("Redaction")
        && dictionary_string(annotation, b"Contents").as_deref() == Some("Marked for redaction")
        && annotation.get(b"AP").is_err()
        && annotation.get(b"BPAppearance").is_ok()
        && annotation.get(b"CA").is_ok()
        && annotation.get(b"ca").is_ok()
        && annotation.get(b"IC").is_ok()
        && annotation
            .get(b"F")
            .ok()
            .and_then(|value| value.as_i64().ok())
            .is_some_and(|flags| flags & 4 != 0)
        && canonical_redact_quad_points(annotation)
}

fn canonical_redact_quad_points(annotation: &Dictionary) -> bool {
    let Ok(rect) = import_pdf_rect(annotation, b"Rect") else {
        return false;
    };
    let Ok(values) = annotation.get(b"QuadPoints").and_then(Object::as_array) else {
        return false;
    };
    let Ok(values) = values
        .iter()
        .map(|value| value.as_float().map(f64::from))
        .collect::<Result<Vec<_>, _>>()
    else {
        return false;
    };
    let expected = [
        rect.x,
        rect.y + rect.height,
        rect.x + rect.width,
        rect.y + rect.height,
        rect.x,
        rect.y,
        rect.x + rect.width,
        rect.y,
    ];
    values.len() == expected.len()
        && values
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual - expected).abs() <= 0.000_1)
}

fn ellipse_annotation_bounds(annotation: &EllipseAnnotation) -> PdfRect {
    let angle = annotation.rotation_degrees.to_radians();
    let radius_x = annotation.rect.width * 0.5;
    let radius_y = annotation.rect.height * 0.5;
    let half_width = ((radius_x * angle.cos()).powi(2)
        + (radius_y * angle.sin()).powi(2))
    .sqrt();
    let half_height = ((radius_x * angle.sin()).powi(2)
        + (radius_y * angle.cos()).powi(2))
    .sqrt();
    let center_x = annotation.rect.x + radius_x;
    let center_y = annotation.rect.y + radius_y;
    PdfRect::new(
        center_x - half_width,
        center_y - half_height,
        half_width * 2.,
        half_height * 2.,
    )
    .expect("a validated Ellipse rotation has finite bounds")
}

fn ellipse_appearance_bounds(annotation: &EllipseAnnotation) -> PdfRect {
    let geometry = ellipse_annotation_bounds(annotation);
    let stroke_inset = annotation.appearance.stroke_width_pt() / 2.;
    PdfRect::new(
        geometry.x - stroke_inset,
        geometry.y - stroke_inset,
        geometry.width + stroke_inset * 2.,
        geometry.height + stroke_inset * 2.,
    )
    .expect("a validated Ellipse stroke has finite appearance bounds")
}

fn add_ellipse_appearance(document: &mut Document, annotation: &EllipseAnnotation) -> ObjectId {
    let appearance = &annotation.appearance;
    let bounds = ellipse_appearance_bounds(annotation);
    let (start, segments) =
        ellipse_cubic_bezier_points(annotation.rect, annotation.rotation_degrees);
    let local = |point: PdfPoint| (point.x - bounds.x, point.y - bounds.y);
    let (start_x, start_y) = local(start);
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
    let mut content = format!(
        "q\n/GS0 gs\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n{start_x:.6} {start_y:.6} m\n",
        appearance.stroke_width_pt(),
    );
    for (control_a, control_b, to) in segments {
        let (control_a_x, control_a_y) = local(control_a);
        let (control_b_x, control_b_y) = local(control_b);
        let (to_x, to_y) = local(to);
        content.push_str(&format!(
            "{control_a_x:.6} {control_a_y:.6} {control_b_x:.6} {control_b_y:.6} {to_x:.6} {to_y:.6} c\n",
        ));
    }
    content.push_str(if fill.is_some() { "B\nQ\n" } else { "S\nQ\n" });
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
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
                    },
                },
            },
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
    let mut stored_appearance = json!({
        "stroke": {
            "color": appearance.stroke_color(),
            "widthPt": appearance.stroke_width_pt(),
        },
        "opacity": appearance.opacity(),
        "blendMode": "normal",
    });
    if appearance.stroke_style() != StrokeStyle::Solid {
        stored_appearance["stroke"]["style"] = Value::String(match appearance.stroke_style() {
            StrokeStyle::Solid => unreachable!(),
            StrokeStyle::Dashed => "dashed".into(),
            StrokeStyle::Dotted => "dotted".into(),
        });
    }
    if let Some(fill_color) = appearance.fill_color() {
        stored_appearance["fill"] = json!({ "color": fill_color });
    }
    let stored_appearance = serde_json::to_string(&stored_appearance)
        .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;

    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Circle",
        "Rect" => pdf_rect(ellipse_appearance_bounds(annotation)),
        "BPRect" => pdf_rect(annotation.rect),
        "BPRotation" => Object::Real(annotation.rotation_degrees.rem_euclid(360.0) as f32),
        "Border" => vec![Object::Integer(0), Object::Integer(0), Object::Real(appearance.stroke_width_pt() as f32)],
        "BS" => dictionary! {
            "Type" => "Border",
            "W" => Object::Real(appearance.stroke_width_pt() as f32),
            "S" => "S",
        },
        "C" => color_array(appearance.stroke_color()),
        "CA" => Object::Real(appearance.opacity() as f32),
        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
        "NM" => pdf_literal(native_name),
        "Subj" => pdf_literal("Ellipse"),
        "Contents" => pdf_literal(""),
        "F" => Object::Integer(4),
        "AP" => dictionary! { "N" => appearance_id },
        "BPAppearance" => pdf_literal(&stored_appearance),
        "BPFillAlpha" => Object::Real(appearance.fill_opacity() as f32),
    };
    if let Some((dash, gap)) =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
    {
        replacement.set(
            "BS",
            dictionary! {
                "Type" => "Border",
                "W" => Object::Real(appearance.stroke_width_pt() as f32),
                "S" => "D",
                "D" => vec![Object::Real(dash as f32), Object::Real(gap as f32)],
            },
        );
    }
    if let Some(fill_color) = appearance.fill_color() {
        replacement.set("IC", color_array(fill_color));
    }
    if annotation.rotation_degrees.rem_euclid(360.0) != 0.0 {
        replacement.set(
            "Rotation",
            Object::Real(annotation.rotation_degrees.rem_euclid(360.0) as f32),
        );
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn add_arc_appearance(document: &mut Document, annotation: &ArcAnnotation) -> ObjectId {
    let appearance = &annotation.appearance;
    let rect = annotation.rect();
    let inset = appearance.stroke_width_pt() * 0.5;
    let path_rect = PdfRect::new(
        rect.x + inset,
        rect.y + inset,
        (rect.width - inset * 2.).max(f64::EPSILON),
        (rect.height - inset * 2.).max(f64::EPSILON),
    )
    .expect("a retained Arc has finite appearance bounds");
    let (red, green, blue) = color_components(appearance.stroke_color());
    let path = arc_pdf_path_commands(
        path_rect,
        annotation.angle1_degrees(),
        annotation.angle2_degrees(),
    );
    let content = format!(
        "q\n/GS0 gs\n{red:.6} {green:.6} {blue:.6} RG\n{:.6} w\n{path}S\nQ\n",
        appearance.stroke_width_pt(),
    )
    .into_bytes();
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => pdf_rect(rect),
            "Resources" => dictionary! {
                "ExtGState" => dictionary! {
                    "GS0" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real(appearance.opacity() as f32),
                    },
                },
            },
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
    let mut stored_appearance = json!({
        "stroke": {
            "color": appearance.stroke_color(),
            "widthPt": appearance.stroke_width_pt(),
        },
        "opacity": appearance.opacity(),
        "blendMode": "normal",
    });
    if appearance.stroke_style() != StrokeStyle::Solid {
        stored_appearance["stroke"]["style"] = Value::String(match appearance.stroke_style() {
            StrokeStyle::Solid => unreachable!(),
            StrokeStyle::Dashed => "dashed".into(),
            StrokeStyle::Dotted => "dotted".into(),
        });
    }
    let stored_appearance = serde_json::to_string(&stored_appearance)
        .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Circle",
        "Rect" => pdf_rect(annotation.rect()),
        "C" => color_array(appearance.stroke_color()),
        "Border" => vec![Object::Integer(0), Object::Integer(0), Object::Real(appearance.stroke_width_pt() as f32)],
        "CA" => Object::Real(appearance.opacity() as f32),
        "ca" => Object::Real(appearance.opacity() as f32),
        "BPFillAlpha" => Object::Real(appearance.fill_opacity() as f32),
        "RD" => vec![Object::Real(0.5), Object::Real(0.5), Object::Real(0.5), Object::Real(0.5)],
        "Angle1" => Object::Real(annotation.angle1_degrees() as f32),
        "Angle2" => Object::Real(annotation.angle2_degrees() as f32),
        "IT" => "CircleArc",
        "NM" => pdf_literal(native_name),
        "Subj" => pdf_literal("Arc"),
        "Contents" => pdf_literal(""),
        "F" => Object::Integer(4),
        "AP" => dictionary! { "N" => appearance_id },
        "BPAppearance" => pdf_literal(&stored_appearance),
    };
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn pen_bounds(annotation: &PenAnnotation) -> PdfRect {
    let half_width = annotation.appearance.width_pt() / 2.0;
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
        max_x - min_x + annotation.appearance.width_pt(),
        max_y - min_y + annotation.appearance.width_pt(),
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
        }
    }
    content.push_str("S\n");
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
    let bounds = pen_bounds(annotation);
    let subject = match annotation.tool() {
        InkTool::Pen => "Pen",
        InkTool::Highlight => "Highlight",
    };
    let blend = match annotation.blend_mode() {
        BlendMode::Normal => "Normal",
        BlendMode::Multiply => "Multiply",
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
    // lopdf represents native PDF real numbers as f32. Preserve Butter Paper's
    // exact finite f64 model coordinates as bit patterns in a private
    // compatibility key while keeping the standard InkList available to every
    // PDF reader.
    let canonical_point_bits = serde_json::to_string(
        &annotation
            .paths()
            .map(|path| {
                path.iter()
                    .map(|point| [point.x.to_bits(), point.y.to_bits()])
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
    )
    .expect("finite validated pen points must serialize");
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Ink",
        "Rect" => pdf_rect(bounds),
        "NM" => pdf_literal(native_name),
        "Subj" => pdf_literal(subject),
        "InkList" => paths,
        "BPCanonicalPointBits" => pdf_literal(&canonical_point_bits),
        "C" => color_array(annotation.appearance.color()),
        "CA" => Object::Real(annotation.appearance.opacity() as f32),
        "BS" => dictionary! { "Type" => "Border", "W" => Object::Real(annotation.appearance.width_pt() as f32), "S" => "S" },
        "BM" => blend,
        "AP" => dictionary! { "N" => appearance_id },
    };
    if annotation.tool() == InkTool::Pen {
        dictionary.set("BPSmoothCurves", Object::Boolean(annotation.smooth_curves));
    }
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn add_standard_font(document: &mut Document) -> ObjectId {
    document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
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

fn callout_bounds(annotation: &CalloutAnnotation) -> PdfRect {
    const PADDING_PT: f64 = 5.5;
    let mut min_x = annotation.text_box.x;
    let mut min_y = annotation.text_box.y;
    let mut max_x = annotation.text_box.x + annotation.text_box.width;
    let mut max_y = annotation.text_box.y + annotation.text_box.height;
    for point in normalize_callout_leader(annotation) {
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
    .expect("validated callout geometry has finite bounds")
}

fn add_callout_appearance(
    document: &mut Document,
    annotation: &CalloutAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let font_id = add_standard_font(document);
    let leader = normalize_callout_leader(annotation);
    let bounds = callout_bounds(annotation);
    let line = annotation.appearance.line();
    let text = annotation.appearance.text();
    let (line_red, line_green, line_blue) = color_components(line.stroke_color());
    let (text_red, text_green, text_blue) = color_components(text.color());
    let mut content = format!(
        "q\n/GS0 gs\n{line_red:.6} {line_green:.6} {line_blue:.6} RG\n{:.6} w\n",
        line.stroke_width_pt()
    );
    if let Some(first) = leader.first() {
        content.push_str(&format!(
            "{:.6} {:.6} m\n",
            first.x - bounds.x,
            first.y - bounds.y
        ));
        for point in leader.iter().skip(1) {
            content.push_str(&format!(
                "{:.6} {:.6} l\n",
                point.x - bounds.x,
                point.y - bounds.y
            ));
        }
        content.push_str("S\n");
    }
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
            content.push_str(&format!(
                "{:.6} {:.6} m\n{:.6} {:.6} l\n{:.6} {:.6} l\nS\n",
                base_x + px - bounds.x,
                base_y + py - bounds.y,
                tip.x - bounds.x,
                tip.y - bounds.y,
                base_x - px - bounds.x,
                base_y - py - bounds.y,
            ));
        }
    }
    let lines = annotation.content().split('\n').collect::<Vec<_>>();
    let line_height = text.font_size_pt() * 1.15;
    let total_height = line_height * lines.len() as f64;
    let start_y = annotation.text_box.y
        + ((annotation.text_box.height - total_height) * 0.5).max(0.)
        + total_height
        - text.font_size_pt();
    content.push_str("BT\n");
    content.push_str(&format!(
        "/Helv {:.6} Tf\n{text_red:.6} {text_green:.6} {text_blue:.6} rg\n",
        text.font_size_pt()
    ));
    for (index, line_text) in lines.iter().enumerate() {
        content.push_str(&format!(
            "1 0 0 1 {:.6} {:.6} Tm\n({}) Tj\n",
            annotation.text_box.x + 3. - bounds.x,
            start_y - index as f64 * line_height - bounds.y,
            escape_pdf_literal(line_text),
        ));
    }
    content.push_str("ET\nQ\n");
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => dictionary! { "Helv" => font_id },
                "ExtGState" => dictionary! { "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(line.opacity() as f32),
                    "ca" => Object::Real(line.opacity() as f32),
                } },
            },
        },
        content.into_bytes(),
    )))
}

fn callout_dictionary(
    annotation: &CalloutAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let bounds = callout_bounds(annotation);
    let leader = normalize_callout_leader(annotation);
    let flattened = leader
        .iter()
        .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
        .collect::<Vec<_>>();
    let text = annotation.appearance.text();
    let line = annotation.appearance.line();
    let (red, green, blue) = color_components(text.color());
    let default_appearance = format!(
        "/Helv {:.6} Tf {red:.6} {green:.6} {blue:.6} rg",
        text.font_size_pt()
    );
    let default_style = format!(
        "font: {:.6}pt {}; color: {}; text-align: left;",
        text.font_size_pt(),
        text.font_family(),
        text.color(),
    );
    let rich_content = format!("<p>{}</p>", annotation.content());
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
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "IT" => "FreeTextCallout",
        "Rect" => pdf_rect(bounds),
        "RD" => rd,
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "Subj" => pdf_literal("Callout"),
        "Contents" => pdf_literal(annotation.content()),
        "CL" => flattened,
        "LE" => vec![Object::Name(b"None".to_vec()), Object::Name(b"OpenArrow".to_vec())],
        "Q" => 0,
        "DA" => pdf_literal(&default_appearance),
        "DS" => pdf_literal(&default_style),
        "RC" => pdf_literal(&rich_content),
        "DR" => dictionary! { "Font" => dictionary! { "Helv" => dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"
        } } },
        "Border" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(0)],
        "BS" => dictionary! { "Type" => "Border", "W" => Object::Integer(0), "S" => "S" },
        "C" => Vec::<Object>::new(),
        "F" => 4,
        "CA" => Object::Real(line.opacity() as f32),
        "BPStrokeColor" => pdf_literal(line.stroke_color()),
        "BPStrokeWidth" => Object::Real(line.stroke_width_pt() as f32),
        "BPFontFamily" => pdf_literal(text.font_family()),
        "BPFontWeight" => i64::from(text.weight()),
        "AP" => dictionary! { "N" => appearance_id },
    };
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    Ok(dictionary)
}

fn add_text_appearance(document: &mut Document, annotation: &TextBoxAnnotation) -> ObjectId {
    let font_id = add_standard_font(document);
    let (red, green, blue) = color_components(annotation.style().color());
    let escaped = escape_pdf_literal(annotation.content());
    let content = format!(
        "q\n/GS0 gs\nBT\n/Helv {:.6} Tf\n{red:.6} {green:.6} {blue:.6} rg\n2 {:.6} Td\n({escaped}) Tj\nET\nQ\n",
        annotation.style().font_size_pt(),
        (annotation.layout_rect.height - annotation.style().font_size_pt()).max(0.0),
    );
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(annotation.layout_rect),
            "Resources" => dictionary! {
                "Font" => dictionary! { "Helv" => font_id },
                "ExtGState" => dictionary! { "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(annotation.style().opacity() as f32),
                    "ca" => Object::Real(annotation.style().opacity() as f32),
                } },
            },
        },
        content.into_bytes(),
    ))
}

fn text_box_dictionary(
    annotation: &TextBoxAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Dictionary {
    let (red, green, blue) = color_components(annotation.style().color());
    let default_appearance = format!(
        "/Helv {:.6} Tf {red:.6} {green:.6} {blue:.6} rg",
        annotation.style().font_size_pt()
    );
    let alignment = match annotation.style().alignment() {
        TextAlignment::Left => 0,
        TextAlignment::Center => 1,
        TextAlignment::Right => 2,
    };
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "FreeText",
        "Rect" => pdf_rect(annotation.layout_rect),
        "NM" => pdf_literal(annotation.id.as_str()),
        "Contents" => pdf_literal(annotation.content()),
        "DA" => pdf_literal(&default_appearance),
        "Q" => alignment,
        "CA" => Object::Real(annotation.style().opacity() as f32),
        "BPFontFamily" => pdf_literal(annotation.style().font_family()),
        "BPFontWeight" => i64::from(annotation.style().weight()),
        "AP" => dictionary! { "N" => appearance_id },
    };
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn length_bounds(annotation: &LengthAnnotation) -> PdfRect {
    let margin = 18.0;
    PdfRect::new(
        annotation.start.x.min(annotation.end.x) - margin,
        annotation.start.y.min(annotation.end.y) - margin,
        (annotation.end.x - annotation.start.x).abs() + margin * 2.0,
        (annotation.end.y - annotation.start.y).abs() + margin * 2.0,
    )
    .expect("validated length endpoints must have finite bounds")
}

fn straight_line_bounds(annotation: &StraightLineAnnotation) -> PdfRect {
    const PADDING_PT: f64 = 4.0;
    PdfRect::new(
        annotation.start.x.min(annotation.end.x) - PADDING_PT,
        annotation.start.y.min(annotation.end.y) - PADDING_PT,
        (annotation.end.x - annotation.start.x).abs() + PADDING_PT * 2.0,
        (annotation.end.y - annotation.start.y).abs() + PADDING_PT * 2.0,
    )
    .expect("validated straight-line endpoints must have finite padded bounds")
}

fn straight_line_dictionary(
    annotation: &StraightLineAnnotation,
    original: &Dictionary,
) -> Dictionary {
    let border_style = dictionary! {
        "Type" => "Border",
        "W" => Object::Real(annotation.appearance.stroke_width_pt() as f32),
        "S" => "S",
    };
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "Rect" => pdf_rect(straight_line_bounds(annotation)),
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "Subj" => pdf_literal(match annotation.kind {
            LineKind::Line => "Line",
            LineKind::Arrow => "Arrow",
        }),
        "Contents" => pdf_literal(""),
        "F" => 4,
        "L" => vec![
            Object::Real(annotation.start.x as f32),
            Object::Real(annotation.start.y as f32),
            Object::Real(annotation.end.x as f32),
            Object::Real(annotation.end.y as f32),
        ],
        "Border" => vec![0.into(), 0.into(), Object::Real(annotation.appearance.stroke_width_pt() as f32)],
        "BS" => border_style,
        "C" => color_array(annotation.appearance.stroke_color()),
        "CA" => Object::Real(annotation.appearance.opacity() as f32),
        "ca" => Object::Real(annotation.appearance.opacity() as f32),
    };
    if annotation.kind == LineKind::Arrow {
        dictionary.set("IT", Object::Name(b"LineArrow".to_vec()));
        dictionary.set(
            "LE",
            vec![
                Object::Name(b"None".to_vec()),
                Object::Name(b"ClosedArrow".to_vec()),
            ],
        );
        dictionary.set("IC", color_array(annotation.appearance.stroke_color()));
    }
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    for key in [b"Subj".as_slice(), b"Contents".as_slice()] {
        if let Ok(value) = original.get(key) {
            dictionary.set(key, value.clone());
        }
    }
    dictionary
}

fn vertex_path_bounds(annotation: &VertexPathAnnotation) -> PdfRect {
    let padding = annotation.appearance.stroke_width_pt() / 2.0 + 1.0;
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
    let mut content = format!(
        "q\n/GS0 gs\n1 J 1 j\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{fill_operation}{dash_operation}{:.6} w\n{:.6} {:.6} m\n",
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
            "Resources" => dictionary! {
                "ExtGState" => dictionary! {
                    "GS0" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
                    },
                },
            },
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
    let mut stored_appearance = json!({
        "stroke": {
            "color": appearance.stroke_color(),
            "widthPt": appearance.stroke_width_pt(),
        },
        "opacity": appearance.opacity(),
        "blendMode": "normal",
    });
    if appearance.stroke_style() != StrokeStyle::Solid {
        stored_appearance["stroke"]["style"] = Value::String(match appearance.stroke_style() {
            StrokeStyle::Solid => unreachable!(),
            StrokeStyle::Dashed => "dashed".into(),
            StrokeStyle::Dotted => "dotted".into(),
        });
    }
    if annotation.kind == VertexPathKind::Polygon
        && let Some(fill_color) = appearance.fill_color()
    {
        stored_appearance["fill"] = json!({ "color": fill_color });
    }
    let stored_appearance = serde_json::to_string(&stored_appearance)
        .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
    let vertices = annotation
        .points()
        .iter()
        .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
        .collect::<Vec<_>>();
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => match annotation.kind {
            VertexPathKind::Polyline => "PolyLine",
            VertexPathKind::Polygon => "Polygon",
        },
        "Rect" => pdf_rect(vertex_path_bounds(annotation)),
        "Vertices" => vertices,
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "Subj" => pdf_literal(match annotation.kind {
            VertexPathKind::Polyline => "PolyLine",
            VertexPathKind::Polygon => "Polygon",
        }),
        "Contents" => pdf_literal(""),
        "F" => Object::Integer(4),
        "C" => color_array(appearance.stroke_color()),
        "CA" => Object::Real(appearance.opacity() as f32),
        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
        "BS" => dictionary! {
            "Type" => "Border",
            "W" => Object::Real(appearance.stroke_width_pt() as f32),
            "S" => "S",
        },
        "BPAppearance" => pdf_literal(&stored_appearance),
        "BPFillAlpha" => Object::Real(appearance.fill_opacity() as f32),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if let Some((dash, gap)) =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
    {
        replacement.set(
            "BS",
            dictionary! {
                "Type" => "Border",
                "W" => Object::Real(appearance.stroke_width_pt() as f32),
                "S" => "D",
                "D" => vec![Object::Real(dash as f32), Object::Real(gap as f32)],
            },
        );
    }
    if annotation.kind == VertexPathKind::Polygon
        && let Some(fill_color) = appearance.fill_color()
    {
        replacement.set("IC", color_array(fill_color));
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    for key in [b"Subj".as_slice(), b"Contents".as_slice(), b"RC".as_slice()] {
        if let Ok(value) = original.get(key) {
            replacement.set(key, value.clone());
        }
    }
    Ok(replacement)
}

fn cloud_bounds(annotation: &CloudAnnotation) -> PdfRect {
    let path = annotation.scallop_path();
    let padding = annotation.appearance.stroke_width_pt() / 2.0 + 1.0;
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
        (max_x - min_x).max(0.0) + padding * 2.0,
        (max_y - min_y).max(0.0) + padding * 2.0,
    )
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
    let mut content = format!(
        "q\n/GS0 gs\n1 J 1 j\n{red:.6} {green:.6} {blue:.6} RG\n{:.6} w\n{:.6} {:.6} m\n",
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
    content.push_str("h S\nQ\n");
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
                        "ca" => Object::Real(appearance.opacity() as f32),
                    },
                },
            },
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
    let stored_appearance = serde_json::to_string(&json!({
        "stroke": {
            "color": appearance.stroke_color(),
            "widthPt": appearance.stroke_width_pt(),
        },
        "opacity": appearance.opacity(),
        "blendMode": "normal",
        "cloudIntensity": annotation.border_effect_intensity(),
    }))
    .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
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
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "Subj" => pdf_literal("Cloud"),
        "Contents" => pdf_literal(""),
        "F" => Object::Integer(4),
        "C" => color_array(appearance.stroke_color()),
        "CA" => Object::Real(appearance.opacity() as f32),
        "BS" => dictionary! {
            "Type" => "Border",
            "W" => Object::Real(appearance.stroke_width_pt() as f32),
            "S" => "S",
        },
        "BE" => dictionary! {
            "S" => "C",
            "I" => Object::Real(annotation.border_effect_intensity() as f32),
        },
        "BPAppearance" => pdf_literal(&stored_appearance),
        "AP" => dictionary! { "N" => appearance_id },
    };
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    for key in [b"Subj".as_slice(), b"Contents".as_slice(), b"RC".as_slice()] {
        if let Ok(value) = original.get(key) {
            replacement.set(key, value.clone());
        }
    }
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
    add_cloud_appearance(document, &cloud_plus_cloud_annotation(annotation)?)
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
    let font_id = add_standard_font(document);
    let bounds = cloud_plus_text_bounds(annotation);
    let line = annotation.appearance.leader();
    let text = annotation.appearance.text();
    let (line_red, line_green, line_blue) = color_components(line.stroke_color());
    let (text_red, text_green, text_blue) = color_components(text.color());
    let mut content = format!(
        "q\n/GS0 gs\n{line_red:.6} {line_green:.6} {line_blue:.6} RG\n{:.6} w\n",
        line.stroke_width_pt()
    );
    if let Some(first) = annotation.leader_points().first() {
        content.push_str(&format!(
            "{:.6} {:.6} m\n",
            first.x - bounds.x,
            first.y - bounds.y
        ));
        for point in annotation.leader_points().iter().skip(1) {
            content.push_str(&format!(
                "{:.6} {:.6} l\n",
                point.x - bounds.x,
                point.y - bounds.y
            ));
        }
        content.push_str("S\n");
    }
    let lines = annotation.content().split('\n').collect::<Vec<_>>();
    let line_height = text.font_size_pt() * 1.15;
    let total_height = line_height * lines.len() as f64;
    let start_y = annotation.text_box.y
        + ((annotation.text_box.height - total_height) * 0.5).max(0.)
        + total_height
        - text.font_size_pt();
    content.push_str("BT\n");
    content.push_str(&format!(
        "/Helv {:.6} Tf\n{text_red:.6} {text_green:.6} {text_blue:.6} rg\n",
        text.font_size_pt()
    ));
    for (index, line_text) in lines.iter().enumerate() {
        content.push_str(&format!(
            "1 0 0 1 {:.6} {:.6} Tm\n({}) Tj\n",
            annotation.text_box.x + 3. - bounds.x,
            start_y - index as f64 * line_height - bounds.y,
            escape_pdf_literal(line_text),
        ));
    }
    content.push_str("ET\nQ\n");
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => dictionary! { "Helv" => font_id },
                "ExtGState" => dictionary! { "GS0" => dictionary! {
                    "Type" => "ExtGState",
                    "CA" => Object::Real(line.opacity() as f32),
                    "ca" => Object::Real(line.opacity() as f32),
                } },
            },
        },
        content.into_bytes(),
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
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let bounds = cloud_plus_text_bounds(annotation);
    let flattened = annotation
        .leader_points()
        .iter()
        .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
        .collect::<Vec<_>>();
    let text = annotation.appearance.text();
    let line = annotation.appearance.leader();
    let (red, green, blue) = color_components(text.color());
    let default_appearance = format!(
        "/Helv {:.6} Tf {red:.6} {green:.6} {blue:.6} rg",
        text.font_size_pt()
    );
    let default_style = format!(
        "font: {:.6}pt {}; color: {}; text-align: left;",
        text.font_size_pt(),
        text.font_family(),
        text.color(),
    );
    let rich_content = format!("<p>{}</p>", escape_xml_text(annotation.content()));
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
        "Contents" => pdf_literal(annotation.content()),
        "CL" => flattened,
        "LE" => vec![Object::Name(b"None".to_vec()), Object::Name(b"None".to_vec())],
        "Q" => 0,
        "DA" => pdf_literal(&default_appearance),
        "DS" => pdf_literal(&default_style),
        "RC" => pdf_literal(&rich_content),
        "DR" => dictionary! { "Font" => dictionary! { "Helv" => dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"
        } } },
        "Border" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(0)],
        "BS" => dictionary! { "Type" => "Border", "W" => Object::Integer(0), "S" => "S" },
        "C" => Vec::<Object>::new(),
        "F" => 4,
        "CA" => Object::Real(line.opacity() as f32),
        "BPStrokeColor" => pdf_literal(line.stroke_color()),
        "BPStrokeWidth" => Object::Real(line.stroke_width_pt() as f32),
        "BPFontFamily" => pdf_literal(text.font_family()),
        "BPFontWeight" => i64::from(text.weight()),
        "GroupNesting" => vec![
            pdf_literal("Cloud+"),
            pdf_literal(text_name),
            pdf_literal(cloud_name),
        ],
        "AP" => dictionary! { "N" => appearance_id },
    };
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    replacement.set("NM", pdf_literal(text_name));
    replacement.set("Subj", pdf_literal("Cloud+"));
    replacement.set("IT", Object::Name(b"FreeTextCallout".to_vec()));
    replacement.set("ITEx", Object::Name(b"PolyText".to_vec()));
    replacement.set(
        "GroupNesting",
        vec![
            pdf_literal("Cloud+"),
            pdf_literal(text_name),
            pdf_literal(cloud_name),
        ],
    );
    Ok(replacement)
}

fn measurement_path_bounds(annotation: &MeasurementPathAnnotation) -> PdfRect {
    const CAPTION_MARGIN_PT: f64 = 18.0;
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
        min_x - CAPTION_MARGIN_PT,
        min_y - CAPTION_MARGIN_PT,
        (max_x - min_x).max(0.0) + CAPTION_MARGIN_PT * 2.0,
        (max_y - min_y).max(0.0) + CAPTION_MARGIN_PT * 2.0,
    )
    .expect("validated measurement-path points must have finite caption-aware bounds")
}

fn add_measurement_path_appearance(
    document: &mut Document,
    annotation: &MeasurementPathAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let bounds = measurement_path_bounds(annotation);
    let appearance = &annotation.appearance;
    let font_id = add_standard_font(document);
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
    if annotation.calibration().show_caption() {
        let caption_x = annotation.points().iter().map(|point| point.x).sum::<f64>()
            / annotation.points().len() as f64
            - bounds.x;
        let caption_y = annotation.points().iter().map(|point| point.y).sum::<f64>()
            / annotation.points().len() as f64
            - bounds.y;
        let escaped = escape_pdf_literal(&annotation.caption());
        content.push_str(&format!(
            "q\n/GSText gs\nBT {stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} rg /Helv 12 Tf 1 0 0 1 {caption_x:.6} {caption_y:.6} Tm ({escaped}) Tj ET\nQ\n"
        ));
    }
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => dictionary! { "Helv" => font_id },
                "ExtGState" => dictionary! {
                    "GSPath" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
                    },
                    "GSText" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(appearance.opacity() as f32),
                        "ca" => Object::Real(appearance.opacity() as f32),
                    },
                },
            },
        },
        content.into_bytes(),
    )))
}

fn measurement_path_dictionary(
    annotation: &MeasurementPathAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let appearance = &annotation.appearance;
    let calibration = annotation.calibration();
    let caption = annotation.caption();
    let conversion = calibration.units_per_point() as f32;
    let decimal_divisor = 10_i64.pow(u32::from(calibration.precision()));
    let number_format = |unit: &str, factor: f32, force_decimal: bool| {
        let mut format = dictionary! {
            "Type" => "NumberFormat",
            "U" => pdf_literal(unit),
            "C" => Object::Real(factor),
            "D" => decimal_divisor,
            "SS" => pdf_literal(""),
        };
        if force_decimal {
            format.set("FD", Object::Boolean(true));
        }
        Object::Dictionary(format)
    };
    let ratio = if calibration.label().is_empty() {
        format!(
            "{} {} = {} pt",
            calibration.real_world_value(),
            calibration.unit(),
            calibration.paper_points()
        )
    } else {
        calibration.label().to_owned()
    };
    let vertices = annotation
        .points()
        .iter()
        .flat_map(|point| [Object::Real(point.x as f32), Object::Real(point.y as f32)])
        .collect::<Vec<_>>();
    let area_unit = format!("{}^2", calibration.unit());
    let volume_unit = format!("{}^3", calibration.unit());
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
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "Border" => vec![0.into(), 0.into(), Object::Real(appearance.stroke_width_pt() as f32)],
        "BS" => dictionary! {
            "Type" => "Border",
            "W" => Object::Real(appearance.stroke_width_pt() as f32),
            "S" => "S",
        },
        "C" => color_array(appearance.stroke_color()),
        "Cap" => Object::Boolean(calibration.show_caption()),
        "AlignOnSegment" => Object::Boolean(true),
        "MeasurementTypes" => match annotation.kind {
            MeasurementPathKind::Polylength => 130,
            MeasurementPathKind::Area => 129,
        },
        "Measure" => dictionary! {
            "Type" => "Measure",
            "Subtype" => "RL",
            "R" => pdf_literal(&ratio),
            "X" => vec![number_format(calibration.unit(), conversion, false)],
            "D" => vec![number_format(calibration.unit(), 1., false)],
            "A" => vec![number_format(&area_unit, 1., true)],
            "T" => vec![number_format("°", 1., true)],
            "V" => vec![number_format(&volume_unit, 1., true)],
            "TargetUnitConversion" => Object::Real(conversion),
        },
        "Contents" => pdf_literal(&caption),
        "RC" => pdf_literal(&format!("<p>{}</p>", escape_xml_text(&caption))),
        "Label" => pdf_literal(""),
        "DA" => pdf_literal("1 0 0 rg /Helv 12 Tf"),
        "DS" => pdf_literal("font: Helvetica 12pt; text-align:center; line-height:13.8pt; color:#FF0000"),
        "CA" => Object::Real(appearance.opacity() as f32),
        "ca" => Object::Real((appearance.opacity() * appearance.fill_opacity()) as f32),
        "F" => 4,
        "BPScale" => dictionary! {
            "PaperPoints" => Object::Real(calibration.paper_points() as f32),
            "RealWorldValue" => Object::Real(calibration.real_world_value() as f32),
            "Unit" => pdf_literal(calibration.unit()),
            "Precision" => i64::from(calibration.precision()),
            "Label" => pdf_literal(calibration.label()),
            "ShowCaption" => Object::Boolean(calibration.show_caption()),
        },
        "BPFillAlpha" => Object::Real(appearance.fill_opacity() as f32),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if annotation.kind == MeasurementPathKind::Area
        && let Some(fill_color) = appearance.fill_color()
    {
        replacement.set("IC", color_array(fill_color));
    }
    if let Some((dash, gap)) =
        rectangle_dash_pattern(appearance.stroke_style(), appearance.stroke_width_pt())
    {
        replacement.set(
            "BS",
            dictionary! {
                "Type" => "Border",
                "W" => Object::Real(appearance.stroke_width_pt() as f32),
                "S" => "D",
                "D" => vec![Object::Real(dash as f32), Object::Real(gap as f32)],
            },
        );
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn add_length_appearance(document: &mut Document, annotation: &LengthAnnotation) -> ObjectId {
    let bounds = length_bounds(annotation);
    let font_id = add_standard_font(document);
    let start_x = annotation.start.x - bounds.x;
    let start_y = annotation.start.y - bounds.y;
    let end_x = annotation.end.x - bounds.x;
    let end_y = annotation.end.y - bounds.y;
    let mut content =
        format!("q\n1 0 0 RG 1 w\n{start_x:.6} {start_y:.6} m {end_x:.6} {end_y:.6} l S\n");
    if annotation.calibration().show_caption() {
        let escaped = escape_pdf_literal(&annotation.caption());
        content.push_str(&format!(
            "BT 1 0 0 rg /Helv 12 Tf {:.6} {:.6} Td ({escaped}) Tj ET\n",
            (start_x + end_x) / 2.0,
            (start_y + end_y) / 2.0 + 3.0,
        ));
    }
    content.push_str("Q\n");
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! { "Font" => dictionary! { "Helv" => font_id } },
        },
        content.into_bytes(),
    ))
}

fn length_dictionary(
    annotation: &LengthAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Dictionary {
    let calibration = annotation.calibration();
    let ratio = format!(
        "{} {} = {} pt",
        calibration.real_world_value(),
        calibration.unit(),
        calibration.paper_points()
    );
    let conversion = calibration.units_per_point() as f32;
    let decimal_divisor = 10_i64.pow(u32::from(calibration.precision()));
    let number_format = || {
        dictionary! {
            "Type" => "NumberFormat",
            "U" => pdf_literal(calibration.unit()),
            "C" => Object::Real(conversion),
            "D" => decimal_divisor,
            "SS" => pdf_literal(""),
        }
    };
    let caption = annotation.caption();
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "IT" => "LineDimension",
        "Subj" => pdf_literal("Length Measurement"),
        "Rect" => pdf_rect(length_bounds(annotation)),
        "NM" => pdf_literal(&format!("bp:{}", annotation.id)),
        "L" => vec![
            Object::Real(annotation.start.x as f32),
            Object::Real(annotation.start.y as f32),
            Object::Real(annotation.end.x as f32),
            Object::Real(annotation.end.y as f32),
        ],
        "Border" => vec![0.into(), 0.into(), 1.into()],
        "BS" => dictionary! { "Type" => "Border", "W" => 1, "S" => "S" },
        "C" => vec![Object::Real(1.), Object::Real(0.), Object::Real(0.)],
        "LE" => vec![Object::Name(b"ClosedArrow".to_vec()), Object::Name(b"ClosedArrow".to_vec())],
        "LL" => 10,
        "LLE" => 2,
        "Contents" => pdf_literal(&caption),
        "Cap" => Object::Boolean(calibration.show_caption()),
        "MeasurementTypes" => 130,
        "Measure" => dictionary! {
            "Type" => "Measure",
            "Subtype" => "RL",
            "R" => pdf_literal(&ratio),
            "X" => vec![Object::Dictionary(number_format())],
            "D" => vec![Object::Dictionary(number_format())],
            "A" => vec![Object::Dictionary(number_format())],
            "T" => vec![Object::Dictionary(number_format())],
            "V" => vec![Object::Dictionary(number_format())],
            "TargetUnitConversion" => Object::Real(conversion),
        },
        "Label" => pdf_literal(calibration.label()),
        "DA" => pdf_literal("1 0 0 rg /Helv 12 Tf"),
        "DS" => pdf_literal("font: Helvetica 12pt; text-align:center; line-height:13.8pt; color:#FF0000"),
        "RC" => pdf_literal(&format!("<p>{}</p>", escape_xml_text(&caption))),
        "CA" => Object::Real(1.),
        "ca" => Object::Real(1.),
        "F" => 4,
        "BPScale" => dictionary! {
            "PaperPoints" => Object::Real(calibration.paper_points() as f32),
            "RealWorldValue" => Object::Real(calibration.real_world_value() as f32),
            "Unit" => pdf_literal(calibration.unit()),
            "Precision" => i64::from(calibration.precision()),
            "Label" => pdf_literal(calibration.label()),
            "ShowCaption" => Object::Boolean(calibration.show_caption()),
        },
        "AP" => dictionary! { "N" => appearance_id },
    };
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn dimension_geometry(
    annotation: &DimensionAnnotation,
) -> (PdfPoint, PdfPoint, PdfPoint, PdfPoint) {
    let (dimension_start, dimension_end) = annotation.dimension_line_points();
    let delta_x = annotation.end.x - annotation.start.x;
    let delta_y = annotation.end.y - annotation.start.y;
    let length = delta_x.hypot(delta_y);
    let normal_x = -delta_y / length;
    let normal_y = delta_x / length;
    let overhang = if annotation.dimension_line_offset() >= 0. {
        4.
    } else {
        -4.
    };
    (
        dimension_start,
        dimension_end,
        PdfPoint {
            x: dimension_start.x + normal_x * overhang,
            y: dimension_start.y + normal_y * overhang,
        },
        PdfPoint {
            x: dimension_end.x + normal_x * overhang,
            y: dimension_end.y + normal_y * overhang,
        },
    )
}

fn dimension_bounds(annotation: &DimensionAnnotation) -> PdfRect {
    let (dimension_start, dimension_end, extension_start, extension_end) =
        dimension_geometry(annotation);
    let caption = annotation.caption_center();
    let caption_half_width = (annotation.content().chars().count() as f64
        * annotation.appearance.text().font_size_pt()
        * 0.3
        + 4.)
        .max(8.);
    let caption_half_height = annotation.appearance.text().font_size_pt() * (13. / 24.) + 2.;
    let points = [
        annotation.start,
        annotation.end,
        dimension_start,
        dimension_end,
        extension_start,
        extension_end,
        PdfPoint {
            x: caption.x - caption_half_width,
            y: caption.y - caption_half_height,
        },
        PdfPoint {
            x: caption.x + caption_half_width,
            y: caption.y + caption_half_height,
        },
    ];
    let margin = annotation.appearance.line().stroke_width_pt().max(1.) + 2.;
    let min_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    PdfRect::new(
        min_x - margin,
        min_y - margin,
        max_x - min_x + margin * 2.,
        max_y - min_y + margin * 2.,
    )
    .expect("validated Dimension geometry has finite bounds")
}

fn add_dimension_appearance(
    document: &mut Document,
    annotation: &DimensionAnnotation,
) -> Result<ObjectId, PdfPersistenceError> {
    let bounds = dimension_bounds(annotation);
    let line = annotation.appearance.line();
    let text = annotation.appearance.text();
    let (dimension_start, dimension_end, extension_start, extension_end) =
        dimension_geometry(annotation);
    let local = |point: PdfPoint| (point.x - bounds.x, point.y - bounds.y);
    let (stroke_red, stroke_green, stroke_blue) = color_components(line.stroke_color());
    let (text_red, text_green, text_blue) = color_components(text.color());
    let dash_operation = rectangle_dash_pattern(line.stroke_style(), line.stroke_width_pt())
        .map_or_else(String::new, |(dash, gap)| {
            format!("[{dash:.6} {gap:.6}] 0 d\n")
        });
    let mut content = format!(
        "q\n/GSDimension gs\n{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} RG\n{dash_operation}{:.6} w\n",
        line.stroke_width_pt(),
    );
    for (from, to) in [
        (extension_start, annotation.start),
        (annotation.start, dimension_start),
        (extension_end, annotation.end),
        (annotation.end, dimension_end),
        (dimension_start, dimension_end),
    ] {
        let (from_x, from_y) = local(from);
        let (to_x, to_y) = local(to);
        content.push_str(&format!(
            "{from_x:.6} {from_y:.6} m {to_x:.6} {to_y:.6} l S\n"
        ));
    }
    let arrow = |from: PdfPoint, to: PdfPoint| {
        let dx = to.x - from.x;
        let dy = to.y - from.y;
        let distance = dx.hypot(dy);
        let unit_x = dx / distance;
        let unit_y = dy / distance;
        let arrow_length = (line.stroke_width_pt() * 8.).max(7.);
        let half_width = (line.stroke_width_pt() * 5.).max(4.) * 0.5;
        let base_x = to.x - unit_x * arrow_length;
        let base_y = to.y - unit_y * arrow_length;
        [
            to,
            PdfPoint {
                x: base_x - unit_y * half_width,
                y: base_y + unit_x * half_width,
            },
            PdfPoint {
                x: base_x + unit_y * half_width,
                y: base_y - unit_x * half_width,
            },
        ]
    };
    content.push_str(&format!(
        "{stroke_red:.6} {stroke_green:.6} {stroke_blue:.6} rg\n"
    ));
    for points in [
        arrow(dimension_end, dimension_start),
        arrow(dimension_start, dimension_end),
    ] {
        let [tip, left, right] = points.map(local);
        content.push_str(&format!(
            "{:.6} {:.6} m {:.6} {:.6} l {:.6} {:.6} l h f\n",
            tip.0, tip.1, left.0, left.1, right.0, right.1,
        ));
    }
    let caption = annotation.caption_center();
    let (caption_x, caption_y) = local(caption);
    content.push_str(&format!(
        "BT {text_red:.6} {text_green:.6} {text_blue:.6} rg /Helv {:.6} Tf 1 0 0 1 {caption_x:.6} {caption_y:.6} Tm ({}) Tj ET\nQ\n",
        text.font_size_pt(),
        escape_pdf_literal(annotation.content()),
    ));
    let font_id = add_standard_font(document);
    Ok(document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(bounds),
            "Resources" => dictionary! {
                "ProcSet" => vec![Object::Name(b"PDF".to_vec()), Object::Name(b"Text".to_vec())],
                "Font" => dictionary! { "Helv" => font_id },
                "ExtGState" => dictionary! {
                    "GSDimension" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(line.opacity() as f32),
                        "ca" => Object::Real(text.opacity() as f32),
                    },
                },
            },
        },
        content.into_bytes(),
    )))
}

fn dimension_dictionary(
    annotation: &DimensionAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Result<Dictionary, PdfPersistenceError> {
    let line = annotation.appearance.line();
    let text = annotation.appearance.text();
    let stored_appearance = serde_json::to_string(&json!({
        "stroke": {
            "color": line.stroke_color(),
            "widthPt": line.stroke_width_pt(),
            "style": match line.stroke_style() {
                StrokeStyle::Solid => "solid",
                StrokeStyle::Dashed => "dashed",
                StrokeStyle::Dotted => "dotted",
            },
        },
        "text": {
            "fontFamily": text.font_family(),
            "fontSizePt": text.font_size_pt(),
            "color": text.color(),
        },
        "opacity": line.opacity(),
    }))
    .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
    let (text_red, text_green, text_blue) = color_components(text.color());
    let mut replacement = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Line",
        "IT" => "LineDimension",
        "Subj" => pdf_literal("Dimension"),
        "Rect" => pdf_rect(dimension_bounds(annotation)),
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "L" => vec![
            Object::Real(annotation.start.x as f32),
            Object::Real(annotation.start.y as f32),
            Object::Real(annotation.end.x as f32),
            Object::Real(annotation.end.y as f32),
        ],
        "Border" => vec![0.into(), 0.into(), Object::Real(line.stroke_width_pt() as f32)],
        "BS" => dictionary! {
            "Type" => "Border",
            "W" => Object::Real(line.stroke_width_pt() as f32),
            "S" => "S",
        },
        "C" => color_array(line.stroke_color()),
        "LE" => vec![Object::Name(b"ClosedArrow".to_vec()), Object::Name(b"ClosedArrow".to_vec())],
        "LL" => Object::Real(annotation.dimension_line_offset() as f32),
        "LLE" => Object::Real(4.),
        "Cap" => Object::Boolean(true),
        "Contents" => pdf_literal(annotation.content()),
        "RC" => pdf_literal(&format!("<p>{}</p>", escape_xml_text(annotation.content()))),
        "DA" => pdf_literal(&format!(
            "{text_red:.6} {text_green:.6} {text_blue:.6} rg /Helv {:.6} Tf",
            text.font_size_pt(),
        )),
        "DS" => pdf_literal(&format!(
            "font: {} {:.6}pt; text-align:center; line-height:{:.6}pt; color:{}",
            text.font_family(),
            text.font_size_pt(),
            text.font_size_pt() * (13. / 12.),
            text.color().to_ascii_uppercase(),
        )),
        "CA" => Object::Real(line.opacity() as f32),
        "ca" => Object::Real(text.opacity() as f32),
        "F" => 4,
        "BPAppearance" => pdf_literal(&stored_appearance),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if let Some((dash, gap)) = rectangle_dash_pattern(line.stroke_style(), line.stroke_width_pt()) {
        replacement.set(
            "BS",
            dictionary! {
                "Type" => "Border",
                "W" => Object::Real(line.stroke_width_pt() as f32),
                "S" => "D",
                "D" => vec![Object::Real(dash as f32), Object::Real(gap as f32)],
            },
        );
    }
    preserve_annotation_metadata(&mut replacement, original, annotation.locked);
    Ok(replacement)
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn add_image_appearance(document: &mut Document, annotation: &ImageAnnotation) -> ObjectId {
    let asset = annotation.asset();
    let mut rgb = Vec::with_capacity(asset.rgba().len() / 4 * 3);
    let mut alpha = Vec::with_capacity(asset.rgba().len() / 4);
    for pixel in asset.rgba().chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
        alpha.push(pixel[3]);
    }
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
    let image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(asset.width_px()),
            "Height" => i64::from(asset.height_px()),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "SMask" => alpha_id,
        },
        rgb,
    ));
    let content = format!(
        "q\n{:.6} 0 0 {:.6} 0 0 cm\n/Im0 Do\nQ\n",
        annotation.rect.width, annotation.rect.height
    );
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(annotation.rect),
            "Resources" => dictionary! { "XObject" => dictionary! { "Im0" => image_id } },
        },
        content.into_bytes(),
    ))
}

fn image_dictionary(
    annotation: &ImageAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Dictionary {
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "IT" => "SquareImage",
        "Subj" => pdf_literal("Image"),
        "Rect" => pdf_rect(annotation.rect),
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "BPAssetId" => pdf_literal(annotation.asset().id().as_str()),
        "BPAspectLocked" => Object::Boolean(annotation.aspect_locked),
        "AP" => dictionary! { "N" => appearance_id },
    };
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    dictionary
}

fn add_snapshot_appearance(document: &mut Document, annotation: &SnapshotAnnotation) -> ObjectId {
    let asset = annotation.asset();
    let mut rgb = Vec::with_capacity(asset.rgba().len() / 4 * 3);
    let mut alpha = Vec::with_capacity(asset.rgba().len() / 4);
    for pixel in asset.rgba().chunks_exact(4) {
        rgb.extend_from_slice(&pixel[..3]);
        alpha.push(pixel[3]);
    }
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
    let image_id = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => i64::from(asset.width_px()),
            "Height" => i64::from(asset.height_px()),
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "SMask" => alpha_id,
        },
        rgb,
    ));
    add_snapshot_form_appearance(document, annotation, image_id)
}

fn add_snapshot_form_appearance(
    document: &mut Document,
    annotation: &SnapshotAnnotation,
    image_id: ObjectId,
) -> ObjectId {
    let content = format!(
        "q\n/GS0 gs\n{:.6} 0 0 {:.6} 0 0 cm\n/Im0 Do\nQ\n",
        annotation.rect.width, annotation.rect.height
    );
    document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => rect_bbox(annotation.rect),
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Im0" => image_id },
                "ExtGState" => dictionary! {
                    "GS0" => dictionary! {
                        "Type" => "ExtGState",
                        "CA" => Object::Real(annotation.opacity() as f32),
                        "ca" => Object::Real(annotation.opacity() as f32),
                    }
                },
            },
        },
        content.into_bytes(),
    ))
}

fn snapshot_dictionary(
    annotation: &SnapshotAnnotation,
    appearance_id: ObjectId,
    original: &Dictionary,
) -> Dictionary {
    let mut dictionary = dictionary! {
        "Type" => "Annot",
        "Subtype" => "Stamp",
        "IT" => "StampSnapshot",
        "Subj" => pdf_literal("Snapshot"),
        "Contents" => pdf_literal(""),
        "Rect" => pdf_rect(annotation.rect),
        "NM" => pdf_literal(&canonical_native_annotation_name(&annotation.id)),
        "BPAssetId" => pdf_literal(annotation.asset().id().as_str()),
        "CA" => Object::Real(annotation.opacity() as f32),
        "ca" => Object::Real(annotation.opacity() as f32),
        "AP" => dictionary! { "N" => appearance_id },
    };
    if annotation.rotation_degrees().abs() > f64::EPSILON {
        dictionary.set(
            "Rotation",
            Object::Real(annotation.rotation_degrees() as f32),
        );
    }
    preserve_annotation_metadata(&mut dictionary, original, annotation.locked);
    let mut flags = original
        .get(b"F")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(4)
        | 4;
    if annotation.locked {
        flags |= 128;
    } else {
        flags &= !128;
    }
    dictionary.set("F", flags);
    dictionary
}

fn canonical_native_annotation_name(id: &MarkupId) -> String {
    format!(
        "bp:{}",
        id.as_str().strip_prefix("bp:").unwrap_or(id.as_str())
    )
}

fn cloud_plus_native_names(id: &MarkupId) -> (String, String) {
    let base = canonical_native_annotation_name(id);
    (format!("{base}:cloud"), format!("{base}:text"))
}

fn require_page(document: &Document, page_index: u32) -> Result<ObjectId, PdfPersistenceError> {
    let page_number = page_index.checked_add(1).ok_or_else(|| {
        PdfPersistenceError::InvalidDocument("page index exceeds the PDF page limit".into())
    })?;
    document
        .get_pages()
        .get(&page_number)
        .copied()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(format!("page {page_index} does not exist"))
        })
}

fn require_canonical_pen_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned Ink id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_ellipse_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned Ellipse id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_redact_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned pending Redact id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_straight_line_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned straight-line id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_vertex_path_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned vertex-path id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_cloud_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned cloud id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_cloud_plus_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:")
        || id.as_str().ends_with(":cloud")
        || id.as_str().ends_with(":text")
    {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned Cloud+ id {id} must not contain native bp: or role suffixes"
        )));
    }
    Ok(())
}

fn require_canonical_callout_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned callout id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_measurement_path_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned measurement-path id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_image_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned Image id {id} must not include the reserved native bp: prefix"
        )));
    }
    Ok(())
}

fn require_canonical_snapshot_stable_id(id: &MarkupId) -> Result<(), PdfPersistenceError> {
    if id.as_str().starts_with("bp:") {
        return Err(PdfPersistenceError::InvalidDocument(format!(
            "application-owned Snapshot id {id} must not include the reserved native bp: prefix"
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
        .and_then(|xobjects| xobjects.get(b"Im0").ok())
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
    Object::String(value.as_bytes().to_vec(), StringFormat::Literal)
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

fn escape_pdf_literal(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace('\r', "")
        .replace('\n', "\\n")
}

fn preserve_annotation_metadata(replacement: &mut Dictionary, original: &Dictionary, locked: bool) {
    let original_flags = original
        .get(b"F")
        .ok()
        .and_then(|value| value.as_i64().ok());
    if let Some(mut flags) = original_flags {
        if locked {
            flags |= 128;
        } else {
            flags &= !128;
        }
        replacement.set("F", flags);
    } else if locked {
        replacement.set("F", 128);
    }
    for key in [
        b"T".as_slice(),
        b"M".as_slice(),
        b"CreationDate".as_slice(),
        b"StateModel".as_slice(),
        b"State".as_slice(),
    ] {
        if let Ok(value) = original.get(key) {
            replacement.set(key, value.clone());
        }
    }
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

struct ImportedAnnotations {
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
    length_native_names: HashMap<MarkupId, String>,
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
    annotation_order: Vec<MarkupId>,
    untouched: Vec<UntouchedAnnotation>,
}

const PAGE_SCALE_DICTIONARY_KEY: &[u8] = b"BPPageScale";

fn import_page_scales(document: &Document) -> Vec<PageScale> {
    let mut scales = Vec::new();
    for (page_number, page_id) in document.get_pages() {
        let Ok(page) = document.get_object(page_id).and_then(Object::as_dict) else {
            continue;
        };
        let Some(serialized) = dictionary_string(page, PAGE_SCALE_DICTIONARY_KEY) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&serialized) else {
            continue;
        };
        let Some(source) = value
            .get("source")
            .and_then(Value::as_str)
            .and_then(|source| match source {
                "preset" => Some(ScaleSource::Preset),
                "custom" => Some(ScaleSource::Custom),
                "calibrated" => Some(ScaleSource::Calibrated),
                _ => None,
            })
        else {
            continue;
        };
        let Some(name) = value.get("name").and_then(Value::as_str) else {
            continue;
        };
        let Some(pdf_units) = value
            .get("pdfUnits")
            .and_then(Value::as_str)
            .and_then(|unit| ScaleUnit::parse(unit).ok())
        else {
            continue;
        };
        let Some(real_units) = value.get("realUnits").and_then(Value::as_str) else {
            continue;
        };
        let Ok(real_units) = ScaleUnit::parse(real_units) else {
            continue;
        };
        let Some(scale_x) = value.get("scaleX").and_then(Value::as_f64) else {
            continue;
        };
        let Some(scale_y) = value.get("scaleY").and_then(Value::as_f64) else {
            continue;
        };
        let Some(precision_mode) = value.pointer("/precision/mode").and_then(Value::as_str) else {
            continue;
        };
        let precision_value = value
            .pointer("/precision/value")
            .and_then(Value::as_f64)
            .unwrap_or(0.);
        let precision = match precision_mode {
            "decimal" => ScalePrecision::decimal(precision_value),
            "fraction"
                if precision_value.fract() == 0. && precision_value <= f64::from(u16::MAX) =>
            {
                ScalePrecision::fraction(precision_value as u16)
            }
            _ => continue,
        };
        let Ok(precision) = precision else {
            continue;
        };
        if let Ok(scale) = PageScale::from_factors(
            page_number.saturating_sub(1),
            source,
            name,
            pdf_units,
            real_units,
            scale_x,
            scale_y,
            precision,
        ) {
            scales.push(scale);
        }
    }
    scales
}

fn write_page_scales(
    document: &mut Document,
    scales: &[PageScale],
) -> Result<(), PdfPersistenceError> {
    let pages = document.get_pages();
    for (page_number, page_id) in pages {
        let page_index = page_number.saturating_sub(1);
        let page = document.get_object_mut(page_id)?.as_dict_mut()?;
        if let Some(scale) = scales.iter().find(|scale| scale.page_index == page_index) {
            let serialized = serde_json::to_string(&json!({
                "pageIndex": page_index,
                "source": scale.source.as_str(),
                "name": scale.name.as_str(),
                "pdfUnits": scale.pdf_units.as_str(),
                "realUnits": scale.real_units.as_str(),
                "scaleX": scale.scale_x,
                "scaleY": scale.scale_y,
                "precision": {
                    "mode": match scale.precision.mode {
                        ScalePrecisionMode::Decimal => "decimal",
                        ScalePrecisionMode::Fraction => "fraction",
                    },
                    "value": scale.precision.value,
                },
            }))
            .map_err(|error| PdfPersistenceError::InvalidDocument(error.to_string()))?;
            page.set(
                PAGE_SCALE_DICTIONARY_KEY,
                Object::String(serialized.into_bytes(), StringFormat::Literal),
            );
        } else {
            page.remove(PAGE_SCALE_DICTIONARY_KEY);
        }
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

fn exact_managed_cloud_plus_member(
    annotation_index: usize,
    annotation_object: &Object,
    annotation: &Dictionary,
) -> Option<CloudPlusPairMember> {
    let Object::Reference(object_id) = annotation_object else {
        return None;
    };
    if !is_cloud_plus_fragment(annotation) {
        return None;
    }
    let raw_name = dictionary_string(annotation, b"NM")?;
    let managed = raw_name.strip_prefix("bp:")?;
    let (stable_name, role) = if let Some(stable_name) = managed.strip_suffix(":cloud") {
        if dictionary_name(annotation, b"Subtype").as_deref() != Some("Polygon") {
            return None;
        }
        (stable_name, CloudPlusRole::Cloud)
    } else if let Some(stable_name) = managed.strip_suffix(":text") {
        if dictionary_name(annotation, b"Subtype").as_deref() != Some("FreeText") {
            return None;
        }
        (stable_name, CloudPlusRole::Text)
    } else {
        return None;
    };
    if stable_name.is_empty() {
        return None;
    }
    let stable_name = stable_name.to_owned();
    Some(CloudPlusPairMember {
        annotation_index,
        object_id: *object_id,
        raw_name,
        stable_name,
        role,
    })
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
            stable_name: raw_name.strip_prefix("bp:").unwrap_or(&raw_name).to_owned(),
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
    let mut members = BTreeMap::<String, Vec<CloudPlusPairMember>>::new();
    for (annotation_index, annotation_object) in annotations.iter().enumerate() {
        let annotation = resolve_object(document, annotation_object)?.as_dict()?;
        if let Some(member) =
            exact_managed_cloud_plus_member(annotation_index, annotation_object, annotation)
        {
            members
                .entry(member.stable_name.clone())
                .or_default()
                .push(member);
        }
    }

    let mut by_first_index = HashMap::new();
    let mut consumed_indices = HashSet::new();
    for grouped in members.into_values() {
        if grouped.len() != 2 {
            continue;
        }
        let Some(cloud) = grouped
            .iter()
            .find(|member| member.role == CloudPlusRole::Cloud)
            .cloned()
        else {
            continue;
        };
        let Some(text) = grouped
            .iter()
            .find(|member| member.role == CloudPlusRole::Text)
            .cloned()
        else {
            continue;
        };
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
        let stable_name = cloud
            .raw_name
            .strip_prefix("bp:")
            .unwrap_or(&cloud.raw_name)
            .to_owned();
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

fn import_annotations(
    document: &Document,
    page_length_calibrations: &BTreeMap<u32, LengthCalibration>,
) -> Result<ImportedAnnotations, PdfPersistenceError> {
    let mut imported = ImportedAnnotations {
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
        length_native_names: HashMap::new(),
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
        annotation_order: Vec::new(),
        untouched: Vec::new(),
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
                let cloud_dictionary = document.get_object(pair.cloud.object_id)?.as_dict()?;
                let text_dictionary = document.get_object(pair.text.object_id)?.as_dict()?;
                match import_cloud_plus_pair(
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
            let annotation = resolve_object(document, annotation_object)?.as_dict()?;
            let subtype = dictionary_name(annotation, b"Subtype").unwrap_or_default();
            let name = dictionary_string(annotation, b"NM").unwrap_or_else(|| {
                format!("page-{}-annotation-{}", page_number - 1, annotation_index)
            });
            let page_index = page_number.saturating_sub(1);
            match subtype.as_str() {
                "Square"
                    if dictionary_name(annotation, b"IT").as_deref() == Some("SquareImage") =>
                {
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let image = import_image(document, annotation, stable_name, page_index)?;
                    imported.image_native_names.insert(image.id.clone(), name);
                    imported.annotation_order.push(image.id.clone());
                    imported.images.push(image);
                }
                "Stamp" => {
                    let Some(stable_name) = name.strip_prefix("bp:").map(str::to_owned) else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let Object::Reference(_) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    if !is_canonical_managed_snapshot(document, annotation, &name) {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    }
                    match import_snapshot(document, annotation, stable_name, page_index) {
                        Ok(snapshot)
                            if !imported.snapshot_native_names.contains_key(&snapshot.id) =>
                        {
                            imported
                                .snapshot_native_names
                                .insert(snapshot.id.clone(), name);
                            imported.annotation_order.push(snapshot.id.clone());
                            imported.snapshots.push(snapshot);
                        }
                        Ok(_) | Err(_) => imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype }),
                    }
                }
                "Square" => {
                    let rectangle = import_rectangle(annotation, name, page_index)?;
                    imported.annotation_order.push(rectangle.id.clone());
                    imported.rectangles.push(rectangle);
                }
                "Redact" => {
                    let Some(stable_name) = name.strip_prefix("bp:").map(str::to_owned) else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    let Object::Reference(object_id) = annotation_object else {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    };
                    if !is_canonical_managed_redact(annotation, &name) {
                        imported
                            .untouched
                            .push(UntouchedAnnotation { name, subtype });
                        continue;
                    }
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.redact_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous pending Redact identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let redact = import_redact(annotation, stable_name, page_index)?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.arc_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous Arc identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let arc = import_arc(annotation, stable_name, page_index)?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.ellipse_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous Ellipse identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let ellipse = import_ellipse(annotation, stable_name, page_index)?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.pen_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous Ink identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let pen = import_pen(annotation, stable_name, page_index)?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.callout_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous callout identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let callout = import_callout(annotation, stable_name, page_index)?;
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
                    let text_box = import_text_box(annotation, name, page_index)?;
                    imported.annotation_order.push(text_box.id.clone());
                    imported.text_boxes.push(text_box);
                }
                "Line" if is_length_dictionary(annotation) => {
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let length = import_length(annotation, stable_name, page_index)?;
                    imported.length_native_names.insert(length.id.clone(), name);
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let dimension = import_dimension(annotation, stable_name, page_index)?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported
                        .straight_line_native_identities
                        .contains_key(&stable_id)
                    {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous straight-line identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let straight_line = import_straight_line(annotation, stable_name, page_index)?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported.cloud_native_identities.contains_key(&stable_id) {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous cloud identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let cloud = import_cloud(annotation, stable_name, page_index)?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
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
                    let measurement = import_measurement_path(
                        annotation,
                        stable_name,
                        page_index,
                        kind,
                        page_length_calibrations.get(&page_index),
                    )?;
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
                    let stable_name = name.strip_prefix("bp:").unwrap_or(&name).to_owned();
                    let stable_id = MarkupId::new(stable_name.clone())?;
                    if imported
                        .vertex_path_native_identities
                        .contains_key(&stable_id)
                    {
                        return Err(PdfPersistenceError::InvalidDocument(format!(
                            "ambiguous vertex-path identity {stable_name}: multiple native names normalize to the same stable id"
                        )));
                    }
                    let vertex_path = import_vertex_path(
                        annotation,
                        stable_name,
                        page_index,
                        if subtype == "PolyLine" {
                            VertexPathKind::Polyline
                        } else {
                            VertexPathKind::Polygon
                        },
                    )?;
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
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<RectangleAnnotation, PdfPersistenceError> {
    let values = annotation
        .get(b"BPRect")
        .or_else(|_| annotation.get(b"Rect"))?
        .as_array()?
        .iter()
        .map(|value| value.as_float().map(f64::from))
        .collect::<Result<Vec<_>, _>>()?;
    let [left, bottom, right, top] = values.as_slice() else {
        return Err(PdfPersistenceError::InvalidDocument(
            "rectangle annotation /Rect must contain four numbers".into(),
        ));
    };
    let stroke = dictionary_color(annotation, b"C").unwrap_or_else(|| "#ff0000".into());
    let fill = dictionary_color(annotation, b"IC");
    let stroke_width = annotation
        .get(b"BS")
        .ok()
        .and_then(|object| object.as_dict().ok())
        .and_then(|border| border.get(b"W").ok())
        .and_then(|width| width.as_float().ok())
        .map_or(1.0, f64::from);
    let opacity = annotation
        .get(b"CA")
        .ok()
        .and_then(|value| value.as_float().ok())
        .map_or(1.0, f64::from);
    let fill_opacity = annotation
        .get(b"BPFillAlpha")
        .ok()
        .and_then(|value| value.as_float().ok())
        .map_or(1.0, f64::from);
    let rotation_degrees = annotation
        .get(b"BPRotation")
        .ok()
        .and_then(|value| value.as_float().ok())
        .map_or(0.0, f64::from)
        .rem_euclid(360.0);
    let stroke_style = annotation
        .get(b"BS")
        .ok()
        .and_then(|object| object.as_dict().ok())
        .and_then(|border| border.get(b"S").ok())
        .and_then(|style| style.as_name().ok())
        .map_or(crate::annotation_model::StrokeStyle::Solid, |style| {
            if style == b"D" {
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
                    crate::annotation_model::StrokeStyle::Dotted
                } else {
                    crate::annotation_model::StrokeStyle::Dashed
                }
            } else {
                crate::annotation_model::StrokeStyle::Solid
            }
        });
    Ok(RectangleAnnotation {
        id: MarkupId::new(name)?,
        page_index,
        rect: PdfRect::new(*left, *bottom, right - left, top - bottom)?,
        rotation_degrees,
        appearance: RectangleAppearance::new(stroke, stroke_width, fill, opacity)?
            .with_fill_opacity(fill_opacity)?
            .with_stroke_style(stroke_style),
        locked: annotation
            .get(b"F")
            .ok()
            .and_then(|value| value.as_i64().ok())
            .is_some_and(|flags| flags & 128 != 0),
    })
}

fn import_redact(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<RedactAnnotation, PdfPersistenceError> {
    let rect = import_pdf_rect(annotation, b"Rect")?;
    let stored_appearance = dictionary_string(annotation, b"BPAppearance")
        .and_then(|serialized| serde_json::from_str::<Value>(&serialized).ok());
    let stroke = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/color"))
        .and_then(Value::as_str)
        .unwrap_or("#ff0000");
    let stroke_width = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/widthPt"))
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    let fill = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/fill/color"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let opacity = stored_appearance
        .as_ref()
        .and_then(|value| value.get("opacity"))
        .and_then(Value::as_f64)
        .or_else(|| dictionary_float(annotation, b"CA"))
        .unwrap_or(0.35);
    let fill_opacity = stored_appearance
        .as_ref()
        .and_then(|value| value.get("fillOpacity"))
        .and_then(Value::as_f64)
        .or_else(|| {
            let nonstroking = dictionary_float(annotation, b"ca")?;
            (opacity > f64::EPSILON).then_some(nonstroking / opacity)
        })
        .unwrap_or(1.0);
    let stroke_style = match stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/style"))
        .and_then(Value::as_str)
    {
        Some("dashed") => StrokeStyle::Dashed,
        Some("dotted") => StrokeStyle::Dotted,
        _ => StrokeStyle::Solid,
    };
    let appearance = RectangleAppearance::new(stroke, stroke_width, fill, opacity)?
        .with_fill_opacity(fill_opacity)?
        .with_stroke_style(stroke_style);
    let mut redact = RedactAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        rect,
        dictionary_color(annotation, b"IC").unwrap_or_else(|| "#000000".into()),
        dictionary_string(annotation, b"OverlayText"),
        appearance,
    )?;
    redact.locked = annotation_locked(annotation);
    Ok(redact)
}

fn import_ellipse(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<EllipseAnnotation, PdfPersistenceError> {
    let rect = if annotation.get(b"BPRect").is_ok() {
        import_pdf_rect(annotation, b"BPRect")?
    } else {
        import_pdf_rect(annotation, b"Rect")?
    };
    let stored_appearance = dictionary_string(annotation, b"BPAppearance")
        .and_then(|serialized| serde_json::from_str::<Value>(&serialized).ok());
    let stroke = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/color"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| dictionary_color(annotation, b"C"))
        .unwrap_or_else(|| "#ff0000".into());
    let fill = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/fill/color"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| dictionary_color(annotation, b"IC"));
    let stroke_width = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/widthPt"))
        .and_then(Value::as_f64)
        .or_else(|| {
            annotation
                .get(b"BS")
                .ok()
                .and_then(|object| object.as_dict().ok())
                .and_then(|border| dictionary_float(border, b"W"))
        })
        .unwrap_or(1.0);
    let opacity = stored_appearance
        .as_ref()
        .and_then(|value| value.get("opacity"))
        .and_then(Value::as_f64)
        .or_else(|| dictionary_float(annotation, b"CA"))
        .unwrap_or(1.0);
    let fill_opacity = dictionary_float(annotation, b"BPFillAlpha")
        .or_else(|| dictionary_float(annotation, b"ca"))
        .unwrap_or(1.0);
    let stored_stroke_style = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/style"))
        .and_then(Value::as_str);
    let stroke_style = match stored_stroke_style {
        Some("dashed") => StrokeStyle::Dashed,
        Some("dotted") => StrokeStyle::Dotted,
        Some(_) => StrokeStyle::Solid,
        None => annotation
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
            }),
    };
    let rotation_degrees = dictionary_float(annotation, b"BPRotation")
        .or_else(|| dictionary_float(annotation, b"Rotation"))
        .unwrap_or(0.0)
        .rem_euclid(360.0);
    Ok(EllipseAnnotation {
        id: MarkupId::new(name)?,
        page_index,
        rect,
        rotation_degrees,
        appearance: RectangleAppearance::new(stroke, stroke_width, fill, opacity)?
            .with_fill_opacity(fill_opacity)?
            .with_stroke_style(stroke_style),
        locked: annotation_locked(annotation),
    })
}

fn import_arc(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<ArcAnnotation, PdfPersistenceError> {
    let ellipse = import_ellipse(annotation, name, page_index)?;
    let rect = ellipse.rect;
    if (rect.width - rect.height).abs() > 0.000_1 {
        return Err(PdfPersistenceError::InvalidDocument(
            "Butter Paper CircleArc bounds must be circular".into(),
        ));
    }
    let angle1 = dictionary_float(annotation, b"Angle1").unwrap_or(90.);
    let angle2 = dictionary_float(annotation, b"Angle2").unwrap_or(180.);
    let sweep = normalize_arc_delta(angle1, angle2);
    let midpoint_angle = angle1 + sweep * 0.5;
    let point_at = |angle_degrees: f64| {
        let angle = angle_degrees.to_radians();
        PdfPoint::new(
            rect.x + rect.width * 0.5 + rect.width * 0.5 * angle.cos(),
            rect.y + rect.height * 0.5 + rect.height * 0.5 * angle.sin(),
        )
    };
    let mut arc = ArcAnnotation::new(
        ellipse.id,
        page_index,
        point_at(angle1)?,
        point_at(angle2)?,
        point_at(midpoint_angle)?,
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
    let paths = if let Some(canonical) = dictionary_string(annotation, b"BPCanonicalPointBits") {
        let bit_paths = serde_json::from_str::<Vec<Vec<[u64; 2]>>>(&canonical)
            .or_else(|_| {
                serde_json::from_str::<Vec<[u64; 2]>>(&canonical).map(|legacy| vec![legacy])
            })
            .map_err(|error| {
                PdfPersistenceError::InvalidDocument(format!(
                    "ink annotation has invalid Butter Paper canonical points: {error}"
                ))
            })?;
        bit_paths
            .into_iter()
            .map(|path| {
                path.into_iter()
                    .map(|point| {
                        PdfPoint::new(f64::from_bits(point[0]), f64::from_bits(point[1]))
                            .map_err(PdfPersistenceError::from)
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        ink_lists
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
            .collect::<Result<Vec<_>, PdfPersistenceError>>()?
    };
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
    let is_highlight = dictionary_string(annotation, b"Subj")
        .is_some_and(|subject| subject.eq_ignore_ascii_case("highlight"));
    let mut imported = if is_highlight {
        PenAnnotation::new_highlight_paths(MarkupId::new(name)?, page_index, paths, appearance)?
    } else {
        let smooth_curves = annotation
            .get(b"BPSmoothCurves")
            .ok()
            .and_then(|value| value.as_bool().ok())
            .unwrap_or(true);
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

fn import_text_box(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<TextBoxAnnotation, PdfPersistenceError> {
    let default_appearance = dictionary_string(annotation, b"DA").unwrap_or_default();
    let tokens = default_appearance.split_whitespace().collect::<Vec<_>>();
    let font_size = tokens
        .windows(2)
        .find(|pair| pair[1] == "Tf")
        .and_then(|pair| pair[0].parse::<f64>().ok())
        .unwrap_or(12.0);
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
        })
        .unwrap_or_else(|| "#000000".into());
    let family =
        dictionary_string(annotation, b"BPFontFamily").unwrap_or_else(|| "Helvetica".into());
    let weight = annotation
        .get(b"BPFontWeight")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(400);
    let alignment = match annotation
        .get(b"Q")
        .ok()
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0)
    {
        1 => TextAlignment::Center,
        2 => TextAlignment::Right,
        _ => TextAlignment::Left,
    };
    let style = TextBoxStyle::new(
        family,
        font_size,
        color,
        dictionary_float(annotation, b"CA").unwrap_or(1.0),
    )?
    .with_weight_and_alignment(weight, alignment)?;
    let mut imported = TextBoxAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        import_pdf_rect(annotation, b"Rect")?,
        dictionary_string(annotation, b"Contents").unwrap_or_default(),
        style,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_callout(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<CalloutAnnotation, PdfPersistenceError> {
    let imported_text = import_text_box(annotation, name.clone(), page_index)?;
    let outer = import_pdf_rect(annotation, b"Rect")?;
    let text_box = annotation
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
    let stroke_color = dictionary_string(annotation, b"BPStrokeColor")
        .unwrap_or_else(|| imported_text.style().color().to_owned());
    let width = annotation
        .get(b"BPStrokeWidth")
        .ok()
        .and_then(|value| value.as_float().ok())
        .map_or(1., f64::from);
    let opacity = imported_text.style().opacity();
    let appearance = CalloutAppearance::new(
        StraightLineAppearance::new(stroke_color, width, opacity, StrokeStyle::Solid)?,
        imported_text.style().clone(),
    )?;
    let mut imported = CalloutAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        leader,
        text_box,
        imported_text.content(),
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

fn import_cloud_plus_pair(
    cloud_dictionary: &Dictionary,
    text_dictionary: &Dictionary,
    stable_name: String,
    page_index: u32,
) -> Result<CloudPlusAnnotation, PdfPersistenceError> {
    let cloud = import_cloud(cloud_dictionary, stable_name.clone(), page_index)?;
    let imported_text = import_text_box(text_dictionary, stable_name.clone(), page_index)?;
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
    let text_style = TextBoxStyle::new(
        imported_text.style().font_family(),
        imported_text.style().font_size_pt(),
        imported_text.style().color(),
        cloud.appearance.opacity(),
    )?
    .with_weight_and_alignment(
        imported_text.style().weight(),
        imported_text.style().alignment(),
    )?;
    let leader_appearance = StraightLineAppearance::new(
        cloud.appearance.stroke_color(),
        cloud.appearance.stroke_width_pt(),
        cloud.appearance.opacity(),
        cloud.appearance.stroke_style(),
    )?;
    let appearance =
        CloudPlusAppearance::new(cloud.appearance.clone(), leader_appearance, text_style)?;
    let mut imported = CloudPlusAnnotation::new(
        MarkupId::new(stable_name)?,
        page_index,
        cloud.points().to_vec(),
        cloud.border_effect_intensity(),
        leader_points,
        import_cloud_plus_text_box(text_dictionary)?,
        imported_text.content(),
        appearance,
    )?;
    imported.locked = cloud.locked || annotation_locked(text_dictionary);
    Ok(imported)
}

fn import_length(
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<LengthAnnotation, PdfPersistenceError> {
    let line = annotation.get(b"L")?.as_array()?;
    let [start_x, start_y, end_x, end_y] = line.as_slice() else {
        return Err(PdfPersistenceError::InvalidDocument(
            "length /L must contain four numbers".into(),
        ));
    };
    let calibration = if let Ok(scale) = annotation.get(b"BPScale").and_then(Object::as_dict) {
        LengthCalibration::from_scale(
            dictionary_float(scale, b"PaperPoints").ok_or_else(|| {
                PdfPersistenceError::InvalidDocument("length paper-point scale is missing".into())
            })?,
            dictionary_float(scale, b"RealWorldValue").ok_or_else(|| {
                PdfPersistenceError::InvalidDocument("length real-world scale is missing".into())
            })?,
            dictionary_string(scale, b"Unit").unwrap_or_else(|| "pt".into()),
            scale
                .get(b"Precision")
                .ok()
                .and_then(|value| value.as_i64().ok())
                .and_then(|value| u8::try_from(value).ok())
                .unwrap_or(2),
            scale
                .get(b"ShowCaption")
                .ok()
                .and_then(|value| value.as_bool().ok())
                .unwrap_or(true),
        )?
        .with_label(dictionary_string(scale, b"Label").unwrap_or_default())?
    } else {
        import_standard_length_calibration(annotation)?
            .with_label(dictionary_string(annotation, b"Label").unwrap_or_default())?
    };
    let mut imported = LengthAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        PdfPoint::new(
            f64::from(start_x.as_float()?),
            f64::from(start_y.as_float()?),
        )?,
        PdfPoint::new(f64::from(end_x.as_float()?), f64::from(end_y.as_float()?))?,
        calibration,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_dimension(
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
    let stored_appearance = dictionary_string(annotation, b"BPAppearance")
        .and_then(|serialized| serde_json::from_str::<Value>(&serialized).ok());
    let stroke_color = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/color"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| dictionary_color(annotation, b"C"))
        .unwrap_or_else(|| "#ff0000".into());
    let stroke_width = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/widthPt"))
        .and_then(Value::as_f64)
        .or_else(|| {
            annotation
                .get(b"BS")
                .ok()
                .and_then(|value| value.as_dict().ok())
                .and_then(|border| dictionary_float(border, b"W"))
        })
        .unwrap_or(1.);
    let stroke_style = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/style"))
        .and_then(Value::as_str)
        .map_or_else(
            || {
                annotation
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
                    })
            },
            |style| match style {
                "dashed" => StrokeStyle::Dashed,
                "dotted" => StrokeStyle::Dotted,
                _ => StrokeStyle::Solid,
            },
        );
    let opacity = stored_appearance
        .as_ref()
        .and_then(|value| value.get("opacity"))
        .and_then(Value::as_f64)
        .or_else(|| dictionary_float(annotation, b"CA"))
        .unwrap_or(1.);
    let text_font_family = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/text/fontFamily"))
        .and_then(Value::as_str)
        .unwrap_or("Helvetica");
    let text_font_size = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/text/fontSizePt"))
        .and_then(Value::as_f64)
        .unwrap_or(12.);
    let text_color = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/text/color"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| stroke_color.clone());
    let appearance = DimensionAppearance::new(
        StraightLineAppearance::new(stroke_color, stroke_width, opacity, stroke_style)?,
        TextBoxStyle::new(text_font_family, text_font_size, text_color, opacity)?,
    )?;
    let content = dictionary_string(annotation, b"Contents")
        .or_else(|| {
            annotation
                .get(b"Cap")
                .ok()
                .and_then(|value| value.as_str().ok())
                .map(|value| String::from_utf8_lossy(value).into_owned())
        })
        .unwrap_or_else(|| "Dimension".into());
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
    let stored_appearance = dictionary_string(annotation, b"BPAppearance")
        .and_then(|serialized| serde_json::from_str::<Value>(&serialized).ok());
    let stroke = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/color"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .or_else(|| dictionary_color(annotation, b"C"))
        .unwrap_or_else(|| "#ff0000".into());
    let fill = (kind == VertexPathKind::Polygon)
        .then(|| {
            stored_appearance
                .as_ref()
                .and_then(|value| value.pointer("/fill/color"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .or_else(|| dictionary_color(annotation, b"IC"))
        })
        .flatten();
    let stroke_width = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/widthPt"))
        .and_then(Value::as_f64)
        .or_else(|| {
            annotation
                .get(b"BS")
                .ok()
                .and_then(|object| object.as_dict().ok())
                .and_then(|border| dictionary_float(border, b"W"))
        })
        .unwrap_or(1.0);
    let opacity = stored_appearance
        .as_ref()
        .and_then(|value| value.get("opacity"))
        .and_then(Value::as_f64)
        .or_else(|| dictionary_float(annotation, b"CA"))
        .unwrap_or(1.0);
    let fill_opacity = dictionary_float(annotation, b"BPFillAlpha")
        .or_else(|| dictionary_float(annotation, b"ca"))
        .unwrap_or(1.0);
    let stored_stroke_style = stored_appearance
        .as_ref()
        .and_then(|value| value.pointer("/stroke/style"))
        .and_then(Value::as_str);
    let stroke_style = match stored_stroke_style {
        Some("dashed") => StrokeStyle::Dashed,
        Some("dotted") => StrokeStyle::Dotted,
        Some(_) => StrokeStyle::Solid,
        None => annotation
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
            }),
    };
    let mut imported = VertexPathAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        points,
        kind,
        RectangleAppearance::new(stroke, stroke_width, fill, opacity)?
            .with_fill_opacity(fill_opacity)?
            .with_stroke_style(stroke_style),
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
    let appearance = RectangleAppearance::new(
        imported.appearance.stroke_color(),
        imported.appearance.stroke_width_pt(),
        None::<String>,
        imported.appearance.opacity(),
    )?
    .with_stroke_style(imported.appearance.stroke_style());
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
    let calibration = import_measurement_path_calibration(annotation, page_calibration)?;
    let mut imported = MeasurementPathAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        vertex.points().to_vec(),
        kind,
        calibration,
        vertex.appearance,
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_measurement_path_calibration(
    annotation: &Dictionary,
    page_calibration: Option<&LengthCalibration>,
) -> Result<LengthCalibration, PdfPersistenceError> {
    if let Ok(scale) = annotation.get(b"BPScale").and_then(Object::as_dict) {
        return Ok(LengthCalibration::from_scale(
            dictionary_float(scale, b"PaperPoints").ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "measurement-path paper-point scale is missing".into(),
                )
            })?,
            dictionary_float(scale, b"RealWorldValue").ok_or_else(|| {
                PdfPersistenceError::InvalidDocument(
                    "measurement-path real-world scale is missing".into(),
                )
            })?,
            dictionary_string(scale, b"Unit").unwrap_or_else(|| "pt".into()),
            scale
                .get(b"Precision")
                .ok()
                .and_then(|value| value.as_i64().ok())
                .and_then(|value| u8::try_from(value).ok())
                .unwrap_or(2),
            scale
                .get(b"ShowCaption")
                .ok()
                .and_then(|value| value.as_bool().ok())
                .unwrap_or(true),
        )?
        .with_label(dictionary_string(scale, b"Label").unwrap_or_default())?);
    }
    let label = dictionary_string(annotation, b"Label").unwrap_or_default();
    if annotation.get(b"Measure").is_ok() {
        return Ok(import_standard_length_calibration(annotation)?.with_label(label)?);
    }
    Ok(page_calibration
        .cloned()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "measurement path has no annotation or page calibration".into(),
            )
        })?
        .with_label(label)?)
}

fn is_length_dictionary(annotation: &Dictionary) -> bool {
    let subject = dictionary_string(annotation, b"Subj")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    subject == "length"
        || subject == "length measurement"
        || (dictionary_name(annotation, b"IT").as_deref() == Some("LineDimension")
            && annotation.get(b"Measure").is_ok())
}

fn import_standard_length_calibration(
    annotation: &Dictionary,
) -> Result<LengthCalibration, PdfPersistenceError> {
    let measure = annotation.get(b"Measure")?.as_dict()?;
    let formats = measure.get(b"X")?.as_array()?;
    let format = formats
        .first()
        .ok_or_else(|| {
            PdfPersistenceError::InvalidDocument(
                "length measurement scale has no horizontal number format".into(),
            )
        })?
        .as_dict()?;
    let units_per_point = dictionary_float(format, b"C").ok_or_else(|| {
        PdfPersistenceError::InvalidDocument(
            "length measurement conversion factor is missing".into(),
        )
    })?;
    let precision = format
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
    LengthCalibration::from_scale(
        1.,
        units_per_point,
        dictionary_string(format, b"U").unwrap_or_else(|| "pt".into()),
        precision,
        annotation
            .get(b"Cap")
            .ok()
            .and_then(|value| value.as_bool().ok())
            .unwrap_or(true),
    )
    .map_err(PdfPersistenceError::from)
}

fn import_image(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<ImageAnnotation, PdfPersistenceError> {
    let asset = import_media_appearance_asset(document, annotation)?;
    let mut imported = ImageAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        import_pdf_rect(annotation, b"Rect")?,
        asset,
        annotation
            .get(b"BPAspectLocked")
            .ok()
            .and_then(|value| value.as_bool().ok())
            .unwrap_or(false),
    )?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn import_snapshot(
    document: &Document,
    annotation: &Dictionary,
    name: String,
    page_index: u32,
) -> Result<SnapshotAnnotation, PdfPersistenceError> {
    let asset = import_media_appearance_asset(document, annotation)?;
    let mut imported = SnapshotAnnotation::new(
        MarkupId::new(name)?,
        page_index,
        import_pdf_rect(annotation, b"Rect")?,
        asset,
        dictionary_float(annotation, b"CA")
            .or_else(|| dictionary_float(annotation, b"ca"))
            .unwrap_or(1.),
    )?
    .with_rotation_degrees(dictionary_float(annotation, b"Rotation").unwrap_or(0.))?;
    imported.locked = annotation_locked(annotation);
    Ok(imported)
}

fn is_canonical_managed_snapshot(
    document: &Document,
    annotation: &Dictionary,
    raw_name: &str,
) -> bool {
    if !raw_name.starts_with("bp:")
        || dictionary_string(annotation, b"NM").as_deref() != Some(raw_name)
        || dictionary_name(annotation, b"Type").as_deref() != Some("Annot")
        || dictionary_name(annotation, b"Subtype").as_deref() != Some("Stamp")
        || dictionary_name(annotation, b"IT").as_deref() != Some("StampSnapshot")
        || dictionary_string(annotation, b"Subj").as_deref() != Some("Snapshot")
        || dictionary_string(annotation, b"Contents").as_deref() != Some("")
        || annotation
            .get(b"F")
            .ok()
            .and_then(|value| value.as_i64().ok())
            .unwrap_or(0)
            & 4
            == 0
        || import_pdf_rect(annotation, b"Rect").is_err()
    {
        return false;
    }
    let Ok(asset) = import_media_appearance_asset(document, annotation) else {
        return false;
    };
    if dictionary_string(annotation, b"BPAssetId").as_deref() != Some(asset.id().as_str()) {
        return false;
    }
    let Ok(appearance) = normal_appearance_stream(document, annotation) else {
        return false;
    };
    dictionary_name(&appearance.dict, b"Type").as_deref() == Some("XObject")
        && dictionary_name(&appearance.dict, b"Subtype").as_deref() == Some("Form")
}

fn import_media_appearance_asset(
    document: &Document,
    annotation: &Dictionary,
) -> Result<DecodedRgbaAsset, PdfPersistenceError> {
    let appearance = normal_appearance_stream(document, annotation)?;
    let image = resolve_object(
        document,
        appearance
            .dict
            .get(b"Resources")?
            .as_dict()?
            .get(b"XObject")?
            .as_dict()?
            .get(b"Im0")?,
    )?
    .as_stream()?;
    let width = u32::try_from(image.dict.get(b"Width")?.as_i64()?).map_err(|_| {
        PdfPersistenceError::InvalidDocument("image width is outside the supported range".into())
    })?;
    let height = u32::try_from(image.dict.get(b"Height")?.as_i64()?).map_err(|_| {
        PdfPersistenceError::InvalidDocument("image height is outside the supported range".into())
    })?;
    let alpha = resolve_object(document, image.dict.get(b"SMask")?)?.as_stream()?;
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| PdfPersistenceError::InvalidDocument("image dimensions overflow".into()))?;
    if image.content.len() != pixel_count * 3 || alpha.content.len() != pixel_count {
        return Err(PdfPersistenceError::InvalidDocument(
            "image XObject byte lengths do not match its dimensions".into(),
        ));
    }
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    for (rgb, alpha) in image.content.chunks_exact(3).zip(&alpha.content) {
        rgba.extend_from_slice(rgb);
        rgba.push(*alpha);
    }
    Ok(DecodedRgbaAsset::new(width, height, rgba)?)
}

fn normal_appearance_stream<'a>(
    document: &'a Document,
    annotation: &'a Dictionary,
) -> Result<&'a Stream, lopdf::Error> {
    resolve_object(document, annotation.get(b"AP")?.as_dict()?.get(b"N")?)?.as_stream()
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
        .ok()?
        .as_str()
        .ok()
        .map(|value| String::from_utf8_lossy(value).into_owned())
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
    use serde_json::json;

    use super::{
        DocumentInfo, DocumentMetadata, DocumentPermissions, DocumentSecurity, EngineErrorCode,
        EngineRequest, EngineResponse, OpenRequest, PDF_ENGINE_PROTOCOL_NAME,
        PDF_ENGINE_PROTOCOL_VERSION, RequestId, SessionId, SourceHandleId, decode_request,
        decode_response, encode_request, encode_response,
    };

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
