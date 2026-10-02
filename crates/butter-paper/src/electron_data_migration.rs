//! Strict predecessor-export validation and immutable backup publication.
//!
//! This boundary never opens Electron's LevelDB or copies its profile. A
//! separately reviewed Electron exporter must first create the narrow v1 tree
//! described here. Applying validated data to native stores is a later step.

use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{OsStr, OsString},
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read as _, Write as _},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    application_shell::{
        ApplicationShellPreferences, ApplicationShellPreferencesStore, PREFERENCES_FILE_NAME,
    },
    generated_document::{GeneratedDocumentRequest, GeneratedPattern},
    native_storage_layout::{NativeProductionStorage, NativeReleaseChannel},
    native_update_policy::{
        UPDATE_SETTINGS_FILE_NAME, UpdateFrequency, UpdateSettings, UpdateSettingsStore,
        serialise_update_settings,
    },
    pdf_engine::PdfPersistenceSession,
    template_library::{
        INDEX_FILE as TEMPLATE_INDEX_FILE, SENTINEL_BYTES as TEMPLATE_SENTINEL_BYTES,
        SENTINEL_FILE as TEMPLATE_SENTINEL_FILE, SOURCE_FILE as TEMPLATE_SOURCE_FILE,
        TemplateLibrary, TemplateRecord, normalise_generated_template_name,
        normalise_imported_template_name, serialise_migration_index,
    },
};

const MANIFEST_FILE: &str = "manifest.json";
const RECEIPT_FILE: &str = "receipt.json";
const BACKUP_SENTINEL: &str = ".butter-paper-electron-migration-backup-v1";
const NATIVE_RECEIPT_FILE: &str = "electron-migration-receipt.json";
const NATIVE_STAGE_SENTINEL: &str = ".butter-paper-electron-migration-stage-v1";
const SCHEMA: &str = "butter-paper/electron-native-migration";
const RECEIPT_SCHEMA: &str = "butter-paper/electron-native-migration-backup";
const VERSION: u32 = 1;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_TOTAL_SOURCE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_TEMPLATES: usize = 256;
const STAGE_RANDOM_BYTES: usize = 16;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ElectronMigrationManifest {
    schema: String,
    version: u32,
    channel: String,
    export_id: String,
    created_at: String,
    source: ElectronSourceIdentity,
    preferences: ExportedPreferences,
    templates: ExportedTemplates,
    unsupported: UnsupportedPredecessorData,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ElectronSourceIdentity {
    product_name: String,
    bundle_identifier: String,
    version: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportedPreferences {
    menu_bar_visible: bool,
    update_frequency: UpdateFrequency,
    last_successful_update_check_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportedTemplates {
    last_template_id: String,
    generated: Vec<ExportedGeneratedTemplate>,
    imported: Vec<ExportedImportedTemplate>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportedGeneratedTemplate {
    id: String,
    name: String,
    title: String,
    width_mm: f64,
    height_mm: f64,
    pattern: Option<ExportedPattern>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportedPattern {
    kind: String,
    spacing_mm: f64,
    color: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportedImportedTemplate {
    id: String,
    name: String,
    created_at: String,
    page_count: usize,
    source: ExportedSource,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportedSource {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UnsupportedPredecessorData {
    recent_signatures: String,
    session: String,
    colour_presets: String,
}

#[derive(Clone, Debug)]
struct ValidatedSource {
    relative_path: PathBuf,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug)]
pub struct ValidatedElectronExport {
    exchange_root: PathBuf,
    manifest: ElectronMigrationManifest,
    manifest_bytes: Vec<u8>,
    manifest_sha256: String,
    sources: Vec<ValidatedSource>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackupPublication {
    Created,
    AlreadyPresent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativePublication {
    Created,
    AlreadyPresent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupMigrationPublication {
    NotPresent {
        legacy_development_store_present: bool,
    },
    Published {
        backup: BackupPublication,
        native: NativePublication,
        legacy_development_store_present: bool,
    },
}

/// Selects whether production startup may inspect and import predecessor data.
///
/// The first public native release deliberately uses `NativeOnly`: the
/// unpublished Electron profile remains an untouched fallback and cannot
/// affect whether the native application starts. Import remains available for
/// a separately reviewed future release, but only through an explicit policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupDataPolicy {
    NativeOnly,
    ImportElectronV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartupDataPreparation {
    NativeOnly { native_root_present: bool },
    ElectronImport(StartupMigrationPublication),
}

/// Prepares production data according to an explicit release policy.
///
/// `NativeOnly` inspects only the bundle-ID-keyed native destination. It does
/// not stat, read, create, import, rename or delete any Electron migration or
/// legacy-development path.
pub fn prepare_startup_data(
    storage: &NativeProductionStorage,
    policy: StartupDataPolicy,
) -> Result<StartupDataPreparation, ElectronMigrationError> {
    match policy {
        StartupDataPolicy::NativeOnly => {
            let native_root_present = validate_native_only_root(storage.layout().durable_root())?;
            Ok(StartupDataPreparation::NativeOnly {
                native_root_present,
            })
        }
        StartupDataPolicy::ImportElectronV1 => {
            publish_startup_migration(storage).map(StartupDataPreparation::ElectronImport)
        }
    }
}

fn validate_native_only_root(path: &Path) -> Result<bool, ElectronMigrationError> {
    validate_absolute_path(path, "native application data root")?;
    let mut current = PathBuf::new();
    let mut final_metadata = None;
    for component in path.components() {
        current.push(component.as_os_str());
        #[cfg(windows)]
        if matches!(component, Component::Prefix(_) | Component::RootDir) {
            // A bare drive prefix such as `C:` is drive-relative and cannot be
            // inspected as an absolute directory. Wait until the first normal
            // component has been joined to the complete `C:\\` root.
            continue;
        }
        let metadata = match fs::symlink_metadata(&current) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(failure(format!(
                "native application data path component {} must be a real directory",
                current.display()
            )));
        }
        final_metadata = Some(metadata);
    }
    let metadata =
        final_metadata.ok_or_else(|| failure("native application data root is empty"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(failure(
                "the native application data root must belong to the current user",
            ));
        }
    }
    Ok(true)
}

/// Performs the production one-time import in its required order: validate the
/// narrow Electron exchange, publish an immutable predecessor backup, then
/// atomically publish native state. The legacy development store is detected
/// and reported but is never read, changed or adopted.
pub fn publish_startup_migration(
    storage: &NativeProductionStorage,
) -> Result<StartupMigrationPublication, ElectronMigrationError> {
    let legacy_development_store_present = storage.legacy_development_store_present()?;
    match fs::symlink_metadata(storage.migration_exchange_root()) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if storage.layout().durable_root().exists() {
                let receipt_bytes = read_regular_single_link(
                    &storage.layout().durable_root().join(NATIVE_RECEIPT_FILE),
                    MAX_MANIFEST_BYTES,
                )?;
                let receipt: NativeMigrationReceipt = serde_json::from_slice(&receipt_bytes)?;
                receipt.validate(storage.channel())?;
                let backup = storage
                    .migration_backup_root()
                    .join(&receipt.manifest_sha256);
                let export = ValidatedElectronExport::load_backup(
                    &backup,
                    storage.migration_exchange_root(),
                    storage.channel(),
                )?;
                export.verify_native_destination(
                    storage.layout().durable_root(),
                    &export.native_receipt(),
                )?;
                return Ok(StartupMigrationPublication::Published {
                    backup: BackupPublication::AlreadyPresent,
                    native: NativePublication::AlreadyPresent,
                    legacy_development_store_present,
                });
            }
            match fs::symlink_metadata(storage.electron_user_data_root()) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
                Ok(_) => {
                    return Err(failure(
                        "the Electron predecessor profile exists but its GPUI migration export is unavailable",
                    ));
                }
            }
            return Ok(StartupMigrationPublication::NotPresent {
                legacy_development_store_present,
            });
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let export =
        ValidatedElectronExport::load(storage.migration_exchange_root(), storage.channel())?;
    let backup = export.publish_backup(storage.migration_backup_root())?;
    let native = export.publish_native(storage)?;
    Ok(StartupMigrationPublication::Published {
        backup,
        native,
        legacy_development_store_present,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupReceipt {
    schema: String,
    version: u32,
    manifest_sha256: String,
    export_id: String,
    channel: String,
    sources: Vec<BackupSourceReceipt>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupSourceReceipt {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NativeMigrationReceipt {
    schema: String,
    version: u32,
    manifest_sha256: String,
    export_id: String,
    channel: String,
    electron_version: String,
    imported_preferences: usize,
    generated_templates: usize,
    imported_templates: usize,
    recent_signatures: String,
    session: String,
    colour_presets: String,
    completion: String,
}

#[derive(Debug)]
pub struct ElectronMigrationError(String);

impl fmt::Display for ElectronMigrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ElectronMigrationError {}

impl From<std::io::Error> for ElectronMigrationError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for ElectronMigrationError {
    fn from(error: serde_json::Error) -> Self {
        Self(format!("invalid Electron migration JSON: {error}"))
    }
}

impl ValidatedElectronExport {
    pub fn load(
        exchange_root: &Path,
        expected_channel: NativeReleaseChannel,
    ) -> Result<Self, ElectronMigrationError> {
        Self::load_source(exchange_root, exchange_root, expected_channel, false)
    }

    fn load_backup(
        backup_root: &Path,
        exchange_root: &Path,
        expected_channel: NativeReleaseChannel,
    ) -> Result<Self, ElectronMigrationError> {
        let export = Self::load_source(backup_root, exchange_root, expected_channel, true)?;
        export.verify_backup(backup_root, &export.expected_receipt())?;
        Ok(export)
    }

    fn load_source(
        source_root: &Path,
        exchange_root: &Path,
        expected_channel: NativeReleaseChannel,
        backup_inventory: bool,
    ) -> Result<Self, ElectronMigrationError> {
        validate_absolute_path(exchange_root, "migration exchange root")?;
        validate_absolute_path(source_root, "migration source root")?;
        validate_directory(source_root, "migration source root")?;
        let manifest_path = source_root.join(MANIFEST_FILE);
        let manifest_bytes = read_regular_single_link(&manifest_path, MAX_MANIFEST_BYTES)?;
        let manifest: ElectronMigrationManifest = serde_json::from_slice(&manifest_bytes)?;
        manifest.validate(expected_channel)?;

        let mut expected_files = BTreeSet::from([PathBuf::from(MANIFEST_FILE)]);
        if backup_inventory {
            expected_files.insert(PathBuf::from(BACKUP_SENTINEL));
            expected_files.insert(PathBuf::from(RECEIPT_FILE));
        }
        let mut expected_directories = BTreeSet::new();
        let mut sources = Vec::with_capacity(manifest.templates.imported.len());
        let mut total_source_bytes = 0_u64;
        for template in &manifest.templates.imported {
            let expected_path = PathBuf::from("templates")
                .join(&template.id)
                .join("source.pdf");
            if Path::new(&template.source.path) != expected_path {
                return Err(failure(format!(
                    "imported template {} must use {}",
                    template.id,
                    expected_path.display()
                )));
            }
            validate_relative_path(&expected_path, "template source")?;
            validate_relative_directory_chain(
                source_root,
                expected_path.parent().unwrap(),
                "template source directory",
            )?;
            if !expected_files.insert(expected_path.clone()) {
                return Err(failure("migration export contains a duplicate source path"));
            }
            expected_directories.insert(PathBuf::from("templates"));
            expected_directories.insert(PathBuf::from("templates").join(&template.id));
            let bytes =
                read_regular_single_link(&source_root.join(&expected_path), MAX_SOURCE_BYTES)?;
            if bytes.len() as u64 != template.source.bytes {
                return Err(failure(format!(
                    "source byte count changed for {}",
                    expected_path.display()
                )));
            }
            let sha256 = digest(&bytes);
            if sha256 != template.source.sha256 {
                return Err(failure(format!(
                    "source checksum changed for {}",
                    expected_path.display()
                )));
            }
            total_source_bytes = total_source_bytes
                .checked_add(template.source.bytes)
                .ok_or_else(|| failure("migration export source size overflow"))?;
            if total_source_bytes > MAX_TOTAL_SOURCE_BYTES {
                return Err(failure("migration export source budget exceeded"));
            }
            sources.push(ValidatedSource {
                relative_path: expected_path,
                bytes: template.source.bytes,
                sha256,
            });
        }
        validate_exact_inventory(source_root, &expected_files, &expected_directories)?;
        let manifest_sha256 = digest(&manifest_bytes);
        Ok(Self {
            exchange_root: exchange_root.to_path_buf(),
            manifest,
            manifest_bytes,
            manifest_sha256,
            sources,
        })
    }

    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }

    pub fn export_id(&self) -> &str {
        &self.manifest.export_id
    }

    pub fn menu_bar_visible(&self) -> bool {
        self.manifest.preferences.menu_bar_visible
    }

    fn manifest_channel(&self) -> Result<NativeReleaseChannel, ElectronMigrationError> {
        match self.manifest.channel.as_str() {
            "stable" => Ok(NativeReleaseChannel::Stable),
            "beta" => Ok(NativeReleaseChannel::Beta),
            _ => Err(failure("validated migration channel is invalid")),
        }
    }

    pub fn generated_template_count(&self) -> usize {
        self.manifest.templates.generated.len()
    }

    pub fn imported_template_count(&self) -> usize {
        self.manifest.templates.imported.len()
    }

    /// Publishes an immutable, checksum-verifiable predecessor backup.
    ///
    /// The source tree remains untouched. An identical completed backup is an
    /// idempotent success; any pre-existing drift fails closed.
    pub fn publish_backup(
        &self,
        backup_root: &Path,
    ) -> Result<BackupPublication, ElectronMigrationError> {
        validate_absolute_path(backup_root, "migration backup root")?;
        ensure_private_directory(backup_root)?;
        let destination = backup_root.join(&self.manifest_sha256);
        let receipt = self.expected_receipt();
        if destination.exists() {
            #[cfg(unix)]
            verify_existing_directory_at(
                backup_root,
                OsStr::new(&self.manifest_sha256),
                "migration backup",
                || self.verify_backup(&destination, &receipt),
            )?;
            #[cfg(not(unix))]
            self.verify_backup(&destination, &receipt)?;
            return Ok(BackupPublication::AlreadyPresent);
        }

        let stage_prefix = format!(".backup-{}-", self.manifest_sha256);
        let sentinel_bytes = format!("{}\n", self.manifest_sha256).into_bytes();
        #[cfg(unix)]
        let stage_authority =
            RetainedStage::create(backup_root, &stage_prefix, BACKUP_SENTINEL, &sentinel_bytes)?;
        #[cfg(unix)]
        let mut staged_files = BTreeMap::from([(
            PathBuf::from(BACKUP_SENTINEL),
            staged_fingerprint(&sentinel_bytes),
        )]);
        #[cfg(not(unix))]
        let temporary = {
            let path = backup_root.join(format!("{stage_prefix}partial"));
            if path.exists() {
                return Err(failure(
                    "pre-existing backup staging directory was preserved",
                ));
            }
            create_private_directory(&path)?;
            write_private_file(&path.join(BACKUP_SENTINEL), &sentinel_bytes)?;
            path
        };
        #[cfg(unix)]
        stage_authority.write_file(
            Path::new(MANIFEST_FILE),
            &self.manifest_bytes,
            &mut staged_files,
        )?;
        #[cfg(not(unix))]
        write_private_file(&temporary.join(MANIFEST_FILE), &self.manifest_bytes)?;
        for source in &self.sources {
            let bytes = read_regular_single_link(
                &self.exchange_root.join(&source.relative_path),
                MAX_SOURCE_BYTES,
            )?;
            if bytes.len() as u64 != source.bytes || digest(&bytes) != source.sha256 {
                return Err(failure(format!(
                    "migration source changed before backup: {}",
                    source.relative_path.display()
                )));
            }
            #[cfg(unix)]
            stage_authority.write_file(&source.relative_path, &bytes, &mut staged_files)?;
            #[cfg(not(unix))]
            {
                let target = temporary.join(&source.relative_path);
                ensure_private_directory(target.parent().unwrap())?;
                write_private_file(&target, &bytes)?;
            }
        }
        let receipt_bytes = receipt_bytes(&receipt)?;
        #[cfg(unix)]
        stage_authority.write_file(Path::new(RECEIPT_FILE), &receipt_bytes, &mut staged_files)?;
        #[cfg(unix)]
        stage_authority.verify_exact_inventory(&staged_files)?;
        #[cfg(not(unix))]
        {
            write_private_file(&temporary.join(RECEIPT_FILE), &receipt_bytes)?;
            sync_tree_directories(&temporary)?;
            self.verify_backup(&temporary, &receipt)?;
        }
        #[cfg(unix)]
        let publish_result =
            stage_authority.publish(OsStr::new(&self.manifest_sha256), &staged_files);
        #[cfg(windows)]
        let publish_result = windows_move_write_through(&temporary, &destination);
        #[cfg(all(not(unix), not(windows)))]
        let publish_result = fs::rename(&temporary, &destination);
        match publish_result {
            Ok(()) => {
                #[cfg(not(unix))]
                sync_directory(backup_root)?;
                Ok(BackupPublication::Created)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                #[cfg(unix)]
                verify_existing_directory_at(
                    backup_root,
                    OsStr::new(&self.manifest_sha256),
                    "migration backup",
                    || self.verify_backup(&destination, &receipt),
                )?;
                #[cfg(not(unix))]
                self.verify_backup(&destination, &receipt)?;
                Ok(BackupPublication::AlreadyPresent)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Builds native preferences and templates from the immutable backup and
    /// publishes the whole `native-v1` directory with one rename.
    ///
    /// A pre-existing destination is accepted only when its complete receipt
    /// names this exact manifest digest and its imported stores still verify.
    pub fn publish_native(
        &self,
        storage: &NativeProductionStorage,
    ) -> Result<NativePublication, ElectronMigrationError> {
        if self.exchange_root != storage.migration_exchange_root() {
            return Err(failure(
                "validated export does not belong to the selected production channel",
            ));
        }
        let backup = storage.migration_backup_root().join(&self.manifest_sha256);
        self.verify_backup(&backup, &self.expected_receipt())?;
        let destination = storage.layout().durable_root();
        validate_absolute_path(destination, "native migration destination")?;
        let expected_receipt = self.native_receipt();
        if destination.exists() {
            #[cfg(unix)]
            verify_existing_directory_at(
                destination.parent().unwrap(),
                destination.file_name().unwrap(),
                "native migration destination",
                || self.verify_native_destination(destination, &expected_receipt),
            )?;
            #[cfg(not(unix))]
            self.verify_native_destination(destination, &expected_receipt)?;
            return Ok(NativePublication::AlreadyPresent);
        }

        let parent = destination
            .parent()
            .ok_or_else(|| failure("native migration destination has no parent"))?;
        ensure_private_directory(parent)?;
        let stage_prefix = format!(".native-v1-migration-{}-", self.manifest_sha256);
        let sentinel_bytes = format!("{}\n", self.manifest_sha256).into_bytes();
        #[cfg(unix)]
        let stage_authority = RetainedStage::create(
            parent,
            &stage_prefix,
            NATIVE_STAGE_SENTINEL,
            &sentinel_bytes,
        )?;
        #[cfg(not(unix))]
        let stage = {
            let path = parent.join(format!("{stage_prefix}partial"));
            if path.exists() {
                return Err(failure(
                    "pre-existing native migration staging directory was preserved",
                ));
            }
            create_private_directory(&path)?;
            write_private_file(&path.join(NATIVE_STAGE_SENTINEL), &sentinel_bytes)?;
            path
        };

        let mut preferences = ApplicationShellPreferences::default();
        preferences.set_menu_bar_visible(self.manifest.preferences.menu_bar_visible);
        let update_settings = self
            .manifest
            .preferences
            .native_update_settings(storage.channel())?;
        #[cfg(unix)]
        let mut staged_files = BTreeMap::from([(
            PathBuf::from(NATIVE_STAGE_SENTINEL),
            staged_fingerprint(&sentinel_bytes),
        )]);
        #[cfg(unix)]
        {
            let mut preferences_bytes = serde_json::to_vec_pretty(&preferences)?;
            preferences_bytes.push(b'\n');
            stage_authority.write_file(
                Path::new(PREFERENCES_FILE_NAME),
                &preferences_bytes,
                &mut staged_files,
            )?;
            stage_authority.write_file(
                Path::new(UPDATE_SETTINGS_FILE_NAME),
                &serialise_update_settings(&update_settings)?,
                &mut staged_files,
            )?;

            let mut records = Vec::new();
            for template in &self.manifest.templates.generated {
                let request = template.request();
                request.to_pdf_bytes().map_err(|error| {
                    failure(format!("cannot validate generated template: {error}"))
                })?;
                records.push(TemplateRecord::Generated {
                    id: template.id.clone(),
                    name: normalise_generated_template_name(&template.name).map_err(|error| {
                        failure(format!("cannot stage generated template: {error}"))
                    })?,
                    request,
                });
            }
            for template in &self.manifest.templates.imported {
                let source = self
                    .sources
                    .iter()
                    .find(|source| source.relative_path == Path::new(&template.source.path))
                    .ok_or_else(|| {
                        failure(format!(
                            "validated imported template source is unavailable: {}",
                            template.id
                        ))
                    })?;
                let bytes = read_verified_backup_source(&backup, source)?;
                let page_count =
                    PdfPersistenceSession::page_count_from_bytes(&bytes).map_err(|error| {
                        failure(format!("cannot validate imported template: {error}"))
                    })?;
                if page_count != template.page_count || digest(&bytes) != template.source.sha256 {
                    return Err(failure(format!(
                        "imported template metadata changed after PDF validation: {}",
                        template.id
                    )));
                }
                records.push(TemplateRecord::ImportedPdf {
                    id: template.id.clone(),
                    name: normalise_imported_template_name(&template.name),
                    page_count,
                    created_at: template.created_at.clone(),
                    sha256: template.source.sha256.clone(),
                });
                stage_authority.write_file(
                    &Path::new("template-library")
                        .join(&template.id)
                        .join(TEMPLATE_SOURCE_FILE),
                    &bytes,
                    &mut staged_files,
                )?;
            }
            stage_authority.write_file(
                &Path::new("template-library").join(TEMPLATE_SENTINEL_FILE),
                TEMPLATE_SENTINEL_BYTES,
                &mut staged_files,
            )?;
            stage_authority.write_file(
                &Path::new("template-library").join(TEMPLATE_INDEX_FILE),
                &serialise_migration_index(&records, &self.manifest.templates.last_template_id)
                    .map_err(|error| failure(format!("cannot stage template library: {error}")))?,
                &mut staged_files,
            )?;
            stage_authority.write_file(
                Path::new(NATIVE_RECEIPT_FILE),
                &native_receipt_bytes(&expected_receipt)?,
                &mut staged_files,
            )?;
            stage_authority.verify_exact_inventory(&staged_files)?;
        }
        #[cfg(not(unix))]
        {
            ApplicationShellPreferencesStore::new(&stage)
                .save(preferences)
                .map_err(|error| {
                    failure(format!("cannot stage application preferences: {error}"))
                })?;
            UpdateSettingsStore::new(&stage, storage.channel())
                .save(&update_settings)
                .map_err(|error| failure(format!("cannot stage update preferences: {error}")))?;

            let mut templates = TemplateLibrary::open(stage.join("template-library"))
                .map_err(|error| failure(format!("cannot stage template library: {error}")))?;
            for template in &self.manifest.templates.generated {
                templates
                    .add_generated(&template.id, &template.name, template.request())
                    .map_err(|error| {
                        failure(format!("cannot stage generated template: {error}"))
                    })?;
            }
            for template in &self.manifest.templates.imported {
                let source = self
                    .sources
                    .iter()
                    .find(|source| source.relative_path == Path::new(&template.source.path))
                    .ok_or_else(|| {
                        failure(format!(
                            "validated imported template source is unavailable: {}",
                            template.id
                        ))
                    })?;
                let bytes = read_verified_backup_source(&backup, source)?;
                let record = templates
                    .import_pdf_bytes(&template.id, &template.name, &template.created_at, &bytes)
                    .map_err(|error| failure(format!("cannot stage imported template: {error}")))?;
                if record.page_count() != Some(template.page_count)
                    || record.sha256() != Some(template.source.sha256.as_str())
                {
                    return Err(failure(format!(
                        "imported template metadata changed after PDF validation: {}",
                        template.id
                    )));
                }
            }
            templates
                .select(&self.manifest.templates.last_template_id)
                .map_err(|error| {
                    failure(format!("cannot restore last template selection: {error}"))
                })?;
            drop(templates);
            write_private_file(
                &stage.join(NATIVE_RECEIPT_FILE),
                &native_receipt_bytes(&expected_receipt)?,
            )?;
            sync_tree_directories(&stage)?;
            self.verify_native_destination(&stage, &expected_receipt)?;
        }
        let destination_leaf = destination
            .file_name()
            .ok_or_else(|| failure("native migration destination has no leaf"))?;
        #[cfg(unix)]
        let publish_result = stage_authority.publish(destination_leaf, &staged_files);
        #[cfg(windows)]
        let publish_result = windows_move_write_through(&stage, destination);
        #[cfg(all(not(unix), not(windows)))]
        let publish_result = fs::rename(&stage, destination);
        match publish_result {
            Ok(()) => {
                #[cfg(not(unix))]
                sync_directory(parent)?;
                Ok(NativePublication::Created)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                #[cfg(unix)]
                verify_existing_directory_at(
                    parent,
                    destination_leaf,
                    "native migration destination",
                    || self.verify_native_destination(destination, &expected_receipt),
                )?;
                #[cfg(not(unix))]
                self.verify_native_destination(destination, &expected_receipt)?;
                Ok(NativePublication::AlreadyPresent)
            }
            Err(error) => Err(error.into()),
        }
    }

    fn native_receipt(&self) -> NativeMigrationReceipt {
        NativeMigrationReceipt {
            schema: "butter-paper/electron-native-migration-application".into(),
            version: VERSION,
            manifest_sha256: self.manifest_sha256.clone(),
            export_id: self.manifest.export_id.clone(),
            channel: self.manifest.channel.clone(),
            electron_version: self.manifest.source.version.clone(),
            imported_preferences: 2,
            generated_templates: self.manifest.templates.generated.len(),
            imported_templates: self.manifest.templates.imported.len(),
            recent_signatures: self.manifest.unsupported.recent_signatures.clone(),
            session: self.manifest.unsupported.session.clone(),
            colour_presets: self.manifest.unsupported.colour_presets.clone(),
            completion: "committed".into(),
        }
    }

    fn verify_native_destination(
        &self,
        destination: &Path,
        expected_receipt: &NativeMigrationReceipt,
    ) -> Result<(), ElectronMigrationError> {
        validate_directory(destination, "native migration destination")?;
        if read_regular_single_link(&destination.join(NATIVE_RECEIPT_FILE), MAX_MANIFEST_BYTES)?
            != native_receipt_bytes(expected_receipt)?
        {
            return Err(failure(
                "pre-existing native destination has a different migration receipt",
            ));
        }
        let preferences = ApplicationShellPreferencesStore::new(destination)
            .load()
            .map_err(|error| failure(format!("cannot verify migrated preferences: {error}")))?;
        if preferences.menu_bar_visible() != self.manifest.preferences.menu_bar_visible {
            return Err(failure("migrated application preferences changed"));
        }
        let update_settings = UpdateSettingsStore::new(destination, self.manifest_channel()?)
            .load()
            .map_err(|error| {
                failure(format!(
                    "cannot verify migrated update preferences: {error}"
                ))
            })?;
        if update_settings
            != self
                .manifest
                .preferences
                .native_update_settings(self.manifest_channel()?)?
        {
            return Err(failure("migrated update preferences changed"));
        }
        let templates = TemplateLibrary::open(destination.join("template-library"))
            .map_err(|error| failure(format!("cannot verify migrated templates: {error}")))?;
        if templates.last_template_id() != self.manifest.templates.last_template_id
            || templates.records().len()
                != self.manifest.templates.generated.len() + self.manifest.templates.imported.len()
        {
            return Err(failure("migrated template index changed"));
        }
        for template in &self.manifest.templates.imported {
            templates
                .managed_source_path(&template.id)
                .map_err(|error| failure(format!("cannot verify migrated template: {error}")))?;
        }
        Ok(())
    }

    fn expected_receipt(&self) -> BackupReceipt {
        BackupReceipt {
            schema: RECEIPT_SCHEMA.into(),
            version: VERSION,
            manifest_sha256: self.manifest_sha256.clone(),
            export_id: self.manifest.export_id.clone(),
            channel: self.manifest.channel.clone(),
            sources: self
                .sources
                .iter()
                .map(|source| BackupSourceReceipt {
                    path: source.relative_path.to_string_lossy().into_owned(),
                    bytes: source.bytes,
                    sha256: source.sha256.clone(),
                })
                .collect(),
        }
    }

    fn verify_backup(
        &self,
        destination: &Path,
        expected_receipt: &BackupReceipt,
    ) -> Result<(), ElectronMigrationError> {
        validate_directory(destination, "migration backup")?;
        let actual_receipt_bytes =
            read_regular_single_link(&destination.join(RECEIPT_FILE), MAX_MANIFEST_BYTES)?;
        if actual_receipt_bytes != receipt_bytes(expected_receipt)? {
            return Err(failure("existing migration backup receipt does not match"));
        }
        let manifest =
            read_regular_single_link(&destination.join(MANIFEST_FILE), MAX_MANIFEST_BYTES)?;
        if manifest != self.manifest_bytes {
            return Err(failure("existing migration backup manifest changed"));
        }
        let mut files = BTreeSet::from([
            PathBuf::from(BACKUP_SENTINEL),
            PathBuf::from(MANIFEST_FILE),
            PathBuf::from(RECEIPT_FILE),
        ]);
        if read_regular_single_link(&destination.join(BACKUP_SENTINEL), 128)?
            != format!("{}\n", self.manifest_sha256).into_bytes()
        {
            return Err(failure("existing migration backup sentinel changed"));
        }
        let mut directories = BTreeSet::new();
        for source in &self.sources {
            let bytes = read_regular_single_link(
                &destination.join(&source.relative_path),
                MAX_SOURCE_BYTES,
            )?;
            if bytes.len() as u64 != source.bytes || digest(&bytes) != source.sha256 {
                return Err(failure(format!(
                    "existing migration backup source changed: {}",
                    source.relative_path.display()
                )));
            }
            files.insert(source.relative_path.clone());
            directories.insert(PathBuf::from("templates"));
            directories.insert(source.relative_path.parent().unwrap().to_path_buf());
        }
        validate_exact_inventory(destination, &files, &directories)
    }
}

impl NativeMigrationReceipt {
    fn validate(
        &self,
        expected_channel: NativeReleaseChannel,
    ) -> Result<(), ElectronMigrationError> {
        let expected_channel = match expected_channel {
            NativeReleaseChannel::Stable => "stable",
            NativeReleaseChannel::Beta => "beta",
        };
        if self.schema != "butter-paper/electron-native-migration-application"
            || self.version != VERSION
            || self.channel != expected_channel
            || self.imported_preferences != 2
            || self.recent_signatures != "requires-secure-bridge"
            || self.session != "not-persisted-by-electron"
            || self.colour_presets != "not-imported-v1"
            || self.completion != "committed"
        {
            return Err(failure("native migration receipt is invalid"));
        }
        validate_lower_hex(&self.manifest_sha256, 64, "native manifest checksum")?;
        validate_lower_hex(&self.export_id, 64, "native export identifier")?;
        validate_nonempty_bounded(&self.electron_version, 64, "Electron source version")?;
        if self.generated_templates + self.imported_templates > MAX_TEMPLATES {
            return Err(failure(
                "native migration receipt template count is invalid",
            ));
        }
        Ok(())
    }
}

impl ElectronMigrationManifest {
    fn validate(
        &self,
        expected_channel: NativeReleaseChannel,
    ) -> Result<(), ElectronMigrationError> {
        if self.schema != SCHEMA || self.version != VERSION {
            return Err(failure("unsupported Electron migration manifest schema"));
        }
        self.preferences.native_update_settings(expected_channel)?;
        let expected_channel = match expected_channel {
            NativeReleaseChannel::Stable => "stable",
            NativeReleaseChannel::Beta => "beta",
        };
        if self.channel != expected_channel {
            return Err(failure(
                "Electron migration channel does not match native channel",
            ));
        }
        let (expected_product_name, expected_bundle_identifier) = match expected_channel {
            "stable" => ("Butter Paper", "com.butterpaper.desktop"),
            "beta" => ("Butter Paper Beta", "com.butterpaper.desktop.beta"),
            _ => unreachable!(),
        };
        if self.source.product_name != expected_product_name
            || self.source.bundle_identifier != expected_bundle_identifier
        {
            return Err(failure(
                "Electron source product identity does not match native channel",
            ));
        }
        validate_nonempty_bounded(&self.source.version, 64, "Electron source version")?;
        validate_lower_hex(&self.export_id, 64, "export identifier")?;
        validate_nonempty_bounded(&self.created_at, 128, "export creation time")?;
        if self.templates.generated.len() + self.templates.imported.len() > MAX_TEMPLATES {
            return Err(failure("migration template count exceeds the v1 limit"));
        }
        let mut ids = BTreeSet::new();
        for template in &self.templates.generated {
            validate_custom_id(&template.id)?;
            validate_template_name(&template.name)?;
            validate_nonempty_bounded(&template.title, 256, "generated template title")?;
            validate_dimension(template.width_mm, "generated template width")?;
            validate_dimension(template.height_mm, "generated template height")?;
            if let Some(pattern) = &template.pattern {
                if !matches!(
                    pattern.kind.as_str(),
                    "dots" | "grid" | "lined" | "isometric" | "triangle"
                ) {
                    return Err(failure("generated template pattern kind is unsupported"));
                }
                if !pattern.spacing_mm.is_finite() || !(1.0..=500.0).contains(&pattern.spacing_mm) {
                    return Err(failure("generated template pattern spacing is invalid"));
                }
                validate_colour(&pattern.color)?;
            }
            if !ids.insert(template.id.as_str()) {
                return Err(failure(
                    "migration export contains duplicate template identifiers",
                ));
            }
        }
        for template in &self.templates.imported {
            validate_imported_id(&template.id)?;
            validate_template_name(&template.name)?;
            validate_nonempty_bounded(&template.created_at, 128, "template creation time")?;
            if template.page_count == 0 {
                return Err(failure("imported template page count must be positive"));
            }
            if template.source.bytes == 0 || template.source.bytes > MAX_SOURCE_BYTES {
                return Err(failure("imported template source size is invalid"));
            }
            validate_lower_hex(&template.source.sha256, 64, "template source checksum")?;
            if !ids.insert(template.id.as_str()) {
                return Err(failure(
                    "migration export contains duplicate template identifiers",
                ));
            }
        }
        if !is_builtin_template_id(&self.templates.last_template_id)
            && !ids.contains(self.templates.last_template_id.as_str())
        {
            return Err(failure("last template identifier is unavailable"));
        }
        if self.unsupported.recent_signatures != "requires-secure-bridge"
            || self.unsupported.session != "not-persisted-by-electron"
            || self.unsupported.colour_presets != "not-imported-v1"
        {
            return Err(failure(
                "unsupported predecessor data must be acknowledged explicitly",
            ));
        }
        Ok(())
    }
}

impl ExportedPreferences {
    fn native_update_settings(
        &self,
        channel: NativeReleaseChannel,
    ) -> Result<UpdateSettings, ElectronMigrationError> {
        let mut settings = UpdateSettings::defaults(channel);
        settings.set_frequency(self.update_frequency);
        if let Some(timestamp) = self.last_successful_update_check_at.as_deref() {
            settings.record_successful_check(timestamp).map_err(|_| {
                failure("last successful update check is not canonical RFC3339 UTC")
            })?;
        }
        Ok(settings)
    }
}

impl ExportedGeneratedTemplate {
    fn request(&self) -> GeneratedDocumentRequest {
        GeneratedDocumentRequest {
            title: self.title.clone(),
            width_mm: self.width_mm,
            height_mm: self.height_mm,
            pattern: self.pattern.as_ref().map(ExportedPattern::pattern),
        }
    }
}

impl ExportedPattern {
    fn pattern(&self) -> GeneratedPattern {
        match self.kind.as_str() {
            "dots" => GeneratedPattern::Dots {
                spacing_mm: self.spacing_mm,
                color: self.color.clone(),
            },
            "grid" => GeneratedPattern::SquareGrid {
                spacing_mm: self.spacing_mm,
                color: self.color.clone(),
            },
            "lined" => GeneratedPattern::Ruled {
                spacing_mm: self.spacing_mm,
                color: self.color.clone(),
            },
            "isometric" => GeneratedPattern::Isometric {
                spacing_mm: self.spacing_mm,
                color: self.color.clone(),
            },
            "triangle" => GeneratedPattern::Triangle {
                spacing_mm: self.spacing_mm,
                color: self.color.clone(),
            },
            _ => unreachable!("validated pattern kind"),
        }
    }
}

fn validate_dimension(value: f64, label: &str) -> Result<(), ElectronMigrationError> {
    if value.is_finite() && (10.0..=5_000.0).contains(&value) {
        Ok(())
    } else {
        Err(failure(format!("{label} is invalid")))
    }
}

fn validate_colour(value: &str) -> Result<(), ElectronMigrationError> {
    if value.len() == 7
        && value.starts_with('#')
        && value[1..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(failure("generated template pattern colour is invalid"))
    }
}

fn validate_template_name(value: &str) -> Result<(), ElectronMigrationError> {
    if value.is_empty()
        || value.encode_utf16().count() > 80
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(failure("template name is not canonical"));
    }
    Ok(())
}

fn validate_nonempty_bounded(
    value: &str,
    maximum: usize,
    label: &str,
) -> Result<(), ElectronMigrationError> {
    if value.is_empty() || value.len() > maximum || value.chars().any(|value| value == '\0') {
        Err(failure(format!("{label} is invalid")))
    } else {
        Ok(())
    }
}

fn validate_custom_id(value: &str) -> Result<(), ElectronMigrationError> {
    validate_identifier(value, "custom-", "generated template identifier")
}

fn validate_imported_id(value: &str) -> Result<(), ElectronMigrationError> {
    let Some(uuid) = value.strip_prefix("imported-") else {
        return Err(failure("imported template identifier is invalid"));
    };
    if uuid.len() != 36
        || uuid.as_bytes().get(8) != Some(&b'-')
        || uuid.as_bytes().get(13) != Some(&b'-')
        || uuid.as_bytes().get(18) != Some(&b'-')
        || uuid.as_bytes().get(23) != Some(&b'-')
        || !uuid.bytes().enumerate().all(|(index, byte)| {
            matches!(index, 8 | 13 | 18 | 23)
                || byte.is_ascii_digit()
                || (b'a'..=b'f').contains(&byte)
        })
    {
        Err(failure("imported template identifier is invalid"))
    } else {
        Ok(())
    }
}

fn validate_identifier(
    value: &str,
    prefix: &str,
    label: &str,
) -> Result<(), ElectronMigrationError> {
    if value.len() <= prefix.len()
        || value.len() > 128
        || !value.starts_with(prefix)
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
    {
        Err(failure(format!("{label} is invalid")))
    } else {
        Ok(())
    }
}

fn is_builtin_template_id(value: &str) -> bool {
    matches!(
        value,
        "built-in-blank"
            | "built-in-dots"
            | "built-in-grid"
            | "built-in-lined"
            | "built-in-isometric"
            | "built-in-triangle"
    )
}

fn validate_lower_hex(
    value: &str,
    length: usize,
    label: &str,
) -> Result<(), ElectronMigrationError> {
    if value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(failure(format!("{label} must be lowercase hexadecimal")))
    }
}

fn validate_absolute_path(path: &Path, label: &str) -> Result<(), ElectronMigrationError> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| component == Component::ParentDir)
    {
        Err(failure(format!(
            "{label} must be absolute and canonical-looking"
        )))
    } else {
        Ok(())
    }
}

fn validate_relative_path(path: &Path, label: &str) -> Result<(), ElectronMigrationError> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        Err(failure(format!("{label} must be a safe relative path")))
    } else {
        Ok(())
    }
}

fn validate_directory(path: &Path, label: &str) -> Result<(), ElectronMigrationError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| failure(format!("cannot inspect {label}: {error}")))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(failure(format!("{label} must be a real directory")));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(failure(format!("{label} must belong to the current user")));
        }
    }
    Ok(())
}

fn validate_relative_directory_chain(
    root: &Path,
    relative: &Path,
    label: &str,
) -> Result<(), ElectronMigrationError> {
    validate_relative_path(relative, label)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(failure(format!("{label} is unsafe")));
        };
        current.push(component);
        validate_directory(&current, label)?;
    }
    Ok(())
}

fn read_regular_single_link(path: &Path, limit: u64) -> Result<Vec<u8>, ElectronMigrationError> {
    let before = fs::symlink_metadata(path)
        .map_err(|error| failure(format!("cannot inspect {}: {error}", path.display())))?;
    if !before.file_type().is_file() || before.file_type().is_symlink() {
        return Err(failure(format!(
            "{} must be a regular file",
            path.display()
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if before.nlink() != 1 || before.uid() != unsafe { libc::geteuid() } {
            return Err(failure(format!(
                "{} must be current-user-owned and single-linked",
                path.display()
            )));
        }
    }
    if before.len() > limit {
        return Err(failure(format!(
            "{} exceeds its byte limit",
            path.display()
        )));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(path)?;
    let opened = file.metadata()?;
    if !opened.is_file() || opened.len() != before.len() {
        return Err(failure(format!("{} changed while opening", path.display())));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if opened.dev() != before.dev()
            || opened.ino() != before.ino()
            || opened.nlink() != 1
            || opened.uid() != unsafe { libc::geteuid() }
        {
            return Err(failure(format!("{} changed while opening", path.display())));
        }
    }
    let mut bytes = Vec::with_capacity(opened.len() as usize);
    std::io::Read::take(&mut file, limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(failure(format!(
            "{} exceeds its byte limit",
            path.display()
        )));
    }
    let after = file.metadata()?;
    if after.len() != opened.len() {
        return Err(failure(format!("{} changed while reading", path.display())));
    }
    Ok(bytes)
}

fn read_verified_backup_source(
    backup: &Path,
    source: &ValidatedSource,
) -> Result<Vec<u8>, ElectronMigrationError> {
    let bytes = read_regular_single_link(&backup.join(&source.relative_path), MAX_SOURCE_BYTES)?;
    if bytes.len() as u64 != source.bytes || digest(&bytes) != source.sha256 {
        return Err(failure(format!(
            "migration backup source changed before import: {}",
            source.relative_path.display()
        )));
    }
    Ok(bytes)
}

fn validate_exact_inventory(
    root: &Path,
    expected_files: &BTreeSet<PathBuf>,
    expected_directories: &BTreeSet<PathBuf>,
) -> Result<(), ElectronMigrationError> {
    let mut actual_files = BTreeSet::new();
    let mut actual_directories = BTreeSet::new();
    collect_inventory(root, root, &mut actual_files, &mut actual_directories)?;
    if &actual_files != expected_files || &actual_directories != expected_directories {
        return Err(failure(
            "migration tree contains missing or unexpected entries",
        ));
    }
    Ok(())
}

fn collect_inventory(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
    directories: &mut BTreeSet<PathBuf>,
) -> Result<(), ElectronMigrationError> {
    validate_directory(directory, "migration tree directory")?;
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| failure("migration inventory escaped its root"))?
            .to_path_buf();
        validate_relative_path(&relative, "migration inventory entry")?;
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(failure(format!(
                "migration tree contains symlink {}",
                relative.display()
            )));
        }
        if metadata.is_dir() {
            directories.insert(relative);
            collect_inventory(root, &path, files, directories)?;
        } else if metadata.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                if metadata.nlink() != 1 {
                    return Err(failure(format!(
                        "migration tree contains hard link {}",
                        relative.display()
                    )));
                }
            }
            files.insert(relative);
        } else {
            return Err(failure("migration tree contains a non-file entry"));
        }
    }
    Ok(())
}

fn ensure_private_directory(path: &Path) -> Result<(), ElectronMigrationError> {
    if path.exists() {
        return validate_directory(path, "migration backup directory");
    }
    let parent = path
        .parent()
        .ok_or_else(|| failure("migration backup directory has no parent"))?;
    if !parent.exists() {
        ensure_private_directory(parent)?;
    } else {
        validate_directory(parent, "migration backup parent")?;
    }
    create_private_directory(path)
}

fn create_private_directory(path: &Path) -> Result<(), ElectronMigrationError> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(path)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), ElectronMigrationError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
#[derive(Debug)]
struct RetainedStage {
    parent: File,
    parent_path: PathBuf,
    leaf: OsString,
    directory: File,
}

#[cfg(unix)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct StagedFileFingerprint {
    bytes: u64,
    sha256: String,
}

#[cfg(unix)]
impl RetainedStage {
    fn create(
        parent_path: &Path,
        prefix: &str,
        sentinel_leaf: &str,
        sentinel_bytes: &[u8],
    ) -> Result<Self, ElectronMigrationError> {
        let parent = open_private_directory(parent_path, "migration staging parent")?;
        reject_interrupted_stages(&parent, prefix)?;
        for _ in 0..128 {
            let mut random = [0_u8; STAGE_RANDOM_BYTES];
            getrandom::fill(&mut random)
                .map_err(|_| failure("secure migration staging randomness is unavailable"))?;
            let token = random
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>();
            let leaf = OsString::from(format!("{prefix}{token}.partial"));
            match rustix::fs::mkdirat(
                &parent,
                &leaf,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR | rustix::fs::Mode::XUSR,
            ) {
                Ok(()) => {
                    let directory =
                        open_private_directory_at(&parent, &leaf, "migration staging directory")?;
                    write_private_file_at(&directory, OsStr::new(sentinel_leaf), sentinel_bytes)?;
                    directory.sync_all()?;
                    parent.sync_all()?;
                    return Ok(Self {
                        parent,
                        parent_path: parent_path.to_path_buf(),
                        leaf,
                        directory,
                    });
                }
                Err(error)
                    if std::io::Error::from_raw_os_error(error.raw_os_error()).kind()
                        == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
        }
        Err(failure(
            "cannot allocate a unique migration staging directory",
        ))
    }

    fn path(&self) -> PathBuf {
        self.parent_path.join(&self.leaf)
    }

    fn write_file(
        &self,
        relative: &Path,
        bytes: &[u8],
        expected: &mut BTreeMap<PathBuf, StagedFileFingerprint>,
    ) -> Result<(), ElectronMigrationError> {
        let (parent, leaf) = self.open_or_create_parent(relative)?;
        write_private_file_at(&parent, &leaf, bytes)?;
        expected.insert(relative.to_path_buf(), staged_fingerprint(bytes));
        Ok(())
    }

    fn verify_exact_inventory(
        &self,
        expected: &BTreeMap<PathBuf, StagedFileFingerprint>,
    ) -> Result<(), ElectronMigrationError> {
        let mut files = BTreeMap::new();
        let mut directories = BTreeSet::from([PathBuf::new()]);
        collect_stage_inventory_at(&self.directory, Path::new(""), &mut files, &mut directories)?;
        let mut expected_directories = BTreeSet::from([PathBuf::new()]);
        for path in expected.keys() {
            let mut parent = path.parent();
            while let Some(path) = parent {
                expected_directories.insert(path.to_path_buf());
                parent = path.parent();
            }
        }
        if files != *expected || directories != expected_directories {
            return Err(failure(
                "migration staging inventory changed before publication",
            ));
        }
        Ok(())
    }

    fn open_or_create_parent(
        &self,
        relative: &Path,
    ) -> Result<(File, OsString), ElectronMigrationError> {
        let mut components = relative.components().peekable();
        let mut directory = self.directory.try_clone()?;
        let mut leaf = None;
        while let Some(component) = components.next() {
            let Component::Normal(name) = component else {
                return Err(failure(
                    "migration staging path is not a safe relative path",
                ));
            };
            if components.peek().is_none() {
                leaf = Some(name.to_os_string());
                break;
            }
            match rustix::fs::mkdirat(
                &directory,
                name,
                rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR | rustix::fs::Mode::XUSR,
            ) {
                Ok(()) => directory.sync_all()?,
                Err(error)
                    if std::io::Error::from_raw_os_error(error.raw_os_error()).kind()
                        == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(std::io::Error::from(error).into()),
            }
            directory = open_private_directory_at(&directory, name, "migration staging child")?;
        }
        let leaf = leaf.ok_or_else(|| failure("migration staging path has no file name"))?;
        Ok((directory, leaf))
    }

    fn publish(
        &self,
        destination_leaf: &OsStr,
        expected: &BTreeMap<PathBuf, StagedFileFingerprint>,
    ) -> std::io::Result<()> {
        self.publish_with_post_rename_hook(destination_leaf, expected, || {})
    }

    fn publish_with_post_rename_hook(
        &self,
        destination_leaf: &OsStr,
        expected: &BTreeMap<PathBuf, StagedFileFingerprint>,
        post_rename: impl FnOnce(),
    ) -> std::io::Result<()> {
        self.assert_parent_path_identity()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        self.verify_exact_inventory(expected)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        assert_directory_entry_identity(&self.parent, &self.leaf, &self.directory)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        rustix::fs::renameat_with(
            &self.parent,
            &self.leaf,
            &self.parent,
            destination_leaf,
            rustix::fs::RenameFlags::NOREPLACE,
        )
        .map_err(std::io::Error::from)?;
        post_rename();
        let published = rustix::fs::statat(
            &self.parent,
            destination_leaf,
            rustix::fs::AtFlags::SYMLINK_NOFOLLOW,
        )
        .map_err(std::io::Error::from)?;
        let retained = rustix::fs::fstat(&self.directory).map_err(std::io::Error::from)?;
        if stat_identity(&published) != stat_identity(&retained) {
            return Err(std::io::Error::other(
                "published migration directory identity changed",
            ));
        }
        self.verify_exact_inventory(expected)
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        self.parent.sync_all()?;
        self.assert_parent_path_identity()
            .map_err(|error| std::io::Error::other(error.to_string()))
    }

    fn assert_parent_path_identity(&self) -> Result<(), ElectronMigrationError> {
        let current = open_private_directory(&self.parent_path, "migration staging parent")?;
        let retained = rustix::fs::fstat(&self.parent).map_err(std::io::Error::from)?;
        let current = rustix::fs::fstat(&current).map_err(std::io::Error::from)?;
        if stat_identity(&retained) != stat_identity(&current) {
            return Err(failure("migration staging parent identity changed"));
        }
        Ok(())
    }
}

#[cfg(unix)]
fn reject_interrupted_stages(parent: &File, prefix: &str) -> Result<(), ElectronMigrationError> {
    let entries = rustix::fs::Dir::read_from(parent).map_err(std::io::Error::from)?;
    for entry in entries {
        let entry = entry.map_err(std::io::Error::from)?;
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        let Ok(name) = std::str::from_utf8(bytes) else {
            continue;
        };
        if !is_stage_leaf(name, prefix) {
            continue;
        }
        return Err(failure(
            "interrupted migration staging directory was preserved because same-user ownership cannot be authenticated",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn is_stage_leaf(name: &str, prefix: &str) -> bool {
    let Some(token) = name
        .strip_prefix(prefix)
        .and_then(|suffix| suffix.strip_suffix(".partial"))
    else {
        return false;
    };
    token.len() == STAGE_RANDOM_BYTES * 2 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(unix)]
fn open_private_directory(path: &Path, label: &str) -> Result<File, ElectronMigrationError> {
    use std::os::unix::fs::OpenOptionsExt as _;
    if !path.is_absolute() {
        return Err(failure(format!("{label} must be absolute")));
    }
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut directory = options.open(Path::new("/"))?;
    for component in path.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(leaf) => {
                directory = File::from(
                    rustix::fs::openat(
                        &directory,
                        leaf,
                        rustix::fs::OFlags::RDONLY
                            | rustix::fs::OFlags::DIRECTORY
                            | rustix::fs::OFlags::NOFOLLOW
                            | rustix::fs::OFlags::CLOEXEC,
                        rustix::fs::Mode::empty(),
                    )
                    .map_err(std::io::Error::from)?,
                );
            }
            Component::ParentDir | Component::Prefix(_) => {
                return Err(failure(format!("{label} contains an unsafe component")));
            }
        }
    }
    validate_private_directory_stat(
        &rustix::fs::fstat(&directory).map_err(std::io::Error::from)?,
        label,
    )?;
    Ok(directory)
}

#[cfg(unix)]
fn verify_existing_directory_at(
    parent_path: &Path,
    leaf: &OsStr,
    label: &str,
    verify: impl FnOnce() -> Result<(), ElectronMigrationError>,
) -> Result<(), ElectronMigrationError> {
    let parent = open_private_directory(parent_path, "migration destination parent")?;
    let directory = open_private_directory_at(&parent, leaf, label)?;
    verify()?;
    let current_parent = open_private_directory(parent_path, "migration destination parent")?;
    let retained_parent = rustix::fs::fstat(&parent).map_err(std::io::Error::from)?;
    let current_parent = rustix::fs::fstat(&current_parent).map_err(std::io::Error::from)?;
    if stat_identity(&retained_parent) != stat_identity(&current_parent) {
        return Err(failure("migration destination parent identity changed"));
    }
    assert_directory_entry_identity(&parent, leaf, &directory)
}

#[cfg(unix)]
fn open_private_directory_at(
    parent: &File,
    leaf: &OsStr,
    label: &str,
) -> Result<File, ElectronMigrationError> {
    let directory = File::from(
        rustix::fs::openat(
            parent,
            leaf,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::DIRECTORY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(std::io::Error::from)?,
    );
    validate_private_directory_stat(
        &rustix::fs::fstat(&directory).map_err(std::io::Error::from)?,
        label,
    )?;
    Ok(directory)
}

#[cfg(unix)]
fn validate_private_directory_stat(
    stat: &rustix::fs::Stat,
    label: &str,
) -> Result<(), ElectronMigrationError> {
    if !rustix::fs::FileType::from_raw_mode(stat.st_mode).is_dir()
        || stat.st_uid != unsafe { libc::geteuid() }
        || rustix::fs::Mode::from_raw_mode(stat.st_mode).bits() & 0o077 != 0
    {
        return Err(failure(format!(
            "{label} must be a private owned directory"
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn write_private_file_at(
    directory: &File,
    leaf: &OsStr,
    bytes: &[u8],
) -> Result<(), ElectronMigrationError> {
    let mut file = File::from(
        rustix::fs::openat(
            directory,
            leaf,
            rustix::fs::OFlags::WRONLY
                | rustix::fs::OFlags::CREATE
                | rustix::fs::OFlags::EXCL
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
        )
        .map_err(std::io::Error::from)?,
    );
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn assert_directory_entry_identity(
    parent: &File,
    leaf: &OsStr,
    directory: &File,
) -> Result<(), ElectronMigrationError> {
    let retained = rustix::fs::fstat(directory).map_err(std::io::Error::from)?;
    validate_private_directory_stat(&retained, "retained migration directory")?;
    let current = rustix::fs::statat(parent, leaf, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
        .map_err(std::io::Error::from)?;
    if stat_identity(&retained) != stat_identity(&current) {
        return Err(failure("migration staging directory identity changed"));
    }
    Ok(())
}

#[cfg(unix)]
fn assert_file_entry_identity(
    parent: &File,
    leaf: &OsStr,
    file: &File,
) -> Result<(), ElectronMigrationError> {
    let retained = rustix::fs::fstat(file).map_err(std::io::Error::from)?;
    let current = rustix::fs::statat(parent, leaf, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
        .map_err(std::io::Error::from)?;
    if stat_identity(&retained) != stat_identity(&current)
        || !rustix::fs::FileType::from_raw_mode(retained.st_mode).is_file()
        || retained.st_uid != unsafe { libc::geteuid() }
        || retained.st_nlink != 1
    {
        return Err(failure("migration staging file identity changed"));
    }
    Ok(())
}

#[cfg(unix)]
fn stat_identity(stat: &rustix::fs::Stat) -> (u64, u64) {
    (stat.st_dev as u64, stat.st_ino as u64)
}

#[cfg(unix)]
fn staged_fingerprint(bytes: &[u8]) -> StagedFileFingerprint {
    StagedFileFingerprint {
        bytes: bytes.len() as u64,
        sha256: digest(bytes),
    }
}

#[cfg(unix)]
fn collect_stage_inventory_at(
    directory: &File,
    prefix: &Path,
    files: &mut BTreeMap<PathBuf, StagedFileFingerprint>,
    directories: &mut BTreeSet<PathBuf>,
) -> Result<(), ElectronMigrationError> {
    use std::os::unix::ffi::OsStrExt as _;
    let entries = rustix::fs::Dir::read_from(directory).map_err(std::io::Error::from)?;
    for entry in entries {
        let entry = entry.map_err(std::io::Error::from)?;
        let bytes = entry.file_name().to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        let leaf = OsStr::from_bytes(bytes);
        let relative = prefix.join(leaf);
        let stat = rustix::fs::statat(directory, leaf, rustix::fs::AtFlags::SYMLINK_NOFOLLOW)
            .map_err(std::io::Error::from)?;
        let file_type = rustix::fs::FileType::from_raw_mode(stat.st_mode);
        if file_type.is_dir() {
            let child = open_private_directory_at(directory, leaf, "migration staging child")?;
            assert_directory_entry_identity(directory, leaf, &child)?;
            directories.insert(relative.clone());
            collect_stage_inventory_at(&child, &relative, files, directories)?;
            assert_directory_entry_identity(directory, leaf, &child)?;
        } else if file_type.is_file()
            && stat.st_uid == unsafe { libc::geteuid() }
            && stat.st_nlink == 1
            && stat.st_size >= 0
        {
            let mut file = File::from(
                rustix::fs::openat(
                    directory,
                    leaf,
                    rustix::fs::OFlags::RDONLY
                        | rustix::fs::OFlags::NOFOLLOW
                        | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )
                .map_err(std::io::Error::from)?,
            );
            assert_file_entry_identity(directory, leaf, &file)?;
            let mut hasher = Sha256::new();
            let mut total = 0_u64;
            let mut buffer = [0_u8; 64 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                total = total
                    .checked_add(read as u64)
                    .ok_or_else(|| failure("migration staging file size overflowed"))?;
                hasher.update(&buffer[..read]);
            }
            assert_file_entry_identity(directory, leaf, &file)?;
            files.insert(
                relative,
                StagedFileFingerprint {
                    bytes: total,
                    sha256: format!("{:x}", hasher.finalize()),
                },
            );
        } else {
            return Err(failure("migration staging tree contains an unsafe entry"));
        }
    }
    directory.sync_all()?;
    Ok(())
}

fn receipt_bytes(receipt: &BackupReceipt) -> Result<Vec<u8>, ElectronMigrationError> {
    let mut bytes = serde_json::to_vec_pretty(receipt)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn native_receipt_bytes(
    receipt: &NativeMigrationReceipt,
) -> Result<Vec<u8>, ElectronMigrationError> {
    let mut bytes = serde_json::to_vec_pretty(receipt)?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(not(unix))]
fn sync_tree_directories(root: &Path) -> Result<(), ElectronMigrationError> {
    let mut directories = Vec::new();
    collect_safe_directories(root, &mut directories)?;
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        sync_directory(&directory)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn collect_safe_directories(
    directory: &Path,
    directories: &mut Vec<PathBuf>,
) -> Result<(), ElectronMigrationError> {
    validate_directory(directory, "migration-owned directory")?;
    directories.push(directory.to_path_buf());
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(failure("migration-owned tree contains a symlink"));
        }
        if metadata.is_dir() {
            collect_safe_directories(&entry.path(), directories)?;
        } else if metadata.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                if metadata.nlink() != 1 || metadata.uid() != unsafe { libc::geteuid() } {
                    return Err(failure(
                        "migration-owned tree contains an unsafe regular file",
                    ));
                }
            }
        } else {
            return Err(failure("migration-owned tree contains a special file"));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory(path: &Path) -> Result<(), ElectronMigrationError> {
    #[cfg(windows)]
    {
        // Windows does not document FlushFileBuffers for directory handles;
        // Rust's File::sync_all maps to that API and fails with ERROR_INVALID_FUNCTION
        // or ERROR_ACCESS_DENIED. File payloads are flushed individually, while
        // directory publication uses MOVEFILE_WRITE_THROUGH below. Validate that
        // this still names a real directory, but directory-entry flushing itself
        // is best-effort on Windows.
        validate_directory(path, "migration directory to synchronise")
    }
    #[cfg(not(windows))]
    {
        File::open(path)?.sync_all()?;
        Ok(())
    }
}

#[cfg(windows)]
fn windows_move_write_through(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt as _;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};

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
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn failure(message: impl Into<String>) -> ElectronMigrationError {
    ElectronMigrationError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn root() -> PathBuf {
        std::env::temp_dir().canonicalize().unwrap().join(format!(
            "bp-electron-migration-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[cfg(unix)]
    fn only_stage(parent: &Path, prefix: &str) -> PathBuf {
        let mut stages = fs::read_dir(parent)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                let name = path.file_name().unwrap().to_string_lossy();
                name.starts_with(prefix) && name.ends_with(".partial")
            })
            .collect::<Vec<_>>();
        assert_eq!(stages.len(), 1, "expected one interrupted stage");
        stages.pop().unwrap()
    }

    #[cfg(unix)]
    fn expected_sentinel(leaf: &str, bytes: &[u8]) -> BTreeMap<PathBuf, StagedFileFingerprint> {
        BTreeMap::from([(PathBuf::from(leaf), staged_fingerprint(bytes))])
    }

    fn export_fixture(channel: &str) -> (PathBuf, Vec<u8>) {
        let root = root();
        let source = crate::generated_document::GeneratedDocumentRequest::a3_landscape_blank()
            .to_pdf_bytes()
            .unwrap();
        let source_path =
            root.join("templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf");
        fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        fs::write(&source_path, &source).unwrap();
        let manifest = json!({
            "schema": SCHEMA,
            "version": 1,
            "channel": channel,
            "exportId": "a".repeat(64),
            "createdAt": "2026-09-28T00:00:00.000Z",
            "source": {
                "productName": if channel == "stable" { "Butter Paper" } else { "Butter Paper Beta" },
                "bundleIdentifier": if channel == "stable" { "com.butterpaper.desktop" } else { "com.butterpaper.desktop.beta" },
                "version": "0.0.26"
            },
            "preferences": {
                "menuBarVisible": false,
                "updateFrequency": "sixHours",
                "lastSuccessfulUpdateCheckAt": "2026-09-27T18:00:00.000Z"
            },
            "templates": {
                "lastTemplateId": "imported-01234567-89ab-cdef-0123-456789abcdef",
                "generated": [{
                    "id": "custom-site-grid",
                    "name": "Site grid",
                    "title": "Untitled",
                    "widthMm": 420.0,
                    "heightMm": 297.0,
                    "pattern": { "kind": "grid", "spacingMm": 10.0, "color": "#d1d5db" }
                }],
                "imported": [{
                    "id": "imported-01234567-89ab-cdef-0123-456789abcdef",
                    "name": "Title block",
                    "createdAt": "2026-09-27T00:00:00.000Z",
                    "pageCount": 1,
                    "source": {
                        "path": "templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf",
                        "bytes": source.len(),
                        "sha256": digest(&source)
                    }
                }]
            },
            "unsupported": {
                "recentSignatures": "requires-secure-bridge",
                "session": "not-persisted-by-electron",
                "colourPresets": "not-imported-v1"
            }
        });
        fs::write(
            root.join(MANIFEST_FILE),
            format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()),
        )
        .unwrap();
        (root, source)
    }

    #[test]
    fn validates_exact_channel_bound_export_and_publishes_idempotent_backup() {
        let (exchange, source) = export_fixture("stable");
        let validated =
            ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).unwrap();
        assert_eq!(validated.export_id(), "a".repeat(64));
        assert!(!validated.menu_bar_visible());
        assert_eq!(validated.generated_template_count(), 1);
        assert_eq!(validated.imported_template_count(), 1);
        let backup_root = root().join("backups");
        let source_path =
            exchange.join("templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf");
        fs::write(&source_path, b"changed after validation").unwrap();
        assert!(validated.publish_backup(&backup_root).is_err());
        fs::write(&source_path, &source).unwrap();
        #[cfg(unix)]
        {
            let stage_prefix = format!(".backup-{}-", validated.manifest_sha256());
            let partial = only_stage(&backup_root, &stage_prefix);
            fs::write(partial.join("interrupted"), b"partial").unwrap();
            let error = validated.publish_backup(&backup_root).unwrap_err();
            assert!(error.to_string().contains("was preserved"));
            assert_eq!(fs::read(partial.join("interrupted")).unwrap(), b"partial");
            fs::remove_dir_all(&partial).unwrap();
            assert_eq!(
                validated.publish_backup(&backup_root).unwrap(),
                BackupPublication::Created
            );
        }
        #[cfg(not(unix))]
        {
            let partial =
                backup_root.join(format!(".backup-{}-partial", validated.manifest_sha256()));
            fs::write(partial.join("interrupted"), b"partial").unwrap();
            let error = validated.publish_backup(&backup_root).unwrap_err();
            assert!(error.to_string().contains("was preserved"));
            assert_eq!(fs::read(partial.join("interrupted")).unwrap(), b"partial");
            fs::remove_dir_all(&partial).unwrap();
            assert_eq!(
                validated.publish_backup(&backup_root).unwrap(),
                BackupPublication::Created
            );
        }
        assert_eq!(
            validated.publish_backup(&backup_root).unwrap(),
            BackupPublication::AlreadyPresent
        );
        let backup = backup_root.join(validated.manifest_sha256());
        assert_eq!(
            fs::read(
                backup.join("templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf")
            )
            .unwrap(),
            source
        );
        fs::remove_dir_all(exchange).unwrap();
        fs::remove_dir_all(backup_root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rejects_replaced_backup_source_before_pdf_import() {
        let (exchange, _) = export_fixture("stable");
        let validated =
            ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).unwrap();
        let backup_root = root().join("backups");
        validated.publish_backup(&backup_root).unwrap();
        let backup = backup_root.join(validated.manifest_sha256());
        let source = validated.sources.first().unwrap();
        fs::write(
            backup.join(&source.relative_path),
            vec![0; source.bytes as usize],
        )
        .unwrap();

        let error = read_verified_backup_source(&backup, source).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("migration backup source changed before import")
        );

        fs::remove_dir_all(exchange).unwrap();
        fs::remove_dir_all(backup_root.parent().unwrap()).unwrap();
    }

    #[test]
    fn rejects_channel_schema_unknown_fields_and_unacknowledged_data() {
        let (exchange, _) = export_fixture("beta");
        assert!(
            ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable)
                .unwrap_err()
                .to_string()
                .contains("channel")
        );
        let path = exchange.join(MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["channel"] = json!("stable");
        manifest["source"]["productName"] = json!("Butter Paper");
        manifest["source"]["bundleIdentifier"] = json!("com.butterpaper.desktop");
        manifest["unexpected"] = json!(true);
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).is_err());
        manifest.as_object_mut().unwrap().remove("unexpected");
        manifest["unsupported"]["recentSignatures"] = json!("copied");
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).is_err());
        fs::remove_dir_all(exchange).unwrap();
    }

    #[test]
    fn atomically_applies_preferences_and_templates_from_backup_then_reopens_as_noop() {
        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        let (fixture, _) = export_fixture("stable");
        fs::create_dir_all(storage.migration_exchange_root().parent().unwrap()).unwrap();
        fs::rename(&fixture, storage.migration_exchange_root()).unwrap();
        let validated = ValidatedElectronExport::load(
            storage.migration_exchange_root(),
            NativeReleaseChannel::Stable,
        )
        .unwrap();
        assert_eq!(
            validated
                .publish_backup(storage.migration_backup_root())
                .unwrap(),
            BackupPublication::Created
        );
        let parent = storage.layout().durable_root().parent().unwrap();
        #[cfg(unix)]
        let stage = parent.join(format!(
            ".native-v1-migration-{}-{}.partial",
            validated.manifest_sha256(),
            "b".repeat(STAGE_RANDOM_BYTES * 2)
        ));
        #[cfg(not(unix))]
        let stage = parent.join(format!(
            ".native-v1-migration-{}-partial",
            validated.manifest_sha256()
        ));
        create_private_directory(&stage).unwrap();
        fs::write(
            stage.join(NATIVE_STAGE_SENTINEL),
            format!("{}\n", validated.manifest_sha256()),
        )
        .unwrap();
        fs::write(stage.join("interrupted"), b"partial").unwrap();
        #[cfg(unix)]
        {
            let error = validated.publish_native(&storage).unwrap_err();
            assert!(error.to_string().contains("was preserved"));
            assert_eq!(fs::read(stage.join("interrupted")).unwrap(), b"partial");
            fs::remove_dir_all(&stage).unwrap();
            assert_eq!(
                validated.publish_native(&storage).unwrap(),
                NativePublication::Created
            );
        }
        #[cfg(not(unix))]
        {
            let error = validated.publish_native(&storage).unwrap_err();
            assert!(error.to_string().contains("was preserved"));
            assert_eq!(fs::read(stage.join("interrupted")).unwrap(), b"partial");
            fs::remove_dir_all(&stage).unwrap();
            assert_eq!(
                validated.publish_native(&storage).unwrap(),
                NativePublication::Created
            );
        }
        assert_eq!(
            validated.publish_native(&storage).unwrap(),
            NativePublication::AlreadyPresent
        );
        let preferences = ApplicationShellPreferencesStore::new(storage.layout().durable_root())
            .load()
            .unwrap();
        assert!(!preferences.menu_bar_visible());
        let update_settings = UpdateSettingsStore::new(
            storage.layout().durable_root(),
            NativeReleaseChannel::Stable,
        )
        .load()
        .unwrap();
        assert_eq!(update_settings.frequency(), UpdateFrequency::SixHours);
        assert_eq!(
            update_settings.last_successful_check_at(),
            Some("2026-09-27T18:00:00.000Z")
        );
        let templates = TemplateLibrary::open(storage.layout().template_library_root()).unwrap();
        assert_eq!(
            templates.record_ids(),
            vec![
                "custom-site-grid",
                "imported-01234567-89ab-cdef-0123-456789abcdef"
            ]
        );
        assert_eq!(
            templates.last_template_id(),
            "imported-01234567-89ab-cdef-0123-456789abcdef"
        );
        assert!(storage.migration_exchange_root().exists());
        assert!(
            storage
                .migration_backup_root()
                .join(validated.manifest_sha256())
                .exists()
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn refuses_to_overwrite_an_unreceipted_native_destination() {
        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        let (fixture, _) = export_fixture("stable");
        fs::create_dir_all(storage.migration_exchange_root().parent().unwrap()).unwrap();
        fs::rename(&fixture, storage.migration_exchange_root()).unwrap();
        let validated = ValidatedElectronExport::load(
            storage.migration_exchange_root(),
            NativeReleaseChannel::Stable,
        )
        .unwrap();
        validated
            .publish_backup(storage.migration_backup_root())
            .unwrap();
        fs::create_dir_all(storage.layout().durable_root()).unwrap();
        let marker = storage.layout().durable_root().join("existing-data");
        fs::write(&marker, b"preserve").unwrap();
        assert!(validated.publish_native(&storage).is_err());
        assert_eq!(fs::read(&marker).unwrap(), b"preserve");
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn rejects_source_tampering_extra_entries_and_unsafe_source_paths() {
        let (exchange, _) = export_fixture("stable");
        let source =
            exchange.join("templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf");
        fs::write(&source, b"changed").unwrap();
        assert!(ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).is_err());
        let (exchange_extra, _) = export_fixture("stable");
        fs::write(exchange_extra.join("extra"), b"unexpected").unwrap();
        assert!(
            ValidatedElectronExport::load(&exchange_extra, NativeReleaseChannel::Stable).is_err()
        );
        let (exchange_path, _) = export_fixture("stable");
        let path = exchange_path.join(MANIFEST_FILE);
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest["templates"]["imported"][0]["source"]["path"] = json!("../source.pdf");
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert!(
            ValidatedElectronExport::load(&exchange_path, NativeReleaseChannel::Stable).is_err()
        );
        for root in [exchange, exchange_extra, exchange_path] {
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_and_hard_link_sources() {
        use std::os::unix::fs::symlink;
        let (exchange, source_bytes) = export_fixture("stable");
        let source =
            exchange.join("templates/imported-01234567-89ab-cdef-0123-456789abcdef/source.pdf");
        let outside = root();
        fs::write(&outside, &source_bytes).unwrap();
        fs::remove_file(&source).unwrap();
        symlink(&outside, &source).unwrap();
        assert!(ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).is_err());
        fs::remove_file(&source).unwrap();
        fs::hard_link(&outside, &source).unwrap();
        assert!(ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).is_err());
        fs::remove_dir_all(exchange).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[test]
    fn rejects_drifted_existing_backup() {
        let (exchange, _) = export_fixture("stable");
        let validated =
            ValidatedElectronExport::load(&exchange, NativeReleaseChannel::Stable).unwrap();
        let backup_root = root().join("backups");
        validated.publish_backup(&backup_root).unwrap();
        fs::write(
            backup_root
                .join(validated.manifest_sha256())
                .join(RECEIPT_FILE),
            b"{}",
        )
        .unwrap();
        assert!(validated.publish_backup(&backup_root).is_err());
        fs::remove_dir_all(exchange).unwrap();
        fs::remove_dir_all(backup_root.parent().unwrap()).unwrap();
    }

    #[test]
    fn startup_orchestration_reports_absence_then_backs_up_and_publishes_once() {
        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        assert_eq!(
            publish_startup_migration(&storage).unwrap(),
            StartupMigrationPublication::NotPresent {
                legacy_development_store_present: false,
            }
        );

        fs::create_dir_all(storage.legacy_development_root()).unwrap();
        let (fixture, _) = export_fixture("stable");
        fs::create_dir_all(storage.migration_exchange_root().parent().unwrap()).unwrap();
        fs::rename(fixture, storage.migration_exchange_root()).unwrap();
        assert_eq!(
            publish_startup_migration(&storage).unwrap(),
            StartupMigrationPublication::Published {
                backup: BackupPublication::Created,
                native: NativePublication::Created,
                legacy_development_store_present: true,
            }
        );
        assert_eq!(
            publish_startup_migration(&storage).unwrap(),
            StartupMigrationPublication::Published {
                backup: BackupPublication::AlreadyPresent,
                native: NativePublication::AlreadyPresent,
                legacy_development_store_present: true,
            }
        );
        assert!(storage.legacy_development_root().is_dir());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn startup_refuses_an_existing_electron_profile_without_an_export() {
        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        fs::create_dir_all(storage.electron_user_data_root()).unwrap();

        let error = publish_startup_migration(&storage).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the Electron predecessor profile exists but its GPUI migration export is unavailable"
        );
        assert!(!storage.layout().durable_root().exists());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn native_only_startup_ignores_and_preserves_predecessor_paths() {
        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        let electron_marker = storage
            .electron_user_data_root()
            .join("legacy-electron-state");
        let exchange_marker = storage.migration_exchange_root().join("untrusted-export");
        let development_marker = storage
            .legacy_development_root()
            .join("legacy-development-state");
        for (path, bytes) in [
            (&electron_marker, b"electron".as_slice()),
            (&exchange_marker, b"exchange".as_slice()),
            (&development_marker, b"development".as_slice()),
        ] {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }

        assert_eq!(
            prepare_startup_data(&storage, StartupDataPolicy::NativeOnly).unwrap(),
            StartupDataPreparation::NativeOnly {
                native_root_present: false,
            }
        );
        assert!(!storage.layout().durable_root().exists());
        assert_eq!(fs::read(electron_marker).unwrap(), b"electron");
        assert_eq!(fs::read(exchange_marker).unwrap(), b"exchange");
        assert_eq!(fs::read(development_marker).unwrap(), b"development");

        fs::create_dir_all(storage.layout().durable_root()).unwrap();
        fs::write(
            storage.layout().durable_root().join("native-state"),
            b"native",
        )
        .unwrap();
        assert_eq!(
            prepare_startup_data(&storage, StartupDataPolicy::NativeOnly).unwrap(),
            StartupDataPreparation::NativeOnly {
                native_root_present: true,
            }
        );
        assert_eq!(
            fs::read(storage.layout().durable_root().join("native-state")).unwrap(),
            b"native"
        );
        fs::remove_dir_all(home).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn native_only_startup_rejects_non_directory_and_symlink_roots_without_touching_targets() {
        use std::os::unix::fs::symlink;

        let file_home = root();
        let file_storage =
            NativeProductionStorage::macos(&file_home, NativeReleaseChannel::Stable).unwrap();
        fs::create_dir_all(file_storage.layout().durable_root().parent().unwrap()).unwrap();
        fs::write(
            file_storage.layout().durable_root(),
            b"partial-native-state",
        )
        .unwrap();
        assert!(prepare_startup_data(&file_storage, StartupDataPolicy::NativeOnly).is_err());
        assert_eq!(
            fs::read(file_storage.layout().durable_root()).unwrap(),
            b"partial-native-state"
        );
        fs::remove_dir_all(file_home).unwrap();

        let link_home = root();
        let link_storage =
            NativeProductionStorage::macos(&link_home, NativeReleaseChannel::Beta).unwrap();
        let target = root();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("marker"), b"outside").unwrap();
        fs::create_dir_all(link_storage.layout().durable_root().parent().unwrap()).unwrap();
        symlink(&target, link_storage.layout().durable_root()).unwrap();
        assert!(prepare_startup_data(&link_storage, StartupDataPolicy::NativeOnly).is_err());
        assert_eq!(fs::read(target.join("marker")).unwrap(), b"outside");
        fs::remove_dir_all(link_home).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn native_only_startup_rejects_a_symlinked_native_parent_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        let native_parent = storage.layout().durable_root().parent().unwrap();
        let target = root();
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("marker"), b"outside").unwrap();
        fs::create_dir_all(native_parent.parent().unwrap()).unwrap();
        symlink(&target, native_parent).unwrap();

        assert!(prepare_startup_data(&storage, StartupDataPolicy::NativeOnly).is_err());
        assert_eq!(fs::read(target.join("marker")).unwrap(), b"outside");
        assert!(!target.join("native-v1").exists());
        fs::remove_dir_all(home).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn predecessor_import_requires_the_explicit_import_policy() {
        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        fs::create_dir_all(storage.electron_user_data_root()).unwrap();

        assert!(matches!(
            prepare_startup_data(&storage, StartupDataPolicy::NativeOnly).unwrap(),
            StartupDataPreparation::NativeOnly { .. }
        ));
        assert!(prepare_startup_data(&storage, StartupDataPolicy::ImportElectronV1).is_err());
        assert!(!storage.layout().durable_root().exists());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn startup_revalidates_the_native_receipt_and_backup_when_export_is_absent() {
        let home = root();
        let storage = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        let (fixture, _) = export_fixture("stable");
        fs::create_dir_all(storage.migration_exchange_root().parent().unwrap()).unwrap();
        fs::rename(fixture, storage.migration_exchange_root()).unwrap();
        assert!(matches!(
            publish_startup_migration(&storage).unwrap(),
            StartupMigrationPublication::Published {
                backup: BackupPublication::Created,
                native: NativePublication::Created,
                ..
            }
        ));

        fs::remove_dir_all(storage.migration_exchange_root()).unwrap();
        assert_eq!(
            publish_startup_migration(&storage).unwrap(),
            StartupMigrationPublication::Published {
                backup: BackupPublication::AlreadyPresent,
                native: NativePublication::AlreadyPresent,
                legacy_development_store_present: false,
            }
        );

        fs::write(
            storage.layout().durable_root().join(NATIVE_RECEIPT_FILE),
            b"{}",
        )
        .unwrap();
        assert!(publish_startup_migration(&storage).is_err());
        fs::remove_dir_all(home).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn retained_stage_rejects_leaf_replacement_without_publishing_or_deleting_it() {
        let parent = root();
        create_private_directory(&parent).unwrap();
        let stage = RetainedStage::create(&parent, ".test-stage-", "sentinel", b"owned\n").unwrap();
        let original = stage.path();
        let held = parent.join("held-original");
        fs::rename(&original, &held).unwrap();
        create_private_directory(&original).unwrap();
        fs::write(original.join("replacement-marker"), b"keep").unwrap();
        let mut expected = expected_sentinel("sentinel", b"owned\n");
        stage
            .write_file(Path::new("capability-marker"), b"owned", &mut expected)
            .unwrap();

        let error = stage
            .publish(OsStr::new("destination"), &expected)
            .unwrap_err();
        assert!(error.to_string().contains("identity changed"));
        assert_eq!(
            fs::read(original.join("replacement-marker")).unwrap(),
            b"keep"
        );
        assert!(!original.join("capability-marker").exists());
        assert_eq!(fs::read(held.join("capability-marker")).unwrap(), b"owned");
        assert!(!parent.join("destination").exists());
        assert!(held.join("sentinel").is_file());
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn retained_stage_rejects_parent_replacement_without_publishing() {
        let parent = root();
        create_private_directory(&parent).unwrap();
        let stage = RetainedStage::create(&parent, ".test-stage-", "sentinel", b"owned\n").unwrap();
        let moved = parent.with_extension("moved");
        fs::rename(&parent, &moved).unwrap();
        create_private_directory(&parent).unwrap();
        fs::write(parent.join("replacement-marker"), b"keep").unwrap();

        let expected = expected_sentinel("sentinel", b"owned\n");
        let error = stage
            .publish(OsStr::new("destination"), &expected)
            .unwrap_err();
        assert!(error.to_string().contains("parent identity changed"));
        assert_eq!(
            fs::read(parent.join("replacement-marker")).unwrap(),
            b"keep"
        );
        assert!(!parent.join("destination").exists());
        assert!(!moved.join("destination").exists());
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(moved).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_parent_open_rejects_an_intermediate_symlink() {
        use std::os::unix::fs::symlink;

        let base = root();
        create_private_directory(&base).unwrap();
        let real = base.join("real");
        create_private_directory(&real).unwrap();
        let target = real.join("target");
        create_private_directory(&target).unwrap();
        symlink(&real, base.join("redirect")).unwrap();

        let error =
            open_private_directory(&base.join("redirect/target"), "test parent").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("symbolic link") || message.contains("Not a directory"));
        assert!(target.is_dir());
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn retained_stage_reports_parent_replacement_after_rename_as_failure() {
        let parent = root();
        create_private_directory(&parent).unwrap();
        let stage = RetainedStage::create(&parent, ".test-stage-", "sentinel", b"owned\n").unwrap();
        let moved = parent.with_extension("moved-after-rename");
        let expected = expected_sentinel("sentinel", b"owned\n");
        let error = stage
            .publish_with_post_rename_hook(OsStr::new("destination"), &expected, || {
                fs::rename(&parent, &moved).unwrap();
                create_private_directory(&parent).unwrap();
            })
            .unwrap_err();

        assert!(error.to_string().contains("parent identity changed"));
        assert!(!parent.join("destination").exists());
        assert!(moved.join("destination").is_dir());
        fs::remove_dir_all(parent).unwrap();
        fs::remove_dir_all(moved).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn retained_stage_never_replaces_an_existing_destination() {
        let parent = root();
        create_private_directory(&parent).unwrap();
        let stage = RetainedStage::create(&parent, ".test-stage-", "sentinel", b"owned\n").unwrap();
        let destination = parent.join("destination");
        create_private_directory(&destination).unwrap();
        fs::write(destination.join("marker"), b"keep").unwrap();

        let expected = expected_sentinel("sentinel", b"owned\n");
        let error = stage
            .publish(OsStr::new("destination"), &expected)
            .unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(destination.join("marker")).unwrap(), b"keep");
        assert!(stage.path().is_dir());
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn existing_destination_verification_rejects_a_replaced_directory() {
        let parent = root();
        create_private_directory(&parent).unwrap();
        let destination = parent.join("destination");
        create_private_directory(&destination).unwrap();
        fs::write(destination.join("original"), b"owned").unwrap();
        let held = parent.join("held-destination");

        let error = verify_existing_directory_at(
            &parent,
            OsStr::new("destination"),
            "test destination",
            || {
                fs::rename(&destination, &held)?;
                create_private_directory(&destination)?;
                fs::write(destination.join("replacement"), b"keep")?;
                Ok(())
            },
        )
        .unwrap_err();

        assert!(error.to_string().contains("identity changed"));
        assert_eq!(fs::read(destination.join("replacement")).unwrap(), b"keep");
        assert_eq!(fs::read(held.join("original")).unwrap(), b"owned");
        fs::remove_dir_all(parent).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn recovery_preserves_every_preexisting_stage_without_inspecting_its_tree() {
        use std::os::unix::fs::symlink;

        let parent = root();
        create_private_directory(&parent).unwrap();
        let prefix = ".test-stage-";
        let interrupted = parent.join(format!(
            "{prefix}{}.partial",
            "a".repeat(STAGE_RANDOM_BYTES * 2)
        ));
        create_private_directory(&interrupted).unwrap();
        fs::write(interrupted.join("sentinel"), b"owned\n").unwrap();
        let outside = parent.with_extension("outside");
        fs::write(&outside, b"outside").unwrap();
        symlink(&outside, interrupted.join("unsafe-link")).unwrap();

        let error = RetainedStage::create(&parent, prefix, "sentinel", b"owned\n").unwrap_err();
        assert!(
            error
                .to_string()
                .contains("same-user ownership cannot be authenticated")
        );
        assert_eq!(fs::read(&outside).unwrap(), b"outside");
        assert!(interrupted.join("sentinel").is_file());
        assert!(
            fs::symlink_metadata(interrupted.join("unsafe-link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_dir_all(parent).unwrap();
        fs::remove_file(outside).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn publication_reports_post_rename_byte_mutation_as_failure() {
        let parent = root();
        create_private_directory(&parent).unwrap();
        let stage = RetainedStage::create(&parent, ".test-stage-", "sentinel", b"owned\n").unwrap();
        let mut expected = expected_sentinel("sentinel", b"owned\n");
        stage
            .write_file(Path::new("payload"), b"verified", &mut expected)
            .unwrap();

        let error = stage
            .publish_with_post_rename_hook(OsStr::new("destination"), &expected, || {
                fs::write(parent.join("destination/payload"), b"changed").unwrap();
            })
            .unwrap_err();

        assert!(error.to_string().contains("inventory changed"));
        assert_eq!(
            fs::read(parent.join("destination/payload")).unwrap(),
            b"changed"
        );
        fs::remove_dir_all(parent).unwrap();
    }
}
