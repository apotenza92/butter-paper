//! Storage ownership for the development application and isolated benchmarks.
//!
//! Durable stores never fall back to the OS temporary directory. This does not
//! migrate Electron data or the former `butter-paper-document-workspace` temp
//! tree: those locations remain untouched pending a validated import workflow.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeReleaseChannel {
    Stable,
    Beta,
}

impl NativeReleaseChannel {
    pub fn product_name(self) -> &'static str {
        match self {
            Self::Stable => "Butter Paper",
            Self::Beta => "Butter Paper Beta",
        }
    }

    pub fn bundle_identifier(self) -> &'static str {
        match self {
            Self::Stable => "com.butterpaper.desktop",
            Self::Beta => "com.butterpaper.desktop.beta",
        }
    }

    /// Service name for native recent-signature encryption material.
    ///
    /// Electron `safeStorage` ciphertext is deliberately not compatible with
    /// this store. Recent signatures remain blocked until a separately
    /// reviewed bridge can decrypt and re-encrypt them without sharing either
    /// channel's native Keychain item.
    pub fn signature_keychain_service(self) -> &'static str {
        match self {
            Self::Stable => "com.butterpaper.desktop.recent-signatures",
            Self::Beta => "com.butterpaper.desktop.beta.recent-signatures",
        }
    }
}

/// Channel-specific roots for a production Electron-to-native transition.
///
/// Native durable state is deliberately keyed by bundle identifier rather than
/// pointed at Electron's product-name directory. The only native reader of the
/// Electron tree will be the versioned migration importer, and it receives the
/// narrow exchange directory rather than the whole legacy store.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeProductionStorage {
    channel: NativeReleaseChannel,
    layout: NativeStorageLayout,
    electron_user_data_root: PathBuf,
    legacy_development_root: PathBuf,
    migration_exchange_root: PathBuf,
    migration_backup_root: PathBuf,
}

impl NativeProductionStorage {
    pub fn macos(home: &Path, channel: NativeReleaseChannel) -> Result<Self, &'static str> {
        validate_production_root(
            home,
            "the production home directory must be absolute and canonical-looking",
        )?;
        let application_support = home.join("Library/Application Support");
        let electron_user_data_root = application_support.join(channel.product_name());
        let native_application_root = application_support.join(channel.bundle_identifier());
        let surface_root = home
            .join("Library/Caches")
            .join(channel.bundle_identifier())
            .join("native-v1/render-surfaces");
        Self::from_platform_roots(
            channel,
            electron_user_data_root,
            application_support.join("GPUI Migration"),
            native_application_root,
            surface_root,
        )
    }

    /// Resolve production storage under the Windows roaming/local application
    /// data roots supplied by the platform directory resolver.
    pub fn windows(
        roaming_application_data: &Path,
        local_application_data: &Path,
        channel: NativeReleaseChannel,
    ) -> Result<Self, &'static str> {
        validate_production_root(
            roaming_application_data,
            "the roaming application data directory must be absolute and canonical-looking",
        )?;
        validate_production_root(
            local_application_data,
            "the local application data directory must be absolute and canonical-looking",
        )?;
        let native_application_root = roaming_application_data.join(channel.bundle_identifier());
        let surface_root = local_application_data
            .join(channel.bundle_identifier())
            .join("native-v1/render-surfaces");
        Self::from_platform_roots(
            channel,
            roaming_application_data.join(channel.product_name()),
            roaming_application_data.join("GPUI Migration"),
            native_application_root,
            surface_root,
        )
    }

    /// Resolve production storage under freedesktop/XDG data, cache and
    /// configuration roots. Electron's predecessor lives under the config
    /// root; native state deliberately uses the data and cache roots instead.
    pub fn linux(
        data_home: &Path,
        cache_home: &Path,
        config_home: &Path,
        channel: NativeReleaseChannel,
    ) -> Result<Self, &'static str> {
        validate_production_root(
            data_home,
            "the XDG data directory must be absolute and canonical-looking",
        )?;
        validate_production_root(
            cache_home,
            "the XDG cache directory must be absolute and canonical-looking",
        )?;
        validate_production_root(
            config_home,
            "the XDG configuration directory must be absolute and canonical-looking",
        )?;
        let native_application_root = data_home.join(channel.bundle_identifier());
        let surface_root = cache_home
            .join(channel.bundle_identifier())
            .join("native-v1/render-surfaces");
        Self::from_platform_roots(
            channel,
            config_home.join(channel.product_name()),
            data_home.join("GPUI Migration"),
            native_application_root,
            surface_root,
        )
    }

    fn from_platform_roots(
        channel: NativeReleaseChannel,
        electron_user_data_root: PathBuf,
        legacy_development_root: PathBuf,
        native_application_root: PathBuf,
        surface_root: PathBuf,
    ) -> Result<Self, &'static str> {
        let native_root = native_application_root.join("native-v1");
        let layout = NativeStorageLayout::development(Some(native_root), surface_root)?;
        Ok(Self {
            channel,
            migration_exchange_root: electron_user_data_root.join("gpui-migration-export/v1"),
            // Keep immutable predecessor backups outside the atomically
            // published native-v1 destination so a migration can back up its
            // complete input before that destination exists.
            migration_backup_root: native_application_root.join("migration-backups/v1"),
            electron_user_data_root,
            legacy_development_root,
            layout,
        })
    }

    pub fn layout(&self) -> &NativeStorageLayout {
        &self.layout
    }

    pub fn channel(&self) -> NativeReleaseChannel {
        self.channel
    }

    pub fn electron_user_data_root(&self) -> &Path {
        &self.electron_user_data_root
    }

    pub fn migration_exchange_root(&self) -> &Path {
        &self.migration_exchange_root
    }

    pub fn legacy_development_root(&self) -> &Path {
        &self.legacy_development_root
    }

    /// Detects the former channel-neutral development store without adopting
    /// or mutating it. Production migration must ask for an explicit policy
    /// before any legacy GPUI data can enter a stable or beta destination.
    pub fn legacy_development_store_present(&self) -> std::io::Result<bool> {
        let metadata = match std::fs::symlink_metadata(&self.legacy_development_root) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(error),
        };
        if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "legacy GPUI migration store must be a real directory",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            if metadata.uid() != unsafe { libc::geteuid() } {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "legacy GPUI migration store belongs to another user",
                ));
            }
        }
        Ok(true)
    }

    pub fn migration_backup_root(&self) -> &Path {
        &self.migration_backup_root
    }
}

fn validate_production_root(path: &Path, error: &'static str) -> Result<(), &'static str> {
    if path.is_absolute()
        && !path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return Ok(());
    }
    Err(error)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeStorageLayout {
    durable_root: PathBuf,
    surface_root: PathBuf,
    preferences_root: PathBuf,
}

impl NativeStorageLayout {
    pub fn development(
        durable_root: Option<PathBuf>,
        surface_root: PathBuf,
    ) -> Result<Self, &'static str> {
        let durable_root =
            durable_root.ok_or("the application data directory is unavailable or invalid")?;
        if !durable_root.is_absolute() || !surface_root.is_absolute() {
            return Err("application data and surface cache directories must be absolute");
        }
        if [&durable_root, &surface_root].iter().any(|path| {
            path.components()
                .any(|component| component == std::path::Component::ParentDir)
        }) {
            return Err("application storage paths must not contain parent-directory components");
        }
        // A cache cleanup must never remove the durable stores.
        if durable_root.starts_with(&surface_root) || surface_root.starts_with(&durable_root) {
            return Err("application data and surface cache directories must be separate");
        }
        Ok(Self {
            preferences_root: durable_root.clone(),
            durable_root,
            surface_root,
        })
    }

    /// Retains isolated benchmark state, with disposable surfaces in their own child.
    pub fn performance(cache_directory: &Path) -> Result<Self, &'static str> {
        if !cache_directory.is_absolute() {
            return Err("the performance cache directory must be absolute");
        }
        let root = cache_directory.join("document-workspace");
        Ok(Self {
            preferences_root: root.join("application-state"),
            surface_root: root.join("render-surfaces"),
            durable_root: root,
        })
    }

    pub fn preferences_root(&self) -> &Path {
        &self.preferences_root
    }
    pub fn durable_root(&self) -> &Path {
        &self.durable_root
    }
    pub fn surface_root(&self) -> &Path {
        &self.surface_root
    }
    /// Only worker-owned surface files count towards render resource cleanup.
    /// Durable sentinels, templates and recovery state must remain untouched.
    pub fn surfaces_released(&self) -> bool {
        match std::fs::read_dir(&self.surface_root) {
            Ok(mut entries) => entries.next().is_none(),
            Err(error) => error.kind() == std::io::ErrorKind::NotFound,
        }
    }

    pub fn generated_documents_root(&self) -> PathBuf {
        self.durable_root.join("generated-documents")
    }
    pub fn template_library_root(&self) -> PathBuf {
        self.durable_root.join("template-library")
    }
    pub fn session_state_root(&self) -> PathBuf {
        self.durable_root.join("session-state")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn absolute(path: &str) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(format!(
                r"C:\{}",
                path.trim_start_matches('/').replace('/', r"\")
            ))
        }
        #[cfg(not(windows))]
        {
            PathBuf::from(path)
        }
    }
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "bp-storage-layout-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn rejects_missing_relative_and_overlapping_data_roots() {
        let base = root();
        let data = base.join("data");
        let cache = base.join("cache");
        assert!(NativeStorageLayout::development(None, cache.clone()).is_err());
        assert!(
            NativeStorageLayout::development(Some(data.join("../cache/data")), cache.clone())
                .is_err()
        );
        assert!(NativeStorageLayout::development(Some("relative".into()), cache.clone()).is_err());
        assert!(NativeStorageLayout::development(Some(data.clone()), "relative".into()).is_err());
        assert!(NativeStorageLayout::development(Some(data.clone()), data.clone()).is_err());
        assert!(NativeStorageLayout::development(Some(data.clone()), data.join("cache")).is_err());
        assert!(NativeStorageLayout::development(Some(cache.join("data")), cache).is_err());
    }

    #[test]
    fn production_macos_identity_matches_electron_channels_without_sharing_state_roots() {
        let home = absolute("/Users/tester");
        let stable = NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        let beta = NativeProductionStorage::macos(&home, NativeReleaseChannel::Beta).unwrap();
        assert_eq!(
            stable.electron_user_data_root(),
            home.join("Library/Application Support/Butter Paper")
        );
        assert_eq!(
            beta.electron_user_data_root(),
            home.join("Library/Application Support/Butter Paper Beta")
        );
        assert_eq!(
            stable.layout().preferences_root(),
            home.join("Library/Application Support/com.butterpaper.desktop/native-v1")
        );
        assert_eq!(
            beta.layout().preferences_root(),
            home.join("Library/Application Support/com.butterpaper.desktop.beta/native-v1")
        );
        assert_eq!(
            stable.layout().surface_root(),
            home.join("Library/Caches/com.butterpaper.desktop/native-v1/render-surfaces")
        );
        assert_eq!(
            stable.migration_exchange_root(),
            home.join("Library/Application Support/Butter Paper/gpui-migration-export/v1")
        );
        assert_eq!(
            stable.migration_backup_root(),
            home.join("Library/Application Support/com.butterpaper.desktop/migration-backups/v1")
        );
        assert_eq!(
            stable.legacy_development_root(),
            home.join("Library/Application Support/GPUI Migration")
        );
        assert_eq!(
            NativeReleaseChannel::Stable.signature_keychain_service(),
            "com.butterpaper.desktop.recent-signatures"
        );
        assert_eq!(
            NativeReleaseChannel::Beta.signature_keychain_service(),
            "com.butterpaper.desktop.beta.recent-signatures"
        );
        assert_ne!(
            NativeReleaseChannel::Stable.signature_keychain_service(),
            NativeReleaseChannel::Beta.signature_keychain_service()
        );
        for left in [
            stable.layout().preferences_root(),
            stable.layout().surface_root(),
            stable.electron_user_data_root(),
            stable.migration_exchange_root(),
            stable.migration_backup_root(),
        ] {
            for right in [
                beta.layout().preferences_root(),
                beta.layout().surface_root(),
                beta.electron_user_data_root(),
                beta.migration_exchange_root(),
                beta.migration_backup_root(),
            ] {
                assert_ne!(left, right, "stable and beta storage must stay isolated");
                assert!(!left.starts_with(right) && !right.starts_with(left));
            }
        }
        for (left, right) in [
            (
                stable.layout().preferences_root(),
                stable.layout().surface_root(),
            ),
            (
                stable.layout().preferences_root(),
                stable.migration_backup_root(),
            ),
            (
                stable.layout().surface_root(),
                stable.migration_backup_root(),
            ),
            (
                stable.migration_backup_root(),
                stable.migration_exchange_root(),
            ),
        ] {
            assert_ne!(left, right);
            assert!(!left.starts_with(right) && !right.starts_with(left));
        }
        assert!(
            NativeProductionStorage::macos(Path::new("relative"), NativeReleaseChannel::Stable)
                .is_err()
        );
        assert!(
            NativeProductionStorage::macos(
                &absolute("/Users/tester/../other"),
                NativeReleaseChannel::Stable
            )
            .is_err()
        );
    }

    #[test]
    fn production_windows_uses_disjoint_roaming_and_local_stable_roots() {
        // Host-independent absolute fixtures exercise the layout contract;
        // native Windows tests additionally supply drive-qualified paths.
        let roaming = absolute("/windows/Users/tester/AppData/Roaming");
        let local = absolute("/windows/Users/tester/AppData/Local");
        let stable =
            NativeProductionStorage::windows(&roaming, &local, NativeReleaseChannel::Stable)
                .unwrap();
        assert_eq!(
            stable.electron_user_data_root(),
            roaming.join("Butter Paper")
        );
        assert_eq!(
            stable.layout().durable_root(),
            roaming.join("com.butterpaper.desktop/native-v1")
        );
        assert_eq!(
            stable.layout().surface_root(),
            local.join("com.butterpaper.desktop/native-v1/render-surfaces")
        );
        assert_eq!(
            stable.legacy_development_root(),
            roaming.join("GPUI Migration")
        );
        assert!(
            NativeProductionStorage::windows(
                Path::new("relative"),
                &local,
                NativeReleaseChannel::Stable,
            )
            .is_err()
        );
    }

    #[test]
    fn production_linux_keeps_native_data_cache_and_electron_config_separate() {
        let data = absolute("/home/tester/.local/share");
        let cache = absolute("/home/tester/.cache");
        let config = absolute("/home/tester/.config");
        let stable =
            NativeProductionStorage::linux(&data, &cache, &config, NativeReleaseChannel::Stable)
                .unwrap();
        assert_eq!(
            stable.electron_user_data_root(),
            config.join("Butter Paper")
        );
        assert_eq!(
            stable.layout().durable_root(),
            data.join("com.butterpaper.desktop/native-v1")
        );
        assert_eq!(
            stable.layout().surface_root(),
            cache.join("com.butterpaper.desktop/native-v1/render-surfaces")
        );
        assert_eq!(
            stable.legacy_development_root(),
            data.join("GPUI Migration")
        );
        let invalid_absolute = absolute("/home/tester/../other");
        for invalid in [Path::new("relative"), invalid_absolute.as_path()] {
            assert!(
                NativeProductionStorage::linux(
                    invalid,
                    &cache,
                    &config,
                    NativeReleaseChannel::Stable,
                )
                .is_err()
            );
        }
    }

    #[test]
    fn legacy_development_store_is_detected_but_never_created_or_adopted() {
        let home = root();
        let production =
            NativeProductionStorage::macos(&home, NativeReleaseChannel::Stable).unwrap();
        assert!(!production.legacy_development_store_present().unwrap());
        assert!(!production.legacy_development_root().exists());
        std::fs::create_dir_all(production.legacy_development_root()).unwrap();
        std::fs::write(
            production
                .legacy_development_root()
                .join("application-shell.json"),
            b"legacy",
        )
        .unwrap();
        assert!(production.legacy_development_store_present().unwrap());
        assert!(!production.layout().durable_root().exists());
        assert_eq!(
            std::fs::read(
                production
                    .legacy_development_root()
                    .join("application-shell.json")
            )
            .unwrap(),
            b"legacy"
        );
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn durable_artifacts_survive_cache_cleanup_and_relaunch() {
        let base = root();
        let data = base.join("data");
        let first =
            NativeStorageLayout::development(Some(data.clone()), base.join("cache-1")).unwrap();
        use crate::generated_document::{GeneratedDocumentRequest, GeneratedDocumentStore};
        use crate::session_manifest::{SessionManifestStore, SessionSnapshot};
        use crate::template_library::TemplateLibrary;
        let request = GeneratedDocumentRequest::a3_landscape_blank();
        let generated_store =
            GeneratedDocumentStore::new(first.generated_documents_root()).unwrap();
        let generated = generated_store
            .create("storage-relaunch", &request)
            .unwrap();
        let pdf_path = generated.path().to_owned();
        let pdf_bytes = std::fs::read(&pdf_path).unwrap();
        let mut templates = TemplateLibrary::open(first.template_library_root()).unwrap();
        templates
            .add_generated("custom-storage-relaunch", "Saved template", request.clone())
            .unwrap();
        std::fs::create_dir_all(first.session_state_root()).unwrap();
        let session = SessionManifestStore::open(first.session_state_root()).unwrap();
        session
            .replace(&SessionSnapshot::new(vec![pdf_path.clone()], Some(0)))
            .unwrap();
        drop((generated_store, templates, session));
        std::fs::create_dir_all(first.surface_root()).unwrap();
        std::fs::write(first.surface_root().join("surface.bgra"), b"disposable").unwrap();
        std::fs::remove_dir_all(first.surface_root()).unwrap();
        let second = NativeStorageLayout::development(Some(data), base.join("cache-2")).unwrap();
        assert_eq!(
            first.template_library_root(),
            second.template_library_root()
        );
        assert_eq!(first.session_state_root(), second.session_state_root());
        assert_eq!(
            first.generated_documents_root(),
            second.generated_documents_root()
        );
        let reopened_templates = TemplateLibrary::open(second.template_library_root()).unwrap();
        assert_eq!(
            reopened_templates.last_template_id(),
            "custom-storage-relaunch"
        );
        assert_eq!(
            reopened_templates.record_ids(),
            vec!["custom-storage-relaunch"]
        );
        let reopened_session = SessionManifestStore::open(second.session_state_root()).unwrap();
        assert_eq!(
            reopened_session.load().unwrap().into_parts(),
            (vec![pdf_path.clone()], Some(0))
        );
        assert_eq!(std::fs::read(&pdf_path).unwrap(), pdf_bytes);
        assert_eq!(pdf_bytes, request.to_pdf_bytes().unwrap());
        let isolated =
            NativeStorageLayout::development(Some(base.join("other-data")), base.join("cache-3"))
                .unwrap();
        assert_ne!(
            first.template_library_root(),
            isolated.template_library_root()
        );
        assert_ne!(first.session_state_root(), isolated.session_state_root());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn performance_keeps_all_state_inside_its_explicit_cache_root() {
        let base = root();
        let layout = NativeStorageLayout::performance(&base).unwrap();
        assert_eq!(
            layout.surface_root(),
            base.join("document-workspace/render-surfaces")
        );
        assert_eq!(
            layout.preferences_root(),
            base.join("document-workspace/application-state")
        );
        for path in [
            layout.template_library_root(),
            layout.session_state_root(),
            layout.generated_documents_root(),
        ] {
            assert!(path.starts_with(&base));
        }
        assert!(NativeStorageLayout::performance(Path::new("relative")).is_err());
        assert!(
            !base.exists(),
            "resolving layout must not create directories"
        );
    }
    #[test]
    fn performance_cleanup_checks_surfaces_without_removing_persistent_state() {
        use crate::generated_document::{GeneratedDocumentRequest, GeneratedDocumentStore};
        let base = root();
        let layout = NativeStorageLayout::performance(&base).unwrap();
        let generated_store =
            GeneratedDocumentStore::new(layout.generated_documents_root()).unwrap();
        let generated = generated_store
            .create(
                "cleanup-check",
                &GeneratedDocumentRequest::a3_landscape_blank(),
            )
            .unwrap();
        let bytes = std::fs::read(generated.path()).unwrap();
        assert!(layout.surfaces_released());
        std::fs::create_dir_all(layout.surface_root()).unwrap();
        let surface = layout.surface_root().join("surface-1.bgra");
        std::fs::write(&surface, b"live surface").unwrap();
        assert!(!layout.surfaces_released());
        std::fs::remove_file(&surface).unwrap();
        assert!(layout.surfaces_released());
        std::fs::remove_dir(layout.surface_root()).unwrap();
        assert!(layout.surfaces_released());
        assert_eq!(std::fs::read(generated.path()).unwrap(), bytes);
        // An unreadable/non-directory cache is not successful cleanup.
        std::fs::write(layout.surface_root(), b"invalid cache directory").unwrap();
        assert!(!layout.surfaces_released());
        std::fs::remove_dir_all(base).unwrap();
    }
}
