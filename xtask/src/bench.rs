//! `cargo xtask render-bench PDF`: times how the worker opens and renders one
//! large local PDF (thumbnails, page navigation, viewer tiles, and a page
//! render requested behind a backlog of thumbnails), through the optimised
//! `render_bench` test. The PDF is only read. Nothing here runs in CI.

use std::path::PathBuf;

use crate::{Args, Result, command, exec, pdfium, root, target::Target, target_dir};

pub fn run(args: &mut Args) -> Result {
    let sample = args.value("--sample")?.unwrap_or_else(|| "24".into());
    let backlog = args.value("--backlog")?.unwrap_or_else(|| "48".into());
    let out = args
        .value("--out")?
        .map(PathBuf::from)
        .unwrap_or_else(|| target_dir().join("render-bench"));
    let pdf = args
        .positional()
        .map(PathBuf::from)
        .ok_or("render-bench needs a PDF")?;
    args.finish()?;
    // The test runs in the crate's directory.
    crate::create_dir(&out)?;
    let out = out
        .canonicalize()
        .map_err(|error| format!("{}: {error}", out.display()))?;
    let pdf = pdf
        .canonicalize()
        .map_err(|error| format!("{}: {error}", pdf.display()))?;
    let library = pdfium::fetch(Target::host()?)?.library;
    exec(
        command(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(root())
            .args([
                "test",
                "--release",
                "--package",
                "butter-paper",
                "--test",
                "render_bench",
                "--",
                "--ignored",
                "--nocapture",
                "--exact",
                "bench_render",
            ])
            .env("BP_PDFIUM_LIBRARY", &library)
            .env("BP_BENCH_PDF", &pdf)
            .env("BP_BENCH_OUT", &out)
            .env("BP_BENCH_SAMPLE", &sample)
            .env("BP_BENCH_BACKLOG", &backlog),
    )?;
    eprintln!("Results: {}", out.join("render-bench.json").display());
    Ok(())
}
