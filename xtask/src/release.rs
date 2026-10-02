//! `cargo xtask release`: instant checks that stop a tag the release
//! workflow would reject, then push the tag. Testing is not part of releasing;
//! run `cargo test` and `cargo xtask check` as you work.

use crate::{Args, Result, command, exec, output, root, version};

fn git(args: &[&str]) -> Result<String> {
    output(command("git").args(args).current_dir(root()))
}

pub fn release(args: &mut Args) -> Result {
    args.finish()?;
    if git(&["branch", "--show-current"])? != "main" {
        return Err("release from main".into());
    }
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err("the working tree has uncommitted changes".into());
    }
    git(&["fetch", "--quiet", "origin", "main", "--tags"])?;
    if git(&["rev-parse", "HEAD"])? != git(&["rev-parse", "origin/main"])? {
        return Err("HEAD must equal origin/main (push or pull first)".into());
    }
    let version = version::current()?;
    let tag = version.tag();
    if !git(&["tag", "--list", &tag])?.is_empty() {
        return Err(format!("{tag} already exists; releases are immutable, so bump the version"));
    }
    version::changelog_notes(version)?;
    let versions = version::recorded()?;
    if versions.iter().any(|(_, value)| *value != versions[0].1) {
        return Err(format!("versions disagree: {versions:?} (run cargo xtask version)"));
    }
    exec(command("git").args(["tag", "-a", &tag, "-m", &format!("Butter Paper {version}")]).current_dir(root()))?;
    exec(command("git").args(["push", "origin", &tag]).current_dir(root()))?;
    println!("Pushed {tag}. Follow it with: gh run watch $(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')");
    Ok(())
}
