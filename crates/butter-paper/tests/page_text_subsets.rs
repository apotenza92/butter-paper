//! Text whose font's ToUnicode map leaves characters out still reads in
//! full: from the embedded TrueType program's own character map, or, for a
//! subset that has none (PDF24 embeds many renumbered subsets of one font,
//! each map missing different letters), from another font of its family in
//! the document that draws the same glyph outline and knows its character.

use butter_paper::pdf_text_edit::{TextEditLimits, extract_page_text};
use lopdf::{Document, Object, Stream, dictionary};

const ARIMO: &[u8] = include_bytes!("../assets/fonts/Arimo-Regular.ttf");

/// `program` with the named tables hidden from readers (their directory
/// tags' last letter moved on, which keeps the directory sorted for readers
/// that search it), as a subset without them.
fn without_tables(program: &[u8], tags: &[&[u8; 4]]) -> Vec<u8> {
    let mut program = program.to_vec();
    let count = u16::from_be_bytes([program[4], program[5]]) as usize;
    for index in 0..count {
        let at = 12 + 16 * index;
        for tag in tags {
            if &program[at..at + 4] == tag.as_slice() {
                program[at + 3] += 1;
            }
        }
    }
    program
}

/// A ToUnicode CMap for `pairs` of glyph (as CID) and character.
fn to_unicode(pairs: &[(u16, char)]) -> Vec<u8> {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
         1 begincodespacerange <0000> <FFFF> endcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", pairs.len()));
    for (glyph, character) in pairs {
        cmap.push_str(&format!("<{glyph:04X}> <{:04X}>\n", *character as u32));
    }
    cmap.push_str("endbfchar endcmap CMapName currentdict /CMap defineresource pop end end\n");
    cmap.into_bytes()
}

fn glyph(character: char) -> u16 {
    let face = ttf_parser::Face::parse(ARIMO, 0).unwrap();
    face.glyph_index(character).unwrap().0
}

/// A Type0 font of `program` named `name` with a ToUnicode map of `known`.
fn font(document: &mut Document, name: &str, program: Vec<u8>, known: &str) -> lopdf::ObjectId {
    let file = document.add_object(Stream::new(dictionary! {}, program));
    let descriptor = document.add_object(dictionary! {
        "Type" => "FontDescriptor", "FontName" => name, "Flags" => 32,
        "FontBBox" => vec![0.into(), (-200).into(), 1000.into(), 900.into()],
        "ItalicAngle" => 0, "Ascent" => 905, "Descent" => -212, "CapHeight" => 716,
        "StemV" => 80, "FontFile2" => file,
    });
    let descendant = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => name,
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"), "Supplement" => 0,
        },
        "FontDescriptor" => descriptor, "CIDToGIDMap" => "Identity", "DW" => 556,
    });
    let pairs = known
        .chars()
        .map(|character| (glyph(character), character))
        .collect::<Vec<_>>();
    let map = document.add_object(Stream::new(dictionary! {}, to_unicode(&pairs)));
    document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "BaseFont" => name,
        "Encoding" => "Identity-H", "DescendantFonts" => vec![descendant.into()],
        "ToUnicode" => map,
    })
}

fn shown(text: &str) -> String {
    let codes = text
        .chars()
        .map(|character| format!("{:04X}", glyph(character)))
        .collect::<String>();
    format!("<{codes}>")
}

#[test]
fn a_subset_without_a_character_map_reads_through_its_family() {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    // The whole font, whose own map knows every character, though its
    // ToUnicode map knows only capitals.
    let whole = font(&mut document, "Arimo", ARIMO.to_vec(), "MRPV ");
    // A subset with no character map or glyph names, its ToUnicode map
    // missing the lower case.
    let subset = font(
        &mut document,
        "ABCDEF+Arimo",
        without_tables(ARIMO, &[b"cmap", b"post"]),
        "MRPV ",
    );
    let content = format!(
        "BT /F1 12 Tf 72 700 Td {} Tj ET BT /F2 12 Tf 72 650 Td {} Tj ET",
        shown("Moreland Road"),
        shown("Pascoe Vale"),
    );
    let content = document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => whole, "F2" => subset } },
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let layer = extract_page_text(&document, 0, &TextEditLimits::default()).unwrap();
    let texts = layer
        .lines
        .iter()
        .map(|line| line.text.clone())
        .collect::<Vec<_>>();
    assert!(texts.contains(&"Moreland Road".to_owned()), "{texts:?}");
    assert!(texts.contains(&"Pascoe Vale".to_owned()), "{texts:?}");
}
