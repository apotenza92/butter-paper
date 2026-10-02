//! Release versions: `X.Y.Z` or `X.Y.Z-beta.N`. The app crate's version is
//! the source of truth; the release tag is `v` followed by it.

use std::fmt;

use crate::{Args, Result, app_crate, read_text, root, write};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub beta: Option<u64>,
}

impl Version {
    pub fn parse(value: &str) -> Result<Self> {
        let invalid = || format!("{value} is not X.Y.Z or X.Y.Z-beta.N");
        let number = |text: &str| -> Option<u64> {
            (!text.is_empty() && (text == "0" || !text.starts_with('0')) && text.bytes().all(|b| b.is_ascii_digit()))
                .then(|| text.parse().ok())
                .flatten()
        };
        let (core, beta) = match value.split_once("-beta.") {
            Some((core, beta)) => (core, Some(number(beta).filter(|n| *n >= 1).ok_or_else(invalid)?)),
            None => (value, None),
        };
        let parts: Vec<_> = core.split('.').collect();
        let [major, minor, patch] = parts.as_slice() else {
            return Err(invalid());
        };
        Ok(Self {
            major: number(major).ok_or_else(invalid)?,
            minor: number(minor).ok_or_else(invalid)?,
            patch: number(patch).ok_or_else(invalid)?,
            beta,
        })
    }

    pub fn is_beta(self) -> bool {
        self.beta.is_some()
    }

    /// `X.Y.Z`; macOS's CFBundleShortVersionString.
    pub fn core(self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }

    /// macOS's CFBundleVersion: `(M*1e6 + m*1e3 + p)*1e5 + N`, with N the
    /// beta number or 90000 for the release (as `ReleaseVersion::build_number`).
    pub fn build_number(self) -> u64 {
        (self.major * 1_000_000 + self.minor * 1_000 + self.patch) * 100_000 + self.beta.unwrap_or(90_000)
    }

    pub fn tag(self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.core())?;
        if let Some(beta) = self.beta {
            write!(formatter, "-beta.{beta}")?;
        }
        Ok(())
    }
}

/// The app crate's version.
pub fn current() -> Result<Version> {
    let manifest = read_text(&app_crate().join("Cargo.toml"))?;
    let line = manifest
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .ok_or("crates/butter-paper/Cargo.toml has no version")?;
    Version::parse(line.trim_end_matches('"'))
}

/// Every place that must carry the version, and the version it has.
pub fn recorded() -> Result<Vec<(&'static str, String)>> {
    let lock = read_text(&root().join("Cargo.lock"))?;
    let locked = lock
        .split("[[package]]\nname = \"butter-paper\"\nversion = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .ok_or("Cargo.lock has no butter-paper entry")?;
    Ok(vec![
        ("crates/butter-paper/Cargo.toml", current()?.to_string()),
        ("Cargo.lock", locked.to_string()),
    ])
}

pub fn run(args: &mut Args) -> Result {
    let value = args.positional().ok_or("usage: cargo xtask version X.Y.Z[-beta.N]")?;
    args.finish()?;
    let version = Version::parse(&value)?;
    set(version)?;
    let changelog = start_changelog_section(version)?;
    println!("Version set to {version}.");
    if changelog {
        println!("Started CHANGELOG.md ## [{version}] from [Unreleased]; check the notes, commit, then run cargo xtask release.");
    } else {
        println!("CHANGELOG.md already has ## [{version}]; commit, then run cargo xtask release.");
    }
    Ok(())
}

fn set(version: Version) -> Result {
    let old = current()?;
    let manifest_path = app_crate().join("Cargo.toml");
    let manifest = read_text(&manifest_path)?;
    let from = format!("\nversion = \"{old}\"\n");
    if manifest.matches(&from).count() != 1 {
        return Err("crates/butter-paper/Cargo.toml: expected exactly one package version".into());
    }
    write(&manifest_path, manifest.replacen(&from, &format!("\nversion = \"{version}\"\n"), 1))?;

    let lock_path = root().join("Cargo.lock");
    let lock = read_text(&lock_path)?;
    let entry = |value: &dyn fmt::Display| format!("[[package]]\nname = \"butter-paper\"\nversion = \"{value}\"\n");
    let (_, locked) = recorded()?.remove(1);
    if !lock.contains(&entry(&locked)) {
        return Err("Cargo.lock: butter-paper entry not found".into());
    }
    write(&lock_path, lock.replacen(&entry(&locked), &entry(&version), 1))?;

    // The development bundle template carries macOS's X.Y.Z form.
    let plist_path = app_crate().join("bundle/Info.plist");
    let plist = read_text(&plist_path)?;
    let mut updated = String::new();
    let mut lines = plist.lines().peekable();
    while let Some(line) = lines.next() {
        updated.push_str(line);
        updated.push('\n');
        if line.contains("<key>CFBundleShortVersionString</key>") || line.contains("<key>CFBundleVersion</key>") {
            if let Some(value) = lines.next() {
                let indent = &value[..value.len() - value.trim_start().len()];
                updated.push_str(&format!("{indent}<string>{}</string>\n", version.core()));
            }
        }
    }
    write(&plist_path, updated)
}

fn start_changelog_section(version: Version) -> Result<bool> {
    let path = root().join("CHANGELOG.md");
    let text = read_text(&path)?;
    if text.contains(&format!("\n## [{version}]\n")) {
        return Ok(false);
    }
    let marker = "## [Unreleased]\n";
    if !text.contains(marker) {
        return Err("CHANGELOG.md has no ## [Unreleased] section".into());
    }
    write(&path, text.replacen(marker, &format!("{marker}\n## [{version}]\n"), 1))?;
    Ok(true)
}

/// The notes under `## [version]` in CHANGELOG.md.
pub fn changelog_notes(version: Version) -> Result<String> {
    let text = read_text(&root().join("CHANGELOG.md"))?;
    let heading = format!("\n## [{version}]\n");
    let notes = text
        .split_once(&heading)
        .map(|(_, rest)| rest.split("\n## [").next().unwrap_or_default().trim().to_string())
        .unwrap_or_default();
    if notes.is_empty() {
        return Err(format!("CHANGELOG.md needs a non-empty ## [{version}] section"));
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_release_and_beta_versions_only() {
        for value in ["0.1.0", "1.2.3", "0.1.1-beta.1", "0.1.1-beta.12"] {
            assert_eq!(Version::parse(value).unwrap().to_string(), value);
        }
        for value in ["0.1.0-beta", "0.1.0-beta.0", "0.1.0-beta.01", "0.1.0-rc.1", "v0.1.0", "0.1", "01.2.3", "1.2.3.4"] {
            assert!(Version::parse(value).is_err(), "{value}");
        }
    }

    #[test]
    fn build_numbers_put_a_release_above_its_betas() {
        let release = Version::parse("0.1.0").unwrap();
        assert_eq!(release.build_number(), 100_090_000);
        assert_eq!(release.core(), "0.1.0");
        let beta = Version::parse("0.1.1-beta.2").unwrap();
        assert_eq!(beta.build_number(), 100_100_002);
        assert_eq!(beta.core(), "0.1.1");
        assert!(beta.build_number() < Version::parse("0.1.1").unwrap().build_number());
        assert_eq!(beta.tag(), "v0.1.1-beta.2");
    }

    #[test]
    fn every_recorded_version_agrees() {
        let versions = recorded().unwrap();
        assert!(versions.iter().all(|(_, value)| *value == versions[0].1), "{versions:?}");
    }
}
