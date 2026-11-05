//! A page's content as an earlier file had it. Saving a page whose tables
//! changed rewrites the page around them, so edits made before the save
//! cannot be expressed against the saved file. Undo reaches back past such a
//! save by keeping the page as it was: a one-page PDF of its content
//! streams and resources, which replaces the saved page's before those
//! edits apply.

use std::collections::BTreeMap;
use std::sync::Arc;

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// How deep a page's resources may nest before a copy gives up.
const MAX_DEPTH: usize = 64;
/// The largest page kept: bigger pages are not kept, and undo stops at the
/// save that rewrote them.
pub const MAX_PAGE_BASE_BYTES: usize = 16 * 1024 * 1024;

/// A page as an earlier file had it: a one-page PDF, named by its SHA-256.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PageBase {
    pub page_index: u32,
    /// The PDF's SHA-256, in lowercase hex.
    pub id: String,
    /// The PDF itself. The worker is sent it once, not with every edit.
    #[serde(skip)]
    pub pdf: Option<Arc<Vec<u8>>>,
}

impl PartialEq for PageBase {
    fn eq(&self, other: &Self) -> bool {
        self.page_index == other.page_index && self.id == other.id
    }
}

impl PageBase {
    pub fn new(page_index: u32, pdf: Arc<Vec<u8>>) -> Self {
        Self {
            page_index,
            id: page_base_id(&pdf),
            pdf: Some(pdf),
        }
    }
}

pub fn page_base_id(pdf: &[u8]) -> String {
    Sha256::digest(pdf)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn page_id(document: &Document, page_index: u32) -> Result<ObjectId, String> {
    page_index
        .checked_add(1)
        .and_then(|number| document.get_pages().get(&number).copied())
        .ok_or_else(|| format!("page {page_index} does not exist"))
}

/// The page's own or nearest inherited value of `key`.
fn inherited<'a>(document: &'a Document, page_id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut node_id = page_id;
    for _ in 0..MAX_DEPTH {
        let node = document.get_object(node_id).ok()?.as_dict().ok()?;
        if let Ok(value) = node.get(key) {
            return Some(value);
        }
        node_id = node.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

/// Copies `object` from `source` into `target` with every object it refers
/// to, once each. Pages and the page tree are not followed.
pub(crate) struct Copier<'a> {
    source: &'a Document,
    copied: BTreeMap<ObjectId, ObjectId>,
}

impl<'a> Copier<'a> {
    pub(crate) fn new(source: &'a Document) -> Self {
        Self {
            source,
            copied: BTreeMap::new(),
        }
    }

    pub(crate) fn copy(&mut self, target: &mut Document, object: &Object, depth: usize) -> Object {
        if depth > MAX_DEPTH {
            return Object::Null;
        }
        match object {
            Object::Reference(id) => {
                if let Some(copied) = self.copied.get(id) {
                    return Object::Reference(*copied);
                }
                let Ok(found) = self.source.get_object(*id) else {
                    return Object::Null;
                };
                let page_node = found.as_dict().ok().is_some_and(|dict| {
                    dict.get(b"Type")
                        .and_then(Object::as_name)
                        .is_ok_and(|name| name == b"Page" || name == b"Pages")
                });
                if page_node {
                    return Object::Null;
                }
                let new_id = target.new_object_id();
                self.copied.insert(*id, new_id);
                let copied = self.copy(target, found, depth + 1);
                target.objects.insert(new_id, copied);
                Object::Reference(new_id)
            }
            Object::Array(items) => Object::Array(
                items
                    .iter()
                    .map(|item| self.copy(target, item, depth + 1))
                    .collect(),
            ),
            Object::Dictionary(dict) => Object::Dictionary(self.copy_dict(target, dict, depth)),
            Object::Stream(stream) => Object::Stream(Stream {
                dict: self.copy_dict(target, &stream.dict, depth),
                content: stream.content.clone(),
                allows_compression: stream.allows_compression,
                start_position: None,
            }),
            other => other.clone(),
        }
    }

    fn copy_dict(&mut self, target: &mut Document, dict: &Dictionary, depth: usize) -> Dictionary {
        let mut copied = Dictionary::new();
        for (key, value) in dict.iter() {
            if key.as_slice() == b"Parent" {
                continue;
            }
            copied.set(key.clone(), self.copy(target, value, depth + 1));
        }
        copied
    }
}

/// Page `page_index` of `document` as a one-page PDF: its content streams
/// and resources, in its own boxes and rotation.
pub fn snapshot_page(document: &Document, page_index: u32) -> Result<Vec<u8>, String> {
    let page = page_id(document, page_index)?;
    let mut snapshot = Document::with_version("1.7");
    let pages_id = snapshot.new_object_id();
    let mut copier = Copier {
        source: document,
        copied: BTreeMap::new(),
    };
    let mut page_dict = dictionary! { "Type" => "Page", "Parent" => pages_id };
    for key in [
        b"MediaBox".as_slice(),
        b"CropBox",
        b"Rotate",
        b"Resources",
        b"Contents",
        b"Group",
    ] {
        if let Some(value) = inherited(document, page, key) {
            let value = copier.copy(&mut snapshot, value, 0);
            page_dict.set(key.to_vec(), value);
        }
    }
    let page_ref = snapshot.add_object(page_dict);
    snapshot.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_ref.into()],
            "Count" => 1,
        }),
    );
    let catalog = snapshot.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    snapshot.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    snapshot
        .save_to(&mut bytes)
        .map_err(|error| format!("the page cannot be kept: {error}"))?;
    if bytes.len() > MAX_PAGE_BASE_BYTES {
        return Err("the page is too large to keep".into());
    }
    Ok(bytes)
}

/// Replaces page `page_index`'s content streams and resources with those of
/// the one-page PDF `pdf`; its annotations, boxes and rotation stay.
pub fn restore_page(document: &mut Document, page_index: u32, pdf: &[u8]) -> Result<(), String> {
    let base =
        Document::load_mem(pdf).map_err(|error| format!("a kept page cannot be read: {error}"))?;
    let base_page = page_id(&base, 0)?;
    let page = page_id(document, page_index)?;
    let base_dict = base
        .get_object(base_page)
        .and_then(Object::as_dict)
        .map_err(|error| format!("a kept page cannot be read: {error}"))?
        .clone();
    let mut copier = Copier {
        source: &base,
        copied: BTreeMap::new(),
    };
    let mut values = Vec::new();
    for key in [b"Resources".as_slice(), b"Contents"] {
        let value = base_dict
            .get(key)
            .map_or(Object::Null, |value| copier.copy(document, value, 0));
        values.push((key, value));
    }
    let page_dict = document
        .get_object_mut(page)
        .and_then(Object::as_dict_mut)
        .map_err(|error| format!("page {page_index} cannot be changed: {error}"))?;
    for (key, value) in values {
        if matches!(value, Object::Null) {
            page_dict.remove(key);
        } else {
            page_dict.set(key.to_vec(), value);
        }
    }
    Ok(())
}

/// Restores each base's page in `document`.
pub fn restore_pages(document: &mut Document, bases: &[PageBase]) -> Result<(), String> {
    for base in bases {
        let pdf = base
            .pdf
            .as_ref()
            .ok_or_else(|| format!("the kept page {} is missing", base.id))?;
        restore_page(document, base.page_index, pdf)?;
    }
    Ok(())
}

/// The largest page content a Snapshot copies, decompressed.
const MAX_REGION_CONTENT_BYTES: usize = 64 * 1024 * 1024;
/// The name a region PDF's page gives its Form.
const REGION_FORM_NAME: &[u8] = b"Snapshot";

/// A box of page `page_index` of `document` as a one-page PDF, as Revu's
/// vector Snapshot keeps it: the page's content and resources become a Form
/// whose `/BBox` is `rect` (in the page's user space), drawn on a page the
/// box's size. Nothing outside the box shows; annotations are left out.
pub fn region_pdf(
    document: &Document,
    page_index: u32,
    rect: crate::page_geometry::PdfRect,
) -> Result<Vec<u8>, String> {
    if !(rect.width > 0. && rect.height > 0.) {
        return Err("the Snapshot box is empty".into());
    }
    let page = page_id(document, page_index)?;
    let content = document
        .get_page_content_with_limit(page, MAX_REGION_CONTENT_BYTES)
        .map_err(|_| "the page is too large to copy as vector".to_owned())?;
    let mut region = Document::with_version("1.7");
    let mut copier = Copier {
        source: document,
        copied: BTreeMap::new(),
    };
    let mut form_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "FormType" => 1,
        "BBox" => vec![
            Object::Real(rect.x as f32),
            Object::Real(rect.y as f32),
            Object::Real((rect.x + rect.width) as f32),
            Object::Real((rect.y + rect.height) as f32),
        ],
    };
    for key in [b"Resources".as_slice(), b"Group"] {
        if let Some(value) = inherited(document, page, key) {
            let value = copier.copy(&mut region, value, 0);
            form_dict.set(key.to_vec(), value);
        }
    }
    let mut form = Stream::new(form_dict, content);
    let _ = form.compress();
    let form_id = region.add_object(form);
    let placement = format!("q 1 0 0 1 {:.4} {:.4} cm /Snapshot Do Q", -rect.x, -rect.y);
    finish_region_pdf(region, form_id, &placement, rect.width, rect.height)
}

/// A region PDF turned `rotation_degrees` (a quarter turn) as its page is
/// shown, for other apps, which show its page as it is.
pub fn upright_region_pdf(pdf: &[u8], rotation_degrees: i64) -> Result<Vec<u8>, String> {
    if rotation_degrees.rem_euclid(360) == 0 {
        return Ok(pdf.to_vec());
    }
    let mut region =
        Document::load_mem(pdf).map_err(|error| format!("a Snapshot cannot be read: {error}"))?;
    let page = page_id(&region, 0)?;
    region
        .get_dictionary_mut(page)
        .map_err(|error| format!("a Snapshot cannot be changed: {error}"))?
        .set("Rotate", rotation_degrees.rem_euclid(360));
    let mut bytes = Vec::new();
    region
        .save_to(&mut bytes)
        .map_err(|error| format!("the Snapshot cannot be copied: {error}"))?;
    Ok(bytes)
}

/// The first page of the PDF `pdf` (such as one another app or Butter Paper
/// put on the clipboard) as a region PDF of it, the way up it is shown,
/// with its width and height in points.
pub fn page_region_pdf(pdf: &[u8]) -> Result<(Vec<u8>, f64, f64), String> {
    if pdf.len() > MAX_PAGE_BASE_BYTES {
        return Err("the copied PDF is too large to paste".into());
    }
    let document = Document::load_mem(pdf)
        .map_err(|error| format!("the copied PDF cannot be read: {error}"))?;
    let page = page_id(&document, 0)?;
    let numbers = |key: &[u8]| -> Option<[f64; 4]> {
        let values = inherited(&document, page, key)?;
        let values = document.dereference(values).ok()?.1.as_array().ok()?;
        let values = values
            .iter()
            .map(|value| value.as_float().ok().map(f64::from))
            .collect::<Option<Vec<_>>>()?;
        <[f64; 4]>::try_from(values).ok()
    };
    let [x0, y0, x1, y1] = numbers(b"CropBox")
        .or_else(|| numbers(b"MediaBox"))
        .ok_or("the copied PDF has no page box")?;
    let (x0, x1) = (x0.min(x1), x0.max(x1));
    let (y0, y1) = (y0.min(y1), y0.max(y1));
    if !(x1 - x0 > 0. && y1 - y0 > 0.) {
        return Err("the copied PDF's page is empty".into());
    }
    let rotation = inherited(&document, page, b"Rotate")
        .and_then(|value| value.as_i64().ok())
        .unwrap_or(0)
        .rem_euclid(360);
    let content = document
        .get_page_content_with_limit(page, MAX_REGION_CONTENT_BYTES)
        .map_err(|_| "the copied PDF is too large to paste".to_owned())?;
    let mut region = Document::with_version("1.7");
    let mut copier = Copier::new(&document);
    let mut form_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "FormType" => 1,
        "BBox" => vec![
            Object::Real(x0 as f32),
            Object::Real(y0 as f32),
            Object::Real(x1 as f32),
            Object::Real(y1 as f32),
        ],
    };
    for key in [b"Resources".as_slice(), b"Group"] {
        if let Some(value) = inherited(&document, page, key) {
            let value = copier.copy(&mut region, value, 0);
            form_dict.set(key.to_vec(), value);
        }
    }
    let mut form = Stream::new(form_dict, content);
    let _ = form.compress();
    let form_id = region.add_object(form);
    // The page as a viewer shows it: `/Rotate` turns it clockwise.
    let (width, height, [a, b, c, d, e, f]) = match rotation {
        90 => (y1 - y0, x1 - x0, [0., -1., 1., 0., -y0, x1]),
        180 => (x1 - x0, y1 - y0, [-1., 0., 0., -1., x1, y1]),
        270 => (y1 - y0, x1 - x0, [0., 1., -1., 0., y1, -x0]),
        _ => (x1 - x0, y1 - y0, [1., 0., 0., 1., -x0, -y0]),
    };
    let placement = format!("q {a:.4} {b:.4} {c:.4} {d:.4} {e:.4} {f:.4} cm /Snapshot Do Q");
    let region = finish_region_pdf(region, form_id, &placement, width, height)?;
    Ok((region, width, height))
}

/// A region PDF of `rect` with nothing on it, for markups to be drawn into
/// when the page itself is copied as a picture.
pub fn blank_region_pdf(rect: crate::page_geometry::PdfRect) -> Result<Vec<u8>, String> {
    let mut region = Document::with_version("1.7");
    let form = region.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "FormType" => 1,
            "BBox" => vec![
                Object::Real(rect.x as f32),
                Object::Real(rect.y as f32),
                Object::Real((rect.x + rect.width) as f32),
                Object::Real((rect.y + rect.height) as f32),
            ],
            "Resources" => Dictionary::new(),
        },
        Vec::new(),
    ));
    let placement = format!("q 1 0 0 1 {:.4} {:.4} cm /Snapshot Do Q", -rect.x, -rect.y);
    finish_region_pdf(region, form, &placement, rect.width, rect.height)
}

/// A Form of `source` (such as an imported Revu Snapshot's appearance)
/// drawn by `placement` (a content stream naming it `/Snapshot`) on a
/// `width` × `height` page: the same one-page PDF `region_pdf` makes.
pub fn form_region_pdf(
    source: &Document,
    form_id: ObjectId,
    placement: &str,
    width: f64,
    height: f64,
) -> Result<Vec<u8>, String> {
    let mut region = Document::with_version("1.7");
    let mut copier = Copier {
        source,
        copied: BTreeMap::new(),
    };
    let Object::Reference(form_id) = copier.copy(&mut region, &Object::Reference(form_id), 0)
    else {
        return Err("the Snapshot's Form cannot be copied".into());
    };
    finish_region_pdf(region, form_id, placement, width, height)
}

fn finish_region_pdf(
    mut region: Document,
    form_id: ObjectId,
    placement: &str,
    width: f64,
    height: f64,
) -> Result<Vec<u8>, String> {
    let pages_id = region.new_object_id();
    let contents = region.add_object(Stream::new(
        Dictionary::new(),
        placement.as_bytes().to_vec(),
    ));
    let page = region.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![
            0.into(),
            0.into(),
            Object::Real(width as f32),
            Object::Real(height as f32),
        ],
        "Resources" => dictionary! {
            "XObject" => dictionary! { REGION_FORM_NAME.to_vec() => form_id },
        },
        "Contents" => contents,
    });
    region.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page.into()],
            "Count" => 1,
        }),
    );
    let catalog = region.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    region.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    region
        .save_to(&mut bytes)
        .map_err(|error| format!("the Snapshot cannot be copied: {error}"))?;
    if bytes.len() > MAX_PAGE_BASE_BYTES {
        return Err("the Snapshot is too large to copy as vector".into());
    }
    Ok(bytes)
}

/// The Form a region PDF's page draws, and the PDF.
pub(crate) fn region_form(pdf: &[u8]) -> Result<(Document, ObjectId), String> {
    let region =
        Document::load_mem(pdf).map_err(|error| format!("a Snapshot cannot be read: {error}"))?;
    let page = page_id(&region, 0)?;
    let form = region
        .get_dictionary(page)
        .and_then(|page| page.get(b"Resources"))
        .and_then(|resources| region.dereference(resources))
        .and_then(|(_, resources)| resources.as_dict())
        .and_then(|resources| resources.get(b"XObject"))
        .and_then(|xobjects| region.dereference(xobjects))
        .and_then(|(_, xobjects)| xobjects.as_dict())
        .and_then(|xobjects| xobjects.get(REGION_FORM_NAME))
        .and_then(Object::as_reference)
        .map_err(|_| "a Snapshot has no Form".to_owned())?;
    Ok((region, form))
}

/// Points the region PDF's page at `form` in place of its Form, and saves it.
pub(crate) fn replace_region_form(mut region: Document, form: ObjectId) -> Result<Vec<u8>, String> {
    let page = page_id(&region, 0)?;
    let page_dict = region
        .get_dictionary_mut(page)
        .map_err(|error| format!("a Snapshot cannot be changed: {error}"))?;
    page_dict.set(
        "Resources",
        dictionary! { "XObject" => dictionary! { REGION_FORM_NAME.to_vec() => form } },
    );
    let mut bytes = Vec::new();
    region
        .save_to(&mut bytes)
        .map_err(|error| format!("the Snapshot cannot be copied: {error}"))?;
    if bytes.len() > MAX_PAGE_BASE_BYTES {
        return Err("the Snapshot is too large to copy as vector".into());
    }
    Ok(bytes)
}

/// Copies the Form of the region PDF `pdf` (made by `region_pdf` or
/// `form_region_pdf`) into `target`, returning its new object.
pub fn import_region_form(target: &mut Document, pdf: &[u8]) -> Result<ObjectId, String> {
    let region =
        Document::load_mem(pdf).map_err(|error| format!("a Snapshot cannot be read: {error}"))?;
    let page = page_id(&region, 0)?;
    let form = region
        .get_dictionary(page)
        .and_then(|page| page.get(b"Resources"))
        .and_then(|resources| region.dereference(resources))
        .and_then(|(_, resources)| resources.as_dict())
        .and_then(|resources| resources.get(b"XObject"))
        .and_then(|xobjects| region.dereference(xobjects))
        .and_then(|(_, xobjects)| xobjects.as_dict())
        .and_then(|xobjects| xobjects.get(REGION_FORM_NAME))
        .map_err(|_| "a Snapshot has no Form".to_owned())?
        .clone();
    if !matches!(form, Object::Reference(_)) {
        return Err("a Snapshot has no Form".into());
    }
    let mut copier = Copier {
        source: &region,
        copied: BTreeMap::new(),
    };
    match copier.copy(target, &form, 0) {
        Object::Reference(id) => Ok(id),
        _ => Err("a Snapshot's Form cannot be copied".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let font = document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = document.add_object(Stream::new(
            dictionary! {},
            format!("BT /F1 12 Tf 10 10 Td ({text}) Tj ET").into_bytes(),
        ));
        let page = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content,
        });
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page.into()],
                "Count" => 1,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        document.trailer.set("Root", catalog);
        document
    }

    fn page_text(document: &Document) -> String {
        crate::pdf_text_edit::extract_page_text(
            document,
            0,
            &crate::pdf_text_edit::TextEditLimits::default(),
        )
        .unwrap()
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect()
    }

    #[test]
    fn a_kept_page_restores_its_content_into_a_later_file() {
        let earlier = document("Before");
        let pdf = snapshot_page(&earlier, 0).unwrap();
        let mut later = document("After");
        assert_eq!(page_text(&later), "After");
        restore_page(&mut later, 0, &pdf).unwrap();
        assert_eq!(page_text(&later), "Before");
        // Written and read again, it is still the earlier page.
        let mut bytes = Vec::new();
        later.save_to(&mut bytes).unwrap();
        assert_eq!(page_text(&Document::load_mem(&bytes).unwrap()), "Before");
        assert_eq!(page_base_id(&pdf).len(), 64);
    }

    #[test]
    fn a_copied_page_becomes_a_region_the_way_up_it_is_shown() {
        let source = document("Turned");
        let pages = source.get_pages();
        let mut turned = source.clone();
        turned
            .get_dictionary_mut(pages[&1])
            .unwrap()
            .set("Rotate", 90);
        let mut bytes = Vec::new();
        turned.save_to(&mut bytes).unwrap();

        let (region, width, height) = page_region_pdf(&bytes).unwrap();
        // The 200 × 100 page, turned a quarter, shows 100 wide and 200 high.
        assert_eq!((width, height), (100., 200.));
        let region = Document::load_mem(&region).unwrap();
        let page = page_id(&region, 0).unwrap();
        assert_eq!(
            String::from_utf8(region.get_page_content(page))
                .unwrap()
                .trim(),
            "q 0.0000 -1.0000 1.0000 0.0000 -0.0000 200.0000 cm /Snapshot Do Q"
        );
        let mut target = document("Elsewhere");
        let form = import_region_form(&mut target, &region_bytes(&region)).unwrap();
        let form = target.get_object(form).unwrap().as_stream().unwrap();
        assert!(
            String::from_utf8(form.decompressed_content().unwrap())
                .unwrap()
                .contains("(Turned) Tj")
        );
        assert!(page_region_pdf(b"not a PDF").is_err());
    }

    fn region_bytes(region: &Document) -> Vec<u8> {
        let mut bytes = Vec::new();
        region.clone().save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn a_region_keeps_its_box_of_the_page_as_a_form_that_imports_elsewhere() {
        let source = document("Inside");
        let rect = crate::page_geometry::PdfRect::new(5., 5., 60., 20.).unwrap();
        let pdf = region_pdf(&source, 0, rect).unwrap();
        let region = Document::load_mem(&pdf).unwrap();
        let page = page_id(&region, 0).unwrap();
        let page_dict = region.get_dictionary(page).unwrap();
        let media_box: Vec<f32> = page_dict
            .get(b"MediaBox")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_float().unwrap())
            .collect();
        assert_eq!(media_box, [0., 0., 60., 20.]);
        assert_eq!(
            String::from_utf8(region.get_page_content(page))
                .unwrap()
                .trim(),
            "q 1 0 0 1 -5.0000 -5.0000 cm /Snapshot Do Q"
        );

        let mut target = document("Elsewhere");
        let form_id = import_region_form(&mut target, &pdf).unwrap();
        let form = target.get_object(form_id).unwrap().as_stream().unwrap();
        assert_eq!(
            form.dict.get(b"Subtype").unwrap().as_name().unwrap(),
            b"Form"
        );
        let bbox: Vec<f32> = form
            .dict
            .get(b"BBox")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_float().unwrap())
            .collect();
        assert_eq!(bbox, [5., 5., 65., 25.]);
        assert!(
            String::from_utf8(form.decompressed_content().unwrap())
                .unwrap()
                .contains("(Inside) Tj")
        );
        // Its font came with it.
        let font = form
            .dict
            .get(b"Resources")
            .and_then(Object::as_dict)
            .and_then(|resources| resources.get(b"Font"))
            .and_then(Object::as_dict)
            .and_then(|fonts| fonts.get(b"F1"))
            .and_then(Object::as_reference)
            .unwrap();
        assert_eq!(
            target
                .get_dictionary(font)
                .unwrap()
                .get(b"BaseFont")
                .unwrap()
                .as_name()
                .unwrap(),
            b"Helvetica"
        );
    }
}
