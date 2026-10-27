use butter_paper::page_geometry::{PageCoordinateSpace, PdfMetadataDocument};
use butter_paper::pdf_content_geometry::{
    ContentGeometryError, ContentGeometryLimits, PageSnapGeometry, extract_page_snap_geometry,
};
use butter_paper::pdf_engine::{
    RetainedAnnotationRender, pdfium_display_render_bytes, retained_annotation_render,
    vector_snapshot_layer,
};
use butter_paper::pdf_forms::{apply_form_edits, needs_appearances, regenerate_appearances};
use butter_paper::pdf_text_edit::{
    PageTextLayer, TextEditError, TextEditLimits, apply_content_edits, extract_page_text,
};
use butter_paper::pdf_worker::{
    AnnotationRenderMode, CancellationRegistry, DocumentEdits, DocumentInfo, FileSurfaceStore,
    PageGeometry, PdfBackend, RenderRequest, Rotation, SourceHandleId, SurfaceLimits, WorkerError,
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

/// A copy of `source` to apply edits to, whose new objects take numbers the
/// file has never used: its own cross-reference stream's, which the parser
/// drops, included. An update reusing that number is ignored by PDFium.
fn editable_copy(source: &PdfMetadataDocument) -> PdfMetadataDocument {
    let mut copy = source.clone();
    if let Ok(size) = copy.trailer.get(b"Size").and_then(lopdf::Object::as_i64)
        && let Ok(size) = u32::try_from(size)
    {
        copy.max_id = copy.max_id.max(size.saturating_sub(1));
    }
    copy
}

/// `source_bytes` followed by an incremental update holding the objects
/// `edited` adds to or changes in `source`. `None` when the source has no
/// cross-reference to continue from, is still encrypted, or lost objects.
fn incremental_update(
    source: &PdfMetadataDocument,
    source_bytes: &[u8],
    edited: &PdfMetadataDocument,
) -> Option<Vec<u8>> {
    if source.xref_start == 0
        || source.trailer.has(b"Encrypt")
        || source
            .objects
            .keys()
            .any(|id| !edited.objects.contains_key(id))
    {
        return None;
    }
    // The update needs only the source's trailer and cross-reference type,
    // not its objects.
    let mut previous = PdfMetadataDocument::new();
    previous.version = source.version.clone();
    previous.max_id = source.max_id;
    previous.xref_start = source.xref_start;
    previous.reference_table.cross_reference_type = source.reference_table.cross_reference_type;
    previous.encryption_state = source.encryption_state.clone();
    for key in [b"Root".as_slice(), b"Info", b"ID"] {
        if let Ok(value) = source.trailer.get(key) {
            previous.trailer.set(key, value.clone());
        }
    }
    let mut update = lopdf::IncrementalDocument::create_from(source_bytes.to_vec(), previous);
    for (id, object) in &edited.objects {
        if source.objects.get(id) != Some(object) {
            update.new_document.objects.insert(*id, object.clone());
        }
    }
    update.new_document.max_id = edited.max_id;
    let mut bytes = Vec::with_capacity(source_bytes.len() + 64 * 1024);
    update.save_to(&mut bytes).ok()?;
    Some(bytes)
}

/// The document as renders show it: the source with page-text edits and
/// filled form fields applied, and appearances drawn for form fields that
/// lack them. `None` when that is the source itself.
type EditedDisplay = Option<(PdfMetadataDocument, Vec<u8>)>;

fn edited_display(
    source: &PdfMetadataDocument,
    source_bytes: &[u8],
    edits: &DocumentEdits,
) -> Result<EditedDisplay, WorkerError> {
    let draw_fields = needs_appearances(source);
    if edits.is_empty() && !draw_fields {
        return Ok(None);
    }
    let mut edited = editable_copy(source);
    // Pages first: the other edits name pages as arranged.
    if !edits.pages.is_empty() {
        let managed =
            butter_paper::pdf_engine::managed_annotation_slots(&edited).map_err(|error| {
                WorkerError::with_detail(WorkerErrorCode::MalformedDocument, error.to_string())
            })?;
        butter_paper::pdf_page_structure::restructure_pages(&mut edited, &edits.pages, &managed)
            .map_err(|error| WorkerError::with_detail(WorkerErrorCode::InvalidRequest, error))?;
    }
    butter_paper::pdf_page_base::restore_pages(&mut edited, &edits.bases)
        .map_err(|error| WorkerError::with_detail(WorkerErrorCode::InvalidRequest, error))?;
    apply_content_edits(
        &mut edited,
        &edits.text,
        &edits.tables,
        &edits.moves,
        &TextEditLimits::for_display(),
    )
    .map_err(map_text_edit_error)?;
    if draw_fields {
        regenerate_appearances(&mut edited).map_err(|error| {
            WorkerError::with_detail(
                WorkerErrorCode::MalformedDocument,
                format!("form fields cannot be drawn: {error}"),
            )
        })?;
    }
    apply_form_edits(&mut edited, &edits.forms).map_err(|error| {
        WorkerError::with_detail(
            WorkerErrorCode::InvalidRequest,
            format!("form fields cannot be filled: {error}"),
        )
    })?;
    // Only the changed objects are written, after the source's bytes: a
    // whole large file is not serialised for each edit.
    if let Some(bytes) = incremental_update(source, source_bytes, &edited) {
        return Ok(Some((edited, bytes)));
    }
    let mut bytes = Vec::with_capacity(source_bytes.len());
    edited.save_to(&mut bytes).map_err(|error| {
        WorkerError::with_detail(
            WorkerErrorCode::MalformedDocument,
            format!("edited PDF cannot be written: {error}"),
        )
    })?;
    // Reparse so the display metadata matches what PDFium reads.
    let edited = PdfMetadataDocument::load_mem(&bytes).map_err(|error| {
        WorkerError::with_detail(
            WorkerErrorCode::MalformedDocument,
            format!("edited PDF cannot be read: {error}"),
        )
    })?;
    Ok(Some((edited, bytes)))
}

/// Which of a document's PDFium documents a page was loaded from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RasterSource {
    Display,
    Retained,
    VectorSnapshots,
}

/// Pages most recently drawn, kept loaded: drawing the same page again
/// (the next viewer tile, or its thumbnail after its page) then neither
/// parses its content nor decodes its images again.
#[derive(Default)]
struct LoadedPages(Vec<(RasterSource, i32, PdfPage<'static>)>);

impl LoadedPages {
    const CAPACITY: usize = 3;

    fn get(
        &mut self,
        source: RasterSource,
        index: i32,
        document: &PdfDocument<'static>,
    ) -> Result<&PdfPage<'static>, WorkerError> {
        if let Some(position) = self
            .0
            .iter()
            .position(|(loaded, page, _)| *loaded == source && *page == index)
        {
            let entry = self.0.remove(position);
            self.0.push(entry);
        } else {
            let page = document.pages().get(index).map_err(map_pdfium_error)?;
            if self.0.len() == Self::CAPACITY {
                self.0.remove(0);
            }
            self.0.push((source, index, page));
        }
        Ok(&self.0.last().expect("a page was just loaded").2)
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

struct PdfiumDocument {
    /// Declared first, so its pages close before the documents they
    /// belong to. Cleared whenever one of those documents is replaced.
    loaded_pages: LoadedPages,
    document: PdfDocument<'static>,
    /// The displayed document: the source with any page-text edits.
    metadata: PdfMetadataDocument,
    /// The unedited source, which page-text edits always apply to.
    source: PdfMetadataDocument,
    source_bytes: Vec<u8>,
    password: Option<String>,
    edits: DocumentEdits,
    /// Kept pages the app sent, by name.
    page_bases: std::collections::HashMap<String, std::sync::Arc<Vec<u8>>>,
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
        let source_bytes = metadata_bytes.clone();
        // A form whose fields lack appearances shows them drawn.
        let (display, display_bytes) =
            match edited_display(&metadata, &source_bytes, &DocumentEdits::default()) {
                Ok(Some(display)) => display,
                _ => (metadata.clone(), metadata_bytes),
            };
        let document = self
            .load_display_document(&display, display_bytes, password)
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
                loaded_pages: LoadedPages::default(),
                document,
                source: metadata,
                metadata: display,
                source_bytes,
                password: password.map(str::to_owned),
                edits: DocumentEdits::default(),
                page_bases: Default::default(),
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

    fn page_text(
        &mut self,
        document: &mut Self::Document,
        page_index: u32,
    ) -> Result<PageTextLayer, WorkerError> {
        extract_page_text(&document.source, page_index, &TextEditLimits::default())
            .map_err(map_text_edit_error)
    }

    fn page_base_text(
        &mut self,
        document: &mut Self::Document,
        page_index: u32,
        base: &str,
    ) -> Result<PageTextLayer, WorkerError> {
        let pdf = document.page_bases.get(base).ok_or_else(|| {
            WorkerError::with_detail(WorkerErrorCode::InvalidRequest, "the kept page is missing")
        })?;
        let kept = PdfMetadataDocument::load_mem(pdf).map_err(|error| {
            WorkerError::with_detail(
                WorkerErrorCode::MalformedDocument,
                format!("a kept page cannot be read: {error}"),
            )
        })?;
        let mut layer =
            extract_page_text(&kept, 0, &TextEditLimits::default()).map_err(map_text_edit_error)?;
        layer.page_index = page_index;
        Ok(layer)
    }

    fn add_page_base(
        &mut self,
        document: &mut Self::Document,
        id: String,
        pdf: Vec<u8>,
    ) -> Result<(), WorkerError> {
        document.page_bases.insert(id, std::sync::Arc::new(pdf));
        Ok(())
    }

    fn set_document_edits(
        &mut self,
        document: &mut Self::Document,
        edits: &DocumentEdits,
    ) -> Result<(), WorkerError> {
        if document.edits == *edits {
            return Ok(());
        }
        let mut edits = edits.clone();
        for base in &mut edits.bases {
            let pdf = document.page_bases.get(&base.id).ok_or_else(|| {
                WorkerError::with_detail(
                    WorkerErrorCode::InvalidRequest,
                    "the kept page is missing",
                )
            })?;
            base.pdf = Some(pdf.clone());
        }
        let edits = &edits;
        let shown = edited_display(&document.source, &document.source_bytes, edits)?;
        let (metadata, bytes) =
            shown.unwrap_or_else(|| (document.source.clone(), document.source_bytes.clone()));
        document.loaded_pages.clear();
        document.document =
            self.load_display_document(&metadata, bytes, document.password.as_deref())?;
        document.metadata = metadata;
        document.retained = RetainedPdfiumDocument::Unprepared;
        document.edits = edits.clone();
        Ok(())
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
            document.loaded_pages.clear();
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
            document.loaded_pages.clear();
            document.vector_snapshots = Some(prepared);
        }
        let (raster_document, source, render_annotations) = match request.annotation_mode {
            AnnotationRenderMode::VectorSnapshots => match &document.vector_snapshots {
                Some(Ok(layer)) => (layer, RasterSource::VectorSnapshots, false),
                Some(Err(error)) => return Err(error.clone()),
                None => unreachable!("vector Snapshot layer prepared above"),
            },
            AnnotationRenderMode::None => (&document.document, RasterSource::Display, false),
            AnnotationRenderMode::All => (&document.document, RasterSource::Display, true),
            AnnotationRenderMode::RetainedOnly => match &document.retained {
                RetainedPdfiumDocument::None => (&document.document, RasterSource::Display, false),
                RetainedPdfiumDocument::Original => {
                    (&document.document, RasterSource::Display, true)
                }
                RetainedPdfiumDocument::Filtered(filtered) => {
                    (filtered, RasterSource::Retained, true)
                }
                RetainedPdfiumDocument::Failed(error) => return Err(error.clone()),
                RetainedPdfiumDocument::Unprepared => {
                    unreachable!("retained document prepared above")
                }
            },
        };
        let page = document
            .loaded_pages
            .get(source, page_index, raster_document)?;
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
        let config = config.set_image_smoothing(!request.draft);
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

fn map_rotation(rotation: butter_paper::page_geometry::Rotation) -> Rotation {
    match rotation {
        butter_paper::page_geometry::Rotation::Degrees0 => Rotation::Degrees0,
        butter_paper::page_geometry::Rotation::Degrees90 => Rotation::Degrees90,
        butter_paper::page_geometry::Rotation::Degrees180 => Rotation::Degrees180,
        butter_paper::page_geometry::Rotation::Degrees270 => Rotation::Degrees270,
    }
}

fn rectangles_match(pdfium: [f32; 4], canonical: butter_paper::page_geometry::PdfRect) -> bool {
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

fn map_text_edit_error(error: TextEditError) -> WorkerError {
    let code = match error {
        TextEditError::LimitExceeded(_) => WorkerErrorCode::LimitExceeded,
        TextEditError::Page(_) => WorkerErrorCode::PageError,
        TextEditError::Malformed(_) => WorkerErrorCode::MalformedDocument,
        TextEditError::Stale(_) | TextEditError::ReadOnly(_) | TextEditError::Unencodable(_) => {
            WorkerErrorCode::InvalidRequest
        }
    };
    WorkerError::with_detail(code, error.to_string())
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
    use butter_paper::pdf_text_edit::PageTextEdit;
    use butter_paper::pdf_text_edit::apply_text_edits;
    use butter_paper::pdf_worker::{
        ClipRect, JobId, RenderRequest, SessionId, SurfaceDescriptor, SurfaceFormat, SurfaceId,
    };

    /// The inherited-source id a worker receives for `file` on this platform.
    pub(super) fn source_handle(file: &File) -> SourceHandleId {
        #[cfg(unix)]
        let raw = std::os::fd::AsRawFd::as_raw_fd(file) as u64;
        #[cfg(windows)]
        let raw = std::os::windows::io::AsRawHandle::as_raw_handle(file) as u64;
        SourceHandleId(raw)
    }

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

    pub(super) fn test_backend(
        library: &Path,
    ) -> (std::sync::MutexGuard<'static, ()>, PdfiumBackend) {
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
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
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
                let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
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
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
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
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
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
    fn retained_apless_form_field_draws_its_value_at_two_scales_and_crop() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for rotation in [0, 90] {
            let path = env::temp_dir().join(format!(
                "bp-retained-apless-widget-review-{}-{rotation}.pdf",
                std::process::id()
            ));
            std::fs::write(&path, retained_apless_widget_fixture(rotation)).unwrap();
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
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
                // A form field without an appearance draws its value, as a
                // viewer asked to regenerate appearances does, not the
                // missing-appearance marker.
                let pixel_at = |x: usize, y: usize| &pixels[(y * width as usize + x) * 4..][..4];
                let red = (0..height as usize)
                    .flat_map(|y| (0..width as usize).map(move |x| (x, y)))
                    .filter(|(x, y)| {
                        let pixel = pixel_at(*x, *y);
                        pixel[2] > 180 && pixel[1] < 150 && pixel[0] < 150
                    })
                    .count();
                assert_eq!(red, 0, "no marker at scale {scale}, rotation {rotation}");
                let value_points = (0..height as usize)
                    .flat_map(|y| (0..width as usize).map(move |x| (x, y)))
                    .filter(|(x, y)| {
                        let pixel = pixel_at(*x, *y);
                        pixel[3] > 128 && pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100
                    })
                    .collect::<Vec<_>>();
                assert!(
                    value_points.len() > (20. * scale) as usize,
                    "the field's value at scale {scale}, rotation {rotation}: {} dark pixels",
                    value_points.len()
                );
                let margin = (5. * scale) as usize;
                let min_x = value_points.iter().map(|(x, _)| *x).min().unwrap();
                let max_x = value_points.iter().map(|(x, _)| *x).max().unwrap();
                let min_y = value_points.iter().map(|(_, y)| *y).min().unwrap();
                let max_y = value_points.iter().map(|(_, y)| *y).max().unwrap();
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
            draft: false,
        }
    }

    /// PDFium maps the crop box's top-left corner to the bitmap origin before
    /// the request's matrix: a page whose boxes start away from (0, 0) needs
    /// no offset in that matrix.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_page_whose_box_starts_away_from_the_origin_renders_from_its_corner() {
        use lopdf::{Document, Object, Stream, dictionary};
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        // A black square in the box's top-left corner, as a printed book's
        // pages often start at (36, 45).
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            b"0 g 36 185 20 20 re f".to_vec(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![36.into(), 45.into(), 196.into(), 205.into()],
            "CropBox" => vec![36.into(), 45.into(), 196.into(), 205.into()],
            "Contents" => content_id,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id =
            document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        let path = env::temp_dir().join(format!("bp-offset-box-{}.pdf", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        let request = retained_test_request(AnnotationRenderMode::None, 160);
        let mut pixels = vec![0; request.surface.byte_len as usize];
        backend
            .render_crop(
                &mut document,
                &request,
                &mut pixels,
                &AtomicBool::new(false),
            )
            .unwrap();
        let pixel = |x: usize, y: usize| pixels[(y * 160 + x) * 4];
        assert!(
            pixel(5, 5) < 64,
            "the corner square is at the bitmap's corner"
        );
        assert!(pixel(40, 40) > 192, "and nowhere else");
    }

    /// A page drawing `text` in a TrueType subset embedded as Word does:
    /// WinAnsi codes, no ToUnicode map.
    fn truetype_subset_fixture(text: &str) -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let program = include_bytes!("../../assets/fonts/Arimo-Regular.ttf").to_vec();
        let file = document.add_object(Stream::new(dictionary! {}, program));
        let descriptor = document.add_object(dictionary! {
            "Type" => "FontDescriptor", "FontName" => "ABCDEF+Arimo", "Flags" => 32,
            "Ascent" => 905, "Descent" => -212, "FontFile2" => file,
        });
        let font_id = document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "TrueType", "BaseFont" => "ABCDEF+Arimo",
            "Encoding" => "WinAnsiEncoding", "FirstChar" => 32, "LastChar" => 126,
            "Widths" => (32..=126).map(|_| Object::Integer(556)).collect::<Vec<_>>(),
            "FontDescriptor" => descriptor,
        });
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 24 Tf 10 60 Td ({text}) Tj ET").into_bytes(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 160.into(), 160.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn text_typed_in_a_truetype_subset_draws_its_own_glyphs() {
        use butter_paper::pdf_text_edit::TextEditRun;
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let mut open = |text: &str| {
            let path = env::temp_dir().join(format!(
                "bp-truetype-subset-{}-{}.pdf",
                std::process::id(),
                text.len()
            ));
            std::fs::write(&path, truetype_subset_fixture(text)).unwrap();
            let source = File::open(&path).unwrap();
            let (document, _) = backend.open(source_handle(&source), None).unwrap();
            std::fs::remove_file(path).unwrap();
            document
        };
        let mut edited = open("Hi");
        let mut expected = open("Hi Wow");
        let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
            let request = retained_test_request(AnnotationRenderMode::RetainedOnly, 160);
            let mut pixels = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                .unwrap();
            pixels
        };
        let layer = backend.page_text(&mut edited, 0).unwrap();
        let line = &layer.lines[0];
        let edit = PageTextEdit {
            page_index: 0,
            anchor: line.anchor,
            original: line.original_runs(),
            runs: vec![TextEditRun {
                style: line.styles[0].clone(),
                text: "Hi Wow".into(),
            }],
            layout: Default::default(),
        };
        backend
            .set_document_edits(
                &mut edited,
                &DocumentEdits::from_text(std::slice::from_ref(&edit)),
            )
            .unwrap();
        // Drawn with the subset's own glyphs, exactly as the source would be.
        assert!(
            render(&mut backend, &mut edited) == render(&mut backend, &mut expected),
            "the typed text draws other glyphs"
        );
    }

    fn page_text_fixture() -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let content_id = document.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 48 Tf 10 60 Td (Hello) Tj ET".to_vec(),
        ));
        let page_id = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 160.into(), 160.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn text_edits_append_an_update_pdfium_and_lopdf_read() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, backend) = test_backend(Path::new(&library));
        let table = page_text_fixture();
        // The same page with a cross-reference stream and object streams.
        let mut stream = Vec::new();
        PdfMetadataDocument::load_mem(&table)
            .unwrap()
            .save_modern(&mut stream)
            .unwrap();
        for bytes in [table, stream] {
            let source = PdfMetadataDocument::load_mem(&bytes).unwrap();
            let layer = extract_page_text(&source, 0, &TextEditLimits::default()).unwrap();
            let line = &layer.lines[0];
            let mut runs = line.original_runs();
            runs[0].text.push_str(" zqx");
            let edit = PageTextEdit {
                page_index: 0,
                anchor: line.anchor,
                original: line.original_runs(),
                runs,
                layout: Default::default(),
            };
            let mut edited = editable_copy(&source);
            apply_text_edits(&mut edited, &[edit], &TextEditLimits::default()).unwrap();
            // New objects never reuse a number the file used (the
            // cross-reference stream's).
            let size = source.trailer.get(b"Size").unwrap().as_i64().unwrap() as u32;
            assert!(
                edited
                    .objects
                    .keys()
                    .filter(|id| !source.objects.contains_key(id))
                    .all(|(number, _)| *number >= size)
            );
            let updated = incremental_update(&source, &bytes, &edited).expect("an update");
            assert!(updated.starts_with(&bytes), "the source bytes are kept");
            assert!(
                updated.len() < bytes.len() + 2048,
                "only changed objects follow"
            );
            let reread = PdfMetadataDocument::load_mem(&updated).unwrap();
            assert_eq!(
                extract_page_text(&reread, 0, &TextEditLimits::default())
                    .unwrap()
                    .lines[0]
                    .text,
                "Hello zqx"
            );
            let pdfium = backend
                .pdfium
                .load_pdf_from_byte_vec(updated, None)
                .unwrap();
            // Every character: the typed text runs past the small page.
            let text = pdfium
                .pages()
                .get(0)
                .unwrap()
                .text()
                .unwrap()
                .chars()
                .iter()
                .filter_map(|character| character.unicode_char())
                .collect::<String>();
            assert!(text.contains("Hello zqx"), "{text:?}");
        }
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn page_text_edits_render_and_clear_without_changing_the_source_text() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let path = env::temp_dir().join(format!("bp-page-text-{}.pdf", std::process::id()));
        std::fs::write(&path, page_text_fixture()).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
            let request = retained_test_request(AnnotationRenderMode::RetainedOnly, 160);
            let mut pixels = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                .unwrap();
            // Dark text pixels on the white page.
            pixels
                .chunks(4)
                .filter(|pixel| pixel[0] < 128 && pixel[3] > 0)
                .count()
        };
        let original = render(&mut backend, &mut document);
        assert!(original > 100, "the source text renders");

        let layer = backend.page_text(&mut document, 0).unwrap();
        let line = &layer.lines[0];
        assert_eq!(line.text, "Hello");
        let emptied = PageTextEdit {
            page_index: 0,
            anchor: line.anchor,
            original: line.original_runs(),
            runs: Vec::new(),
            layout: Default::default(),
        };
        backend
            .set_document_edits(
                &mut document,
                &DocumentEdits::from_text(std::slice::from_ref(&emptied)),
            )
            .unwrap();
        assert_eq!(render(&mut backend, &mut document), 0, "the edit renders");
        // Page text and later edits still address the unedited source.
        assert_eq!(
            backend.page_text(&mut document, 0).unwrap().lines[0].text,
            "Hello"
        );

        backend
            .set_document_edits(&mut document, &DocumentEdits::from_text(&[]))
            .unwrap();
        assert_eq!(render(&mut backend, &mut document), original);
    }

    /// A table edit renders through PDFium: a grown row moves the rows
    /// below and their borders down, and a new row draws its model's
    /// borders and its new cell text.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn table_edits_render_grown_and_new_rows() {
        use butter_paper::pdf_tables::{
            BandSource, PageTableEdit, TableBand, TableCellLine, TableCellText, line_box,
            page_tables,
        };
        use butter_paper::pdf_text_edit::TextEditRun;
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let bytes = include_bytes!("../../tests/fixtures/text-edit/chrome-table.pdf");
        let path = env::temp_dir().join(format!("bp-table-{}.pdf", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        let (width, height) = (612_u32, 792_u32);
        let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
            let mut request = retained_test_request(AnnotationRenderMode::RetainedOnly, width);
            request.clip.height = height;
            request.surface.height = height;
            request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
            let mut pixels = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                .unwrap();
            pixels
        };
        // Dark pixels in a page-space rectangle: left, bottom, right, top.
        let dark_in = |pixels: &[u8], rect: [f64; 4]| {
            let mut count = 0;
            let top = (f64::from(height) - rect[3]).max(0.) as u32;
            let bottom = ((f64::from(height) - rect[1]) as u32).min(height);
            for y in top..bottom {
                for x in rect[0].max(0.) as u32..(rect[2] as u32).min(width) {
                    let pixel = &pixels[((y * width + x) * 4) as usize..][..4];
                    if pixel[0] < 110 && pixel[1] < 110 && pixel[2] < 110 && pixel[3] > 0 {
                        count += 1;
                    }
                }
            }
            count
        };
        let layer = backend.page_text(&mut document, 0).unwrap();
        let table = page_tables(&layer).remove(0);
        let washers = layer
            .lines
            .iter()
            .find(|line| line.text.trim() == "Washers")
            .and_then(line_box)
            .unwrap();
        let before = render(&mut backend, &mut document);
        assert!(dark_in(&before, washers) > 20, "the table renders");

        let mut edit = PageTableEdit::unchanged(0, &table);
        edit.rows[1].size += 20.;
        let size = edit.rows[3].size;
        edit.rows.push(TableBand {
            source: BandSource::New(3),
            size,
            set_size: None,
        });
        let style = layer.line(&table.cells[9].lines[0]).unwrap().styles[0].clone();
        edit.cell_texts.push(TableCellText {
            row: 4,
            column: 0,
            lines: vec![TableCellLine {
                x: 6.,
                baseline: 14.,
                runs: vec![TextEditRun {
                    style,
                    text: "Nuts and bolts".into(),
                }],
                hard_break: false,
            }],
        });
        backend
            .set_document_edits(
                &mut document,
                &DocumentEdits {
                    tables: vec![edit.clone()],
                    ..DocumentEdits::default()
                },
            )
            .unwrap();
        let after = render(&mut backend, &mut document);
        let moved = [washers[0], washers[1] - 20., washers[2], washers[3] - 20.];
        assert!(dark_in(&after, washers) < 5, "the row below moved away");
        assert!(dark_in(&after, moved) > 20, "the row below is drawn lower");
        let rows = edit.output_rows();
        let [left, _, right, _] = table.bounds();
        // The new row's bottom border, a model row's copy, spans the table.
        let border = dark_in(&after, [left, rows[5] - 1.5, right, rows[5] + 1.5]);
        assert!(border as f64 > 0.8 * (right - left), "{border}");
        // Its text is drawn in its first cell.
        let text = [left + 4., rows[4] - 20., left + 120., rows[4] - 2.];
        assert!(dark_in(&before, text) == 0 || dark_in(&before, text) < dark_in(&after, text));
        assert!(dark_in(&after, text) > 30, "the new cell text renders");

        backend
            .set_document_edits(&mut document, &DocumentEdits::default())
            .unwrap();
        assert_eq!(render(&mut backend, &mut document), before);
    }

    /// Times what typing in a growing table cell costs the worker per
    /// keystroke: the edits applied, the shown document rebuilt and the
    /// page redrawn. Prints the times; run with `--nocapture`.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn table_typing_latency() {
        use butter_paper::pdf_tables::{PageTableEdit, TableCellLine, TableCellText, page_tables};
        use butter_paper::pdf_text_edit::TextEditRun;
        use std::time::Instant;
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let cases = [
            ("one light page", 1, 12, 0),
            ("one heavy page", 1, 38, 40_000),
            ("400 heavy pages", 400, 38, 40_000),
        ];
        for (name, pages, rows, paths) in cases {
            let bytes = super::table_pages::table_pages_pdf(pages, rows, paths);
            let path = env::temp_dir().join(format!("bp-latency-{}.pdf", std::process::id()));
            std::fs::write(&path, &bytes).unwrap();
            let source = File::open(&path).unwrap();
            let started = Instant::now();
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
            let opened = started.elapsed();
            std::fs::remove_file(&path).unwrap();
            let page = pages as u32 / 2;
            let layer = backend.page_text(&mut document, page).unwrap();
            let table = page_tables(&layer).remove(0);
            let style = layer.line(&table.cells[5].lines[0]).unwrap().styles[0].clone();
            let width = 1200_u32;
            // The whole page at about 145% zoom on a Retina display.
            let mut request = retained_test_request(AnnotationRenderMode::RetainedOnly, width);
            let scale = width as f32 / 612.;
            let height = (792. * scale) as u32;
            request.page_index = page;
            request.transform = [scale, 0., 0., scale, 0., 0.];
            request.clip.height = height;
            request.surface.height = height;
            request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
            let mut pixels = vec![0; request.surface.byte_len as usize];
            let mut edit_times = Vec::new();
            let mut render_times = Vec::new();
            let mut text = String::new();
            for keystroke in 0..24 {
                text.push(if keystroke % 6 == 5 { ' ' } else { 'x' });
                // Each keystroke's edit: the cell's new text, its row grown
                // a line every twelve characters.
                let mut edit = PageTableEdit::unchanged(page, &table);
                edit.rows[1].size += 12. * (keystroke / 12) as f64;
                edit.cell_texts.push(TableCellText {
                    row: rows - 1,
                    column: 1,
                    lines: vec![TableCellLine {
                        x: 4.,
                        baseline: 13.,
                        runs: vec![TextEditRun {
                            style: style.clone(),
                            text: text.clone(),
                        }],
                        hard_break: false,
                    }],
                });
                let started = Instant::now();
                backend
                    .set_document_edits(
                        &mut document,
                        &DocumentEdits {
                            tables: vec![edit],
                            ..DocumentEdits::default()
                        },
                    )
                    .unwrap();
                edit_times.push(started.elapsed());
                let started = Instant::now();
                backend
                    .render_crop(
                        &mut document,
                        &request,
                        &mut pixels,
                        &AtomicBool::new(false),
                    )
                    .unwrap();
                render_times.push(started.elapsed());
            }
            let summary = |times: &mut Vec<std::time::Duration>| {
                times.sort();
                format!(
                    "median {:>6.1} ms, worst {:>6.1} ms",
                    times[times.len() / 2].as_secs_f64() * 1000.,
                    times[times.len() - 1].as_secs_f64() * 1000.
                )
            };
            eprintln!(
                "{name:>16} ({:>5.1} MB, opened in {:>6.1} ms): edits {}; page drawn {}",
                bytes.len() as f64 / 1e6,
                opened.as_secs_f64() * 1000.,
                summary(&mut edit_times),
                summary(&mut render_times),
            );
            backend.close(document);
        }
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_kept_page_shows_the_page_as_the_earlier_file_had_it() {
        use butter_paper::pdf_page_base::{PageBase, snapshot_page};
        use butter_paper::pdf_tables::{PageTableEdit, line_box, page_tables};
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        // The file as saved with its second row grown 20 points.
        let earlier = PdfMetadataDocument::load_mem(include_bytes!(
            "../../tests/fixtures/text-edit/chrome-table.pdf"
        ))
        .unwrap();
        let earlier_layer = extract_page_text(&earlier, 0, &TextEditLimits::default()).unwrap();
        let table = page_tables(&earlier_layer).remove(0);
        let mut edit = PageTableEdit::unchanged(0, &table);
        edit.rows[1].size += 20.;
        let mut saved = earlier.clone();
        butter_paper::pdf_text_edit::apply_content_edits(
            &mut saved,
            &[],
            &[edit],
            &[],
            &TextEditLimits::default(),
        )
        .unwrap();
        let mut bytes = Vec::new();
        saved.save_to(&mut bytes).unwrap();
        let path = env::temp_dir().join(format!("bp-kept-{}.pdf", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        let (width, height) = (612_u32, 792_u32);
        let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
            let mut request = retained_test_request(AnnotationRenderMode::RetainedOnly, width);
            request.clip.height = height;
            request.surface.height = height;
            request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
            let mut pixels = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                .unwrap();
            pixels
        };
        let dark_in = |pixels: &[u8], rect: [f64; 4]| {
            let mut count = 0;
            let top = (f64::from(height) - rect[3]).max(0.) as u32;
            let bottom = ((f64::from(height) - rect[1]) as u32).min(height);
            for y in top..bottom {
                for x in rect[0].max(0.) as u32..(rect[2] as u32).min(width) {
                    let pixel = &pixels[((y * width + x) * 4) as usize..][..4];
                    if pixel[0] < 110 && pixel[1] < 110 && pixel[2] < 110 && pixel[3] > 0 {
                        count += 1;
                    }
                }
            }
            count
        };
        let washers = earlier_layer
            .lines
            .iter()
            .find(|line| line.text.trim() == "Washers")
            .and_then(line_box)
            .unwrap();
        let saved_render = render(&mut backend, &mut document);
        assert!(
            dark_in(&saved_render, washers) < 5,
            "the saved row is lower"
        );

        // Undo past the save: the page as the earlier file had it.
        let kept = PageBase::new(0, std::sync::Arc::new(snapshot_page(&earlier, 0).unwrap()));
        backend
            .add_page_base(
                &mut document,
                kept.id.clone(),
                kept.pdf.as_deref().unwrap().clone(),
            )
            .unwrap();
        let text = |layer: &PageTextLayer| {
            layer
                .lines
                .iter()
                .map(|line| (line.text.clone(), line.origin()))
                .collect::<Vec<_>>()
        };
        let kept_layer = backend.page_base_text(&mut document, 0, &kept.id).unwrap();
        assert_eq!(text(&kept_layer), text(&earlier_layer));
        assert_ne!(
            text(&backend.page_text(&mut document, 0).unwrap()),
            text(&earlier_layer)
        );
        let reference = PageBase {
            pdf: None,
            ..kept.clone()
        };
        backend
            .set_document_edits(
                &mut document,
                &DocumentEdits {
                    bases: vec![reference],
                    ..DocumentEdits::default()
                },
            )
            .unwrap();
        let undone = render(&mut backend, &mut document);
        assert!(dark_in(&undone, washers) > 20, "the row is where it was");

        // A kept page the worker was not sent is refused.
        let unknown = PageBase {
            id: "0".repeat(64),
            pdf: None,
            page_index: 0,
        };
        assert!(
            backend
                .set_document_edits(
                    &mut document,
                    &DocumentEdits {
                        bases: vec![unknown],
                        ..DocumentEdits::default()
                    },
                )
                .is_err()
        );
        backend
            .set_document_edits(&mut document, &DocumentEdits::default())
            .unwrap();
        assert_eq!(render(&mut backend, &mut document), saved_render);
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn filled_form_fields_render_and_unfilled_ones_draw_without_appearances() {
        use butter_paper::pdf_forms::{FormFieldEdit, FormValue, extract_form_layer};
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let bytes = include_bytes!("../../tests/fixtures/forms/libreoffice.pdf");
        let form = extract_form_layer(&PdfMetadataDocument::load_mem(bytes).unwrap());
        let path = env::temp_dir().join(format!("bp-form-{}.pdf", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        // The whole page at one pixel per point.
        let (width, height) = (595_u32, 842_u32);
        let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
            let mut request = retained_test_request(AnnotationRenderMode::RetainedOnly, width);
            request.clip.height = height;
            request.surface.height = height;
            request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
            let mut pixels = vec![0; request.surface.byte_len as usize];
            backend
                .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                .unwrap();
            pixels
        };
        let dark_in = |pixels: &[u8], name: &str| {
            let rect = form.field(name).unwrap().widgets[0].rect;
            let mut count = 0;
            for y in (height as f64 - rect[3]).max(0.) as u32..(height as f64 - rect[1]) as u32 {
                for x in rect[0] as u32..rect[2] as u32 {
                    let pixel = &pixels[((y * width + x) * 4) as usize..][..4];
                    if pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100 && pixel[3] > 0 {
                        count += 1;
                    }
                }
            }
            count
        };
        let before = render(&mut backend, &mut document);
        // Fields without appearances are drawn, not marked as missing ones.
        let red = before
            .chunks(4)
            .filter(|pixel| pixel[2] > 200 && pixel[1] < 120 && pixel[0] < 120)
            .count();
        assert_eq!(red, 0, "no missing-appearance markers");
        assert!(
            dark_in(&before, "Reference") > 20,
            "the prefilled value is drawn"
        );
        let empty_name = dark_in(&before, "ApplicantName");

        let check = form.field("SiteReady").unwrap();
        let edits = DocumentEdits {
            text: Vec::new(),
            forms: vec![
                FormFieldEdit {
                    name: "ApplicantName".into(),
                    original: FormValue::Text(String::new()),
                    value: FormValue::Text("Ada Lovelace".into()),
                },
                FormFieldEdit {
                    name: "SiteReady".into(),
                    original: check.value.clone(),
                    value: FormValue::Button(check.widgets[0].on_state.clone()),
                },
            ],
            tables: Vec::new(),
            moves: Vec::new(),
            bases: Vec::new(),
            pages: Vec::new(),
        };
        backend.set_document_edits(&mut document, &edits).unwrap();
        let filled = render(&mut backend, &mut document);
        assert!(
            dark_in(&filled, "ApplicantName") > empty_name + 30,
            "the filled name renders"
        );
        assert!(
            dark_in(&filled, "SiteReady") > dark_in(&before, "SiteReady"),
            "the check box shows checked"
        );

        backend
            .set_document_edits(&mut document, &DocumentEdits::default())
            .unwrap();
        assert_eq!(render(&mut backend, &mut document), before);
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn arranged_pages_render_in_their_new_order() {
        use butter_paper::pdf_page_structure::PageSlot;
        use lopdf::{Document, Object, Stream, dictionary};
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        // A white page and a black one, of different sizes.
        let mut file = Document::with_version("1.7");
        let root = file.new_object_id();
        let mut kids = Vec::new();
        for (fill, width) in [("1 g", 200), ("0 g", 300)] {
            let content = file.add_object(Stream::new(
                dictionary! {},
                format!("{fill} 0 0 {width} 100 re f").into_bytes(),
            ));
            kids.push(Object::Reference(file.add_object(dictionary! {
                "Type" => "Page", "Parent" => root, "Contents" => content,
                "MediaBox" => vec![0.into(), 0.into(), width.into(), 100.into()],
            })));
        }
        file.objects.insert(
            root,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 2 }),
        );
        let catalog = file.add_object(dictionary! { "Type" => "Catalog", "Pages" => root });
        file.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        file.save_to(&mut bytes).unwrap();
        let path = env::temp_dir().join(format!("bp-arranged-{}.pdf", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        let edits = DocumentEdits {
            pages: vec![
                PageSlot::Source { index: 1 },
                PageSlot::Blank {
                    width: 150.,
                    height: 150.,
                },
                PageSlot::Copy { index: 1 },
                PageSlot::Source { index: 0 },
            ],
            ..DocumentEdits::default()
        };
        backend.set_document_edits(&mut document, &edits).unwrap();
        assert_eq!(document.document.pages().len(), 4);
        let widths = (0..4)
            .map(|page| document.document.pages().get(page).unwrap().width().value)
            .collect::<Vec<_>>();
        assert_eq!(widths, [300., 150., 300., 200.]);
        // The first page is the black one now.
        let mut request = retained_test_request(AnnotationRenderMode::RetainedOnly, 30);
        request.clip.height = 10;
        request.surface.height = 10;
        request.surface.byte_len = 30 * 10 * 4;
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
            pixels.chunks(4).all(|pixel| pixel[0] < 40),
            "page 1 is black"
        );
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_re_encrypted_owner_password_pdf_opens_and_renders_in_pdfium() {
        use lopdf::{Document, EncryptionState, EncryptionVersion, Object, Permissions};
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let mut document = Document::load_mem(&page_text_fixture()).unwrap();
        let file_id = Object::string_literal("butter-paper-enc");
        document.trailer.set("ID", vec![file_id.clone(), file_id]);
        let state = EncryptionState::try_from(EncryptionVersion::V2 {
            document: &document,
            owner_password: "owner",
            user_password: "",
            key_length: 128,
            permissions: Permissions::PRINTABLE,
        })
        .unwrap();
        document.encrypt(&state).unwrap();
        let mut encrypted = Vec::new();
        document.save_to(&mut encrypted).unwrap();
        // As Save does: open (decrypting), then encrypt again on writing.
        let mut opened = Document::load_mem(&encrypted).unwrap();
        let state = opened.encryption_state.clone().unwrap();
        opened.encrypt(&state).unwrap();
        let mut saved = Vec::new();
        opened.save_to(&mut saved).unwrap();

        let path = env::temp_dir().join(format!("bp-re-encrypted-{}.pdf", std::process::id()));
        std::fs::write(&path, &saved).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
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
        let dark = pixels
            .chunks(4)
            .filter(|pixel| pixel[0] < 128 && pixel[3] > 0)
            .count();
        assert!(dark > 100, "the decrypted text renders");
        assert_eq!(
            backend.page_text(&mut document, 0).unwrap().lines[0].text,
            "Hello"
        );
    }

    /// A 300-page document of about 30 MB: a text line and an
    /// incompressible 100 KB image on every page.
    fn large_text_fixture() -> Vec<u8> {
        use lopdf::{Document, Object, Stream, dictionary};
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let font_id = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let mut kids = Vec::new();
        for page in 0..300 {
            let pixels = (0..100 * 1024)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    seed as u8
                })
                .collect::<Vec<_>>();
            let image_id = document.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject",
                    "Subtype" => "Image",
                    "Width" => 320,
                    "Height" => 320,
                    "ColorSpace" => "DeviceGray",
                    "BitsPerComponent" => 8,
                },
                pixels,
            ));
            let content_id = document.add_object(Stream::new(
                dictionary! {},
                format!(
                    "q 200 0 0 200 300 400 cm /Im0 Do Q BT /F1 24 Tf 72 700 Td (Page {page} heading text) Tj ET"
                )
                .into_bytes(),
            ));
            kids.push(Object::Reference(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content_id,
                "Resources" => dictionary! {
                    "Font" => dictionary! { "F1" => font_id },
                    "XObject" => dictionary! { "Im0" => image_id },
                },
            })));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => kids,
                "Count" => 300,
            }),
        );
        let catalog_id =
            document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn bench_text_edit_update_time_on_a_large_document() {
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let bytes = large_text_fixture();
        let path = env::temp_dir().join(format!("bp-large-text-{}.pdf", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        // The page is on screen before it is edited.
        let mut request = retained_test_request(AnnotationRenderMode::None, 160);
        request.page_index = 150;
        let mut pixels = vec![0; request.surface.byte_len as usize];
        backend
            .render_crop(
                &mut document,
                &request,
                &mut pixels,
                &AtomicBool::new(false),
            )
            .unwrap();
        let layer = backend.page_text(&mut document, 150).unwrap();
        let line = &layer.lines[0];
        let mut timings = Vec::new();
        for text in [
            "Page 150 heading textX",
            "Page 150 heading textXY",
            "Page 150 heading textXYZ",
        ] {
            let edit = PageTextEdit {
                page_index: 150,
                anchor: line.anchor,
                original: line.original_runs(),
                runs: vec![butter_paper::pdf_text_edit::TextEditRun {
                    style: line.styles[0].clone(),
                    text: text.into(),
                }],
                layout: Default::default(),
            };
            let started = std::time::Instant::now();
            backend
                .set_document_edits(&mut document, &DocumentEdits::from_text(&[edit]))
                .unwrap();
            backend
                .render_crop(
                    &mut document,
                    &request,
                    &mut pixels,
                    &AtomicBool::new(false),
                )
                .unwrap();
            timings.push(started.elapsed());
        }
        eprintln!(
            "{} MB, 300 pages: keystroke to tile {:?}",
            bytes.len() / 1_000_000,
            timings
        );
        let budget = std::time::Duration::from_millis(
            env::var("BP_TEXT_EDIT_BUDGET_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(250),
        );
        assert!(
            timings.iter().all(|timing| *timing < budget),
            "each update must stay under {budget:?}: {timings:?}"
        );
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn form_xobject_text_edits_render_on_their_page_only() {
        use butter_paper::pdf_text_edit::TextEditRun;
        use lopdf::{Dictionary, Document, Object, Stream, dictionary};
        // Two pages drawing one form whose text says "Shared form text".
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let font = document.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
            "Encoding" => "WinAnsiEncoding",
        });
        let form = document.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 400.into(), 100.into()],
                "Matrix" => vec![1.into(), 0.into(), 0.into(), 1.into(), 40.into(), 400.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            },
            b"BT /F1 24 Tf 0 20 Td (Shared form text) Tj ET".to_vec(),
        ));
        let mut kids = Vec::new();
        for _ in 0..2 {
            let content =
                document.add_object(Stream::new(Dictionary::new(), b"q /Fm0 Do Q".to_vec()));
            kids.push(Object::Reference(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => content,
                "Resources" => dictionary! { "XObject" => dictionary! { "Fm0" => form } },
            })));
        }
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => 2 }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();

        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let path = env::temp_dir().join(format!("bp-form-text-{}.pdf", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (mut opened, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(path).unwrap();
        let layer = backend.page_text(&mut opened, 0).unwrap();
        let line = &layer.lines[0];
        assert_eq!(line.text, "Shared form text");
        let edit = PageTextEdit {
            page_index: 0,
            anchor: line.anchor,
            original: line.original_runs(),
            runs: vec![TextEditRun {
                style: line.styles[0].clone(),
                text: "Edited form text Ж".into(),
            }],
            layout: Default::default(),
        };
        backend
            .set_document_edits(&mut opened, &DocumentEdits::from_text(&[edit]))
            .unwrap();
        let text = |opened: &PdfiumDocument, page: i32| {
            opened
                .document
                .pages()
                .get(page)
                .unwrap()
                .text()
                .unwrap()
                .all()
        };
        assert_eq!(text(&opened, 0).trim(), "Edited form text Ж");
        assert_eq!(text(&opened, 1).trim(), "Shared form text");
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn paragraph_reflow_renders_and_reads_back_in_pdfium() {
        use butter_paper::pdf_text_blocks::paragraph_blocks;
        use butter_paper::pdf_text_reflow::Paragraph;
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for name in ["chrome", "libreoffice"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/text-edit")
                .join(format!("{name}.pdf"));
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
            let (width, height) = {
                let page = document.document.pages().get(0).unwrap();
                (
                    page.width().value.ceil() as u32,
                    page.height().value.ceil() as u32,
                )
            };
            let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
                let mut request = retained_test_request(AnnotationRenderMode::None, width);
                request.clip.height = height;
                request.surface.height = height;
                request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
                let mut pixels = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                    .unwrap();
                pixels
            };
            let before = render(&mut backend, &mut document);
            let layer = std::sync::Arc::new(backend.page_text(&mut document, 0).unwrap());
            let block = paragraph_blocks(&layer)
                .into_iter()
                .find(|block| {
                    layer
                        .line(&block.lines[0])
                        .unwrap()
                        .text
                        .starts_with("The contractor")
                })
                .unwrap();
            let paragraph = Paragraph::new(layer.clone(), &block).unwrap();
            let mut text = paragraph.text(&[], &[]);
            let style = text[4].1.clone();
            let insert = " together with all welding, bolting and painting of every member";
            text.splice(
                15..15,
                insert.chars().map(|character| (character, style.clone())),
            );
            let wrapped = paragraph.wrap(&text).unwrap();
            let edits = paragraph.edits(&text, &wrapped);
            backend
                .set_document_edits(&mut document, &DocumentEdits::from_text(&edits))
                .unwrap();
            let after = render(&mut backend, &mut document);

            // PDFium reads the paragraph as edited.
            let normalise = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
            let read = normalise(
                &document
                    .document
                    .pages()
                    .get(0)
                    .unwrap()
                    .text()
                    .unwrap()
                    .all(),
            );
            let expected = normalise(
                &text
                    .iter()
                    .map(|(character, _)| *character)
                    .collect::<String>(),
            );
            assert!(read.contains(&expected), "{name}: {read}");

            // Pixels change only from the paragraph's top down to its last
            // line, added lines included.
            let top = height as f64 - layer.line(&block.lines[0]).unwrap().quad(0., 1.)[3].1 - 4.;
            let last = wrapped.last().unwrap();
            let bottom = height as f64 - last.line.quad(0., 1.)[0].1 + 4.;
            for (index, (old, new)) in before.chunks(4).zip(after.chunks(4)).enumerate() {
                if old != new {
                    let y = (index as u32 / width) as f64;
                    assert!(
                        (top..=bottom).contains(&y),
                        "{name}: row {y} outside {top}..{bottom}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_list_item_added_with_enter_reads_back_in_pdfium() {
        use butter_paper::page_text_editor::PageTextEditState;
        use butter_paper::pdf_text_blocks::paragraph_blocks;
        use butter_paper::pdf_text_reflow::Paragraph;
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/text-edit/libreoffice.pdf");
        let source = File::open(&path).unwrap();
        let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
        let layer = std::sync::Arc::new(backend.page_text(&mut document, 0).unwrap());
        let block = paragraph_blocks(&layer)
            .into_iter()
            .find(|block| block.list.is_some())
            .unwrap();
        let paragraph = Paragraph::new(layer.clone(), &block).unwrap();
        let mut state = PageTextEditState::new(paragraph, &[], &[], &[], 0).unwrap();
        let first_break = state.text().chars().position(|c| c == '\n').unwrap();
        state.move_to(first_break, false);
        assert!(state.insert_newline());
        assert!(state.insert("Weld inspection"));
        backend
            .set_document_edits(&mut document, &DocumentEdits::from_text(&state.edits()))
            .unwrap();

        let normalise = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let read = normalise(
            &document
                .document
                .pages()
                .get(0)
                .unwrap()
                .text()
                .unwrap()
                .all(),
        );
        let items = [
            "• Grade S355 steel for primary members.",
            "• Weld inspection",
            "• Hot-dip galvanised bolts, nuts and washers.",
            "• Office efficient fluid workflow, café façade naïve.",
        ];
        assert!(read.contains(&items.join(" ")), "{read}");
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn text_properties_render_and_read_back_in_pdfium() {
        use butter_paper::pdf_text_blocks::{TextBlockAlignment, paragraph_blocks};
        use butter_paper::pdf_text_edit::{
            FallbackFace, FallbackFamily, ParagraphFormat, TextFace,
        };
        use butter_paper::pdf_text_reflow::Paragraph;
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for name in ["chrome", "libreoffice"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/text-edit")
                .join(format!("{name}.pdf"));
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
            let (width, height) = {
                let page = document.document.pages().get(0).unwrap();
                (
                    page.width().value.ceil() as u32,
                    page.height().value.ceil() as u32,
                )
            };
            let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
                let mut request = retained_test_request(AnnotationRenderMode::None, width);
                request.clip.height = height;
                request.surface.height = height;
                request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
                let mut pixels = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                    .unwrap();
                pixels
            };
            let before = render(&mut backend, &mut document);
            let layer = std::sync::Arc::new(backend.page_text(&mut document, 0).unwrap());
            let block = paragraph_blocks(&layer)
                .into_iter()
                .find(|block| {
                    layer
                        .line(&block.lines[0])
                        .unwrap()
                        .text
                        .starts_with("The contractor")
                })
                .unwrap();
            let format = ParagraphFormat {
                alignment: Some(TextBlockAlignment::Right),
                line_spacing: Some(1.6),
            };
            let paragraph = Paragraph::new(layer.clone(), &block)
                .unwrap()
                .with_format(format);
            let mut text = paragraph.text(&[], &[]);
            // "contractor": red, bold, larger, in the bundled serif.
            let face = TextFace {
                postscript_name: None,
                fallback: FallbackFace {
                    family: FallbackFamily::Serif,
                    bold: true,
                    italic: false,
                },
            };
            for (_, style) in &mut text[4..14] {
                style.fill = Some([1., 0., 0.]);
                style.face = Some(face.clone());
                style.font_size *= 1.25;
            }
            let wrapped = paragraph.wrap(&text).unwrap();
            let edits = paragraph.edits(&text, &wrapped);
            backend
                .set_document_edits(&mut document, &DocumentEdits::from_text(&edits))
                .unwrap();
            let after = render(&mut backend, &mut document);

            let normalise = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
            let read = normalise(
                &document
                    .document
                    .pages()
                    .get(0)
                    .unwrap()
                    .text()
                    .unwrap()
                    .all(),
            );
            let expected = normalise(
                &text
                    .iter()
                    .map(|(character, _)| *character)
                    .collect::<String>(),
            );
            assert!(read.contains(&expected), "{name}: {read}");

            // The word is drawn red; nothing outside the paragraph changes.
            let red = after
                .chunks(4)
                .filter(|pixel| {
                    let (high, low) = (pixel[0].max(pixel[2]), pixel[0].min(pixel[2]));
                    high > 180 && low < 90 && pixel[1] < 90
                })
                .count();
            assert!(red > 20, "{name}: {red} red pixels");
            let top = height as f64 - layer.line(&block.lines[0]).unwrap().quad(0., 1.)[3].1 - 8.;
            let bottom = height as f64
                - wrapped
                    .iter()
                    .map(|line| line.line.quad(0., 1.)[0].1)
                    .fold(f64::INFINITY, f64::min)
                + 8.;
            let bottom = bottom.max(
                height as f64
                    - layer
                        .line(block.lines.last().unwrap())
                        .unwrap()
                        .quad(0., 1.)[0]
                        .1
                    + 8.,
            );
            for (index, (old, new)) in before.chunks(4).zip(after.chunks(4)).enumerate() {
                if old != new {
                    let y = (index as u32 / width) as f64;
                    assert!(
                        (top..=bottom).contains(&y),
                        "{name}: row {y} outside {top}..{bottom}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn producer_text_edits_change_only_their_lines_and_read_back_in_pdfium() {
        use butter_paper::pdf_text_edit::{TextEditRun, layout_text_edit};
        let library = env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY");
        let (_guard, mut backend) = test_backend(Path::new(&library));
        for name in ["chrome", "libreoffice", "quartz"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/text-edit")
                .join(format!("{name}.pdf"));
            let source = File::open(&path).unwrap();
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
            let (page_width, page_height) = {
                let page = document.document.pages().get(0).unwrap();
                (
                    f64::from(page.width().value),
                    f64::from(page.height().value),
                )
            };
            let (width, height) = (page_width.ceil() as u32, page_height.ceil() as u32);
            let render = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument| {
                let mut request = retained_test_request(AnnotationRenderMode::None, width);
                request.clip.height = height;
                request.surface.height = height;
                request.surface.byte_len = u64::from(width) * u64::from(height) * 4;
                let mut pixels = vec![0; request.surface.byte_len as usize];
                backend
                    .render_crop(document, &request, &mut pixels, &AtomicBool::new(false))
                    .unwrap();
                pixels
            };
            let before = render(&mut backend, &mut document);

            // Edit every other line: insert a word with a fallback letter in
            // the middle and drop as many letters from the end, so lines that
            // fill the page width stay on it (PDFium reads no text past it).
            let layer = backend.page_text(&mut document, 0).unwrap();
            let inserted = "Zebra Ж ";
            let edits = layer
                .lines
                .iter()
                .filter(|line| {
                    line.read_only.is_none()
                        && line.text.trim().chars().count() > 2 * inserted.chars().count()
                })
                .step_by(2)
                .map(|line| {
                    let chars = line.text.trim_end().chars().collect::<Vec<_>>();
                    let middle = chars.len() / 2;
                    let text = chars[..middle].iter().collect::<String>()
                        + inserted
                        + &chars[middle..chars.len() - inserted.chars().count()]
                            .iter()
                            .collect::<String>();
                    let style =
                        line.styles[line.chars[middle.min(line.chars.len() - 1)].style].clone();
                    let mut runs = line.original_runs();
                    runs.clear();
                    runs.push(TextEditRun { style, text });
                    PageTextEdit {
                        page_index: 0,
                        anchor: line.anchor,
                        original: line.original_runs(),
                        runs,
                        layout: Default::default(),
                    }
                })
                .collect::<Vec<_>>();
            assert!(edits.len() >= 2, "{name}");
            backend
                .set_document_edits(&mut document, &DocumentEdits::from_text(&edits))
                .unwrap();
            let after = render(&mut backend, &mut document);

            // Pixels may change only inside the edited lines' boxes (the
            // wider of the original and edited line), with a small margin.
            let boxes = edits
                .iter()
                .map(|edit| {
                    let line = layer.line(&edit.anchor).unwrap();
                    let laid = layout_text_edit(&layer, line, &edit.runs).unwrap();
                    let quad = line.quad(0f64.min(laid.caret_x(0)), line.end_x.max(laid.end_x));
                    let xs = quad.iter().map(|(x, _)| *x);
                    let ys = quad.iter().map(|(_, y)| page_height - *y);
                    (
                        xs.clone().fold(f64::MAX, f64::min) - 3.,
                        ys.clone().fold(f64::MAX, f64::min) - 3.,
                        xs.fold(f64::MIN, f64::max) + 3.,
                        ys.fold(f64::MIN, f64::max) + 3.,
                    )
                })
                .collect::<Vec<_>>();
            let mut changed_inside = 0;
            for (index, (old, new)) in before.chunks(4).zip(after.chunks(4)).enumerate() {
                if old == new {
                    continue;
                }
                let (x, y) = ((index as u32 % width) as f64, (index as u32 / width) as f64);
                let inside = boxes.iter().any(|(left, top, right, bottom)| {
                    (*left..=*right).contains(&x) && (*top..=*bottom).contains(&y)
                });
                assert!(
                    inside,
                    "{name}: pixel {x},{y} changed outside the edited lines"
                );
                changed_inside += 1;
            }
            assert!(changed_inside > 100, "{name}: the edits render");

            // PDFium reads the edited text from the edited document.
            let text = document
                .document
                .pages()
                .get(0)
                .unwrap()
                .text()
                .unwrap()
                .all();
            let normalise = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
            let text = normalise(&text);
            for edit in &edits {
                assert!(
                    text.contains(&normalise(&edit.text())),
                    "{name}: PDFium does not read {:?} in {text:?}",
                    edit.text()
                );
            }
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
            let (mut document, _) = backend.open(source_handle(&source), None).unwrap();
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
        let (mut document, info) = backend.open(source_handle(&source), None).unwrap();
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
            draft: false,
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

#[cfg(test)]
#[path = "../../tests/support/table_pages.rs"]
mod table_pages;

#[cfg(test)]
#[path = "butter-paper-pdf-worker/corpus.rs"]
mod corpus;
