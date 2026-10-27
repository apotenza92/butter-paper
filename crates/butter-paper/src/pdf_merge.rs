//! Merging PDFs into one new PDF: each file's pages in order, with their
//! markups and form fields. The first file's document settings (outline,
//! metadata) stay; the others' pages join it.

use std::collections::{BTreeSet, HashSet};

use lopdf::{Dictionary, Document, Object, ObjectId};

/// The PDFs in `sources`, in order, merged into one.
pub fn merge_pdfs(sources: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let mut sources = sources.iter();
    let first = sources.next().ok_or("there are no documents to merge")?;
    let mut merged = load(first, 1)?;
    let root = pages_root(&merged)?;
    let first_pages = merged.get_pages().values().copied().collect::<Vec<_>>();
    crate::pdf_page_structure::take_inherited_attributes(&mut merged, &first_pages)?;
    let mut names = annotation_names(&merged);
    let mut field_names = top_field_names(&merged);
    let mut kids = first_pages
        .iter()
        .map(|id| Object::Reference(*id))
        .collect::<Vec<_>>();
    for (offset, bytes) in sources.enumerate() {
        let number = offset + 2;
        let mut document = load(bytes, number)?;
        document.renumber_objects_with(merged.max_id + 1);
        let pages = document.get_pages().values().copied().collect::<Vec<_>>();
        crate::pdf_page_structure::take_inherited_attributes(&mut document, &pages)?;
        // Markups are told apart by name: a file sharing any name with
        // the files before takes its own.
        let own = annotation_names(&document);
        if !own.is_disjoint(&names) {
            rename_annotations(&mut document, &format!("merged-{number}:"));
        }
        names.extend(annotation_names(&document));
        let fields = form_fields(&document, number, &mut field_names);
        merged.max_id = document.max_id.max(merged.max_id);
        merged.objects.extend(std::mem::take(&mut document.objects));
        for page in &pages {
            if let Ok(page) = merged.get_object_mut(*page).and_then(Object::as_dict_mut) {
                page.set("Parent", root);
            }
            kids.push(Object::Reference(*page));
        }
        if !fields.fields.is_empty() {
            add_form_fields(&mut merged, fields)?;
        }
    }
    let count = kids.len() as i64;
    let pages = merged
        .get_object_mut(root)
        .and_then(Object::as_dict_mut)
        .map_err(|error| format!("the page tree cannot be read: {error}"))?;
    pages.set("Kids", Object::Array(kids));
    pages.set("Count", count);
    for page in &first_pages {
        if let Ok(page) = merged.get_object_mut(*page).and_then(Object::as_dict_mut) {
            page.set("Parent", root);
        }
    }
    merged.prune_objects();
    let mut bytes = Vec::new();
    merged
        .save_to(&mut bytes)
        .map_err(|error| format!("the merged PDF cannot be written: {error}"))?;
    Ok(bytes)
}

fn load(bytes: &[u8], number: usize) -> Result<Document, String> {
    let document = Document::load_mem(bytes)
        .map_err(|error| format!("document {number} cannot be read: {error}"))?;
    // Opened with an empty password or not, a protected file's
    // restrictions do not carry over, so it is not merged.
    if document.is_encrypted() || document.encryption_state.is_some() {
        return Err(format!(
            "document {number} is password protected, so it cannot be merged"
        ));
    }
    Ok(document)
}

fn pages_root(document: &Document) -> Result<ObjectId, String> {
    document
        .catalog()
        .and_then(|catalog| catalog.get(b"Pages"))
        .and_then(Object::as_reference)
        .map_err(|error| format!("the page tree cannot be read: {error}"))
}

fn page_annotations(document: &Document) -> Vec<ObjectId> {
    document
        .get_pages()
        .values()
        .filter_map(|page| document.get_dictionary(*page).ok())
        .filter_map(|page| page.get(b"Annots").ok())
        .filter_map(|annotations| match annotations {
            Object::Reference(id) => document.get_object(*id).ok()?.as_array().ok().cloned(),
            Object::Array(items) => Some(items.clone()),
            _ => None,
        })
        .flatten()
        .filter_map(|annotation| annotation.as_reference().ok())
        .collect()
}

fn annotation_names(document: &Document) -> HashSet<Vec<u8>> {
    page_annotations(document)
        .into_iter()
        .filter_map(|id| document.get_dictionary(id).ok())
        .filter_map(|annotation| annotation.get(b"NM").ok())
        .filter_map(|name| name.as_str().ok().map(<[u8]>::to_vec))
        .collect()
}

fn rename_annotations(document: &mut Document, prefix: &str) {
    for id in page_annotations(document) {
        let Ok(annotation) = document.get_object_mut(id).and_then(Object::as_dict_mut) else {
            continue;
        };
        if let Ok(name) = annotation.get(b"NM").and_then(Object::as_str) {
            let mut renamed = prefix.as_bytes().to_vec();
            renamed.extend_from_slice(name);
            annotation.set("NM", Object::string_literal(renamed));
        }
    }
}

fn form_dictionary(document: &Document) -> Option<Dictionary> {
    let form = document.catalog().ok()?.get(b"AcroForm").ok()?;
    match form {
        Object::Reference(id) => document.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(form) => Some(form.clone()),
        _ => None,
    }
}

fn top_field_names(document: &Document) -> BTreeSet<Vec<u8>> {
    let Some(fields) = form_dictionary(document)
        .and_then(|form| form.get(b"Fields").ok().cloned())
        .and_then(|fields| match fields {
            Object::Reference(id) => document.get_object(id).ok()?.as_array().ok().cloned(),
            Object::Array(items) => Some(items),
            _ => None,
        })
    else {
        return BTreeSet::new();
    };
    fields
        .iter()
        .filter_map(|field| field.as_reference().ok())
        .filter_map(|id| document.get_dictionary(id).ok())
        .filter_map(|field| field.get(b"T").ok()?.as_str().ok().map(<[u8]>::to_vec))
        .collect()
}

/// A merged file's form fields, the resources their text uses, and the
/// fields renamed so no two share a name.
#[derive(Default)]
struct FormFields {
    fields: Vec<Object>,
    resources: Option<Dictionary>,
    renames: Vec<(ObjectId, Vec<u8>)>,
}

/// `document`'s form fields, renamed where a field before has the same
/// name: fields of one name share one value.
fn form_fields(document: &Document, number: usize, taken: &mut BTreeSet<Vec<u8>>) -> FormFields {
    let Some(form) = form_dictionary(document) else {
        return FormFields::default();
    };
    let fields = match form.get(b"Fields").ok() {
        Some(Object::Reference(id)) => document
            .get_object(*id)
            .ok()
            .and_then(|fields| fields.as_array().ok().cloned())
            .unwrap_or_default(),
        Some(Object::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    let resources = form.get(b"DR").ok().and_then(|resources| match resources {
        Object::Reference(id) => document.get_dictionary(*id).ok().cloned(),
        Object::Dictionary(resources) => Some(resources.clone()),
        _ => None,
    });
    let mut renames = Vec::new();
    for field in &fields {
        let Ok(id) = field.as_reference() else {
            continue;
        };
        let Some(name) = document
            .get_dictionary(id)
            .ok()
            .and_then(|field| field.get(b"T").ok()?.as_str().ok().map(<[u8]>::to_vec))
        else {
            continue;
        };
        if taken.insert(name.clone()) {
            continue;
        }
        let mut renamed = name.clone();
        renamed.extend_from_slice(format!(" ({number})").as_bytes());
        taken.insert(renamed.clone());
        renames.push((id, renamed));
    }
    FormFields {
        fields,
        resources,
        renames,
    }
}

/// Adds a merged file's form fields to the merged form, with the fonts
/// their text uses.
fn add_form_fields(merged: &mut Document, fields: FormFields) -> Result<(), String> {
    let FormFields {
        fields,
        resources,
        renames,
    } = fields;
    let resources = resources.unwrap_or_default();
    for (id, name) in renames {
        if let Ok(field) = merged.get_object_mut(id).and_then(Object::as_dict_mut) {
            field.set("T", Object::string_literal(name));
        }
    }
    let mut form = form_dictionary(merged).unwrap_or_default();
    let mut all = match form.get(b"Fields").ok() {
        Some(Object::Reference(id)) => merged
            .get_object(*id)
            .ok()
            .and_then(|fields| fields.as_array().ok().cloned())
            .unwrap_or_default(),
        Some(Object::Array(items)) => items.clone(),
        _ => Vec::new(),
    };
    all.extend(fields);
    form.set("Fields", Object::Array(all));
    // Fonts the merged fields name and the form does not have yet.
    let mut merged_resources = match form.get(b"DR").ok() {
        Some(Object::Reference(id)) => merged.get_dictionary(*id).ok().cloned(),
        Some(Object::Dictionary(resources)) => Some(resources.clone()),
        _ => None,
    }
    .unwrap_or_default();
    for (kind, entries) in resources.iter() {
        let entries = match entries {
            Object::Reference(id) => merged.get_dictionary(*id).ok().cloned(),
            Object::Dictionary(entries) => Some(entries.clone()),
            _ => None,
        };
        let Some(entries) = entries else {
            continue;
        };
        let mut combined = match merged_resources.get(kind).ok() {
            Some(Object::Reference(id)) => merged.get_dictionary(*id).ok().cloned(),
            Some(Object::Dictionary(existing)) => Some(existing.clone()),
            _ => None,
        }
        .unwrap_or_default();
        for (name, value) in entries.iter() {
            if !combined.has(name) {
                combined.set(name.clone(), value.clone());
            }
        }
        merged_resources.set(kind.clone(), combined);
    }
    form.set("DR", merged_resources);
    merged
        .catalog_mut()
        .map_err(|error| format!("the merged form cannot be written: {error}"))?
        .set("AcroForm", form);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};

    /// A file of pages saying `texts`, with a markup named `markup` on its
    /// first page and a text field named `field`.
    fn file(texts: &[&str], markup: &str, field: &str) -> Vec<u8> {
        let mut document = Document::with_version("1.7");
        let root = document.new_object_id();
        let widget = document.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "FT" => "Tx",
            "T" => Object::string_literal(field), "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        let square = document.add_object(dictionary! {
            "Type" => "Annot", "Subtype" => "Square",
            "NM" => Object::string_literal(markup), "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        });
        let mut kids = Vec::new();
        for (index, text) in texts.iter().enumerate() {
            let content = document.add_object(Stream::new(
                dictionary! {},
                format!("BT /F1 12 Tf 10 10 Td ({text}) Tj ET").into_bytes(),
            ));
            let mut page =
                dictionary! { "Type" => "Page", "Parent" => root, "Contents" => content };
            if index == 0 {
                page.set("Annots", vec![square.into(), widget.into()]);
            }
            kids.push(Object::Reference(document.add_object(page)));
        }
        let count = kids.len() as i64;
        document.objects.insert(
            root,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => kids, "Count" => count,
                "MediaBox" => vec![0.into(), 0.into(), 200.into(), 100.into()],
            }),
        );
        let catalog = document.add_object(dictionary! {
            "Type" => "Catalog", "Pages" => root,
            "AcroForm" => dictionary! { "Fields" => vec![widget.into()] },
        });
        document.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        bytes
    }

    #[test]
    fn files_merge_in_order_with_their_markups_and_fields() {
        let merged = merge_pdfs(&[
            file(&["A", "B"], "markup", "Name"),
            file(&["C"], "markup", "Name"),
        ])
        .unwrap();
        let merged = Document::load_mem(&merged).unwrap();
        let pages = merged.get_pages();
        let texts = pages
            .values()
            .map(|page| String::from_utf8_lossy(&merged.get_page_content(*page)).into_owned())
            .collect::<Vec<_>>();
        assert_eq!(texts.len(), 3);
        for (text, letter) in texts.iter().zip(["(A)", "(B)", "(C)"]) {
            assert!(text.contains(letter), "{text}");
        }
        // Every page keeps its inherited size.
        for page in pages.values() {
            assert!(merged.get_dictionary(*page).unwrap().has(b"MediaBox"));
        }
        // Markup names stay unique, and fields keep their own values.
        let mut names = annotation_names(&merged).into_iter().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, [b"markup".to_vec(), b"merged-2:markup".to_vec()]);
        let mut fields = top_field_names(&merged).into_iter().collect::<Vec<_>>();
        fields.sort();
        assert_eq!(fields, [b"Name".to_vec(), b"Name (2)".to_vec()]);
    }

    #[test]
    fn merging_needs_a_document() {
        assert!(merge_pdfs(&[]).is_err());
        assert!(merge_pdfs(&[b"not a pdf".to_vec()]).is_err());
    }
}
