use butter_paper_gpui_gallery::pdf_worker::{
    CancellationRegistry, DocumentInfo, FileSurfaceStore, PageGeometry, PdfBackend, RenderRequest,
    Rotation, SourceHandleId, SurfaceLimits, WorkerError, WorkerErrorCode, WorkerState,
    run_worker_protocol,
};
use butter_paper_gpui_gallery::page_geometry::{PageCoordinateSpace, PdfMetadataDocument};
use pdfium_render::prelude::*;
use std::env;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// The only module that exposes PDFium types. Nothing in the application-facing
/// protocol or client depends on this adapter's supplier API.
struct PdfiumBackend {
    pdfium: &'static Pdfium,
}

struct PdfiumDocument {
    document: PdfDocument<'static>,
    metadata: PdfMetadataDocument,
}

impl PdfiumBackend {
    fn bind(library_path: &Path) -> Result<Self, WorkerError> {
        let bindings = Pdfium::bind_to_library(library_path).map_err(map_pdfium_error)?;
        let pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
        Ok(Self { pdfium })
    }
}

impl PdfBackend for PdfiumBackend {
    type Document = PdfiumDocument;

    fn open(
        &mut self,
        source: SourceHandleId,
        password: Option<&str>,
    ) -> Result<(Self::Document, DocumentInfo), WorkerError> {
        let mut source = duplicate_inherited_source(source)?;
        let mut metadata_bytes = Vec::new();
        source.read_to_end(&mut metadata_bytes).map_err(WorkerError::from)?;
        source
            .seek(SeekFrom::Start(0))
            .map_err(WorkerError::from)?;
        let metadata = PdfMetadataDocument::load_mem(&metadata_bytes).map_err(|error| {
            WorkerError::with_detail(
                WorkerErrorCode::MalformedDocument,
                format!("page metadata parser rejected the PDF: {error}"),
            )
        })?;
        let document = self
            .pdfium
            .load_pdf_from_reader(source, password)
            .map_err(|error| {
                let mapped = map_pdfium_error(error);
                if password.is_none() && mapped.code == WorkerErrorCode::BadPassword {
                    WorkerError::with_detail(
                        WorkerErrorCode::PasswordRequired,
                        mapped
                            .detail
                            .unwrap_or_else(|| "password required".to_owned()),
                    )
                } else {
                    mapped
                }
            })?;
        let page_count = u32::try_from(document.pages().len())
            .map_err(|_| WorkerError::new(WorkerErrorCode::LimitExceeded))?;
        Ok((
            PdfiumDocument { document, metadata },
            DocumentInfo {
                page_count,
                // PDFium does not expose a reliable repaired-document signal at
                // this wrapper seam. A future audited build must wire its parser
                // diagnostics before this can become true.
                repaired: false,
            },
        ))
    }

    fn page_geometry(
        &mut self,
        document: &mut Self::Document,
        page_index: u32,
    ) -> Result<PageGeometry, WorkerError> {
        let page_index =
            i32::try_from(page_index).map_err(|_| WorkerError::new(WorkerErrorCode::PageError))?;
        let page_number = u32::try_from(page_index)
            .ok()
            .and_then(|index| index.checked_add(1))
            .ok_or_else(|| WorkerError::new(WorkerErrorCode::PageError))?;
        let page_id = *document
            .metadata
            .get_pages()
            .get(&page_number)
            .ok_or_else(|| {
                WorkerError::with_detail(
                    WorkerErrorCode::PageError,
                    format!("PDF metadata has no page {page_number}"),
                )
            })?;
        let canonical = PageCoordinateSpace::from_lopdf_page(&document.metadata, page_id)
            .map_err(|error| {
                WorkerError::with_detail(WorkerErrorCode::MalformedDocument, error.to_string())
            })?;
        let page = document
            .document
            .pages()
            .get(page_index)
            .map_err(map_pdfium_error)?;
        let media = page.boundaries().media().map_err(map_pdfium_error)?.bounds;
        let crop = page
            .boundaries()
            .crop()
            .map(|boundary| boundary.bounds)
            .unwrap_or(media);
        let rotation = match page.rotation().map_err(map_pdfium_error)? {
            PdfPageRenderRotation::None => Rotation::Degrees0,
            PdfPageRenderRotation::Degrees90 => Rotation::Degrees90,
            PdfPageRenderRotation::Degrees180 => Rotation::Degrees180,
            PdfPageRenderRotation::Degrees270 => Rotation::Degrees270,
        };
        let pdfium_media = rect_array(media);
        let pdfium_crop = rect_array(crop);
        if !rectangles_match(pdfium_media, canonical.media_box())
            || !rectangles_match(pdfium_crop, canonical.view_box())
            || rotation != map_rotation(canonical.rotation())
        {
            return Err(WorkerError::with_detail(
                WorkerErrorCode::MalformedDocument,
                "PDFium page boundaries disagree with the inherited PDF page dictionary",
            ));
        }
        let (display_width_points, display_height_points) = canonical.display_size_points();
        Ok(PageGeometry {
            media_box: pdfium_media,
            crop_box: pdfium_crop,
            rotation,
            display_width_points: display_width_points as f32,
            display_height_points: display_height_points as f32,
            user_unit: canonical.user_unit() as f32,
        })
    }

    fn render_crop(
        &mut self,
        document: &mut Self::Document,
        request: &RenderRequest,
        output: &mut [u8],
        cancelled: &AtomicBool,
    ) -> Result<(), WorkerError> {
        if cancelled.load(Ordering::Acquire) {
            return Err(WorkerError::new(WorkerErrorCode::Cancelled));
        }
        let page_index = i32::try_from(request.page_index)
            .map_err(|_| WorkerError::new(WorkerErrorCode::PageError))?;
        let page = document
            .document
            .pages()
            .get(page_index)
            .map_err(map_pdfium_error)?;
        let width = i32::try_from(request.surface.width)
            .map_err(|_| WorkerError::new(WorkerErrorCode::LimitExceeded))?;
        let height = i32::try_from(request.surface.height)
            .map_err(|_| WorkerError::new(WorkerErrorCode::LimitExceeded))?;
        let mut bitmap = PdfBitmap::from_bytes(width, height, PdfBitmapFormat::BGRA, output)
            .map_err(map_pdfium_error)?;
        let [a, b, c, d, e, f] = request.transform;
        let config = PdfRenderConfig::new()
            .set_fixed_size(width, height)
            .render_form_data(false)
            .render_annotations(request.include_pdf_annotations)
            .reset_matrix(PdfMatrix::new(a, b, c, d, e, f))
            .map_err(map_pdfium_error)?
            .clip(0, 0, width, height);
        page.render_into_bitmap_with_config(&mut bitmap, &config)
            .map_err(map_pdfium_error)?;
        if cancelled.load(Ordering::Acquire) {
            // PDFium's stable bitmap API is not progressively cancellable. The
            // result is discarded if cancellation arrived during the native call.
            return Err(WorkerError::new(WorkerErrorCode::Cancelled));
        }
        Ok(())
    }

    fn close(&mut self, document: Self::Document) {
        drop(document);
    }
}

fn map_rotation(rotation: butter_paper_gpui_gallery::page_geometry::Rotation) -> Rotation {
    match rotation {
        butter_paper_gpui_gallery::page_geometry::Rotation::Degrees0 => Rotation::Degrees0,
        butter_paper_gpui_gallery::page_geometry::Rotation::Degrees90 => Rotation::Degrees90,
        butter_paper_gpui_gallery::page_geometry::Rotation::Degrees180 => Rotation::Degrees180,
        butter_paper_gpui_gallery::page_geometry::Rotation::Degrees270 => Rotation::Degrees270,
    }
}

fn rectangles_match(pdfium: [f32; 4], canonical: butter_paper_gpui_gallery::page_geometry::PdfRect) -> bool {
    let expected = [
        canonical.x as f32,
        canonical.y as f32,
        canonical.right() as f32,
        canonical.top() as f32,
    ];
    pdfium
        .into_iter()
        .zip(expected)
        .all(|(actual, expected)| (actual - expected).abs() <= 0.01)
}

fn rect_array(rect: PdfRect) -> [f32; 4] {
    [
        rect.left().value,
        rect.bottom().value,
        rect.right().value,
        rect.top().value,
    ]
}

#[cfg(unix)]
fn duplicate_inherited_source(source: SourceHandleId) -> Result<File, WorkerError> {
    use std::os::fd::FromRawFd;

    let raw = i32::try_from(source.0).map_err(|_| {
        WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            "invalid inherited file descriptor",
        )
    })?;
    // SAFETY: `dup` creates a new owned descriptor. The returned File owns only
    // the duplicate, not the descriptor inherited from the parent process.
    let duplicate = unsafe { libc::dup(raw) };
    if duplicate < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: `duplicate` is a fresh descriptor returned by `dup` above.
    Ok(unsafe { File::from_raw_fd(duplicate) })
}

#[cfg(not(unix))]
fn duplicate_inherited_source(_source: SourceHandleId) -> Result<File, WorkerError> {
    Err(WorkerError::with_detail(
        WorkerErrorCode::BackendUnavailable,
        "inherited source-handle duplication is not implemented on this platform",
    ))
}

fn map_pdfium_error(error: PdfiumError) -> WorkerError {
    let code = match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            WorkerErrorCode::BadPassword
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            WorkerErrorCode::UnsupportedSecurity
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PageError)
        | PdfiumError::PageIndexOutOfBounds => WorkerErrorCode::PageError,
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError)
        | PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            WorkerErrorCode::MalformedDocument
        }
        PdfiumError::LoadLibraryError(_) | PdfiumError::LoadLibraryFunctionNameError(_) => {
            WorkerErrorCode::BackendUnavailable
        }
        _ => WorkerErrorCode::PageError,
    };
    WorkerError::with_detail(code, error.to_string())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let library_path = env::var_os("BP_PDFIUM_LIBRARY")
        .ok_or("BP_PDFIUM_LIBRARY must name the exact packaged PDFium library")?;
    let surface_root = env::var_os("BP_PDF_WORKER_SURFACE_ROOT")
        .ok_or("BP_PDF_WORKER_SURFACE_ROOT must name the controlled mapping directory")?;
    let cancellation = CancellationRegistry::default();
    let state = WorkerState::new(
        PdfiumBackend::bind(Path::new(&library_path))?,
        FileSurfaceStore::new(surface_root),
        SurfaceLimits::default(),
        cancellation.clone(),
    );
    run_worker_protocol(
        std::io::stdin(),
        std::io::stdout().lock(),
        state,
        cancellation,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use butter_paper_gpui_gallery::pdf_worker::{
        ClipRect, JobId, RenderRequest, SessionId, SurfaceDescriptor, SurfaceFormat, SurfaceId,
    };
    use std::os::fd::AsRawFd;

    #[test]
    #[ignore = "set BP_PDFIUM_LIBRARY and BP_PDFIUM_PUBLIC_TEST_PDF to opt in"]
    fn renders_public_pdf_with_pinned_development_library() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let pdf_path = env::var_os("BP_PDFIUM_PUBLIC_TEST_PDF").expect("BP_PDFIUM_PUBLIC_TEST_PDF");
        let source = File::open(pdf_path).unwrap();
        let mut backend = PdfiumBackend::bind(Path::new(&library)).unwrap();
        let (mut document, info) = backend
            .open(SourceHandleId(source.as_raw_fd() as u64), None)
            .unwrap();
        assert!(info.page_count > 0);
        let descriptor = SurfaceDescriptor {
            surface_id: SurfaceId(1),
            width: 256,
            height: 256,
            stride: 1024,
            byte_len: 256 * 256 * 4,
            format: SurfaceFormat::Bgra8Premultiplied,
        };
        let request = RenderRequest {
            job_id: JobId(1),
            session_id: SessionId(1),
            page_index: 0,
            include_pdf_annotations: false,
            transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            clip: ClipRect {
                x: 0,
                y: 0,
                width: 256,
                height: 256,
            },
            surface: descriptor,
        };
        let mut pixels = vec![0; request.surface.byte_len as usize];
        backend
            .render_crop(
                &mut document,
                &request,
                &mut pixels,
                &AtomicBool::new(false),
            )
            .unwrap();
        assert!(pixels.iter().any(|value| *value != 0));
    }
}
