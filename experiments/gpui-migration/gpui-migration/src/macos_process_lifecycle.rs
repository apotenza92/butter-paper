//! Opt-in, final self-accounting for authorised macOS performance runs.
//!
//! The application root publishes only after GPUI returns normally. Abrupt
//! termination therefore leaves no receipt and is rejected by the runner.

use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::{Mutex, OnceLock};

pub const APP_LIFECYCLE_TOKEN_ENV: &str = "BP_MACOS_APP_LIFECYCLE_TOKEN";
pub const LIFECYCLE_REQUEST_ENV: &str = "BP_MACOS_WORKER_LIFECYCLE";
pub const LIFECYCLE_RECEIPT_DIR_ENV: &str = "BP_MACOS_WORKER_LIFECYCLE_RECEIPT_DIR";

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct ProcessUsage {
    pid: u32,
    start_abstime: u64,
    user_ns: u64,
    system_ns: u64,
    lifetime_max_phys_footprint_bytes: u64,
}

#[derive(Serialize)]
struct PublishedAppLifecycleReceipt<'a> {
    schema_version: u8,
    #[serde(rename = "type")]
    receipt_type: &'static str,
    token: &'a str,
    pid: u32,
    start_abstime: u64,
    user_ns: u64,
    system_ns: u64,
    lifetime_max_phys_footprint_bytes: u64,
    clean_exit: bool,
    exit_code: i32,
}

/// Captures the immutable identity of the application root at authorised
/// performance-run startup and owns its private publication destination.
pub struct AppRootLifecycleReceipt {
    #[cfg(target_os = "macos")]
    token: String,
    #[cfg(target_os = "macos")]
    directory: PathBuf,
    #[cfg(target_os = "macos")]
    pid: u32,
    #[cfg(target_os = "macos")]
    start_abstime: u64,
}

impl AppRootLifecycleReceipt {
    /// Read the runner-issued capability. The caller must invoke this only
    /// after the performance configuration has passed its existing authority
    /// checks. The root-only token is removed before GPUI or workers start.
    pub fn begin_from_authorized_performance_environment() -> io::Result<Option<Self>> {
        let token = std::env::var_os(APP_LIFECYCLE_TOKEN_ENV);
        if token.is_none() {
            return Ok(None);
        }
        // SAFETY: main calls this during single-threaded startup, before GPUI
        // and any worker threads exist. The bearer token must not reach them.
        unsafe { std::env::remove_var(APP_LIFECYCLE_TOKEN_ENV) };
        if std::env::var_os(LIFECYCLE_REQUEST_ENV).as_deref() != Some("1".as_ref()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("{LIFECYCLE_REQUEST_ENV} must be exactly 1 for app lifecycle accounting"),
            ));
        }
        let token = token
            .and_then(|value| value.into_string().ok())
            .filter(|value| valid_token(value))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "invalid app lifecycle token")
            })?;
        let directory = std::env::var_os(LIFECYCLE_RECEIPT_DIR_ENV)
            .map(PathBuf::from)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{LIFECYCLE_RECEIPT_DIR_ENV} is required"),
                )
            })?;
        Self::begin(token, directory).map(Some)
    }

    #[cfg(target_os = "macos")]
    fn begin(token: String, directory: PathBuf) -> io::Result<Self> {
        validate_private_directory(&directory)?;
        let usage = current_process_usage()?;
        Ok(Self {
            token,
            directory,
            pid: usage.pid,
            start_abstime: usage.start_abstime,
        })
    }

    #[cfg(not(target_os = "macos"))]
    fn begin(_token: String, _directory: PathBuf) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "app lifecycle accounting is implemented only on macOS",
        ))
    }

    /// Publish final self CPU and lifetime physical-footprint evidence after
    /// `Application::run` has returned normally.
    pub fn finish_and_publish(self) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let usage = current_process_usage()?;
            if usage.pid != self.pid || usage.start_abstime != self.start_abstime {
                return Err(io::Error::other(
                    "application process identity changed before final lifecycle receipt",
                ));
            }
            if usage.user_ns.saturating_add(usage.system_ns) == 0
                || usage.lifetime_max_phys_footprint_bytes == 0
            {
                return Err(io::Error::other(
                    "application final lifecycle measurements were empty",
                ));
            }
            let receipt = PublishedAppLifecycleReceipt {
                schema_version: 1,
                receipt_type: "app-root-lifecycle-final",
                token: &self.token,
                pid: usage.pid,
                start_abstime: usage.start_abstime,
                user_ns: usage.user_ns,
                system_ns: usage.system_ns,
                lifetime_max_phys_footprint_bytes: usage.lifetime_max_phys_footprint_bytes,
                clean_exit: true,
                exit_code: 0,
            };
            publish(&receipt, &self.token, &self.directory)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = self;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "app lifecycle accounting is implemented only on macOS",
            ))
        }
    }

    /// Install the publisher on the process' normal-exit path. AppKit's
    /// `terminate:` exits the process instead of returning through
    /// `Application::run`, so code placed after the GPUI run loop is not a
    /// reliable macOS shutdown hook.
    pub fn install_normal_exit_publisher(self) -> io::Result<()> {
        #[cfg(target_os = "macos")]
        {
            static PUBLISHER: OnceLock<Mutex<Option<AppRootLifecycleReceipt>>> = OnceLock::new();

            extern "C" fn publish_at_normal_exit() {
                let Some(publisher) = PUBLISHER.get() else {
                    return;
                };
                let receipt = publisher.lock().ok().and_then(|mut slot| slot.take());
                if let Some(receipt) = receipt
                    && let Err(error) = receipt.finish_and_publish()
                {
                    eprintln!("failed to publish GPUI application lifecycle receipt: {error}");
                }
            }

            let publisher = PUBLISHER.get_or_init(|| Mutex::new(None));
            let mut slot = publisher
                .lock()
                .map_err(|_| io::Error::other("app lifecycle publisher lock was poisoned"))?;
            if slot.is_some() {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "app lifecycle publisher was already installed",
                ));
            }
            *slot = Some(self);
            if unsafe { libc::atexit(publish_at_normal_exit) } != 0 {
                let _ = slot.take();
                return Err(io::Error::other(
                    "could not register the app lifecycle normal-exit publisher",
                ));
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = self;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "app lifecycle accounting is implemented only on macOS",
            ))
        }
    }
}

fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[cfg(target_os = "macos")]
fn validate_private_directory(directory: &Path) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;

    if !directory.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "app lifecycle receipt directory must be absolute",
        ));
    }
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "app lifecycle receipt directory must be private and owned by the current user",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn mach_abstime_to_ns(value: u64) -> io::Result<u64> {
    let mut timebase = libc::mach_timebase_info { numer: 0, denom: 0 };
    if unsafe { libc::mach_timebase_info(&mut timebase) } != 0 || timebase.denom == 0 {
        return Err(io::Error::other("macOS Mach timebase is unavailable"));
    }
    let value = (u128::from(value) * u128::from(timebase.numer)) / u128::from(timebase.denom);
    u64::try_from(value).map_err(|_| io::Error::other("process CPU time overflowed u64"))
}

#[cfg(target_os = "macos")]
fn current_process_usage() -> io::Result<ProcessUsage> {
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage_info_v4>() };
    let result = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V4,
            (&mut usage as *mut libc::rusage_info_v4).cast(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    if usage.ri_proc_start_abstime == 0 {
        return Err(io::Error::other("macOS process identity has no start time"));
    }
    Ok(ProcessUsage {
        pid: std::process::id(),
        start_abstime: usage.ri_proc_start_abstime,
        user_ns: mach_abstime_to_ns(usage.ri_user_time)?,
        system_ns: mach_abstime_to_ns(usage.ri_system_time)?,
        lifetime_max_phys_footprint_bytes: usage.ri_lifetime_max_phys_footprint,
    })
}

#[cfg(target_os = "macos")]
fn publish(
    receipt: &PublishedAppLifecycleReceipt<'_>,
    token: &str,
    directory: &Path,
) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
    validate_private_directory(directory)?;
    if !valid_token(token) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid app lifecycle token",
        ));
    }
    let mut bytes = serde_json::to_vec(receipt).map_err(io::Error::other)?;
    bytes.push(b'\n');
    let destination = directory.join(format!("{token}.json"));
    let temporary = directory.join(format!(
        ".{token}.{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::hard_link(&temporary, &destination)?;
        fs::remove_file(&temporary)?;
        File::open(directory)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn final_receipt_is_private_atomic_exact_and_no_replace() {
        let root = std::env::temp_dir().join(format!(
            "bp-app-root-lifecycle-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        let token = "app-root-test".to_owned();

        let publisher = AppRootLifecycleReceipt::begin(token.clone(), root.clone()).unwrap();
        for value in 0_u64..100_000 {
            std::hint::black_box(value.wrapping_mul(value));
        }
        publisher.finish_and_publish().unwrap();

        let path = root.join(format!("{token}.json"));
        let metadata = fs::metadata(&path).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
        let receipt: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            receipt
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "clean_exit",
                "exit_code",
                "lifetime_max_phys_footprint_bytes",
                "pid",
                "schema_version",
                "start_abstime",
                "system_ns",
                "token",
                "type",
                "user_ns",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert_eq!(receipt["type"], "app-root-lifecycle-final");
        assert_eq!(receipt["token"], token);
        assert_eq!(receipt["pid"], std::process::id());
        assert_eq!(receipt["clean_exit"], true);
        assert_eq!(receipt["exit_code"], 0);
        assert!(receipt["start_abstime"].as_u64().unwrap() > 0);
        assert!(receipt["user_ns"].as_u64().unwrap() + receipt["system_ns"].as_u64().unwrap() > 0);
        assert!(
            receipt["lifetime_max_phys_footprint_bytes"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(fs::read_dir(&root).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        }));

        let duplicate = AppRootLifecycleReceipt::begin(token, root.clone()).unwrap();
        assert!(duplicate.finish_and_publish().is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
