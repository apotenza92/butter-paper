//! A document's pages as the user arranged them: which page of the file
//! each shown page is, a copy of one, or a new blank page. Renders and
//! saves rebuild the file's page tree in that order
//! ([`restructure_pages`]); everything else names pages by where they are
//! shown.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde::{Deserialize, Serialize};

/// One shown page.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PageSlot {
    /// Page `index` of the file.
    Source { index: u32 },
    /// Another copy of page `index` of the file: its content, without the
    /// markups the app edits (the app adds copies of those) or form fields.
    Copy { index: u32 },
    /// A new blank page, in points.
    Blank { width: f64, height: f64 },
}

impl PageSlot {
    /// The file page this page shows, if any.
    pub fn file_page(self) -> Option<u32> {
        match self {
            Self::Source { index } | Self::Copy { index } => Some(index),
            Self::Blank { .. } => None,
        }
    }

    /// This page copied again.
    pub fn copied(self) -> Self {
        match self {
            Self::Source { index } | Self::Copy { index } => Self::Copy { index },
            blank => blank,
        }
    }
}

/// Whether `slots` shows a file's `page_count` pages as the file has them.
pub fn is_unchanged(slots: &[PageSlot], page_count: usize) -> bool {
    slots.is_empty()
        || (slots.len() == page_count
            && slots.iter().enumerate().all(|(index, slot)| {
                *slot
                    == PageSlot::Source {
                        index: index as u32,
                    }
            }))
}

/// The file's pages in order, as `slots` names none of them twice as a
/// source.
pub fn validate(slots: &[PageSlot], page_count: usize) -> Result<(), String> {
    if slots.is_empty() {
        return Ok(());
    }
    let mut sources = BTreeSet::new();
    for slot in slots {
        match *slot {
            PageSlot::Source { index } | PageSlot::Copy { index } => {
                if index as usize >= page_count {
                    return Err(format!("page {} is not in the file", index + 1));
                }
                if matches!(slot, PageSlot::Source { .. }) && !sources.insert(index) {
                    return Err(format!("page {} is shown twice", index + 1));
                }
            }
            PageSlot::Blank { width, height } => {
                if !(width.is_finite() && height.is_finite() && width >= 3. && height >= 3.) {
                    return Err("a new page has no size".into());
                }
            }
        }
    }
    Ok(())
}

/// Page attributes a page may take from its parents.
const INHERITED: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];

/// Rebuilds `document`'s page tree as `slots` shows it. Pages left out are
/// removed with their form fields, and links to them are cleared; a save
/// then drops them from the file. `managed` names each page's `/Annots`
/// slots of markups the app edits ([`crate::pdf_engine::managed_annotation_slots`]),
/// which copies leave out.
pub fn restructure_pages(
    document: &mut Document,
    slots: &[PageSlot],
    managed: &BTreeMap<ObjectId, HashSet<usize>>,
) -> Result<(), String> {
    let pages = document.get_pages();
    let page_ids = pages.values().copied().collect::<Vec<_>>();
    if is_unchanged(slots, page_ids.len()) {
        return Ok(());
    }
    validate(slots, page_ids.len())?;
    let root = document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|error| format!("the page tree cannot be read: {error}"))?;
    // Each page takes its inherited attributes along, as it leaves its
    // parent for the root.
    take_inherited_attributes(document, &page_ids)?;
    let mut kids = Vec::with_capacity(slots.len());
    for slot in slots {
        let id = match *slot {
            PageSlot::Source { index } => page_ids[index as usize],
            PageSlot::Copy { index } => {
                let source = page_ids[index as usize];
                copy_page(document, source, managed.get(&source))?
            }
            PageSlot::Blank { width, height } => document.add_object(dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), Object::Real(width as f32), Object::Real(height as f32)],
                "Resources" => dictionary! {},
            }),
        };
        page_dictionary_mut(document, id)?.set("Parent", root);
        kids.push(Object::Reference(id));
    }
    let kept = slots
        .iter()
        .filter_map(|slot| match *slot {
            PageSlot::Source { index } => Some(page_ids[index as usize]),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let removed = page_ids
        .iter()
        .copied()
        .filter(|id| !kept.contains(id))
        .collect::<BTreeSet<_>>();
    let root_pages = document
        .get_object_mut(root)
        .and_then(Object::as_dict_mut)
        .map_err(|error| format!("the page tree cannot be read: {error}"))?;
    let count = kids.len() as i64;
    root_pages.set("Kids", Object::Array(kids));
    root_pages.set("Count", count);
    root_pages.remove(b"Parent");
    if !removed.is_empty() {
        remove_form_widgets_of(document, &removed);
        clear_references_to(document, &removed);
    }
    Ok(())
}

/// Gives each of `pages` the attributes it inherits from its parents, so it
/// can move to another parent unchanged.
pub(crate) fn take_inherited_attributes(
    document: &mut Document,
    pages: &[ObjectId],
) -> Result<(), String> {
    for &page_id in pages {
        let inherited = INHERITED
            .iter()
            .filter_map(|key| {
                inherited_attribute(document, page_id, key).map(|value| (*key, value))
            })
            .collect::<Vec<_>>();
        let page = page_dictionary_mut(document, page_id)?;
        for (key, value) in inherited {
            if !page.has(key) {
                page.set(key, value);
            }
        }
    }
    Ok(())
}

fn page_dictionary_mut(document: &mut Document, id: ObjectId) -> Result<&mut Dictionary, String> {
    document
        .get_object_mut(id)
        .and_then(Object::as_dict_mut)
        .map_err(|error| format!("a page cannot be read: {error}"))
}

/// `key` of page `id` or the nearest parent that has it.
fn inherited_attribute(document: &Document, id: ObjectId, key: &[u8]) -> Option<Object> {
    let mut node = document.get_dictionary(id).ok()?;
    for _ in 0..64 {
        if let Ok(value) = node.get(key) {
            return Some(value.clone());
        }
        let parent = node.get(b"Parent").and_then(Object::as_reference).ok()?;
        node = document.get_dictionary(parent).ok()?;
    }
    None
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap_or(object),
        _ => object,
    }
}

/// A new page with page `source`'s content: its content streams and the
/// top of its resources are its own, so editing one copy's text leaves
/// the other's alone. Fonts and images are shared.
fn copy_page(
    document: &mut Document,
    source: ObjectId,
    managed: Option<&HashSet<usize>>,
) -> Result<ObjectId, String> {
    let mut page = document
        .get_dictionary(source)
        .map_err(|error| format!("a page cannot be read: {error}"))?
        .clone();
    for key in [&b"StructParents"[..], b"B", b"Annots", b"Parent"] {
        page.remove(key);
    }
    if let Ok(contents) = page.get(b"Contents").cloned() {
        let copied = match resolve(document, &contents).clone() {
            Object::Array(parts) => Object::Array(
                parts
                    .iter()
                    .map(|part| copy_stream(document, part))
                    .collect(),
            ),
            _ => copy_stream(document, &contents),
        };
        page.set("Contents", copied);
    }
    if let Ok(resources) = page.get(b"Resources")
        && let Ok(resources) = resolve(document, resources).as_dict()
    {
        let mut own = Dictionary::new();
        for (key, value) in resources.iter() {
            let value = match resolve(document, value) {
                Object::Dictionary(inner) => Object::Dictionary(inner.clone()),
                _ => value.clone(),
            };
            own.set(key.clone(), value);
        }
        page.set("Resources", own);
    }
    let annotations = source_annotations(document, source);
    let page_id = document.add_object(page);
    // Leave out the app's markups and form fields, with the popups of
    // anything left out.
    let left_out = annotations
        .iter()
        .enumerate()
        .filter(|(slot, annotation)| {
            managed.is_some_and(|managed| managed.contains(slot))
                || annotation_subtype(document, annotation).as_deref() == Some("Widget")
        })
        .filter_map(|(_, annotation)| annotation.as_reference().ok())
        .collect::<BTreeSet<_>>();
    let mut copies = BTreeMap::new();
    let mut kept = Vec::new();
    for (slot, annotation) in annotations.iter().enumerate() {
        if managed.is_some_and(|managed| managed.contains(&slot))
            || annotation_subtype(document, annotation).as_deref() == Some("Widget")
        {
            continue;
        }
        let Ok(mut dictionary) = resolve(document, annotation).as_dict().cloned() else {
            continue;
        };
        if dictionary
            .get(b"Parent")
            .and_then(Object::as_reference)
            .is_ok_and(|parent| left_out.contains(&parent))
        {
            continue;
        }
        dictionary.remove(b"StructParent");
        dictionary.set("P", page_id);
        let id = document.add_object(dictionary);
        if let Ok(original) = annotation.as_reference() {
            copies.insert(original, id);
        }
        kept.push(id);
    }
    // Popups and their parents point at each other's copies.
    for &id in &kept {
        let Ok(dictionary) = document.get_object_mut(id).and_then(Object::as_dict_mut) else {
            continue;
        };
        for key in [&b"Popup"[..], b"Parent", b"IRT"] {
            if let Ok(target) = dictionary.get(key).and_then(Object::as_reference)
                && let Some(copy) = copies.get(&target)
            {
                dictionary.set(key, *copy);
            }
        }
    }
    if !kept.is_empty() {
        page_dictionary_mut(document, page_id)?.set(
            "Annots",
            Object::Array(kept.into_iter().map(Object::Reference).collect()),
        );
    }
    Ok(page_id)
}

fn copy_stream(document: &mut Document, object: &Object) -> Object {
    match resolve(document, object) {
        Object::Stream(stream) => {
            let stream: Stream = stream.clone();
            Object::Reference(document.add_object(stream))
        }
        _ => object.clone(),
    }
}

fn source_annotations(document: &Document, page: ObjectId) -> Vec<Object> {
    document
        .get_dictionary(page)
        .ok()
        .and_then(|page| page.get(b"Annots").ok())
        .and_then(|annotations| resolve(document, annotations).as_array().ok())
        .cloned()
        .unwrap_or_default()
}

fn annotation_subtype(document: &Document, annotation: &Object) -> Option<String> {
    let dictionary = resolve(document, annotation).as_dict().ok()?;
    let name = dictionary.get(b"Subtype").ok()?.as_name().ok()?;
    Some(String::from_utf8_lossy(name).into_owned())
}

/// Removes the form fields whose widgets are all on `pages`.
fn remove_form_widgets_of(document: &mut Document, pages: &BTreeSet<ObjectId>) {
    let widgets = pages
        .iter()
        .flat_map(|page| source_annotations(document, *page))
        .filter_map(|annotation| annotation.as_reference().ok())
        .collect::<BTreeSet<_>>();
    if widgets.is_empty() {
        return;
    }
    let Some(form) = document
        .catalog()
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok())
        .cloned()
    else {
        return;
    };
    let fields = match &form {
        Object::Reference(id) => document
            .get_dictionary(*id)
            .ok()
            .and_then(|form| form.get(b"Fields").ok())
            .cloned(),
        Object::Dictionary(form) => form.get(b"Fields").ok().cloned(),
        _ => None,
    };
    let Some(fields) = fields else {
        return;
    };
    let kept = prune_fields(document, &fields, &widgets, 0);
    let kept = Object::Array(kept);
    match form {
        Object::Reference(id) => {
            if let Ok(form) = document.get_object_mut(id).and_then(Object::as_dict_mut) {
                form.set("Fields", kept);
            }
        }
        Object::Dictionary(mut form) => {
            form.set("Fields", kept);
            if let Ok(catalog) = document.catalog_mut() {
                catalog.set("AcroForm", form);
            }
        }
        _ => {}
    }
}

/// `fields` without `widgets`, and without fields left with no widgets.
fn prune_fields(
    document: &mut Document,
    fields: &Object,
    widgets: &BTreeSet<ObjectId>,
    depth: usize,
) -> Vec<Object> {
    let Ok(list) = resolve(document, fields).as_array().cloned() else {
        return Vec::new();
    };
    let mut kept = Vec::new();
    for field in list {
        let Ok(id) = field.as_reference() else {
            kept.push(field);
            continue;
        };
        if widgets.contains(&id) {
            continue;
        }
        let kids = document
            .get_dictionary(id)
            .ok()
            .and_then(|field| field.get(b"Kids").ok())
            .cloned();
        if let Some(kids) = kids
            && depth < 32
        {
            let remaining = prune_fields(document, &kids, widgets, depth + 1);
            if remaining.is_empty() {
                continue;
            }
            if let Ok(field) = document.get_object_mut(id).and_then(Object::as_dict_mut) {
                field.set("Kids", Object::Array(remaining));
            }
        }
        kept.push(field);
    }
    kept
}

/// Replaces every reference to `removed` with null: outline entries and
/// links to a removed page lead nowhere rather than keep it in the file.
fn clear_references_to(document: &mut Document, removed: &BTreeSet<ObjectId>) {
    fn clear(object: &mut Object, removed: &BTreeSet<ObjectId>) {
        match object {
            Object::Reference(id) if removed.contains(id) => *object = Object::Null,
            Object::Array(items) => items.iter_mut().for_each(|item| clear(item, removed)),
            Object::Dictionary(dictionary) => dictionary
                .iter_mut()
                .for_each(|(_, value)| clear(value, removed)),
            Object::Stream(stream) => stream
                .dict
                .iter_mut()
                .for_each(|(_, value)| clear(value, removed)),
            _ => {}
        }
    }
    for (id, object) in document.objects.iter_mut() {
        if removed.contains(id) {
            continue;
        }
        clear(object, removed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file of pages saying "A", "B", ... under an intermediate node, with
    /// a markup and a link on the first page.
    fn document(texts: &[&str]) -> Document {
        let mut document = Document::with_version("1.7");
        let root = document.new_object_id();
        let middle = document.new_object_id();
        let mut kids = Vec::new();
        for text in texts {
            let content = document.add_object(Stream::new(
                dictionary! {},
                format!("BT /F1 12 Tf 10 10 Td ({text}) Tj ET").into_bytes(),
            ));
            kids.push(Object::Reference(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => middle,
                "Contents" => content,
            })));
        }
        let count = kids.len() as i64;
        document.objects.insert(
            middle,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Parent" => root, "Kids" => kids, "Count" => count,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
                "Resources" => dictionary! { "Font" => dictionary! {} },
            }),
        );
        document.objects.insert(
            root,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![middle.into()], "Count" => count,
            }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => root });
        document.trailer.set("Root", catalog);
        document
    }

    fn texts(document: &Document) -> Vec<String> {
        document
            .get_pages()
            .values()
            .map(|page| {
                let content = document.get_page_content(*page);
                String::from_utf8_lossy(&content).into_owned()
            })
            .collect()
    }

    fn reloaded(mut document: Document) -> Document {
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        Document::load_mem(&bytes).unwrap()
    }

    #[test]
    fn pages_are_shown_reordered_copied_added_and_removed() {
        let mut file = document(&["A", "B", "C"]);
        restructure_pages(
            &mut file,
            &[
                PageSlot::Source { index: 2 },
                PageSlot::Copy { index: 0 },
                PageSlot::Blank {
                    width: 300.,
                    height: 400.,
                },
                PageSlot::Source { index: 0 },
            ],
            &BTreeMap::new(),
        )
        .unwrap();
        let file = reloaded(file);
        let shown = texts(&file);
        assert_eq!(shown.len(), 4);
        assert!(shown[0].contains("(C)"));
        assert!(shown[1].contains("(A)"));
        assert_eq!(shown[2], "");
        assert!(shown[3].contains("(A)"));
        let pages = file.get_pages();
        // Inherited attributes travel with each page.
        for page in pages.values() {
            let page = file.get_dictionary(*page).unwrap();
            assert!(page.has(b"MediaBox") && page.has(b"Resources"));
        }
        // A copy has its own content stream, so editing one leaves the
        // other.
        let contents = |index: u32| {
            file.get_dictionary(pages[&index])
                .unwrap()
                .get(b"Contents")
                .unwrap()
                .clone()
        };
        assert_ne!(contents(2), contents(4));
    }

    #[test]
    fn a_removed_page_leaves_no_links_or_fields() {
        let mut file = document(&["A", "B"]);
        let removed = file.get_pages()[&2];
        let widget = file.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx", "T" => "name", "P" => removed,
        });
        file.get_object_mut(removed)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set("Annots", vec![widget.into()]);
        let outline = file.add_object(dictionary! {
            "Title" => "B", "Dest" => vec![removed.into(), "Fit".into()],
        });
        let catalog = file.catalog_mut().unwrap();
        catalog.set("AcroForm", dictionary! { "Fields" => vec![widget.into()] });
        catalog.set("Outlines", outline);
        restructure_pages(
            &mut file,
            &[PageSlot::Source { index: 0 }],
            &BTreeMap::new(),
        )
        .unwrap();
        let fields = file
            .catalog()
            .unwrap()
            .get(b"AcroForm")
            .unwrap()
            .as_dict()
            .unwrap()
            .get(b"Fields")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        assert!(fields.is_empty());
        let destination = file
            .get_dictionary(outline)
            .unwrap()
            .get(b"Dest")
            .unwrap()
            .as_array()
            .unwrap()
            .clone();
        assert_eq!(destination[0], Object::Null);
        let mut file = file;
        file.prune_objects();
        assert!(
            file.get_object(removed).is_err(),
            "the page leaves the file"
        );
        assert_eq!(texts(&reloaded(file)).len(), 1);
    }

    #[test]
    fn a_copy_leaves_out_the_apps_markups_and_fields() {
        let mut file = document(&["A"]);
        let page = file.get_pages()[&1];
        let managed = file.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Square" });
        let popup = file.add_object(
            dictionary! { "Type" => "Annot", "Subtype" => "Popup", "Parent" => managed },
        );
        let widget = file.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Widget" });
        let link = file.add_object(dictionary! { "Type" => "Annot", "Subtype" => "Link" });
        file.get_object_mut(page)
            .unwrap()
            .as_dict_mut()
            .unwrap()
            .set(
                "Annots",
                vec![managed.into(), popup.into(), widget.into(), link.into()],
            );
        let slots = BTreeMap::from([(page, HashSet::from([0]))]);
        restructure_pages(
            &mut file,
            &[PageSlot::Source { index: 0 }, PageSlot::Copy { index: 0 }],
            &slots,
        )
        .unwrap();
        let pages = file.get_pages();
        let annotations = |page: ObjectId| {
            file.get_dictionary(page)
                .unwrap()
                .get(b"Annots")
                .unwrap()
                .as_array()
                .unwrap()
                .clone()
        };
        assert_eq!(annotations(pages[&1]).len(), 4, "the page itself keeps all");
        let copied = annotations(pages[&2]);
        assert_eq!(copied.len(), 1);
        let copy = file
            .get_dictionary(copied[0].as_reference().unwrap())
            .unwrap();
        assert_eq!(copy.get(b"Subtype").unwrap().as_name().unwrap(), b"Link");
        assert_eq!(copy.get(b"P").unwrap().as_reference().unwrap(), pages[&2]);
    }

    #[test]
    fn slots_must_name_each_file_page_once() {
        assert!(
            validate(
                &[PageSlot::Source { index: 0 }, PageSlot::Source { index: 0 }],
                1
            )
            .is_err()
        );
        assert!(validate(&[PageSlot::Copy { index: 3 }], 1).is_err());
        assert!(validate(&[PageSlot::Copy { index: 0 }], 1).is_ok());
        assert!(is_unchanged(&[], 4));
        assert!(is_unchanged(&[PageSlot::Source { index: 0 }], 1));
        assert!(!is_unchanged(&[PageSlot::Copy { index: 0 }], 1));
    }
}
