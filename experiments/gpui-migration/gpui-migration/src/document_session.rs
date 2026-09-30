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
        DEFAULT_PAGE_RENDER_WIDTH, DEFAULT_THUMBNAIL_WIDTH, DocumentId, NativeDocumentResource,
        OpenedNativeDocument, RasterSurface, coordinate_rotation,
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
}

impl SavedNativeDocument {
    pub fn new(opened: OpenedNativeDocument, validated_revision: u64) -> Self {
        Self {
            opened,
            validated_revision,
            publication_warning: None,
        }
    }

    pub fn with_publication_warning(mut self, warning: impl Into<String>) -> Self {
        self.publication_warning = Some(warning.into());
        self
    }

    pub fn publication_warning(&self) -> Option<&str> {
        self.publication_warning.as_deref()
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
    pub(crate) highlight_pixels: usize,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DocumentRecoveryPreparation {
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

impl Default for DocumentRecoveryPreparation {
    fn default() -> Self {
        Self::Unbound
    }
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
    pub(crate) presentation_error: Option<String>,
    pub(crate) recovery_generation: Option<u64>,
    pub(crate) recovery_preparation_generation: u64,
    pub(crate) recovery_preparation: DocumentRecoveryPreparation,
    pub(crate) resource_epoch: u64,
    pub(crate) pending_rotation_generation: Option<u64>,
    pub(crate) current_page: u32,
    pub(crate) requested_page: u32,
    pub(crate) pending_navigation_scroll: Option<u32>,
    pub(crate) pending_zoom_anchor: Option<crate::zoom_anchor::ZoomAnchor>,
    pub(crate) generation: u64,
    pub(crate) save_generation: u64,
    pub(crate) image_prepare_generation: u64,
    pub(crate) save_status: NativeDocumentSaveStatus,
    pub(crate) publication_durability_warning: Option<PublicationDurabilityWarning>,
    pub(crate) current_base_raster: Option<RasterSurface>,
    pub(crate) current_image: Option<Arc<RenderImage>>,
    pub(crate) thumbnails: Vec<ThumbnailPresentation>,
    pub(crate) thumbnail_requests: HashMap<u32, ThumbnailRequestState>,
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
            presentation_error: None,
            recovery_generation: None,
            recovery_preparation_generation: 0,
            recovery_preparation: DocumentRecoveryPreparation::Unbound,
            resource_epoch: 0,
            pending_rotation_generation: None,
            current_page: 0,
            requested_page: 0,
            pending_navigation_scroll: None,
            pending_zoom_anchor: None,
            generation,
            save_generation: 0,
            image_prepare_generation: 0,
            save_status: NativeDocumentSaveStatus::Idle,
            publication_durability_warning: None,
            current_base_raster: None,
            current_image: None,
            thumbnails: Vec::new(),
            thumbnail_requests: HashMap::new(),
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
        let mut thumbnails = Vec::with_capacity(self.thumbnails.len());
        for thumbnail in &self.thumbnails {
            let page_index = thumbnail.page_index;
            let delta = self
                .page_rotation(page_index)
                .ok_or_else(|| "thumbnail page rotation is unavailable".to_owned())?
                .delta_from(self.source_page_rotations[page_index as usize]);
            let raster = resource
                .render_page(page_index, DEFAULT_THUMBNAIL_WIDTH)?
                .rotated(delta)?;
            thumbnails.push((page_index, raster.clone(), raster.into_render_image()?));
        }
        self.current_base_raster = Some(current_raster);
        self.current_image = Some(current_image);
        for (page_index, raster, image) in thumbnails {
            if let Some(thumbnail) = self
                .thumbnails
                .iter_mut()
                .find(|thumbnail| thumbnail.page_index == page_index)
            {
                thumbnail.base_raster = raster;
                thumbnail.image = image;
            }
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
            return Ok(());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let current_page = self.current_page;
            let current_coordinate_space = self
                .annotation_page_coordinate_space(current_page)
                .ok_or_else(|| "current page coordinate space is unavailable".to_owned())?;
            let coordinate_spaces = (0..self.page_sizes.len())
                .map(|page_index| {
                    self.annotation_page_coordinate_space(page_index as u32)
                        .ok_or_else(|| "thumbnail coordinate space is unavailable".to_owned())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let mut current_pixels = 0;
            if let Some(base) = self.current_base_raster.clone() {
                let mut composited = base.clone();
                current_pixels = composited.precompose_highlights(
                    current_page,
                    current_coordinate_space,
                    0.,
                    0.,
                    (f64::from(base.width()), f64::from(base.height())),
                    &snapshot.pens,
                )?;
                self.current_image = Some(composited.into_render_image()?);
            }
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
                thumbnail.image = composited.into_render_image()?;
            }
            self.highlight_composite.annotation_revision = snapshot.revision;
            self.highlight_composite.current_page_pixels = current_pixels;
            self.highlight_composite.thumbnail_pixels = thumbnail_pixels;
            self.highlight_composite.viewer_tile_pixels = 0;
            // Tile identities include this raster revision. In-flight results from
            // the prior annotation revision must be rejected, not painted stale.
            self.viewer.invalidate_raster();
            Ok(())
        }
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
