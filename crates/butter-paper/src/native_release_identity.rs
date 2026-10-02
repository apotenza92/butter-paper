//! Authenticated stable/beta package identity for production storage selection.
//!
//! A bundle path, Info.plist, environment variable, or bundled runtime layout
//! is not production authority by itself. The macOS implementation validates
//! the complete app bundle against Butter Paper's exact Developer ID
//! requirement before returning a release channel.

use std::{
    ffi::OsStr,
    fmt,
    path::{Path, PathBuf},
};

use crate::native_storage_layout::NativeReleaseChannel;

const APPLE_TEAM_ID: &str = "27JL2VERNC";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeReleaseIdentity {
    channel: NativeReleaseChannel,
    bundle_root: PathBuf,
    executable: PathBuf,
    version: String,
}

impl NativeReleaseIdentity {
    pub fn channel(&self) -> NativeReleaseChannel {
        self.channel
    }

    pub fn bundle_root(&self) -> &Path {
        &self.bundle_root
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BundleMetadata {
    bundle_identifier: String,
    display_name: String,
    bundle_name: String,
    executable: String,
    package_type: String,
    short_version: String,
    bundle_version: String,
}

trait PackageAttestor {
    fn attest(
        &self,
        bundle_root: &Path,
        channel: NativeReleaseChannel,
    ) -> Result<(), NativeReleaseIdentityError>;
}

#[derive(Debug, Eq, PartialEq)]
pub struct NativeReleaseIdentityError(String);

impl fmt::Display for NativeReleaseIdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for NativeReleaseIdentityError {}

fn failure(message: impl Into<String>) -> NativeReleaseIdentityError {
    NativeReleaseIdentityError(message.into())
}

fn release_channel(
    metadata: &BundleMetadata,
) -> Result<NativeReleaseChannel, NativeReleaseIdentityError> {
    let channel = match metadata.bundle_identifier.as_str() {
        "com.butterpaper.desktop" => NativeReleaseChannel::Stable,
        "com.butterpaper.desktop.beta" => NativeReleaseChannel::Beta,
        _ => {
            return Err(failure(
                "the app bundle identifier is not a Butter Paper release identity",
            ));
        }
    };
    let product_name = channel.product_name();
    if metadata.display_name != product_name
        || metadata.bundle_name != product_name
        || metadata.executable != product_name
        || metadata.package_type != "APPL"
    {
        return Err(failure(
            "the app bundle metadata does not match its release channel",
        ));
    }
    if metadata.short_version.is_empty()
        || metadata.short_version.len() > 64
        || metadata.short_version.chars().any(char::is_whitespace)
        || metadata.bundle_version.is_empty()
        || metadata.bundle_version.len() > 64
        || !metadata
            .bundle_version
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'.')
    {
        return Err(failure("the app bundle version metadata is invalid"));
    }
    Ok(channel)
}

fn resolve_identity(
    executable: &Path,
    metadata: BundleMetadata,
    attestor: &dyn PackageAttestor,
) -> Result<NativeReleaseIdentity, NativeReleaseIdentityError> {
    let executable = std::fs::canonicalize(executable)
        .map_err(|_| failure("the current executable path cannot be canonicalised"))?;
    let macos = executable
        .parent()
        .ok_or_else(|| failure("the current executable is outside an app bundle"))?;
    let contents = macos
        .parent()
        .ok_or_else(|| failure("the current executable is outside an app bundle"))?;
    let bundle_root = contents
        .parent()
        .ok_or_else(|| failure("the current executable is outside an app bundle"))?;
    if macos.file_name() != Some(OsStr::new("MacOS"))
        || contents.file_name() != Some(OsStr::new("Contents"))
        || bundle_root.extension() != Some(OsStr::new("app"))
    {
        return Err(failure(
            "the current executable is outside a canonical app bundle",
        ));
    }
    let bundle_root = std::fs::canonicalize(bundle_root)
        .map_err(|_| failure("the app bundle path cannot be canonicalised"))?;
    let channel = release_channel(&metadata)?;
    let expected_executable = bundle_root
        .join("Contents/MacOS")
        .join(channel.product_name());
    if executable != expected_executable {
        return Err(failure(
            "the signed bundle executable does not match the running executable",
        ));
    }
    attestor.attest(&bundle_root, channel)?;
    Ok(NativeReleaseIdentity {
        channel,
        bundle_root,
        executable,
        version: metadata.short_version,
    })
}

#[cfg(target_os = "macos")]
pub fn attest_current_release() -> Result<NativeReleaseIdentity, NativeReleaseIdentityError> {
    let executable = std::env::current_exe()
        .map_err(|_| failure("the current executable path is unavailable"))?;
    let executable = std::fs::canonicalize(executable)
        .map_err(|_| failure("the current executable path cannot be canonicalised"))?;
    let bundle_root = executable
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| failure("the current executable is outside an app bundle"))?;
    let metadata = macos::read_bundle_metadata(bundle_root)?;
    resolve_identity(&executable, metadata, &macos::SystemPackageAttestor)
}

#[cfg(target_os = "macos")]
pub fn current_user_home_directory() -> Result<PathBuf, NativeReleaseIdentityError> {
    macos::current_user_home_directory()
}

#[cfg(target_os = "macos")]
pub fn display_fatal_launch_error(detail: &str) {
    macos::display_fatal_launch_error(detail);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{CStr, OsString},
        os::unix::ffi::OsStringExt as _,
        ptr,
    };

    use core_foundation::base::TCFType as _;
    use core_foundation::{bundle::CFBundle, string::CFString, url::CFURL};
    use core_foundation_sys::user_notification::CFUserNotificationDisplayAlert;
    use security_framework::os::macos::code_signing::{Flags, SecRequirement, SecStaticCode};

    use super::*;

    pub(super) struct SystemPackageAttestor;

    impl PackageAttestor for SystemPackageAttestor {
        fn attest(
            &self,
            bundle_root: &Path,
            channel: NativeReleaseChannel,
        ) -> Result<(), NativeReleaseIdentityError> {
            let url = CFURL::from_path(bundle_root, true)
                .ok_or_else(|| failure("the app bundle URL is invalid"))?;
            let code = SecStaticCode::from_path(&url, Flags::NONE)
                .map_err(|_| failure("the app bundle signature is unavailable"))?;
            let requirement = format!(
                "anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"{APPLE_TEAM_ID}\" and identifier \"{}\"",
                channel.bundle_identifier(),
            )
            .parse::<SecRequirement>()
            .map_err(|_| failure("the Butter Paper signing requirement is invalid"))?;
            code.check_validity(
                Flags::CHECK_ALL_ARCHITECTURES
                    | Flags::CHECK_NESTED_CODE
                    | Flags::STRICT_VALIDATE
                    | Flags::RESTRICT_SYMLINKS
                    | Flags::RESTRICT_TO_APP_LIKE
                    | Flags::NO_NETWORK_ACCESS,
                &requirement,
            )
            .map_err(|_| {
                failure("the app bundle does not satisfy the Butter Paper Developer ID requirement")
            })
        }
    }

    fn info_string(bundle: &CFBundle, key: &str) -> Result<String, NativeReleaseIdentityError> {
        let dictionary = bundle.info_dictionary();
        let key = CFString::new(key);
        let value = dictionary
            .find(&key)
            .and_then(|value| value.downcast::<CFString>())
            .ok_or_else(|| failure("the app bundle metadata is incomplete"))?;
        Ok(value.to_string())
    }

    pub(super) fn read_bundle_metadata(
        bundle_root: &Path,
    ) -> Result<BundleMetadata, NativeReleaseIdentityError> {
        let url = CFURL::from_path(bundle_root, true)
            .ok_or_else(|| failure("the app bundle URL is invalid"))?;
        let bundle =
            CFBundle::new(url).ok_or_else(|| failure("the app bundle metadata is unavailable"))?;
        Ok(BundleMetadata {
            bundle_identifier: info_string(&bundle, "CFBundleIdentifier")?,
            display_name: info_string(&bundle, "CFBundleDisplayName")?,
            bundle_name: info_string(&bundle, "CFBundleName")?,
            executable: info_string(&bundle, "CFBundleExecutable")?,
            package_type: info_string(&bundle, "CFBundlePackageType")?,
            short_version: info_string(&bundle, "CFBundleShortVersionString")?,
            bundle_version: info_string(&bundle, "CFBundleVersion")?,
        })
    }

    pub(super) fn current_user_home_directory() -> Result<PathBuf, NativeReleaseIdentityError> {
        let uid = unsafe { libc::geteuid() };
        let suggested = unsafe { libc::sysconf(libc::_SC_GETPW_R_SIZE_MAX) };
        let capacity = if suggested > 0 {
            suggested as usize
        } else {
            16 * 1024
        };
        let mut buffer = vec![0_u8; capacity];
        let mut password = std::mem::MaybeUninit::<libc::passwd>::uninit();
        let mut result = ptr::null_mut();
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                password.as_mut_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if status != 0 || result.is_null() {
            return Err(failure(
                "the effective user's home directory is unavailable",
            ));
        }
        let directory = unsafe { CStr::from_ptr((*result).pw_dir) }.to_bytes();
        let path = PathBuf::from(OsString::from_vec(directory.to_vec()));
        if !path.is_absolute() {
            return Err(failure("the effective user's home directory is invalid"));
        }
        Ok(path)
    }

    pub(super) fn display_fatal_launch_error(detail: &str) {
        let header = CFString::new("Butter Paper could not start safely");
        let detail: String = detail.chars().take(512).collect();
        let message = CFString::new(&format!(
            "Butter Paper stopped before opening or creating production data because its package or migration could not be verified. Your existing app and data were not replaced. Reopen the previous Butter Paper version or reinstall a trusted update, then try again.\n\nDetails: {detail}"
        ));
        let quit = CFString::new("Quit");
        let mut response = 0;
        unsafe {
            CFUserNotificationDisplayAlert(
                0.0,
                0,
                ptr::null(),
                ptr::null(),
                ptr::null(),
                header.as_concrete_TypeRef(),
                message.as_concrete_TypeRef(),
                quit.as_concrete_TypeRef(),
                ptr::null(),
                ptr::null(),
                &mut response,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct FakeAttestor(bool);

    impl PackageAttestor for FakeAttestor {
        fn attest(
            &self,
            _bundle_root: &Path,
            _channel: NativeReleaseChannel,
        ) -> Result<(), NativeReleaseIdentityError> {
            self.0
                .then_some(())
                .ok_or_else(|| failure("signature rejected"))
        }
    }

    fn fixture(channel: NativeReleaseChannel) -> (PathBuf, BundleMetadata) {
        let root = std::env::temp_dir().join(format!(
            "bp-native-release-identity-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        ));
        let executable = root
            .join(format!("{}.app", channel.product_name()))
            .join("Contents/MacOS")
            .join(channel.product_name());
        fs::create_dir_all(executable.parent().unwrap()).unwrap();
        fs::write(&executable, b"fixture").unwrap();
        let metadata = BundleMetadata {
            bundle_identifier: channel.bundle_identifier().into(),
            display_name: channel.product_name().into(),
            bundle_name: channel.product_name().into(),
            executable: channel.product_name().into(),
            package_type: "APPL".into(),
            short_version: "1.2.3".into(),
            bundle_version: "123".into(),
        };
        (executable, metadata)
    }

    #[test]
    fn exact_stable_and_beta_identity_require_a_valid_signature() {
        for channel in [NativeReleaseChannel::Stable, NativeReleaseChannel::Beta] {
            let (executable, metadata) = fixture(channel);
            let identity =
                resolve_identity(&executable, metadata.clone(), &FakeAttestor(true)).unwrap();
            assert_eq!(identity.channel(), channel);
            assert_eq!(
                identity.executable(),
                std::fs::canonicalize(&executable).unwrap()
            );
            assert_eq!(identity.version(), "1.2.3");
            assert!(resolve_identity(&executable, metadata, &FakeAttestor(false)).is_err());
            fs::remove_dir_all(executable.ancestors().nth(4).unwrap()).unwrap();
        }
    }

    #[test]
    fn mismatched_metadata_and_noncanonical_paths_are_rejected() {
        let (executable, metadata) = fixture(NativeReleaseChannel::Stable);
        for mutate in [
            |value: &mut BundleMetadata| {
                value.bundle_identifier = "dev.butterpaper.development".into()
            },
            |value: &mut BundleMetadata| value.display_name = "Butter Paper Beta".into(),
            |value: &mut BundleMetadata| value.executable = "butter-paper".into(),
            |value: &mut BundleMetadata| value.package_type = "BNDL".into(),
            |value: &mut BundleMetadata| value.bundle_version = "1 beta".into(),
        ] {
            let mut candidate = metadata.clone();
            mutate(&mut candidate);
            assert!(resolve_identity(&executable, candidate, &FakeAttestor(true)).is_err());
        }
        let outside = executable.ancestors().nth(4).unwrap().join("Butter Paper");
        fs::write(&outside, b"fixture").unwrap();
        assert!(resolve_identity(&outside, metadata, &FakeAttestor(true)).is_err());
        fs::remove_dir_all(executable.ancestors().nth(4).unwrap()).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn real_security_framework_rejects_an_adhoc_release_shaped_bundle() {
        use std::process::Command;

        let (executable, _) = fixture(NativeReleaseChannel::Stable);
        fs::copy("/usr/bin/true", &executable).unwrap();
        let bundle_root = executable.ancestors().nth(3).unwrap();
        fs::write(
            bundle_root.join("Contents/Info.plist"),
            br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleDisplayName</key><string>Butter Paper</string>
<key>CFBundleExecutable</key><string>Butter Paper</string>
<key>CFBundleIdentifier</key><string>com.butterpaper.desktop</string>
<key>CFBundleName</key><string>Butter Paper</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>1.2.3</string>
<key>CFBundleVersion</key><string>123</string>
</dict></plist>
"#,
        )
        .unwrap();
        let status = Command::new("/usr/bin/codesign")
            .args(["--force", "--deep", "--sign", "-"])
            .arg(bundle_root)
            .status()
            .unwrap();
        assert!(status.success());
        let metadata = macos::read_bundle_metadata(bundle_root).unwrap();
        let error =
            resolve_identity(&executable, metadata, &macos::SystemPackageAttestor).unwrap_err();
        assert_eq!(
            error.to_string(),
            "the app bundle does not satisfy the Butter Paper Developer ID requirement"
        );
        fs::remove_dir_all(executable.ancestors().nth(4).unwrap()).unwrap();
    }
}
