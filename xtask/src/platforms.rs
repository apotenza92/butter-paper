//! `cargo xtask platform-checks [--base REF]`: the platforms a change needs a
//! check on by hand, beyond the Linux tests. It reads the change against
//! `REF` (default `origin/main`, including uncommitted and new files) and
//! names each platform-only part it touches: code compiled for one platform,
//! packaging, installers and templates. Windows checks run in a Windows VM
//! or on a PC, macOS checks on a Mac.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Args, Result, command, output, root};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Platform {
    Windows,
    MacOs,
    Linux,
}

impl Platform {
    fn name(self) -> &'static str {
        match self {
            Platform::Windows => "Windows (a Windows VM or PC)",
            Platform::MacOs => "macOS (a Mac)",
            Platform::Linux => "Linux desktop (beyond the headless checks)",
        }
    }
}

const ALL: [Platform; 3] = [Platform::Windows, Platform::MacOs, Platform::Linux];

/// Files that belong to one platform, or to all (packaging, installers,
/// updates and the system integration each platform does its own way).
fn path_platforms(path: &str) -> Vec<Platform> {
    let path = path.to_ascii_lowercase();
    let windows = path.contains("templates/windows/")
        || path.ends_with(".ps1")
        || path.contains("appxmanifest")
        || path.ends_with("xtask/src/store.rs")
        || path.ends_with("crates/butter-paper/build.rs");
    let macos = path.contains("/bundle/")
        || path.ends_with(".plist")
        || path.ends_with(".entitlements")
        || path.ends_with("xtask/src/store_chrome.rs")
        || path.ends_with("xtask/src/thumb_bench.rs");
    let linux = path.contains("templates/linux/") || path.ends_with(".desktop");
    let all = [
        "xtask/src/package.rs",
        ".github/workflows/release.yml",
        ".github/release-targets.json",
        "src/native_updater.rs",
        "src/default_pdf_app.rs",
        "src/system_apps.rs",
        ".cargo/config.toml",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix));
    if all {
        return ALL.to_vec();
    }
    [
        (windows, Platform::Windows),
        (macos, Platform::MacOs),
        (linux, Platform::Linux),
    ]
    .into_iter()
    .filter_map(|(found, platform)| found.then_some(platform))
    .collect()
}

/// The platform a changed line of Rust is compiled or written for.
fn line_platforms(line: &str) -> Vec<Platform> {
    let mut found = Vec::new();
    if line.contains("cfg(windows)")
        || line.contains("target_os = \"windows\"")
        || line.contains("windows_sys::")
        || line.contains("cfg!(windows)")
    {
        found.push(Platform::Windows);
    }
    if line.contains("target_os = \"macos\"")
        || line.contains("core_foundation")
        || line.contains("objc2")
        || line.contains("cocoa::")
    {
        found.push(Platform::MacOs);
    }
    if line.contains("target_os = \"linux\"") || line.contains("cfg(unix)") {
        found.push(Platform::Linux);
    }
    found
}

/// What each platform needs checked in `diff` (unified, with `+++ b/` file
/// headers), and why: one line per file.
fn classify(diff: &str) -> BTreeMap<Platform, BTreeSet<String>> {
    let mut found: BTreeMap<Platform, BTreeSet<String>> = BTreeMap::new();
    let mut file = String::new();
    for line in diff.lines() {
        // A deleted file's new side is /dev/null: it is named on the old side.
        if let Some(path) = line.strip_prefix("--- a/") {
            file = path.to_owned();
            continue;
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            if let Some(path) = path.strip_prefix("b/") {
                file = path.to_owned();
            }
            for platform in path_platforms(&file) {
                found
                    .entry(platform)
                    .or_default()
                    .insert(format!("{file}: platform-specific file"));
            }
            continue;
        }
        if line.starts_with("---") {
            continue;
        }
        let Some(changed) = line.strip_prefix('+').or_else(|| line.strip_prefix('-')) else {
            continue;
        };
        // This file names the patterns it looks for.
        if !file.ends_with(".rs") || file.ends_with("xtask/src/platforms.rs") {
            continue;
        }
        for platform in line_platforms(changed) {
            found
                .entry(platform)
                .or_default()
                .insert(format!("{file}: code compiled only for it"));
        }
    }
    found
}

pub fn run(args: &mut Args) -> Result {
    let base = args
        .value("--base")?
        .unwrap_or_else(|| "origin/main".to_owned());
    args.finish()?;
    let git = |arguments: &[&str]| {
        let mut git = command("git");
        git.args(arguments).current_dir(root());
        output(&mut git)
    };
    let merge_base = git(&["merge-base", &base, "HEAD"])?;
    let mut diff = git(&["diff", "--unified=0", merge_base.trim()])?;
    // New files count as wholly added.
    for path in git(&["ls-files", "--others", "--exclude-standard"])?.lines() {
        diff.push_str(&format!("\n+++ b/{path}\n"));
        if let Ok(text) = std::fs::read_to_string(root().join(path)) {
            for line in text.lines() {
                diff.push_str(&format!("+{line}\n"));
            }
        }
    }
    let found = classify(&diff);
    if found.is_empty() {
        println!("No platform-specific code changed since {base}: the Linux checks cover it.");
        return Ok(());
    }
    println!("Check by hand, beyond the Linux tests (changes since {base}):");
    for (platform, reasons) in found {
        println!("\n{}:", platform.name());
        for reason in reasons {
            println!("  - {reason}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_code_and_files_are_flagged_for_their_platforms() {
        let diff = "\
+++ b/crates/butter-paper/src/document_workspace.rs
+    let shared = 1;
+++ b/crates/butter-paper/src/viewer.rs
+#[cfg(target_os = \"macos\")]
-#[cfg(windows)]
+++ b/xtask/templates/windows/install.ps1
+Write-Host hi
+++ b/docs/agents/releases.md
+cfg(windows) in prose is not code
+++ b/crates/butter-paper/src/default_pdf_app.rs
+fn shared() {}
";
        let found = classify(diff);
        let windows = &found[&Platform::Windows];
        assert!(windows.contains("crates/butter-paper/src/viewer.rs: code compiled only for it"));
        assert!(windows.contains("xtask/templates/windows/install.ps1: platform-specific file"));
        assert!(
            found[&Platform::MacOs]
                .contains("crates/butter-paper/src/viewer.rs: code compiled only for it")
        );
        assert!(
            found[&Platform::Linux]
                .contains("crates/butter-paper/src/default_pdf_app.rs: platform-specific file")
        );
        assert!(
            !found
                .values()
                .flatten()
                .any(|reason| reason.contains("document_workspace")
                    || reason.contains("releases.md"))
        );
    }

    #[test]
    fn a_deleted_file_is_named_by_its_old_path() {
        let diff = "\
--- a/crates/butter-paper/src/word_export.rs
+++ /dev/null
-#[cfg(target_os = \"windows\")]
";
        let found = classify(diff);
        assert!(
            found[&Platform::Windows]
                .contains("crates/butter-paper/src/word_export.rs: code compiled only for it")
        );
    }

    #[test]
    fn a_change_without_platform_code_needs_no_platform_check() {
        assert!(
            classify("+++ b/crates/butter-paper/src/docx_export.rs\n+fn convert() {}\n").is_empty()
        );
    }
}
