//! Dark pages in the worker. A Dark render draws the page as usual
//! (pass A, real colours), then draws only the page's pictures on a
//! transparent bitmap (pass B) to find where pictures are, and maps the
//! ink and paper of pass A outside them with
//! [`butter_paper::paper_appearance::DarkPaperMap`].
//!
//! Pictures keep their real colours: images that are not stencil masks,
//! shading objects (`sh`), and the rectangles of Stamp annotations and
//! signature fields. Paths, text, stencil masks and form fields are ink.
//! The view draws a form field's widget through a render-only Stamp
//! (`pdf_engine::retained_widget_surrogate`); its name tells a signature
//! field (a picture) from other fields (ink).
//!
//! Pass B hides the ink objects with `FPDFPageObj_SetIsActive` and sets them
//! active again before it returns. This is safe because the worker's PDFium
//! documents are display-only: the worker edits with lopdf and loads the
//! result again. It never calls `FPDFPage_GenerateContent` and never saves
//! through PDFium, so an inactive object can never be written out. Keep it
//! so: regenerating page content while objects are inactive loses them.

use super::{LoadedPages, RasterSource, map_pdfium_error};
use butter_paper::page_geometry::{
    PageCoordinateSpace, PdfMetadataDocument, PdfPoint, PdfRect as PageRect,
    Rotation as PageRotation,
};
use butter_paper::paper_appearance::{DarkPaperMap, PaperAppearance};
use butter_paper::pdf_engine::RETAINED_WIDGET_SURROGATE_NAME;
use butter_paper::pdf_worker::{RenderReport, WorkerError, WorkerErrorCode};
use pdfium_render::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many picture passes (pass B) the worker drew: tests check that a
/// tile without pictures draws none.
pub(super) static PICTURE_PASSES: AtomicUsize = AtomicUsize::new(0);

/// Starts building the dark-page colour table on a background thread, once
/// for each process. Dark pages turn on with a render request, so the table
/// builds while PDFium draws the first page (pass A and pass B), and the
/// first page does not wait for both one after the other. A render that
/// needs the table before it is complete waits in
/// [`DarkPaperMap::shared`].
pub(super) fn warm_map() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let spawned = std::thread::Builder::new()
            .name("butter-paper-dark-pages-table".to_owned())
            .spawn(|| {
                DarkPaperMap::shared();
            });
        // Without a thread, the first render builds the table itself.
        drop(spawned);
    });
}

/// What a loaded page holds for Dark pages, found once for each page.
pub(super) struct PictureIndex {
    /// The objects pass B hides: leaf ink objects, and whole Form XObjects
    /// that hold no picture. Handles into the loaded page, which outlives
    /// this index ([`super::LoadedPage`] drops it first).
    ink: Vec<PdfPageObject<'static>>,
    /// The union of the pictures' bounds in page space (left, bottom,
    /// right, top). A picture inside a Form XObject counts with the bounds
    /// of its outermost form. `None`: no pictures.
    bounds: Option<[f64; 4]>,
    /// Stamp annotations (signature fields among them, drawn through their
    /// render-only Stamps), in page space: pictures when the render draws
    /// annotations.
    annotations: Vec<[f64; 4]>,
}

impl PictureIndex {
    pub(super) fn of(page: &PdfPage<'static>) -> Self {
        let mut index = Self {
            ink: Vec::new(),
            bounds: None,
            annotations: Vec::new(),
        };
        let objects = page.objects();
        for position in 0..objects.len() {
            let Ok(object) = objects.get(position) else {
                continue;
            };
            let may_hold_picture = matches!(
                object.object_type(),
                PdfPageObjectType::Image
                    | PdfPageObjectType::Shading
                    | PdfPageObjectType::XObjectForm
            );
            let bounds = may_hold_picture
                .then(|| object.bounds().ok())
                .flatten()
                .map(|quad| {
                    [quad.left(), quad.bottom(), quad.right(), quad.top()]
                        .map(|value| f64::from(value.value))
                });
            if classify(loaded_page_object(object), &mut index.ink) {
                // A picture without bounds: assume the whole page. Finite,
                // so that the device transform gives no infinity or NaN.
                let bounds = bounds.unwrap_or([-1e7, -1e7, 1e7, 1e7]);
                index.bounds = Some(match index.bounds {
                    None => bounds,
                    Some(union) => [
                        union[0].min(bounds[0]),
                        union[1].min(bounds[1]),
                        union[2].max(bounds[2]),
                        union[3].max(bounds[3]),
                    ],
                });
            }
        }
        for annotation in page.annotations().iter() {
            if annotation.is_hidden() {
                continue;
            }
            let picture = match annotation.annotation_type() {
                // A Stamp is a picture, unless it stands in for a form
                // field's widget in the render-only document: form fields
                // are ink. Signature fields stay pictures.
                PdfPageAnnotationType::Stamp => {
                    annotation.name().as_deref() != Some(RETAINED_WIDGET_SURROGATE_NAME)
                }
                // PDFium's annotation pass never draws a Widget itself, so
                // a Widget's rectangle holds no picture.
                _ => false,
            };
            if picture && let Ok(rect) = annotation.bounds() {
                index.annotations.push(
                    [rect.left(), rect.bottom(), rect.right(), rect.top()]
                        .map(|value| f64::from(value.value)),
                );
            }
        }
        index
    }
}

/// `object` with the lifetime of the loaded page it belongs to.
///
/// pdfium-render ties an object to the borrow of its page. A
/// `PdfPageObject` holds only PDFium handles and a `PhantomData` lifetime,
/// and the handles stay valid while PDFium keeps the page loaded.
/// [`super::LoadedPage`] drops its [`PictureIndex`] before its page, so no
/// handle outlives the page.
fn loaded_page_object(object: PdfPageObject<'_>) -> PdfPageObject<'static> {
    // SAFETY: the two types differ only in a lifetime that no field holds
    // a reference for. See above for why the handles stay valid.
    unsafe { std::mem::transmute::<PdfPageObject<'_>, PdfPageObject<'static>>(object) }
}

/// Adds the ink in `object` to `ink`, and tells whether `object` is or
/// holds a picture. A form that holds no picture is hidden whole.
fn classify(object: PdfPageObject<'static>, ink: &mut Vec<PdfPageObject<'static>>) -> bool {
    match object {
        PdfPageObject::Image(ref image) => {
            if is_stencil_mask(image) {
                ink.push(object);
                false
            } else {
                true
            }
        }
        PdfPageObject::Shading(_) => true,
        PdfPageObject::XObjectForm(form) => {
            let start = ink.len();
            let mut pictures = false;
            for position in 0..form.len() {
                if let Ok(child) = form.get(position) {
                    pictures |= classify(child, ink);
                }
            }
            if !pictures {
                ink.truncate(start);
                ink.push(PdfPageObject::XObjectForm(form));
            }
            pictures
        }
        _ => {
            ink.push(object);
            false
        }
    }
}

/// A stencil mask (`/ImageMask true`) paints the fill colour as a path
/// does. PDFium 7881 reports it with 1 bit for each pixel and no colour
/// space (checked by `stencil_masks_report_one_bit_and_no_colour_space`).
/// A 1-bit image with a colour space (scanned line art) is a picture.
fn is_stencil_mask(image: &PdfPageImageObject) -> bool {
    image.bits_per_pixel().is_ok_and(|bits| bits == 1)
        && image
            .color_space()
            .is_ok_and(|space| space == PdfColorSpace::Unknown)
}

/// Hides ink objects while it lives and sets them active again when it is
/// dropped or restored.
struct InkHidden<'i> {
    ink: &'i mut [PdfPageObject<'static>],
    hidden: usize,
}

impl<'i> InkHidden<'i> {
    fn hide(ink: &'i mut [PdfPageObject<'static>]) -> Result<Self, DarkError> {
        let mut guard = Self { ink, hidden: 0 };
        while guard.hidden < guard.ink.len() {
            if let Err(error) = guard.ink[guard.hidden].set_inactive() {
                // Restore what was hidden before the error.
                return Err(match guard.restore() {
                    true => DarkError::Render(map_pdfium_error(error)),
                    false => DarkError::Restore,
                });
            }
            guard.hidden += 1;
        }
        Ok(guard)
    }

    /// Sets every hidden object active again. False if one failed.
    fn restore(&mut self) -> bool {
        let mut restored = true;
        for object in &mut self.ink[..self.hidden] {
            restored &= object.set_active().is_ok();
        }
        self.hidden = 0;
        restored
    }
}

impl Drop for InkHidden<'_> {
    fn drop(&mut self) {
        self.restore();
    }
}

/// A Dark render that failed.
#[derive(Debug)]
pub(super) enum DarkError {
    Render(WorkerError),
    /// An ink object could not be set active again: the loaded page must
    /// not be drawn again.
    Restore,
}

impl From<WorkerError> for DarkError {
    fn from(error: WorkerError) -> Self {
        Self::Render(error)
    }
}

impl DarkError {
    /// The error to report for a page that is not kept loaded.
    pub(super) fn into_worker_error(self) -> WorkerError {
        match self {
            Self::Render(error) => error,
            Self::Restore => WorkerError::new(WorkerErrorCode::PageError),
        }
    }

    pub(super) fn needs_reload(&self) -> bool {
        matches!(self, Self::Restore)
    }

    /// The error to report. A page that must be loaded again is closed
    /// first.
    pub(super) fn settle(
        self,
        pages: &mut LoadedPages,
        source: RasterSource,
        index: i32,
    ) -> WorkerError {
        match self {
            Self::Render(error) => error,
            Self::Restore => {
                pages.forget(source, index);
                WorkerError::with_detail(
                    WorkerErrorCode::PageError,
                    "Dark pages could not restore the page's objects",
                )
            }
        }
    }
}

/// The page's space with no `/UserUnit`, as PDFium draws it, from the
/// displayed document's inherited page dictionary.
pub(super) fn canonical_space(
    metadata: &PdfMetadataDocument,
    page_index: i32,
) -> Option<PageCoordinateSpace> {
    let number = u32::try_from(page_index).ok()?.checked_add(1)?;
    let id = *metadata.get_pages().get(&number)?;
    let space = PageCoordinateSpace::from_lopdf_page(metadata, id).ok()?;
    PageCoordinateSpace::new(space.media_box(), space.view_box(), space.rotation(), 1.).ok()
}

/// The page's space from PDFium's boxes (for pages with their own boxes).
fn pdfium_space(page: &PdfPage) -> Option<PageCoordinateSpace> {
    let boundaries = page.boundaries();
    let media = boundaries.media().ok()?.bounds;
    let view = boundaries.crop().map(|crop| crop.bounds).unwrap_or(media);
    let rect = |rect: PdfRect| {
        PageRect::new(
            f64::from(rect.left().value),
            f64::from(rect.bottom().value),
            f64::from(rect.width().value),
            f64::from(rect.height().value),
        )
        .ok()
    };
    let rotation = match page.rotation().ok()? {
        PdfPageRenderRotation::None => PageRotation::Degrees0,
        PdfPageRenderRotation::Degrees90 => PageRotation::Degrees90,
        PdfPageRenderRotation::Degrees180 => PageRotation::Degrees180,
        PdfPageRenderRotation::Degrees270 => PageRotation::Degrees270,
    };
    PageCoordinateSpace::new(rect(media)?, rect(view)?, rotation, 1.).ok()
}

/// The pixel box (left, top, right, bottom) a page-space box covers.
fn device_box(space: PageCoordinateSpace, transform: [f32; 6], [l, b, r, t]: [f64; 4]) -> [f64; 4] {
    let [a, bb, c, d, e, f] = transform.map(f64::from);
    let corners = [(l, b), (r, b), (l, t), (r, t)].map(|(x, y)| {
        let point = space.pdf_to_viewport(PdfPoint::new(x, y));
        (
            a * point.x + c * point.y + e,
            bb * point.x + d * point.y + f,
        )
    });
    let xs = corners.map(|point| point.0);
    let ys = corners.map(|point| point.1);
    [
        xs.into_iter().fold(f64::INFINITY, f64::min),
        ys.into_iter().fold(f64::INFINITY, f64::min),
        xs.into_iter().fold(f64::NEG_INFINITY, f64::max),
        ys.into_iter().fold(f64::NEG_INFINITY, f64::max),
    ]
}

fn reaches(device: [f64; 4], (width, height): (i32, i32)) -> bool {
    device[0] < f64::from(width)
        && device[1] < f64::from(height)
        && device[2] > 0.
        && device[3] > 0.
}

/// Draws pass B and maps `output` (pass A) for a Dark render. Returns the
/// picture coverage of each pixel, when pictures reach the surface.
pub(super) fn darken(
    page: &PdfPage<'static>,
    pictures: &mut Option<PictureIndex>,
    space: Option<PageCoordinateSpace>,
    transform: [f32; 6],
    (width, height): (i32, i32),
    render_annotations: bool,
    draft: bool,
    output: &mut [u8],
) -> Result<Option<Vec<u8>>, DarkError> {
    let index = pictures.get_or_insert_with(|| PictureIndex::of(page));
    let space = space.or_else(|| pdfium_space(page));
    let size = (width, height);
    // Without the page's space, a tile cannot be tested: draw pass B.
    let pictures_reach = index.bounds.is_some_and(|bounds| {
        space.is_none_or(|space| reaches(device_box(space, transform, bounds), size))
    });
    let mut coverage = if pictures_reach {
        Some(picture_pass(page, &mut index.ink, transform, size, draft)?)
    } else {
        None
    };
    if render_annotations && let Some(space) = space {
        for rect in &index.annotations {
            let device = device_box(space, transform, *rect);
            if !reaches(device, size) {
                continue;
            }
            let mask = coverage.get_or_insert_with(|| vec![0; (width * height) as usize]);
            let clamp = |value: f64, limit: i32| value.round().clamp(0., f64::from(limit)) as usize;
            let (left, right) = (clamp(device[0], width), clamp(device[2], width));
            for row in clamp(device[1], height)..clamp(device[3], height) {
                let start = row * width as usize;
                mask[start + left..start + right].fill(255);
            }
        }
    }
    let map = DarkPaperMap::shared();
    match &coverage {
        Some(mask) => map.map_bgra_premultiplied_outside(output, mask),
        None => map.map_bgra_premultiplied(output),
    }
    Ok(coverage)
}

/// [`darken`] for a page loaded only for this render (a one-page PDF of
/// dragged content): its picture index lives only as long as the call.
pub(super) fn darken_page(
    page: &PdfPage<'_>,
    transform: [f32; 6],
    size: (i32, i32),
    output: &mut [u8],
) -> Result<(), DarkError> {
    // SAFETY: the picture index made here holds handles into `page` and is
    // dropped before this function returns, while `page` is still loaded.
    let page = unsafe { std::mem::transmute::<&PdfPage<'_>, &PdfPage<'static>>(page) };
    let mut pictures = None;
    darken(
        page,
        &mut pictures,
        None,
        transform,
        size,
        false,
        false,
        output,
    )?;
    drop(pictures);
    Ok(())
}

/// Pass B: only the page's pictures, on a transparent bitmap, without
/// annotations. The alpha of each pixel is its picture coverage: it
/// includes clip paths, soft masks and partial transparency.
fn picture_pass(
    page: &PdfPage<'static>,
    ink: &mut [PdfPageObject<'static>],
    [a, b, c, d, e, f]: [f32; 6],
    (width, height): (i32, i32),
    draft: bool,
) -> Result<Vec<u8>, DarkError> {
    PICTURE_PASSES.fetch_add(1, Ordering::Relaxed);
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    let mut hidden = InkHidden::hide(ink)?;
    let rendered = (|| {
        let mut bitmap = PdfBitmap::from_bytes(width, height, PdfBitmapFormat::BGRA, &mut pixels)
            .map_err(map_pdfium_error)?;
        let config = PdfRenderConfig::new()
            .set_reverse_byte_order(false)
            .set_fixed_size(width, height)
            .render_form_data(false)
            .render_annotations(false)
            .reset_matrix(PdfMatrix::new(a, b, c, d, e, f))
            .map_err(map_pdfium_error)?
            .clip(0, 0, width, height)
            .set_image_smoothing(!draft)
            .set_clear_color(PdfColor::new(255, 255, 255, 0));
        page.render_into_bitmap_with_config(&mut bitmap, &config)
            .map_err(map_pdfium_error)
    })();
    if !hidden.restore() {
        return Err(DarkError::Restore);
    }
    drop(hidden);
    rendered?;
    Ok(pixels.chunks_exact(4).map(|pixel| pixel[3]).collect())
}

/// The report of a render with `paper` and picture `coverage`.
pub(super) fn report(paper: PaperAppearance, coverage: Option<&[u8]>) -> RenderReport {
    let picture_coverage = match paper {
        PaperAppearance::Real => None,
        PaperAppearance::Dark => Some(coverage.map_or(0., |mask| {
            let sum: u64 = mask.iter().map(|&value| u64::from(value)).sum();
            (sum as f64 / (mask.len().max(1) as f64 * 255.)) as f32
        })),
    };
    RenderReport { picture_coverage }
}

#[cfg(test)]
pub(super) mod fixtures {
    use lopdf::{Document, Object, ObjectId, Stream, dictionary};

    fn image(pdf: &mut Document, width: i64, height: i64, rgb: Vec<u8>) -> ObjectId {
        pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => width, "Height" => height,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            rgb,
        ))
    }

    fn one_page(
        mut pdf: Document,
        pages: ObjectId,
        size: [i64; 2],
        resources: lopdf::Dictionary,
        content: Vec<u8>,
        annotations: Vec<Object>,
    ) -> Vec<u8> {
        let contents = pdf.add_object(Stream::new(dictionary! {}, content));
        let page = pdf.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), size[0].into(), size[1].into()],
            "Resources" => resources, "Contents" => contents, "Annots" => annotations,
        });
        pdf.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let catalog = pdf.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        pdf.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        bytes
    }

    /// A 200 x 200 pt page with ink of several colours, text, an RGB image,
    /// a Form XObject with an image and a black path, a Form XObject with
    /// only a path, a stencil mask, a 1-bit grey image and an axial shading.
    /// [`super::tests::MIXED_POINTS`] names a point in each.
    pub(crate) fn mixed_page() -> Vec<u8> {
        let mut pdf = Document::with_version("1.7");
        let pages = pdf.new_object_id();
        // Black, white / red, blue: the 2 x 2 image, top row first.
        let corners = image(
            &mut pdf,
            2,
            2,
            vec![0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 0, 255],
        );
        let form_image = image(&mut pdf, 1, 1, vec![0x20, 0x40, 0x80]);
        let form = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 60.into(), 40.into()],
                "Resources" => dictionary! { "XObject" => dictionary! { "Im2" => form_image } },
            },
            b"q 30 0 0 40 0 0 cm /Im2 Do Q 0 0 0 rg 35 5 20 30 re f".to_vec(),
        ));
        let ink_form = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 30.into(), 20.into()],
                "Resources" => dictionary! {},
            },
            b"0 0 0 rg 0 0 30 20 re f".to_vec(),
        ));
        let stencil = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 8, "Height" => 8,
                "ImageMask" => true, "BitsPerComponent" => 1,
            },
            vec![0; 8],
        ));
        let line_art = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 8, "Height" => 8,
                "ColorSpace" => "DeviceGray", "BitsPerComponent" => 1,
            },
            vec![0; 8],
        ));
        let shading = pdf.add_object(dictionary! {
            "ShadingType" => 2, "ColorSpace" => "DeviceRGB",
            "Coords" => vec![140.into(), 0.into(), 190.into(), 0.into()],
            "Extend" => vec![true.into(), true.into()],
            "Function" => dictionary! {
                "FunctionType" => 2, "Domain" => vec![0.into(), 1.into()],
                "C0" => vec![1.into(), 0.into(), 0.into()],
                "C1" => vec![0.into(), 0.into(), 1.into()], "N" => 1,
            },
        });
        let content = b"q 0 0 0.502 rg 10 10 40 40 re f Q\n\
            q 0 g 60 10 30 30 re f Q\n\
            q 1 1 0 rg 100 10 30 30 re f Q\n\
            q 0.867 0.933 1 rg 140 10 50 30 re f Q\n\
            q 1 0 0 RG 8 w 10 70 m 90 70 l S Q\n\
            q 0 0 1 RG 8 w 110 70 m 190 70 l S Q\n\
            BT /F1 16 Tf 0 g 10 85 Td (Ink and paper) Tj ET\n\
            q 40 0 0 40 10 110 cm /Im1 Do Q\n\
            q 1 0 0 1 60 110 cm /Fm1 Do Q\n\
            q 1 0 0 1 60 160 cm /Fm2 Do Q\n\
            q 0.545 0 0 rg 30 0 0 30 125 110 cm /Mk1 Do Q\n\
            q 24 0 0 24 170 110 cm /Im3 Do Q\n\
            q 140 160 50 30 re W n /Sh1 sh Q\n\
            q 0 G 2 w 10 190 m 130 190 l S Q\n"
            .to_vec();
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            } },
            "XObject" => dictionary! {
                "Im1" => corners, "Fm1" => form, "Fm2" => ink_form, "Mk1" => stencil,
                "Im3" => line_art,
            },
            "Shading" => dictionary! { "Sh1" => shading },
        };
        one_page(pdf, pages, [200, 200], resources, content, Vec::new())
    }

    /// A 100 x 100 pt page that is one image: a scan.
    pub(crate) fn scanned_page() -> Vec<u8> {
        let mut pdf = Document::with_version("1.7");
        let pages = pdf.new_object_id();
        let scan = image(&mut pdf, 2, 1, vec![0, 0, 0, 0xF0, 0xF0, 0xE0]);
        let resources = dictionary! { "XObject" => dictionary! { "Im1" => scan } };
        one_page(
            pdf,
            pages,
            [100, 100],
            resources,
            b"q 100 0 0 100 0 0 cm /Im1 Do Q".to_vec(),
            Vec::new(),
        )
    }

    /// A 160 x 160 pt page with a black square, a red Stamp annotation at
    /// (20, 20, 60, 60) and a blue Square annotation at (100, 20, 140, 60).
    pub(crate) fn stamp_page() -> Vec<u8> {
        let mut pdf = Document::with_version("1.7");
        let pages = pdf.new_object_id();
        let mut annotations = Vec::new();
        for (subtype, x, colour) in [("Stamp", 20, "1 0 0"), ("Square", 100, "0 0 1")] {
            let appearance = pdf.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), 40.into(), 40.into()],
                    "Resources" => dictionary! {},
                },
                format!("{colour} rg 0 0 40 40 re f").into_bytes(),
            ));
            let annotation = pdf.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => subtype, "F" => 4,
                "Rect" => vec![x.into(), 20.into(), (x + 40).into(), 60.into()],
                "AP" => dictionary! { "N" => appearance },
            });
            annotations.push(Object::Reference(annotation));
        }
        one_page(
            pdf,
            pages,
            [160, 160],
            dictionary! {},
            b"0 0 0 rg 5 100 40 40 re f".to_vec(),
            annotations,
        )
    }

    /// A 160 x 160 pt page with a black square at (5, 100, 45, 140), a
    /// visible digital-signature field (`/FT /Sig`) at (20, 20, 100, 60)
    /// whose appearance is a red box with a blue stroke inside (an image of
    /// a hand signature stands in the same place in real files), and a text
    /// field (`/FT /Tx`) at (60, 100, 150, 140): white with a navy box.
    pub(crate) fn signature_page() -> Vec<u8> {
        let mut pdf = Document::with_version("1.7");
        let pages = pdf.new_object_id();
        let appearance = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 80.into(), 40.into()],
                "Resources" => dictionary! {},
            },
            b"1 0 0 rg 0 0 80 40 re f 0 0 1 RG 4 w 10 20 m 70 20 l S".to_vec(),
        ));
        let widget = pdf.new_object_id();
        let text_field = pdf.new_object_id();
        let page = pdf.new_object_id();
        let text_appearance = pdf.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 90.into(), 40.into()],
                "Resources" => dictionary! {},
            },
            b"1 1 1 rg 0 0 90 40 re f 0 0 0.5 rg 10 10 70 20 re f".to_vec(),
        ));
        pdf.objects.insert(
            text_field,
            Object::Dictionary(dictionary! {
                "Type" => "Annot", "Subtype" => "Widget", "F" => 4,
                "FT" => "Tx", "T" => Object::string_literal("Name"),
                "Rect" => vec![60.into(), 100.into(), 150.into(), 140.into()],
                "AP" => dictionary! { "N" => text_appearance },
                "P" => page,
            }),
        );
        pdf.objects.insert(
            widget,
            Object::Dictionary(dictionary! {
                "Type" => "Annot", "Subtype" => "Widget", "F" => 4,
                "FT" => "Sig", "T" => Object::string_literal("Signature1"),
                "Rect" => vec![20.into(), 20.into(), 100.into(), 60.into()],
                "AP" => dictionary! { "N" => appearance },
                "P" => page,
            }),
        );
        let contents = pdf.add_object(Stream::new(
            dictionary! {},
            b"0 0 0 rg 5 100 40 40 re f".to_vec(),
        ));
        pdf.objects.insert(
            page,
            Object::Dictionary(dictionary! {
                "Type" => "Page", "Parent" => pages,
                "MediaBox" => vec![0.into(), 0.into(), 160.into(), 160.into()],
                "Resources" => dictionary! {}, "Contents" => contents,
                "Annots" => vec![widget.into(), text_field.into()],
            }),
        );
        pdf.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let catalog = pdf.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => pages,
            "AcroForm" => dictionary! {
                "Fields" => vec![widget.into(), text_field.into()], "SigFlags" => 3,
            },
        });
        pdf.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        pdf.save_to(&mut bytes).unwrap();
        bytes
    }

    /// An A1 drawing (2384 x 1684 pt) with `lines` line segments, hatching,
    /// text, and a small logo image when `logo` is set. A `dense` page has
    /// long segments at any angle in six colours that cross everywhere (a
    /// worst case: almost every pixel is a different blend). Otherwise the
    /// page is like a drawing: short, mostly black, mostly axis-aligned
    /// segments.
    pub(crate) fn large_vector_page(lines: usize, dense: bool, logo: bool) -> Vec<u8> {
        let mut pdf = Document::with_version("1.7");
        let pages = pdf.new_object_id();
        let colours = [
            "0 0 0",
            "1 0 0",
            "0 0 0.5",
            "0 0.4 0",
            "0.5 0.5 0.5",
            "0 0 1",
        ];
        let mut content = String::from("0.5 w\n");
        let mut state: u64 = 0x5EED;
        let mut next = |limit: u64| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state % limit
        };
        for line in 0..lines {
            if line % 500 == 0 {
                // A drawing: five black layers for each coloured one.
                let colour = match dense {
                    true => colours[(line / 500) % colours.len()],
                    false if (line / 500) % 6 == 5 => colours[1 + (line / 3_000) % 5],
                    false => colours[0],
                };
                content.push_str(&format!("{colour} RG\n"));
            }
            let (x, y) = (next(2_300) + 40, next(1_600) + 40);
            let (dx, dy) = match (dense, next(10)) {
                (true, _) => (next(200), next(200)),
                (false, 0..=3) => (next(40), 0),
                (false, 4..=7) => (0, next(40)),
                (false, _) => (next(40), next(40)),
            };
            content.push_str(&format!("{x} {y} m {} {} l S\n", x + dx, y + dy));
        }
        // Hatching: 45-degree lines over a 600 x 400 box.
        content.push_str("0 G 0.25 w\n");
        for step in (0..1_000).step_by(6) {
            content.push_str(&format!(
                "{} 200 m {} 600 l S\n",
                1_400 + step,
                1_000 + step
            ));
        }
        content.push_str("BT /F1 8 Tf 0 g\n");
        for row in 0..120 {
            content.push_str(&format!(
                "1 0 0 1 60 {} Tm (Room {row} - door schedule D{row:03} - 2400 x 900) Tj\n",
                60 + row * 12
            ));
        }
        content.push_str("ET\n");
        let mut xobjects = dictionary! {};
        if logo {
            let pixels: Vec<u8> = (0..64 * 64)
                .flat_map(|i| [(i % 64 * 4) as u8, (i / 64 * 4) as u8, 0x80])
                .collect();
            xobjects.set("Logo", image(&mut pdf, 64, 64, pixels));
            content.push_str("q 120 0 0 120 2200 1500 cm /Logo Do Q\n");
        }
        let resources = dictionary! {
            "Font" => dictionary! { "F1" => dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
            } },
            "XObject" => xobjects,
        };
        one_page(
            pdf,
            pages,
            [2_384, 1_684],
            resources,
            content.into_bytes(),
            Vec::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{source_handle, test_backend};
    use super::super::*;
    use super::PICTURE_PASSES;
    use super::fixtures;
    use butter_paper::paper_appearance::{DarkPaper, DarkPaperMap, PaperAppearance};
    use butter_paper::pdf_worker::{
        ClipRect, JobId, SessionId, SurfaceDescriptor, SurfaceFormat, SurfaceId,
    };
    use std::sync::atomic::Ordering;

    /// Points (PDF user space) in [`fixtures::mixed_page`], and whether
    /// each keeps its real colour in Dark pages.
    pub(super) const MIXED_POINTS: [(&str, f32, f32, bool); 17] = [
        ("paper", 150., 100., false),
        ("navy fill", 30., 30., false),
        ("black fill", 75., 25., false),
        ("yellow fill", 115., 25., false),
        ("pale blue fill", 165., 25., false),
        ("red line", 50., 70., false),
        ("blue line", 150., 70., false),
        ("black line", 100., 190., false),
        ("image black", 20., 140., true),
        ("image white", 40., 140., true),
        ("image red", 20., 120., true),
        ("image blue", 40., 120., true),
        ("image in a form", 75., 130., true),
        ("path in a form", 105., 130., false),
        ("form of only a path", 75., 170., false),
        ("stencil mask", 140., 125., false),
        ("1-bit grey image", 182., 122., true),
    ];
    const SHADING_POINT: (f32, f32) = (165., 175.);

    fn library() -> std::ffi::OsString {
        env::var_os("BP_PDFIUM_LIBRARY").expect("BP_PDFIUM_LIBRARY")
    }

    fn open(backend: &mut PdfiumBackend, bytes: &[u8], name: &str) -> PdfiumDocument {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-scratch");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}-{}.pdf", std::process::id()));
        std::fs::write(&path, bytes).unwrap();
        let source = File::open(&path).unwrap();
        let (document, _) = backend.open(source_handle(&source), None).unwrap();
        std::fs::remove_file(&path).unwrap();
        document
    }

    fn request(
        (width, height): (u32, u32),
        transform: [f32; 6],
        annotation_mode: AnnotationRenderMode,
        paper: PaperAppearance,
    ) -> RenderRequest {
        RenderRequest {
            job_id: JobId(1),
            session_id: SessionId(1),
            page_index: 0,
            annotation_mode,
            transform,
            clip: ClipRect {
                x: 0,
                y: 0,
                width,
                height,
            },
            surface: SurfaceDescriptor {
                surface_id: SurfaceId(1),
                width,
                height,
                stride: width * 4,
                byte_len: u64::from(width) * u64::from(height) * 4,
                format: SurfaceFormat::Bgra8Premultiplied,
            },
            draft: false,
            paper,
        }
    }

    fn render(
        backend: &mut PdfiumBackend,
        document: &mut PdfiumDocument,
        request: &RenderRequest,
    ) -> (Vec<u8>, RenderReport) {
        let mut pixels = vec![0; request.surface.byte_len as usize];
        let report = backend
            .render_crop(document, request, &mut pixels, &AtomicBool::new(false))
            .unwrap();
        (pixels, report)
    }

    /// The RGB at PDF point (x, y) of a `width`-pixel-wide render of a
    /// page `page_height` points high at `scale`.
    fn rgb_at(
        pixels: &[u8],
        width: u32,
        page_height: f32,
        scale: f32,
        (x, y): (f32, f32),
    ) -> [u8; 3] {
        let (column, row) = ((x * scale) as usize, ((page_height - y) * scale) as usize);
        let at = (row * width as usize + column) * 4;
        [pixels[at + 2], pixels[at + 1], pixels[at]]
    }

    /// Saves BGRA `pixels` as a PNG under the workspace's
    /// `target/dark-mode-step1`, for review.
    fn save_png(name: &str, pixels: &[u8], width: u32, height: u32) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/dark-mode-step1");
        std::fs::create_dir_all(&dir).unwrap();
        let rgba: Vec<u8> = pixels
            .chunks_exact(4)
            .flat_map(|pixel| [pixel[2], pixel[1], pixel[0], pixel[3]])
            .collect();
        image::RgbaImage::from_raw(width, height, rgba)
            .unwrap()
            .save(dir.join(format!("{name}.png")))
            .unwrap();
    }

    /// Every object of the page, forms' children too, is active.
    fn all_active(page: &PdfPage) -> bool {
        fn walk(object: &PdfPageObject) -> bool {
            object.is_active().unwrap()
                && object.as_x_object_form_object().is_none_or(|form| {
                    (0..form.len()).all(|position| walk(&form.get(position).unwrap()))
                })
        }
        page.objects().iter().all(|object| walk(&object))
    }

    #[test]
    fn page_raster_keys_differ_for_real_and_dark() {
        let size = (8, 8);
        let real = request(
            size,
            [1., 0., 0., 1., 0., 0.],
            AnnotationRenderMode::None,
            PaperAppearance::Real,
        );
        let dark = RenderRequest {
            paper: PaperAppearance::Dark,
            ..real.clone()
        };
        let key = |request: &RenderRequest| PageRasterKey::of(RasterSource::Display, 0, request);
        assert!(key(&real) != key(&dark));
        assert!(key(&real) == key(&real.clone()));
    }

    /// Checked fact (PDFium 7881): `FPDFPageObj_SetIsActive` hides an
    /// object inside a Form XObject, and making it active again draws the
    /// page as before, byte for byte.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn set_is_active_hides_objects_inside_form_xobjects() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let mut document = open(&mut backend, &fixtures::mixed_page(), "bp-dark-forms");
        let real = request(
            (400, 400),
            [2., 0., 0., 2., 0., 0.],
            AnnotationRenderMode::None,
            PaperAppearance::Real,
        );
        let (before, _) = render(&mut backend, &mut document, &real);
        {
            let page = document.document.pages().get(0).unwrap();
            let form = page
                .objects()
                .iter()
                .find(|object| {
                    object
                        .as_x_object_form_object()
                        .is_some_and(|form| form.len() == 2)
                })
                .unwrap();
            let form = form.as_x_object_form_object().unwrap();
            let mut path = (0..form.len())
                .map(|position| form.get(position).unwrap())
                .find(|child| child.object_type() == PdfPageObjectType::Path)
                .unwrap();
            path.set_inactive().unwrap();
            let mut pixels = vec![0; 400 * 400 * 4];
            render_page_into(&page, real.transform, (400, 400), false, &real, &mut pixels).unwrap();
            let at = |pixels: &[u8], point| rgb_at(pixels, 400, 200., 2., point);
            assert_eq!(
                at(&before, (105., 130.)),
                [0, 0, 0],
                "the form's path draws"
            );
            assert_eq!(
                at(&pixels, (105., 130.)),
                [255; 3],
                "the inactive path is hidden"
            );
            assert_eq!(
                at(&pixels, (75., 130.)),
                [0x20, 0x40, 0x80],
                "its sibling draws"
            );
            path.set_active().unwrap();
            render_page_into(&page, real.transform, (400, 400), false, &real, &mut pixels).unwrap();
            assert!(pixels == before, "active again, the page draws as before");
        }
        backend.close(document);
    }

    /// Checked fact (PDFium 7881): a stencil mask reports 1 bit for each
    /// pixel and no colour space. A 1-bit DeviceGray image reports its
    /// colour space.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn stencil_masks_report_one_bit_and_no_colour_space() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let document = open(&mut backend, &fixtures::mixed_page(), "bp-dark-stencil");
        let page = document.document.pages().get(0).unwrap();
        let images: Vec<(u8, PdfColorSpace)> = page
            .objects()
            .iter()
            .filter_map(|object| {
                object.as_image_object().map(|image| {
                    (
                        image.bits_per_pixel().unwrap(),
                        image.color_space().unwrap(),
                    )
                })
            })
            .collect();
        assert_eq!(
            images,
            [
                (24, PdfColorSpace::DeviceRGB),
                (1, PdfColorSpace::Unknown),
                (1, PdfColorSpace::DeviceGray),
            ]
        );
        drop(page);
        backend.close(document);
    }

    /// Pictures keep their real pixels, ink and paper are mapped, every
    /// object is active after the render, the next Real render is
    /// unchanged, and the document is not.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn dark_pages_map_ink_and_keep_pictures_real() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let bytes = fixtures::mixed_page();
        let mut document = open(&mut backend, &bytes, "bp-dark-mixed");
        // A PDFium save gives each copy a new random trailer `/ID`.
        let save = |document: &PdfiumDocument| {
            let bytes = document.document.save_to_bytes().unwrap();
            let trailer = bytes
                .windows(7)
                .rposition(|window| window == b"trailer")
                .unwrap();
            bytes[..trailer].to_vec()
        };
        let saved_before = save(&document);
        let (scale, size) = (3., 600);
        let real = request(
            (size, size),
            [scale, 0., 0., scale, 0., 0.],
            AnnotationRenderMode::None,
            PaperAppearance::Real,
        );
        let dark = RenderRequest {
            paper: PaperAppearance::Dark,
            ..real.clone()
        };
        let (real_pixels, real_report) = render(&mut backend, &mut document, &real);
        let passes = PICTURE_PASSES.load(Ordering::Relaxed);
        let (dark_pixels, dark_report) = render(&mut backend, &mut document, &dark);
        assert_eq!(PICTURE_PASSES.load(Ordering::Relaxed), passes + 1);
        save_png("mixed-real", &real_pixels, size, size);
        save_png("mixed-dark", &dark_pixels, size, size);

        let map = DarkPaperMap::shared();
        let at = |pixels: &[u8], point| rgb_at(pixels, size, 200., scale, point);
        for (name, x, y, picture) in MIXED_POINTS {
            let (real, dark) = (at(&real_pixels, (x, y)), at(&dark_pixels, (x, y)));
            let expected = if picture { real } else { map.map(real) };
            assert_eq!(dark, expected, "{name}: real {real:02X?}, dark {dark:02X?}");
        }
        assert_eq!(at(&dark_pixels, (150., 100.)), DarkPaper::DEFAULT.paper);
        assert_eq!(at(&dark_pixels, (75., 25.)), DarkPaper::DEFAULT.ink);
        let (real_shading, dark_shading) = (
            at(&real_pixels, SHADING_POINT),
            at(&dark_pixels, SHADING_POINT),
        );
        assert_eq!(dark_shading, real_shading, "a shading is a picture");
        assert!(real_shading != map.map(real_shading));
        // Text is ink: some text pixel is light on dark paper.
        let text_lightest = (12..110)
            .flat_map(|x| (85..97).map(move |y| (x as f32, y as f32)))
            .map(|point| at(&dark_pixels, point)[0])
            .max()
            .unwrap();
        assert!(text_lightest > 0xC0, "text is light: {text_lightest:02X}");

        assert_eq!(real_report.picture_coverage, None);
        let coverage = dark_report.picture_coverage.unwrap();
        assert!(
            (0.05..0.3).contains(&coverage),
            "picture coverage {coverage}"
        );

        let page = document.document.pages().get(0).unwrap();
        assert!(all_active(&page), "every object is active after the render");
        drop(page);
        // Clear the cached page rasters, then draw Real again.
        document.loaded_pages.1.rasters.clear();
        let (again, _) = render(&mut backend, &mut document, &real);
        assert!(again == real_pixels, "the next Real render is unchanged");
        assert!(
            save(&document) == saved_before,
            "PDFium's document is unchanged"
        );
        assert!(document.source_bytes.as_slice() == bytes.as_slice());
        backend.close(document);
    }

    /// A tile with no picture in it draws no picture pass. A tile with one
    /// does.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_tile_without_pictures_draws_no_picture_pass() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let mut document = open(&mut backend, &fixtures::mixed_page(), "bp-dark-tiles");
        // Scale 2: the bottom-left 50 x 50 pt (navy and black fills only).
        let ink_tile = request(
            (100, 100),
            [2., 0., 0., 2., 0., -300.],
            AnnotationRenderMode::None,
            PaperAppearance::Dark,
        );
        let passes = PICTURE_PASSES.load(Ordering::Relaxed);
        let (_, report) = render(&mut backend, &mut document, &ink_tile);
        assert_eq!(PICTURE_PASSES.load(Ordering::Relaxed), passes, "no pass B");
        assert_eq!(report.picture_coverage, Some(0.));
        // The tile with the 2 x 2 image (10..50, 110..150 pt).
        let picture_tile = request(
            (100, 100),
            [2., 0., 0., 2., 0., -100.],
            AnnotationRenderMode::None,
            PaperAppearance::Dark,
        );
        let (_, report) = render(&mut backend, &mut document, &picture_tile);
        assert_eq!(PICTURE_PASSES.load(Ordering::Relaxed), passes + 1);
        assert!(report.picture_coverage.unwrap() > 0.1);
        backend.close(document);
    }

    /// A full-page scan keeps its real colours and reports its coverage.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_full_page_scan_keeps_real_colours_and_reports_its_coverage() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let mut document = open(&mut backend, &fixtures::scanned_page(), "bp-dark-scan");
        let real = request(
            (200, 200),
            [2., 0., 0., 2., 0., 0.],
            AnnotationRenderMode::None,
            PaperAppearance::Real,
        );
        let dark = RenderRequest {
            paper: PaperAppearance::Dark,
            ..real.clone()
        };
        let (real_pixels, _) = render(&mut backend, &mut document, &real);
        let (dark_pixels, report) = render(&mut backend, &mut document, &dark);
        save_png("scan-real", &real_pixels, 200, 200);
        save_png("scan-dark", &dark_pixels, 200, 200);
        assert!(dark_pixels == real_pixels, "a scan keeps its real colours");
        assert!(report.picture_coverage.unwrap() >= 0.99);
        backend.close(document);
    }

    /// A Stamp annotation keeps its real colours inside its `/Rect`. Other
    /// annotations are ink.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_stamp_annotation_keeps_real_colours_inside_its_rect() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let mut document = open(&mut backend, &fixtures::stamp_page(), "bp-dark-stamp");
        let real = request(
            (320, 320),
            [2., 0., 0., 2., 0., 0.],
            AnnotationRenderMode::All,
            PaperAppearance::Real,
        );
        let dark = RenderRequest {
            paper: PaperAppearance::Dark,
            ..real.clone()
        };
        let (real_pixels, _) = render(&mut backend, &mut document, &real);
        let passes = PICTURE_PASSES.load(Ordering::Relaxed);
        let (dark_pixels, report) = render(&mut backend, &mut document, &dark);
        save_png("stamp-real", &real_pixels, 320, 320);
        save_png("stamp-dark", &dark_pixels, 320, 320);
        assert_eq!(
            PICTURE_PASSES.load(Ordering::Relaxed),
            passes,
            "no picture objects"
        );
        let at = |pixels: &[u8], point| rgb_at(pixels, 320, 160., 2., point);
        assert_eq!(at(&real_pixels, (40., 40.)), [255, 0, 0]);
        assert_eq!(
            at(&dark_pixels, (40., 40.)),
            [255, 0, 0],
            "the Stamp is real"
        );
        let map = DarkPaperMap::shared();
        assert_eq!(
            at(&dark_pixels, (120., 40.)),
            map.map([0, 0, 255]),
            "a Square is ink"
        );
        assert_eq!(at(&dark_pixels, (25., 120.)), DarkPaper::DEFAULT.ink);
        let coverage = report.picture_coverage.unwrap();
        assert!(
            (coverage - 0.0625).abs() < 0.01,
            "the Stamp's 40 x 40 of 160 x 160: {coverage}"
        );
        backend.close(document);
    }

    /// A visible digital-signature field (a real `/FT /Sig` widget) keeps
    /// its real colours inside its `/Rect` in the view's render, as a hand
    /// signature image must. A text field is ink, as is the page around.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_signature_field_keeps_real_colours_and_other_fields_are_ink() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let mut document = open(
            &mut backend,
            &fixtures::signature_page(),
            "bp-dark-signature",
        );
        let map = DarkPaperMap::shared();
        let at = |pixels: &[u8], point| rgb_at(pixels, 320, 160., 2., point);
        // The view draws with RetainedOnly: form fields through render-only
        // Stamps.
        let real = request(
            (320, 320),
            [2., 0., 0., 2., 0., 0.],
            AnnotationRenderMode::RetainedOnly,
            PaperAppearance::Real,
        );
        let dark = RenderRequest {
            paper: PaperAppearance::Dark,
            ..real.clone()
        };
        let (real_pixels, _) = render(&mut backend, &mut document, &real);
        let (dark_pixels, report) = render(&mut backend, &mut document, &dark);
        save_png("signature-real", &real_pixels, 320, 320);
        save_png("signature-dark", &dark_pixels, 320, 320);
        assert_eq!(
            at(&real_pixels, (30., 30.)),
            [255, 0, 0],
            "the view draws the field"
        );
        for point in [(30., 30.), (90., 50.), (60., 40.)] {
            assert_eq!(
                at(&dark_pixels, point),
                at(&real_pixels, point),
                "the signature field is real at {point:?}"
            );
        }
        // The text field: its white ground is paper, its navy box is ink.
        assert_eq!(at(&real_pixels, (140., 135.)), [255, 255, 255]);
        assert_eq!(at(&dark_pixels, (140., 135.)), DarkPaper::DEFAULT.paper);
        assert_eq!(at(&dark_pixels, (100., 120.)), map.map([0, 0, 128]));
        assert_eq!(at(&dark_pixels, (25., 120.)), DarkPaper::DEFAULT.ink);
        let coverage = report.picture_coverage.unwrap();
        assert!(
            (coverage - 0.125).abs() < 0.01,
            "the signature field's 80 x 40 of 160 x 160: {coverage}"
        );
        // The PDFium oracle (All) draws no widgets: no rectangle stays real.
        let all = RenderRequest {
            annotation_mode: AnnotationRenderMode::All,
            ..dark.clone()
        };
        let (all_pixels, all_report) = render(&mut backend, &mut document, &all);
        assert_eq!(at(&all_pixels, (30., 30.)), DarkPaper::DEFAULT.paper);
        assert_eq!(all_report.picture_coverage, Some(0.));
        backend.close(document);
    }

    /// The picture of dragged content (`RenderPdf`, a one-page PDF on a
    /// transparent ground) follows the view's paper: on Dark pages its ink
    /// is mapped, its pictures stay real, and its ground stays transparent.
    /// Without the paper (Real), it is as before.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn a_dragged_content_picture_follows_the_paper() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let (scale, size) = (3., 600);
        let surface = request(
            (size, size),
            [scale, 0., 0., scale, 0., 0.],
            AnnotationRenderMode::None,
            PaperAppearance::Real,
        )
        .surface;
        let draw = |backend: &mut PdfiumBackend, paper| {
            let mut pixels = vec![0; surface.byte_len as usize];
            backend
                .render_pdf(
                    fixtures::mixed_page(),
                    [scale, 0., 0., scale, 0., 0.],
                    &surface,
                    paper,
                    &mut pixels,
                )
                .unwrap();
            pixels
        };
        let real = draw(&mut backend, PaperAppearance::Real);
        let dark = draw(&mut backend, PaperAppearance::Dark);
        save_png("dragged-real", &real, size, size);
        save_png("dragged-dark", &dark, size, size);
        let map = DarkPaperMap::shared();
        let at = |pixels: &[u8], (x, y): (f32, f32)| {
            let (column, row) = ((x * scale) as usize, ((200. - y) * scale) as usize);
            let at = (row * size as usize + column) * 4;
            [pixels[at + 2], pixels[at + 1], pixels[at], pixels[at + 3]]
        };
        for (name, x, y, picture) in MIXED_POINTS {
            let (real, dark) = (at(&real, (x, y)), at(&dark, (x, y)));
            if real[3] != 255 {
                // The transparent ground and anti-aliased edges.
                assert_eq!(dark[3], real[3], "{name}: alpha stays");
                continue;
            }
            let rgb = [real[0], real[1], real[2]];
            let expected = if picture { rgb } else { map.map(rgb) };
            assert_eq!([dark[0], dark[1], dark[2]], expected, "{name}");
        }
        assert_eq!(at(&dark, (150., 100.))[3], 0, "no ground is added");
    }

    /// Tiles of a slow page cut from one Dark page raster equal Dark tiles
    /// drawn alone, with the same reports.
    #[test]
    #[ignore = "requires the pinned real PDFium library through BP_PDFIUM_LIBRARY"]
    fn dark_tiles_cut_from_a_page_raster_match_tiles_drawn_alone() {
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let mut document = open(&mut backend, &fixtures::mixed_page(), "bp-dark-raster");
        let tile = |backend: &mut PdfiumBackend, document: &mut PdfiumDocument, at: f32| {
            let request = request(
                (100, 100),
                [2., 0., 0., 2., -at, -at],
                AnnotationRenderMode::None,
                PaperAppearance::Dark,
            );
            render(backend, document, &request)
        };
        // Tiles with no text across their edges: PDFium draws glyphs cut by
        // a tile's edge a little differently from the same glyphs in a
        // whole page, in Real renders too.
        let alone = [0., 100.].map(|at| tile(&mut backend, &mut document, at));
        assert!(document.loaded_pages.1.rasters.is_empty(), "a quick page");
        document
            .loaded_pages
            .1
            .heavy
            .push((RasterSource::Display, 0));
        let cut = [0., 100.].map(|at| tile(&mut backend, &mut document, at));
        assert_eq!(document.loaded_pages.1.rasters.len(), 1, "drawn once");
        assert!(document.loaded_pages.1.rasters[0].coverage.is_some());
        for (alone, cut) in alone.iter().zip(&cut) {
            assert!(
                alone.0 == cut.0,
                "tiles cut from the page draw as they do alone"
            );
            let (a, c) = (
                alone.1.picture_coverage.unwrap(),
                cut.1.picture_coverage.unwrap(),
            );
            assert!((a - c).abs() < 1e-6, "coverage {a} vs {c}");
        }
        // A Real tile is not cut from the Dark raster.
        let real = request(
            (100, 100),
            [2., 0., 0., 2., 0., 0.],
            AnnotationRenderMode::None,
            PaperAppearance::Real,
        );
        let (real_pixels, _) = render(&mut backend, &mut document, &real);
        assert!(real_pixels != cut[0].0);
        assert_eq!(document.loaded_pages.1.rasters.len(), 2);
        backend.close(document);
    }

    /// Measures (does not assert) what Dark pages costs on an A1 drawing.
    /// Run with `--nocapture`. Build this crate optimised for numbers that
    /// match a release build.
    #[test]
    #[ignore = "measurement; requires BP_PDFIUM_LIBRARY"]
    fn measure_dark_render_cost_on_a_large_vector_page() {
        use std::time::{Duration, Instant};
        let (_guard, mut backend) = test_backend(Path::new(&library()));
        let started = Instant::now();
        let map = DarkPaperMap::new(DarkPaper::DEFAULT);
        eprintln!("table build: {:?}", started.elapsed());
        drop(map);
        DarkPaperMap::shared();
        let median = |mut times: Vec<Duration>| {
            times.sort();
            times[times.len() / 2]
        };
        for (kind, dense, logo) in [
            ("drawing", false, false),
            ("drawing", false, true),
            ("dense", true, false),
            ("dense", true, true),
        ] {
            let mut document = open(
                &mut backend,
                &fixtures::large_vector_page(30_000, dense, logo),
                "bp-dark-a1",
            );
            // The whole page, 2400 pixels wide, and a 2048 x 2048 tile at
            // 200% zoom.
            let scale = 2_400. / 2_384.;
            let cases = [
                (
                    "page 2400 x 1696",
                    (2_400, 1_696),
                    [scale, 0., 0., scale, 0., 0.],
                ),
                (
                    "tile 2048 x 2048 at 200% (logo corner)",
                    (2_048, 2_048),
                    [2., 0., 0., 2., -2_800., 0.],
                ),
            ];
            for (name, size, transform) in cases {
                for paper in [PaperAppearance::Real, PaperAppearance::Dark] {
                    let request = request(size, transform, AnnotationRenderMode::None, paper);
                    let mut times = Vec::new();
                    for _ in 0..9 {
                        document.loaded_pages.1.rasters.clear();
                        document.loaded_pages.1.heavy.clear();
                        let started = Instant::now();
                        render(&mut backend, &mut document, &request);
                        times.push(started.elapsed());
                    }
                    eprintln!(
                        "{kind}, logo {logo}, {name}, {paper:?}: median {:?} (first {:?})",
                        median(times.clone()),
                        times[0]
                    );
                }
            }
            // The map alone on the page's real pixels.
            let request = request(
                (2_400, 1_696),
                cases[0].2,
                AnnotationRenderMode::None,
                PaperAppearance::Real,
            );
            let (pixels, _) = render(&mut backend, &mut document, &request);
            let mut times = Vec::new();
            for _ in 0..5 {
                let mut copy = pixels.clone();
                let started = Instant::now();
                DarkPaperMap::shared().map_bgra_premultiplied(&mut copy);
                times.push(started.elapsed());
            }
            eprintln!(
                "{kind}, logo {logo}, map alone on 2400 x 1696: median {:?}",
                median(times)
            );
            {
                let page = document.document.pages().get(0).unwrap();
                let started = Instant::now();
                let mut index = super::PictureIndex::of(&page);
                eprintln!(
                    "{kind}, logo {logo}, picture index of {} ink objects: {:?}",
                    index.ink.len(),
                    started.elapsed()
                );
                if logo {
                    let mut times = Vec::new();
                    for _ in 0..9 {
                        let started = Instant::now();
                        super::picture_pass(
                            &page,
                            &mut index.ink,
                            cases[0].2,
                            (2_400, 1_696),
                            false,
                        )
                        .unwrap();
                        times.push(started.elapsed());
                    }
                    eprintln!(
                        "{kind}, logo {logo}, picture pass alone on 2400 x 1696: median {:?}",
                        median(times)
                    );
                }
                drop(index);
                drop(page);
            }
            if logo {
                let mut dark = pixels.clone();
                let request = RenderRequest {
                    paper: PaperAppearance::Dark,
                    ..request
                };
                let (rendered, _) = render(&mut backend, &mut document, &request);
                dark.copy_from_slice(&rendered);
                save_png(&format!("a1-{kind}-real"), &pixels, 2_400, 1_696);
                save_png(&format!("a1-{kind}-dark"), &dark, 2_400, 1_696);
            }
            backend.close(document);
        }
    }
}
