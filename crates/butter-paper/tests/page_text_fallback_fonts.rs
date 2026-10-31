//! Characters no page font or installed font draws come from the bundled
//! fallback fonts, embedded as a small subset that later edits extend. No
//! fonts are installed here (`BP_SYSTEM_FONT_DIRS` names a missing folder), so
//! a same-name installed font, such as Ubuntu's Liberation fonts, cannot draw
//! them instead.

use butter_paper::pdf_text_edit::{
    PageTextEdit, PageTextLayer, PageTextLine, TextEditLimits, TextEditRun, apply_text_edits,
    extract_page_text,
};
use lopdf::{Document, Object};

fn producer(name: &str) -> Document {
    let path = format!(
        "{}/tests/fixtures/text-edit/{name}.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::load(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

fn layer(document: &Document) -> PageTextLayer {
    extract_page_text(document, 0, &TextEditLimits::default()).expect("extract")
}

fn editable(line: &PageTextLine) -> bool {
    line.read_only.is_none() && !line.text.trim().is_empty()
}

/// The file as the app saves it: without objects edits left unreachable.
fn saved(document: &mut Document) -> Vec<u8> {
    document.prune_objects();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("save");
    bytes
}

/// The embedded Type0 fonts Edit PDF Content added, by base font name.
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
    // SAFETY: this test binary has one test, which sets the variable before
    // any font lookup. A missing folder is skipped, leaving no installed fonts.
    unsafe {
        std::env::set_var(
            "BP_SYSTEM_FONT_DIRS",
            concat!(env!("CARGO_MANIFEST_DIR"), "/target/test-scratch/no-fonts"),
        );
    }
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
