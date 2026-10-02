//! Platform directory resolution for production native storage.
//!
//! Package authenticity is deliberately outside this module. macOS continues
//! to derive its channel from the authenticated app bundle; the first Windows
//! and Linux native packages are stable-only and pass that compile-time channel
//! after their external package-signing/provenance gates succeed.

use std::{ffi::OsStr, path::PathBuf};

use crate::native_storage_layout::{NativeProductionStorage, NativeReleaseChannel};

pub fn windows_production_storage(
    roaming_application_data: Option<&OsStr>,
    local_application_data: Option<&OsStr>,
    channel: NativeReleaseChannel,
) -> Result<NativeProductionStorage, &'static str> {
    let roaming_application_data = roaming_application_data
        .map(PathBuf::from)
        .ok_or("the roaming application data directory is unavailable")?;
    let local_application_data = local_application_data
        .map(PathBuf::from)
        .ok_or("the local application data directory is unavailable")?;
    NativeProductionStorage::windows(&roaming_application_data, &local_application_data, channel)
}

pub fn linux_production_storage(
    home: Option<&OsStr>,
    data_home: Option<&OsStr>,
    cache_home: Option<&OsStr>,
    config_home: Option<&OsStr>,
    channel: NativeReleaseChannel,
) -> Result<NativeProductionStorage, &'static str> {
    let home = home
        .map(PathBuf::from)
        .ok_or("the production home directory is unavailable")?;
    if !home.is_absolute() {
        return Err("the production home directory must be absolute");
    }
    let data_home = xdg_root(data_home, home.join(".local/share"), "XDG_DATA_HOME")?;
    let cache_home = xdg_root(cache_home, home.join(".cache"), "XDG_CACHE_HOME")?;
    let config_home = xdg_root(config_home, home.join(".config"), "XDG_CONFIG_HOME")?;
    NativeProductionStorage::linux(&data_home, &cache_home, &config_home, channel)
}

fn xdg_root(
    configured: Option<&OsStr>,
    fallback: PathBuf,
    variable: &'static str,
) -> Result<PathBuf, &'static str> {
    let path = configured.map(PathBuf::from).unwrap_or(fallback);
    if !path.is_absolute() {
        return Err(match variable {
            "XDG_DATA_HOME" => "XDG_DATA_HOME must be absolute when set",
            "XDG_CACHE_HOME" => "XDG_CACHE_HOME must be absolute when set",
            "XDG_CONFIG_HOME" => "XDG_CONFIG_HOME must be absolute when set",
            _ => "the XDG directory must be absolute when set",
        });
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_requires_both_absolute_platform_roots() {
        assert!(
            windows_production_storage(
                Some(OsStr::new(r"C:\Users\tester\AppData\Roaming")),
                Some(OsStr::new(r"C:\Users\tester\AppData\Local")),
                NativeReleaseChannel::Stable,
            )
            .is_ok()
        );
        assert!(
            windows_production_storage(
                None,
                Some(OsStr::new(r"C:\Users\tester\AppData\Local")),
                NativeReleaseChannel::Stable,
            )
            .is_err()
        );
        assert!(
            windows_production_storage(
                Some(OsStr::new("relative")),
                Some(OsStr::new(r"C:\Users\tester\AppData\Local")),
                NativeReleaseChannel::Stable,
            )
            .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_uses_xdg_defaults_without_mixing_stable_and_beta() {
        let stable = linux_production_storage(
            Some(OsStr::new("/home/tester")),
            None,
            None,
            None,
            NativeReleaseChannel::Stable,
        )
        .unwrap();
        let beta = linux_production_storage(
            Some(OsStr::new("/home/tester")),
            Some(OsStr::new("/data")),
            Some(OsStr::new("/cache")),
            Some(OsStr::new("/config")),
            NativeReleaseChannel::Beta,
        )
        .unwrap();
        assert_eq!(
            stable.layout().durable_root(),
            Path::new("/home/tester/.local/share/com.butterpaper.desktop/native-v1")
        );
        assert_eq!(
            stable.layout().surface_root(),
            Path::new("/home/tester/.cache/com.butterpaper.desktop/native-v1/render-surfaces")
        );
        assert_eq!(
            beta.layout().durable_root(),
            Path::new("/data/com.butterpaper.desktop.beta/native-v1")
        );
        assert_ne!(stable.layout().durable_root(), beta.layout().durable_root());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_rejects_missing_or_relative_authority_roots() {
        assert!(
            linux_production_storage(None, None, None, None, NativeReleaseChannel::Stable,)
                .is_err()
        );
        for roots in [
            (Some("relative"), None, None),
            (None, Some("relative"), None),
            (None, None, Some("relative")),
        ] {
            assert!(
                linux_production_storage(
                    Some(OsStr::new("/home/tester")),
                    roots.0.map(OsStr::new),
                    roots.1.map(OsStr::new),
                    roots.2.map(OsStr::new),
                    NativeReleaseChannel::Stable,
                )
                .is_err()
            );
        }
    }
}
