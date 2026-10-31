//! Convert to Word Document: the producer and table fixtures, the untagged
//! Word-style report and a built page with images, converted, unzipped and
//! read back: paragraph order and text, run styles, tables' rows, columns
//! and cell text, embedded images and each page's section.

use std::collections::BTreeMap;
use std::io::Read as _;

use butter_paper::docx_export::docx_from_pdf;
use lopdf::{Dictionary, Document, Object, Stream, dictionary};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!(
        "{}/tests/fixtures/text-edit/{name}.pdf",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

/// The archive's files by name.
fn unzip(bytes: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let number = |at: usize, width: usize| {
        bytes[at..at + width]
            .iter()
            .rev()
            .fold(0usize, |value, byte| value << 8 | *byte as usize)
    };
    let mut files = BTreeMap::new();
    let mut at = 0;
    while number(at, 4) == 0x0403_4b50 {
        let method = number(at + 8, 2);
        let compressed = number(at + 18, 4);
        let size = number(at + 22, 4);
        let name_length = number(at + 26, 2);
        let extra = number(at + 28, 2);
        let name = String::from_utf8(bytes[at + 30..at + 30 + name_length].to_vec()).unwrap();
        let start = at + 30 + name_length + extra;
        let data = &bytes[start..start + compressed];
        let contents = match method {
            0 => data.to_vec(),
            8 => {
                let mut out = Vec::new();
                flate2::read::DeflateDecoder::new(data)
                    .read_to_end(&mut out)
                    .unwrap();
                out
            }
            other => panic!("method {other}"),
        };
        assert_eq!(contents.len(), size, "{name}");
        assert_eq!(
            crc32fast::hash(&contents),
            number(at + 14, 4) as u32,
            "{name}"
        );
        files.insert(name, contents);
        at = start + compressed;
    }
    assert_eq!(number(at, 4), 0x0201_4b50, "the central directory follows");
    files
}

#[derive(Debug, Default)]
struct Run {
    text: String,
    font: String,
    half_points: u32,
    bold: bool,
    italic: bool,
    underline: bool,
    colour: Option<String>,
}

#[derive(Debug, Default)]
struct Para {
    runs: Vec<Run>,
    justification: String,
    /// Relationship ids of the images it draws.
    images: Vec<String>,
    section: bool,
}

impl Para {
    fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }
}

#[derive(Debug)]
enum Block {
    Para(Para),
    /// Its grid columns, and each row's cells' paragraphs.
    Table(usize, Vec<Vec<Vec<Para>>>),
}

fn attribute(attributes: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=\"");
    let start = attributes.find(&pattern)? + pattern.len();
    let end = attributes[start..].find('"')? + start;
    Some(attributes[start..end].to_owned())
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

/// `word/document.xml`'s body as paragraphs and tables.
fn body(xml: &str) -> (Vec<Block>, usize) {
    let tag = regex::Regex::new(r"<(/?)([\w:]+)([^>]*?)(/?)>|([^<]+)").unwrap();
    let mut blocks = Vec::new();
    let mut table: Option<(usize, Vec<Vec<Vec<Para>>>)> = None;
    let (mut para, mut run): (Option<Para>, Option<Run>) = (None, None);
    let mut in_text = false;
    let mut sections = 0;
    for token in tag.captures_iter(xml) {
        if let Some(text) = token.get(5) {
            if in_text && let Some(run) = &mut run {
                run.text.push_str(&unescape(text.as_str()));
            }
            continue;
        }
        let closing = &token[1] == "/";
        let name = &token[2];
        let attributes = &token[3];
        let empty = &token[4] == "/";
        if !closing {
            match name {
                "w:tbl" => table = Some((0, Vec::new())),
                "w:gridCol" => {
                    if let Some(table) = &mut table {
                        table.0 += 1;
                    }
                }
                "w:tr" => table.as_mut().unwrap().1.push(Vec::new()),
                "w:tc" => table
                    .as_mut()
                    .unwrap()
                    .1
                    .last_mut()
                    .unwrap()
                    .push(Vec::new()),
                "w:p" => para = Some(Para::default()),
                "w:r" => run = Some(Run::default()),
                "w:t" => in_text = !empty,
                "w:br" => run.as_mut().unwrap().text.push('\n'),
                "w:tab" => {
                    if let Some(run) = &mut run {
                        run.text.push('\t');
                    }
                }
                "w:sectPr" => {
                    sections += 1;
                    if let Some(para) = &mut para {
                        para.section = true;
                    }
                }
                "w:jc" => {
                    para.as_mut().unwrap().justification = attribute(attributes, "w:val").unwrap()
                }
                "a:blip" => para
                    .as_mut()
                    .unwrap()
                    .images
                    .push(attribute(attributes, "r:embed").unwrap()),
                _ => {}
            }
            if let Some(run) = &mut run {
                match name {
                    "w:rFonts" => run.font = attribute(attributes, "w:ascii").unwrap(),
                    "w:sz" => {
                        run.half_points = attribute(attributes, "w:val").unwrap().parse().unwrap()
                    }
                    "w:b" => run.bold = true,
                    "w:i" => run.italic = true,
                    "w:u" => run.underline = true,
                    "w:color" => run.colour = attribute(attributes, "w:val"),
                    _ => {}
                }
            }
        }
        if closing || empty {
            match name {
                "w:t" => in_text = false,
                "w:r" => {
                    if let (Some(para), Some(done)) = (&mut para, run.take()) {
                        para.runs.push(done);
                    }
                }
                "w:p" => {
                    let done = para.take().unwrap();
                    match &mut table {
                        Some((_, rows)) => rows.last_mut().unwrap().last_mut().unwrap().push(done),
                        None => blocks.push(Block::Para(done)),
                    }
                }
                "w:tbl" => {
                    let (columns, rows) = table.take().unwrap();
                    blocks.push(Block::Table(columns, rows));
                }
                _ => {}
            }
        }
    }
    (blocks, sections)
}

/// Words separated by single spaces.
fn normalise(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Converted {
    files: BTreeMap<String, Vec<u8>>,
    blocks: Vec<Block>,
    sections: usize,
}

fn convert(pdf: &[u8]) -> Converted {
    let files = unzip(&docx_from_pdf(pdf).unwrap());
    for part in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/_rels/document.xml.rels",
        "word/styles.xml",
        "word/settings.xml",
    ] {
        assert!(files.contains_key(part), "{part}");
    }
    let xml = String::from_utf8(files["word/document.xml"].clone()).unwrap();
    let (blocks, sections) = body(&xml);
    Converted {
        files,
        blocks,
        sections,
    }
}

impl Converted {
    /// The text of each top-level paragraph that has some, in order.
    fn paragraphs(&self) -> Vec<String> {
        self.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Para(para) => Some(normalise(&para.text())),
                Block::Table(..) => None,
            })
            .filter(|text| !text.is_empty())
            .collect()
    }

    fn para(&self, starting: &str) -> &Para {
        self.blocks
            .iter()
            .find_map(|block| match block {
                Block::Para(para) if normalise(&para.text()).starts_with(starting) => Some(para),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no paragraph {starting:?}: {:#?}", self.paragraphs()))
    }

    fn tables(&self) -> Vec<(usize, Vec<Vec<String>>)> {
        self.blocks
            .iter()
            .filter_map(|block| match block {
                Block::Table(columns, rows) => Some((
                    *columns,
                    rows.iter()
                        .map(|cells| {
                            cells
                                .iter()
                                .map(|paras| {
                                    paras
                                        .iter()
                                        .map(|para| normalise(&para.text()))
                                        .filter(|text| !text.is_empty())
                                        .collect::<Vec<_>>()
                                        .join(" | ")
                                })
                                .collect()
                        })
                        .collect(),
                )),
                Block::Para(_) => None,
            })
            .collect()
    }
}

/// `expected` appear in `paragraphs` in this order.
fn in_order(paragraphs: &[String], expected: &[&str]) {
    let mut at = 0;
    for text in expected {
        match paragraphs[at..]
            .iter()
            .position(|paragraph| paragraph == text)
        {
            Some(found) => at += found + 1,
            None => panic!("{text:?} is not after paragraph {at} of {paragraphs:#?}"),
        }
    }
}

fn run<'a>(para: &'a Para, text: &str) -> &'a Run {
    para.runs
        .iter()
        .find(|run| run.text.contains(text))
        .unwrap_or_else(|| panic!("no run {text:?} in {para:#?}"))
}

#[test]
fn the_untagged_report_converts_to_paragraphs_and_a_table() {
    let converted = convert(&fixture("word-table"));
    assert_eq!(
        converted.paragraphs(),
        [
            "Home Visit Assessment:",
            "Mr Sample Person",
            "12 Example Way, Testville Heights 3999",
            "Thank you for referring Sam for physiotherapy treatment at home.",
            "Please contact the clinic with any questions about this report.",
        ]
    );
    // The table comes between the paragraph above it and the one below.
    let order = converted
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Para(para) if !para.text().trim().is_empty() => {
                Some(normalise(&para.text())[..5].to_owned())
            }
            Block::Table(..) => Some("table".to_owned()),
            Block::Para(_) => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        ["Home ", "Mr Sa", "12 Ex", "Thank", "table", "Pleas"]
    );
    let title = run(converted.para("Home Visit"), "Home Visit");
    assert!(title.underline && !title.bold, "{title:?}");
    assert_eq!(title.font, "Liberation Sans");
    assert_eq!(title.half_points, 32);
    let name = run(converted.para("Mr Sample"), "Mr Sample");
    assert!(name.bold && name.half_points == 28, "{name:?}");
    let body = run(converted.para("Thank you"), "Thank you");
    assert!(
        !body.bold && !body.underline && body.half_points == 24,
        "{body:?}"
    );

    let tables = converted.tables();
    assert_eq!(tables.len(), 1);
    let (columns, rows) = &tables[0];
    assert_eq!(*columns, 2);
    assert_eq!(
        rows,
        &[
            vec![
                "Home assessment".to_owned(),
                "Sam lives alone in a double storey house with level pathways to all rooms. \
                 There are 3 steps between the living area and the shower. There are no \
                 modifications to the home. There is a rail at the back door for access."
                    .to_owned(),
            ],
            vec![
                "Steps".to_owned(),
                "There are 2 steps at the front entrance and a staircase to the second level \
                 where the bedroom is. Sam climbs them twice a day, morning and night. We \
                 reviewed gait and use of the rail."
                    .to_owned(),
            ],
            vec![
                "Mobility and transfers".to_owned(),
                // Two paragraphs of one cell.
                "Sam uses a single point stick to walk around the house and uses it out in \
                 the community. | Sam reports no falls in the past year."
                    .to_owned(),
            ],
        ]
    );
    // One page: the body's own section, A4.
    assert_eq!(converted.sections, 1);
    let xml = String::from_utf8(converted.files["word/document.xml"].clone()).unwrap();
    assert!(xml.contains("<w:pgSz w:w=\"11906\" w:h=\"16838\"/>"), "A4");
}

/// The producers' paragraphs, styles and alignment (their two-column
/// passage is not recognised as columns yet, so it is not checked).
#[test]
fn producer_pages_convert_to_styled_paragraphs() {
    for producer in ["chrome", "libreoffice"] {
        let converted = convert(&fixture(producer));
        let paragraphs = converted.paragraphs();
        in_order(
            &paragraphs,
            &[
                "Project Specification",
                "The contractor shall provide all labour, plant and materials required for \
                 the structural steel works, including connections, fixings and temporary \
                 bracing. Members shall be fabricated in accordance with the approved shop \
                 drawings and erected plumb, level and true to line within the tolerances \
                 given in the specification.",
                "Ragged paragraph: the engineer may inspect any part of the works at any \
                 reasonable time and the contractor shall give access, assistance and the \
                 information the engineer requires.",
                "Centred note: all dimensions are in millimetres unless noted otherwise.",
                "Revision B — issued 12/10/2026",
            ],
        );
        let heading = run(converted.para("Project Specification"), "Project");
        assert!(
            heading.bold && heading.half_points == 36,
            "{producer}: {heading:?}"
        );
        let body = converted.para("The contractor shall");
        assert_eq!(body.justification, "both", "{producer}");
        assert!(run(body, "structural steel").bold, "{producer}");
        assert!(run(body, "connections").italic, "{producer}");
        assert!(!run(body, "The contractor").bold, "{producer}");
        assert_eq!(
            converted.para("Centred note").justification,
            "center",
            "{producer}"
        );
        assert_eq!(
            converted.para("Revision B").justification,
            "right",
            "{producer}"
        );
        assert!(
            paragraphs
                .iter()
                .any(|text| text.contains("Serif paragraph: Total contract sum $1,234,567.89")),
            "{producer}: {paragraphs:#?}"
        );
        assert!(converted.tables().is_empty(), "{producer}");
    }
    let quartz = convert(&fixture("quartz"));
    in_order(
        &quartz.paragraphs(),
        &[
            "Quartz plain text corpus line one.",
            "Second line with a tab here and café façade.",
        ],
    );
}

#[test]
fn ruled_and_declared_tables_convert_to_word_tables() {
    for producer in ["chrome-table", "libreoffice-table"] {
        let converted = convert(&fixture(producer));
        in_order(
            &converted.paragraphs(),
            &[
                "Order summary for the site works.",
                "Delivery is expected within two weeks of the order date.",
            ],
        );
        let tables = converted.tables();
        assert_eq!(tables.len(), 1, "{producer}");
        assert_eq!(
            tables[0],
            (
                3,
                [
                    ["Item", "Qty", "Price"],
                    ["Bolts, M8 zinc-plated", "200", "$14.00"],
                    ["Washers", "500", "$6.50"],
                    ["Brackets", "40", "$88.00"],
                ]
                .iter()
                .map(|row| row.iter().map(|cell| cell.to_string()).collect())
                .collect()
            ),
            "{producer}"
        );
    }
}

/// A page with text, a PNG-bound image and a JPEG between its paragraphs,
/// then a landscape page.
fn illustrated() -> (Vec<u8>, Vec<u8>) {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
        .encode(
            &[200u8, 30, 30].repeat(16),
            4,
            4,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font = document.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    // Two by two: red, green; blue, white.
    let pixels = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];
    let mut png_bound = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
        },
        pixels,
    );
    png_bound.compress().unwrap();
    let png_bound = document.add_object(png_bound);
    let jpeg_image = document.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 4, "Height" => 4,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8, "Filter" => "DCTDecode",
        },
        jpeg.clone(),
    ));
    let page = |document: &mut Document, size: (i64, i64), content: &[u8], images: bool| {
        let content = document.add_object(Stream::new(Dictionary::new(), content.to_vec()));
        let mut resources = dictionary! { "Font" => dictionary! { "F1" => font } };
        if images {
            resources.set(
                "XObject",
                dictionary! { "Im1" => png_bound, "Im2" => jpeg_image },
            );
        }
        document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), size.0.into(), size.1.into()],
            "Contents" => content, "Resources" => resources,
        })
    };
    let first = page(
        &mut document,
        (612, 792),
        b"BT /F1 12 Tf 72 700 Td (Before the images) Tj ET \
          q 80 0 0 80 72 580 cm /Im1 Do Q \
          BT /F1 12 Tf 72 540 Td (Between the images) Tj ET \
          q 60 0 0 60 72 420 cm /Im2 Do Q \
          BT /F1 12 Tf 72 380 Td (After the images) Tj ET",
        true,
    );
    let second = page(
        &mut document,
        (792, 612),
        b"BT /F1 12 Tf 72 500 Td (A landscape page) Tj ET",
        false,
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![first.into(), second.into()], "Count" => 2,
        }),
    );
    let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    (bytes, jpeg)
}

#[test]
fn images_embed_inline_and_each_page_is_a_section() {
    let (pdf, jpeg) = illustrated();
    let converted = convert(&pdf);
    let order = converted
        .blocks
        .iter()
        .filter_map(|block| match block {
            Block::Para(para) if !para.images.is_empty() => Some("image".to_owned()),
            Block::Para(para) if !para.text().trim().is_empty() => Some(normalise(&para.text())),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        order,
        [
            "Before the images",
            "image",
            "Between the images",
            "image",
            "After the images",
            "A landscape page",
        ]
    );
    // The image without a JPEG is a PNG of its pixels; the JPEG embeds as
    // it is.
    let png = image::load_from_memory(&converted.files["word/media/image1.png"])
        .unwrap()
        .to_rgb8();
    assert_eq!(png.dimensions(), (2, 2));
    assert_eq!(png.get_pixel(0, 0).0, [255, 0, 0]);
    assert_eq!(png.get_pixel(1, 1).0, [255, 255, 255]);
    assert_eq!(converted.files["word/media/image2.jpeg"], jpeg);
    let relationships =
        String::from_utf8(converted.files["word/_rels/document.xml.rels"].clone()).unwrap();
    assert!(relationships.contains("Target=\"media/image1.png\""));
    assert!(relationships.contains("Target=\"media/image2.jpeg\""));

    // The first page's section ends with its last paragraph; the second is
    // the body's, landscape.
    assert_eq!(converted.sections, 2);
    assert!(converted.para("After the images").section);
    let xml = String::from_utf8(converted.files["word/document.xml"].clone()).unwrap();
    assert!(xml.contains("<w:pgSz w:w=\"12240\" w:h=\"15840\"/>"));
    assert!(xml.contains("<w:pgSz w:w=\"15840\" w:h=\"12240\" w:orient=\"landscape\"/>"));
}

#[test]
fn an_unreadable_pdf_says_so() {
    let error = docx_from_pdf(b"not a pdf").unwrap_err();
    assert!(error.contains("could not be read"), "{error}");
}

/// LibreOffice opens each conversion as a Word document and prints it with
/// its text: the package is valid OOXML. Writes the documents and their PDFs
/// to `target/test-scratch/docx` for inspection beside their sources.
#[test]
#[ignore = "needs LibreOffice (soffice) and pdftotext"]
fn libreoffice_opens_the_conversions() {
    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-scratch/docx");
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let mut documents = vec![("illustrated".to_owned(), illustrated().0)];
    for name in [
        "word-table",
        "chrome",
        "libreoffice",
        "quartz",
        "chrome-table",
        "libreoffice-table",
    ] {
        documents.push((name.to_owned(), fixture(name)));
    }
    for (name, pdf) in documents {
        let docx = out.join(format!("{name}.docx"));
        std::fs::write(&docx, docx_from_pdf(&pdf).unwrap()).unwrap();
        let status = std::process::Command::new("soffice")
            .args(["--headless", "--convert-to", "pdf", "--outdir"])
            .arg(&out)
            .arg(&docx)
            .env("HOME", &out)
            .status()
            .unwrap();
        assert!(status.success(), "{name}");
        let text = std::process::Command::new("pdftotext")
            .arg(out.join(format!("{name}.pdf")))
            .arg("-")
            .output()
            .unwrap();
        let text = normalise(&String::from_utf8_lossy(&text.stdout));
        let expected = match name.as_str() {
            "illustrated" => "Between the images",
            "word-table" => "There are no modifications to the home",
            "quartz" => "Quartz plain text corpus line one.",
            "chrome-table" | "libreoffice-table" => "Bolts, M8 zinc-plated",
            _ => "Project Specification",
        };
        assert!(text.contains(expected), "{name}: {text}");
    }
}
