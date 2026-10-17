//! Render cost benchmark for one large local PDF, run by
//! `cargo xtask render-bench PDF`. It opens the PDF through the worker as the
//! app does and times opening, thumbnails, page navigation renders and
//! viewer tiles, then the latency of a page render requested while a
//! backlog of thumbnail renders is waiting (a click after a long scroll).
//! Results go to `BP_BENCH_OUT/render-bench.json`. Nothing here runs in CI.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use butter_paper::{
    document_resource::{
        DEFAULT_PAGE_RENDER_WIDTH, DocumentId, NativeDocumentOpener, NativeDocumentResource,
        OpenDocumentRequest, PdfiumWorkerBackend, THUMBNAIL_EDGE, thumbnail_width,
    },
    viewer::{PixelRect, RenderSource, TileRequest},
};
use serde_json::{Value, json};

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} is not set")))
}

fn env_number(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.
}

/// n, mean, p50, p95 and max of `values` in milliseconds.
fn summary(values: &[f64]) -> Value {
    if values.is_empty() {
        return json!({ "n": 0 });
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |fraction: f64| sorted[((sorted.len() - 1) as f64 * fraction).round() as usize];
    json!({
        "n": sorted.len(),
        "mean": sorted.iter().sum::<f64>() / sorted.len() as f64,
        "p50": at(0.5),
        "p95": at(0.95),
        "max": sorted[sorted.len() - 1],
        "total": sorted.iter().sum::<f64>(),
    })
}

fn line(name: &str, value: &Value) -> String {
    let number = |key: &str| value[key].as_f64().unwrap_or(0.);
    format!(
        "{name:<34} n {:>4}  p50 {:>8.1}  p95 {:>8.1}  max {:>8.1}  mean {:>8.1} ms",
        value["n"].as_u64().unwrap_or(0),
        number("p50"),
        number("p95"),
        number("max"),
        number("mean"),
    )
}

#[test]
#[ignore = "benchmark: needs BP_PDFIUM_LIBRARY, BP_BENCH_PDF and BP_BENCH_OUT"]
fn render_bench() {
    let library = env_path("BP_PDFIUM_LIBRARY");
    let pdf = env_path("BP_BENCH_PDF");
    let out = env_path("BP_BENCH_OUT");
    let sample = env_number("BP_BENCH_SAMPLE", 24).max(2);
    let backlog = env_number("BP_BENCH_BACKLOG", 48);
    std::fs::create_dir_all(&out).unwrap();
    let worker = PathBuf::from(env!("CARGO_BIN_EXE_butter-paper-pdf-worker"));
    let backend = PdfiumWorkerBackend::new(worker, library, out.join("surfaces"));
    let mut report = serde_json::Map::new();
    let mut lines = Vec::new();

    let started = Instant::now();
    let opened = backend
        .open(&OpenDocumentRequest {
            document_id: DocumentId::new(1),
            generation: 1,
            path: pdf.clone(),
        })
        .expect("the benchmark PDF opens");
    let open_ms = ms(started.elapsed());
    let resource = opened.resource();
    let worker_kib = |resource: &dyn NativeDocumentResource| {
        let pid = resource.worker_pid()?;
        let output = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u64>()
            .ok()
    };
    let opened_kib = worker_kib(resource.as_ref());
    let page_sizes = opened.page_sizes().to_vec();
    let pages = page_sizes.len();
    report.insert("pages".into(), json!(pages));
    report.insert("open_ms".into(), json!(open_ms));
    lines.push(format!(
        "pages {pages}; open {open_ms:.0} ms (includes first page and {} thumbnails)",
        opened.thumbnails().len()
    ));

    // Evenly spaced pages, first and last included.
    let sampled = (0..sample.min(pages))
        .map(|index| (index * (pages - 1) / (sample.min(pages) - 1).max(1)) as u32)
        .collect::<Vec<_>>();
    let mut thumbnail = Vec::new();
    let mut thumbnail_by_page = Vec::new();
    for &page in &sampled {
        let started = Instant::now();
        resource
            .render_thumbnail(page, thumbnail_width(page_sizes[page as usize]))
            .unwrap();
        let elapsed = ms(started.elapsed());
        thumbnail.push(elapsed);
        thumbnail_by_page.push((page, elapsed));
    }
    let thumbnail = summary(&thumbnail);
    lines.push(line(
        &format!("thumbnail {THUMBNAIL_EDGE} px edge"),
        &thumbnail,
    ));
    lines.push(format!(
        "  estimated all {pages} thumbnails: {:.1} s on one worker",
        thumbnail["mean"].as_f64().unwrap_or(0.) * pages as f64 / 1_000.
    ));
    report.insert("thumbnail".into(), thumbnail);
    report.insert(
        "thumbnail_by_page".into(),
        json!(
            thumbnail_by_page
                .iter()
                .map(|(page, ms)| json!([page, ms]))
                .collect::<Vec<_>>()
        ),
    );

    let mut navigation = Vec::new();
    for &page in &sampled {
        let started = Instant::now();
        resource
            .render_page(page, DEFAULT_PAGE_RENDER_WIDTH)
            .unwrap();
        navigation.push(ms(started.elapsed()));
    }
    let navigation = summary(&navigation);
    lines.push(line(
        &format!("page render {DEFAULT_PAGE_RENDER_WIDTH} px"),
        &navigation,
    ));
    report.insert("page_render".into(), navigation);

    // The heaviest sampled page, drawn at fit width in a 1,400 point wide
    // viewport on a 2x display, in 1,024 pixel tiles; twice, since a
    // second pass shows what reloading the page costs.
    let heaviest = thumbnail_by_page
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(0, |(page, _)| *page);
    let (width, height) = page_sizes[heaviest as usize];
    let zoom = 1_400. / width;
    let device_scale = 2.;
    let (pixel_width, pixel_height) = (
        (width * zoom * device_scale).ceil() as usize,
        (height * zoom * device_scale).ceil() as usize,
    );
    let tiles = (0..pixel_height.div_ceil(1_024))
        .flat_map(|row| (0..pixel_width.div_ceil(1_024)).map(move |column| (column, row)))
        .map(|(column, row)| TileRequest {
            source: RenderSource {
                document_id: 1,
                revision: 1,
            },
            generation: 1,
            page: heaviest as usize,
            zoom_tenths: (zoom * 1_000.).round() as u32,
            device_scale_millis: (device_scale * 1_000.) as u32,
            rotation_quarter_turns: 0,
            crop: PixelRect {
                x: column * 1_024,
                y: row * 1_024,
                width: 1_024.min(pixel_width - column * 1_024),
                height: 1_024.min(pixel_height - row * 1_024),
            },
        })
        .collect::<Vec<_>>();
    // The same page again and again: what is left once the page is loaded.
    let mut repeat = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        resource
            .render_thumbnail(heaviest, thumbnail_width(page_sizes[heaviest as usize]))
            .unwrap();
        repeat.push(ms(started.elapsed()));
    }
    let repeat = summary(&repeat);
    lines.push(line(
        &format!("thumbnail page {} repeated", heaviest + 1),
        &repeat,
    ));
    report.insert("thumbnail_repeat".into(), repeat);
    for pass in ["first", "second"] {
        let mut times = Vec::new();
        let started = Instant::now();
        for tile in &tiles {
            let tile_started = Instant::now();
            resource.render_tile(*tile).unwrap();
            times.push(ms(tile_started.elapsed()));
        }
        let whole = ms(started.elapsed());
        let times = summary(&times);
        lines.push(line(
            &format!("tiles page {} ({pass} pass)", heaviest + 1),
            &times,
        ));
        lines.push(format!(
            "  whole page in {} tiles: {whole:.0} ms",
            tiles.len()
        ));
        report.insert(format!("tiles_{pass}"), times);
        report.insert(format!("tiles_{pass}_page_ms"), json!(whole));
    }

    // A click after a long scroll: thumbnails of `backlog` rows already
    // asked for, each waiting on its own thread as the sidebar asks, then a
    // page render for the clicked page.
    let resource_for_backlog = Arc::clone(&resource);
    let sizes = page_sizes.clone();
    let backlog_pages = (0..backlog)
        .map(|index| ((index * 7) % pages) as u32)
        .collect::<Vec<_>>();
    let started = Instant::now();
    let threads = backlog_pages
        .into_iter()
        .map(|page| {
            let resource = Arc::clone(&resource_for_backlog);
            let width = thumbnail_width(sizes[page as usize]);
            std::thread::spawn(move || {
                resource.render_thumbnail(page, width).unwrap();
                Instant::now()
            })
        })
        .collect::<Vec<_>>();
    std::thread::sleep(Duration::from_millis(20));
    let target = (pages * 3 / 4) as u32;
    let clicked = Instant::now();
    resource
        .render_page(target, DEFAULT_PAGE_RENDER_WIDTH)
        .unwrap();
    let click_ms = ms(clicked.elapsed());
    let backlog_done = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .max()
        .map_or(0., |finished| ms(finished - started));
    lines.push(format!(
        "click during {backlog}-thumbnail backlog: page {} shown after {click_ms:.0} ms (backlog drained in {backlog_done:.0} ms)",
        target + 1
    ));
    report.insert("click_during_backlog_ms".into(), json!(click_ms));
    report.insert("backlog_drain_ms".into(), json!(backlog_done));

    let worker =
        json!({ "after_open_kib": opened_kib, "at_end_kib": worker_kib(resource.as_ref()) });
    lines.push(format!("worker resident memory {worker}"));
    report.insert("worker_resident".into(), worker);
    resource.close().unwrap();
    let load = std::process::Command::new("sysctl")
        .args(["-n", "vm.loadavg"])
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    if let Some(load) = &load {
        lines.push(format!("load average {load}"));
    }
    report.insert("load_average".into(), json!(load));
    let report = Value::Object(report);
    std::fs::write(
        out.join("render-bench.json"),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    let text = lines.join("\n");
    std::fs::write(out.join("render-bench.txt"), &text).unwrap();
    println!("\n{text}\n");
}
