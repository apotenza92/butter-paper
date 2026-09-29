//! Deterministic update scheduling preferences.
//!
//! This module deliberately does not fetch, verify, or install updates. It only
//! validates the user's check frequency and determines whether a check is due.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::native_storage_layout::NativeReleaseChannel;

pub const UPDATE_SETTINGS_FILE_NAME: &str = "update-settings.json";
pub const UPDATE_SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const UPDATE_SCHEDULER_MAX_WAKE_MS: i64 = 60 * 60 * 1_000;
const DAY_MS: i64 = 24 * 60 * 60 * 1_000;
static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateFrequency {
    Never,
    Startup,
    Hourly,
    SixHours,
    TwelveHours,
    Daily,
    Weekly,
    Monthly,
}

impl UpdateFrequency {
    pub fn interval_ms(self) -> Option<i64> {
        match self {
            Self::Never | Self::Startup => None,
            Self::Hourly => Some(60 * 60 * 1_000),
            Self::SixHours => Some(6 * 60 * 60 * 1_000),
            Self::TwelveHours => Some(12 * 60 * 60 * 1_000),
            Self::Daily => Some(DAY_MS),
            Self::Weekly => Some(7 * DAY_MS),
            Self::Monthly => Some(30 * DAY_MS),
        }
    }

    pub fn scheduler_wake_interval_ms(self) -> Option<i64> {
        self.interval_ms()
            .map(|interval| interval.min(UPDATE_SCHEDULER_MAX_WAKE_MS))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSettings {
    schema_version: u32,
    frequency: UpdateFrequency,
    last_successful_check_at: Option<String>,
}

impl UpdateSettings {
    pub fn defaults(channel: NativeReleaseChannel) -> Self {
        Self {
            schema_version: UPDATE_SETTINGS_SCHEMA_VERSION,
            frequency: match channel {
                NativeReleaseChannel::Stable => UpdateFrequency::Weekly,
                NativeReleaseChannel::Beta => UpdateFrequency::Daily,
            },
            last_successful_check_at: None,
        }
    }

    pub fn frequency(&self) -> UpdateFrequency {
        self.frequency
    }

    pub fn set_frequency(&mut self, frequency: UpdateFrequency) {
        self.frequency = frequency;
    }

    pub fn last_successful_check_at(&self) -> Option<&str> {
        self.last_successful_check_at.as_deref()
    }

    pub fn record_successful_check(&mut self, timestamp: &str) -> Result<(), TimestampError> {
        parse_canonical_utc_timestamp(timestamp)?;
        self.last_successful_check_at = Some(timestamp.to_owned());
        Ok(())
    }

    pub fn is_check_due(&self, now_utc: &str) -> Result<bool, TimestampError> {
        let now_ms = parse_canonical_utc_timestamp(now_utc)?;
        match self.frequency {
            UpdateFrequency::Never => Ok(false),
            UpdateFrequency::Startup => Ok(true),
            frequency => {
                let Some(last) = self.last_successful_check_at.as_deref() else {
                    return Ok(true);
                };
                let last_ms = parse_canonical_utc_timestamp(last)?;
                let Some(interval_ms) = frequency.interval_ms() else {
                    return Ok(false);
                };
                Ok(last_ms > now_ms || now_ms - last_ms >= interval_ms)
            }
        }
    }

    fn validate(&self) -> io::Result<()> {
        if self.schema_version != UPDATE_SETTINGS_SCHEMA_VERSION {
            return Err(invalid_settings(
                "unsupported update settings schema version",
            ));
        }
        if let Some(timestamp) = self.last_successful_check_at.as_deref() {
            parse_canonical_utc_timestamp(timestamp).map_err(|_| {
                invalid_settings("update check timestamp is not canonical RFC3339 UTC")
            })?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampError;

/// Parse the single canonical representation used by update settings:
/// `YYYY-MM-DDTHH:MM:SS.sssZ`.
pub fn parse_canonical_utc_timestamp(value: &str) -> Result<i64, TimestampError> {
    let bytes = value.as_bytes();
    if bytes.len() != 24
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'.'
        || bytes[23] != b'Z'
    {
        return Err(TimestampError);
    }
    let year = digits(bytes, 0, 4).ok_or(TimestampError)? as i64;
    let month = digits(bytes, 5, 2).ok_or(TimestampError)? as i64;
    let day = digits(bytes, 8, 2).ok_or(TimestampError)? as i64;
    let hour = digits(bytes, 11, 2).ok_or(TimestampError)? as i64;
    let minute = digits(bytes, 14, 2).ok_or(TimestampError)? as i64;
    let second = digits(bytes, 17, 2).ok_or(TimestampError)? as i64;
    let millis = digits(bytes, 20, 3).ok_or(TimestampError)? as i64;
    if year == 0 || !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return Err(TimestampError);
    }
    let days_in_month = match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if day == 0 || day > days_in_month {
        return Err(TimestampError);
    }

    let previous_year = year - 1;
    let mut days =
        365 * previous_year + previous_year / 4 - previous_year / 100 + previous_year / 400;
    for prior_month in 1..month {
        days += match prior_month {
            2 if is_leap_year(year) => 29,
            2 => 28,
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
    }
    days += day - 1;
    Ok((((days * 24 + hour) * 60 + minute) * 60 + second) * 1_000 + millis)
}

fn digits(bytes: &[u8], start: usize, length: usize) -> Option<u32> {
    bytes
        .get(start..start + length)?
        .iter()
        .try_fold(0, |value, byte| {
            byte.is_ascii_digit()
                .then_some(value * 10 + u32::from(*byte - b'0'))
        })
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

pub fn parse_update_settings(bytes: &[u8]) -> io::Result<UpdateSettings> {
    let settings: UpdateSettings = serde_json::from_slice(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    settings.validate()?;
    Ok(settings)
}

pub fn serialise_update_settings(settings: &UpdateSettings) -> io::Result<Vec<u8>> {
    settings.validate()?;
    let mut bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub struct UpdateSettingsStore {
    directory: PathBuf,
    channel: NativeReleaseChannel,
}

impl UpdateSettingsStore {
    pub fn new(directory: impl Into<PathBuf>, channel: NativeReleaseChannel) -> Self {
        Self {
            directory: directory.into(),
            channel,
        }
    }

    pub fn path(&self) -> PathBuf {
        self.directory.join(UPDATE_SETTINGS_FILE_NAME)
    }

    pub fn load(&self) -> io::Result<UpdateSettings> {
        match fs::read(self.path()) {
            Ok(bytes) => parse_update_settings(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                Ok(UpdateSettings::defaults(self.channel))
            }
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, settings: &UpdateSettings) -> io::Result<()> {
        let bytes = serialise_update_settings(settings)?;
        fs::create_dir_all(&self.directory)?;
        let temporary_path = self.directory.join(format!(
            ".{UPDATE_SETTINGS_FILE_NAME}.{}.{}.tmp",
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
        let mut temporary_created = false;
        let result = (|| {
            let mut file = options.open(&temporary_path)?;
            temporary_created = true;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            replace_file(&temporary_path, &self.path())?;
            #[cfg(unix)]
            File::open(&self.directory)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() && temporary_created {
            let _ = fs::remove_file(&temporary_path);
        }
        result
    }
}

fn replace_file(temporary_path: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary_path, path)
}

fn invalid_settings(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "butter-paper-native-update-policy-{}-{nonce}-{}",
                std::process::id(),
                TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed),
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_are_explicitly_channel_specific() {
        assert_eq!(
            UpdateSettings::defaults(NativeReleaseChannel::Stable).frequency(),
            UpdateFrequency::Weekly
        );
        assert_eq!(
            UpdateSettings::defaults(NativeReleaseChannel::Beta).frequency(),
            UpdateFrequency::Daily
        );
    }

    #[test]
    fn due_calculation_handles_all_frequencies_and_injected_time() {
        let now = "2026-09-28T12:00:00.000Z";
        let cases = [
            (
                UpdateFrequency::Never,
                Some("2026-01-01T00:00:00.000Z"),
                false,
            ),
            (UpdateFrequency::Startup, Some(now), true),
            (
                UpdateFrequency::Hourly,
                Some("2026-09-28T11:00:00.000Z"),
                true,
            ),
            (
                UpdateFrequency::SixHours,
                Some("2026-09-28T06:00:00.001Z"),
                false,
            ),
            (
                UpdateFrequency::TwelveHours,
                Some("2026-09-28T00:00:00.000Z"),
                true,
            ),
            (
                UpdateFrequency::Daily,
                Some("2026-09-27T12:00:00.000Z"),
                true,
            ),
            (
                UpdateFrequency::Daily,
                Some("2026-09-27T12:00:00.001Z"),
                false,
            ),
            (
                UpdateFrequency::Weekly,
                Some("2026-09-21T12:00:00.000Z"),
                true,
            ),
            (
                UpdateFrequency::Monthly,
                Some("2026-08-29T12:00:00.000Z"),
                true,
            ),
            (
                UpdateFrequency::Weekly,
                Some("2026-09-29T12:00:00.000Z"),
                true,
            ),
            (UpdateFrequency::Monthly, None, true),
        ];
        for (frequency, last, expected) in cases {
            let mut settings = UpdateSettings::defaults(NativeReleaseChannel::Stable);
            settings.set_frequency(frequency);
            settings.last_successful_check_at = last.map(str::to_owned);
            assert_eq!(
                settings.is_check_due(now).unwrap(),
                expected,
                "{frequency:?}"
            );
        }
        assert!(
            UpdateSettings::defaults(NativeReleaseChannel::Stable)
                .is_check_due("2026-09-28T12:00:00.000Z")
                .unwrap()
        );
    }

    #[test]
    fn timestamps_accept_only_canonical_valid_utc_milliseconds() {
        assert!(parse_canonical_utc_timestamp("2024-02-29T23:59:59.999Z").is_ok());
        for invalid in [
            "2023-02-29T00:00:00.000Z",
            "2026-04-31T00:00:00.000Z",
            "2026-09-28T24:00:00.000Z",
            "2026-09-28T12:00:60.000Z",
            "2026-09-28T12:00:00Z",
            "2026-09-28T12:00:00.00Z",
            "2026-09-28T12:00:00.000+00:00",
            "2026-09-28t12:00:00.000z",
        ] {
            assert!(parse_canonical_utc_timestamp(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn parser_rejects_wrong_schema_unknown_fields_invalid_frequency_and_timestamp() {
        for json in [
            br#"{"schemaVersion":2,"frequency":"weekly","lastSuccessfulCheckAt":null}"#.as_slice(),
            br#"{"schemaVersion":1,"frequency":"everyHour","lastSuccessfulCheckAt":null}"#.as_slice(),
            br#"{"schemaVersion":1,"frequency":"weekly","lastSuccessfulCheckAt":null,"extra":true}"#.as_slice(),
            br#"{"schemaVersion":1,"frequency":"weekly","lastSuccessfulCheckAt":"2026-09-28T12:00:00Z"}"#.as_slice(),
        ] {
            assert_eq!(parse_update_settings(json).unwrap_err().kind(), io::ErrorKind::InvalidData);
        }
    }

    #[test]
    fn every_existing_menu_frequency_round_trips_through_the_strict_schema() {
        for frequency in [
            UpdateFrequency::Never,
            UpdateFrequency::Startup,
            UpdateFrequency::Hourly,
            UpdateFrequency::SixHours,
            UpdateFrequency::TwelveHours,
            UpdateFrequency::Daily,
            UpdateFrequency::Weekly,
            UpdateFrequency::Monthly,
        ] {
            let mut settings = UpdateSettings::defaults(NativeReleaseChannel::Stable);
            settings.set_frequency(frequency);
            let bytes = serialise_update_settings(&settings).unwrap();
            assert_eq!(parse_update_settings(&bytes).unwrap(), settings);
        }
    }

    #[test]
    fn periodic_schedules_wake_at_most_hourly_without_inventing_startup_timers() {
        assert_eq!(UpdateFrequency::Never.scheduler_wake_interval_ms(), None);
        assert_eq!(UpdateFrequency::Startup.scheduler_wake_interval_ms(), None);
        for frequency in [
            UpdateFrequency::Hourly,
            UpdateFrequency::SixHours,
            UpdateFrequency::TwelveHours,
            UpdateFrequency::Daily,
            UpdateFrequency::Weekly,
            UpdateFrequency::Monthly,
        ] {
            assert_eq!(
                frequency.scheduler_wake_interval_ms(),
                Some(UPDATE_SCHEDULER_MAX_WAKE_MS),
                "{frequency:?}",
            );
        }
    }

    #[test]
    fn store_defaults_on_missing_file_and_round_trips_private_atomic_state() {
        let directory = TestDirectory::new();
        let store = UpdateSettingsStore::new(&directory.0, NativeReleaseChannel::Beta);
        assert_eq!(
            store.load().unwrap(),
            UpdateSettings::defaults(NativeReleaseChannel::Beta)
        );
        let mut settings = store.load().unwrap();
        settings.set_frequency(UpdateFrequency::Monthly);
        settings
            .record_successful_check("2026-09-28T12:00:00.000Z")
            .unwrap();
        store.save(&settings).unwrap();
        assert_eq!(store.load().unwrap(), settings);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn corrupt_persisted_state_fails_closed_without_overwriting_it() {
        let directory = TestDirectory::new();
        let store = UpdateSettingsStore::new(&directory.0, NativeReleaseChannel::Stable);
        fs::write(store.path(), b"{}").unwrap();
        assert_eq!(store.load().unwrap_err().kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(store.path()).unwrap(), b"{}");
    }
}
