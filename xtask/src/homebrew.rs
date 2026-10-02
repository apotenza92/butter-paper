//! The release's SHA256SUMS.txt and homebrew-publication.tar.gz: the
//! checksum-sealed bundle (manifest.json, Casks/*.rb, SHA256SUMS) that the tap
//! (apotenza92/homebrew-tap, scripts/homebrew_publication.py) validates. A
//! stable release also advances the Beta cask (`stable_advances_beta`).

use std::{fs, path::Path};

use serde_json::json;

use crate::{
    Args, Result, archive::{self, Entry}, read, sha256, target::Channel, version::Version, write,
};

const REPOSITORY: &str = "apotenza92/butter-paper";
const MINIMUM_MACOS: &str = "13.0";
const ARCHITECTURES: [&str; 2] = ["arm64", "x64"];

struct Cask {
    file: &'static str,
    token: &'static str,
    name: &'static str,
    desc: &'static str,
    application: &'static str,
    bundle_identifier: &'static str,
    asset_prefix: &'static str,
}

fn cask(channel: Channel) -> Cask {
    match channel {
        Channel::Stable => Cask {
            file: "butter-paper.rb",
            token: "butter-paper",
            name: "Butter Paper",
            desc: "Cross-platform PDF review and markup",
            application: "Butter Paper.app",
            bundle_identifier: "com.butterpaper.desktop",
            asset_prefix: "Butter-Paper-macOS",
        },
        Channel::Beta => Cask {
            file: "butter-paper@beta.rb",
            token: "butter-paper@beta",
            name: "Butter Paper Beta",
            desc: "Cross-platform PDF review and markup (beta channel)",
            application: "Butter Paper Beta.app",
            bundle_identifier: "com.butterpaper.desktop.beta",
            asset_prefix: "Butter-Paper-Beta-macOS",
        },
    }
}

fn zap(cask: &Cask) -> String {
    let product = cask.application.trim_end_matches(".app");
    let id = cask.bundle_identifier;
    [
        format!("~/Library/Application Support/{product}"),
        format!("~/Library/Application Support/{id}"),
        format!("~/Library/Caches/{id}"),
        format!("~/Library/Preferences/{id}.plist"),
        format!("~/Library/Saved Application State/{id}.savedState"),
    ]
    .iter()
    .map(|path| format!("    \"{path}\",\n"))
    .collect()
}

pub fn render_cask(channel: Channel, version: Version, digests: [&str; 2]) -> String {
    let cask = cask(channel);
    let block = |arch: &str, condition: &str, digest: &str| {
        format!(
            "  {condition} do\n    sha256 \"{digest}\"\n\n    url \"https://github.com/{REPOSITORY}/releases/download/v#{{version}}/{}-{arch}.zip\"\n  end",
            cask.asset_prefix
        )
    };
    format!(
        "cask \"{token}\" do\n  version \"{version}\"\n\n{arm}\n{intel}\n\n  name \"{name}\"\n  desc \"{desc}\"\n  homepage \"https://github.com/{REPOSITORY}\"\n\n  livecheck do\n    skip \"Updated by the Butter Paper release workflow\"\n  end\n\n  auto_updates true\n  depends_on macos: :ventura\n\n  app \"{app}\"\n\n  zap trash: [\n{zap}  ]\nend\n",
        token = cask.token,
        arm = block("arm64", "on_arm", digests[0]),
        intel = block("x64", "on_intel", digests[1]),
        name = cask.name,
        desc = cask.desc,
        app = cask.application,
        zap = zap(&cask),
    )
}

pub struct Publication {
    pub files: Vec<(String, Vec<u8>)>,
    pub casks: Vec<&'static str>,
}

pub fn publication(
    version: Version,
    commit: &str,
    assets: &Path,
    run_id: &str,
    run_attempt: &str,
    jobs: &[String],
) -> Result<Publication> {
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()) {
        return Err("commit must be a full lowercase SHA".into());
    }
    let positive = |value: &str| !value.is_empty() && !value.starts_with('0') && value.bytes().all(|b| b.is_ascii_digit());
    if !positive(run_id) || !positive(run_attempt) {
        return Err("run id and attempt must be positive integers".into());
    }
    if jobs.is_empty() {
        return Err("native validation jobs are required".into());
    }
    let tag = version.tag();
    let channels: &[Channel] = if version.is_beta() { &[Channel::Beta] } else { &[Channel::Stable, Channel::Beta] };
    let mut files = Vec::new();
    let mut artifacts = Vec::new();
    let mut casks = Vec::new();
    let mut applications = serde_json::Map::new();
    let mut identifiers = serde_json::Map::new();
    for &channel in channels {
        let config = cask(channel);
        let mut digests = Vec::new();
        for arch in ARCHITECTURES {
            let name = format!("{}-{arch}.zip", config.asset_prefix);
            let path = assets.join(&name);
            if !path.is_file() {
                return Err(format!("missing release asset {name}"));
            }
            let bytes = read(&path)?;
            let digest = sha256(&bytes);
            artifacts.push(json!({
                "name": name,
                "url": format!("https://github.com/{REPOSITORY}/releases/download/{tag}/{name}"),
                "size": bytes.len(),
                "sha256": digest,
                "channel": match channel { Channel::Stable => "stable", Channel::Beta => "beta" },
                "architecture": arch,
            }));
            digests.push(digest);
        }
        files.push((
            format!("Casks/{}", config.file),
            render_cask(channel, version, [&digests[0], &digests[1]]).into_bytes(),
        ));
        casks.push(config.file);
        let key = match channel {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        };
        applications.insert(key.into(), config.application.into());
        identifiers.insert(key.into(), config.bundle_identifier.into());
    }
    let manifest = json!({
        "schema_version": 1,
        "product": "butter-paper",
        "source_repository": REPOSITORY,
        "release_tag": tag,
        "release_commit": commit,
        "channel": if version.is_beta() { "beta" } else { "stable" },
        "casks": casks,
        "artifacts": artifacts,
        "applications": applications,
        "bundle_identifiers": identifiers,
        "architectures": ARCHITECTURES,
        "minimum_macos": MINIMUM_MACOS,
        "native_validation": {
            "workflow_run_id": run_id,
            "workflow_run_attempt": run_attempt,
            "jobs": jobs,
        },
    });
    let manifest = format!("{}\n", serde_json::to_string_pretty(&manifest).unwrap()).into_bytes();
    let mut sums = format!("{}  manifest.json\n", sha256(&manifest));
    for (path, bytes) in &files {
        sums.push_str(&format!("{}  {path}\n", sha256(bytes)));
    }
    files.insert(0, ("manifest.json".into(), manifest));
    files.push(("SHA256SUMS".into(), sums.into_bytes()));
    Ok(Publication { files, casks })
}

pub fn run(args: &mut Args) -> Result {
    let assets = std::path::PathBuf::from(args.required("--assets")?);
    let tag = args.required("--tag")?;
    let commit = args.required("--commit")?;
    let run_id = args.required("--run-id")?;
    let run_attempt = args.required("--run-attempt")?;
    let jobs: Vec<String> = args.required("--jobs")?.split(',').filter(|j| !j.is_empty()).map(String::from).collect();
    args.finish()?;
    let version = Version::parse(tag.strip_prefix('v').ok_or("tag must start with v")?)?;
    let publication = publication(version, &commit, &assets, &run_id, &run_attempt, &jobs)?;
    let mut entries: Vec<Entry> = publication
        .files
        .into_iter()
        .map(|(path, bytes)| Entry { path, bytes, mode: 0o644 })
        .collect();
    write(&assets.join("homebrew-publication.tar.gz"), archive::tar_gz(&mut entries)?)?;
    write_checksums(&assets)?;
    println!("Casks: {}", publication.casks.join(", "));
    Ok(())
}

/// SHA256SUMS.txt over every other file in `assets`, as the updater reads it.
pub fn write_checksums(assets: &Path) -> Result {
    let mut names: Vec<String> = fs::read_dir(assets)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "SHA256SUMS.txt")
        .collect();
    names.sort();
    let mut sums = String::new();
    for name in names {
        sums.push_str(&format!("{}  {name}\n", sha256(&read(&assets.join(&name))?)));
    }
    write(&assets.join("SHA256SUMS.txt"), sums)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn assets(names: &[&str]) -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("bp-homebrew-{}-{}", std::process::id(), names.len()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        for name in names {
            fs::write(directory.join(name), format!("{name}\n")).unwrap();
        }
        directory
    }

    const ALL: [&str; 4] = [
        "Butter-Paper-macOS-arm64.zip",
        "Butter-Paper-macOS-x64.zip",
        "Butter-Paper-Beta-macOS-arm64.zip",
        "Butter-Paper-Beta-macOS-x64.zip",
    ];

    #[test]
    fn a_stable_release_advances_both_casks() {
        let directory = assets(&ALL);
        let jobs = vec!["package macos-arm64".to_string()];
        let publication = publication(Version::parse("0.1.0").unwrap(), COMMIT, &directory, "7", "1", &jobs).unwrap();
        assert_eq!(publication.casks, ["butter-paper.rb", "butter-paper@beta.rb"]);
        let names: Vec<_> = publication.files.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["manifest.json", "Casks/butter-paper.rb", "Casks/butter-paper@beta.rb", "SHA256SUMS"]);
        let stable = String::from_utf8(publication.files[1].1.clone()).unwrap();
        assert!(stable.contains("version \"0.1.0\""));
        assert!(stable.contains("download/v#{version}/Butter-Paper-macOS-arm64.zip"));
        assert!(stable.contains("app \"Butter Paper.app\""));
        assert!(stable.contains("depends_on macos: :ventura"));
        let manifest: serde_json::Value = serde_json::from_slice(&publication.files[0].1).unwrap();
        assert_eq!(manifest["channel"], "stable");
        assert_eq!(manifest["minimum_macos"], "13.0");
        assert_eq!(manifest["artifacts"].as_array().unwrap().len(), 4);
        let sums = String::from_utf8(publication.files[3].1.clone()).unwrap();
        assert_eq!(sums.lines().count(), 3);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_beta_release_updates_only_the_beta_cask_and_missing_packages_fail() {
        let directory = assets(&ALL[2..]);
        let jobs = vec!["package macos-arm64".to_string()];
        let beta = publication(Version::parse("0.1.1-beta.2").unwrap(), COMMIT, &directory, "7", "1", &jobs).unwrap();
        assert_eq!(beta.casks, ["butter-paper@beta.rb"]);
        let manifest: serde_json::Value = serde_json::from_slice(&beta.files[0].1).unwrap();
        assert_eq!(manifest["applications"], json!({ "beta": "Butter Paper Beta.app" }));
        let error = publication(Version::parse("0.1.0").unwrap(), COMMIT, &directory, "7", "1", &jobs).err().unwrap();
        assert!(error.contains("missing release asset Butter-Paper-macOS-arm64.zip"), "{error}");
        fs::remove_dir_all(directory).unwrap();
    }
}
