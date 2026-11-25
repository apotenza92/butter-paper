//! Loads a PDF with lopdf and repairs two faults that PDFium and Revu read
//! without error, but that lopdf drops or reports:
//!
//! - A name with a `#` that two hex digits do not follow (Bluebeam writes
//!   `/Name /#YURy...`). lopdf rejects the name, so it drops the whole
//!   object. qpdf and PDFium read the `#` as a literal `#`. This module
//!   parses each dropped object again with such a `#` written as `#23`.
//! - A reference to an object that the file does not define. ISO 32000-1,
//!   7.3.10, reads it as null, and a dictionary entry with a null value is
//!   the same as no entry. This module removes such dictionary entries, sets
//!   such array items to null, and removes null items from each page's
//!   `/Annots`. The saved file is then equal in meaning to the source.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use lopdf::xref::XrefEntry;
use lopdf::{Dictionary, Document, Object, ObjectId, ObjectStream, Stream};

/// What [`load_pdf_mem`] repaired. Tests and diagnostics read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PdfLoadRepairs {
    /// Objects that lopdf dropped and that parse with literal `#` in names.
    pub recovered_objects: Vec<ObjectId>,
    /// References to undefined objects, now null or removed.
    pub dangling_references: usize,
}

/// Reads the file and loads it as [`load_pdf_mem`] does.
pub fn load_pdf(path: impl AsRef<Path>) -> lopdf::Result<Document> {
    let bytes = std::fs::read(path)?;
    load_pdf_mem(&bytes)
}

/// Loads `bytes` with lopdf, then applies the repairs of this module.
pub fn load_pdf_mem(bytes: &[u8]) -> lopdf::Result<Document> {
    load_pdf_mem_with_repairs(bytes).map(|(document, _)| document)
}

/// Loads `bytes` as [`load_pdf_mem`] does and tells what it repaired.
pub fn load_pdf_mem_with_repairs(bytes: &[u8]) -> lopdf::Result<(Document, PdfLoadRepairs)> {
    let mut document = Document::load_mem(bytes)?;
    let recovered_objects = recover_objects_with_stray_name_hashes(&mut document, bytes);
    let dangling_references = remove_dangling_references(&mut document);
    Ok((
        document,
        PdfLoadRepairs {
            recovered_objects,
            dangling_references,
        },
    ))
}

/// Parses again each object in the cross-reference table that lopdf did not
/// load, if a stray `#` in a name is the cause. Other missing objects stay
/// missing: [`remove_dangling_references`] then reads them as null.
fn recover_objects_with_stray_name_hashes(document: &mut Document, source: &[u8]) -> Vec<ObjectId> {
    let missing: Vec<(ObjectId, XrefEntry)> = document
        .reference_table
        .entries
        .iter()
        .filter_map(|(&number, entry)| {
            let id = match *entry {
                XrefEntry::Normal { generation, .. } => (number, generation),
                XrefEntry::Compressed { .. } => (number, 0),
                XrefEntry::Free | XrefEntry::UnusableFree => return None,
            };
            (!document.objects.contains_key(&id)).then(|| (id, entry.clone()))
        })
        .collect();
    let mut containers: BTreeMap<u32, Option<ObjectStreamContent>> = BTreeMap::new();
    let mut recovered = Vec::new();
    for (id, entry) in missing {
        let body = match entry {
            XrefEntry::Compressed { container, index } => {
                let content = containers
                    .entry(container)
                    .or_insert_with(|| ObjectStreamContent::decode(document, container));
                let Some(content) = content else {
                    continue;
                };
                let Some(body) = content.object_body(id.0, usize::from(index)) else {
                    continue;
                };
                body.to_vec()
            }
            XrefEntry::Normal { offset, .. } => {
                let Some(body) = indirect_object_body(source, offset as usize) else {
                    continue;
                };
                body.to_vec()
            }
            XrefEntry::Free | XrefEntry::UnusableFree => continue,
        };
        let Some(repaired) = escape_stray_name_hashes(&body) else {
            continue;
        };
        let Some(mut object) = parse_direct_object(&repaired) else {
            continue;
        };
        if matches!(entry, XrefEntry::Normal { .. })
            && let Some(state) = document.encryption_state.as_ref()
            && lopdf::encryption::decrypt_object(state, id, &mut object).is_err()
        {
            continue;
        }
        document.objects.insert(id, object);
        recovered.push(id);
    }
    recovered
}

/// The decoded content of an object stream and its object offsets.
struct ObjectStreamContent {
    content: Vec<u8>,
    first: usize,
    /// Object number and offset from `first`, in stream order.
    entries: Vec<(u32, usize)>,
}

impl ObjectStreamContent {
    fn decode(document: &Document, container: u32) -> Option<Self> {
        let stream = document.objects.get(&(container, 0))?.as_stream().ok()?;
        let content = stream.get_plain_content().ok()?;
        let first = usize::try_from(stream.dict.get(b"First").ok()?.as_i64().ok()?).ok()?;
        let header = std::str::from_utf8(content.get(..first)?).ok()?;
        let numbers: Vec<usize> = header
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .ok()?;
        let entries = numbers
            .chunks_exact(2)
            .map(|pair| Some((u32::try_from(pair[0]).ok()?, pair[1])))
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            content,
            first,
            entries,
        })
    }

    /// The bytes of object `number`, from its offset to the next offset.
    fn object_body(&self, number: u32, index: usize) -> Option<&[u8]> {
        let offset = match self.entries.get(index) {
            Some(&(listed, offset)) if listed == number => offset,
            _ => self.entries.iter().find(|(listed, _)| *listed == number)?.1,
        };
        let start = self.first.checked_add(offset)?;
        let end = self
            .entries
            .iter()
            .filter_map(|&(_, other)| (other > offset).then_some(other))
            .min()
            .and_then(|next| self.first.checked_add(next))
            .unwrap_or(self.content.len())
            .min(self.content.len());
        self.content.get(start..end)
    }
}

/// The bytes between `obj` and `endobj` of the indirect object at `offset`.
/// `None` for a stream object: its data cannot pass through
/// [`parse_direct_object`].
fn indirect_object_body(source: &[u8], offset: usize) -> Option<&[u8]> {
    let rest = source.get(offset..)?;
    let header_end = find(rest.get(..rest.len().min(64))?, b"obj")? + b"obj".len();
    let body = &rest[header_end..];
    let body = &body[..find(body, b"endobj")?];
    if find(body, b"stream").is_some() {
        return None;
    }
    Some(body)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Parses one direct object with lopdf's own parser. It goes through a
/// one-object object stream because that is lopdf's public parse entry.
fn parse_direct_object(body: &[u8]) -> Option<Object> {
    const NUMBER: u32 = 1;
    let header = format!("{NUMBER} 0 ");
    let mut content = header.clone().into_bytes();
    content.extend_from_slice(body);
    let mut dict = Dictionary::new();
    dict.set("Type", Object::Name(b"ObjStm".to_vec()));
    dict.set("N", 1);
    dict.set("First", i64::try_from(header.len()).ok()?);
    let stream = Stream::new(dict, content);
    ObjectStream::new(&stream)
        .ok()?
        .objects
        .remove(&(NUMBER, 0))
}

/// Writes each `#` in a name that two hex digits do not follow as `#23`.
/// Strings, hex strings and comments stay as they are. `None` if no name
/// has a stray `#`.
pub(crate) fn escape_stray_name_hashes(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len() + 8);
    let mut changed = false;
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        match bytes[index] {
            b'(' => {
                let mut depth = 0usize;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 1,
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                index += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    index += 1;
                }
                let end = index.min(bytes.len());
                out.extend_from_slice(&bytes[start..end]);
                index = end;
            }
            b'<' if bytes.get(index + 1) == Some(&b'<') => {
                out.extend_from_slice(b"<<");
                index += 2;
            }
            b'<' => {
                let end = bytes[index..]
                    .iter()
                    .position(|&byte| byte == b'>')
                    .map_or(bytes.len(), |position| index + position + 1);
                out.extend_from_slice(&bytes[start..end]);
                index = end;
            }
            b'%' => {
                let end = bytes[index..]
                    .iter()
                    .position(|&byte| byte == b'\r' || byte == b'\n')
                    .map_or(bytes.len(), |position| index + position);
                out.extend_from_slice(&bytes[start..end]);
                index = end;
            }
            b'/' => {
                out.push(b'/');
                index += 1;
                while index < bytes.len() && is_regular(bytes[index]) {
                    let byte = bytes[index];
                    let escaped = byte == b'#'
                        && bytes.get(index + 1).is_some_and(u8::is_ascii_hexdigit)
                        && bytes.get(index + 2).is_some_and(u8::is_ascii_hexdigit);
                    if escaped {
                        out.extend_from_slice(&bytes[index..index + 3]);
                        index += 3;
                    } else if byte == b'#' {
                        out.extend_from_slice(b"#23");
                        changed = true;
                        index += 1;
                    } else {
                        out.push(byte);
                        index += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    changed.then_some(out)
}

fn is_regular(byte: u8) -> bool {
    !matches!(
        byte,
        b'\0'
            | b'\t'
            | b'\n'
            | b'\x0C'
            | b'\r'
            | b' '
            | b'('
            | b')'
            | b'<'
            | b'>'
            | b'['
            | b']'
            | b'{'
            | b'}'
            | b'/'
            | b'%'
    )
}

/// Reads each reference to an undefined object as null: removes it from a
/// dictionary, sets it to null in an array, and removes null items from
/// each page's `/Annots`. An object is undefined when the cross-reference
/// table has no in-use entry for it (ISO 32000-1, 7.3.10). Returns the
/// number of references it changed.
fn remove_dangling_references(document: &mut Document) -> usize {
    // An object that the cross-reference table lists as in use is defined,
    // even if lopdf could not parse it: an incremental update keeps a
    // reference to it, and the base revision still holds it.
    let mut defined: HashSet<ObjectId> = document.objects.keys().copied().collect();
    defined.extend(
        document
            .reference_table
            .entries
            .iter()
            .filter_map(|(&number, entry)| match *entry {
                XrefEntry::Normal { generation, .. } => Some((number, generation)),
                XrefEntry::Compressed { .. } => Some((number, 0)),
                XrefEntry::Free | XrefEntry::UnusableFree => None,
            }),
    );
    let mut changed = 0;
    for object in document.objects.values_mut() {
        // A cross-reference stream keeps its trailer entries: its /Encrypt
        // names the dictionary that lopdf removes after decryption.
        if matches!(object, Object::Stream(stream) if stream.dict.has_type(b"XRef")) {
            continue;
        }
        changed += null_dangling_in(object, &defined);
    }
    if changed == 0 {
        return 0;
    }
    let pages: Vec<ObjectId> = document.get_pages().into_values().collect();
    for page_id in pages {
        let annots = match document
            .objects
            .get_mut(&page_id)
            .and_then(|page| page.as_dict_mut().ok())
            .and_then(|page| page.get_mut(b"Annots").ok())
        {
            Some(Object::Array(annots)) => {
                annots.retain(|annot| !matches!(annot, Object::Null));
                continue;
            }
            Some(Object::Reference(id)) => *id,
            _ => continue,
        };
        if let Some(Object::Array(annots)) = document.objects.get_mut(&annots) {
            annots.retain(|annot| !matches!(annot, Object::Null));
        }
    }
    changed
}

fn null_dangling_in(object: &mut Object, defined: &HashSet<ObjectId>) -> usize {
    let dangling =
        |object: &Object| matches!(object, Object::Reference(id) if !defined.contains(id));
    match object {
        Object::Array(items) => items
            .iter_mut()
            .map(|item| {
                if dangling(item) {
                    *item = Object::Null;
                    1
                } else {
                    null_dangling_in(item, defined)
                }
            })
            .sum(),
        Object::Dictionary(dict) => null_dangling_in_dict(dict, defined),
        Object::Stream(stream) => null_dangling_in_dict(&mut stream.dict, defined),
        _ => 0,
    }
}

fn null_dangling_in_dict(dict: &mut Dictionary, defined: &HashSet<ObjectId>) -> usize {
    let keys: Vec<Vec<u8>> = dict
        .iter()
        .filter(|(_, value)| matches!(value, Object::Reference(id) if !defined.contains(id)))
        .map(|(key, _)| key.clone())
        .collect();
    for key in &keys {
        dict.remove(key);
    }
    keys.len()
        + dict
            .iter_mut()
            .map(|(_, value)| null_dangling_in(value, defined))
            .sum::<usize>()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use lopdf::dictionary;

    #[test]
    fn a_stray_hash_in_a_name_becomes_an_escaped_hash() {
        let escaped = escape_stray_name_hashes(b"<</Name /#YURy /Subtype/Stamp>>").unwrap();
        assert_eq!(escaped, b"<</Name /#23YURy /Subtype/Stamp>>");
        // One hex digit only, and a `#` at the end of the input.
        let escaped = escape_stray_name_hashes(b"/A#4 /B#").unwrap();
        assert_eq!(escaped, b"/A#234 /B#23");
    }

    #[test]
    fn valid_escapes_strings_and_comments_stay_as_they_are() {
        for unchanged in [
            &b"<</Name /A#20B /C#2f>>"[..],
            b"(a /#b (nested /#c) \\) /#d)",
            b"<23>",
            b"% /#comment\n/Plain",
            b"[1 0 R (#)]",
        ] {
            assert_eq!(escape_stray_name_hashes(unchanged), None, "{unchanged:?}");
        }
    }

    /// A one-page PDF 1.5 file. Its page's `/Annots` names objects 5 and 6,
    /// which an uncompressed object stream holds. Object 6 has the stray `#`
    /// that Bluebeam writes. `extra_annots` adds more `/Annots` items and
    /// `extra_square` adds entries to the Square annotation, object 5.
    pub(crate) fn stray_hash_object_stream_pdf(extra_annots: &str, extra_square: &str) -> Vec<u8> {
        let mut pdf = b"%PDF-1.5\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = BTreeMap::new();
        let mut object = |pdf: &mut Vec<u8>, number: u32, body: &[u8]| {
            offsets.insert(number, pdf.len());
            pdf.extend_from_slice(format!("{number} 0 obj\n").as_bytes());
            pdf.extend_from_slice(body);
            pdf.extend_from_slice(b"\nendobj\n");
        };
        object(&mut pdf, 1, b"<< /Type /Catalog /Pages 2 0 R >>");
        object(&mut pdf, 2, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
        object(
            &mut pdf,
            3,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] \
                 /Annots [5 0 R 6 0 R{extra_annots}] >>"
            )
            .as_bytes(),
        );
        let square = format!(
            "<< /Type /Annot /Subtype /Square /Rect [10 10 60 40] \
             /NM (square-kept) /C [1 0 0] /P 3 0 R{extra_square} >>"
        );
        let square = square.as_bytes();
        let stamp = b"<< /Type /Annot /Subtype /Stamp /Rect [80 80 150 120] \
                       /NM (stamp-with-stray-hash) /Name /#YURy297yQ \
                       /Subj (REV E) /P 3 0 R >>";
        let header = format!("5 0 6 {} ", square.len() + 1);
        let mut content = header.clone().into_bytes();
        content.extend_from_slice(square);
        content.push(b' ');
        content.extend_from_slice(stamp);
        let mut stream = format!(
            "<< /Type /ObjStm /N 2 /First {} /Length {} >>\nstream\n",
            header.len(),
            content.len()
        )
        .into_bytes();
        stream.extend_from_slice(&content);
        stream.extend_from_slice(b"\nendstream");
        object(&mut pdf, 4, &stream);
        // The cross-reference stream: type, field 2 (4 bytes), field 3.
        let xref_offset = pdf.len();
        let mut rows = Vec::new();
        let mut row = |kind: u8, field: u32, last: u16| {
            rows.push(kind);
            rows.extend_from_slice(&field.to_be_bytes());
            rows.extend_from_slice(&last.to_be_bytes());
        };
        row(0, 0, 0xFFFF);
        for number in 1..=4 {
            row(1, offsets[&number] as u32, 0);
        }
        row(2, 4, 0);
        row(2, 4, 1);
        row(1, xref_offset as u32, 0);
        pdf.extend_from_slice(
            format!(
                "7 0 obj\n<< /Type /XRef /Size 8 /W [1 4 2] /Root 1 0 R /Length {} >>\nstream\n",
                rows.len()
            )
            .as_bytes(),
        );
        pdf.extend_from_slice(&rows);
        pdf.extend_from_slice(
            format!("\nendstream\nendobj\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
        );
        pdf
    }

    #[test]
    fn lopdf_alone_drops_the_object_with_a_stray_hash() {
        let document = Document::load_mem(&stray_hash_object_stream_pdf("", "")).unwrap();
        assert!(document.objects.contains_key(&(5, 0)));
        assert!(!document.objects.contains_key(&(6, 0)));
    }

    #[test]
    fn an_object_stream_member_with_a_stray_hash_loads() {
        let (document, repairs) =
            load_pdf_mem_with_repairs(&stray_hash_object_stream_pdf("", "")).unwrap();
        assert_eq!(repairs.recovered_objects, vec![(6, 0)]);
        assert_eq!(repairs.dangling_references, 0);
        let stamp = document.get_dictionary((6, 0)).unwrap();
        assert_eq!(
            stamp.get(b"Name").unwrap().as_name().unwrap(),
            b"#YURy297yQ"
        );
        assert_eq!(stamp.get(b"Subtype").unwrap().as_name().unwrap(), b"Stamp");
        assert_eq!(stamp.get(b"Subj").unwrap().as_str().unwrap(), b"REV E");
        // lopdf writes the `#` as `#23`, so the saved file needs no repair.
        let mut document = document;
        let mut saved = Vec::new();
        document.save_to(&mut saved).unwrap();
        let reloaded = Document::load_mem(&saved).unwrap();
        assert_eq!(
            reloaded
                .get_dictionary((6, 0))
                .unwrap()
                .get(b"Name")
                .unwrap()
                .as_name()
                .unwrap(),
            b"#YURy297yQ"
        );
    }

    #[test]
    fn an_uncompressed_object_with_a_stray_hash_loads() {
        let mut document = Document::with_version("1.4");
        let pages = document.new_object_id();
        let page = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        });
        let stamp = document.add_object(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Stamp",
            "Rect" => vec![1.into(), 1.into(), 9.into(), 9.into()],
            "Name" => "PLACEHOLDER",
        });
        document
            .get_dictionary_mut(page)
            .unwrap()
            .set("Annots", vec![Object::Reference(stamp)]);
        document.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page.into()],
                "Count" => 1,
            }),
        );
        let catalog = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages,
        });
        document.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        // Same length, so the cross-reference offsets stay correct.
        let at = find(&bytes, b"/PLACEHOLDER").unwrap();
        bytes[at..at + b"/PLACEHOLDER".len()].copy_from_slice(b"/#PLACEHOLDE");

        assert!(
            !Document::load_mem(&bytes)
                .unwrap()
                .objects
                .contains_key(&stamp)
        );
        let (document, repairs) = load_pdf_mem_with_repairs(&bytes).unwrap();
        assert_eq!(repairs.recovered_objects, vec![stamp]);
        let name = document
            .get_dictionary(stamp)
            .unwrap()
            .get(b"Name")
            .unwrap();
        assert_eq!(name.as_name().unwrap(), b"#PLACEHOLDE");
    }

    /// A one-page PDF whose content stream (a black square) has a stray `#`
    /// in a name of its dictionary, so lopdf cannot parse it.
    fn unparsed_content_stream_pdf() -> (Vec<u8>, ObjectId, ObjectId) {
        let mut document = Document::with_version("1.4");
        let pages = document.new_object_id();
        let content = document.add_object(Stream::new(
            dictionary! { "Note" => "PLACEHOLDE" },
            b"0 0 0 rg 20 20 160 160 re f".to_vec(),
        ));
        let page = document.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
            "Contents" => content,
        });
        document.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page.into()],
                "Count" => 1,
            }),
        );
        let catalog = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages,
        });
        document.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        let at = find(&bytes, b"/PLACEHOLDE").unwrap();
        bytes[at..at + b"/PLACEHOLDE".len()].copy_from_slice(b"/#PLACEHOLD");
        (bytes, page, content)
    }

    #[test]
    fn a_reference_to_an_object_lopdf_cannot_parse_stays() {
        let (bytes, page, content) = unparsed_content_stream_pdf();
        let (document, repairs) = load_pdf_mem_with_repairs(&bytes).unwrap();
        assert!(!document.objects.contains_key(&content), "lopdf drops it");
        assert_eq!(repairs.dangling_references, 0);
        let page = document.get_dictionary(page).unwrap();
        assert_eq!(
            page.get(b"Contents").unwrap(),
            &Object::Reference(content),
            "the cross-reference table still lists the content stream"
        );
    }

    #[test]
    fn a_missing_annotation_object_reads_as_null() {
        // Object 9 is not in the file. Object 5 names it as its /Popup.
        let pdf = stray_hash_object_stream_pdf(" 9 0 R", " /Popup 9 0 R");
        let (document, repairs) = load_pdf_mem_with_repairs(&pdf).unwrap();
        assert_eq!(repairs.dangling_references, 2);
        let page = document.get_dictionary((3, 0)).unwrap();
        let annots = page.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(
            annots,
            &vec![Object::Reference((5, 0)), Object::Reference((6, 0))]
        );
        let square = document.get_dictionary((5, 0)).unwrap();
        assert!(square.get(b"Popup").is_err());
        assert!(square.get(b"P").is_ok());
    }
}
