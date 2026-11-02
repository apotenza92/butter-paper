//! Link annotations: where a document's pages link to, so a click on a link
//! can follow it.
//!
//! Butter Paper follows web and mail links and links to the document's own
//! pages. It does not run JavaScript, launch files or open other documents.

use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::{BTreeMap, HashMap};

const ANNOTATION_HIDDEN: i64 = 1 << 1;
const ANNOTATION_NO_VIEW: i64 = 1 << 5;
/// How deep a name tree is followed, and how many `/Next` actions.
const MAX_DEPTH: usize = 32;

/// The links a document's pages carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LinkLayer {
    pub links: Vec<PdfLink>,
}

/// One link area on a page.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfLink {
    /// The file page it is on.
    pub page_index: u32,
    /// `[left, bottom, right, top]` in page user space.
    pub rect: [f64; 4],
    pub target: LinkTarget,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LinkTarget {
    /// A web or mail address the system opens.
    Uri(String),
    /// A file page of this document.
    Page(u32),
}

impl LinkLayer {
    /// The link under a page point: the smallest where links overlap.
    pub fn link_at(&self, page_index: u32, x: f64, y: f64, tolerance: f64) -> Option<&PdfLink> {
        self.links
            .iter()
            .filter(|link| {
                link.page_index == page_index
                    && x >= link.rect[0] - tolerance
                    && x <= link.rect[2] + tolerance
                    && y >= link.rect[1] - tolerance
                    && y <= link.rect[3] + tolerance
            })
            .min_by(|a, b| area(a.rect).total_cmp(&area(b.rect)))
    }
}

fn area(rect: [f64; 4]) -> f64 {
    (rect[2] - rect[0]) * (rect[3] - rect[1])
}

/// Whether a link's address is one Butter Paper hands to the system: web
/// and mail addresses only, never local files or other programs' schemes.
pub fn openable_uri(uri: &str) -> bool {
    let uri = uri.trim();
    let Some((scheme, rest)) = uri.split_once(':') else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    match scheme.as_str() {
        "http" | "https" => rest.starts_with("//") && rest.len() > 2,
        "mailto" => !rest.is_empty(),
        _ => false,
    }
}

pub fn extract_link_layer(document: &Document) -> LinkLayer {
    let pages = document.get_pages();
    let page_numbers: HashMap<ObjectId, u32> = pages
        .iter()
        .map(|(number, id)| (*id, number.saturating_sub(1)))
        .collect();
    let named = NamedDestinations::new(document);
    let mut links = Vec::new();
    for (number, page_id) in &pages {
        let Ok(page) = document.get_dictionary(*page_id) else {
            continue;
        };
        let annotations = page
            .get(b"Annots")
            .ok()
            .and_then(|value| resolve(document, value).as_array().ok());
        for annotation in annotations.into_iter().flatten() {
            let Ok(annotation) = resolve(document, annotation).as_dict() else {
                continue;
            };
            if annotation.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Link") {
                continue;
            }
            let flags = annotation
                .get(b"F")
                .ok()
                .and_then(|flags| resolve(document, flags).as_i64().ok())
                .unwrap_or(0);
            if flags & (ANNOTATION_HIDDEN | ANNOTATION_NO_VIEW) != 0 {
                continue;
            }
            let Some(rect) = rect(document, annotation) else {
                continue;
            };
            let resolver = Resolver {
                document,
                page_numbers: &page_numbers,
                named: &named,
            };
            let Some(target) = resolver.link_target(annotation) else {
                continue;
            };
            links.push(PdfLink {
                page_index: number.saturating_sub(1),
                rect,
                target,
            });
        }
    }
    LinkLayer { links }
}

fn resolve<'a>(document: &'a Document, mut object: &'a Object) -> &'a Object {
    for _ in 0..16 {
        match object {
            Object::Reference(id) => match document.get_object(*id) {
                Ok(target) => object = target,
                Err(_) => return &Object::Null,
            },
            _ => return object,
        }
    }
    &Object::Null
}

fn number(document: &Document, object: &Object) -> Option<f64> {
    match resolve(document, object) {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some(f64::from(*value)),
        _ => None,
    }
}

fn rect(document: &Document, annotation: &Dictionary) -> Option<[f64; 4]> {
    let values = resolve(document, annotation.get(b"Rect").ok()?)
        .as_array()
        .ok()?;
    if values.len() != 4 {
        return None;
    }
    let mut numbers = [0.; 4];
    for (slot, value) in numbers.iter_mut().zip(values) {
        *slot = number(document, value).filter(|value| value.is_finite())?;
    }
    let [x0, y0, x1, y1] = numbers;
    let rect = [x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1)];
    (rect[2] > rect[0] && rect[3] > rect[1]).then_some(rect)
}

fn string(document: &Document, object: &Object) -> Option<String> {
    match resolve(document, object) {
        object @ Object::String(..) => {
            Some(lopdf::decode_text_string(object).unwrap_or_else(|_| {
                String::from_utf8_lossy(object.as_str().unwrap_or_default()).into_owned()
            }))
        }
        Object::Name(name) => Some(String::from_utf8_lossy(name).into_owned()),
        _ => None,
    }
}

/// Destinations named in the catalog's `/Dests` dictionary (PDF 1.1) or its
/// `/Names` `/Dests` name tree.
struct NamedDestinations<'a> {
    names: BTreeMap<Vec<u8>, &'a Object>,
}

impl<'a> NamedDestinations<'a> {
    fn new(document: &'a Document) -> Self {
        let mut names = BTreeMap::new();
        let Ok(catalog) = document.catalog() else {
            return Self { names };
        };
        if let Some(Ok(dests)) = catalog
            .get(b"Dests")
            .ok()
            .map(|dests| resolve(document, dests).as_dict())
        {
            for (name, destination) in dests.iter() {
                names.insert(name.clone(), destination);
            }
        }
        if let Some(Ok(tree)) = catalog
            .get(b"Names")
            .ok()
            .and_then(|names| resolve(document, names).as_dict().ok())
            .and_then(|names| names.get(b"Dests").ok())
            .map(|tree| resolve(document, tree).as_dict())
        {
            Self::collect(document, tree, &mut names, 0);
        }
        Self { names }
    }

    fn collect(
        document: &'a Document,
        node: &'a Dictionary,
        names: &mut BTreeMap<Vec<u8>, &'a Object>,
        depth: usize,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        if let Ok(pairs) = node
            .get(b"Names")
            .map(|pairs| resolve(document, pairs))
            .and_then(Object::as_array)
        {
            for pair in pairs.chunks_exact(2) {
                if let Ok(name) = resolve(document, &pair[0]).as_str() {
                    names.entry(name.to_vec()).or_insert(&pair[1]);
                }
            }
        }
        if let Ok(kids) = node
            .get(b"Kids")
            .map(|kids| resolve(document, kids))
            .and_then(Object::as_array)
        {
            for kid in kids {
                if let Ok(kid) = resolve(document, kid).as_dict() {
                    Self::collect(document, kid, names, depth + 1);
                }
            }
        }
    }
}

struct Resolver<'a> {
    document: &'a Document,
    page_numbers: &'a HashMap<ObjectId, u32>,
    named: &'a NamedDestinations<'a>,
}

impl Resolver<'_> {
    fn link_target(&self, annotation: &Dictionary) -> Option<LinkTarget> {
        if let Ok(action) = annotation.get(b"A") {
            return self.action_target(action, 0);
        }
        let destination = annotation.get(b"Dest").ok()?;
        self.destination_page(destination, 0).map(LinkTarget::Page)
    }

    fn action_target(&self, action: &Object, depth: usize) -> Option<LinkTarget> {
        if depth > MAX_DEPTH {
            return None;
        }
        let action = resolve(self.document, action).as_dict().ok()?;
        let target = match action.get(b"S").and_then(Object::as_name).ok()? {
            b"URI" => action
                .get(b"URI")
                .ok()
                .and_then(|uri| resolve(self.document, uri).as_str().ok())
                // URIs are 7-bit ASCII; anything else is read leniently.
                .map(|uri| String::from_utf8_lossy(uri).trim().to_owned())
                .filter(|uri| !uri.is_empty())
                .map(LinkTarget::Uri),
            b"GoTo" => action
                .get(b"D")
                .ok()
                .and_then(|destination| self.destination_page(destination, depth + 1))
                .map(LinkTarget::Page),
            _ => None,
        };
        // An action this viewer cannot perform may chain to one it can.
        target.or_else(|| {
            let next = resolve(self.document, action.get(b"Next").ok()?);
            match next {
                Object::Array(actions) => actions
                    .iter()
                    .find_map(|action| self.action_target(action, depth + 1)),
                next => self.action_target(next, depth + 1),
            }
        })
    }

    /// The file page an explicit or named destination shows.
    fn destination_page(&self, destination: &Object, depth: usize) -> Option<u32> {
        if depth > MAX_DEPTH {
            return None;
        }
        match resolve(self.document, destination) {
            Object::Array(array) => match array.first()? {
                Object::Reference(id) => self.page_numbers.get(id).copied(),
                // Destinations into other documents (`GoToR`) number their
                // pages; in this document's own links that is non-standard
                // but common.
                Object::Integer(page) => u32::try_from(*page)
                    .ok()
                    .filter(|page| (*page as usize) < self.page_numbers.len()),
                _ => None,
            },
            // A destination dictionary keeps the array in `/D`.
            Object::Dictionary(dictionary) => {
                self.destination_page(dictionary.get(b"D").ok()?, depth + 1)
            }
            named @ (Object::String(..) | Object::Name(_)) => {
                let name = match named {
                    Object::Name(name) => name.clone(),
                    named => named.as_str().ok()?.to_vec(),
                };
                let destination = self.named.names.get(&name).copied().or_else(|| {
                    // Name tree keys may be text strings with a byte-order
                    // mark where the link names them without one.
                    let text = string(self.document, named)?;
                    self.named.names.iter().find_map(|(key, value)| {
                        (string(self.document, &Object::string_literal(key.clone())).as_deref()
                            == Some(text.as_str()))
                        .then_some(*value)
                    })
                })?;
                self.destination_page(destination, depth + 1)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Stream, dictionary};

    fn document_with_links(links: Vec<Dictionary>) -> Document {
        let mut document = Document::with_version("1.7");
        let pages_id = document.new_object_id();
        let content = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_ids: Vec<ObjectId> = (0..2)
            .map(|_| {
                document.add_object(dictionary! {
                    "Type" => "Page",
                    "Parent" => pages_id,
                    "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                    "Contents" => content,
                })
            })
            .collect();
        let annots: Vec<Object> = links
            .into_iter()
            .map(|mut link| {
                if let Ok(Object::Integer(page)) = link.get(b"Dest").cloned()
                    && page >= 100
                {
                    link.set(
                        "Dest",
                        vec![page_ids[(page - 100) as usize].into(), "Fit".into()],
                    );
                }
                document.add_object(link).into()
            })
            .collect();
        let first = document.get_dictionary_mut(page_ids[0]).unwrap();
        first.set("Annots", annots);
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => page_ids.iter().map(|id| Object::from(*id)).collect::<Vec<_>>(),
                "Count" => 2,
            }),
        );
        let named_target = vec![page_ids[1].into(), "Fit".into()];
        let names = document.add_object(dictionary! {
            "Dests" => dictionary! {
                "Names" => vec![Object::string_literal("chapter-two"), named_target.into()],
            },
        });
        let catalog = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "Names" => names,
        });
        document.trailer.set("Root", catalog);
        document
    }

    fn link(rect: [f64; 4], extra: Dictionary) -> Dictionary {
        let mut link = dictionary! {
            "Type" => "Annot",
            "Subtype" => "Link",
            "Rect" => rect.iter().map(|value| Object::Real(*value as f32)).collect::<Vec<_>>(),
        };
        link.extend(&extra);
        link
    }

    #[test]
    fn reads_web_links_and_links_to_pages() {
        let document = document_with_links(vec![
            link(
                [62., 532., 92., 547.],
                dictionary! {
                    "A" => dictionary! {
                        "S" => "URI",
                        "URI" => Object::string_literal("https://example.com/watch"),
                    },
                },
            ),
            link([10., 10., 50., 30.], dictionary! { "Dest" => 101 }),
            link(
                [10., 40., 50., 60.],
                dictionary! {
                    "A" => dictionary! {
                        "S" => "GoTo",
                        "D" => Object::string_literal("chapter-two"),
                    },
                },
            ),
            // Hidden, script-only and degenerate links are skipped.
            link(
                [10., 70., 50., 90.],
                dictionary! {
                    "F" => ANNOTATION_HIDDEN,
                    "A" => dictionary! {
                        "S" => "URI",
                        "URI" => Object::string_literal("https://hidden.example"),
                    },
                },
            ),
            link(
                [10., 100., 50., 120.],
                dictionary! {
                    "A" => dictionary! {
                        "S" => "JavaScript",
                        "JS" => Object::string_literal("app.alert(1)"),
                    },
                },
            ),
            link([10., 130., 10., 150.], dictionary! { "Dest" => 101 }),
        ]);
        let layer = extract_link_layer(&document);
        assert_eq!(
            layer
                .links
                .iter()
                .map(|link| (link.page_index, link.target.clone()))
                .collect::<Vec<_>>(),
            vec![
                (0, LinkTarget::Uri("https://example.com/watch".into())),
                (0, LinkTarget::Page(1)),
                (0, LinkTarget::Page(1)),
            ]
        );
        assert_eq!(layer.links[0].rect, [62., 532., 92., 547.]);
    }

    #[test]
    fn finds_the_smallest_link_under_a_point() {
        let layer = LinkLayer {
            links: vec![
                PdfLink {
                    page_index: 0,
                    rect: [0., 0., 100., 100.],
                    target: LinkTarget::Page(1),
                },
                PdfLink {
                    page_index: 0,
                    rect: [10., 10., 20., 20.],
                    target: LinkTarget::Uri("https://example.com".into()),
                },
            ],
        };
        assert_eq!(
            layer.link_at(0, 15., 15., 0.).map(|link| &link.target),
            Some(&LinkTarget::Uri("https://example.com".into()))
        );
        assert_eq!(
            layer.link_at(0, 50., 50., 0.).map(|link| &link.target),
            Some(&LinkTarget::Page(1))
        );
        assert_eq!(layer.link_at(1, 15., 15., 0.), None);
        assert_eq!(layer.link_at(0, 101., 50., 0.), None);
        assert!(layer.link_at(0, 101., 50., 2.).is_some());
    }

    #[test]
    fn opens_only_web_and_mail_addresses() {
        assert!(openable_uri("https://www.myer.com.au/p/fossil"));
        assert!(openable_uri("HTTP://example.com"));
        assert!(openable_uri("mailto:someone@example.com"));
        assert!(!openable_uri("file:///etc/passwd"));
        assert!(!openable_uri("javascript:alert(1)"));
        assert!(!openable_uri("smb://server/share"));
        assert!(!openable_uri("https:"));
        assert!(!openable_uri("www.example.com"));
    }
}
