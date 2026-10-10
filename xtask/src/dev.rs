//! `cargo xtask dev`: runs the app and rebuilds and restarts it whenever its
//! sources change, with a disposable data root and the approved PDFium.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, SystemTime},
};

use crate::{Args, Result, command, pdfium, root, target::Target, target_dir};

const POLL: Duration = Duration::from_millis(500);
/// Editors save in bursts; wait for the files to stop changing.
const SETTLE: Duration = Duration::from_millis(300);

pub fn run(args: &mut Args) -> Result {
    let release = args.flag("--release");
    let data = args
        .value("--data")?
        .map(PathBuf::from)
        .unwrap_or_else(|| target_dir().join("dev-profile"));
    let mut documents = Vec::new();
    while let Some(document) = args.positional() {
        documents.push(document);
    }
    args.finish()?;

    let host = Target::host()?;
    let library = pdfium::fetch(host)?.library;
    let app = target_dir()
        .join(if release { "release" } else { "debug" })
        .join(host.exe("butter-paper"));
    let launch = || {
        let mut launch = Command::new(&app);
        launch
            .args(&documents)
            .env("BP_NATIVE_DEVELOPMENT", "1")
            .env("BP_GPUI_DATA_DIR", &data)
            .env("BP_PDFIUM_LIBRARY", &library);
        launch
    };
    eprintln!(
        "Watching crates/ (data in {}). Save a file to rebuild and restart; Ctrl+C to stop.",
        data.display()
    );

    let mut running: Option<Child> = None;
    let mut sources = latest_change();
    loop {
        // Windows cannot replace a running executable, so stop the app before
        // building there; elsewhere it keeps running until the build is done.
        if cfg!(windows) {
            stop(&mut running);
        }
        let built_before = modified(&app);
        let mut build = command(cargo());
        build.args(["build", "--package", "butter-paper", "--bins"]);
        if release {
            build.arg("--release");
        }
        let built = build
            .status()
            .map_err(|error| format!("cargo build: {error}"))?
            .success();
        let still_running = running
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
        if !built {
            eprintln!("Build failed; waiting for changes.");
        } else if !still_running || modified(&app) != built_before {
            stop(&mut running);
            running = Some(
                launch()
                    .spawn()
                    .map_err(|error| format!("{}: {error}", app.display()))?,
            );
        }
        loop {
            thread::sleep(POLL);
            let now = latest_change();
            if now != sources {
                thread::sleep(SETTLE);
                sources = latest_change();
                break;
            }
        }
    }
}

fn cargo() -> PathBuf {
    std::env::var_os("CARGO").map_or_else(|| "cargo".into(), PathBuf::from)
}

fn stop(running: &mut Option<Child>) {
    if let Some(mut child) = running.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// The newest modification time and the file count under the watched paths,
/// so an edit, an added file or a removed file all register.
fn latest_change() -> (Option<SystemTime>, usize) {
    let root = root();
    let mut latest = None;
    let mut count = 0;
    for path in [root.join("Cargo.toml"), root.join("Cargo.lock")] {
        latest = latest.max(modified(&path));
        count += 1;
    }
    let mut pending = vec![root.join("crates")];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            if path.is_dir() {
                pending.push(path);
            } else {
                latest = latest.max(modified(&path));
                count += 1;
            }
        }
    }
    (latest, count)
}
