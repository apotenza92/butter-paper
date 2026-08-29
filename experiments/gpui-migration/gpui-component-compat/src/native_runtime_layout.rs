//! Resolves fixed package-relative native-process entries before GPUI starts.
//!
//! Canonical package containment and code-signature validation belong to the
//! packaged-candidate qualification gate and are intentionally not claimed by
//! this development/runtime path resolver.

use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::{Path, PathBuf},
};

#[cfg(any(test, target_os = "linux", target_os = "macos"))]
const WORKER_BASENAME: &str = "butter-paper-pdf-worker";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeDevelopmentAuthorityError;

impl fmt::Display for NativeDevelopmentAuthorityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BP_NATIVE_DEVELOPMENT must be exactly 1")
    }
}

impl std::error::Error for NativeDevelopmentAuthorityError {}

pub fn require_explicit_development_authority(
    value: Option<&OsStr>,
) -> Result<(), NativeDevelopmentAuthorityError> {
    if value == Some(OsStr::new("1")) {
        Ok(())
    } else {
        Err(NativeDevelopmentAuthorityError)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeRuntimeMode {
    Bundled,
    Development { pdfium_library: PathBuf },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PdfiumRuntimeSource {
    Bundled,
    ExplicitDevelopmentOverride,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRuntimeLayout {
    worker_executable: PathBuf,
    pdfium_library: PathBuf,
    pdfium_source: PdfiumRuntimeSource,
}

impl NativeRuntimeLayout {
    pub fn discover(mode: NativeRuntimeMode) -> Result<Self, NativeRuntimeLayoutError> {
        let executable = std::env::current_exe().map_err(|error| {
            NativeRuntimeLayoutError::CurrentExecutableUnavailable(error.to_string())
        })?;
        resolve_layout(
            NativePlatform::CURRENT,
            &executable,
            mode,
            &FilesystemPathProbe,
        )
    }

    pub fn worker_executable(&self) -> &Path {
        &self.worker_executable
    }

    pub fn pdfium_library(&self) -> &Path {
        &self.pdfium_library
    }

    pub fn pdfium_source(&self) -> PdfiumRuntimeSource {
        self.pdfium_source
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeRuntimePathRole {
    ApplicationExecutable,
    PdfWorker,
    PdfiumLibrary,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeRuntimeLayoutError {
    CurrentExecutableUnavailable(String),
    PathNotAbsolute {
        role: NativeRuntimePathRole,
        path: PathBuf,
    },
    InvalidMacApplicationLayout(PathBuf),
    InvalidPdfiumBasename {
        expected: &'static str,
        actual: Option<OsString>,
    },
    Missing {
        role: NativeRuntimePathRole,
        path: PathBuf,
    },
    Symlink {
        role: NativeRuntimePathRole,
        path: PathBuf,
    },
    NotRegularFile {
        role: NativeRuntimePathRole,
        path: PathBuf,
    },
    NotExecutable {
        role: NativeRuntimePathRole,
        path: PathBuf,
    },
    InspectionFailed {
        role: NativeRuntimePathRole,
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for NativeRuntimeLayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CurrentExecutableUnavailable(message) => {
                write!(formatter, "current executable is unavailable: {message}")
            }
            Self::PathNotAbsolute { role, path } => {
                write!(formatter, "{role} path is not absolute: {}", path.display())
            }
            Self::InvalidMacApplicationLayout(path) => write!(
                formatter,
                "macOS application executable is not under Contents/MacOS: {}",
                path.display()
            ),
            Self::InvalidPdfiumBasename { expected, actual } => write!(
                formatter,
                "PDFium override basename must be {expected}, got {}",
                actual
                    .as_deref()
                    .map(Path::new)
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "<none>".to_owned())
            ),
            Self::Missing { role, path } => {
                write!(formatter, "{role} is missing: {}", path.display())
            }
            Self::Symlink { role, path } => {
                write!(
                    formatter,
                    "{role} must not be a symlink: {}",
                    path.display()
                )
            }
            Self::NotRegularFile { role, path } => write!(
                formatter,
                "{role} is not a regular file: {}",
                path.display()
            ),
            Self::NotExecutable { role, path } => {
                write!(formatter, "{role} is not executable: {}", path.display())
            }
            Self::InspectionFailed {
                role,
                path,
                message,
            } => write!(
                formatter,
                "failed to inspect {role} at {}: {message}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for NativeRuntimeLayoutError {}

impl fmt::Display for NativeRuntimePathRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ApplicationExecutable => "application executable",
            Self::PdfWorker => "PDF worker",
            Self::PdfiumLibrary => "PDFium library",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativePlatform {
    #[cfg(any(test, target_os = "linux"))]
    Linux,
    #[cfg(any(test, target_os = "macos"))]
    MacOs,
    #[cfg(any(test, target_os = "windows"))]
    Windows,
}

impl NativePlatform {
    #[cfg(target_os = "linux")]
    const CURRENT: Self = Self::Linux;
    #[cfg(target_os = "macos")]
    const CURRENT: Self = Self::MacOs;
    #[cfg(target_os = "windows")]
    const CURRENT: Self = Self::Windows;

    fn worker_basename(self) -> &'static str {
        match self {
            #[cfg(any(test, target_os = "windows"))]
            Self::Windows => "butter-paper-pdf-worker.exe",
            #[cfg(any(test, target_os = "linux"))]
            Self::Linux => WORKER_BASENAME,
            #[cfg(any(test, target_os = "macos"))]
            Self::MacOs => WORKER_BASENAME,
        }
    }

    fn pdfium_basename(self) -> &'static str {
        match self {
            #[cfg(any(test, target_os = "linux"))]
            Self::Linux => "libpdfium.so",
            #[cfg(any(test, target_os = "macos"))]
            Self::MacOs => "libpdfium.dylib",
            #[cfg(any(test, target_os = "windows"))]
            Self::Windows => "pdfium.dll",
        }
    }

    fn requires_unix_execute_bit(self) -> bool {
        match self {
            #[cfg(any(test, target_os = "linux"))]
            Self::Linux => true,
            #[cfg(any(test, target_os = "macos"))]
            Self::MacOs => true,
            #[cfg(any(test, target_os = "windows"))]
            Self::Windows => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PathKind {
    RegularFile { executable: bool },
    Symlink,
    Other,
}

trait PathProbe {
    fn inspect(&self, path: &Path) -> std::io::Result<Option<PathKind>>;
}

struct FilesystemPathProbe;

impl PathProbe for FilesystemPathProbe {
    fn inspect(&self, path: &Path) -> std::io::Result<Option<PathKind>> {
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() {
            return Ok(Some(PathKind::Symlink));
        }
        if !metadata.file_type().is_file() {
            return Ok(Some(PathKind::Other));
        }
        #[cfg(unix)]
        let executable = {
            use std::os::unix::fs::PermissionsExt as _;
            metadata.permissions().mode() & 0o111 != 0
        };
        #[cfg(not(unix))]
        let executable = true;
        Ok(Some(PathKind::RegularFile { executable }))
    }
}

fn resolve_layout(
    platform: NativePlatform,
    executable: &Path,
    mode: NativeRuntimeMode,
    probe: &impl PathProbe,
) -> Result<NativeRuntimeLayout, NativeRuntimeLayoutError> {
    validate_absolute(NativeRuntimePathRole::ApplicationExecutable, executable)?;
    validate_regular_file(
        NativeRuntimePathRole::ApplicationExecutable,
        executable,
        platform.requires_unix_execute_bit(),
        probe,
    )?;
    let executable_directory =
        executable
            .parent()
            .ok_or_else(|| NativeRuntimeLayoutError::NotRegularFile {
                role: NativeRuntimePathRole::ApplicationExecutable,
                path: executable.to_owned(),
            })?;
    let worker_executable = executable_directory.join(platform.worker_basename());
    let (pdfium_library, pdfium_source) = match mode {
        NativeRuntimeMode::Bundled => (
            bundled_pdfium_path(platform, executable, executable_directory)?,
            PdfiumRuntimeSource::Bundled,
        ),
        NativeRuntimeMode::Development { pdfium_library } => {
            validate_absolute(NativeRuntimePathRole::PdfiumLibrary, &pdfium_library)?;
            if pdfium_library.file_name() != Some(platform.pdfium_basename().as_ref()) {
                return Err(NativeRuntimeLayoutError::InvalidPdfiumBasename {
                    expected: platform.pdfium_basename(),
                    actual: pdfium_library.file_name().map(OsString::from),
                });
            }
            (
                pdfium_library,
                PdfiumRuntimeSource::ExplicitDevelopmentOverride,
            )
        }
    };
    validate_regular_file(
        NativeRuntimePathRole::PdfWorker,
        &worker_executable,
        platform.requires_unix_execute_bit(),
        probe,
    )?;
    validate_regular_file(
        NativeRuntimePathRole::PdfiumLibrary,
        &pdfium_library,
        false,
        probe,
    )?;
    Ok(NativeRuntimeLayout {
        worker_executable,
        pdfium_library,
        pdfium_source,
    })
}

fn bundled_pdfium_path(
    platform: NativePlatform,
    executable: &Path,
    executable_directory: &Path,
) -> Result<PathBuf, NativeRuntimeLayoutError> {
    match platform {
        #[cfg(any(test, target_os = "linux"))]
        NativePlatform::Linux => Ok(executable_directory.join(platform.pdfium_basename())),
        #[cfg(any(test, target_os = "windows"))]
        NativePlatform::Windows => Ok(executable_directory.join(platform.pdfium_basename())),
        #[cfg(any(test, target_os = "macos"))]
        NativePlatform::MacOs => {
            let contents = executable_directory.parent().filter(|contents| {
                executable_directory.file_name() == Some("MacOS".as_ref())
                    && contents.file_name() == Some("Contents".as_ref())
            });
            let Some(contents) = contents else {
                return Err(NativeRuntimeLayoutError::InvalidMacApplicationLayout(
                    executable.to_owned(),
                ));
            };
            Ok(contents.join("Frameworks").join(platform.pdfium_basename()))
        }
    }
}

fn validate_absolute(
    role: NativeRuntimePathRole,
    path: &Path,
) -> Result<(), NativeRuntimeLayoutError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(NativeRuntimeLayoutError::PathNotAbsolute {
            role,
            path: path.to_owned(),
        })
    }
}

fn validate_regular_file(
    role: NativeRuntimePathRole,
    path: &Path,
    require_executable: bool,
    probe: &impl PathProbe,
) -> Result<(), NativeRuntimeLayoutError> {
    match probe.inspect(path) {
        Ok(None) => Err(NativeRuntimeLayoutError::Missing {
            role,
            path: path.to_owned(),
        }),
        Ok(Some(PathKind::Symlink)) => Err(NativeRuntimeLayoutError::Symlink {
            role,
            path: path.to_owned(),
        }),
        Ok(Some(PathKind::Other)) => Err(NativeRuntimeLayoutError::NotRegularFile {
            role,
            path: path.to_owned(),
        }),
        Ok(Some(PathKind::RegularFile { executable: false })) if require_executable => {
            Err(NativeRuntimeLayoutError::NotExecutable {
                role,
                path: path.to_owned(),
            })
        }
        Ok(Some(PathKind::RegularFile { .. })) => Ok(()),
        Err(error) => Err(NativeRuntimeLayoutError::InspectionFailed {
            role,
            path: path.to_owned(),
            message: error.to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct FakeProbe(HashMap<PathBuf, PathKind>);

    impl FakeProbe {
        fn regular(mut self, path: impl Into<PathBuf>, executable: bool) -> Self {
            self.0
                .insert(path.into(), PathKind::RegularFile { executable });
            self
        }

        fn with_kind(mut self, path: impl Into<PathBuf>, kind: PathKind) -> Self {
            self.0.insert(path.into(), kind);
            self
        }
    }

    impl PathProbe for FakeProbe {
        fn inspect(&self, path: &Path) -> std::io::Result<Option<PathKind>> {
            Ok(self.0.get(path).copied())
        }
    }

    #[test]
    fn bundled_linux_layout_uses_only_exact_sibling_entries() {
        let executable = Path::new("/opt/butter-paper/component_story");
        let worker = Path::new("/opt/butter-paper/butter-paper-pdf-worker");
        let pdfium = Path::new("/opt/butter-paper/libpdfium.so");
        let probe = FakeProbe::default()
            .regular(executable, true)
            .regular(worker, true)
            .regular(pdfium, false);

        let layout = resolve_layout(
            NativePlatform::Linux,
            executable,
            NativeRuntimeMode::Bundled,
            &probe,
        )
        .unwrap();

        assert_eq!(layout.worker_executable(), worker);
        assert_eq!(layout.pdfium_library(), pdfium);
        assert_eq!(layout.pdfium_source(), PdfiumRuntimeSource::Bundled);
    }

    #[test]
    fn performance_mode_requires_exact_explicit_development_authority() {
        assert_eq!(
            require_explicit_development_authority(None),
            Err(NativeDevelopmentAuthorityError)
        );
        assert_eq!(
            require_explicit_development_authority(Some(OsStr::new("true"))),
            Err(NativeDevelopmentAuthorityError)
        );
        assert_eq!(
            require_explicit_development_authority(Some(OsStr::new("1"))),
            Ok(())
        );
    }

    #[test]
    fn bundled_windows_layout_uses_exe_worker_and_dll_siblings() {
        let executable = Path::new("/package/component_story.exe");
        let worker = Path::new("/package/butter-paper-pdf-worker.exe");
        let pdfium = Path::new("/package/pdfium.dll");
        let probe = FakeProbe::default()
            .regular(executable, false)
            .regular(worker, false)
            .regular(pdfium, false);
        let layout = resolve_layout(
            NativePlatform::Windows,
            executable,
            NativeRuntimeMode::Bundled,
            &probe,
        )
        .unwrap();
        assert_eq!(layout.worker_executable(), worker);
        assert_eq!(layout.pdfium_library(), pdfium);
    }

    #[test]
    fn bundled_macos_layout_uses_macos_worker_and_frameworks_pdfium() {
        let executable = Path::new("/Applications/Butter Paper.app/Contents/MacOS/component_story");
        let worker =
            Path::new("/Applications/Butter Paper.app/Contents/MacOS/butter-paper-pdf-worker");
        let pdfium =
            Path::new("/Applications/Butter Paper.app/Contents/Frameworks/libpdfium.dylib");
        let probe = FakeProbe::default()
            .regular(executable, true)
            .regular(worker, true)
            .regular(pdfium, false);
        let layout = resolve_layout(
            NativePlatform::MacOs,
            executable,
            NativeRuntimeMode::Bundled,
            &probe,
        )
        .unwrap();
        assert_eq!(layout.worker_executable(), worker);
        assert_eq!(layout.pdfium_library(), pdfium);
    }

    #[test]
    fn non_app_macos_layout_requires_explicit_development_override() {
        let executable = Path::new("/tmp/debug/component_story");
        let probe = FakeProbe::default().regular(executable, true);
        assert_eq!(
            resolve_layout(
                NativePlatform::MacOs,
                executable,
                NativeRuntimeMode::Bundled,
                &probe,
            ),
            Err(NativeRuntimeLayoutError::InvalidMacApplicationLayout(
                executable.to_owned()
            ))
        );
    }

    #[test]
    fn development_override_is_absolute_and_has_exact_platform_basename() {
        let executable = Path::new("/tmp/debug/component_story");
        let worker = Path::new("/tmp/debug/butter-paper-pdf-worker");
        let pdfium = Path::new("/verified/pdfium/libpdfium.dylib");
        let probe = FakeProbe::default()
            .regular(executable, true)
            .regular(worker, true)
            .regular(pdfium, false);
        let layout = resolve_layout(
            NativePlatform::MacOs,
            executable,
            NativeRuntimeMode::Development {
                pdfium_library: pdfium.to_owned(),
            },
            &probe,
        )
        .unwrap();
        assert_eq!(
            layout.pdfium_source(),
            PdfiumRuntimeSource::ExplicitDevelopmentOverride
        );

        let error = resolve_layout(
            NativePlatform::Linux,
            Path::new("/tmp/debug/component_story"),
            NativeRuntimeMode::Development {
                pdfium_library: PathBuf::from("libpdfium.so"),
            },
            &FakeProbe::default().regular("/tmp/debug/component_story", true),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            NativeRuntimeLayoutError::PathNotAbsolute {
                role: NativeRuntimePathRole::PdfiumLibrary,
                ..
            }
        ));
    }

    #[test]
    fn selected_entries_reject_symlinks_and_non_executable_unix_workers() {
        let executable = Path::new("/opt/app/component_story");
        let worker = Path::new("/opt/app/butter-paper-pdf-worker");
        let pdfium = Path::new("/opt/app/libpdfium.so");
        let symlink_error = resolve_layout(
            NativePlatform::Linux,
            executable,
            NativeRuntimeMode::Bundled,
            &FakeProbe::default()
                .regular(executable, true)
                .regular(worker, true)
                .with_kind(pdfium, PathKind::Symlink),
        )
        .unwrap_err();
        assert!(matches!(
            symlink_error,
            NativeRuntimeLayoutError::Symlink {
                role: NativeRuntimePathRole::PdfiumLibrary,
                ..
            }
        ));

        let worker_error = resolve_layout(
            NativePlatform::Linux,
            executable,
            NativeRuntimeMode::Bundled,
            &FakeProbe::default()
                .regular(executable, true)
                .regular(worker, false)
                .regular(pdfium, false),
        )
        .unwrap_err();
        assert!(matches!(
            worker_error,
            NativeRuntimeLayoutError::NotExecutable {
                role: NativeRuntimePathRole::PdfWorker,
                ..
            }
        ));
    }

    #[test]
    fn missing_and_non_file_entries_return_typed_errors() {
        let executable = Path::new("/opt/app/component_story");
        let worker = Path::new("/opt/app/butter-paper-pdf-worker");
        let pdfium = Path::new("/opt/app/libpdfium.so");
        let missing = resolve_layout(
            NativePlatform::Linux,
            executable,
            NativeRuntimeMode::Bundled,
            &FakeProbe::default().regular(executable, true),
        )
        .unwrap_err();
        assert_eq!(
            missing,
            NativeRuntimeLayoutError::Missing {
                role: NativeRuntimePathRole::PdfWorker,
                path: worker.to_owned(),
            }
        );

        let not_file = resolve_layout(
            NativePlatform::Linux,
            executable,
            NativeRuntimeMode::Bundled,
            &FakeProbe::default()
                .regular(executable, true)
                .regular(worker, true)
                .with_kind(pdfium, PathKind::Other),
        )
        .unwrap_err();
        assert_eq!(
            not_file,
            NativeRuntimeLayoutError::NotRegularFile {
                role: NativeRuntimePathRole::PdfiumLibrary,
                path: pdfium.to_owned(),
            }
        );
    }

    #[test]
    fn invalid_override_basename_fails_without_trying_a_bundled_fallback() {
        let executable = Path::new("/opt/app/component_story");
        let bundled_pdfium = Path::new("/opt/app/libpdfium.so");
        let override_path = Path::new("/verified/pdfium/pdfium.so");
        let error = resolve_layout(
            NativePlatform::Linux,
            executable,
            NativeRuntimeMode::Development {
                pdfium_library: override_path.to_owned(),
            },
            &FakeProbe::default()
                .regular(executable, true)
                .regular("/opt/app/butter-paper-pdf-worker", true)
                .regular(bundled_pdfium, false),
        )
        .unwrap_err();
        assert_eq!(
            error,
            NativeRuntimeLayoutError::InvalidPdfiumBasename {
                expected: "libpdfium.so",
                actual: Some(OsString::from("pdfium.so")),
            }
        );
    }
}
