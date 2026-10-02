//! Fail-closed encrypted persistence for recently used signatures.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use chacha20poly1305::{
    Key, XChaCha20Poly1305, XNonce,
    aead::{Aead as _, Generate as _, KeyInit as _, Payload},
};
use image::{ExtendedColorType, ImageEncoder as _, codecs::png::PngEncoder};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize as _, Zeroizing};

use crate::annotation_model::DecodedRgbaAsset;

pub const RECENT_SIGNATURES_FILE_NAME: &str = "recent-signatures.v1.enc";
pub const MAX_RECENT_SIGNATURES: usize = 5;

const MAGIC: &[u8; 8] = b"BPSIGR01";
const AAD: &[u8] = b"butter-paper/gpui/recent-signatures/v1";
const KEY_BYTES: usize = 32;
const NONCE_BYTES: usize = 24;
const MAX_SIGNATURE_DIMENSION: u32 = 4096;
const MAX_SIGNATURE_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_ENCODED_ASSET_BYTES: usize = 2 * 1024 * 1024;
const MAX_PLAINTEXT_BYTES: usize = 12 * 1024 * 1024;
const MAX_ENCRYPTED_BYTES: usize = MAX_PLAINTEXT_BYTES + 1024 * 1024;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecentSignatureSource {
    Drawn,
    Typed,
    Image,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecentSignature {
    id: String,
    last_used_at_ms: u64,
    source: RecentSignatureSource,
    asset: DecodedRgbaAsset,
}

impl RecentSignature {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn last_used_at_ms(&self) -> u64 {
        self.last_used_at_ms
    }

    pub fn source(&self) -> RecentSignatureSource {
        self.source
    }

    pub fn asset(&self) -> &DecodedRgbaAsset {
        &self.asset
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecentSignaturesSnapshot {
    pub available: bool,
    pub signatures: Vec<RecentSignature>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum RecentSignatureStoreError {
    Unavailable,
    Read,
    TooLarge,
    Write,
}

impl std::fmt::Display for RecentSignatureStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "Secure signature storage is not available on this device.",
            Self::Read => "Recent signatures could not be read securely.",
            Self::TooLarge => "This signature is too large to remember securely.",
            Self::Write => "Recent signatures could not be saved securely.",
        })
    }
}

impl std::error::Error for RecentSignatureStoreError {}

#[derive(Debug)]
pub enum SignatureKeyStoreError {
    Unavailable,
    Failure,
}

pub trait SignatureKeyStore: Send + Sync {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SignatureKeyStoreError>;
    fn save(&self, key: &[u8]) -> Result<(), SignatureKeyStoreError>;
}

pub struct PlatformSignatureKeyStore {
    service: String,
    user: String,
}

impl PlatformSignatureKeyStore {
    pub fn new(service: impl Into<String>, user: impl Into<String>) -> Self {
        Self {
            service: service.into(),
            user: user.into(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry, SignatureKeyStoreError> {
        keyring::Entry::new(&self.service, &self.user).map_err(map_keyring_error)
    }
}

impl SignatureKeyStore for PlatformSignatureKeyStore {
    fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SignatureKeyStoreError> {
        match self.entry()?.get_secret() {
            Ok(secret) => Ok(Some(Zeroizing::new(secret))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn save(&self, key: &[u8]) -> Result<(), SignatureKeyStoreError> {
        self.entry()?.set_secret(key).map_err(map_keyring_error)
    }
}

fn map_keyring_error(error: keyring::Error) -> SignatureKeyStoreError {
    match error {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            SignatureKeyStoreError::Unavailable
        }
        _ => SignatureKeyStoreError::Failure,
    }
}

pub struct RecentSignatureStore {
    path: PathBuf,
    keys: Arc<dyn SignatureKeyStore>,
    operation: Mutex<()>,
}

impl RecentSignatureStore {
    pub fn new(path: impl Into<PathBuf>, keys: Arc<dyn SignatureKeyStore>) -> Self {
        Self {
            path: path.into(),
            keys,
            operation: Mutex::new(()),
        }
    }

    pub fn list(&self) -> Result<RecentSignaturesSnapshot, RecentSignatureStoreError> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| RecentSignatureStoreError::Read)?;
        self.remove_temporary_files()
            .map_err(|_| RecentSignatureStoreError::Read)?;
        let key = match self.keys.load() {
            Ok(key) => key,
            Err(SignatureKeyStoreError::Unavailable) => {
                return Ok(RecentSignaturesSnapshot::default());
            }
            Err(SignatureKeyStoreError::Failure) => return Err(RecentSignatureStoreError::Read),
        };
        if !self
            .path_exists()
            .map_err(|_| RecentSignatureStoreError::Read)?
        {
            return Ok(RecentSignaturesSnapshot {
                available: true,
                signatures: Vec::new(),
            });
        }
        let key = key.ok_or(RecentSignatureStoreError::Read)?;
        Ok(RecentSignaturesSnapshot {
            available: true,
            signatures: self.read(&key)?,
        })
    }

    pub fn remember(
        &self,
        asset: DecodedRgbaAsset,
        source: RecentSignatureSource,
        now_ms: u64,
    ) -> Result<RecentSignaturesSnapshot, RecentSignatureStoreError> {
        let candidate = PersistedRecentSignature::encode(asset, source, now_ms)?;
        let _operation = self
            .operation
            .lock()
            .map_err(|_| RecentSignatureStoreError::Write)?;
        self.remove_temporary_files()
            .map_err(|_| RecentSignatureStoreError::Write)?;
        let exists = self
            .path_exists()
            .map_err(|_| RecentSignatureStoreError::Read)?;
        let mut key = self.load_key_for_write(exists)?;
        let mut signatures = if exists { self.read(&key)? } else { Vec::new() };
        signatures.retain(|entry| entry.id() != candidate.id);
        signatures.insert(0, candidate.decode()?);
        signatures.truncate(MAX_RECENT_SIGNATURES);
        self.write(&signatures, &key)?;
        key.zeroize();
        Ok(RecentSignaturesSnapshot {
            available: true,
            signatures,
        })
    }

    pub fn remove(&self, id: &str) -> Result<RecentSignaturesSnapshot, RecentSignatureStoreError> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| RecentSignatureStoreError::Write)?;
        self.remove_temporary_files()
            .map_err(|_| RecentSignatureStoreError::Write)?;
        if !self
            .path_exists()
            .map_err(|_| RecentSignatureStoreError::Read)?
        {
            return Ok(RecentSignaturesSnapshot {
                available: true,
                signatures: Vec::new(),
            });
        }
        let key = self.load_existing_key()?;
        let mut signatures = self.read(&key)?;
        signatures.retain(|entry| entry.id() != id);
        self.write(&signatures, &key)?;
        Ok(RecentSignaturesSnapshot {
            available: true,
            signatures,
        })
    }

    pub fn clear(&self) -> Result<RecentSignaturesSnapshot, RecentSignatureStoreError> {
        let _operation = self
            .operation
            .lock()
            .map_err(|_| RecentSignatureStoreError::Write)?;
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(RecentSignatureStoreError::Write),
        }
        self.remove_temporary_files()
            .map_err(|_| RecentSignatureStoreError::Write)?;
        Ok(RecentSignaturesSnapshot {
            available: !matches!(self.keys.load(), Err(SignatureKeyStoreError::Unavailable)),
            signatures: Vec::new(),
        })
    }

    fn load_key_for_write(
        &self,
        encrypted_file_exists: bool,
    ) -> Result<Zeroizing<Vec<u8>>, RecentSignatureStoreError> {
        match self.keys.load() {
            Ok(Some(key)) if key.len() == KEY_BYTES => Ok(key),
            Ok(Some(_)) => Err(RecentSignatureStoreError::Read),
            Ok(None) if encrypted_file_exists => Err(RecentSignatureStoreError::Read),
            Ok(None) => {
                let key = Zeroizing::new(Key::generate().to_vec());
                self.keys.save(&key).map_err(|error| match error {
                    SignatureKeyStoreError::Unavailable => RecentSignatureStoreError::Unavailable,
                    SignatureKeyStoreError::Failure => RecentSignatureStoreError::Write,
                })?;
                Ok(key)
            }
            Err(SignatureKeyStoreError::Unavailable) => Err(RecentSignatureStoreError::Unavailable),
            Err(SignatureKeyStoreError::Failure) => Err(RecentSignatureStoreError::Write),
        }
    }

    fn load_existing_key(&self) -> Result<Zeroizing<Vec<u8>>, RecentSignatureStoreError> {
        match self.keys.load() {
            Ok(Some(key)) if key.len() == KEY_BYTES => Ok(key),
            Ok(_) | Err(SignatureKeyStoreError::Failure) => Err(RecentSignatureStoreError::Read),
            Err(SignatureKeyStoreError::Unavailable) => Err(RecentSignatureStoreError::Unavailable),
        }
    }

    fn path_exists(&self) -> io::Result<bool> {
        match fs::symlink_metadata(&self.path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "state path is not a regular file",
                ))
            }
            Ok(_) => Ok(true),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn read(&self, key: &[u8]) -> Result<Vec<RecentSignature>, RecentSignatureStoreError> {
        let file = File::open(&self.path).map_err(|_| RecentSignatureStoreError::Read)?;
        let metadata = file
            .metadata()
            .map_err(|_| RecentSignatureStoreError::Read)?;
        if !metadata.is_file() || metadata.len() > MAX_ENCRYPTED_BYTES as u64 {
            return Err(RecentSignatureStoreError::Read);
        }
        let mut encrypted = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_ENCRYPTED_BYTES + 1) as u64)
            .read_to_end(&mut encrypted)
            .map_err(|_| RecentSignatureStoreError::Read)?;
        if encrypted.len() != metadata.len() as usize || encrypted.len() > MAX_ENCRYPTED_BYTES {
            return Err(RecentSignatureStoreError::Read);
        }
        let encrypted = Zeroizing::new(encrypted);
        let (magic, payload) = encrypted
            .split_at_checked(MAGIC.len())
            .ok_or(RecentSignatureStoreError::Read)?;
        if magic != MAGIC {
            return Err(RecentSignatureStoreError::Read);
        }
        let (nonce, ciphertext) = payload
            .split_at_checked(NONCE_BYTES)
            .ok_or(RecentSignatureStoreError::Read)?;
        let cipher =
            XChaCha20Poly1305::new_from_slice(key).map_err(|_| RecentSignatureStoreError::Read)?;
        let nonce = XNonce::try_from(nonce).map_err(|_| RecentSignatureStoreError::Read)?;
        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: AAD,
                },
            )
            .map_err(|_| RecentSignatureStoreError::Read)?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(RecentSignatureStoreError::Read);
        }
        let plaintext = Zeroizing::new(plaintext);
        let persisted: PersistedRecentSignatures =
            serde_json::from_slice(&plaintext).map_err(|_| RecentSignatureStoreError::Read)?;
        persisted.decode()
    }

    fn write(
        &self,
        signatures: &[RecentSignature],
        key: &[u8],
    ) -> Result<(), RecentSignatureStoreError> {
        let persisted = PersistedRecentSignatures::encode(signatures)?;
        let plaintext = Zeroizing::new(
            serde_json::to_vec(&persisted).map_err(|_| RecentSignatureStoreError::Write)?,
        );
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(RecentSignatureStoreError::TooLarge);
        }
        let nonce = XNonce::generate();
        let cipher =
            XChaCha20Poly1305::new_from_slice(key).map_err(|_| RecentSignatureStoreError::Write)?;
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: AAD,
                },
            )
            .map_err(|_| RecentSignatureStoreError::Write)?;
        let mut output = Zeroizing::new(Vec::with_capacity(
            MAGIC.len() + NONCE_BYTES + ciphertext.len(),
        ));
        output.extend_from_slice(MAGIC);
        output.extend_from_slice(&nonce);
        output.extend_from_slice(&ciphertext);
        if output.len() > MAX_ENCRYPTED_BYTES {
            return Err(RecentSignatureStoreError::TooLarge);
        }
        self.write_atomic(&output)
    }

    fn write_atomic(&self, encrypted: &[u8]) -> Result<(), RecentSignatureStoreError> {
        let directory = self.path.parent().ok_or(RecentSignatureStoreError::Write)?;
        fs::create_dir_all(directory).map_err(|_| RecentSignatureStoreError::Write)?;
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(RecentSignatureStoreError::Write)?;
        let temporary = directory.join(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let result = (|| {
            let mut file = options.open(&temporary)?;
            file.write_all(encrypted)?;
            file.sync_all()?;
            drop(file);
            publish(&temporary, &self.path)?;
            #[cfg(unix)]
            File::open(directory)?.sync_all()?;
            Ok::<(), io::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result.map_err(|_| RecentSignatureStoreError::Write)
    }

    fn remove_temporary_files(&self) -> io::Result<()> {
        let Some(directory) = self.path.parent() else {
            return Ok(());
        };
        let Some(file_name) = self.path.file_name().and_then(|name| name.to_str()) else {
            return Ok(());
        };
        let prefix = format!(".{file_name}.");
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&prefix) && name.ends_with(".tmp") {
                let metadata = entry.path().symlink_metadata()?;
                if metadata.is_file() && !metadata.file_type().is_symlink() {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedRecentSignatures {
    schema_version: u8,
    signatures: Vec<PersistedRecentSignature>,
}

impl PersistedRecentSignatures {
    fn encode(signatures: &[RecentSignature]) -> Result<Self, RecentSignatureStoreError> {
        Ok(Self {
            schema_version: 1,
            signatures: signatures
                .iter()
                .cloned()
                .map(|signature| {
                    PersistedRecentSignature::encode(
                        signature.asset,
                        signature.source,
                        signature.last_used_at_ms,
                    )
                })
                .collect::<Result<_, _>>()?,
        })
    }

    fn decode(self) -> Result<Vec<RecentSignature>, RecentSignatureStoreError> {
        if self.schema_version != 1 || self.signatures.len() > MAX_RECENT_SIGNATURES {
            return Err(RecentSignatureStoreError::Read);
        }
        let mut decoded = Vec::with_capacity(self.signatures.len());
        for signature in self.signatures {
            let signature = signature.decode()?;
            if decoded
                .iter()
                .any(|existing: &RecentSignature| existing.id == signature.id)
            {
                return Err(RecentSignatureStoreError::Read);
            }
            decoded.push(signature);
        }
        Ok(decoded)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedRecentSignature {
    id: String,
    last_used_at_ms: u64,
    source: RecentSignatureSource,
    width: u32,
    height: u32,
    png: String,
}

impl PersistedRecentSignature {
    fn encode(
        asset: DecodedRgbaAsset,
        source: RecentSignatureSource,
        last_used_at_ms: u64,
    ) -> Result<Self, RecentSignatureStoreError> {
        validate_dimensions(asset.width_px(), asset.height_px())?;
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(
                asset.rgba(),
                asset.width_px(),
                asset.height_px(),
                ExtendedColorType::Rgba8,
            )
            .map_err(|_| RecentSignatureStoreError::Write)?;
        if png.len() > MAX_ENCODED_ASSET_BYTES {
            return Err(RecentSignatureStoreError::TooLarge);
        }
        Ok(Self {
            id: asset.id().as_str().to_owned(),
            last_used_at_ms,
            source,
            width: asset.width_px(),
            height: asset.height_px(),
            png: BASE64.encode(png),
        })
    }

    fn decode(self) -> Result<RecentSignature, RecentSignatureStoreError> {
        validate_dimensions(self.width, self.height)
            .map_err(|_| RecentSignatureStoreError::Read)?;
        let png = BASE64
            .decode(self.png)
            .map_err(|_| RecentSignatureStoreError::Read)?;
        if png.len() > MAX_ENCODED_ASSET_BYTES {
            return Err(RecentSignatureStoreError::Read);
        }
        let image = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|_| RecentSignatureStoreError::Read)?
            .to_rgba8();
        if image.width() != self.width || image.height() != self.height {
            return Err(RecentSignatureStoreError::Read);
        }
        let asset = DecodedRgbaAsset::new(self.width, self.height, image.into_raw())
            .map_err(|_| RecentSignatureStoreError::Read)?;
        if asset.id().as_str() != self.id {
            return Err(RecentSignatureStoreError::Read);
        }
        Ok(RecentSignature {
            id: self.id,
            last_used_at_ms: self.last_used_at_ms,
            source: self.source,
            asset,
        })
    }
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), RecentSignatureStoreError> {
    if width == 0
        || height == 0
        || width > MAX_SIGNATURE_DIMENSION
        || height > MAX_SIGNATURE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_SIGNATURE_PIXELS
    {
        return Err(RecentSignatureStoreError::TooLarge);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[derive(Clone)]
    enum FakeKeyState {
        Available(Option<Vec<u8>>),
        Unavailable,
        Failure,
    }

    struct FakeKeyStore {
        state: Mutex<FakeKeyState>,
    }

    impl FakeKeyStore {
        fn available() -> Arc<Self> {
            Arc::new(Self {
                state: Mutex::new(FakeKeyState::Available(None)),
            })
        }

        fn with_state(state: FakeKeyState) -> Arc<Self> {
            Arc::new(Self {
                state: Mutex::new(state),
            })
        }

        fn set_state(&self, state: FakeKeyState) {
            *self.state.lock().unwrap() = state;
        }
    }

    impl SignatureKeyStore for FakeKeyStore {
        fn load(&self) -> Result<Option<Zeroizing<Vec<u8>>>, SignatureKeyStoreError> {
            match self.state.lock().unwrap().clone() {
                FakeKeyState::Available(key) => Ok(key.map(Zeroizing::new)),
                FakeKeyState::Unavailable => Err(SignatureKeyStoreError::Unavailable),
                FakeKeyState::Failure => Err(SignatureKeyStoreError::Failure),
            }
        }

        fn save(&self, key: &[u8]) -> Result<(), SignatureKeyStoreError> {
            let mut state = self.state.lock().unwrap();
            match &*state {
                FakeKeyState::Available(_) => {
                    *state = FakeKeyState::Available(Some(key.to_vec()));
                    Ok(())
                }
                FakeKeyState::Unavailable => Err(SignatureKeyStoreError::Unavailable),
                FakeKeyState::Failure => Err(SignatureKeyStoreError::Failure),
            }
        }
    }

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "butter-paper-recent-signatures-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn state_path(&self) -> PathBuf {
            self.0.join(RECENT_SIGNATURES_FILE_NAME)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn asset(seed: u8) -> DecodedRgbaAsset {
        DecodedRgbaAsset::new(2, 1, vec![seed, 7, 11, 255, 13, seed, 17, 128]).unwrap()
    }

    #[test]
    fn encrypted_round_trip_contains_no_plaintext_asset_identity() {
        let directory = TestDirectory::new("round-trip");
        let keys = FakeKeyStore::available();
        let store = RecentSignatureStore::new(directory.state_path(), keys);
        let expected_asset = asset(41);
        let expected_id = expected_asset.id().as_str().to_owned();

        let remembered = store
            .remember(expected_asset.clone(), RecentSignatureSource::Drawn, 42)
            .unwrap();
        assert!(remembered.available);
        assert_eq!(remembered.signatures.len(), 1);
        assert_eq!(remembered.signatures[0].asset(), &expected_asset);
        let encrypted = fs::read(directory.state_path()).unwrap();
        assert!(encrypted.starts_with(MAGIC));
        assert!(
            !encrypted
                .windows(expected_id.len())
                .any(|window| window == expected_id.as_bytes())
        );
        assert!(
            !encrypted
                .windows(b"schemaVersion".len())
                .any(|window| window == b"schemaVersion")
        );

        assert_eq!(store.list().unwrap(), remembered);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(directory.state_path())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn remember_deduplicates_moves_to_front_and_caps_at_five() {
        let directory = TestDirectory::new("lru");
        let store = RecentSignatureStore::new(directory.state_path(), FakeKeyStore::available());
        let assets = (0..6).map(asset).collect::<Vec<_>>();
        for (index, candidate) in assets.iter().cloned().enumerate() {
            store
                .remember(candidate, RecentSignatureSource::Image, index as u64)
                .unwrap();
        }
        let snapshot = store
            .remember(assets[2].clone(), RecentSignatureSource::Typed, 99)
            .unwrap();
        assert_eq!(snapshot.signatures.len(), MAX_RECENT_SIGNATURES);
        assert_eq!(snapshot.signatures[0].id(), assets[2].id().as_str());
        assert_eq!(
            snapshot.signatures[0].source(),
            RecentSignatureSource::Typed
        );
        assert_eq!(snapshot.signatures[0].last_used_at_ms(), 99);
        assert_eq!(
            snapshot
                .signatures
                .iter()
                .map(RecentSignature::id)
                .collect::<Vec<_>>(),
            vec![
                assets[2].id().as_str(),
                assets[5].id().as_str(),
                assets[4].id().as_str(),
                assets[3].id().as_str(),
                assets[1].id().as_str(),
            ]
        );
    }

    #[test]
    fn remove_and_clear_publish_empty_snapshots() {
        let directory = TestDirectory::new("remove-clear");
        let store = RecentSignatureStore::new(directory.state_path(), FakeKeyStore::available());
        let first = asset(1);
        let second = asset(2);
        store
            .remember(first.clone(), RecentSignatureSource::Drawn, 1)
            .unwrap();
        store
            .remember(second.clone(), RecentSignatureSource::Image, 2)
            .unwrap();

        let remaining = store.remove(second.id().as_str()).unwrap();
        assert_eq!(remaining.signatures.len(), 1);
        assert_eq!(remaining.signatures[0].asset(), &first);
        assert!(store.clear().unwrap().signatures.is_empty());
        assert!(!directory.state_path().exists());
        assert!(store.list().unwrap().signatures.is_empty());
    }

    #[test]
    fn unavailable_or_failed_key_storage_never_falls_back_to_plaintext() {
        let directory = TestDirectory::new("unavailable");
        let unavailable = RecentSignatureStore::new(
            directory.state_path(),
            FakeKeyStore::with_state(FakeKeyState::Unavailable),
        );
        assert_eq!(
            unavailable.list().unwrap(),
            RecentSignaturesSnapshot::default()
        );
        assert_eq!(
            unavailable.remember(asset(3), RecentSignatureSource::Drawn, 1),
            Err(RecentSignatureStoreError::Unavailable)
        );
        assert!(!directory.state_path().exists());

        let failed = RecentSignatureStore::new(
            directory.state_path(),
            FakeKeyStore::with_state(FakeKeyState::Failure),
        );
        assert_eq!(failed.list(), Err(RecentSignatureStoreError::Read));
        assert_eq!(
            failed.remember(asset(4), RecentSignatureSource::Drawn, 1),
            Err(RecentSignatureStoreError::Write)
        );
        assert!(!directory.state_path().exists());
    }

    #[test]
    fn missing_malformed_or_changed_keys_fail_closed_for_existing_ciphertext() {
        let directory = TestDirectory::new("missing-key");
        let keys = FakeKeyStore::available();
        let store = RecentSignatureStore::new(directory.state_path(), keys.clone());
        store
            .remember(asset(5), RecentSignatureSource::Typed, 1)
            .unwrap();

        keys.set_state(FakeKeyState::Available(None));
        assert_eq!(store.list(), Err(RecentSignatureStoreError::Read));
        assert_eq!(
            store.remember(asset(6), RecentSignatureSource::Typed, 2),
            Err(RecentSignatureStoreError::Read)
        );
        keys.set_state(FakeKeyState::Available(Some(vec![0; KEY_BYTES - 1])));
        assert_eq!(store.list(), Err(RecentSignatureStoreError::Read));
        keys.set_state(FakeKeyState::Available(Some(vec![9; KEY_BYTES])));
        assert_eq!(store.list(), Err(RecentSignatureStoreError::Read));
    }

    #[test]
    fn ciphertext_tampering_is_rejected() {
        let directory = TestDirectory::new("tamper");
        let store = RecentSignatureStore::new(directory.state_path(), FakeKeyStore::available());
        store
            .remember(asset(8), RecentSignatureSource::Image, 1)
            .unwrap();
        let mut encrypted = fs::read(directory.state_path()).unwrap();
        *encrypted.last_mut().unwrap() ^= 0x80;
        fs::write(directory.state_path(), encrypted).unwrap();
        assert_eq!(store.list(), Err(RecentSignatureStoreError::Read));
    }

    #[test]
    fn oversized_assets_are_rejected_before_key_creation() {
        let directory = TestDirectory::new("oversized");
        let keys = FakeKeyStore::available();
        let store = RecentSignatureStore::new(directory.state_path(), keys.clone());
        let oversized = DecodedRgbaAsset::new(
            MAX_SIGNATURE_DIMENSION + 1,
            1,
            vec![255; (MAX_SIGNATURE_DIMENSION as usize + 1) * 4],
        )
        .unwrap();
        assert_eq!(
            store.remember(oversized, RecentSignatureSource::Image, 1),
            Err(RecentSignatureStoreError::TooLarge)
        );
        assert!(matches!(
            *keys.state.lock().unwrap(),
            FakeKeyState::Available(None)
        ));
        assert!(!directory.state_path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn state_symlinks_are_rejected_without_touching_the_target() {
        use std::os::unix::fs::symlink;

        let directory = TestDirectory::new("symlink");
        let target = directory.0.join("target");
        fs::write(&target, b"leave me alone").unwrap();
        symlink(&target, directory.state_path()).unwrap();
        let store = RecentSignatureStore::new(directory.state_path(), FakeKeyStore::available());
        assert_eq!(store.list(), Err(RecentSignatureStoreError::Read));
        assert_eq!(
            store.remember(asset(7), RecentSignatureSource::Image, 1),
            Err(RecentSignatureStoreError::Read)
        );
        assert_eq!(fs::read(target).unwrap(), b"leave me alone");
    }
}
