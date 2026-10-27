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
struct Copier<'a> {
    source: &'a Document,
    copied: BTreeMap<ObjectId, ObjectId>,
}

impl Copier<'_> {
    fn copy(&mut self, target: &mut Document, object: &Object, depth: usize) -> Object {
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
}
