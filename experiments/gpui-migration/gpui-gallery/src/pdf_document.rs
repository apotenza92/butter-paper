use std::{
    collections::{HashMap, VecDeque, hash_map::DefaultHasher},
    fmt::Write as _,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use butter_paper_gpui_gallery::{
    pdf_worker::{
        ClipRect, DocumentInfo, JobId, JsonLineSender, PageGeometry, RenderRequest, RequestId,
        SessionId, SurfaceDescriptor, SurfaceFormat, SurfaceId, WorkerError, WorkerProcessClient,
        WorkerRequest, WorkerResponse,
    },
    viewer::{RenderSource, TileRequest},
};
use gpui::RenderImage;
use image::{ColorType, Frame, ImageBuffer, ImageFormat, Rgba};
use smallvec::smallvec;

const MAX_SURFACE_DIMENSION: u32 = 8_192;
const MAX_SURFACE_PIXELS: u64 = 32 * 1_024 * 1_024;
const PAGE_IMAGE_CACHE_BYTES: usize = 128 * 1_024 * 1_024;

pub fn create_blank_pdf(
    document_id: u64,
    title: &str,
    page_width: f32,
    page_height: f32,
) -> Result<PathBuf, String> {
    let directory = cache_root().join("generated-documents");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let slug = title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>();
    let path = directory.join(format!("{slug}-{document_id}.pdf"));
    fs::write(
        &path,
        blank_pdf_bytes(title, page_width.max(1.0), page_height.max(1.0)),
    )
    .map_err(|error| error.to_string())?;
    Ok(path)
}

fn blank_pdf_bytes(title: &str, page_width: f32, page_height: f32) -> Vec<u8> {
    let escaped_title = title
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {page_width:.2} {page_height:.2}] /Resources <<>> /Contents 4 0 R >>"
        ),
        "<< /Length 0 >>\nstream\n\nendstream".to_string(),
        format!("<< /Title ({escaped_title}) /Producer (Butter Paper GPUI migration) >>"),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        let _ = write!(pdf, "{} 0 obj\n{}\nendobj\n", index + 1, object);
    }
    let xref_offset = pdf.len();
    let _ = write!(pdf, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        let _ = writeln!(pdf, "{offset:010} 00000 n ");
    }
    let _ = write!(
        pdf,
        "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
        objects.len() + 1
    );
    pdf.into_bytes()
}

#[derive(Clone, Debug, PartialEq)]
struct OpenedRasterDocument {
    page_sizes: Vec<(f32, f32)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RasterCrop {
    page_pixel_width: u32,
    page_pixel_height: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

struct RasterSurface {
    width: u32,
    height: u32,
    pixels_bgra: Vec<u8>,
}

trait RasterAdapter: Send + Sync {
    fn opened(&self) -> OpenedRasterDocument;
    fn render(
        &self,
        page: usize,
        crop: RasterCrop,
        tile_generation: Option<u64>,
    ) -> Result<RasterSurface, String>;
    fn cancel_before(&self, generation: u64);
}

struct WorkerRasterAdapter {
    opened: OpenedRasterDocument,
    session_id: SessionId,
    client: Mutex<WorkerProcessClient>,
    control: JsonLineSender<std::process::ChildStdin>,
    surface_root: PathBuf,
    next_request: AtomicU64,
    next_job: AtomicU64,
    next_surface: AtomicU64,
    minimum_generation: AtomicU64,
    active_jobs: Mutex<HashMap<JobId, ActiveRasterJob>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ActiveRasterJob {
    generation: u64,
    cancel_requested: bool,
}

fn mark_stale_jobs_cancel_requested(
    active_jobs: &mut HashMap<JobId, ActiveRasterJob>,
    minimum_generation: u64,
) -> Vec<JobId> {
    active_jobs
        .iter_mut()
        .filter_map(|(job_id, job)| {
            if job.generation < minimum_generation && !job.cancel_requested {
                job.cancel_requested = true;
                Some(*job_id)
            } else {
                None
            }
        })
        .collect()
}

impl WorkerRasterAdapter {
    fn open(path: &Path, cache_dir: &Path, document_id: u64) -> Result<Arc<Self>, String> {
        let worker = worker_executable()?;
        let library = pdfium_library()?;
        static NEXT_WORKER_ROOT: AtomicU64 = AtomicU64::new(1);
        let nonce = NEXT_WORKER_ROOT.fetch_add(1, Ordering::Relaxed);
        let surface_root = cache_dir.join(format!(
            "worker-{}-{document_id}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&surface_root).map_err(|error| error.to_string())?;
        let (mut client, source_handle_id) =
            WorkerProcessClient::spawn_with_inherited_source(worker, &surface_root, library, path)
                .map_err(worker_error)?;
        let session_id = SessionId(document_id.max(1));
        let opened = client
            .exchange(&WorkerRequest::Open {
                request_id: RequestId(1),
                session_id,
                source_handle_id,
                password: None,
            })
            .map_err(worker_error)?;
        let DocumentInfo { page_count, .. } = match opened {
            WorkerResponse::Opened { document, .. } => document,
            WorkerResponse::Failed { error, .. } => return Err(worker_error(error)),
            response => return Err(unexpected_response("open", response)),
        };
        if page_count == 0 {
            return Err("PDFium reported a document with no pages.".into());
        }
        let mut page_sizes = Vec::with_capacity(page_count as usize);
        let mut next_request = 2_u64;
        for page_index in 0..page_count {
            let response = client
                .exchange(&WorkerRequest::PageGeometry {
                    request_id: RequestId(next_request),
                    session_id,
                    page_index,
                })
                .map_err(worker_error)?;
            next_request += 1;
            let geometry = match response {
                WorkerResponse::PageGeometry { geometry, .. } => geometry,
                WorkerResponse::Failed { error, .. } => return Err(worker_error(error)),
                response => return Err(unexpected_response("page geometry", response)),
            };
            page_sizes.push(display_size(geometry)?);
        }
        let control = client.control_sender();
        Ok(Arc::new(Self {
            opened: OpenedRasterDocument { page_sizes },
            session_id,
            client: Mutex::new(client),
            control,
            surface_root,
            next_request: AtomicU64::new(next_request),
            next_job: AtomicU64::new(1),
            next_surface: AtomicU64::new(1),
            minimum_generation: AtomicU64::new(0),
            active_jobs: Mutex::new(HashMap::new()),
        }))
    }

    fn request_id(&self) -> RequestId {
        RequestId(self.next_request.fetch_add(1, Ordering::Relaxed))
    }
}

impl RasterAdapter for WorkerRasterAdapter {
    fn opened(&self) -> OpenedRasterDocument {
        self.opened.clone()
    }

    fn render(
        &self,
        page: usize,
        crop: RasterCrop,
        tile_generation: Option<u64>,
    ) -> Result<RasterSurface, String> {
        if tile_generation
            .is_some_and(|generation| generation < self.minimum_generation.load(Ordering::Acquire))
        {
            return Err("PDF tile render was cancelled as stale.".into());
        }
        let job_id = JobId(self.next_job.fetch_add(1, Ordering::Relaxed));
        if let Some(generation) = tile_generation {
            self.active_jobs
                .lock()
                .map_err(|_| "PDF worker active-job lock was poisoned.".to_owned())?
                .insert(
                    job_id,
                    ActiveRasterJob {
                        generation,
                        cancel_requested: false,
                    },
                );
        }
        let result = (|| {
            let surface_id = SurfaceId(self.next_surface.fetch_add(1, Ordering::Relaxed));
            let byte_len = u64::from(crop.width)
                .checked_mul(u64::from(crop.height))
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| "PDF render surface dimensions overflow.".to_owned())?;
            let descriptor = SurfaceDescriptor {
                surface_id,
                width: crop.width,
                height: crop.height,
                stride: crop
                    .width
                    .checked_mul(4)
                    .ok_or_else(|| "PDF render stride overflowed.".to_owned())?,
                byte_len,
                format: SurfaceFormat::Bgra8Premultiplied,
            };
            let mut client = self
                .client
                .lock()
                .map_err(|_| "PDF worker client lock was poisoned.".to_owned())?;
            if tile_generation.is_some_and(|generation| {
                generation < self.minimum_generation.load(Ordering::Acquire)
            }) {
                return Err("PDF tile render was cancelled as stale.".into());
            }
            let surface = client.create_surface(&descriptor).map_err(worker_error)?;
            let page_size = self
                .opened
                .page_sizes
                .get(page.saturating_sub(1))
                .copied()
                .ok_or_else(|| "PDF page is outside the document.".to_owned())?;
            let scale_x = crop.page_pixel_width as f32 / page_size.0;
            let scale_y = crop.page_pixel_height as f32 / page_size.1;
            let response = client
                .exchange(&WorkerRequest::RenderCrop {
                    request_id: self.request_id(),
                    render: RenderRequest {
                        job_id,
                        session_id: self.session_id,
                        page_index: u32::try_from(page.saturating_sub(1))
                            .map_err(|_| "PDF page index overflowed.".to_owned())?,
                        include_pdf_annotations: false,
                        transform: [
                            scale_x,
                            0.0,
                            0.0,
                            scale_y,
                            -(crop.x as f32),
                            -(crop.y as f32),
                        ],
                        clip: ClipRect {
                            x: 0,
                            y: 0,
                            width: crop.width,
                            height: crop.height,
                        },
                        surface: descriptor,
                    },
                })
                .map_err(worker_error)?;
            match response {
                WorkerResponse::Rendered {
                    job_id: rendered_job,
                    surface_id: rendered_surface,
                    ..
                } if rendered_job == job_id && rendered_surface == surface_id => {}
                WorkerResponse::Failed { error, .. } => return Err(worker_error(error)),
                response => return Err(unexpected_response("render", response)),
            }
            if tile_generation.is_some_and(|generation| {
                generation < self.minimum_generation.load(Ordering::Acquire)
            }) {
                return Err("PDF tile render completed for a stale generation.".into());
            }
            Ok(RasterSurface {
                width: crop.width,
                height: crop.height,
                pixels_bgra: surface.pixels().to_vec(),
            })
        })();
        if tile_generation.is_some()
            && let Ok(mut active) = self.active_jobs.lock()
        {
            active.remove(&job_id);
        }
        result
    }

    fn cancel_before(&self, generation: u64) {
        self.minimum_generation
            .fetch_max(generation, Ordering::AcqRel);
        let jobs = self
            .active_jobs
            .lock()
            .map(|mut jobs| mark_stale_jobs_cancel_requested(&mut jobs, generation))
            .unwrap_or_default();
        for job_id in jobs {
            let _ = self.control.send(&WorkerRequest::Cancel {
                request_id: self.request_id(),
                job_id,
            });
        }
    }
}

impl Drop for WorkerRasterAdapter {
    fn drop(&mut self) {
        if let Ok(mut client) = self.client.lock() {
            let _ = client.exchange(&WorkerRequest::Close {
                request_id: self.request_id(),
                session_id: self.session_id,
            });
        }
        let _ = fs::remove_dir(&self.surface_root);
    }
}

fn display_size(geometry: PageGeometry) -> Result<(f32, f32), String> {
    let size = (
        geometry.display_width_points,
        geometry.display_height_points,
    );
    if !size.0.is_finite() || !size.1.is_finite() || size.0 <= 0.0 || size.1 <= 0.0 {
        return Err("PDFium reported invalid page geometry.".into());
    }
    Ok(size)
}

fn unexpected_response(operation: &str, response: WorkerResponse) -> String {
    format!("PDF worker returned an unexpected response for {operation}: {response:?}")
}

fn worker_error(error: WorkerError) -> String {
    format!(
        "PDF worker {:?}: {}",
        error.code,
        error.detail.unwrap_or_default()
    )
}

fn worker_executable() -> Result<PathBuf, String> {
    resolve_packaged_or_env(
        "BP_PDF_WORKER_EXE",
        if cfg!(windows) {
            "butter-paper-pdf-worker.exe"
        } else {
            "butter-paper-pdf-worker"
        },
    )
    .ok_or_else(|| {
        "Butter Paper's PDF worker is unavailable. Build and package butter-paper-pdf-worker."
            .to_owned()
    })
}

fn pdfium_library() -> Result<PathBuf, String> {
    let name = if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };
    if let Some(path) = std::env::var_os("BP_PDFIUM_LIBRARY")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Ok(path);
    }
    let packaged = std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(PathBuf::from))
        .map(|directory| directory.join("../Resources/pdfium").join(name))
        .filter(|path| path.is_file());
    packaged.ok_or_else(|| {
        "Butter Paper's pinned PDFium library is unavailable. Set BP_PDFIUM_LIBRARY for development."
            .to_owned()
    })
}

fn resolve_packaged_or_env(variable: &str, sibling_name: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(PathBuf::from))
                .map(|directory| directory.join(sibling_name))
                .filter(|path| path.is_file())
        })
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PageImageKey {
    Viewport { page: usize, pixel_width: u32 },
    Thumbnail { page: usize },
}

struct CachedPageImage {
    image: Arc<RenderImage>,
    bytes: usize,
}

struct PageImageCache {
    entries: HashMap<PageImageKey, CachedPageImage>,
    order: VecDeque<PageImageKey>,
    bytes: usize,
}

impl PageImageCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
        }
    }

    fn get(&mut self, key: PageImageKey) -> Option<Arc<RenderImage>> {
        let image = self.entries.get(&key)?.image.clone();
        self.order.retain(|candidate| *candidate != key);
        self.order.push_back(key);
        Some(image)
    }

    fn insert(&mut self, key: PageImageKey, image: Arc<RenderImage>, bytes: usize) {
        if bytes > PAGE_IMAGE_CACHE_BYTES {
            return;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.bytes = self.bytes.saturating_sub(previous.bytes);
            self.order.retain(|candidate| *candidate != key);
        }
        while self.bytes.saturating_add(bytes) > PAGE_IMAGE_CACHE_BYTES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.bytes = self.bytes.saturating_sub(removed.bytes);
            }
        }
        self.bytes += bytes;
        self.order.push_back(key);
        self.entries.insert(key, CachedPageImage { image, bytes });
    }
}

#[derive(Clone)]
pub struct PdfDocument {
    pub id: u64,
    pub title: String,
    pub page_count: usize,
    pub current_page: usize,
    pub page_width: f32,
    pub page_height: f32,
    revision: u64,
    page_sizes: Vec<(f32, f32)>,
    viewport_pixel_width: usize,
    viewport_image: Option<Arc<RenderImage>>,
    render_generation: Arc<AtomicU64>,
    rasterizer: Arc<dyn RasterAdapter>,
    page_images: Arc<Mutex<PageImageCache>>,
}

pub struct RenderedViewport {
    pub page: usize,
    pub page_width: f32,
    pub page_height: f32,
    pub pixel_width: usize,
    pub image: Arc<RenderImage>,
}

pub struct RenderedTile {
    pub request: TileRequest,
    pub image: Arc<RenderImage>,
}

impl PdfDocument {
    pub fn open(id: u64, path: PathBuf) -> Result<Self, String> {
        if !path.is_file()
            || path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
                != Some("pdf")
        {
            return Err("Choose a readable PDF file.".into());
        }
        let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
        let mut hasher = DefaultHasher::new();
        path.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        metadata.modified().ok().hash(&mut hasher);
        let revision = hasher.finish();
        let cache_dir = cache_root().join(format!("{revision:016x}"));
        fs::create_dir_all(&cache_dir).map_err(|error| error.to_string())?;
        let title = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Untitled PDF")
            .to_owned();
        let rasterizer = WorkerRasterAdapter::open(&path, &cache_dir, id)?;
        Self::from_rasterizer(id, title, revision, rasterizer)
    }

    fn from_rasterizer(
        id: u64,
        title: String,
        revision: u64,
        rasterizer: Arc<dyn RasterAdapter>,
    ) -> Result<Self, String> {
        let page_sizes = rasterizer.opened().page_sizes;
        let &(page_width, page_height) = page_sizes
            .first()
            .ok_or_else(|| "PDFium did not report any pages.".to_owned())?;
        Ok(Self {
            id,
            title,
            page_count: page_sizes.len(),
            current_page: 1,
            page_width,
            page_height,
            revision,
            page_sizes,
            viewport_pixel_width: 1_310,
            viewport_image: None,
            render_generation: Arc::new(AtomicU64::new(0)),
            rasterizer,
            page_images: Arc::new(Mutex::new(PageImageCache::new())),
        })
    }

    pub fn select_page(&mut self, page: usize) -> usize {
        let page = page.clamp(1, self.page_count);
        self.current_page = page;
        (self.page_width, self.page_height) = self.page_size(page);
        page
    }

    pub fn render_viewport(
        &self,
        page: usize,
        zoom_percent: f32,
        scale_factor: f32,
    ) -> Result<RenderedViewport, String> {
        let page = page.clamp(1, self.page_count);
        let (page_width, page_height) = self.page_size(page);
        let desired_width = viewport_pixel_width(page_width, zoom_percent, scale_factor);
        let (pixel_width, pixel_height) = bounded_full_page_dimensions(
            page_width,
            page_height,
            u32::try_from(desired_width).unwrap_or(MAX_SURFACE_DIMENSION),
        );
        let key = PageImageKey::Viewport { page, pixel_width };
        let image = self.render_page(key, page, pixel_width, pixel_height)?;
        Ok(RenderedViewport {
            page,
            page_width,
            page_height,
            pixel_width: pixel_width as usize,
            image,
        })
    }

    pub fn apply_rendered_viewport(&mut self, rendered: RenderedViewport) {
        self.current_page = rendered.page;
        self.page_width = rendered.page_width;
        self.page_height = rendered.page_height;
        self.viewport_pixel_width = rendered.pixel_width;
        self.viewport_image = Some(rendered.image);
    }

    pub fn viewport_image(&self) -> Option<Arc<RenderImage>> {
        self.viewport_image.clone()
    }

    pub fn viewport_render_density(&self, zoom_percent: f32, scale_factor: f32) -> f32 {
        self.viewport_pixel_width as f32
            / (self.page_width * zoom_percent / 100.0 * scale_factor).max(1.0)
    }

    pub fn viewport_pixel_width_for(
        &self,
        page: usize,
        zoom_percent: f32,
        scale_factor: f32,
    ) -> usize {
        viewport_pixel_width(self.page_size(page).0, zoom_percent, scale_factor)
    }

    pub fn cached_viewport_image(
        &self,
        page: usize,
        zoom_percent: f32,
        scale_factor: f32,
    ) -> Option<Arc<RenderImage>> {
        let page = page.clamp(1, self.page_count);
        let (width, height) = self.page_size(page);
        let desired = self.viewport_pixel_width_for(page, zoom_percent, scale_factor) as u32;
        let (pixel_width, _) = bounded_full_page_dimensions(width, height, desired);
        self.page_images
            .lock()
            .ok()?
            .get(PageImageKey::Viewport { page, pixel_width })
    }

    pub fn export_cached_viewport_png(
        &self,
        page: usize,
        zoom_percent: f32,
        scale_factor: f32,
        path: &Path,
    ) -> Result<(u32, u32), String> {
        let image = self
            .cached_viewport_image(page, zoom_percent, scale_factor)
            .ok_or_else(|| format!("page {page} has no current cached viewport raster"))?;
        let size = image.size(0);
        let width = u32::try_from(size.width.0).map_err(|_| "raster width overflowed")?;
        let height = u32::try_from(size.height.0).map_err(|_| "raster height overflowed")?;
        let bgra = image
            .as_bytes(0)
            .ok_or_else(|| "current raster has no frame bytes".to_owned())?;
        let mut rgb = Vec::with_capacity(bgra.len() / 4 * 3);
        for pixel in bgra.chunks_exact(4) {
            rgb.extend_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
        if let Some(directory) = path.parent() {
            fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        }
        image::save_buffer_with_format(
            path,
            &rgb,
            width,
            height,
            ColorType::Rgb8,
            ImageFormat::Png,
        )
        .map_err(|error| error.to_string())?;
        Ok((width, height))
    }

    pub fn cached_image_bytes(&self) -> usize {
        self.page_images
            .lock()
            .map(|cache| cache.bytes)
            .unwrap_or(usize::MAX)
    }

    pub fn thumbnail_image(&self, page: usize) -> Option<Arc<RenderImage>> {
        self.page_images.lock().ok()?.get(PageImageKey::Thumbnail {
            page: page.clamp(1, self.page_count),
        })
    }

    pub fn render_thumbnail(&self, page: usize) -> Result<Arc<RenderImage>, String> {
        let page = page.clamp(1, self.page_count);
        let (width, height) = self.page_size(page);
        let (pixel_width, pixel_height) = bounded_full_page_dimensions(width, height, 228);
        self.render_page(
            PageImageKey::Thumbnail { page },
            page,
            pixel_width,
            pixel_height,
        )
    }

    pub fn page_size(&self, page: usize) -> (f32, f32) {
        self.page_sizes[page.clamp(1, self.page_count) - 1]
    }

    pub fn render_source(&self) -> RenderSource {
        RenderSource {
            document_id: self.id,
            revision: self.revision,
        }
    }

    pub fn set_render_generation(&self, generation: u64) {
        self.render_generation.store(generation, Ordering::Release);
        self.rasterizer.cancel_before(generation);
    }

    pub fn render_tile(
        &self,
        request: TileRequest,
        zoom_percent: f32,
        scale_factor: f32,
    ) -> Result<RenderedTile, String> {
        let zoom_tenths = (zoom_percent.max(0.1) * 10.0).round() as u32;
        let device_scale_millis = (scale_factor.max(0.1) * 1_000.0).round() as u32;
        if request.source != self.render_source()
            || request.zoom_tenths != zoom_tenths
            || request.device_scale_millis != device_scale_millis
        {
            return Err("PDF tile request does not match its render source or scale.".into());
        }
        if self.render_generation.load(Ordering::Acquire) != request.generation {
            return Err("PDF tile render was cancelled as stale.".into());
        }
        let (page_width, page_height) = self.page_size(request.page);
        let scale = zoom_percent.max(0.1) / 100.0 * scale_factor.max(0.1);
        let page_pixel_width = scaled_dimension(page_width, scale)?;
        let page_pixel_height = scaled_dimension(page_height, scale)?;
        let crop = tile_crop(request, page_pixel_width, page_pixel_height)?;
        let surface = self
            .rasterizer
            .render(request.page, crop, Some(request.generation))?;
        if self.render_generation.load(Ordering::Acquire) != request.generation {
            return Err("PDF tile render completed for a stale generation.".into());
        }
        Ok(RenderedTile {
            request,
            image: render_image(surface)?,
        })
    }

    fn render_page(
        &self,
        key: PageImageKey,
        page: usize,
        pixel_width: u32,
        pixel_height: u32,
    ) -> Result<Arc<RenderImage>, String> {
        if let Some(image) = self
            .page_images
            .lock()
            .map_err(|_| "PDF image cache lock was poisoned.".to_owned())?
            .get(key)
        {
            return Ok(image);
        }
        let surface = self.rasterizer.render(
            page,
            RasterCrop {
                page_pixel_width: pixel_width,
                page_pixel_height: pixel_height,
                x: 0,
                y: 0,
                width: pixel_width,
                height: pixel_height,
            },
            None,
        )?;
        let bytes = surface.pixels_bgra.len();
        let image = render_image(surface)?;
        self.page_images
            .lock()
            .map_err(|_| "PDF image cache lock was poisoned.".to_owned())?
            .insert(key, image.clone(), bytes);
        Ok(image)
    }
}

fn tile_crop(
    request: TileRequest,
    page_pixel_width: u32,
    page_pixel_height: u32,
) -> Result<RasterCrop, String> {
    let x = u32::try_from(request.crop.x).map_err(|_| "PDF tile x offset overflowed.")?;
    let y = u32::try_from(request.crop.y).map_err(|_| "PDF tile y offset overflowed.")?;
    let width = u32::try_from(request.crop.width).map_err(|_| "PDF tile width overflowed.")?;
    let height = u32::try_from(request.crop.height).map_err(|_| "PDF tile height overflowed.")?;
    if width == 0
        || height == 0
        || width > MAX_SURFACE_DIMENSION
        || height > MAX_SURFACE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_SURFACE_PIXELS
        || x.checked_add(width)
            .is_none_or(|right| right > page_pixel_width)
        || y.checked_add(height)
            .is_none_or(|bottom| bottom > page_pixel_height)
    {
        return Err("PDF tile crop exceeds its bounded page surface.".into());
    }
    Ok(RasterCrop {
        page_pixel_width,
        page_pixel_height,
        x,
        y,
        width,
        height,
    })
}

fn render_image(surface: RasterSurface) -> Result<Arc<RenderImage>, String> {
    let expected = u64::from(surface.width) * u64::from(surface.height) * 4;
    if expected != surface.pixels_bgra.len() as u64 {
        return Err("PDF worker returned an invalid BGRA surface length.".into());
    }
    // GPUI RenderImage consumes image::Frame storage as platform-native BGRA.
    // The Rgba wrapper is only the byte container; no PNG or channel conversion
    // is performed on this hot path.
    let pixels = ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(
        surface.width,
        surface.height,
        surface.pixels_bgra,
    )
    .ok_or_else(|| "Could not construct GPUI's PDF render surface.".to_owned())?;
    Ok(Arc::new(RenderImage::new(smallvec![Frame::new(pixels)])))
}

fn bounded_full_page_dimensions(width: f32, height: f32, desired_width: u32) -> (u32, u32) {
    let mut width_px = desired_width.clamp(1, MAX_SURFACE_DIMENSION) as f64;
    let mut height_px = (width_px * f64::from(height) / f64::from(width.max(1.0))).ceil();
    if height_px > f64::from(MAX_SURFACE_DIMENSION) {
        let factor = f64::from(MAX_SURFACE_DIMENSION) / height_px;
        width_px *= factor;
        height_px *= factor;
    }
    let pixels = width_px * height_px;
    if pixels > MAX_SURFACE_PIXELS as f64 {
        let factor = (MAX_SURFACE_PIXELS as f64 / pixels).sqrt();
        width_px *= factor;
        height_px *= factor;
    }
    (
        width_px.floor().max(1.0) as u32,
        height_px.floor().max(1.0) as u32,
    )
}

fn scaled_dimension(points: f32, scale: f32) -> Result<u32, String> {
    let value = (points * scale).ceil();
    if !value.is_finite() || value < 1.0 || value > u32::MAX as f32 {
        return Err("PDF page device dimensions are outside the supported range.".into());
    }
    Ok(value as u32)
}

fn viewport_pixel_width(page_width: f32, zoom_percent: f32, scale_factor: f32) -> usize {
    let scale_factor = if scale_factor.is_finite() {
        scale_factor.max(0.1)
    } else {
        1.0
    };
    ((page_width * zoom_percent / 100.0 * scale_factor).ceil() as usize).clamp(256, 4096)
}

fn cache_root() -> PathBuf {
    if let Some(path) = std::env::var_os("BP_GPUI_CACHE_DIR") {
        return PathBuf::from(path);
    }
    platform_cache_root(dirs::cache_dir())
}

fn platform_cache_root(platform_cache: Option<PathBuf>) -> PathBuf {
    platform_cache
        .unwrap_or_else(std::env::temp_dir)
        .join("com.butterpaper.gpui-spike")
        .join("pdf-cache")
}

#[cfg(test)]
mod tests {
    use super::*;
    use butter_paper_gpui_gallery::viewer::PixelRect;
    use std::{
        sync::{
            Barrier,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    struct RecordingFake {
        opened: OpenedRasterDocument,
        renders: Mutex<Vec<RasterCrop>>,
        minimum_generation: AtomicU64,
        started: Option<Arc<Barrier>>,
        release: Option<Arc<Barrier>>,
        drops: Option<Arc<AtomicUsize>>,
    }

    impl RecordingFake {
        fn mixed() -> Self {
            Self {
                opened: OpenedRasterDocument {
                    page_sizes: vec![(612.0, 792.0), (1_440.0, 1_080.0), (792.0, 612.0)],
                },
                renders: Mutex::new(Vec::new()),
                minimum_generation: AtomicU64::new(0),
                started: None,
                release: None,
                drops: None,
            }
        }
    }

    impl RasterAdapter for RecordingFake {
        fn opened(&self) -> OpenedRasterDocument {
            self.opened.clone()
        }

        fn render(
            &self,
            _page: usize,
            crop: RasterCrop,
            _tile_generation: Option<u64>,
        ) -> Result<RasterSurface, String> {
            self.renders.lock().unwrap().push(crop);
            if let Some(started) = &self.started {
                started.wait();
            }
            if let Some(release) = &self.release {
                release.wait();
            }
            Ok(RasterSurface {
                width: crop.width,
                height: crop.height,
                pixels_bgra: vec![0x7f; crop.width as usize * crop.height as usize * 4],
            })
        }

        fn cancel_before(&self, generation: u64) {
            self.minimum_generation
                .fetch_max(generation, Ordering::AcqRel);
        }
    }

    impl Drop for RecordingFake {
        fn drop(&mut self) {
            if let Some(drops) = &self.drops {
                drops.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn document_with(adapter: Arc<dyn RasterAdapter>) -> PdfDocument {
        PdfDocument::from_rasterizer(41, "Mixed".into(), 99, adapter).unwrap()
    }

    #[test]
    fn document_uses_mixed_page_geometry_from_the_raster_adapter() {
        let document = document_with(Arc::new(RecordingFake::mixed()));
        assert_eq!(document.page_count, 3);
        assert_eq!(document.page_size(1), (612.0, 792.0));
        assert_eq!(document.page_size(2), (1_440.0, 1_080.0));
        assert_eq!(document.page_size(3), (792.0, 612.0));
    }

    #[test]
    fn exports_the_exact_current_bgra_raster_as_rgb_png_evidence() {
        let document = document_with(Arc::new(RecordingFake::mixed()));
        let rendered = document.render_viewport(1, 100.0, 2.0).unwrap();
        let path = std::env::temp_dir().join(format!(
            "bp-current-raster-{}-{}.png",
            std::process::id(),
            rendered.pixel_width
        ));
        let dimensions = document
            .export_cached_viewport_png(1, 100.0, 2.0, &path)
            .unwrap();
        assert_eq!(dimensions, (1_224, 1_584));
        let image = image::open(&path).unwrap().to_rgb8();
        assert_eq!(image.dimensions(), dimensions);
        assert_eq!(image.get_pixel(0, 0).0, [0x7f; 3]);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn high_zoom_renders_only_the_bounded_requested_crop() {
        let adapter = Arc::new(RecordingFake::mixed());
        let document = document_with(adapter.clone());
        let request = TileRequest {
            source: document.render_source(),
            generation: 7,
            page: 1,
            zoom_tenths: 100_000,
            device_scale_millis: 1_000,
            rotation_quarter_turns: 0,
            crop: PixelRect {
                x: 40_000,
                y: 50_000,
                width: 1_024,
                height: 768,
            },
        };
        document.set_render_generation(7);
        let rendered = document.render_tile(request, 10_000.0, 1.0).unwrap();
        assert_eq!(rendered.image.size(0).width.0, 1_024);
        assert_eq!(rendered.image.size(0).height.0, 768);
        let crop = adapter.renders.lock().unwrap()[0];
        assert_eq!((crop.width, crop.height), (1_024, 768));
        assert_eq!(
            (crop.page_pixel_width, crop.page_pixel_height),
            (61_200, 79_200)
        );
    }

    #[test]
    fn stale_worker_jobs_are_requested_for_cancellation_only_once() {
        let mut active_jobs = HashMap::from([
            (
                JobId(1),
                ActiveRasterJob {
                    generation: 10,
                    cancel_requested: false,
                },
            ),
            (
                JobId(2),
                ActiveRasterJob {
                    generation: 11,
                    cancel_requested: false,
                },
            ),
        ]);

        let mut first = mark_stale_jobs_cancel_requested(&mut active_jobs, 12);
        first.sort();
        assert_eq!(first, vec![JobId(1), JobId(2)]);
        assert!(mark_stale_jobs_cancel_requested(&mut active_jobs, 13).is_empty());
        assert!(mark_stale_jobs_cancel_requested(&mut active_jobs, 100).is_empty());

        active_jobs.insert(
            JobId(3),
            ActiveRasterJob {
                generation: 12,
                cancel_requested: false,
            },
        );
        assert_eq!(
            mark_stale_jobs_cancel_requested(&mut active_jobs, 13),
            vec![JobId(3)]
        );
        active_jobs.remove(&JobId(3));
        assert!(!active_jobs.contains_key(&JobId(3)));
    }

    #[test]
    #[ignore = "set BP_PDFIUM_LIBRARY, BP_PDF_WORKER_EXE, and BP_PDF_TILE_PROBE_PDF to opt in"]
    fn public_pdf_high_zoom_tile_completes_within_probe_budget() {
        let pdf = std::env::var_os("BP_PDF_TILE_PROBE_PDF")
            .map(PathBuf::from)
            .expect("BP_PDF_TILE_PROBE_PDF");
        let document = PdfDocument::open(91, pdf).expect("open public PDF through worker");
        let zoom_percent = 1_600.0;
        let scale_factor = 1.0;
        let page_pixel_width = scaled_dimension(document.page_size(1).0, 16.0).unwrap();
        let page_pixel_height = scaled_dimension(document.page_size(1).1, 16.0).unwrap();
        let crop_width = 1_024_u32.min(page_pixel_width);
        let crop_height = 1_024_u32.min(page_pixel_height);
        let request = TileRequest {
            source: document.render_source(),
            generation: 1,
            page: 1,
            zoom_tenths: 16_000,
            device_scale_millis: 1_000,
            rotation_quarter_turns: 0,
            crop: PixelRect {
                x: 0,
                y: 0,
                width: usize::try_from(crop_width).unwrap(),
                height: usize::try_from(crop_height).unwrap(),
            },
        };
        document.set_render_generation(1);
        let started = std::time::Instant::now();
        let rendered = document
            .render_tile(request, zoom_percent, scale_factor)
            .expect("render high-zoom crop");
        let elapsed = started.elapsed();
        eprintln!("top-left high-zoom tile rendered in {elapsed:?}");
        assert_eq!(rendered.image.size(0).width.0, crop_width as i32);
        assert_eq!(rendered.image.size(0).height.0, crop_height as i32);
        assert!(
            elapsed <= std::time::Duration::from_secs(10),
            "one bounded high-zoom tile took {elapsed:?}"
        );
    }

    #[test]
    fn rejects_a_render_that_completes_after_its_generation_is_stale() {
        let started = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let mut fake = RecordingFake::mixed();
        fake.started = Some(started.clone());
        fake.release = Some(release.clone());
        let adapter = Arc::new(fake);
        let document = document_with(adapter);
        document.set_render_generation(1);
        let request = TileRequest {
            source: document.render_source(),
            generation: 1,
            page: 1,
            zoom_tenths: 1_000,
            device_scale_millis: 1_000,
            rotation_quarter_turns: 0,
            crop: PixelRect {
                x: 0,
                y: 0,
                width: 256,
                height: 256,
            },
        };
        let rendering = {
            let document = document.clone();
            thread::spawn(move || document.render_tile(request, 100.0, 1.0))
        };
        started.wait();
        document.set_render_generation(2);
        release.wait();
        let error = match rendering.join().unwrap() {
            Ok(_) => panic!("stale render must not be accepted"),
            Err(error) => error,
        };
        assert!(error.contains("stale"));
    }

    #[test]
    fn viewport_render_survives_a_new_tile_plan_generation() {
        let started = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let mut fake = RecordingFake::mixed();
        fake.started = Some(started.clone());
        fake.release = Some(release.clone());
        let document = document_with(Arc::new(fake));
        let rendering = {
            let document = document.clone();
            thread::spawn(move || document.render_viewport(1, 100.0, 1.0))
        };
        started.wait();
        document.set_render_generation(2);
        release.wait();
        let rendered = rendering
            .join()
            .unwrap()
            .expect("a tile-plan generation must not cancel a full-page viewport render");
        assert_eq!(rendered.page, 1);
    }

    #[test]
    fn close_then_reopen_releases_each_document_adapter() {
        let drops = Arc::new(AtomicUsize::new(0));
        for revision in [1, 2] {
            let mut fake = RecordingFake::mixed();
            fake.drops = Some(drops.clone());
            let adapter = Arc::new(fake);
            let document =
                PdfDocument::from_rasterizer(41, "Mixed".into(), revision, adapter).unwrap();
            drop(document);
        }
        assert_eq!(drops.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn generated_blank_pdf_has_valid_object_offsets() {
        let bytes = blank_pdf_bytes("Engineering (blank)", 612.0, 792.0);
        let text = String::from_utf8(bytes).unwrap();
        let xref = text
            .split("xref\n0 6\n0000000000 65535 f \n")
            .nth(1)
            .unwrap();
        for (index, line) in xref.lines().take(5).enumerate() {
            let offset = line[..10].parse::<usize>().unwrap();
            assert!(text[offset..].starts_with(&format!("{} 0 obj", index + 1)));
        }
    }

    #[test]
    fn full_page_surface_is_bounded_by_dimensions_and_memory() {
        let (width, height) = bounded_full_page_dimensions(612.0, 20_000.0, 8_192);
        assert!(width <= MAX_SURFACE_DIMENSION);
        assert!(height <= MAX_SURFACE_DIMENSION);
        assert!(u64::from(width) * u64::from(height) <= MAX_SURFACE_PIXELS);
    }

    #[test]
    fn keeps_pdf_cache_below_the_platform_cache_directory() {
        assert_eq!(
            platform_cache_root(Some(PathBuf::from("/platform/cache"))),
            PathBuf::from("/platform/cache/com.butterpaper.gpui-spike/pdf-cache")
        );
    }

    #[test]
    #[ignore = "set BP_PDF_WORKER_EXE and BP_PDFIUM_LIBRARY to opt in"]
    fn pdfium_document_path_returns_a_direct_gpui_surface() {
        let id = u64::from(std::process::id()) + 190_000;
        let path = create_blank_pdf(id, "pdfium-document-integration", 612.0, 792.0).unwrap();
        let mut document = PdfDocument::open(id, path.clone()).unwrap();
        let rendered = document.render_viewport(1, 100.0, 1.0).unwrap();
        assert_eq!(rendered.image.size(0).width.0, rendered.pixel_width as i32);
        document.apply_rendered_viewport(rendered);
        assert!(document.viewport_image().is_some());
        drop(document);
        let _ = fs::remove_file(path);
    }

    #[test]
    #[ignore = "set BP_PDF_WORKER_EXE, BP_PDFIUM_LIBRARY, and BP_PDFIUM_PUBLIC_TEST_PDF to opt in"]
    fn pdfium_public_document_path_returns_a_direct_gpui_surface() {
        let path = std::env::var_os("BP_PDFIUM_PUBLIC_TEST_PDF")
            .map(PathBuf::from)
            .expect("set BP_PDFIUM_PUBLIC_TEST_PDF to a public PDF fixture");
        let id = u64::from(std::process::id()) + 191_000;
        let mut document = PdfDocument::open(id, path).unwrap();
        let rendered = document.render_viewport(1, 100.0, 1.0).unwrap();
        assert_eq!(rendered.image.size(0).width.0, rendered.pixel_width as i32);
        document.apply_rendered_viewport(rendered);
        assert!(document.viewport_image().is_some());
    }
}
