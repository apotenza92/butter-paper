//! Edit Text against real producer output (`tests/fixtures/text-edit`) and
//! built pages with less common text state. For every editable line: an
//! unchanged edit keeps every content byte, and seeded random edits read back
//! as written, drawn where the editor laid them out, without moving any other
//! line.

use butter_paper::pdf_text_blocks::{TextBlockAlignment, paragraph_blocks};
use butter_paper::pdf_text_edit::{
    PageTextEdit, PageTextLayer, PageTextLine, TextEditLimits, TextEditRun, apply_text_edits,
    extract_page_text, layout_text_edit,
};
use lopdf::{Dictionary, Document, Object, Stream, dictionary};

const PRODUCERS: [&str; 3] = ["chrome", "libreoffice", "quartz"];
/// Characters no fixture font has, drawn by the bundled fallback fonts.
const FALLBACK: [char; 3] = ['Ж', 'Ω', 'ł'];

fn producer(name: &str) -> Document {
    let path = format!(
        "{}/tests/fixtures/text-edit/{name}.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::load(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn helvetica() -> Object {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }
    .into()
}

/// A one-page document with Helvetica as `/F1` and `/F2`, drawn by
/// `contents` (one stream each), with extra page entries.
fn built(contents: &[&[u8]], page: Dictionary) -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let f1 = document.add_object(helvetica());
    let f2 = document.add_object(helvetica());
    let streams = contents
        .iter()
        .map(|content| {
            Object::Reference(document.add_object(Stream::new(Dictionary::new(), content.to_vec())))
        })
        .collect::<Vec<_>>();
    let mut page_dictionary = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => if streams.len() == 1 { streams[0].clone() } else { Object::Array(streams) },
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => f1, "F2" => f2 } },
    };
    for (key, value) in page.into_iter() {
        page_dictionary.set(key, value);
    }
    let page_id = document.add_object(page_dictionary);
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    reload(&mut document)
}

/// Pages whose text state the producers above do not exercise.
fn built_cases() -> Vec<(&'static str, Document)> {
    vec![
        (
            "kerned TJ",
            built(
                &[b"BT /F1 12 Tf 72 700 Td [(W) 80 (A) 120 (V) -40 (E) 60 (s) 0 ( are) -250 (kerned)] TJ ET"],
                Dictionary::new(),
            ),
        ),
        (
            "spacing, scaling and rise",
            built(
                &[b"BT /F1 12 Tf 1.5 Tc 4 Tw 80 Tz 2 Ts 72 700 Td (Spaced and scaled words) Tj ET\n\
                    BT /F1 12 Tf 0 Tc 0 Tw 100 Tz 0 Ts 72 680 Td (Plain neighbour line) Tj ET"],
                Dictionary::new(),
            ),
        ),
        (
            "text matrix scaling and fonts",
            built(
                &[b"BT 2 0 0 2 72 650 Tm /F1 9 Tf (Twice ) Tj /F2 9 Tf (as large) Tj ET\n\
                    BT 1 0 0 1 72 620 Tm /F1 10 Tf (Unscaled line below) Tj ET"],
                Dictionary::new(),
            ),
        ),
        (
            "split content streams",
            built(
                &[
                    // Streams may divide only between tokens.
                    b"BT /F1 12 Tf 72 700 Td (Begins in the first stream)" as &[u8],
                    b"Tj ET BT /F1 12 Tf 72 680 Td (Second stream line) Tj ET",
                ],
                Dictionary::new(),
            ),
        ),
        (
            "rotated page",
            built(
                &[b"BT /F1 12 Tf 72 700 Td (On a rotated page) Tj ET"],
                dictionary! { "Rotate" => 90 },
            ),
        ),
        (
            "offset crop box and user unit",
            built(
                &[b"BT /F1 12 Tf 172 600 Td (Inside an offset crop box) Tj ET"],
                dictionary! {
                    "CropBox" => vec![100.into(), 100.into(), 500.into(), 700.into()],
                    "UserUnit" => 2,
                },
            ),
        ),
    ]
}

fn reload(document: &mut Document) -> Document {
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("save");
    Document::load_mem(&bytes).expect("reload")
}

fn layer(document: &Document) -> PageTextLayer {
    extract_page_text(document, 0, &TextEditLimits::default()).expect("extract")
}

fn content(document: &Document) -> Vec<u8> {
    let page = document.get_pages()[&1];
    document.get_page_content(page)
}

fn editable(line: &PageTextLine) -> bool {
    line.read_only.is_none() && !line.text.trim().is_empty()
}

/// A small deterministic generator (xorshift64*).
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// One random insert, delete or replace on `line`, as runs. New characters
/// come from the line itself, or now and then from the fallback set, and take
/// the style of the character before them.
fn random_edit(line: &PageTextLine, rng: &mut Seeded) -> PageTextEdit {
    let mut chars = line
        .chars
        .iter()
        .map(|character| (character.character, character.style))
        .collect::<Vec<_>>();
    let pool = line
        .text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<Vec<_>>();
    let new_char = |rng: &mut Seeded| {
        if rng.below(5) == 0 {
            FALLBACK[rng.below(FALLBACK.len())]
        } else {
            pool[rng.below(pool.len())]
        }
    };
    loop {
        let mut edited = chars.clone();
        let at = rng.below(edited.len() + 1);
        let style = edited
            .get(at.saturating_sub(1))
            .or(edited.first())
            .map_or(0, |(_, style)| *style);
        match rng.below(3) {
            0 => {
                for offset in 0..1 + rng.below(6) {
                    edited.insert(at + offset, (new_char(rng), style));
                }
            }
            1 if at < edited.len() => {
                let end = (at + 1 + rng.below(4)).min(edited.len());
                edited.drain(at..end);
            }
            _ if at < edited.len() => {
                let end = (at + 1 + rng.below(4)).min(edited.len());
                let count = 1 + rng.below(4);
                edited.splice(
                    at..end,
                    (0..count)
                        .map(|_| (new_char(rng), style))
                        .collect::<Vec<_>>(),
                );
            }
            _ => continue,
        }
        let text = edited.iter().map(|(c, _)| *c).collect::<String>();
        if text.trim().is_empty() || text == line.text {
            continue;
        }
        chars = edited;
        break;
    }
    let mut runs: Vec<TextEditRun> = Vec::new();
    for (character, style) in chars {
        let style = &line.styles[style];
        match runs.last_mut() {
            Some(run) if &run.style == style => run.text.push(character),
            _ => runs.push(TextEditRun {
                style: style.clone(),
                text: character.to_string(),
            }),
        }
    }
    PageTextEdit {
        page_index: 0,
        anchor: line.anchor,
        original: line.original_runs(),
        runs,
        layout: Default::default(),
    }
}

fn assert_line_unmoved(case: &str, before: &PageTextLine, after: &PageTextLine) {
    assert_eq!(before.text, after.text, "{case}: neighbour text changed");
    assert_eq!(before.chars.len(), after.chars.len(), "{case}");
    for (old, new) in before.chars.iter().zip(&after.chars) {
        assert!(
            (old.x - new.x).abs() < 0.01,
            "{case}: {:?} moved from {} to {} in {:?}",
            old.character,
            old.x,
            new.x,
            before.text
        );
    }
    for (old, new) in before.matrix.iter().zip(&after.matrix) {
        assert!((old - new).abs() < 0.01, "{case}: line matrix moved");
    }
}

/// Applies `edits` to a copy, reloads it, and checks that edited lines read
/// back as written where the editor laid them out, and that every other
/// line is unchanged.
fn check_edits(case: &str, document: &Document, before: &PageTextLayer, edits: &[PageTextEdit]) {
    let mut edited = document.clone();
    apply_text_edits(&mut edited, edits, &TextEditLimits::default())
        .unwrap_or_else(|error| panic!("{case}: {error} applying {edits:#?}"));
    let after = layer(&reload(&mut edited));
    for line in &before.lines {
        let found = after
            .line(&line.anchor)
            .unwrap_or_else(|| panic!("{case}: line {:?} lost its anchor", line.text));
        match edits.iter().find(|edit| edit.anchor == line.anchor) {
            None => assert_line_unmoved(case, line, found),
            Some(edit) => {
                assert_eq!(found.text, edit.text(), "{case}: from {:?}", line.text);
                let laid = layout_text_edit(before, line, &edit.runs).unwrap();
                let text = edit.text();
                let shown = found.chars.iter().filter(|c| !c.synthetic);
                let laid_out = laid
                    .chars
                    .iter()
                    .zip(text.chars())
                    .filter(|(_, character)| !character.is_whitespace());
                for ((laid, character), extracted) in
                    laid_out.zip(shown.filter(|c| !c.character.is_whitespace()))
                {
                    assert_eq!(character, extracted.character, "{case}: {:?}", edit.text());
                    assert!(
                        (laid.x - extracted.x).abs() < 0.01,
                        "{case}: {character:?} laid out at {} but drawn at {} in {:?}",
                        laid.x,
                        extracted.x,
                        edit.text()
                    );
                }
            }
        }
    }
    assert_eq!(
        after.lines.len(),
        before.lines.len(),
        "{case}: line count changed"
    );
}

fn corpus() -> Vec<(String, Document)> {
    PRODUCERS
        .iter()
        .map(|name| (name.to_string(), producer(name)))
        .chain(
            built_cases()
                .into_iter()
                .map(|(name, document)| (name.to_string(), document)),
        )
        .collect()
}

#[test]
fn producers_extract_their_text_as_written() {
    let text = |name| {
        layer(&producer(name))
            .lines
            .iter()
            .map(|line| line.text.trim_end().to_owned())
            .collect::<Vec<_>>()
    };
    let chrome = text("chrome");
    assert!(chrome.contains(&"Project Specification".to_owned()));
    assert!(chrome.contains(&"Office efficient fluid workflow, café façade naïve.".to_owned()));
    assert!(chrome.contains(&"Mono line: BEAM-01 356x171x51 UB L=6000".to_owned()));
    // LibreOffice maps a ligature glyph to the text of its first use
    // ("fix"); each later use reads from its own /ActualText.
    let libreoffice = text("libreoffice");
    assert!(libreoffice.contains(&"specification.".to_owned()));
    assert!(
        libreoffice.contains(&"• Office efficient fluid workflow, café façade naïve.".to_owned())
    );
    assert!(
        libreoffice
            .iter()
            .any(|line| line.contains("connections, fixings and"))
    );
    let quartz = text("quartz");
    assert!(quartz.contains(&"Second line with a tab  here and café façade.".to_owned()));
    for (name, document) in corpus() {
        let layer = layer(&document);
        assert!(layer.lines.iter().any(editable), "{name}: no editable text");
    }
}

#[test]
fn unchanged_edits_keep_every_content_byte() {
    for (name, document) in corpus() {
        let before = layer(&document);
        let edits = before
            .lines
            .iter()
            .filter(|line| editable(line))
            .map(|line| PageTextEdit {
                page_index: 0,
                anchor: line.anchor,
                original: line.original_runs(),
                runs: line.original_runs(),
                layout: Default::default(),
            })
            .collect::<Vec<_>>();
        let mut edited = document.clone();
        apply_text_edits(&mut edited, &edits, &TextEditLimits::default()).unwrap();
        assert_eq!(content(&edited), content(&document), "{name}");
    }
}

#[test]
fn random_edits_read_back_where_laid_out_without_moving_other_lines() {
    for (name, document) in corpus() {
        let before = layer(&document);
        let mut rng = Seeded(0x9e37_79b9_7f4a_7c15 ^ name.len() as u64);
        let lines = before
            .lines
            .iter()
            .filter(|line| editable(line))
            .collect::<Vec<_>>();
        // Each line alone, a few times; BP_TEXT_EDIT_ROUNDS runs a longer
        // search locally.
        let rounds = std::env::var("BP_TEXT_EDIT_ROUNDS")
            .ok()
            .and_then(|rounds| rounds.parse().ok())
            .unwrap_or(3_usize);
        for line in &lines {
            for round in 0..rounds {
                let edit = random_edit(line, &mut rng);
                check_edits(
                    &format!("{name} round {round}"),
                    &document,
                    &before,
                    &[edit],
                );
            }
        }
        // Every line at once.
        for round in 0..rounds.div_ceil(3) {
            let edits = lines
                .iter()
                .map(|line| random_edit(line, &mut rng))
                .collect::<Vec<_>>();
            check_edits(
                &format!("{name} all lines {round}"),
                &document,
                &before,
                &edits,
            );
        }
    }
}

/// The file as the app saves it: without objects edits left unreachable.
fn saved(document: &mut Document) -> Vec<u8> {
    document.prune_objects();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("save");
    bytes
}

/// The embedded Type0 fonts Edit Text added, by base font name.
fn fallback_fonts(document: &Document) -> Vec<String> {
    document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dictionary| {
            dictionary.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type0".as_slice())
        })
        .filter_map(|dictionary| dictionary.get(b"BaseFont").and_then(Object::as_name).ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .filter(|name| name.contains("Arimo") || name.contains("Tinos"))
        .collect()
}

#[test]
fn fallback_letters_embed_a_small_subset_that_later_edits_extend() {
    let mut document = producer("libreoffice");
    let before = saved(&mut document).len();
    let first = layer(&document);
    let line = first.lines.iter().find(|line| editable(line)).unwrap();
    let edit = |line: &PageTextLine, text: String| PageTextEdit {
        page_index: 0,
        anchor: line.anchor,
        original: line.original_runs(),
        runs: vec![TextEditRun {
            style: line.styles[0].clone(),
            text,
        }],
        layout: Default::default(),
    };
    apply_text_edits(
        &mut document,
        &[edit(line, format!("{} ЖΩł", line.text.trim_end()))],
        &TextEditLimits::default(),
    )
    .unwrap();
    let mut document = Document::load_mem(&saved(&mut document)).unwrap();
    let after = saved(&mut document).len();
    assert!(after - before < 30_000, "grew by {} bytes", after - before);
    let fonts = fallback_fonts(&document);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    let (tag, name) = fonts[0].split_once('+').expect("a subset is tagged");
    assert!(tag.len() == 6 && tag.chars().all(|letter| letter.is_ascii_uppercase()));
    // The heading is bold Helvetica-like text.
    assert_eq!(name, "Arimo-Bold");

    // Editing the saved line again extends the same font.
    let second = layer(&document);
    let next = second.line(&line.anchor).unwrap();
    apply_text_edits(
        &mut document,
        &[edit(next, format!("{} ДЯ", next.text.trim_end()))],
        &TextEditLimits::default(),
    )
    .unwrap();
    let mut document = Document::load_mem(&saved(&mut document)).unwrap();
    let fonts = fallback_fonts(&document);
    assert_eq!(fonts.len(), 1, "{fonts:?}");
    let texts = layer(&document)
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect::<Vec<_>>();
    assert!(
        texts.iter().any(|text| text.ends_with(" ЖΩł ДЯ")),
        "{texts:?}"
    );
    assert!(saved(&mut document).len() - before < 40_000);
}

/// Each producer's paragraph blocks: alignment and the start of each line.
#[test]
fn producer_lines_group_into_their_paragraphs() {
    use TextBlockAlignment::{Justified, Left};
    let blocks = |name| {
        let layer = layer(&producer(name));
        paragraph_blocks(&layer)
            .into_iter()
            .map(|block| {
                let starts = block
                    .lines
                    .iter()
                    .map(|anchor| {
                        layer
                            .line(anchor)
                            .unwrap()
                            .text
                            .trim()
                            .chars()
                            .take(12)
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>();
                (block.alignment, starts)
            })
            .collect::<Vec<_>>()
    };
    let expect = |rows: &[(TextBlockAlignment, &[&str])]| {
        rows.iter()
            .map(|(alignment, starts)| {
                (
                    *alignment,
                    starts
                        .iter()
                        .map(|start| start.to_string())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        blocks("chrome"),
        expect(&[
            (Left, &["Project Spec"]),
            (
                Justified,
                &[
                    "The contract",
                    "works, inclu",
                    "accordance w",
                    "tolerances g"
                ]
            ),
            (Left, &["Ragged parag", "the contract"]),
            (Left, &["Centred note"]),
            (Left, &["Revision B —"]),
            // Chrome draws list bullets as shapes, so the items read as one
            // paragraph.
            (Left, &["Grade S355 s", "Hot-dip galv", "Office effic"]),
            (
                Justified,
                &[
                    "Left column:",
                    "unless shown",
                    "inspected vi",
                    "welds shall "
                ]
            ),
            (
                Justified,
                &["Right column", "cleaned to S", "hours of cle", "occurs."]
            ),
            (Left, &["Serif paragr"]),
            (Left, &["Mono line: B"]),
        ])
    );
    assert_eq!(
        blocks("libreoffice"),
        expect(&[
            (Left, &["Project Spec"]),
            (
                Justified,
                &[
                    "The contract",
                    "including co",
                    "approved sho",
                    "specificatio"
                ]
            ),
            (Left, &["Ragged parag", "contractor s"]),
            (Left, &["Centred note"]),
            (Left, &["Revision B —"]),
            // The bulleted items are one list.
            (Left, &["• Grade S355", "• Hot-dip ga", "• Office eff"]),
            (
                Left,
                &["Left column:", "shown otherw", "before paint", "replaced."]
            ),
            (Left, &["2½ and prime", "any visible "]),
            (Left, &["Serif paragr", "including 20"]),
            (Left, &["Right column"]),
            (Left, &["Mono line: B"]),
        ])
    );
    // Blank lines of spaces belong to no block.
    assert_eq!(
        blocks("quartz"),
        expect(&[
            (Left, &["Quartz plain", "Second line "]),
            (Left, &["A longer par", "only where t"]),
        ])
    );
}

#[test]
fn owner_restrictions_decide_whether_text_can_change() {
    // Both open without a password; one forbids changing content.
    let locked = producer("quartz-no-modify");
    let locked_text = layer(&locked);
    let line = locked_text
        .lines
        .iter()
        .find(|line| !line.text.trim().is_empty())
        .unwrap();
    assert_eq!(
        line.read_only.as_deref(),
        Some("the document's permissions do not allow changing its content")
    );
    let edit = PageTextEdit {
        page_index: 0,
        anchor: line.anchor,
        original: line.original_runs(),
        runs: vec![TextEditRun {
            style: line.styles[0].clone(),
            text: "Changed".into(),
        }],
        layout: Default::default(),
    };
    assert!(matches!(
        apply_text_edits(&mut locked.clone(), &[edit], &TextEditLimits::default()),
        Err(butter_paper::pdf_text_edit::TextEditError::ReadOnly(_))
    ));

    let mut open = producer("quartz-owner-password");
    let open_text = layer(&open);
    let line = open_text.lines.iter().find(|line| editable(line)).unwrap();
    let edit = PageTextEdit {
        page_index: 0,
        anchor: line.anchor,
        original: line.original_runs(),
        runs: vec![TextEditRun {
            style: line.styles[0].clone(),
            text: "Quartz plain text, edited.".into(),
        }],
        layout: Default::default(),
    };
    apply_text_edits(&mut open, &[edit], &TextEditLimits::default()).unwrap();
    let reopened = Document::load_mem(&saved(&mut open)).unwrap();
    assert_eq!(
        layer(&reopened).line(&line.anchor).unwrap().text,
        "Quartz plain text, edited."
    );
}

mod paragraphs {
    use super::*;
    use butter_paper::pdf_text_reflow::{Paragraph, StyledChar};
    use std::sync::Arc;

    fn joined(text: &[StyledChar]) -> String {
        text.iter().map(|(character, _)| *character).collect()
    }

    /// Applies `text` as the paragraph of `block`, returning the reloaded
    /// page's text layer.
    fn reflow(document: &Document, paragraph: &Paragraph, text: &[StyledChar]) -> PageTextLayer {
        let wrapped = paragraph.wrap(text).unwrap();
        let edits = paragraph.edits(text, &wrapped);
        let mut edited = document.clone();
        apply_text_edits(&mut edited, &edits, &TextEditLimits::default()).unwrap();
        layer(&reload(&mut edited))
    }

    fn normalise(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn inserted_words_flow_through_the_paragraph_and_justify() {
        for name in ["chrome", "libreoffice"] {
            let document = producer(name);
            let before = Arc::new(layer(&document));
            let block = paragraph_blocks(&before)
                .into_iter()
                .find(|block| {
                    before
                        .line(&block.lines[0])
                        .unwrap()
                        .text
                        .starts_with("The contractor")
                })
                .unwrap();
            let paragraph = Paragraph::new(before.clone(), &block).unwrap();
            let mut text = paragraph.text(&[], &[]);
            let at = joined(&text).find("structural steel").unwrap() + "structural steel".len();
            let at = joined(&text)[..at].chars().count();
            let style = text[at - 1].1.clone();
            let insert = " and miscellaneous metalwork, with all welding, painting and";
            text.splice(
                at..at,
                insert.chars().map(|character| (character, style.clone())),
            );
            let after = reflow(&document, &paragraph, &text);

            // The paragraph reads as edited, from its first line down.
            let first = after.line(&block.lines[0]).unwrap();
            let start = after
                .lines
                .iter()
                .position(|line| line.anchor == first.anchor)
                .unwrap();
            let read = after.lines[start..]
                .iter()
                .take_while(|line| !line.text.starts_with("Ragged"))
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(normalise(&read), normalise(&joined(&text)), "{name}");
            // A first line the change leaves alone keeps its glyph positions.
            let original_first = before.line(&block.lines[0]).unwrap();
            if first.text == original_first.text {
                assert_line_unmoved(name, original_first, first);
            }
            // Justified lines end at the paragraph's right edge.
            let edge = |line: &PageTextLine| {
                let last = line
                    .chars
                    .iter()
                    .rposition(|c| !c.character.is_whitespace())
                    .unwrap();
                let character = &line.chars[last];
                let (x, _) = line.quad(character.x, character.x + character.advance)[1];
                x
            };
            let right = edge(before.line(&block.lines[0]).unwrap());
            for line in &after.lines[start + 1..start + 3] {
                assert!(
                    (edge(line) - right).abs() < 0.2,
                    "{name}: {} ends at {}",
                    line.text,
                    edge(line)
                );
            }
            // Other paragraphs are untouched.
            for line in before
                .lines
                .iter()
                .filter(|line| !block.lines.contains(&line.anchor))
            {
                assert_line_unmoved(name, line, after.line(&line.anchor).unwrap());
            }
        }
    }

    #[test]
    fn chosen_alignment_and_line_spacing_place_every_line() {
        use butter_paper::pdf_text_blocks::TextBlockAlignment;
        use butter_paper::pdf_text_edit::ParagraphFormat;
        for name in ["chrome", "libreoffice"] {
            let document = producer(name);
            let before = Arc::new(layer(&document));
            let block = paragraph_blocks(&before)
                .into_iter()
                .find(|block| {
                    before
                        .line(&block.lines[0])
                        .unwrap()
                        .text
                        .starts_with("The contractor")
                })
                .unwrap();
            let format = ParagraphFormat {
                alignment: Some(TextBlockAlignment::Right),
                line_spacing: Some(2.),
            };
            let paragraph = Paragraph::new(before.clone(), &block)
                .unwrap()
                .with_format(format);
            let text = paragraph.text(&[], &[]);
            let wrapped = paragraph.wrap(&text).unwrap();
            let edits = paragraph.edits(&text, &wrapped);
            // The first line's edit keeps the settings; reopening the
            // paragraph from the edits gives back its text.
            let first_edit = edits
                .iter()
                .find(|edit| edit.anchor == block.lines[0])
                .unwrap();
            assert_eq!(first_edit.layout.paragraph, format);
            let current = block
                .lines
                .iter()
                .map(|anchor| {
                    edits
                        .iter()
                        .find(|edit| edit.anchor == *anchor)
                        .map_or_else(
                            || before.line(anchor).unwrap().original_runs(),
                            |edit| edit.runs.clone(),
                        )
                })
                .collect::<Vec<_>>();
            let added = edits
                .iter()
                .find(|edit| Some(&edit.anchor) == block.lines.last())
                .map(|edit| edit.layout.extra_lines.clone())
                .unwrap_or_default();
            let reopened = Paragraph::new(before.clone(), &block)
                .unwrap()
                .with_format(format);
            assert_eq!(
                normalise(&joined(&reopened.text(&current, &added))),
                normalise(&joined(&text)),
                "{name}"
            );

            let mut edited = document.clone();
            apply_text_edits(&mut edited, &edits, &TextEditLimits::default()).unwrap();
            let after = layer(&reload(&mut edited));
            let first = after.line(&block.lines[0]).unwrap();
            let mut lines = after
                .lines
                .iter()
                .filter(|line| !line.text.is_empty())
                .filter(|line| {
                    line.anchor == first.anchor
                        || !before.lines.iter().any(|old| old.anchor == line.anchor)
                })
                .collect::<Vec<_>>();
            lines.sort_by(|a, b| b.origin().1.total_cmp(&a.origin().1));
            let read = lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            assert_eq!(normalise(&read), normalise(&joined(&text)), "{name}");
            for line in &block.lines[1..] {
                assert_eq!(after.line(line).unwrap().text, "", "{name}");
            }
            // Baselines two font sizes apart, each line ending at the right
            // edge.
            let size = first.styles[first.chars[0].style].font_size
                * first.matrix[3].hypot(first.matrix[2]);
            for pair in lines.windows(2) {
                let gap = pair[0].origin().1 - pair[1].origin().1;
                assert!((gap - 2. * size).abs() < 0.05, "{name}: {gap} {size}");
            }
            for line in &lines {
                let last = line
                    .chars
                    .iter()
                    .rposition(|c| !c.character.is_whitespace())
                    .unwrap();
                let character = &line.chars[last];
                let (x, _) = line.quad(character.x, character.x + character.advance)[1];
                assert!(
                    (x - block.right).abs() < 0.5,
                    "{name}: {} ends at {x}",
                    line.text
                );
            }
        }
    }

    #[test]
    fn a_list_item_added_with_enter_saves_and_reopens_as_an_item() {
        use butter_paper::page_text_editor::PageTextEditState;
        let document = producer("libreoffice");
        let before = Arc::new(layer(&document));
        let block = paragraph_blocks(&before)
            .into_iter()
            .find(|block| block.list.is_some())
            .unwrap();
        let paragraph = Paragraph::new(before.clone(), &block).unwrap();
        let mut state = PageTextEditState::new(paragraph.clone(), &[], &[], &[], 0).unwrap();
        let first_break = state.text().find('\n').unwrap();
        state.move_to(state.text()[..first_break].chars().count(), false);
        assert!(state.insert_newline());
        assert!(state.insert("Weld inspection to the approved procedure for all site joints"));
        let text = state.text();
        let edits = state.edits();

        // Reopening from the edits keeps the items apart.
        let edit_of = |anchor| edits.iter().find(|edit| edit.anchor == anchor);
        let current = block
            .lines
            .iter()
            .map(|anchor| {
                edit_of(*anchor).map_or_else(
                    || before.line(anchor).unwrap().original_runs(),
                    |edit| edit.runs.clone(),
                )
            })
            .collect::<Vec<_>>();
        let breaks = block
            .lines
            .iter()
            .map(|anchor| edit_of(*anchor).map(|edit| edit.layout.hard_break))
            .collect::<Vec<_>>();
        let added = block
            .lines
            .last()
            .and_then(|anchor| edit_of(*anchor))
            .map(|edit| edit.layout.extra_lines.clone())
            .unwrap_or_default();
        assert_eq!(
            joined(&paragraph.text_with_breaks(&current, &breaks, &added)),
            text
        );

        // Saved, the page shows four items at the bullets' edge, one list.
        let mut edited = document.clone();
        apply_text_edits(&mut edited, &edits, &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut edited));
        let list = paragraph_blocks(&after)
            .into_iter()
            .find(|block| block.list.is_some())
            .unwrap();
        let items = list
            .lines
            .iter()
            .map(|anchor| after.line(anchor).unwrap().text.clone())
            .collect::<Vec<_>>();
        // Four items; the new one wraps onto a line at the items' text.
        assert_eq!(items.len(), 5, "{items:?}");
        assert_eq!(
            items[1].trim(),
            "• Weld inspection to the approved procedure for",
            "{items:?}"
        );
        assert_eq!(items[2].trim(), "all site joints", "{items:?}");
        assert_eq!(items.iter().filter(|item| item.starts_with('•')).count(), 4);
        let text_start = |anchor| {
            let line = after.line(anchor).unwrap();
            let skip = usize::from(line.text.starts_with('•'));
            let first = line
                .chars
                .iter()
                .skip(skip)
                .find(|c| !c.character.is_whitespace())
                .unwrap();
            line.quad(first.x, first.x)[0].0
        };
        let starts = list.lines.iter().map(text_start).collect::<Vec<_>>();
        assert!(
            starts.iter().all(|start| (start - starts[0]).abs() < 0.05),
            "{starts:?}"
        );
    }

    #[test]
    fn a_paragraph_flows_from_one_column_into_the_next() {
        use butter_paper::pdf_text_edit::ParagraphFormat;
        // Three lines at the foot of the left column, ending mid-sentence,
        // and two at the top of the right one.
        let document = built(
            &[
                b"BT /F1 10 Tf 72 700 Td (the quick brown fox jumps over the lazy) Tj \
                0 -14 Td (the quick brown fox jumps over the lazy) Tj \
                0 -14 Td (the quick brown fox jumps over the lazy) Tj ET\n\
                BT /F1 10 Tf 320 700 Td (the quick brown fox jumps over the lazy) Tj \
                0 -14 Td (dog sleeps.) Tj ET",
            ],
            Dictionary::new(),
        );
        let before = Arc::new(layer(&document));
        let blocks = paragraph_blocks(&before);
        assert_eq!(blocks.len(), 1, "{blocks:?}");
        let block = &blocks[0];
        assert_eq!(block.lines.len(), 5);

        // At double spacing the left column holds two lines; the rest
        // continue at the top of the right column.
        let paragraph =
            Paragraph::new(before.clone(), block)
                .unwrap()
                .with_format(ParagraphFormat {
                    alignment: None,
                    line_spacing: Some(2.),
                });
        let text = paragraph.text(&[], &[]);
        let wrapped = paragraph.wrap(&text).unwrap();
        let edits = paragraph.edits(&text, &wrapped);
        let mut edited = document.clone();
        apply_text_edits(&mut edited, &edits, &TextEditLimits::default()).unwrap();
        let after = layer(&reload(&mut edited));
        let mut drawn = after
            .lines
            .iter()
            .filter(|line| !line.text.trim().is_empty())
            .map(|line| {
                let first = line
                    .chars
                    .iter()
                    .find(|c| !c.character.is_whitespace())
                    .unwrap();
                let (x, y) = line.quad(first.x, first.x)[0];
                (
                    (x * 10.).round() / 10.,
                    ((y - line.descent) * 10.).round() / 10.,
                )
            })
            .collect::<Vec<_>>();
        drawn.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.total_cmp(&a.1)));
        assert_eq!(
            drawn,
            [
                (72., 700.),
                (72., 680.),
                (320., 700.),
                (320., 680.),
                (320., 660.)
            ]
        );
    }

    #[test]
    fn deleted_words_pull_lines_up_and_empty_the_last() {
        let document = producer("libreoffice");
        let before = Arc::new(layer(&document));
        let block = paragraph_blocks(&before)
            .into_iter()
            .find(|block| {
                before
                    .line(&block.lines[0])
                    .unwrap()
                    .text
                    .starts_with("The contractor")
            })
            .unwrap();
        let paragraph = Paragraph::new(before.clone(), &block).unwrap();
        let mut text = paragraph.text(&[], &[]);
        // Delete "approved shop drawings and erected plumb, level and true to
        // line within the tolerances given in the " (most of line 3).
        let all = joined(&text);
        let from = all[..all.find("approved shop").unwrap()].chars().count();
        let to = all[..all.find("specification").unwrap()].chars().count();
        text.drain(from..to);
        let after = reflow(&document, &paragraph, &text);
        let lines = block
            .lines
            .iter()
            .map(|anchor| after.line(anchor).unwrap().text.clone())
            .collect::<Vec<_>>();
        assert_eq!(normalise(&lines.join(" ")), normalise(&joined(&text)));
        assert_eq!(lines.last().map(String::as_str), Some(""), "{lines:?}");
    }

    #[test]
    fn random_paragraph_edits_read_back_and_leave_other_lines_alone() {
        let rounds = std::env::var("BP_TEXT_EDIT_ROUNDS")
            .ok()
            .and_then(|rounds| rounds.parse().ok())
            .unwrap_or(3_usize);
        for name in ["chrome", "libreoffice", "quartz"] {
            let document = producer(name);
            let before = Arc::new(layer(&document));
            let mut rng = Seeded(0xa076_1d64_78bd_642f ^ name.len() as u64);
            for block in paragraph_blocks(&before)
                .into_iter()
                .filter(|block| block.lines.len() > 1)
            {
                let paragraph = Paragraph::new(before.clone(), &block).unwrap();
                for round in 0..rounds {
                    let mut text = paragraph.text(&[], &[]);
                    let pool = text
                        .iter()
                        .filter(|(character, _)| !character.is_whitespace())
                        .cloned()
                        .collect::<Vec<_>>();
                    let at = rng.below(text.len() + 1);
                    let choice = rng.below(5);
                    if choice == 0 {
                        // A hard line break (Enter).
                        let style = text[at.saturating_sub(1).min(text.len() - 1)].1.clone();
                        text.insert(at, ('\n', style));
                    } else if choice < 3 {
                        let words = (0..1 + rng.below(12))
                            .flat_map(|_| {
                                let word = (0..1 + rng.below(8))
                                    .map(|_| pool[rng.below(pool.len())].clone())
                                    .collect::<Vec<_>>();
                                std::iter::once((' ', word[0].1.clone())).chain(word)
                            })
                            .collect::<Vec<_>>();
                        text.splice(at..at, words);
                    } else {
                        let end = (at + 1 + rng.below(60)).min(text.len());
                        text.drain(at..end);
                    }
                    let after = reflow(&document, &paragraph, &text);
                    let case = format!("{name} {:?} round {round}", block.lines[0]);
                    // The paragraph's lines and lines added to it (below, or
                    // in place of a list line), in order down the page.
                    let mut lines = after
                        .lines
                        .iter()
                        .filter(|line| {
                            block.lines.contains(&line.anchor)
                                || !before.lines.iter().any(|old| old.anchor == line.anchor)
                        })
                        .collect::<Vec<_>>();
                    lines.sort_by(|a, b| {
                        b.origin()
                            .1
                            .total_cmp(&a.origin().1)
                            .then(a.origin().0.total_cmp(&b.origin().0))
                    });
                    let read = lines
                        .iter()
                        .map(|line| line.text.clone())
                        .collect::<Vec<_>>();
                    assert_eq!(
                        normalise(&read.join(" ")),
                        normalise(&joined(&text)),
                        "{case}"
                    );
                    for line in before
                        .lines
                        .iter()
                        .filter(|line| !block.lines.contains(&line.anchor))
                    {
                        assert_line_unmoved(&case, line, after.line(&line.anchor).unwrap());
                    }
                }
            }
        }
    }
}
