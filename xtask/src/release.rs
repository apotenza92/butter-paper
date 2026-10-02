//! `cargo xtask release-check` and `cargo xtask release`.

use crate::{
    Args, Result, check, command, homebrew, output, root, exec,
    target::{Channel, Target},
    target_dir,
    version::{self, Version},
    write,
};

fn step(name: &str) {
    println!("\n▸ {name}");
}

fn git(args: &[&str]) -> Result<String> {
    output(command("git").args(args).current_dir(root()))
}

/// Everything the release workflow would reject, checked locally first.
pub fn check(args: &mut Args) -> Result<Version> {
    let skip_tests = args.flag("--skip-tests");
    args.finish()?;

    step("Repository state");
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

    step("Version and changelog");
    let version = version::current()?;
    let tag = version.tag();
    if !git(&["tag", "--list", &tag])?.is_empty() || !git(&["ls-remote", "--tags", "origin", &tag])?.is_empty() {
        return Err(format!("{tag} already exists; releases are immutable, so bump the version"));
    }
    version::changelog_notes(version)?;
    println!("{tag}: changelog section present.");

    step("Repository checks");
    check::run()?;
    if output(command("actionlint").arg("--version")).is_ok() {
        exec(command("actionlint").current_dir(root()))?;
    } else {
        println!("actionlint is not installed; skipped.");
    }
    if output(command("cargo").args(["deny", "--version"])).is_ok() {
        exec(command("cargo")
            .args(["deny", "--exclude-dev", "--locked", "check", "licenses", "sources", "bans"])
            .current_dir(root()))?;
    } else {
        println!("cargo-deny is not installed; licence and source policy skipped.");
    }

    step("Homebrew bundle");
    let scratch = target_dir().join("release-check");
    crate::fresh_dir(&scratch)?;
    let channels: &[Channel] = if version.is_beta() { &[Channel::Beta] } else { &[Channel::Stable, Channel::Beta] };
    for &channel in channels {
        for triple in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
            let name = Target::parse(triple)?.asset_name(channel);
            write(&scratch.join(&name), format!("placeholder {name}\n"))?;
        }
    }
    let publication =
        homebrew::publication(version, &git(&["rev-parse", "HEAD"])?, &scratch, "1", "1", &["package macos-arm64".into()])?;
    println!("Casks: {}", publication.casks.join(", "));

    if cfg!(target_os = "macos") {
        step("macOS packaging dry run (stub binaries, unsigned)");
        let stubs = scratch.join("stubs");
        write(&stubs.join("stub.c"), "int main(void) { return 0; }\n")?;
        for (arch, triple) in [("arm64", "aarch64-apple-darwin"), ("x86_64", "x86_64-apple-darwin")] {
            let binaries = stubs.join(triple);
            for name in ["butter-paper", "butter-paper-pdf-worker"] {
                crate::create_dir(&binaries)?;
                exec(command("clang")
                    .args(["-target", &format!("{arch}-apple-macos{}", crate::package::MINIMUM_MACOS)])
                    .arg(stubs.join("stub.c"))
                    .arg("-o")
                    .arg(binaries.join(name)))?;
            }
            let out = scratch.join("packages");
            crate::package::package(Target::parse(triple)?, channels, version, &out, false, Some(&binaries))?;
            println!("{triple}: packaged {}", channels.iter().map(|c| Target::parse(triple).unwrap().asset_name(*c)).collect::<Vec<_>>().join(", "));
        }
    }

    if !skip_tests {
        step("Tests");
        exec(command("cargo").args(["test", "--workspace", "--locked", "--quiet"]).current_dir(root()))?;
    }
    println!("\n✓ {tag} is ready to release.");
    Ok(version)
}

pub fn release(args: &mut Args) -> Result {
    let version = check(args)?;
    let tag = version.tag();
    exec(command("git").args(["tag", "-a", &tag, "-m", &format!("Butter Paper {version}")]).current_dir(root()))?;
    exec(command("git").args(["push", "origin", &tag]).current_dir(root()))?;
    println!("\n✓ Pushed {tag}. Follow it with: gh run watch $(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId')");
    Ok(())
}
