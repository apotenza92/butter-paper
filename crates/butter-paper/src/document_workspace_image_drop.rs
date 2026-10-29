//! Images dropped on a document: each PNG or JPEG is placed at once where
//! it is dropped, as an Image markup, selected, as Revu places a dropped
//! image. PDFs dropped with them open as documents.

use super::*;

/// The pointer the placement presses with: not a real pointer's.
/// How far each further image dropped at once is placed from the last, in
/// points, so none hides another.
const DROP_CASCADE_POINTS: f64 = 24.;

/// Whether `path` names an image the Image tool takes.
pub(super) fn is_image_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        ["png", "jpg", "jpeg"].iter().any(|image| {
            extension
                .as_encoded_bytes()
                .eq_ignore_ascii_case(image.as_bytes())
        })
    })
}

impl DocumentWorkspace {
    /// Files dropped on the window at `position`: images are placed on the
    /// page under it in the active document, and everything else opens as
    /// documents. Returns whether any image was taken.
    pub fn drop_paths(
        &mut self,
        paths: Vec<PathBuf>,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        // While Merge Documents is open, dropped PDFs join its list.
        if let Some(dialog) = self.merge_dialog() {
            dialog.update(cx, |dialog, cx| dialog.add_paths(paths, cx));
            return false;
        }
        let (images, documents): (Vec<_>, Vec<_>) =
            paths.into_iter().partition(|path| is_image_path(path));
        if !documents.is_empty() {
            self.open_documents(
                DocumentOpenBatchRequest::new(DocumentOpenOrigin::Drop, documents),
                cx,
            );
        }
        if images.is_empty() {
            return false;
        }
        let Some(document_id) = self.active_document_id else {
            return false;
        };
        let Some((page_index, point)) = self.drop_target(document_id, position) else {
            return false;
        };
        let background = cx.background_executor().clone();
        cx.spawn(async move |entity, cx| {
            for (index, path) in images.into_iter().enumerate() {
                let decoded = background
                    .spawn(
                        async move { decode_image_path(path).map_err(|error| error.to_string()) },
                    )
                    .await;
                let shift = DROP_CASCADE_POINTS * index as f64;
                let point = PdfPoint {
                    x: point.x + shift,
                    y: point.y - shift,
                };
                let placed = entity.update(cx, |workspace, cx| {
                    let result = decoded.and_then(|decoded| {
                        workspace.place_dropped_image(document_id, page_index, decoded, point, cx)
                    });
                    if let Err(error) = result {
                        workspace
                            .annotation_statuses
                            .insert(document_id, format!("The image was not added: {error}"));
                        cx.notify();
                    }
                });
                if placed.is_err() {
                    return;
                }
            }
        })
        .detach();
        true
    }

    /// The page and point a drop at `position` places on: the page under
    /// it, or the shown page nearest to it, at the nearest point on it.
    fn drop_target(
        &self,
        document_id: DocumentId,
        position: gpui::Point<Pixels>,
    ) -> Option<(u32, PdfPoint)> {
        let distance = |bounds: &Bounds<Pixels>| {
            let dx = (bounds.left() - position.x)
                .max(position.x - bounds.right())
                .max(px(0.));
            let dy = (bounds.top() - position.y)
                .max(position.y - bounds.bottom())
                .max(px(0.));
            f32::from(dx).hypot(f32::from(dy))
        };
        let interaction = self
            .page_interactions
            .values()
            .filter(|interaction| interaction.document_id == document_id)
            .min_by(|a, b| distance(&a.bounds).total_cmp(&distance(&b.bounds)))
            .copied()?;
        let bounds = interaction.bounds;
        let inside = gpui::point(
            position.x.clamp(bounds.left(), bounds.right()),
            position.y.clamp(bounds.top(), bounds.bottom()),
        );
        let point = Self::interaction_point(interaction, inside, false)?;
        Some((interaction.page_index, point))
    }

    /// Places a decoded image on `page_index` at `point`, as a click with
    /// the Image tool does, as one undoable step, and selects it.
    fn place_dropped_image(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        decoded: DecodedImageFile,
        point: PdfPoint,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let ready = self.session(document_id, cx).is_some_and(|session| {
            let session = session.read(cx);
            matches!(session.status, NativeDocumentStatus::Ready)
                && session.save_status != NativeDocumentSaveStatus::Saving
        });
        if !ready {
            return Err("the document is not ready for changes".into());
        }
        self.commit_page_text_editing(cx);
        self.apply_decoded_image(document_id, decoded, cx)?;
        self.set_annotation_tool(document_id, AnnotationTool::Image, cx)?;
        self.update_native_session_history_with_result(document_id, cx, move |session| {
            let (page_size, _) = session
                .annotation_page_geometry(page_index)
                .ok_or_else(|| "image placement page geometry is unavailable".to_owned())?;
            session
                .annotations
                .set_image_placement_page(
                    f64::from(page_size.0),
                    f64::from(page_size.1),
                    NATURAL_IMAGE_MAX_PAGE_FRACTION,
                )
                .map_err(|error| error.to_string())?;
            let outcome = session
                .annotations
                .place_image_at(document_id.value(), page_index, point)
                .map_err(|error| error.to_string())?;
            if !matches!(outcome, PointerPhaseOutcome::AnnotationCreated(_)) {
                return Err(format!("the image could not be placed: {outcome:?}"));
            }
            Ok::<(), String>(())
        })?;
        // The tool returns to Select with the new image selected, as after
        // placing a picked image.
        self.set_annotation_tool(document_id, AnnotationTool::Select, cx)?;
        if let Some(session) = self.session(document_id, cx).cloned() {
            session.update(cx, |session, cx| {
                if let Ok(removed) = session.sync_image_assets() {
                    defer_drop_images(removed, cx);
                }
                cx.notify();
            });
        }
        self.annotation_statuses.remove(&document_id);
        cx.notify();
        Ok(())
    }
}
