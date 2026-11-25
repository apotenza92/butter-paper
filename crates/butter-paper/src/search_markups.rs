//! What a search reads of a document's markups: each markup's text, label or
//! caption, author, subject and replies. The live annotation model gives the
//! text of the markups it holds, edits included. The file's own entries,
//! read once when the document opens ([`MarkupMetadata`]), give author,
//! subject and dates, and give the whole record of annotations the model
//! does not hold (sticky notes, comments of other apps) and of replies.

use std::{collections::HashMap, ops::Range};

use crate::annotation_model::{Annotation, AnnotationKind, InkTool};

/// One entry of a page's `/Annots`, as the file has it. Recorded when the
/// document opens, for every annotation except links and pop-ups.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkupMetadata {
    /// The `/NM`, or `page-{page}-annotation-{index}` when it has none.
    pub name: String,
    /// The annotation's object, which a reply names as its parent.
    pub object: Option<(u32, u16)>,
    pub page_index: u32,
    pub subtype: String,
    pub subject: Option<String>,
    pub author: Option<String>,
    /// `/Contents`, or `/RC` read as plain text.
    pub contents: Option<String>,
    pub created: Option<String>,
    pub modified: Option<String>,
    /// The object of the annotation this one replies to (`/IRT`).
    pub in_reply_to: Option<(u32, u16)>,
    /// The annotation's box in page space: x0, y0, x1, y1.
    pub rect: Option<[f64; 4]>,
    /// The editable model holds the annotation, under `name`.
    pub in_model: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkupReply {
    pub author: String,
    pub text: String,
}

/// A markup, as a search sees it: one unit.
#[derive(Clone, Debug, PartialEq)]
pub struct MarkupSearchRecord {
    /// The model's id for a markup it holds, else the annotation's name.
    pub id: String,
    pub page_index: u32,
    /// x0, y0, x1, y1 in page space.
    pub bounds: Option<[f64; 4]>,
    /// The editable model holds it, so a result can select it.
    pub in_model: bool,
    pub subject: String,
    pub author: String,
    /// Its own text: content, label or caption.
    pub text: String,
    pub replies: Vec<MarkupReply>,
    pub modified: Option<String>,
    /// What the unit's terms match in: the text, then each reply's text.
    pub body: String,
    /// Where each reply's text is in `body`.
    reply_ranges: Vec<Range<usize>>,
    /// The author and the replies' authors, one per line.
    pub authors: String,
}

impl MarkupSearchRecord {
    pub fn new(
        id: String,
        page_index: u32,
        bounds: Option<[f64; 4]>,
        in_model: bool,
        subject: String,
        author: String,
        text: String,
        replies: Vec<MarkupReply>,
        modified: Option<String>,
    ) -> Self {
        // A line break in a markup is a space for a search.
        let flat = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = flat(&text);
        let mut body = text.clone();
        let mut reply_ranges = Vec::with_capacity(replies.len());
        let mut authors = author.clone();
        let replies = replies
            .into_iter()
            .map(|reply| MarkupReply {
                author: flat(&reply.author),
                text: flat(&reply.text),
            })
            .collect::<Vec<_>>();
        for reply in &replies {
            if !body.is_empty() {
                body.push(' ');
            }
            let start = body.len();
            body.push_str(&reply.text);
            reply_ranges.push(start..body.len());
            authors.push('\n');
            authors.push_str(&reply.author);
        }
        Self {
            id,
            page_index,
            bounds,
            in_model,
            subject,
            author,
            text,
            replies,
            modified,
            body,
            reply_ranges,
            authors,
        }
    }

    /// The reply whose text holds `offset` of [`Self::body`].
    pub fn reply_at(&self, offset: usize) -> Option<usize> {
        self.reply_ranges
            .iter()
            .position(|range| range.contains(&offset))
    }
}

/// The subject Revu gives each tool's markups (`pdf-format.md`).
pub fn native_subject(annotation: &Annotation) -> &'static str {
    match annotation {
        Annotation::Pen(pen) => match pen.tool() {
            InkTool::Pen => "Pen",
            InkTool::Highlight => "Highlight",
        },
        other => match other.kind() {
            AnnotationKind::Rectangle => "Rectangle",
            AnnotationKind::Redact => "Redaction",
            AnnotationKind::Ellipse => "Ellipse",
            AnnotationKind::Arc => "Arc",
            AnnotationKind::Line => "Line",
            AnnotationKind::Arrow => "Arrow",
            AnnotationKind::Polyline => "PolyLine",
            AnnotationKind::Polygon => "Polygon",
            AnnotationKind::Polylength => "Polylength Measurement",
            AnnotationKind::Area => "Area Measurement",
            AnnotationKind::Cloud => "Cloud",
            AnnotationKind::CloudPlus => "Cloud+",
            AnnotationKind::Callout => "Callout",
            AnnotationKind::Pen => "Pen",
            AnnotationKind::TextBox => "Text Box",
            AnnotationKind::Dimension => "Dimension",
            AnnotationKind::Length => "Length Measurement",
            AnnotationKind::Image => "Image",
            AnnotationKind::Snapshot => "Snapshot",
        },
    }
}

/// The text a markup shows: a Text Box's, Callout's or Cloud+'s content, a
/// measurement's caption. `None` for a kind with no text of its own, whose
/// comment (`/Contents`) the file keeps and the model does not read.
pub fn native_text(annotation: &Annotation) -> Option<String> {
    match annotation {
        Annotation::TextBox(markup) => Some(markup.content().to_owned()),
        Annotation::Callout(markup) => Some(markup.content().to_owned()),
        Annotation::CloudPlus(markup) => Some(markup.content().to_owned()),
        Annotation::MeasurementPath(markup) => Some(markup.caption()),
        Annotation::Length(markup) => Some(markup.caption()),
        Annotation::Dimension(markup) => Some(markup.content().to_owned()),
        _ => None,
    }
}

/// The records of a document's markups, in the order a result list shows
/// them: the model's markups in drawing order, then the annotations it does
/// not hold in file order. Replies attach to the markup they answer.
///
/// `annotations` is the live model, whose text wins over the file's.
/// `metadata` is what the file had when it opened. A markup the model holds
/// that the file does not know (made in this session) has `session_author`
/// and the subject of its tool.
pub fn markup_records(
    annotations: &[Annotation],
    metadata: &[MarkupMetadata],
    session_author: &str,
) -> Vec<MarkupSearchRecord> {
    let by_object = metadata
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| entry.object.map(|object| (object, index)))
        .collect::<HashMap<_, _>>();
    // Each reply goes to the first annotation up its chain that is not a
    // reply.
    let mut replies: HashMap<usize, Vec<MarkupReply>> = HashMap::new();
    for entry in metadata {
        let Some(mut parent) = entry.in_reply_to else {
            continue;
        };
        let mut root = by_object.get(&parent).copied();
        for _ in 0..8 {
            let Some(index) = root else { break };
            match metadata[index].in_reply_to {
                Some(next) => {
                    parent = next;
                    root = by_object.get(&parent).copied();
                }
                None => break,
            }
        }
        if let Some(root) = root {
            replies.entry(root).or_default().push(MarkupReply {
                author: entry.author.clone().unwrap_or_default(),
                text: entry.contents.clone().unwrap_or_default(),
            });
        }
    }
    let by_name = metadata
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.in_model)
        .map(|(index, entry)| (entry.name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut records = Vec::new();
    for annotation in annotations {
        let entry = by_name
            .get(annotation.id().as_str())
            .map(|index| (*index, &metadata[*index]));
        records.push(MarkupSearchRecord::new(
            annotation.id().as_str().to_owned(),
            annotation.page_index(),
            annotation.page_bounds(),
            true,
            entry
                .and_then(|(_, entry)| entry.subject.clone())
                .unwrap_or_else(|| native_subject(annotation).to_owned()),
            entry
                .and_then(|(_, entry)| entry.author.clone())
                .unwrap_or_else(|| session_author.to_owned()),
            // The model's text wins, as it has the unsaved edits; a markup
            // with none has the file's comment.
            native_text(annotation)
                .or_else(|| entry.and_then(|(_, entry)| entry.contents.clone()))
                .unwrap_or_default(),
            entry
                .and_then(|(index, _)| replies.remove(&index))
                .unwrap_or_default(),
            entry.and_then(|(_, entry)| entry.modified.clone()),
        ));
    }
    for (index, entry) in metadata.iter().enumerate() {
        if entry.in_model || entry.in_reply_to.is_some() {
            continue;
        }
        records.push(MarkupSearchRecord::new(
            entry.name.clone(),
            entry.page_index,
            entry.rect,
            false,
            entry.subject.clone().unwrap_or_default(),
            entry.author.clone().unwrap_or_default(),
            entry.contents.clone().unwrap_or_default(),
            replies.remove(&index).unwrap_or_default(),
            entry.modified.clone(),
        ));
    }
    records
}

/// Rich text (`/RC`, a fragment of XHTML) as plain text.
pub fn plain_text_of_rich_text(rich: &str) -> String {
    let mut plain = String::with_capacity(rich.len());
    let mut rest = rich;
    while let Some(open) = rest.find('<') {
        plain.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[open + 1..open + close].trim().to_ascii_lowercase();
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default();
        if matches!(name, "p" | "br" | "div" | "li") {
            plain.push(' ');
        }
        rest = &rest[open + close + 1..];
    }
    plain.push_str(rest);
    let mut decoded = String::with_capacity(plain.len());
    let mut rest = plain.as_str();
    while let Some(start) = rest.find('&') {
        decoded.push_str(&rest[..start]);
        let entity = &rest[start + 1..];
        let replacement = entity.find(';').filter(|end| *end <= 8).and_then(|end| {
            let name = &entity[..end];
            let character = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .map(|hex| u32::from_str_radix(hex, 16))
                    .or_else(|| name.strip_prefix('#').map(|digits| digits.parse::<u32>()))
                    .and_then(Result::ok)
                    .and_then(char::from_u32),
            }?;
            Some((character, end + 2))
        });
        match replacement {
            Some((character, consumed)) => {
                decoded.push(character);
                rest = &rest[start + consumed..];
            }
            None => {
                decoded.push('&');
                rest = entity;
            }
        }
    }
    decoded.push_str(rest);
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::annotation_model::{
        MarkupId, PdfRect, RectangleAnnotation, RectangleAppearance, TextBoxAnnotation,
        TextBoxStyle,
    };

    fn entry(name: &str, object: u32, page_index: u32) -> MarkupMetadata {
        MarkupMetadata {
            name: name.to_owned(),
            object: Some((object, 0)),
            page_index,
            subtype: "Text".into(),
            ..MarkupMetadata::default()
        }
    }

    fn text_box(id: &str, page_index: u32, content: &str) -> Annotation {
        Annotation::TextBox(
            TextBoxAnnotation::new(
                MarkupId::new(id).unwrap(),
                page_index,
                PdfRect::new(10., 10., 100., 30.).unwrap(),
                content.to_owned(),
                TextBoxStyle::new("Helvetica", 12., "#000000", 1.).unwrap(),
            )
            .unwrap(),
        )
    }

    #[test]
    fn a_markup_the_file_does_not_know_has_the_session_author_and_its_tools_subject() {
        let records = markup_records(&[text_box("NEW", 2, "Check the door")], &[], "alex");
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(
            (
                record.id.as_str(),
                record.page_index,
                record.subject.as_str(),
                record.author.as_str(),
                record.text.as_str(),
                record.in_model,
            ),
            ("NEW", 2, "Text Box", "alex", "Check the door", true)
        );
        assert_eq!(record.bounds, Some([10., 10., 110., 40.]));
    }

    #[test]
    fn the_live_text_wins_and_the_file_gives_author_and_subject() {
        let mut file = entry("TB", 7, 0);
        file.in_model = true;
        file.subject = Some("Text Box".into());
        file.author = Some("Sam".into());
        file.contents = Some("old text from the file".into());
        file.modified = Some("D:20261001000000".into());
        let records = markup_records(&[text_box("TB", 0, "edited text")], &[file], "alex");
        assert_eq!(records[0].text, "edited text");
        assert_eq!(records[0].author, "Sam");
        assert_eq!(records[0].modified.as_deref(), Some("D:20261001000000"));
        assert!(!records[0].body.contains("old text"));
    }

    #[test]
    fn a_markup_the_model_dropped_leaves_no_record() {
        let mut file = entry("GONE", 7, 0);
        file.in_model = true;
        file.contents = Some("deleted in this session".into());
        assert!(markup_records(&[], &[file], "alex").is_empty());
    }

    #[test]
    fn an_annotation_the_model_does_not_hold_is_a_record_from_the_file() {
        let mut sticky = entry("note-1", 9, 4);
        sticky.subtype = "Text".into();
        sticky.subject = Some("Note".into());
        sticky.author = Some("Sam".into());
        sticky.contents = Some("Agreed\nwith this".into());
        sticky.rect = Some([1., 2., 3., 4.]);
        let records = markup_records(&[], &[sticky], "alex");
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert!(!record.in_model);
        assert_eq!(record.text, "Agreed with this");
        assert_eq!(record.bounds, Some([1., 2., 3., 4.]));
        assert_eq!(record.page_index, 4);
    }

    #[test]
    fn replies_attach_to_the_markup_they_answer_and_are_not_markups() {
        let mut parent = entry("TB", 7, 0);
        parent.in_model = true;
        parent.author = Some("Alex".into());
        let mut reply = entry("r1", 8, 0);
        reply.in_reply_to = Some((7, 0));
        reply.author = Some("Sam".into());
        reply.contents = Some("Agreed, 60 min".into());
        let mut second = entry("r2", 10, 0);
        // A reply to a reply goes to the same markup.
        second.in_reply_to = Some((8, 0));
        second.author = Some("Kim".into());
        second.contents = Some("Me too".into());
        let records = markup_records(
            &[text_box("TB", 0, "Check the fire door")],
            &[parent, reply, second],
            "alex",
        );
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(
            record.replies,
            [
                MarkupReply {
                    author: "Sam".into(),
                    text: "Agreed, 60 min".into()
                },
                MarkupReply {
                    author: "Kim".into(),
                    text: "Me too".into()
                }
            ]
        );
        assert_eq!(record.body, "Check the fire door Agreed, 60 min Me too");
        assert_eq!(record.authors, "Alex\nSam\nKim");
        let at = record.body.find("Me too").unwrap();
        assert_eq!(record.reply_at(at), Some(1));
        assert_eq!(record.reply_at(0), None);
    }

    #[test]
    fn a_reply_to_a_retained_annotation_attaches_to_it() {
        let mut note = entry("note", 3, 1);
        note.contents = Some("Question".into());
        let mut reply = entry("answer", 4, 1);
        reply.in_reply_to = Some((3, 0));
        reply.contents = Some("Answer".into());
        let records = markup_records(&[], &[note, reply], "alex");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].replies.len(), 1);
    }

    #[test]
    fn a_reply_to_an_unknown_parent_is_dropped() {
        let mut reply = entry("lost", 4, 1);
        reply.in_reply_to = Some((99, 0));
        assert!(markup_records(&[], &[reply], "alex").is_empty());
    }

    #[test]
    fn a_markup_with_no_text_of_its_own_has_the_comment_the_file_keeps() {
        let rectangle = Annotation::Rectangle(RectangleAnnotation {
            id: MarkupId::new("R").unwrap(),
            page_index: 0,
            rect: PdfRect::new(0., 0., 10., 10.).unwrap(),
            rotation_degrees: 0.,
            appearance: RectangleAppearance::default(),
            locked: false,
        });
        let mut file = entry("R", 3, 0);
        file.in_model = true;
        file.contents = Some("Boundary of the steel frame".into());
        let records = markup_records(&[rectangle], &[file], "alex");
        assert_eq!(records[0].text, "Boundary of the steel frame");
        // A text markup emptied in this session does not get the old one.
        let mut file = entry("TB", 4, 0);
        file.in_model = true;
        file.contents = Some("old".into());
        let records = markup_records(&[text_box("TB", 0, " ")], &[file], "alex");
        assert_eq!(records[0].text, "");
    }

    #[test]
    fn a_markup_with_no_text_still_has_author_and_subject() {
        let rectangle = Annotation::Rectangle(RectangleAnnotation {
            id: MarkupId::new("R").unwrap(),
            page_index: 0,
            rect: PdfRect::new(0., 0., 10., 10.).unwrap(),
            rotation_degrees: 0.,
            appearance: RectangleAppearance::default(),
            locked: false,
        });
        let records = markup_records(&[rectangle], &[], "alex");
        assert_eq!(records[0].subject, "Rectangle");
        assert_eq!(records[0].body, "");
    }

    #[test]
    fn rich_text_reads_as_plain_text() {
        assert_eq!(
            plain_text_of_rich_text(
                "<?xml version=\"1.0\"?><body><p>Fire&#160;door &amp; <b>frame</b></p><p>60 &lt; 90 min</p></body>"
            ),
            "Fire door & frame 60 < 90 min"
        );
        assert_eq!(plain_text_of_rich_text("a<br/>b"), "a b");
        assert_eq!(plain_text_of_rich_text("5 & 6"), "5 & 6");
        assert_eq!(plain_text_of_rich_text("&#x41;&#66;"), "AB");
    }
}
