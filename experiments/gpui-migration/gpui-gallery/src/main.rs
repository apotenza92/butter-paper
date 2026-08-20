use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, VecDeque},
    ffi::OsString,
    fs,
    path::PathBuf,
};

use anyhow::Result;
use gpui::{
    AnyElement, App, Application, AssetSource, Bounds, Context, Entity, IntoElement, KeyBinding,
    Menu, MenuItem, ObjectFit, Render, ScrollHandle, ScrollStrategy, SharedString, StyledImage,
    TitlebarOptions, UniformListScrollHandle, Window, WindowBounds, WindowOptions, actions, div,
    img, point, prelude::*, px, rgb, size, svg, uniform_list,
};

mod nova_theme;
mod pdf_document;
mod perf;
use nova_theme::*;
use pdf_document::{PdfDocument, RenderedViewport};
use serde_json::json;

actions!(
    butter_paper,
    [
        OpenPdf,
        NextPage,
        PreviousPage,
        ZoomIn,
        ZoomOut,
        ZoomReset,
        FitWidth,
        FitPage,
    ]
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ZoomPreset {
    Manual,
    FitWidth,
    FitPage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScrollMode {
    Continuous,
    SinglePage,
}

const MIN_ZOOM_PERCENT: f32 = 6.25;
const MAX_ZOOM_PERCENT: f32 = 6400.0;
const PAGE_LAYOUT_GAP: f32 = 24.0;
const FIT_ZOOM_STEP_PERCENT: f32 = 2.0;
const MAX_CONCURRENT_THUMBNAIL_JOBS: usize = 2;
const PERF_PAGE_SEQUENCE: &[usize] = &[935, 75, 674, 234, 842, 468, 11, 896, 309, 1];
const PERF_ZOOM_SEQUENCE: &[f32] = &[
    100.0, 200.0, 400.0, 800.0, 1600.0, 400.0, 100.0, 800.0, 200.0, 100.0, 1200.0, 100.0,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PerfScenarioKind {
    EmptyShell,
    OpenPdf,
    PageNavigation,
    Zoom,
}

impl PerfScenarioKind {
    fn from_environment() -> Option<Self> {
        match perf::scenario()? {
            "empty-shell" => Some(Self::EmptyShell),
            "open-pdf" | "open-first-page" => Some(Self::OpenPdf),
            "page-navigation" => Some(Self::PageNavigation),
            "zoom" => Some(Self::Zoom),
            other => {
                perf::emit(
                    "scenario-error",
                    perf::fields([("message", json!(format!("Unsupported scenario: {other}")))]),
                );
                None
            }
        }
    }
}

#[derive(Clone, Copy)]
struct PerfOperation {
    kind: &'static str,
    value: f64,
    started_ms: f64,
}

struct PerfScenario {
    kind: PerfScenarioKind,
    step_index: usize,
    first_frame_emitted: bool,
    frame_callback_scheduled: bool,
    initial_viewport_visible: bool,
    open_started_ms: Option<f64>,
    active_operation: Option<PerfOperation>,
    pending_visible_operation: Option<PerfOperation>,
    pending_initial_visible: bool,
    last_frame_ms: Option<f64>,
}

impl PerfScenario {
    fn new(kind: PerfScenarioKind) -> Self {
        Self {
            kind,
            step_index: 0,
            first_frame_emitted: false,
            frame_callback_scheduled: false,
            initial_viewport_visible: false,
            open_started_ms: None,
            active_operation: None,
            pending_visible_operation: None,
            pending_initial_visible: false,
            last_frame_ms: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RequestToken {
    document_id: u64,
    generation: u64,
}

fn is_current_request(current: Option<RequestToken>, completed: RequestToken) -> bool {
    current == Some(completed)
}

fn should_refresh_equal_zoom(scenario: Option<&PerfScenario>) -> bool {
    scenario.is_some_and(|scenario| {
        scenario
            .active_operation
            .is_some_and(|operation| operation.kind == "zoom")
    })
}

#[derive(Clone, Copy)]
struct ViewportRequest {
    token: RequestToken,
    page: usize,
    zoom_percent: f32,
    scroll_y: Option<f32>,
    started_ms: f64,
}

#[derive(Clone, Copy)]
struct ThumbnailRequest {
    token: RequestToken,
    page: usize,
}

fn clamp_zoom_percent(zoom_percent: f32) -> f32 {
    if zoom_percent <= MIN_ZOOM_PERCENT {
        return MIN_ZOOM_PERCENT;
    }
    if zoom_percent >= MAX_ZOOM_PERCENT {
        return MAX_ZOOM_PERCENT;
    }
    (zoom_percent * 10.0).round() / 10.0
}

fn initial_pdf_paths(args: impl IntoIterator<Item = OsString>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg.as_os_str() == "-ApplePersistenceIgnoreState" {
            let _ = args.next();
            continue;
        }
        paths.push(PathBuf::from(arg));
    }
    paths
}

fn quantize_fit_zoom_down(zoom_percent: f32) -> f32 {
    ((zoom_percent / FIT_ZOOM_STEP_PERCENT).floor() * FIT_ZOOM_STEP_PERCENT)
        .clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT)
}

fn resolve_fit_width_zoom(viewport_width: f32, page_width: f32) -> f32 {
    quantize_fit_zoom_down(
        viewport_width.max(1.0) / (page_width + PAGE_LAYOUT_GAP * 2.0).max(1.0) * 100.0,
    )
}

fn resolve_fit_page_zoom(
    viewport_width: f32,
    viewport_height: f32,
    page_width: f32,
    page_height: f32,
) -> f32 {
    let width_zoom = resolve_fit_width_zoom(viewport_width, page_width);
    let height_zoom =
        viewport_height.max(1.0) / (page_height + PAGE_LAYOUT_GAP * 2.0).max(1.0) * 100.0;
    quantize_fit_zoom_down(width_zoom.min(height_zoom))
}

struct Assets {
    base: PathBuf,
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        fs::read(self.base.join(path))
            .map(|data| Some(Cow::Owned(data)))
            .map_err(Into::into)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        fs::read_dir(self.base.join(path))
            .map(|entries| {
                entries
                    .filter_map(|entry| {
                        entry
                            .ok()
                            .and_then(|entry| entry.file_name().into_string().ok())
                            .map(SharedString::from)
                    })
                    .collect()
            })
            .map_err(Into::into)
    }
}

fn asset_base() -> PathBuf {
    if let Some(path) = std::env::var_os("BP_GPUI_ASSET_DIR") {
        return path.into();
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(macos_directory) = executable.parent()
        && macos_directory
            .file_name()
            .is_some_and(|name| name == "MacOS")
    {
        return macos_directory.join("../Resources/assets");
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

struct Gallery {
    active_tab: usize,
    capture_shell: bool,
    documents: Vec<PdfDocument>,
    active_document: Option<usize>,
    document_error: Option<String>,
    document_scroll: ScrollHandle,
    thumbnail_scroll: UniformListScrollHandle,
    zoom_percent: f32,
    zoom_preset: ZoomPreset,
    scroll_mode: ScrollMode,
    next_document_id: u64,
    next_request_generation: u64,
    latest_open_request: Option<u64>,
    pending_open_requests: HashSet<u64>,
    pending_viewport_requests: HashMap<u64, ViewportRequest>,
    active_viewport_jobs: HashSet<u64>,
    pending_thumbnail_requests: HashMap<(u64, usize), u64>,
    thumbnail_queue: VecDeque<ThumbnailRequest>,
    active_thumbnail_jobs: usize,
    thumbnail_failures: HashMap<(u64, usize), String>,
    initial_page: usize,
    diagnostics: bool,
    perf_scenario: Option<PerfScenario>,
}

impl Gallery {
    fn document(&self) -> Option<&PdfDocument> {
        self.active_document
            .and_then(|index| self.documents.get(index))
    }

    fn next_request(&mut self, document_id: u64) -> RequestToken {
        let token = RequestToken {
            document_id,
            generation: self.next_request_generation,
        };
        self.next_request_generation = self.next_request_generation.saturating_add(1);
        token
    }

    fn document_index_by_id(&self, document_id: u64) -> Option<usize> {
        self.documents
            .iter()
            .position(|document| document.id == document_id)
    }

    fn open_path(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let document_id = self.next_document_id;
        self.next_document_id = self.next_document_id.saturating_add(1);
        let token = self.next_request(document_id);
        self.latest_open_request = Some(token.generation);
        self.pending_open_requests.insert(token.generation);
        self.document_error = None;
        if let Some(scenario) = self.perf_scenario.as_mut()
            && scenario.open_started_ms.is_none()
        {
            let started_ms = perf::elapsed_ms();
            scenario.open_started_ms = Some(started_ms);
            perf::emit(
                "pdf-open-requested",
                perf::fields([("document_id", json!(document_id)), ("path", json!(path))]),
            );
        }
        cx.notify();

        let task = cx
            .background_executor()
            .spawn(async move { PdfDocument::open(document_id, path) });
        cx.spawn(async move |entity, cx| {
            let result = task.await;
            let _ = entity.update(cx, |this, cx| {
                this.finish_open(token, result, cx);
            });
        })
        .detach();
    }

    fn finish_open(
        &mut self,
        token: RequestToken,
        result: Result<PdfDocument, String>,
        cx: &mut Context<Self>,
    ) {
        if !self.pending_open_requests.remove(&token.generation) {
            return;
        }
        match result {
            Ok(document) => {
                if let Some(scenario) = self.perf_scenario.as_mut()
                    && let Some(started_ms) = scenario.open_started_ms
                {
                    perf::emit(
                        "pdf-open-completed",
                        perf::fields([
                            ("duration_ms", json!(perf::elapsed_ms() - started_ms)),
                            ("document_id", json!(document.id)),
                            ("pages", json!(document.page_count)),
                        ]),
                    );
                }
                let previously_active_id = self.document().map(|document| document.id);
                let insert_at = self
                    .documents
                    .partition_point(|existing| existing.id < document.id);
                self.documents.insert(insert_at, document);
                let should_activate = self.latest_open_request == Some(token.generation)
                    || self.active_document.is_none();
                if should_activate {
                    let index = self.document_index_by_id(token.document_id).unwrap();
                    self.active_document = Some(index);
                    self.document_error = None;
                    let page = self.initial_page;
                    let scroll_y = Some(if self.capture_shell { -255.0 } else { 0.0 });
                    self.request_viewport(token.document_id, page, self.zoom_percent, scroll_y, cx);
                } else {
                    self.active_document = previously_active_id
                        .and_then(|document_id| self.document_index_by_id(document_id));
                }
            }
            Err(error) => {
                if self.latest_open_request == Some(token.generation) {
                    self.document_error = Some(error);
                }
            }
        }
        if self.diagnostics {
            let titles = self
                .documents
                .iter()
                .map(|document| document.title.as_str())
                .collect::<Vec<_>>()
                .join(" | ");
            eprintln!(
                "BP_GPUI_DIAGNOSTICS tabs={} active={:?} pending_opens={} titles={titles}",
                self.documents.len(),
                self.active_document,
                self.pending_open_requests.len(),
            );
        }
        cx.notify();
    }

    fn select_document(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.documents.len() {
            return;
        }
        self.active_document = Some(index);
        self.document_error = None;
        let document_id = self.documents[index].id;
        let page = self.documents[index].current_page;
        self.document_scroll.set_offset(point(px(0.0), px(0.0)));
        if let Some(document) = self.document() {
            self.thumbnail_scroll.scroll_to_item(
                document.current_page.saturating_sub(1),
                ScrollStrategy::Center,
            );
        }
        self.request_viewport(document_id, page, self.zoom_percent, Some(0.0), cx);
    }

    fn close_document(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.documents.len() {
            return;
        }
        let was_active = self.active_document == Some(index);
        let document_id = self.documents[index].id;
        self.documents.remove(index);
        self.active_document = match self.active_document {
            None => None,
            Some(_) if self.documents.is_empty() => None,
            Some(active) if active > index => Some(active - 1),
            Some(active) if active == index => Some(index.min(self.documents.len() - 1)),
            Some(active) => Some(active),
        };
        self.document_error = None;
        self.pending_viewport_requests.remove(&document_id);
        self.pending_thumbnail_requests
            .retain(|(id, _), _| *id != document_id);
        self.thumbnail_queue
            .retain(|request| request.token.document_id != document_id);
        self.thumbnail_failures
            .retain(|(id, _), _| *id != document_id);
        self.document_scroll.set_offset(point(px(0.0), px(0.0)));
        if was_active {
            if let Some((document_id, page)) = self
                .document()
                .map(|document| (document.id, document.current_page))
            {
                self.request_viewport(document_id, page, self.zoom_percent, Some(0.0), cx);
            } else {
                cx.notify();
            }
        } else {
            cx.notify();
        }
    }

    fn open_pdf_dialog(&mut self, cx: &mut Context<Self>) {
        let picker = rfd::AsyncFileDialog::new()
            .add_filter("PDF documents", &["pdf"])
            .pick_file();
        cx.spawn(async move |entity, cx| {
            if let Some(file) = picker.await {
                let path = file.path().to_path_buf();
                let _ = entity.update(cx, |this, cx| this.open_path(path, cx));
            }
        })
        .detach();
    }

    fn set_page(&mut self, page: usize, cx: &mut Context<Self>) {
        let Some(document_id) = self.document().map(|document| document.id) else {
            return;
        };
        self.request_viewport(document_id, page, self.zoom_percent, Some(0.0), cx);
    }

    fn change_zoom(&mut self, delta: f32, cx: &mut Context<Self>) {
        let next_zoom = if delta.is_sign_positive() {
            self.zoom_percent * 1.1
        } else {
            self.zoom_percent / 1.1
        };
        self.set_zoom(next_zoom, ZoomPreset::Manual, cx);
    }

    fn set_zoom(&mut self, next_zoom: f32, preset: ZoomPreset, cx: &mut Context<Self>) {
        let next_zoom = clamp_zoom_percent(next_zoom);
        if (next_zoom - self.zoom_percent).abs() < f32::EPSILON {
            self.zoom_preset = preset;
            // A deterministic performance sequence can intentionally revisit
            // the current zoom (the first step is 100%). Re-request the
            // viewport for an active benchmark operation so that equal-value
            // steps still produce a completion event instead of waiting
            // forever for a request that was skipped as a no-op.
            let refresh_equal_zoom = should_refresh_equal_zoom(self.perf_scenario.as_ref());
            if refresh_equal_zoom {
                if let Some((document_id, page)) = self
                    .document()
                    .map(|document| (document.id, document.current_page))
                {
                    self.request_viewport(document_id, page, next_zoom, None, cx);
                }
            }
            cx.notify();
            return;
        }
        self.zoom_percent = next_zoom;
        self.zoom_preset = preset;
        if let Some(document) = self.document() {
            self.request_viewport(document.id, document.current_page, next_zoom, None, cx);
        } else {
            cx.notify();
        }
    }

    fn request_viewport(
        &mut self,
        document_id: u64,
        page: usize,
        zoom_percent: f32,
        scroll_y: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.document_index_by_id(document_id) else {
            return;
        };
        let page = self.documents[index].select_page(page);
        let token = self.next_request(document_id);
        self.pending_viewport_requests.insert(
            document_id,
            ViewportRequest {
                token,
                page,
                zoom_percent,
                scroll_y,
                started_ms: perf::elapsed_ms(),
            },
        );
        if self.active_document == Some(index) {
            self.document_error = None;
        }
        self.start_viewport_job(document_id, cx);
        cx.notify();
    }

    fn start_viewport_job(&mut self, document_id: u64, cx: &mut Context<Self>) {
        if self.active_viewport_jobs.contains(&document_id) {
            return;
        }
        let Some(request) = self.pending_viewport_requests.get(&document_id).copied() else {
            return;
        };
        let Some(index) = self.document_index_by_id(document_id) else {
            self.pending_viewport_requests.remove(&document_id);
            return;
        };
        let document = self.documents[index].clone();
        self.active_viewport_jobs.insert(document_id);
        let task = cx
            .background_executor()
            .spawn(async move { document.render_viewport(request.page, request.zoom_percent) });
        cx.spawn(async move |entity, cx| {
            let result = task.await;
            let _ = entity.update(cx, |this, cx| {
                this.finish_viewport(request, result, cx);
            });
        })
        .detach();
    }

    fn finish_viewport(
        &mut self,
        request: ViewportRequest,
        result: Result<RenderedViewport, String>,
        cx: &mut Context<Self>,
    ) {
        self.active_viewport_jobs.remove(&request.token.document_id);
        let current = self
            .pending_viewport_requests
            .get(&request.token.document_id)
            .map(|request| request.token);
        if is_current_request(current, request.token) {
            self.pending_viewport_requests
                .remove(&request.token.document_id);
            if let Some(index) = self.document_index_by_id(request.token.document_id) {
                match result {
                    Ok(rendered) => {
                        let operation = self
                            .perf_scenario
                            .as_mut()
                            .and_then(|scenario| scenario.active_operation.take());
                        let duration_ms = perf::elapsed_ms() - request.started_ms;
                        perf::emit(
                            if operation.is_some() {
                                "operation-raster-completed"
                            } else {
                                "viewport-raster-completed"
                            },
                            perf::fields([
                                ("duration_ms", json!(duration_ms)),
                                ("document_id", json!(request.token.document_id)),
                                ("page", json!(rendered.page)),
                                ("zoom_percent", json!(request.zoom_percent)),
                                ("pixel_width", json!(rendered.pixel_width)),
                                ("image_path", json!(rendered.image_path)),
                            ]),
                        );
                        self.documents[index].apply_rendered_viewport(rendered);
                        if let Some(scenario) = self.perf_scenario.as_mut() {
                            if let Some(operation) = operation {
                                scenario.pending_visible_operation = Some(operation);
                            } else if !scenario.initial_viewport_visible {
                                scenario.pending_initial_visible = true;
                            }
                        }
                        if self.active_document == Some(index) {
                            self.document_error = None;
                            if let Some(scroll_y) = request.scroll_y {
                                self.document_scroll
                                    .set_offset(point(px(0.0), px(scroll_y)));
                                self.thumbnail_scroll.scroll_to_item(
                                    request.page.saturating_sub(1),
                                    ScrollStrategy::Center,
                                );
                            }
                        }
                    }
                    Err(error) if self.active_document == Some(index) => {
                        self.document_error = Some(error);
                    }
                    Err(_) => {}
                }
            }
        }
        if self
            .pending_viewport_requests
            .contains_key(&request.token.document_id)
        {
            self.start_viewport_job(request.token.document_id, cx);
        }
        cx.notify();
    }

    fn start_next_perf_operation(&mut self, cx: &mut Context<Self>) {
        let Some((scenario_kind, step_index)) = self
            .perf_scenario
            .as_ref()
            .map(|scenario| (scenario.kind, scenario.step_index))
        else {
            return;
        };
        let next = match scenario_kind {
            PerfScenarioKind::PageNavigation => PERF_PAGE_SEQUENCE
                .get(step_index)
                .map(|page| ("page", *page as f64)),
            PerfScenarioKind::Zoom => PERF_ZOOM_SEQUENCE
                .get(step_index)
                .map(|zoom| ("zoom", f64::from(*zoom))),
            PerfScenarioKind::EmptyShell | PerfScenarioKind::OpenPdf => None,
        };
        let Some((kind, value)) = next else {
            self.complete_perf_scenario(cx);
            return;
        };
        let operation = PerfOperation {
            kind,
            value,
            started_ms: perf::elapsed_ms(),
        };
        if let Some(scenario) = self.perf_scenario.as_mut() {
            scenario.active_operation = Some(operation);
        }
        perf::emit(
            "operation-started",
            perf::fields([
                ("operation", json!(kind)),
                ("value", json!(value)),
                ("step", json!(step_index)),
            ]),
        );
        match kind {
            "page" => self.set_page(value as usize, cx),
            "zoom" => self.set_zoom(value as f32, ZoomPreset::Manual, cx),
            _ => unreachable!(),
        }
    }

    fn complete_perf_scenario(&mut self, cx: &mut Context<Self>) {
        perf::emit("scenario-complete", Default::default());
        cx.quit();
    }

    fn on_perf_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let now = perf::elapsed_ms();
        let Some(scenario) = self.perf_scenario.as_mut() else {
            return;
        };
        scenario.frame_callback_scheduled = false;
        if let Some(previous) = scenario.last_frame_ms.replace(now) {
            perf::emit(
                "frame",
                perf::fields([("interval_ms", json!(now - previous))]),
            );
        }
        if !scenario.first_frame_emitted {
            scenario.first_frame_emitted = true;
            perf::emit("first-frame", Default::default());
            if scenario.kind == PerfScenarioKind::EmptyShell {
                self.complete_perf_scenario(cx);
                return;
            }
        }

        let initial_visible = scenario.pending_initial_visible;
        let operation = scenario.pending_visible_operation.take();
        if initial_visible {
            scenario.pending_initial_visible = false;
            scenario.initial_viewport_visible = true;
        }
        if operation.is_some() {
            scenario.step_index += 1;
        }
        let scenario_kind = scenario.kind;

        if initial_visible {
            let duration_ms = self
                .perf_scenario
                .as_ref()
                .and_then(|scenario| scenario.open_started_ms)
                .map(|started_ms| now - started_ms);
            perf::emit(
                "viewport-visible",
                perf::fields([("duration_ms", json!(duration_ms))]),
            );
            if scenario_kind == PerfScenarioKind::OpenPdf {
                self.complete_perf_scenario(cx);
            } else {
                self.start_next_perf_operation(cx);
            }
        } else if let Some(operation) = operation {
            perf::emit(
                "operation-visible",
                perf::fields([
                    ("duration_ms", json!(now - operation.started_ms)),
                    ("operation", json!(operation.kind)),
                    ("value", json!(operation.value)),
                ]),
            );
            self.start_next_perf_operation(cx);
        }
        window.refresh();
        cx.notify();
    }

    fn schedule_perf_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scenario) = self.perf_scenario.as_mut() else {
            return;
        };
        if !scenario.frame_callback_scheduled {
            scenario.frame_callback_scheduled = true;
            cx.on_next_frame(window, |this, window, cx| {
                this.on_perf_frame(window, cx);
            });
        }
    }

    fn request_thumbnail(&mut self, document_id: u64, page: usize, cx: &mut Context<Self>) {
        let key = (document_id, page);
        if self.pending_thumbnail_requests.contains_key(&key)
            || self.thumbnail_failures.contains_key(&key)
        {
            return;
        }
        let token = self.next_request(document_id);
        self.pending_thumbnail_requests
            .insert(key, token.generation);
        self.thumbnail_queue
            .push_back(ThumbnailRequest { token, page });
        self.pump_thumbnail_queue(cx);
    }

    fn pump_thumbnail_queue(&mut self, cx: &mut Context<Self>) {
        while self.active_thumbnail_jobs < MAX_CONCURRENT_THUMBNAIL_JOBS {
            let Some(request) = self.thumbnail_queue.pop_front() else {
                break;
            };
            let key = (request.token.document_id, request.page);
            if self.pending_thumbnail_requests.get(&key).copied() != Some(request.token.generation)
            {
                continue;
            }
            let Some(index) = self.document_index_by_id(request.token.document_id) else {
                self.pending_thumbnail_requests.remove(&key);
                continue;
            };
            let document = self.documents[index].clone();
            self.active_thumbnail_jobs += 1;
            let task = cx
                .background_executor()
                .spawn(async move { document.render_thumbnail(request.page) });
            cx.spawn(async move |entity, cx| {
                let result = task.await;
                let _ = entity.update(cx, |this, cx| {
                    this.finish_thumbnail(request, result, cx);
                });
            })
            .detach();
        }
    }

    fn finish_thumbnail(
        &mut self,
        request: ThumbnailRequest,
        result: Result<PathBuf, String>,
        cx: &mut Context<Self>,
    ) {
        self.active_thumbnail_jobs = self.active_thumbnail_jobs.saturating_sub(1);
        let key = (request.token.document_id, request.page);
        if self.pending_thumbnail_requests.get(&key).copied() == Some(request.token.generation) {
            self.pending_thumbnail_requests.remove(&key);
            if let Err(error) = result {
                self.thumbnail_failures.insert(key, error.clone());
                if self
                    .document()
                    .is_some_and(|document| document.id == request.token.document_id)
                {
                    self.document_error = Some(format!(
                        "Could not render thumbnail for page {}: {error}",
                        request.page
                    ));
                }
            }
        }
        self.pump_thumbnail_queue(cx);
        cx.notify();
    }

    fn fit_width(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.document() else {
            return;
        };
        let available_width =
            f32::from(window.viewport_size().width) - RAIL_WIDTH - SIDEBAR_WIDTH - RIGHT_RAIL_WIDTH;
        let zoom = resolve_fit_width_zoom(available_width, document.page_width);
        self.set_zoom(zoom, ZoomPreset::FitWidth, cx);
    }

    fn fit_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.document() else {
            return;
        };
        let available_width =
            f32::from(window.viewport_size().width) - RAIL_WIDTH - SIDEBAR_WIDTH - RIGHT_RAIL_WIDTH;
        let available_height = f32::from(window.viewport_size().height)
            - WINDOW_TITLE_BAR_HEIGHT
            - MENU_BAR_HEIGHT
            - DOCUMENT_TAB_BAR_HEIGHT
            - PRIMARY_BAND_HEIGHT
            - 32.0;
        self.set_zoom(
            resolve_fit_page_zoom(
                available_width,
                available_height,
                document.page_width,
                document.page_height,
            ),
            ZoomPreset::FitPage,
            cx,
        );
    }

    fn set_scroll_mode(&mut self, scroll_mode: ScrollMode, cx: &mut Context<Self>) {
        self.scroll_mode = scroll_mode;
        if scroll_mode == ScrollMode::SinglePage {
            self.document_scroll.set_offset(point(px(0.0), px(0.0)));
        }
        cx.notify();
    }

    fn open_pdf_action(&mut self, _: &OpenPdf, _: &mut Window, cx: &mut Context<Self>) {
        self.open_pdf_dialog(cx);
    }

    fn next_page_action(&mut self, _: &NextPage, _: &mut Window, cx: &mut Context<Self>) {
        let page = self
            .document()
            .map(|document| (document.current_page + 1).min(document.page_count));
        if let Some(page) = page {
            self.set_page(page, cx);
        }
    }

    fn previous_page_action(&mut self, _: &PreviousPage, _: &mut Window, cx: &mut Context<Self>) {
        let page = self
            .document()
            .map(|document| document.current_page.saturating_sub(1).max(1));
        if let Some(page) = page {
            self.set_page(page, cx);
        }
    }

    fn zoom_in_action(&mut self, _: &ZoomIn, _: &mut Window, cx: &mut Context<Self>) {
        self.change_zoom(1.0, cx);
    }

    fn zoom_out_action(&mut self, _: &ZoomOut, _: &mut Window, cx: &mut Context<Self>) {
        self.change_zoom(-1.0, cx);
    }

    fn zoom_reset_action(&mut self, _: &ZoomReset, _: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(100.0, ZoomPreset::Manual, cx);
    }

    fn fit_width_action(&mut self, _: &FitWidth, window: &mut Window, cx: &mut Context<Self>) {
        self.fit_width(window, cx);
    }

    fn fit_page_action(&mut self, _: &FitPage, window: &mut Window, cx: &mut Context<Self>) {
        self.fit_page(window, cx);
    }

    fn control(label: impl Into<SharedString>, selected: bool) -> AnyElement {
        div()
            .flex()
            .h(px(CONTROL_HEIGHT))
            .px_3()
            .items_center()
            .justify_center()
            .rounded(px(CONTROL_RADIUS))
            .border_1()
            .border_color(rgb(if selected { FOCUS } else { BORDER }))
            .bg(rgb(if selected { ACCENT } else { SURFACE }))
            .text_color(rgb(TEXT))
            .child(label.into())
            .into_any_element()
    }

    fn icon(name: &'static str, size: f32) -> AnyElement {
        svg()
            .path(SharedString::from(format!("icons/{name}.svg")))
            .size(px(size))
            .text_color(rgb(TEXT))
            .into_any_element()
    }

    fn rail_icon(name: &'static str, size: f32) -> AnyElement {
        svg()
            .path(SharedString::from(format!("icons/rail/{name}.svg")))
            .size(px(size))
            .text_color(rgb(TEXT))
            .into_any_element()
    }

    fn icon_button(name: &'static str, selected: bool) -> AnyElement {
        div()
            .flex()
            .size(px(RAIL_BUTTON_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(CONTROL_RADIUS))
            .border_1()
            .border_color(rgb(if selected { FOCUS } else { BORDER }))
            .bg(rgb(if selected { ACCENT } else { SURFACE }))
            .child(Self::icon(name, CONTROL_ICON_SIZE))
            .into_any_element()
    }

    fn rail_icon_button(name: &'static str, selected: bool) -> AnyElement {
        div()
            .flex()
            .size(px(RAIL_BUTTON_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(CONTROL_RADIUS))
            .border_1()
            .border_color(rgb(if selected { FOCUS } else { BORDER }))
            .bg(rgb(if selected { ACCENT } else { SURFACE }))
            .child(Self::rail_icon(name, CONTROL_ICON_SIZE))
            .into_any_element()
    }

    fn joined_icon_segment(name: &'static str, selected: bool, separated: bool) -> AnyElement {
        div()
            .flex()
            .size(px(RAIL_BUTTON_SIZE))
            .items_center()
            .justify_center()
            .when(separated, |element| {
                element.border_l_1().border_color(rgb(BORDER))
            })
            .bg(rgb(if selected { ACCENT } else { SURFACE }))
            .child(Self::icon(name, CONTROL_ICON_SIZE))
            .into_any_element()
    }

    fn joined_zoom_trigger(zoom_percent: f32) -> AnyElement {
        div()
            .flex()
            .h(px(RAIL_BUTTON_SIZE))
            .w(px(72.0))
            .items_center()
            .justify_center()
            .gap_1()
            .border_l_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(format!("{zoom_percent:.0}%"))
            .child(Self::icon("chevron-down", 13.0))
            .into_any_element()
    }

    fn section_title(title: &'static str, note: &'static str) -> AnyElement {
        div()
            .flex()
            .items_end()
            .justify_between()
            .child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(title))
            .child(div().text_color(rgb(MUTED)).text_xs().child(note))
            .into_any_element()
    }

    fn comparison_card(&self, title: &'static str, butter_paper_skin: bool) -> AnyElement {
        // GPUI does not provide a default widget density or visual theme. Keep
        // the reviewed Butter Paper metrics in the thinnest possible wrapper.
        let radius = px(CONTROL_RADIUS);
        let gap = px(RAIL_BUTTON_GAP);
        let height = px(CONTROL_HEIGHT);
        let focus = FOCUS;

        let base = |text: &'static str| {
            div()
                .flex()
                .h(height)
                .px_3()
                .items_center()
                .justify_center()
                .rounded(radius)
                .border_1()
                .border_color(rgb(BORDER))
                .bg(rgb(SURFACE))
                .child(text)
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .p_4()
            .rounded(px(BASE_RADIUS))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .flex()
            .flex_col()
            .gap_4()
            .child(Self::section_title(
                title,
                if butter_paper_skin {
                    "Butter Paper skin on GPUI primitives"
                } else {
                    "div · focus · actions · input handler"
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(gap)
                    .child(base("Button"))
                    .child(
                        div()
                            .flex()
                            .size(height)
                            .items_center()
                            .justify_center()
                            .rounded(radius)
                            .border_1()
                            .border_color(rgb(focus))
                            .bg(rgb(ACCENT))
                            .child(Self::icon("command", CONTROL_ICON_SIZE)),
                    )
                    .child(
                        div()
                            .flex()
                            .h(height)
                            .items_center()
                            .rounded(radius)
                            .border_1()
                            .border_color(rgb(BORDER))
                            .overflow_hidden()
                            .child(div().h_full().px_3().flex().items_center().child("Open"))
                            .child(
                                div()
                                    .h_full()
                                    .px_2()
                                    .flex()
                                    .items_center()
                                    .border_l_1()
                                    .border_color(rgb(BORDER))
                                    .child(Self::icon("chevron-down", 14.0)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(gap)
                    .items_center()
                    .child(base("Input value").w(px(140.0)).justify_start())
                    .child(base("Select  Solid"))
                    .child(base("✓  Locked")),
            )
            .child(
                div()
                    .flex()
                    .gap(gap)
                    .items_center()
                    .child(
                        div()
                            .w(px(140.0))
                            .h(px(4.0))
                            .rounded_full()
                            .bg(rgb(BORDER))
                            .child(div().w(px(92.0)).h_full().rounded_full().bg(rgb(TEXT))),
                    )
                    .child(div().text_color(rgb(MUTED)).child("64%"))
                    .child(base("Tooltip target")),
            )
            .child(
                div()
                    .flex()
                    .gap_1()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .border_b_2()
                            .border_color(rgb(focus))
                            .child("Document A"),
                    )
                    .child(
                        div()
                            .px_3()
                            .py_2()
                            .text_color(rgb(MUTED))
                            .child("Document B"),
                    ),
            )
            .into_any_element()
    }

    fn window_title_bar(&self) -> AnyElement {
        let title = self
            .document()
            .map(|document| document.title.as_str())
            .unwrap_or("Butter Paper");
        div()
            .h(px(WINDOW_TITLE_BAR_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .text_xs()
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(MUTED))
            .child(format!("{title} — Butter Paper Dev · GPUI spike"))
            .into_any_element()
    }

    fn app_menu_bar() -> AnyElement {
        div()
            .h(px(MENU_BAR_HEIGHT))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .children(["Butter Paper", "File", "Edit", "View"].map(|label| {
                div()
                    .h(px(24.0))
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(4.0))
                    .child(label)
            }))
            .into_any_element()
    }

    fn document_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let entity = cx.entity();
        let tabs = self
            .documents
            .iter()
            .enumerate()
            .map(|(index, document)| {
                let selected = self.active_document == Some(index);
                let label = document
                    .title
                    .strip_suffix(".pdf")
                    .unwrap_or(&document.title)
                    .to_owned();
                let width = (label.chars().count() as f32 * 7.0 + 44.0).clamp(96.0, 450.0);
                let select_entity = entity.clone();
                let close_entity = entity.clone();
                div()
                    .id(SharedString::from(format!("document-tab-{index}")))
                    .h(px(TAB_HEIGHT))
                    .w(px(width))
                    .flex_none()
                    .px_2()
                    .flex()
                    .items_center()
                    .rounded(px(CONTROL_RADIUS))
                    .bg(rgb(if selected { ACCENT } else { SURFACE }))
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        select_entity.update(cx, |this, cx| this.select_document(index, cx));
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(label),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("close-document-{index}")))
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                close_entity.update(cx, |this, cx| this.close_document(index, cx));
                            })
                            .child(Self::icon("x", 14.0)),
                    )
            })
            .collect::<Vec<_>>();
        let document_actions = div()
            .h(px(TAB_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("open-pdf")
                    .cursor_pointer()
                    .on_click(cx.listener(|this, _, _, cx| this.open_pdf_dialog(cx)))
                    .child(Self::icon_button("plus", false)),
            )
            .child(
                div()
                    .flex()
                    .overflow_hidden()
                    .rounded(px(CONTROL_RADIUS))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(Self::joined_icon_segment("file-plus", false, false))
                    .child(Self::joined_icon_segment("chevron-down", false, true)),
            );
        div()
            .h(px(DOCUMENT_TAB_BAR_HEIGHT))
            .flex_none()
            .p_2()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .id("document-tab-scroll")
                    .flex_1()
                    .min_w(px(0.0))
                    .h(px(TAB_HEIGHT))
                    .flex()
                    .items_center()
                    .gap_2()
                    .overflow_x_scroll()
                    .children(tabs)
                    .when(!self.documents.is_empty(), |element| {
                        element.child(div().w(px(1.0)).h(px(20.0)).flex_none().bg(rgb(BORDER)))
                    })
                    .child(document_actions),
            )
            .into_any_element()
    }

    fn left_rail() -> AnyElement {
        div()
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .p_2()
            .flex()
            .flex_col()
            .items_center()
            .border_r_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(Self::rail_icon_button("files", true))
            .into_any_element()
    }

    fn thumbnail_card(
        page: usize,
        selected: bool,
        image_path: Option<std::path::PathBuf>,
        loading: bool,
        failed: bool,
        entity: Entity<Self>,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("thumbnail-{page}")))
            .cursor_pointer()
            .on_click(move |_, _, cx| {
                entity.update(cx, |this, cx| this.set_page(page, cx));
            })
            .h(px(195.0))
            .mr_4()
            .p_2()
            .rounded(px(BASE_RADIUS))
            .border_1()
            .border_color(rgb(if selected { 0x60a5fa } else { BORDER }))
            .bg(rgb(if selected { ACCENT } else { SURFACE }))
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .items_center()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(format!("Page {page}"))
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_color(rgb(MUTED))
                            .child(Self::icon("scan-line", 14.0))
                            .child(Self::icon("rotate-ccw", 14.0))
                            .child(Self::icon("rotate-cw", 14.0)),
                    ),
            )
            .child(
                div()
                    .mt_2()
                    .mx_auto()
                    .w(px(114.0))
                    .h(px(132.0))
                    .bg(rgb(PAGE))
                    .border_1()
                    .border_color(rgb(0xbdbdbd))
                    .when_some(image_path, |element, image_path| {
                        element.child(img(image_path).size_full().object_fit(ObjectFit::Contain))
                    })
                    .when(loading, |element| {
                        element
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(div().text_xs().text_color(rgb(MUTED)).child("Rendering…"))
                    })
                    .when(failed, |element| {
                        element.flex().items_center().justify_center().child(
                            div()
                                .text_xs()
                                .text_color(rgb(0x991b1b))
                                .child("Unavailable"),
                        )
                    }),
            )
            .into_any_element()
    }

    fn thumbnail_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let page_count = self
            .document()
            .map(|document| document.page_count)
            .unwrap_or(0);
        let entity = cx.entity();
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .h(px(PRIMARY_BAND_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("Page Thumbnails"),
            )
            .child(
                uniform_list(
                    "page-thumbnails",
                    page_count,
                    cx.processor(move |this, range: std::ops::Range<usize>, _, cx| {
                        let mut items = Vec::with_capacity(range.end - range.start);
                        for index in range {
                            let page = index + 1;
                            let document_state = this.document().map(|document| {
                                (
                                    document.id,
                                    document.current_page == page,
                                    document.thumbnail_image(page),
                                )
                            });
                            let (selected, image_path, loading, failed) =
                                if let Some((document_id, selected, image_path)) = document_state {
                                    let key = (document_id, page);
                                    let failed = this.thumbnail_failures.contains_key(&key);
                                    if image_path.is_none()
                                        && !this.pending_thumbnail_requests.contains_key(&key)
                                        && !failed
                                    {
                                        this.request_thumbnail(document_id, page, cx);
                                    }
                                    (
                                        selected,
                                        image_path,
                                        this.pending_thumbnail_requests.contains_key(&key),
                                        failed,
                                    )
                                } else {
                                    (false, None, false, false)
                                };
                            items.push(div().h(px(203.0)).pt_2().pl_2().child(
                                Self::thumbnail_card(
                                    page,
                                    selected,
                                    image_path,
                                    loading,
                                    failed,
                                    entity.clone(),
                                ),
                            ));
                        }
                        items
                    }),
                )
                .flex_1()
                .min_h(px(0.0))
                .track_scroll(self.thumbnail_scroll.clone()),
            )
            .into_any_element()
    }

    fn viewer_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let zoom_out_enabled = self.document().is_some() && self.zoom_percent > MIN_ZOOM_PERCENT;
        let zoom_in_enabled = self.document().is_some() && self.zoom_percent < MAX_ZOOM_PERCENT;
        div()
            .h(px(PRIMARY_BAND_HEIGHT))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .justify_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .flex()
                    .overflow_hidden()
                    .rounded(px(CONTROL_RADIUS))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .id("zoom-out")
                            .when(zoom_out_enabled, |element| {
                                element.cursor_pointer().on_click(
                                    cx.listener(|this, _, _, cx| this.change_zoom(-10.0, cx)),
                                )
                            })
                            .opacity(if zoom_out_enabled { 1.0 } else { 0.4 })
                            .child(Self::joined_icon_segment("zoom-out", false, false)),
                    )
                    .child(
                        div()
                            .id("zoom-in")
                            .when(zoom_in_enabled, |element| {
                                element.cursor_pointer().on_click(
                                    cx.listener(|this, _, _, cx| this.change_zoom(10.0, cx)),
                                )
                            })
                            .opacity(if zoom_in_enabled { 1.0 } else { 0.4 })
                            .child(Self::joined_icon_segment("zoom-in", false, true)),
                    )
                    .child(Self::joined_zoom_trigger(self.zoom_percent)),
            )
            .child(
                div()
                    .flex()
                    .overflow_hidden()
                    .rounded(px(CONTROL_RADIUS))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .id("fit-width")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| this.fit_width(window, cx)))
                            .child(Self::joined_icon_segment(
                                "move-horizontal",
                                self.zoom_preset == ZoomPreset::FitWidth,
                                false,
                            )),
                    )
                    .child(
                        div()
                            .id("fit-page")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| this.fit_page(window, cx)))
                            .child(Self::joined_icon_segment(
                                "expand",
                                self.zoom_preset == ZoomPreset::FitPage,
                                true,
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .overflow_hidden()
                    .rounded(px(CONTROL_RADIUS))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .id("continuous-view")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_scroll_mode(ScrollMode::Continuous, cx)
                            }))
                            .child(Self::joined_icon_segment(
                                "continuous",
                                self.scroll_mode == ScrollMode::Continuous,
                                false,
                            )),
                    )
                    .child(Self::joined_icon_segment("chevron-down", false, true)),
            )
            .child(
                div()
                    .flex()
                    .overflow_hidden()
                    .rounded(px(CONTROL_RADIUS))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .child(
                        div()
                            .id("single-page-view")
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_scroll_mode(ScrollMode::SinglePage, cx)
                            }))
                            .child(Self::joined_icon_segment(
                                "rectangle-vertical",
                                self.scroll_mode == ScrollMode::SinglePage,
                                false,
                            )),
                    )
                    .child(Self::joined_icon_segment("chevron-down", false, true)),
            )
            .into_any_element()
    }

    fn document_viewport(&self) -> AnyElement {
        let image_path = self.document().and_then(PdfDocument::viewport_image);
        let page_size = self.document().map(|document| {
            (
                document.page_width * self.zoom_percent / 100.0,
                document.page_height * self.zoom_percent / 100.0,
            )
        });
        let (page_width, page_height) = page_size.unwrap_or((655.0, 758.0));
        let empty = self.document().is_none();
        let opening = !self.pending_open_requests.is_empty();
        let viewport_loading = self
            .document()
            .is_some_and(|document| self.pending_viewport_requests.contains_key(&document.id));
        let loading_label = self.document().map(|document| {
            format!(
                "Rendering page {} at {:.0}%…",
                document.current_page, self.zoom_percent
            )
        });
        let error = self.document_error.clone();
        div()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .relative()
            .bg(rgb(VIEWPORT))
            .child(
                div()
                    .id("document-scroll")
                    .size_full()
                    .overflow_scroll()
                    .track_scroll(&self.document_scroll)
                    .flex()
                    .items_start()
                    .justify_center()
                    .child(
                        div()
                            .w(px(page_width))
                            .h(px(page_height))
                            .flex_none()
                            .bg(rgb(PAGE))
                            .border_2()
                            .border_color(rgb(0x60a5fa))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when_some(image_path, |element, image_path| {
                                element.child(
                                    img(image_path).size_full().object_fit(ObjectFit::Contain),
                                )
                            })
                            .when(empty, |element| {
                                element.child(div().text_color(rgb(MUTED)).child(if opening {
                                    "Opening PDF…"
                                } else {
                                    "Open a PDF with the + button"
                                }))
                            }),
                    ),
            )
            .when(viewport_loading, |element| {
                element.child(
                    div()
                        .absolute()
                        .top_3()
                        .right_3()
                        .px_3()
                        .py_2()
                        .rounded(px(CONTROL_RADIUS))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .bg(rgb(SURFACE))
                        .text_color(rgb(MUTED))
                        .child(loading_label.unwrap_or_else(|| "Rendering…".into())),
                )
            })
            .when_some(error, |element, error| {
                element.child(
                    div()
                        .absolute()
                        .top_3()
                        .left_3()
                        .right_3()
                        .p_3()
                        .rounded(px(CONTROL_RADIUS))
                        .border_1()
                        .border_color(rgb(0xfca5a5))
                        .bg(rgb(0xfef2f2))
                        .text_color(rgb(0x991b1b))
                        .child(error),
                )
            })
            .child(
                div()
                    .absolute()
                    .bottom_1()
                    .left_4()
                    .right_4()
                    .h(px(7.0))
                    .rounded_full()
                    .bg(rgb(0xd4d4d4))
                    .child(div().w(px(445.0)).h_full().rounded_full().bg(rgb(0xa3a3a3))),
            )
            .into_any_element()
    }

    fn right_rail_group(title: &'static str, icons: &'static [&'static str]) -> AnyElement {
        div()
            .w_full()
            .py_3()
            .px_2()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(title),
            )
            .child(
                div()
                    .w(px(72.0))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .gap_2()
                    .children(
                        icons
                            .iter()
                            .map(|icon| Self::rail_icon_button(*icon, false)),
                    ),
            )
            .into_any_element()
    }

    fn right_rail() -> AnyElement {
        div()
            .w(px(RIGHT_RAIL_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .border_l_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .child(
                div()
                    .h(px(PRIMARY_BAND_HEIGHT))
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(BORDER))
                    .child(Self::icon_button("sliders-horizontal", false))
                    .child(Self::icon_button("scan-line", false)),
            )
            .child(Self::right_rail_group(
                "General",
                &["mouse-pointer-2", "hand"],
            ))
            .child(Self::right_rail_group(
                "Markup",
                &[
                    "type",
                    "arrow-right",
                    "highlighter",
                    "cloud",
                    "message-square",
                    "shield-x",
                    "pen-line",
                    "image",
                    "scan-search",
                ],
            ))
            .child(Self::right_rail_group(
                "Draw",
                &[
                    "square",
                    "circle",
                    "minus",
                    "waypoints",
                    "pen-line",
                    "spline",
                    "pentagon",
                    "ruler",
                ],
            ))
            .child(Self::right_rail_group(
                "Measure",
                &["ruler", "ruler-dimension-line", "route", "chart-area"],
            ))
            .into_any_element()
    }

    fn shell_preview(&self, fill_window: bool, cx: &mut Context<Self>) -> AnyElement {
        let shell = div()
            .id("butter-paper-shell")
            .on_action(cx.listener(Self::open_pdf_action))
            .on_action(cx.listener(Self::next_page_action))
            .on_action(cx.listener(Self::previous_page_action))
            .on_action(cx.listener(Self::zoom_in_action))
            .on_action(cx.listener(Self::zoom_out_action))
            .on_action(cx.listener(Self::zoom_reset_action))
            .on_action(cx.listener(Self::fit_width_action))
            .on_action(cx.listener(Self::fit_page_action))
            .rounded(px(BASE_RADIUS))
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(SURFACE))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.window_title_bar())
            .child(Self::app_menu_bar())
            .child(self.document_tab_bar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .child(Self::left_rail())
                    .child(self.thumbnail_sidebar(cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .child(self.viewer_toolbar(cx))
                            .child(self.document_viewport()),
                    )
                    .child(Self::right_rail()),
            );

        if fill_window {
            shell.size_full().rounded(px(0.0)).into_any_element()
        } else {
            shell.h(px(360.0)).into_any_element()
        }
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.schedule_perf_frame(window, cx);
        if self.capture_shell {
            return div()
                .size_full()
                .bg(rgb(BG))
                .text_color(rgb(TEXT))
                .text_size(px(BODY_FONT_SIZE))
                .child(self.shell_preview(true, cx))
                .into_any_element();
        }

        let active_tab = self.active_tab;
        div()
            .size_full()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(BODY_FONT_SIZE))
            .p_5()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .child(
                        div()
                            .child(div().text_2xl().font_weight(gpui::FontWeight::BOLD).child("Butter Paper GPUI gallery"))
                            .child(div().mt_1().text_color(rgb(MUTED)).child("Pinned GPUI 0.2.2 · default-first primitives with current Nova metrics")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .children(["Primitives", "Compound", "Shell"].into_iter().enumerate().map(|(index, label)| {
                        div()
                            .id(SharedString::from(format!("tab-{index}")))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.active_tab = index;
                                cx.notify();
                            }))
                            .child(Self::control(label, active_tab == index))
                    })),
            )
            .child(
                if active_tab == 2 {
                    self.shell_preview(false, cx)
                } else {
                    div()
                        .flex()
                        .gap_4()
                        .child(self.comparison_card("GPUI foundation", false))
                        .child(self.comparison_card("Butter Paper wrapper", true))
                        .into_any_element()
                },
            )
            .child(
                div()
                    .text_color(rgb(MUTED))
                    .text_xs()
                    .child("Direct GPUI: div/flex/grid, focus, actions, native app menus, anchored/deferred layers, input plumbing, uniform lists, images/surfaces and canvas. Thin Butter Paper wrappers: ordinary controls and Nova styling. Domain UI: document viewport and annotation behavior."),
            )
            .into_any_element()
    }
}

fn main() {
    perf::init();
    let perf_scenario_kind = PerfScenarioKind::from_environment();
    let initial_pdfs = initial_pdf_paths(std::env::args_os().skip(1));
    Application::new()
        .with_assets(Assets { base: asset_base() })
        .run(move |cx: &mut App| {
            cx.bind_keys([
                KeyBinding::new("cmd-o", OpenPdf, None),
                KeyBinding::new("ctrl-o", OpenPdf, None),
                KeyBinding::new("pagedown", NextPage, None),
                KeyBinding::new("pageup", PreviousPage, None),
                KeyBinding::new("cmd-=", ZoomIn, None),
                KeyBinding::new("cmd--", ZoomOut, None),
                KeyBinding::new("cmd-0", ZoomReset, None),
                KeyBinding::new("ctrl-=", ZoomIn, None),
                KeyBinding::new("ctrl--", ZoomOut, None),
                KeyBinding::new("ctrl-0", ZoomReset, None),
            ]);
            cx.set_menus(vec![
                Menu {
                    name: "File".into(),
                    items: vec![MenuItem::action("Open…", OpenPdf)],
                },
                Menu {
                    name: "View".into(),
                    items: vec![
                        MenuItem::action("Zoom In", ZoomIn),
                        MenuItem::action("Zoom Out", ZoomOut),
                        MenuItem::action("Actual Size", ZoomReset),
                        MenuItem::separator(),
                        MenuItem::action("Fit Width", FitWidth),
                        MenuItem::action("Fit Page", FitPage),
                    ],
                },
            ]);
            let capture_shell =
                std::env::var_os("BP_GPUI_CAPTURE_SHELL").is_some() || perf_scenario_kind.is_some();
            let window_size = if capture_shell {
                // The Electron evidence was captured at 1200×800 logical pixels
                // and exported at 1152×768. Use the same 0.96 export scale.
                size(px(1200.0), px(800.0))
            } else {
                size(px(1120.0), px(760.0))
            };
            let bounds = Bounds::centered(None, window_size, cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: (!capture_shell).then(|| "Butter Paper GPUI gallery".into()),
                        appears_transparent: capture_shell,
                        traffic_light_position: capture_shell.then(|| point(px(10.0), px(16.0))),
                    }),
                    ..Default::default()
                },
                move |_, cx| {
                    perf::emit("window-created", Default::default());
                    let initial_pdfs = initial_pdfs.clone();
                    cx.new(move |cx| {
                        let zoom_percent = std::env::var("BP_GPUI_ZOOM")
                            .ok()
                            .and_then(|value| value.parse::<f32>().ok())
                            .unwrap_or(if perf_scenario_kind.is_some() {
                                100.0
                            } else {
                                194.0
                            });
                        let zoom_percent = clamp_zoom_percent(zoom_percent);
                        let initial_page = std::env::var("BP_GPUI_INITIAL_PAGE")
                            .ok()
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(1);
                        let mut gallery = Gallery {
                            active_tab: if capture_shell { 2 } else { 0 },
                            capture_shell,
                            documents: Vec::new(),
                            active_document: None,
                            document_error: None,
                            document_scroll: ScrollHandle::new(),
                            thumbnail_scroll: UniformListScrollHandle::new(),
                            zoom_percent,
                            zoom_preset: ZoomPreset::Manual,
                            scroll_mode: ScrollMode::Continuous,
                            next_document_id: 1,
                            next_request_generation: 1,
                            latest_open_request: None,
                            pending_open_requests: HashSet::new(),
                            pending_viewport_requests: HashMap::new(),
                            active_viewport_jobs: HashSet::new(),
                            pending_thumbnail_requests: HashMap::new(),
                            thumbnail_queue: VecDeque::new(),
                            active_thumbnail_jobs: 0,
                            thumbnail_failures: HashMap::new(),
                            initial_page,
                            diagnostics: std::env::var_os("BP_GPUI_DIAGNOSTICS").is_some(),
                            perf_scenario: perf_scenario_kind.map(PerfScenario::new),
                        };
                        if perf_scenario_kind != Some(PerfScenarioKind::EmptyShell) {
                            for path in initial_pdfs {
                                gallery.open_path(path, cx);
                            }
                        }
                        gallery
                    })
                },
            )
            .unwrap();
            cx.activate(true);
        });
}

#[cfg(test)]
mod shell_tests {
    use super::*;

    #[test]
    fn filters_appkit_persistence_arguments_from_document_paths() {
        let paths = initial_pdf_paths([
            OsString::from("-ApplePersistenceIgnoreState"),
            OsString::from("YES"),
            OsString::from("/tmp/Hibbeler.pdf"),
        ]);
        assert_eq!(paths, vec![PathBuf::from("/tmp/Hibbeler.pdf")]);
    }

    #[test]
    fn matches_the_electron_fit_width_quantization_contract() {
        let zoom = resolve_fit_width_zoom(919.6, 952.0);
        assert_eq!(zoom, 90.0);
        assert!((952.0 + PAGE_LAYOUT_GAP * 2.0) * zoom / 100.0 <= 919.6);
    }

    #[test]
    fn fit_page_uses_the_more_constrained_axis() {
        assert_eq!(resolve_fit_page_zoom(1000.0, 500.0, 500.0, 1000.0), 46.0);
    }

    #[test]
    fn manual_zoom_uses_the_current_product_limits() {
        assert_eq!(clamp_zoom_percent(0.0), MIN_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(9000.0), MAX_ZOOM_PERCENT);
        assert_eq!(clamp_zoom_percent(194.04), 194.0);
    }

    #[test]
    fn stale_or_cross_document_results_are_rejected() {
        let current = RequestToken {
            document_id: 7,
            generation: 12,
        };
        assert!(is_current_request(Some(current), current));
        assert!(!is_current_request(
            Some(current),
            RequestToken {
                document_id: 7,
                generation: 11,
            }
        ));
        assert!(!is_current_request(
            Some(current),
            RequestToken {
                document_id: 8,
                generation: 12,
            }
        ));
        assert!(!is_current_request(None, current));
    }

    #[test]
    fn equal_zoom_refreshes_only_for_an_active_zoom_operation() {
        let mut scenario = PerfScenario::new(PerfScenarioKind::Zoom);
        assert!(!should_refresh_equal_zoom(Some(&scenario)));
        scenario.active_operation = Some(PerfOperation {
            kind: "zoom",
            value: 100.0,
            started_ms: 0.0,
        });
        assert!(should_refresh_equal_zoom(Some(&scenario)));
        scenario.active_operation = Some(PerfOperation {
            kind: "page",
            value: 1.0,
            started_ms: 0.0,
        });
        assert!(!should_refresh_equal_zoom(Some(&scenario)));
    }
}
