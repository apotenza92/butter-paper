//! Edit PDF Content draws characters a page font lacks with an installed font of
//! the same name, and edits fonts without widths through installed or
//! bundled metrics. The installed fonts here are the bundled font folder
//! (`BP_SYSTEM_FONT_DIRS`), so the test does not depend on the machine.

use butter_paper::pdf_text_edit::{
    PageTextEdit, TextEditLimits, TextEditRun, apply_text_edits, extract_page_text,
};
use lopdf::{Document, Object, Stream, dictionary};

fn document(font: lopdf::Dictionary) -> Document {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(font);
    let content = document.add_object(Stream::new(
        lopdf::Dictionary::new(),
        b"BT /F1 24 Tf 72 700 Td (Signed by hand) Tj ET".to_vec(),
    ));
    let page = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    Document::load_mem(&bytes).unwrap()
}

fn base_fonts(document: &Document) -> Vec<String> {
    document
        .objects
        .values()
        .filter_map(|object| object.as_dict().ok())
        .filter(|dictionary| {
            dictionary.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type0".as_slice())
        })
        .filter_map(|dictionary| dictionary.get(b"BaseFont").and_then(Object::as_name).ok())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

#[test]
fn installed_fonts_draw_what_page_fonts_lack() {
    // SAFETY: this test binary has one test, which sets the variable before
    // any font lookup.
    unsafe {
        std::env::set_var(
            "BP_SYSTEM_FONT_DIRS",
            concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts"),
        );
    }
    // A non-embedded font named like an installed one, without widths.
    for widths in [true, false] {
        let mut font = dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "Allura-Regular",
            "Encoding" => "WinAnsiEncoding",
        };
        if widths {
            font.set("FirstChar", 32);
            font.set("LastChar", 126);
            font.set(
                "Widths",
                (32..=126).map(|_| Object::Integer(400)).collect::<Vec<_>>(),
            );
        }
        let mut document = document(font);
        let layer = extract_page_text(&document, 0, &TextEditLimits::default()).unwrap();
        let line = &layer.lines[0];
        assert_eq!(line.read_only, None, "widths {widths}");
        // "ł" is outside WinAnsi: an installed font of the same name draws it.
        let edit = PageTextEdit {
            page_index: 0,
            anchor: line.anchor,
            original: line.original_runs(),
            runs: vec![TextEditRun {
                style: line.styles[0].clone(),
                text: "Signed by hand ł".into(),
            }],
            layout: Default::default(),
        };
        apply_text_edits(&mut document, &[edit], &TextEditLimits::default()).unwrap();
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        let reopened = Document::load_mem(&bytes).unwrap();
        let fonts = base_fonts(&reopened);
        assert!(
            fonts.iter().any(|name| name.ends_with("+Allura-Regular")),
            "{fonts:?}"
        );
        assert_eq!(
            extract_page_text(&reopened, 0, &TextEditLimits::default())
                .unwrap()
                .lines[0]
                .text,
            "Signed by hand ł"
        );
    }
}
