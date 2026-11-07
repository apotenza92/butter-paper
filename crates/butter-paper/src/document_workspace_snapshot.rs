//! The Snapshot tool copies a box of a page, as Revu's does: drawing the box
//! places no markup. The copy pastes as a Snapshot markup into this or any
//! open PDF, keeping the box's content as vector where the page allows; other
//! apps get a picture of it on the system clipboard.

use super::*;
use crate::annotation_model::{DecodedRgbaAsset, SnapshotAnnotation, SnapshotVector};
use gpui::BorrowAppContext as _;
use gpui::ClipboardItem;

/// The canvas picture of a Snapshot: as Revu vector Snapshots are drawn,
/// up to four pixels to the point and 2048 pixels along its longer side.
const CANVAS_PIXELS_PER_POINT: f64 = 4.;
const CANVAS_MAX_EDGE_PX: f64 = 2_048.;
/// The picture other apps get: 300 dpi, within the PDF worker's surfaces.
const CLIPBOARD_PIXELS_PER_POINT: f64 = 300. / 72.;
const CLIPBOARD_MAX_EDGE_PX: f64 = 8_192.;
const CLIPBOARD_MAX_PIXELS: f64 = 24. * 1024. * 1024.;

/// A copied box: the markup that pastes, and the picture for other apps.
struct SnapshotCopy {
    snapshot: SnapshotAnnotation,
    png: Option<Vec<u8>>,
    /// Its vector content, the way up its page is shown.
    pdf: Option<Vec<u8>>,
}

/// What other apps get of a Snapshot: a picture, and its vector content as a
/// PDF, which apps that read PDF paste as vector.
fn other_apps_clipboard(png: Option<Vec<u8>>, pdf: Option<Vec<u8>>) -> Option<ClipboardItem> {
    let entries = png
        .map(|png| {
            gpui::ClipboardEntry::Image(gpui::Image::from_bytes(gpui::ImageFormat::Png, png))
        })
        .into_iter()
        .chain(pdf.map(gpui::ClipboardEntry::Pdf))
        .collect::<Vec<_>>();
    (!entries.is_empty()).then_some(ClipboardItem { entries })
}

fn snapshot_copied_status(vector: bool, other_apps: bool) -> String {
    let copied = if vector {
        "Snapshot copied."
    } else {
        "Snapshot copied as a picture."
    };
    let into = if other_apps {
        "Paste it into any PDF or app."
    } else {
        "Paste it into any PDF."
    };
    format!("{copied} {into}")
}

fn canvas_pixels_per_point(size: (f64, f64)) -> f64 {
    CANVAS_PIXELS_PER_POINT.min(CANVAS_MAX_EDGE_PX / size.0.max(size.1))
}

fn clipboard_pixels_per_point(size: (f64, f64)) -> f64 {
    CLIPBOARD_PIXELS_PER_POINT
        .min(CLIPBOARD_MAX_EDGE_PX / size.0.max(size.1))
        .min((CLIPBOARD_MAX_PIXELS / (size.0 * size.1)).sqrt())
}

impl DocumentWorkspace {
    /// A Snapshot box has been drawn on `page_index`: it is taken for a
    /// signature when one asked for it, and copied otherwise.
    pub(super) fn finish_snapshot_box(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        rect: PdfRect,
        cx: &mut Context<Self>,
    ) {
        if self.take_signature_box(document_id, page_index, rect, cx) {
            return;
        }
        self.copy_snapshot_box(document_id, page_index, rect, cx);
    }

    /// Copies a box of a page in the background: its vector content and
    /// canvas picture for pasting here, and a 300 dpi picture for other
    /// apps. A page too large to copy as vector is copied as its picture.
    fn copy_snapshot_box(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        rect: PdfRect,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx).cloned() else {
            return;
        };
        let (resource, rotation, opacity, fallback, markups, media_box) = {
            let session = session.read(cx);
            // The markups on the box, which it copies with the page.
            let box_bounds = [rect.x, rect.y, rect.x + rect.width, rect.y + rect.height];
            let markups = session
                .annotations
                .annotations_on_page(document_id.value(), page_index)
                .into_iter()
                .filter(|markup| {
                    markup.page_bounds().is_some_and(|[x0, y0, x1, y1]| {
                        x0 < box_bounds[2]
                            && x1 > box_bounds[0]
                            && y0 < box_bounds[3]
                            && y1 > box_bounds[1]
                    })
                })
                .collect::<Vec<_>>();
            let media_box = session
                .source_page_coordinate_space(page_index)
                .map(|space| space.media_box());
            // The page image already on screen, if the worker cannot draw.
            let fallback = session
                .annotation_page_coordinate_space(page_index)
                .and_then(|space| {
                    session
                        .base_raster_of(page_index)?
                        .snapshot_asset(rect, space)
                        .ok()
                });
            (
                session.resource.clone(),
                session
                    .page_rotation(page_index)
                    .unwrap_or(PageRotation::Degrees0),
                session
                    .annotations
                    .tool_properties(AnnotationTool::Snapshot)
                    .opacity,
                fallback,
                markups,
                media_box,
            )
        };
        self.annotation_statuses
            .insert(document_id, "Copying snapshot…".into());
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let size = (rect.width, rect.height);
            // The markups, flattened into a region PDF as their appearances
            // draw them: into the page's own, or onto nothing.
            let with_markups = |region: Vec<u8>| -> Result<Vec<u8>, String> {
                let media_box = media_box.ok_or("the page's box is unavailable")?;
                let media_box =
                    PdfRect::new(media_box.x, media_box.y, media_box.width, media_box.height)
                        .map_err(|error| error.to_string())?;
                crate::pdf_engine::region_pdf_with_markups(
                    &region,
                    media_box,
                    rotation.degrees(),
                    &markups,
                )
                .map_err(|error| error.to_string())
            };
            let region_rect =
                crate::page_geometry::PdfRect::new(rect.x, rect.y, rect.width, rect.height).ok();
            let (vector, canvas, picture) = match resource {
                Some(resource) => {
                    let vector = resource.region_pdf(page_index, rect).ok().map(|region| {
                        if markups.is_empty() {
                            region
                        } else {
                            with_markups(region.clone()).unwrap_or(region)
                        }
                    });
                    // A page copied as a picture has its markups drawn over it.
                    let overlay = (vector.is_none() && !markups.is_empty())
                        .then(|| {
                            crate::pdf_page_base::blank_region_pdf(region_rect?)
                                .ok()
                                .and_then(|blank| with_markups(blank).ok())
                        })
                        .flatten();
                    let draw = |pixels_per_point: f64| match &vector {
                        Some(pdf) => {
                            resource.render_region_pdf(pdf, size, pixels_per_point, rotation)
                        }
                        None => resource
                            .render_page_region(page_index, rect, pixels_per_point, rotation)
                            .map(|mut page| {
                                if let Some(markups) = overlay.as_ref().and_then(|overlay| {
                                    resource
                                        .render_region_pdf(
                                            overlay,
                                            size,
                                            pixels_per_point,
                                            rotation,
                                        )
                                        .ok()
                                }) {
                                    page.draw_over(&markups);
                                }
                                page
                            }),
                    };
                    let canvas = draw(canvas_pixels_per_point(size))
                        .and_then(|raster| raster.to_rgba_asset())
                        .ok();
                    let picture = draw(clipboard_pixels_per_point(size)).ok();
                    (vector, canvas, picture)
                }
                None => (None, None, None),
            };
            let asset = canvas
                .or(fallback)
                .ok_or_else(|| "This page cannot be copied yet. Try again.".to_owned())?;
            let id = MarkupId::new("clipboard:snapshot").map_err(|error| error.to_string())?;
            let snapshot = SnapshotAnnotation::new(id, page_index, rect, asset, opacity)
                .map_err(|error| error.to_string())?
                .with_vector(vector.map(|pdf| SnapshotVector::new(Arc::new(pdf))));
            let png = picture.and_then(|picture| picture.to_png_on_white().ok());
            let pdf = snapshot.vector().and_then(|vector| {
                crate::pdf_page_base::upright_region_pdf(vector.pdf(), rotation.degrees()).ok()
            });
            Ok::<_, String>(SnapshotCopy { snapshot, png, pdf })
        });
        cx.spawn(async move |workspace, cx| {
            let copied = task.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let status = match copied {
                    Ok(copied) => {
                        let vector = copied.snapshot.vector().is_some();
                        let other_apps = copied.png.is_some() || copied.pdf.is_some();
                        let source = (cx.entity_id(), document_id);
                        let system_pdf = copied
                            .pdf
                            .as_deref()
                            .map(crate::pdf_page_base::page_base_id);
                        let system_image = copied
                            .png
                            .as_deref()
                            .map(crate::pdf_page_base::page_base_id);
                        cx.set_global(MarkupClipboard {
                            annotations: vec![Annotation::Snapshot(copied.snapshot)],
                            source: Some(source),
                            pastes: 0,
                            system_pdf,
                            system_image,
                        });
                        if let Some(item) = other_apps_clipboard(copied.png, copied.pdf) {
                            cx.write_to_clipboard(item);
                        }
                        snapshot_copied_status(vector, other_apps)
                    }
                    Err(error) => error,
                };
                workspace.annotation_statuses.insert(document_id, status);
                cx.notify();
            });
        })
        .detach();
    }
}

impl DocumentWorkspace {
    /// Pastes a PDF another app or another Butter Paper put on the clipboard
    /// as a Snapshot of its first page, centred on `at` (the pointer) or else
    /// on page `page_index`: vector, as a Snapshot copied here pastes.
    pub(super) fn paste_pdf_as_snapshot(
        &mut self,
        document_id: DocumentId,
        page_index: u32,
        at: Option<PdfPoint>,
        pdf: Vec<u8>,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let (resource, rotation, view_box) = {
            let session = session.read(cx);
            (
                session.resource.clone(),
                session
                    .page_rotation(page_index)
                    .unwrap_or(PageRotation::Degrees0),
                session
                    .source_page_coordinate_space(page_index)
                    .map(|space| space.view_box()),
            )
        };
        let system_pdf = crate::pdf_page_base::page_base_id(&pdf);
        self.annotation_statuses
            .insert(document_id, "Pasting…".into());
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            let (region, width, height) = crate::pdf_page_base::page_region_pdf(&pdf)?;
            let size = (width, height);
            let asset = resource
                .and_then(|resource| {
                    resource
                        .render_region_pdf(&region, size, canvas_pixels_per_point(size), rotation)
                        .and_then(|raster| raster.to_rgba_asset())
                        .ok()
                })
                .map_or_else(
                    // Drawn when the PDF worker can; transparent until then.
                    || DecodedRgbaAsset::new(1, 1, vec![0; 4]).map_err(|error| error.to_string()),
                    Ok,
                )?;
            let (x, y) = view_box.map_or((0., 0.), |view_box| {
                (
                    view_box.x + (view_box.width - width) / 2.,
                    view_box.y + (view_box.height - height) / 2.,
                )
            });
            let rect = PdfRect::new(x, y, width, height).map_err(|error| error.to_string())?;
            let id = MarkupId::new("clipboard:snapshot").map_err(|error| error.to_string())?;
            SnapshotAnnotation::new(id, page_index, rect, asset, 1.)
                .map(|snapshot| snapshot.with_vector(Some(SnapshotVector::new(Arc::new(region)))))
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |workspace, cx| {
            let snapshot = task.await;
            let _ = workspace.update(cx, |workspace, cx| {
                let result = snapshot.and_then(|snapshot| {
                    cx.set_global(MarkupClipboard {
                        annotations: vec![Annotation::Snapshot(snapshot)],
                        source: None,
                        pastes: 0,
                        system_pdf: Some(system_pdf),
                        system_image: system_clipboard_image_id(cx),
                    });
                    workspace.paste_annotations_at(document_id, page_index, at, cx)
                });
                if let Err(error) = result {
                    workspace.annotation_statuses.insert(document_id, error);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Puts a picture of a copied Snapshot markup on the system clipboard
    /// for other apps: drawn again at 300 dpi from its vector content, or
    /// its own picture.
    pub(super) fn copy_snapshot_picture(
        &mut self,
        document_id: DocumentId,
        snapshot: &SnapshotAnnotation,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self.session(document_id, cx) else {
            return;
        };
        let session = session.read(cx);
        let resource = session.resource.clone();
        let rotation = session
            .page_rotation(snapshot.page_index)
            .unwrap_or(PageRotation::Degrees0);
        let vector = snapshot.vector().map(|vector| vector.pdf().clone());
        let asset = snapshot.asset().clone();
        let size = (snapshot.rect.width, snapshot.rect.height);
        let task = cx.background_executor().spawn(async move {
            let png = vector
                .clone()
                .zip(resource)
                .and_then(|(pdf, resource)| {
                    resource
                        .render_region_pdf(&pdf, size, clipboard_pixels_per_point(size), rotation)
                        .ok()
                })
                .or_else(|| RasterSurface::from_rgba_asset(&asset).ok())
                .and_then(|picture| picture.to_png_on_white().ok());
            let pdf = vector.and_then(|pdf| {
                crate::pdf_page_base::upright_region_pdf(&pdf, rotation.degrees()).ok()
            });
            other_apps_clipboard(png, pdf)
        });
        cx.spawn(async move |_, cx| {
            if let Some(item) = task.await {
                cx.update(|cx| {
                    // This PDF and picture are the copied markup's own, not
                    // ones copied since.
                    let system_pdf = item.entries().iter().find_map(|entry| match entry {
                        gpui::ClipboardEntry::Pdf(pdf) => {
                            Some(crate::pdf_page_base::page_base_id(pdf))
                        }
                        _ => None,
                    });
                    let system_image = item.entries().iter().find_map(|entry| match entry {
                        gpui::ClipboardEntry::Image(image) => {
                            Some(crate::pdf_page_base::page_base_id(&image.bytes))
                        }
                        _ => None,
                    });
                    cx.write_to_clipboard(item);
                    if cx.has_global::<MarkupClipboard>() {
                        cx.update_global::<MarkupClipboard, _>(|clipboard, _| {
                            clipboard.system_pdf = system_pdf;
                            clipboard.system_image = system_image;
                        });
                    }
                });
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_sized_by_density_and_capped_by_edge_and_area() {
        // A small detail: four pixels to the point on the canvas, 300 dpi
        // for other apps.
        assert_eq!(canvas_pixels_per_point((200., 100.)), 4.);
        assert!((clipboard_pixels_per_point((200., 100.)) - 300. / 72.).abs() < 1e-9);
        // A whole A1 sheet: held to 2048 pixels on the canvas, and to the
        // worker's surface for the clipboard.
        let a1 = (2_384., 1_684.);
        assert!((canvas_pixels_per_point(a1) * a1.0 - 2_048.).abs() < 1e-6);
        let clipboard = clipboard_pixels_per_point(a1);
        assert!(clipboard * a1.0 <= 8_192.);
        assert!(clipboard * clipboard * a1.0 * a1.1 <= 24. * 1024. * 1024. + 1.);
    }
}
