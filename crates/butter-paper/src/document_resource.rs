use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use crate::{
    annotation_model::{
        Annotation, DecodedRgbaAsset, LengthCalibration, PageRotation, PageScale, PageTransform,
        PdfRect, PenAnnotation, RetainedAnnotationObstacle, ScalePreset,
    },
    highlight_compositor::{HighlightRasterMapping, precompose_highlights_multiply_bgra_mapped},
    page_geometry::{
        PageCoordinateSpace, PdfRect as CoordinateRect, Rotation as CoordinateRotation,
    },
    pdf_content_geometry::PageSnapGeometry,
    pdf_engine::PdfPersistenceSession,
    pdf_worker::{
        AnnotationRenderMode, ClipRect, DocumentInfo, JobId, PageGeometry, RenderRequest,
        RequestId, Rotation, SessionId, SurfaceDescriptor, SurfaceFormat, SurfaceId, WorkerError,
        WorkerErrorCode, WorkerProcessClient, WorkerRequest, WorkerResponse,
    },
    semantic_snapping::PageGridDefinition,
    viewer::TileRequest,
};
use gpui::RenderImage;
use image::{Frame, ImageBuffer, Rgba};
use sha2::{Digest as _, Sha256};
use smallvec::smallvec;

pub const DEFAULT_PAGE_RENDER_WIDTH: u32 = 900;
pub const DEFAULT_THUMBNAIL_WIDTH: u32 = 104;
pub const DEFAULT_THUMBNAIL_COUNT: usize = 12;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DocumentId(u64);

impl DocumentId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "document-{}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RasterSurface {
    width: u32,
    height: u32,
    pixels_bgra: Vec<u8>,
}

impl RasterSurface {
    pub fn new(width: u32, height: u32, pixels_bgra: Vec<u8>) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("a document raster must have non-zero dimensions".into());
        }
        let expected = u64::from(width)
            .checked_mul(u64::from(height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or_else(|| "document raster dimensions overflow".to_owned())?;
        if expected != pixels_bgra.len() as u64 {
            return Err("document raster BGRA length does not match its dimensions".into());
        }
        Ok(Self {
            width,
            height,
            pixels_bgra,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width
    }

    pub const fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels_bgra(&self) -> &[u8] {
        &self.pixels_bgra
    }

    pub fn rotated(&self, rotation: PageRotation) -> Result<Self, String> {
        if rotation == PageRotation::Degrees0 {
            return Ok(self.clone());
        }
        let (target_width, target_height) = if rotation.swaps_axes() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        };
        let mut pixels = vec![0; self.pixels_bgra.len()];
        for source_y in 0..self.height {
            for source_x in 0..self.width {
                let (target_x, target_y) = match rotation {
                    PageRotation::Degrees0 => (source_x, source_y),
                    PageRotation::Degrees90 => (self.height - 1 - source_y, source_x),
                    PageRotation::Degrees180 => {
                        (self.width - 1 - source_x, self.height - 1 - source_y)
                    }
                    PageRotation::Degrees270 => (source_y, self.width - 1 - source_x),
                };
                let source = ((source_y * self.width + source_x) * 4) as usize;
                let target = ((target_y * target_width + target_x) * 4) as usize;
                pixels[target..target + 4].copy_from_slice(&self.pixels_bgra[source..source + 4]);
            }
        }
        Self::new(target_width, target_height, pixels)
    }

    pub fn cropped(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Self, String> {
        if width == 0
            || height == 0
            || x.saturating_add(width) > self.width
            || y.saturating_add(height) > self.height
        {
            return Err("document raster crop is outside the surface".into());
        }
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in y..y + height {
            let start = ((row * self.width + x) * 4) as usize;
            let end = start + (width * 4) as usize;
            pixels.extend_from_slice(&self.pixels_bgra[start..end]);
        }
        Self::new(width, height, pixels)
    }

    pub(crate) fn snapshot_asset(
        &self,
        rect: PdfRect,
        coordinate_space: PageCoordinateSpace,
    ) -> Result<DecodedRgbaAsset, String> {
        let transform = PageTransform::from_page_coordinate_space(coordinate_space, 1.)
            .map_err(|error| error.to_string())?;
        let local = transform.rect_to_local_pixels(rect);
        let (local_width, local_height) = coordinate_space.display_size_points();
        let scale_x = f64::from(self.width) / local_width;
        let scale_y = f64::from(self.height) / local_height;
        let x = (local.x * scale_x).round().clamp(0., f64::from(self.width)) as u32;
        let y = (local.y * scale_y)
            .round()
            .clamp(0., f64::from(self.height)) as u32;
        let width = (local.width * scale_x)
            .round()
            .clamp(0., f64::from(self.width.saturating_sub(x))) as u32;
        let height = (local.height * scale_y)
            .round()
            .clamp(0., f64::from(self.height.saturating_sub(y))) as u32;
        self.cropped(x, y, width, height)?.to_rgba_asset()
    }

    /// The whole surface as straight-alpha RGBA.
    fn to_rgba_asset(&self) -> Result<DecodedRgbaAsset, String> {
        let crop = self;
        let (width, height) = (self.width, self.height);
        let mut rgba = Vec::with_capacity(crop.pixels_bgra.len());
        for pixel in crop.pixels_bgra.chunks_exact(4) {
            let alpha = u32::from(pixel[3]);
            let unpremultiply = |component: u8| -> u8 {
                if alpha == 0 {
                    0
                } else {
                    ((u32::from(component) * 255 + alpha / 2) / alpha).min(255) as u8
                }
            };
            rgba.extend_from_slice(&[
                unpremultiply(pixel[2]),
                unpremultiply(pixel[1]),
                unpremultiply(pixel[0]),
                pixel[3],
            ]);
        }
        DecodedRgbaAsset::new(width, height, rgba).map_err(|error| error.to_string())
    }

    pub fn has_spatial_variation(&self) -> bool {
        let mut pixels = self.pixels_bgra.chunks_exact(4);
        let Some(first) = pixels.next() else {
            return false;
        };
        pixels.any(|pixel| pixel != first)
    }

    // Windows and Linux composite highlights into the raster; macOS draws them.
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) fn precompose_highlights(
        &mut self,
        page_index: u32,
        coordinate_space: PageCoordinateSpace,
        crop_x_px: f64,
        crop_y_px: f64,
        full_page_pixel_size: (f64, f64),
        pens: &[PenAnnotation],
    ) -> Result<usize, String> {
        let display_size = coordinate_space.display_size_points();
        let mapping = HighlightRasterMapping::from_coordinate_space(
            page_index,
            coordinate_space,
            full_page_pixel_size.0 / display_size.0,
            full_page_pixel_size.1 / display_size.1,
            crop_x_px,
            crop_y_px,
        )?;
        precompose_highlights_multiply_bgra_mapped(
            &mut self.pixels_bgra,
            self.width,
            self.height,
            mapping,
            pens,
        )
    }

    pub(crate) fn into_render_image(self) -> Result<Arc<RenderImage>, String> {
        let pixels =
            ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(self.width, self.height, self.pixels_bgra)
                .ok_or_else(|| "GPUI rejected the document raster buffer".to_owned())?;
        Ok(Arc::new(RenderImage::new(smallvec![Frame::new(pixels)])))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThumbnailSurface {
    pub page_index: u32,
    pub raster: RasterSurface,
}

impl ThumbnailSurface {
    pub fn new(page_index: u32, raster: RasterSurface) -> Self {
        Self { page_index, raster }
    }
}

pub trait NativeDocumentResource: Send + Sync {
    fn worker_pid(&self) -> Option<u32>;
    fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String>;
    fn render_page_with_pdf_annotations(
        &self,
        page_index: u32,
        width: u32,
    ) -> Result<RasterSurface, String> {
        self.render_page(page_index, width)
    }
    fn render_page_without_pdf_annotations(
        &self,
        page_index: u32,
        width: u32,
    ) -> Result<RasterSurface, String> {
        self.render_page(page_index, width)
    }
    fn page_snap_geometry(&self, page_index: u32) -> Result<PageSnapGeometry, String> {
        Ok(PageSnapGeometry {
            page_index,
            primitives: Vec::new(),
        })
    }
    fn render_tile(&self, request: TileRequest) -> Result<RasterSurface, String>;
    fn close(&self) -> Result<(), String>;
    fn is_released(&self) -> bool;
}

pub struct OpenedNativeDocument {
    pub(crate) title: String,
    pub(crate) page_sizes: Vec<(f32, f32)>,
    pub(crate) current_page: RasterSurface,
    pub(crate) thumbnails: Vec<ThumbnailSurface>,
    pub(crate) resource: Arc<dyn NativeDocumentResource>,
    pub(crate) annotations: Vec<Annotation>,
    pub(crate) retained_annotation_obstacles: Vec<RetainedAnnotationObstacle>,
    pub(crate) page_scales: Vec<PageScale>,
    pub(crate) scale_presets: Vec<ScalePreset>,
    pub(crate) page_length_calibrations: Vec<(u32, LengthCalibration)>,
    pub(crate) page_rotations: Vec<PageRotation>,
    pub(crate) page_coordinate_spaces: Vec<PageCoordinateSpace>,
    pub(crate) page_grid_definition: Option<PageGridDefinition>,
    pub(crate) source_sha256: Option<[u8; 32]>,
}

impl OpenedNativeDocument {
    pub fn new(
        title: impl Into<String>,
        page_sizes: Vec<(f32, f32)>,
        current_page: RasterSurface,
        thumbnails: Vec<ThumbnailSurface>,
        resource: Arc<dyn NativeDocumentResource>,
    ) -> Result<Self, String> {
        if page_sizes.is_empty() {
            return Err("a native document must contain at least one page".into());
        }
        if page_sizes.iter().any(|(width, height)| {
            !width.is_finite() || !height.is_finite() || *width <= 0. || *height <= 0.
        }) {
            return Err("native document page geometry is invalid".into());
        }
        if thumbnails.is_empty()
            || thumbnails
                .iter()
                .any(|thumbnail| thumbnail.page_index as usize >= page_sizes.len())
        {
            return Err("native document thumbnails do not identify real pages".into());
        }
        let page_rotations = vec![PageRotation::Degrees0; page_sizes.len()];
        let page_coordinate_spaces = page_sizes
            .iter()
            .map(|(width, height)| {
                PageCoordinateSpace::new(
                    CoordinateRect::new(0.0, 0.0, f64::from(*width), f64::from(*height))
                        .expect("validated page dimensions must form a box"),
                    CoordinateRect::new(0.0, 0.0, f64::from(*width), f64::from(*height))
                        .expect("validated page dimensions must form a box"),
                    CoordinateRotation::Degrees0,
                    1.0,
                )
                .expect("validated page dimensions must form a coordinate space")
            })
            .collect();
        Ok(Self {
            title: title.into(),
            page_sizes,
            current_page,
            thumbnails,
            resource,
            annotations: Vec::new(),
            retained_annotation_obstacles: Vec::new(),
            page_scales: Vec::new(),
            scale_presets: Vec::new(),
            page_length_calibrations: Vec::new(),
            page_rotations,
            page_coordinate_spaces,
            page_grid_definition: None,
            source_sha256: None,
        })
    }

    pub fn with_annotations(mut self, annotations: Vec<Annotation>) -> Self {
        self.annotations = annotations;
        self
    }

    pub fn with_retained_annotation_obstacles(
        mut self,
        obstacles: Vec<RetainedAnnotationObstacle>,
    ) -> Self {
        self.retained_annotation_obstacles = obstacles;
        self
    }

    pub fn with_page_length_calibrations(
        mut self,
        calibrations: Vec<(u32, LengthCalibration)>,
    ) -> Self {
        self.page_length_calibrations = calibrations;
        self
    }

    pub fn with_page_scales(mut self, page_scales: Vec<PageScale>) -> Self {
        self.page_scales = page_scales;
        self
    }

    pub fn with_scale_presets(mut self, scale_presets: Vec<ScalePreset>) -> Self {
        self.scale_presets = scale_presets;
        self
    }

    pub fn with_page_rotations(mut self, rotations: Vec<PageRotation>) -> Self {
        assert_eq!(
            rotations.len(),
            self.page_sizes.len(),
            "every opened page must have one stable rotation identity"
        );
        self.page_rotations = rotations;
        self
    }

    pub fn with_page_coordinate_spaces(mut self, spaces: Vec<PageCoordinateSpace>) -> Self {
        assert_eq!(
            spaces.len(),
            self.page_sizes.len(),
            "every opened page must have one stable coordinate-space identity"
        );
        self.page_coordinate_spaces = spaces;
        self
    }

    pub fn with_page_grid_definition(mut self, grid: Option<PageGridDefinition>) -> Self {
        self.page_grid_definition = grid;
        self
    }

    pub fn with_source_sha256(mut self, source_sha256: [u8; 32]) -> Self {
        self.source_sha256 = Some(source_sha256);
        self
    }

    pub fn page_count(&self) -> usize {
        self.page_sizes.len()
    }

    pub fn page_coordinate_space(&self, page_index: u32) -> Option<PageCoordinateSpace> {
        self.page_coordinate_spaces
            .get(page_index as usize)
            .copied()
    }

    pub fn page_coordinate_spaces(&self) -> &[PageCoordinateSpace] {
        &self.page_coordinate_spaces
    }

    pub fn current_page(&self) -> &RasterSurface {
        &self.current_page
    }

    pub fn thumbnails(&self) -> &[ThumbnailSurface] {
        &self.thumbnails
    }

    pub fn worker_pid(&self) -> Option<u32> {
        self.resource.worker_pid()
    }

    pub fn render_page(&self, page_index: u32, width: u32) -> Result<RasterSurface, String> {
        self.resource.render_page(page_index, width)
    }

    pub fn render_page_with_pdf_annotations(
        &self,
        page_index: u32,
        width: u32,
    ) -> Result<RasterSurface, String> {
        self.resource
            .render_page_with_pdf_annotations(page_index, width)
    }

    pub fn render_page_without_pdf_annotations(
        &self,
        page_index: u32,
        width: u32,
    ) -> Result<RasterSurface, String> {
        self.resource
            .render_page_without_pdf_annotations(page_index, width)
    }

    pub fn page_snap_geometry(&self, page_index: u32) -> Result<PageSnapGeometry, String> {
        self.resource.page_snap_geometry(page_index)
    }

    pub fn close(&self) -> Result<(), String> {
        self.resource.close()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenDocumentRequest {
    pub document_id: DocumentId,
    pub generation: u64,
    pub path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRenderRequest {
    pub document_id: DocumentId,
    pub generation: u64,
    pub page_index: u32,
    pub source_rotation: PageRotation,
    pub target_rotation: PageRotation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageRotationRequest {
    pub document_id: DocumentId,
    pub generation: u64,
    pub page_index: u32,
    pub source_rotation: PageRotation,
    pub target_rotation: PageRotation,
    pub document_revision: u64,
    pub resource_epoch: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageRotationPresentation {
    pub current_page: RasterSurface,
    pub thumbnail: RasterSurface,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DocumentRecoveryRequest {
    pub document_id: DocumentId,
    pub generation: u64,
    pub resource_epoch: u64,
    pub path: PathBuf,
    pub expected_source_sha256: Option<[u8; 32]>,
    pub current_page: u32,
    pub expected_page_sizes: Vec<(f32, f32)>,
    pub expected_source_rotations: Vec<PageRotation>,
    pub expected_source_coordinate_spaces: Vec<PageCoordinateSpace>,
    pub target_rotations: Vec<PageRotation>,
}

pub trait NativeDocumentOpener: Send + Sync {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String>;
}
pub struct PdfiumWorkerBackend {
    worker_executable: PathBuf,
    pdfium_library: PathBuf,
    surface_root: PathBuf,
}

impl PdfiumWorkerBackend {
    pub fn new(worker_executable: PathBuf, pdfium_library: PathBuf, surface_root: PathBuf) -> Self {
        Self {
            worker_executable,
            pdfium_library,
            surface_root,
        }
    }
}

impl NativeDocumentOpener for PdfiumWorkerBackend {
    fn open(&self, request: &OpenDocumentRequest) -> Result<OpenedNativeDocument, String> {
        let source_sha256: [u8; 32] =
            Sha256::digest(fs::read(&request.path).map_err(|error| error.to_string())?).into();
        let persistence = PdfPersistenceSession::open(&request.path)
            .map_err(|error| format!("failed to import PDF annotations: {error}"))?;
        let vector_snapshot_ids = persistence.vector_snapshot_ids();
        let mut annotations = persistence.annotations_in_document_order();
        let retained_annotation_obstacles = persistence.retained_annotation_obstacles().to_vec();
        let page_length_calibrations = persistence
            .page_length_calibrations()
            .iter()
            .map(|(page_index, calibration)| (*page_index, calibration.clone()))
            .collect();
        let page_scales = persistence.page_scales().to_vec();
        let page_rotations = persistence
            .page_rotations()
            .iter()
            .map(|(_, rotation)| *rotation)
            .collect();
        let page_grid_definition = persistence.page_grid_definition();
        let resource = PdfiumWorkerResource::open(
            &self.worker_executable,
            &self.pdfium_library,
            &self.surface_root,
            request,
        )?;
        // Revu vector Snapshots: the worker draws each Form for the canvas.
        // A failure keeps the placeholder; the saved file keeps the Form.
        for (layer_page, id) in vector_snapshot_ids.iter().enumerate() {
            let Some(Annotation::Snapshot(snapshot)) = annotations
                .iter_mut()
                .find(|annotation| annotation.id() == id)
            else {
                continue;
            };
            let Ok(layer_page) = u32::try_from(layer_page) else {
                break;
            };
            if let Ok(asset) = resource.render_vector_snapshot(layer_page, snapshot.rect) {
                *snapshot = snapshot.clone().with_asset(asset);
            }
        }
        let page_sizes = resource.page_sizes.clone();
        let page_coordinate_spaces = resource.page_coordinate_spaces.clone();
        let current_page = resource.render_page(0, DEFAULT_PAGE_RENDER_WIDTH)?;
        let mut thumbnails = Vec::new();
        for page_index in 0..u32::try_from(page_sizes.len().min(DEFAULT_THUMBNAIL_COUNT))
            .map_err(|_| "thumbnail count overflowed".to_owned())?
        {
            thumbnails.push(ThumbnailSurface::new(
                page_index,
                resource.render_page(page_index, DEFAULT_THUMBNAIL_WIDTH)?,
            ));
        }
        let title = request
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled PDF")
            .to_owned();
        OpenedNativeDocument::new(title, page_sizes, current_page, thumbnails, resource).map(
            |opened| {
                opened
                    .with_annotations(annotations)
                    .with_retained_annotation_obstacles(retained_annotation_obstacles)
                    .with_page_scales(page_scales)
                    .with_page_length_calibrations(page_length_calibrations)
                    .with_page_rotations(page_rotations)
                    .with_page_coordinate_spaces(page_coordinate_spaces)
                    .with_page_grid_definition(page_grid_definition)
                    .with_source_sha256(source_sha256)
            },
        )
    }
}

struct PdfiumWorkerResource {
    client: Mutex<Option<WorkerProcessClient>>,
    session_id: SessionId,
    page_sizes: Vec<(f32, f32)>,
    page_rotations: Vec<PageRotation>,
    page_coordinate_spaces: Vec<PageCoordinateSpace>,
    surface_root: PathBuf,
    worker_pid: u32,
    next_request: AtomicU64,
    next_job: AtomicU64,
    next_surface: AtomicU64,
    released: AtomicBool,
}

impl PdfiumWorkerResource {
    fn open(
        worker: &Path,
        library: &Path,
        root: &Path,
        request: &OpenDocumentRequest,
    ) -> Result<Arc<Self>, String> {
        let surface_root = root.join(format!(
            "worker-{}-{}-{}",
            std::process::id(),
            request.document_id.value(),
            request.generation
        ));
        let result: Result<Arc<Self>, String> = (|| {
            fs::create_dir_all(&surface_root).map_err(|error| error.to_string())?;
            let (mut client, source_handle_id) = WorkerProcessClient::spawn_with_inherited_source(
                worker,
                &surface_root,
                library,
                &request.path,
            )
            .map_err(worker_error)?;
            let worker_pid = client.child_id();
            let session_id = SessionId(request.document_id.value().max(1));
            let response = client
                .exchange(&WorkerRequest::Open {
                    request_id: RequestId(1),
                    session_id,
                    source_handle_id,
                    password: None,
                })
                .map_err(worker_error)?;
            let DocumentInfo { page_count, .. } = match response {
                WorkerResponse::Opened { document, .. } => document,
                WorkerResponse::Failed { error, .. } => return Err(worker_error(error)),
                response => return Err(unexpected_response("open", response)),
            };
            if page_count == 0 {
                return Err("PDFium reported a document with no pages".into());
            }
            let mut next_request = 2;
            let mut page_sizes = Vec::with_capacity(page_count as usize);
            let mut page_rotations = Vec::with_capacity(page_count as usize);
            let mut page_coordinate_spaces = Vec::with_capacity(page_count as usize);
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
                page_rotations.push(match geometry.rotation {
                    Rotation::Degrees0 => PageRotation::Degrees0,
                    Rotation::Degrees90 => PageRotation::Degrees90,
                    Rotation::Degrees180 => PageRotation::Degrees180,
                    Rotation::Degrees270 => PageRotation::Degrees270,
                });
                page_coordinate_spaces.push(coordinate_space_from_worker_geometry(&geometry)?);
                page_sizes.push(display_size(geometry)?);
            }
            Ok(Arc::new(Self {
                client: Mutex::new(Some(client)),
                session_id,
                page_sizes,
                page_rotations,
                page_coordinate_spaces,
                surface_root: surface_root.clone(),
                worker_pid,
                next_request: AtomicU64::new(next_request),
                next_job: AtomicU64::new(1),
                next_surface: AtomicU64::new(1),
                released: AtomicBool::new(false),
            }))
        })();
        if result.is_err() {
            let _ = fs::remove_dir_all(&surface_root);
        }
        result
    }

    fn render_page_with_policy(
        &self,
        page_index: u32,
        desired_width: u32,
        annotation_mode: AnnotationRenderMode,
    ) -> Result<RasterSurface, String> {
        let (page_width, page_height) = *self
            .page_sizes
            .get(page_index as usize)
            .ok_or_else(|| "PDF page is outside the document".to_owned())?;
        let coordinate_space = self
            .page_coordinate_spaces
            .get(page_index as usize)
            .copied()
            .ok_or_else(|| "PDF page coordinate space is unavailable".to_owned())?;
        let width = desired_width.clamp(1, 8_192);
        let height = ((width as f64 * f64::from(page_height) / f64::from(page_width)).ceil()
            as u32)
            .clamp(1, 8_192);
        let byte_len = u64::from(width) * u64::from(height) * 4;
        let surface_id = SurfaceId(self.next_surface.fetch_add(1, Ordering::Relaxed));
        let descriptor = SurfaceDescriptor {
            surface_id,
            width,
            height,
            stride: width * 4,
            byte_len,
            format: SurfaceFormat::Bgra8Premultiplied,
        };
        let mut guard = self
            .client
            .lock()
            .map_err(|_| "PDF worker client lock was poisoned".to_owned())?;
        let client = guard
            .as_mut()
            .ok_or_else(|| "PDF worker resource is released".to_owned())?;
        let surface = client.create_surface(&descriptor).map_err(worker_error)?;
        let job_id = JobId(self.next_job.fetch_add(1, Ordering::Relaxed));
        let pdf_scale_x = width as f32 / page_width * coordinate_space.user_unit() as f32;
        let pdf_scale_y = height as f32 / page_height * coordinate_space.user_unit() as f32;
        let view_box = coordinate_space.view_box();
        let response = client
            .exchange(&WorkerRequest::RenderCrop {
                request_id: self.request_id(),
                render: RenderRequest {
                    job_id,
                    session_id: self.session_id,
                    page_index,
                    annotation_mode,
                    transform: [
                        pdf_scale_x,
                        0.,
                        0.,
                        pdf_scale_y,
                        -(view_box.x as f32) * pdf_scale_x,
                        -(view_box.y as f32) * pdf_scale_y,
                    ],
                    clip: ClipRect {
                        x: 0,
                        y: 0,
                        width,
                        height,
                    },
                    surface: descriptor,
                },
            })
            .map_err(worker_error)?;
        match response {
            WorkerResponse::Rendered {
                job_id: actual_job,
                surface_id: actual_surface,
                ..
            } if actual_job == job_id && actual_surface == surface_id => {}
            WorkerResponse::Failed { error, .. } => return Err(worker_error(error)),
            response => return Err(unexpected_response("render", response)),
        }
        RasterSurface::new(width, height, surface.pixels().to_vec())
    }

    /// Rasterises page `layer_page` of the vector Snapshot layer, whose page
    /// is the Snapshot's unrotated `rect`, at up to four pixels per point.
    fn render_vector_snapshot(
        &self,
        layer_page: u32,
        rect: PdfRect,
    ) -> Result<DecodedRgbaAsset, String> {
        let scale = (2_048. / rect.width.max(rect.height)).min(4.).max(0.01);
        let width = ((rect.width * scale).round() as u32).clamp(1, 8_192);
        let height = ((rect.height * scale).round() as u32).clamp(1, 8_192);
        let byte_len = u64::from(width) * u64::from(height) * 4;
        let surface_id = SurfaceId(self.next_surface.fetch_add(1, Ordering::Relaxed));
        let descriptor = SurfaceDescriptor {
            surface_id,
            width,
            height,
            stride: width * 4,
            byte_len,
            format: SurfaceFormat::Bgra8Premultiplied,
        };
        let mut guard = self
            .client
            .lock()
            .map_err(|_| "PDF worker client lock was poisoned".to_owned())?;
        let client = guard
            .as_mut()
            .ok_or_else(|| "PDF worker resource is released".to_owned())?;
        let surface = client.create_surface(&descriptor).map_err(worker_error)?;
        let job_id = JobId(self.next_job.fetch_add(1, Ordering::Relaxed));
        let scale_x = width as f32 / rect.width as f32;
        let scale_y = height as f32 / rect.height as f32;
        let response = client
            .exchange(&WorkerRequest::RenderCrop {
                request_id: self.request_id(),
                render: RenderRequest {
                    job_id,
                    session_id: self.session_id,
                    page_index: layer_page,
                    annotation_mode: AnnotationRenderMode::VectorSnapshots,
                    transform: [scale_x, 0., 0., scale_y, 0., 0.],
                    clip: ClipRect {
                        x: 0,
                        y: 0,
                        width,
                        height,
                    },
                    surface: descriptor,
                },
            })
            .map_err(worker_error)?;
        match response {
            WorkerResponse::Rendered {
                job_id: actual_job,
                surface_id: actual_surface,
                ..
            } if actual_job == job_id && actual_surface == surface_id => {}
            WorkerResponse::Failed { error, .. } => return Err(worker_error(error)),
            response => return Err(unexpected_response("vector Snapshot render", response)),
        }
        RasterSurface::new(width, height, surface.pixels().to_vec())?.to_rgba_asset()
    }

    fn request_id(&self) -> RequestId {
        RequestId(self.next_request.fetch_add(1, Ordering::Relaxed))
    }

    fn finish_release(&self) -> Result<(), String> {
        match fs::remove_dir_all(&self.surface_root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to remove PDF surface directory: {error}")),
        }
        self.released.store(true, Ordering::Release);
        Ok(())
    }
}

impl NativeDocumentResource for PdfiumWorkerResource {
    fn worker_pid(&self) -> Option<u32> {
        Some(self.worker_pid)
    }

    fn render_page(&self, page_index: u32, desired_width: u32) -> Result<RasterSurface, String> {
        self.render_page_with_policy(
            page_index,
            desired_width,
            AnnotationRenderMode::RetainedOnly,
        )
    }

    fn render_page_with_pdf_annotations(
        &self,
        page_index: u32,
        desired_width: u32,
    ) -> Result<RasterSurface, String> {
        self.render_page_with_policy(page_index, desired_width, AnnotationRenderMode::All)
    }

    fn render_page_without_pdf_annotations(
        &self,
        page_index: u32,
        desired_width: u32,
    ) -> Result<RasterSurface, String> {
        self.render_page_with_policy(page_index, desired_width, AnnotationRenderMode::None)
    }

    fn page_snap_geometry(&self, page_index: u32) -> Result<PageSnapGeometry, String> {
        if page_index as usize >= self.page_sizes.len() {
            return Err("PDF page is outside the document".to_owned());
        }
        let mut guard = self
            .client
            .lock()
            .map_err(|_| "PDF worker client lock was poisoned".to_owned())?;
        let client = guard
            .as_mut()
            .ok_or_else(|| "PDF worker resource is released".to_owned())?;
        match client
            .exchange(&WorkerRequest::PageSnapGeometry {
                request_id: self.request_id(),
                session_id: self.session_id,
                page_index,
            })
            .map_err(worker_error)?
        {
            WorkerResponse::PageSnapGeometry {
                session_id,
                page_index: returned_page,
                geometry,
                ..
            } if session_id == self.session_id
                && returned_page == page_index
                && geometry.page_index == page_index =>
            {
                Ok(geometry)
            }
            WorkerResponse::Failed { error, .. } => Err(worker_error(error)),
            response => Err(unexpected_response("page snap geometry", response)),
        }
    }

    fn render_tile(&self, request: TileRequest) -> Result<RasterSurface, String> {
        let page_index = u32::try_from(request.page)
            .map_err(|_| "viewer tile page index overflowed".to_owned())?;
        let target_rotation =
            PageRotation::from_degrees(i64::from(request.rotation_quarter_turns % 4) * 90)
                .map_err(|error| error.to_string())?;
        let delta = target_rotation.delta_from(self.page_rotations[request.page]);
        if delta != PageRotation::Degrees0 {
            let scale =
                request.zoom_tenths as f32 / 1_000. * request.device_scale_millis as f32 / 1_000.;
            let source_width = (self.page_sizes[request.page].0 * scale).ceil().max(1.) as u32;
            let full = self
                .render_page_with_policy(
                    page_index,
                    source_width,
                    AnnotationRenderMode::RetainedOnly,
                )?
                .rotated(delta)?;
            return full.cropped(
                u32::try_from(request.crop.x)
                    .map_err(|_| "viewer tile crop x overflowed".to_owned())?,
                u32::try_from(request.crop.y)
                    .map_err(|_| "viewer tile crop y overflowed".to_owned())?,
                u32::try_from(request.crop.width)
                    .map_err(|_| "viewer tile crop width overflowed".to_owned())?,
                u32::try_from(request.crop.height)
                    .map_err(|_| "viewer tile crop height overflowed".to_owned())?,
            );
        }
        let _page_size = *self
            .page_sizes
            .get(request.page)
            .ok_or_else(|| "viewer tile page is outside the document".to_owned())?;
        let coordinate_space = self
            .page_coordinate_spaces
            .get(request.page)
            .copied()
            .ok_or_else(|| "viewer tile page coordinate space is unavailable".to_owned())?;
        let width = u32::try_from(request.crop.width)
            .map_err(|_| "viewer tile width overflowed".to_owned())?;
        let height = u32::try_from(request.crop.height)
            .map_err(|_| "viewer tile height overflowed".to_owned())?;
        if width == 0 || height == 0 || width > 8_192 || height > 8_192 {
            return Err("viewer tile dimensions are outside the worker limit".into());
        }
        let zoom = request.zoom_tenths as f32 / 1_000.;
        let device_scale = request.device_scale_millis as f32 / 1_000.;
        let scale = zoom * device_scale;
        if !scale.is_finite() || scale <= 0. {
            return Err("viewer tile scale is invalid".into());
        }
        let pdf_scale_x = scale * coordinate_space.user_unit() as f32;
        let pdf_scale_y = scale * coordinate_space.user_unit() as f32;
        let view_box = coordinate_space.view_box();
        let byte_len = u64::from(width) * u64::from(height) * 4;
        let surface_id = SurfaceId(self.next_surface.fetch_add(1, Ordering::Relaxed));
        let descriptor = SurfaceDescriptor {
            surface_id,
            width,
            height,
            stride: width * 4,
            byte_len,
            format: SurfaceFormat::Bgra8Premultiplied,
        };
        let mut guard = self
            .client
            .lock()
            .map_err(|_| "PDF worker client lock was poisoned".to_owned())?;
        let client = guard
            .as_mut()
            .ok_or_else(|| "PDF worker resource is released".to_owned())?;
        let surface = client.create_surface(&descriptor).map_err(worker_error)?;
        let job_id = JobId(self.next_job.fetch_add(1, Ordering::Relaxed));
        let response = client
            .exchange(&WorkerRequest::RenderCrop {
                request_id: self.request_id(),
                render: RenderRequest {
                    job_id,
                    session_id: self.session_id,
                    page_index,
                    annotation_mode: AnnotationRenderMode::RetainedOnly,
                    transform: [
                        pdf_scale_x,
                        0.,
                        0.,
                        pdf_scale_y,
                        -(view_box.x as f32) * pdf_scale_x - request.crop.x as f32,
                        -(view_box.y as f32) * pdf_scale_y - request.crop.y as f32,
                    ],
                    clip: ClipRect {
                        // The worker protocol defines the clip in output-surface
                        // coordinates. The page-space crop offset is already
                        // represented by the translation above, and this surface
                        // contains only the requested tile.
                        x: 0,
                        y: 0,
                        width,
                        height,
                    },
                    surface: descriptor,
                },
            })
            .map_err(worker_error)?;
        match response {
            WorkerResponse::Rendered {
                job_id: actual_job,
                surface_id: actual_surface,
                ..
            } if actual_job == job_id && actual_surface == surface_id => {}
            WorkerResponse::Failed { error, .. } => return Err(worker_error(error)),
            response => return Err(unexpected_response("tile render", response)),
        }
        RasterSurface::new(width, height, surface.pixels().to_vec())
    }

    fn close(&self) -> Result<(), String> {
        if self.released.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut client_slot = match self.client.lock() {
            Ok(client) => client,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(mut client) = client_slot.take() {
            let lifecycle_measurement_requested = client.lifecycle_measurement_requested();
            let close_response = client.exchange(&WorkerRequest::Close {
                request_id: self.request_id(),
                session_id: self.session_id,
            });
            let close_result = match close_response {
                Ok(response) => {
                    if matches!(response, WorkerResponse::Closed { .. }) {
                        Ok(())
                    } else {
                        Err(unexpected_response("close", response))
                    }
                }
                Err(error) if error.code == WorkerErrorCode::WorkerCrashed => {
                    // The child is already terminal. Dropping the owned client
                    // kills if necessary and always waits before surface cleanup.
                    drop(client);
                    return self.finish_release();
                }
                Err(error) => Err(worker_error(error)),
            };
            if let Err(error) = close_result {
                *client_slot = Some(client);
                return Err(error);
            }
            let lifecycle_result = if lifecycle_measurement_requested {
                client
                    .finish_and_publish_lifecycle_measurement()
                    .map(|_| ())
                    .map_err(worker_error)
            } else {
                drop(client);
                Ok(())
            };
            let release_result = self.finish_release();
            return match lifecycle_result {
                Ok(()) => release_result,
                Err(error) => match release_result {
                    Ok(()) => Err(error),
                    Err(release_error) => Err(format!(
                        "{error}; worker surface cleanup also failed: {release_error}"
                    )),
                },
            };
        }
        self.finish_release()
    }

    fn is_released(&self) -> bool {
        self.released.load(Ordering::Acquire)
    }
}

impl Drop for PdfiumWorkerResource {
    fn drop(&mut self) {
        if self.close().is_err() {
            let client_slot = match self.client.get_mut() {
                Ok(client) => client,
                Err(poisoned) => poisoned.into_inner(),
            };
            drop(client_slot.take());
            let _ = fs::remove_dir_all(&self.surface_root);
            self.released.store(true, Ordering::Release);
        }
    }
}
fn display_size(geometry: PageGeometry) -> Result<(f32, f32), String> {
    let size = (
        geometry.display_width_points,
        geometry.display_height_points,
    );
    if !size.0.is_finite() || !size.1.is_finite() || size.0 <= 0. || size.1 <= 0. {
        return Err("PDFium reported invalid page geometry".into());
    }
    Ok(size)
}

fn coordinate_space_from_worker_geometry(
    geometry: &PageGeometry,
) -> Result<PageCoordinateSpace, String> {
    let rect_from_edges = |values: [f32; 4], name: &str| {
        let [left, bottom, right, top] = values.map(f64::from);
        CoordinateRect::new(left, bottom, right - left, top - bottom)
            .map_err(|error| format!("invalid {name}: {error}"))
    };
    let media_box = rect_from_edges(geometry.media_box, "PDF media box")?;
    let crop_box = rect_from_edges(geometry.crop_box, "PDF crop box")?;
    let rotation = match geometry.rotation {
        Rotation::Degrees0 => CoordinateRotation::Degrees0,
        Rotation::Degrees90 => CoordinateRotation::Degrees90,
        Rotation::Degrees180 => CoordinateRotation::Degrees180,
        Rotation::Degrees270 => CoordinateRotation::Degrees270,
    };
    PageCoordinateSpace::new(media_box, crop_box, rotation, f64::from(geometry.user_unit))
        .map_err(|error| error.to_string())
}

pub(crate) fn coordinate_rotation(rotation: PageRotation) -> CoordinateRotation {
    match rotation {
        PageRotation::Degrees0 => CoordinateRotation::Degrees0,
        PageRotation::Degrees90 => CoordinateRotation::Degrees90,
        PageRotation::Degrees180 => CoordinateRotation::Degrees180,
        PageRotation::Degrees270 => CoordinateRotation::Degrees270,
    }
}

fn worker_error(error: WorkerError) -> String {
    format!(
        "PDF worker {:?}: {}",
        error.code,
        error.detail.unwrap_or_default()
    )
}

fn unexpected_response(operation: &str, response: WorkerResponse) -> String {
    format!("PDF worker returned an unexpected response for {operation}: {response:?}")
}

#[cfg(test)]
mod raster_colour_tests {
    #[test]
    #[ignore = "requires the checksum-pinned development PDFium library and the worker binary"]
    fn opening_a_revu_vector_snapshot_rasterises_its_form_for_the_canvas() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let worker = std::env::current_exe()
            .unwrap()
            .parent()
            .and_then(|deps| deps.parent())
            .unwrap()
            .join(if cfg!(windows) {
                "butter-paper-pdf-worker.exe"
            } else {
                "butter-paper-pdf-worker"
            });
        let library = std::path::PathBuf::from(
            std::env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY names libpdfium"),
        );
        let surfaces = std::env::temp_dir()
            .join(format!("bp-vector-snapshot-surfaces-{}", std::process::id()));
        std::fs::create_dir_all(&surfaces).unwrap();
        let backend = PdfiumWorkerBackend::new(worker, library, surfaces.clone());
        let opened = backend
            .open(&OpenDocumentRequest {
                document_id: DocumentId::new(1),
                generation: 1,
                path: manifest_dir.join("tests/fixtures/bluebeam/revu-measure-text-media.pdf"),
            })
            .unwrap();
        let snapshot = opened
            .annotations
            .iter()
            .find_map(|annotation| match annotation {
                Annotation::Snapshot(snapshot) => Some(snapshot.clone()),
                _ => None,
            })
            .expect("Revu's vector Snapshot imports as a Snapshot");
        let asset = snapshot.asset();
        assert!(asset.width_px() > 100 && asset.height_px() > 100, "{asset:?}");
        let pixels = asset.rgba().chunks_exact(4).collect::<Vec<_>>();
        // Revu snapshotted a red Area markup: red strokes on a clear ground.
        assert!(pixels.iter().any(|pixel| pixel[0] > 200 && pixel[1] < 80 && pixel[2] < 80 && pixel[3] > 200));
        assert!(pixels.iter().any(|pixel| pixel[3] == 0));
        opened.resource.close().unwrap();
        std::fs::remove_dir_all(&surfaces).ok();
    }

    use super::*;
    use crate::annotation_model::{MarkupId, PdfPoint, PenAppearance};

    #[test]
    fn bgra_highlights_and_snapshot_preserve_asymmetric_colour_and_alpha() {
        let rect = CoordinateRect::new(0., 0., 20., 20.).unwrap();
        let space = PageCoordinateSpace::new(rect, rect, CoordinateRotation::Degrees0, 1.).unwrap();
        for (colour, expected) in [
            ("#ff0000", [20, 40, 100, 128]),
            ("#0000ff", [40, 40, 50, 128]),
            ("#ffff00", [20, 80, 100, 128]),
        ] {
            let mut raster = RasterSurface::new(20, 20, [40, 80, 100, 128].repeat(400)).unwrap();
            let highlight = PenAnnotation::new_highlight(
                MarkupId::new("colour:highlight").unwrap(),
                0,
                vec![
                    PdfPoint::new(2., 10.).unwrap(),
                    PdfPoint::new(18., 10.).unwrap(),
                ],
                PenAppearance::new(colour, 8., 0.5).unwrap(),
            )
            .unwrap();
            assert!(
                raster
                    .precompose_highlights(0, space, 0., 0., (20., 20.), &[highlight])
                    .unwrap()
                    > 0
            );
            let offset = (10 * 20 + 10) * 4;
            assert_eq!(
                &raster.pixels_bgra()[offset..offset + 4],
                &expected,
                "{colour}"
            );
            let asset = raster
                .snapshot_asset(PdfRect::new(0., 0., 20., 20.).unwrap(), space)
                .unwrap();
            let unpremultiply = |value: u8| ((u32::from(value) * 255 + 64) / 128) as u8;
            assert_eq!(
                &asset.rgba()[offset..offset + 4],
                &[
                    unpremultiply(expected[2]),
                    unpremultiply(expected[1]),
                    unpremultiply(expected[0]),
                    128
                ]
            );
        }
    }
}
