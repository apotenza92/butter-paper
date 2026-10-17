//! `cargo xtask thumb-bench PDF`: the in-app thumbnails benchmark (the review
//! driver's `thumb-bench` command) on one large local PDF, in a uniquely
//! named review bundle launched in the background with a disposable profile.
//! It runs `--runs` times and reports each measure's median, so changes can
//! be compared under the same conditions. macOS only; the PDF is only read.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

use crate::{
    Args, Result, app_crate, command, copy, create_dir, exec, fresh_dir, output, pdfium, read_text,
    root, target::Target, target_dir, write,
};

const BUNDLE_ID: &str = "dev.butterpaper.thumb-bench";
/// A run takes about a minute; longer means the app is stuck.
const RUN_LIMIT: Duration = Duration::from_secs(600);

pub fn run(args: &mut Args) -> Result {
    if !cfg!(target_os = "macos") {
        return Err("thumb-bench drives a macOS review bundle".into());
    }
    let runs = args
        .value("--runs")?
        .map(|value| value.parse::<usize>().map_err(|_| "--runs needs a number"))
        .transpose()?
        .unwrap_or(3)
        .max(1);
    let fraction = args.value("--fraction")?.unwrap_or_else(|| "0.9".into());
    // Row 59 is page 60, the heaviest page of the benchmark booklet.
    let click_row = args.value("--click-row")?.unwrap_or_else(|| "59".into());
    let fling = args.value("--fling-ms")?.unwrap_or_else(|| "1500".into());
    let label = args.value("--label")?.unwrap_or_else(|| "latest".into());
    let mode = args.value("--mode")?.unwrap_or_else(|| "single".into());
    let sweep = args.value("--sweep")?.unwrap_or_else(|| "0".into());
    if mode != "single" && mode != "continuous" {
        return Err("--mode is single or continuous".into());
    }
    let pdf = args
        .positional()
        .map(PathBuf::from)
        .ok_or("thumb-bench needs a PDF")?;
    args.finish()?;
    let pdf = pdf
        .canonicalize()
        .map_err(|error| format!("{}: {error}", pdf.display()))?;
    let library = pdfium::fetch(Target::host()?)?.library;
    // Always: other commands (render-bench, tests) rebuild the same binaries
    // without the review driver, and a bundle of those never runs a script.
    exec(
        command(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(root())
            .args([
                "build",
                "--release",
                "--package",
                "butter-paper",
                "--features",
                "review-driver",
            ]),
    )?;
    let work = target_dir().join("thumb-bench");
    create_dir(&work)?;
    let bundle = bundle(&work)?;
    stop_running(&bundle)?;

    let mut results = Vec::new();
    for index in 0..runs {
        let run = work.join(format!("{label}-run-{index}"));
        fresh_dir(&run)?;
        let captures = run.join("captures");
        create_dir(&captures)?;
        let script = run.join("script.txt");
        // Off screen, so the window never covers anyone's work.
        write(
            &script,
            format!(
                "place 0 9000 9000 1400 900\nwait 500\nthumb-bench 0 {fraction} {fling} {click_row} {mode} {sweep}\nquit\n"
            ),
        )?;
        eprintln!("run {}/{runs}", index + 1);
        let started = Instant::now();
        let mut launch = command("open");
        launch
            .args(["-g", "-n", "-W", "-a"])
            .arg(&bundle)
            .args(["--env", "BP_NATIVE_DEVELOPMENT=1"])
            .arg("--env")
            .arg(format!("BP_PDFIUM_LIBRARY={}", library.display()))
            .arg("--env")
            .arg(format!(
                "BP_GPUI_DATA_DIR={}",
                run.join("profile").display()
            ))
            .arg("--env")
            .arg(format!("BP_REVIEW_SCRIPT={}", script.display()))
            .arg("--env")
            .arg(format!("BP_REVIEW_OUT={}", captures.display()))
            .arg("--args")
            .arg(&pdf);
        let mut child = launch.spawn().map_err(|error| format!("open: {error}"))?;
        loop {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_some()
            {
                break;
            }
            if started.elapsed() > RUN_LIMIT {
                stop_running(&bundle)?;
                let _ = child.kill();
                return Err(format!(
                    "run {index} did not finish within {RUN_LIMIT:?}; see {}",
                    captures.join("review.log").display()
                ));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let result = captures.join("thumb-bench.json");
        let text = read_text(&result).map_err(|error| {
            format!(
                "run {index} wrote no results after {:?}: {error}",
                started.elapsed()
            )
        })?;
        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        eprintln!("{}", one_line(&value));
        results.push(value);
        std::thread::sleep(Duration::from_secs(2));
    }
    let summary = medians(&results);
    let load = output(command("sysctl").args(["-n", "vm.loadavg"])).unwrap_or_default();
    let report = json!({ "label": label, "runs": results.len(), "load_average": load, "median": summary, "all": results });
    let path = work.join(format!("{label}.json"));
    write(&path, serde_json::to_string_pretty(&report).unwrap())?;
    eprintln!(
        "\nmedian of {} runs ({label}):\n{}",
        results.len(),
        table(&summary)
    );
    eprintln!("Results: {}", path.display());
    Ok(())
}

/// A copy of the release build in its own bundle, so LaunchServices starts
/// exactly this build with the benchmark's environment.
fn bundle(work: &Path) -> Result<PathBuf> {
    let bundle = work.join("Butter Paper Thumb Bench.app");
    let macos = bundle.join("Contents/MacOS");
    create_dir(&macos)?;
    let plist = read_text(&app_crate().join("bundle/Info.plist"))?
        .replace("dev.butterpaper.development", BUNDLE_ID)
        .replace("Butter Paper Development", "Butter Paper Thumb Bench");
    write(&bundle.join("Contents/Info.plist"), plist)?;
    for binary in ["butter-paper", "butter-paper-pdf-worker"] {
        let installed = macos.join(binary);
        // Copying over a binary that has run gets the next launch killed.
        if installed.exists() {
            fs::remove_file(&installed).map_err(|error| error.to_string())?;
        }
        copy(&target_dir().join("release").join(binary), &installed)?;
    }
    Ok(bundle)
}

fn stop_running(bundle: &Path) -> Result {
    let executable = bundle.join("Contents/MacOS/butter-paper");
    let _ = command("pkill")
        .arg("-f")
        .arg(executable.display().to_string())
        .status();
    Ok(())
}

fn numbers(value: &Value, prefix: &str, into: &mut BTreeMap<String, f64>) {
    match value {
        Value::Number(number) => {
            if let Some(number) = number.as_f64() {
                into.insert(prefix.to_owned(), number);
            }
        }
        Value::Object(map) => {
            for (key, value) in map {
                let name = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                numbers(value, &name, into);
            }
        }
        _ => {}
    }
}

fn medians(results: &[Value]) -> BTreeMap<String, f64> {
    let mut columns: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for result in results {
        let mut values = BTreeMap::new();
        numbers(result, "", &mut values);
        for (key, value) in values {
            columns.entry(key).or_default().push(value);
        }
    }
    columns
        .into_iter()
        .map(|(key, mut values)| {
            values.sort_by(f64::total_cmp);
            (key, values[values.len() / 2])
        })
        .collect()
}

const HEADLINE: [&str; 19] = [
    "first_rows_ms",
    "fling_first_row_ms",
    "fling_rows_ms",
    "fling_click_shown_ms",
    "fling_click_sharp_ms",
    "jump_first_row_ms",
    "jump_rows_ms",
    "jump_click_shown_ms",
    "jump_click_sharp_ms",
    "main_thread_lateness.max",
    "main_thread_lateness.p99",
    "stalls_over_50ms",
    "frame_cpu.p95",
    "canvas_regressions",
    "canvas_flashes",
    "app_resident_kib",
    "app_footprint_kib",
    "sweep_page_sharp.p50",
    "sweep_footprint_after_kib",
];

fn table(summary: &BTreeMap<String, f64>) -> String {
    HEADLINE
        .iter()
        .map(|key| {
            format!(
                "  {key:<28} {}",
                summary
                    .get(*key)
                    .map_or("-".to_owned(), |value| format!("{value:.0}"))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn one_line(value: &Value) -> String {
    let mut values = BTreeMap::new();
    numbers(value, "", &mut values);
    HEADLINE
        .iter()
        .filter_map(|key| values.get(*key).map(|value| format!("{key}={value:.0}")))
        .collect::<Vec<_>>()
        .join(" ")
}
