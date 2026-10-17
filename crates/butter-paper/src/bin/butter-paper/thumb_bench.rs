//! The review driver's `thumb-bench` command: a repeatable benchmark of the
//! thumbnails sidebar on a large PDF, run in the app itself.
//!
//! A background window is given no frames, so the benchmark draws one every
//! 16 ms itself, as a 60 Hz display would ask. Meanwhile a 4 ms heartbeat on
//! the main thread measures how long input would have waited. It then:
//! waits for the first thumbnails; flings the list to FRACTION of its length
//! with wheel events over FLING_MS and at once clicks row CLICK_ROW (or the
//! middle row), timing until the rows shown have thumbnails and until the
//! clicked page shows and is sharp; then snaps the list to its end, as a
//! click on its scroll bar does, and times the same for rows nobody asked
//! for yet. Every frame it checks that the current page never loses a tile
//! it had drawn, or drops back to a lower quality, while it stays the same
//! page; and in quiet spells it compares captured frames of the canvas,
//! counting changes the tile state does not explain: the flashing a user
//! sees. Results go to `thumb-bench.json` and the log.

use std::{
    cell::{Cell, RefCell},
    io::Write as _,
    path::Path,
    rc::Rc,
    time::{Duration, Instant},
};

use butter_paper::document_workspace::ThumbnailBenchProbe;
use gpui::{
    AsyncApp, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, PlatformInput, ScrollDelta,
    ScrollWheelEvent, TouchPhase, point, px,
};
use serde_json::{Value, json};

const FRAME: Duration = Duration::from_millis(16);
const HEARTBEAT: Duration = Duration::from_millis(4);
const WAIT_LIMIT: Duration = Duration::from_secs(90);

#[derive(Default)]
struct Recorder {
    /// How late each heartbeat ran, in ms.
    lateness: Vec<f64>,
    /// Main-thread time spent drawing each frame, in ms.
    frames: Vec<f64>,
    /// Frames on which the current page lost a drawn tile or quality.
    regressions: usize,
    regression_log: Vec<String>,
    previous: Option<ThumbnailBenchProbe>,
    latest: Option<ThumbnailBenchProbe>,
    started: Option<Instant>,
    /// Bumped around each canvas capture, which blocks the main thread
    /// itself: heartbeats spanning one are not the app's lateness.
    captures: u64,
    /// Each change of what the canvas shows, timed from the start.
    transitions: Vec<String>,
}

fn mark(recorder: &RefCell<Recorder>, what: &str) {
    let mut recorder = recorder.borrow_mut();
    let at = recorder.started.get_or_insert_with(Instant::now).elapsed();
    let line = format!("{:>8.0} ms -- {what}", at.as_secs_f64() * 1_000.);
    recorder.transitions.push(line);
}

fn summary(values: &[f64]) -> Value {
    if values.is_empty() {
        return json!({ "n": 0 });
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let at = |fraction: f64| sorted[((sorted.len() - 1) as f64 * fraction).round() as usize];
    json!({
        "n": sorted.len(),
        "p50": at(0.5),
        "p95": at(0.95),
        "p99": at(0.99),
        "max": sorted[sorted.len() - 1],
    })
}

fn probe(cx: &mut AsyncApp, window: usize) -> Option<ThumbnailBenchProbe> {
    cx.update(|cx| {
        let entry = super::document_window_entries(cx).get(window).cloned()?;
        entry
            .workspace
            .upgrade()?
            .read(cx)
            .thumbnail_bench_probe(cx)
    })
}

fn send(cx: &mut AsyncApp, window: usize, event: PlatformInput) {
    cx.update(|cx| {
        if let Some(entry) = super::document_window_entries(cx).get(window).cloned() {
            let _ = entry
                .handle
                .update(cx, |_, window, cx| window.dispatch_event(event, cx));
        }
    });
}

/// Draws a frame, as the display would ask, and records what it showed.
fn draw_frame(cx: &mut AsyncApp, window: usize, recorder: &RefCell<Recorder>) {
    let started = Instant::now();
    cx.update(|cx| {
        if let Some(entry) = super::document_window_entries(cx).get(window).cloned()
            && let Some(workspace) = entry.workspace.upgrade()
        {
            let _ = entry.handle.update(cx, |_, window, cx| {
                workspace.update(cx, |workspace, cx| {
                    workspace.run_scheduled_viewport_refresh(window, cx)
                });
                window.draw(cx).clear(cx);
            });
        }
    });
    let elapsed = started.elapsed().as_secs_f64() * 1_000.;
    let latest = probe(cx, window);
    let mut recorder = recorder.borrow_mut();
    recorder.frames.push(elapsed);
    if let (Some(previous), Some(latest)) = (recorder.previous.clone(), latest.clone())
        && previous.current_page == latest.current_page
        && previous.current_tiles_planned == latest.current_tiles_planned
        && (latest.current_tiles_drawn < previous.current_tiles_drawn
            || latest.current_tiles_sharp < previous.current_tiles_sharp
            || latest.current_tiles_detail < previous.current_tiles_detail
            || (previous.current_base_image && !latest.current_base_image))
    {
        recorder.regressions += 1;
        let line = format!(
            "page {} plan {} -> {}: drawn {} -> {}, sharp {} -> {}, detail {} -> {}, base {} -> {}",
            latest.current_page + 1,
            previous.plan_generation,
            latest.plan_generation,
            previous.current_tiles_drawn,
            latest.current_tiles_drawn,
            previous.current_tiles_sharp,
            latest.current_tiles_sharp,
            previous.current_tiles_detail,
            latest.current_tiles_detail,
            previous.current_base_image,
            latest.current_base_image
        );
        if recorder.regression_log.len() < 40 {
            recorder.regression_log.push(line);
        }
    }
    if let Some(latest) = &latest {
        let key = |probe: &ThumbnailBenchProbe| {
            (
                probe.current_page,
                probe.plan_generation,
                probe.current_tiles_planned,
                probe.current_tiles_drawn,
                probe.current_tiles_sharp,
                probe.current_tiles_detail,
                probe.visible_tiles_by_quality,
                probe.current_base_width,
                format!("{:.1} {:?}", probe.zoom_percent, probe.viewport),
            )
        };
        if recorder.previous.as_ref().map(key) != Some(key(latest)) {
            let at = recorder.started.get_or_insert_with(Instant::now).elapsed();
            let line = format!("{:>8.0} ms {:?}", at.as_secs_f64() * 1_000., key(latest));
            if recorder.transitions.len() < 400 {
                recorder.transitions.push(line);
            }
        }
    }
    if latest.is_some() {
        recorder.previous = latest.clone();
        recorder.latest = latest;
    }
}

async fn frame(cx: &mut AsyncApp, window: usize, recorder: &RefCell<Recorder>) {
    let started = Instant::now();
    draw_frame(cx, window, recorder);
    cx.background_executor()
        .timer(FRAME.saturating_sub(started.elapsed()))
        .await;
}

/// Draws frames until `done` holds; the milliseconds it took, or `None`
/// after the wait limit.
async fn until(
    cx: &mut AsyncApp,
    window: usize,
    recorder: &RefCell<Recorder>,
    done: impl Fn(&ThumbnailBenchProbe) -> bool,
) -> Option<f64> {
    let started = Instant::now();
    while started.elapsed() < WAIT_LIMIT {
        if recorder.borrow().latest.as_ref().is_some_and(&done) {
            return Some(started.elapsed().as_secs_f64() * 1_000.);
        }
        frame(cx, window, recorder).await;
    }
    None
}

fn all_visible_ready(probe: &ThumbnailBenchProbe) -> bool {
    !probe.visible_rows.is_empty() && probe.visible_ready == probe.visible_rows.len()
}

/// Clicks row `row`'s preview, or the middle visible row's; the page clicked.
fn click_row(
    cx: &mut AsyncApp,
    window: usize,
    recorder: &RefCell<Recorder>,
    row: Option<usize>,
) -> Option<u32> {
    let latest = recorder.borrow().latest.clone()?;
    let rows = latest.visible_rows.clone();
    let row = row
        .filter(|row| rows.contains(row))
        .unwrap_or((rows.start + rows.end.saturating_sub(1)) / 2);
    let y = f32::from(latest.list_bounds.origin.y) + row as f32 * latest.row_height
        - latest.scroll_top
        + latest.row_height * 0.6;
    let position = point(latest.list_bounds.center().x, px(y));
    send(
        cx,
        window,
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }),
    );
    send(
        cx,
        window,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }),
    );
    Some(row as u32)
}

/// After a move of the list to rows not yet shown, and a click on one of
/// them: the times until the first and then every row shown has its
/// thumbnail, and until the clicked page shows and then is sharp.
async fn measure_move_and_click(
    cx: &mut AsyncApp,
    window: usize,
    recorder: &RefCell<Recorder>,
    name: &str,
    row: Option<usize>,
    results: &mut serde_json::Map<String, Value>,
) {
    frame(cx, window, recorder).await;
    results.insert(
        format!("{name}_app_threads"),
        json!(thread_count(std::process::id())),
    );
    let rows = recorder
        .borrow()
        .latest
        .as_ref()
        .map(|probe| format!("{:?}", probe.visible_rows));
    let page = click_row(cx, window, recorder, row);
    mark(
        recorder,
        &format!(
            "{name}: rows {rows:?}, clicked page {:?}",
            page.map(|page| page + 1)
        ),
    );
    let started = Instant::now();
    let mut times: [Option<f64>; 4] = [None; 4];
    while started.elapsed() < WAIT_LIMIT && times.iter().any(Option::is_none) {
        if let Some(probe) = recorder.borrow().latest.clone() {
            let elapsed = started.elapsed().as_secs_f64() * 1_000.;
            // Shown: the clicked page is current and shows its content,
            // not a blank page.
            let shown = Some(probe.current_page) == page
                && (probe.current_base_image || probe.current_tiles_drawn > 0);
            let checks = [
                probe.visible_ready > 0,
                all_visible_ready(&probe),
                shown,
                shown
                    && probe.current_tiles_planned > 0
                    && probe.current_tiles_sharp == probe.current_tiles_planned,
            ];
            for (time, check) in times.iter_mut().zip(checks) {
                if time.is_none() && check {
                    *time = Some(elapsed);
                }
            }
        }
        frame(cx, window, recorder).await;
    }
    results.insert(format!("{name}_rows"), json!(rows));
    results.insert(
        format!("{name}_click_page"),
        json!(page.map(|page| page + 1)),
    );
    for (key, time) in [
        "first_row_ms",
        "rows_ms",
        "click_shown_ms",
        "click_sharp_ms",
    ]
    .into_iter()
    .zip(times)
    {
        results.insert(format!("{name}_{key}"), json!(time));
    }
}

/// The canvas part of a frame drawn now, as RGBA rows.
fn capture_canvas(cx: &mut AsyncApp, window: usize, left: f32) -> Option<image::RgbaImage> {
    cx.update(|cx| {
        let entry = super::document_window_entries(cx).get(window).cloned()?;
        entry
            .handle
            .update(cx, |_, window, _| {
                let image = window.render_to_image().ok()?;
                let scale = window.scale_factor();
                let x = ((left + 8.) * scale) as u32;
                let y = (140. * scale) as u32;
                (image.width() > x + 16 && image.height() > y + 16).then(|| {
                    image::imageops::crop_imm(&image, x, y, image.width() - x, image.height() - y)
                        .to_image()
                })
            })
            .ok()
            .flatten()
    })
}

/// A quiet spell of `seconds`: frames keep drawing, and every eighth one is
/// captured and compared with the one before. A change while the current
/// page's tile state stayed the same is a flash.
async fn quiet_spell(
    cx: &mut AsyncApp,
    window: usize,
    recorder: &RefCell<Recorder>,
    seconds: u64,
    out: &Path,
    flashes: &mut usize,
    captures: &mut usize,
) {
    mark(recorder, "quiet spell");
    let started = Instant::now();
    type CanvasState = (u32, usize, usize, usize, usize, usize, [usize; 3], u64, i32);
    let mut previous: Option<(image::RgbaImage, Option<CanvasState>)> = None;
    let mut frames = 0;
    while started.elapsed() < Duration::from_secs(seconds) {
        frame(cx, window, recorder).await;
        frames += 1;
        if frames % 8 != 0 {
            continue;
        }
        // Taken after the capture, which draws the state as it is now.
        let state = |probe: Option<ThumbnailBenchProbe>| {
            probe.map(|probe| {
                (
                    probe.current_page,
                    probe.current_tiles_planned,
                    probe.current_tiles_drawn,
                    probe.current_tiles_sharp,
                    probe.current_tiles_detail,
                    probe.visible_tiles_drawn,
                    probe.visible_tiles_by_quality,
                    probe.plan_generation,
                    probe.current_base_width,
                )
            })
        };
        let left = recorder
            .borrow()
            .latest
            .as_ref()
            .map_or(0., |probe| f32::from(probe.list_bounds.right()));
        recorder.borrow_mut().captures += 1;
        let image = capture_canvas(cx, window, left);
        recorder.borrow_mut().captures += 1;
        let Some(image) = image else {
            continue;
        };
        let state = state(probe(cx, window));
        *captures += 1;
        if let Some((before, before_state)) = &previous
            && *before_state == state
            && before.dimensions() == image.dimensions()
        {
            let changed = before
                .pixels()
                .zip(image.pixels())
                .filter(|(a, b)| a.0.iter().zip(b.0.iter()).any(|(a, b)| a.abs_diff(*b) > 24))
                .count();
            if changed * 1_000 > image.pixels().len() {
                if *flashes < 6 {
                    let _ = before.save(out.join(format!("flash-{flashes}-before.png")));
                    let _ = image.save(out.join(format!("flash-{flashes}-after.png")));
                }
                *flashes += 1;
                mark(recorder, &format!("flash: {changed} pixels changed"));
            }
        }
        previous = Some((image, state));
    }
}

fn thread_count(pid: u32) -> Option<usize> {
    let output = std::process::Command::new("ps")
        .args(["-M", "-p", &pid.to_string()])
        .output()
        .ok()?;
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .count()
            .saturating_sub(1),
    )
}

/// The process's physical footprint, in KiB: its memory, GPU textures
/// included, as Activity Monitor's Memory column shows.
#[cfg(target_os = "macos")]
fn footprint_kib() -> Option<u64> {
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as i32,
            libc::RUSAGE_INFO_V2,
            (&mut info as *mut libc::rusage_info_v2).cast(),
        )
    };
    (result == 0).then_some(info.ri_phys_footprint / 1024)
}

#[cfg(not(target_os = "macos"))]
fn footprint_kib() -> Option<u64> {
    None
}

/// Moves to each of the first `pages` pages in turn, waiting until each is
/// sharp (or four seconds): the canvas then draws tiles for every one, which
/// shows whether their memory is released as later pages replace them.
async fn sweep(
    cx: &mut AsyncApp,
    window: usize,
    recorder: &RefCell<Recorder>,
    pages: u32,
    results: &mut serde_json::Map<String, Value>,
) {
    results.insert("sweep_footprint_before_kib".into(), json!(footprint_kib()));
    let mut times = Vec::new();
    for page in 0..pages {
        cx.update(|cx| {
            if let Some(workspace) = super::document_window_entries(cx)
                .get(window)
                .and_then(|entry| entry.workspace.upgrade())
            {
                workspace.update(cx, |workspace, cx| {
                    if let Some(id) = workspace.active_document_id() {
                        workspace.activate_thumbnail_page(id, page, cx);
                    }
                });
            }
        });
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(4) {
            frame(cx, window, recorder).await;
            if recorder.borrow().latest.as_ref().is_some_and(|probe| {
                probe.current_page == page
                    && probe.current_tiles_planned > 0
                    && probe.current_tiles_sharp == probe.current_tiles_planned
            }) {
                break;
            }
        }
        times.push(started.elapsed().as_secs_f64() * 1_000.);
    }
    results.insert("sweep_page_sharp".into(), summary(&times));
    results.insert("sweep_footprint_after_kib".into(), json!(footprint_kib()));
}

fn resident_kib(pid: u32) -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout).trim().parse().ok()
}

pub async fn run(
    cx: &mut AsyncApp,
    window: usize,
    fraction: f32,
    fling_ms: u64,
    click_row: Option<usize>,
    continuous: bool,
    sweep_pages: u32,
    out: &Path,
    log: &mut std::fs::File,
) {
    let recorder = Rc::new(RefCell::new(Recorder::default()));
    let running = Rc::new(Cell::new(true));
    let heartbeat = {
        let recorder = recorder.clone();
        let running = running.clone();
        cx.spawn(async move |cx| {
            while running.get() {
                let started = Instant::now();
                let captures = recorder.borrow().captures;
                cx.background_executor().timer(HEARTBEAT).await;
                let late = started.elapsed().saturating_sub(HEARTBEAT);
                let mut recorder = recorder.borrow_mut();
                if recorder.captures == captures {
                    recorder.lateness.push(late.as_secs_f64() * 1_000.);
                }
            }
        })
    };
    let mut results = serde_json::Map::new();
    let started = Instant::now();
    let opened = until(cx, window, &recorder, |probe| probe.page_count > 0).await;
    results.insert("ready_ms".into(), json!(opened));
    cx.update(|cx| {
        if let Some(workspace) = super::document_window_entries(cx)
            .get(window)
            .and_then(|entry| entry.workspace.upgrade())
        {
            workspace.update(cx, |workspace, cx| {
                workspace.thumbnail_bench_open_pages_panel(cx);
                if continuous && let Some(id) = workspace.active_document_id() {
                    workspace.set_page_view_mode(
                        id,
                        butter_paper::page_view_control::PageViewMode::Continuous,
                        cx,
                    );
                }
            });
        }
    });
    let first = until(cx, window, &recorder, all_visible_ready).await;
    results.insert("first_rows_ms".into(), json!(first));
    let (mut flashes, mut captures) = (0, 0);
    quiet_spell(cx, window, &recorder, 2, out, &mut flashes, &mut captures).await;

    // Fling: wheel events once a frame, down to FRACTION of the list.
    let Some(latest) = recorder.borrow().latest.clone() else {
        writeln!(log, "thumb-bench: the document did not open").unwrap();
        return;
    };
    writeln!(log, "thumb-bench start: {latest:?}").unwrap();
    let target = latest.max_scroll * fraction;
    let steps = (fling_ms / FRAME.as_millis() as u64).max(1);
    let position = latest.list_bounds.center();
    mark(&recorder, "fling");
    let fling_started = Instant::now();
    for _ in 0..steps {
        send(
            cx,
            window,
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position,
                delta: ScrollDelta::Pixels(point(px(0.), px(-target / steps as f32))),
                modifiers: Modifiers::default(),
                touch_phase: TouchPhase::Moved,
            }),
        );
        frame(cx, window, &recorder).await;
    }
    results.insert(
        "fling_ms".into(),
        json!(fling_started.elapsed().as_secs_f64() * 1_000.),
    );
    measure_move_and_click(cx, window, &recorder, "fling", click_row, &mut results).await;
    quiet_spell(cx, window, &recorder, 2, out, &mut flashes, &mut captures).await;

    // Snap to the end, as a click on the scroll bar does.
    cx.update(|cx| {
        if let Some(workspace) = super::document_window_entries(cx)
            .get(window)
            .and_then(|entry| entry.workspace.upgrade())
        {
            workspace.read(cx).thumbnail_bench_scroll_to(1.);
        }
    });
    measure_move_and_click(cx, window, &recorder, "jump", None, &mut results).await;
    quiet_spell(cx, window, &recorder, 3, out, &mut flashes, &mut captures).await;
    results.insert("canvas_flashes".into(), json!(flashes));
    results.insert("canvas_captures".into(), json!(captures));
    if sweep_pages > 0 {
        sweep(cx, window, &recorder, sweep_pages, &mut results).await;
    }

    running.set(false);
    heartbeat.await;
    let recorder = recorder.borrow();
    let late = &recorder.lateness;
    results.insert("main_thread_lateness".into(), summary(late));
    results.insert(
        "stalls_over_50ms".into(),
        json!(late.iter().filter(|ms| **ms > 50.).count()),
    );
    results.insert(
        "stalls_over_100ms".into(),
        json!(late.iter().filter(|ms| **ms > 100.).count()),
    );
    results.insert("frame_cpu".into(), summary(&recorder.frames));
    results.insert("canvas_regressions".into(), json!(recorder.regressions));
    results.insert("canvas_transitions".into(), json!(recorder.transitions));
    results.insert(
        "canvas_regression_log".into(),
        json!(recorder.regression_log),
    );
    results.insert(
        "loaded_thumbnails".into(),
        json!(
            recorder
                .latest
                .as_ref()
                .map(|probe| probe.loaded_thumbnails)
        ),
    );
    results.insert(
        "total_ms".into(),
        json!(started.elapsed().as_secs_f64() * 1_000.),
    );
    results.insert(
        "app_resident_kib".into(),
        json!(resident_kib(std::process::id())),
    );
    results.insert("app_footprint_kib".into(), json!(footprint_kib()));
    let results = Value::Object(results);
    let text = serde_json::to_string_pretty(&results).unwrap();
    std::fs::write(out.join("thumb-bench.json"), &text).unwrap();
    writeln!(log, "thumb-bench {text}").unwrap();
}
