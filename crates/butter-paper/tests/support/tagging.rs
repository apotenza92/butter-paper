//! Tagging a generated page as a Tagged PDF's producer does: each
//! marked-content identifier its content draws is one paragraph (`P`) of a
//! structure tree. Shared by tests through `#[path]`.

use lopdf::{Document, Object, ObjectId, dictionary};

/// Gives page `page_id` a structure tree with a `P` for each `/MCID` its
/// content uses (in the order of their identifiers), when it uses any.
pub fn tag_paragraphs(document: &mut Document, page_id: ObjectId) {
    let content = document.get_page_content(page_id);
    let text = String::from_utf8_lossy(&content);
    let mut ids = text
        .split("/MCID")
        .skip(1)
        .filter_map(|rest| {
            rest.trim_start()
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse::<i64>()
                .ok()
        })
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();
    let Some(&last) = ids.last() else {
        return;
    };
    let root_id = document.new_object_id();
    let mut parents = vec![Object::Null; last as usize + 1];
    let mut paragraphs = Vec::new();
    for id in ids {
        let paragraph = document.add_object(dictionary! {
            "Type" => "StructElem", "S" => "P", "P" => root_id, "Pg" => page_id, "K" => id,
        });
        parents[id as usize] = Object::Reference(paragraph);
        paragraphs.push(Object::Reference(paragraph));
    }
    document.objects.insert(
        root_id,
        Object::Dictionary(dictionary! {
            "Type" => "StructTreeRoot",
            "K" => paragraphs,
            "ParentTree" => dictionary! { "Nums" => vec![0.into(), Object::Array(parents)] },
            "ParentTreeNextKey" => 1,
        }),
    );
    if let Ok(page) = document
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
    {
        page.set("StructParents", 0);
    }
    if let Ok(catalog) = document.catalog_mut() {
        catalog.set("StructTreeRoot", root_id);
        catalog.set("MarkInfo", dictionary! { "Marked" => true });
    }
}
