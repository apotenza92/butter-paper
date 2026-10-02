//! `cargo xtask check`: repository rules a compiler cannot see.

use std::path::Path;

use crate::{Result, command, output, root, version};

pub fn run() -> Result {
    let files = repository_files()?;
    let mut violations = Vec::new();
    for file in &files {
        violations.extend(check_path(file));
        let path = root().join(file);
        if is_text(file) {
            if let Ok(text) = std::fs::read_to_string(&path) {
                violations.extend(check_text(file, &text));
            }
        }
    }
    let versions = version::recorded()?;
    if versions.iter().any(|(_, value)| *value != versions[0].1) {
        violations.push(format!("versions disagree: {versions:?} (run cargo xtask version)"));
    }
    if !violations.is_empty() {
        return Err(format!("repository check failed:\n- {}", violations.join("\n- ")));
    }
    println!("Repository check passed ({} files).", files.len());
    Ok(())
}

fn repository_files() -> Result<Vec<String>> {
    let listing = output(
        command("git")
            .args(["ls-files", "--cached", "--others", "--exclude-standard"])
            .current_dir(root()),
    )?;
    Ok(listing.lines().filter(|file| root().join(file).exists()).map(String::from).collect())
}

fn is_text(file: &str) -> bool {
    let extension = Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("");
    ["css", "html", "js", "json", "md", "mjs", "toml", "ts", "yaml", "yml", "rs", "sh", "ps1", "py", "go"]
        .contains(&extension)
}

/// Planning state belongs in docs/planning; build output stays untracked;
/// the Electron and migration-era trees do not return.
pub fn check_path(file: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let name = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase();
    let planning = file.starts_with("docs/planning/") && name.ends_with(".md");
    let components: Vec<&str> = file.split('/').collect();
    let state_directory = components[..components.len() - 1]
        .iter()
        .any(|part| ["plan", "plans", "subagent", "subagents"].contains(&part.to_ascii_lowercase().as_str()));
    let state_name = name.ends_with(".md")
        && ["memory", "now", "worklog", "backlog", "handoff", "roadmap", "plan"]
            .iter()
            .any(|stem| name == format!("{stem}.md") || name.starts_with(&format!("{stem}-")) || name.starts_with(&format!("{stem}_")));
    if !planning && (state_directory || state_name) {
        violations.push(format!("{file}: keep work state in docs/planning/"));
    }
    let generated = ["target", "dist", "test-results", "playwright-report", "coverage", "node_modules", "release", ".prepared"];
    if components[..components.len() - 1].iter().any(|part| generated.contains(part)) {
        violations.push(format!("{file}: generated output must stay untracked"));
    }
    for retired in ["apps/", "packages/", "experiments/", "native/", "pnpm-workspace.yaml", "package.json"] {
        if file == retired.trim_end_matches('/') || (retired.ends_with('/') && file.starts_with(retired)) {
            violations.push(format!("{file}: the Electron-era and migration trees were removed; do not revive them"));
        }
    }
    violations
}

pub fn check_text(file: &str, text: &str) -> Vec<String> {
    let mut violations = Vec::new();
    // Rust sources use fictional homes in fixtures; only this machine's own
    // home path is a leak there.
    let own_home = std::env::var("HOME").ok().filter(|home| home.len() > 1);
    if own_home.is_some_and(|home| text.contains(&format!("{home}/"))) {
        violations.push(format!("{file}: contains this machine's home-directory path"));
        return violations;
    }
    let rust = file.ends_with(".rs");
    for home in ["/Users/", "/home/"] {
        if !rust && text.match_indices(home).any(|(index, _)| {
            let rest = &text[index + home.len()..];
            let user: String = rest.chars().take_while(|c| *c != '/' && !c.is_whitespace() && *c != '"').collect();
            !user.is_empty() && rest[user.len()..].starts_with('/') && user != "runner"
        }) {
            violations.push(format!("{file}: contains a machine-specific home-directory path"));
        }
    }
    if file.starts_with(".github/workflows/") {
        for line in text.lines() {
            let Some(action) = line.trim_start().strip_prefix("uses:").or_else(|| line.trim_start().strip_prefix("- uses:")) else {
                continue;
            };
            let action = action.split('#').next().unwrap().trim();
            let pinned = action
                .rsplit_once('@')
                .is_some_and(|(_, sha)| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()));
            if !action.starts_with("./") && !pinned {
                violations.push(format!("{file}: action is not pinned to a full commit SHA: {action}"));
            }
        }
        if text.contains("gpui-migration") || text.contains("pnpm") || text.contains("setup-node") {
            violations.push(format!("{file}: refers to the retired migration crate or Node tooling"));
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_that_belong_elsewhere_are_reported() {
        assert!(check_path("docs/planning/release.md").is_empty());
        assert!(check_path("crates/butter-paper/src/lib.rs").is_empty());
        assert!(!check_path("notes/worklog.md").is_empty());
        assert!(!check_path("plans/next.md").is_empty());
        assert!(!check_path("crates/butter-paper/target/debug/x").is_empty());
        assert!(!check_path("apps/cli/src/index.ts").is_empty());
        assert!(!check_path("package.json").is_empty());
        assert!(check_path("services/signature-relay/package.json").is_empty());
    }

    #[test]
    fn workflows_pin_actions_and_files_avoid_home_paths() {
        let pinned = "    - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7\n";
        assert!(check_text(".github/workflows/release.yml", pinned).is_empty());
        assert!(!check_text(".github/workflows/release.yml", "      - uses: actions/checkout@v7\n").is_empty());
        assert!(!check_text("README.md", "see /Users/someone/code").is_empty());
        assert!(check_text("README.md", "C:/Users/ and /home/ are prefixes").is_empty());
    }

    #[test]
    fn the_repository_passes() {
        run().unwrap();
    }
}
