//! Self-update for installed production copies of Butter Paper.
//!
//! An update comes only from the latest published GitHub release and only
//! when that release carries this platform's native package and the
//! `SHA256SUMS.txt` that the release pipeline publishes beside it. The
//! package must match its published size and SHA-256. On macOS the unpacked
//! app must also satisfy Butter Paper's Developer ID requirement and
//! Gatekeeper before it is staged.
//!
//! Installing happens in two steps. `prepare_update` does everything that can
//! run while the app is open: download, verify, and stage (macOS), install
//! beside the running version (Windows) or unpack (Linux). `spawn_handover`
//! then starts a small detached script that waits for this process to exit,
//! swaps the installed version with rollback on failure, and optionally
//! reopens the app.

use std::{
    collections::HashMap,
    fmt,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Recent releases, newest first. Prereleases are betas; drafts are ignored.
pub const RELEASE_FEED_URL: &str =
    "https://api.github.com/repos/apotenza92/butter-paper/releases?per_page=30";
/// Every accepted download lives under this repository's release downloads.
pub const RELEASE_DOWNLOAD_PREFIX: &str =
    "https://github.com/apotenza92/butter-paper/releases/download/";
pub const CHECKSUMS_ASSET_NAME: &str = "SHA256SUMS.txt";
const APPLE_TEAM_ID: &str = "27JL2VERNC";

const MAX_PACKAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_FEED_BYTES: u64 = 4 * 1024 * 1024;
const MAX_CHECKSUMS_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateError(String);

impl UpdateError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// An error with a user-facing message from outside this module.
    pub fn new_public(message: impl Into<String>) -> Self {
        Self::new(message)
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UpdateError {}

/// A `major.minor.patch` release, or its `major.minor.patch-beta.N` betas,
/// optionally written with a `v`. The last field is the beta number, or
/// `u64::MAX` for the release itself, so a release sorts above its betas.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ReleaseVersion(pub u64, pub u64, pub u64, pub u64);

impl ReleaseVersion {
    const RELEASE: u64 = u64::MAX;

    pub const fn stable(major: u64, minor: u64, patch: u64) -> Self {
        Self(major, minor, patch, Self::RELEASE)
    }

    pub const fn beta(major: u64, minor: u64, patch: u64, beta: u64) -> Self {
        Self(major, minor, patch, beta)
    }

    pub const fn is_beta(self) -> bool {
        self.3 != Self::RELEASE
    }

    /// `major.minor.patch`, which macOS shows as CFBundleShortVersionString.
    pub fn core(self) -> String {
        format!("{}.{}.{}", self.0, self.1, self.2)
    }

    /// CFBundleVersion: `(M*1e6 + m*1e3 + p)*1e5 + N`, where N is the beta
    /// number or 90000 for the release, so a release builds above its betas.
    pub fn build_number(self) -> u64 {
        let stage = if self.is_beta() { self.3 } else { 90_000 };
        (self.0 * 1_000_000 + self.1 * 1_000 + self.2) * 100_000 + stage
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.strip_prefix('v').unwrap_or(value);
        let (core, beta) = match value.split_once("-beta.") {
            Some((core, beta)) => {
                let number = beta.parse::<u64>().ok().filter(|number| {
                    *number >= 1 && *number < Self::RELEASE && !beta.starts_with('0')
                })?;
                (core, number)
            }
            None => (value, Self::RELEASE),
        };
        let mut parts = core.split('.');
        let mut next = || parts.next()?.parse::<u64>().ok();
        let version = Self(next()?, next()?, next()?, beta);
        parts.next().is_none().then_some(version)
    }
}

impl fmt::Display for ReleaseVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}.{}", self.0, self.1, self.2)?;
        if self.is_beta() {
            write!(formatter, "-beta.{}", self.3)?;
        }
        Ok(())
    }
}

/// Which releases a copy follows. Beta copies take betas and also any newer
/// stable release, so they are never behind the stable channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateChannel {
    Stable,
    Beta,
}

/// The app identity a package installs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageIdentity {
    Stable,
    Beta,
}

impl PackageIdentity {
    pub fn macos_bundle_name(self) -> &'static str {
        match self {
            Self::Stable => "Butter Paper.app",
            Self::Beta => "Butter Paper Beta.app",
        }
    }

    /// The Developer ID requirement a downloaded app of this identity must meet.
    pub fn macos_designated_requirement(self) -> String {
        let identifier = match self {
            Self::Stable => "com.butterpaper.desktop",
            Self::Beta => "com.butterpaper.desktop.beta",
        };
        format!(
            "=identifier \"{identifier}\" and anchor apple generic and certificate leaf[subject.OU] = \"{APPLE_TEAM_ID}\""
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UpdateTarget {
    MacosArm64,
    MacosX64,
    WindowsArm64,
    WindowsX64,
    LinuxArm64,
    LinuxX64,
}

impl UpdateTarget {
    pub fn current() -> Option<Self> {
        let arm = cfg!(target_arch = "aarch64");
        let x64 = cfg!(target_arch = "x86_64");
        if cfg!(target_os = "macos") {
            return arm.then_some(Self::MacosArm64).or(x64.then_some(Self::MacosX64));
        }
        if cfg!(target_os = "windows") {
            return arm
                .then_some(Self::WindowsArm64)
                .or(x64.then_some(Self::WindowsX64));
        }
        if cfg!(target_os = "linux") {
            return arm.then_some(Self::LinuxArm64).or(x64.then_some(Self::LinuxX64));
        }
        None
    }

    /// The package name the release workflow publishes for `identity`;
    /// beta packages carry a `Beta` infix.
    pub fn asset_name(self, identity: PackageIdentity) -> String {
        let platform = match self {
            Self::MacosArm64 => "macOS-arm64.zip",
            Self::MacosX64 => "macOS-x64.zip",
            Self::WindowsArm64 => "Windows-arm64.zip",
            Self::WindowsX64 => "Windows-x64.zip",
            Self::LinuxArm64 => "Linux-arm64.tar.xz",
            Self::LinuxX64 => "Linux-x64.tar.xz",
        };
        match identity {
            PackageIdentity::Stable => format!("Butter-Paper-{platform}"),
            PackageIdentity::Beta => format!("Butter-Paper-Beta-{platform}"),
        }
    }

    /// The architecture name used by the Windows and Linux install layouts.
    pub fn package_architecture(self) -> &'static str {
        match self {
            Self::MacosArm64 | Self::WindowsArm64 | Self::LinuxArm64 => "arm64",
            Self::MacosX64 | Self::WindowsX64 => "x86_64",
            Self::LinuxX64 => "x86_64",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseAsset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Release {
    pub version: ReleaseVersion,
    pub prerelease: bool,
    pub page_url: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct FeedRelease {
    tag_name: String,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<FeedAsset>,
}

#[derive(Deserialize)]
struct FeedAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

/// Published releases with recognisable versions; drafts and tags such as
/// `electron-final` are skipped.
pub fn parse_releases(bytes: &[u8]) -> Result<Vec<Release>, UpdateError> {
    let releases: Vec<FeedRelease> = serde_json::from_slice(bytes)
        .map_err(|_| UpdateError::new("The release feed could not be read."))?;
    Ok(releases
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let version = ReleaseVersion::parse(&release.tag_name)?;
            Some(Release {
                version,
                // A `-beta.N` tag is a beta even if its release is not
                // marked as a prerelease.
                prerelease: release.prerelease || version.is_beta(),
                page_url: release.html_url,
                assets: release
                    .assets
                    .into_iter()
                    .map(|asset| ReleaseAsset {
                        name: asset.name,
                        url: asset.browser_download_url,
                        size: asset.size,
                    })
                    .collect(),
            })
        })
        .collect())
}

/// `sha256sum` output: a lowercase 64-digit hash, whitespace, then the name
/// (optionally `*`-prefixed for binary mode).
pub fn parse_checksums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.trim().split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            let valid = hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit());
            (valid && !name.is_empty()).then(|| (name.to_owned(), hash.to_ascii_lowercase()))
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableUpdate {
    pub version: ReleaseVersion,
    pub target: UpdateTarget,
    /// The identity the package installs; a beta copy may move to stable.
    pub identity: PackageIdentity,
    pub package: ReleaseAsset,
    pub checksums: ReleaseAsset,
    pub page_url: String,
}

/// The newest installable release for this channel that is newer than
/// `current`. Releases without this platform's package and checksums (such
/// as the Electron-only handover releases) are skipped.
pub fn select_update(
    current: ReleaseVersion,
    channel: UpdateChannel,
    releases: &[Release],
    target: UpdateTarget,
) -> Option<AvailableUpdate> {
    let mut candidates = releases
        .iter()
        .filter(|release| release.version > current)
        .filter(|release| channel == UpdateChannel::Beta || !release.prerelease)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| right.version.cmp(&left.version));
    candidates.into_iter().find_map(|release| {
        // Beta copies keep their identity when the release carries a beta
        // package; otherwise a stable release moves them to stable.
        let identities: &[PackageIdentity] = match (channel, release.prerelease) {
            (UpdateChannel::Stable, _) => &[PackageIdentity::Stable],
            (UpdateChannel::Beta, true) => &[PackageIdentity::Beta],
            (UpdateChannel::Beta, false) => &[PackageIdentity::Beta, PackageIdentity::Stable],
        };
        identities
            .iter()
            .find_map(|identity| installable(release, target, *identity))
    })
}

fn installable(
    release: &Release,
    target: UpdateTarget,
    identity: PackageIdentity,
) -> Option<AvailableUpdate> {
    let find = |name: &str| release.assets.iter().find(|asset| asset.name == name).cloned();
    let package = find(&target.asset_name(identity))?;
    let checksums = find(CHECKSUMS_ASSET_NAME)?;
    let tag_prefix = format!("{RELEASE_DOWNLOAD_PREFIX}v{}/", release.version);
    let trusted = [&package, &checksums]
        .iter()
        .all(|asset| asset.url == format!("{tag_prefix}{}", asset.name));
    (trusted && package.size > 0 && package.size <= MAX_PACKAGE_BYTES).then(|| AvailableUpdate {
        version: release.version,
        target,
        identity,
        package,
        checksums,
        page_url: release.page_url.clone(),
    })
}

fn http_client() -> Result<reqwest::blocking::Client, UpdateError> {
    reqwest::blocking::Client::builder()
        .https_only(true)
        // Release downloads redirect to GitHub's asset host.
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent(concat!("Butter-Paper/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(30 * 60))
        .build()
        .map_err(|_| UpdateError::new("The update connection could not be started."))
}

fn get_bounded(
    client: &reqwest::blocking::Client,
    url: &str,
    limit: u64,
) -> Result<Vec<u8>, UpdateError> {
    let response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .map_err(|_| UpdateError::new("Butter Paper could not reach GitHub."))?;
    if !response.status().is_success() {
        return Err(UpdateError::new(format!(
            "GitHub returned HTTP {}.",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| UpdateError::new("The update download was interrupted."))?;
    if bytes.len() as u64 > limit {
        return Err(UpdateError::new("The update response was unexpectedly large."));
    }
    Ok(bytes)
}

pub fn fetch_releases(feed_url: &str) -> Result<Vec<Release>, UpdateError> {
    let client = http_client()?;
    parse_releases(&get_bounded(&client, feed_url, MAX_FEED_BYTES)?)
}

/// Downloads the package into `directory`, checking its size and its
/// published SHA-256 as it streams.
pub fn download_update(update: &AvailableUpdate, directory: &Path) -> Result<PathBuf, UpdateError> {
    let client = http_client()?;
    let checksums = get_bounded(&client, &update.checksums.url, MAX_CHECKSUMS_BYTES)?;
    let checksums = parse_checksums(&String::from_utf8_lossy(&checksums));
    let expected = checksums.get(&update.package.name).ok_or_else(|| {
        UpdateError::new("The release checksums do not list this platform's package.")
    })?;
    let response = client
        .get(&update.package.url)
        .send()
        .map_err(|_| UpdateError::new("Butter Paper could not reach GitHub."))?;
    if !response.status().is_success() {
        return Err(UpdateError::new(format!(
            "The download failed (HTTP {}).",
            response.status().as_u16()
        )));
    }
    let path = directory.join(&update.package.name);
    let written = write_verified(response, &path, update.package.size, expected);
    if written.is_err() {
        let _ = fs::remove_file(&path);
    }
    written.map(|()| path)
}

/// Streams `source` to a new file at `path`, requiring exactly `size` bytes
/// whose SHA-256 is `expected_sha256`.
pub fn write_verified(
    mut source: impl Read,
    path: &Path,
    size: u64,
    expected_sha256: &str,
) -> Result<(), UpdateError> {
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| UpdateError::new("The update could not be saved."))?;
    let mut hasher = Sha256::new();
    let mut received = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|_| UpdateError::new("The update download was interrupted."))?;
        if read == 0 {
            break;
        }
        received += read as u64;
        if received > size {
            return Err(UpdateError::new("The download was larger than published."));
        }
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|_| UpdateError::new("The update could not be saved."))?;
    }
    file.sync_all()
        .map_err(|_| UpdateError::new("The update could not be saved."))?;
    let actual = hex(&hasher.finalize());
    if received != size || actual != expected_sha256.to_ascii_lowercase() {
        return Err(UpdateError::new("The download did not match the published release."));
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Where the running copy is installed, as each platform's package lays it out.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Installation {
    /// `…/Butter Paper.app`, whose parent directory receives the new bundle.
    Macos { bundle: PathBuf },
    /// `%LOCALAPPDATA%\Programs\Butter Paper\<version>\<arch>`.
    Windows { root: PathBuf, version: String, architecture: String },
    /// `$XDG_DATA_HOME/butter-paper/<version>`.
    Linux { root: PathBuf, version: String },
}

/// The installation that holds `executable`, or `None` for development
/// builds and copies run from anywhere else.
pub fn current_installation(executable: &Path, version: ReleaseVersion) -> Option<Installation> {
    let parent = executable.parent()?;
    if cfg!(target_os = "macos") {
        let contents = parent.parent()?;
        let bundle = contents.parent()?;
        let is_bundle = parent.file_name()? == "MacOS"
            && contents.file_name()? == "Contents"
            && bundle.extension().is_some_and(|extension| extension == "app");
        return is_bundle.then(|| Installation::Macos {
            bundle: bundle.to_path_buf(),
        });
    }
    if cfg!(target_os = "windows") {
        let architecture = parent.file_name()?.to_str()?.to_owned();
        let version_directory = parent.parent()?;
        let installed_version = version_directory.file_name()?.to_str()?.to_owned();
        let product = version_directory.parent()?;
        let owned = product.file_name()? == "Butter Paper"
            && installed_version == version.to_string()
            && parent.join(".butter-paper-install.json").is_file()
            && parent.join("uninstall.ps1").is_file();
        return owned.then(|| Installation::Windows {
            root: parent.to_path_buf(),
            version: installed_version,
            architecture,
        });
    }
    if cfg!(target_os = "linux") {
        let installed_version = parent.file_name()?.to_str()?.to_owned();
        let owned = parent.parent()?.file_name()? == "butter-paper"
            && installed_version == version.to_string()
            && parent.join("uninstall-user.sh").is_file()
            && parent.join("install-user.sh").is_file();
        return owned.then(|| Installation::Linux {
            root: parent.to_path_buf(),
            version: installed_version,
        });
    }
    None
}

/// An update made ready while the app runs, waiting for the app to exit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreparedUpdate {
    pub version: ReleaseVersion,
    work_directory: PathBuf,
    handover: Handover,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Handover {
    Macos {
        staged: PathBuf,
        destination: PathBuf,
        /// The running bundle when the update installs under another name.
        retired: Option<PathBuf>,
    },
    Windows {
        old_root: PathBuf,
        new_root: PathBuf,
    },
    Linux {
        old_root: PathBuf,
        package: PathBuf,
        new_executable: PathBuf,
    },
}

fn run(command: &mut Command, failure: &str) -> Result<(), UpdateError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|_| UpdateError::new(failure.to_owned()))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(UpdateError::new(if detail.is_empty() {
            failure.to_owned()
        } else {
            format!("{failure} {detail}")
        }))
    }
}

/// Unpacks and verifies a downloaded package and installs or stages it
/// beside the running copy. Nothing the running app uses is replaced yet.
pub fn prepare_update(
    update: &AvailableUpdate,
    package: &Path,
    work_directory: &Path,
    installation: &Installation,
) -> Result<PreparedUpdate, UpdateError> {
    let handover = match installation {
        Installation::Macos { bundle } => prepare_macos(update, package, work_directory, bundle)?,
        Installation::Windows { root, .. } => prepare_windows(update, package, work_directory, root)?,
        Installation::Linux { root, .. } => prepare_linux(update, package, work_directory, root)?,
    };
    Ok(PreparedUpdate {
        version: update.version,
        work_directory: work_directory.to_path_buf(),
        handover,
    })
}

fn prepare_macos(
    update: &AvailableUpdate,
    package: &Path,
    work_directory: &Path,
    bundle: &Path,
) -> Result<Handover, UpdateError> {
    let extracted = work_directory.join("extracted");
    fs::create_dir_all(&extracted)
        .map_err(|_| UpdateError::new("The update could not be unpacked."))?;
    run(
        Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(package).arg(&extracted),
        "The update could not be unpacked.",
    )?;
    let downloaded = extracted.join(update.identity.macos_bundle_name());
    run(
        Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict", "-R"])
            .arg(update.identity.macos_designated_requirement())
            .arg(&downloaded),
        "The new app's signature could not be verified.",
    )?;
    run(
        Command::new("/usr/sbin/spctl")
            .args(["--assess", "--type", "execute"])
            .arg(&downloaded),
        "macOS did not accept the new app.",
    )?;
    let plist_value = |key: &str| {
        Command::new("/usr/bin/plutil")
            .args(["-extract", key, "raw", "-o", "-"])
            .arg(downloaded.join("Contents/Info.plist"))
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    // A beta shows its core version; its build number tells it apart.
    if plist_value("CFBundleShortVersionString") != Some(update.version.core())
        || plist_value("CFBundleVersion") != Some(update.version.build_number().to_string())
    {
        return Err(UpdateError::new("The downloaded app is not the expected version."));
    }
    // Stage beside the installed app so the final swap is a same-volume rename.
    let parent = bundle
        .parent()
        .ok_or_else(|| UpdateError::new("The installed app has no folder."))?;
    let staged = parent.join(format!(".Butter Paper {} update.app", update.version));
    let _ = fs::remove_dir_all(&staged);
    run(
        Command::new("/usr/bin/ditto").arg(&downloaded).arg(&staged),
        &format!("Butter Paper could not write to {}.", parent.display()),
    )?;
    let _ = fs::remove_dir_all(&extracted);
    let _ = fs::remove_file(package);
    // A beta copy moving to stable installs as Butter Paper beside it, and
    // the beta app is removed once the new one is in place.
    let destination = parent.join(update.identity.macos_bundle_name());
    let retired = (destination != bundle).then(|| bundle.to_path_buf());
    Ok(Handover::Macos {
        staged,
        destination,
        retired,
    })
}

fn prepare_windows(
    update: &AvailableUpdate,
    package: &Path,
    work_directory: &Path,
    old_root: &Path,
) -> Result<Handover, UpdateError> {
    let unpacked = work_directory.join("package");
    run(
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Expand-Archive -LiteralPath {} -DestinationPath {}",
                powershell_literal(package),
                powershell_literal(&unpacked)
            )),
        "The update could not be unpacked.",
    )?;
    let install = unpacked.join("install.ps1");
    if !install.is_file() {
        return Err(UpdateError::new("The downloaded package is incomplete."));
    }
    // Versions install side by side; the old one is removed after it exits.
    let new_root = old_root
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| UpdateError::new("The installed app has no folder."))?
        .join(update.version.to_string())
        .join(update.target.package_architecture());
    if !new_root.join("butter-paper.exe").is_file() {
        run(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
                .arg(&install),
            "The new version could not be installed.",
        )?;
    }
    if !new_root.join("butter-paper.exe").is_file() {
        return Err(UpdateError::new("The new version did not install where expected."));
    }
    let _ = fs::remove_file(package);
    Ok(Handover::Windows {
        old_root: old_root.to_path_buf(),
        new_root,
    })
}

fn prepare_linux(
    update: &AvailableUpdate,
    package: &Path,
    work_directory: &Path,
    old_root: &Path,
) -> Result<Handover, UpdateError> {
    run(
        Command::new("tar").arg("-xJf").arg(package).arg("-C").arg(work_directory),
        "The update could not be unpacked.",
    )?;
    let directory = format!(
        "butter-paper-linux-{}-{}",
        update.target.package_architecture(),
        update.version
    );
    let package_directory = work_directory.join(directory);
    if !package_directory.join("install-user.sh").is_file()
        || !package_directory.join("butter-paper").is_file()
    {
        return Err(UpdateError::new("The downloaded package is incomplete."));
    }
    let new_executable = old_root
        .parent()
        .ok_or_else(|| UpdateError::new("The installed app has no folder."))?
        .join(update.version.to_string())
        .join("butter-paper");
    let _ = fs::remove_file(package);
    Ok(Handover::Linux {
        old_root: old_root.to_path_buf(),
        package: package_directory,
        new_executable,
    })
}

fn powershell_literal(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn shell_literal(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

impl PreparedUpdate {
    /// The script that completes the update after `process_id` exits.
    pub fn handover_script(&self, process_id: u32, relaunch: bool) -> String {
        let work = &self.work_directory;
        match &self.handover {
            Handover::Macos { staged, destination, retired } => {
                macos_script(process_id, relaunch, work, staged, destination, retired.as_deref())
            }
            Handover::Windows { old_root, new_root } => windows_script(process_id, relaunch, work, old_root, new_root),
            Handover::Linux { old_root, package, new_executable } => {
                linux_script(process_id, relaunch, work, old_root, package, new_executable)
            }
        }
    }

    /// Writes the handover script and starts it detached from this process.
    pub fn spawn_handover(&self, relaunch: bool) -> io::Result<()> {
        let script = self.handover_script(std::process::id(), relaunch);
        let mut command = if matches!(self.handover, Handover::Windows { .. }) {
            let path = self.work_directory.join("handover.ps1");
            fs::write(&path, script)?;
            let mut command = Command::new("powershell.exe");
            command
                .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-WindowStyle", "Hidden", "-File"])
                .arg(path);
            command
        } else {
            let path = self.work_directory.join("handover.sh");
            fs::write(&path, script)?;
            let mut command = Command::new("/bin/sh");
            command.arg(path);
            command
        };
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            // A new session, so the script survives this process exiting.
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
        }
        command.spawn().map(drop)
    }
}

fn macos_script(
    pid: u32,
    relaunch: bool,
    work: &Path,
    staged: &Path,
    destination: &Path,
    retired: Option<&Path>,
) -> String {
    let previous = work.join("previous.app");
    let retired = retired.map(shell_literal).unwrap_or_default();
    format!(
        r#"#!/bin/sh
# Completes a Butter Paper update once the running app has exited.
pid={pid}
work={work}
staged={staged}
destination={destination}
previous={previous}
retired={retired}
relaunch={relaunch}
while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
rm -rf "$previous"
if [ ! -e "$destination" ] || mv "$destination" "$previous"; then
  if mv "$staged" "$destination"; then
    rm -rf "$previous"
    if [ -n "$retired" ]; then rm -rf "$retired"; fi
  elif [ -e "$previous" ]; then
    mv "$previous" "$destination"
  fi
fi
rm -rf "$staged"
if [ "$relaunch" = 1 ]; then /usr/bin/open -n "$destination"; fi
rm -rf "$work"
"#,
        work = shell_literal(work),
        staged = shell_literal(staged),
        destination = shell_literal(destination),
        previous = shell_literal(&previous),
        relaunch = u8::from(relaunch),
    )
}

fn linux_script(
    pid: u32,
    relaunch: bool,
    work: &Path,
    old_root: &Path,
    package: &Path,
    new_executable: &Path,
) -> String {
    let backup = work.join("previous");
    format!(
        r#"#!/bin/sh
# Completes a Butter Paper update once the running app has exited.
pid={pid}
work={work}
old={old}
package={package}
new={new}
backup={backup}
relaunch={relaunch}
while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done
launch="$old/butter-paper"
rm -rf "$backup"
if cp -a "$old" "$backup" && sh "$old/uninstall-user.sh"; then
  if sh "$package/install-user.sh"; then
    launch="$new"
  else
    # Restore the previous version from its own copy of the package.
    sh "$backup/install-user.sh"
  fi
fi
if [ "$relaunch" = 1 ]; then setsid "$launch" >/dev/null 2>&1 < /dev/null & fi
rm -rf "$work"
"#,
        work = shell_literal(work),
        old = shell_literal(old_root),
        package = shell_literal(package),
        new = shell_literal(new_executable),
        backup = shell_literal(&backup),
        relaunch = u8::from(relaunch),
    )
}

fn windows_script(pid: u32, relaunch: bool, work: &Path, old_root: &Path, new_root: &Path) -> String {
    format!(
        r#"# Completes a Butter Paper update once the running app has exited.
$ErrorActionPreference = 'Continue'
$processId = {pid}
$work = {work}
$oldRoot = {old_root}
$newRoot = {new_root}
$relaunch = ${relaunch}
while (Get-Process -Id $processId -ErrorAction SilentlyContinue) {{ Start-Sleep -Milliseconds 200 }}
$oldExe = Join-Path $oldRoot 'butter-paper.exe'
$newExe = Join-Path $newRoot 'butter-paper.exe'
$launch = $newExe
# A default-app choice names a ProgID. Keep the user's choice working by
# pointing any Butter Paper ProgID that opens the old copy at the new one.
$choice = $null
$userChoice = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\FileExts\.pdf\UserChoice' -ErrorAction SilentlyContinue
if ($null -ne $userChoice) {{ $choice = $userChoice.ProgId }}
if ($choice -like 'ButterPaper.PDF.*') {{
  $key = 'HKCU:\Software\Classes\' + $choice
  $command = (Get-ItemProperty -LiteralPath ($key + '\shell\open\command') -ErrorAction SilentlyContinue).'(default)'
  if ($command -eq ('"' + $oldExe + '" "%1"')) {{
    Set-ItemProperty -LiteralPath ($key + '\shell\open\command') -Name '(default)' -Value ('"' + $newExe + '" "%1"')
    Set-ItemProperty -LiteralPath ($key + '\DefaultIcon') -Name '(default)' -Value ('"' + (Join-Path $newRoot 'butter-paper.ico') + '"') -ErrorAction SilentlyContinue
  }}
}}
$uninstall = Join-Path $oldRoot 'uninstall.ps1'
if (Test-Path -LiteralPath $uninstall) {{
  try {{ & $uninstall | Out-Null }} catch {{ }}
}}
if ($relaunch) {{ Start-Process -FilePath $launch -WorkingDirectory $newRoot }}
Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
"#,
        work = powershell_literal(work),
        old_root = powershell_literal(old_root),
        new_root = powershell_literal(new_root),
        relaunch = if relaunch { "true" } else { "false" },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, prerelease: bool, names: &[String]) -> Release {
        Release {
            version: ReleaseVersion::parse(tag).unwrap(),
            prerelease,
            page_url: format!("https://github.com/apotenza92/butter-paper/releases/tag/{tag}"),
            assets: names
                .iter()
                .map(|name| ReleaseAsset {
                    name: name.clone(),
                    url: format!("{RELEASE_DOWNLOAD_PREFIX}{tag}/{name}"),
                    size: 1_000,
                })
                .collect(),
        }
    }

    fn assets(target: UpdateTarget, identities: &[PackageIdentity]) -> Vec<String> {
        identities
            .iter()
            .map(|identity| target.asset_name(*identity))
            .chain([CHECKSUMS_ASSET_NAME.to_owned()])
            .collect()
    }

    const MAC: UpdateTarget = UpdateTarget::MacosArm64;
    const CURRENT: ReleaseVersion = ReleaseVersion::stable(0, 0, 31);

    #[test]
    fn versions_parse_and_order_numerically() {
        assert_eq!(ReleaseVersion::parse("v0.0.31"), Some(ReleaseVersion::stable(0, 0, 31)));
        assert_eq!(ReleaseVersion::parse("1.2.3"), Some(ReleaseVersion::stable(1, 2, 3)));
        assert!(ReleaseVersion::parse("0.0.31-beta").is_none());
        assert!(ReleaseVersion::parse("0.0.31-beta.0").is_none());
        assert!(ReleaseVersion::parse("0.0.31-beta.01").is_none());
        assert!(ReleaseVersion::parse("0.0.31-rc.1").is_none());
        let beta = ReleaseVersion::parse("v0.0.33-beta.2").unwrap();
        assert_eq!(beta, ReleaseVersion::beta(0, 0, 33, 2));
        assert_eq!(beta.to_string(), "0.0.33-beta.2");
        assert!(beta.is_beta());
        // A release sorts above its own betas and below the next ones.
        assert!(ReleaseVersion::stable(0, 0, 32) < ReleaseVersion::beta(0, 0, 33, 1));
        assert!(ReleaseVersion::beta(0, 0, 33, 1) < ReleaseVersion::beta(0, 0, 33, 10));
        assert!(ReleaseVersion::beta(0, 0, 33, 10) < ReleaseVersion::stable(0, 0, 33));
        assert!(ReleaseVersion::parse("0.31").is_none());
        assert!(ReleaseVersion::stable(0, 0, 10) > ReleaseVersion::stable(0, 0, 9));
        assert_eq!(ReleaseVersion::stable(0, 0, 31).to_string(), "0.0.31");
    }

    #[test]
    fn macos_bundle_versions_match_the_release_workflow() {
        // The workflow writes these into Info.plist; the updater checks them.
        assert_eq!(ReleaseVersion::stable(0, 1, 0).core(), "0.1.0");
        assert_eq!(ReleaseVersion::stable(0, 1, 0).build_number(), 100_090_000);
        let beta = ReleaseVersion::beta(0, 1, 1, 2);
        assert_eq!(beta.core(), "0.1.1");
        assert_eq!(beta.build_number(), 100_100_002);
        assert!(beta.build_number() < ReleaseVersion::stable(0, 1, 1).build_number());
        assert_eq!(ReleaseVersion::stable(1, 2, 3).build_number(), 100_200_390_000);
    }

    #[test]
    fn feed_skips_drafts_and_unversioned_tags() {
        let feed = br#"[
            {"tag_name":"v0.0.32","prerelease":true,"assets":[]},
            {"tag_name":"v0.0.33","draft":true,"assets":[]},
            {"tag_name":"electron-final","assets":[]},
            {"tag_name":"v0.0.31","html_url":"https://x","assets":[{"name":"SHA256SUMS.txt","browser_download_url":"u","size":5}]}
        ]"#;
        let releases = parse_releases(feed).unwrap();
        assert_eq!(
            releases.iter().map(|release| (release.version, release.prerelease)).collect::<Vec<_>>(),
            [(ReleaseVersion::stable(0, 0, 32), true), (ReleaseVersion::stable(0, 0, 31), false)]
        );
        assert!(parse_releases(b"not json").is_err());
        // Beta tags as Butter Paper's other apps and the Homebrew tap name
        // them; one missing the prerelease flag is still a beta.
        let betas = parse_releases(
            br#"[{"tag_name":"v0.0.33-beta.1","prerelease":false,"assets":[]},
                {"tag_name":"v0.0.33-beta.2","prerelease":true,"assets":[]}]"#,
        )
        .unwrap();
        assert_eq!(
            betas.iter().map(|release| (release.version, release.prerelease)).collect::<Vec<_>>(),
            [(ReleaseVersion::beta(0, 0, 33, 1), true), (ReleaseVersion::beta(0, 0, 33, 2), true)]
        );
    }

    #[test]
    fn checksums_parse_sha256sum_output() {
        let hash = "a".repeat(64);
        let sums = parse_checksums(&format!(
            "{hash}  Butter-Paper-macOS-arm64.zip\n{hash} *Butter-Paper-Linux-x64.tar.xz\nnot a line\n{}  short.zip\n",
            "b".repeat(10)
        ));
        assert_eq!(sums.get("Butter-Paper-macOS-arm64.zip"), Some(&hash));
        assert_eq!(sums.get("Butter-Paper-Linux-x64.tar.xz"), Some(&hash));
        assert_eq!(sums.len(), 2);
    }

    #[test]
    fn stable_copies_take_the_newest_complete_stable_release() {
        let stable = assets(MAC, &[PackageIdentity::Stable]);
        let releases = [
            release("v0.0.34", true, &assets(MAC, &[PackageIdentity::Beta])),
            // Electron-only handover releases carry no native package.
            release("v0.0.33", false, &[CHECKSUMS_ASSET_NAME.to_owned()]),
            release("v0.0.32", false, &stable),
            release("v0.0.31", false, &stable),
        ];
        let update = select_update(CURRENT, UpdateChannel::Stable, &releases, MAC).unwrap();
        assert_eq!((update.version, update.identity), (ReleaseVersion::stable(0, 0, 32), PackageIdentity::Stable));
        assert!(select_update(CURRENT, UpdateChannel::Stable, &releases[3..], MAC).is_none());
    }

    #[test]
    fn beta_copies_take_newer_betas_and_newer_stable_releases() {
        let beta = assets(MAC, &[PackageIdentity::Beta]);
        let stable = assets(MAC, &[PackageIdentity::Stable]);
        // A beta newer than any stable release.
        let update = select_update(
            CURRENT,
            UpdateChannel::Beta,
            &[release("v0.0.33", true, &beta), release("v0.0.32", false, &stable)],
            MAC,
        )
        .unwrap();
        assert_eq!((update.version, update.identity), (ReleaseVersion::stable(0, 0, 33), PackageIdentity::Beta));
        // A stable release newer than the betas moves the copy to stable.
        let update = select_update(
            CURRENT,
            UpdateChannel::Beta,
            &[release("v0.0.32", true, &beta), release("v0.0.34", false, &stable)],
            MAC,
        )
        .unwrap();
        assert_eq!((update.version, update.identity), (ReleaseVersion::stable(0, 0, 34), PackageIdentity::Stable));
        // A stable release that also ships a beta package keeps the beta identity.
        let both = assets(MAC, &[PackageIdentity::Stable, PackageIdentity::Beta]);
        let update = select_update(CURRENT, UpdateChannel::Beta, &[release("v0.0.34", false, &both)], MAC).unwrap();
        assert_eq!(update.identity, PackageIdentity::Beta);
        // A prerelease with only a stable package is not offered to anyone.
        assert!(select_update(CURRENT, UpdateChannel::Beta, &[release("v0.0.35", true, &stable)], MAC).is_none());
    }

    #[test]
    fn downloads_from_other_hosts_or_tags_are_refused() {
        let names = assets(MAC, &[PackageIdentity::Stable]);
        let mut foreign = release("v0.0.32", false, &names);
        foreign.assets[0].url = "https://example.com/Butter-Paper-macOS-arm64.zip".to_owned();
        assert!(select_update(CURRENT, UpdateChannel::Stable, &[foreign], MAC).is_none());
        let mut other_tag = release("v0.0.32", false, &names);
        other_tag.assets[1].url = format!("{RELEASE_DOWNLOAD_PREFIX}v0.0.26/SHA256SUMS.txt");
        assert!(select_update(CURRENT, UpdateChannel::Stable, &[other_tag], MAC).is_none());
    }

    #[test]
    fn package_names_and_signing_requirements_follow_identity() {
        assert_eq!(UpdateTarget::LinuxX64.asset_name(PackageIdentity::Stable), "Butter-Paper-Linux-x64.tar.xz");
        assert_eq!(UpdateTarget::WindowsArm64.asset_name(PackageIdentity::Beta), "Butter-Paper-Beta-Windows-arm64.zip");
        assert!(PackageIdentity::Beta.macos_designated_requirement().contains("\"com.butterpaper.desktop.beta\""));
        assert!(PackageIdentity::Stable.macos_designated_requirement().contains("\"com.butterpaper.desktop\" and"));
        assert!(PackageIdentity::Stable.macos_designated_requirement().contains("\"27JL2VERNC\""));
    }

    #[test]
    fn verified_writes_require_exact_size_and_hash() {
        let directory = std::env::temp_dir().join(format!("bp-updater-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let bytes = b"butter paper package";
        let hash = hex(&Sha256::digest(bytes));
        let good = directory.join("good");
        write_verified(&bytes[..], &good, bytes.len() as u64, &hash).unwrap();
        assert_eq!(fs::read(&good).unwrap(), bytes);
        assert!(write_verified(&bytes[..], &directory.join("long"), 4, &hash).is_err());
        assert!(write_verified(&bytes[..], &directory.join("hash"), bytes.len() as u64, &"0".repeat(64)).is_err());
        // Never overwrites an existing file.
        assert!(write_verified(&bytes[..], &good, bytes.len() as u64, &hash).is_err());
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn handover_scripts_wait_swap_and_quote_paths() {
        let prepared = PreparedUpdate {
            version: ReleaseVersion::stable(0, 0, 32),
            work_directory: PathBuf::from("/tmp/it's work"),
            handover: Handover::Macos {
                staged: PathBuf::from("/Applications/.Butter Paper 0.0.32 update.app"),
                destination: PathBuf::from("/Applications/Butter Paper.app"),
                retired: None,
            },
        };
        let script = prepared.handover_script(42, true);
        assert!(script.contains("pid=42"));
        assert!(script.contains(r"work='/tmp/it'\''s work'"));
        assert!(script.contains("/usr/bin/open -n \"$destination\""));
        let windows = PreparedUpdate {
            handover: Handover::Windows {
                old_root: PathBuf::from(r"C:\Users\a\AppData\Local\Programs\Butter Paper\0.0.31\arm64"),
                new_root: PathBuf::from(r"C:\Users\a\AppData\Local\Programs\Butter Paper\0.0.32\arm64"),
            },
            ..prepared.clone()
        }
        .handover_script(42, false);
        assert!(windows.contains("$relaunch = $false"));
        assert!(windows.contains("UserChoice"));
    }

    /// Runs the real macOS handover script against stand-in bundles.
    #[cfg(unix)]
    #[test]
    fn macos_handover_swaps_bundles_and_retires_a_beta() {
        let root = std::env::temp_dir().join(format!("bp-handover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let applications = root.join("Applications");
        let work = root.join("work");
        fs::create_dir_all(applications.join("Butter Paper Beta.app")).unwrap();
        fs::create_dir_all(applications.join(".Butter Paper 0.0.32 update.app")).unwrap();
        fs::write(applications.join(".Butter Paper 0.0.32 update.app/new"), b"").unwrap();
        fs::create_dir_all(&work).unwrap();
        let prepared = PreparedUpdate {
            version: ReleaseVersion::stable(0, 0, 32),
            work_directory: work.clone(),
            handover: Handover::Macos {
                staged: applications.join(".Butter Paper 0.0.32 update.app"),
                destination: applications.join("Butter Paper.app"),
                retired: Some(applications.join("Butter Paper Beta.app")),
            },
        };
        // An already-exited process id, and no relaunch.
        let script = prepared.handover_script(999_999, false);
        let status = Command::new("/bin/sh").arg("-c").arg(script).status().unwrap();
        assert!(status.success());
        assert!(applications.join("Butter Paper.app/new").is_file());
        assert!(!applications.join("Butter Paper Beta.app").exists());
        assert!(!applications.join(".Butter Paper 0.0.32 update.app").exists());
        assert!(!work.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    /// Installs the real latest stable macOS package over a stand-in app in a
    /// temporary folder: feed, checksums, download, Developer ID and
    /// Gatekeeper checks, staging and the handover swap. Needs the network.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn real_release_updates_a_stand_in_install() {
        let target = UpdateTarget::current().unwrap();
        let releases = fetch_releases(RELEASE_FEED_URL).unwrap();
        let update = select_update(ReleaseVersion::stable(0, 0, 25), UpdateChannel::Stable, &releases, target)
            .expect("a newer stable macOS release is published");
        let root = std::env::temp_dir().join(format!("bp-real-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let applications = root.join("Applications");
        let bundle = applications.join("Butter Paper.app");
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        let work = root.join("work");
        fs::create_dir_all(&work).unwrap();
        let package = download_update(&update, &work).unwrap();
        let prepared = prepare_update(&update, &package, &work, &Installation::Macos { bundle: bundle.clone() })
            .unwrap();
        let status = Command::new("/bin/sh")
            .arg("-c")
            .arg(prepared.handover_script(999_999, false))
            .status()
            .unwrap();
        assert!(status.success());
        let version = Command::new("/usr/bin/plutil")
            .args(["-extract", "CFBundleShortVersionString", "raw", "-o", "-"])
            .arg(bundle.join("Contents/Info.plist"))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&version.stdout).trim(), update.version.to_string());
        let verified = Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict", "-R"])
            .arg(PackageIdentity::Stable.macos_designated_requirement())
            .arg(&bundle)
            .status()
            .unwrap();
        assert!(verified.success());
        assert!(!work.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    /// Updates a stand-in Linux install to the real latest package under a
    /// throwaway home, then checks a failing next version rolls back. The
    /// "installed" version is the real package relabelled one version lower.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore]
    fn real_release_updates_and_rolls_back_a_stand_in_linux_install() {
        let target = UpdateTarget::current().unwrap();
        let releases = fetch_releases(RELEASE_FEED_URL).unwrap();
        let update = select_update(ReleaseVersion::stable(0, 0, 25), UpdateChannel::Stable, &releases, target)
            .expect("a newer stable Linux release is published");
        let new = update.version.to_string();
        let old = ReleaseVersion(update.version.0, update.version.1, update.version.2 - 1).to_string();
        let root = std::env::temp_dir().join(format!("bp-linux-update-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let data = home.join(".local/share");
        fs::create_dir_all(&data).unwrap();
        let sh = |script: &str| {
            Command::new("/bin/sh")
                .arg("-c")
                .arg(script)
                .env("HOME", &home)
                .env("XDG_DATA_HOME", &data)
                .status()
                .unwrap()
                .success()
        };
        let work = root.join("work");
        fs::create_dir_all(&work).unwrap();
        let package = download_update(&update, &work).unwrap();
        // Install the same package relabelled as the previous version.
        let fake = root.join("fake-old");
        fs::create_dir_all(&fake).unwrap();
        assert!(sh(&format!(
            "tar -xJf {} -C {fake} && cd {fake}/butter-paper-linux-* && sed -i 's#/{new}#/{old}#g' install-user.sh uninstall-user.sh && sh install-user.sh",
            shell_literal(&package),
            fake = shell_literal(&fake),
        )));
        let old_root = data.join("butter-paper").join(&old);
        assert!(old_root.join("butter-paper").is_file());
        let installation = Installation::Linux { root: old_root.clone(), version: old.clone() };
        let prepared = prepare_update(&update, &package, &work, &installation).unwrap();
        assert!(sh(&prepared.handover_script(999_999, false)));
        let new_root = data.join("butter-paper").join(&new);
        assert!(new_root.join("butter-paper").is_file());
        assert!(!old_root.exists());
        let launcher = fs::read_to_string(home.join(".local/bin/butter-paper")).unwrap();
        assert!(launcher.contains(&format!("/{new}/")));
        assert!(!work.exists());

        // A next version whose install fails leaves the current one working.
        let broken_work = root.join("broken-work");
        let broken_package = broken_work.join("package");
        fs::create_dir_all(&broken_package).unwrap();
        fs::write(broken_package.join("install-user.sh"), "exit 1\n").unwrap();
        let broken = PreparedUpdate {
            version: ReleaseVersion(update.version.0, update.version.1, update.version.2 + 1),
            work_directory: broken_work.clone(),
            handover: Handover::Linux {
                old_root: new_root.clone(),
                package: broken_package,
                new_executable: data.join("butter-paper/unused/butter-paper"),
            },
        };
        sh(&broken.handover_script(999_999, false));
        assert!(new_root.join("butter-paper").is_file(), "the previous version was restored");
        let launcher = fs::read_to_string(home.join(".local/bin/butter-paper")).unwrap();
        assert!(launcher.contains(&format!("/{new}/")));
        fs::remove_dir_all(&root).unwrap();
    }

    /// Writes the Windows handover script for a manual run on a Windows host:
    /// `BP_HANDOVER_OUT`, `BP_HANDOVER_PID`, `BP_HANDOVER_WORK`,
    /// `BP_HANDOVER_OLD` and `BP_HANDOVER_NEW` name its inputs.
    #[test]
    #[ignore]
    fn write_windows_handover_for_manual_review() {
        let var = |name: &str| std::env::var(name).unwrap();
        let prepared = PreparedUpdate {
            version: ReleaseVersion::stable(0, 0, 0),
            work_directory: PathBuf::from(var("BP_HANDOVER_WORK")),
            handover: Handover::Windows {
                old_root: PathBuf::from(var("BP_HANDOVER_OLD")),
                new_root: PathBuf::from(var("BP_HANDOVER_NEW")),
            },
        };
        let script = prepared.handover_script(var("BP_HANDOVER_PID").parse().unwrap(), false);
        fs::write(var("BP_HANDOVER_OUT"), script).unwrap();
    }
}
