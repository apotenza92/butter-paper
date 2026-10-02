//! Butter Paper's build, packaging and release tasks: `cargo xtask <command>`.

mod archive;
mod check;
mod homebrew;
mod notices;
mod package;
mod pdfium;
mod phone;
mod release;
mod target;
mod version;

use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use sha2::{Digest, Sha256};

pub type Result<T = ()> = std::result::Result<T, String>;

const USAGE: &str = "usage: cargo xtask <command>

  check                       repository, version and workflow checks
  version X.Y.Z[-beta.N]      set the app version and start its CHANGELOG section
  pdfium [--target T]         fetch the approved PDFium for T (default: host)
  phone-helper --target T --out DIR
                              build the phone signature helper and its notices
  notices --target T --out FILE
                              write the complete third-party notices for T
  package --target T --channel stable|beta --out DIR [--sign] [--binaries DIR]
                              build and package one release asset
  publish-assets --assets DIR --tag vX.Y.Z --commit SHA --run-id N --run-attempt N --jobs a,b
                              add the Homebrew bundle and SHA256SUMS.txt
  release-check [--skip-tests]
                              everything the release workflow would reject
  release [--skip-tests]      release-check, then push the version tag";

fn main() -> ExitCode {
    let mut args = Args(env::args().skip(1).collect());
    let command = args.0.first().cloned().unwrap_or_default();
    if !command.is_empty() {
        args.0.remove(0);
    }
    let result = match command.as_str() {
        "check" => args.finish().and_then(|()| check::run()),
        "version" => version::run(&mut args),
        "pdfium" => pdfium::run(&mut args),
        "phone-helper" => phone::run(&mut args),
        "notices" => notices::run(&mut args),
        "package" => package::run(&mut args),
        "publish-assets" => homebrew::run(&mut args),
        "release-check" => release::check(&mut args).map(|_| ()),
        "release" => release::release(&mut args),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Command-line options, consumed as they are read.
pub struct Args(Vec<String>);

impl Args {
    pub fn value(&mut self, name: &str) -> Result<Option<String>> {
        let Some(index) = self.0.iter().position(|arg| arg == name) else {
            return Ok(None);
        };
        if index + 1 >= self.0.len() {
            return Err(format!("{name} needs a value"));
        }
        let value = self.0.remove(index + 1);
        self.0.remove(index);
        Ok(Some(value))
    }

    pub fn required(&mut self, name: &str) -> Result<String> {
        self.value(name)?.ok_or_else(|| format!("{name} is required"))
    }

    pub fn flag(&mut self, name: &str) -> bool {
        let found = self.0.iter().position(|arg| arg == name);
        if let Some(index) = found {
            self.0.remove(index);
        }
        found.is_some()
    }

    pub fn positional(&mut self) -> Option<String> {
        let index = self.0.iter().position(|arg| !arg.starts_with("--"))?;
        Some(self.0.remove(index))
    }

    pub fn finish(&self) -> Result {
        match self.0.first() {
            Some(extra) => Err(format!("unexpected argument {extra}\n\n{USAGE}")),
            None => Ok(()),
        }
    }
}

/// The repository root (the parent of this crate).
pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

/// The app crate.
pub fn app_crate() -> PathBuf {
    root().join("crates/butter-paper")
}

/// Cargo's target directory for this workspace.
pub fn target_dir() -> PathBuf {
    env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root().join("target"))
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256(&read(path)?))
}

pub fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn read_text(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn write(path: &Path, bytes: impl AsRef<[u8]>) -> Result {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    fs::write(path, bytes).map_err(|error| format!("{}: {error}", path.display()))
}

pub fn create_dir(path: &Path) -> Result {
    fs::create_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// Removes a directory if it exists, then creates it empty.
pub fn fresh_dir(path: &Path) -> Result {
    if path.exists() {
        fs::remove_dir_all(path).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    create_dir(path)
}

pub fn copy(from: &Path, to: &Path) -> Result {
    if let Some(parent) = to.parent() {
        create_dir(parent)?;
    }
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|error| format!("copy {} to {}: {error}", from.display(), to.display()))
}

#[cfg(unix)]
pub fn set_mode(path: &Path, mode: u32) -> Result {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(not(unix))]
pub fn set_mode(_path: &Path, _mode: u32) -> Result {
    Ok(())
}

pub fn command(program: impl AsRef<OsStr>) -> Command {
    Command::new(program)
}

/// Runs a command with inherited output; fails on a non-zero exit.
pub fn exec(command: &mut Command) -> Result {
    let status = command.status().map_err(|error| format!("{}: {error}", describe(command)))?;
    if status.success() { Ok(()) } else { Err(format!("{} failed ({status})", describe(command))) }
}

/// Runs a command and returns its trimmed standard output.
pub fn output(command: &mut Command) -> Result<String> {
    let output = command.output().map_err(|error| format!("{}: {error}", describe(command)))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed ({}): {}",
            describe(command),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn describe(command: &Command) -> String {
    let mut text = command.get_program().to_string_lossy().into_owned();
    for arg in command.get_args() {
        text.push(' ');
        text.push_str(&arg.to_string_lossy());
    }
    text
}

/// Downloads `url` to `path` with curl and checks its SHA-256.
pub fn download(url: &str, path: &Path, sha256: &str) -> Result {
    if path.is_file() && sha256_file(path)? == sha256 {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }
    let partial = path.with_extension("partial");
    exec(command("curl")
        .args(["--fail", "--location", "--silent", "--show-error", "--retry", "3", "--output"])
        .arg(&partial)
        .arg(url))?;
    let actual = sha256_file(&partial)?;
    if actual != sha256 {
        let _ = fs::remove_file(&partial);
        return Err(format!("{url}: SHA-256 {actual} does not match the pinned {sha256}"));
    }
    fs::rename(&partial, path).map_err(|error| format!("{}: {error}", path.display()))
}
