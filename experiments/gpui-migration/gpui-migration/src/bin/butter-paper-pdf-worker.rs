use butter_paper_gpui_migration::page_geometry::{PageCoordinateSpace, PdfMetadataDocument};
use butter_paper_gpui_migration::pdf_content_geometry::{
    ContentGeometryError, ContentGeometryLimits, PageSnapGeometry, extract_page_snap_geometry,
};
use butter_paper_gpui_migration::pdf_engine::{
    RetainedAnnotationRender, pdfium_display_render_bytes, retained_annotation_render,
    vector_snapshot_layer,
};
use butter_paper_gpui_migration::pdf_worker::{
    AnnotationRenderMode, CancellationRegistry, DocumentInfo, FileSurfaceStore, PageGeometry,
    PdfBackend, RenderRequest, Rotation, SourceHandleId, SurfaceLimits, WorkerError,
    WorkerErrorCode, WorkerLifecycleReporter, WorkerState, run_worker_protocol,
};
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

fn prepare_pdfium_display_bytes(
    metadata: &PdfMetadataDocument,
    bytes: Vec<u8>,
) -> Result<Vec<u8>, WorkerError> {
    pdfium_display_render_bytes(metadata, bytes).map_err(|error| {
        WorkerError::with_detail(
            WorkerErrorCode::MalformedDocument,
            format!("PDFium display preparation failed: {error}"),
        )
    })
}

struct PdfiumDocument {
    document: PdfDocument<'static>,
    metadata: PdfMetadataDocument,
    retained: RetainedPdfiumDocument,
    vector_snapshots: Option<Result<PdfDocument<'static>, WorkerError>>,
}

enum RetainedPdfiumDocument {
    Unprepared,
    None,
    Original,
    Filtered(PdfDocument<'static>),
    Failed(WorkerError),
}

impl PdfiumBackend {
    fn bind(library_path: &Path) -> Result<Self, WorkerError> {
        let bindings = Pdfium::bind_to_library(library_path).map_err(map_pdfium_error)?;
        let pdfium = Box::leak(Box::new(Pdfium::new(bindings)));
        Ok(Self { pdfium })
    }

    fn load_display_document(
        &self,
        metadata: &PdfMetadataDocument,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<PdfDocument<'static>, WorkerError> {
        let bytes = prepare_pdfium_display_bytes(metadata, bytes)?;
        self.pdfium
            .load_pdf_from_byte_vec(bytes, password)
            .map_err(map_pdfium_error)
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
        source.seek(SeekFrom::Start(0)).map_err(WorkerError::from)?;
        let mut metadata_bytes = Vec::new();
        source
            .read_to_end(&mut metadata_bytes)
            .map_err(WorkerError::from)?;
        let metadata = PdfMetadataDocument::load_mem(&metadata_bytes).map_err(|error| {
            WorkerError::with_detail(
                WorkerErrorCode::MalformedDocument,
                format!("page metadata parser rejected the PDF: {error}"),
            )
        })?;
        let document = self
            .load_display_document(&metadata, metadata_bytes, password)
            .map_err(|error| {
                if password.is_none() && error.code == WorkerErrorCode::BadPassword {
                    WorkerError::with_detail(
                        WorkerErrorCode::PasswordRequired,
                        error
                            .detail
                            .unwrap_or_else(|| "password required".to_owned()),
                    )
                } else {
                    error
                }
            })?;
        let page_count = u32::try_from(document.pages().len())
            .map_err(|_| WorkerError::new(WorkerErrorCode::LimitExceeded))?;
        Ok((
            PdfiumDocument {
                document,
                metadata,
                retained: RetainedPdfiumDocument::Unprepared,
                vector_snapshots: None,
            },
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
        let canonical =
            PageCoordinateSpace::from_lopdf_page(&document.metadata, page_id).map_err(|error| {
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

    fn page_snap_geometry(
        &mut self,
        document: &mut Self::Document,
        page_index: u32,
    ) -> Result<PageSnapGeometry, WorkerError> {
        extract_page_snap_geometry(
            &document.metadata,
            page_index,
            ContentGeometryLimits::default(),
        )
        .map_err(|error| match error {
            ContentGeometryError::LimitExceeded(detail) => {
                WorkerError::with_detail(WorkerErrorCode::LimitExceeded, detail)
            }
            ContentGeometryError::Page(detail) => {
                WorkerError::with_detail(WorkerErrorCode::PageError, detail)
            }
            ContentGeometryError::Malformed(detail) => {
                WorkerError::with_detail(WorkerErrorCode::MalformedDocument, detail)
            }
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
        if request.annotation_mode == AnnotationRenderMode::RetainedOnly
            && matches!(document.retained, RetainedPdfiumDocument::Unprepared)
        {
            // Classification uses the immutable snapshot read from the inherited handle.
            // A second PDFium document is allocated only for genuinely mixed/converted content.
            let prepared = retained_annotation_render(&document.metadata)
                .map_err(|error| {
                    WorkerError::with_detail(
                        WorkerErrorCode::MalformedDocument,
                        format!("retained annotation preparation failed: {error}"),
                    )
                })
                .and_then(|render| match render {
                    RetainedAnnotationRender::None => Ok(RetainedPdfiumDocument::None),
                    RetainedAnnotationRender::Original => Ok(RetainedPdfiumDocument::Original),
                    RetainedAnnotationRender::Filtered(bytes) => {
                        let metadata = PdfMetadataDocument::load_mem(&bytes).map_err(|error| {
                            WorkerError::with_detail(
                                WorkerErrorCode::MalformedDocument,
                                format!("retained PDFium display parser rejected the PDF: {error}"),
                            )
                        })?;
                        self.load_display_document(&metadata, bytes, None)
                            .map(RetainedPdfiumDocument::Filtered)
                    }
                });
            document.retained = prepared.unwrap_or_else(RetainedPdfiumDocument::Failed);
        }
        if request.annotation_mode == AnnotationRenderMode::VectorSnapshots
            && document.vector_snapshots.is_none()
        {
            let prepared = vector_snapshot_layer(&document.metadata)
                .map_err(|error| {
                    WorkerError::with_detail(
                        WorkerErrorCode::MalformedDocument,
                        format!("vector Snapshot preparation failed: {error}"),
                    )
                })
                .and_then(|layer| {
                    let bytes = layer.ok_or_else(|| {
                        WorkerError::with_detail(
                            WorkerErrorCode::PageError,
                            "the document has no vector Snapshots",
                        )
                    })?;
                    let metadata = PdfMetadataDocument::load_mem(&bytes).map_err(|error| {
                        WorkerError::with_detail(
                            WorkerErrorCode::MalformedDocument,
                            format!("vector Snapshot layer parser rejected the PDF: {error}"),
                        )
                    })?;
                    self.load_display_document(&metadata, bytes, None)
                });
            document.vector_snapshots = Some(prepared);
        }
        let (raster_document, render_annotations) = match request.annotation_mode {
            AnnotationRenderMode::VectorSnapshots => match &document.vector_snapshots {
                Some(Ok(layer)) => (layer, false),
                Some(Err(error)) => return Err(error.clone()),
                None => unreachable!("vector Snapshot layer prepared above"),
            },
            AnnotationRenderMode::None => (&document.document, false),
            AnnotationRenderMode::All => (&document.document, true),
            AnnotationRenderMode::RetainedOnly => match &document.retained {
                RetainedPdfiumDocument::None => (&document.document, false),
                RetainedPdfiumDocument::Original => (&document.document, true),
                RetainedPdfiumDocument::Filtered(filtered) => (filtered, true),
                RetainedPdfiumDocument::Failed(error) => return Err(error.clone()),
                RetainedPdfiumDocument::Unprepared => {
                    unreachable!("retained document prepared above")
                }
            },
        };
        let page = raster_document
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
            // Shared surfaces and GPUI consume BGRA; the supplier defaults to RGBA.
            .set_reverse_byte_order(false)
            .set_fixed_size(width, height)
            .render_form_data(false)
            .render_annotations(render_annotations)
            .reset_matrix(PdfMatrix::new(a, b, c, d, e, f))
            .map_err(map_pdfium_error)?
            .clip(0, 0, width, height);
        // A vector Snapshot's Form is drawn on a transparent page.
        let config = if request.annotation_mode == AnnotationRenderMode::VectorSnapshots {
            config.set_clear_color(PdfColor::new(255, 255, 255, 0))
        } else {
            config
        };
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

fn map_rotation(rotation: butter_paper_gpui_migration::page_geometry::Rotation) -> Rotation {
    match rotation {
        butter_paper_gpui_migration::page_geometry::Rotation::Degrees0 => Rotation::Degrees0,
        butter_paper_gpui_migration::page_geometry::Rotation::Degrees90 => Rotation::Degrees90,
        butter_paper_gpui_migration::page_geometry::Rotation::Degrees180 => Rotation::Degrees180,
        butter_paper_gpui_migration::page_geometry::Rotation::Degrees270 => Rotation::Degrees270,
    }
}

fn rectangles_match(
    pdfium: [f32; 4],
    canonical: butter_paper_gpui_migration::page_geometry::PdfRect,
) -> bool {
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

#[cfg(windows)]
fn duplicate_inherited_source(source: SourceHandleId) -> Result<File, WorkerError> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{
        DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let raw = usize::try_from(source.0).map_err(|_| {
        WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            "invalid inherited Windows source handle",
        )
    })?;
    let inherited = raw as HANDLE;
    if inherited.is_null() || inherited == INVALID_HANDLE_VALUE {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            "invalid inherited Windows source handle",
        ));
    }
    let mut duplicate: HANDLE = std::ptr::null_mut();
    // SAFETY: `inherited` names a handle in this worker process. On success,
    // `duplicate` is a distinct owned handle for the same open file object.
    let duplicated = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            inherited,
            GetCurrentProcess(),
            &mut duplicate,
            0,
            0,
            DUPLICATE_SAME_ACCESS,
        )
    };
    if duplicated == 0 {
        return Err(WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            format!(
                "invalid inherited Windows source handle: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    // SAFETY: `duplicate` is a fresh owned handle returned by DuplicateHandle.
    Ok(unsafe { File::from_raw_handle(duplicate) })
}

#[cfg(not(any(unix, windows)))]
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

fn run() -> Result<(), Box<dyn std::error::Error>> {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Registration is the first opt-in action. The final self receipt is
    // attempted for both protocol success and setup/runtime failure.
    let lifecycle = WorkerLifecycleReporter::from_environment()?;
    let result = run();
    let lifecycle_result = lifecycle.map(WorkerLifecycleReporter::finish).transpose();
    match (result, lifecycle_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(Box::new(error)),
        (Ok(()), Ok(_)) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use butter_paper_gpui_migration::pdf_worker::{
        ClipRect, JobId, RenderRequest, SessionId, SurfaceDescriptor, SurfaceFormat, SurfaceId,
    };
    use std::os::fd::AsRawFd;

    #[test]
    fn worker_prepares_page_and_form_optional_content_before_pdfium_load() {
        use lopdf::{Document, Object, Stream, dictionary};

        let mut pdf = Document::with_version("1.7");
        let pages = pdf.new_object_id();
        let group = pdf.add_object(dictionary! {
            "Type" => "OCG",
            "Usage" => dictionary! { "View" => dictionary! { "ViewState" => "ON" } },
        });
        let form = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
                "Resources" => dictionary! {}, "OC" => group,
            },
            b"0 0 10 10 re f".to_vec(),
        ));
        let content_bytes = b"/OC /Layer BDC 0 0 10 10 re f EMC q /Fm1 Do Q".to_vec();
        let contents = pdf.add_object(Stream::new(dictionary! {}, content_bytes.clone()));
        let page = pdf.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            "Resources" => dictionary! {
                "Properties" => dictionary! { "Layer" => group },
                "XObject" => dictionary! { "Fm1" => form },
            },
            "Contents" => contents,
        });
        pdf.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let catalog = pdf.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages,
            "OCProperties" => dictionary! {
                "OCGs" => vec![group.into()],
                "D" => dictionary! { "BaseState" => "OFF" },
            },
        });
        pdf.trailer.set("Root", catalog);
        let mut source_bytes = Vec::new();
        pdf.save_to(&mut source_bytes).unwrap();

        let display_bytes = prepare_pdfium_display_bytes(&pdf, source_bytes).unwrap();
        let display = Document::load_mem(&display_bytes).unwrap();
        let usage = display
            .get_object(group)
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Usage")
            .unwrap()
            .as_dict()
            .unwrap();
        assert!(
            usage
                .get(b"View")
                .unwrap()
                .as_dict()
                .unwrap()
                .get(b"ViewState")
                .is_err(),
            "the worker load seam must neutralise valid display-on usage",
        );
        assert_eq!(
            display.get_page_content(page),
            [content_bytes.as_slice(), b"\n"].concat(),
        );
        assert_eq!(
            display
                .get_object(form)
                .unwrap()
                .as_stream()
                .unwrap()
                .dict
                .get(b"OC")
                .unwrap(),
            &Object::Reference(group),
            "normalisation must cover both consumers by changing only their shared OCG",
        );
    }

    fn test_backend(library: &Path) -> (std::sync::MutexGuard<'static, ()>, PdfiumBackend) {
        // The pinned supplier binds once per process. Serialize native calls and
        // reuse that exact binding across independently reported real-pixel tests.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        static LIBRARY: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        let guard = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let backend = if let Some(bound) = LIBRARY.get() {
            assert_eq!(bound, library, "all tests must use the same pinned library");
            PdfiumBackend {
                pdfium: Box::leak(Box::new(Pdfium::default())),
            }
        } else {
            let backend = PdfiumBackend::bind(library).unwrap();
            LIBRARY.set(library.to_path_buf()).unwrap();
            backend
        };
        (guard, backend)
    }

    fn retained_pixel_fixture(rotation: i32, managed: bool, opaque: bool) -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut pdf = Document::with_version("1.7");
        let pages = pdf.new_object_id();
        let mut annotations = Vec::new();
        for (include, subtype, x, colour, name) in [
            (opaque, "Stamp", 20, "1 0 0", "opaque"),
            (managed, "Square", 100, "0 0 1", "bp:managed"),
        ] {
            if !include {
                continue;
            }
            let appearance = pdf.add_object(Stream::new(dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
                "Resources" => dictionary! {},
            }, format!("{colour} rg 0 0 40 40 re f").into_bytes()));
            let annotation = pdf.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => subtype, "NM" => Object::string_literal(name),
                "Rect" => vec![x.into(), 20.into(), (x+40).into(), 60.into()], "F" => 4,
                "AP" => dictionary! { "N" => appearance },
            });
            annotations.push(Object::Reference(annotation));
        }
        let contents = pdf.add_object(Stream::new(
            dictionary! {},
            b"0 0 0 rg 5 5 5 5 re f".to_vec(),
        ));
        let page = pdf.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages, "MediaBox" => vec![0.into(), 0.into(), 160.into(), 160.into()],
            "Resources" => dictionary! {}, "Contents" => contents, "Annots" => annotations, "Rotate" => rotation,
        });
        pdf.objects.insert(
            pages,
            Object::Dictionary(
                dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
            ),
        );
        let catalog = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        pdf.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        bytes
    }

    fn retained_widget_fixture(rotation: i32) -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut pdf = Document::load_mem(&retained_pixel_fixture(rotation, true, true)).unwrap();
        let page = *pdf.get_pages().get(&1).unwrap();
        let font = pdf.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let appearance = pdf.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
            "Resources" => dictionary! {},
        }, b"0 1 0 rg 0 0 40 40 re f".to_vec()));
        let off = pdf.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
        }, b"1 1 0 rg 0 0 40 40 re f".to_vec()));
        let field = pdf.add_object(dictionary! {
            "FT" => "Btn", "T" => Object::string_literal("approved"), "V" => "Yes",
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } },
            "AA" => dictionary! { "K" => Object::string_literal("do not execute") },
        });
        let widget = pdf.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "Parent" => field, "AS" => "Yes",
            "Rect" => vec![20.into(), 100.into(), 60.into(), 140.into()], "F" => 4,
            "AP" => dictionary! { "N" => dictionary! { "Yes" => appearance, "Off" => off } },
            "A" => dictionary! { "S" => "JavaScript", "JS" => Object::string_literal("do not execute") },
        });
        pdf.get_object_mut(field)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Kids", Object::Array(vec![Object::Reference(widget)]));
        pdf.get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .get_mut(b"Annots")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(Object::Reference(widget));
        let acro_form = pdf.add_object(dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field)]), "NeedAppearances" => true,
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } },
        });
        let catalog = pdf.trailer.get(b"Root").unwrap().as_reference().unwrap();
        pdf.get_object_mut(catalog)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("AcroForm", acro_form);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        bytes
    }

    fn retained_popup_fixture(rotation: i32, include_popup: bool) -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut pdf = Document::load_mem(&retained_pixel_fixture(rotation, false, false)).unwrap();
        let page = *pdf.get_pages().get(&1).unwrap();
        let parent_appearance = pdf.add_object(Stream::new(dictionary! {
            "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
            "Resources" => dictionary! {},
        }, b"0 1 0 rg 0 0 40 40 re f".to_vec()));
        let parent = pdf.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Stamp",
            "Rect" => vec![20.into(), 100.into(), 60.into(), 140.into()], "F" => 4,
            "AP" => dictionary! { "N" => parent_appearance },
        });
        let mut annotations = vec![Object::Reference(parent)];
        if include_popup {
            let popup_appearance = pdf.add_object(Stream::new(dictionary! {
                "Type" => "XObject", "Subtype" => "Form", "BBox" => vec![0.into(), 0.into(), 60.into(), 40.into()],
                "Resources" => dictionary! {},
            }, b"1 0 1 rg 0 0 60 40 re f".to_vec()));
            let popup = pdf.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => "Popup", "Parent" => parent,
                "Rect" => vec![80.into(), 100.into(), 140.into(), 140.into()], "F" => 4,
                "Open" => true, "Contents" => Object::string_literal("Popup must remain non-rendering"),
                "T" => Object::string_literal("Reviewer"),
                "AP" => dictionary! { "N" => popup_appearance },
            });
            pdf.get_object_mut(parent)
                .unwrap()
                .as_dict_mut()
                .unwrap()
                .set("Popup", popup);
            annotations.push(Object::Reference(popup));
        }
        pdf.get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Annots", annotations);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        bytes
    }

    fn retained_apless_widget_fixture(rotation: i32) -> Vec<u8> {
        use lopdf::{Document, Object, dictionary};
        let mut pdf = Document::load_mem(&retained_pixel_fixture(rotation, true, false)).unwrap();
        let page = *pdf.get_pages().get(&1).unwrap();
        let font = pdf.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding",
        });
        let field = pdf.add_object(dictionary! {
            "FT" => "Tx", "T" => Object::string_literal("site-reference"),
            "V" => Object::string_literal("A-104"), "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
        });
        let widget = pdf.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "Parent" => field,
            "Rect" => vec![20.into(), 100.into(), 60.into(), 140.into()], "F" => 4,
        });
        pdf.get_object_mut(field)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Kids", Object::Array(vec![Object::Reference(widget)]));
        pdf.get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .get_mut(b"Annots")
            .unwrap()
            .as_array_mut()
            .unwrap()
            .push(Object::Reference(widget));
        let acro_form = pdf.add_object(dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field)]), "NeedAppearances" => true,
            "DA" => Object::string_literal("/Helv 12 Tf 0 g"),
            "DR" => dictionary! { "Font" => dictionary! { "Helv" => font } },
        });
        let catalog = pdf.trailer.get(b"Root").unwrap().as_reference().unwrap();
        pdf.get_object_mut(catalog)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("AcroForm", acro_form);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn retained_widget_pixels_use_selected_static_ap_in_full_and_cropped_rotated_pages() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for rotation in [0, 90] {
            let path = env::temp_dir().join(format!(
                "bp-retained-widget-{}-{rotation}.pdf",
                std::process::id()
            ));
            std::fs::write(&path, retained_widget_fixture(rotation)).unwrap();
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend
                .open(SourceHandleId(source.as_raw_fd() as u64), None)
                .unwrap();
            std::fs::remove_file(path).unwrap();
            let mut retained = Vec::new();
            for mode in [
                AnnotationRenderMode::None,
                AnnotationRenderMode::All,
                AnnotationRenderMode::RetainedOnly,
            ] {
                let request = retained_test_request(mode, 160);
                let mut pixels = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(
                        &mut document,
                        &request,
                        &mut pixels,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                let green = pixels
                    .chunks_exact(4)
                    .filter(|p| p[1] > 240 && p[0] < 10 && p[2] < 10)
                    .count();
                let yellow = pixels
                    .chunks_exact(4)
                    .filter(|p| p[0] > 240 && p[1] > 240 && p[2] < 10)
                    .count();
                assert_eq!(
                    green > 100,
                    mode == AnnotationRenderMode::RetainedOnly,
                    "static Widget /AS Yes AP in {mode:?}, rotation {rotation}"
                );
                assert_eq!(
                    yellow, 0,
                    "Widget Off AP must not render in {mode:?}, rotation {rotation}"
                );
                if mode == AnnotationRenderMode::RetainedOnly {
                    retained = pixels;
                }
            }
            let mut request = retained_test_request(AnnotationRenderMode::RetainedOnly, 80);
            request.transform[4] = -80.;
            let mut tile = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(&mut document, &request, &mut tile, &AtomicBool::new(false))
                .unwrap();
            for row in 0..160 {
                assert_eq!(
                    &tile[row * 80 * 4..(row + 1) * 80 * 4],
                    &retained[row * 160 * 4 + 80 * 4..(row + 1) * 160 * 4],
                    "translated tile row {row}, rotation {rotation}"
                );
            }
            backend.close(document);
        }
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn linked_content_popup_contributes_no_retained_pixels() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for rotation in [0, 90] {
            let mut rendered = Vec::new();
            for include_popup in [false, true] {
                let path = env::temp_dir().join(format!(
                    "bp-retained-popup-{}-{rotation}-{include_popup}.pdf",
                    std::process::id()
                ));
                std::fs::write(&path, retained_popup_fixture(rotation, include_popup)).unwrap();
                let source = File::open(&path).unwrap();
                let (mut document, _) = backend
                    .open(SourceHandleId(source.as_raw_fd() as u64), None)
                    .unwrap();
                std::fs::remove_file(path).unwrap();
                let request = retained_test_request(AnnotationRenderMode::RetainedOnly, 160);
                let mut pixels = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(
                        &mut document,
                        &request,
                        &mut pixels,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                let green = pixels
                    .chunks_exact(4)
                    .filter(|pixel| pixel[1] > 240 && pixel[0] < 10 && pixel[2] < 10)
                    .count();
                let magenta = pixels
                    .chunks_exact(4)
                    .filter(|pixel| pixel[0] > 240 && pixel[1] < 10 && pixel[2] > 240)
                    .count();
                assert!(
                    green > 100,
                    "linked parent appearance must render, rotation {rotation}"
                );
                assert_eq!(
                    magenta, 0,
                    "content-bearing Popup appearance must remain omitted, rotation {rotation}"
                );
                rendered.push(pixels);
                backend.close(document);
            }
            assert_eq!(
                rendered[1], rendered[0],
                "adding the linked Popup must not change retained pixels, rotation {rotation}"
            );
        }
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn retained_annotations_lazy_shortcuts_and_failed_preparation_are_explicit() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for (managed, opaque) in [(false, false), (true, false), (false, true)] {
            let path = env::temp_dir().join(format!(
                "bp-retained-shortcut-{}-{managed}-{opaque}.pdf",
                std::process::id()
            ));
            std::fs::write(&path, retained_pixel_fixture(0, managed, opaque)).unwrap();
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend
                .open(SourceHandleId(source.as_raw_fd() as u64), None)
                .unwrap();
            std::fs::remove_file(path).unwrap();
            assert!(matches!(
                document.retained,
                RetainedPdfiumDocument::Unprepared
            ));
            let request = retained_test_request(AnnotationRenderMode::RetainedOnly, 160);
            let mut pixels = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(
                    &mut document,
                    &request,
                    &mut pixels,
                    &AtomicBool::new(false),
                )
                .unwrap();
            assert!(
                if opaque {
                    matches!(document.retained, RetainedPdfiumDocument::Original)
                } else {
                    matches!(document.retained, RetainedPdfiumDocument::None)
                },
                "ordinary documents must not allocate a duplicate PDFium document"
            );
            backend.close(document);
        }
        let mut pdf =
            PdfMetadataDocument::load_mem(&retained_pixel_fixture(0, true, true)).unwrap();
        let page = *pdf.get_pages().get(&1).unwrap();
        let annotations = pdf
            .get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .get_mut(b"Annots")
            .unwrap()
            .as_array_mut()
            .unwrap();
        annotations.push(annotations[1].clone()); // Duplicate managed identity must not silently become an unfiltered fallback.
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        let path = env::temp_dir().join(format!("bp-retained-failure-{}.pdf", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend
            .open(SourceHandleId(source.as_raw_fd() as u64), None)
            .unwrap();
        std::fs::remove_file(path).unwrap();
        let request = retained_test_request(AnnotationRenderMode::RetainedOnly, 160);
        let mut pixels = vec![0; request.surface.byte_len as usize];
        let first = backend
            .render_crop(
                &mut document,
                &request,
                &mut pixels,
                &AtomicBool::new(false),
            )
            .unwrap_err();
        assert_eq!(first.code, WorkerErrorCode::MalformedDocument);
        assert!(matches!(
            document.retained,
            RetainedPdfiumDocument::Failed(_)
        ));
        let second = backend
            .render_crop(
                &mut document,
                &request,
                &mut pixels,
                &AtomicBool::new(false),
            )
            .unwrap_err();
        assert_eq!(second, first);
        let original = retained_test_request(AnnotationRenderMode::All, 160);
        backend
            .render_crop(
                &mut document,
                &original,
                &mut pixels,
                &AtomicBool::new(false),
            )
            .unwrap();
        backend.close(document);
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn retained_annotations_review_widget_fixture_pixels_at_two_scales_and_crop() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for rotation in [0, 90] {
            let path = env::temp_dir().join(format!(
                "bp-retained-apless-widget-review-{}-{rotation}.pdf",
                std::process::id()
            ));
            std::fs::write(&path, retained_apless_widget_fixture(rotation)).unwrap();
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend
                .open(SourceHandleId(source.as_raw_fd() as u64), None)
                .unwrap();
            std::fs::remove_file(path).unwrap();
            let geometry = backend.page_geometry(&mut document, 0).unwrap();
            for scale in [1., 2.] {
                let width = (geometry.display_width_points * scale).ceil() as u32;
                let height = (geometry.display_height_points * scale).ceil() as u32;
                let mut request = retained_test_request(AnnotationRenderMode::RetainedOnly, width);
                request.surface.height = height;
                request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
                request.clip.height = height;
                request.transform = [scale, 0., 0., scale, 0., 0.];
                let mut pixels = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(
                        &mut document,
                        &request,
                        &mut pixels,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                let red_points = (0..height as usize)
                    .flat_map(|y| (0..width as usize).map(move |x| (x, y)))
                    .filter(|(x, y)| {
                        let pixel = &pixels[(*y * width as usize + *x) * 4..][..4];
                        pixel[2] > 180 && pixel[1] < 150 && pixel[0] < 150
                    })
                    .collect::<Vec<_>>();
                assert!(
                    red_points.len() > (20. * scale) as usize,
                    "AP-less Widget fallback at scale {scale}, rotation {rotation}: {} red pixels",
                    red_points.len()
                );
                let margin = (5. * scale) as usize;
                let min_x = red_points.iter().map(|(x, _)| *x).min().unwrap();
                let max_x = red_points.iter().map(|(x, _)| *x).max().unwrap();
                let min_y = red_points.iter().map(|(_, y)| *y).min().unwrap();
                let max_y = red_points.iter().map(|(_, y)| *y).max().unwrap();
                let crop_x = min_x.saturating_sub(margin);
                let crop_y = min_y.saturating_sub(margin);
                let crop_right = (max_x + margin + 1).min(width as usize);
                let crop_bottom = (max_y + margin + 1).min(height as usize);
                let crop_width = crop_right - crop_x;
                let crop_height = crop_bottom - crop_y;
                request.surface.width = crop_width as u32;
                request.surface.height = crop_height as u32;
                request.surface.stride = crop_width as u32 * 4;
                request.surface.byte_len = (crop_width * crop_height * 4) as u64;
                request.clip.width = crop_width as u32;
                request.clip.height = crop_height as u32;
                request.transform[4] = -(crop_x as f32);
                request.transform[5] = -(crop_y as f32);
                let mut tile = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(&mut document, &request, &mut tile, &AtomicBool::new(false))
                    .unwrap();
                for row in 0..crop_height {
                    let full_start = ((row + crop_y) * width as usize + crop_x) * 4;
                    assert_eq!(
                        &tile[row * crop_width * 4..(row + 1) * crop_width * 4],
                        &pixels[full_start..full_start + crop_width * 4],
                        "translated crop row {row}, scale {scale}, rotation {rotation}"
                    );
                }
            }
            backend.close(document);
        }
    }

    fn retained_test_request(mode: AnnotationRenderMode, width: u32) -> RenderRequest {
        RenderRequest {
            job_id: JobId(1),
            session_id: SessionId(1),
            page_index: 0,
            annotation_mode: mode,
            transform: [1., 0., 0., 1., 0., 0.],
            clip: ClipRect {
                x: 0,
                y: 0,
                width,
                height: 160,
            },
            surface: SurfaceDescriptor {
                surface_id: SurfaceId(1),
                width,
                height: 160,
                stride: width * 4,
                byte_len: u64::from(width) * 160 * 4,
                format: SurfaceFormat::Bgra8Premultiplied,
            },
        }
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn retained_annotations_pixels_preserve_opaque_ap_without_stale_managed_ap() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for rotation in [0, 90] {
            let path = env::temp_dir().join(format!(
                "bp-retained-pixels-{}-{rotation}.pdf",
                std::process::id()
            ));
            std::fs::write(&path, retained_pixel_fixture(rotation, true, true)).unwrap();
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend
                .open(SourceHandleId(source.as_raw_fd() as u64), None)
                .unwrap();
            std::fs::remove_file(path).unwrap(); // Rendering must use the authorised handle/snapshot, never reopen paths.
            assert!(matches!(
                document.retained,
                RetainedPdfiumDocument::Unprepared
            ));
            let mut retained = Vec::new();
            for mode in [
                AnnotationRenderMode::None,
                AnnotationRenderMode::RetainedOnly,
                AnnotationRenderMode::All,
            ] {
                let request = retained_test_request(mode, 160);
                let mut pixels = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(
                        &mut document,
                        &request,
                        &mut pixels,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                let red = pixels
                    .chunks_exact(4)
                    .filter(|p| p[2] > 240 && p[1] < 10 && p[0] < 10)
                    .count();
                let blue = pixels
                    .chunks_exact(4)
                    .filter(|p| p[0] > 240 && p[1] < 10 && p[2] < 10)
                    .count();
                assert_eq!(
                    red > 100,
                    mode != AnnotationRenderMode::None,
                    "opaque AP in {mode:?}, rotation {rotation}"
                );
                assert_eq!(
                    blue > 100,
                    mode == AnnotationRenderMode::All,
                    "managed AP in {mode:?}, rotation {rotation}"
                );
                if mode == AnnotationRenderMode::None {
                    assert!(matches!(
                        document.retained,
                        RetainedPdfiumDocument::Unprepared
                    ));
                }
                if mode == AnnotationRenderMode::RetainedOnly {
                    assert!(matches!(
                        document.retained,
                        RetainedPdfiumDocument::Filtered(_)
                    ));
                    retained = pixels;
                }
            }
            let request = retained_test_request(AnnotationRenderMode::RetainedOnly, 80);
            let mut tile = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(&mut document, &request, &mut tile, &AtomicBool::new(false))
                .unwrap();
            for row in 0..160 {
                assert_eq!(
                    &tile[row * 80 * 4..(row + 1) * 80 * 4],
                    &retained[row * 160 * 4..row * 160 * 4 + 80 * 4],
                    "tile row {row}, rotation {rotation}"
                );
            }
            backend.close(document);
        }
    }

    #[test]
    #[ignore = "set BP_PDFIUM_LIBRARY and BP_PDFIUM_PUBLIC_TEST_PDF to opt in"]
    fn renders_public_pdf_with_pinned_development_library() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let pdf_path = env::var_os("BP_PDFIUM_PUBLIC_TEST_PDF").expect("BP_PDFIUM_PUBLIC_TEST_PDF");
        let source = File::open(pdf_path).unwrap();
        let (_guard, mut backend) = test_backend(Path::new(&library));
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
            annotation_mode: AnnotationRenderMode::None,
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
