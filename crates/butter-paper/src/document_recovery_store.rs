//! Durable, content-addressed storage for recoverable native document state.
//!
//! The active index is the authority boundary. Objects and heads written before
//! an index publication are deliberately harmless orphans; clearing performs
//! the inverse ordering and removes the index entry before physical cleanup.
//!
//! On Unix the authority path retains the store/object/head/staging directories
//! and uses descriptor-relative leases, reads, publication, exact retirement and
//! orphan enumeration. External source files remain separately identity-checked.

use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, MutexGuard, OnceLock, TryLockError,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _, PermissionsExt as _};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const STORE_DIRECTORY: &str = "document-recovery-v1";
const OBJECTS_DIRECTORY: &str = "objects";
const HEADS_DIRECTORY: &str = "heads";
const STAGING_DIRECTORY: &str = "staging";
const STAGING_LEASE_NAME: &str = "staging.lock";
const INDEX_NAME: &str = "index.json";
const LEASE_NAME: &str = "store.lock";
const INDEX_VERSION: u64 = 1;
const HEAD_VERSION: u64 = 2;
const MAX_ACTIVE_DOCUMENTS: usize = 64;
const MAX_INDEX_BYTES: u64 = 16 * 1024;
const MAX_HEAD_BYTES: u64 = 128 * 1024;
const MAX_PATH_UNITS: usize = 32_768;
const MAX_BASE_PDF_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_TIMELINE_BYTES: u64 = 256 * 1024 * 1024;
const STREAM_BUFFER_BYTES: usize = 64 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);
static PROCESS_ROOT_LOCKS: OnceLock<Mutex<HashMap<ProcessRootKey, &'static ProcessRootLocks>>> =
    OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ProcessRootKey {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Path(PathBuf),
}

struct ProcessRootLocks {
    store: Mutex<()>,
    staging: Mutex<()>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RecoveryDocumentId([u8; 16]);

impl RecoveryDocumentId {
    pub fn generate() -> Result<Self, DocumentRecoveryStoreError> {
        let mut bytes = [0_u8; 16];
        getrandom::fill(&mut bytes).map_err(|_| DocumentRecoveryStoreError::RandomUnavailable)?;
        Ok(Self(bytes))
    }

    pub fn from_hex(value: &str) -> Result<Self, DocumentRecoveryStoreError> {
        let bytes = decode_fixed_hex::<16>(value).ok_or(DocumentRecoveryStoreError::Corrupt(
            Corruption::InvalidDocumentId,
        ))?;
        Ok(Self(bytes))
    }

    pub fn to_hex(self) -> String {
        hex_encode(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RecoverySourceKind {
    Opened,
    Generated,
}

pub struct RecoveryPublication<'a> {
    pub source_path: &'a Path,
    pub source_kind: RecoverySourceKind,
    pub source_sha256: [u8; 32],
    pub base_pdf: &'a [u8],
    pub timeline: &'a [u8],
    pub current_revision: u64,
    pub saved_revision: u64,
    pub requires_save_as: bool,
}

#[derive(Debug)]
struct PreparedBasePdf {
    temporary: RecoveryTemporary,
    _staging_file_lease: File,
    _process_staging_lease: MutexGuard<'static, ()>,
    sha256: [u8; 32],
    byte_len: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StoredBasePdf {
    sha256: [u8; 32],
    byte_len: u64,
}

/// Recovery metadata and timeline content published against a staged base PDF.
pub struct StagedRecoveryPublication<'a> {
    pub source_path: &'a Path,
    pub source_kind: RecoverySourceKind,
    pub timeline: &'a [u8],
    pub current_revision: u64,
    pub saved_revision: u64,
    pub requires_save_as: bool,
}

/// Compare-and-swap token for the currently authoritative recovery HEAD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecoveryAuthority {
    document_id: RecoveryDocumentId,
    checkpoint_sequence: u64,
    current_revision: u64,
    timeline_sha256: [u8; 32],
}

impl RecoveryAuthority {
    pub fn document_id(self) -> RecoveryDocumentId {
        self.document_id
    }

    pub fn checkpoint_sequence(self) -> u64 {
        self.checkpoint_sequence
    }

    pub fn current_revision(self) -> u64 {
        self.current_revision
    }

    pub fn timeline_sha256(self) -> [u8; 32] {
        self.timeline_sha256
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveredDocument {
    pub id: RecoveryDocumentId,
    pub source_path: PathBuf,
    pub source_kind: RecoverySourceKind,
    pub source_sha256: [u8; 32],
    pub base_pdf: Vec<u8>,
    pub timeline: Vec<u8>,
    pub current_revision: u64,
    pub saved_revision: u64,
    pub requires_save_as: bool,
    pub authority: RecoveryAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PublicationFault {
    None,
    AfterBaseObjectRenameBeforeSync,
    AfterBaseObjectObjectsSyncBeforeStagingSync,
    AfterTimelineObjectRenameBeforeSync,
    BeforeHeadRename,
    AfterHeadRenameBeforeSync,
    AfterHeadBeforeIndex,
    AfterIndexRenameBeforeSync,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishedArtifact {
    /// The named document's HEAD rename completed. Existing index membership
    /// determines whether that HEAD is authoritative.
    Head,
    /// The index rename completed and is the observable authority, but its
    /// containing-directory durability is uncertain. Callers must reload the
    /// index using `document_id` rather than assuming either the old or new set.
    Index,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryObjectDirectory {
    Objects,
    Staging,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corruption {
    InvalidIndex,
    InvalidHead,
    UnsupportedVersion,
    TooManyDocuments,
    Oversize,
    InvalidDocumentId,
    InvalidHash,
    HashMismatch,
    DuplicateDocument,
    ForeignPathEncoding,
    InvalidPath,
    InvalidFilesystemEntry,
}

#[derive(Debug)]
pub enum DocumentRecoveryStoreError {
    Io {
        operation: &'static str,
        kind: io::ErrorKind,
    },
    Corrupt(Corruption),
    InvalidInput(&'static str),
    RandomUnavailable,
    InjectedFault,
    PublishedObjectButDirectorySyncFailed {
        object_sha256: [u8; 32],
        directory: RecoveryObjectDirectory,
        kind: io::ErrorKind,
    },
    StaleAuthority {
        document_id: RecoveryDocumentId,
    },
    PublishedButDirectorySyncFailed {
        artifact: PublishedArtifact,
        document_id: RecoveryDocumentId,
        kind: io::ErrorKind,
    },
}

impl std::fmt::Display for DocumentRecoveryStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for DocumentRecoveryStoreError {}

#[derive(Debug)]
pub struct DocumentRecoveryStore {
    store: PathBuf,
    objects: PathBuf,
    heads: PathBuf,
    staging: PathBuf,
    staging_lease: PathBuf,
    index: PathBuf,
    lease: PathBuf,
    #[cfg(unix)]
    store_directory: File,
    #[cfg(unix)]
    objects_directory: File,
    #[cfg(unix)]
    heads_directory: File,
    #[cfg(unix)]
    staging_directory: File,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryIndex {
    version: u64,
    active: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryHead {
    version: u64,
    document_id: String,
    path_encoding: String,
    source_path: String,
    source_kind: RecoverySourceKind,
    source_sha256: String,
    base_object_sha256: String,
    timeline_object_sha256: String,
    checkpoint_sequence: u64,
    current_revision: u64,
    saved_revision: u64,
    requires_save_as: bool,
}

impl DocumentRecoveryStore {
    /// Opens or creates the private recovery subtree under an existing session root.
    pub fn open(session_state_root: impl AsRef<Path>) -> Result<Self, DocumentRecoveryStoreError> {
        let root = session_state_root.as_ref();
        validate_directory(root, "inspect session-state root")?;
        let store = root.join(STORE_DIRECTORY);
        #[cfg(unix)]
        let root_directory = open_retained_directory(root, false)?;
        #[cfg(unix)]
        let store_directory =
            open_or_create_private_directory_at(&root_directory, STORE_DIRECTORY, &store)?;
        #[cfg(windows)]
        create_private_directory(&store)?;
        let objects = store.join(OBJECTS_DIRECTORY);
        let heads = store.join(HEADS_DIRECTORY);
        #[cfg(unix)]
        let objects_directory =
            open_or_create_private_directory_at(&store_directory, OBJECTS_DIRECTORY, &objects)?;
        #[cfg(unix)]
        let heads_directory =
            open_or_create_private_directory_at(&store_directory, HEADS_DIRECTORY, &heads)?;
        #[cfg(windows)]
        create_private_directory(&objects)?;
        #[cfg(windows)]
        create_private_directory(&heads)?;
        let staging = store.join(STAGING_DIRECTORY);
        #[cfg(unix)]
        let staging_directory =
            open_or_create_private_directory_at(&store_directory, STAGING_DIRECTORY, &staging)?;
        #[cfg(windows)]
        create_private_directory(&staging)?;
        let staging_lease = store.join(STAGING_LEASE_NAME);
        let index = store.join(INDEX_NAME);
        let lease = store.join(LEASE_NAME);
        let result = Self {
            store,
            objects,
            heads,
            staging,
            staging_lease,
            index,
            lease,
            #[cfg(unix)]
            store_directory,
            #[cfg(unix)]
            objects_directory,
            #[cfg(unix)]
            heads_directory,
            #[cfg(unix)]
            staging_directory,
        };
        let _lease = result.acquire_lease()?;
        result.validate_index_entry()?;
        result.collect_orphans_locked()?;
        Ok(result)
    }

    pub fn active_document_ids(
        &self,
    ) -> Result<Vec<RecoveryDocumentId>, DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        self.load_index()
    }

    /// Streams the exact source PDF and publishes its first recovery HEAD/index
    /// under one safe operation. The potentially multi-gigabyte source read is
    /// performed before the authority lease is acquired. A per-root staging
    /// lease keeps concurrent open/orphan collection from deleting the private
    /// temporary before it is consumed.
    pub fn stage_and_publish_new(
        &self,
        expected_sha256: [u8; 32],
        publication: &StagedRecoveryPublication<'_>,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        self.stage_and_publish_new_with_fault(expected_sha256, publication, PublicationFault::None)
    }

    fn stage_and_publish_new_with_fault(
        &self,
        expected_sha256: [u8; 32],
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_streaming_publication(publication)?;
        let candidate_id = {
            let _lease = self.acquire_lease()?;
            let active = self.load_index()?;
            self.select_new_document_id(&active)?
        };
        let prepared = self.prepare_base_pdf(
            publication.source_path,
            expected_sha256,
            MAX_BASE_PDF_BYTES,
            |_| {},
        )?;
        let _lease = self.acquire_lease()?;
        let active = self.load_index()?;
        if active.len() >= MAX_ACTIVE_DOCUMENTS {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "too many active recovery documents",
            ));
        }
        if active.contains(&candidate_id) || self.head_exists(candidate_id)? {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "preflight recovery document id is no longer available",
            ));
        }
        let stored = self.publish_prepared_base(prepared, fault)?;
        self.publish_new_stored_locked(candidate_id, &stored, publication, fault, active)
    }

    fn select_new_document_id(
        &self,
        active: &[RecoveryDocumentId],
    ) -> Result<RecoveryDocumentId, DocumentRecoveryStoreError> {
        if active.len() >= MAX_ACTIVE_DOCUMENTS {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "too many active recovery documents",
            ));
        }
        loop {
            let candidate = RecoveryDocumentId::generate()?;
            if !active.contains(&candidate) && !self.head_exists(candidate)? {
                return Ok(candidate);
            }
        }
    }

    fn prepare_base_pdf<F>(
        &self,
        source_path: &Path,
        expected_sha256: [u8; 32],
        byte_limit: u64,
        mut progress: F,
    ) -> Result<PreparedBasePdf, DocumentRecoveryStoreError>
    where
        F: FnMut(u64),
    {
        if validate_source_path(source_path).is_err() {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "source path is not an absolute native path",
            ));
        }
        let mut source = open_source_file(source_path)?;
        let opened_metadata = source
            .metadata()
            .map_err(|error| io_error("inspect base PDF source", error))?;
        validate_source_file_metadata(&opened_metadata)?;
        if opened_metadata.len() > byte_limit {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "base PDF is too large",
            ));
        }

        let (process_staging_lease, staging_file_lease) = self.acquire_staging_read_lease()?;
        let mut temporary = self.create_staging_temporary("stage")?;
        let mut hasher = Sha256::new();
        let mut byte_len = 0_u64;
        let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
        loop {
            let count = source
                .read(&mut buffer)
                .map_err(|error| io_error("read base PDF source", error))?;
            if count == 0 {
                break;
            }
            byte_len = byte_len.checked_add(count as u64).ok_or(
                DocumentRecoveryStoreError::InvalidInput("base PDF is too large"),
            )?;
            if byte_len > byte_limit {
                return Err(DocumentRecoveryStoreError::InvalidInput(
                    "base PDF is too large",
                ));
            }
            hasher.update(&buffer[..count]);
            temporary
                .file_mut()
                .write_all(&buffer[..count])
                .map_err(|error| io_error("write staged base PDF", error))?;
            progress(byte_len);
        }
        let actual_sha256: [u8; 32] = hasher.finalize().into();
        if actual_sha256 != expected_sha256 {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "source digest does not match the exact base PDF",
            ));
        }

        let final_open_metadata = source
            .metadata()
            .map_err(|error| io_error("reinspect base PDF source", error))?;
        validate_source_file_metadata(&final_open_metadata)?;
        let final_path_metadata = fs::symlink_metadata(source_path)
            .map_err(|error| io_error("reinspect base PDF source path", error))?;
        validate_source_file_metadata(&final_path_metadata)?;
        if !same_file_identity(&opened_metadata, &final_open_metadata)
            || !same_file_identity(&opened_metadata, &final_path_metadata)
            || final_open_metadata.len() != byte_len
        {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "base PDF source changed while it was staged",
            ));
        }

        temporary
            .file_mut()
            .sync_all()
            .map_err(|error| io_error("sync staged base PDF", error))?;
        Ok(PreparedBasePdf {
            temporary,
            _staging_file_lease: staging_file_lease,
            _process_staging_lease: process_staging_lease,
            sha256: actual_sha256,
            byte_len,
        })
    }

    fn publish_prepared_base(
        &self,
        mut prepared: PreparedBasePdf,
        fault: PublicationFault,
    ) -> Result<StoredBasePdf, DocumentRecoveryStoreError> {
        prepared.temporary.close();
        let destination = self.object_path(prepared.sha256);
        match self.open_optional_object(prepared.sha256, "inspect staged base PDF object")? {
            Some(_) => {
                self.verify_existing_object(&destination, prepared.sha256, prepared.byte_len)?;
                self.sync_objects_for_reuse(prepared.sha256)?;
                self.remove_prepared_staging_and_sync(&mut prepared)?;
                return Ok(StoredBasePdf {
                    sha256: prepared.sha256,
                    byte_len: prepared.byte_len,
                });
            }
            None => {}
        }
        prepared.temporary.publish(
            &self.objects,
            #[cfg(unix)]
            &self.objects_directory,
            &Self::object_leaf(prepared.sha256),
        )?;
        if fault == PublicationFault::AfterBaseObjectRenameBeforeSync {
            return Err(object_sync_error(
                prepared.sha256,
                RecoveryObjectDirectory::Objects,
                io::ErrorKind::Interrupted,
            ));
        }
        self.sync_objects_for_reuse(prepared.sha256)?;
        if fault == PublicationFault::AfterBaseObjectObjectsSyncBeforeStagingSync {
            return Err(object_sync_error(
                prepared.sha256,
                RecoveryObjectDirectory::Staging,
                io::ErrorKind::Interrupted,
            ));
        }
        self.sync_staging_for_object(prepared.sha256)?;
        self.verify_existing_object(&destination, prepared.sha256, prepared.byte_len)?;
        Ok(StoredBasePdf {
            sha256: prepared.sha256,
            byte_len: prepared.byte_len,
        })
    }

    fn remove_prepared_staging_and_sync(
        &self,
        prepared: &mut PreparedBasePdf,
    ) -> Result<(), DocumentRecoveryStoreError> {
        prepared.temporary.remove()?;
        self.sync_staging_for_object(prepared.sha256)
    }

    fn sync_staging_for_object(&self, hash: [u8; 32]) -> Result<(), DocumentRecoveryStoreError> {
        self.sync_staging_directory().map_err(|error| match error {
            DocumentRecoveryStoreError::Io { kind, .. } => {
                object_sync_error(hash, RecoveryObjectDirectory::Staging, kind)
            }
            other => other,
        })
    }

    pub fn publish_new(
        &self,
        publication: &RecoveryPublication<'_>,
    ) -> Result<RecoveryDocumentId, DocumentRecoveryStoreError> {
        self.publish_new_with_fault(publication, PublicationFault::None)
    }

    pub(crate) fn publish_new_with_fault(
        &self,
        publication: &RecoveryPublication<'_>,
        fault: PublicationFault,
    ) -> Result<RecoveryDocumentId, DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        validate_publication(publication)?;
        if self.load_index()?.len() >= MAX_ACTIVE_DOCUMENTS {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "too many active recovery documents",
            ));
        }
        let base_hash = sha256(publication.base_pdf);
        self.store_object(
            base_hash,
            publication.base_pdf,
            fault == PublicationFault::AfterBaseObjectRenameBeforeSync,
        )?;
        let stored = StoredBasePdf {
            sha256: base_hash,
            byte_len: publication.base_pdf.len() as u64,
        };
        let staged_publication = StagedRecoveryPublication {
            source_path: publication.source_path,
            source_kind: publication.source_kind,
            timeline: publication.timeline,
            current_revision: publication.current_revision,
            saved_revision: publication.saved_revision,
            requires_save_as: publication.requires_save_as,
        };
        let active = self.load_index()?;
        let id = self.select_new_document_id(&active)?;
        self.publish_new_stored_locked(id, &stored, &staged_publication, fault, active)
            .map(RecoveryAuthority::document_id)
    }

    fn publish_new_stored_locked(
        &self,
        id: RecoveryDocumentId,
        stored_base: &StoredBasePdf,
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
        mut active: Vec<RecoveryDocumentId>,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_stored_publication(stored_base, publication)?;
        self.verify_stored_base(stored_base)?;
        if active.len() >= MAX_ACTIVE_DOCUMENTS {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "too many active recovery documents",
            ));
        }
        if active.contains(&id) || self.head_exists(id)? {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "recovery document id is not available",
            ));
        }
        let authority = self.publish_stored_head(id, 1, stored_base, publication, fault, true)?;
        if fault == PublicationFault::AfterHeadBeforeIndex {
            return Err(DocumentRecoveryStoreError::InjectedFault);
        }
        active.push(id);
        self.replace_index(&active, id, fault)?;
        Ok(authority)
    }

    pub fn stage_and_replace(
        &self,
        expected: &RecoveryAuthority,
        expected_sha256: [u8; 32],
        publication: &StagedRecoveryPublication<'_>,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_streaming_publication(publication)?;
        {
            let _lease = self.acquire_lease()?;
            self.require_current_authority(expected)?;
        }
        let prepared = self.prepare_base_pdf(
            publication.source_path,
            expected_sha256,
            MAX_BASE_PDF_BYTES,
            |_| {},
        )?;
        let _lease = self.acquire_lease()?;
        self.require_current_authority(expected)?;
        let stored = self.publish_prepared_base(prepared, PublicationFault::None)?;
        self.replace_stored_cas_locked(expected, &stored, publication, PublicationFault::None, true)
    }

    /// Replaces the authoritative base and timeline after a validated save.
    ///
    /// Saving does not create a new edit revision, but it does move the saved
    /// revision to the current revision and may change the source identity for
    /// Save As. Keep that same-revision transition separate from ordinary
    /// edit/undo/redo publication so the latter continues to reject divergent
    /// equal-revision checkpoints.
    pub fn rebase_after_save(
        &self,
        expected: &RecoveryAuthority,
        expected_sha256: [u8; 32],
        publication: &StagedRecoveryPublication<'_>,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_streaming_publication(publication)?;
        if publication.source_kind != RecoverySourceKind::Opened
            || publication.requires_save_as
            || publication.current_revision != expected.current_revision
            || publication.saved_revision != publication.current_revision
        {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "saved recovery rebase is not a clean current-revision opened source",
            ));
        }
        {
            let _lease = self.acquire_lease()?;
            self.require_current_authority(expected)?;
        }
        let prepared = self.prepare_base_pdf(
            publication.source_path,
            expected_sha256,
            MAX_BASE_PDF_BYTES,
            |_| {},
        )?;
        let _lease = self.acquire_lease()?;
        let old_head = self.require_current_authority(expected)?;
        let stored = self.publish_prepared_base(prepared, PublicationFault::None)?;
        let next = self.replace_stored_saved_rebase_cas_locked(
            expected,
            &stored,
            publication,
            PublicationFault::None,
        )?;
        for hash in [old_head.base_object_sha256, old_head.timeline_object_sha256] {
            if let Ok(hash) = parse_hash(&hash) {
                let _ = self.remove_object_if_unreferenced(hash);
            }
        }
        Ok(next)
    }

    /// Rebinds an exact recovery checkpoint to a materialised startup-recovery copy.
    ///
    /// The copy is a generated source which still requires Save As. Rebinding
    /// changes only its source path/base identity and checkpoint sequence: the
    /// current and saved revisions and the complete timeline remain unchanged.
    pub fn rebind_after_copy_recovery(
        &self,
        expected: &RecoveryAuthority,
        expected_sha256: [u8; 32],
        publication: &StagedRecoveryPublication<'_>,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        self.rebind_after_copy_recovery_with_fault(
            expected,
            expected_sha256,
            publication,
            PublicationFault::None,
        )
    }

    fn rebind_after_copy_recovery_with_fault(
        &self,
        expected: &RecoveryAuthority,
        expected_sha256: [u8; 32],
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_streaming_publication(publication)?;
        {
            let _lease = self.acquire_lease()?;
            let old_head = self.require_current_authority(expected)?;
            validate_copy_recovery_rebind(&old_head, expected, publication)?;
        }
        let prepared = self.prepare_base_pdf(
            publication.source_path,
            expected_sha256,
            MAX_BASE_PDF_BYTES,
            |_| {},
        )?;
        let _lease = self.acquire_lease()?;
        let old_head = self.require_current_authority(expected)?;
        validate_copy_recovery_rebind(&old_head, expected, publication)?;
        let stored = self.publish_prepared_base(prepared, fault)?;
        let next_sequence = expected.checkpoint_sequence.checked_add(1).ok_or(
            DocumentRecoveryStoreError::InvalidInput("recovery checkpoint sequence exhausted"),
        )?;
        let next = self.publish_stored_head(
            expected.document_id,
            next_sequence,
            &stored,
            publication,
            fault,
            true,
        )?;
        for hash in [old_head.base_object_sha256, old_head.timeline_object_sha256] {
            if let Ok(hash) = parse_hash(&hash) {
                let _ = self.remove_object_if_unreferenced(hash);
            }
        }
        Ok(next)
    }

    #[cfg(test)]
    fn replace(
        &self,
        id: RecoveryDocumentId,
        publication: &RecoveryPublication<'_>,
    ) -> Result<(), DocumentRecoveryStoreError> {
        self.replace_with_fault(id, publication, PublicationFault::None)
    }

    /// Replaces only the recovery timeline while retaining the already
    /// authoritative base object. This is the fast path for edit/undo/redo
    /// checkpoints after `stage_and_publish_new` established the document.
    pub fn replace_timeline(
        &self,
        expected: &RecoveryAuthority,
        publication: &StagedRecoveryPublication<'_>,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        self.replace_timeline_with_fault(expected, publication, PublicationFault::None)
    }

    fn replace_timeline_with_fault(
        &self,
        expected: &RecoveryAuthority,
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_streaming_publication(publication)?;
        let _lease = self.acquire_lease()?;
        let old_head = self.require_current_authority(expected)?;
        if decode_path(&old_head.path_encoding, &old_head.source_path)? != publication.source_path
            || old_head.source_kind != publication.source_kind
            || old_head.requires_save_as != publication.requires_save_as
        {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "timeline replacement changed source identity",
            ));
        }
        let base_hash = parse_hash(&old_head.base_object_sha256)?;
        let base_file = self.open_object(base_hash, "inspect active recovery base object")?;
        let base_metadata = base_file
            .metadata()
            .map_err(|error| io_error("inspect active recovery base object", error))?;
        if base_metadata.len() > MAX_BASE_PDF_BYTES {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
        }
        self.sync_objects_for_reuse(base_hash)?;
        let stored = StoredBasePdf {
            sha256: base_hash,
            byte_len: base_metadata.len(),
        };
        let next = self.replace_stored_cas_locked(expected, &stored, publication, fault, false)?;
        if let Ok(old_timeline_hash) = parse_hash(&old_head.timeline_object_sha256) {
            let _ = self.remove_object_if_unreferenced(old_timeline_hash);
        }
        Ok(next)
    }

    #[cfg(test)]
    fn replace_with_fault(
        &self,
        id: RecoveryDocumentId,
        publication: &RecoveryPublication<'_>,
        fault: PublicationFault,
    ) -> Result<(), DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        validate_publication(publication)?;
        if !self.load_index()?.contains(&id) {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "document is not active",
            ));
        }
        let base_hash = sha256(publication.base_pdf);
        self.store_object(
            base_hash,
            publication.base_pdf,
            fault == PublicationFault::AfterBaseObjectRenameBeforeSync,
        )?;
        let stored = StoredBasePdf {
            sha256: base_hash,
            byte_len: publication.base_pdf.len() as u64,
        };
        let staged_publication = StagedRecoveryPublication {
            source_path: publication.source_path,
            source_kind: publication.source_kind,
            timeline: publication.timeline,
            current_revision: publication.current_revision,
            saved_revision: publication.saved_revision,
            requires_save_as: publication.requires_save_as,
        };
        self.replace_stored_legacy_locked(id, &stored, &staged_publication, fault)
    }

    #[cfg(test)]
    fn replace_stored_legacy_locked(
        &self,
        id: RecoveryDocumentId,
        stored_base: &StoredBasePdf,
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
    ) -> Result<(), DocumentRecoveryStoreError> {
        validate_stored_publication(stored_base, publication)?;
        self.verify_stored_base(stored_base)?;
        if !self.load_index()?.contains(&id) {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "document is not active",
            ));
        }
        let old_head = self.read_head(id)?;
        let next_sequence = old_head.checkpoint_sequence.checked_add(1).ok_or(
            DocumentRecoveryStoreError::InvalidInput("recovery checkpoint sequence exhausted"),
        )?;
        self.publish_stored_head(id, next_sequence, stored_base, publication, fault, true)?;
        for hash in [old_head.base_object_sha256, old_head.timeline_object_sha256] {
            if let Ok(hash) = parse_hash(&hash) {
                let _ = self.remove_object_if_unreferenced(hash);
            }
        }
        Ok(())
    }

    fn replace_stored_cas_locked(
        &self,
        expected: &RecoveryAuthority,
        stored_base: &StoredBasePdf,
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
        verify_base_hash: bool,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_stored_publication(stored_base, publication)?;
        let old_head = self.require_current_authority(expected)?;
        let new_timeline_hash = sha256(publication.timeline);
        if publication.current_revision == expected.current_revision {
            let same_source = decode_path(&old_head.path_encoding, &old_head.source_path)?
                == publication.source_path;
            if new_timeline_hash == expected.timeline_sha256
                && old_head.base_object_sha256 == hex_encode(&stored_base.sha256)
                && old_head.saved_revision == publication.saved_revision
                && old_head.source_kind == publication.source_kind
                && old_head.requires_save_as == publication.requires_save_as
                && same_source
            {
                self.confirm_authority_durable_locked(expected)?;
                return Ok(*expected);
            }
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "equal revision has different recovery timeline",
            ));
        }
        let next_sequence = expected.checkpoint_sequence.checked_add(1).ok_or(
            DocumentRecoveryStoreError::InvalidInput("recovery checkpoint sequence exhausted"),
        )?;
        let next = self.publish_stored_head(
            expected.document_id,
            next_sequence,
            stored_base,
            publication,
            fault,
            verify_base_hash,
        )?;
        for hash in [old_head.base_object_sha256, old_head.timeline_object_sha256] {
            if let Ok(hash) = parse_hash(&hash) {
                let _ = self.remove_object_if_unreferenced(hash);
            }
        }
        Ok(next)
    }

    fn replace_stored_saved_rebase_cas_locked(
        &self,
        expected: &RecoveryAuthority,
        stored_base: &StoredBasePdf,
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_stored_publication(stored_base, publication)?;
        self.require_current_authority(expected)?;
        if publication.source_kind != RecoverySourceKind::Opened
            || publication.requires_save_as
            || publication.current_revision != expected.current_revision
            || publication.saved_revision != publication.current_revision
        {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "saved recovery rebase is not a clean current-revision opened source",
            ));
        }
        let next_sequence = expected.checkpoint_sequence.checked_add(1).ok_or(
            DocumentRecoveryStoreError::InvalidInput("recovery checkpoint sequence exhausted"),
        )?;
        self.publish_stored_head(
            expected.document_id,
            next_sequence,
            stored_base,
            publication,
            fault,
            true,
        )
    }

    /// Confirms that an observable recovery authority is durably anchored.
    ///
    /// This reconciles post-rename directory-sync ambiguity: it validates the
    /// exact current CAS token and both content-addressed objects, then syncs
    /// every directory whose entries establish that authority.
    pub fn confirm_authority_durable(
        &self,
        authority: &RecoveryAuthority,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        self.confirm_authority_durable_locked(authority)
    }

    fn confirm_authority_durable_locked(
        &self,
        authority: &RecoveryAuthority,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        let head = self.require_current_authority(authority)?;
        let base_hash = parse_hash(&head.base_object_sha256)?;
        let timeline_hash = parse_hash(&head.timeline_object_sha256)?;
        self.verify_authority_object(base_hash, MAX_BASE_PDF_BYTES)?;
        self.verify_authority_object(timeline_hash, MAX_TIMELINE_BYTES)?;
        self.sync_objects_for_reuse(base_hash)?;
        self.sync_staging_for_object(base_hash)?;
        self.sync_heads_directory().map_err(|error| {
            publication_sync_error(PublishedArtifact::Head, authority.document_id, error)
        })?;
        self.sync_store_directory().map_err(|error| {
            publication_sync_error(PublishedArtifact::Index, authority.document_id, error)
        })?;
        Ok(*authority)
    }

    fn verify_authority_object(
        &self,
        hash: [u8; 32],
        limit: u64,
    ) -> Result<(), DocumentRecoveryStoreError> {
        let file = self.open_object(hash, "inspect authoritative recovery object")?;
        let metadata = file
            .metadata()
            .map_err(|error| io_error("inspect authoritative recovery object", error))?;
        if metadata.len() > limit {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
        }
        self.verify_existing_object(&self.object_path(hash), hash, metadata.len())
    }

    pub fn load(
        &self,
        id: RecoveryDocumentId,
    ) -> Result<Option<RecoveredDocument>, DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        self.load_locked(id)
    }

    fn load_locked(
        &self,
        id: RecoveryDocumentId,
    ) -> Result<Option<RecoveredDocument>, DocumentRecoveryStoreError> {
        if !self.load_index()?.contains(&id) {
            return Ok(None);
        }
        let head = self.read_head(id)?;
        let source_path = decode_path(&head.path_encoding, &head.source_path)?;
        validate_source_path(&source_path).map_err(DocumentRecoveryStoreError::Corrupt)?;
        let source_sha256 = parse_hash(&head.source_sha256)?;
        let base_hash = parse_hash(&head.base_object_sha256)?;
        let timeline_hash = parse_hash(&head.timeline_object_sha256)?;
        let base_pdf = self.read_object(base_hash, MAX_BASE_PDF_BYTES)?;
        let timeline = self.read_object(timeline_hash, MAX_TIMELINE_BYTES)?;
        let authority = RecoveryAuthority {
            document_id: id,
            checkpoint_sequence: head.checkpoint_sequence,
            current_revision: head.current_revision,
            timeline_sha256: timeline_hash,
        };
        Ok(Some(RecoveredDocument {
            id,
            source_path,
            source_kind: head.source_kind,
            source_sha256,
            base_pdf,
            timeline,
            current_revision: head.current_revision,
            saved_revision: head.saved_revision,
            requires_save_as: head.requires_save_as,
            authority,
        }))
    }

    /// Removes authority first. Failure to clean physical files cannot revive a document.
    pub fn clear(&self, id: RecoveryDocumentId) -> Result<bool, DocumentRecoveryStoreError> {
        self.clear_with_fault(id, PublicationFault::None)
    }

    /// Retires only the exact checkpoint held by the caller. A stale session must
    /// never erase a newer checkpoint published under the same document id.
    pub fn clear_authority(
        &self,
        expected: &RecoveryAuthority,
    ) -> Result<bool, DocumentRecoveryStoreError> {
        self.clear_authority_with_fault(expected, PublicationFault::None)
    }

    pub(crate) fn clear_authority_with_fault(
        &self,
        expected: &RecoveryAuthority,
        fault: PublicationFault,
    ) -> Result<bool, DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        self.require_current_authority(expected)?;
        self.clear_locked(expected.document_id, fault)
    }

    pub(crate) fn clear_with_fault(
        &self,
        id: RecoveryDocumentId,
        fault: PublicationFault,
    ) -> Result<bool, DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        self.clear_locked(id, fault)
    }

    fn clear_locked(
        &self,
        id: RecoveryDocumentId,
        fault: PublicationFault,
    ) -> Result<bool, DocumentRecoveryStoreError> {
        let mut active = self.load_index()?;
        let Some(position) = active.iter().position(|candidate| *candidate == id) else {
            return Ok(false);
        };
        active.remove(position);
        self.replace_index(&active, id, fault)?;

        let head = self.read_head(id).ok();
        let _ = self.remove_head_child(id);
        if let Some(head) = head {
            for hash in [head.base_object_sha256, head.timeline_object_sha256] {
                if let Ok(hash) = parse_hash(&hash) {
                    let _ = self.remove_object_if_unreferenced(hash);
                }
            }
        }
        let _ = self.sync_heads_directory();
        let _ = self.sync_objects_directory();
        Ok(true)
    }

    /// Removes only unreferenced, store-owned heads, objects and temporary files.
    pub fn collect_orphans(&self) -> Result<(), DocumentRecoveryStoreError> {
        let _lease = self.acquire_lease()?;
        self.collect_orphans_locked()
    }

    fn publish_stored_head(
        &self,
        id: RecoveryDocumentId,
        checkpoint_sequence: u64,
        stored_base: &StoredBasePdf,
        publication: &StagedRecoveryPublication<'_>,
        fault: PublicationFault,
        verify_base_hash: bool,
    ) -> Result<RecoveryAuthority, DocumentRecoveryStoreError> {
        validate_stored_publication(stored_base, publication)?;
        if verify_base_hash {
            self.verify_stored_base(stored_base)?;
        }
        let timeline_hash = sha256(publication.timeline);
        self.store_object(
            timeline_hash,
            publication.timeline,
            fault == PublicationFault::AfterTimelineObjectRenameBeforeSync,
        )?;
        let (path_encoding, source_path) = encode_path(publication.source_path);
        let head = RecoveryHead {
            version: HEAD_VERSION,
            document_id: id.to_hex(),
            path_encoding: path_encoding.into(),
            source_path,
            source_kind: publication.source_kind,
            source_sha256: hex_encode(&stored_base.sha256),
            base_object_sha256: hex_encode(&stored_base.sha256),
            timeline_object_sha256: hex_encode(&timeline_hash),
            checkpoint_sequence,
            current_revision: publication.current_revision,
            saved_revision: publication.saved_revision,
            requires_save_as: publication.requires_save_as,
        };
        let bytes = serde_json::to_vec(&head)
            .map_err(|_| DocumentRecoveryStoreError::InvalidInput("head is not serializable"))?;
        if bytes.len() as u64 > MAX_HEAD_BYTES {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "head is too large",
            ));
        }
        let mut temporary = self.create_heads_temporary("head")?;
        write_and_sync(temporary.file_mut(), &bytes)?;
        temporary.close();
        if fault == PublicationFault::BeforeHeadRename {
            return Err(DocumentRecoveryStoreError::InjectedFault);
        }
        temporary.publish(
            &self.heads,
            #[cfg(unix)]
            &self.heads_directory,
            &Self::head_leaf(id),
        )?;
        if fault == PublicationFault::AfterHeadRenameBeforeSync {
            return Err(
                DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                    artifact: PublishedArtifact::Head,
                    document_id: id,
                    kind: io::ErrorKind::Interrupted,
                },
            );
        }
        self.sync_heads_directory()
            .map_err(|error| publication_sync_error(PublishedArtifact::Head, id, error))?;
        Ok(RecoveryAuthority {
            document_id: id,
            checkpoint_sequence,
            current_revision: publication.current_revision,
            timeline_sha256: timeline_hash,
        })
    }

    fn verify_stored_base(
        &self,
        stored_base: &StoredBasePdf,
    ) -> Result<(), DocumentRecoveryStoreError> {
        if stored_base.byte_len > MAX_BASE_PDF_BYTES {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "staged base PDF is too large",
            ));
        }
        let path = self.object_path(stored_base.sha256);
        self.verify_existing_object(&path, stored_base.sha256, stored_base.byte_len)?;
        self.sync_objects_for_reuse(stored_base.sha256)
    }

    fn store_object(
        &self,
        hash: [u8; 32],
        bytes: &[u8],
        fault_after_rename: bool,
    ) -> Result<(), DocumentRecoveryStoreError> {
        match self.open_optional_object(hash, "inspect object")? {
            Some(_) => {
                let destination = self.object_path(hash);
                self.verify_existing_object(&destination, hash, bytes.len() as u64)?;
                return self.sync_objects_for_reuse(hash);
            }
            None => {}
        }
        let mut temporary = self.create_objects_temporary("object")?;
        write_and_sync(temporary.file_mut(), bytes)?;
        temporary.close();
        // This private directory and digest-derived name mean concurrent
        // writers can only publish identical bytes at this destination.
        temporary.publish(
            &self.objects,
            #[cfg(unix)]
            &self.objects_directory,
            &Self::object_leaf(hash),
        )?;
        if fault_after_rename {
            return Err(object_sync_error(
                hash,
                RecoveryObjectDirectory::Objects,
                io::ErrorKind::Interrupted,
            ));
        }
        self.sync_objects_for_reuse(hash)
    }

    fn sync_objects_for_reuse(&self, hash: [u8; 32]) -> Result<(), DocumentRecoveryStoreError> {
        self.sync_objects_directory().map_err(|error| match error {
            DocumentRecoveryStoreError::Io { kind, .. } => {
                object_sync_error(hash, RecoveryObjectDirectory::Objects, kind)
            }
            other => other,
        })
    }

    fn verify_existing_object(
        &self,
        _path: &Path,
        expected_hash: [u8; 32],
        expected_len: u64,
    ) -> Result<(), DocumentRecoveryStoreError> {
        let file = self.open_object(expected_hash, "inspect object")?;
        let metadata = file
            .metadata()
            .map_err(|error| io_error("inspect object", error))?;
        if metadata.len() != expected_len {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::HashMismatch,
            ));
        }
        let (actual_hash, actual_len) = hash_open_file_bounded(file, expected_len)?;
        if actual_len != expected_len || actual_hash != expected_hash {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::HashMismatch,
            ));
        }
        Ok(())
    }

    fn read_object(
        &self,
        hash: [u8; 32],
        limit: u64,
    ) -> Result<Vec<u8>, DocumentRecoveryStoreError> {
        let file = self.open_object(hash, "inspect object")?;
        let metadata = file
            .metadata()
            .map_err(|error| io_error("inspect object", error))?;
        if metadata.len() > limit {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
        }
        let bytes = read_bounded_file(file, limit)?;
        if sha256(&bytes) != hash {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::HashMismatch,
            ));
        }
        Ok(bytes)
    }

    fn load_index(&self) -> Result<Vec<RecoveryDocumentId>, DocumentRecoveryStoreError> {
        let Some(file) = self.open_optional_index()? else {
            return Ok(Vec::new());
        };
        let metadata = file
            .metadata()
            .map_err(|error| io_error("inspect recovery index", error))?;
        if metadata.len() > MAX_INDEX_BYTES {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
        }
        let bytes = read_bounded_file(file, MAX_INDEX_BYTES)?;
        let index: RecoveryIndex = serde_json::from_slice(&bytes)
            .map_err(|_| DocumentRecoveryStoreError::Corrupt(Corruption::InvalidIndex))?;
        if index.version != INDEX_VERSION {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::UnsupportedVersion,
            ));
        }
        if index.active.len() > MAX_ACTIVE_DOCUMENTS {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::TooManyDocuments,
            ));
        }
        let mut seen = HashSet::new();
        index
            .active
            .into_iter()
            .map(|value| {
                let id = RecoveryDocumentId::from_hex(&value)?;
                if !seen.insert(id) {
                    return Err(DocumentRecoveryStoreError::Corrupt(
                        Corruption::DuplicateDocument,
                    ));
                }
                Ok(id)
            })
            .collect()
    }

    fn replace_index(
        &self,
        active: &[RecoveryDocumentId],
        document_id: RecoveryDocumentId,
        fault: PublicationFault,
    ) -> Result<(), DocumentRecoveryStoreError> {
        let index = RecoveryIndex {
            version: INDEX_VERSION,
            active: active.iter().map(|id| id.to_hex()).collect(),
        };
        let bytes = serde_json::to_vec(&index)
            .map_err(|_| DocumentRecoveryStoreError::InvalidInput("index is not serializable"))?;
        let mut temporary = self.create_store_temporary("index")?;
        write_and_sync(temporary.file_mut(), &bytes)?;
        temporary.close();
        temporary.publish(
            &self.store,
            #[cfg(unix)]
            &self.store_directory,
            Path::new(INDEX_NAME),
        )?;
        if fault == PublicationFault::AfterIndexRenameBeforeSync {
            return Err(
                DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                    artifact: PublishedArtifact::Index,
                    document_id,
                    kind: io::ErrorKind::Interrupted,
                },
            );
        }
        self.sync_store_directory()
            .map_err(|error| publication_sync_error(PublishedArtifact::Index, document_id, error))
    }

    fn read_head(
        &self,
        id: RecoveryDocumentId,
    ) -> Result<RecoveryHead, DocumentRecoveryStoreError> {
        let file = self.open_head(id, "inspect recovery head")?;
        let metadata = file
            .metadata()
            .map_err(|error| io_error("inspect recovery head", error))?;
        if metadata.len() > MAX_HEAD_BYTES {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
        }
        let bytes = read_bounded_file(file, MAX_HEAD_BYTES)?;
        let head: RecoveryHead = serde_json::from_slice(&bytes)
            .map_err(|_| DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead))?;
        if head.version != HEAD_VERSION {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::UnsupportedVersion,
            ));
        }
        if head.document_id != id.to_hex() {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead));
        }
        if head.saved_revision > head.current_revision {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead));
        }
        if head.checkpoint_sequence == 0 {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead));
        }
        let source_hash = parse_hash(&head.source_sha256)?;
        let base_hash = parse_hash(&head.base_object_sha256)?;
        parse_hash(&head.timeline_object_sha256)?;
        if source_hash != base_hash
            || !matches!(
                (head.source_kind, head.requires_save_as),
                (RecoverySourceKind::Opened, false) | (RecoverySourceKind::Generated, true)
            )
        {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead));
        }
        Ok(head)
    }

    fn require_current_authority(
        &self,
        expected: &RecoveryAuthority,
    ) -> Result<RecoveryHead, DocumentRecoveryStoreError> {
        if !self.load_index()?.contains(&expected.document_id) {
            return Err(DocumentRecoveryStoreError::StaleAuthority {
                document_id: expected.document_id,
            });
        }
        let head = self.read_head(expected.document_id)?;
        let timeline_sha256 = parse_hash(&head.timeline_object_sha256)?;
        if head.checkpoint_sequence != expected.checkpoint_sequence
            || head.current_revision != expected.current_revision
            || timeline_sha256 != expected.timeline_sha256
        {
            return Err(DocumentRecoveryStoreError::StaleAuthority {
                document_id: expected.document_id,
            });
        }
        Ok(head)
    }

    fn validate_index_entry(&self) -> Result<(), DocumentRecoveryStoreError> {
        self.open_optional_index().map(|_| ())
    }

    fn remove_object_if_unreferenced(
        &self,
        hash: [u8; 32],
    ) -> Result<(), DocumentRecoveryStoreError> {
        for id in self.load_index()? {
            let head = self.read_head(id)?;
            if head.base_object_sha256 == hex_encode(&hash)
                || head.timeline_object_sha256 == hex_encode(&hash)
            {
                return Ok(());
            }
        }
        self.remove_object_child(hash)
    }

    fn acquire_lease(&self) -> Result<StoreLease, DocumentRecoveryStoreError> {
        let process_guard = self
            .process_root_locks()?
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(unix)]
        let file = open_private_lease_at(
            &self.store_directory,
            LEASE_NAME,
            "open recovery-store lease",
        )?;
        #[cfg(windows)]
        let file = {
            let mut options = OpenOptions::new();
            options.read(true).write(true).create(true);
            {
                use std::os::windows::fs::OpenOptionsExt as _;
                options.share_mode(0);
            }
            options
                .open(&self.lease)
                .map_err(|error| io_error("open recovery-store lease", error))?
        };
        validate_regular_metadata(
            &file
                .metadata()
                .map_err(|error| io_error("inspect recovery-store lease", error))?,
        )?;
        #[cfg(unix)]
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
            .map_err(|error| io_error("lock recovery store", io::Error::from(error)))?;
        Ok(StoreLease {
            _file: file,
            _process_guard: process_guard,
        })
    }

    fn process_root_locks(&self) -> Result<&'static ProcessRootLocks, DocumentRecoveryStoreError> {
        #[cfg(unix)]
        let key = {
            let metadata = self
                .store_directory
                .metadata()
                .map_err(|error| io_error("identify recovery-store root", error))?;
            ProcessRootKey::Unix {
                device: metadata.dev(),
                inode: metadata.ino(),
            }
        };
        #[cfg(windows)]
        let key = ProcessRootKey::Path(
            fs::canonicalize(&self.store)
                .map_err(|error| io_error("identify recovery-store root", error))?,
        );
        let mut roots = PROCESS_ROOT_LOCKS
            .get_or_init(|| Mutex::new(HashMap::new()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(*roots.entry(key).or_insert_with(|| {
            Box::leak(Box::new(ProcessRootLocks {
                store: Mutex::new(()),
                staging: Mutex::new(()),
            }))
        }))
    }

    fn acquire_staging_read_lease(
        &self,
    ) -> Result<(MutexGuard<'static, ()>, File), DocumentRecoveryStoreError> {
        let process_guard = self
            .process_root_locks()?
            .staging
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(unix)]
        let file = open_private_lease_at(
            &self.store_directory,
            STAGING_LEASE_NAME,
            "open recovery staging lease",
        )?;
        #[cfg(windows)]
        let file = open_private_lease(&self.staging_lease, "open recovery staging lease")?;
        #[cfg(unix)]
        rustix::fs::flock(&file, rustix::fs::FlockOperation::LockExclusive)
            .map_err(|error| io_error("lock recovery staging", io::Error::from(error)))?;
        Ok((process_guard, file))
    }

    fn try_acquire_staging_write_lease(
        &self,
    ) -> Result<Option<(MutexGuard<'static, ()>, File)>, DocumentRecoveryStoreError> {
        let process_guard = match self.process_root_locks()?.staging.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        #[cfg(unix)]
        let file = open_private_lease_at(
            &self.store_directory,
            STAGING_LEASE_NAME,
            "open recovery staging lease",
        )?;
        #[cfg(windows)]
        let file = open_private_lease(&self.staging_lease, "open recovery staging lease")?;
        #[cfg(unix)]
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(error)
                if io::Error::from_raw_os_error(error.raw_os_error()).kind()
                    == io::ErrorKind::WouldBlock =>
            {
                return Ok(None);
            }
            Err(error) => {
                return Err(io_error(
                    "lock recovery staging for collection",
                    io::Error::from(error),
                ));
            }
        }
        Ok(Some((process_guard, file)))
    }

    fn collect_orphans_locked(&self) -> Result<(), DocumentRecoveryStoreError> {
        let active = self.load_index()?;
        let active_names = active
            .iter()
            .map(|id| format!("{}.json", id.to_hex()))
            .collect::<HashSet<_>>();
        let mut referenced_objects = HashSet::new();
        for id in active {
            let head = self.read_head(id)?;
            referenced_objects.insert(hex_encode(&parse_hash(&head.base_object_sha256)?));
            referenced_objects.insert(hex_encode(&parse_hash(&head.timeline_object_sha256)?));
        }

        let heads_changed = self.remove_matching_children(
            &self.heads,
            #[cfg(unix)]
            &self.heads_directory,
            "scan recovery heads",
            "remove orphan recovery head",
            |name| {
                let owned_head = name
                    .strip_suffix(".json")
                    .is_some_and(|id| decode_fixed_hex::<16>(id).is_some());
                let owned_temp = is_owned_temporary_name(&name, "head");
                (owned_head && !active_names.contains(name)) || owned_temp
            },
        )?;

        let objects_changed = self.remove_matching_children(
            &self.objects,
            #[cfg(unix)]
            &self.objects_directory,
            "scan recovery objects",
            "remove orphan recovery object",
            |name| {
                let owned_object = decode_fixed_hex::<32>(name).is_some();
                let owned_temp = is_owned_temporary_name(name, "object");
                (owned_object && !referenced_objects.contains(name)) || owned_temp
            },
        )?;

        let mut staging_changed = false;
        if let Some((_process_staging_guard, _staging_file_guard)) =
            self.try_acquire_staging_write_lease()?
        {
            staging_changed = self.remove_matching_children(
                &self.staging,
                #[cfg(unix)]
                &self.staging_directory,
                "scan recovery staging",
                "remove orphan recovery staging",
                |name| is_owned_temporary_name(name, "stage"),
            )?;
        }

        let store_changed = self.remove_matching_children(
            &self.store,
            #[cfg(unix)]
            &self.store_directory,
            "scan recovery store",
            "remove orphan recovery index",
            |name| is_owned_temporary_name(name, "index"),
        )?;
        if heads_changed {
            self.sync_heads_directory()?;
        }
        if objects_changed {
            self.sync_objects_directory()?;
        }
        if staging_changed {
            self.sync_staging_directory()?;
        }
        if store_changed {
            self.sync_store_directory()?;
        }
        Ok(())
    }

    fn remove_matching_children(
        &self,
        directory_path: &Path,
        #[cfg(unix)] directory: &File,
        scan_operation: &'static str,
        remove_operation: &'static str,
        mut matches: impl FnMut(&str) -> bool,
    ) -> Result<bool, DocumentRecoveryStoreError> {
        let mut changed = false;
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt as _;
            let entries = rustix::fs::Dir::read_from(directory)
                .map_err(|error| io_error(scan_operation, io::Error::from(error)))?;
            for entry in entries {
                let entry =
                    entry.map_err(|error| io_error(scan_operation, io::Error::from(error)))?;
                let bytes = entry.file_name().to_bytes();
                if bytes == b"." || bytes == b".." {
                    continue;
                }
                let Ok(name) = std::str::from_utf8(bytes) else {
                    continue;
                };
                if matches(name) {
                    remove_regular_at(
                        directory,
                        Path::new(OsStr::from_bytes(bytes)),
                        remove_operation,
                    )?;
                    changed = true;
                }
            }
        }
        #[cfg(windows)]
        for entry in
            fs::read_dir(directory_path).map_err(|error| io_error(scan_operation, error))?
        {
            let entry = entry.map_err(|error| io_error(scan_operation, error))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if matches(&name) {
                validate_regular_file(&entry.path(), remove_operation)?;
                fs::remove_file(entry.path()).map_err(|error| io_error(remove_operation, error))?;
                changed = true;
            }
        }
        #[cfg(unix)]
        let _ = directory_path;
        Ok(changed)
    }

    fn head_path(&self, id: RecoveryDocumentId) -> PathBuf {
        self.heads.join(format!("{}.json", id.to_hex()))
    }

    fn object_path(&self, hash: [u8; 32]) -> PathBuf {
        self.objects.join(hex_encode(&hash))
    }

    fn head_leaf(id: RecoveryDocumentId) -> PathBuf {
        PathBuf::from(format!("{}.json", id.to_hex()))
    }

    fn object_leaf(hash: [u8; 32]) -> PathBuf {
        PathBuf::from(hex_encode(&hash))
    }

    fn head_exists(&self, id: RecoveryDocumentId) -> Result<bool, DocumentRecoveryStoreError> {
        #[cfg(unix)]
        return open_optional_regular_at(
            &self.heads_directory,
            &Self::head_leaf(id),
            "inspect recovery head",
        )
        .map(|file| file.is_some());
        #[cfg(windows)]
        match fs::symlink_metadata(self.head_path(id)) {
            Ok(metadata) => {
                validate_regular_metadata(&metadata)?;
                Ok(true)
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(io_error("inspect recovery head", error)),
        }
    }

    fn open_optional_object(
        &self,
        hash: [u8; 32],
        operation: &'static str,
    ) -> Result<Option<File>, DocumentRecoveryStoreError> {
        #[cfg(unix)]
        return open_optional_regular_at(
            &self.objects_directory,
            &Self::object_leaf(hash),
            operation,
        );
        #[cfg(windows)]
        match fs::symlink_metadata(self.object_path(hash)) {
            Ok(_) => open_regular_file(&self.object_path(hash), operation).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error(operation, error)),
        }
    }

    fn open_object(
        &self,
        hash: [u8; 32],
        operation: &'static str,
    ) -> Result<File, DocumentRecoveryStoreError> {
        #[cfg(unix)]
        return open_regular_at(&self.objects_directory, &Self::object_leaf(hash), operation);
        #[cfg(windows)]
        open_regular_file(&self.object_path(hash), operation)
    }

    fn open_head(
        &self,
        id: RecoveryDocumentId,
        operation: &'static str,
    ) -> Result<File, DocumentRecoveryStoreError> {
        #[cfg(unix)]
        return open_regular_at(&self.heads_directory, &Self::head_leaf(id), operation);
        #[cfg(windows)]
        open_regular_file(&self.head_path(id), operation)
    }

    fn open_optional_index(&self) -> Result<Option<File>, DocumentRecoveryStoreError> {
        #[cfg(unix)]
        return open_optional_regular_at(
            &self.store_directory,
            Path::new(INDEX_NAME),
            "inspect recovery index",
        );
        #[cfg(windows)]
        match fs::symlink_metadata(&self.index) {
            Ok(_) => open_regular_file(&self.index, "inspect recovery index").map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_error("inspect recovery index", error)),
        }
    }

    fn remove_head_child(&self, id: RecoveryDocumentId) -> Result<(), DocumentRecoveryStoreError> {
        #[cfg(unix)]
        return remove_regular_at(
            &self.heads_directory,
            &Self::head_leaf(id),
            "remove recovery head",
        );
        #[cfg(windows)]
        match fs::remove_file(self.head_path(id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("remove recovery head", error)),
        }
    }

    fn remove_object_child(&self, hash: [u8; 32]) -> Result<(), DocumentRecoveryStoreError> {
        #[cfg(unix)]
        return remove_regular_at(
            &self.objects_directory,
            &Self::object_leaf(hash),
            "remove recovery object",
        );
        #[cfg(windows)]
        match fs::remove_file(self.object_path(hash)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(io_error("remove recovery object", error)),
        }
    }

    fn create_store_temporary(
        &self,
        label: &str,
    ) -> Result<RecoveryTemporary, DocumentRecoveryStoreError> {
        create_temporary(
            &self.store,
            #[cfg(unix)]
            &self.store_directory,
            label,
        )
    }

    fn create_heads_temporary(
        &self,
        label: &str,
    ) -> Result<RecoveryTemporary, DocumentRecoveryStoreError> {
        create_temporary(
            &self.heads,
            #[cfg(unix)]
            &self.heads_directory,
            label,
        )
    }

    fn create_objects_temporary(
        &self,
        label: &str,
    ) -> Result<RecoveryTemporary, DocumentRecoveryStoreError> {
        create_temporary(
            &self.objects,
            #[cfg(unix)]
            &self.objects_directory,
            label,
        )
    }

    fn create_staging_temporary(
        &self,
        label: &str,
    ) -> Result<RecoveryTemporary, DocumentRecoveryStoreError> {
        create_temporary(
            &self.staging,
            #[cfg(unix)]
            &self.staging_directory,
            label,
        )
    }

    fn sync_store_directory(&self) -> Result<(), DocumentRecoveryStoreError> {
        sync_directory(
            &self.store,
            #[cfg(unix)]
            &self.store_directory,
        )
    }

    fn sync_heads_directory(&self) -> Result<(), DocumentRecoveryStoreError> {
        sync_directory(
            &self.heads,
            #[cfg(unix)]
            &self.heads_directory,
        )
    }

    fn sync_objects_directory(&self) -> Result<(), DocumentRecoveryStoreError> {
        sync_directory(
            &self.objects,
            #[cfg(unix)]
            &self.objects_directory,
        )
    }

    fn sync_staging_directory(&self) -> Result<(), DocumentRecoveryStoreError> {
        sync_directory(
            &self.staging,
            #[cfg(unix)]
            &self.staging_directory,
        )
    }
}

struct StoreLease {
    _file: File,
    _process_guard: MutexGuard<'static, ()>,
}

fn validate_publication(
    publication: &RecoveryPublication<'_>,
) -> Result<(), DocumentRecoveryStoreError> {
    if validate_source_path(publication.source_path).is_err() {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "source path is not an absolute native path",
        ));
    }
    if publication.base_pdf.len() as u64 > MAX_BASE_PDF_BYTES {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "base PDF is too large",
        ));
    }
    if publication.timeline.len() as u64 > MAX_TIMELINE_BYTES {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "timeline is too large",
        ));
    }
    if publication.saved_revision > publication.current_revision {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "saved revision exceeds current revision",
        ));
    }
    if publication.source_sha256 != sha256(publication.base_pdf) {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "source digest does not match the exact base PDF",
        ));
    }
    match (publication.source_kind, publication.requires_save_as) {
        (RecoverySourceKind::Opened, false) | (RecoverySourceKind::Generated, true) => {}
        _ => {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "source kind and Save-As requirement are inconsistent",
            ));
        }
    }
    Ok(())
}

fn validate_stored_publication(
    stored_base: &StoredBasePdf,
    publication: &StagedRecoveryPublication<'_>,
) -> Result<(), DocumentRecoveryStoreError> {
    if stored_base.byte_len > MAX_BASE_PDF_BYTES {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "base PDF is too large",
        ));
    }
    validate_streaming_publication(publication)
}

fn validate_copy_recovery_rebind(
    old_head: &RecoveryHead,
    expected: &RecoveryAuthority,
    publication: &StagedRecoveryPublication<'_>,
) -> Result<(), DocumentRecoveryStoreError> {
    if publication.source_kind != RecoverySourceKind::Generated
        || !publication.requires_save_as
        || publication.current_revision != expected.current_revision
        || publication.saved_revision != old_head.saved_revision
        || sha256(publication.timeline) != expected.timeline_sha256
    {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "copy recovery rebind changed recovery history",
        ));
    }
    Ok(())
}

fn validate_streaming_publication(
    publication: &StagedRecoveryPublication<'_>,
) -> Result<(), DocumentRecoveryStoreError> {
    if validate_source_path(publication.source_path).is_err() {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "source path is not an absolute native path",
        ));
    }
    if publication.timeline.len() as u64 > MAX_TIMELINE_BYTES {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "timeline is too large",
        ));
    }
    if publication.saved_revision > publication.current_revision {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "saved revision exceeds current revision",
        ));
    }
    match (publication.source_kind, publication.requires_save_as) {
        (RecoverySourceKind::Opened, false) | (RecoverySourceKind::Generated, true) => Ok(()),
        _ => Err(DocumentRecoveryStoreError::InvalidInput(
            "source kind and Save-As requirement are inconsistent",
        )),
    }
}

#[cfg(unix)]
fn validate_source_path(path: &Path) -> Result<(), Corruption> {
    use std::os::unix::ffi::OsStrExt as _;
    let bytes = path.as_os_str().as_bytes();
    if !path.is_absolute() || bytes.is_empty() || bytes.len() > MAX_PATH_UNITS || bytes.contains(&0)
    {
        return Err(Corruption::InvalidPath);
    }
    Ok(())
}

#[cfg(windows)]
fn validate_source_path(path: &Path) -> Result<(), Corruption> {
    use std::os::windows::ffi::OsStrExt as _;
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if !path.is_absolute() || units.is_empty() || units.len() > MAX_PATH_UNITS || units.contains(&0)
    {
        return Err(Corruption::InvalidPath);
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<(), DocumentRecoveryStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => validate_private_directory(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt as _;
                builder.mode(0o700);
            }
            match builder.create(path) {
                Ok(()) => {
                    sync_directory_path(path.parent().ok_or(
                        DocumentRecoveryStoreError::InvalidInput("directory has no parent"),
                    )?)?;
                    validate_private_directory(path)
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    validate_private_directory(path)
                }
                Err(error) => Err(io_error("create recovery directory", error)),
            }
        }
        Err(error) => Err(io_error("inspect recovery directory", error)),
    }
}

fn validate_private_directory(path: &Path) -> Result<(), DocumentRecoveryStoreError> {
    validate_directory(path, "inspect recovery directory")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::symlink_metadata(path)
            .map_err(|error| io_error("inspect recovery directory", error))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidFilesystemEntry,
            ));
        }
    }
    Ok(())
}

fn validate_directory(
    path: &Path,
    operation: &'static str,
) -> Result<(), DocumentRecoveryStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(operation, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(DocumentRecoveryStoreError::Corrupt(
            Corruption::InvalidFilesystemEntry,
        ));
    }
    Ok(())
}

fn validate_regular_file(
    path: &Path,
    operation: &'static str,
) -> Result<fs::Metadata, DocumentRecoveryStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| io_error(operation, error))?;
    validate_regular_metadata(&metadata)?;
    Ok(metadata)
}

fn open_regular_file(
    path: &Path,
    operation: &'static str,
) -> Result<File, DocumentRecoveryStoreError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = options
        .open(path)
        .map_err(|error| io_error(operation, error))?;
    validate_regular_metadata(
        &file
            .metadata()
            .map_err(|error| io_error(operation, error))?,
    )?;
    Ok(file)
}

fn validate_regular_metadata(metadata: &fs::Metadata) -> Result<(), DocumentRecoveryStoreError> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DocumentRecoveryStoreError::Corrupt(
            Corruption::InvalidFilesystemEntry,
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidFilesystemEntry,
            ));
        }
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.nlink() != 1 {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidFilesystemEntry,
            ));
        }
    }
    Ok(())
}

fn open_source_file(path: &Path) -> Result<File, DocumentRecoveryStoreError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options
        .open(path)
        .map_err(|error| io_error("open base PDF source", error))
}

fn validate_source_file_metadata(
    metadata: &fs::Metadata,
) -> Result<(), DocumentRecoveryStoreError> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(DocumentRecoveryStoreError::InvalidInput(
            "base PDF source is not a regular file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.nlink() != 1 {
            return Err(DocumentRecoveryStoreError::InvalidInput(
                "base PDF source ownership or link count is unsafe",
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(windows)]
fn same_file_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
    left.len() == right.len()
}

#[derive(Debug)]
struct RecoveryTemporary {
    path: PathBuf,
    leaf: OsString,
    file: Option<File>,
    #[cfg(unix)]
    directory: File,
}

impl RecoveryTemporary {
    fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("temporary recovery file is open")
    }

    fn close(&mut self) {
        drop(self.file.take());
    }

    fn publish(
        &mut self,
        destination_directory: &Path,
        #[cfg(unix)] destination_directory_fd: &File,
        destination_leaf: &Path,
    ) -> Result<(), DocumentRecoveryStoreError> {
        self.close();
        #[cfg(unix)]
        rustix::fs::renameat(
            &self.directory,
            self.leaf.as_os_str(),
            destination_directory_fd,
            destination_leaf,
        )
        .map_err(|error| io_error("publish recovery file", io::Error::from(error)))?;
        #[cfg(windows)]
        publish_replace(&self.path, &destination_directory.join(destination_leaf))?;
        self.path.clear();
        self.leaf.clear();
        Ok(())
    }

    fn remove(&mut self) -> Result<(), DocumentRecoveryStoreError> {
        self.close();
        if self.path.as_os_str().is_empty() {
            return Ok(());
        }
        #[cfg(unix)]
        let result = rustix::fs::unlinkat(
            &self.directory,
            self.leaf.as_os_str(),
            rustix::fs::AtFlags::empty(),
        )
        .map_err(io::Error::from);
        #[cfg(windows)]
        let result = fs::remove_file(&self.path);
        match result {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("remove temporary recovery file", error)),
        }
        self.path.clear();
        self.leaf.clear();
        Ok(())
    }
}

impl Drop for RecoveryTemporary {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

#[cfg(unix)]
fn create_temporary(
    directory: &Path,
    directory_fd: &File,
    label: &str,
) -> Result<RecoveryTemporary, DocumentRecoveryStoreError> {
    for _ in 0..128 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let leaf = OsString::from(format!(".{label}.{}.{}.tmp", std::process::id(), sequence));
        match rustix::fs::openat(
            directory_fd,
            leaf.as_os_str(),
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        ) {
            Ok(file) => {
                let file = File::from(file);
                validate_regular_metadata(
                    &file
                        .metadata()
                        .map_err(|error| io_error("inspect temporary recovery file", error))?,
                )?;
                return Ok(RecoveryTemporary {
                    path: directory.join(&leaf),
                    leaf,
                    file: Some(file),
                    directory: directory_fd
                        .try_clone()
                        .map_err(|error| io_error("retain recovery directory", error))?,
                });
            }
            Err(error)
                if io::Error::from_raw_os_error(error.raw_os_error()).kind()
                    == io::ErrorKind::AlreadyExists =>
            {
                continue;
            }
            Err(error) => {
                return Err(io_error(
                    "create temporary recovery file",
                    io::Error::from(error),
                ));
            }
        }
    }
    Err(DocumentRecoveryStoreError::Io {
        operation: "create temporary recovery file",
        kind: io::ErrorKind::AlreadyExists,
    })
}

#[cfg(windows)]
fn create_temporary(
    directory: &Path,
    label: &str,
) -> Result<RecoveryTemporary, DocumentRecoveryStoreError> {
    for _ in 0..128 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let leaf = OsString::from(format!(".{label}.{}.{}.tmp", std::process::id(), sequence));
        let path = directory.join(&leaf);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        match options.open(&path) {
            Ok(file) => {
                return Ok(RecoveryTemporary {
                    path,
                    leaf,
                    file: Some(file),
                });
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(io_error("create temporary recovery file", error)),
        }
    }
    Err(DocumentRecoveryStoreError::Io {
        operation: "create temporary recovery file",
        kind: io::ErrorKind::AlreadyExists,
    })
}

#[cfg(unix)]
fn open_retained_directory(
    path: &Path,
    require_private: bool,
) -> Result<File, DocumentRecoveryStoreError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let directory = options
        .open(path)
        .map_err(|error| io_error("open recovery directory", error))?;
    let metadata = directory
        .metadata()
        .map_err(|error| io_error("inspect recovery directory", error))?;
    if !metadata.is_dir()
        || (require_private && metadata.permissions().mode() & 0o077 != 0)
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(DocumentRecoveryStoreError::Corrupt(
            Corruption::InvalidFilesystemEntry,
        ));
    }
    Ok(directory)
}

#[cfg(unix)]
fn open_or_create_private_directory_at(
    parent: &File,
    leaf: &str,
    display_path: &Path,
) -> Result<File, DocumentRecoveryStoreError> {
    let directory = match rustix::fs::openat(
        parent,
        leaf,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::DIRECTORY
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Ok(directory) => File::from(directory),
        Err(error)
            if io::Error::from_raw_os_error(error.raw_os_error()).kind()
                == io::ErrorKind::NotFound =>
        {
            match rustix::fs::mkdirat(
                parent,
                leaf,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR | rustix::fs::Mode::XUSR,
            ) {
                Ok(()) => parent
                    .sync_all()
                    .map_err(|error| io_error("sync recovery directory", error))?,
                Err(error)
                    if io::Error::from_raw_os_error(error.raw_os_error()).kind()
                        == io::ErrorKind::AlreadyExists => {}
                Err(error) => {
                    return Err(io_error(
                        "create recovery directory",
                        io::Error::from(error),
                    ));
                }
            }
            File::from(
                rustix::fs::openat(
                    parent,
                    leaf,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::DIRECTORY
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )
                .map_err(|error| io_error("open recovery directory", io::Error::from(error)))?,
            )
        }
        Err(error) if matches!(error.raw_os_error(), libc::ELOOP | libc::ENOTDIR) => {
            return Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidFilesystemEntry,
            ));
        }
        Err(error) => {
            return Err(io_error("open recovery directory", io::Error::from(error)));
        }
    };
    let metadata = directory
        .metadata()
        .map_err(|error| io_error("inspect recovery directory", error))?;
    if !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(DocumentRecoveryStoreError::Corrupt(
            Corruption::InvalidFilesystemEntry,
        ));
    }
    let _ = display_path;
    Ok(directory)
}

#[cfg(unix)]
fn open_regular_at(
    directory: &File,
    leaf: &Path,
    operation: &'static str,
) -> Result<File, DocumentRecoveryStoreError> {
    let file = rustix::fs::openat(
        directory,
        leaf,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )
    .map(File::from)
    .map_err(|error| {
        if matches!(error.raw_os_error(), libc::ELOOP | libc::ENOTDIR) {
            DocumentRecoveryStoreError::Corrupt(Corruption::InvalidFilesystemEntry)
        } else {
            io_error(operation, io::Error::from(error))
        }
    })?;
    validate_regular_metadata(
        &file
            .metadata()
            .map_err(|error| io_error(operation, error))?,
    )?;
    Ok(file)
}

#[cfg(unix)]
fn open_optional_regular_at(
    directory: &File,
    leaf: &Path,
    operation: &'static str,
) -> Result<Option<File>, DocumentRecoveryStoreError> {
    match rustix::fs::openat(
        directory,
        leaf,
        rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    ) {
        Ok(file) => {
            let file = File::from(file);
            validate_regular_metadata(
                &file
                    .metadata()
                    .map_err(|error| io_error(operation, error))?,
            )?;
            Ok(Some(file))
        }
        Err(error)
            if io::Error::from_raw_os_error(error.raw_os_error()).kind()
                == io::ErrorKind::NotFound =>
        {
            Ok(None)
        }
        Err(error) if matches!(error.raw_os_error(), libc::ELOOP | libc::ENOTDIR) => Err(
            DocumentRecoveryStoreError::Corrupt(Corruption::InvalidFilesystemEntry),
        ),
        Err(error) => Err(io_error(operation, io::Error::from(error))),
    }
}

#[cfg(unix)]
fn remove_regular_at(
    directory: &File,
    leaf: &Path,
    operation: &'static str,
) -> Result<(), DocumentRecoveryStoreError> {
    let Some(file) = open_optional_regular_at(directory, leaf, operation)? else {
        return Ok(());
    };
    let opened = file
        .metadata()
        .map_err(|error| io_error(operation, error))?;
    let current = rustix::fs::statat(directory, leaf, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
        .map_err(|error| io_error(operation, io::Error::from(error)))?;
    if opened.dev() != current.st_dev as u64 || opened.ino() != current.st_ino as u64 {
        return Err(DocumentRecoveryStoreError::Corrupt(
            Corruption::InvalidFilesystemEntry,
        ));
    }
    rustix::fs::unlinkat(directory, leaf, rustix::fs::AtFlags::empty())
        .map_err(|error| io_error(operation, io::Error::from(error)))
}

fn read_bounded_file(mut file: File, limit: u64) -> Result<Vec<u8>, DocumentRecoveryStoreError> {
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read recovery file", error))?;
    if bytes.len() as u64 > limit {
        return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
    }
    Ok(bytes)
}

fn hash_open_file_bounded(
    mut file: File,
    limit: u64,
) -> Result<([u8; 32], u64), DocumentRecoveryStoreError> {
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| io_error("read recovery object", error))?;
        if count == 0 {
            break;
        }
        byte_len = byte_len
            .checked_add(count as u64)
            .ok_or(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize))?;
        if byte_len > limit {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
        }
        hasher.update(&buffer[..count]);
    }
    Ok((hasher.finalize().into(), byte_len))
}

#[cfg(unix)]
fn open_private_lease_at(
    directory: &File,
    leaf: &str,
    operation: &'static str,
) -> Result<File, DocumentRecoveryStoreError> {
    let create = rustix::fs::openat(
        directory,
        leaf,
        rustix::fs::OFlags::RDWR
            | rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::EXCL
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    );
    let file = match create {
        Ok(file) => File::from(file),
        Err(error)
            if io::Error::from_raw_os_error(error.raw_os_error()).kind()
                == io::ErrorKind::AlreadyExists =>
        {
            File::from(
                rustix::fs::openat(
                    directory,
                    leaf,
                    rustix::fs::OFlags::RDWR
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )
                .map_err(|error| io_error(operation, io::Error::from(error)))?,
            )
        }
        Err(error) => return Err(io_error(operation, io::Error::from(error))),
    };
    validate_regular_metadata(
        &file
            .metadata()
            .map_err(|error| io_error(operation, error))?,
    )?;
    Ok(file)
}

#[cfg(windows)]
fn open_private_lease(
    path: &Path,
    operation: &'static str,
) -> Result<File, DocumentRecoveryStoreError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    let file = options
        .open(path)
        .map_err(|error| io_error(operation, error))?;
    validate_regular_metadata(
        &file
            .metadata()
            .map_err(|error| io_error(operation, error))?,
    )?;
    Ok(file)
}

fn write_and_sync(file: &mut File, bytes: &[u8]) -> Result<(), DocumentRecoveryStoreError> {
    file.write_all(bytes)
        .map_err(|error| io_error("write recovery file", error))?;
    file.sync_all()
        .map_err(|error| io_error("sync recovery file", error))
}

fn hash_file_bounded(
    path: &Path,
    limit: u64,
) -> Result<([u8; 32], u64), DocumentRecoveryStoreError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options
        .open(path)
        .map_err(|error| io_error("open recovery object", error))?;
    validate_regular_metadata(
        &file
            .metadata()
            .map_err(|error| io_error("inspect recovery object", error))?,
    )?;
    let mut hasher = Sha256::new();
    let mut byte_len = 0_u64;
    let mut buffer = [0_u8; STREAM_BUFFER_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| io_error("read recovery object", error))?;
        if count == 0 {
            break;
        }
        byte_len = byte_len
            .checked_add(count as u64)
            .ok_or(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize))?;
        if byte_len > limit {
            return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
        }
        hasher.update(&buffer[..count]);
    }
    Ok((hasher.finalize().into(), byte_len))
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, DocumentRecoveryStoreError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options
        .open(path)
        .map_err(|error| io_error("open recovery file", error))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| io_error("read recovery file", error))?;
    if bytes.len() as u64 > limit {
        return Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize));
    }
    Ok(bytes)
}

fn sync_directory(
    path: &Path,
    #[cfg(unix)] directory: &File,
) -> Result<(), DocumentRecoveryStoreError> {
    #[cfg(unix)]
    directory
        .sync_all()
        .map_err(|error| io_error("sync recovery directory", error))?;
    #[cfg(windows)]
    let _ = path;
    Ok(())
}

fn sync_directory_path(path: &Path) -> Result<(), DocumentRecoveryStoreError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync recovery directory", error))?;
    #[cfg(windows)]
    let _ = path;
    Ok(())
}

#[cfg(windows)]
fn publish_replace(source: &Path, destination: &Path) -> Result<(), DocumentRecoveryStoreError> {
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
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io_error(
            "publish recovery file",
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

fn io_error(operation: &'static str, error: io::Error) -> DocumentRecoveryStoreError {
    DocumentRecoveryStoreError::Io {
        operation,
        kind: error.kind(),
    }
}

fn publication_sync_error(
    artifact: PublishedArtifact,
    document_id: RecoveryDocumentId,
    error: DocumentRecoveryStoreError,
) -> DocumentRecoveryStoreError {
    match error {
        DocumentRecoveryStoreError::Io { kind, .. } => {
            DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                artifact,
                document_id,
                kind,
            }
        }
        other => other,
    }
}

fn object_sync_error(
    object_sha256: [u8; 32],
    directory: RecoveryObjectDirectory,
    kind: io::ErrorKind,
) -> DocumentRecoveryStoreError {
    DocumentRecoveryStoreError::PublishedObjectButDirectorySyncFailed {
        object_sha256,
        directory,
        kind,
    }
}

fn is_owned_temporary_name(name: &str, label: &str) -> bool {
    let Some(remainder) = name.strip_prefix(&format!(".{label}.")) else {
        return false;
    };
    let Some(remainder) = remainder.strip_suffix(".tmp") else {
        return false;
    };
    let mut components = remainder.split('.');
    matches!(
        (components.next(), components.next(), components.next()),
        (Some(pid), Some(sequence), None)
            if !pid.is_empty()
                && !sequence.is_empty()
                && pid.bytes().all(|byte| byte.is_ascii_digit())
                && sequence.bytes().all(|byte| byte.is_ascii_digit())
    )
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn parse_hash(value: &str) -> Result<[u8; 32], DocumentRecoveryStoreError> {
    decode_fixed_hex(value).ok_or(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHash))
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

#[cfg(unix)]
fn decode_path(encoding: &str, encoded: &str) -> Result<PathBuf, DocumentRecoveryStoreError> {
    use std::os::unix::ffi::OsStringExt as _;
    if encoding != "unix-bytes" {
        return Err(DocumentRecoveryStoreError::Corrupt(
            Corruption::ForeignPathEncoding,
        ));
    }
    let bytes =
        hex_decode(encoded).ok_or(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidPath))?;
    Ok(PathBuf::from(OsString::from_vec(bytes)))
}

#[cfg(windows)]
fn decode_path(encoding: &str, encoded: &str) -> Result<PathBuf, DocumentRecoveryStoreError> {
    use std::os::windows::ffi::OsStringExt as _;
    if encoding != "windows-utf16le" {
        return Err(DocumentRecoveryStoreError::Corrupt(
            Corruption::ForeignPathEncoding,
        ));
    }
    let bytes =
        hex_decode(encoded).ok_or(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidPath))?;
    if !bytes.len().is_multiple_of(2) {
        return Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidPath));
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
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

fn hex_decode(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| Some((hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?))
        .collect()
}

fn decode_fixed_hex<const N: usize>(value: &str) -> Option<[u8; N]> {
    let bytes = hex_decode(value)?;
    bytes.try_into().ok()
}

fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

struct TemporaryGuard(PathBuf);

impl TemporaryGuard {
    fn new(path: PathBuf) -> Self {
        Self(path)
    }
    fn disarm(&mut self) {
        self.0.clear();
    }
}

impl Drop for TemporaryGuard {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    static TEST_ID: AtomicU64 = AtomicU64::new(1);

    struct TempRoot(PathBuf);
    impl TempRoot {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "bp-recovery-store-{}-{}",
                std::process::id(),
                TEST_ID.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn publication<'a>(
        path: &'a Path,
        base: &'a [u8],
        timeline: &'a [u8],
        revision: u64,
    ) -> RecoveryPublication<'a> {
        RecoveryPublication {
            source_path: path,
            source_kind: RecoverySourceKind::Opened,
            source_sha256: sha256(base),
            base_pdf: base,
            timeline,
            current_revision: revision,
            saved_revision: 0,
            requires_save_as: false,
        }
    }

    fn staged_publication<'a>(
        path: &'a Path,
        timeline: &'a [u8],
        revision: u64,
    ) -> StagedRecoveryPublication<'a> {
        StagedRecoveryPublication {
            source_path: path,
            source_kind: RecoverySourceKind::Opened,
            timeline,
            current_revision: revision,
            saved_revision: 0,
            requires_save_as: false,
        }
    }

    #[test]
    fn streamed_base_round_trip_spans_many_fixed_buffers() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("large-drawing.pdf");
        let mut bytes = vec![0_u8; STREAM_BUFFER_BYTES * 129 + 17];
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = (index.wrapping_mul(31) % 251) as u8;
        }
        fs::write(&source, &bytes).unwrap();
        let expected_hash = sha256(&bytes);

        let authority = store
            .stage_and_publish_new(expected_hash, &staged_publication(&source, b"timeline", 9))
            .unwrap();
        let recovered = store.load(authority.document_id()).unwrap().unwrap();
        assert_eq!(recovered.base_pdf, bytes);
        assert_eq!(recovered.source_sha256, expected_hash);
        assert_eq!(recovered.current_revision, 9);
    }

    #[test]
    fn wrong_stream_digest_leaves_no_object_or_authoritative_recovery() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"actual source").unwrap();
        assert!(matches!(
            store.stage_and_publish_new(
                sha256(b"expected source"),
                &staged_publication(&source, b"timeline", 1)
            ),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
        assert!(store.active_document_ids().unwrap().is_empty());
        assert_eq!(fs::read_dir(&store.objects).unwrap().count(), 0);
    }

    #[test]
    fn full_store_is_rejected_before_source_open_or_streaming() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let active = (0..MAX_ACTIVE_DOCUMENTS)
            .map(|_| RecoveryDocumentId::generate().unwrap().to_hex())
            .collect();
        fs::write(
            &store.index,
            serde_json::to_vec(&RecoveryIndex {
                version: INDEX_VERSION,
                active,
            })
            .unwrap(),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&store.index, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let missing_source = root.0.join("does-not-exist.pdf");
        assert!(matches!(
            store.stage_and_publish_new(
                sha256(b"base"),
                &staged_publication(&missing_source, b"timeline", 1)
            ),
            Err(DocumentRecoveryStoreError::InvalidInput(
                "too many active recovery documents"
            ))
        ));
        assert_eq!(fs::read_dir(&store.staging).unwrap().count(), 0);
    }

    #[test]
    fn streaming_does_not_hold_authority_for_this_or_another_root() {
        use std::{
            sync::{Arc, mpsc},
            thread,
            time::Duration,
        };

        let root = Arc::new(TempRoot::new());
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        let bytes = vec![b'x'; STREAM_BUFFER_BYTES * 3];
        fs::write(&source, &bytes).unwrap();
        let expected = sha256(&bytes);
        let (started_tx, started_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        let stream = thread::spawn(move || {
            let mut first = true;
            let prepared = store
                .prepare_base_pdf(&source, expected, MAX_BASE_PDF_BYTES, |_| {
                    if first {
                        first = false;
                        started_tx.send(()).unwrap();
                        resume_rx.recv().unwrap();
                    }
                })
                .unwrap();
            drop(prepared);
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();

        let same_root = Arc::clone(&root);
        let (same_tx, same_rx) = mpsc::channel();
        thread::spawn(move || {
            same_tx
                .send(DocumentRecoveryStore::open(&same_root.0).is_ok())
                .unwrap();
        });
        assert!(same_rx.recv_timeout(Duration::from_secs(2)).unwrap());

        let other_root = TempRoot::new();
        let (other_tx, other_rx) = mpsc::channel();
        let other_path = other_root.0.clone();
        thread::spawn(move || {
            let result = DocumentRecoveryStore::open(other_path)
                .and_then(|store| store.active_document_ids());
            other_tx.send(result.is_ok()).unwrap();
        });
        assert!(other_rx.recv_timeout(Duration::from_secs(2)).unwrap());

        let second_source = root.0.join("second.pdf");
        fs::write(&second_source, b"second").unwrap();
        let same_store = DocumentRecoveryStore::open(&root.0).unwrap();
        let (second_tx, second_rx) = mpsc::channel();
        thread::spawn(move || {
            let result = same_store.prepare_base_pdf(
                &second_source,
                sha256(b"second"),
                MAX_BASE_PDF_BYTES,
                |_| {},
            );
            second_tx.send(result.is_ok()).unwrap();
        });
        assert!(second_rx.recv_timeout(Duration::from_millis(100)).is_err());

        resume_tx.send(()).unwrap();
        stream.join().unwrap();
        assert!(second_rx.recv_timeout(Duration::from_secs(2)).unwrap());
    }

    #[test]
    fn object_directory_sync_ambiguity_is_explicit_and_retryable() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        let base_hash = sha256(b"base");
        let timeline_hash = sha256(b"timeline");

        let base_fault = store.publish_new_with_fault(
            &publication(&source, b"base", b"timeline", 1),
            PublicationFault::AfterBaseObjectRenameBeforeSync,
        );
        assert!(matches!(
            base_fault,
            Err(
                DocumentRecoveryStoreError::PublishedObjectButDirectorySyncFailed {
                    object_sha256,
                    directory: RecoveryObjectDirectory::Objects,
                    kind: io::ErrorKind::Interrupted,
                }
            ) if object_sha256 == base_hash
        ));
        assert!(store.active_document_ids().unwrap().is_empty());
        let base_retry = store
            .publish_new(&publication(&source, b"base", b"timeline", 1))
            .unwrap();
        assert!(store.load(base_retry).unwrap().is_some());
        store.clear(base_retry).unwrap();

        let timeline_fault = store.publish_new_with_fault(
            &publication(&source, b"base", b"timeline", 2),
            PublicationFault::AfterTimelineObjectRenameBeforeSync,
        );
        assert!(matches!(
            timeline_fault,
            Err(
                DocumentRecoveryStoreError::PublishedObjectButDirectorySyncFailed {
                    object_sha256,
                    directory: RecoveryObjectDirectory::Objects,
                    kind: io::ErrorKind::Interrupted,
                }
            ) if object_sha256 == timeline_hash
        ));
        assert!(store.active_document_ids().unwrap().is_empty());
        let timeline_retry = store
            .publish_new(&publication(&source, b"base", b"timeline", 2))
            .unwrap();
        assert!(store.load(timeline_retry).unwrap().is_some());
        store.clear(timeline_retry).unwrap();

        fs::write(&source, b"base").unwrap();
        let staging_fault = store.stage_and_publish_new_with_fault(
            base_hash,
            &staged_publication(&source, b"timeline", 3),
            PublicationFault::AfterBaseObjectObjectsSyncBeforeStagingSync,
        );
        assert!(matches!(
            staging_fault,
            Err(
                DocumentRecoveryStoreError::PublishedObjectButDirectorySyncFailed {
                    object_sha256,
                    directory: RecoveryObjectDirectory::Staging,
                    kind: io::ErrorKind::Interrupted,
                }
            ) if object_sha256 == base_hash
        ));
        assert!(store.active_document_ids().unwrap().is_empty());
        let stale_stage = store
            .staging
            .join(format!(".stage.{}.888888.tmp", std::process::id()));
        fs::write(&stale_stage, b"stale pre-retry link").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&stale_stage, fs::Permissions::from_mode(0o600)).unwrap();
        }
        let retry = store
            .stage_and_publish_new(base_hash, &staged_publication(&source, b"timeline", 3))
            .unwrap();
        assert!(store.load(retry.document_id()).unwrap().is_some());
        assert!(stale_stage.exists());
        store.collect_orphans().unwrap();
        assert!(!stale_stage.exists());
    }

    #[test]
    fn streamed_objects_deduplicate_and_crash_staging_is_collected() {
        let root = TempRoot::new();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"shared streamed base").unwrap();
        let hash = sha256(b"shared streamed base");
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        store
            .stage_and_publish_new(hash, &staged_publication(&source, b"timeline", 1))
            .unwrap();
        store
            .stage_and_publish_new(hash, &staged_publication(&source, b"timeline", 2))
            .unwrap();
        assert_eq!(fs::read_dir(&store.objects).unwrap().count(), 2);
        let orphan = store
            .staging
            .join(format!(".stage.{}.999999.tmp", std::process::id()));
        fs::write(&orphan, b"crash orphan").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&orphan, fs::Permissions::from_mode(0o600)).unwrap();
        }
        drop(store);

        let reopened = DocumentRecoveryStore::open(&root.0).unwrap();
        assert!(!orphan.exists());
        assert_eq!(reopened.active_document_ids().unwrap().len(), 2);
    }

    #[test]
    fn atomic_stream_publish_and_replace_revalidate_existing_object() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"base").unwrap();
        let result = store.stage_and_publish_new_with_fault(
            sha256(b"base"),
            &staged_publication(&source, b"orphan", 1),
            PublicationFault::AfterHeadBeforeIndex,
        );
        assert!(matches!(
            result,
            Err(DocumentRecoveryStoreError::InjectedFault)
        ));
        fs::write(store.object_path(sha256(b"base")), b"evil").unwrap();
        assert!(matches!(
            store.stage_and_publish_new(
                sha256(b"base"),
                &staged_publication(&source, b"timeline", 1)
            ),
            Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::HashMismatch
            ))
        ));
        assert!(store.active_document_ids().unwrap().is_empty());

        fs::remove_file(store.object_path(sha256(b"base"))).unwrap();
        let first = store
            .stage_and_publish_new(sha256(b"base"), &staged_publication(&source, b"one", 1))
            .unwrap();
        let second = store
            .stage_and_replace(
                &first,
                sha256(b"base"),
                &staged_publication(&source, b"two", 2),
            )
            .unwrap();
        assert_eq!(
            store.load(first.document_id()).unwrap().unwrap().timeline,
            b"two"
        );
        let third = store
            .replace_timeline(&second, &staged_publication(&source, b"three", 3))
            .unwrap();
        assert_eq!(
            store.load(third.document_id()).unwrap().unwrap().timeline,
            b"three"
        );
    }

    #[test]
    fn save_rebase_replaces_base_and_clean_timeline_without_weakening_equal_revision_cas() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let generated = root.0.join("generated.pdf");
        let saved = root.0.join("saved.pdf");
        fs::write(&generated, b"generated base").unwrap();
        fs::write(&saved, b"saved base").unwrap();
        let initial = StagedRecoveryPublication {
            source_path: &generated,
            source_kind: RecoverySourceKind::Generated,
            timeline: b"dirty timeline",
            current_revision: 4,
            saved_revision: 0,
            requires_save_as: true,
        };
        let before = store
            .stage_and_publish_new(sha256(b"generated base"), &initial)
            .unwrap();
        let clean = StagedRecoveryPublication {
            source_path: &saved,
            source_kind: RecoverySourceKind::Opened,
            timeline: b"clean timeline",
            current_revision: 4,
            saved_revision: 4,
            requires_save_as: false,
        };
        let rebased = store
            .rebase_after_save(&before, sha256(b"saved base"), &clean)
            .unwrap();
        let recovered = store.load(rebased.document_id()).unwrap().unwrap();
        assert_eq!(
            rebased.checkpoint_sequence(),
            before.checkpoint_sequence() + 1
        );
        assert_eq!(recovered.source_path, saved);
        assert_eq!(recovered.source_kind, RecoverySourceKind::Opened);
        assert_eq!(recovered.base_pdf, b"saved base");
        assert_eq!(recovered.timeline, b"clean timeline");
        assert_eq!(recovered.current_revision, 4);
        assert_eq!(recovered.saved_revision, 4);
        assert!(!recovered.requires_save_as);

        assert!(matches!(
            store.rebase_after_save(&before, sha256(b"saved base"), &clean),
            Err(DocumentRecoveryStoreError::StaleAuthority { .. })
        ));
        assert!(matches!(
            store.replace_timeline(
                &rebased,
                &StagedRecoveryPublication {
                    timeline: b"divergent equal revision",
                    ..clean
                },
            ),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
    }

    #[test]
    fn copy_recovery_rebind_preserves_history_and_rejects_stale_authority() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let original = root.0.join("original.pdf");
        let recovered_copy = root.0.join("recovered-copy.pdf");
        fs::write(&original, b"original base").unwrap();
        fs::write(&recovered_copy, b"materialised recovered base").unwrap();
        let initial = StagedRecoveryPublication {
            source_path: &original,
            source_kind: RecoverySourceKind::Opened,
            timeline: b"complete edit timeline",
            current_revision: 7,
            saved_revision: 3,
            requires_save_as: false,
        };
        let before = store
            .stage_and_publish_new(sha256(b"original base"), &initial)
            .unwrap();
        let rebound_publication = StagedRecoveryPublication {
            source_path: &recovered_copy,
            source_kind: RecoverySourceKind::Generated,
            timeline: initial.timeline,
            current_revision: initial.current_revision,
            saved_revision: initial.saved_revision,
            requires_save_as: true,
        };

        let rebound = store
            .rebind_after_copy_recovery(
                &before,
                sha256(b"materialised recovered base"),
                &rebound_publication,
            )
            .unwrap();
        let recovered = store.load(rebound.document_id()).unwrap().unwrap();
        assert_eq!(
            rebound.checkpoint_sequence(),
            before.checkpoint_sequence() + 1
        );
        assert_eq!(rebound.current_revision(), before.current_revision());
        assert_eq!(rebound.timeline_sha256(), before.timeline_sha256());
        assert_eq!(recovered.source_path, recovered_copy);
        assert_eq!(recovered.source_kind, RecoverySourceKind::Generated);
        assert_eq!(recovered.base_pdf, b"materialised recovered base");
        assert_eq!(recovered.timeline, initial.timeline);
        assert_eq!(recovered.current_revision, initial.current_revision);
        assert_eq!(recovered.saved_revision, initial.saved_revision);
        assert!(recovered.requires_save_as);

        assert!(matches!(
            store.rebind_after_copy_recovery(
                &before,
                sha256(b"materialised recovered base"),
                &rebound_publication,
            ),
            Err(DocumentRecoveryStoreError::StaleAuthority { document_id })
                if document_id == before.document_id()
        ));
    }

    #[test]
    fn copy_recovery_rebind_head_sync_ambiguity_exposes_rebind_authority() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let original = root.0.join("original.pdf");
        let recovered_copy = root.0.join("recovered-copy.pdf");
        fs::write(&original, b"original base").unwrap();
        fs::write(&recovered_copy, b"recovered base").unwrap();
        let initial = store
            .stage_and_publish_new(
                sha256(b"original base"),
                &staged_publication(&original, b"timeline", 5),
            )
            .unwrap();
        let publication = StagedRecoveryPublication {
            source_path: &recovered_copy,
            source_kind: RecoverySourceKind::Generated,
            timeline: b"timeline",
            current_revision: 5,
            saved_revision: 0,
            requires_save_as: true,
        };

        let result = store.rebind_after_copy_recovery_with_fault(
            &initial,
            sha256(b"recovered base"),
            &publication,
            PublicationFault::AfterHeadRenameBeforeSync,
        );
        assert!(matches!(
            result,
            Err(DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                artifact: PublishedArtifact::Head,
                document_id,
                kind: io::ErrorKind::Interrupted,
            }) if document_id == initial.document_id()
        ));
        let observed = store.load(initial.document_id()).unwrap().unwrap();
        assert_eq!(observed.source_path, recovered_copy);
        assert_eq!(observed.base_pdf, b"recovered base");
        assert_eq!(observed.timeline, b"timeline");
        assert_eq!(observed.current_revision, 5);
        assert_eq!(observed.saved_revision, 0);
        assert!(observed.requires_save_as);
        assert_eq!(
            store
                .confirm_authority_durable(&observed.authority)
                .unwrap(),
            observed.authority
        );
    }

    #[test]
    fn timeline_authority_rejects_stale_equal_and_aba_checkpoints() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"base").unwrap();
        let first = store
            .stage_and_publish_new(
                sha256(b"base"),
                &staged_publication(&source, b"state-one", 1),
            )
            .unwrap();
        let second = store
            .replace_timeline(&first, &staged_publication(&source, b"state-two", 2))
            .unwrap();
        assert_eq!(second.checkpoint_sequence(), 2);

        assert!(matches!(
            store.replace_timeline(&first, &staged_publication(&source, b"stale-three", 3)),
            Err(DocumentRecoveryStoreError::StaleAuthority { .. })
        ));
        assert!(matches!(
            store.replace_timeline(
                &second,
                &staged_publication(&source, b"different-at-two", 2)
            ),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
        assert_eq!(
            store
                .replace_timeline(&second, &staged_publication(&source, b"state-two", 2))
                .unwrap(),
            second
        );

        let undone = store
            .replace_timeline(&second, &staged_publication(&source, b"undo-one", 1))
            .unwrap();
        let redone = store
            .replace_timeline(&undone, &staged_publication(&source, b"state-two", 2))
            .unwrap();
        assert_eq!(redone.timeline_sha256(), second.timeline_sha256());
        assert!(redone.checkpoint_sequence() > second.checkpoint_sequence());
        assert!(matches!(
            store.replace_timeline(&second, &staged_publication(&source, b"stale-after-aba", 3)),
            Err(DocumentRecoveryStoreError::StaleAuthority { .. })
        ));
    }

    #[test]
    fn concurrent_timeline_cas_allows_exactly_one_successor() {
        use std::{sync::Arc, sync::Barrier, thread};

        let root = TempRoot::new();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"base").unwrap();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let initial = store
            .stage_and_publish_new(sha256(b"base"), &staged_publication(&source, b"initial", 1))
            .unwrap();
        drop(store);

        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for timeline in [b"successor-a".as_slice(), b"successor-b".as_slice()] {
            let root = root.0.clone();
            let source = source.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                let store = DocumentRecoveryStore::open(root).unwrap();
                barrier.wait();
                store.replace_timeline(&initial, &staged_publication(&source, timeline, 2))
            }));
        }
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(
                    result,
                    Err(DocumentRecoveryStoreError::StaleAuthority { .. })
                ))
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[test]
    fn streamed_source_rejects_symlink_hardlink_substitution_and_oversize() {
        use std::os::unix::fs::symlink;

        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"base").unwrap();
        let symlink_path = root.0.join("symlink.pdf");
        symlink(&source, &symlink_path).unwrap();
        assert!(
            store
                .stage_and_publish_new(
                    sha256(b"base"),
                    &staged_publication(&symlink_path, b"timeline", 1),
                )
                .is_err()
        );

        let hardlink_path = root.0.join("hardlink.pdf");
        fs::hard_link(&source, &hardlink_path).unwrap();
        assert!(matches!(
            store.stage_and_publish_new(
                sha256(b"base"),
                &staged_publication(&source, b"timeline", 1)
            ),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
        fs::remove_file(hardlink_path).unwrap();

        let old_source = root.0.join("old.pdf");
        fs::rename(&source, &old_source).unwrap();
        fs::write(&source, b"substituted").unwrap();
        assert!(matches!(
            store.stage_and_publish_new(
                sha256(b"base"),
                &staged_publication(&source, b"timeline", 1)
            ),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));

        let oversized = root.0.join("oversized.pdf");
        File::create(&oversized)
            .unwrap()
            .set_len(MAX_BASE_PDF_BYTES + 1)
            .unwrap();
        assert!(matches!(
            store.stage_and_publish_new(
                sha256(b""),
                &staged_publication(&oversized, b"timeline", 1)
            ),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
        assert!(store.active_document_ids().unwrap().is_empty());
    }

    #[test]
    fn round_trip_preserves_lossless_path_and_payloads() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        #[cfg(unix)]
        let source = {
            use std::os::unix::ffi::OsStringExt as _;
            PathBuf::from(OsString::from_vec(b"/tmp/non-utf8-\xff.pdf".to_vec()))
        };
        #[cfg(windows)]
        let source = PathBuf::from(r"C:\drawing.pdf");
        let id = store
            .publish_new(&publication(&source, b"%PDF-base", b"timeline", 7))
            .unwrap();
        let recovered = store.load(id).unwrap().unwrap();
        assert_eq!(recovered.source_path, source);
        assert_eq!(recovered.base_pdf, b"%PDF-base");
        assert_eq!(recovered.timeline, b"timeline");
        assert_eq!(recovered.current_revision, 7);
    }

    #[cfg(unix)]
    #[test]
    fn retained_store_tree_survives_path_replacement_for_publish_load_and_clear() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let lexical_store = root.0.join(STORE_DIRECTORY);
        let retained_store = root.0.join("retained-store");
        fs::rename(&lexical_store, &retained_store).unwrap();
        fs::create_dir(&lexical_store).unwrap();
        fs::set_permissions(&lexical_store, fs::Permissions::from_mode(0o700)).unwrap();
        for child in [OBJECTS_DIRECTORY, HEADS_DIRECTORY, STAGING_DIRECTORY] {
            let path = lexical_store.join(child);
            fs::create_dir(&path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let canary = lexical_store.join("attacker-canary");
        fs::write(&canary, b"untouched").unwrap();

        let id = store
            .publish_new(&publication(
                &root.0.join("drawing.pdf"),
                b"base",
                b"timeline",
                1,
            ))
            .unwrap();
        assert_eq!(store.load(id).unwrap().unwrap().timeline, b"timeline");
        assert!(store.clear(id).unwrap());
        assert_eq!(fs::read(&canary).unwrap(), b"untouched");
        assert!(!lexical_store.join(INDEX_NAME).exists());
        assert!(retained_store.join(LEASE_NAME).exists());
    }

    #[cfg(unix)]
    #[test]
    fn retained_store_tree_collects_orphans_without_touching_path_replacement() {
        use std::os::unix::fs::PermissionsExt as _;

        fn write_private(path: &Path, contents: &[u8]) {
            fs::write(path, contents).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let id = store
            .publish_new(&publication(
                &root.0.join("drawing.pdf"),
                b"base",
                b"timeline",
                1,
            ))
            .unwrap();

        let orphan_head = "ffffffffffffffffffffffffffffffff.json";
        let orphan_object = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
        let orphan_stage = format!(".stage.{}.777777.tmp", std::process::id());
        let orphan_index = format!(".index.{}.777777.tmp", std::process::id());
        write_private(&store.heads.join(orphan_head), b"orphan head");
        write_private(&store.objects.join(orphan_object), b"orphan object");
        write_private(&store.staging.join(&orphan_stage), b"orphan stage");
        write_private(&store.store.join(&orphan_index), b"orphan index");

        let lexical_store = root.0.join(STORE_DIRECTORY);
        let retained_store = root.0.join("retained-store");
        fs::rename(&lexical_store, &retained_store).unwrap();
        fs::create_dir(&lexical_store).unwrap();
        fs::set_permissions(&lexical_store, fs::Permissions::from_mode(0o700)).unwrap();
        for child in [OBJECTS_DIRECTORY, HEADS_DIRECTORY, STAGING_DIRECTORY] {
            let path = lexical_store.join(child);
            fs::create_dir(&path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let decoy_canary = lexical_store.join("attacker-canary");
        write_private(&decoy_canary, b"untouched");
        for path in [
            lexical_store.join(HEADS_DIRECTORY).join(orphan_head),
            lexical_store.join(OBJECTS_DIRECTORY).join(orphan_object),
            lexical_store.join(STAGING_DIRECTORY).join(&orphan_stage),
            lexical_store.join(&orphan_index),
        ] {
            write_private(&path, b"decoy");
        }

        store.collect_orphans().unwrap();

        assert!(store.load(id).unwrap().is_some());
        assert!(
            !retained_store
                .join(HEADS_DIRECTORY)
                .join(orphan_head)
                .exists()
        );
        assert!(
            !retained_store
                .join(OBJECTS_DIRECTORY)
                .join(orphan_object)
                .exists()
        );
        assert!(
            !retained_store
                .join(STAGING_DIRECTORY)
                .join(&orphan_stage)
                .exists()
        );
        assert!(!retained_store.join(&orphan_index).exists());
        assert_eq!(fs::read(&decoy_canary).unwrap(), b"untouched");
        assert!(
            lexical_store
                .join(HEADS_DIRECTORY)
                .join(orphan_head)
                .exists()
        );
        assert!(
            lexical_store
                .join(OBJECTS_DIRECTORY)
                .join(orphan_object)
                .exists()
        );
        assert!(
            lexical_store
                .join(STAGING_DIRECTORY)
                .join(orphan_stage)
                .exists()
        );
        assert!(lexical_store.join(orphan_index).exists());
    }

    #[cfg(unix)]
    #[test]
    fn retained_staging_directory_survives_replacement_during_streaming() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("large-drawing.pdf");
        let bytes = vec![b's'; STREAM_BUFFER_BYTES * 2 + 17];
        fs::write(&source, &bytes).unwrap();
        let expected_hash = sha256(&bytes);
        let retained_staging = store.store.join("retained-staging");
        let lexical_staging = store.staging.clone();
        let decoy_canary = lexical_staging.join("attacker-canary");
        let mut replaced = false;

        let prepared = store
            .prepare_base_pdf(&source, expected_hash, MAX_BASE_PDF_BYTES, |_| {
                if replaced {
                    return;
                }
                fs::rename(&lexical_staging, &retained_staging).unwrap();
                fs::create_dir(&lexical_staging).unwrap();
                fs::set_permissions(&lexical_staging, fs::Permissions::from_mode(0o700)).unwrap();
                fs::write(&decoy_canary, b"untouched").unwrap();
                fs::set_permissions(&decoy_canary, fs::Permissions::from_mode(0o600)).unwrap();
                replaced = true;
            })
            .unwrap();

        assert!(replaced);
        assert_eq!(fs::read(&decoy_canary).unwrap(), b"untouched");
        assert_eq!(fs::read_dir(&retained_staging).unwrap().count(), 1);
        let _lease = store.acquire_lease().unwrap();
        let stored = store
            .publish_prepared_base(prepared, PublicationFault::None)
            .unwrap();

        assert_eq!(stored.sha256, expected_hash);
        assert_eq!(stored.byte_len, bytes.len() as u64);
        assert!(store.object_path(expected_hash).exists());
        assert_eq!(fs::read_dir(&retained_staging).unwrap().count(), 0);
        assert_eq!(fs::read(&decoy_canary).unwrap(), b"untouched");
    }

    #[test]
    fn identical_payloads_are_deduplicated() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        store
            .publish_new(&publication(&source, b"same", b"same", 1))
            .unwrap();
        store
            .publish_new(&publication(&source, b"same", b"same", 2))
            .unwrap();
        assert_eq!(fs::read_dir(&store.objects).unwrap().count(), 1);
    }

    #[test]
    fn orphaned_first_head_is_not_visible_before_index_update() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let result = store.publish_new_with_fault(
            &publication(&root.0.join("drawing.pdf"), b"base", b"timeline", 1),
            PublicationFault::AfterHeadBeforeIndex,
        );
        assert!(matches!(
            result,
            Err(DocumentRecoveryStoreError::InjectedFault)
        ));
        assert!(store.active_document_ids().unwrap().is_empty());
        assert_eq!(fs::read_dir(&store.heads).unwrap().count(), 1);
    }

    #[test]
    fn failed_replacement_before_rename_retains_old_head() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        let id = store
            .publish_new(&publication(&source, b"old", b"old timeline", 1))
            .unwrap();
        let result = store.replace_with_fault(
            id,
            &publication(&source, b"new", b"new timeline", 2),
            PublicationFault::BeforeHeadRename,
        );
        assert!(matches!(
            result,
            Err(DocumentRecoveryStoreError::InjectedFault)
        ));
        let recovered = store.load(id).unwrap().unwrap();
        assert_eq!(recovered.base_pdf, b"old");
        assert_eq!(recovered.timeline, b"old timeline");
    }

    #[test]
    fn strict_json_oversize_and_hash_mismatch_are_rejected() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        fs::write(&store.index, br#"{"version":1,"active":[],"extra":true}"#).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&store.index, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(matches!(
            store.active_document_ids(),
            Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidIndex
            ))
        ));
        fs::write(&store.index, vec![b'x'; MAX_INDEX_BYTES as usize + 1]).unwrap();
        assert!(matches!(
            store.active_document_ids(),
            Err(DocumentRecoveryStoreError::Corrupt(Corruption::Oversize))
        ));

        fs::remove_file(&store.index).unwrap();
        let id = store
            .publish_new(&publication(
                &root.0.join("drawing.pdf"),
                b"base",
                b"timeline",
                1,
            ))
            .unwrap();
        let valid_head = fs::read(store.head_path(id)).unwrap();
        fs::write(store.head_path(id), b"not json").unwrap();
        assert!(matches!(
            store.load(id),
            Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead))
        ));

        fs::write(store.head_path(id), valid_head).unwrap();
        let object = store.object_path(sha256(b"base"));
        fs::write(object, b"evil").unwrap();
        assert!(matches!(
            store.load(id),
            Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::HashMismatch
            ))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_and_traversal_shaped_ids_are_rejected() {
        use std::os::unix::fs::symlink;
        let root = TempRoot::new();
        let outside = TempRoot::new();
        symlink(&outside.0, root.0.join(STORE_DIRECTORY)).unwrap();
        assert!(matches!(
            DocumentRecoveryStore::open(&root.0),
            Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidFilesystemEntry
            ))
        ));
        assert!(RecoveryDocumentId::from_hex("../../outside").is_err());

        let clean_root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&clean_root.0).unwrap();
        let object = store.object_path(sha256(b"base"));
        symlink(&outside.0, &object).unwrap();
        let result = store.publish_new(&publication(
            &clean_root.0.join("drawing.pdf"),
            b"base",
            b"timeline",
            1,
        ));
        assert!(matches!(
            result,
            Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidFilesystemEntry
            ))
        ));
        fs::remove_file(&object).unwrap();
        fs::write(&object, b"base").unwrap();
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&object, fs::Permissions::from_mode(0o600)).unwrap();
        }
        fs::hard_link(&object, outside.0.join("linked-object")).unwrap();
        assert!(matches!(
            store.publish_new(&publication(
                &clean_root.0.join("drawing.pdf"),
                b"base",
                b"timeline",
                1,
            )),
            Err(DocumentRecoveryStoreError::Corrupt(
                Corruption::InvalidFilesystemEntry
            ))
        ));
    }

    #[test]
    fn clear_updates_index_before_best_effort_cleanup() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let id = store
            .publish_new(&publication(
                &root.0.join("drawing.pdf"),
                b"base",
                b"timeline",
                1,
            ))
            .unwrap();
        fs::remove_file(store.head_path(id)).unwrap();
        assert!(store.clear(id).unwrap());
        assert!(store.active_document_ids().unwrap().is_empty());
        assert!(store.load(id).unwrap().is_none());
    }

    #[test]
    fn authority_checked_clear_cannot_retire_a_newer_checkpoint() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"base").unwrap();
        let first = store
            .stage_and_publish_new(
                sha256(b"base"),
                &staged_publication(&source, b"timeline-one", 1),
            )
            .unwrap();
        let second = store
            .replace_timeline(&first, &staged_publication(&source, b"timeline-two", 2))
            .unwrap();

        assert!(matches!(
            store.clear_authority(&first),
            Err(DocumentRecoveryStoreError::StaleAuthority { .. })
        ));
        let recovered = store.load(second.document_id()).unwrap().unwrap();
        assert_eq!(recovered.authority, second);
        assert_eq!(recovered.timeline, b"timeline-two");
        assert!(store.clear_authority(&second).unwrap());
        assert!(store.load(second.document_id()).unwrap().is_none());
    }

    #[test]
    fn publication_contract_rejects_digest_and_source_kind_mismatches() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        let relative = PathBuf::from("drawing.pdf");
        assert!(matches!(
            store.publish_new(&publication(&relative, b"base", b"timeline", 1)),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
        let mut invalid = publication(&source, b"base", b"timeline", 1);
        invalid.source_sha256 = sha256(b"different");
        assert!(matches!(
            store.publish_new(&invalid),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
        let mut generated = publication(&source, b"base", b"timeline", 1);
        generated.source_kind = RecoverySourceKind::Generated;
        assert!(matches!(
            store.publish_new(&generated),
            Err(DocumentRecoveryStoreError::InvalidInput(_))
        ));
    }

    #[test]
    fn persisted_head_revalidates_source_digest_and_save_as_contract() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        let id = store
            .publish_new(&publication(&source, b"base", b"timeline", 1))
            .unwrap();
        let head_path = store.head_path(id);
        let original = fs::read(&head_path).unwrap();

        let mut mismatched_digest: RecoveryHead = serde_json::from_slice(&original).unwrap();
        mismatched_digest.source_sha256 = hex_encode(&sha256(b"different"));
        fs::write(&head_path, serde_json::to_vec(&mismatched_digest).unwrap()).unwrap();
        assert!(matches!(
            store.load(id),
            Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead))
        ));

        let mut mismatched_kind: RecoveryHead = serde_json::from_slice(&original).unwrap();
        mismatched_kind.source_kind = RecoverySourceKind::Generated;
        fs::write(&head_path, serde_json::to_vec(&mismatched_kind).unwrap()).unwrap();
        assert!(matches!(
            store.load(id),
            Err(DocumentRecoveryStoreError::Corrupt(Corruption::InvalidHead))
        ));
    }

    #[test]
    fn post_rename_sync_ambiguity_identifies_the_published_authority() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        let id = store
            .publish_new(&publication(&source, b"old", b"old timeline", 1))
            .unwrap();
        let result = store.replace_with_fault(
            id,
            &publication(&source, b"new", b"new timeline", 2),
            PublicationFault::AfterHeadRenameBeforeSync,
        );
        assert!(matches!(
            result,
            Err(
                DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                    artifact: PublishedArtifact::Head,
                    ..
                }
            )
        ));
        assert_eq!(store.load(id).unwrap().unwrap().base_pdf, b"new");

        let second = store.publish_new_with_fault(
            &publication(&source, b"second", b"second timeline", 3),
            PublicationFault::AfterIndexRenameBeforeSync,
        );
        assert!(matches!(
            second,
            Err(
                DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                    artifact: PublishedArtifact::Index,
                    ..
                }
            )
        ));
        assert_eq!(store.active_document_ids().unwrap().len(), 2);
    }

    #[test]
    fn durable_authority_confirmation_reconciles_head_and_initial_index_ambiguity() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"base").unwrap();
        let initial = store
            .stage_and_publish_new(sha256(b"base"), &staged_publication(&source, b"initial", 1))
            .unwrap();

        let head_fault = store.replace_timeline_with_fault(
            &initial,
            &staged_publication(&source, b"after-head", 2),
            PublicationFault::AfterHeadRenameBeforeSync,
        );
        assert!(matches!(
            head_fault,
            Err(
                DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                    artifact: PublishedArtifact::Head,
                    document_id,
                    kind: io::ErrorKind::Interrupted,
                }
            ) if document_id == initial.document_id()
        ));
        let observed_head = store
            .load(initial.document_id())
            .unwrap()
            .unwrap()
            .authority;
        assert_eq!(observed_head.current_revision(), 2);
        assert_eq!(
            store.confirm_authority_durable(&observed_head).unwrap(),
            observed_head
        );
        assert_eq!(
            store
                .replace_timeline(
                    &observed_head,
                    &staged_publication(&source, b"after-head", 2)
                )
                .unwrap(),
            observed_head
        );

        let second_source = root.0.join("second.pdf");
        fs::write(&second_source, b"second base").unwrap();
        let index_fault = store.stage_and_publish_new_with_fault(
            sha256(b"second base"),
            &staged_publication(&second_source, b"initial second", 1),
            PublicationFault::AfterIndexRenameBeforeSync,
        );
        let second_id = match index_fault {
            Err(DocumentRecoveryStoreError::PublishedButDirectorySyncFailed {
                artifact: PublishedArtifact::Index,
                document_id,
                kind: io::ErrorKind::Interrupted,
            }) => document_id,
            other => panic!("unexpected initial-index fault result: {other:?}"),
        };
        let observed_index = store.load(second_id).unwrap().unwrap().authority;
        assert_eq!(observed_index.current_revision(), 1);
        assert_eq!(
            store.confirm_authority_durable(&observed_index).unwrap(),
            observed_index
        );
    }

    #[test]
    fn replacement_and_open_gc_remove_only_unreferenced_store_objects() {
        let root = TempRoot::new();
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let source = root.0.join("drawing.pdf");
        let id = store
            .publish_new(&publication(&source, b"old", b"old timeline", 1))
            .unwrap();
        let old_base = store.object_path(sha256(b"old"));
        store
            .replace(id, &publication(&source, b"new", b"new timeline", 2))
            .unwrap();
        assert!(!old_base.exists());

        let result = store.publish_new_with_fault(
            &publication(&source, b"orphan", b"orphan timeline", 3),
            PublicationFault::AfterHeadBeforeIndex,
        );
        assert!(matches!(
            result,
            Err(DocumentRecoveryStoreError::InjectedFault)
        ));
        drop(store);
        let reopened = DocumentRecoveryStore::open(&root.0).unwrap();
        assert_eq!(reopened.active_document_ids().unwrap(), vec![id]);
        assert!(!reopened.object_path(sha256(b"orphan")).exists());
    }

    #[test]
    fn subprocess_publish_helper() {
        let Ok(root) = std::env::var("BP_RECOVERY_STORE_CHILD_ROOT") else {
            return;
        };
        let store = DocumentRecoveryStore::open(root).unwrap();
        let source = PathBuf::from(std::env::var("BP_RECOVERY_STORE_CHILD_SOURCE").unwrap());
        store
            .stage_and_publish_new(
                sha256(b"shared base"),
                &staged_publication(&source, b"shared timeline", 1),
            )
            .unwrap();
    }

    #[test]
    fn subprocess_staging_pause_helper() {
        use std::{thread, time::Duration};

        let Ok(root) = std::env::var("BP_RECOVERY_STAGE_CHILD_ROOT") else {
            return;
        };
        let source = PathBuf::from(std::env::var("BP_RECOVERY_STAGE_CHILD_SOURCE").unwrap());
        let ready = PathBuf::from(std::env::var("BP_RECOVERY_STAGE_CHILD_READY").unwrap());
        let resume = PathBuf::from(std::env::var("BP_RECOVERY_STAGE_CHILD_RESUME").unwrap());
        let bytes = fs::read(&source).unwrap();
        let store = DocumentRecoveryStore::open(root).unwrap();
        let mut first = true;
        let prepared = store
            .prepare_base_pdf(&source, sha256(&bytes), MAX_BASE_PDF_BYTES, |_| {
                if first {
                    first = false;
                    fs::write(&ready, b"ready").unwrap();
                    while !resume.exists() {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            })
            .unwrap();
        drop(prepared);
    }

    #[test]
    fn cross_process_staging_lease_prevents_open_gc_deletion() {
        use std::{thread, time::Duration};

        let root = TempRoot::new();
        let executable = std::env::current_exe().unwrap();
        let source = root.0.join("large.pdf");
        fs::write(&source, vec![b'x'; STREAM_BUFFER_BYTES * 3]).unwrap();
        let ready = root.0.join("ready");
        let resume = root.0.join("resume");
        let mut child = Command::new(&executable)
            .args([
                "--exact",
                "document_recovery_store::tests::subprocess_staging_pause_helper",
            ])
            .env("BP_RECOVERY_STAGE_CHILD_ROOT", &root.0)
            .env("BP_RECOVERY_STAGE_CHILD_SOURCE", &source)
            .env("BP_RECOVERY_STAGE_CHILD_READY", &ready)
            .env("BP_RECOVERY_STAGE_CHILD_RESUME", &resume)
            .spawn()
            .unwrap();
        for _ in 0..500 {
            if ready.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(ready.exists(), "staging child did not reach its pause");

        let opened = DocumentRecoveryStore::open(&root.0).unwrap();
        assert_eq!(fs::read_dir(&opened.staging).unwrap().count(), 1);
        fs::write(&resume, b"resume").unwrap();
        assert!(child.wait().unwrap().success());
        opened.collect_orphans().unwrap();
        assert_eq!(fs::read_dir(&opened.staging).unwrap().count(), 0);
    }

    #[test]
    fn killed_staging_child_releases_kernel_lease_and_exact_temp_is_collected() {
        use std::{thread, time::Duration};

        let root = TempRoot::new();
        let executable = std::env::current_exe().unwrap();
        let source = root.0.join("large-kill.pdf");
        fs::write(&source, vec![b'k'; STREAM_BUFFER_BYTES * 3]).unwrap();
        let ready = root.0.join("kill-ready");
        let resume = root.0.join("never-resume");
        let mut child = Command::new(&executable)
            .args([
                "--exact",
                "document_recovery_store::tests::subprocess_staging_pause_helper",
            ])
            .env("BP_RECOVERY_STAGE_CHILD_ROOT", &root.0)
            .env("BP_RECOVERY_STAGE_CHILD_SOURCE", &source)
            .env("BP_RECOVERY_STAGE_CHILD_READY", &ready)
            .env("BP_RECOVERY_STAGE_CHILD_RESUME", &resume)
            .spawn()
            .unwrap();
        for _ in 0..500 {
            if ready.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(ready.exists(), "staging child did not reach its pause");
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let staged = fs::read_dir(&store.staging)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(staged.len(), 1);
        let exact_temp = staged[0].clone();
        child.kill().unwrap();
        child.wait().unwrap();
        drop(store);

        let reopened = DocumentRecoveryStore::open(&root.0).unwrap();
        assert!(!exact_temp.exists());
        assert_eq!(fs::read_dir(&reopened.staging).unwrap().count(), 0);
    }

    #[test]
    fn subprocess_publishers_serialize_index_and_shared_object_updates() {
        let root = TempRoot::new();
        let executable = std::env::current_exe().unwrap();
        let source = root.0.join("drawing.pdf");
        fs::write(&source, b"shared base").unwrap();
        let mut children = Vec::new();
        for _ in 0..6 {
            children.push(
                Command::new(&executable)
                    .args([
                        "--exact",
                        "document_recovery_store::tests::subprocess_publish_helper",
                    ])
                    .env("BP_RECOVERY_STORE_CHILD_ROOT", &root.0)
                    .env("BP_RECOVERY_STORE_CHILD_SOURCE", &source)
                    .spawn()
                    .unwrap(),
            );
        }
        for mut child in children {
            assert!(child.wait().unwrap().success());
        }
        let store = DocumentRecoveryStore::open(&root.0).unwrap();
        let ids = store.active_document_ids().unwrap();
        assert_eq!(ids.len(), 6);
        for id in ids {
            assert_eq!(store.load(id).unwrap().unwrap().base_pdf, b"shared base");
        }
        assert_eq!(fs::read_dir(&store.objects).unwrap().count(), 2);
    }
}
