use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use crate::document_recovery_store::{RecoveryAuthority, RecoverySourceKind};
use crate::{
    adaptive_performance::AdaptiveViewerPerformance,
    document_resource::{
        DEFAULT_PAGE_RENDER_WIDTH, DocumentId, NativeDocumentResource, OpenedNativeDocument,
        RasterSurface, coordinate_rotation,
    },
    document_viewer::DocumentViewerState,
    native_document_view_state::NativeDocumentViewState,
};
use crate::{
    annotation_adapter::{AnnotationAdapter, IMAGE_PLACEMENT_PREVIEW_OPACITY},
    annotation_model::{AnnotationSnapshot, PageRotation},
    generated_document::{GeneratedDocumentStore, OwnedGeneratedDocument},
    page_geometry::{PageCoordinateSpace, PdfRect as CoordinateRect},
    pdf_engine::InPlacePublicationCapability,
    pdf_file_authority::SaveAsTargetAuthority,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentSaveRoute {
    OpenedSource,
    NewTargetRequired,
}

pub const fn resolve_document_save_route(
    save_as_required: bool,
    capability: InPlacePublicationCapability,
) -> DocumentSaveRoute {
    if save_as_required || matches!(capability, InPlacePublicationCapability::NewTargetRequired) {
        DocumentSaveRoute::NewTargetRequired
    } else {
        DocumentSaveRoute::OpenedSource
    }
}
use gpui::RenderImage;
use image::{Frame, ImageBuffer, Rgba};
use smallvec::smallvec;

/// GPUI RenderImage stores BGRA even though its Frame container is named Rgba.
/// Keep decoded/model/export bytes in RGBA and convert only this upload copy.
pub(crate) fn decoded_asset_render_image(
    asset: &crate::annotation_model::DecodedRgbaAsset,
    opacity: f64,
) -> Option<Arc<RenderImage>> {
    let mut bgra = asset.rgba().to_vec();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if opacity != 1. {
            pixel[3] = (f64::from(pixel[3]) * opacity).round() as u8;
        }
    }
    let pixels =
        ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(asset.width_px(), asset.height_px(), bgra)?;
    Some(Arc::new(RenderImage::new(smallvec![Frame::new(pixels)])))
}

pub(crate) fn annotation_image_cache_key(asset_id: &str, opacity: f64) -> String {
    if opacity == 1. {
        asset_id.to_owned()
    } else {
        format!("{asset_id}@{opacity:.6}")
    }
}

#[derive(Debug)]
pub struct SaveDocumentRequest {
    pub document_id: DocumentId,
    pub generation: u64,
    pub source_path: PathBuf,
    pub destination: SaveDestination,
    pub current_page: u32,
    pub annotation_revision: u64,
    pub annotations: AnnotationSnapshot,
    pub expected_source_sha256: Option<[u8; 32]>,
}

#[derive(Debug)]
pub enum SaveDestination {
    OpenedSource,
    NewTarget(SaveAsTargetAuthority),
}

impl SaveDocumentRequest {
    pub fn is_in_place(&self) -> bool {
        matches!(self.destination, SaveDestination::OpenedSource)
    }

    pub fn target_path(&self) -> &Path {
        match &self.destination {
            SaveDestination::OpenedSource => &self.source_path,
            SaveDestination::NewTarget(authority) => authority.path(),
        }
    }
}

pub struct SavedNativeDocument {
    pub(crate) opened: OpenedNativeDocument,
    pub(crate) validated_revision: u64,
    pub(crate) publication_warning: Option<String>,
    /// Pages as the file had them before the save, which undo reaches back
    /// to ([`crate::annotation_model::AnnotationDocument::mark_saved_keeping`]).
    pub(crate) kept_pages: Vec<crate::pdf_page_base::PageBase>,
}

impl SavedNativeDocument {
    pub fn new(opened: OpenedNativeDocument, validated_revision: u64) -> Self {
        Self {
            opened,
            validated_revision,
            publication_warning: None,
            kept_pages: Vec::new(),
        }
    }

    pub fn with_kept_pages(mut self, pages: Vec<crate::pdf_page_base::PageBase>) -> Self {
        self.kept_pages = pages;
        self
    }

    pub fn with_publication_warning(mut self, warning: impl Into<String>) -> Self {
        self.publication_warning = Some(warning.into());
        self
    }

    pub fn publication_warning(&self) -> Option<&str> {
        self.publication_warning.as_deref()
    }

    pub fn kept_pages(&self) -> &[crate::pdf_page_base::PageBase] {
        &self.kept_pages
    }

    pub fn opened(&self) -> &OpenedNativeDocument {
        &self.opened
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeDocumentStatus {
    Opening,
    Ready,
    Failed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeDocumentSaveStatus {
    Idle,
    Saving,
    Failed(DocumentSaveFailure),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentSaveFailureOperation {
    InPlace,
    SaveAs,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DocumentSaveFailureKind {
    Retriable,
    ExternalSourceChanged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DocumentSaveFailure {
    pub generation: u64,
    pub operation: DocumentSaveFailureOperation,
    pub kind: DocumentSaveFailureKind,
    pub message: String,
}

impl DocumentSaveFailure {
    pub fn new(
        generation: u64,
        operation: DocumentSaveFailureOperation,
        message: impl Into<String>,
    ) -> Self {
        let message = message.into();
        let kind = if message.contains("source PDF changed")
            || message.contains("source PDF identity changed")
            || message.contains("source PDF directory changed")
        {
            DocumentSaveFailureKind::ExternalSourceChanged
        } else {
            DocumentSaveFailureKind::Retriable
        };
        Self {
            generation,
            operation,
            kind,
            message,
        }
    }

    pub fn can_retry(&self) -> bool {
        self.kind == DocumentSaveFailureKind::Retriable
    }

    pub fn presentation_message(&self) -> &str {
        match self.kind {
            DocumentSaveFailureKind::Retriable => &self.message,
            DocumentSaveFailureKind::ExternalSourceChanged => {
                "The source PDF changed outside Butter Paper. Use Save As to preserve this document's edits without replacing the newer source."
            }
        }
    }
}

pub(crate) struct ThumbnailPresentation {
    pub(crate) page_index: u32,
    pub(crate) base_raster: RasterSurface,
    pub(crate) image: Arc<RenderImage>,
    // Read where highlights are precomposed (Windows and Linux).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(crate) highlight_pixels: usize,
    /// The page rotation the thumbnail shows.
    pub(crate) rotation: PageRotation,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HighlightCompositeEvidence {
    pub annotation_revision: u64,
    pub current_page_pixels: usize,
    pub thumbnail_pixels: usize,
    pub viewer_tile_pixels: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PenAnnotationDefaults {
    pub color: String,
    pub width_pt: f64,
    pub opacity: f64,
}

pub(crate) struct ThumbnailRequestState {
    pub epoch: u64,
    pub rotation: PageRotation,
    /// The page's content revision the render shows.
    pub content: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PageContentGeometryRequestState {
    Pending { epoch: u64 },
    Ready { epoch: u64 },
    Failed { epoch: u64, message: String },
}

impl PageContentGeometryRequestState {
    pub(crate) fn epoch(&self) -> u64 {
        match self {
            Self::Pending { epoch } | Self::Ready { epoch } | Self::Failed { epoch, .. } => *epoch,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub enum DocumentRecoveryPreparation {
    #[default]
    Unbound,
    Pending {
        generation: u64,
        source_kind: RecoverySourceKind,
    },
    Ready {
        generation: u64,
        authority: RecoveryAuthority,
        source_kind: RecoverySourceKind,
    },
    Failed {
        generation: u64,
        source_kind: RecoverySourceKind,
        message: String,
    },
    Ambiguous {
        generation: u64,
        authority: RecoveryAuthority,
        source_kind: RecoverySourceKind,
        message: String,
    },
    RebaseFailed {
        generation: u64,
        authority: RecoveryAuthority,
        source_kind: RecoverySourceKind,
        target_path: PathBuf,
        target_sha256: [u8; 32],
        saved_revision: u64,
        message: String,
    },
}

impl DocumentRecoveryPreparation {
    pub fn edit_guard(&self) -> Result<(), String> {
        match self {
            Self::Unbound | Self::Ready { .. } => Ok(()),
            Self::Pending { .. } => {
                Err("Crash recovery is still preparing for this document".into())
            }
            Self::Failed { message, .. }
            | Self::Ambiguous { message, .. }
            | Self::RebaseFailed { message, .. } => Err(format!(
                "Crash recovery is unavailable for this document: {message}"
            )),
        }
    }
}

pub struct NativeDocumentSession {
    pub(crate) id: DocumentId,
    pub(crate) path: PathBuf,
    pub(crate) title: String,
    pub(crate) status: NativeDocumentStatus,
    pub(crate) page_sizes: Vec<(f32, f32)>,
    pub(crate) source_page_sizes: Vec<(f32, f32)>,
    pub(crate) source_page_rotations: Vec<PageRotation>,
    pub(crate) source_page_coordinate_spaces: Vec<PageCoordinateSpace>,
    /// The file's own pages, which the `source_page_*` lists show as
    /// arranged ([`Self::sync_page_arrangement`]).
    pub(crate) file_page_sizes: Vec<(f32, f32)>,
    pub(crate) file_page_rotations: Vec<PageRotation>,
    pub(crate) file_page_coordinate_spaces: Vec<PageCoordinateSpace>,
    /// The arrangement the `source_page_*` lists show, one slot a page.
    pub(crate) arranged_slots: Vec<crate::pdf_page_structure::PageSlot>,
    pub(crate) presentation_error: Option<String>,
    pub(crate) recovery_generation: Option<u64>,
    pub(crate) recovery_preparation_generation: u64,
    pub(crate) recovery_preparation: DocumentRecoveryPreparation,
    pub(crate) resource_epoch: u64,
    pub(crate) pending_rotation_generation: Option<u64>,
    pub(crate) current_page: u32,
    pub(crate) requested_page: u32,
    pub(crate) pending_navigation_scroll: Option<u32>,
    /// A page gone back to from the top of the next: it shows from its
    /// bottom, as scrolling up would reach it.
    pub(crate) navigation_from_below: Option<u32>,
    pub(crate) pending_zoom_anchor: Option<crate::zoom_anchor::ZoomAnchor>,
    /// A page to frame once the new layout is planned: centred for Fit
    /// Page, centred across for Fit Width.
    pub(crate) pending_page_frame: Option<(u32, crate::document_viewer::ViewerFitPreset)>,
    pub(crate) generation: u64,
    pub(crate) save_generation: u64,
    pub(crate) image_prepare_generation: u64,
    pub(crate) save_status: NativeDocumentSaveStatus,
    pub(crate) publication_durability_warning: Option<PublicationDurabilityWarning>,
    pub(crate) current_base_raster: Option<RasterSurface>,
    pub(crate) current_image: Option<Arc<RenderImage>>,
    /// The page `current_image` and `current_base_raster` show: in
    /// Continuous view the current page follows scrolling and can differ.
    pub(crate) current_image_page: Option<u32>,
    /// The page shown from its thumbnail while its render is on the way;
    /// `current_base_raster` is then empty.
    pub(crate) placeholder_page: Option<u32>,
    pub(crate) thumbnails: Vec<ThumbnailPresentation>,
    pub(crate) thumbnail_requests: HashMap<u32, ThumbnailRequestState>,
    /// Bumped when a page's drawn content changes without a new resource
    /// (page-text edits), so its thumbnail is rendered again.
    pub(crate) thumbnail_content_revisions: HashMap<u32, u64>,
    /// The thumbnail rows the sidebar showed in its last frame.
    pub(crate) thumbnail_rows_shown: std::ops::Range<u32>,
    /// Images no longer shown, whose GPU textures are released once
    /// nothing else holds them.
    pub(crate) retired_images: Vec<Arc<RenderImage>>,
    pub(crate) page_content_geometry_requests: HashMap<u32, PageContentGeometryRequestState>,
    pub(crate) page_content_geometry_lru: VecDeque<u32>,
    pub(crate) image_assets: HashMap<String, Arc<RenderImage>>,
    pub(crate) resource: Option<Arc<dyn NativeDocumentResource>>,
    pub(crate) annotations: AnnotationAdapter,
    pub(crate) viewer: DocumentViewerState,
    pub(crate) adaptive_performance: AdaptiveViewerPerformance,
    pub(crate) adaptive_last_evaluated_at: Option<Instant>,
    pub(crate) view_state: NativeDocumentViewState,
    pub(crate) highlight_composite: HighlightCompositeEvidence,
    /// The highlights the page images and viewer tiles were composed with,
    /// where highlights are precomposed (Windows and Linux).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    composed_highlights: Option<Vec<crate::annotation_model::PenAnnotation>>,
    pub(crate) source_sha256: Option<[u8; 32]>,
    pub(crate) viewport_size: Option<(f32, f32)>,
    pub(crate) temporary_source: Option<(GeneratedDocumentStore, OwnedGeneratedDocument)>,
    pub(crate) save_as_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PublicationDurabilityWarning {
    pub(crate) save_generation: u64,
    pub(crate) message: String,
}

impl NativeDocumentSession {
    pub(crate) fn opening(id: DocumentId, path: PathBuf, generation: u64) -> Self {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled PDF")
            .to_owned();
        Self {
            id,
            path,
            title,
            status: NativeDocumentStatus::Opening,
            page_sizes: Vec::new(),
            source_page_sizes: Vec::new(),
            source_page_rotations: Vec::new(),
            source_page_coordinate_spaces: Vec::new(),
            file_page_sizes: Vec::new(),
            file_page_rotations: Vec::new(),
            file_page_coordinate_spaces: Vec::new(),
            arranged_slots: Vec::new(),
            presentation_error: None,
            recovery_generation: None,
            recovery_preparation_generation: 0,
            recovery_preparation: DocumentRecoveryPreparation::Unbound,
            resource_epoch: 0,
            pending_rotation_generation: None,
            current_page: 0,
            requested_page: 0,
            pending_navigation_scroll: None,
            navigation_from_below: None,
            pending_zoom_anchor: None,
            pending_page_frame: None,
            generation,
            save_generation: 0,
            image_prepare_generation: 0,
            save_status: NativeDocumentSaveStatus::Idle,
            publication_durability_warning: None,
            current_base_raster: None,
            current_image: None,
            current_image_page: None,
            placeholder_page: None,
            thumbnails: Vec::new(),
            thumbnail_requests: HashMap::new(),
            thumbnail_content_revisions: HashMap::new(),
            thumbnail_rows_shown: 0..0,
            retired_images: Vec::new(),
            page_content_geometry_requests: HashMap::new(),
            page_content_geometry_lru: VecDeque::new(),
            image_assets: HashMap::new(),
            resource: None,
            annotations: AnnotationAdapter::default(),
            viewer: DocumentViewerState::default(),
            adaptive_performance: AdaptiveViewerPerformance::default(),
            adaptive_last_evaluated_at: None,
            view_state: NativeDocumentViewState::default(),
            highlight_composite: HighlightCompositeEvidence::default(),
            composed_highlights: None,
            source_sha256: None,
            viewport_size: None,
            temporary_source: None,
            save_as_required: false,
        }
    }

    pub(crate) fn opening_generated(
        id: DocumentId,
        source: OwnedGeneratedDocument,
        store: GeneratedDocumentStore,
        generation: u64,
    ) -> Self {
        let path = source.path().to_owned();
        let mut session = Self::opening(id, path, generation);
        session.temporary_source = Some((store, source));
        session.save_as_required = true;
        session
    }

    pub const fn id(&self) -> DocumentId {
        self.id
    }

    pub const fn current_page(&self) -> u32 {
        self.current_page
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub const fn requested_page(&self) -> u32 {
        self.requested_page
    }

    pub fn status(&self) -> &NativeDocumentStatus {
        &self.status
    }

    pub fn save_status(&self) -> &NativeDocumentSaveStatus {
        &self.save_status
    }

    pub fn publication_durability_warning(&self) -> Option<&str> {
        self.publication_durability_warning
            .as_ref()
            .map(|warning| warning.message.as_str())
    }

    pub(crate) fn publication_durability_warning_generation(&self) -> Option<u64> {
        self.publication_durability_warning
            .as_ref()
            .map(|warning| warning.save_generation)
    }

    pub fn recovery_preparation(&self) -> &DocumentRecoveryPreparation {
        &self.recovery_preparation
    }

    pub fn worker_pid(&self) -> Option<u32> {
        self.resource
            .as_ref()
            .and_then(|resource| resource.worker_pid())
    }

    /// The page point at the viewport centre, to hold there across a zoom or
    /// mode change.
    pub(crate) fn viewport_centre_anchor(&self) -> Option<crate::zoom_anchor::ZoomAnchor> {
        let plan = self.viewer.plan_snapshot()?;
        let (width, height) = self.viewport_size?;
        crate::zoom_anchor::zoom_anchor_at(
            &plan.page_layouts,
            self.viewer.logical_scroll(),
            (width / 2., height / 2.),
        )
    }

    /// Before a page-mode change: keep the view centre if it is on the
    /// current page, otherwise show the current page from its top. Before
    /// the first layout there is no view to keep.
    pub(crate) fn hold_current_page_across_mode_change(&mut self) {
        if self.viewer.plan_snapshot().is_none() {
            return;
        }
        match self
            .viewport_centre_anchor()
            .filter(|anchor| anchor.page == self.current_page as usize)
        {
            Some(anchor) => self.pending_zoom_anchor = Some(anchor),
            None => self.pending_navigation_scroll = Some(self.current_page),
        }
    }

    pub fn page_count(&self) -> usize {
        self.page_sizes.len()
    }

    pub fn page_rotation(&self, page_index: u32) -> Option<PageRotation> {
        self.annotations
            .document_page_rotation(self.id.value(), page_index)
    }

    pub fn page_size(&self, page_index: u32) -> Option<(f32, f32)> {
        self.page_sizes.get(page_index as usize).copied()
    }

    pub(crate) fn source_page_coordinate_space(
        &self,
        page_index: u32,
    ) -> Option<PageCoordinateSpace> {
        let page_index_usize = page_index as usize;
        self.source_page_coordinate_spaces
            .get(page_index_usize)
            .copied()
            .or_else(|| {
                // Legacy/mock openers only supplied display sizes. Preserve a
                // deterministic zero-origin/unit-one fallback until they are
                // migrated to the canonical metadata seam.
                let source_size = *self.source_page_sizes.get(page_index_usize)?;
                let source_rotation = *self.source_page_rotations.get(page_index_usize)?;
                let (raw_width, raw_height) = if source_rotation.swaps_axes() {
                    (source_size.1, source_size.0)
                } else {
                    source_size
                };
                PageCoordinateSpace::new(
                    CoordinateRect::new(0., 0., f64::from(raw_width), f64::from(raw_height))
                        .ok()?,
                    CoordinateRect::new(0., 0., f64::from(raw_width), f64::from(raw_height))
                        .ok()?,
                    coordinate_rotation(source_rotation),
                    1.,
                )
                .ok()
            })
    }

    pub(crate) fn annotation_page_coordinate_space(
        &self,
        page_index: u32,
    ) -> Option<PageCoordinateSpace> {
        let source = self.source_page_coordinate_space(page_index)?;
        Some(source.with_rotation(coordinate_rotation(self.page_rotation(page_index)?)))
    }

    pub(crate) fn annotation_page_geometry(
        &self,
        page_index: u32,
    ) -> Option<((f32, f32), PageRotation)> {
        let source = self.source_page_coordinate_space(page_index)?;
        let view_box = source.view_box();
        let pdf_size = if source.rotation().swaps_axes() {
            (view_box.height, view_box.width)
        } else {
            (view_box.width, view_box.height)
        };
        Some((
            (pdf_size.0 as f32, pdf_size.1 as f32),
            self.page_rotation(page_index)?,
        ))
    }

    pub fn current_base_raster(&self) -> Option<&RasterSurface> {
        self.current_base_raster.as_ref()
    }

    /// The page raster of `page_index`, if it is the one held.
    pub fn base_raster_of(&self, page_index: u32) -> Option<&RasterSurface> {
        self.current_base_raster
            .as_ref()
            .filter(|_| self.current_image_page == Some(page_index))
    }

    pub fn thumbnail_base_raster(&self, page_index: u32) -> Option<&RasterSurface> {
        self.thumbnails
            .iter()
            .find(|thumbnail| thumbnail.page_index == page_index)
            .map(|thumbnail| &thumbnail.base_raster)
    }

    pub fn presentation_error(&self) -> Option<&str> {
        self.presentation_error.as_deref()
    }

    pub(crate) fn is_dirty(&self) -> bool {
        self.save_as_required || self.annotations.is_dirty(self.id.value())
    }

    pub(crate) fn dirty_revision(&self) -> Option<u64> {
        self.is_dirty().then(|| {
            self.annotations
                .snapshot(self.id.value())
                .map_or(0, |snapshot| snapshot.revision)
        })
    }

    pub(crate) fn release_temporary_source(&mut self) -> Result<(), String> {
        let Some((store, source)) = self.temporary_source.as_ref() else {
            return Ok(());
        };
        store.release(source).map_err(|error| error.to_string())?;
        self.temporary_source.take();
        Ok(())
    }

    /// The file's pages, as a file just opened or saved has them: shown as
    /// the file has them until [`Self::sync_page_arrangement`].
    pub(crate) fn set_file_pages(
        &mut self,
        sizes: Vec<(f32, f32)>,
        rotations: Vec<PageRotation>,
        coordinate_spaces: Vec<PageCoordinateSpace>,
    ) {
        self.arranged_slots = (0..sizes.len() as u32)
            .map(|index| crate::pdf_page_structure::PageSlot::Source { index })
            .collect();
        self.source_page_sizes = sizes.clone();
        self.source_page_rotations = rotations.clone();
        self.source_page_coordinate_spaces = coordinate_spaces.clone();
        self.file_page_sizes = sizes;
        self.file_page_rotations = rotations;
        self.file_page_coordinate_spaces = coordinate_spaces;
    }

    /// Shows the pages as the document arranges them, when the page images
    /// and thumbnails shown were already drawn so (a recovered document
    /// opens with its arrangement).
    pub(crate) fn adopt_page_arrangement(&mut self) {
        if self
            .annotations
            .document_page_slots(self.id.value())
            .is_empty()
        {
            return;
        }
        let thumbnails = std::mem::take(&mut self.thumbnails);
        let current = (
            self.current_image.take(),
            self.current_base_raster.take(),
            self.current_image_page,
            self.current_page,
        );
        self.sync_page_arrangement();
        let count = self.source_page_sizes.len() as u32;
        self.thumbnails = thumbnails
            .into_iter()
            .filter(|thumbnail| thumbnail.page_index < count)
            .collect();
        let (image, raster, image_page, page) = current;
        self.current_image = image;
        self.current_base_raster = raster;
        self.current_image_page = image_page;
        self.current_page = page.min(count.saturating_sub(1));
        self.requested_page = self.current_page;
    }

    /// Shows the pages as the document arranges them now: each page's size,
    /// rotation and thumbnail follow it, a new blank page shows white, and
    /// the current page stays the page it was. Returns whether the
    /// arrangement changed.
    pub(crate) fn sync_page_arrangement(&mut self) -> bool {
        use crate::pdf_page_structure::PageSlot;
        let file_count = self.file_page_sizes.len();
        if file_count == 0 {
            return false;
        }
        let mut slots = self
            .annotations
            .document_page_slots(self.id.value())
            .to_vec();
        if slots.is_empty() {
            slots = (0..file_count as u32)
                .map(|index| PageSlot::Source { index })
                .collect();
        }
        if slots == self.arranged_slots
            || crate::pdf_page_structure::validate(&slots, file_count).is_err()
        {
            return false;
        }
        let previous = std::mem::replace(&mut self.arranged_slots, slots.clone());
        // Where each page was shown before: the same slot, else another
        // copy of the same file page, whose thumbnail stands in until the
        // page is drawn again.
        let was_at = |slot: PageSlot| {
            previous
                .iter()
                .position(|old| *old == slot)
                .or_else(|| {
                    slot.file_page().and_then(|page| {
                        previous
                            .iter()
                            .position(|old| old.file_page() == Some(page))
                    })
                })
                .map(|index| index as u32)
        };
        let blank_size = |slot: PageSlot| match slot {
            PageSlot::Blank { width, height } => (width as f32, height as f32),
            _ => (612., 792.),
        };
        self.source_page_sizes = slots
            .iter()
            .map(|slot| {
                slot.file_page()
                    .and_then(|page| self.file_page_sizes.get(page as usize).copied())
                    .unwrap_or_else(|| blank_size(*slot))
            })
            .collect();
        self.source_page_rotations = slots
            .iter()
            .map(|slot| {
                slot.file_page()
                    .and_then(|page| self.file_page_rotations.get(page as usize).copied())
                    .unwrap_or(PageRotation::Degrees0)
            })
            .collect();
        self.source_page_coordinate_spaces = if self.file_page_coordinate_spaces.len() == file_count
        {
            slots
                .iter()
                .filter_map(|slot| match slot.file_page() {
                    Some(page) => self.file_page_coordinate_spaces.get(page as usize).copied(),
                    None => {
                        let (width, height) = blank_size(*slot);
                        let rect = CoordinateRect::new(0., 0., f64::from(width), f64::from(height))
                            .ok()?;
                        PageCoordinateSpace::new(
                            rect,
                            rect,
                            crate::page_geometry::Rotation::Degrees0,
                            1.,
                        )
                        .ok()
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        let mut thumbnails = Vec::new();
        for (index, slot) in slots.iter().enumerate() {
            let index = index as u32;
            let shown = was_at(*slot).and_then(|old| self.thumbnail(old));
            if let Some(old) = shown {
                thumbnails.push(ThumbnailPresentation {
                    page_index: index,
                    base_raster: old.base_raster.clone(),
                    image: old.image.clone(),
                    highlight_pixels: old.highlight_pixels,
                    rotation: old.rotation,
                });
            } else if let PageSlot::Blank { .. } = slot {
                let size = blank_size(*slot);
                let width = crate::document_resource::thumbnail_width(size);
                let height = ((width as f32 * size.1 / size.0).round() as u32).max(1);
                let raster = RasterSurface::new(
                    width,
                    height,
                    vec![0xff; width as usize * height as usize * 4],
                );
                if let Ok(raster) = raster
                    && let Ok(image) = raster.clone().into_render_image()
                {
                    thumbnails.push(ThumbnailPresentation {
                        page_index: index,
                        base_raster: raster,
                        image,
                        highlight_pixels: 0,
                        rotation: PageRotation::Degrees0,
                    });
                }
            }
        }
        let retired = std::mem::replace(&mut self.thumbnails, thumbnails);
        self.retired_images
            .extend(retired.into_iter().map(|thumbnail| thumbnail.image));
        self.thumbnail_requests.clear();
        self.thumbnail_content_revisions.clear();
        self.page_content_geometry_requests.clear();
        self.page_content_geometry_lru.clear();
        // The current page stays the page it was, or the nearest left.
        let follow = |page: u32| {
            previous.get(page as usize).and_then(|slot| {
                slots
                    .iter()
                    .position(|candidate| candidate == slot)
                    .map(|index| index as u32)
            })
        };
        let last = slots.len().saturating_sub(1) as u32;
        let current = follow(self.current_page).unwrap_or(self.current_page.min(last));
        self.current_image_page = self
            .current_image_page
            .and_then(follow)
            .filter(|page| *page == current);
        if self.current_image_page.is_none()
            && let Some(image) = self.current_image.take()
        {
            self.retired_images.push(image);
            self.current_base_raster = None;
        }
        self.current_page = current;
        self.requested_page = current;
        self.placeholder_page = None;
        self.pending_navigation_scroll = None;
        self.navigation_from_below = None;
        for (index, size) in self.source_page_sizes.iter().enumerate() {
            self.annotations.set_semantic_snap_page_size(
                self.id.value(),
                index as u32,
                f64::from(size.0),
                f64::from(size.1),
            );
        }
        self.sync_rotation_geometry();
        true
    }

    pub(crate) fn sync_rotation_geometry(&mut self) {
        self.page_sizes = self
            .source_page_sizes
            .iter()
            .enumerate()
            .map(|(page_index, _)| {
                let source_rotation = self.source_page_rotations[page_index];
                let effective = self
                    .annotations
                    .document_page_rotation(self.id.value(), page_index as u32)
                    .unwrap_or(source_rotation);
                self.source_page_coordinate_space(page_index as u32)
                    .map(|space| {
                        let display = space
                            .with_rotation(coordinate_rotation(effective))
                            .display_size_points();
                        (display.0 as f32, display.1 as f32)
                    })
                    .unwrap_or_else(|| {
                        if effective.delta_from(source_rotation).swaps_axes() {
                            let (width, height) = self.source_page_sizes[page_index];
                            (height, width)
                        } else {
                            self.source_page_sizes[page_index]
                        }
                    })
            })
            .collect();
        self.viewer.invalidate_raster();
    }

    /// Turns the pixels already shown for `page_index`, its page image and
    /// thumbnail, from `previous` to the page's rotation now, so a rotated
    /// page never shows its old image stretched to its new shape while the
    /// fresh render is on the way.
    pub(crate) fn turn_shown_page(
        &mut self,
        page_index: u32,
        previous: PageRotation,
    ) -> Result<(), String> {
        let target = self
            .page_rotation(page_index)
            .ok_or_else(|| "page rotation is unavailable".to_owned())?;
        if self.current_image_page == Some(page_index)
            && let Some(raster) = self.current_base_raster.as_ref()
        {
            let raster = raster.rotated(target.delta_from(previous))?;
            let image = raster.clone().into_render_image()?;
            self.current_base_raster = Some(raster);
            if let Some(previous) = self.current_image.replace(image) {
                self.retired_images.push(previous);
            }
        }
        if let Ok(index) = self
            .thumbnails
            .binary_search_by_key(&page_index, |thumbnail| thumbnail.page_index)
        {
            let thumbnail = &mut self.thumbnails[index];
            if thumbnail.rotation != target {
                let raster = thumbnail
                    .base_raster
                    .rotated(target.delta_from(thumbnail.rotation))?;
                let image = raster.clone().into_render_image()?;
                thumbnail.base_raster = raster;
                thumbnail.rotation = target;
                let previous = std::mem::replace(&mut thumbnail.image, image);
                self.retired_images.push(previous);
            }
        }
        self.rebuild_stable_highlight_presentations()
    }

    pub(crate) fn page_rotation_quarter_turns(&self) -> Vec<u8> {
        self.source_page_rotations
            .iter()
            .copied()
            .enumerate()
            .map(|(page_index, source)| {
                self.page_rotation(page_index as u32)
                    .unwrap_or(source)
                    .quarter_turns()
            })
            .collect()
    }

    pub(crate) fn refresh_rotation_presentations(&mut self) -> Result<(), String> {
        let resource = self
            .resource
            .as_ref()
            .ok_or_else(|| "document resource is unavailable".to_owned())?;
        let current_page = self.current_page;
        let current_delta = self
            .page_rotation(current_page)
            .ok_or_else(|| "current page rotation is unavailable".to_owned())?
            .delta_from(self.source_page_rotations[current_page as usize]);
        let current_raster = resource
            .render_page(current_page, DEFAULT_PAGE_RENDER_WIDTH)?
            .rotated(current_delta)?;
        let current_image = current_raster.clone().into_render_image()?;
        self.current_base_raster = Some(current_raster);
        self.current_image_page = Some(current_page);
        if let Some(previous) = self.current_image.replace(current_image) {
            self.retired_images.push(previous);
        }
        // A rotation turns a thumbnail's pixels; the worker need not draw it
        // again, which for every thumbnail would hold up the window.
        for index in 0..self.thumbnails.len() {
            let page_index = self.thumbnails[index].page_index;
            let target = self
                .page_rotation(page_index)
                .ok_or_else(|| "thumbnail page rotation is unavailable".to_owned())?;
            let thumbnail = &mut self.thumbnails[index];
            if thumbnail.rotation == target {
                continue;
            }
            let raster = thumbnail
                .base_raster
                .rotated(target.delta_from(thumbnail.rotation))?;
            let image = raster.clone().into_render_image()?;
            thumbnail.base_raster = raster;
            thumbnail.rotation = target;
            let previous = std::mem::replace(&mut thumbnail.image, image);
            self.retired_images.push(previous);
        }
        self.rebuild_stable_highlight_presentations()
    }

    /// The loaded thumbnail of `page_index`.
    pub(crate) fn thumbnail(&self, page_index: u32) -> Option<&ThumbnailPresentation> {
        self.thumbnails
            .binary_search_by_key(&page_index, |thumbnail| thumbnail.page_index)
            .ok()
            .map(|index| &self.thumbnails[index])
    }

    /// Shows a newly drawn thumbnail, replacing any older one, with the
    /// document's highlights where they are precomposed, without redrawing
    /// other thumbnails or the canvas.
    pub(crate) fn present_thumbnail(
        &mut self,
        page_index: u32,
        rotation: PageRotation,
        base_raster: RasterSurface,
        image: Arc<RenderImage>,
    ) -> Result<(), String> {
        let presentation = ThumbnailPresentation {
            page_index,
            base_raster,
            image,
            highlight_pixels: 0,
            rotation,
        };
        let index = match self
            .thumbnails
            .binary_search_by_key(&page_index, |thumbnail| thumbnail.page_index)
        {
            Ok(index) => {
                let previous = std::mem::replace(&mut self.thumbnails[index], presentation);
                self.retired_images.push(previous.image);
                index
            }
            Err(index) => {
                self.thumbnails.insert(index, presentation);
                index
            }
        };
        #[cfg(target_os = "macos")]
        let _ = index;
        #[cfg(not(target_os = "macos"))]
        {
            let Some(snapshot) = self.annotations.snapshot(self.id.value()) else {
                return Ok(());
            };
            if snapshot.revision != self.highlight_composite.annotation_revision {
                self.sync_stable_highlight_presentations()?;
            }
            let coordinate_space = self
                .annotation_page_coordinate_space(page_index)
                .ok_or_else(|| "thumbnail coordinate space is unavailable".to_owned())?;
            let thumbnail = &mut self.thumbnails[index];
            let mut composited = thumbnail.base_raster.clone();
            let pixels = composited.precompose_highlights(
                page_index,
                coordinate_space,
                0.,
                0.,
                (
                    f64::from(thumbnail.base_raster.width()),
                    f64::from(thumbnail.base_raster.height()),
                ),
                &snapshot.pens,
            )?;
            thumbnail.highlight_pixels = pixels;
            let plain = std::mem::replace(&mut thumbnail.image, composited.into_render_image()?);
            self.retired_images.push(plain);
            self.highlight_composite.thumbnail_pixels = self
                .thumbnails
                .iter()
                .map(|thumbnail| thumbnail.highlight_pixels)
                .sum();
        }
        Ok(())
    }

    /// Unloads the thumbnails furthest from the rows shown until those
    /// left take at most `budget` bytes.
    pub(crate) fn evict_thumbnails(&mut self, budget: usize) {
        let mut bytes = self
            .thumbnails
            .iter()
            .map(|thumbnail| thumbnail.base_raster.pixels_bgra().len())
            .sum::<usize>();
        if bytes <= budget {
            return;
        }
        let shown = self.thumbnail_rows_shown.clone();
        let distance = |page: u32| {
            if page < shown.start {
                shown.start - page
            } else {
                page.saturating_sub(shown.end.saturating_sub(1))
            }
        };
        let mut furthest = self
            .thumbnails
            .iter()
            .map(|thumbnail| thumbnail.page_index)
            .filter(|page| !shown.contains(page) && *page != self.current_page)
            .collect::<Vec<_>>();
        furthest.sort_by_key(|page| std::cmp::Reverse(distance(*page)));
        let mut evicted = std::collections::HashSet::new();
        for page in furthest {
            if bytes <= budget {
                break;
            }
            if let Some(thumbnail) = self.thumbnail(page) {
                bytes -= thumbnail.base_raster.pixels_bgra().len();
                evicted.insert(page);
            }
        }
        let (kept, removed) = std::mem::take(&mut self.thumbnails)
            .into_iter()
            .partition::<Vec<_>, _>(|thumbnail| !evicted.contains(&thumbnail.page_index));
        self.thumbnails = kept;
        self.retired_images
            .extend(removed.into_iter().map(|thumbnail| thumbnail.image));
    }

    /// Retired images (the viewer's included) that nothing else holds now,
    /// so their GPU textures can be released.
    pub(crate) fn take_releasable_images(&mut self) -> Vec<Arc<RenderImage>> {
        self.retired_images
            .extend(self.viewer.take_retired_images());
        if self.retired_images.is_empty() {
            return Vec::new();
        }
        let mut retired = std::mem::take(&mut self.retired_images);
        retired.sort_by_key(|image| Arc::as_ptr(image) as usize);
        retired.dedup_by(|a, b| Arc::ptr_eq(a, b));
        let (releasable, held) = retired
            .into_iter()
            .partition(|image| Arc::strong_count(image) == 1);
        self.retired_images = held;
        releasable
    }

    /// Brings precomposed highlights up to date after an annotation edit.
    /// Edits that leave the highlights as they were (most edits) keep the
    /// page images and tiles on screen untouched.
    pub(crate) fn sync_stable_highlight_presentations(&mut self) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        if let Some(snapshot) = self.annotations.snapshot(self.id.value())
            && self.composed_highlights.as_ref() == Some(&composed_highlights(&snapshot.pens))
        {
            self.highlight_composite.annotation_revision = snapshot.revision;
            return Ok(());
        }
        self.rebuild_stable_highlight_presentations()
    }

    /// Composes the highlights into a newly shown current page. Thumbnails
    /// and tiles are recomposed only when the highlights changed too.
    pub(crate) fn present_current_page_highlights(&mut self) -> Result<(), String> {
        #[cfg(not(target_os = "macos"))]
        if let Some(snapshot) = self.annotations.snapshot(self.id.value())
            && self.composed_highlights.as_ref() == Some(&composed_highlights(&snapshot.pens))
        {
            self.compose_current_page_highlights(&snapshot.pens)?;
            self.highlight_composite.annotation_revision = snapshot.revision;
            return Ok(());
        }
        self.rebuild_stable_highlight_presentations()
    }

    pub(crate) fn rebuild_stable_highlight_presentations(&mut self) -> Result<(), String> {
        let Some(snapshot) = self.annotations.snapshot(self.id.value()) else {
            return Ok(());
        };
        #[cfg(target_os = "macos")]
        {
            // Ordered native paths blend against the live opaque page scene.
            // Annotation edits therefore leave PDF raster/cache identities intact.
            self.highlight_composite = HighlightCompositeEvidence {
                annotation_revision: snapshot.revision,
                ..HighlightCompositeEvidence::default()
            };
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let coordinate_spaces = (0..self.page_sizes.len())
                .map(|page_index| {
                    self.annotation_page_coordinate_space(page_index as u32)
                        .ok_or_else(|| "thumbnail coordinate space is unavailable".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            self.compose_current_page_highlights(&snapshot.pens)?;
            let mut thumbnail_pixels = 0usize;
            for thumbnail in &mut self.thumbnails {
                let mut composited = thumbnail.base_raster.clone();
                thumbnail.highlight_pixels = composited.precompose_highlights(
                    thumbnail.page_index,
                    coordinate_spaces[thumbnail.page_index as usize],
                    0.,
                    0.,
                    (
                        f64::from(thumbnail.base_raster.width()),
                        f64::from(thumbnail.base_raster.height()),
                    ),
                    &snapshot.pens,
                )?;
                thumbnail_pixels = thumbnail_pixels.saturating_add(thumbnail.highlight_pixels);
                let previous =
                    std::mem::replace(&mut thumbnail.image, composited.into_render_image()?);
                self.retired_images.push(previous);
            }
            self.highlight_composite.annotation_revision = snapshot.revision;
            self.highlight_composite.thumbnail_pixels = thumbnail_pixels;
            let highlights = composed_highlights(&snapshot.pens);
            if self.composed_highlights.as_ref() != Some(&highlights) {
                // Tiles carry the highlights too. Their identities include the
                // raster revision, so in-flight results from the prior
                // highlights are rejected; the tiles on screen stay until
                // their replacements arrive rather than blanking the page.
                self.highlight_composite.viewer_tile_pixels = 0;
                self.viewer.refresh_highlights();
                self.composed_highlights = Some(highlights);
            }
            Ok(())
        }
    }

    #[cfg(not(target_os = "macos"))]
    fn compose_current_page_highlights(
        &mut self,
        pens: &[crate::annotation_model::PenAnnotation],
    ) -> Result<(), String> {
        let current_page = self.current_image_page.unwrap_or(self.current_page);
        let current_coordinate_space = self
            .annotation_page_coordinate_space(current_page)
            .ok_or_else(|| "current page coordinate space is unavailable".to_owned())?;
        let mut current_pixels = 0;
        if let Some(base) = self.current_base_raster.clone() {
            let mut composited = base.clone();
            current_pixels = composited.precompose_highlights(
                current_page,
                current_coordinate_space,
                0.,
                0.,
                (f64::from(base.width()), f64::from(base.height())),
                pens,
            )?;
            if let Some(previous) = self.current_image.replace(composited.into_render_image()?) {
                self.retired_images.push(previous);
            }
        }
        self.highlight_composite.current_page_pixels = current_pixels;
        Ok(())
    }

    pub(crate) fn sync_image_assets(&mut self) -> Result<Vec<Arc<RenderImage>>, String> {
        let mut assets =
            self.annotations
                .snapshot(self.id.value())
                .map(|snapshot| {
                    snapshot
                        .images
                        .iter()
                        .map(|image| (image.asset().clone(), image.opacity()))
                        .chain(
                            snapshot.snapshots.iter().map(|annotation| {
                                (annotation.asset().clone(), annotation.opacity())
                            }),
                        )
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
        if let Some(asset) = self.annotations.image_asset() {
            assets.push((asset.clone(), IMAGE_PLACEMENT_PREVIEW_OPACITY));
        }
        let retained_ids = assets
            .iter()
            .map(|(asset, opacity)| annotation_image_cache_key(asset.id().as_str(), *opacity))
            .collect::<std::collections::HashSet<_>>();
        let removed_ids = self
            .image_assets
            .keys()
            .filter(|asset_id| !retained_ids.contains(*asset_id))
            .cloned()
            .collect::<Vec<_>>();
        let removed = removed_ids
            .into_iter()
            .filter_map(|asset_id| self.image_assets.remove(&asset_id))
            .collect::<Vec<_>>();
        for (asset, opacity) in assets {
            let cache_key = annotation_image_cache_key(asset.id().as_str(), opacity);
            if self.image_assets.contains_key(&cache_key) {
                continue;
            }
            let image = decoded_asset_render_image(&asset, opacity)
                .ok_or_else(|| "GPUI rejected the decoded annotation image".to_owned())?;
            self.image_assets.insert(cache_key, image);
        }
        Ok(removed)
    }

    pub(crate) fn release(&mut self) -> Result<Vec<Arc<RenderImage>>, String> {
        if let Some(resource) = self.resource.as_ref() {
            resource.close()?;
        }
        self.resource.take();
        self.release_temporary_source()?;
        self.generation = self.generation.saturating_add(1);
        self.viewer.close();
        Ok(self.image_assets.drain().map(|(_, image)| image).collect())
    }
}

impl Drop for NativeDocumentSession {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

/// The pens composed into page images: highlights.
#[cfg(not(target_os = "macos"))]
fn composed_highlights(
    pens: &[crate::annotation_model::PenAnnotation],
) -> Vec<crate::annotation_model::PenAnnotation> {
    pens.iter()
        .filter(|pen| pen.tool() == crate::annotation_model::InkTool::Highlight)
        .cloned()
        .collect()
}

#[cfg(test)]
mod decoded_asset_upload_tests {
    use super::*;

    #[test]
    fn decoded_asset_upload_uses_bgra_without_changing_model_rgba_or_alpha_semantics() {
        let rgba = vec![240, 31, 9, 255, 7, 43, 211, 128, 83, 17, 149, 0];
        let asset = crate::annotation_model::DecodedRgbaAsset::new(3, 1, rgba.clone()).unwrap();
        let image = decoded_asset_render_image(&asset, 1.).unwrap();
        assert_eq!(
            image.as_bytes(0).unwrap(),
            &[9, 31, 240, 255, 211, 43, 7, 128, 149, 17, 83, 0]
        );
        let translucent = decoded_asset_render_image(&asset, 0.5).unwrap();
        assert_eq!(
            translucent.as_bytes(0).unwrap(),
            &[9, 31, 240, 128, 211, 43, 7, 64, 149, 17, 83, 0]
        );
        assert_eq!(
            asset.rgba(),
            rgba.as_slice(),
            "upload must not mutate saved/exported RGBA"
        );
    }

    #[test]
    fn recovery_preparation_allows_unbound_but_blocks_pending_and_failed() {
        assert_eq!(DocumentRecoveryPreparation::Unbound.edit_guard(), Ok(()));

        let state = DocumentRecoveryPreparation::Pending {
            generation: 4,
            source_kind: RecoverySourceKind::Opened,
        };
        assert!(state.edit_guard().unwrap_err().contains("still preparing"));

        let state = DocumentRecoveryPreparation::Failed {
            generation: 4,
            source_kind: RecoverySourceKind::Opened,
            message: "disk unavailable".into(),
        };
        let error = state.edit_guard().unwrap_err();
        assert!(error.contains("unavailable"));
        assert!(error.contains("disk unavailable"));
    }
}
