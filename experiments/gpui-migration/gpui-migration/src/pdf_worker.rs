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
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RenderRequest {
    pub job_id: JobId,
    pub session_id: SessionId,
    pub page_index: u32,
    /// When false, the worker renders only immutable page content so the
    /// application-owned annotation scene remains the single presentation
    /// owner. Native annotation rendering is reserved for validation oracles.
    pub include_pdf_annotations: bool,
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
    writer: Arc<Mutex<BufWriter<W>>>,
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
            writer: Arc::new(Mutex::new(BufWriter::new(writer))),
        }
    }

    pub fn send(&self, request: &WorkerRequest) -> Result<(), WorkerError> {
        let mut writer = self.writer.lock().map_err(|_| {
            WorkerError::with_detail(WorkerErrorCode::WorkerCrashed, "worker input lock poisoned")
        })?;
        serde_json::to_writer(&mut *writer, request).map_err(protocol_error)?;
        writer.write_all(b"\n")?;
        writer.flush()?;
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

pub struct WorkerProcessClient {
    child: Child,
    sender: JsonLineSender<ChildStdin>,
    receiver: JsonLineReceiver<ChildStdout>,
    surface_root: PathBuf,
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
        Self::spawn_command(command, surface_root.as_ref())
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
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;

        const WORKER_SOURCE_FD: i32 = 198;
        fs::create_dir_all(surface_root.as_ref())?;
        let source = File::open(source_path)?;
        let parent_fd = source.as_raw_fd();
        let mut command = Command::new(executable.as_ref());
        configure_worker_command(&mut command, surface_root.as_ref(), pdfium_library.as_ref());
        // SAFETY: the closure calls only async-signal-safe descriptor functions.
        // It changes the child between fork and exec, not the parent process.
        unsafe {
            command.pre_exec(move || {
                if parent_fd != WORKER_SOURCE_FD && libc::dup2(parent_fd, WORKER_SOURCE_FD) < 0 {
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
        let client = Self::spawn_command(command, surface_root.as_ref())?;
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
        match Self::from_spawned_child(child, surface_root) {
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

    fn spawn_command(mut command: Command, surface_root: &Path) -> Result<Self, WorkerError> {
        let child = command.spawn()?;
        Self::from_spawned_child(child, surface_root)
    }

    fn from_spawned_child(mut child: Child, surface_root: &Path) -> Result<Self, WorkerError> {
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
        Ok(Self {
            child,
            sender: JsonLineSender::new(stdin),
            receiver: JsonLineReceiver::new(stdout),
            surface_root: surface_root.to_path_buf(),
        })
    }

    pub fn exchange(&mut self, request: &WorkerRequest) -> Result<WorkerResponse, WorkerError> {
        self.sender.send(request)?;
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
        self.sender.clone()
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
}

impl Drop for WorkerProcessClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn decode_request(line: &str) -> Result<WorkerRequest, WorkerError> {
    serde_json::from_str(line).map_err(protocol_error)
}

pub fn encode_response(response: &WorkerResponse) -> Result<Vec<u8>, WorkerError> {
    let mut encoded = serde_json::to_vec(response).map_err(protocol_error)?;
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
