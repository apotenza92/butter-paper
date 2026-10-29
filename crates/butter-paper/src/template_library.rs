//! GPUI-free, experiment-owned persistent template-library boundary.

use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    generated_document::{
        GeneratedDocumentError, GeneratedDocumentRequest, GeneratedDocumentStore, GeneratedPattern,
        OwnedGeneratedDocument,
    },
    pdf_engine::PdfPersistenceSession,
};

pub const BUILT_IN_BLANK_ID: &str = "built-in-blank";
/// Built-in templates: each paper at A3 landscape, then at A4 portrait. The A3
/// identifiers predate the A4 set and stay unsuffixed because stored
/// libraries and last-used selections refer to them.
pub const BUILT_IN_TEMPLATE_IDS: [&str; 12] = [
    BUILT_IN_BLANK_ID,
    "built-in-dots",
    "built-in-grid",
    "built-in-lined",
    "built-in-isometric",
    "built-in-triangle",
    "built-in-blank-a4",
    "built-in-dots-a4",
    "built-in-grid-a4",
    "built-in-lined-a4",
    "built-in-isometric-a4",
    "built-in-triangle-a4",
];
/// Suffix of the A4 portrait built-in identifiers.
pub const BUILT_IN_A4_SUFFIX: &str = "-a4";
const INDEX_VERSION: u32 = 1;
pub(crate) const INDEX_FILE: &str = "library.json";
pub(crate) const SENTINEL_FILE: &str = ".butter-paper-template-library-v1";
pub(crate) const SENTINEL_BYTES: &[u8] = b"butter-paper-template-library-v1\n";
pub(crate) const SOURCE_FILE: &str = "source.pdf";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateRecord {
    Generated {
        id: String,
        name: String,
        request: GeneratedDocumentRequest,
    },
    ImportedPdf {
        id: String,
        name: String,
        page_count: usize,
        created_at: String,
        sha256: String,
    },
}

impl TemplateRecord {
    pub fn id(&self) -> &str {
        match self {
            Self::Generated { id, .. } | Self::ImportedPdf { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Generated { name, .. } | Self::ImportedPdf { name, .. } => name,
        }
    }

    pub fn page_count(&self) -> Option<usize> {
        match self {
            Self::ImportedPdf { page_count, .. } => Some(*page_count),
            Self::Generated { .. } => None,
        }
    }

    pub fn sha256(&self) -> Option<&str> {
        match self {
            Self::ImportedPdf { sha256, .. } => Some(sha256),
            Self::Generated { .. } => None,
        }
    }
}

#[derive(Debug)]
pub struct TemplateLibrary {
    root: PathBuf,
    records: Vec<TemplateRecord>,
    last_template_id: String,
    legacy_blank_migrated: bool,
}

impl TemplateLibrary {
    pub fn open(root: PathBuf) -> Result<Self, TemplateLibraryError> {
        reject_symlink(&root, "template library root")?;
        fs::create_dir_all(&root)?;
        ensure_sentinel(&root)?;
        let index_path = root.join(INDEX_FILE);
        let stored = match fs::read(&index_path) {
            Ok(bytes) => serde_json::from_slice::<StoredIndex>(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => StoredIndex::default(),
            Err(error) => return Err(error.into()),
        };
        let StoredIndex {
            version,
            records: stored_records,
            last_template_id,
            legacy_blank_migrated,
        } = stored;
        if version != INDEX_VERSION {
            return Err(TemplateLibraryError(format!(
                "unsupported template library index version {version}"
            )));
        }
        let mut records = Vec::new();
        for stored_record in stored_records {
            let record = stored_record.into_record()?;
            if BUILT_IN_TEMPLATE_IDS.contains(&record.id())
                || records
                    .iter()
                    .any(|existing: &TemplateRecord| existing.id() == record.id())
            {
                return Err(TemplateLibraryError(
                    "template library contains a duplicate identifier".into(),
                ));
            }
            records.push(record);
        }
        let mut library = Self {
            root,
            records,
            last_template_id,
            legacy_blank_migrated,
        };
        if !library.contains_id(&library.last_template_id) {
            library.last_template_id = BUILT_IN_BLANK_ID.into();
        }
        library.verify_managed_sources()?;
        Ok(library)
    }

    pub fn last_template_id(&self) -> &str {
        &self.last_template_id
    }

    pub fn record_ids(&self) -> Vec<&str> {
        self.records.iter().map(TemplateRecord::id).collect()
    }

    pub fn records(&self) -> &[TemplateRecord] {
        &self.records
    }

    pub fn migrate_legacy_blank_request(
        &mut self,
        request: Option<GeneratedDocumentRequest>,
    ) -> Result<(), TemplateLibraryError> {
        if self.legacy_blank_migrated {
            return Ok(());
        }
        let previous_records = self.records.clone();
        let previous_last = self.last_template_id.clone();
        let previous_migrated = self.legacy_blank_migrated;
        if self.records.is_empty()
            && let Some(request) = request
            && request.to_pdf_bytes().is_ok()
        {
            if let Some(id) = built_in_id_for_request(&request) {
                self.last_template_id = id.into();
            } else {
                let id = "custom-migrated-blank-pdf-default";
                self.records.push(TemplateRecord::Generated {
                    id: id.into(),
                    name: "Previous Blank PDF".into(),
                    request,
                });
                self.last_template_id = id.into();
            }
        }
        self.legacy_blank_migrated = true;
        if let Err(error) = self.persist() {
            self.records = previous_records;
            self.last_template_id = previous_last;
            self.legacy_blank_migrated = previous_migrated;
            return Err(error);
        }
        Ok(())
    }

    pub fn add_generated(
        &mut self,
        id: &str,
        name: &str,
        request: GeneratedDocumentRequest,
    ) -> Result<&TemplateRecord, TemplateLibraryError> {
        validate_custom_id(id)?;
        self.ensure_unique(id)?;
        request.to_pdf_bytes()?;
        let previous_last = self.last_template_id.clone();
        self.records.push(TemplateRecord::Generated {
            id: id.into(),
            name: normalize_custom_name(name)?,
            request,
        });
        self.last_template_id = id.into();
        if let Err(error) = self.persist() {
            self.records.pop();
            self.last_template_id = previous_last;
            return Err(error);
        }
        Ok(self.records.last().unwrap())
    }

    pub fn import_pdf(
        &mut self,
        id: &str,
        name: &str,
        created_at: &str,
        source: &Path,
    ) -> Result<&TemplateRecord, TemplateLibraryError> {
        let bytes = fs::read(source)?;
        self.import_pdf_bytes(id, name, created_at, &bytes)
    }

    pub(crate) fn import_pdf_bytes(
        &mut self,
        id: &str,
        name: &str,
        created_at: &str,
        bytes: &[u8],
    ) -> Result<&TemplateRecord, TemplateLibraryError> {
        validate_imported_id(id)?;
        self.ensure_unique(id)?;
        if created_at.trim().is_empty() {
            return Err(TemplateLibraryError(
                "template creation time is required".into(),
            ));
        }
        let sha256 = digest(bytes);
        let directory = self.root.join(id);
        fs::create_dir(&directory)?;
        let source_path = directory.join(SOURCE_FILE);
        let previous_last = self.last_template_id.clone();
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&source_path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            let session = PdfPersistenceSession::open(&source_path)?;
            let page_count = session.page_count();
            if page_count == 0 {
                return Err(TemplateLibraryError("the template PDF has no pages".into()));
            }
            drop(session);
            self.records.push(TemplateRecord::ImportedPdf {
                id: id.into(),
                name: normalize_imported_name(name),
                page_count,
                created_at: created_at.into(),
                sha256,
            });
            self.last_template_id = id.into();
            self.persist()?;
            Ok::<(), TemplateLibraryError>(())
        })();
        if let Err(error) = result {
            self.records.retain(|record| record.id() != id);
            self.last_template_id = previous_last;
            let _ = fs::remove_dir_all(&directory);
            return Err(error);
        }
        Ok(self.records.last().unwrap())
    }

    pub fn select(&mut self, id: &str) -> Result<(), TemplateLibraryError> {
        if !self.contains_id(id) {
            return Err(TemplateLibraryError("the template does not exist".into()));
        }
        let previous = std::mem::replace(&mut self.last_template_id, id.into());
        if let Err(error) = self.persist() {
            self.last_template_id = previous;
            return Err(error);
        }
        Ok(())
    }

    /// Where an imported template's managed source is, without reading it
    /// to verify its checksum: for previews, which must not hold the library
    /// while a large source is hashed.
    pub fn unverified_managed_source_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, TemplateLibraryError> {
        validate_imported_id(id)?;
        if !self
            .records
            .iter()
            .any(|record| matches!(record, TemplateRecord::ImportedPdf { .. }) && record.id() == id)
        {
            return Err(TemplateLibraryError(
                "the PDF template no longer exists".into(),
            ));
        }
        let path = self.root.join(id).join(SOURCE_FILE);
        reject_symlink(&path, "managed template source")?;
        Ok(path)
    }

    pub fn managed_source_path(&self, id: &str) -> Result<PathBuf, TemplateLibraryError> {
        validate_imported_id(id)?;
        let Some(TemplateRecord::ImportedPdf { sha256, .. }) =
            self.records.iter().find(|record| record.id() == id)
        else {
            return Err(TemplateLibraryError(
                "the PDF template no longer exists".into(),
            ));
        };
        let path = self.root.join(id).join(SOURCE_FILE);
        reject_symlink(&path, "managed template source")?;
        if digest(&fs::read(&path)?) != *sha256 {
            return Err(TemplateLibraryError(
                "managed template source checksum changed".into(),
            ));
        }
        Ok(path)
    }

    pub fn materialize(
        &self,
        id: &str,
        document_key: &str,
        store: &GeneratedDocumentStore,
    ) -> Result<OwnedGeneratedDocument, TemplateLibraryError> {
        self.materialize_oriented(id, document_key, store, None)
    }

    /// As [`Self::materialize`], with generated paper turned to `orientation`
    /// when one is given. Imported PDFs keep their own pages.
    pub fn materialize_oriented(
        &self,
        id: &str,
        document_key: &str,
        store: &GeneratedDocumentStore,
        orientation: Option<crate::template_preview::PaperOrientation>,
    ) -> Result<OwnedGeneratedDocument, TemplateLibraryError> {
        let record = self
            .records
            .iter()
            .find(|record| record.id() == id)
            .ok_or_else(|| TemplateLibraryError("the template does not exist".into()))?;
        match record {
            TemplateRecord::Generated { request, .. } => {
                let request = match orientation {
                    Some(orientation) => {
                        crate::template_preview::oriented_request(request.clone(), orientation)
                    }
                    None => request.clone(),
                };
                Ok(store.create(document_key, &request)?)
            }
            TemplateRecord::ImportedPdf { .. } => {
                let bytes = fs::read(self.managed_source_path(id)?)?;
                Ok(store.create_from_pdf_bytes(document_key, &bytes)?)
            }
        }
    }

    pub fn remove(&mut self, id: &str) -> Result<(), TemplateLibraryError> {
        let Some(index) = self.records.iter().position(|record| record.id() == id) else {
            if id.starts_with("imported-") {
                validate_imported_id(id)?;
            }
            return Ok(());
        };
        let removed = self.records.remove(index);
        let previous_last = self.last_template_id.clone();
        if self.last_template_id == id {
            self.last_template_id = BUILT_IN_BLANK_ID.into();
        }
        let source = self.root.join(id);
        let tombstone = self.root.join(format!(".{id}.removing"));
        let moved_source = matches!(removed, TemplateRecord::ImportedPdf { .. }) && source.exists();
        if moved_source && let Err(error) = fs::rename(&source, &tombstone) {
            self.last_template_id = previous_last;
            self.records.insert(index, removed);
            return Err(error.into());
        }
        if let Err(error) = self.persist() {
            self.last_template_id = previous_last;
            self.records.insert(index, removed);
            if moved_source {
                let _ = fs::rename(&tombstone, &source);
            }
            return Err(error);
        }
        if moved_source {
            // The durable index no longer references this owned directory.
            // Failure to reap the tombstone must not resurrect or partially
            // report a template that was already removed transactionally.
            let _ = fs::remove_dir_all(&tombstone);
        }
        Ok(())
    }

    fn contains_id(&self, id: &str) -> bool {
        BUILT_IN_TEMPLATE_IDS.contains(&id) || self.records.iter().any(|record| record.id() == id)
    }

    fn ensure_unique(&self, id: &str) -> Result<(), TemplateLibraryError> {
        if self.contains_id(id) {
            return Err(TemplateLibraryError(
                "template identifier already exists".into(),
            ));
        }
        Ok(())
    }

    fn verify_managed_sources(&self) -> Result<(), TemplateLibraryError> {
        for record in &self.records {
            if matches!(record, TemplateRecord::ImportedPdf { .. }) {
                self.managed_source_path(record.id())?;
            }
        }
        Ok(())
    }

    fn persist(&self) -> Result<(), TemplateLibraryError> {
        let stored = StoredIndex {
            version: INDEX_VERSION,
            records: self.records.iter().map(StoredRecord::from_record).collect(),
            last_template_id: self.last_template_id.clone(),
            legacy_blank_migrated: self.legacy_blank_migrated,
        };
        let bytes = serde_json::to_vec_pretty(&stored)?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temporary = self.root.join(format!(
            "{INDEX_FILE}.tmp-{}-{sequence}",
            std::process::id()
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary, self.root.join(INDEX_FILE))?;
            Ok::<(), TemplateLibraryError>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredIndex {
    version: u32,
    records: Vec<StoredRecord>,
    #[serde(rename = "lastTemplateId")]
    last_template_id: String,
    #[serde(rename = "legacyBlankMigrated", default)]
    legacy_blank_migrated: bool,
}

impl Default for StoredIndex {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            records: Vec::new(),
            last_template_id: BUILT_IN_BLANK_ID.into(),
            legacy_blank_migrated: false,
        }
    }
}

pub fn built_in_request(id: &str) -> Option<GeneratedDocumentRequest> {
    if !BUILT_IN_TEMPLATE_IDS.contains(&id) {
        return None;
    }
    let (base, (width_mm, height_mm)) = match id.strip_suffix(BUILT_IN_A4_SUFFIX) {
        Some(base) => (base, (210., 297.)),
        None => (id, (420., 297.)),
    };
    let pattern = match base {
        BUILT_IN_BLANK_ID => None,
        "built-in-dots" => Some(GeneratedPattern::Dots {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-grid" => Some(GeneratedPattern::SquareGrid {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-lined" => Some(GeneratedPattern::Ruled {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-isometric" => Some(GeneratedPattern::Isometric {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        "built-in-triangle" => Some(GeneratedPattern::Triangle {
            spacing_mm: 10.,
            color: "#d1d5db".into(),
        }),
        _ => return None,
    };
    Some(GeneratedDocumentRequest {
        title: "Untitled".into(),
        width_mm,
        height_mm,
        pattern,
    })
}

fn built_in_id_for_request(request: &GeneratedDocumentRequest) -> Option<&'static str> {
    BUILT_IN_TEMPLATE_IDS
        .into_iter()
        .find(|id| built_in_request(id).as_ref() == Some(request))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind")]
enum StoredRecord {
    #[serde(rename = "generated")]
    Generated {
        id: String,
        name: String,
        title: String,
        #[serde(rename = "widthMm")]
        width_mm: f64,
        #[serde(rename = "heightMm")]
        height_mm: f64,
        pattern: Option<StoredPattern>,
    },
    #[serde(rename = "imported-pdf")]
    ImportedPdf {
        id: String,
        name: String,
        #[serde(rename = "pageCount")]
        page_count: usize,
        #[serde(rename = "createdAt")]
        created_at: String,
        sha256: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StoredPattern {
    kind: String,
    #[serde(rename = "spacingMm")]
    spacing_mm: f64,
    color: String,
}

impl StoredRecord {
    fn from_record(record: &TemplateRecord) -> Self {
        match record {
            TemplateRecord::Generated { id, name, request } => Self::Generated {
                id: id.clone(),
                name: name.clone(),
                title: request.title.clone(),
                width_mm: request.width_mm,
                height_mm: request.height_mm,
                pattern: request.pattern.as_ref().map(StoredPattern::from_pattern),
            },
            TemplateRecord::ImportedPdf {
                id,
                name,
                page_count,
                created_at,
                sha256,
            } => Self::ImportedPdf {
                id: id.clone(),
                name: name.clone(),
                page_count: *page_count,
                created_at: created_at.clone(),
                sha256: sha256.clone(),
            },
        }
    }

    fn into_record(self) -> Result<TemplateRecord, TemplateLibraryError> {
        match self {
            Self::Generated {
                id,
                name,
                title,
                width_mm,
                height_mm,
                pattern,
            } => {
                validate_custom_id(&id)?;
                let request = GeneratedDocumentRequest {
                    title,
                    width_mm,
                    height_mm,
                    pattern: pattern.map(StoredPattern::into_pattern).transpose()?,
                };
                request.to_pdf_bytes()?;
                Ok(TemplateRecord::Generated {
                    id,
                    name: normalize_custom_name(&name)?,
                    request,
                })
            }
            Self::ImportedPdf {
                id,
                name,
                page_count,
                created_at,
                sha256,
            } => {
                validate_imported_id(&id)?;
                if page_count == 0
                    || created_at.trim().is_empty()
                    || sha256.len() != 64
                    || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(TemplateLibraryError(
                        "imported template metadata is invalid".into(),
                    ));
                }
                Ok(TemplateRecord::ImportedPdf {
                    id,
                    name: normalize_imported_name(&name),
                    page_count,
                    created_at,
                    sha256,
                })
            }
        }
    }
}

impl StoredPattern {
    fn from_pattern(pattern: &GeneratedPattern) -> Self {
        let (kind, spacing_mm, color) = match pattern {
            GeneratedPattern::SquareGrid { spacing_mm, color } => ("grid", spacing_mm, color),
            GeneratedPattern::Dots { spacing_mm, color } => ("dots", spacing_mm, color),
            GeneratedPattern::Ruled { spacing_mm, color } => ("lined", spacing_mm, color),
            GeneratedPattern::Isometric { spacing_mm, color } => ("isometric", spacing_mm, color),
            GeneratedPattern::Triangle { spacing_mm, color } => ("triangle", spacing_mm, color),
        };
        Self {
            kind: kind.into(),
            spacing_mm: *spacing_mm,
            color: color.clone(),
        }
    }

    fn into_pattern(self) -> Result<GeneratedPattern, TemplateLibraryError> {
        let pattern = match self.kind.as_str() {
            "grid" => GeneratedPattern::SquareGrid {
                spacing_mm: self.spacing_mm,
                color: self.color,
            },
            "dots" => GeneratedPattern::Dots {
                spacing_mm: self.spacing_mm,
                color: self.color,
            },
            "lined" => GeneratedPattern::Ruled {
                spacing_mm: self.spacing_mm,
                color: self.color,
            },
            "isometric" => GeneratedPattern::Isometric {
                spacing_mm: self.spacing_mm,
                color: self.color,
            },
            "triangle" => GeneratedPattern::Triangle {
                spacing_mm: self.spacing_mm,
                color: self.color,
            },
            _ => return Err(TemplateLibraryError("unknown generated pattern".into())),
        };
        Ok(pattern)
    }
}

fn ensure_sentinel(root: &Path) -> Result<(), TemplateLibraryError> {
    let path = root.join(SENTINEL_FILE);
    if path.exists() {
        if fs::read_to_string(path)? != "butter-paper-template-library-v1\n" {
            return Err(TemplateLibraryError(
                "template library ownership sentinel is invalid".into(),
            ));
        }
        return Ok(());
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(SENTINEL_BYTES)?;
    file.sync_all()?;
    Ok(())
}

fn reject_symlink(path: &Path, label: &str) -> Result<(), TemplateLibraryError> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(TemplateLibraryError(format!(
            "{label} must not be a symlink"
        )));
    }
    Ok(())
}

fn validate_custom_id(id: &str) -> Result<(), TemplateLibraryError> {
    if !id.starts_with("custom-") || !safe_id(id) {
        return Err(TemplateLibraryError(
            "custom template identifier is invalid".into(),
        ));
    }
    Ok(())
}

fn validate_imported_id(id: &str) -> Result<(), TemplateLibraryError> {
    let Some(uuid) = id.strip_prefix("imported-") else {
        return Err(TemplateLibraryError(
            "template identifier is invalid".into(),
        ));
    };
    if uuid.len() != 36
        || uuid.bytes().enumerate().any(|(index, byte)| match index {
            8 | 13 | 18 | 23 => byte != b'-',
            _ => !byte.is_ascii_hexdigit(),
        })
    {
        return Err(TemplateLibraryError(
            "template identifier is invalid".into(),
        ));
    }
    Ok(())
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn normalize_custom_name(name: &str) -> Result<String, TemplateLibraryError> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(TemplateLibraryError("template name is required".into()));
    }
    if name.encode_utf16().count() > 80 {
        return Err(TemplateLibraryError(
            "template name must be 80 characters or fewer".into(),
        ));
    }
    Ok(name)
}

fn normalize_imported_name(name: &str) -> String {
    let normalized = name.trim();
    let without_extension = normalized
        .get(..normalized.len().saturating_sub(4))
        .filter(|_| {
            normalized
                .get(normalized.len().saturating_sub(4)..)
                .is_some_and(|suffix| suffix.eq_ignore_ascii_case(".pdf"))
        })
        .unwrap_or(normalized);
    let collapsed = without_extension
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let fallback = if collapsed.is_empty() {
        "Imported PDF"
    } else {
        &collapsed
    };
    let mut code_units = 0;
    fallback
        .chars()
        .take_while(|character| {
            let next = code_units + character.len_utf16();
            if next > 80 {
                return false;
            }
            code_units = next;
            true
        })
        .collect()
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug)]
pub struct TemplateLibraryError(String);

impl std::fmt::Display for TemplateLibraryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for TemplateLibraryError {}

impl From<std::io::Error> for TemplateLibraryError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for TemplateLibraryError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<GeneratedDocumentError> for TemplateLibraryError {
    fn from(error: GeneratedDocumentError) -> Self {
        Self(error.to_string())
    }
}

impl From<crate::pdf_engine::PdfPersistenceError> for TemplateLibraryError {
    fn from(error: crate::pdf_engine::PdfPersistenceError) -> Self {
        Self(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "target/test-scratch/template-library-{label}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn every_built_in_has_an_a4_portrait_twin_with_the_same_pattern() {
        let (a3, a4) = BUILT_IN_TEMPLATE_IDS.split_at(6);
        for (a3_id, a4_id) in a3.iter().zip(a4) {
            assert_eq!(*a4_id, format!("{a3_id}{BUILT_IN_A4_SUFFIX}"));
            let a3_request = built_in_request(a3_id).unwrap();
            let a4_request = built_in_request(a4_id).unwrap();
            assert_eq!((a3_request.width_mm, a3_request.height_mm), (420., 297.));
            assert_eq!((a4_request.width_mm, a4_request.height_mm), (210., 297.));
            assert_eq!(a3_request.pattern, a4_request.pattern, "{a4_id}");
            let pdf = String::from_utf8_lossy(&a4_request.to_pdf_bytes().unwrap()).into_owned();
            assert!(
                pdf.contains("/MediaBox [0 0 595.2756 841.8898]"),
                "{a4_id} generates an A4 portrait page"
            );
        }
        assert!(built_in_request("built-in-unknown-a4").is_none());
        assert!(built_in_request("built-in-blank-a3").is_none());
    }

    #[test]
    fn a4_built_ins_are_selectable_reserved_and_migrated_from_legacy_settings() {
        let root = scratch("a4");
        let mut library = TemplateLibrary::open(root.clone()).unwrap();
        library.select("built-in-dots-a4").unwrap();
        assert!(
            library
                .add_generated(
                    "built-in-grid-a4",
                    "Clash",
                    built_in_request("built-in-grid-a4").unwrap()
                )
                .is_err()
        );
        drop(library);
        let mut library = TemplateLibrary::open(root.clone()).unwrap();
        assert_eq!(library.last_template_id(), "built-in-dots-a4");
        // A legacy A4 portrait grid default maps onto its built-in, not a copy.
        library
            .migrate_legacy_blank_request(built_in_request("built-in-grid-a4"))
            .unwrap();
        assert_eq!(library.last_template_id(), "built-in-grid-a4");
        assert!(library.records().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
