//! Application-owned PDF worker protocol and state machine.
//!
//! The public seam intentionally contains only Butter Paper types. PDFium types
//! belong in the feature-gated worker binary adapter. Render pixels cross the
//! process boundary through a bounded BGRA file mapping, never PNG or base64.

use memmap2::{MmapMut, MmapOptions};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::pdf_content_geometry::PageSnapGeometry;

pub const MAX_PROTOCOL_LINE_BYTES: usize = 32 * 1024 * 1024;
const WORKER_LIFECYCLE_REQUEST_ENV: &str = "BP_MACOS_WORKER_LIFECYCLE";
const WORKER_LIFECYCLE_RECEIPT_DIR_ENV: &str = "BP_MACOS_WORKER_LIFECYCLE_RECEIPT_DIR";
const WORKER_LIFECYCLE_FD_ENV: &str = "BP_PDF_WORKER_LIFECYCLE_FD";
const WORKER_LIFECYCLE_TOKEN_ENV: &str = "BP_PDF_WORKER_LIFECYCLE_TOKEN";
#[cfg(target_os = "macos")]
const WORKER_LIFECYCLE_FD: i32 = 199;
#[cfg(unix)]
const WORKER_SOURCE_FD: i32 = 198;
const WORKER_LIFECYCLE_EVENT_LIMIT: usize = 8 * 1024;
const WORKER_LIFECYCLE_REGISTRATION_TIMEOUT: Duration = Duration::from_secs(2);
const WORKER_LIFECYCLE_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type")]
enum WorkerLifecycleEvent {
    #[serde(rename = "child-register")]
    Register {
        token: String,
        pid: u32,
        ppid: u32,
        start_abstime: u64,
    },
    #[serde(rename = "child-final")]
    Final {
        token: String,
        pid: u32,
        start_abstime: u64,
        user_ns: u64,
        system_ns: u64,
        lifetime_max_phys_footprint_bytes: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WorkerLifecycleReceipt {
    pub token: String,
    pub pid: u32,
    pub start_abstime: u64,
    pub user_ns: u64,
    pub system_ns: u64,
    pub lifetime_max_phys_footprint_bytes: u64,
    pub clean_reap: bool,
    pub exit_code: Option<i32>,
}

#[derive(Serialize)]
struct PublishedWorkerLifecycleReceipt<'a> {
    schema_version: u8,
    #[serde(rename = "type")]
    receipt_type: &'static str,
    token: &'a str,
    pid: u32,
    start_abstime: u64,
    user_ns: u64,
    system_ns: u64,
    lifetime_max_phys_footprint_bytes: u64,
    clean_reap: bool,
    exit_code: i32,
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);
    };
}

id_type!(RequestId);
id_type!(SessionId);
id_type!(SourceHandleId);
id_type!(JobId);
id_type!(SurfaceId);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerErrorCode {
    PasswordRequired,
    BadPassword,
    UnsupportedSecurity,
    MalformedDocument,
    RepairedDocument,
    PageError,
    Cancelled,
    LimitExceeded,
    WorkerCrashed,
    InvalidRequest,
    BackendUnavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkerError {
    pub code: WorkerErrorCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl WorkerError {
    pub fn new(code: WorkerErrorCode) -> Self {
        Self { code, detail: None }
    }

    pub fn with_detail(code: WorkerErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
        }
    }
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}", self.code)?;
        if let Some(detail) = &self.detail {
            write!(formatter, ": {detail}")?;
        }
        Ok(())
    }
}

impl std::error::Error for WorkerError {}

impl From<io::Error> for WorkerError {
    fn from(error: io::Error) -> Self {
        Self::with_detail(WorkerErrorCode::WorkerCrashed, error.to_string())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceFormat {
    Bgra8Premultiplied,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SurfaceDescriptor {
    pub surface_id: SurfaceId,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub byte_len: u64,
    pub format: SurfaceFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceLimits {
    pub max_dimension: u32,
    pub max_pixels: u64,
    pub max_bytes: u64,
}

impl Default for SurfaceLimits {
    fn default() -> Self {
        Self {
            max_dimension: 8_192,
            max_pixels: 32 * 1024 * 1024,
            max_bytes: 128 * 1024 * 1024,
        }
    }
}

impl SurfaceDescriptor {
    pub fn validate(&self, limits: SurfaceLimits) -> Result<usize, WorkerError> {
        if self.width == 0
            || self.height == 0
            || self.width > limits.max_dimension
            || self.height > limits.max_dimension
        {
            return Err(WorkerError::new(WorkerErrorCode::LimitExceeded));
        }

        let pixels = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .ok_or_else(|| WorkerError::new(WorkerErrorCode::LimitExceeded))?;
        let minimum_stride = self
            .width
            .checked_mul(4)
            .ok_or_else(|| WorkerError::new(WorkerErrorCode::LimitExceeded))?;
        let expected_bytes = u64::from(self.stride)
            .checked_mul(u64::from(self.height))
            .ok_or_else(|| WorkerError::new(WorkerErrorCode::LimitExceeded))?;
        if pixels > limits.max_pixels
            || self.stride != minimum_stride
            || expected_bytes != self.byte_len
            || expected_bytes > limits.max_bytes
            || self.format != SurfaceFormat::Bgra8Premultiplied
        {
            return Err(WorkerError::new(WorkerErrorCode::LimitExceeded));
        }
        usize::try_from(expected_bytes)
            .map_err(|_| WorkerError::new(WorkerErrorCode::LimitExceeded))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Rotation {
    Degrees0,
    Degrees90,
    Degrees180,
    Degrees270,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DocumentInfo {
    pub page_count: u32,
    pub repaired: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PageGeometry {
    pub media_box: [f32; 4],
    pub crop_box: [f32; 4],
    pub rotation: Rotation,
    pub display_width_points: f32,
    pub display_height_points: f32,
    /// PDF `/UserUnit`, retained in the application protocol even though the
    /// PDFium adapter does not expose this dictionary entry directly.
    #[serde(default = "default_user_unit")]
    pub user_unit: f32,
}

fn default_user_unit() -> f32 {
    1.0
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClipRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Annotation ownership policy for a raster request. Production retains only
/// annotations not admitted to the editable native scene; diagnostics may choose either extreme.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationRenderMode {
    None,
    RetainedOnly,
    /// Raw original PDFium annotation oracle; excludes Widgets/form drawing.
    All,
    /// The render-only page of `pdf_engine::vector_snapshot_layer` at
    /// `page_index`: one Revu vector Snapshot drawn for the canvas.
    VectorSnapshots,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RenderRequest {
    pub job_id: JobId,
    pub session_id: SessionId,
    pub page_index: u32,
    pub annotation_mode: AnnotationRenderMode,
    /// PDF user-space to output-surface device-space affine transform.
    pub transform: [f32; 6],
    pub clip: ClipRect,
    pub surface: SurfaceDescriptor,
}

impl RenderRequest {
    pub fn validate(&self, limits: SurfaceLimits) -> Result<(), WorkerError> {
        self.surface.validate(limits)?;
        if self.transform.iter().any(|value| !value.is_finite())
            || self.clip.x < 0
            || self.clip.y < 0
            || self.clip.width == 0
            || self.clip.height == 0
        {
            return Err(WorkerError::new(WorkerErrorCode::InvalidRequest));
        }
        let right = u32::try_from(self.clip.x)
            .ok()
            .and_then(|x| x.checked_add(self.clip.width));
        let bottom = u32::try_from(self.clip.y)
            .ok()
            .and_then(|y| y.checked_add(self.clip.height));
        if right.is_none_or(|right| right > self.surface.width)
            || bottom.is_none_or(|bottom| bottom > self.surface.height)
        {
            return Err(WorkerError::new(WorkerErrorCode::InvalidRequest));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerRequest {
    Open {
        request_id: RequestId,
        session_id: SessionId,
        source_handle_id: SourceHandleId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<String>,
    },
    PageGeometry {
        request_id: RequestId,
        session_id: SessionId,
        page_index: u32,
    },
    PageSnapGeometry {
        request_id: RequestId,
        session_id: SessionId,
        page_index: u32,
    },
    RenderCrop {
        request_id: RequestId,
        #[serde(flatten)]
        render: RenderRequest,
    },
    Cancel {
        request_id: RequestId,
        job_id: JobId,
    },
    Close {
        request_id: RequestId,
        session_id: SessionId,
    },
}

impl WorkerRequest {
    pub fn request_id(&self) -> RequestId {
        match self {
            Self::Open { request_id, .. }
            | Self::PageGeometry { request_id, .. }
            | Self::PageSnapGeometry { request_id, .. }
            | Self::RenderCrop { request_id, .. }
            | Self::Cancel { request_id, .. }
            | Self::Close { request_id, .. } => *request_id,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WorkerResponse {
    Opened {
        request_id: RequestId,
        session_id: SessionId,
        document: DocumentInfo,
    },
    PageGeometry {
        request_id: RequestId,
        session_id: SessionId,
        page_index: u32,
        geometry: PageGeometry,
    },
    PageSnapGeometry {
        request_id: RequestId,
        session_id: SessionId,
        page_index: u32,
        geometry: PageSnapGeometry,
    },
    Rendered {
        request_id: RequestId,
        job_id: JobId,
        surface_id: SurfaceId,
    },
    Cancelled {
        request_id: RequestId,
        job_id: JobId,
    },
    Closed {
        request_id: RequestId,
        session_id: SessionId,
    },
    Failed {
        request_id: RequestId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        job_id: Option<JobId>,
        error: WorkerError,
    },
}

impl WorkerResponse {
    pub fn request_id(&self) -> RequestId {
        match self {
            Self::Opened { request_id, .. }
            | Self::PageGeometry { request_id, .. }
            | Self::PageSnapGeometry { request_id, .. }
            | Self::Rendered { request_id, .. }
            | Self::Cancelled { request_id, .. }
            | Self::Closed { request_id, .. }
            | Self::Failed { request_id, .. } => *request_id,
        }
    }
}

pub trait PdfBackend {
    type Document;

    fn open(
        &mut self,
        source: SourceHandleId,
        password: Option<&str>,
    ) -> Result<(Self::Document, DocumentInfo), WorkerError>;

    fn page_geometry(
        &mut self,
        document: &mut Self::Document,
        page_index: u32,
    ) -> Result<PageGeometry, WorkerError>;

    fn page_snap_geometry(
        &mut self,
        document: &mut Self::Document,
        page_index: u32,
    ) -> Result<PageSnapGeometry, WorkerError>;

    fn render_crop(
        &mut self,
        document: &mut Self::Document,
        request: &RenderRequest,
        output: &mut [u8],
        cancelled: &AtomicBool,
    ) -> Result<(), WorkerError>;

    fn close(&mut self, document: Self::Document);
}

#[derive(Clone, Default)]
pub struct CancellationRegistry {
    jobs: Arc<Mutex<CancellationJobs>>,
}

#[derive(Default)]
struct CancellationJobs {
    tokens: HashMap<JobId, Arc<AtomicBool>>,
    active: HashSet<JobId>,
}

impl CancellationRegistry {
    pub fn register(&self, job_id: JobId) -> Result<Arc<AtomicBool>, WorkerError> {
        let mut jobs = self.jobs.lock().expect("cancellation registry poisoned");
        if jobs.active.contains(&job_id) {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::InvalidRequest,
                "job identifier is already active",
            ));
        }
        let token = jobs
            .tokens
            .entry(job_id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();
        jobs.active.insert(job_id);
        Ok(token)
    }

    pub fn cancel(&self, job_id: JobId) -> bool {
        let mut jobs = self.jobs.lock().expect("cancellation registry poisoned");
        let was_known = jobs.tokens.contains_key(&job_id);
        jobs.tokens
            .entry(job_id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::Release);
        was_known
    }

    pub fn finish(&self, job_id: JobId) {
        let mut jobs = self.jobs.lock().expect("cancellation registry poisoned");
        jobs.active.remove(&job_id);
        jobs.tokens.remove(&job_id);
    }

    fn acknowledge(&self, job_id: JobId) {
        let mut jobs = self.jobs.lock().expect("cancellation registry poisoned");
        if !jobs.active.contains(&job_id) {
            jobs.tokens.remove(&job_id);
        }
    }
}

pub trait SurfaceStore {
    fn with_surface<T>(
        &mut self,
        descriptor: &SurfaceDescriptor,
        action: impl FnOnce(&mut [u8]) -> Result<T, WorkerError>,
    ) -> Result<T, WorkerError>;
}

pub struct FileSurfaceStore {
    root: PathBuf,
}

impl FileSurfaceStore {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn surface_path(root: impl AsRef<Path>, surface_id: SurfaceId) -> PathBuf {
        root.as_ref().join(format!("surface-{}.bgra", surface_id.0))
    }

    pub fn create_surface(
        root: impl AsRef<Path>,
        descriptor: &SurfaceDescriptor,
    ) -> Result<FileMappedSurface, WorkerError> {
        descriptor.validate(SurfaceLimits::default())?;
        fs::create_dir_all(root.as_ref())?;
        let path = Self::surface_path(root, descriptor.surface_id);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)?;
        file.set_len(descriptor.byte_len)?;
        // SAFETY: this object owns the file mapping and prevents the file from
        // being resized while the mapping is live.
        let mapping = unsafe {
            MmapOptions::new()
                .len(descriptor.byte_len as usize)
                .map_mut(&file)
        }?;
        Ok(FileMappedSurface {
            descriptor: descriptor.clone(),
            path,
            file: Some(file),
            mapping: Some(mapping),
        })
    }
}

impl SurfaceStore for FileSurfaceStore {
    fn with_surface<T>(
        &mut self,
        descriptor: &SurfaceDescriptor,
        action: impl FnOnce(&mut [u8]) -> Result<T, WorkerError>,
    ) -> Result<T, WorkerError> {
        let byte_len = descriptor.validate(SurfaceLimits::default())?;
        let path = Self::surface_path(&self.root, descriptor.surface_id);
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        if file.metadata()?.len() != descriptor.byte_len {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::InvalidRequest,
                "surface mapping length does not match its descriptor",
            ));
        }
        // SAFETY: the descriptor has been checked against the file length, and
        // this worker holds the mapping only for the duration of the render.
        let mut mapping = unsafe { MmapOptions::new().len(byte_len).map_mut(&file) }?;
        let result = action(&mut mapping)?;
        mapping.flush()?;
        Ok(result)
    }
}

pub struct FileMappedSurface {
    descriptor: SurfaceDescriptor,
    path: PathBuf,
    file: Option<File>,
    mapping: Option<MmapMut>,
}

impl FileMappedSurface {
    pub fn descriptor(&self) -> &SurfaceDescriptor {
        &self.descriptor
    }

    pub fn pixels(&self) -> &[u8] {
        self.mapping.as_deref().expect("surface mapping is live")
    }

    pub fn pixels_mut(&mut self) -> &mut [u8] {
        self.mapping
            .as_deref_mut()
            .expect("surface mapping is live")
    }

    pub fn flush(&self) -> Result<(), WorkerError> {
        self.mapping
            .as_ref()
            .expect("surface mapping is live")
            .flush()
            .map_err(Into::into)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self) -> &File {
        self.file.as_ref().expect("surface file is live")
    }
}

impl Drop for FileMappedSurface {
    fn drop(&mut self) {
        if let Some(mapping) = self.mapping.take() {
            let _ = mapping.flush();
            drop(mapping);
        }
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}

pub struct WorkerState<B, S>
where
    B: PdfBackend,
    S: SurfaceStore,
{
    backend: B,
    surfaces: S,
    limits: SurfaceLimits,
    cancellation: CancellationRegistry,
    documents: HashMap<SessionId, B::Document>,
}

impl<B, S> WorkerState<B, S>
where
    B: PdfBackend,
    S: SurfaceStore,
{
    pub fn new(
        backend: B,
        surfaces: S,
        limits: SurfaceLimits,
        cancellation: CancellationRegistry,
    ) -> Self {
        Self {
            backend,
            surfaces,
            limits,
            cancellation,
            documents: HashMap::new(),
        }
    }

    pub fn handle(&mut self, request: WorkerRequest) -> WorkerResponse {
        let request_id = request.request_id();
        match request {
            WorkerRequest::Open {
                session_id,
                source_handle_id,
                password,
                ..
            } => {
                if self.documents.contains_key(&session_id) {
                    return failed(
                        request_id,
                        None,
                        WorkerError::with_detail(
                            WorkerErrorCode::InvalidRequest,
                            "session identifier is already open",
                        ),
                    );
                }
                match self.backend.open(source_handle_id, password.as_deref()) {
                    Ok((document, info)) => {
                        self.documents.insert(session_id, document);
                        WorkerResponse::Opened {
                            request_id,
                            session_id,
                            document: info,
                        }
                    }
                    Err(error) => failed(request_id, None, error),
                }
            }
            WorkerRequest::PageGeometry {
                session_id,
                page_index,
                ..
            } => {
                let Some(document) = self.documents.get_mut(&session_id) else {
                    return failed(
                        request_id,
                        None,
                        WorkerError::with_detail(
                            WorkerErrorCode::InvalidRequest,
                            "session identifier is not open",
                        ),
                    );
                };
                match self.backend.page_geometry(document, page_index) {
                    Ok(geometry) => WorkerResponse::PageGeometry {
                        request_id,
                        session_id,
                        page_index,
                        geometry,
                    },
                    Err(error) => failed(request_id, None, error),
                }
            }
            WorkerRequest::PageSnapGeometry {
                session_id,
                page_index,
                ..
            } => {
                let Some(document) = self.documents.get_mut(&session_id) else {
                    return failed(
                        request_id,
                        None,
                        WorkerError::with_detail(
                            WorkerErrorCode::InvalidRequest,
                            "session identifier is not open",
                        ),
                    );
                };
                match self.backend.page_snap_geometry(document, page_index) {
                    Ok(geometry) => WorkerResponse::PageSnapGeometry {
                        request_id,
                        session_id,
                        page_index,
                        geometry,
                    },
                    Err(error) => failed(request_id, None, error),
                }
            }
            WorkerRequest::RenderCrop { render, .. } => {
                if let Err(error) = render.validate(self.limits) {
                    return failed(request_id, Some(render.job_id), error);
                }
                let Some(document) = self.documents.get_mut(&render.session_id) else {
                    return failed(
                        request_id,
                        Some(render.job_id),
                        WorkerError::with_detail(
                            WorkerErrorCode::InvalidRequest,
                            "session identifier is not open",
                        ),
                    );
                };
                let token = match self.cancellation.register(render.job_id) {
                    Ok(token) => token,
                    Err(error) => return failed(request_id, Some(render.job_id), error),
                };
                let job_id = render.job_id;
                let surface_id = render.surface.surface_id;
                let backend = &mut self.backend;
                let result = self.surfaces.with_surface(&render.surface, |output| {
                    backend.render_crop(document, &render, output, &token)
                });
                self.cancellation.finish(job_id);
                match result {
                    Ok(()) => WorkerResponse::Rendered {
                        request_id,
                        job_id,
                        surface_id,
                    },
                    Err(error) => failed(request_id, Some(job_id), error),
                }
            }
            WorkerRequest::Cancel { job_id, .. } => {
                // The protocol reader already set the token before this request
                // reached the serialized actor.
                self.cancellation.acknowledge(job_id);
                WorkerResponse::Cancelled { request_id, job_id }
            }
            WorkerRequest::Close { session_id, .. } => {
                let Some(document) = self.documents.remove(&session_id) else {
                    return failed(
                        request_id,
                        None,
                        WorkerError::with_detail(
                            WorkerErrorCode::InvalidRequest,
                            "session identifier is not open",
                        ),
                    );
                };
                self.backend.close(document);
                WorkerResponse::Closed {
                    request_id,
                    session_id,
                }
            }
        }
    }
}

fn failed(request_id: RequestId, job_id: Option<JobId>, error: WorkerError) -> WorkerResponse {
    WorkerResponse::Failed {
        request_id,
        job_id,
        error,
    }
}

pub struct JsonLineClient<R: Read, W: Write> {
    reader: BufReader<R>,
    writer: BufWriter<W>,
}

pub struct JsonLineSender<W: Write> {
    writer: Arc<Mutex<Option<BufWriter<W>>>>,
}

impl<W: Write> Clone for JsonLineSender<W> {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
        }
    }
}

impl<W: Write> JsonLineSender<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer: Arc::new(Mutex::new(Some(BufWriter::new(writer)))),
        }
    }

    pub fn send(&self, request: &WorkerRequest) -> Result<(), WorkerError> {
        let mut writer = self.writer.lock().map_err(|_| {
            WorkerError::with_detail(WorkerErrorCode::WorkerCrashed, "worker input lock poisoned")
        })?;
        let writer = writer.as_mut().ok_or_else(|| {
            WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "PDF worker input is already closed",
            )
        })?;
        serde_json::to_writer(&mut *writer, request).map_err(protocol_error)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
        Ok(())
    }

    fn close(&self) -> Result<(), WorkerError> {
        self.writer
            .lock()
            .map_err(|_| {
                WorkerError::with_detail(
                    WorkerErrorCode::WorkerCrashed,
                    "worker input lock poisoned",
                )
            })?
            .take();
        Ok(())
    }
}

pub struct JsonLineReceiver<R: Read> {
    reader: BufReader<R>,
}

impl<R: Read> JsonLineReceiver<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    pub fn receive(&mut self) -> Result<WorkerResponse, WorkerError> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "PDF worker closed its response stream",
            ));
        }
        serde_json::from_str(&line).map_err(protocol_error)
    }
}

impl<R: Read, W: Write> JsonLineClient<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer: BufWriter::new(writer),
        }
    }

    pub fn exchange(&mut self, request: &WorkerRequest) -> Result<WorkerResponse, WorkerError> {
        serde_json::to_writer(&mut self.writer, request).map_err(protocol_error)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "PDF worker closed its response stream",
            ));
        }
        serde_json::from_str(&line).map_err(protocol_error)
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct MacosProcessUsage {
    pid: u32,
    ppid: u32,
    start_abstime: u64,
    user_ns: u64,
    system_ns: u64,
    lifetime_max_phys_footprint_bytes: u64,
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
fn mach_abstime_to_ns(value: u64) -> Result<u64, WorkerError> {
    let mut timebase = libc::mach_timebase_info { numer: 0, denom: 0 };
    // SAFETY: `timebase` is a valid writable value for the duration of the call.
    if unsafe { libc::mach_timebase_info(&mut timebase) } != 0 || timebase.denom == 0 {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::BackendUnavailable,
            "macOS Mach timebase is unavailable",
        ));
    }
    let nanoseconds = (u128::from(value) * u128::from(timebase.numer)) / u128::from(timebase.denom);
    u64::try_from(nanoseconds).map_err(|_| {
        WorkerError::with_detail(
            WorkerErrorCode::LimitExceeded,
            "macOS process CPU time exceeds the supported range",
        )
    })
}

#[cfg(target_os = "macos")]
fn current_macos_process_usage() -> Result<MacosProcessUsage, WorkerError> {
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage_info_v4>() };
    // SAFETY: the buffer points to a correctly sized `rusage_info_v4`, and the
    // public API writes it synchronously before returning.
    let result = unsafe {
        libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V4,
            (&mut usage as *mut libc::rusage_info_v4).cast(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error().into());
    }
    if usage.ri_proc_start_abstime == 0 {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::WorkerCrashed,
            "macOS process identity has no start time",
        ));
    }
    Ok(MacosProcessUsage {
        pid: std::process::id(),
        // SAFETY: getppid has no preconditions.
        ppid: unsafe { libc::getppid() as u32 },
        start_abstime: usage.ri_proc_start_abstime,
        user_ns: mach_abstime_to_ns(usage.ri_user_time)?,
        system_ns: mach_abstime_to_ns(usage.ri_system_time)?,
        lifetime_max_phys_footprint_bytes: usage.ri_lifetime_max_phys_footprint,
    })
}

fn valid_lifecycle_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// Child-side endpoint for the opt-in macOS resource-accounting lane. It owns
/// only the dedicated diagnostics descriptor; PDF protocol stdout is untouched.
pub struct WorkerLifecycleReporter {
    #[cfg(target_os = "macos")]
    writer: BufWriter<File>,
    #[cfg(target_os = "macos")]
    token: String,
    #[cfg(target_os = "macos")]
    start_abstime: u64,
}

impl WorkerLifecycleReporter {
    pub fn from_environment() -> Result<Option<Self>, WorkerError> {
        let descriptor = std::env::var_os(WORKER_LIFECYCLE_FD_ENV);
        let token = std::env::var_os(WORKER_LIFECYCLE_TOKEN_ENV);
        if descriptor.is_none() && token.is_none() {
            return Ok(None);
        }
        // SAFETY: the worker calls this once during single-threaded startup,
        // before either the protocol reader thread or supplier code can spawn.
        // The descriptor number and bearer token must not reach descendants.
        unsafe {
            std::env::remove_var(WORKER_LIFECYCLE_FD_ENV);
            std::env::remove_var(WORKER_LIFECYCLE_TOKEN_ENV);
        }
        let Some(descriptor) = descriptor else {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle measurement requested without a diagnostics descriptor",
            ));
        };
        let Some(token) = token.and_then(|value| value.into_string().ok()) else {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle measurement requested without a valid token",
            ));
        };
        if !valid_lifecycle_token(&token) {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle measurement token is invalid",
            ));
        }

        #[cfg(target_os = "macos")]
        {
            use std::os::fd::{FromRawFd, RawFd};

            let descriptor = descriptor
                .into_string()
                .ok()
                .and_then(|value| value.parse::<RawFd>().ok())
                .filter(|value| *value == WORKER_LIFECYCLE_FD)
                .ok_or_else(|| {
                    WorkerError::with_detail(
                        WorkerErrorCode::WorkerCrashed,
                        "worker lifecycle diagnostics descriptor is invalid",
                    )
                })?;
            // SAFETY: F_GETFD/F_SETFD operate on the validated inherited descriptor.
            let flags = unsafe { libc::fcntl(descriptor, libc::F_GETFD) };
            if flags < 0
                || unsafe { libc::fcntl(descriptor, libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0
            {
                return Err(io::Error::last_os_error().into());
            }
            // SAFETY: the dedicated inherited descriptor has a single owner in
            // the child after exec. Constructing File transfers that ownership.
            let file = unsafe { File::from_raw_fd(descriptor) };
            let usage = current_macos_process_usage()?;
            let mut reporter = Self {
                writer: BufWriter::new(file),
                token,
                start_abstime: usage.start_abstime,
            };
            reporter.write_event(&WorkerLifecycleEvent::Register {
                token: reporter.token.clone(),
                pid: usage.pid,
                ppid: usage.ppid,
                start_abstime: usage.start_abstime,
            })?;
            Ok(Some(reporter))
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = descriptor;
            let _ = token;
            Err(WorkerError::with_detail(
                WorkerErrorCode::BackendUnavailable,
                "worker lifecycle measurement is implemented only on macOS",
            ))
        }
    }

    #[cfg(target_os = "macos")]
    fn write_event(&mut self, event: &WorkerLifecycleEvent) -> Result<(), WorkerError> {
        serde_json::to_writer(&mut self.writer, event).map_err(protocol_error)?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn finish(mut self) -> Result<(), WorkerError> {
        #[cfg(target_os = "macos")]
        {
            let usage = current_macos_process_usage()?;
            if usage.start_abstime != self.start_abstime {
                return Err(WorkerError::with_detail(
                    WorkerErrorCode::WorkerCrashed,
                    "worker lifecycle process identity changed before final receipt",
                ));
            }
            let token = self.token.clone();
            self.write_event(&WorkerLifecycleEvent::Final {
                token,
                pid: usage.pid,
                start_abstime: usage.start_abstime,
                user_ns: usage.user_ns,
                system_ns: usage.system_ns,
                lifetime_max_phys_footprint_bytes: usage.lifetime_max_phys_footprint_bytes,
            })?;
        }
        Ok(())
    }
}

struct PendingWorkerLifecycle {
    token: String,
    reader: File,
    writer: File,
}

struct WorkerLifecycleOwner {
    token: String,
    pid: u32,
    start_abstime: u64,
    reader: BufReader<File>,
}

impl WorkerLifecycleOwner {
    #[cfg(target_os = "macos")]
    fn attach(pending: PendingWorkerLifecycle, child_pid: u32) -> Result<Self, WorkerError> {
        drop(pending.writer);
        let mut reader = BufReader::new(pending.reader);
        let event = read_lifecycle_event(&mut reader, WORKER_LIFECYCLE_REGISTRATION_TIMEOUT)?;
        let WorkerLifecycleEvent::Register {
            token,
            pid,
            ppid,
            start_abstime,
        } = event
        else {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle channel did not begin with registration",
            ));
        };
        if token != pending.token
            || pid != child_pid
            || ppid != std::process::id()
            || start_abstime == 0
        {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle registration identity did not match its owner",
            ));
        }
        Ok(Self {
            token,
            pid,
            start_abstime,
            reader,
        })
    }

    #[cfg(target_os = "macos")]
    fn finish(mut self, status: ExitStatus) -> Result<WorkerLifecycleReceipt, WorkerError> {
        let event = read_lifecycle_event(&mut self.reader, WORKER_LIFECYCLE_REGISTRATION_TIMEOUT)?;
        let WorkerLifecycleEvent::Final {
            token,
            pid,
            start_abstime,
            user_ns,
            system_ns,
            lifetime_max_phys_footprint_bytes,
        } = event
        else {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle channel did not end with a final self receipt",
            ));
        };
        if token != self.token
            || pid != self.pid
            || start_abstime != self.start_abstime
            || user_ns.saturating_add(system_ns) == 0
            || lifetime_max_phys_footprint_bytes == 0
            || !status.success()
        {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle final receipt or reap status was invalid",
            ));
        }
        Ok(WorkerLifecycleReceipt {
            token,
            pid,
            start_abstime,
            user_ns,
            system_ns,
            lifetime_max_phys_footprint_bytes,
            clean_reap: true,
            exit_code: status.code(),
        })
    }
}

#[cfg(target_os = "macos")]
fn publish_worker_lifecycle_receipt(
    receipt: &WorkerLifecycleReceipt,
    directory: &Path,
) -> Result<(), WorkerError> {
    use std::os::unix::fs::{MetadataExt as _, OpenOptionsExt as _};
    use std::sync::atomic::AtomicU64;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(1);
    if !directory.is_absolute() {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            format!("{WORKER_LIFECYCLE_RECEIPT_DIR_ENV} must be an absolute path"),
        ));
    }
    let metadata = fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            "worker lifecycle receipt directory must be a private, current-user directory",
        ));
    }
    if !valid_lifecycle_token(&receipt.token)
        || !receipt.clean_reap
        || receipt.exit_code != Some(0)
        || receipt.user_ns.saturating_add(receipt.system_ns) == 0
        || receipt.lifetime_max_phys_footprint_bytes == 0
    {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::WorkerCrashed,
            "refusing to publish an invalid worker lifecycle receipt",
        ));
    }

    let published = PublishedWorkerLifecycleReceipt {
        schema_version: 1,
        receipt_type: "pdf-worker-lifecycle-final",
        token: &receipt.token,
        pid: receipt.pid,
        start_abstime: receipt.start_abstime,
        user_ns: receipt.user_ns,
        system_ns: receipt.system_ns,
        lifetime_max_phys_footprint_bytes: receipt.lifetime_max_phys_footprint_bytes,
        clean_reap: receipt.clean_reap,
        exit_code: 0,
    };
    let mut bytes = serde_json::to_vec(&published).map_err(protocol_error)?;
    bytes.push(b'\n');
    let destination = directory.join(format!("{}.json", receipt.token));
    let temporary = directory.join(format!(
        ".{}.{}-{}.tmp",
        receipt.token,
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| -> Result<(), WorkerError> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        // A hard-link publication is atomic and, unlike rename, cannot replace
        // an existing token receipt. Both names are in the runner-owned folder.
        fs::hard_link(&temporary, &destination)?;
        fs::remove_file(&temporary)?;
        File::open(directory)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(target_os = "macos")]
fn read_lifecycle_event(
    reader: &mut BufReader<File>,
    timeout: Duration,
) -> Result<WorkerLifecycleEvent, WorkerError> {
    use std::os::fd::AsRawFd;

    let deadline = Instant::now() + timeout;
    let mut line = Vec::new();
    loop {
        if reader.buffer().is_empty() {
            let mut descriptor = libc::pollfd {
                fd: reader.get_ref().as_raw_fd(),
                events: libc::POLLIN | libc::POLLHUP,
                revents: 0,
            };
            loop {
                let now = Instant::now();
                if now >= deadline {
                    return Err(WorkerError::with_detail(
                        WorkerErrorCode::WorkerCrashed,
                        "timed out waiting for a complete worker lifecycle event",
                    ));
                }
                let remaining = deadline.saturating_duration_since(now);
                let timeout_ms = i32::try_from(remaining.as_millis().max(1)).unwrap_or(i32::MAX);
                // SAFETY: `descriptor` points to one valid pollfd for the duration of the call.
                let result = unsafe { libc::poll(&mut descriptor, 1, timeout_ms) };
                if result > 0 {
                    break;
                }
                if result == 0 {
                    return Err(WorkerError::with_detail(
                        WorkerErrorCode::WorkerCrashed,
                        "timed out waiting for a complete worker lifecycle event",
                    ));
                }
                let error = io::Error::last_os_error();
                if error.kind() != io::ErrorKind::Interrupted {
                    return Err(error.into());
                }
            }
        }

        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle channel closed before its required event",
            ));
        }
        let frame_end = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|index| index + 1);
        let consumed = frame_end.unwrap_or(available.len());
        if line.len().saturating_add(consumed) > WORKER_LIFECYCLE_EVENT_LIMIT {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle event exceeded its line limit",
            ));
        }
        line.extend_from_slice(&available[..consumed]);
        reader.consume(consumed);
        if frame_end.is_some() {
            return serde_json::from_slice(&line).map_err(protocol_error);
        }
        if line.len() == WORKER_LIFECYCLE_EVENT_LIMIT {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "worker lifecycle event exceeded its line limit",
            ));
        }
    }
}

fn configure_worker_lifecycle(
    command: &mut Command,
) -> Result<Option<PendingWorkerLifecycle>, WorkerError> {
    let Some(request) = std::env::var_os(WORKER_LIFECYCLE_REQUEST_ENV) else {
        command
            .env_remove(WORKER_LIFECYCLE_RECEIPT_DIR_ENV)
            .env_remove(WORKER_LIFECYCLE_FD_ENV)
            .env_remove(WORKER_LIFECYCLE_TOKEN_ENV);
        return Ok(None);
    };
    if request != "1" {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            format!("{WORKER_LIFECYCLE_REQUEST_ENV} must be exactly 1 when requested"),
        ));
    }

    #[cfg(target_os = "macos")]
    {
        use std::sync::atomic::AtomicU64;

        static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(1);
        let counter = TOKEN_COUNTER.fetch_add(1, Ordering::Relaxed);
        let token = format!("pdf-worker-{}-{counter}", std::process::id());
        configure_worker_lifecycle_with_token(command, token).map(Some)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = command;
        Err(WorkerError::with_detail(
            WorkerErrorCode::BackendUnavailable,
            "worker lifecycle measurement is implemented only on macOS",
        ))
    }
}

#[cfg(target_os = "macos")]
fn configure_worker_lifecycle_with_token(
    command: &mut Command,
    token: String,
) -> Result<PendingWorkerLifecycle, WorkerError> {
    if !valid_lifecycle_token(&token) {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            "worker lifecycle measurement token is invalid",
        ));
    }
    {
        use std::os::fd::{AsRawFd, FromRawFd};
        use std::os::unix::process::CommandExt;

        let mut descriptors = [-1; 2];
        // SAFETY: the array provides storage for exactly two pipe descriptors.
        if unsafe { libc::pipe(descriptors.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error().into());
        }
        // SAFETY: pipe returned two newly owned descriptors on success.
        let reader = unsafe { File::from_raw_fd(descriptors[0]) };
        // SAFETY: pipe returned two newly owned descriptors on success.
        let writer = unsafe { File::from_raw_fd(descriptors[1]) };
        for descriptor in [&reader, &writer] {
            // SAFETY: F_GETFD/F_SETFD operate on the valid owned descriptor.
            let flags = unsafe { libc::fcntl(descriptor.as_raw_fd(), libc::F_GETFD) };
            if flags < 0
                || unsafe {
                    libc::fcntl(
                        descriptor.as_raw_fd(),
                        libc::F_SETFD,
                        flags | libc::FD_CLOEXEC,
                    )
                } < 0
            {
                return Err(io::Error::last_os_error().into());
            }
        }
        let writer_fd = writer.as_raw_fd();
        // SAFETY: the closure uses only async-signal-safe descriptor operations
        // between fork and exec. The parent-owned File keeps `writer_fd` valid.
        unsafe {
            command.pre_exec(move || {
                if writer_fd != WORKER_LIFECYCLE_FD
                    && libc::dup2(writer_fd, WORKER_LIFECYCLE_FD) < 0
                {
                    return Err(io::Error::last_os_error());
                }
                let flags = libc::fcntl(WORKER_LIFECYCLE_FD, libc::F_GETFD);
                if flags < 0
                    || libc::fcntl(
                        WORKER_LIFECYCLE_FD,
                        libc::F_SETFD,
                        flags & !libc::FD_CLOEXEC,
                    ) < 0
                {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command
            .env_remove(WORKER_LIFECYCLE_REQUEST_ENV)
            .env_remove(WORKER_LIFECYCLE_RECEIPT_DIR_ENV)
            .env(WORKER_LIFECYCLE_FD_ENV, WORKER_LIFECYCLE_FD.to_string())
            .env(WORKER_LIFECYCLE_TOKEN_ENV, &token);
        Ok(PendingWorkerLifecycle {
            token,
            reader,
            writer,
        })
    }
}

#[cfg(unix)]
fn duplicate_source_for_child(source: &File) -> Result<File, WorkerError> {
    use std::os::fd::{AsRawFd, FromRawFd};

    // Keep the remap source above both fixed child destinations. Otherwise a
    // lifecycle dup2 onto 199 could destroy a source that happened to be opened
    // as descriptor 199 before the source dup2 onto 198 runs.
    let descriptor = unsafe { libc::fcntl(source.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 200) };
    if descriptor < 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: F_DUPFD_CLOEXEC returned a new descriptor owned by this process.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

#[cfg(unix)]
fn configure_inherited_source(command: &mut Command, source: &File) -> Result<File, WorkerError> {
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    let inherited_source = duplicate_source_for_child(source)?;
    let parent_fd = inherited_source.as_raw_fd();
    // SAFETY: the closure calls only async-signal-safe descriptor functions.
    // It changes the child between fork and exec, not the parent process.
    unsafe {
        command.pre_exec(move || {
            if libc::dup2(parent_fd, WORKER_SOURCE_FD) < 0 {
                return Err(io::Error::last_os_error());
            }
            let flags = libc::fcntl(WORKER_SOURCE_FD, libc::F_GETFD);
            if flags < 0
                || libc::fcntl(WORKER_SOURCE_FD, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(inherited_source)
}

pub struct WorkerProcessClient {
    child: Child,
    sender: Option<JsonLineSender<ChildStdin>>,
    receiver: JsonLineReceiver<ChildStdout>,
    surface_root: PathBuf,
    lifecycle: Option<WorkerLifecycleOwner>,
}

impl WorkerProcessClient {
    pub fn spawn(
        executable: impl AsRef<Path>,
        surface_root: impl AsRef<Path>,
        pdfium_library: impl AsRef<Path>,
    ) -> Result<Self, WorkerError> {
        fs::create_dir_all(surface_root.as_ref())?;
        let mut command = Command::new(executable.as_ref());
        configure_worker_command(&mut command, surface_root.as_ref(), pdfium_library.as_ref());
        let lifecycle = configure_worker_lifecycle(&mut command)?;
        Self::spawn_command(command, surface_root.as_ref(), lifecycle)
    }

    /// Spawns a worker with one destination-scoped source descriptor inherited
    /// out of band. The returned identifier is the only source value the caller
    /// should place in its Open request.
    #[cfg(unix)]
    pub fn spawn_with_inherited_source(
        executable: impl AsRef<Path>,
        surface_root: impl AsRef<Path>,
        pdfium_library: impl AsRef<Path>,
        source_path: impl AsRef<Path>,
    ) -> Result<(Self, SourceHandleId), WorkerError> {
        fs::create_dir_all(surface_root.as_ref())?;
        let source = File::open(source_path)?;
        Self::spawn_with_inherited_source_file(executable, surface_root, pdfium_library, source)
    }

    #[cfg(unix)]
    fn spawn_with_inherited_source_file(
        executable: impl AsRef<Path>,
        surface_root: impl AsRef<Path>,
        pdfium_library: impl AsRef<Path>,
        source: File,
    ) -> Result<(Self, SourceHandleId), WorkerError> {
        fs::create_dir_all(surface_root.as_ref())?;
        let mut command = Command::new(executable.as_ref());
        configure_worker_command(&mut command, surface_root.as_ref(), pdfium_library.as_ref());
        let lifecycle = configure_worker_lifecycle(&mut command)?;
        let inherited_source = configure_inherited_source(&mut command, &source)?;
        let client = Self::spawn_command(command, surface_root.as_ref(), lifecycle)?;
        drop(inherited_source);
        drop(source);
        Ok((client, SourceHandleId(WORKER_SOURCE_FD as u64)))
    }

    #[cfg(windows)]
    pub fn spawn_with_inherited_source(
        executable: impl AsRef<Path>,
        surface_root: impl AsRef<Path>,
        pdfium_library: impl AsRef<Path>,
        source_path: impl AsRef<Path>,
    ) -> Result<(Self, SourceHandleId), WorkerError> {
        let surface_root = surface_root.as_ref();
        let source = File::open(source_path)?;
        let owns_surface_root = prepare_surface_root(surface_root)?;
        let mut command = Command::new(executable.as_ref());
        configure_worker_command(&mut command, surface_root, pdfium_library.as_ref());
        let lifecycle = configure_worker_lifecycle(&mut command)?;
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                remove_owned_surface_root(surface_root, owns_surface_root);
                return Err(error.into());
            }
        };
        let source_handle_id = match duplicate_source_into_child(&source, &child) {
            Ok(source_handle_id) => source_handle_id,
            Err(error) => {
                stop_child(&mut child);
                remove_owned_surface_root(surface_root, owns_surface_root);
                return Err(error);
            }
        };
        drop(source);
        match Self::from_spawned_child(child, surface_root, lifecycle) {
            Ok(client) => Ok((client, source_handle_id)),
            Err(error) => {
                remove_owned_surface_root(surface_root, owns_surface_root);
                Err(error)
            }
        }
    }

    #[cfg(not(any(unix, windows)))]
    pub fn spawn_with_inherited_source(
        _executable: impl AsRef<Path>,
        _surface_root: impl AsRef<Path>,
        _pdfium_library: impl AsRef<Path>,
        _source_path: impl AsRef<Path>,
    ) -> Result<(Self, SourceHandleId), WorkerError> {
        Err(WorkerError::with_detail(
            WorkerErrorCode::BackendUnavailable,
            "inherited source-handle spawn is not implemented on this platform",
        ))
    }

    fn spawn_command(
        mut command: Command,
        surface_root: &Path,
        lifecycle: Option<PendingWorkerLifecycle>,
    ) -> Result<Self, WorkerError> {
        let child = command.spawn()?;
        Self::from_spawned_child(child, surface_root, lifecycle)
    }

    fn from_spawned_child(
        mut child: Child,
        surface_root: &Path,
        lifecycle: Option<PendingWorkerLifecycle>,
    ) -> Result<Self, WorkerError> {
        let Some(stdout) = child.stdout.take() else {
            stop_child(&mut child);
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "missing worker stdout",
            ));
        };
        let Some(stdin) = child.stdin.take() else {
            stop_child(&mut child);
            return Err(WorkerError::with_detail(
                WorkerErrorCode::WorkerCrashed,
                "missing worker stdin",
            ));
        };
        #[cfg(target_os = "macos")]
        let lifecycle = match lifecycle {
            Some(pending) => match WorkerLifecycleOwner::attach(pending, child.id()) {
                Ok(lifecycle) => Some(lifecycle),
                Err(error) => {
                    stop_child(&mut child);
                    return Err(error);
                }
            },
            None => None,
        };
        #[cfg(not(target_os = "macos"))]
        let lifecycle = {
            debug_assert!(lifecycle.is_none());
            None
        };
        Ok(Self {
            child,
            sender: Some(JsonLineSender::new(stdin)),
            receiver: JsonLineReceiver::new(stdout),
            surface_root: surface_root.to_path_buf(),
            lifecycle,
        })
    }

    pub fn exchange(&mut self, request: &WorkerRequest) -> Result<WorkerResponse, WorkerError> {
        self.sender
            .as_ref()
            .ok_or_else(|| {
                WorkerError::with_detail(
                    WorkerErrorCode::WorkerCrashed,
                    "PDF worker input is already closed",
                )
            })?
            .send(request)?;
        loop {
            let response = self.receiver.receive()?;
            if response.request_id() == request.request_id() {
                return Ok(response);
            }
            // An out-of-band Cancel acknowledgement can arrive after the
            // cancelled render response. It has no payload the caller needs.
        }
    }

    /// Returns a clonable writer for out-of-band Cancel requests while another
    /// thread waits for the corresponding render response.
    pub fn control_sender(&self) -> JsonLineSender<ChildStdin> {
        self.sender
            .as_ref()
            .expect("worker input remains available while the client is borrowed")
            .clone()
    }

    pub fn create_surface(
        &self,
        descriptor: &SurfaceDescriptor,
    ) -> Result<FileMappedSurface, WorkerError> {
        FileSurfaceStore::create_surface(&self.surface_root, descriptor)
    }

    pub fn child_id(&self) -> u32 {
        self.child.id()
    }

    pub fn lifecycle_measurement_requested(&self) -> bool {
        self.lifecycle.is_some()
    }

    /// Closes the worker protocol, waits for a clean process exit, and binds the
    /// final self receipt to the exact child that was reaped.
    pub fn finish_lifecycle_measurement(mut self) -> Result<WorkerLifecycleReceipt, WorkerError> {
        let lifecycle = self.lifecycle.take().ok_or_else(|| {
            WorkerError::with_detail(
                WorkerErrorCode::InvalidRequest,
                "worker lifecycle measurement was not requested for this process",
            )
        })?;
        if let Some(sender) = self.sender.take() {
            sender.close()?;
        }
        let deadline = std::time::Instant::now() + WORKER_LIFECYCLE_SHUTDOWN_TIMEOUT;
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            if std::time::Instant::now() >= deadline {
                stop_child(&mut self.child);
                return Err(WorkerError::with_detail(
                    WorkerErrorCode::WorkerCrashed,
                    "timed out waiting for measured PDF worker to exit cleanly",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        #[cfg(target_os = "macos")]
        {
            lifecycle.finish(status)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = lifecycle;
            let _ = status;
            Err(WorkerError::with_detail(
                WorkerErrorCode::BackendUnavailable,
                "worker lifecycle measurement is implemented only on macOS",
            ))
        }
    }

    /// Finalises an explicitly requested measurement and publishes its exact
    /// child receipt into the runner-owned per-iteration directory.
    pub fn finish_and_publish_lifecycle_measurement(
        self,
    ) -> Result<WorkerLifecycleReceipt, WorkerError> {
        let directory = std::env::var_os(WORKER_LIFECYCLE_RECEIPT_DIR_ENV).ok_or_else(|| {
            WorkerError::with_detail(
                WorkerErrorCode::InvalidRequest,
                format!(
                    "{WORKER_LIFECYCLE_RECEIPT_DIR_ENV} is required when worker lifecycle measurement is requested"
                ),
            )
        })?;
        let receipt = self.finish_lifecycle_measurement()?;
        #[cfg(target_os = "macos")]
        publish_worker_lifecycle_receipt(&receipt, Path::new(&directory))?;
        #[cfg(not(target_os = "macos"))]
        {
            let _ = directory;
            return Err(WorkerError::with_detail(
                WorkerErrorCode::BackendUnavailable,
                "worker lifecycle receipt publication is implemented only on macOS",
            ));
        }
        Ok(receipt)
    }
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
fn duplicate_source_into_child(
    source: &File,
    child: &Child,
) -> Result<SourceHandleId, WorkerError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut child_source: HANDLE = std::ptr::null_mut();
    // SAFETY: both input handles remain valid for the call. `child_source` is
    // written only on success and is owned by the child process, not this one.
    let duplicated = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            source.as_raw_handle(),
            child.as_raw_handle(),
            &mut child_source,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if duplicated == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(SourceHandleId(child_source as usize as u64))
}

#[cfg(windows)]
fn remove_owned_surface_root(surface_root: &Path, owns_surface_root: bool) {
    if owns_surface_root {
        let _ = fs::remove_dir_all(surface_root);
    }
}

#[cfg(windows)]
fn prepare_surface_root(surface_root: &Path) -> Result<bool, WorkerError> {
    if let Some(parent) = surface_root.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    match fs::create_dir(surface_root) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            fs::create_dir_all(surface_root)?;
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}

fn configure_worker_command(command: &mut Command, surface_root: &Path, pdfium_library: &Path) {
    command
        .env("BP_PDF_WORKER_SURFACE_ROOT", surface_root)
        .env("BP_PDFIUM_LIBRARY", pdfium_library)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    // The worker is a console program; started from the windowed application
    // it would otherwise open a console window for every document.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt as _;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
}

impl Drop for WorkerProcessClient {
    fn drop(&mut self) {
        self.sender.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn decode_request(line: &str) -> Result<WorkerRequest, WorkerError> {
    serde_json::from_str(line).map_err(protocol_error)
}

pub fn encode_response(response: &WorkerResponse) -> Result<Vec<u8>, WorkerError> {
    let mut encoded = serde_json::to_vec(response).map_err(protocol_error)?;
    if encoded.len() >= MAX_PROTOCOL_LINE_BYTES {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::LimitExceeded,
            "PDF worker response exceeds the protocol line limit",
        ));
    }
    encoded.push(b'\n');
    Ok(encoded)
}

fn protocol_error(error: serde_json::Error) -> WorkerError {
    WorkerError::with_detail(WorkerErrorCode::InvalidRequest, error.to_string())
}

/// Runs the serialized backend actor while a separate reader/control thread
/// observes cancellation requests immediately. PDFium calls remain confined to
/// the calling thread; only atomic cancellation tokens cross threads.
pub fn run_worker_protocol<R, W, B, S>(
    reader: R,
    mut writer: W,
    mut state: WorkerState<B, S>,
    cancellation: CancellationRegistry,
) -> Result<(), WorkerError>
where
    R: Read + Send + 'static,
    W: Write,
    B: PdfBackend,
    S: SurfaceStore,
{
    let (sender, receiver) = std::sync::mpsc::channel::<Result<WorkerRequest, WorkerError>>();
    let reader_thread = std::thread::Builder::new()
        .name("butter-paper-pdf-worker-control".to_owned())
        .spawn(move || {
            let mut reader = BufReader::new(reader);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => match decode_request(&line) {
                        Ok(request) => {
                            if let WorkerRequest::Cancel { job_id, .. } = &request {
                                // This can precede actor registration. The registry retains
                                // the pre-cancelled token until RenderCrop registers the job.
                                cancellation.cancel(*job_id);
                            }
                            if sender.send(Ok(request)).is_err() {
                                break;
                            }
                        }
                        Err(error) => {
                            if sender.send(Err(error)).is_err() {
                                break;
                            }
                        }
                    },
                    Err(error) => {
                        let _ = sender.send(Err(error.into()));
                        break;
                    }
                }
            }
        })?;

    for incoming in receiver {
        let response = match incoming {
            Ok(request) => state.handle(request),
            Err(error) => failed(RequestId(0), None, error),
        };
        writer.write_all(&encode_response(&response)?)?;
        writer.flush()?;
    }
    reader_thread.join().map_err(|_| {
        WorkerError::with_detail(
            WorkerErrorCode::WorkerCrashed,
            "PDF worker control thread panicked",
        )
    })?;
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod worker_lifecycle_tests {
    use super::*;

    const CHILD_FIXTURE_ENV: &str = "BP_TEST_WORKER_LIFECYCLE_CHILD";
    const TEST_NAME: &str =
        "pdf_worker::worker_lifecycle_tests::dedicated_lifecycle_fd_preserves_protocol_stdout";
    const NOOP_CHILD_ENV: &str = "BP_TEST_WORKER_LIFECYCLE_NOOP_CHILD";
    const NOOP_TEST_NAME: &str =
        "pdf_worker::worker_lifecycle_tests::ordinary_worker_environment_is_a_noop";
    const CLIENT_CHILD_ENV: &str = "BP_TEST_WORKER_LIFECYCLE_CLIENT_CHILD";
    const CLIENT_TEST_NAME: &str =
        "pdf_worker::worker_lifecycle_tests::worker_client_records_clean_reap";
    const DESCENDANT_ENV: &str = "BP_TEST_WORKER_LIFECYCLE_DESCENDANT";
    const COLLISION_PARENT_ENV: &str = "BP_TEST_WORKER_LIFECYCLE_COLLISION_PARENT";
    const COLLISION_CHILD_ENV: &str = "BP_TEST_WORKER_LIFECYCLE_COLLISION_CHILD";
    const COLLISION_SOURCE_ENV: &str = "BP_TEST_WORKER_LIFECYCLE_COLLISION_SOURCE";
    const COLLISION_TEST_NAME: &str =
        "pdf_worker::worker_lifecycle_tests::source_remap_survives_forced_lifecycle_fd_collision";

    #[test]
    fn ordinary_worker_environment_is_a_noop() {
        if std::env::var_os(NOOP_CHILD_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
            assert!(
                WorkerLifecycleReporter::from_environment()
                    .unwrap()
                    .is_none()
            );
            println!("ordinary-worker-stdout");
            return;
        }

        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NOOP_TEST_NAME, "--nocapture"])
            .env(NOOP_CHILD_ENV, "1")
            .env_remove(WORKER_LIFECYCLE_REQUEST_ENV)
            .env_remove(WORKER_LIFECYCLE_FD_ENV)
            .env_remove(WORKER_LIFECYCLE_TOKEN_ENV)
            .output()
            .unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("ordinary-worker-stdout"));
        assert!(!stdout.contains("child-register"));
        assert!(!stdout.contains("child-final"));
    }

    #[test]
    fn dedicated_lifecycle_fd_preserves_protocol_stdout() {
        if std::env::var_os(DESCENDANT_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
            assert!(std::env::var_os(WORKER_LIFECYCLE_FD_ENV).is_none());
            assert!(std::env::var_os(WORKER_LIFECYCLE_TOKEN_ENV).is_none());
            // SAFETY: F_GETFD only probes whether the descriptor survived exec.
            assert_eq!(
                unsafe { libc::fcntl(WORKER_LIFECYCLE_FD, libc::F_GETFD) },
                -1
            );
            return;
        }
        if std::env::var_os(CHILD_FIXTURE_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
            let reporter = WorkerLifecycleReporter::from_environment()
                .expect("fixture lifecycle environment must be valid")
                .expect("fixture lifecycle must be requested");
            assert!(std::env::var_os(WORKER_LIFECYCLE_FD_ENV).is_none());
            assert!(std::env::var_os(WORKER_LIFECYCLE_TOKEN_ENV).is_none());
            assert_ne!(
                unsafe { libc::fcntl(WORKER_LIFECYCLE_FD, libc::F_GETFD) } & libc::FD_CLOEXEC,
                0
            );
            assert!(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", TEST_NAME, "--nocapture"])
                    .env(DESCENDANT_ENV, "1")
                    .status()
                    .unwrap()
                    .success()
            );
            println!("protocol-stdout-sentinel");
            reporter
                .finish()
                .expect("fixture must publish its final self receipt");
            return;
        }

        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_FIXTURE_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let pending = configure_worker_lifecycle_with_token(
            &mut command,
            "deterministic-worker-token".to_owned(),
        )
        .unwrap();
        let mut child = command.spawn().unwrap();
        let lifecycle = WorkerLifecycleOwner::attach(pending, child.id()).unwrap();
        let mut stdout = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        let status = child.wait().unwrap();
        let receipt = lifecycle.finish(status).unwrap();

        assert_eq!(receipt.token, "deterministic-worker-token");
        assert_eq!(receipt.pid, child.id());
        assert!(receipt.start_abstime > 0);
        assert!(receipt.user_ns.saturating_add(receipt.system_ns) > 0);
        assert!(receipt.lifetime_max_phys_footprint_bytes > 0);
        assert!(receipt.clean_reap);
        assert_eq!(receipt.exit_code, Some(0));
        assert!(stdout.contains("protocol-stdout-sentinel"));
        assert!(!stdout.contains("child-register"));
        assert!(!stdout.contains("child-final"));
    }

    #[test]
    fn worker_client_records_clean_reap() {
        if std::env::var_os(CLIENT_CHILD_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
            let reporter = WorkerLifecycleReporter::from_environment()
                .expect("fixture lifecycle environment must be valid")
                .expect("fixture lifecycle must be requested");
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            reporter
                .finish()
                .expect("fixture must publish its final self receipt");
            return;
        }

        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", CLIENT_TEST_NAME, "--nocapture"])
            .env(CLIENT_CHILD_ENV, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let pending = configure_worker_lifecycle_with_token(
            &mut command,
            "worker-client-clean-reap".to_owned(),
        )
        .unwrap();
        let client = WorkerProcessClient::spawn_command(
            command,
            Path::new("/unused-lifecycle-test-surface-root"),
            Some(pending),
        )
        .unwrap();
        assert!(client.lifecycle_measurement_requested());
        let retained_sender = client.control_sender();
        let receipt = client.finish_lifecycle_measurement().unwrap();
        assert_eq!(receipt.token, "worker-client-clean-reap");
        assert!(receipt.clean_reap);
        assert_eq!(receipt.exit_code, Some(0));
        let error = retained_sender
            .send(&WorkerRequest::Cancel {
                request_id: RequestId(1),
                job_id: JobId(1),
            })
            .unwrap_err();
        assert_eq!(error.code, WorkerErrorCode::WorkerCrashed);
        assert!(
            error
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("closed")
        );
    }

    #[test]
    fn lifecycle_receipt_publication_is_private_atomic_and_no_replace() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = std::env::temp_dir().join(format!(
            "bp-worker-lifecycle-receipt-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        struct Scratch(PathBuf);
        impl Drop for Scratch {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _scratch = Scratch(root.clone());
        let receipt = WorkerLifecycleReceipt {
            token: "pdf-worker-123-1".to_owned(),
            pid: 123,
            start_abstime: 456,
            user_ns: 700,
            system_ns: 80,
            lifetime_max_phys_footprint_bytes: 4096,
            clean_reap: true,
            exit_code: Some(0),
        };

        publish_worker_lifecycle_receipt(&receipt, &root).unwrap();
        let entries = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec!["pdf-worker-123-1.json"]);
        let path = root.join("pdf-worker-123-1.json");
        let metadata = fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file());
        assert!(!metadata.file_type().is_symlink());
        assert_eq!(
            std::os::unix::fs::MetadataExt::mode(&metadata) & 0o777,
            0o600
        );
        assert_eq!(std::os::unix::fs::MetadataExt::nlink(&metadata), 1);
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "schema_version": 1,
                "type": "pdf-worker-lifecycle-final",
                "token": "pdf-worker-123-1",
                "pid": 123,
                "start_abstime": 456,
                "user_ns": 700,
                "system_ns": 80,
                "lifetime_max_phys_footprint_bytes": 4096,
                "clean_reap": true,
                "exit_code": 0,
            })
        );
        let original = fs::read(&path).unwrap();
        assert!(publish_worker_lifecycle_receipt(&receipt, &root).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn partial_lifecycle_frame_obeys_the_full_frame_deadline() {
        use std::os::fd::FromRawFd;

        let mut descriptors = [-1; 2];
        assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
        let reader = unsafe { File::from_raw_fd(descriptors[0]) };
        let mut writer = unsafe { File::from_raw_fd(descriptors[1]) };
        writer.write_all(b"{\"type\":\"child-register\"").unwrap();
        writer.flush().unwrap();
        let started = Instant::now();
        let error = read_lifecycle_event(&mut BufReader::new(reader), Duration::from_millis(75))
            .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            error
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("complete")
        );
    }

    #[test]
    fn source_remap_survives_forced_lifecycle_fd_collision() {
        use std::os::fd::{AsRawFd, FromRawFd};

        if std::env::var_os(COLLISION_CHILD_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
            let reporter = WorkerLifecycleReporter::from_environment()
                .unwrap()
                .expect("collision fixture must receive lifecycle capability");
            let mut source = unsafe { File::from_raw_fd(WORKER_SOURCE_FD) };
            let mut contents = String::new();
            source.read_to_string(&mut contents).unwrap();
            assert_eq!(contents, "forced-fd-source");
            let mut input = Vec::new();
            std::io::stdin().read_to_end(&mut input).unwrap();
            reporter.finish().unwrap();
            return;
        }

        if std::env::var_os(COLLISION_PARENT_ENV).as_deref() == Some(std::ffi::OsStr::new("1")) {
            let source_path = std::env::var_os(COLLISION_SOURCE_ENV).unwrap();
            let source = File::open(source_path).unwrap();
            let forced_source = if source.as_raw_fd() == WORKER_LIFECYCLE_FD {
                source
            } else {
                assert_eq!(
                    unsafe { libc::dup2(source.as_raw_fd(), WORKER_LIFECYCLE_FD) },
                    WORKER_LIFECYCLE_FD
                );
                // SAFETY: dup2 created a new descriptor distinct from `source`.
                unsafe { File::from_raw_fd(WORKER_LIFECYCLE_FD) }
            };
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", COLLISION_TEST_NAME, "--nocapture"])
                .env(COLLISION_CHILD_ENV, "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let pending = configure_worker_lifecycle_with_token(
                &mut command,
                "forced-fd-collision".to_owned(),
            )
            .unwrap();
            let inherited_source =
                configure_inherited_source(&mut command, &forced_source).unwrap();
            assert!(inherited_source.as_raw_fd() > WORKER_LIFECYCLE_FD);
            let client = WorkerProcessClient::spawn_command(
                command,
                Path::new("/unused-lifecycle-test-surface-root"),
                Some(pending),
            )
            .unwrap();
            let receipt = client.finish_lifecycle_measurement().unwrap();
            assert!(receipt.clean_reap);
            return;
        }

        let source_path =
            std::env::temp_dir().join(format!("bp-worker-fd-collision-{}", std::process::id()));
        fs::write(&source_path, "forced-fd-source").unwrap();
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", COLLISION_TEST_NAME, "--nocapture"])
            .env(COLLISION_PARENT_ENV, "1")
            .env(COLLISION_SOURCE_ENV, &source_path)
            .status()
            .unwrap();
        fs::remove_file(source_path).unwrap();
        assert!(status.success());
    }

    #[test]
    fn requested_lifecycle_fails_closed_when_child_does_not_register() {
        let mut command = Command::new("/usr/bin/true");
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let pending =
            configure_worker_lifecycle_with_token(&mut command, "missing-registration".to_owned())
                .unwrap();
        let mut child = command.spawn().unwrap();
        let error = match WorkerLifecycleOwner::attach(pending, child.id()) {
            Ok(_) => panic!("an uninstrumented child must not satisfy the lifecycle request"),
            Err(error) => error,
        };
        child.wait().unwrap();
        assert_eq!(error.code, WorkerErrorCode::WorkerCrashed);
        assert!(
            error
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("closed")
        );
    }

    #[test]
    fn lifecycle_token_is_bounded_and_transport_safe() {
        assert!(valid_lifecycle_token("pdf-worker_1-2"));
        assert!(!valid_lifecycle_token(""));
        assert!(!valid_lifecycle_token("contains space"));
        assert!(!valid_lifecycle_token(&"a".repeat(129)));
    }
}

#[cfg(test)]
mod annotation_mode_tests {
    use super::*;

    #[test]
    fn retained_annotation_protocol_requires_explicit_known_mode() {
        for mode in [
            AnnotationRenderMode::None,
            AnnotationRenderMode::RetainedOnly,
            AnnotationRenderMode::All,
        ] {
            let request = WorkerRequest::RenderCrop {
                request_id: RequestId(1),
                render: RenderRequest {
                    job_id: JobId(1),
                    session_id: SessionId(1),
                    page_index: 0,
                    annotation_mode: mode,
                    transform: [1., 0., 0., 1., 0., 0.],
                    clip: ClipRect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    },
                    surface: SurfaceDescriptor {
                        surface_id: SurfaceId(1),
                        width: 1,
                        height: 1,
                        stride: 4,
                        byte_len: 4,
                        format: SurfaceFormat::Bgra8Premultiplied,
                    },
                },
            };
            let mut value = serde_json::to_value(&request).unwrap();
            let decoded = decode_request(&value.to_string()).unwrap();
            assert!(
                matches!(decoded, WorkerRequest::RenderCrop { render, .. } if render.annotation_mode == mode)
            );
            value.as_object_mut().unwrap().remove("annotation_mode");
            value["include_pdf_annotations"] = serde_json::json!(false);
            let error = decode_request(&value.to_string()).unwrap_err();
            assert_eq!(error.code, WorkerErrorCode::InvalidRequest);
            assert!(
                error.detail.unwrap().contains("annotation_mode"),
                "old clients must fail clearly, not silently render a default"
            );
            value["annotation_mode"] = serde_json::json!("future_mode");
            assert_eq!(
                decode_request(&value.to_string()).unwrap_err().code,
                WorkerErrorCode::InvalidRequest
            );
        }
    }
}

#[cfg(test)]
mod page_snap_geometry_tests {
    use super::*;
    use crate::pdf_content_geometry::{PdfContentPrimitive, PdfPoint, PdfRect};

    struct NoopSurfaceStore;

    impl SurfaceStore for NoopSurfaceStore {
        fn with_surface<T>(
            &mut self,
            _: &SurfaceDescriptor,
            _: impl FnOnce(&mut [u8]) -> Result<T, WorkerError>,
        ) -> Result<T, WorkerError> {
            Err(WorkerError::new(WorkerErrorCode::InvalidRequest))
        }
    }

    struct GeometryBackend;

    impl PdfBackend for GeometryBackend {
        type Document = ();

        fn open(
            &mut self,
            _: SourceHandleId,
            _: Option<&str>,
        ) -> Result<(Self::Document, DocumentInfo), WorkerError> {
            Ok((
                (),
                DocumentInfo {
                    page_count: 1,
                    repaired: false,
                },
            ))
        }

        fn page_geometry(
            &mut self,
            _: &mut Self::Document,
            page_index: u32,
        ) -> Result<PageGeometry, WorkerError> {
            if page_index != 0 {
                return Err(WorkerError::new(WorkerErrorCode::PageError));
            }
            Ok(PageGeometry {
                media_box: [0., 0., 100., 100.],
                crop_box: [0., 0., 100., 100.],
                rotation: Rotation::Degrees0,
                display_width_points: 100.,
                display_height_points: 100.,
                user_unit: 1.,
            })
        }

        fn page_snap_geometry(
            &mut self,
            _: &mut Self::Document,
            page_index: u32,
        ) -> Result<PageSnapGeometry, WorkerError> {
            if page_index != 0 {
                return Err(WorkerError::new(WorkerErrorCode::PageError));
            }
            Ok(sample_geometry(page_index))
        }

        fn render_crop(
            &mut self,
            _: &mut Self::Document,
            _: &RenderRequest,
            _: &mut [u8],
            _: &AtomicBool,
        ) -> Result<(), WorkerError> {
            Ok(())
        }

        fn close(&mut self, _: Self::Document) {}
    }

    fn sample_geometry(page_index: u32) -> PageSnapGeometry {
        PageSnapGeometry {
            page_index,
            primitives: vec![
                PdfContentPrimitive::Line {
                    start: PdfPoint { x: 1., y: 2. },
                    end: PdfPoint { x: 3., y: 4. },
                },
                PdfContentPrimitive::Rect {
                    rect: PdfRect {
                        x: 5.,
                        y: 6.,
                        width: 7.,
                        height: 8.,
                    },
                },
                PdfContentPrimitive::Polyline {
                    points: vec![PdfPoint { x: 9., y: 10. }, PdfPoint { x: 11., y: 12. }],
                    closed: true,
                },
            ],
        }
    }

    #[test]
    fn page_snap_geometry_protocol_round_trips_every_primitive() {
        let response = WorkerResponse::PageSnapGeometry {
            request_id: RequestId(3),
            session_id: SessionId(4),
            page_index: 0,
            geometry: sample_geometry(0),
        };
        let encoded = encode_response(&response).unwrap();
        let decoded: WorkerResponse = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, response);
    }

    #[test]
    fn page_snap_geometry_requires_open_session_and_preserves_page_identity() {
        let cancellation = CancellationRegistry::default();
        let mut state = WorkerState::new(
            GeometryBackend,
            NoopSurfaceStore,
            SurfaceLimits::default(),
            cancellation,
        );
        let request = |request_id, page_index| WorkerRequest::PageSnapGeometry {
            request_id: RequestId(request_id),
            session_id: SessionId(7),
            page_index,
        };
        assert!(matches!(
            state.handle(request(1, 0)),
            WorkerResponse::Failed {
                error: WorkerError {
                    code: WorkerErrorCode::InvalidRequest,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            state.handle(WorkerRequest::Open {
                request_id: RequestId(2),
                session_id: SessionId(7),
                source_handle_id: SourceHandleId(1),
                password: None,
            }),
            WorkerResponse::Opened {
                session_id: SessionId(7),
                ..
            }
        ));
        assert_eq!(
            state.handle(request(3, 0)),
            WorkerResponse::PageSnapGeometry {
                request_id: RequestId(3),
                session_id: SessionId(7),
                page_index: 0,
                geometry: sample_geometry(0),
            }
        );
        assert!(matches!(
            state.handle(request(4, 1)),
            WorkerResponse::Failed {
                error: WorkerError {
                    code: WorkerErrorCode::PageError,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            state.handle(WorkerRequest::PageGeometry {
                request_id: RequestId(5),
                session_id: SessionId(7),
                page_index: 0,
            }),
            WorkerResponse::PageGeometry { page_index: 0, .. }
        ));
    }
}
